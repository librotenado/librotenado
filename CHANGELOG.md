# Changelog

Specification and library share one version. Until 1.0, any version may change what an earlier one specified.

## 0.7.2 – 2026-10-29

No change to the protocol or the library's interface.

- Dependencies at least 14 days old, as falkOSS projects keep them: `hpke` 0.14, `lopdf` 0.45 and `roxmltree` 0.21.1
  are the lowest versions `Cargo.toml` asks for; `scripts/check-age.py` checks `Cargo.lock`.
- CI on Linux and macOS, `cargo deny` for advisories, licences and sources; the licence check and the sign-off from
  falkoss/starterpack.

## 0.7.1 – 2026-10-29

- [0007 Filings](spec/0007-filings.md): a report content without any amount leaves `fields` out instead of being
  refused when read – a VAT return with nothing to report is filed all the same. Before, such a filing could be
  written but not read back.

## 0.7.0 – 2026-10-29

The interface between an implementation and the adapters that submit filings to authorities, no longer a proposal.

- [0007 Filings](spec/0007-filings.md): what implementation and adapter send each other as types – `submission`
  (the filing, its filer as `organisation` or `person`, `test`), `submission_status`, `receipt` (bytes in base64url
  without padding, as in part 0006) and `adapter_error` –; how a status becomes an event and a receipt a document;
  an adapter refuses a submission whose filer is not the filing's. The implementation authenticates itself to the
  adapter with a bearer token (RFC 6750).
- Library: `Submission`, `Filer`, `SubmissionError`, `SubmissionStatus`, `SubmissionState`, `Receipt`,
  `AdapterError`; `Submission::new`, `Submission::check`, `Receipt::original`, `SubmissionStatus::payload`.

## 0.6.0 – 2026-10-29

What books need to be shared between devices and people: the parties they deal with, how invoices are written, and
invoice numbers no two devices write twice.

- [0008 Parties](spec/0008-parties.md): organisations and persons carry how to reach them (`contact`: address, whom
  to contact, telephone, e-mail, electronic address, buyer reference); customers and suppliers are parties of the
  books like owners, with an empty legal form where it is not known.
- [0008 Parties](spec/0008-parties.md): `invoicing_configured` – an owner's prefix of invoice numbers, days until
  due and account – and `series_assigned` – each device's series of invoice numbers for an owner, so that devices
  offline never write the same number; a series assigned to several devices is for review.
- Library: `Contact`, `Invoicing`, `Series`, `Payload::InvoicingConfigured`, `Payload::SeriesAssigned`,
  `EventSet::invoicing`, `EventSet::series`, `PartyProblem::SeriesTaken`; `Organisation::contact`, `Person::contact`.

## 0.5.0 – 2026-10-29

End-to-end encryption, so that books travel between devices through relays that cannot read them.

- [0006 Encryption](spec/0006-encryption.md) specified and implemented: Ed25519, HPKE (RFC 9180) with X25519 and
  ChaCha20Poly1305, XChaCha20-Poly1305, HKDF-SHA256 and HMAC-SHA256; members and their keys from a seed; the chain of
  books keys; the ordered member list and the six rules every member checks; envelopes for events and originals;
  signed requests to relays and what a relay offers; invitations to add a device; recovery codes in Crockford's
  base32.
- Library: `MemberKeys`, `MemberId`, `ReceivingKey`, `Member`, `MemberRole`, `Rights`, `BooksKey`, `Grant`,
  `KeyChange`, `MemberRecord`, `MemberList`, `RecordProblem`, `Envelope`, `EnvelopeContent`, `Bytes`,
  `request_signature`, `verify_request`, `verify`, `Invitation`, `JoinRequest`, `RecoveryCode`, `EncryptionError`.

## 0.4.1 – 2026-10-29

- [0005 Documents](spec/0005-documents.md): a sale under reverse charge to a buyer abroad is a service to another EU
  country (`de-out-eu-service`, § 3a Abs. 2 UStG), no longer a domestic one (`de-out-13b`); `de_tax_code` takes the
  country of the other party of the invoice – the seller of a purchase, the buyer of a sale.
