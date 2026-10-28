# 0009 Reports

Version 0.1, draft. Implemented in the reference library (`ReportMapping`, `EventSet::account_report`,
`EventSet::period_report`).

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
| `vat_settlement` | `debit` and `credit`: the fields for VAT paid to and refunded by the tax office |
| `taxes` | per tax code, the fields it fills (below); absent for a report computed from accounts alone |

### Reports from accounts

- **An account falls into the position of its own mapping or of the nearest account it refines.** One entry for a
  group covers every account below it – of the chart and opened in the books – unless one of them has its own.
- **The side can follow the balance.** Where `credit_accounts` names a position for an account, a credit balance
  falls there: a bank account in overdraft is a liability to a credit institution, not cash.
- **A report on a date** – a balance sheet – takes the balances (part 0004) of the accounts that fall into a position,
  for one owner (part 0008), in base units, debit positive. Presenting it – signs, totals, subtotals – is a matter of
  the report.

### Reports over a period

A report over a period – a profit and loss account, a VAT return, an income statement on a cash basis – takes the
postings of one owner in the bookings dated within the period, one by one:

1. **With a tax code that has a rule** (below): its field, and nothing else – unless the rule has no `base` and the
   posting is on no VAT account; then the posting falls to 3.
2. **Without a tax code, on a VAT account**: the field of `vat_settlement` for its side – a payment to the tax office
   on the debit, a refund on the credit.
3. **Otherwise**: the position of its account. Its amount counts positive on the normal side of the account's kind –
   income credit, expenses debit; where `credit_accounts` distinguishes the sides of an account, on the side mapped.

#### Tax codes

A VAT return depends on the tax treatment of each posting, not on the account alone. A rule in `taxes` is `tax` – a
tax code – and the fields it fills:

| Member | Postings with the code … | Value |
|---|---|---|
| `base` | on accounts that hold no VAT | the net amount: credit positive for sales, debit positive for purchases |
| `output` | on the output VAT accounts | VAT owed, credit positive |
| `input` | on the input VAT accounts | deductible VAT, debit positive |

An account holds VAT if it is one of `vat_accounts` or refines one. A reverse charge purchase fills all three: the
net amount, the VAT the recipient owes, and the same VAT deducted.

## The German reports of 2026

| Report | File | Source |
|---|---|---|
| Balance sheet, HGB § 266 | [`de-hgb-266-2026.json`](data/reports/de-hgb-266-2026.json) | [§ 266 HGB](https://www.gesetze-im-internet.de/hgb/__266.html) |
| Profit and loss account by nature of expense, HGB § 275 Abs. 2 | [`de-hgb-275-gkv-2026.json`](data/reports/de-hgb-275-gkv-2026.json) | [§ 275 HGB](https://www.gesetze-im-internet.de/hgb/__275.html) |
| Income statement on a cash basis 2025 (Anlage EÜR, § 4 Abs. 3 EStG) | [`de-euer-2025.json`](data/reports/de-euer-2025.json) | BMF letter of 29 August 2025, IV C 6 - S 2142/00023/010/001 ([BMF](https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Einkommensteuer/2025-08-29-anlage-EUER-2025.html)) |
| VAT return 2026 (Umsatzsteuer-Voranmeldung) | [`de-ustva-2026.json`](data/reports/de-ustva-2026.json) | BMF letter of 29 December 2025, III C 3 - S 7344/00039/007/036, form USt 1 A ([BMF](https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Umsatzsteuer/2025-12-29-vordruckmuster-USt-voranmeldung-2026.html)) |

- **Positions of the HGB** are written as their place in the law: `aktiv.B.II.1` is § 266 Abs. 2 B. II. 1.
  (receivables from trade), `passiv.C.2` § 266 Abs. 3 C. 2., `gkv.6.a` § 275 Abs. 2 Nr. 6 Buchst. a. Positions the
  HGB does not list, which a French account may still carry – formation expenses, regulated provisions, investment
  grants – are written `aktiv.extra.…` and `passiv.extra.…`; a German set of books does not use those accounts.
- **Fields of the VAT return** are its Kennzahlen (`81`, `66`, …). The fields the form computes or asks for directly –
  the totals, the advance payment (Kz 83), the special advance payment (Kz 39), the yes/no questions – are not part of
  the mapping.
- **Fields of the Anlage EÜR** are its section and Kennzahl (`1.112`, `2.280`, `5.122`): the same Kennzahl appears in
  more than one section. Income falls into its line by its tax code – taxable, exempt or zero-rated, small business –,
  expenses by their account; VAT received and deductible VAT by tax code, VAT paid to and refunded by the tax office by
  `vat_settlement`; withdrawals and contributions of the proprietor by the side of their account. The EÜR counts
  receipts and payments when they occur (§ 11 EStG), so a set of books that determines its profit this way dates its
  bookings when paid. The lines the form computes – totals, the profit, its corrections – are not part of the
  mapping.
- Every account of the harmonised chart 0.1 that is an asset, a liability or equity has a position of § 266; every
  account of income or expense one of § 275 Abs. 2; every account of expense a line of the Anlage EÜR.

## Open points

- Review of the German mappings by tax advisers; until then they are drafts.
- The profit and loss account by function of expense (§ 275 Abs. 3), which needs costs by function, not by account.
- The notes the HGB requires on positions (§ 268 Abs. 4 and 5: remaining terms; "davon" items), and the shortened
  balance sheets of small and micro companies (§ 266 Abs. 1, § 267, § 267a) – the latter follow from the positions by
  dropping levels.
- The Anlage EÜR: the annex for depreciation (Anlage AVEÜR), which the lines of depreciation are carried from; the
  split of business entertainment into its deductible and non-deductible part (§ 4 Abs. 5 Satz 1 Nr. 2 EStG), which
  the books record on two accounts; the 10-day rule for regular payments (§ 11 Abs. 1 Satz 2, Abs. 2 Satz 2 EStG).
- The E-Bilanz (§ 5b EStG, taxonomy 6.9 for financial years from 2026, 6.10 from 2027): positions by element
  identifier only, until the conditions for reusing the taxonomy (© XBRL Deutschland e.V.) are clarified.
- The recapitulative statement (§ 18a UStG), which needs the customer's VAT identification number per posting or
  document.
- Whether the official forms published with BMF letters may be reproduced (§ 5 UrhG); the mappings carry only their
  field numbers.
