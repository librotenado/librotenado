# 0005 Documents

Version 0.1, draft. Implemented in the reference library (`Document`, `Original`, `InvoiceData`, `en16931`,
`propose_booking`, event kind `document_recorded`).

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
| `invoice` | the data of an invoice or credit note (below); absent when not recorded |

Later versions add kinds and members. An implementation keeps a document of a kind it does not know, but does not
interpret it.

## The data of an invoice

The data of an invoice or credit note is an object of type `invoice_data` – whether entered by hand or translated
from an electronic invoice (below). Its terms follow EN 16931, the European semantic model of an electronic invoice;
the business terms (BT) it takes them from are named in brackets.

| Member | Content |
|---|---|
| `currency` | the currency of the invoice (BT-5) |
| `seller`, `buyer` | the parties (type `invoice_party`): `name` (BT-27, BT-44), and where stated `vat_id` (BT-31, BT-48), `tax_number` (BT-32), `street`, `postcode`, `city`, `country` (BG-5, BG-8), `contact` (type `invoice_contact`: `name`, `phone`, `email`, BG-6, BG-9), `endpoint` (type `endpoint`: `scheme` and `address`, BT-34, BT-49), and `party` – the organisation or person of the books it is (part 0008), once known |
| `due_date` | when payment is due (BT-9) |
| `delivery_date` | when the supply was made (BT-72) |
| `buyer_reference` | the reference the buyer asked for, e.g. the Leitweg-ID of a German public authority (BT-10) |
| `payment_means` | how it is paid, a code of UNTDID 4461, e.g. `58` SEPA credit transfer (BT-81) |
| `iban` | the account to pay to (BT-84) |
| `payment_terms` | the terms of payment as text (BT-20) |
| `taxes` | the VAT per category and rate (BG-23, type `vat_breakdown`), at least one: `category`, `rate` in percent where the category has one, `base` and `tax` as money, and `exemption` where stated (BT-116 to BT-120) |
| `lines` | the lines (BG-25, type `invoice_line`): `id`, `name`, `quantity` and `unit` (UN/ECE Rec. 20), `price` per unit as a decimal number, `net` as money, `category`, `rate` (BT-126, BT-153, BT-129, BT-130, BT-146, BT-131, BT-151, BT-152); absent when not recorded |
| `totals` | type `invoice_totals`: `net` (BT-109), `tax` (BT-110), `gross` (BT-112), `due` (BT-115) |

The VAT categories are the codes EN 16931 uses (UNCL 5305): `S` standard or reduced rate, `Z` zero rate, `E` exempt,
`AE` reverse charge, `K` intra-Community supply, `G` export, `O` not subject to VAT, `L` and `M` the indirect taxes of
the Canary Islands and of Ceuta and Melilla. Amounts are money of the invoice's currency (part 0001), as printed –
positive on a credit note, too: its `kind` says which way it books.

## Electronic invoices

An electronic invoice according to EN 16931 is translated into a document's data: its number (BT-1), date (BT-2),
kind – `credit_note` for type code 381 or a UBL CreditNote, otherwise `invoice` (BT-3) – and its `invoice` data. Both
syntaxes give the same data:

- **UBL** – XRechnung, Peppol BIS Billing 3.0 – as a UBL Invoice or CreditNote;
- **UN/CEFACT CII** – XRechnung, and the XML that ZUGFeRD and Factur-X embed in a PDF; the translation reads the XML
  attached to the PDF.

