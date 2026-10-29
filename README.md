<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/svg/librotenado-lockup-on-dark.svg">
    <img src="brand/svg/librotenado-lockup-on-light.svg" alt="librotenado – open accounting protocol" width="520">
  </picture>
</p>

# librotenado

**An open protocol for accounting data in Europe** – one model for bookings, receipts, invoices, reports and
filings that the existing European formats translate into and out of.

> **Status:** version 0.7, a draft: base types, events and synchronisation, the canonical encoding, the harmonised
> chart of accounts with the French PCG and the German SKR 03 and SKR 04, documents and e-invoices after EN 16931,
> bank statements, end-to-end encryption, parties, reports – HGB, VAT return, Anlage EÜR, E-Bilanz –, filings with
> the interface to adapters, and the GoBD export are specified and implemented. Everything may change before 1.0.
> The first implementation is [spezado](https://github.com/spezado/spezado). The reasoning is in the
> [whitepaper](https://github.com/librotenado/whitepaper); the documentation is at
> [librotenado.dev](https://librotenado.dev).

## Aim

- **One harmonised model** – accounts, bookings, currencies, receipts, invoices, reports and filings, designed once
  and well, instead of one format per country.
- **Compatible with what exists** – EN 16931 and Peppol, national e-invoicing systems, SAF-T, ISO 20022 (camt), XBRL
  and national charts of accounts are translations of this model, unambiguous and reversible.
- **Made for synchronisation** – an event model with end-to-end encryption, so that apps, relays and servers of any
  vendor can exchange books.
- **Open for everyone** – the specification may be used and implemented by anyone, including commercial vendors.

## What is here

| Path | Content | Licence |
|---|---|---|
| [`spec/`](spec/) | the specification | CC BY 4.0 |
| [`brand/`](brand/) | copies from the brand kit, [librotenado/brand](https://github.com/librotenado/brand): lockups, icons, fonts | artwork CC BY-ND 4.0, fonts OFL-1.1; name and mark: see [brand/README.md](brand/README.md) |
| `crates/librotenado` | the reference library in Rust: data model, events, merging – later formats and encryption | MIT |
| `crates/librotenado/tests/data` | test invoices of the XRechnung test suite (KoSIT) and of CEN/TC 434 | Apache-2.0, EUPL-1.2 |
| [`docs/`](docs/) | the documentation at [librotenado.dev](https://librotenado.dev) (Jekyll with Just the Docs; brings in `spec/` when it is built, does not copy it) | text CC BY 4.0, build files MIT |

```sh
cargo test                         # Rust 1.94 or later
cargo run --example quickstart     # two devices, one set of books
docs/prepare.sh                    # then Jekyll builds docs/ – see .github/workflows/docs.yml
```

Changes per version are in [CHANGELOG.md](CHANGELOG.md).

Licences follow the [REUSE specification](https://reuse.software); the texts are in [LICENSES/](LICENSES/).

## Contributing

Issues and pull requests are welcome – see [CONTRIBUTING.md](CONTRIBUTING.md): a
[DCO](https://developercertificate.org) sign-off (`git commit -s`), no CLA.

© 2026 falkOSS – Falcon Lake Open Source Software gUG (haftungsbeschränkt) i. G. and the librotenado contributors
