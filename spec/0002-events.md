# 0002 Events and synchronisation

Version 0.3, draft. Implemented in the reference library (`crates/librotenado`).

## Events

A set of books consists of events. An event is immutable; its identity is the SHA-256 hash of its canonical encoding
(part 0003), written `sha256:` followed by 64 lower-case hexadecimal digits, like every hash in the protocol. An
event is always a root object of type `event`:

```json
{"type": "event", "kind": "booking_recorded", "time": {…}, "seq": 1, "prev": null, "booking": {…}}
```

Every event has:

| Member | Content |
|---|---|
| `type` | `event` |
| `kind` | what happened (below); the members that come with it stand next to the others |
| `time` | a hybrid logical clock timestamp (type `timestamp`): `wall_ms`, `counter`, `device` |
| `seq` | the event's position in its device's chain, starting at 1 |
| `prev` | the hash of the device's previous event; `null` for position 1, never left out |

Kinds:

| `kind` | Members | What happened |
|---|---|---|
| `books_founded` | `books`, a UUID | the set of books was founded under this identity (below) |
| `organisation_described` | `organisation` | an organisation was described, or its description changed (part 0008) |
| `person_described` | `person` | a person was described, or their description changed (part 0008) |
| `owner_added` | `owner` | the books belong to this organisation or person, too (part 0008) |
| `owner_removed` | `owner` | the books no longer belong to them (part 0008) |
| `owner_configured` | `configuration` | how an owner keeps its books from a date on: currency, financial year, accounting, VAT (part 0008) |
| `chart_selected` | `chart`, as `name/version` | the books use this harmonised chart of accounts (part 0004) |
| `account_opened` | `account` | an account of the books' own was opened, refining a known one (below) |
| `booking_recorded` | `booking` | a booking (part 0001) was recorded |
| `booking_reversed` | `reverses`, `booking` | the booking recorded by the event `reverses` was cancelled by `booking`, with the opposite amounts |
| `document_recorded` | `document` | a document was recorded with its data and original (part 0005) |
| `period_closed` | `closing` | an owner's books were closed up to a date (below) |

Later versions add kinds and members. An implementation keeps, chains and passes on events of a kind or with members
it does not know, unchanged (part 0003), but does not interpret what it does not know.

## The identity of the books

A set of books has an identity: a UUID (part 0001), given by its first event, `books_founded`. Whoever starts a set
of books writes it before anything else; the identity never changes. It names the books where they leave their own
events – in the envelope that carries an encrypted event (part 0006), in a transaction across books (below), in an
export. An implementation MUST report, and MUST NOT silently resolve:

- **Not founded** – events without a `books_founded` among them (it may not have arrived yet).
- **Founded twice** – more than one `books_founded`, even with the same identity: two sets of books were merged, or
  one was founded twice. The first in the agreed order (below) gives the identity.

## Devices and clocks

Every device that writes events has a name unique within the set of books: 1 to 64 characters of `a-z`, `0-9` and
`-`. Its timestamps come from a hybrid logical clock: milliseconds since 1970-01-01 UTC, a counter for events within
the same millisecond, and the device name. Timestamps are ordered by wall time, counter and device. A device MUST
make every new timestamp later than every timestamp it has made or seen.

## Merging

Two copies of the same books merge by set union of their events. Merging MUST give the same result in any order and
any number of times. Events are processed in the order of their timestamps, then their hashes – an order every
device agrees on.

## The state of the books

The **state** of a set of books is the SHA-256 hash of the canonical encoding (part 0003) of the array of all its
event hashes in the agreed order, written `sha256:…`. Two copies with the same events have the same state, whatever
order the events arrived in. Whatever is derived from the books – a filing (part 0007), a report – records the state
it was derived from.

## Chains and problems

The events of one device form a chain: positions 1, 2, 3, … without gaps, each event pointing to its predecessor
with `prev`. An implementation MUST report, and MUST NOT silently resolve:

- **Fork** – two different events of one device at the same position.
- **Gap** – missing positions (events that have not arrived yet).
- **Broken link** – an event whose `prev` is not the device's previous event.

Conflicts in the content of the books go to a review list in the same way; resolving them is a new event, never a
change to an old one.

## Bookings and reversals