In Germany an invoice is electronic in the sense of § 14 UStG if it follows EN 16931: XRechnung, which is in force in
version 3.0 ([KoSIT](https://xeinkauf.de/xrechnung/versionen-und-bundles/)), and ZUGFeRD from version 2.0.1 except
its profiles MINIMUM and BASIC WL (BMF letter of 15 October 2024, III C 2 - S 7287-a/23/10001 :007, Rz. 29 and 30).
The paths of the business terms in both syntaxes follow the open syntax bindings of Peppol BIS Billing 3.0
([OpenPEPPOL](https://docs.peppol.eu/poacc/billing/3.0/)) and of the XRechnung visualisation (KoSIT). The reference
library is tested with the XRechnung test suite of KoSIT and a credit note of CEN/TC 434.

## Writing XRechnung and ZUGFeRD

From the data of a document an implementation writes the invoice it issues: as **XRechnung 3.0** in UBL or in CII,
and – attaching the CII to a PDF/A-3 – as a **ZUGFeRD / Factur-X** hybrid of the profile XRECHNUNG. Before writing,
it checks that the data carries what XRechnung requires beyond EN 16931: the buyer reference (BT-10), the means of
payment (BT-81) and, for a transfer, the IBAN (BT-84), the seller's address, contact and VAT or tax number, the
electronic addresses of both parties (BT-34, BT-49), the buyer's city, postcode and country, lines with identifier,
quantity, unit and price, and a reason for every amount that is exempt or not taxed (BT-120). It names what is missing
instead of writing an invalid invoice.

In the PDF the XML is the embedded file `xrechnung.xml`, related to the document as its `Alternative`, listed in the
catalogue's associated files (`AF`) and embedded files, and described in the XMP metadata with the Factur-X schema
(`fx:ConformanceLevel` XRECHNUNG). The PDF itself – fonts, colours, output intent of PDF/A-3 – is the implementation's.

The reference library writes both syntaxes from every test invoice of the KoSIT test suite that carries the terms
XRechnung requires, reads them back to the same data, and the files validate against the schemas of UBL 2.1 and of CII
D16B shipped with the XRechnung validator configuration of KoSIT.

## Booking proposals

From the data of an invoice an implementation proposes a booking; whoever books reviews it and may change every
posting. Per VAT category and rate, the proposal books the net amount on a counter account – an expense for a
purchase, income for a sale – and the VAT on the VAT account, each with the tax code of the country (part 0001), and
the gross amount on the account of the supplier or customer. A credit note books the other way round. Under reverse
charge and on an intra-Community acquisition the buyer computes the VAT it owes and deducts it in the same booking.

The German tax codes follow from category, rate and side:

| Category | Sale | Purchase |
|---|---|---|
| `S` 19 % / 7 % | `de-out-19` / `de-out-7` | `de-in-19` / `de-in-7` |
| `Z` | `de-out-0` | – |
| `E` | `de-out-exempt` | – |
| `AE` | `de-out-13b` | `de-in-13b-domestic`, or `de-in-13b-eu-service` from a seller abroad, at 19 % |
| `K` | `de-out-ig-supply` | `de-in-ig-acquisition-19` |
| `G` | `de-out-export` | – |
| `O` | `de-out-not-taxable` | – |

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
| `en16931-cii` | an invoice according to EN 16931 in the UN/CEFACT CII syntax – also a PDF with an embedded CII invoice (ZUGFeRD, Factur-X) |
| `iso20022-camt.053` | a bank statement (ISO 20022 BankToCustomerStatement) |

## Events

- `document_recorded` with `document` – a document was recorded with its data and original (part 0002).
- A booking MAY list the documents it is based on in `documents`, as an array of document identities; the member is
  absent when there are none (part 0001). A document may back several bookings (an invoice and its payment), a
  booking may rest on several documents.
- An implementation MUST report, and MUST NOT silently resolve, a booking based on a document that has not been
  recorded (it may not have arrived yet), and a document recorded twice under the same identity.

## Open points

- Further terms of EN 16931: allowances and charges on the document and its lines (BG-20, BG-21, BG-27, BG-28), the
  invoicing period (BG-14), prepaid amounts, VAT in a second currency (BT-6, BT-111), payment terms (BT-20).
- Reverse charge and intra-Community acquisitions at the reduced rate: the proposal assumes 19 %; rates other than
  19 % and 7 % get no tax code.
- Validating written invoices against the business rules of EN 16931 and XRechnung (the schematron of the KoSIT
  validator) – so far against the XML schemas only.
- Allowances and charges on the document level, which writing does not support yet; profiles of ZUGFeRD other than
  XRECHNUNG (EN 16931, EXTENDED); the file name `xrechnung.xml` of the profile XRECHNUNG, to be verified against the
  ZUGFeRD specification.
- Correcting the data of a recorded document by a later event.
- Whether the names and definitions of the business terms of EN 16931 – a standard of CEN, © CEN and its members –
  may be quoted in a specification under CC BY; this part names their identifiers only.
- Whether the names of formats should be the identifiers the standards define (e.g. the specification identifier of
  EN 16931 and its CIUS such as XRechnung) instead of names of our own.
- Which rules require keeping the structured part of an e-invoice as received (see the whitepaper).
