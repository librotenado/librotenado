# 0013 Payroll

Version 0.22, draft. Implemented in the reference library (module `payroll`: `Employment`, `Payslip`, event kinds
`employment_described` and `payslip_recorded`; `EventSet::employments`, `EventSet::payslips`,
`EventSet::recorded_payslips`, `EventSet::review_payroll`, `EventSet::payslip_figures`,
`EventSet::contribution_statement`, `EventSet::wage_tax_certificate`, `payroll::wage_tax`).

An owner that employs people pays wages and, on them, withholds the employee's taxes and contributions and pays its
own. The books keep the **employments** – who works for the owner, on what terms the law asks about – and a
**payslip** per employment and period: what the wages are made of, and the figures withheld and paid. The figures are
computed by the rules of the country and recorded as computed; the filings of the employer – in Germany the
Lohnsteuer-Anmeldung and the Beitragsnachweis – and the bookings follow from them.

## Employments

`employment_described` records an employment (type `employment`):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001) |
| `owner` | the employer, an owner of the books (part 0008) |
| `employee` | the employee, a person (part 0008) |
| `from` | the first day this description applies |
| `start` | the first day of the employment |
| `end` | its last day; absent while it lasts |
| `number` | the employer's number for the employee; absent when none |
| `born` | the employee's day of birth; absent when not given |
| `de` | what German law asks about it (below); absent for an employment not under it |

A later description with the same identity replaces the earlier from its `from`: the description that applies on a day
is the one with the latest `from` not after it, the later in the agreed order where two share it. It keeps the
employer and the employee.

## Payslips

`payslip_recorded` records the wages of an employment for a period (type `payslip`):

| Member | Content |
|---|---|
| `id` | its identity, a UUID |
| `employment` | the employment |
| `from`, `to` | the period the wages are for: a calendar month, or the part of it the employment lasted |
| `paid` | the day the wages are paid – for a correction, the day the difference is paid or withheld |
| `replaces` | the payslip of the same employment and period a correction replaces; absent otherwise |
| `items` | what the wages are made of, at least one: `kind`, `name` as the payslip shows it, `amount` |
| `figures` | what was withheld and what the employer pays, by name (below), in the currency of the wages |

The kinds of items:

| `kind` | What it is | Germany |
|---|---|---|
| `wage` | current wages of the period – salary, hourly wages, regular allowances | taxed as current wages, insured |
| `one_off` | a one-off payment – a bonus, holiday or Christmas pay | taxed as a sonstiger Bezug, insured as an Einmalzahlung |
| `compensation` | compensation for the loss of the employment | taxed as a sonstiger Bezug by the fifth (§ 24 Nr. 1, § 34 EStG), not insured |
| `in_kind` | a benefit in kind | taxed and insured like wages, not paid out |
| `tax_free` | paid free of tax and contributions – a reimbursement of expenses | |
| `deduction` | deducted from the net pay – an advance, an attachment | |
| `pension` | a pension paid by the employer – a Werkspension | taxed as current wages with the allowance for pensions (§ 19 Abs. 2 EStG); its health and care contributions the pensioner bears (below) |

A payslip records the figures as they were computed and is never changed. A **correction** (Rückrechnung) is a payslip
of the same employment and period that `replaces` the one that applied: from then on it applies in its place – in the
payslips of later periods, the returns of its period and the certificate of the year –, and the difference is paid or
withheld on its day `paid`. The payslips of later periods computed from the replaced one then show other figures than
the rules give, and are corrected in turn. An implementation SHOULD compute the figures with the library's rules and
MUST report a payslip that applies whose figures the rules would compute otherwise.

An implementation MUST report, and MUST NOT silently resolve:

- **Not an owner** – an employment with someone who is not an owner of the books.
- **Changed** – a later description of an employment naming another employer or employee.
- **Unknown employment** – a payslip of an employment not described (it may not have arrived yet).
- **Duplicate** – a payslip recorded twice under one identity; the first in the agreed order applies.
- **Period** – a payslip whose period ends before it begins, lies outside the employment or spans two calendar months.
- **Overlap** – a payslip whose period overlaps that of another of the same employment.
- **Correction** – a correction of a payslip not recorded, of another employment or period, or replaced already.
- **Figures** – a payslip that applies whose figures the rules compute otherwise from the books as they are.

## Germany

### The employment

`de` holds `tax` and `insurance`.

