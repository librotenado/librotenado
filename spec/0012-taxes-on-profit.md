# 0012 Taxes on profit

Version 0.19, draft. Implemented in the reference library (`EventSet::profit_tax`, `ProfitTax`, `TradeTax`,
`ProfitTaxFacts`, `EventSet::income_tax`, `IncomeTax`, `IncomeTaxFacts`).

The taxes a business pays on its profit follow from its books: the result of the financial year in the tax balance
sheet (part 0001, `basis`), corrected for what the tax law does not count, and – for trade tax – what it adds and
deducts. The corrections are attributes of the accounts (part 0004), so that a national chart that translates into
the harmonised one brings them along. What the books cannot know – the multiplier of the municipality, losses of
earlier years, the size of holdings – is given with them.

## Attributes

| Attribute | Value | The balance of the account … |
|---|---|---|
| `profit` | `excluded` | does not count in the taxable profit: expenses not deductible, income exempt, the taxes on profit themselves |
| | `participation-dividend` | is a dividend from a holding in a corporation (below) |
| | `participation-income` | is another gain from such a holding – a sale, a write-up |
| | `participation-loss` | is a loss on such a holding – a write-down, a sale below book value |
| | `participation-cost` | is a cost connected with such a holding |
| | `donation` | is a donation for tax-privileged purposes (§§ 52 to 54 AO) |
| | `partner-remuneration` | is a remuneration of a partner, which in a partnership is part of the partner's profit (§ 15 Abs. 1 Satz 1 Nr. 2 EStG) |
| `trade_tax` | `interest`, `annuity`, `silent-partner` | is a charge of § 8 Nr. 1 Buchstabe a, b or c GewStG – interest on debts, annuities, profits of silent partners – in full |
| | `rent-movable`, `rent-movable-half` | is rent or leasing of movable assets (Buchstabe d) – one fifth; for electric vehicles and bicycles one tenth |
| | `rent-immovable` | is rent or leasing of land and buildings (Buchstabe e) – one half |
| | `licence` | is a fee for rights granted for a time (Buchstabe f) – one quarter |
| | `partnership-loss` | is a share of the loss of a partnership (§ 8 Nr. 8) |
| | `partnership-share` | is a share of the profit of a partnership (§ 9 Nr. 2) |

The chart of version 0.1 carries them on the accounts of SKR 03 and SKR 04 that DATEV marks for the programs of
corporation tax and trade tax (`K`, `G`; DATEV, Kontenrahmen SKR 03 and SKR 04 valid for 2026), by the provision
their name gives.

## The taxable profit

From the bookings dated in the financial year that count in the tax balance sheet – where profit is determined by
receipts and payments, on the day they are `attributed` to (§ 11 EStG, part 0001) –, in the owner's functional
currency:

1. **The result** – income less expenses.
2. Less the balances of accounts `excluded`; for a partnership, less the remunerations of its partners.
3. **Holdings in corporations:** a corporation leaves out 95 % of dividends and other gains, the whole of losses
   (§ 8b Abs. 1 to 3 and 5 KStG), and dividends of a holding below 10 % count in full (§ 8b Abs. 4 KStG); a person
   leaves out 40 % of gains, losses and costs (§ 3 Nr. 40, § 3c Abs. 2 EStG).
4. **Donations:** a corporation deducts them up to 20 % of the income before them, or 4 per mille of turnover and
   wages where that is more (§ 9 Abs. 1 Nr. 2 KStG); a person deducts them privately (§ 10b EStG), not from the
   profit of the business.
