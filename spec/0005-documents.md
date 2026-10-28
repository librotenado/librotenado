# 0005 Documents

Version 0.3, draft. Implemented in the reference library (`Document`, `Original`, `InvoiceData`, `en16931`,
`propose_booking`, `StatementData`, `camt`, event kind `document_recorded`).

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
| `statement` | the data of a bank statement (below); absent when not recorded |

Later versions add kinds and members. An implementation keeps a document of a kind it does not know, but does not
interpret it.

## The data of an invoice

The data of an invoice or credit note is an object of type `invoice_data` – whether entered by hand or translated
from an electronic invoice (below). It is the semantic model of EN 16931, the European model of an electronic invoice,
in full: every business term (BT) and group (BG) has a member, named in brackets. The document's number, date and kind
are BT-1 to BT-3; the process and specification identifiers (BT-23, BT-24) belong to the syntax, not to the data.
Members absent from the invoice are absent here, lists without entries too (part 0001).

| Member | Content |
|---|---|
| `type_code` | the type of invoice (BT-3, UNTDID 1001) where it is neither `380` invoice nor `381` credit note – e.g. `384` corrected invoice, `389` self-billed invoice (Gutschrift), `326` partial invoice, `875` to `877` partial and final construction invoices |
| `currency` | the currency of the invoice (BT-5) |
| `notes` | notes (BG-1, type `invoice_note`): `subject` a code of UNTDID 4451 (BT-21), `text` (BT-22) |
| `seller`, `buyer` | the parties (BG-4, BG-7, type `invoice_party`, below) |
| `payee` | who is paid, where not the seller (BG-10, type `invoice_party`: `name`, `identifiers`, `registration`) |
| `tax_representative` | the seller's tax representative (BG-11, type `invoice_party`: `name`, `vat_id`, `address`) |
| `buyer_reference` | the reference the buyer asked for, e.g. the Leitweg-ID of a German public authority (BT-10) |
| `references` | type `invoice_references`: `project` (BT-11), `contract` (BT-12), `order` the buyer's purchase order (BT-13), `sales_order` (BT-14), `receiving_advice` (BT-15), `despatch_advice` (BT-16), `tender` tender or lot (BT-17), `object` the object invoiced, an `identifier` (BT-18), `accounting` the buyer's accounting reference (BT-19) |
| `preceding` | invoices referred to, e.g. the one a credit note corrects (BG-3, type `preceding_invoice`): `number` (BT-25), `date` (BT-26) |
| `delivery_date` | when the supply was made (BT-72) |
| `period` | the period invoiced (BG-14, type `period`): `start` (BT-73), `end` (BT-74), days included |
| `delivery` | where the supply went (BG-13, type `delivery`): `name` (BT-70), `location` an `identifier` (BT-71), `address` (BG-15) |
| `tax_point_date` | when VAT becomes chargeable, where it differs from the invoice date (BT-7) |
| `tax_point_code` | the same as a code of UNTDID 2005 (BT-8): `3` invoice date, `35` delivery date, `432` date paid – CII writes these as `5`, `29`, `72` of UNTDID 2475 |
| `due_date` | when payment is due (BT-9) |
| `payment_terms` | the terms of payment as text (BT-20), e.g. the cash discounts XRechnung writes as lines `#SKONTO#TAGE=7#PROZENT=2.00#` |
| `payment` | how it is paid (BG-16, type `payment`): `means` a code of UNTDID 4461, e.g. `58` SEPA credit transfer, `59` SEPA direct debit (BT-81), `means_text` (BT-82), `remittance` the remittance information (BT-83), `transfers` the accounts to pay to (BG-17, type `credit_transfer`: `account` an IBAN for SEPA, `name`, `provider` a BIC, BT-84 to BT-86), `card` (BG-18, type `payment_card`: `number` the last digits only, `holder`, BT-87, BT-88), `direct_debit` (BG-19, type `direct_debit`: `mandate` the mandate reference, `creditor` the creditor identifier, `debited` the IBAN debited, BT-89 to BT-91) |
| `allowances`, `charges` | allowances and charges on the invoice (BG-20, BG-21, type `allowance_charge`): `amount` without VAT, `base` and `percent` where it is a percentage, `category` and `rate`, `reason` as text and `reason_code` (UNTDID 5189 for allowances, 7161 for charges) (BT-92 to BT-105) |
| `taxes` | the VAT per category and rate (BG-23, type `vat_breakdown`), at least one: `category`, `rate` in percent where the category has one, `base` and `tax` as money, `exemption` and `exemption_code` (VATEX) where stated (BT-116 to BT-121) |
| `attachments` | supporting documents (BG-24, type `supporting_document`): `reference` (BT-122), `description` (BT-123), `uri` (BT-124), `file` the attached file (BT-125) as an `original` – hash, size, media type and name; its bytes are stored like any original, outside the events |
| `lines` | the lines (BG-25, type `invoice_line`, below); absent when not recorded |
| `totals` | type `invoice_totals`: `net` (BT-109), `tax` (BT-110), `tax_accounting` the VAT in the currency VAT is accounted in where it differs – its currency is BT-6, its amount BT-111 –, `gross` (BT-112), `prepaid` paid in advance (BT-113) and `rounding` (BT-114) where stated, `due` (BT-115) = gross − prepaid + rounding |

