# 0002 Events and synchronisation

Version 0.1, draft. Implemented in the reference library (`crates/librotenado`).

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

Kinds in version 0.1:

| `kind` | Members | What happened |
|---|---|---|
| `organisation_described` | `organisation` | an organisation was described, or its description changed (part 0008) |
| `person_described` | `person` | a person was described, or their description changed (part 0008) |
| `owner_added` | `owner` | the books belong to this organisation or person, too (part 0008) |
| `owner_removed` | `owner` | the books no longer belong to them (part 0008) |
| `chart_selected` | `chart`, as `name/version` | the books use this harmonised chart of accounts (part 0004) |
| `account_opened` | `account` | an account of the books' own was opened, refining a known one (below) |
| `booking_recorded` | `booking` | a booking (part 0001) was recorded |
| `booking_reversed` | `reverses`, `booking` | the booking recorded by the event `reverses` was cancelled by `booking`, with the opposite amounts |
| `document_recorded` | `document` | a document was recorded with its data and original (part 0005) |

Later versions add kinds and members. An implementation keeps, chains and passes on events of a kind or with members
it does not know, unchanged (part 0003), but does not interpret what it does not know.

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

Conflicts in the content of the books – such as reversing a booking twice or reversing one that does not exist – go
to a review list in the same way; resolving them is a new event, never a change to an old one.

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
