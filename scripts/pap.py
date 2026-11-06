#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# The German wage tax from the BMF's Programmablaufplan (PAP) in its XML pseudocode, published by the ITZBund at
# https://www.bmf-steuerrechner.de/interface/pseudocodes.xhtml – translated, not copied: the pseudocode is Java with
# BigDecimal; this script parses it and either writes it as Rust (`generate`, for crates/librotenado/src/payroll/) or
# runs it itself on Python's decimal arithmetic (`cases`, the expected results the Rust code is tested against).
#   scripts/pap.py generate <Lohnsteuer2026.xml> <year>  > crates/librotenado/src/payroll/pap<year>.rs  (then cargo fmt)
#   scripts/pap.py cases <Lohnsteuer2026.xml> [count]   > crates/librotenado/tests/data/de-lst-<year>.json
# The interpreter and the Rust code share only the parser; the arithmetic is written twice. pap2026.rs and
# de-lst-2026.json come from Lohnsteuer2026.xml version 1.0 (Stand 2025-10-23), SHA-256
# 63d8981646d139eba2f4dd990c13b43c4fb3883b402a5a40cddf253aa7aa96b4; pap2025.rs and de-lst-2025.json from
# Lohnsteuer2025.xml version 1.0 (Stand 2025-09-17), SHA-256
# 878ea4a0a1e0bb7122e44b02fe7f2e1d70b605768a2b42ec84f895365f4fb7e1. Both cases files were also checked against the
# BMF's external interface for programme tests (bmf-steuerrechner.de/interface/einganginterface.xhtml).
import decimal
import json
import random
import re
import sys
import xml.etree.ElementTree as ET

decimal.getcontext().prec = 60


# ---------------------------------------------------------------------------------------------------------------------
# Parsing

TOKEN = re.compile(r"\s*(?:(\d+\.\d+|\d+)|([A-Za-z_][A-Za-z_0-9]*)|(==|!=|<=|>=|&&|\|\||[-+*/<>().,\[\]]))")


def tokens(text):
    pos, out = 0, []
    text = text.strip()
    while pos < len(text):
        m = TOKEN.match(text, pos)
        if not m:
            raise SyntaxError(f"at {pos}: {text!r}")
        pos = m.end()
        out.append(("num", m.group(1)) if m.group(1) else ("id", m.group(2)) if m.group(2) else ("op", m.group(3)))
    return out


class Parser:
    def __init__(self, text):
        self.t = tokens(text)
        self.i = 0

    def peek(self):
        return self.t[self.i] if self.i < len(self.t) else (None, None)

    def take(self, value=None):
        tok = self.peek()
        if value is not None and tok[1] != value:
            raise SyntaxError(f"expected {value}, got {tok} in {self.t}")
        self.i += 1
        return tok

    def done(self):
        if self.i != len(self.t):
            raise SyntaxError(f"trailing {self.t[self.i:]}")

    def binary(self, ops, sub):
        left = sub()
        while self.peek()[1] in ops:
            op = self.take()[1]
            left = ("bin", op, left, sub())
        return left

    def expr(self):
        return self.binary(("||",), lambda: self.binary(("&&",), self.compare))

    def compare(self):
        left = self.additive()
        if self.peek()[1] in ("==", "!=", "<", ">", "<=", ">="):
            op = self.take()[1]
            return ("bin", op, left, self.additive())
        return left

    def additive(self):
        return self.binary(("+", "-"), lambda: self.binary(("*", "/"), self.unary))

    def unary(self):
        if self.peek()[1] == "-":
            self.take()
            return ("neg", self.unary())
        return self.postfix()

    def postfix(self):
        node = self.primary()
        while True:
            if self.peek()[1] == ".":
                self.take()
                name = self.take()[1]
                self.take("(")
                args = []
                if self.peek()[1] != ")":
                    args.append(self.expr())
                    while self.peek()[1] == ",":
                        self.take()
                        args.append(self.expr())
                self.take(")")
                node = ("call", node, name, args)
            elif self.peek()[1] == "[":
                self.take()
                index = self.expr()
                self.take("]")
                node = ("index", node, index)
            else:
                return node

    def primary(self):
        kind, value = self.take()
        if kind == "num":
            return ("num", value)
        if value == "(":
            node = self.expr()
            self.take(")")
            return node
        if value == "new":
            # `new BigDecimal(x)`, as `BigDecimal.valueOf(x)` for the integers and decimals the PAP gives it.
            self.take("BigDecimal")
            self.take("(")
            arg = self.expr()
            self.take(")")
            return ("valueOf", arg)
        if value == "BigDecimal":
            self.take(".")
            name = self.take()[1]
            if name == "valueOf":
                self.take("(")
                arg = self.expr()
                self.take(")")
                return ("valueOf", arg)
            return ("big", name)
        return ("var", value)


