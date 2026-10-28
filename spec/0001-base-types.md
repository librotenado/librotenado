# 0001 Base types

Version 0.3, draft. Implemented in the reference library (`crates/librotenado`).

## Money

Money (type `money`, part 0003) is an amount in a currency, counted in the currency's smallest unit, its
**base unit**:
`{"currency": "EUR", "base_units": "-1240"}` is −12.40 euros.

- `base_units` is an integer written as a string in decimal: an optional `-`, then `0` or a digit `1`–`9` followed
  by digits (`"0"`, `"1240"`, `"-5"`). Every amount has this one form; a reader MUST reject any other – leading
  zeros, `+`, `"-0"`, a decimal point, a JSON number – so the same amount always has the same bytes and the same
  hash (part 0003).
- It is a string because amounts in some currencies exceed what JSON numbers carry exactly (2^53 − 1, part 0003):
  one ether is 10^18 wei. An implementation MUST accept magnitudes up to 10^38 − 1 and MUST reject an amount it
  cannot represent rather than round it.
- Money MUST NOT be represented as a binary floating-point number. Sums are exact. Rounding to the base unit happens
  before an amount enters the books; how and when to round is a rule of the law that applies.
- Reading, checking and summing amounts does not need to know how the currency divides.

## Currencies

A currency is identified by its code:

