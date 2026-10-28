# 0004 Chart of accounts

Version 0.1, draft. Implemented in the reference library (`Chart`, `ChartTranslation`).

librotenado has one **harmonised chart of accounts**. National charts – prescribed ones such as France's Plan
comptable général and common ones such as Germany's standard charts – are **translations** of it. Where a national
chart distinguishes more finely than the harmonised one, the harmonised chart grows; a translation never bends.

## Accounts

An account is an object of type `account` (part 0003):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001), stable across versions of the chart |
| `kind` | `asset`, `liability`, `equity`, `income` or `expense` |
| `parent` | the account it refines; absent for a top-level account |
| `attributes` | further distinctions as attribute → value, e.g. a tax treatment or a non-profit sphere; absent when empty |

Assets and expenses normally have a debit balance, the other kinds a credit balance (part 0001: debit positive).

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

## Open points

- The content of the harmonised chart 0.1 – the accounts themselves – is the next step; this part defines its form.
- Attributes: a registry of attribute names and values (tax treatment, sphere, …).