def parse_expr(text):
    p = Parser(text)
    node = p.expr()
    p.done()
    return node


def parse_assignment(text):
    target, value = text.split("=", 1)
    return target.strip(), parse_expr(value)


def statements(element):
    out = []
    for child in element:
        if child.tag == "EVAL":
            out.append(("eval", *parse_assignment(child.get("exec"))))
        elif child.tag == "EXECUTE":
            out.append(("exec", child.get("method")))
        elif child.tag == "IF":
            then = child.find("THEN")
            other = child.find("ELSE")
            out.append(
                (
                    "if",
                    parse_expr(child.get("expr")),
                    statements(then) if then is not None else [],
                    statements(other) if other is not None else [],
                )
            )
        else:
            raise SyntaxError(child.tag)
    return out


class Pap:
    def __init__(self, path):
        root = ET.fromstring(open(path, encoding="utf-8-sig").read())
        self.name = root.get("name")
        self.types, self.defaults, self.inputs, self.outputs = {}, {}, [], []
        # The comment before each variable is the PAP's explanation of it.
        self.docs = {}
        commented = ET.fromstring(
            open(path, encoding="utf-8-sig").read(), parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
        )
        for group in commented.iter():
            last = None
            for child in group:
                if child.tag is ET.Comment:
                    last = " ".join((child.text or "").split())
                elif child.tag in ("INPUT", "OUTPUT", "INTERNAL"):
                    if last:
                        self.docs[child.get("name")] = last
                    last = None
        self.internals = [v.get("name") for v in root.iter("INTERNAL")]
        for kind in ("INPUT", "OUTPUT", "INTERNAL"):
            for v in root.iter(kind):
                t = v.get("type")
                self.types[v.get("name")] = "int" if t == "int" else "dec"
                self.defaults[v.get("name")] = v.get("default") or "0"
                (self.inputs if kind == "INPUT" else self.outputs if kind == "OUTPUT" else []).append(v.get("name"))
        self.constants = {}
        for c in root.iter("CONSTANT"):
            value = c.get("value").strip()
            if c.get("type").endswith("[]"):
                items = [parse_expr(x) for x in split_top(value.strip("{}"))]
                self.constants[c.get("name")] = ("array", items)
            else:
                self.constants[c.get("name")] = ("scalar", parse_expr(value))
        self.main = statements(root.find("METHODS/MAIN"))
        self.methods = {m.get("name"): statements(m) for m in root.iter("METHOD")}


def split_top(text):
    parts, depth, cur = [], 0, ""
    for ch in text:
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
            continue
        depth += ch == "("
        depth -= ch == ")"
        cur += ch
    if cur.strip():
        parts.append(cur)
    return parts


# ---------------------------------------------------------------------------------------------------------------------
# Rust

RUST_HEADER = """\
// SPDX-License-Identifier: MIT
// Generated by scripts/pap.py from the XML pseudocode of the Programmablaufplan {name} (BMF, ITZBund) – do not edit.
//! The PAP {name}, step by step as published; the names are the PAP's.
#![allow(non_snake_case, clippy::all, clippy::pedantic, unused_parens, dead_code)]

use super::dec::{{Dec, Round}};
"""


