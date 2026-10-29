# Contributing to librotenado

Thank you for helping. librotenado is the open protocol for accounting data in Europe: the specification in `spec/`
and the reference library in `crates/`.

## How

1. Open an issue first for anything larger than a fix – the model is meant to stay harmonious, so changes are
   discussed before they are written.
2. Fork, change, and open a pull request. CI checks format, lints, tests and licences.
3. Sign off every commit (`git commit -s`): with it you confirm the
   [Developer Certificate of Origin](https://developercertificate.org). There is no CLA.

## Rules

- **Licences:** the specification is CC BY 4.0, the library MIT. Every source file names its licence in an SPDX
  header; files that cannot carry one are listed in `REUSE.toml`. Never copy code from projects under other
  licences – in particular not from spezado (AGPL).
- **Sources:** every statement about an existing standard, law or system needs a source; what is not verified is
  marked **OPEN**.
- **Translations do not bend:** where a country distinguishes more finely than the model, propose to extend the model.
- **Money is never a float;** it is an integer count of the smallest unit, written as a string; every value has
  one written form.
- English in code, specification and commits.

## Building

```sh
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo deny check            # dependencies: advisories, licences, sources
scripts/check-age.py        # after changing Cargo.lock: no crate younger than 14 days
```

Dependencies are locked in `Cargo.lock`, each the newest version that is at least 14 days old (`scripts/check-age.py
--fix` sets back the younger ones); a fix for a security advisory may be younger. `Cargo.toml` keeps ranges, not exact
versions, so that projects building on the library resolve their own.
