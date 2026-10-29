# 0006 Encryption

Version 0.5, draft. Implemented in the reference library (`MemberKeys`, `BooksKey`, `MemberRecord`, `MemberList`,
`Envelope`, `RecoveryCode`, `Invitation`, `request_signature`).

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
  this. On a member's device the events are kept in plain text; they are encrypted for the way between devices.
- **The members**: a strictly ordered list of records, each pointing to its predecessor by hash. A record adds or
  removes members and may change the key. Only this list needs an order.

The first record is written by the founder of the books. A relay orders the list: it accepts a record only if it
follows the current head and rejects the others, which their writers apply again on the new head. Every member
checks every record again (below); a relay sees records, not books.

## Algorithms

| Purpose | Algorithm |
|---|---|
| signing | Ed25519 ([RFC 8032](https://www.rfc-editor.org/rfc/rfc8032)) |
| wrapping a key for a member | HPKE ([RFC 9180](https://www.rfc-editor.org/rfc/rfc9180)) in mode Base with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and ChaCha20Poly1305 – suite `0x0020`, `0x0001`, `0x0003` |
| encrypting events, originals and earlier keys | XChaCha20-Poly1305 with a random 24-byte nonce ([draft-irtf-cfrg-xchacha-03](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha-03)) |
| deriving keys from a seed | HKDF-SHA256 ([RFC 5869](https://www.rfc-editor.org/rfc/rfc5869)) |
| proving an invitation | HMAC-SHA256 ([RFC 2104](https://www.rfc-editor.org/rfc/rfc2104)) |

Bytes are written as text in base64url without padding ([RFC 4648](https://www.rfc-editor.org/rfc/rfc4648), section
5). Hashes are SHA-256, written `sha256:` and 64 lower-case hexadecimal digits, like every hash of the protocol.
Labels below are UTF-8 strings; `{books}` stands for the identity of the books (part 0002), `{index}` for the
position of a key in decimal digits, `{member}` for a member's identity.

## Members and their keys

A member – a device of a person, a backend, a recovery code – has one **seed** of 32 random bytes, made where the
member is and never sent anywhere. From it:

- the **signing key**: the Ed25519 private key whose 32 bytes are HKDF-SHA256 of the seed, with no salt and the info
  `librotenado/0006 signing`;
- the **receiving key**: the X25519 key pair that DeriveKeyPair of the HPKE KEM makes (RFC 9180, section 7.1.3) from
  the 32 bytes of HKDF-SHA256 of the seed, with no salt and the info `librotenado/0006 receiving`.

A member's **identity** is its public signing key, written `ed25519:` and 64 lower-case hexadecimal digits; its public
receiving key is written `x25519:` and 64 such digits. A member (type `member`) is:

| Member | Content |
|---|---|
| `id` | its identity |
| `receiving` | its public receiving key |
| `role` | `device` – writes events under its device name –, `backend` – the same, run for others –, or `recovery` – a recovery code (below), which writes no events |
| `device` | the device name it writes events under (part 0002), for `device` and `backend`; absent for `recovery` |
| `name` | what people call it, e.g. `Laptop`; absent if not given |
| `rights` | `manage` – read, write, add and remove members –, `write` – read and write events – or `read` |

Device names are unique among the current members, as part 0002 requires within a set of books.

## The books key

The **books key** is 32 random bytes. The keys of a set of books form a chain: key 0 is made by the founder, every
further key by whoever changes it, and each key from 1 on carries its predecessor encrypted with itself – so whoever
holds the current key can open every earlier one and read the whole history.

- **Wrapping** a key for a member: HPKE SealBase to the member's receiving key, with the info
  `librotenado/0006 key {books} {index} {member}` and no additional data. A **grant** (type `grant`) is `member`,
  `key` (the index), `enc` (the encapsulated key, 32 bytes) and `ciphertext` (48 bytes).
- **The previous key** in a key change is XChaCha20-Poly1305 of the 32 bytes of the key before, under the new key,
  with the additional data `librotenado/0006 chain {books} {index}` (the new index), written as the nonce followed by
  the ciphertext (72 bytes).

## The member list

A record (type `member_record`) is:

| Member | Content |
|---|---|
| `books` | the identity of the books |
| `seq` | its position, from 0 |
| `prev` | the hash of the record before it; absent at 0 |
| `author` | the identity of the member who wrote it |
| `add` | members added; absent when none |
| `remove` | identities of members removed; absent when none |
| `key` | a new key: `index` and `previous` (absent for key 0) – type `key_change`; absent when the key stays |
| `grants` | grants of the current key, after this record, for members who did not have it |
| `signature` | Ed25519 of the author over the canonical encoding (part 0003) of the record without `signature` |

The hash of a record is SHA-256 of its canonical encoding with its signature. A record is valid if:

1. its signature is the author's;
2. at 0: `prev` is absent, the author is among the members added, `key` has index 0 and nothing is removed;
3. after 0: `prev` is the hash of the record before, and the author is a current member with the right `manage`;
4. a member added is not a current member, and its device name is not one a current member has; a member removed is a
   current member;
5. whoever removes a member changes the key: `key` has the index after the current one;
6. after the record every member has a grant of the current key – from this record or, for the key unchanged, from
   an earlier one – and at least one member has the right `manage`.

A member MUST check every record before it uses a key or trusts a member, and MUST NOT apply a record that is not
valid. Records that are valid but did not reach the relay in time are written again on the new head.

## Envelopes

An event (part 0002) or an original (part 0005) travels in an **envelope** (type `envelope`):

| Member | Content |
|---|---|
| `books` | the identity of the books |
| `key` | the index of the key it is encrypted with |
| `author` | the identity of the member who sent it |
| `content` | `event` or `original` |
| `nonce` | 24 bytes |
| `ciphertext` | XChaCha20-Poly1305 of the content – the canonical encoding of the event as a root object, or the bytes of the original – with the additional data below |
| `id` | `sha256:` over the nonce followed by the ciphertext: what a relay routes and deduplicates by |
| `signature` | Ed25519 of the author over the canonical encoding of the envelope without `signature` |

The additional data is the canonical encoding of the object of `books`, `key`, `author` and `content`. A relay
accepts an envelope only from a current member with the right `write` or `manage` and with a key index no later than
the current one. A member who opens an envelope checks the signature, and that an event comes from the device name of
its author; the hash of an event (part 0002) is computed over the plain text and is visible only to members.

## Relays

A relay offers the books it holds over HTTP; it is reached over TLS. Every request but those marked open carries
`Authorization: librotenado {member} {time} {signature}` – the member's identity, the time in seconds since
1970-01-01 UTC and Ed25519 over `{method} {path} {time}` (the path with its query), written as base64url. A relay
accepts a time within 300 seconds of its own and requests of current members only.

| Request | |
|---|---|
| `GET /v1/books/{books}/members?after={seq}` | the records after `seq`, in order – open, so that a new member can read the list it was added in |
| `POST /v1/books/{books}/members` | a record: `201` when it is the next valid record, `409` when it does not follow the head, `403` when it is not valid. The record 0 of books the relay does not hold is open |
| `POST /v1/books/{books}/envelopes` | an envelope: `201`, or `200` if the relay holds it already |
| `GET /v1/books/{books}/envelopes?after={n}&limit={m}` | the envelopes the relay received after its own position `n`, as `{"envelopes": [{"position": …, "envelope": …}], "more": …}` |
| `POST /v1/books/{books}/requests` | a request to join (below) – open |
| `GET /v1/books/{books}/requests` | the requests to join, for members with the right `manage` |

## Adding a device

An **invitation** is the address of the relay, the identity of the books and a secret of 16 random bytes, made by a
member with the right `manage` and carried to the new device by the person – typed, scanned or pasted. The new device
makes its seed and sends a request to join (type `join_request`): the member it would be (`member`) and `proof`,
HMAC-SHA256 with the secret as key over the canonical encoding of that member. The inviting member checks the proof,
adds the member and grants it the current key; the new device reads the member list, unwraps the key and the chain,
and receives the envelopes. A relay deletes a request when the member was added, and after 24 hours.

## Recovery

A **recovery code** is the seed of a member with the role `recovery` and the right `manage`, added when the books are
founded – or later – and kept by the person on paper. It is written in Crockford's base32
([Crockford](https://www.crockford.com/base32.html)) in groups of four characters: 32 bytes and a check character
(the sum of the bytes modulo 32), 53 characters in 14 groups. Read back, it is checked and reads in upper or lower
case, `O` as `0` and `I` and `L` as `1`.

When every device is lost, a new device takes the recovery code, the address of the relay and the identity of the
books: as the recovery member it reads the member list, unwraps the key, and adds the new device as a member. Without
the recovery code and without a device the books are lost – implementations say so when they make the code.

## Limits

These limits are part of the design and implementations SHOULD state them:

- **Removing a member protects the future, not the past.** What a member has read, it may have kept.
- **A compromised device can read until the next change of key.** Unlike Messaging Layer Security
  ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420)), the design does not heal itself after a compromise; it
  trades this for an order only on members and for a history that stays readable.
- **Losing every key of the books loses the books.** There is no master key but the recovery code.
- **A relay sees** which books it holds, which member sends when and how much, and the member list – identities,
  device names and names of the members –, not the events.

## Why not MLS

MLS was the first candidate. It fits chats with many members, keeps no history for new members, and its changes of
membership have to be applied in one agreed order (OPEN: to be verified in RFC 9420 and the MLS architecture
document). Books have few members, need their whole history for a decade, and synchronise without an order. Whether
MLS could serve as the transport of the member list later is OPEN.

## Open points

- Whether names in the member list should be encrypted, and sizes padded, to show a relay less.
- Access for a limited time – an auditor for one period – and whether it can be narrower than the whole history.
- Keeping seeds in the key store of the operating system rather than beside the books.
- Archives for retention periods of ten years and more.