- [0005 Documents](spec/0005-documents.md): `en16931::attach_to_pdf` adds the Factur-X extension schema to the list
  of extension schemas a PDF/A-3 already has, instead of a second list – XMP allows a property only once, and PDF/A
  validators rejected the metadata –, and replaces the document's metadata in place.

## 0.4.0 – 2026-10-29

What the first release of spezado needs: English names of the German report positions, opening balances, and the
facts that move deadlines – the region whose public holidays apply and the permanent extension of VAT returns.

- [0009 Reports](spec/0009-reports.md): English names of the positions of the German reports – the HGB statements,
  the VAT return and the Anlage EÜR –, a translation of their own; the German wording is the one that counts.
- [0004 Chart of accounts](spec/0004-chart-of-accounts.md): opening balances are booked against the opening balance
  sheet, a memo account (`Chart::opening_balances_v0_1`); balances carry on between years by themselves.
- [0008 Parties](spec/0008-parties.md): a configuration names the `region` whose public holidays move the owner's
  deadlines (ISO 3166-2, `Region`), and a VAT scheme whether its deadlines are `extended` for good (§ 46 UStDV).

## 0.3.0 – 2026-10-29

Sets of books as a whole – their identity, transactions shared between them, how each owner keeps its books, closed
periods –, the review of bookings and reversals as rules of the protocol, every expense account its line of the
Anlage EÜR 2025 and 2026, and bank statements read from camt.053, MT940 and the CSV files of banks.

- [0002 Events](spec/0002-events.md): every set of books is founded with an identity (`books_founded`), named in the
  envelope of encrypted events (part 0006); not founded or founded twice is for review.
- [0001 Base types](spec/0001-base-types.md), [0002 Events](spec/0002-events.md): a booking may name a `transaction`
  it shares with bookings in other sets of books – a bill split between a business and its owner's private books –,
  with the identities of those sets; whoever holds several checks that the sides exist, name each other, have the
  same date and are reversed alike.
- [0008 Parties](spec/0008-parties.md): how an owner keeps its books from a date on (`owner_configured`):
  functional currency, start of the financial year, accounting and VAT scheme with basis and period of advance
  returns, with the codes for Germany; a posting before an owner's first configuration and a configuration of a
  non-owner are for review.
- [0002 Events](spec/0002-events.md), [0010 Audit](spec/0010-audit.md): closing an owner's books up to a date
  (`period_closed`) with the state it was decided on and the software the figures were derived with; events it had
  not seen, and bookings or configurations written later into the closed period, are for review.
- [0002 Events](spec/0002-events.md): the review of bookings and reversals as rules of the protocol – invalid
  bookings, reversals of what is not a recorded booking, double reversals, and reversals that do not cancel exactly.
- [0008 Parties](spec/0008-parties.md): the configuration names the national chart an owner works with (`chart`,
  `de-skr03`, `de-skr04`), so that every device shows the same.
- [0009 Reports](spec/0009-reports.md): the Anlage EÜR of 2026 (BMF letter of 1 September 2026), with the
  Kennzahlen of 2025; the German names of its fields in the wording of the form of 2026.
- [0009 Reports](spec/0009-reports.md): report mappings can leave accounts out (`excluded`); a reversal counts in
  the field of the posting it cancels, also where the side decides the field. The Anlage EÜR 2025 and 2026 gives every
  expense account its line after the instructions to the form – 244 accounts, most of them from SKR 03/04, had fallen
  to the line of other expenses –, leaves out what a cash basis does not have, and books commuting and the proceeds of
  assets sold at a loss as the form asks.
- [0005 Documents](spec/0005-documents.md): bank statements – account, period, balances, entries with their
  transactions (`statement_data`) –, read from ISO 20022 camt.053 in the versions 001.02 and 001.08 that German banks
  deliver; a booking names the entries of a statement it books (`entries`, part 0001); entries booked twice, entries
  of no recorded statement, entries two statements show, and statements that do not close at the balance of their
  account (the one opened with the attribute `iban`, part 0004) are for review.
