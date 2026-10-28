# 0008 Parties

Version 0.1, draft. Implemented in the reference library (`Organisation`, `Person`, event kinds
`organisation_described`, `person_described`, `owner_added`, `owner_removed`).

**A set of books always belongs to at least one organisation or natural person** – its owners – and every posting
names the owner whose books it is in (part 0001). Who they are decides which law applies, which national chart and
filings follow, and who files (part 0007).

Organisations and persons are described by events (part 0002). A later description with the same identity replaces
an earlier one in the agreed order of events; the earlier one stays in the books, as everything does.

## Organisations

An organisation – a company, an association, a foundation, a partnership – is an object of type `organisation`:

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001) |
| `name` | its name as registered |
| `legal_form` | its legal form as a code, e.g. `de-gmbh`, `de-ev`, `fr-sas` |
| `country` | the country whose law it is formed under, ISO 3166-1 alpha-2 (`DE`) |
| `registrations` | its entries in public registers (below); absent when none, never empty |
| `tax_ids` | its tax numbers (below); absent when none, never empty |
| `shares` | how its capital is divided into shares (below); absent when it has none |

A registration (type `registration`) is `register` – the register as a code, e.g. `de-hrb` –, `number`, and,
where a register is kept by several authorities, `authority` (e.g. the local court).

Shares (type `shares`) are `count` – a whole number, at least 1 – and `nominal`, the nominal amount of each, money
above zero (part 0001). An organisation lists one entry per nominal amount; its share capital is the sum of count ×
nominal. A company with a share capital of 1,024 euros in 512 shares of 2 euros has
`"shares": [{"count": 512, "nominal": {"currency": "EUR", "base_units": "200"}}]`. Shares are always counted in
whole numbers; what fraction of the capital someone holds follows from the counts – 257 of 512 shares are 50.2 %.

A tax number (type `tax_id`) is `scheme` and `number`. The scheme `vat` is an EU VAT identification number, written
with its country prefix; national schemes have codes such as `de-steuernummer`.

## Persons

A natural person is an object of type `person`:

| Member | Content |
|---|---|
| `id` | their identity, a UUID |
| `name` | their name as they use it |
| `tax_ids` | their tax numbers, e.g. a personal tax identification number; absent when none |
| `roles` | their roles in organisations (below); absent when none |

A role (type `role`) is `organisation` – the identity of an organisation –, `kind`, and, where the role holds
shares, `shares`: how many of one nominal amount the organisation has (`{"count": 257, "nominal": …}`). A person
holding shares of several nominal amounts has one role for each.

| `kind` | The person … |
|---|---|
| `owner` | owns the organisation alone, without shares – a sole trader's business |
| `shareholder` | holds shares of a company |
| `partner` | is a partner of a partnership |
| `managing_director` | manages and represents it |
| `board_member` | is a member of its board |
| `member` | is a member of an association or a cooperative |
| `authorised_signatory` | may sign for it, e.g. under a commercial power of attorney |

Later versions add kinds; an implementation keeps a role of a kind it does not know but does not interpret it.

## Owners

`owner_added` with `owner` – the identity of an organisation or a person – makes them an owner of the books;
`owner_removed` ends it. An implementation MUST report, and MUST NOT silently resolve:

- **No owner** – the books belong to no organisation and no person.
- **Unknown owner** – an owner that is not described (the description may not have arrived yet).
- **Unknown organisation** – a person's role in an organisation that is not described.
- **Shares** – shares held of a nominal amount the organisation does not have, or more of them than it has.
- **Not an owner** – a posting (part 0001) in the books of an organisation or person that is not an owner.

## Personal data

Persons are personal data. The protocol carries what accounting and filings need and no more; who may read the books
is a matter of encryption (part 0006).

## Open points

- The registries of codes: legal forms, registers, tax schemes – per country, with sources.
- Further data an organisation's books depend on: the financial year, the period of VAT returns, the applicable VAT
  regime (e.g. small businesses), the accounting standard – and whether they belong to the organisation or to the
  books.
- Addresses, and the parties of documents (sellers, buyers, part 0005).
- Relations between organisations: groups, holdings, branches.
- Capital that is not divided into shares: the contributions of partners in a partnership, the assets of a
  foundation.