A booking is never changed; it is cancelled by a reversal (part 0001). An implementation MUST report, and MUST NOT
silently resolve:

- **Invalid booking** – a booking or reversal that breaks the rules of part 0001: fewer than two postings, a posting
  of zero, postings that do not balance for an owner in a currency, converted or not.
- **Unknown reversal** – a reversal of an event that is not a recorded booking: it may not have arrived yet, or it is
  a reversal itself.
- **Double reversal** – a further reversal of a booking that is reversed already; the first in the agreed order
  cancels it.
- **Reversal mismatch** – a reversal whose postings are not those of the booking with the opposite amounts: the same
  owners, accounts, tax codes and exchange rates in the same order, the amounts and converted amounts negated. Notes,
  the date and the text may differ.

## Accounts of the books

The books select the harmonised chart they use with `chart_selected`; more than one selected chart is for review.
Accounts of their own – each bank account, each customer and supplier – are opened with `account_opened`: an account
(part 0004) whose `parent` is required and is an account of the chart or another opened account, of the same
`kind`. An implementation MUST report, and MUST NOT silently resolve:

- **Unknown account** – a posting on an account that is neither in the chart nor opened (the event that opens it
  may not have arrived yet).
- **Unknown parent** – an opened account without a parent, or whose parent is neither in the chart nor opened.
- **Kind mismatch** – an opened account of another kind than its parent.
- **Duplicate** – an account opened twice, or opened with the identity of an account of the chart.

Parts 0005 and 0008 add problems of documents and of parties in the same way.

## Transactions across books

One business transaction can concern books that are kept apart – a business and its owner's private books, shared
with different people and encrypted with different keys (part 0006). A booking cannot reach into another set of
books, so each set records its own booking, balanced for its own owners (part 0001), and the bookings name the same
**transaction**: `transaction` on a booking (type `transaction`) is `id`, a UUID shared by all bookings of the
transaction, and `books`, the identities of the other sets of books that record it – an array of at least one,
never empty. An internet bill paid from the business account, one fifth of it private:

| Books | Booking |
|---|---|
| the business | internet 48.00 and private withdrawal 12.00, against the bank account 60.00 |
| the owner's private books | internet 12.00, against the withdrawal from the business 12.00 |

Both name the same `transaction` and rest on documents with the same original (part 0005). A booking within one set
of books that concerns several of its owners needs no transaction: it is one booking (part 0001).

Whoever holds only one set of books sees one booking and the identities of the others, nothing more. An
implementation that holds several MUST report, and MUST NOT silently resolve, across the sets it holds:

- **Duplicate** – two bookings with the same transaction in one set of books.
- **Missing** – a booking names a set of books that is held, but has no booking of the transaction.
- **Books mismatch** – the bookings of one transaction do not name each other's sets of books.
- **Date mismatch** – the bookings of one transaction have different dates.
- **Reversal mismatch** – the booking of a transaction is reversed in one set of books and not in another.

A reversal (`booking_reversed`) is a booking of a transaction of its own: whoever reverses one side of a transaction
reverses the others, and the reversals name a new transaction, so that they are checked as a pair as well.

## Closing a period

Every event is immutable from the moment it is written (part 0010), but new bookings can still be dated in the past.
`period_closed` closes an owner's books up to a date: the period's figures are final, reports and filings over it
(parts 0007, 0009) will not change. The closing (type `closing`) is:

| Member | Content |
|---|---|
| `owner` | the organisation or person whose books are closed (part 0008) |
| `to` | the last day closed; everything up to it is closed |
| `state` | the state of the books (above) the closing was decided on – the events before it |
| `software` | the implementation and the modules the period's figures were derived with, name → version (`{"spezado-law-de": "0.1.0"}`); absent when not given, never empty |

Since a device makes every new timestamp later than every one it has seen (above), all events the closing was
decided on stand before it in the agreed order. An implementation MUST report, and MUST NOT silently resolve:

- **State mismatch** – the events before a closing do not have the state it names: events arrived that whoever
  closed had not seen, written offline on another device.
- **Late booking** – a booking after a closing, in the books of its owner, dated on or before `to`.
- **Late configuration** – a configuration (part 0008) after a closing, for its owner, from a date on or before `to`.

A closed period is corrected in the open one, with bookings dated after `to`. Reopening a period is OPEN.
