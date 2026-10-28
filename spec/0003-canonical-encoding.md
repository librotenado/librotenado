# 0003 Canonical encoding

Version 0.2, draft. Implemented in the reference library (`canonicalize`, `to_canonical_vec`, `EventSet::receive`).

Every implementation must compute the same hash for the same event. The protocol therefore defines one byte-exact
encoding: the **JSON Canonicalization Scheme** ([RFC 8785](https://www.rfc-editor.org/rfc/rfc8785)), restricted to
what librotenado uses.

## Rules

- The encoding is UTF-8 without a byte order mark and without whitespace between tokens.
- Object members are sorted by their names, compared as sequences of UTF-16 code units (RFC 8785, section 3.2.3).
- Strings are written as RFC 8785 requires: `"` and `\` escaped; the control characters U+0008, U+0009, U+000A,
  U+000C and U+000D as `\b`, `\t`, `\n`, `\f`, `\r`; other control characters below U+0020 as `\u00xx` in lower
  case; every other character – including `/` and all non-ASCII characters – as is.
- Strings are not normalised: `é` written as one character (NFC) and as `e` with a combining accent (NFD) are
  different strings with different hashes, as in RFC 8785 – an event records what was entered. Software that
  writes events SHOULD write text in Unicode Normalization Form C.
- An object MUST NOT have two members of the same name; a reader MUST reject such JSON rather than keep one of them
  (RFC 8785 requires I-JSON, [RFC 7493](https://www.rfc-editor.org/rfc/rfc7493)).
- Numbers MUST be integers between −(2^53 − 1) and 2^53 − 1 and are written in their shortest decimal form.
  librotenado has no floating-point numbers: money and decimal numbers are strings, each in its one form
  (part 0001). An implementation MUST reject a number outside this range when reading an event.
- `null`, `true` and `false` are written as such. A member whose value is absent is left out; a member whose value
  is `null` is written (for example `prev` of the first event of a device).

## Objects and types

Every object of the protocol has a **type**, defined in its part:

| Type | Part | | Type | Part |
|---|---|---|---|---|
| `money` | 0001 | | `event` | 0002 |
| `currency` | 0001 | | `timestamp` | 0002 |
| `posting` | 0001 | | `account` | 0004 |
| `booking` | 0001 | | `chart` | 0004 |
| `document` | 0005 | | `chart_translation` | 0004 |
| `original` | 0005 | | `organisation` | 0008 |
| `person` | 0008 | | `registration` | 0008 |
| `tax_id` | 0008 | | `role` | 0008 |
| `filing` | 0007 | | `shares` | 0008 |
| `report_content` | 0007 | | `submission` | 0007 |
| `submission_status` | 0007 | | | |
| `currency_table` | 0001 | | `names` | 0001 |
| `tax_code` | 0001 | | `tax_table` | 0001 |
| `report_mapping` | 0009 | | `invoice_data` | 0005 |
| `invoice_party` | 0005 | | `vat_breakdown` | 0005 |
| `invoice_line` | 0005 | | `invoice_totals` | 0005 |
| `invoice_contact` | 0005 | | `endpoint` | 0005 |
| `direct_debit` | 0005 | | `identifier` | 0005 |
| `postal_address` | 0005 | | `invoice_references` | 0005 |
| `invoice_note` | 0005 | | `preceding_invoice` | 0005 |
| `period` | 0005 | | `delivery` | 0005 |
| `payment` | 0005 | | `credit_transfer` | 0005 |
| `payment_card` | 0005 | | `allowance_charge` | 0005 |
| `supporting_document` | 0005 | | `classification` | 0005 |
| `item_attribute` | 0005 | | `exchange` | 0001 |

The type is written as the member `type` exactly where the context does not determine it, and nowhere else:

- on a **root object** – one that stands on its own: stored, transmitted, exported or published as a file. An event
  is always a root object; an account exported on its own is the root object `{"type": "account", …}`;
- on each object in an array that admits objects of more than one type.

Inside a root object the type of every nested object follows from where it stands – the `amount` of a posting is
`money` – so nested objects MUST NOT carry `type`, and root objects MUST. A reader MUST reject an object that
breaks this, as it would otherwise have two forms.

Where a type has subtypes, the member `kind` names them: the kind of an event (`booking_recorded`, …), the kind of an
account (`asset`, …), the kind of a document (`invoice`, …), of a role, of a filing. `type` says what an object is,
`kind` which variety of it.

## One form of "none"

Where a member is optional, absence is its one form of "none": it is left out, never written as `null` or as an
empty array, unless its part says otherwise (`prev` is written as `null`). A reader MUST reject the other forms, so
that the same content never arrives with two hashes.

## Use

- The identity of an event (part 0002) is the SHA-256 hash of the canonical encoding **of the event as received** –
  all of its members, including those the reader does not know, which a later version may have added.
- An implementation MUST keep a received event in a form from which it reproduces exactly that encoding, and MUST
  pass that on – never an encoding of its own reading of the event, which would drop what it does not know and
  change the hash. It MAY store and transmit events in any formatting.
- The reference library keeps every event's canonical bytes next to its reading (`EventSet::receive`,
  `EventSet::canonical`).

## Example

The member order is irrelevant on input; the canonical form sorts it:

```json
{"seq":1,"time":{"wall_ms":1790812800000,"device":"laptop","counter":0},"prev":null,"payload":{…}}
```

becomes

```json
{"payload":{…},"prev":null,"seq":1,"time":{"counter":0,"device":"laptop","wall_ms":1790812800000}}
```
