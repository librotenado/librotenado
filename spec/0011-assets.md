# 0011 Fixed assets

Version 0.14, draft. Implemented in the reference library (`Asset`, `Disposal`, `Depreciation`, event kinds
`asset_acquired` and `asset_disposed`; `EventSet::assets`, `EventSet::review_assets`, `EventSet::asset_report`).

What an owner acquires to use for longer than a year – a laptop, a vehicle, a building – is not an expense when it is
paid: its cost is spread over the years it is used. The books keep a **register of fixed assets**: each asset with
its cost and how it is depreciated, and when it leaves the business. Depreciation, book values and the reports of the
register follow from it; the bookings of depreciation are an implementation's (below).

## Assets

`asset_acquired` records an asset (type `asset`):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001) |
| `owner` | the owner whose asset it is (part 0008) |
| `name` | what it is |
| `account` | the account of fixed assets it is kept on (part 0004) |
| `acquired` | the day it was acquired, or made or brought into the business |
| `cost` | its cost of acquisition or production, or the value it was brought in at – in the owner's functional currency, without deductible VAT |
| `depreciation` | how it is depreciated (below) |

`depreciation` is an object with `method`:

| `method` | Members | Depreciation |
|---|---|---|
| `straight_line` | `months`, the useful life, at least 1 | the cost in equal parts per month, from the month it was acquired |
| `immediate` | | all of it in the year it was acquired – a low-value asset |
| `pool` | | a fifth in the year it was acquired and in each of the four years after it |
| `none` | | none – land, financial assets |

`asset_disposed` records that an asset left the business – sold, withdrawn, scrapped – (type `disposal`): `asset`,
its identity, `date`, and `proceeds`, what it was sold for without VAT, absent when it was not sold.

## Depreciation

The depreciation accumulated up to a day, in base units of the cost:

- **Straight line:** the cost × the months from the month acquired to the month of the day, both counted, at most
  `months` – and no further than the month the asset left –, ÷ `months`, rounded half away from zero on the whole, so
  that the parts add up to the cost. In Germany the year of acquisition is reduced by a twelfth for each full month
  before the month of acquisition ([§ 7 Abs. 1 Satz 4 EStG](https://www.gesetze-im-internet.de/estg/__7.html)); the
  months of the year it leaves count up to and including the month it leaves (OPEN: to be checked against R 7.4
  Abs. 8 EStR).
- **Immediate:** the whole cost, from the day acquired. In Germany for movable assets of at most 800 euros without
  VAT ([§ 6 Abs. 2 EStG](https://www.gesetze-im-internet.de/estg/__6.html)).
- **Pool:** the cost × the calendar years from the year acquired to the year of the day, both counted, at most five,
  ÷ 5, rounded the same way – whether the asset left or not. In Germany for movable assets above 250 and up to 1,000
  euros, one pool per year, not reduced when an asset leaves (§ 6 Abs. 2a EStG). A financial year that is not the
  calendar year is OPEN.
- **None:** nothing.

The depreciation of a period is what accumulates in it. The **book value** at the end of a day is the cost less the
depreciation accumulated; it is zero once the asset has left – except in a pool, which keeps it until dissolved. An
asset leaves at its book value on the day it leaves.

Which method and useful life apply is for whoever books to decide; an implementation SHOULD propose them – in Germany
by cost and by the official tables of useful lives. The register does not book: an implementation books the
depreciation of a period – the expense against the asset account, part 0001 – from what the register gives, so that
the balances and every report from accounts agree with it.

An implementation MUST report, and MUST NOT silently resolve:

- **Not an owner** – an asset of someone who is not an owner of the books.
- **Duplicate** – an asset registered twice under one identity; the first in the agreed order applies.
- **Invalid** – an asset whose cost is not above zero, or on a straight line without months.
- **Unknown asset** – a disposal of an asset not registered (it may not have arrived yet).
- **Disposed twice** – a further disposal of an asset that left already; the first in the agreed order applies.
- **Disposed before acquired** – a disposal dated before the asset was acquired.

## Reports of the register

A report of the register is a report mapping (part 0009) with these members instead of `accounts` and `taxes`:

| Member | Content |
|---|---|
| `asset_groups` | account → group, for the assets depreciated on a straight line or not at all; an account holds for every account that refines it, unless a refinement has its own |
| `asset_fields` | per group, its fields: `cost` and `opening` – the cost and the book value of the assets held at the beginning of the period –, `additions` – the cost of those acquired in it –, `depreciation`, `disposals` – the book value of those that left in it –, `closing` – the book value at its end; each absent where the report has no such field |
| `pools` | the fields of the pools: of the year of the period, then of each year before it |

Low-value assets depreciated at once are not part of it. Fields without an amount are left out.

### The Anlage AVEÜR

A German business that determines its profit by receipts and payments files its register with the Anlage EÜR as the
Anlage AVEÜR (BMF letter of 1 September 2026, IV C 6 - S 2142/00024/010/001,
[BMF](https://www.bundesfinanzministerium.de/Content/DE/Downloads/BMF_Schreiben/Steuerarten/Einkommensteuer/2026-09-01-anlage-EUER-2026.html)).
Its mapping of 2026 is [`de-aveuer-2026.json`](data/reports/de-aveuer-2026.json); its fields are the Kennzahlen of
the form, named in the translation data under `de-aveuer/<Kennzahl>`:

| Group | Accounts (SKR 03) | Kennzahlen |
|---|---|---|
| `land` – Grund und Boden | 0050, 0065, 0075, 0085 | 100, 101, 102, 105, 106 |
| `buildings` – Gebäude | 0080 to 0194 but those below | 110, 111, 112, 114, 115, 116 |
| `land_rights` – grundstücksgleiche Rechte | 0060, 0070 | 120, 121, 122, 124, 125, 126 |
| `home_office_land`, `home_office_building` – häusliches Arbeitszimmer | 0059, 0149 | 200 to 206, 210 to 216 |
| `intangible` – immaterielle Wirtschaftsgüter | 0010 to 0048 | 320 to 326 |
| `vehicle` – Kfz | 0320, 0350 | 400 to 406 |
| `office` – Büroausstattung | 0410, 0420 | 410 to 416 |
| `other` – andere bewegliche Wirtschaftsgüter | 0200 to 0310, 0380 to 0499 but those above | 420 to 426 |
| `shares` – Anteile (Teileinkünfteverfahren, § 8b KStG) | 0500 to 0519, 0530, 0538 | 500 to 506 |
| `financial` – andere Finanzanlagen | the other accounts of 0505 to 0595 | 510 to 516 |
| Sammelposten of the year and of the four before | | 432 to 436, 440 to 446, …, 470 to 474 |

The form computes the totals (Kz 190, 480, 490, 499) and carries them into the Anlage EÜR; the bookings of
depreciation reach the Anlage EÜR on their expense accounts (part 0009). Special depreciation (§ 7b, § 7g EStG),
grants and reserves that reduce the cost, and the question about electric vehicles (Kz 407) are not part of this
version (OPEN). The form wants disposals in the year they reduce profit, which for an asset sold on credit may be later
(OPEN).

## Open points

- Special depreciation, declining balance, depreciation by use (§ 7 Abs. 1 Satz 6, Abs. 2, § 7b, § 7g EStG).
- Subsequent costs and reductions of the cost after the year of acquisition.
- Assets of a balance sheet under the HGB: the register serves both; a statement of changes in fixed assets
  (Anlagenspiegel, § 284 Abs. 3 HGB) is not part of this version.
- A financial year that is not the calendar year, for pools.
