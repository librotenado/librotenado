# librotenado specification

Version 0.6 – a draft, not yet stable. Normative statements will use the key words of
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174).

| Part | Content | State |
|---|---|---|
| [0001 Base types](0001-base-types.md) | amounts, currencies, dates, accounts, bookings, tax codes | draft, implemented |
| [0002 Events and synchronisation](0002-events.md) | events, hybrid logical clocks, chains per device, merging; the identity and accounts of a set of books, transactions across books, closing periods | draft, implemented |
| [0003 Canonical encoding](0003-canonical-encoding.md) | a byte-exact encoding every implementation can hash (RFC 8785); objects and their types | draft, implemented |
| [0004 Chart of accounts](0004-chart-of-accounts.md) | the harmonised chart: hierarchy with attributes; national charts as translations | draft, implemented (form; the accounts themselves follow) |
| [0005 Documents](0005-documents.md) | invoices, receipts, statements as records of their own; invoice data; XRechnung and ZUGFeRD; booking proposals | draft, implemented |
| [0006 Encryption](0006-encryption.md) | end-to-end encryption: members and their keys, an ordered member list, a books key per member, a chain of keys, envelopes, relays, invitations, recovery codes | draft, implemented |
| [0007 Filings](0007-filings.md) | submissions to authorities: derived from a state of the books, never changed; adapters over HTTPS | draft; filings implemented, adapter interface a proposal |
| [0008 Parties](0008-parties.md) | the organisations and persons books belong to and deal with: legal form, registers, tax numbers, roles and shares, contacts; how each keeps its books and writes invoices, series of invoice numbers per device | draft, implemented |
| [0009 Reports](0009-reports.md) | report mappings per country and version: HGB balance sheet and profit and loss account, VAT return | draft, implemented |
| [0010 Audit and retention](0010-audit.md) | what the protocol guarantees for audits; Germany: retention, GoBD, data transfer (Z3) | draft, implemented |

The reasoning behind the protocol is in the [whitepaper](https://github.com/librotenado/whitepaper).