class Rust:
    def __init__(self, pap):
        self.pap = pap

    def type_of(self, node):
        kind = node[0]
        if kind == "num":
            return "int" if "." not in node[1] else "dec"
        if kind == "var":
            if node[1] in self.pap.constants:
                return "dec"
            return self.pap.types[node[1]]
        if kind in ("valueOf", "big", "index"):
            return "dec"
        if kind == "call":
            return "int" if node[2] in ("compareTo", "longValue") else "dec"
        if kind == "neg":
            return self.type_of(node[1])
        if kind == "bin":
            if node[1] in ("==", "!=", "<", ">", "<=", ">=", "&&", "||"):
                return "bool"
            return self.type_of(node[2])
        raise ValueError(node)

    def literal(self, text):
        if "." in text:
            whole, frac = text.split(".")
            return f"Dec::lit({int(whole + frac)}, {len(frac)})"
        return f"Dec::lit({int(text)}, 0)"

    def expr(self, node):
        kind = node[0]
        if kind == "num":
            return node[1] if "." not in node[1] else self.literal(node[1])
        if kind == "var":
            name = node[1]
            if name in self.pap.constants:
                return name
            return f"self.{name}"
        if kind == "big":
            return {"ZERO": "Dec::ZERO", "ONE": "Dec::ONE", "ROUND_DOWN": "Round::Down", "ROUND_UP": "Round::Up"}[
                node[1]
            ]
        if kind == "valueOf":
            arg = node[1]
            if arg[0] == "num":
                return self.literal(arg[1])
            return f"Dec::of({self.expr(arg)})"
        if kind == "index":
            return f"{self.expr(node[1])}[{self.expr(node[2])} as usize]"
        if kind == "neg":
            return f"-{self.expr(node[1])}"
        if kind == "bin":
            op, a, b = node[1], node[2], node[3]
            return f"({self.expr(a)} {op} {self.expr(b)})"
        if kind == "call":
            target, name, args = self.expr(node[1]), node[2], node[3]
            if name == "divide" and len(args) == 1:
                return f"{target}.divide_exact({self.expr(args[0])})"
            method = {
                "add": "add",
                "subtract": "subtract",
                "multiply": "multiply",
                "divide": "divide",
                "setScale": "set_scale",
                "compareTo": "compare_to",
                "longValue": "long_value",
            }[name]
            return f"{target}.{method}({', '.join(self.expr(a) for a in args)})"
        raise ValueError(node)

    def assign(self, target, value):
        t = self.pap.types[target]
        rhs = self.expr(value)
        if t == "dec" and self.type_of(value) == "int":
            rhs = f"Dec::of({rhs})"
        return f"self.{target} = {rhs};"

    def block(self, stmts, indent):
        pad = "    " * indent
        out = []
        for s in stmts:
            if s[0] == "eval":
                out.append(pad + self.assign(s[1], s[2]))
            elif s[0] == "exec":
                out.append(f"{pad}self.{s[1]}();")
            else:
                out.append(f"{pad}if {self.expr(s[1])} {{")
                out.extend(self.block(s[2], indent + 1))
                if s[3]:
                    out.append(f"{pad}}} else {{")
                    out.extend(self.block(s[3], indent + 1))
                out.append(f"{pad}}}")
        return out

    def default(self, name):
        value = self.pap.defaults[name]
        if self.pap.types[name] == "int":
            return str(int(float(value)))
        if value.startswith(("BigDecimal", "new ")):
            return self.expr(parse_expr(value))
        return self.literal(value if "." in value else value)

    def write(self, year):
        p = self.pap
        out = [RUST_HEADER.format(name=p.name)]
        for name, (kind, value) in p.constants.items():
            if kind == "array":
                items = ", ".join(self.expr(x) for x in value)
                out.append(f"const {name}: [Dec; {len(value)}] = [{items}];")
            else:
                out.append(f"const {name}: Dec = {self.expr(value)};")
        out.append("")
        out.append(f"/// The variables of the PAP {p.name}: inputs, outputs and internal fields.")
        out.append("#[derive(Debug, Clone)]")
        out.append(f"pub(super) struct Pap{year} {{")
        for name in p.types:
            out.append(f"    pub {name}: {'i64' if p.types[name] == 'int' else 'Dec'},")
        out.append("}")
        out.append("")
        out.append(f"impl Default for Pap{year} {{")
        out.append("    fn default() -> Self {")
        out.append("        Self {")
        for name in p.types:
            out.append(f"            {name}: {self.default(name)},")
        out.append("        }")
        out.append("    }")
        out.append("}")
        out.append("")
        out.append(f"impl Pap{year} {{")
        out.append("    /// The values of the PAP after a run – its inputs, internal fields and outputs –, each with its name and the")
        out.append("    /// PAP's explanation of it.")
        out.append("    pub fn trace(&self) -> Vec<(&'static str, &'static str, Dec)> {")
        out.append("        vec![")
        for name in p.inputs + p.internals + p.outputs:
            doc = p.docs.get(name, "").replace("\\", "\\\\").replace('"', '\\"')
            value = f"Dec::of(self.{name})" if p.types[name] == "int" else f"self.{name}"
            out.append(f'            ("{name}", "{doc}", {value}),')
        out.append("        ]")
        out.append("    }")
        out.append("")
        out.append("    /// Runs the PAP on the inputs set.")
        out.append("    pub fn run(&mut self) {")
        out.extend(self.block(p.main, 2))
        out.append("    }")
        for name, body in p.methods.items():
            out.append("")
            out.append(f"    fn {name}(&mut self) {{")
            out.extend(self.block(body, 2))
            out.append("    }")
        out.append("}")
        return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------------------------------------------------
