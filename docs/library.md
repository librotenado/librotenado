---
title: Getting started with the library
nav_order: 3
---

# Getting started with the library

`librotenado` is the reference library in Rust (MIT). It implements the parts of the specification marked as
implemented: base types, events and merging, the canonical encoding, the chart of accounts with national charts as
translations, documents and bank statements, filings, parties and their configuration, reports and the audit data
transfer. The full API is in the [API documentation](api/librotenado/index.html).

The crate is not yet published on crates.io; use it from Git:

```toml
[dependencies]
librotenado = { git = "https://github.com/librotenado/librotenado", tag = "v0.7.2" }
```

## Two devices, one set of books

A laptop and a phone each record a booking. They exchange their events – in any order, as often as they like – and
end up with the same books. Each event is identified by the hash of its canonical encoding, so every implementation,
in any language, computes the same identifier.

```rust
{% include quickstart.rs %}
```

Run it from a checkout:

```sh
cargo run --example quickstart
```

It prints each event's hash and its canonical encoding:

```text
sha256:ee872f6362d30fc9f5a605100f42f88d9a73dc15b6769d766435fe161a7022e7
{"booking":{"date":"2026-10-24","postings":[{"account":"0192f1e4-0000-7000-8000-000000000002","amount":{"base_units":"1240","currency":"EUR"},"owner":"0192f1e4-0000-7000-8000-0000000000a1"},{"account":"0192f1e4-0000-7000-8000-000000000001","amount":{"base_units":"-1240","currency":"EUR"},"owner":"0192f1e4-0000-7000-8000-0000000000a1"}],"text":"Office paper"},"kind":"booking_recorded","prev":null,"seq":1,"time":{"counter":0,"device":"laptop","wall_ms":1000},"type":"event"}
```

## What to know

- **Money counts the base unit.** `Money` is an integer number of cents – the smallest unit of its currency –,
  a string in JSON: `{"currency": "EUR", "base_units": "1240"}` is 12.40 euros. How a currency divides is reference
  data (`CurrencyDefinition` with `subdivisions`: `[100]` for EUR, `[17, 29]` for Galleon, Sickle and Knut), so
  the UI formats it; names come from translation data by the key `<code>/<stage>`. Codes are ISO 4217 (`EUR`) or
  begin with `x-` (`x-btc`). Prices and rates with more places are `Decimal`.
- **One value, one form.** Amounts and decimal numbers have exactly one written form (`"1240"`, `"1.5"`); other
  forms are rejected, so the same booking always has the same hash.
- **Objects say what they are.** A root object carries its `type` (`"event"`, `"account"`, …), nested objects do
  not; `kind` names the variety (`"booking_recorded"`, `"asset"`). Write and read root objects with `to_root` and
  `from_root`.
- **Accounts are UUIDs.** What an account is follows from the chart (`Chart`) or from `account_opened` events;
  `EventSet::review_accounts` reports postings on unknown accounts instead of rejecting them.
- **Documents are records of their own.** A `Document` has its own identity and data – kind, number, date – and
  points to the file it came as (`Original`), an artefact checked by size and hash. Without a file there is no
  document; a posting's `note` says why. `EventSet::review_documents` reports bookings on unrecorded documents.
- **Books belong to someone.** `Organisation` and `Person` are described by events; `owner_added` binds them to
  the books. `EventSet::review_parties` reports books without owners, `EventSet::state` the state of the books that
  filings record.
- **Filings go to adapters.** A `Filing` is derived from the books (`EventSet::report_content`) and recorded with
  `filing_prepared`; a `Submission` – the filing, its `Filer` and whether it is a test – goes to an adapter, whose
  `SubmissionStatus` carries the authority's receipts (`Receipt::original`) and becomes the next event
  (`SubmissionStatus::payload`). `EventSet::filings` says where each filing stands (part 0007).
- **The chart comes with the library.** `Chart::v0_1()` is the harmonised chart, `ChartTranslation::fr_pcg()` the
  French PCG translated into it, `Names::v0_1("de")`, `"en"` and `"fr"` name its accounts, tax codes and German
  report positions.
- **Electronic invoices.** `en16931::read` translates XRechnung and other EN 16931 invoices in UBL or CII,
  `en16931::read_pdf` the XML in a ZUGFeRD or Factur-X PDF (feature `pdf`, on by default), into a document's
  `InvoiceData`; `propose_booking` proposes a booking with German tax codes. `en16931::write_ubl` and `write_cii`
  write XRechnung 3.0, `en16931::attach_to_pdf` makes a ZUGFeRD PDF of it.
- **Tax audits.** `gobd::export` writes the books of one owner for a period as CSV files with an `index.xml`
  for the German data transfer (Z3), with account names from `Names::v0_1("de")`.
- **German reports.** Postings carry tax codes (`TaxTable::de()`); `EventSet::period_report` fills the VAT return
  of 2026, the Anlage EÜR of 2025 and the HGB profit and loss account (`ReportMapping::de_ustva_2026()`,
  `de_euer_2025()`, `de_hgb_275_gkv_2026()`), `EventSet::account_report` the HGB balance sheet
  (`de_hgb_266_2026()`).
- **Books are encrypted between their members.** `MemberKeys` come from a seed that stays on the device;
  `MemberRecord::found` starts the member list with key 0, `MemberList::apply` checks each record, `add` and `remove`
  write the next one, `keys` opens the chain of books keys. `Envelope::seal_event` and `open_event` carry events
  to and from a relay, `request_signature` signs requests to it, `Invitation` adds a device, `RecoveryCode` brings
  the books back (part 0006).
- **Balances are derived.** `EventSet::balances(until)` sums the postings per owner, account and currency;
  `EventSet::trial_balance(chart, until)` rolls them up the hierarchy. `CurrencyTable::v0_1()` says how each currency
  divides, for formatting.
- **Postings are signed and owned.** Every posting names its owner. Positive is debit, negative is credit; a booking
  balances when the postings of each owner sum to zero in every currency (`Booking::validate`).
- **Nothing is changed or deleted.** A mistake is corrected by a counter-booking (`Payload::BookingReversed`).
- **Events travel as received.** Pass on `EventSet::canonical(hash)` and read with `EventSet::receive(bytes)`: the
  hash is taken over what was received, so members and payload types of later versions survive unchanged.
- **Merging never guesses.** `EventSet::merge` is a union; `EventSet::verify` reports forks, gaps and broken links in
  a device's chain instead of resolving them.
