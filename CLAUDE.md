# CLAUDE.md – librotenado

Rules for working in this repository. This file is public: nothing personal goes here.

## What this is

librotenado – the open protocol for accounting data: the specification (`spec/`, CC BY 4.0) and the reference library
in Rust (`crates/`, MIT). [spezado](https://github.com/spezado/spezado) is its first implementation, but the protocol
belongs to no product: anyone may implement it, including commercial vendors.

Version 0.5 (draft): `spec/0001`–`0010` implemented in the library (base types, events and sets of books, canonical
encoding, chart of accounts, documents and bank statements, encryption, filings, parties and their configuration,
reports, audit), the adapter interface of `0007` a proposal. Reference data –
the currency table, the chart, translations, names, tax tables, report mappings – lives in `spec/data/`. State per
part in `spec/README.md`, changes in `CHANGELOG.md`.

Documentation (`docs/`, mdBook, published to librotenado.dev by `.github/workflows/docs.yml`): the pages under
`docs/src/spec/` only include `spec/` – the specification has one home. A new part of the specification needs an
include file there and a line in `docs/src/SUMMARY.md`.

## Principles

- **A meta layer:** one harmonised model; EN 16931/Peppol, national e-invoicing systems, SAF-T, ISO 20022 (camt),
  XBRL and national charts of accounts are translations – unambiguous, reversible, versioned, tested per country.
- **The model grows, translations do not bend:** where a country distinguishes more finely, extend the model.
- **Originals stay:** a translated document never replaces the original, which remains the receipt.
- **Sync built in:** a growing set of immutable events, hybrid logical clocks, a hash chain; conflicts on invariants
  go to a review list, never merged silently.

## Licences

- `spec/`: **CC BY 4.0**; library code: **MIT**; SPDX headers, REUSE (`REUSE.toml`).
- Never copy code from spezado (AGPL) into this repository – only code written for it.

## Rules

- Library and specification change together: what `spec/` says is what the library does, and the other way round.
- `cargo fmt`, `cargo clippy --all-targets` (no warnings) and `cargo test` before every commit.

- English in code, specification and commits; commits signed off (`git commit -s`).
- No personal data in tracked files.
- Every claim about an existing standard needs a source; unverified statements are marked **OPEN**.