`tax` – the employee's features for the wage tax as the tax authority gives them electronically (ELStAM,
[§ 39e EStG](https://www.gesetze-im-internet.de/estg/__39e.html)):

| Member | Content |
|---|---|
| `class` | the tax class, 1 to 6; absent where the wages are taxed at the flat rate |
| `factor` | the factor of class 4 (§ 39f EStG), three places; absent without |
| `children` | the number of child allowances, one place (`"1.5"`); absent when none |
| `church` | the religious community that levies church tax, as the ELStAM name it (`ev`, `rk`, …); absent when none |
| `allowance` | the allowance (Freibetrag), `year` and `month`; absent when none |
| `addition` | the addition (Hinzurechnungsbetrag), `year` and `month`; absent when none |
| `flat` | `true` where the wages of a minor employment are taxed at the flat rate of 2 % instead ([§ 40a Abs. 2 EStG](https://www.gesetze-im-internet.de/estg/__40a.html)) |
| `short_term` | `true` where the wages of a short-term employment are taxed at the flat rate of 25 % instead (§ 40a Abs. 1 EStG) |
| `pension_start` | for pensions paid by the employer: `year`, the year they began, and `monthly`, the pension of the first whole month – the base of the allowance for pensions ([§ 19 Abs. 2 EStG](https://www.gesetze-im-internet.de/estg/__19.html)) |
| `active_pension` | `true` for work after the month the employee reached the standard retirement age, for which the employer pays pension contributions: 2,000 euros a month free of tax ([§ 3 Nr. 21 EStG](https://www.gesetze-im-internet.de/estg/__3.html), Aktivrente; its first year under § 52 EStG is OPEN, applied from 2026) |

`insurance` – how the employee is insured, as the employer reports it:

| Member | Content |
|---|---|
| `insurer` | the company number (Betriebsnummer) of the health insurer that collects the contributions – for a minor employment the Minijob-Zentrale |
| `contribution_group` | the key of the contribution groups (Beitragsgruppenschlüssel), four digits for health, pension, unemployment and care insurance |
| `person_group` | the key of the group of persons (Personengruppenschlüssel), three digits |
| `additional_rate` | the insurer's additional rate in percent (§ 242 SGB V); absent when the employee is not in the statutory health insurance |
| `private` | for an employee insured privately: `premium`, the monthly premium for basic health and compulsory care insurance, and `subsidy`, the employer's subsidy to it; absent otherwise |
| `childless` | `true` where the employee pays the surcharge of the care insurance for having no children |
| `children` | the children under 25 the care insurance counts; the second to the fifth reduce the employee's rate |
| `saxony` | `true` where the employee works in Saxony |
| `u1`, `u2` | the rates of the levies for sick pay and maternity of the insurer in percent ([§ 7 AAG](https://www.gesetze-im-internet.de/aag/__7.html)); each absent where the employer does not take part |
| `household` | `true` for a minor employment in a private household ([§ 8a SGB IV](https://www.gesetze-im-internet.de/sgb_4/__8a.html)) |

The person group decides the kind of employment: `109` a minor employment, `110` a short-term one (§ 8 Abs. 1 Nr. 1, 2
SGB IV; Geringfügigkeits-Richtlinien of 5 January 2026,
[Minijob-Zentrale](https://www.minijob-zentrale.de/SharedDocs/Downloads/DE/Rundschreiben/Geringfuegigkeitsrichtlinien-2026.pdf?__blob=publicationFile)),
`121` a trainee with pay up to 325 euros a month, any other one subject to insurance by its contribution groups – an
employee past the standard retirement age with a full pension, for instance, by the employer's half alone in pension
(`3`) and unemployment (`2`) insurance ([§ 172 Abs. 1 SGB VI](https://www.gesetze-im-internet.de/sgb_6/__172.html),
[§ 346 Abs. 3 SGB III](https://www.gesetze-im-internet.de/sgb_3/__346.html)).

### The wage tax

The wage tax, the solidarity surcharge and the base of church tax are computed by the **Programmablaufplan** (PAP) of
the year, which the BMF publishes for the machine computation of the wage tax
([§ 39b Abs. 6 EStG](https://www.gesetze-im-internet.de/estg/__39b.html)), as XML pseudocode at
[bmf-steuerrechner.de](https://www.bmf-steuerrechner.de/interface/pseudocodes.xhtml). The reference library translates
the pseudocode of 2025 (version 1.0, of 17 September 2025) and of 2026 (version 1.0, of 23 October 2025) mechanically
and tests the translation against the same pseudocode run by an interpreter (`scripts/pap.py`), 600 cases a year; the
cases were also checked against the BMF's interface for programme tests. Earlier years are not part of this version
(OPEN).

Inputs of the PAP from a payslip and its employment: the period (`LZZ`, 2 for a month); the current wages – items
`wage` and `in_kind` – (`RE4`); one-off payments – `one_off` and `compensation` – (`SONSTB`), with `compensation` also
as `SONSTENT`; for one-off payments the wages expected for the year (`JRE4`): the current wages of the payslips before
in the year, those of the period for each month left in it, and the one-off payments paid before in the year (OPEN: an
employment that ends in the year); part of a month by the daily tables (`LZZ` 4) on the wages per calendar day; the
tax features
(`STKL`, `f`, `ZKF`, `R`, `JFREIB`, `LZZFREIB`, `JHINZU`, `LZZHINZU`); the age relief (`ALTER1`, `AJAHR`) from `born`;
and the insurance for the deduction of contributions (`KVZ`, `PKV`, `PKPV`, `PKPVAGZ`, `PVZ`, `PVA`, `PVS`, `KRV`,
`ALV`) from `insurance`. Pensions (Versorgungsbezüge) are not part of this version (OPEN).

Church tax is the base × the rate of the state the employer's establishment is in – 8 % in Bavaria and
Baden-Württemberg, 9 % elsewhere (the church tax laws of the states) –, fractions of a cent left out; the state is the
`region` of the employer's configuration (part 0008).

Wages of a minor employment taxed at the flat rate bear 2 % on the wages, which include the solidarity surcharge and
church tax (§ 40a Abs. 2 EStG); the employer bears it unless agreed otherwise. Wages of a short-term employment taxed
at the flat rate bear 25 % (`flat_wage_tax`) and the solidarity surcharge of 5.5 % on it (`flat_solidarity`,
[§ 3 Abs. 1 Nr. 3, § 4 SolZG](https://www.gesetze-im-internet.de/solzg_1995/__3.html)) – where the employment lasts at
most 18 days and the wages are at most 150 € a day (§ 40a Abs. 1 EStG), counted in calendar days of the employment;
the limit of 19 € an hour (Abs. 4 Nr. 1) and church tax on the flat tax, which the states set, are OPEN.

### The social insurance

The contributions of a month follow from the key of the contribution groups – per branch `0` none, `1` the full
contribution, `3` the reduced (health) or the employer's half alone (pension), `2` the employer's half alone
(unemployment, care), `6` and `5` the employer's flat rates of a minor employment (health, pension) – and the values of
the year. For 2026:

| Value | 2026 | Source |
|---|---|---|
| Contribution ceiling, health and care | 69,750 € a year (5,812.50 a month) | [SVBezGrV 2026](https://www.gesetze-im-internet.de/svbezgrv_2026/BJNR1160A0025.html) § 2 Abs. 2 |
| Contribution ceiling, pension and unemployment | 101,400 € a year (8,450 a month) | SVBezGrV 2026 § 4 Abs. 1 Nr. 1 |
| Health, general and reduced rate | 14.6 %, 14.0 %, in halves | [§§ 241](https://www.gesetze-im-internet.de/sgb_5/__241.html), [243](https://www.gesetze-im-internet.de/sgb_5/__243.html), 249 SGB V |
| Additional rate of the insurer | as given, in halves; 2.9 % on average in 2026 | § 242 SGB V; BAnz AT 10.11.2025 B7 |
| Care | 3.6 %, in halves; without children 0.6 % more, from the second to the fifth child under 25 0.25 % less each, the employee's alone; in Saxony the employee 1 % more and the employer 1 % less | [§ 1 PBAV 2025](https://www.gesetze-im-internet.de/pbav_2025/__1.html), [§§ 55](https://www.gesetze-im-internet.de/sgb_11/__55.html), [58 SGB XI](https://www.gesetze-im-internet.de/sgb_11/__58.html) |
| Pension | 18.6 %, in halves | [RVBeitrSBek 2026](https://www.gesetze-im-internet.de/rvbeitrsbek_2026/BJNR1230A0025.html) |
| Unemployment | 2.6 %, in halves | [§ 341 SGB III](https://www.gesetze-im-internet.de/sgb_3/__341.html) |
| Insolvency levy (U3) | 0.15 % | [§ 360 SGB III](https://www.gesetze-im-internet.de/sgb_3/__360.html) |
| Minor employment | up to 603 € a month; employer 13 % health, 15 % pension; with compulsory pension insurance the employee tops up to 18.6 % on at least 175 €; U1 0.8 %, U2 0.22 % at the Minijob-Zentrale | [§ 8 SGB IV](https://www.gesetze-im-internet.de/sgb_4/__8.html), [§ 249b SGB V](https://www.gesetze-im-internet.de/sgb_5/__249b.html), [§ 168](https://www.gesetze-im-internet.de/sgb_6/__168.html), § 163 Abs. 8 SGB VI, [Minijob-Zentrale](https://www.minijob-zentrale.de/DE/fuer-gewerbetreibende/abgaben-und-steuern) |
| Transition range | 603.01 € to 2,000 € a month, factor F 0.6619 | [§ 20 Abs. 2, 2a SGB IV](https://www.gesetze-im-internet.de/sgb_4/__20.html); F: BAnz AT 18.12.2025 B5 (OPEN: not read in the original, the value as the insurers publish it) |

The bases:

- **Current wages** – items `wage` and `in_kind` – count up to the ceiling of the days insured: 30 for a whole month,
  else the calendar days of the period ([§ 1 Abs. 1 BVV](https://www.gesetze-im-internet.de/beitrvv/__1.html)).
- **One-off payments** – `one_off` – count up to what the ceiling of the days of the year so far leaves after the
  bases of the payslips of the year before and the current wages of the month (§ 23a Abs. 3 SGB IV).
- **Märzklausel** ([§ 23a Abs. 4, 5 SGB IV](https://www.gesetze-im-internet.de/sgb_4/__23a.html)): a one-off payment
  from January to March that, with the bases of the year so far, exceeds the ceiling of those months – of health where
  the employee is insured in it, else of pension – belongs to the last month of the year before, where the employment
  had payslips in it: it counts up to what the ceiling of that year leaves, by the values of that year and the
  employment as described on its last day; its bases are `health_base_previous` and `pension_base_previous`, its
  contributions part of the payslip's. The values of 2025: ceilings of 66,150 € and 96,600 €
  ([SVBezGrV 2025](https://www.gesetze-im-internet.de/svbezgrv_2025/__2.html), § 4), minor limit 556 €, factor F 0.6683
  and U1 1.1 % at the Minijob-Zentrale (as the insurers publish them), the rates otherwise those of 2026.
- `compensation` and `tax_free` are not insured.

Each share is the base × the rate, rounded half up to a cent, without rounding what leads to it
([§ 1 Abs. 2, § 2 BVV](https://www.gesetze-im-internet.de/beitrvv/__2.html)): a contribution in halves is the half
rate rounded, for each side; a share of one side alone is its rate rounded. In the **transition range** – a whole
month with wages above the minor limit up to 2,000 € – the total is twice the half rate × the reduced base
`BE = F × G + (2000 / (2000 − G) − G / (2000 − G) × F) × (AE − G)`, the employee's share the half rate × `BE_AN =
2000 / (2000 − G) × (AE − G)`, the employer's the difference; the surcharge without children on `BE`, the reductions
for children on `BE_AN` (§ 2 Abs. 2 BVV). For part of a month, for Saxony and for one-off payments in it the rules of
the range are OPEN. In a **minor employment** the employer's flat rates are on the wages; with compulsory pension
insurance the total of 18.6 % on at least 175 € less the employer's 15 % is the employee's (§ 2 Abs. 3 BVV). In a
**private household** the employer pays 5 % health and 5 % pension, the accident insurance of 1.6 % (`accident`), U1
and U2 and no insolvency levy; the Minijob-Zentrale collects them by the Haushaltsscheck (§ 28a Abs. 7 SGB IV). A
**short-term employment** (person group 110, contribution groups 0000) bears no contributions; at the Minijob-Zentrale
U1 only where it lasts more than four weeks, U2 and the insolvency levy always (Geringfügigkeits-Richtlinien 2026,
section G). For a **trainee with low pay** (person group 121) the employer bears every contribution while the pay
of the month stays within 325 euros, health with the average additional rate of 2.9 %; of what a one-off payment adds
above it the employee bears half ([§ 20 Abs. 3 SGB IV](https://www.gesetze-im-internet.de/sgb_4/__20.html), [§ 242
Abs. 3 Nr. 6 SGB V](https://www.gesetze-im-internet.de/sgb_5/__242.html)); that the employer also bears the surcharge
without children is the reading of the commentaries (OPEN). **Pensions** paid by the employer are not wages of the
social insurance: the pensioner bears health – the general rate with the insurer's additional rate – and care in full,
which the employer withholds as paying agency (`pension_health`, `pension_care`;
[§§ 248, 250, 256 SGB V](https://www.gesetze-im-internet.de/sgb_5/__256.html), § 59 SGB XI); nothing is due up to a
twentieth of the reference value, 197.75 euros in 2026, and for health that twentieth is deducted from company
pensions ([§ 226 Abs. 2 SGB V](https://www.gesetze-im-internet.de/sgb_5/__226.html)). They are paid in the procedure of
paying agencies, by the 15th of the following month; the Beitragsnachweis shows them as `zahlstelle_kv`,
`zahlstelle_pv`.

The levies U1 and U2 are on the current wages up to the pension ceiling, U3 on the base of the pension insurance
([§ 7 AAG](https://www.gesetze-im-internet.de/aag/__7.html), [§ 358 SGB III](https://www.gesetze-im-internet.de/sgb_3/__358.html);
OPEN: the bases as stated here are not yet checked against the text of both), in the transition range on the wages
themselves (OPEN).
An employee insured privately gets the employer's subsidy as given, free of tax and contributions (§ 3 Nr. 62 EStG;
OPEN: its limits under § 257 SGB V, § 61 SGB XI).

### The figures

A German payslip names its figures – in euros, each absent when zero:

| Figure | Content |
|---|---|
| `gross` | all items but `deduction` |
| `wage_tax`, `solidarity`, `church` | withheld by the PAP; church tax as above |
| `flat_tax` | the flat tax of 2 %, borne by the employer |
| `flat_wage_tax`, `flat_solidarity` | the flat tax of 25 % of a short-term employment and its solidarity surcharge |
| `health_base_previous`, `pension_base_previous` | the bases of a one-off payment attributed to the year before |
| `health_base`, `pension_base` | the bases of health and care, of pension and unemployment insurance |
| `health_employee`, `health_employer`, `health_additional_employee`, `health_additional_employer`, `care_employee`, `care_employer`, `pension_employee`, `pension_employer`, `unemployment_employee`, `unemployment_employer` | the shares |
| `health_flat`, `pension_flat` | the employer's flat rates of a minor employment |
| `u1`, `u2`, `u3` | the levies |
| `accident` | the accident insurance of a private household |
| `pension_health`, `pension_care` | the contributions on pensions, the pensioner's |
| `active_pension_free` | the wages free of tax as Aktivrente |
| `private_subsidy` | the employer's subsidy to a private insurance |
| `net` | `gross` less the taxes and the employee's shares withheld, plus `private_subsidy` |
| `payout` | `net` less `in_kind` and `deduction` |

### Filings

**Lohnsteuer-Anmeldung** ([§ 41a EStG](https://www.gesetze-im-internet.de/estg/__41a.html); the form of 2026, BMF
letter of 21 August 2025, IV C 5 - S 2533/00120/006/018,
[BMF](https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Lohnsteuer/2025-08-14-Muster-Lohnsteuer-Anmeldung-2026.html)):
the mapping [`de-lsta-2026.json`](data/reports/de-lsta-2026.json) sums the figures of the payslips for periods ending
in the period of the return (`payroll_fields`, a report of the kind `Payroll`, part 0009) into Kz 41 (the flat tax of
25 %), 42 (wage tax), 48, 49 (solidarity surcharge), 61 and 62 (church tax `ev`, `rk`), the church taxes of the other
communities as the form of 2026 lists them per state – 63 `ak`; 64 `is`, `jd`, `jh`, `ih`, `issl`; 65 `fm`; 66 `fs`; 67
`fb`; 68 `fg`; 72 `fa`; 73 `iw`; 74 `il`; 78 `ib` – and 83 (the total); a name with `@` counts only payslips of employees
of that religious community. Their rates are taken as those of the large churches of the state (OPEN: the rates of the
smaller communities). A correction changes the return of the period it corrects, which is filed again in full (notes
to the form, Nr. 13). Kz 86, the number of employees, is not an amount: `EventSet::employees_paid`
gives it. The period of the return is a month where the wage tax of the year before was above 5,000 €, a quarter above
1,080 €, else the year; it is due on the tenth day after it (§ 41a Abs. 1, 2 EStG). The flat tax of § 37b (Kz 44), the
deductions of Kz 33 and 45 and the Arbeitnehmerkammer contributions of Bremen and the Saarland are not part of this
version (OPEN); the flat tax of 2 % goes to the Minijob-Zentrale with the contributions.

**Beitragsnachweis** (§ 28f Abs. 3 SGB IV; Gemeinsame Grundsätze of 21 May 2025, valid from 1 January 2026,
[informationsportal.de](https://www.informationsportal.de/wp-content/uploads/GG_BNA_202601.pdf)), per insurer and
month: `EventSet::contribution_statement` sums the shares by the groups of the record – `1000`, `3000`, `6000` health,
`zusatzbeitrag` the additional contribution, `0001` care, `0100`, `0300`, `0500` pension, `0010`, `0020`
unemployment, `0050` the insolvency levy, `u1`, `u2`, and `pauschsteuer` the flat tax of 2 % at the Minijob-Zentrale.
A correction counts in the statement of the month it is paid, as the difference to the payslip it replaces (Gemeinsame
Grundsätze, Ziffer 6). It is due two working days before the contributions, which are due on the third last banking
day of the month (§ 23 Abs. 1 SGB IV). Beitragsnachweise and the notifications of the DEÜV are transmitted only from
programs tested and approved by the GKV-Spitzenverband ([§ 95b SGB IV](https://www.gesetze-im-internet.de/sgb_4/__95b.html),
§§ 19 to 22, 26 DEÜV): a closed application in a version that can be identified. The protocol computes their content;
an implementation that is not approved has its users enter it in the SV-Meldeportal of the insurers (§ 95a SGB IV).

**Lohnsteuerbescheinigung** ([§ 41b EStG](https://www.gesetze-im-internet.de/estg/__41b.html); the form of 2026, BMF
letter of 29 August 2025, IV C 5 - S 2533/00123/007/007): `EventSet::wage_tax_certificate` gives per employment and
calendar year the lines from the payslips that apply for periods ending in it – 3 (wages with benefits in kind and
pensions, without those of line 10 and the Aktivrente), 8 (the pensions in it), 4, 5, 6 (wage tax, solidarity surcharge, the employee's church tax), 10 (compensation), 22a
and 23a (pension, employer and employee), 24b (the subsidy to a private insurance), 25, 26, 27 (the employee's health,
care and unemployment contributions); names `de-lstb/<line>`. Wages taxed at a flat rate have none (§ 41b Abs. 6). It
is transmitted to the tax authority by the last day of February of the year after; the letters of line 2, the church
tax of a spouse (line 7) and the split of line 24 into health and care are OPEN.

### Bookings

`Payslip::postings` gives the postings of a payslip on the harmonised chart (in SKR 03 / SKR 04): the gross pay as
Gehälter (4120 / 6020) or, for a minor employment, Aushilfslöhne (4190 / 6030); what the employer pays on top as
gesetzliche soziale Aufwendungen (4130 / 6110); the flat tax as pauschale Steuer für Aushilfen (4199 / 6036); against
them the liabilities for wage and church tax (1741 / 3730), for social security (1742 / 3740), for the net pay
(1740 / 3720), and benefits in kind and deductions on the Lohn- und Gehaltsverrechnungskonto (1755 / 3790) to be
settled. The flat taxes go to the pauschale Steuer, that of 25 % against the tax liabilities. They balance.
`Payslip::correction_postings` gives those of a correction: its postings less those of the payslip it replaces.

## Open points

- Corrections after the certificate of the year was transmitted (§ 41c Abs. 3 EStG); back pay for earlier periods.
- The notifications to the social insurance (DEÜV), the records of the electronic Lohnkonto (§ 41 EStG, LStDV) and
  their transmission; the approval of the GKV-Spitzenverband.
- Kurzarbeit, Altersteilzeit, the accident insurance of businesses; pensions as one-off payments (Sterbegeld,
  capital payments) and the pensions of the transition range.
- Pensions paid by the employer (Versorgungsbezüge), the Faktorverfahren beyond the factor itself.
- Other countries.
