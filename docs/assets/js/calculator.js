// SPDX-License-Identifier: MIT
// The calculators of the rulebook: each form becomes a case for the library, compiled to WebAssembly, and its result
// with its way of computing a table – the same code that computes everywhere else.
import { runWasi } from "./wasi.js";

const moduleUrl = document.querySelector("[data-calculator-module]").dataset.calculatorModule;
const compiled = WebAssembly.compileStreaming
  ? WebAssembly.compileStreaming(fetch(moduleUrl))
  : fetch(moduleUrl).then((r) => r.arrayBuffer()).then((b) => WebAssembly.compile(b));

function table(title, rows) {
  const section = document.createElement("section");
  const heading = document.createElement("h3");
  heading.textContent = title;
  section.append(heading);
  const t = document.createElement("table");
  t.innerHTML = "<thead><tr><th>Wert</th><th>Bezeichnung</th><th style='text-align:right'>Betrag</th></tr></thead>";
  const body = document.createElement("tbody");
  for (const row of rows) {
    const tr = document.createElement("tr");
    for (const [text, right] of [[row.name, false], [row.label, false], [row.value, true]]) {
      const td = document.createElement("td");
      td.textContent = text;
      if (right) td.style.textAlign = "right";
      tr.append(td);
    }
    body.append(tr);
  }
  t.append(body);
  section.append(t);
  return section;
}

for (const form of document.querySelectorAll("form[data-calculation]")) {
  const output = form.nextElementSibling;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const nameless = { calculation: form.dataset.calculation };
    for (const element of form.elements) {
      if (!element.name) continue;
      nameless[element.name] = element.type === "checkbox" ? element.checked : element.value;
    }
    output.textContent = "…";
    try {
      const result = JSON.parse(await runWasi(await compiled, JSON.stringify(nameless)));
      output.textContent = "";
      if (result.error) {
        output.textContent = result.error;
        return;
      }
      const ok = result.ok;
      output.append(table("Ergebnis (librotenado " + result.version + ")", ok.figures));
      if (ok.trade && ok.trade.length) output.append(table("Gewerbesteuer", ok.trade));
      if (ok.steps) output.append(table(ok.steps_title || "Rechenweg", ok.steps));
    } catch (error) {
      output.textContent = String(error);
    }
  });
}
