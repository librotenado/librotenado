#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Generates the librotenado brand kit: master SVGs, PNG exports, favicon.ico and the website's head icons.

The mark is a T-account, the oldest picture of bookkeeping: a crossbar over a stem. Two short strokes have been
punched out of the crossbar and stand beneath it – debit and credit, equal in length, so the account balances. Two
corner marks, top left and bottom right, frame it like an excerpt from a larger, open whole: one model, many formats.
Small sizes (favicon, 16–48 px) use the punched T without the corners.

The wordmark is set in Atkinson Hyperlegible Next SemiBold, the descriptor in Atkinson Hyperlegible Mono (fonts/,
SIL OFL 1.1), both converted to outlines, so no font is needed where they are shown.

Needs rsvg-convert (Homebrew: librsvg) and fontTools (pip install fonttools). Run from anywhere:
    python3 generate.py
Edit this file, not the exports.
"""
import os, shutil, struct, subprocess

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

HERE = os.path.dirname(os.path.abspath(__file__))
SVG, PNG, WEB, FONTS = (os.path.join(HERE, d) for d in ("svg", "png", "web", "fonts"))

INK = "#14213D"          # navy – the T, text on light, dark backgrounds
BLUE = "#1F5FAD"         # the one accent on light: debit/credit strokes and corners
PAPER = "#F4F6FA"        # light backgrounds, the T on dark
BLUE_LIGHT = "#7FB2F0"   # the accent on dark
STEEL = "#4A5D70"        # descriptor and secondary text on light
MIST = "#A9B6C8"         # descriptor and secondary text on dark
NIGHT = "#0E1729"        # page background in dark mode (ink stays free for surfaces on it)

LIGHT = dict(ink=INK, acc=BLUE, sub=STEEL, bg=PAPER)
DARK = dict(ink=PAPER, acc=BLUE_LIGHT, sub=MIST, bg=INK)
BLACK = dict(ink="#000000", acc="#000000", sub="#000000", bg="#FFFFFF")
WHITE = dict(ink="#FFFFFF", acc="#FFFFFF", sub="#FFFFFF", bg="#000000")

WORDMARK_FONT = "AtkinsonHyperlegibleNext-SemiBold.ttf"
MONO_FONT = "AtkinsonHyperlegibleMono-Regular.ttf"
NAME = "librotenado"
DESCRIPTOR = "open accounting protocol"

# --- mark (drawn in a 72×72 box) -----------------------------------------------------------------------------------


def punched_t(p, inset=False):
    """The T with debit and credit punched out of its crossbar. inset: the smaller T inside the corners."""
    if inset:
        bar, stem, strokes = (16, 17, 7, [(16, 6), (26, 20), (50, 6)]), (32.5, 24, 7, 34), [(22, 28.5, 4, 16), (46, 28.5, 4, 16)]
    else:
        bar, stem, strokes = (6, 10, 9, [(6, 8), (20, 32), (58, 8)]), (31.5, 19, 9, 47), [(14, 25, 6, 22), (52, 25, 6, 22)]
    _, y, h, pieces = bar
    out = "".join(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{p["ink"]}"/>' for x, w in pieces)
    sx, sy, sw, sh = stem
    out += f'<rect x="{sx}" y="{sy}" width="{sw}" height="{sh}" fill="{p["ink"]}"/>'
    out += "".join(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{p["acc"]}"/>' for x, y, w, h in strokes)
    return out


def corners(p):
    return f'<path d="M5 20 V5 H20 M67 52 V67 H52" fill="none" stroke="{p["acc"]}" stroke-width="4.5"/>'


def mark(p=LIGHT):
    return corners(p) + punched_t(p, inset=True)


def small_mark(p=LIGHT):
    return punched_t(p)


def placed(body, x, y, size):
    """A 72×72 body scaled to size, top left at x, y."""
    return f'<g transform="translate({x:.2f} {y:.2f}) scale({size / 72:.5f})">{body}</g>'


def icon(bg=PAPER, p=LIGHT, rounded=True, small=False):
    """A square for app slots and touch icons; the mark on a plain background."""
    r = ' rx="16"' if rounded else ""
    body = small_mark(p) if small else mark(p)
    pad = 10 if small else 14
    return svg("0 0 100 100", f'<rect width="100" height="100"{r} fill="{bg}"/>' + placed(body, pad, pad, 100 - 2 * pad))


# --- type ------------------------------------------------------------------------------------------------------------

_FONTS = {}


def font(name):
    if name not in _FONTS:
        f = TTFont(os.path.join(FONTS, name))
        _FONTS[name] = (f.getGlyphSet(), f.getBestCmap(), f["hmtx"], f["head"].unitsPerEm)
    return _FONTS[name]


def text_outline(text, name, size, x, baseline, fill):
    """Text as outlines: font size `size`, starting at x on `baseline`. Returns (svg, advance width, top, bottom)."""
    gs, cmap, hmtx, upem = font(name)
    k = size / upem
    parts, top, bottom = [], baseline, baseline
    for ch in text:
        g = cmap[ord(ch)]
        sp, bp = SVGPathPen(gs), BoundsPen(gs)
        gs[g].draw(TransformPen(sp, (k, 0, 0, -k, x, baseline)))
        gs[g].draw(bp)
        if bp.bounds:
            top, bottom = min(top, baseline - bp.bounds[3] * k), max(bottom, baseline - bp.bounds[1] * k)
        if sp.getCommands():
            parts.append(sp.getCommands())
        x += hmtx[g][0] * k
    return f'<path d="{" ".join(parts)}" fill="{fill}"/>', x, top, bottom


def wordmark(p=LIGHT):
    body, width, top, bottom = text_outline(NAME, WORDMARK_FONT, 100, 0, 100, p["ink"])
    return svg(f"-6 {top - 6:.1f} {width + 12:.1f} {bottom - top + 12:.1f}", body)


def lockup(p=LIGHT, descriptor=True):
    """Mark left, name right – with the descriptor underneath, or the name alone."""
    name, width, top, _ = text_outline(NAME, WORDMARK_FONT, 100, 0, 100, p["ink"])
    if descriptor:
        desc, dwidth, _, dbottom = text_outline(DESCRIPTOR, MONO_FONT, 40, 3, 160, p["sub"])
        height = dbottom - top
    else:
        desc, dwidth, height = "", 0, 100 - top
    size = max(height * 1.08, 96)
    x0 = size + size * 0.28
    y0 = top + height / 2 - size / 2
    w = x0 + max(width, dwidth)
    body = placed(mark(p), 0, y0, size) + f'<g transform="translate({x0:.2f} 0)">{name}{desc}</g>'
    return svg(f"-8 {y0 - 8:.1f} {w + 16:.1f} {size + 16:.1f}", body)


def social():
    """1280×640: GitHub social preview, link previews."""
    name, nw, ntop, _ = text_outline(NAME, WORDMARK_FONT, 120, 0, 0, INK)
    desc, _, _, _ = text_outline(DESCRIPTOR, MONO_FONT, 40, 4, 70, STEEL)
    proto, _, _, _ = text_outline("spec CC BY 4.0 · library MIT", MONO_FONT, 26, 4, 0, STEEL)
    pub, pw, _, _ = text_outline("a falkOSS project", MONO_FONT, 26, 0, 0, STEEL)
    return svg("0 0 1280 640", f'<rect width="1280" height="640" fill="{PAPER}"/>'
               + placed(mark(LIGHT), 120, 200, 240)
               + f'<g transform="translate(420 300)">{name}{desc}</g>'
               + f'<rect x="120" y="520" width="1040" height="2" fill="#C9D1D9"/>'
               + f'<g transform="translate(116 572)">{proto}</g><g transform="translate({1160 - pw:.1f} 96)">{pub}</g>')


def favicon_svg():
    """The punched T, transparent: navy on light tabs, paper on dark ones."""
    style = (f'<style>.i{{fill:{INK}}}.a{{fill:{BLUE}}}'
             f'@media (prefers-color-scheme: dark){{.i{{fill:{PAPER}}}.a{{fill:{BLUE_LIGHT}}}}}</style>')
    body = small_mark().replace(f'fill="{INK}"', 'class="i"').replace(f'fill="{BLUE}"', 'class="a"')
    return svg("0 0 72 72", style + body)


def adaptive(light_svg):
    """The light version with its colours as CSS classes: follows prefers-color-scheme as an <img>; inlined, a page
    can set --logo-ink, --logo-accent and --logo-sub to follow its own light/dark switch."""
    style = (f'<style>.i{{fill:var(--logo-ink,{INK})}}.a{{fill:var(--logo-accent,{BLUE})}}'
             f'.ac{{stroke:var(--logo-accent,{BLUE})}}.s{{fill:var(--logo-sub,{STEEL})}}'
             f'@media (prefers-color-scheme: dark){{.i{{fill:var(--logo-ink,{PAPER})}}.a{{fill:var(--logo-accent,{BLUE_LIGHT})}}'
             f'.ac{{stroke:var(--logo-accent,{BLUE_LIGHT})}}.s{{fill:var(--logo-sub,{MIST})}}}}</style>')
    body = (light_svg.replace(f'fill="{INK}"', 'class="i"').replace(f'fill="{BLUE}"', 'class="a"')
            .replace(f'stroke="{BLUE}"', 'class="ac"').replace(f'fill="{STEEL}"', 'class="s"'))
    head, rest = body.split(">", 1)
    return head + ">" + style + rest


# --- output ----------------------------------------------------------------------------------------------------------

def svg(view_box, body):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}">\n{body}\n</svg>\n'


def write(path, content):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)


def render(src, dst, w, h=None):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    args = ["rsvg-convert", "-w", str(w)] + (["-h", str(h)] if h else []) + ["-o", dst, src]
    subprocess.run(args, check=True)


def ico(dst, pngs):
    """favicon.ico with PNG entries (16, 32, 48)."""
    data = [open(p, "rb").read() for p in pngs]
    out = struct.pack("<HHH", 0, 1, len(data))
    offset = 6 + 16 * len(data)
    for s, d in zip((16, 32, 48), data):
        out += struct.pack("<BBBBHHII", s, s, 0, 0, 1, 32, len(d), offset)
        offset += len(d)
    with open(dst, "wb") as f:
        f.write(out + b"".join(data))


def main():
    for d in (SVG, PNG, WEB):
        if os.path.isdir(d):
            shutil.rmtree(d)   # generated output only
    files = {
        "librotenado-mark-on-light.svg": svg("0 0 72 72", mark(LIGHT)),
        "librotenado-mark-on-dark.svg": svg("0 0 72 72", mark(DARK)),
        "librotenado-mark-black.svg": svg("0 0 72 72", mark(BLACK)),
        "librotenado-mark-white.svg": svg("0 0 72 72", mark(WHITE)),
        "librotenado-mark-small-on-light.svg": svg("0 0 72 72", small_mark(LIGHT)),
        "librotenado-mark-small-on-dark.svg": svg("0 0 72 72", small_mark(DARK)),
        "librotenado-icon.svg": icon(),
        "librotenado-icon-square.svg": icon(rounded=False),
        "librotenado-icon-small.svg": icon(small=True),
        "librotenado-wordmark-on-light.svg": wordmark(LIGHT),
        "librotenado-wordmark-on-dark.svg": wordmark(DARK),
        "librotenado-lockup-on-light.svg": lockup(LIGHT),
        "librotenado-lockup-on-dark.svg": lockup(DARK),
        "librotenado-lockup-name-on-light.svg": lockup(LIGHT, descriptor=False),
        "librotenado-lockup-name-on-dark.svg": lockup(DARK, descriptor=False),
        "librotenado-mark-adaptive.svg": adaptive(svg("0 0 72 72", mark(LIGHT))),
        "librotenado-lockup-adaptive.svg": adaptive(lockup(LIGHT)),
        "librotenado-lockup-name-adaptive.svg": adaptive(lockup(LIGHT, descriptor=False)),
        "librotenado-social-1280x640.svg": social(),
        "favicon.svg": favicon_svg(),
    }
    for name, content in files.items():
        write(os.path.join(SVG, name), content)
    s = lambda n: os.path.join(SVG, n)
    for size in (64, 128, 256, 512, 1024):
        render(s("librotenado-icon.svg"), os.path.join(PNG, "icon", f"librotenado-icon-{size}.png"), size)
        render(s("librotenado-icon-square.svg"), os.path.join(PNG, "icon-square", f"librotenado-icon-square-{size}.png"), size)
    for size in (16, 32, 48):
        render(s("librotenado-icon-small.svg"), os.path.join(PNG, "icon-small", f"librotenado-icon-small-{size}.png"), size)
    for size in (256, 512, 1024):
        for v in ("on-light", "on-dark", "black", "white"):
            render(s(f"librotenado-mark-{v}.svg"), os.path.join(PNG, "mark", f"librotenado-mark-{v}-{size}.png"), size)
    for v in ("on-light", "on-dark"):
        render(s(f"librotenado-wordmark-{v}.svg"), os.path.join(PNG, "wordmark", f"librotenado-wordmark-{v}-1200.png"), 1200)
        render(s(f"librotenado-lockup-{v}.svg"), os.path.join(PNG, "lockup", f"librotenado-lockup-{v}-1200.png"), 1200)
        render(s(f"librotenado-lockup-name-{v}.svg"), os.path.join(PNG, "lockup", f"librotenado-lockup-name-{v}-1200.png"), 1200)
    render(s("librotenado-icon-square.svg"), os.path.join(PNG, "social", "github-avatar-500.png"), 500)
    render(s("librotenado-social-1280x640.svg"), os.path.join(PNG, "social", "librotenado-social-1280x640.png"), 1280, 640)

    # Website head assets (copied into the website repository)
    os.makedirs(WEB, exist_ok=True)
    shutil.copy(s("favicon.svg"), os.path.join(WEB, "favicon.svg"))
    render(s("librotenado-icon-square.svg"), os.path.join(WEB, "apple-touch-icon.png"), 180)
    render(s("librotenado-icon.svg"), os.path.join(WEB, "icon-192.png"), 192)
    render(s("librotenado-icon.svg"), os.path.join(WEB, "icon-512.png"), 512)
    shutil.copy(os.path.join(PNG, "social", "librotenado-social-1280x640.png"), os.path.join(WEB, "social.png"))
    ico(os.path.join(WEB, "favicon.ico"),
        [os.path.join(PNG, "icon-small", f"librotenado-icon-small-{n}.png") for n in (16, 32, 48)])
    write(os.path.join(WEB, "site.webmanifest"),
          '{\n  "name": "librotenado",\n  "short_name": "librotenado",\n  "icons": [\n'
          '    {"src": "/icon-192.png", "sizes": "192x192", "type": "image/png"},\n'
          '    {"src": "/icon-512.png", "sizes": "512x512", "type": "image/png"}\n  ],\n'
          f'  "theme_color": "{PAPER}",\n  "background_color": "{PAPER}",\n  "display": "standalone"\n}}\n')
    print("brand kit written to", HERE)


if __name__ == "__main__":
    main()