5. **Losses carried forward:** up to 1 million euros in full, above it up to 70 % of the rest
   ([§ 10d Abs. 2 EStG](https://www.gesetze-im-internet.de/estg/__10d.html), § 8 Abs. 1 KStG).

**Corporation tax** is 15 % of the taxable income up to 2027, then one point less each year to 10 % from 2032
([§ 23 Abs. 1 KStG](https://www.gesetze-im-internet.de/kstg_1977/__23.html)); the **solidarity surcharge** 5.5 % of
it, fractions of a cent left out ([§ 4 SolZG](https://www.gesetze-im-internet.de/solzg_1995/__4.html)).

## Trade tax

For an owner with a `trade_tax_multiplier` (part 0008), from the profit before losses carried forward
([§ 7 GewStG](https://www.gesetze-im-internet.de/gewstg/__7.html)):

1. **Additions** ([§ 8](https://www.gesetze-im-internet.de/gewstg/__8.html)): a quarter of the sum of the charges of
   Nr. 1 – in full, a fifth, a tenth, a half, a quarter by their attribute – above 200,000 euros; dividends left out
   of the profit from holdings below 15 % (Nr. 5); shares of losses of partnerships (Nr. 8); donations a corporation
   deducted (Nr. 9).
2. **Reductions** ([§ 9](https://www.gesetze-im-internet.de/gewstg/__9.html)): for business land, as given (Nr. 1);
   shares of profits of partnerships (Nr. 2); donations up to 20 % of the profit with the additions of Nr. 9, or 4 per
   mille of turnover and wages (Nr. 5).
3. **Losses carried forward:** up to 1 million euros in full, above it up to 60 % of the rest
   ([§ 10a](https://www.gesetze-im-internet.de/gewstg/__10a.html)).
4. Rounded down to full 100 euros, less an allowance of 24,500 euros for persons and partnerships, of 5,000 euros for
   associations and like bodies, none for corporations; times 3.5 %, the base amount
   ([§ 11](https://www.gesetze-im-internet.de/gewstg/__11.html)); times the multiplier, the trade tax.

## Income tax

A person running a business pays income tax on its profit together with all other income. The books give the
profit – the income of the taxes on profit for a person, from the bookings of the calendar year –; what they do not
hold, the person states: other positive income (of both spouses where they are assessed jointly), deductions –
special expenses, extraordinary burdens, allowances –, losses carried forward, and the rate of church tax.

1. **Taxable income:** profit and other income, less losses carried forward – up to 1 million euros, 2 million for
   spouses, in full, above it up to 70 % of the rest (§ 10d Abs. 2 EStG) –, less the deductions; rounded down to
   full euros.
2. **The tariff** of 2026 ([§ 32a Abs. 1 EStG](https://www.gesetze-im-internet.de/estg/__32a.html)): none up to
   12,348 euros; (914.51 · y + 1,400) · y up to 17,799 euros, y a ten-thousandth of what exceeds 12,348; (173.10 · z
   + 2,397) · z + 1,034.87 up to 69,878 euros, z a ten-thousandth of what exceeds 17,799; 0.42 · x − 11,135.63 up to
   277,825 euros; 0.45 · x − 19,470.38 above; rounded down to full euros. Spouses pay twice the tax on half their
   income (Abs. 5). The tariffs of earlier years are not part of this version (OPEN).
3. **The reduction for trade tax** ([§ 35 Abs. 1 EStG](https://www.gesetze-im-internet.de/estg/__35.html)): four
   times the base amount of trade tax, no more than the trade tax, nor than the share of the tariff tax that falls on
   the profit of the business among the positive income.
4. **The income tax assessed:** the tariff tax less the reduction. The **solidarity surcharge** is 5.5 % of it, none
   up to 20,350 euros – 40,700 euros for spouses – and no more than 11.9 % of what exceeds that limit
   ([§ 3 Abs. 3, § 4 SolZG](https://www.gesetze-im-internet.de/solzg_1995/__4.html)); **church tax** is the rate of
   the person's church – 8 % in Bavaria and Baden-Württemberg, 9 % elsewhere – of the tariff tax without the reduction
   for trade tax ([§ 51a Abs. 2 Satz 3 EStG](https://www.gesetze-im-internet.de/estg/__51a.html)).
5. **Advance payments:** a quarter of the income tax assessed, due on 10 March, June, September and December, none
   below 400 euros a year or 100 euros a payment
   ([§ 37 Abs. 1 and 5 EStG](https://www.gesetze-im-internet.de/estg/__37.html)).

Children's allowances and their effect on the solidarity surcharge and church tax (§ 32 Abs. 6 EStG, § 3 Abs. 2
SolZG), income taxed at a flat rate (§ 32d EStG), income under special tariffs (§§ 32b, 34 EStG), partners'
shares of a partnership's profit and the reduction of § 35 for them, and the exemption limits of the solidarity
surcharge for years other than 2026 are not part of this version (OPEN).

## The figures

The figures are the fields of two reports, named in the translation data under `de-kst/<field>` and
`de-gewst/<field>`: `result`, `excluded`, `participation`, `partner_remuneration`, `donations`,
`donations_deducted`, `income`, `loss_deduction`, `taxable`, `corporation_tax`, `solidarity`; and `profit`,
`financing_a` to `financing_f` – the parts of § 8 Nr. 1 at their fractions –, `financing_addition`, `dividends`,
`partnership_losses`, `donations_added`, `land`, `partnership_profits`, `donations`, `trade_income`,
`loss_deduction`, `rounded`, `allowance`, `base`, `tax`; and for income tax, under `de-est/<field>`: `business`,
`other_income`, `loss_deduction`, `deductions`, `taxable`, `tariff`, `trade_tax_credit`, `assessed`, `solidarity`,
`church`, `advance_payment`. Fields without an amount are left out. They are the
computation, not the forms (OPEN, below).

## What is given

| Fact | Content |
|---|---|
| subject | a corporation, a person, a partnership, an association |
| multiplier | of the configuration (part 0008) |
| losses | carried forward for corporation or income tax and for trade tax, not yet used – as the tax office assessed them |
| land | the reduction for business land (§ 9 Nr. 1 GewStG) |
| turnover and wages | for the limit of 4 per mille of donations |
| holding | at least 15 %, at least 10 %, below 10 % – one for all holdings |
| cash basis | whether profit is determined by receipts and payments |

## Open points

- The size of each holding, rather than one for all; holdings in partnerships of corporations.
- The interest barrier (§ 4h EStG, § 8a KStG), hidden profit distributions and contributions (§ 8 Abs. 3 KStG),
  the minimum taxation of losses carried back (§ 10d Abs. 1 EStG), groups (Organschaft, §§ 14 ff. KStG).
- Whether corporation tax is rounded to full euros; the trade tax of a year that is not the calendar year
  (§ 10 Abs. 2 GewStG).
- The forms – KSt 1, GewSt 1 A – and their fields; the reduction for business land from its value.
- The review by tax advisers of the attributes of every account.