The sum of the lines and the totals of allowances and charges (BT-106 to BT-108) are not recorded: they follow from
the lines and from the allowances and charges, and the net total is the lines less the allowances plus the charges.

A party on an invoice (type `invoice_party`): `name` (BT-27, BT-44, BT-59, BT-62), `trading_name` (BT-28, BT-45),
`identifiers` – each an `identifier` with `id` and the `scheme` (ISO/IEC 6523) where stated (BT-29, BT-46, BT-60) –,
`registration` its legal registration, an `identifier` (BT-30, BT-47, BT-61), `vat_id` (BT-31, BT-48, BT-63),
`tax_number` (BT-32), `legal_info` further legal information (BT-33), `address`, `contact` (type `invoice_contact`:
`name`, `phone`, `email`, BG-6, BG-9), `endpoint` (type `endpoint`: `scheme` and `address`, BT-34, BT-49), and `party`
– the organisation or person of the books it is (part 0008), once known. An address (type `postal_address`) has up to
three `lines`, the street first, `city`, `postcode`, `subdivision` and `country` (BG-5, BG-8, BG-12, BG-15).

A line (type `invoice_line`): `id` (BT-126), `note` (BT-127), `object` an `identifier` (BT-128), `order_line`
(BT-132), `accounting` (BT-133), `period` (BG-26), the item – `name` (BT-153), `description` (BT-154), `seller_item`
and `buyer_item` (BT-155, BT-156), `standard_item` an `identifier`, e.g. a GTIN (BT-157), `classifications` (type
`classification`: `code`, `list` of UNTDID 7143, `version`, BT-158), `origin` a country (BT-159), `attributes` (type
`item_attribute`: `name`, `value`, BG-32) –, `quantity` and `unit` (UN/ECE Rec. 20) (BT-129, BT-130), `price` per unit
as a decimal number (BT-146), `price_quantity` the quantity the price is for (BT-149, in the line's unit, BT-150),
`gross_price` and `price_discount` (BT-148, BT-147), `allowances` and `charges` (BG-27, BG-28, type
`allowance_charge` without category and rate), `net` as money after them (BT-131), `category`, `rate` (BT-151,
BT-152).

The VAT categories are the codes EN 16931 uses (UNCL 5305): `S` standard or reduced rate, `Z` zero rate, `E` exempt,
`AE` reverse charge, `K` intra-Community supply, `G` export, `O` not subject to VAT, `L` and `M` the indirect taxes of
the Canary Islands and of Ceuta and Melilla. Amounts are money of the invoice's currency (part 0001), as printed –
positive on a credit note, too: its `kind` says which way it books.

## Electronic invoices

An electronic invoice according to EN 16931 is translated into a document's data: its number (BT-1), date (BT-2),
kind – `credit_note` for type code 381 or a UBL CreditNote, otherwise `invoice` (BT-3) – and its `invoice` data; the
bytes of attached files (BT-125) are decoded and kept by their hash. Both syntaxes give the same data:

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
it checks what XRechnung requires beyond the model
([XRechnung specification](https://xeinkauf.de/xrechnung/versionen-und-bundles/), rules BR-DE-1 to BR-DE-31) and the
rules of EN 16931 a writer cannot repair, and names what is missing instead of writing an invalid invoice:

- a type of invoice XRechnung allows (BR-DE-17): `326`, `380`, `381`, `384`, `389`, `875`, `876`, `877`;
- the buyer reference (BT-10) and the payment instructions (BG-16) – for a transfer an account (BG-17), for a card
  its number (BG-18), for a direct debit the mandate, the creditor identifier and the IBAN debited (BT-89 to BT-91);
- the seller's address with city and postcode, its contact with name, telephone and e-mail (BG-6), an identifier,
  legal registration or VAT identifier (BR-CO-26), and – unless every amount is not subject to VAT – a VAT
  identifier or tax number of the seller or of its tax representative (BR-DE-16);
- the electronic addresses of both parties (BT-34, BT-49), the buyer's address with city and postcode, and city and
  postcode of a delivery address;
- lines with identifier, quantity, unit and price; a category and a reason for every allowance and charge on the
  invoice and a reason for those on lines; a reason or reason code for every amount exempt or not taxed (BT-120,
  BT-121); a file name and the bytes of every attached file;
- totals that agree: the net total is the lines less allowances plus charges (BR-CO-13), the amount due is gross less
  prepaid plus rounding (BR-CO-16).

Writing follows what XRechnung asks of the syntax: the VAT rate of a breakdown is written also where the category has
none, as 0 (BR-DE-14); structured lines of the payment terms end with a line break (BR-DE-18). CII D16B carries one
preceding invoice only; an invoice referring to several is written in UBL.

In the PDF the XML is the embedded file `xrechnung.xml`, related to the document as its `Alternative`, listed in the
catalogue's associated files (`AF`) and embedded files, and described in the XMP metadata with the Factur-X schema
(`fx:ConformanceLevel` XRECHNUNG). The PDF itself – fonts, colours, output intent of PDF/A-3 – is the implementation's.

The reference library reads every test invoice of the standard business cases and the CIUS technical cases of the
KoSIT test suite, writes it in both syntaxes and reads it back to the same data. The written files are accepted by the
KoSIT validator 1.6.3 with the XRechnung configuration 3.0.2 – schemas and business rules of EN 16931, XRechnung and
Peppol ([validator](https://github.com/itplr-kosit/validator),
[configuration](https://github.com/itplr-kosit/validator-configuration-xrechnung)) –, which the continuous
integration runs on them.

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
| `iso20022-camt.053` | a bank statement (ISO 20022 BankToCustomerStatement), read as below |

## Bank statements

A bank statement lists what was booked on one account over a period. Its data is an object of type `statement_data`:

| Member | Content |
|---|---|
| `account` | the account (type `statement_account`): `iban` (ISO 13616), `currency`, and `holder`, the holder as the bank names them, absent if not given |
| `from`, `to` | the period it covers, both days included; absent if the statement does not say |
| `opening`, `closing` | the booked balance before the first and after the last entry, money – positive when in the holder's favour; absent if not given |
| `entries` | the entries (type `statement_entry`) in the order of the statement; absent when there are none |

An entry is numbered by its position, from 1. It has:

| Member | Content |
|---|---|
| `booked` | the day it was booked |
| `value` | the value date; absent if not given |
| `amount` | money: positive when credited to the account – money in –, negative when debited |
| `reversal` | `true` where the bank marks the entry as the reversal of an earlier one; absent otherwise |
| `reference` | the bank's own reference for the entry; absent if none |
| `code` | the bank's transaction code as given, e.g. the ISO 20022 domain, family and sub-family `PMNT/RCDT/ESCT`; absent if none |
| `details` | the transactions it consists of (type `entry_detail`) – usually one, several for a batch; absent if the bank gives none |

A transaction is `counterparty` – the other party, the payer of money in, the payee of money out (type `counterparty`:
`name`, `iban`, `bic`, each absent if not given) –, `purpose` – the remittance information as text –,
`end_to_end` – the payer's end-to-end reference –, `mandate` – the reference of a SEPA direct debit mandate –, and,
where an entry bundles several, `amount`, each absent if not given.

The bank account of a set of books is an account opened with the attribute `iban` (part 0004).

### Reading camt.053

The ISO 20022 message BankToCustomerStatement ([message definitions](https://www.iso20022.org/iso-20022-message-definitions))
is read in the versions German banks deliver under the agreement on data transfer of the Deutsche Kreditwirtschaft,
annex 3 (Spezifikation der Datenformate; version 3.7 of 5 April 2023 found, whether a later one applies is OPEN):
`camt.053.001.02` and `camt.053.001.08`. Elements are matched by name, whatever their namespace:

- **One document per statement** (`Stmt`): its `Id` is the document's number, the day of `CreDtTm` its date.
- **The account:** `Acct/Id/IBAN`, `Acct/Ccy`, `Acct/Ownr/Nm`.
- **Balances:** the opening balance is the one of type `PRCD` (closing balance of the previous statement) or `OPBD`,
  the closing balance the one of type `CLBD`; their sign by `CdtDbtInd` (`CRDT` positive, `DBIT` negative).
- **Entries:** only booked ones – status `BOOK`, in version 001.08 as `Sts/Cd`. `Amt`, its sign by `CdtDbtInd`,
  `RvslInd`, `BookgDt` and `ValDt` (`Dt`, or the day of `DtTm`), `AcctSvcrRef`, `BkTxCd/Domn` as
  domain/family/sub-family.
- **Transactions:** each `NtryDtls/TxDtls`: `Refs/EndToEndId` (left out when `NOTPROVIDED`), `Refs/MndtId`, the
  counterparty from `RltdPties` – for money in the debtor, for money out the creditor; their `Nm` directly or under
  `Pty`, `Id/IBAN` of their account –, its BIC from `RltdAgts`, `RmtInf/Ustrd` joined by spaces, and `AmtDtls/TxAmt/Amt`
  where an entry has more than one.

## Events

- `document_recorded` with `document` – a document was recorded with its data and original (part 0002).
- A booking MAY list the documents it is based on in `documents`, as an array of document identities; the member is
  absent when there are none (part 0001). A document may back several bookings (an invoice and its payment), a
  booking may rest on several documents.
- An implementation MUST report, and MUST NOT silently resolve, a booking based on a document that has not been
  recorded (it may not have arrived yet), and a document recorded twice under the same identity.
- A booking of what a bank statement shows names the entries it books in `entries`: an array of entry references
  (type `entry_reference`) – `document`, a recorded bank statement, and `entry`, the position of the entry, from 1 –,
  absent when there are none (part 0001). An implementation MUST report, and MUST NOT silently resolve:
  - **Unknown entry** – a reference to a document that is not a recorded bank statement, or has no such entry.
  - **Entry booked twice** – two bookings that stand, neither reversed nor a reversal, naming the same entry.
  - **Entry shown twice** – an entry that two recorded statements of the same account show: the same bank
    reference, or without one, the same day, amount, counterparty and purpose – statements that overlap. It is
    booked once.
  - **Balance mismatch** – the closing balance of a statement differs from the balance, on its last day, of the one
    account of the books with its IBAN; no account, or more than one, with it is not checked.

## Open points

- The extension of XRechnung (sub-invoice lines, third-party payments), which the model does not carry yet.
- Code lists (ISO/IEC 6523, UNTDID 7143, VATEX …) are not checked before writing; the KoSIT validator checks them.
- Reverse charge and intra-Community acquisitions at the reduced rate: the proposal assumes 19 %; rates other than
  19 % and 7 % get no tax code.
- Profiles of ZUGFeRD other than XRECHNUNG (EN 16931, EXTENDED); the file name `xrechnung.xml` of the profile XRECHNUNG, to be verified against the
  ZUGFeRD specification.
- Correcting the data of a recorded document by a later event.
- Statements in other formats – MT940, the CSV files of banks –, and the intraday reports camt.052 and camt.054.
- Whether the names and definitions of the business terms of EN 16931 – a standard of CEN, © CEN and its members –
  may be quoted in a specification under CC BY; this part names their identifiers only.
- Whether the names of formats should be the identifiers the standards define (e.g. the specification identifier of
  EN 16931 and its CIUS such as XRechnung) instead of names of our own.
- Which rules require keeping the structured part of an e-invoice as received (see the whitepaper).
