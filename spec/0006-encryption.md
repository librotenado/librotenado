# 0006 Encryption

Version 0.3, draft – not yet implemented. Everything here is a proposal for discussion.

Books are end-to-end encrypted between the people and devices that share them. Whoever relays events – a relay run
by anyone – stores and forwards them without being able to read them. A backend that processes books in plain text
(recognition, scheduled bank retrieval, filings) is a member like any device, and implementations make that visible
to the people whose books it reads.

The design takes its architecture from [any-sync](https://sync.any.org/access-control/), the protocol of Anytype: a
strictly ordered list of members and keys beside unordered data, one symmetric key for reading, wrapped for each
member, and a chain of keys that keeps the whole history readable. librotenado specifies and implements it on its
own; no code is taken from any-sync.

## Two strands

A set of books has two strands:

- **The events** (part 0002): a set without an order, merged by union, written offline. Encryption does not change
  this.
- **The members**: a strictly ordered list of records, each pointing to its predecessor by hash. A record adds or
  removes a member, changes a member's rights, or changes the key. Only this list needs an order.

The first record is written by the founder of the books. While two devices or people share the books, they order
the list between themselves. With more, an **orderer** decides the order of records it is given and rejects a record
that does not follow the current head – it sees records, not books. A relay MAY be the orderer. Which records an
orderer accepts – only those signed by a member with the right to make them – is checked by every member again.

## Keys

- Every member – a person's device or a backend – has a key pair for signing and one for receiving keys. Keys are
  made on the device and never leave it unencrypted.
- **The books key** is a symmetric key. Events and originals (part 0005) are encrypted with it. It is wrapped for
  every member with the member's public key, and the wrapped keys stand in the member list.
- **When a member is removed**, a new books key is made and wrapped for the remaining members only. Everything
  written afterwards is encrypted with the new key.
- **The new key encrypts the previous one**, forming a chain: whoever holds the current key can open every earlier
  one and read the whole history. A new member – an accountant, an auditor – can therefore read the books from the
  beginning, as accounting needs for ten years and longer.

## What a relay sees

An event travels in an **envelope**: the books it belongs to (their identity, part 0002), an identifier to route and deduplicate it by, the key
it is encrypted with (by its position in the key chain), and the ciphertext of its canonical encoding (part 0003).
The event's hash is computed over the plain text and is visible only to members. Originals travel the same way.

A relay therefore sees which books exist, who writes when, and how much – not what. Whether sizes are padded to hide
more is OPEN.

## Limits

These limits are part of the design and implementations SHOULD state them:

- **Removing a member protects the future, not the past.** What a member has read, it may have kept.
- **A compromised device can read until the next change of key.** Unlike Messaging Layer Security
  ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420)), the design does not heal itself after a compromise; it
  trades this for an order only on members and for a history that stays readable.
- **Losing every key of the books loses the books.** There is no master key. Recovery – a member kept in reserve,
  a printed recovery key – is OPEN.

## Why not MLS

MLS was the first candidate. It fits chats with many members, keeps no history for new members, and its changes of
membership have to be applied in one agreed order (OPEN: to be verified in RFC 9420 and the MLS architecture
document). Books have few members, need their whole history for a decade, and synchronise without an order. Whether
MLS could serve as the transport of the member list later is OPEN.

## Open points

- The algorithms: signing, wrapping keys, encrypting events and originals.
- The member list byte for byte: records, rights (read, write, manage members), invitations and requests to join.
- The envelope byte for byte, and the identifier a relay routes by.
- Access for a limited time – an auditor for one period – and whether it can be narrower than the whole history.
- Recovery when keys are lost; archives for retention periods of ten years and more.
