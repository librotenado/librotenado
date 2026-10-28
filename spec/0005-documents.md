# 0005 Documents

Version 0.1, draft. Implemented in the reference library (`Document`, `Original`, event kind `document_recorded`).

A document is what a booking is based on: an invoice, a credit note, a receipt, a bank statement. It is a **record of
its own**: its data – kind, number, date – stand in the document, never only in its file. The file it came as, the
**original**, is an artefact: kept byte for byte as the receipt in the legal sense, but nothing in the books depends
on reading it. Everything derived from the original – the document's data, a booking proposal, a translation into
the model – refers to it and never replaces it.

**There is no document without its original.** If the file is missing – a lost till receipt – there is no
document; the booking has no `documents`, and its postings say why in their `note` (part 0001).

The original is the document itself – the PDF, the XML of an e-invoice, the image of a receipt – not the way it
travelled: an e-mail that carried an invoice is not part of the document.

## Describing a document

A document is an object of type `document` (part 0003):

| Member | Content |
|---|---|
| `id` | its identity, a UUID (part 0001), like an account's |
| `kind` | `invoice`, `credit_note`, `receipt`, `bank_statement`, or – from an authority (part 0007) – `acknowledgement` or `notice` |
| `number` | its number as printed on it, e.g. the invoice number; absent if it has none, never `null` or empty |
| `date` | the date it bears, e.g. the invoice date (part 0001) |
| `received` | the day it was received; it may be earlier than the day it was recorded – a letter scanned days later |
| `original` | the file it came as (below) |

Later versions add kinds and members – the parties, the due date, the taxes and totals of an invoice (OPEN). An
implementation keeps a document of a kind it does not know, but does not interpret it.

## The original

The original is an object of type `original`:

| Member | Content |
|---|---|
| `hash` | the SHA-256 hash of its bytes, written `sha256:` followed by 64 lower-case hexadecimal digits, like every hash in the protocol |
| `size` | its size in bytes |
| `media_type` | its media type, e.g. `application/pdf`, `application/xml` |
| `format` | the structured format it carries, if any (below); absent otherwise, never `null` or empty |
| `name` | the file name it arrived with; absent if none, never `null` or empty |

- Version 0.1 knows only SHA-256; the prefix lets a later version add another algorithm without confusion.
- The bytes are not part of an event. They are stored in any store addressed by their hash (a directory, an
  S3-compatible bucket, …) and synchronised alongside the events. An implementation MUST check the size and the
  hash of bytes it receives before it accepts them as the original.

Formats in version 0.1 (names are lower case; more follow with their translations):

| `format` | Meaning |
|---|---|
| `en16931-ubl` | an invoice according to EN 16931 in the UBL syntax (e.g. XRechnung, Peppol BIS Billing 3.0) |
| `en16931-cii` | an invoice according to EN 16931 in the UN/CEFACT CII syntax – also a PDF with an embedded CII invoice (ZUGFeRD/Factur-X) |
| `iso20022-camt.053` | a bank statement (ISO 20022 BankToCustomerStatement) |

## Events

- `document_recorded` with `document` – a document was recorded with its data and original (part 0002).
- A booking MAY list the documents it is based on in `documents`, as an array of document identities; the member is
  absent when there are none (part 0001). A document may back several bookings (an invoice and its payment), a
  booking may rest on several documents.
- An implementation MUST report, and MUST NOT silently resolve, a booking based on a document that has not been
  recorded (it may not have arrived yet), and a document recorded twice under the same identity.

## Open points

- The further data of a document: parties, due date, period of service, taxes and totals – for invoices taken from
  EN 16931 by translation.
- Correcting the data of a recorded document by a later event.
- Translations of documents into bookings: their form and their versioning, per format.
- Whether the names of formats should be the identifiers the standards define (e.g. the specification identifier of
  EN 16931 and its CIUS such as XRechnung) instead of names of our own.
- Which rules require keeping the structured part of an e-invoice as received (see the whitepaper).