- Library: `StatementData`, `StatementAccount`, `StatementEntry`, `EntryDetail`, `Counterparty`, `EntryReference`,
  `Document::statement`, `Booking::entries`, `camt::read`, `EventSet::statements`, `EventSet::booked_entries`,
  `EventSet::review_statements`, `StatementProblem`.
- [0005 Documents](spec/0005-documents.md): statements read from SWIFT MT940 as German banks write it – the account
  from BLZ and account number, the structured field `:86:` with its SEPA references – and from the CSV files of banks,
  their columns found by name; one entry point tells the formats apart. Library: `read_statements`, `ReadStatement`,
  `ReadError`, `mt940::read`, `bank_csv::read`.
- Library: `EventSet::review_bookings`, `BookingProblem`, `Booking::cancels`, `Configuration::chart`,
  `ReportMapping::de_euer_2026`, `ReportMapping::excluded`.
- Library: `BooksId`, `Payload::BooksFounded`, `EventSet::books`, `EventSet::review_books`, `Transaction`,
  `TransactionId`, `Booking::transaction`, `review_transactions`, `Configuration`, `VatScheme`, `VatBasis`,
  `ReturnPeriod`, `RegimeCode`, `Payload::OwnerConfigured`, `EventSet::configuration`, `Closing`,
  `Payload::PeriodClosed`, `EventSet::closings`, `EventSet::closed_until`, `EventSet::review_closings`; new problems
  `PartyProblem::NotConfigured`, `PartyProblem::ConfigurationOfNonOwner`, `BooksProblem`, `TransactionProblem`,
  `ClosingProblem`.

## 0.2.0 – 2026-10-28

German law: e-invoices after EN 16931 in full, the reports and the E-Bilanz, the GoBD export, foreign currency,
filings, and the standard charts SKR 03 and SKR 04.

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
- [0004 Chart of accounts](spec/0004-chart-of-accounts.md): translations of DATEV's standard charts SKR 03 and SKR 04,
  valid for 2026 (`de-skr03`, `de-skr04`), every account of classes 0 to 8 in its HGB position, both reversible: the
  chart grows by 1,514 accounts, with names in German, English and French – drafts for review by tax advisers.
- [0007 Filings](spec/0007-filings.md): filings derived from the books – VAT return, income statement on a cash
  basis, annual accounts (E-Bilanz) – with the content of their reports, the events of preparing, submitting,
  accepting and rejecting them, their review, and the REST interface of adapters such as the German one for ELSTER.
- [0001 Base types](spec/0001-base-types.md): postings in foreign currency carry the exchange rate and the amount in
  the owner's functional currency (`exchange`); converted, a booking balances as well; reports use the converted
  amounts, and the GoBD export lists rate and amounts in the functional currency.
- [0009 Reports](spec/0009-reports.md): the E-Bilanz (§ 5b EStG) in the taxonomy 6.10 – balance sheet and profit
  and loss account by nature of expense, derived from the HGB down to the Mussfelder and catch-all positions, sales
  by VAT; tax rules that apply `within` a position, and the `mandatory` positions of a report.
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
  `en16931::write_ubl`, `en16931::write_cii`, `en16931::attach_to_pdf`, `propose_booking`, `gobd::export`,
  `ChartTranslation::de_skr03()`, `ChartTranslation::de_skr04()`, `ReportMapping::de_ebilanz_bs_6_10()`,
  `ReportMapping::de_ebilanz_is_gkv_6_10()`, `Exchange`, `Posting::functional`, `EventSet::functional_balances`,
  `Filing`, `EventSet::report_content`, `EventSet::filings`, `EventSet::review_filings`; dependencies `roxmltree`,
  `base64`, and `lopdf` for the feature `pdf`.

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