# Interpreter: Java's BigDecimal on Python's Decimal. Values are exact; division with a scale rounds as asked, division
# without one must come out exact (Java throws otherwise).

D = decimal.Decimal


class Interpreter:
    def __init__(self, pap, inputs):
        self.pap = pap
        self.v = {}
        for name, t in pap.types.items():
            d = pap.defaults[name]
            if t == "int":
                self.v[name] = int(float(d))
            else:
                self.v[name] = self.eval(parse_expr(d)) if d.startswith(("BigDecimal", "new ")) else D(d)
        for name, value in inputs.items():
            self.v[name] = value if pap.types[name] == "int" else D(value)
        self.c = {}
        for name, (kind, value) in pap.constants.items():
            self.c[name] = [self.eval(x) for x in value] if kind == "array" else self.eval(value)

    @staticmethod
    def scale(x, n, mode):
        q = D(1).scaleb(-n)
        return x.quantize(q, rounding=decimal.ROUND_DOWN if mode == "ROUND_DOWN" else decimal.ROUND_UP)

    def eval(self, node):
        kind = node[0]
        if kind == "num":
            return int(node[1]) if "." not in node[1] else D(node[1])
        if kind == "var":
            return self.c[node[1]] if node[1] in self.c else self.v[node[1]]
        if kind == "big":
            return {"ZERO": D(0), "ONE": D(1)}.get(node[1], node[1])
        if kind == "valueOf":
            x = self.eval(node[1])
            return D(x) if not isinstance(x, D) else x
        if kind == "index":
            return self.eval(node[1])[self.eval(node[2])]
        if kind == "neg":
            return -self.eval(node[1])
        if kind == "bin":
            op = node[1]
            if op == "&&":
                return self.eval(node[2]) and self.eval(node[3])
            if op == "||":
                return self.eval(node[2]) or self.eval(node[3])
            a, b = self.eval(node[2]), self.eval(node[3])
            return {
                "==": lambda: a == b,
                "!=": lambda: a != b,
                "<": lambda: a < b,
                ">": lambda: a > b,
                "<=": lambda: a <= b,
                ">=": lambda: a >= b,
                "+": lambda: a + b,
                "-": lambda: a - b,
                "*": lambda: a * b,
            }[op]()
        if kind == "call":
            x = self.eval(node[1])
            args = [self.eval(a) for a in node[3]]
            name = node[2]
            if name == "add":
                return x + args[0]
            if name == "subtract":
                return x - args[0]
            if name == "multiply":
                return x * args[0]
            if name == "divide":
                if len(args) == 1:
                    q = x / args[0]
                    if q * args[0] != x:
                        raise ArithmeticError("non-terminating division")
                    return q
                return self.scale(x / args[0], args[1], args[2])
            if name == "setScale":
                return self.scale(x, args[0], args[1])
            if name == "compareTo":
                return (x > args[0]) - (x < args[0])
            if name == "longValue":
                return int(x)
        raise ValueError(node)

    def run(self, stmts=None):
        for s in self.pap.main if stmts is None else stmts:
            if s[0] == "eval":
                value = self.eval(s[2])
                if self.pap.types[s[1]] == "int":
                    value = int(value)
                elif not isinstance(value, D):
                    value = D(value)
                self.v[s[1]] = value
            elif s[0] == "exec":
                self.run(self.pap.methods[s[1]])
            else:
                self.run(s[2] if self.eval(s[1]) else s[3])


