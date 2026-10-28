# 0009 Reports

Version 0.1, draft. Implemented in the reference library (`ReportMapping`, `EventSet::account_report`,
`EventSet::tax_report`).

Books end in national reports: a balance sheet, a profit and loss account, a VAT return, a tax return. Each country
prescribes their positions, and many change every year. A **report mapping** says how the harmonised chart (part
0004) and the tax codes (part 0001) fall into the positions of **one version** of one report.

**The protocol publishes the mappings**, per country, in a new version whenever the report changes – for most tax
forms every year. A mapping is a draft until tax advisers of its country have reviewed it.

## Report mappings

A report mapping is a root object of type `report_mapping`:

| Member | Content |
|---|---|
| `report` | the report, e.g. `de-hgb-266`, `de-ustva` |
| `country` | the country, ISO 3166-1 alpha-2 |
| `version` | the version of the report – a year, a taxonomy version |
| `valid_from` | the first day of the periods it applies to |
| `into` | the harmonised chart it maps, as `name/version` |
| `status` | `draft` or `reviewed` – reviewed by tax advisers of the country |
| `accounts` | account → position; absent for a report computed from tax codes alone |
| `credit_accounts` | account → position for a credit balance, where it differs; absent when none |
| `vat_accounts` | `output` and `input`: the accounts that hold VAT owed and deductible VAT |
| `taxes` | per tax code, the fields it fills (below); absent for a report computed from accounts alone |

### Reports from accounts

- **An account falls into the position of its own mapping or of the nearest account it refines.** One entry for a
  group covers every account below it – of the chart and opened in the books – unless one of them has its own.
- **The side can follow the balance.** Where `credit_accounts` names a position for an account, a credit balance
  falls there: a bank account in overdraft is a liability to a credit institution, not cash.
- The value of a position on a date is the sum of the balances (part 0004) of the accounts that fall into it, for one
  owner (part 0008), in base units, debit positive. Presenting it – signs, totals, subtotals – is a matter of the
  report.

### Reports from tax codes

A VAT return depends on the tax treatment of each posting, not on the account alone. A rule in `taxes` is `tax` – a
tax code – and the fields it fills:

| Member | Postings with the code … | Value |
|---|---|---|
| `base` | on accounts that hold no VAT | the net amount: credit positive for sales, debit positive for purchases |
| `output` | on the output VAT accounts | VAT owed, credit positive |
| `input` | on the input VAT accounts | deductible VAT, debit positive |

An account holds VAT if it is one of `vat_accounts` or refines one. The value of a field for a period is the sum over
the bookings dated within it, for one owner. A reverse charge purchase fills all three: the net amount, the VAT the
recipient owes, and the same VAT deducted.

## The German reports of 2026

| Report | File | Source |
|---|---|---|
| Balance sheet, HGB § 266 | [`de-hgb-266-2026.json`](data/reports/de-hgb-266-2026.json) | [§ 266 HGB](https://www.gesetze-im-internet.de/hgb/__266.html) |
| Profit and loss account by nature of expense, HGB § 275 Abs. 2 | [`de-hgb-275-gkv-2026.json`](data/reports/de-hgb-275-gkv-2026.json) | [§ 275 HGB](https://www.gesetze-im-internet.de/hgb/__275.html) |
| VAT return 2026 (Umsatzsteuer-Voranmeldung) | [`de-ustva-2026.json`](data/reports/de-ustva-2026.json) | BMF letter of 29 December 2025, III C 3 - S 7344/00039/007/036, form USt 1 A ([BMF](https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Umsatzsteuer/2025-12-29-vordruckmuster-USt-voranmeldung-2026.html)) |

- **Positions of the HGB** are written as their place in the law: `aktiv.B.II.1` is § 266 Abs. 2 B. II. 1.
  (receivables from trade), `passiv.C.2` § 266 Abs. 3 C. 2., `gkv.6.a` § 275 Abs. 2 Nr. 6 Buchst. a. Positions the
  HGB does not list, which a French account may still carry – formation expenses, regulated provisions, investment
  grants – are written `aktiv.extra.…` and `passiv.extra.…`; a German set of books does not use those accounts.
- **Fields of the VAT return** are its Kennzahlen (`81`, `66`, …). The fields the form computes or asks for directly –
  the totals, the advance payment (Kz 83), the special advance payment (Kz 39), the yes/no questions – are not part of
  the mapping.
- Every account of the harmonised chart 0.1 that is an asset, a liability or equity has a position of § 266; every
  account of income or expense one of § 275 Abs. 2.

## Open points

- Review of the German mappings by tax advisers; until then they are drafts.
- The profit and loss account by function of expense (§ 275 Abs. 3), which needs costs by function, not by account.
- The notes the HGB requires on positions (§ 268 Abs. 4 and 5: remaining terms; "davon" items), and the shortened
  balance sheets of small and micro companies (§ 266 Abs. 1, § 267, § 267a) – the latter follow from the positions by
  dropping levels.
- The Anlage EÜR (§ 4 Abs. 3 EStG, form of 2025 per BMF letter of 29 August 2025): its Kennzahlen are unique only
  within their section, so its fields need the section in their key.
- The E-Bilanz (§ 5b EStG, taxonomy 6.9 for financial years from 2026, 6.10 from 2027): positions by element
  identifier only, until the conditions for reusing the taxonomy (© XBRL Deutschland e.V.) are clarified.
- The recapitulative statement (§ 18a UStG), which needs the customer's VAT identification number per posting or
  document.
- Whether the official forms published with BMF letters may be reproduced (§ 5 UrhG); the mappings carry only their
  field numbers.
