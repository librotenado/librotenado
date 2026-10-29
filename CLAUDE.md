# CLAUDE.md – librotenado

Rules for working in this repository. This file is public: nothing personal goes here.

## What this is

librotenado – the open protocol for accounting data: the specification (`spec/`, CC BY 4.0) and the reference library
in Rust (`crates/`, MIT). [spezado](https://github.com/spezado/spezado) is its first implementation, but the protocol
belongs to no product: anyone may implement it, including commercial vendors.

Version 0.7 (draft): `spec/0001`–`0010` implemented in the library (base types, events and sets of books, canonical
encoding, chart of accounts, documents and bank statements, encryption, filings, parties and their configuration,
reports, audit), with the adapter interface of `0007`. Reference data –
the currency table, the chart, translations, names, tax tables, report mappings – lives in `spec/data/`. State per
part in `spec/README.md`, changes in `CHANGELOG.md`.

Documentation (`docs/`, Jekyll with Just the Docs, published to librotenado.dev by `.github/workflows/docs.yml`):
`docs/prepare.sh` brings in `spec/`, its data, the changelog and the example when the site is built – nothing of it is
tracked in `docs/`, the specification has one home; a new part needs nothing more than its file in `spec/`. The API
documentation of the library (rustdoc) goes to `api/`. Local build: `docs/prepare.sh`, then Jekyll as GitHub Pages
runs it (the image of `actions/jekyll-build-pages`).

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
- `brand/` holds copies from [librotenado/brand](https://github.com/librotenado/brand) (artwork CC BY-ND 4.0, fonts
  OFL-1.1): changed there, copied here, never edited in place.

## Rules

- Library and specification change together: what `spec/` says is what the library does, and the other way round.
- `cargo fmt`, `cargo clippy --all-targets` (no warnings) and `cargo test` before every commit; `cargo deny check`
  and `scripts/check-age.py` after changing dependencies.
- **Shape after falkoss/starterpack** (`docs/practices.md`), with these deviations and their reasons:
  - **Ranges in `Cargo.toml`, not exact versions:** the library is built on by others, and exact versions would force
    theirs. The rule of 14 days holds for `Cargo.lock`.
  - **Its own Pages workflow, not the shared `pages-jekyll.yml`:** the documentation brings in `spec/` and the
    changelog before Jekyll builds, and adds the rustdoc of the library; the shared workflow builds a folder only. It
    uses the same actions, pinned alike.

- English in code, specification and commits; commits signed off (`git commit -s`).
- No personal data in tracked files.
- Every claim about an existing standard needs a source; unverified statements are marked **OPEN**.
