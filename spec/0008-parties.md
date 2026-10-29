# 0008 Parties

Version 0.6, draft. Implemented in the reference library (`Organisation`, `Person`, `Contact`, event kinds
`organisation_described`, `person_described`, `owner_added`, `owner_removed`, `owner_configured`,
`invoicing_configured`, `series_assigned`; `Configuration`, `Invoicing`, `Series`).

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
| `legal_form` | its legal form as a code, e.g. `de-gmbh`, `de-ev`, `fr-sas`; empty when not known – a customer or supplier |
| `country` | the country whose law it is formed under, ISO 3166-1 alpha-2 (`DE`) |
| `registrations` | its entries in public registers (below); absent when none, never empty |
| `tax_ids` | its tax numbers (below); absent when none, never empty |
| `shares` | how its capital is divided into shares (below); absent when it has none |
| `contact` | how to reach it (below); absent when not given |

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
| `contact` | how to reach them (below); absent when not given |

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

## Contact

Organisations and persons – owners, and the customers and suppliers of the books – carry how to reach them, as a
contact (type `contact`); every member is absent when not given:

| Member | Content |
|---|---|
| `address` | the postal address (type `postal_address`, part 0005) |
| `name` | whom to contact |
| `phone` | the telephone number |
| `email` | the e-mail address |
| `endpoint` | where electronic invoices go: `scheme` and `address`, as EN 16931 writes them (BT-34, BT-49; part 0005) |
| `buyer_reference` | what the party asks to be named on invoices to it (BT-10) – the Leitweg-ID of a German authority |

An invoice an owner writes takes the seller's details from the owner and the buyer's from the customer.

## Owners

`owner_added` with `owner` – the identity of an organisation or a person – makes them an owner of the books;
`owner_removed` ends it. An implementation MUST report, and MUST NOT silently resolve:

- **No owner** – the books belong to no organisation and no person.
- **Unknown owner** – an owner that is not described (the description may not have arrived yet).
- **Unknown organisation** – a person's role in an organisation that is not described.
- **Shares** – shares held of a nominal amount the organisation does not have, or more of them than it has.
- **Not an owner** – a posting (part 0001) in the books of an organisation or person that is not an owner.

## Configuration

How an owner keeps its books depends on who it is and changes over time – a small business grows out of the VAT
exemption, a sole trader moves from cash basis to double entry. `owner_configured` records it from a date on, as a
configuration (type `configuration`):

| Member | Content |
|---|---|
| `owner` | the organisation or person (an owner of the books) |
| `from` | the first day it applies |
| `currency` | the functional currency the owner's statements are in (part 0001) |
| `year_start` | the month, 1 to 12, on whose first day the owner's financial year begins |
| `chart` | the national chart of accounts the owner works with, as a code (`de-skr03`); its translation valid on a date (part 0004) applies; absent when none |
| `accounting` | how profit is determined, as a code (below); absent when the owner determines none – a private household |
| `vat` | how the owner is treated for VAT (type `vat_scheme`, below); absent when it is not liable – not an entrepreneur |
| `region` | the subdivision of the country whose public holidays move the owner's deadlines – where its tax authority sits –, as an ISO 3166-2 code (`DE-BY`); absent when not known |

A VAT scheme is `scheme`, a code (below), `basis` – `accrual` when VAT is due on what was agreed, `cash` when on what
was received – and `period`, the period of advance returns: `month`, `quarter`, or `year` when there are none, only
the annual return. `extended` is `true` when the tax authority has extended the deadlines of advance returns for
good; absent when it has not.

A configuration applies from `from` until the next configuration of the same owner; of two with the same `from`, the
later in the agreed order of events (part 0002) applies. Codes are the country in lower case, `-`, and 1 to 61
characters of `a-z`, `0-9` and `-`, like tax codes (part 0001); an implementation keeps a code it does not know but
does not interpret it. A financial year that begins on another day than the first of a month is OPEN.

### Germany

