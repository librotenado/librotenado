# 0007 Filings

Version 0.1, draft – not yet implemented. Everything here is a proposal for discussion.

Books end in filings: VAT returns, recapitulative statements, annual financial statements, audit files. Every country
has its own systems for them. librotenado defines one open interface; **adapters** connect it to the systems of
each country.

## Filings

A filing is an object of type `filing` (part 0003):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001) |
| `kind` | what it is, e.g. `vat_return`, `recapitulative_statement`, `annual_accounts` |
| `jurisdiction` | the country it is filed in, ISO 3166-1 alpha-2 (`DE`) |
| `filer` | the organisation or person filing (part 0008), an owner of the books |
| `period` | the period it covers: `from` and `to`, both dates (part 0001), inclusive |
| `basis` | the state of the books it was derived from (part 0002) |
| `corrects` | the filing it corrects, if any; absent otherwise |
| `content` | what it reports, in the terms of the model – per kind (OPEN) |

- **A filing is derived from the books and records from what.** `basis` is the state of the books when it was
  derived: whoever has the same events can derive it again and get the same content. When an event that concerns its
  period arrives later, the state no longer matches – the filing may need a correction.
- **A filing is never changed.** A correction is a new filing whose `corrects` names the first, as a reversal does
  for a booking.

## Events

| `kind` | Members | What happened |
|---|---|---|
| `filing_prepared` | `filing` | a filing was derived and is ready to be submitted |
| `filing_submitted` | `filing`, `documents` | it was submitted; `documents` are what the authority returned on submission |
| `filing_accepted` | `filing`, `documents` | the authority accepted it |
| `filing_rejected` | `filing`, `documents` | the authority rejected it |

In the last three, `filing` is the identity of the prepared filing. What an authority returns – a confirmation of
receipt, a protocol, a notice – is a document (part 0005) of kind `acknowledgement` or `notice`, with its original.
A rejected filing, and a filing whose `basis` no longer matches the books for its period, go to the review list
(part 0002).

## Adapters

An **adapter** translates a filing into the format of one country and submits it. Adapters are separate services:
an implementation talks to an adapter over **HTTPS**, with a documented REST or GraphQL interface (OPEN: which, or
both). So an adapter can run on the same machine or as a service of a provider, and can use libraries that cannot be
part of the implementation itself – for example Germany's ERiC for ELSTER, which is not open source (OPEN: the
conditions of its use).

**Credentials are not part of the protocol.** Certificates, passwords and tokens for an authority's system belong to
the adapter or to the device that submits; an event records that a filing was submitted and by whom, never how
access was gained.

Adapters are tested with the official test cases of each authority where they are published.

## Open points

- The kinds of filings and their content per country, starting with Germany.
- The adapter interface, request by request: REST or GraphQL, authentication of the implementation towards the
  adapter.
- Which events concern a filing's period – and so make its `basis` outdated – per kind.
