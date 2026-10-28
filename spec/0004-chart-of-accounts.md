# 0004 Chart of accounts

Version 0.1, draft. Implemented in the reference library (`Chart`, `ChartTranslation`, `Balances`, `Names`).

librotenado has one **harmonised chart of accounts**. National charts – prescribed ones such as France's Plan
comptable général and common ones such as Germany's standard charts – are **translations** of it. Where a national
chart distinguishes more finely than the harmonised one, the harmonised chart grows; a translation never bends.

## Accounts

An account is an object of type `account` (part 0003):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001), stable across versions of the chart |
| `kind` | `asset`, `liability`, `equity`, `income`, `expense` or `memo` |
| `parent` | the account it refines; absent for a top-level account |
| `attributes` | further distinctions as attribute → value, e.g. a tax treatment or a non-profit sphere; absent when empty |

Assets and expenses normally have a debit balance, liabilities, equity and income a credit balance (part 0001: debit
positive). **Memo** accounts stand outside the balance sheet and the result – commitments given and received, the
opening and closing balance sheet – and have no normal side.

Names are not part of an account, as they differ by language: translation data names an account by its `id`. The
identity carries no meaning – no path, no name – so moving an account in the hierarchy or renaming it changes
nothing that refers to it. Accounts that a set of books opens itself – each bank account, each customer – refine
accounts of the chart (part 0002).

## Charts

A chart is an object of type `chart` with `name`, `version` and `accounts`. A chart MUST satisfy:

- every `id` occurs once;
- every `parent` is an account of the chart, of the same `kind`;
- no account refines itself, directly or through others.

An account inherits the attributes of the accounts it refines; its own values take precedence.

## Translations of national charts

A translation is an object of type `chart_translation`:

| Member | Content |
|---|---|
| `chart` | the national chart, e.g. `de-skr04` |
| `version` | its version |
| `into` | the harmonised chart it translates into, as `name/version` |
| `accounts` | national account number → harmonised account |

- A translation is **unambiguous** by construction: each national account maps onto exactly one harmonised account.
- Every harmonised account it names MUST exist in the chart it translates into.
- It is **reversible** for a harmonised account onto which exactly one national account maps. Where several national
  accounts map onto the same harmonised account, the harmonised chart is coarser than the national one: the model
  has to grow before that national chart is complete. Implementations report such accounts.

Translations are versioned together with the national chart and tested with published cases per country (OPEN:
which charts may be published, see the whitepaper).

## The chart of version 0.1

The harmonised chart of version 0.1 is published as [`spec/data/chart.json`](data/chart.json), with names in
[German](data/names/de.json), [English](data/names/en.json) and [French](data/names/fr.json). It is built against
its first national chart, the French Plan comptable général in its version of 1 January 2026
([Autorité des normes comptables, PCG](https://www.anc.gouv.fr/files/anc/files/1_Normes_fran%C3%A7aises/Plans%20comptables/2026/PCG--1er-janvier-2026.pdf),
articles 1121-1 and 1231-80 to 1231-89):

- **Six roots**, one per kind: assets, liabilities, equity, income, expenses, memo accounts.
- **One harmonised account for every account of the PCG** – 851, classes 1 to 8 – refining the harmonised account
  of its national parent. The translation [`fr-pcg`](data/translations/fr-pcg.json) is therefore reversible
  everywhere.
- **Groups of mixed kind are split.** Where a national group holds accounts of different kinds – suppliers (40, a
  liability) and suppliers with debit balances (409, an asset), or the VAT accounts of 445 – the accounts of the other
  kind refine a harmonised group of their own, under the root of their kind: 20 such groups.
- Contra accounts – depreciation and impairment, rebates obtained or granted – keep the kind of what they correct
  and have the opposite balance.

Each further national chart is translated against this chart, and the chart grows where it distinguishes more
finely. The English and German names are the project's translation; the German ones follow the terms of the HGB and
of German bookkeeping practice where there are such terms.

The German balance sheet (HGB § 266, part 0009) already distinguishes more finely than the PCG: shares, receivables,
payables and current securities of **affiliated companies** and of **companies linked by participating interests**,
and the reserve for shares in a controlling company. The chart 0.1 has seven accounts for them, refining the PCG
accounts 261, 411, 401, 503 and 106; they have no account of the PCG of their own. The Anlage EÜR distinguishes more
finely still – vehicle costs, business entertainment and gifts by their deductible part, home office, telecommunications
apart from postage, legal and tax advice, low-value assets and others: twenty further accounts.

## Balances

The **balance** of an account, for one owner in one currency on a date, is the sum of the `base_units` of the postings
of that owner on that account in that currency, in all bookings – recorded and reversing – dated on or before that date
(part 0001). A positive balance is a debit balance, a negative one a credit balance. Over all accounts of one owner in
one currency, the balances sum to zero.

In the hierarchy, the balance of an account **includes the accounts that refine it** – of the chart and opened in the
books (part 0002): the balance of office expenses includes that of software. Balances rolled up this way over all
accounts are the **trial balance**, from which reports are derived.

## Open points

- Further national charts, starting with Germany's standard charts – whether a translation of them may be published.
- The accounts of the PCG as a template rather than a harmonisation: grouping them by concept across countries
  (statement items, tax treatment) once a second chart shows where they meet.
- The PCG lists its suspense accounts as a range, "471 à 473"; the translation maps 471 only.
- The kind of accounts the PCG lets run either way – group and partners' current accounts (451, 455), suspense
  accounts (471) – follows their usual side.
- The conditions for reusing the text of the PCG, which the French names follow.
- Attributes: a registry of attribute names and values (tax treatment, sphere, …).
