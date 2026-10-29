# librotenado

**The open protocol for accounting data in Europe** – one model for bookings, receipts, invoices, reports and
filings that the existing European formats translate into and out of.

This is the documentation of version 0.6, a draft. Everything may change before 1.0.

## What it is

- **One harmonised model.** Accounts, bookings, currencies, documents and filings are designed once. EN 16931 and
  Peppol, national e-invoicing systems, SAF-T, ISO 20022 (camt), XBRL and national charts of accounts become
  translations of it – unambiguous, reversible, versioned and tested per country.
- **The model grows, translations do not bend.** Where a country distinguishes more finely, the model is extended.
- **Originals stay.** A translated document never replaces the original; the original remains the receipt.
- **Made for synchronisation.** A set of books is a growing set of immutable events. Devices exchange events and merge
  them by union; what cannot be merged silently goes to a review list, never guessed.
- **Open for everyone.** The specification is CC BY 4.0, the reference library MIT. Anyone may implement it,
  including commercial vendors. The first implementation is [spezado](https://spezado.dev).

## How to read it

| If you want to … | read |
|---|---|
| understand why the protocol exists | the [whitepaper](https://github.com/librotenado/whitepaper) |
| know what an implementation must do | the [specification](spec/overview.md), starting with [0001](spec/0001-base-types.md) and [0002](spec/0002-events.md) |
| build on the Rust library | [Getting started](library.md) and the [API documentation](api/librotenado/index.html) |
| contribute | [CONTRIBUTING.md](https://github.com/librotenado/librotenado/blob/main/CONTRIBUTING.md) |

The specification uses the key words MUST, SHOULD and MAY as defined in RFC 2119 and RFC 8174. Statements that are not
yet verified against their source are marked **OPEN**.
