# 0007 Filings

Version 0.7, draft. Implemented in the reference library: filings and their events (`Filing`,
`EventSet::report_content`, `EventSet::filings`, `EventSet::review_filings`) and what an implementation and an
adapter send each other (`Submission`, `SubmissionStatus`, `Receipt`, `AdapterError`).

Books end in filings: VAT returns, income statements, annual accounts, recapitulative statements. Every country has
its own systems for them. librotenado defines one open interface; **adapters** connect it to the systems of each
country.

## Filings

A filing is an object of type `filing` (part 0003):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001) |
| `kind` | what it is (below) |
| `jurisdiction` | the country it is filed in, ISO 3166-1 alpha-2 (`DE`) |
| `filer` | the organisation or person filing (part 0008), an owner of the books |
| `from`, `to` | the period it covers, both days included (part 0001) |
| `basis` | the state of the books it was derived from (part 0002) |
| `corrects` | the filing it corrects, if any; absent otherwise |
| `content` | what it reports: one `report_content` per report, at least one |

A report content (type `report_content`) is `report` and `version` – of the report mapping it was derived with (part
0009) – and `fields`: position → money, in the filer's functional currency (part 0001), positions without an amount
left out. A mandatory position of the mapping without an amount is stated as empty by the adapter.

| `kind` | Content | Derived | Germany |
|---|---|---|---|
| `vat_return` | the VAT return | over the period | Umsatzsteuer-Voranmeldung (`de-ustva`) |
| `cash_basis_statement` | the income statement on a cash basis | over the period | Anlage EÜR (`de-euer`) |
| `annual_accounts` | balance sheet and profit and loss account | balances on the last day, and over the period | E-Bilanz (`de-ebilanz-bs`, `de-ebilanz-is-gkv`) |

- **A filing is derived from the books and records from what.** `basis` is the state of the books when it was
  derived: whoever has the same events derives it again and gets the same content. When an event that concerns its
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

In the last three, `filing` is the identity of the prepared filing and `documents` an array of document identities,
absent when empty. What an authority returns – a confirmation of receipt, a protocol, a notice – is a document (part
0005) of kind `acknowledgement` or `notice`, with its original. A filing moves on with each event; after a rejection
it may be submitted again. The review list (part 0002) holds a rejected filing, an event about a filing never
prepared, and a filing prepared twice under one identity.

## Adapters

An **adapter** translates a filing into the format of one country and submits it. Adapters are separate services:
an implementation talks to an adapter over **HTTPS** with **REST**, bodies in the canonical encoding (part 0003). So
an adapter can run on the same machine or as a service of a provider, and can use libraries that cannot be part of
the implementation itself – for example Germany's ERiC for ELSTER, which is not open source.

| Request | Body | Answer |
|---|---|---|
| `POST /filings` | a `submission` | `202` and a `submission_status` |
| `GET /filings/{id}` | – | `200` and the `submission_status` of the filing; `404` if the adapter has none |

Bodies are root objects (part 0003) with the media type `application/json`.

| Type | Members |
|---|---|
| `submission` | `filing` – the filing as prepared –, `filer` and `test`: `true` for a test submission the authority does not process, `false` otherwise |
| `submission_status` | `filing` – its identity –, `state` – `submitted`, `accepted` or `rejected` –, `receipts` – what the authority returned, absent when nothing – and `message` – how the authority explained a rejection, absent otherwise |
| `receipt` | `media_type`, `name` – its file name – and `content` – its bytes, written as in part 0006: base64url without padding |
| `adapter_error` | `message` – why the adapter cannot process a request, in words |

`filer` is the organisation or person filing (part 0008) with its tax identities, as an object with one member:
`organisation` or `person`. It is the filing's `filer`; an adapter refuses a submission whose filer is another.

The implementation records each receipt as a document – its original made from `media_type`, `name` and `content`
(part 0005) – and the state as an event: `submitted` as `filing_submitted`, `accepted` as `filing_accepted`,
`rejected` as `filing_rejected`, each naming the documents of the receipts. The adapter keeps nothing it needs after
the answer. An adapter answers a request it cannot process with `4xx` or `5xx` and an `adapter_error`.

**The implementation authenticates itself to the adapter with a token**, sent as a bearer token in the
`Authorization` header ([RFC 6750](https://www.rfc-editor.org/rfc/rfc6750), section 2.1). Adapter and implementation
agree on the token outside the protocol; an adapter answers a request without a token it accepts with `401`.
Mutual TLS may be used in addition.

**Credentials are not part of the protocol.** Certificates, passwords and tokens for an authority's system belong to
the adapter or to the device that submits; an event records that a filing was submitted and by whom, never how
access was gained.

Adapters are tested with the official test cases of each authority where they are published.

### Germany

The German adapter submits through ELSTER with ERiC, a C library the tax administration provides free of charge to
registered developers ([ELSTER for developers](https://www.elster.de/eportal/infoseite/entwickler)): the VAT return as Umsatzsteuer-Voranmeldung
with its Kennzahlen, the annual accounts as the XBRL instance of the E-Bilanz – the GCD module from the filer's
master data, the GAAP module from the content, mandatory positions without an amount as empty (NIL) – and the
income statement on a cash basis as the Anlage EÜR with the income tax or trade tax return it belongs to.

## Open points

- The conditions of using ERiC, and its data formats, which are published to registered developers only.
- The master data the E-Bilanz needs beyond part 0008 (GCD: report type, legal form, Finanzamt), and the other
  modules of the taxonomy.
- Which events concern a filing's period – and so make its `basis` outdated – per kind.
- Further kinds: the recapitulative statement (§ 18a UStG), the annual VAT return, the income, corporation and trade
  tax returns.
