# librotenado specification

Version 0.24 – a draft, not yet stable. Normative statements will use the key words of
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174).

| Part | Content | State |
|---|---|---|
| [0001 Base types](0001-base-types.md) | amounts, currencies, dates, accounts, bookings, tax codes | draft, implemented |
| [0002 Events and synchronisation](0002-events.md) | events, hybrid logical clocks, chains per device, merging; the identity and accounts of a set of books, transactions across books, closing and reopening periods | draft, implemented |
| [0003 Canonical encoding](0003-canonical-encoding.md) | a byte-exact encoding every implementation can hash (RFC 8785); objects and their types | draft, implemented |
| [0004 Chart of accounts](0004-chart-of-accounts.md) | the harmonised chart: hierarchy with attributes; national charts as translations | draft, implemented (form; the accounts themselves follow) |
| [0005 Documents](0005-documents.md) | invoices, receipts, statements as records of their own; invoice data; XRechnung and ZUGFeRD; booking proposals | draft, implemented |
| [0006 Encryption](0006-encryption.md) | end-to-end encryption: members and their keys, an ordered member list, a books key per member, a chain of keys, envelopes, relays, invitations, recovery codes | draft, implemented |
| [0007 Filings](0007-filings.md) | submissions to authorities: derived from a state of the books, never changed; adapters over HTTPS | draft, implemented |
| [0008 Parties](0008-parties.md) | the organisations and persons books belong to and deal with: legal form, registers, tax numbers, roles and shares, contacts and bank accounts, customers and suppliers and how documents name them; how each keeps its books and writes invoices, series of invoice numbers per device | draft, implemented |
| [0009 Reports](0009-reports.md) | report mappings per country and version: HGB balance sheet with remaining terms and profit and loss account, E-Bilanz with its Mussfelder per legal form and the reconciliation to the tax balance sheet, advance and annual VAT return, recapitulative statement | draft, implemented |
| [0010 Audit and retention](0010-audit.md) | what the protocol guarantees for audits; Germany: retention, GoBD, data transfer (Z3) | draft, implemented |
| [0011 Fixed assets](0011-assets.md) | the register of fixed assets: cost, depreciation on a straight line, at once or in pools, disposals; Germany: the Anlage AVEÜR | draft, implemented |
| [0012 Taxes on profit](0012-taxes-on-profit.md) | the taxable profit from the tax balance sheet; Germany: corporation tax, the solidarity surcharge, trade tax with its additions and reductions, income tax with the reduction for trade tax, church tax and advance payments | draft, implemented |
| [0013 Payroll](0013-payroll.md) | employments and payslips; Germany: the wage tax by the BMF's Programmablaufplan, the social insurance with its ceilings, the transition range and minor employment, the levies; the Lohnsteuer-Anmeldung, the Beitragsnachweis, the bookings | draft, implemented |

The reasoning behind the protocol is in the [whitepaper](https://github.com/librotenado/whitepaper).
