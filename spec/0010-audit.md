# 0010 Audit and retention

Version 0.3, draft. Implemented in the reference library (`gobd::export`).

Books must be kept so that an auditor can follow every business transaction from its document to the statements and
back, for years. This part says what the protocol guarantees for that, and how books are handed over to an auditor
– first for Germany.

## What the protocol guarantees

- **Nothing is changed or deleted.** A set of books is a growing set of immutable events (part 0002); a correction is
  a reversal and a new booking.
- **Every entry is recorded with when and where.** Each event carries the time it was written and the device that
  wrote it (part 0002); each device's events form a hash chain, so a gap or an altered event shows.
- **The original stays the receipt.** A document is a record of its own; its original is kept byte for byte and
  identified by its hash (part 0005).
- **Conflicts and gaps are visible.** Forks, gaps, broken links, unknown accounts, documents and parties go to a
  review list and are resolved by new events (parts 0002, 0005, 0008).
- **Derived figures can be derived again.** Reports record the state of the books they came from (parts 0002, 0007).
- **Closed periods stay closed.** A closing names the state of the books it was decided on and the software the
  figures were derived with; a booking dated in a closed period, written later, is reported (part 0002).

How long books are kept is a rule of the law that applies; the protocol never deletes. Erasing books whose period has
ended is an act of the implementation, outside the events (OPEN: how it is recorded).

## Germany

### Retention

§ 147 Abs. 3 AO: books, inventories, annual financial statements and the documents needed to understand them (Abs.
1 Nr. 1 and 4a) are kept **ten years**, booking documents (Nr. 4) **eight years**, other documents relevant for tax
(e.g. business letters) **six years**. The eight years apply to all booking documents whose period had not ended by
31 December 2024, except for credit institutions, insurers and investment firms (Art. 97 § 19a EGAO)
([AO](https://www.gesetze-im-internet.de/ao_1977/__147.html)).

### The GoBD

The GoBD – BMF letter of 28 November 2019, IV A 4 - S 0316/19/10003 :001, BStBl I S. 1269, amended on 11 March 2024
(IV D 2 - S 0316/21/10001 :002, BStBl I S. 374) and on 14 July 2025 (IV D 2 - S 0316/00128/005/088) – set out how
books are kept electronically. Their requirements and how the protocol meets them, for an implementation's
procedural documentation (Verfahrensdokumentation, Rz. 150 to 155):

| Requirement | GoBD | In the protocol |
|---|---|---|
| Completeness | Rz. 36–43 | every event is kept; gaps in a device's chain are reported (part 0002) |
| Timeliness | Rz. 45–52, 87 | each event carries the time it was written, apart from the booking date (part 0002) |
| Order | Rz. 53–57 | events in an order every device agrees on; the journal by booking date (part 0002) |
| Immutability and logging | Rz. 58–60, 107–112 | events are immutable from the moment they are written – stricter than a later fixing ("Festschreibung"); a correction is a reversal (parts 0001, 0002) |
| Documents | Rz. 61–81 | documents as records of their own, linked from bookings; the original kept and identified by its hash (part 0005) |
| Journal | Rz. 90–94 | per posting: booking and document data, amount, currency, for foreign currency the exchange rate and the amount in the functional currency, tax code and rate, account with its kind and type, debit and credit – exported below |
| Retention of e-invoices | Rz. 118–131 (as amended 2025) | the structured part (the XML) is the original; a PDF only where it carries more (part 0005) |
| Data access | Rz. 158–178 | inspection (Z1) and evaluation (Z2) by the implementation; data transfer (Z3) as below |

### Data transfer (Z3)

For the data transfer (§ 147 Abs. 6 Satz 1 Nr. 3 AO, GoBD Rz. 167) an implementation exports the books of one owner
for a period as CSV files described by an `index.xml` after the **Beschreibungsstandard für die
Datenträgerüberlassung**, referring to its DTD `gdpdu-01-09-2004.dtd` (published by the Federal Central Tax Office,
e.g. with the [DSFinV-K](https://www.bzst.de/SharedDocs/Downloads/DE/Aussenpruefung/dsfinv_k_v_2_4.zip?__blob=publicationFile&v=19)).
All files are UTF-8, columns separated by `;`, text in `"`, records by CR LF, a header row first, decimal comma, dates
as `DD.MM.YYYY`.

| File | Table | One row per |
|---|---|---|
| `konten.csv` | Konten | account posted on, and the accounts it refines: identity, name, kind (Aktiva, Passiva, Kapital, Aufwand, Ertrag, sonstige), type (Bilanz, GuV, sonstige), parent, chart or opened |
| `buchungen.csv` | Buchungen | posting: booking (event hash) and position, booking date, time and device of entry, text, account with name, kind and type, debit, credit, currency, exchange rate, debit and credit in the functional currency and that currency, tax code and rate, documents with numbers and dates, the booking a reversal reverses, note |
| `belege.csv` | Belege | document of the exported bookings: identity, kind, number, date, day received, its original in the export (`belege/<hash>`) with hash, media type and file name, seller and buyer with VAT identification and tax numbers, net, VAT, gross, currency |

The originals are added to the export by the implementation from its store. The reference library's `index.xml`
validates against the DTD (the DTD's own content model of `Media` is not deterministic, which validators note).

## Open points

- The Verordnung under § 147b AO (Buchführungsdatenschnittstellenverordnung, DSFinV-BV), a draft as of October 2026:
  a standard export on XBRL-CSV, in force from the end of the third year after its promulgation. The export above
  follows it once it is final.
- Fields of GoBD Rz. 94 the protocol does not record yet: authorisation by a person rather than a device, the booking
  period of the VAT return.
- Account numbers of a national chart alongside the identities in the export.
- The terms of use of the DTD and of the description of the Beschreibungsstandard, which are not published under a
  licence; the library writes the reference to the DTD, not the DTD.
- How erasing books after their retention period is recorded.