- three upper-case letters: a currency of [ISO 4217](https://www.iso.org/iso-4217-currency-codes.html) by its
  alphabetic code (`"EUR"`, `"CHF"`, `"JPY"`);
- `x-` followed by 1 to 62 characters of `a-z`, `0-9` and `-`: a currency defined outside ISO 4217 – crypto assets,
  vouchers, bonus points, fictional currencies (`"x-btc"`, `"x-galleon"`).

The two forms cannot collide. A code in common use that ISO 4217 has not assigned is written in the second form:
Bitcoin is `x-btc`, not `BTC` – ISO 4217 could assign `BTC` one day, as the first two letters of its codes are
usually the country (BT is Bhutan, whose currency is BTN). Whether a code denotes an existing currency is a question
of the reference data.

### Definitions

How a currency divides into its base unit is **reference data**, published in versions with the protocol and not
part of any amount. A definition (type `currency`, part 0003) is `{"code": …, "subdivisions": […]}`: from the unit
down, how many of the next smaller stage one stage contains.

| Currency | `subdivisions` | Stages | Base units per unit |
|---|---|---|---|
| `EUR` | `[100]` | euro, cent | 100 |
| `JPY` | `[]` | yen | 1 |
| `x-btc` | `[100000000]` | bitcoin, satoshi | 10^8 |
| `x-eth` | `[1000000000, 1000000000]` | ether, gwei, wei | 10^18 |
| `x-galleon` | `[17, 29]` | Galleon, Sickle, Knut | 493 |

- Every entry is an integer from 2 to 2^53 − 1, so it is exact as a JSON number (part 0003); a larger division
  needs an intermediate stage, as for ether. Base units per unit – the product of the entries – MUST NOT exceed
  10^38, the range of amounts.
- An ISO 4217 currency is divided as ISO 4217 gives it ("minor unit"): once by 10^minor unit, or not at all if
  that is 0.
- Money converts to and from decimal numbers (below) when a unit is a power of ten base units – EUR, JPY, ether –,
  not for Galleons.
- Later versions may add other members to a definition. Software that does not understand a definition ignores it:
  it still reads, checks and sums amounts in the currency, but MUST NOT convert or format them by guessing; it
  shows the base units.

### Names

Names are not part of a definition, as they differ by language. Stage `n` is the unit after `n` subdivisions:
stage 0 is the unit itself, the last stage the base unit. Translation data names stage `n` by the key `<code>/<n>`:
`EUR/0` euro, `EUR/1` cent; `x-galleon/0` Galleon, `x-galleon/1` Sickle, `x-galleon/2` Knut. The `/` cannot occur
in a code, so keys are unambiguous.

Translation data is a root object of type `names`: `language`, a BCP 47 tag (`de`, `en`, `fr`), and `names`, key →
name. The keys are unambiguous by their form:

| Key | Names |
|---|---|
| `<code>/<stage>` | a stage of a currency, as above: `EUR/1` cent |
| an account's identity | an account (part 0004) |
| a tax code | a tax code (below): `de-out-19` Umsatzsteuer 19 % |
| `<report>/<position>` | a position of a report (part 0009): `de-hgb-266/aktiv.B.II.1` Forderungen aus Lieferungen und Leistungen |

Version 0.1 publishes names in German, English and French ([`spec/data/names/`](data/names/de.json)).

### The table of version 0.1

The definitions of version 0.1 are published as [`spec/data/currencies.json`](data/currencies.json), a root object of
type `currency_table` with `version`, `iso_4217` – the date of the ISO 4217 list it follows – and `currencies`, each
code once. It holds every currency of ISO 4217 list one with a minor unit, as published by its maintenance agency on
2026-09-17 ([SIX, ISO 4217 list one](https://www.six-group.com/dam/download/financial-information/data-center/iso-currrency/lists/list-one.xml)),
and `x-btc` and `x-eth`. ISO 4217 gives the Malagasy ariary (MGA) and the Mauritanian ouguiya (MRU) two decimal
places, and so does the table – although each divides in theory into five subunits
([Wikipedia, ISO 4217](https://en.wikipedia.org/wiki/ISO_4217)).

OPEN:

- The divisions of bitcoin and ether are common usage, not yet sourced; further currencies outside ISO 4217.
- Units without a minor unit in ISO 4217 – precious metals such as gold (XAU), units of account such as the special
  drawing right (XDR), the codes for testing and for no currency (XTS, XXX) – are not in the table; and a validity
  period per definition (DEM, replaced by the euro).
- The form of translation data, and whether the names of ISO 4217 currencies and their subunits can be taken from
  [Unicode CLDR](https://cldr.unicode.org), including plural forms.

## Decimal numbers

Prices per unit, quantities, exchange and tax rates are not money and may have more places than a currency
(fuel at 1.799 euros per litre). They are decimal numbers m · 10^−s with an integer m and 0 ≤ s ≤ 18, written as
a string in their one form: the smallest s, an optional `-`, the integer part without leading zeros and, only if
s > 0, `.` and the places, the last of which is not `0` (`"1.799"`, `"0.19"`, `"-2.5"`, `"100"`). A reader MUST
reject any other form (`"1.50"`, `"01.5"`, `".5"`, `"-0"`). Version 0.1 defines the type; later parts use it.

## Dates

A date is a day of the Gregorian calendar written `YYYY-MM-DD`, years 0001 to 9999, without time or time zone.

## Accounts

An account is identified by a UUID ([RFC 9562](https://www.rfc-editor.org/rfc/rfc9562)) in its one form: lower-case
hexadecimal digits in groups of 8-4-4-4-12 (`"0192f1e4-7a3c-7b1e-9c2d-5e8f0a1b2c3d"`). A reader MUST reject
upper-case digits. The identity carries no meaning, so it never has to change: what an account is follows from the
chart of accounts (part 0004), its name from translation data. Any version of UUID is accepted; version 7 is
RECOMMENDED for new accounts.

## Bookings

A booking (type `booking`) is an object with `date`, `text`, `postings` and, if it is based on documents,
`documents` – an array of document identities (part 0005), absent when empty, never an empty array (part 0003) – and, if the same business transaction is
recorded in other sets of books as well, `transaction`: its identity and those other sets (part 0002).
A posting (type `posting`) is `{"owner": …, "account": …, "amount": Money}` and, if there is something to
remark, `note` – for example why there is no document for it; absent if none, never empty – and, if it is treated for
VAT, `tax`, a tax code (below), and, if its currency is not the functional currency of its owner, `exchange`
(below). `owner` is the
organisation or person whose books the posting is in (part 0008); every posting has one. A positive amount is a
debit, a negative amount a credit.

A booking MUST have at least two postings, MUST NOT contain a posting of zero, and MUST balance **for every owner in
every currency**: the `base_units` of the postings of one owner in one currency sum to zero. Each owner's books thus
stay balanced on their own, while one booking can concern several owners – an internet bill split between a business
and its owner's private books, both resting on the same document. A booking that converts between currencies
therefore books each currency against an exchange account.

### Foreign currency

Every owner keeps its statements in one **functional currency** – the euro for German books (§ 244 HGB). A posting in
another currency carries `exchange` (type `exchange`): `rate`, the units of the functional currency per unit of the
posting's currency, as a decimal number, and `amount`, the amount in the functional currency – the posting's amount
times the rate, rounded half away from zero to base units, or as the bank converted it. German books convert at the
rate of the day of the transaction (§ 256a HGB) and record rate and converted amount with every posting in foreign
currency (GoBD Rz. 94, part 0010).

- The postings of an owner are converted into **one** currency, never into their own; once one posting of an owner in
  a booking has an `exchange`, every posting of that owner is in that currency or converted into it.
- Converted, the postings of each owner balance as well: the converted amounts and the amounts already in the
  functional currency sum to zero. A rounding difference is evened out on one posting's converted amount.
- Balances stay per currency (part 0004); reports in one currency (part 0009) take the converted amounts.

A booking is never changed. It is cancelled by a reversal: a booking with the same accounts, the same tax codes and
the opposite amounts (part 0002).

## Tax codes

How a posting is treated for value added tax depends on the transaction, not on the account alone: the same account
of services sold may carry sales at the standard rate, at a reduced rate, tax-exempt or abroad. Every posting
therefore MAY carry a **tax code**, and each posting of a document – each line of an invoice – its own.
Implementations propose a code by the kind of account or document; whoever books decides.

A tax code is the country in lower case, `-`, and 1 to 61 characters of `a-z`, `0-9` and `-` (`de-out-19`). What a
code means is reference data of its country, a root object of type `tax_table` with `country`, `valid_from` and
`codes`; each code (type `tax_code`) has `code`, `direction` – `output` for sales, `input` for purchases –, `rate` in
percent as a decimal number if it taxes at one, and `law`, the provisions it rests on.

The German table is [`spec/data/tax/de.json`](data/tax/de.json): the rates of § 12 UStG, exempt supplies with and
without deduction (§ 4 Nr. 1 to 7, § 4 Nr. 8 to 29), small businesses (§ 19), intra-Community supplies and
acquisitions (§ 4 Nr. 1 Buchst. b, § 1a), reverse charge (§ 13b), import VAT and non-deductible input tax
([UStG](https://www.gesetze-im-internet.de/ustg_1980/)). How the codes fill a VAT return is a report mapping (part
0009).
