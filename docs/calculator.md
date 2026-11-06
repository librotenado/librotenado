---
title: Calculators
parent: Rulebook
nav_order: 1
permalink: /rules/calculator.html
---

# Calculators

The rules of the library, to try out: each form is a case the library computes – compiled to WebAssembly and run in
this page, with the same code and the same [values of the law](de-values.html) as everywhere else. Nothing is sent
anywhere. Amounts in euros; fields left empty count as zero.

Every result is a **draft** until tax advisers have reviewed the rules. A result that does not hold is a finding:
please report it with the case ([issues](https://github.com/librotenado/librotenado/issues)).

<div data-calculator-module="{{ '/assets/calculator.wasm' | relative_url }}"></div>

## Payslip of a month

The wage tax by the Programmablaufplan of the BMF with all its intermediate values, the social insurance, net pay.

<form data-calculation="payslip">
<p>
<label>Jahr <input name="year" value="2026" size="5"></label>
<label>Monat <input name="month" value="1" size="3"></label>
<label>Lohn <input name="wage" value="4000.00" size="10"></label>
<label>Einmalzahlung <input name="one_off" size="10"></label>
</p>
<p>
<label>Steuerklasse <input name="class" value="1" size="2"></label>
<label>Kinderfreibeträge (Zehntel) <input name="child_allowances" value="0" size="3"></label>
<label>Kirche <select name="church"><option value="">keine</option><option value="ev">ev</option><option value="rk">rk</option></select></label>
<label>Bundesland <input name="region" value="DE-NW" size="6"></label>
</p>
<p>
<label>Beitragsgruppen <input name="groups" value="1111" size="5"></label>
<label>Personengruppe <input name="person_group" value="101" size="4"></label>
<label>Zusatzbeitrag % <input name="additional_rate" value="2.9" size="5"></label>
<label><input type="checkbox" name="childless"> kinderlos (PV)</label>
<label>Kinder unter 25 (PV) <input name="care_children" value="0" size="2"></label>
<label><input type="checkbox" name="saxony"> Sachsen</label>
</p>
<button type="submit">Berechnen</button>
</form>
<div class="calculator-output"></div>

## Income tax of a person running a business

Profit of the business, trade tax and its reduction (§ 35 EStG), the tariff, children, capital income.

<form data-calculation="income_tax">
<p>
<label>Jahr <input name="year" value="2026" size="5"></label>
<label>Gewinn <input name="profit" value="60000.00" size="10"></label>
<label>Hebesatz (0 = freiberuflich) <input name="multiplier" value="0" size="4"></label>
<label><input type="checkbox" name="joint"> zusammen veranlagt</label>
</p>
<p>
<label>Andere Einkünfte <input name="other_income" size="10"></label>
<label>Abzüge <input name="deductions" size="10"></label>
<label>Verlustvortrag <input name="losses" size="10"></label>
<label>Kirchensteuer % <input name="church_rate" value="0" size="2"></label>
</p>
<p>
<label>Kinderfreibeträge (Zehntel) <input name="child_allowances" value="0" size="3"></label>
<label>Kindergeld im Jahr <input name="child_benefit" size="8"></label>
<label>Alleinerziehend, Kinder <input name="single_parent_children" value="0" size="2"></label>
<label>Außerordentliche Einkünfte (§ 34) <input name="extraordinary" size="10"></label>
<label>Kapitalerträge (§ 32d) <input name="capital_income" size="10"></label>
</p>
<button type="submit">Berechnen</button>
</form>
<div class="calculator-output"></div>

## Corporation tax and trade tax

The taxable income of a corporation and its trade tax, from its result and the expenses the rules treat apart.

<form data-calculation="profit_tax">
<p>
<label>Rechtsform <select name="subject"><option value="corporation">Kapitalgesellschaft</option><option value="individual">Einzelunternehmen</option><option value="partnership">Personengesellschaft</option><option value="association">Verein</option></select></label>
<label>Jahr <input name="year" value="2026" size="5"></label>
<label>Jahresergebnis <input name="profit" value="200000.00" size="12"></label>
<label>Hebesatz <input name="multiplier" value="400" size="4"></label>
</p>
<p>
<label>Nicht abziehbare Aufwendungen <input name="excluded" size="10"></label>
<label>Zinsaufwand <input name="interest" size="10"></label>
<label>Miete für Grundbesitz <input name="rent_immovable" size="10"></label>
<label>Verdeckte Gewinnausschüttung <input name="hidden_distributions" size="10"></label>
<label>Verlustvortrag <input name="income_losses" size="10"></label>
<label>Gewerbeverlust <input name="trade_losses" size="10"></label>
</p>
<button type="submit">Berechnen</button>
</form>
<div class="calculator-output"></div>

<script type="module" src="{{ '/assets/js/calculator.js' | relative_url }}"></script>
