# Changelog

Specification and library share one version. Until 1.0, any version may change what an earlier one specified.

## Unreleased

- [0001 Base types](spec/0001-base-types.md): the currency table of version 0.1, `spec/data/currencies.json` – every
  ISO 4217 currency with a minor unit (list of 2026-09-17), and `x-btc`, `x-eth`; type `currency_table`.
- [0004 Chart of accounts](spec/0004-chart-of-accounts.md): balances per owner, account and currency on a date, and
  the trial balance rolled up the hierarchy.
- [0004 Chart of accounts](spec/0004-chart-of-accounts.md): the harmonised chart of version 0.1,
  `spec/data/chart.json` – six roots by kind, one account for every account of the French PCG (version of
  1 January 2026), groups of mixed kind split – with the translation `fr-pcg` and names in English and French; the
  kind `memo` for commitments and the opening and closing balance sheet.
- [0001 Base types](spec/0001-base-types.md): translation data as root objects of type `names` – in German, English and
  French: accounts, euro and cent, tax codes, the positions of the HGB in the wording of the law, and the fields of the
  VAT return and the Anlage EÜR in the wording of the official forms; tax codes on postings, per country in a
  `tax_table` – the German one in `spec/data/tax/de.json`.
- [0004 Chart of accounts](spec/0004-chart-of-accounts.md): seven accounts the HGB distinguishes and the PCG does not
  (affiliated companies, companies linked by participating interests, reserve for shares in a controlling company).
- [0009 Reports](spec/0009-reports.md): report mappings per country and version, from accounts and from tax codes; the
  German balance sheet (HGB § 266), profit and loss account (§ 275 Abs. 2) and VAT return of 2026 and the Anlage EÜR
  of 2025, as drafts for review by tax advisers; reports over a period combine tax codes, accounts and the settlement
  of VAT with the tax office. The chart grows by twenty accounts the Anlage EÜR distinguishes.
- [0005 Documents](spec/0005-documents.md): the data of an invoice or credit note (`invoice_data`: parties, dates,
  VAT breakdown, lines, totals, after EN 16931); translation of electronic invoices in UBL and CII – XRechnung,
  ZUGFeRD and Factur-X PDFs –, tested with the XRechnung test suite of KoSIT; booking proposals with German tax
  codes; writing XRechnung 3.0 in UBL and CII and attaching it to a PDF/A-3 as ZUGFeRD, after checking the terms
  XRechnung requires. The invoice data is the semantic model of EN 16931 in full – notes, references, preceding
  invoices, payee and tax representative, delivery and invoicing period, payment instructions with transfers, card
  and SEPA direct debit, allowances and charges on the invoice and its lines, attached documents kept by hash, item
  identifiers, classifications and attributes, VAT in the accounting currency, prepaid amounts and rounding. Every
  standard and CIUS test invoice of the XRechnung test suite is read, written in UBL and CII and read back unchanged;
  the continuous integration has the KoSIT validator 1.6.3 with the XRechnung configuration 3.0.2 accept every
  written file.
- [0010 Audit and retention](spec/0010-audit.md): what the protocol guarantees for audits; for Germany the retention
  periods of § 147 AO, the requirements of the GoBD (as amended 2024 and 2025) against the protocol for the
  procedural documentation, and the data transfer (Z3) after the Beschreibungsstandard.
- Library: `CurrencyTable::v0_1()`, `EventSet::balances`, `EventSet::trial_balance`, `Balances`, `Chart::v0_1()`,
  `ChartTranslation::fr_pcg()`, `Names`, `AccountKind::Memo`, `TaxCode`, `TaxTable`, `ReportMapping`,
  `EventSet::period_report`, `EventSet::account_report`, `InvoiceData`, `en16931::read`, `en16931::read_pdf`,
  `en16931::write_ubl`, `en16931::write_cii`, `en16931::attach_to_pdf`, `propose_booking`, `gobd::export`;
  dependencies `roxmltree`, and `lopdf` for the feature `pdf`.

## 0.1.0 – 2026-10-28

The first draft of the protocol.

### Specification

- [0001 Base types](spec/0001-base-types.md): money as an exact count of a currency's base unit
  (`{"currency": "EUR", "base_units": "1240"}`); currency codes from ISO 4217 or beginning with `x-`; definitions of
  how a currency divides – `subdivisions`, e.g. `[100]` for EUR – as reference data, names by language as keys
  `<code>/<stage>`; decimal numbers for prices and rates; amounts and numbers in one written form only; dates,
  account identifiers, postings that name their owner, bookings that balance for every owner in every currency.
- [0002 Events and synchronisation](spec/0002-events.md): immutable events as root objects with a `kind`, hybrid
  logical clocks, a hash chain per device, merging by union, forks, gaps and broken links reported rather than
  resolved; the chart a set of books uses (`chart_selected`) and accounts of its own (`account_opened`), with
  postings on unknown accounts for review.
- [0003 Canonical encoding](spec/0003-canonical-encoding.md): byte-exact JSON after RFC 8785 as the basis of event
  hashes, taken over the event as received so that members of later versions survive; objects and their types
  (`type` on root objects, `kind` for varieties); one form of "none"; no duplicate members; strings not normalised
  (NFC recommended).
- [0004 Chart of accounts](spec/0004-chart-of-accounts.md): the form of the harmonised chart – accounts identified by
  UUIDs, without names, in a hierarchy with attributes – and translations of national charts. The accounts
  themselves are not yet specified.
- [0005 Documents](spec/0005-documents.md): invoices, credit notes, receipts and bank statements as records of their
  own – identity, kind, number, date, day received – with the file they came as, the original, as an artefact
  identified by its hash; no document without its original, a note on the posting instead. Every hash is written
  `sha256:…`.
- [0006 Encryption](spec/0006-encryption.md): proposal, inspired by any-sync – an ordered list of members and keys
  beside the unordered events, a books key wrapped per member, a chain of keys that keeps the history readable.
- [0007 Filings](spec/0007-filings.md): proposal – filings derived from a recorded state of the books, never changed
  but corrected, answers of authorities as documents, adapters over HTTPS, credentials outside the protocol.
- [0008 Parties](spec/0008-parties.md): every set of books belongs to at least one organisation or person –
  organisations with legal form, registers, tax numbers and their capital in whole shares of a nominal amount, persons
  with tax numbers, roles and the shares they hold.

### Library

- `librotenado` 0.1.0 implements 0001–0005 and 0008 (`Money`, `Currency`, `CurrencyDefinition`, `Decimal`,
  `Booking`, `Event`, `EventSet`, `Chart`, `Document`, `Organisation`, `Person`, …), with the example `quickstart`.

### Documentation

- The documentation site (`docs/`, built with mdBook) for librotenado.dev: introduction, specification, a guide to the
  library and the API documentation.