def random_case(rng):
    """Inputs as an employer sets them: a period, a wage, the employee's tax features and insurance."""
    lzz = rng.choice([1, 2, 2, 2, 3, 4])
    stkl = rng.choice([1, 1, 2, 3, 4, 5, 6])
    scale = {1: 12, 2: 1, 3: 7 / 30, 4: 1 / 30}[lzz]
    monthly = rng.choice([rng.randint(0, 2_000), rng.randint(2_000, 7_000), rng.randint(7_000, 30_000)])
    case = {
        "LZZ": lzz,
        "STKL": stkl,
        "RE4": int(monthly * scale * 100) + rng.randint(0, 99),
        "R": rng.choice([0, 0, 1]),
        "ZKF": rng.choice(["0", "0", "0.5", "1", "1.5", "2", "3"]) if stkl < 5 else "0",
        "KVZ": rng.choice(["2.50", "2.90", "3.10", "1.70"]),
        "PKV": rng.choice([0, 0, 0, 0, 1]),
        "PVZ": rng.choice([0, 1]),
        "PVS": rng.choice([0, 0, 0, 1]),
        "KRV": rng.choice([0, 0, 0, 0, 1]),
        "ALV": rng.choice([0, 0, 0, 0, 1]),
    }
    if case["PVZ"] == 0:
        case["PVA"] = str(rng.choice([0, 0, 1, 2, 3, 4]))
    if case["PKV"]:
        case["PKPV"] = rng.randint(20_000, 90_000)
        case["PKPVAGZ"] = rng.choice([0, case["PKPV"] // 2])
    if stkl == 4 and rng.random() < 0.3:
        case["af"] = 1
        case["f"] = rng.choice(["0.913", "0.950", "0.987"])
    if rng.random() < 0.15:
        case["LZZFREIB"] = int(rng.randint(10, 500) * scale * 100)
    if rng.random() < 0.05:
        case["LZZHINZU"] = int(rng.randint(10, 500) * scale * 100)
    if rng.random() < 0.15:
        case["ALTER1"] = 1
        case["AJAHR"] = rng.randint(2000, 2027)
    if rng.random() < 0.1:
        case["VBEZ"] = case["RE4"] // 2
        case["VBEZM"] = case["VBEZ"] if lzz == 2 else int(case["VBEZ"] / scale)
        case["VJAHR"] = rng.randint(2000, 2026)
        case["ZMVB"] = 12
    if rng.random() < 0.25:
        case["JRE4"] = int(monthly * 12 * 100)
        case["SONSTB"] = rng.randint(10_000, 2_000_000)
        if rng.random() < 0.3:
            case["SONSTENT"] = case["SONSTB"] // 2
        if rng.random() < 0.2:
            case["JFREIB"] = rng.randint(0, 300_000)
    if case.get("PVA") is None:
        case["PVA"] = "0"
    return case


def cases(pap, count):
    rng = random.Random(2026)
    out = []
    for _ in range(count):
        inputs = {k: v for k, v in random_case(rng).items() if k in pap.inputs}
        run = Interpreter(pap, inputs)
        run.run()
        out.append(
            {
                "inputs": {k: (v if isinstance(v, int) else str(v)) for k, v in inputs.items()},
                "outputs": {k: str(run.v[k]) for k in ("LSTLZZ", "SOLZLZZ", "BK", "STS", "SOLZS", "BKS")},
            }
        )
    return out


def main():
    command, path = sys.argv[1], sys.argv[2]
    pap = Pap(path)
    if command == "generate":
        sys.stdout.write(Rust(pap).write(sys.argv[3]))
    elif command == "cases":
        count = int(sys.argv[3]) if len(sys.argv) > 3 else 1000
        json.dump(cases(pap, count), sys.stdout, indent=None, separators=(",", ":"))
        sys.stdout.write("\n")
    else:
        raise SystemExit(f"unknown command {command}")


if __name__ == "__main__":
    main()