| Member | Code or value | Meaning |
|---|---|---|
| `currency` | `EUR` | the euro ([§ 244 HGB](https://www.gesetze-im-internet.de/hgb/__244.html)) |
| `year_start` | `1` to `12` | the calendar year, or a financial year of its own ([§ 4a EStG](https://www.gesetze-im-internet.de/estg/__4a.html)) |
| `chart` | `de-skr03`, `de-skr04` | DATEV's standard charts SKR 03 and SKR 04 (part 0004) – common, not prescribed |
| `accounting` | `de-cash-basis` | income over expenses ([§ 4 Abs. 3 EStG](https://www.gesetze-im-internet.de/estg/__4.html)), reported in the Anlage EÜR |
| `accounting` | `de-double-entry` | double-entry bookkeeping ([§§ 238 ff. HGB](https://www.gesetze-im-internet.de/hgb/__238.html), [§ 141 AO](https://www.gesetze-im-internet.de/ao_1977/__141.html)), profit by comparing net assets (§ 4 Abs. 1, § 5 EStG) |
| `vat.scheme` | `de-regular` | VAT charged and deducted under the general rules of the [UStG](https://www.gesetze-im-internet.de/ustg_1980/) |
| `vat.scheme` | `de-small-business` | no VAT charged, no input VAT deducted ([§ 19 UStG](https://www.gesetze-im-internet.de/ustg_1980/__19.html)) |
| `vat.basis` | `accrual` | on agreed consideration ([§ 16 Abs. 1 UStG](https://www.gesetze-im-internet.de/ustg_1980/__16.html)) |
| `vat.basis` | `cash` | on consideration received ([§ 20 UStG](https://www.gesetze-im-internet.de/ustg_1980/__20.html)) |
| `vat.period` | `month`, `quarter`, `year` | advance returns per month or quarter, or none after an exemption by the tax office ([§ 18 Abs. 2 UStG](https://www.gesetze-im-internet.de/ustg_1980/__18.html)) |
| `vat.extended` | `true` | the permanent extension of deadlines by one month (Dauerfristverlängerung, [§ 46 UStDV](https://www.gesetze-im-internet.de/ustdv_1980/__46.html)) |
| `region` | `DE-BW` … `DE-TH` | the state whose public holidays move deadlines ([§ 108 Abs. 3 AO](https://www.gesetze-im-internet.de/ao_1977/__108.html)): for returns, the holidays where the tax office sits, after a regulation of the Brandenburg tax administration of 2004 ([BRAVORS](https://bravors.brandenburg.de/verwaltungsvorschriften/feiertagsrecht)) – whether the current AEAO says the same is **OPEN** |

An implementation MUST report, and MUST NOT silently resolve:

- **Not configured** – a posting in the books of an owner, dated before the owner's first configuration.
- **Configuration of a non-owner** – a configuration, invoicing settings or a series for an organisation or person
  that is not an owner of the books.

## Invoicing

`invoicing_configured` records how an owner writes invoices (type `invoicing`); a later one replaces an earlier one in
the agreed order:

| Member | Content |
|---|---|
| `owner` | the owner |
| `prefix` | what invoice numbers start with, before the year: `RE-` |
| `payment_days` | the days until an invoice is due; absent when not given |
| `account` | the account invoices are paid to, an IBAN; absent when not given |

Invoice numbers must be unique, in a sequence "based on one or more series"
([Directive 2006/112/EC, Article 226](https://eur-lex.europa.eu/eli/dir/2006/112/oj)). So that devices working
offline never write the same number, each device writes its own **series** for an owner: `series_assigned` (type
`series`) is `owner`, `device` – the device name (part 0002) – and `series`, empty or one to four of `A-Z` and `0-9`.
The latest assignment of a device applies. An implementation SHOULD write numbers as the prefix, the year, `-`, the
series and a sequence of the year – `RE-2026-001` in the empty series, `RE-2026-B001` in the series `B` – and MUST
report, and MUST NOT silently resolve:

- **Series taken** – one series of an owner assigned to several devices, perhaps by two devices offline at once.

## Personal data

Persons are personal data. The protocol carries what accounting and filings need and no more; who may read the books
is a matter of encryption (part 0006).

## Open points

- The registries of codes: legal forms, registers, tax schemes – per country, with sources.
- Further data the books depend on beyond the configuration: accounting standards beyond Germany (IFRS, national
  GAAP), a financial year that does not begin on the first of a month, codes of further countries.
- The parties of documents (sellers, buyers, part 0005) as parties of the books.
- Relations between organisations: groups, holdings, branches.
- Capital that is not divided into shares: the contributions of partners in a partnership, the assets of a
  foundation.
