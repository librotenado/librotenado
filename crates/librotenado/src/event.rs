// SPDX-License-Identifier: MIT
//! Events: the immutable facts a set of books consists of, chained per device and merged by union.

use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    fmt,
    str::FromStr,
    sync::Arc,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};

use crate::{
    Account, AccountId, Booking, BooksId, Chart, Closing, Configuration, ContentHash, Currency, Date, DeviceId,
    Document, DocumentId, Filing, FilingId, Hlc, Money, Object, ObjectError, Organisation, ParseError, PartyId, Person,
    canonical, from_root, to_root,
};

/// The SHA-256 hash of an event's canonical encoding – its identity. Written as `sha256:` and 64 lower-case
/// hexadecimal digits, like every hash in the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventHash([u8; 32]);

impl FromStr for EventHash {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        crate::parse_sha256(s).map(EventHash).ok_or_else(|| ParseError::Hash(s.to_owned()))
    }
}

impl fmt::Display for EventHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", hex::encode(self.0))
    }
}

impl Serialize for EventHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for EventHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// What happened – the event's `kind` and the members that come with it, written at the level of the event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Payload {
    /// The set of books was founded under this identity – its first event (spec part 0002).
    BooksFounded {
        /// The identity of the books.
        books: BooksId,
    },
    /// An organisation was described, or its description changed (spec part 0008).
    OrganisationDescribed {
        /// The organisation; a later description with the same identity replaces an earlier one.
        organisation: Organisation,
    },
    /// A person was described, or their description changed (spec part 0008).
    PersonDescribed {
        /// The person; a later description with the same identity replaces an earlier one.
        person: Person,
    },
    /// The books belong to this organisation or person, too.
    OwnerAdded {
        /// The organisation or person.
        owner: PartyId,
    },
    /// The books no longer belong to this organisation or person.
    OwnerRemoved {
        /// The organisation or person.
        owner: PartyId,
    },
    /// How an owner keeps its books from a date on (spec part 0008).
    OwnerConfigured {
        /// The configuration; it applies until the owner's next one.
        configuration: Configuration,
    },
    /// The books use this harmonised chart of accounts (spec part 0004).
    ChartSelected {
        /// The chart, as `name/version`, e.g. `librotenado/0.1`.
        chart: String,
    },
    /// An account of the books' own was opened, refining an account of the chart or another opened account.
    AccountOpened {
        /// The account; its parent is required, its kind is the parent's.
        account: Account,
    },
    /// A booking was recorded.
    BookingRecorded {
        /// The booking.
        booking: Booking,
    },
    /// An earlier booking was cancelled by a counter-booking; nothing is ever changed or deleted.
    BookingReversed {
        /// The event that recorded the booking.
        reverses: EventHash,
        /// The counter-booking, with the opposite amounts.
        booking: Booking,
    },
    /// A document was recorded, with its data and its original; the original's bytes are stored by their hash,
    /// outside the events.
    DocumentRecorded {
        /// The document.
        document: Document,
    },
    /// An owner's books were closed up to a date (spec part 0002).
    PeriodClosed {
        /// The closing.
        closing: Closing,
    },
    /// A filing was derived from the books and is ready to be submitted (spec part 0007).
    FilingPrepared {
        /// The filing.
        filing: Filing,
    },
    /// A filing was submitted to the authority.
    FilingSubmitted {
        /// The prepared filing.
        filing: FilingId,
        /// What the authority returned on submission, as documents; absent when nothing.
        #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
        documents: Vec<DocumentId>,
    },
    /// The authority accepted a filing.
    FilingAccepted {
        /// The prepared filing.
        filing: FilingId,
        /// What the authority returned, as documents; absent when nothing.
        #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
        documents: Vec<DocumentId>,
    },
    /// The authority rejected a filing.
    FilingRejected {
        /// The prepared filing.
        filing: FilingId,
        /// What the authority returned, as documents; absent when nothing.
        #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
        documents: Vec<DocumentId>,
    },
    /// A kind of a later version of the protocol. The event is kept and synchronised as received, but not
    /// interpreted; it cannot be written by this version.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// One event: who wrote it when, its place in the device's chain, and what happened. An event is always a root
/// object: `{"type": "event", "kind": …, "time": …, "seq": …, "prev": …, …}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Event {
    /// The timestamp; its device is the device that wrote the event.
    pub time: Hlc,
    /// Position in the device's chain, starting at 1.
    #[serde(deserialize_with = "crate::safe_u64")]
    pub seq: u64,
    /// The hash of the device's previous event; none for `seq` 1.
    #[serde(deserialize_with = "written")]
    pub prev: Option<EventHash>,
    /// What happened: `kind` and its members.
    #[serde(flatten)]
    pub payload: Payload,
}

impl Object for Event {
    const TYPE: &'static str = "event";
}

impl Event {
    /// The device that wrote the event.
    pub fn device(&self) -> &DeviceId {
        &self.time.device
    }

    /// The identity of an event written on this device: SHA-256 over its canonical encoding (spec part 0003).
    ///
    /// A received event is identified by what was received, which may hold members this version does not know –
    /// use the hash [`EventSet::receive`] returns. An event built in code must keep its integers at or below
    /// 2^53 − 1.
    pub fn hash(&self) -> EventHash {
        let json = to_root(self).expect("an event has only strings and safe integers");
        EventHash(Sha256::digest(json).into())
    }
}

/// The state of a list of events in the agreed order: the hash of the canonical encoding of their hashes.
pub(crate) fn state_of(hashes: &[EventHash]) -> ContentHash {
    let hashes: Vec<String> = hashes.iter().map(|hash| hash.to_string()).collect();
    ContentHash::of(&canonical::to_canonical_vec(&hashes).expect("an array of strings"))
}

/// Deserialises `prev`, which is always written – `null` for the first event of a device, never left out.
fn written<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<EventHash>, D::Error> {
    Option::deserialize(deserializer)
}

/// Something about the accounts of a set of books that needs review (spec part 0002): never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccountProblem {
    /// A posting on an account that is neither in the chart nor opened – perhaps not arrived yet.
    #[error("event {event} posts on account {account}, which is neither in the chart nor opened")]
    UnknownAccount {
        /// The event with the booking.
        event: EventHash,
        /// The account.
        account: AccountId,
    },
    /// An opened account whose parent is missing, or neither in the chart nor opened.
    #[error("account {account}, opened in event {event}, refines no known account")]
    UnknownParent {
        /// The event that opened the account.
        event: EventHash,
        /// The account.
        account: AccountId,
    },
    /// An opened account of another kind than its parent.
    #[error("account {account}, opened in event {event}, is of another kind than its parent")]
    KindMismatch {
        /// The event that opened the account.
        event: EventHash,
        /// The account.
        account: AccountId,
    },
    /// An account opened again, or opened with the identity of an account of the chart.
    #[error("account {account} is opened again in event {event}")]
    Duplicate {
        /// The later event.
        event: EventHash,
        /// The account.
        account: AccountId,
    },
}

/// Something about the parties of a set of books that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PartyProblem {
    /// The books belong to no organisation and no person.
    #[error("the books belong to no organisation and no person")]
    NoOwner,
    /// An owner that is not described – perhaps not arrived yet.
    #[error("event {event} names owner {owner}, who is not described")]
    UnknownOwner {
        /// The event that added the owner.
        event: EventHash,
        /// The owner.
        owner: PartyId,
    },
    /// A person's role in an organisation that is not described.
    #[error("person {person} has a role in {organisation}, which is not described")]
    UnknownOrganisation {
        /// The person.
        person: PartyId,
        /// The organisation.
        organisation: PartyId,
    },
    /// Shares held of a nominal amount the organisation does not have, or more of it than it has.
    #[error("the shares held in {organisation} do not fit its shares of {nominal}")]
    Shares {
        /// The organisation.
        organisation: PartyId,
        /// The nominal amount.
        nominal: Money,
    },
    /// A posting in the books of an owner, dated before the owner's first configuration.
    #[error("event {event} posts in the books of {owner} before they are configured")]
    NotConfigured {
        /// The event with the booking.
        event: EventHash,
        /// The owner named in the posting.
        owner: PartyId,
    },
    /// A configuration for an organisation or person that is not an owner of the books.
    #[error("event {event} configures {owner}, who is not an owner")]
    ConfigurationOfNonOwner {
        /// The event with the configuration.
        event: EventHash,
        /// The organisation or person.
        owner: PartyId,
    },
    /// A posting in the books of someone who is not an owner of the books.
    #[error("event {event} posts in the books of {owner}, who is not an owner")]
    NotAnOwner {
        /// The event with the booking.
        event: EventHash,
        /// The owner named in the posting.
        owner: PartyId,
    },
}

/// Something about the documents of a set of books that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentProblem {
    /// A booking based on a document that has not been recorded – perhaps not arrived yet.
    #[error("event {event} is based on document {document}, which has not been recorded")]
    UnknownDocument {
        /// The event with the booking.
        event: EventHash,
        /// The document.
        document: DocumentId,
    },
    /// A document recorded again under the same identity.
    #[error("document {document} is recorded again in event {event}")]
    Duplicate {
        /// The later event.
        event: EventHash,
        /// The document.
        document: DocumentId,
    },
}

/// Something in a set of events that does not fit a clean chain per device.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChainProblem {
    /// Two different events of one device with the same position – the device's history was forked.
    #[error("device {device} has two events at position {seq}")]
    Fork {
        /// The device.
        device: DeviceId,
        /// The position.
        seq: u64,
    },
    /// Positions of a device are missing – events have not arrived (yet).
    #[error("device {device} is missing events {from} to {to}")]
    Gap {
        /// The device.
        device: DeviceId,
        /// First missing position.
        from: u64,
        /// Last missing position.
        to: u64,
    },
    /// An event does not point to the device's previous event.
    #[error("event {event} of device {device} does not follow the device's previous event")]
    BrokenLink {
        /// The device.
        device: DeviceId,
        /// The event.
        event: EventHash,
    },
}

/// A set of events – the books as they are known on one device. Merging is set union, in any order, any number
/// of times, with the same result.
///
/// Every event is kept in its canonical encoding as well as read: the encoding is what is hashed and what is passed
/// on, so members a later version adds survive this one unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventSet {
    events: BTreeMap<EventHash, Stored>,
}

/// An event as read, and the canonical bytes it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stored {
    event: Event,
    canonical: Arc<[u8]>,
}

impl EventSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an event written on this device; returns its hash and whether it was new.
    ///
    /// # Panics
    ///
    /// If the event has no canonical encoding: an integer above 2^53 − 1, or a [`Payload::Unknown`] (which can only
    /// be received).
    pub fn insert(&mut self, event: Event) -> (EventHash, bool) {
        let canonical = to_root(&event).expect("an event written here has a canonical encoding");
        self.add(Stored { event, canonical: canonical.into() })
    }

    /// Adds an event received as JSON, in any formatting; returns its hash and whether it was new. The hash is taken
    /// over the canonical encoding of what was received, members this version does not know included.
    pub fn receive(&mut self, json: &[u8]) -> Result<(EventHash, bool), ObjectError> {
        let canonical = canonical::canonicalize(json)?;
        let event = from_root(&canonical)?;
        Ok(self.add(Stored { event, canonical: canonical.into() }))
    }

    fn add(&mut self, stored: Stored) -> (EventHash, bool) {
        let hash = EventHash(Sha256::digest(&stored.canonical).into());
        match self.events.entry(hash) {
            Entry::Vacant(slot) => {
                slot.insert(stored);
                (hash, true)
            }
            Entry::Occupied(_) => (hash, false),
        }
    }

    /// Adds all events of another set; returns how many were new.
    pub fn merge(&mut self, other: &EventSet) -> usize {
        other.events.values().filter(|stored| self.add((*stored).clone()).1).count()
    }

    /// The event with this hash.
    pub fn get(&self, hash: &EventHash) -> Option<&Event> {
        self.events.get(hash).map(|stored| &stored.event)
    }

    /// The canonical encoding of the event with this hash – what is passed on to other devices.
    pub fn canonical(&self, hash: &EventHash) -> Option<&[u8]> {
        self.events.get(hash).map(|stored| &*stored.canonical)
    }

    /// The number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether there are no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// All events in the order every device agrees on: by timestamp, then by hash.
    pub fn ordered(&self) -> Vec<(EventHash, &Event)> {
        let mut events: Vec<_> = self.events.iter().map(|(hash, stored)| (*hash, &stored.event)).collect();
        events.sort_by(|(ha, a), (hb, b)| a.time.cmp(&b.time).then(ha.cmp(hb)));
        events
    }

    /// The next position and previous hash for a new event of `device`, if its chain is clean.
    pub fn chain_head(&self, device: &DeviceId) -> (u64, Option<EventHash>) {
        self.events
            .iter()
            .map(|(hash, stored)| (hash, &stored.event))
            .filter(|(_, event)| event.device() == device)
            .max_by_key(|(_, event)| event.seq)
            .map_or((1, None), |(hash, event)| (event.seq + 1, Some(*hash)))
    }

    /// The charts the books selected, in the agreed order of events, each once. More than one is for review.
    pub fn selected_charts(&self) -> Vec<&str> {
        let mut charts: Vec<&str> = Vec::new();
        for (_, event) in self.ordered() {
            if let Payload::ChartSelected { chart } = &event.payload
                && !charts.contains(&chart.as_str())
            {
                charts.push(chart);
            }
        }
        charts
    }

    /// Checks the accounts against `chart` and the accounts opened in the books: every opened account refines a
    /// known account of the same kind and is opened once; every posting is on a known account.
    pub fn review_accounts(&self, chart: &Chart) -> Vec<AccountProblem> {
        let mut problems = Vec::new();
        let mut opened: BTreeMap<&AccountId, (EventHash, &Account)> = BTreeMap::new();
        for (hash, event) in self.ordered() {
            if let Payload::AccountOpened { account } = &event.payload {
                if chart.get(&account.id).is_some() || opened.contains_key(&account.id) {
                    problems.push(AccountProblem::Duplicate { event: hash, account: account.id.clone() });
                } else {
                    opened.insert(&account.id, (hash, account));
                }
            }
        }
        let kind_of = |id: &AccountId| chart.get(id).map(|a| a.kind).or_else(|| opened.get(id).map(|(_, a)| a.kind));
        for (hash, account) in opened.values() {
            match account.parent.as_ref().and_then(kind_of) {
                None => problems.push(AccountProblem::UnknownParent { event: *hash, account: account.id.clone() }),
                Some(kind) if kind != account.kind => {
                    problems.push(AccountProblem::KindMismatch { event: *hash, account: account.id.clone() })
                }
                Some(_) => {}
            }
        }
        for (hash, event) in self.ordered() {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            for posting in &booking.postings {
                if kind_of(&posting.account).is_none() {
                    problems.push(AccountProblem::UnknownAccount { event: hash, account: posting.account.clone() });
                }
            }
        }
        problems
    }

    /// The organisations and persons as last described, in the agreed order of events.
    pub fn parties(&self) -> (BTreeMap<&PartyId, &Organisation>, BTreeMap<&PartyId, &Person>) {
        let (mut organisations, mut persons) = (BTreeMap::new(), BTreeMap::new());
        for (_, event) in self.ordered() {
            match &event.payload {
                Payload::OrganisationDescribed { organisation } => {
                    organisations.insert(&organisation.id, organisation);
                }
                Payload::PersonDescribed { person } => {
                    persons.insert(&person.id, person);
                }
                _ => {}
            }
        }
        (organisations, persons)
    }

    /// The owners of the books – organisations and persons – after all additions and removals, in order.
    pub fn owners(&self) -> Vec<&PartyId> {
        let mut owners: Vec<&PartyId> = Vec::new();
        for (_, event) in self.ordered() {
            match &event.payload {
                Payload::OwnerAdded { owner } if !owners.contains(&owner) => owners.push(owner),
                Payload::OwnerRemoved { owner } => owners.retain(|o| *o != owner),
                _ => {}
            }
        }
        owners
    }

    /// The configuration of `owner` that applies on `date`: the one with the latest `from` on or before it, and of
    /// two with the same `from` the later in the agreed order.
    pub fn configuration(&self, owner: &PartyId, date: Date) -> Option<&Configuration> {
        let mut applies: Option<&Configuration> = None;
        for (_, event) in self.ordered() {
            if let Payload::OwnerConfigured { configuration } = &event.payload
                && configuration.owner == *owner
                && configuration.from <= date
                && applies.is_none_or(|a| a.from <= configuration.from)
            {
                applies = Some(configuration);
            }
        }
        applies
    }

    /// Checks the parties: the books belong to at least one organisation or person, every owner is described and
    /// configured before its first posting, only owners are configured, every organisation a person has a role in is
    /// described, the shares held fit the shares an organisation has, and every posting is in the books of an owner.
    pub fn review_parties(&self) -> Vec<PartyProblem> {
        let (organisations, persons) = self.parties();
        let owners = self.owners();
        let mut problems = Vec::new();
        if owners.is_empty() {
            problems.push(PartyProblem::NoOwner);
        }
        for (hash, event) in self.ordered() {
            if let Payload::OwnerAdded { owner } = &event.payload
                && owners.contains(&owner)
                && !organisations.contains_key(owner)
                && !persons.contains_key(owner)
            {
                problems.push(PartyProblem::UnknownOwner { event: hash, owner: owner.clone() });
            }
        }
        // Per organisation and nominal amount (by currency and base units): how many shares persons hold.
        let mut held: BTreeMap<(&PartyId, &Currency, i128), u64> = BTreeMap::new();
        for person in persons.values() {
            for role in &person.roles {
                if !organisations.contains_key(&role.organisation) {
                    problems.push(PartyProblem::UnknownOrganisation {
                        person: person.id.clone(),
                        organisation: role.organisation.clone(),
                    });
                } else if let Some(shares) = &role.shares {
                    let nominal = (shares.nominal.currency(), shares.nominal.base_units());
                    let count = held.entry((&role.organisation, nominal.0, nominal.1)).or_insert(0);
                    *count = count.saturating_add(shares.count);
                }
            }
        }
        for ((organisation, currency, units), count) in held {
            let nominal = Money::new(units, currency.clone()).expect("taken from an amount");
            let has = organisations[organisation].shares.iter().find(|s| s.nominal == nominal).map(|s| s.count);
            if has.is_none_or(|has| count > has) {
                problems.push(PartyProblem::Shares { organisation: organisation.clone(), nominal });
            }
        }
        // Per owner: the first day a configuration applies from.
        let mut first_configured: BTreeMap<&PartyId, Date> = BTreeMap::new();
        for (hash, event) in self.ordered() {
            if let Payload::OwnerConfigured { configuration } = &event.payload {
                if !owners.contains(&&configuration.owner) {
                    problems.push(PartyProblem::ConfigurationOfNonOwner {
                        event: hash,
                        owner: configuration.owner.clone(),
                    });
                }
                let from = first_configured.entry(&configuration.owner).or_insert(configuration.from);
                *from = (*from).min(configuration.from);
            }
        }
        for (hash, event) in self.ordered() {
            if let Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } = &event.payload {
                let mut named: Vec<&PartyId> = booking.postings.iter().map(|p| &p.owner).collect();
                named.sort();
                named.dedup();
                for owner in named {
                    if !owners.contains(&owner) {
                        problems.push(PartyProblem::NotAnOwner { event: hash, owner: owner.clone() });
                    } else if first_configured.get(owner).is_none_or(|from| booking.date < *from) {
                        problems.push(PartyProblem::NotConfigured { event: hash, owner: owner.clone() });
                    }
                }
            }
        }
        problems
    }

    /// The state of the books: the SHA-256 hash of the canonical encoding of the array of all event hashes in the
    /// agreed order. Whatever is derived from the books – a filing, a report – records the state it was derived from.
    pub fn state(&self) -> ContentHash {
        let hashes: Vec<EventHash> = self.ordered().iter().map(|(hash, _)| *hash).collect();
        state_of(&hashes)
    }

    /// Checks the documents: each recorded once, and every document a booking is based on recorded. Whether the
    /// bytes of the originals are at hand is a question of the store, not of the events.
    pub fn review_documents(&self) -> Vec<DocumentProblem> {
        let mut problems = Vec::new();
        let mut recorded = BTreeSet::new();
        for (hash, event) in self.ordered() {
            if let Payload::DocumentRecorded { document } = &event.payload
                && !recorded.insert(&document.id)
            {
                problems.push(DocumentProblem::Duplicate { event: hash, document: document.id.clone() });
            }
        }
        for (hash, event) in self.ordered() {
            if let Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } = &event.payload {
                for document in booking.documents.iter().filter(|d| !recorded.contains(d)) {
                    problems.push(DocumentProblem::UnknownDocument { event: hash, document: document.clone() });
                }
            }
        }
        problems
    }

    /// Checks every device's chain: one event per position, no gaps, each event pointing to its predecessor.
    pub fn verify(&self) -> Vec<ChainProblem> {
        // Per device, per position: the events found there (more than one is a fork).
        type Positions<'a> = BTreeMap<u64, Vec<(EventHash, &'a Event)>>;
        let mut chains: BTreeMap<&DeviceId, Positions> = BTreeMap::new();
        for (hash, Stored { event, .. }) in &self.events {
            chains.entry(event.device()).or_default().entry(event.seq).or_default().push((*hash, event));
        }
        let mut problems = Vec::new();
        for (device, positions) in chains {
            let mut expected = 1;
            let mut previous: Option<EventHash> = None;
            for (seq, events) in positions {
                if seq > expected {
                    problems.push(ChainProblem::Gap { device: device.clone(), from: expected, to: seq - 1 });
                    previous = None;
                }
                if events.len() > 1 {
                    problems.push(ChainProblem::Fork { device: device.clone(), seq });
                    previous = None;
                } else {
                    let (hash, event) = events[0];
                    let linked =
                        if seq == 1 { event.prev.is_none() } else { previous.is_none() || event.prev == previous };
                    if !linked {
                        problems.push(ChainProblem::BrokenLink { device: device.clone(), event: hash });
                    }
                    previous = Some(hash);
                }
                expected = seq + 1;
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccountKind, Clock, Money, Posting, booking::tests::account, party::tests::party};

    fn booking(base_units: i128) -> Booking {
        let money = |units: i128| Money::new(units, "EUR".parse().unwrap()).unwrap();
        Booking {
            date: "2026-10-24".parse().unwrap(),
            text: "groceries".into(),
            postings: vec![
                Posting {
                    owner: party(1),
                    account: account(1),
                    amount: money(base_units),
                    note: None,
                    tax: None,
                    exchange: None,
                },
                Posting {
                    owner: party(1),
                    account: account(2),
                    amount: money(-base_units),
                    note: None,
                    tax: None,
                    exchange: None,
                },
            ],
            documents: vec![],
            transaction: None,
        }
    }

    fn record(set: &mut EventSet, clock: &mut Clock, device: &str, ms: u64, base_units: i128) -> EventHash {
        let (seq, prev) = set.chain_head(&device.parse().unwrap());
        let event = Event {
            time: clock.now(ms),
            seq,
            prev,
            payload: Payload::BookingRecorded { booking: booking(base_units) },
        };
        set.insert(event).0
    }

    #[test]
    fn hash_is_stable_and_round_trips_through_json() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let hash = record(&mut set, &mut clock, "a", 1000, 10);
        let event = set.get(&hash).unwrap();
        let json = serde_json::to_string(event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hash(), hash);
        assert_eq!(hash.to_string().parse::<EventHash>().unwrap(), hash);
    }

    #[test]
    fn the_hash_does_not_depend_on_field_order_or_whitespace() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let hash = record(&mut set, &mut clock, "a", 1000, 10);
        let event: serde_json::Value = serde_json::from_slice(set.canonical(&hash).unwrap()).unwrap();
        // The same event, written with its members in reverse order and with whitespace.
        let reordered = match event {
            serde_json::Value::Object(map) => {
                let members: Vec<String> = map.iter().rev().map(|(k, v)| format!("{k:?} : {v}")).collect();
                format!("{{ {} }}", members.join(" ,\n "))
            }
            _ => unreachable!(),
        };
        let back: Event = from_root(reordered.as_bytes()).unwrap();
        assert_eq!(back.hash(), hash);
        let mut received = EventSet::new();
        assert_eq!(received.receive(reordered.as_bytes()).unwrap(), (hash, true));
        assert_eq!(received.canonical(&hash), set.canonical(&hash));
    }

    /// The canonical encoding of the one event in `set` as a JSON value, to be changed by a test.
    fn first_as_value(set: &EventSet) -> serde_json::Value {
        let (hash, _) = set.ordered()[0];
        serde_json::from_slice(set.canonical(&hash).unwrap()).unwrap()
    }

    #[test]
    fn members_of_later_versions_survive() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let local = record(&mut set, &mut clock, "a", 1000, 10);
        let mut value = first_as_value(&set);
        value["booking"]["future"] = serde_json::json!({"z": 1});
        let json = serde_json::to_vec(&value).unwrap();

        let mut b = EventSet::new();
        let (hash, new) = b.receive(&json).unwrap();
        assert!(new);
        assert_ne!(hash, local, "a different event");
        assert_ne!(b.get(&hash).unwrap().hash(), hash, "re-encoding would lose the member");
        let mut c = EventSet::new();
        c.merge(&b);
        assert!(String::from_utf8_lossy(c.canonical(&hash).unwrap()).contains(r#""future":{"z":1}"#));
    }

    #[test]
    fn payloads_of_later_versions_are_kept_but_not_read() {
        let json = r#"{"time":{"wall_ms":1,"counter":0,"device":"a"},"seq":1,"prev":null,"type":"event","kind":"budget_set","amount":"5"}"#;
        let mut set = EventSet::new();
        let (hash, _) = set.receive(json.as_bytes()).unwrap();
        assert_eq!(set.get(&hash).unwrap().payload, Payload::Unknown);
        assert!(set.verify().is_empty());
        assert_eq!(set.chain_head(&"a".parse().unwrap()), (2, Some(hash)));
    }

    #[test]
    fn none_has_one_form() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        record(&mut set, &mut clock, "a", 1000, 10);
        let value = first_as_value(&set);
        let changed = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut value = value.clone();
            change(&mut value);
            EventSet::new().receive(&serde_json::to_vec(&value).unwrap())
        };
        assert!(changed(&|_| {}).is_ok());
        assert!(changed(&|v| v["booking"]["documents"] = serde_json::json!([])).is_err());
        assert!(changed(&|v| _ = v.as_object_mut().unwrap().remove("prev")).is_err());
        let document = r#"{"time":{"wall_ms":1,"counter":0,"device":"a"},"seq":1,"prev":null,"type":"event","kind":"document_recorded","document":{"id":"0192f1e4-0000-7000-8000-0000000000d1","kind":"receipt","date":"2026-10-24","received":"2026-10-24","original":{"hash":"sha256:0000000000000000000000000000000000000000000000000000000000000000","size":1,"media_type":"application/pdf","format":null}}}"#;
        assert!(EventSet::new().receive(document.as_bytes()).is_err());
        assert!(EventSet::new().receive(document.replace(r#","format":null"#, "").as_bytes()).is_ok());
    }

    #[test]
    fn an_event_is_a_root_object_with_its_kind() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let hash = record(&mut set, &mut clock, "a", 1000, 10);
        let text = String::from_utf8(set.canonical(&hash).unwrap().to_vec()).unwrap();
        assert!(text.contains(r#""kind":"booking_recorded""#) && text.ends_with(r#""type":"event"}"#), "{text}");
        assert!(!text.contains("payload"));
    }

    /// A chart with 1 (asset) and 2 (expense), the accounts of `booking`.
    fn chart() -> Chart {
        let account = |n, kind| Account { id: account(n), kind, parent: None, attributes: Default::default() };
        Chart {
            name: "test".into(),
            version: "1".into(),
            accounts: vec![account(1, AccountKind::Expense), account(2, AccountKind::Asset)],
        }
    }

    fn open(set: &mut EventSet, clock: &mut Clock, ms: u64, n: u8, kind: AccountKind, parent: Option<u8>) {
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        let account = Account { id: account(n), kind, parent: parent.map(account), attributes: Default::default() };
        set.insert(Event { time: clock.now(ms), seq, prev, payload: Payload::AccountOpened { account } });
    }

    #[test]
    fn opened_accounts_refine_known_ones() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        let selected = Payload::ChartSelected { chart: "test/1".into() };
        set.insert(Event { time: clock.now(1), seq, prev, payload: selected });
        open(&mut set, &mut clock, 2, 10, AccountKind::Asset, Some(2));
        open(&mut set, &mut clock, 3, 11, AccountKind::Asset, Some(10));
        record(&mut set, &mut clock, "a", 4, 5);
        assert_eq!(set.selected_charts(), ["test/1"]);
        assert_eq!(set.review_accounts(&chart()), []);

        open(&mut set, &mut clock, 5, 12, AccountKind::Income, Some(2));
        open(&mut set, &mut clock, 6, 13, AccountKind::Asset, Some(99));
        open(&mut set, &mut clock, 7, 13, AccountKind::Asset, Some(2));
        open(&mut set, &mut clock, 8, 14, AccountKind::Asset, None);
        let found: Vec<(&str, AccountId)> = set
            .review_accounts(&chart())
            .into_iter()
            .map(|problem| match problem {
                AccountProblem::KindMismatch { account: a, .. } => ("kind", a),
                AccountProblem::UnknownParent { account: a, .. } => ("parent", a),
                AccountProblem::Duplicate { account: a, .. } => ("duplicate", a),
                AccountProblem::UnknownAccount { account: a, .. } => ("unknown", a),
            })
            .collect();
        for want in [("kind", 12), ("parent", 13), ("duplicate", 13), ("parent", 14)] {
            assert!(found.contains(&(want.0, account(want.1))), "{want:?} in {found:?}");
        }
        assert_eq!(found.len(), 4);
    }

    #[test]
    fn postings_on_unknown_accounts_are_for_review() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        record(&mut set, &mut clock, "a", 1, 5);
        assert_eq!(set.review_accounts(&chart()), []);
        let empty = Chart { accounts: vec![], ..chart() };
        assert_eq!(set.review_accounts(&empty).len(), 2, "both postings, until the accounts arrive");
    }

    #[test]
    fn bookings_on_unrecorded_documents_are_for_review() {
        use crate::{Document, DocumentKind, Original};
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let id: DocumentId = "0192f1e4-0000-7000-8000-0000000000d1".parse().unwrap();
        let document = Document {
            id: id.clone(),
            kind: DocumentKind::Receipt,
            number: None,
            date: "2026-10-24".parse().unwrap(),
            received: "2026-10-24".parse().unwrap(),
            original: Original::of(b"a receipt", "image/jpeg"),
            invoice: None,
        };
        let mut based = booking(5);
        based.documents = vec![id.clone()];
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        let event = Event { time: clock.now(1), seq, prev, payload: Payload::BookingRecorded { booking: based } };
        let hash = set.insert(event).0;
        assert_eq!(set.review_documents(), [DocumentProblem::UnknownDocument { event: hash, document: id.clone() }]);

        for ms in [2, 3] {
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            let payload = Payload::DocumentRecorded { document: document.clone() };
            set.insert(Event { time: clock.now(ms), seq, prev, payload });
        }
        let problems = set.review_documents();
        assert!(matches!(problems.as_slice(), [DocumentProblem::Duplicate { document, .. }] if *document == id));
    }

    #[test]
    fn a_note_explains_a_posting_and_is_never_empty() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let mut noted = booking(5);
        noted.postings[0].note = Some("receipt lost, own receipt to follow".into());
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        let hash =
            set.insert(Event { time: clock.now(1), seq, prev, payload: Payload::BookingRecorded { booking: noted } }).0;
        let text = String::from_utf8(set.canonical(&hash).unwrap().to_vec()).unwrap();
        assert!(text.contains(r#""note":"receipt lost, own receipt to follow""#));
        let empty = text.replace("receipt lost, own receipt to follow", "");
        assert!(EventSet::new().receive(empty.as_bytes()).is_err());
    }

    /// Appends an event of `payload` to device a's chain.
    fn append(set: &mut EventSet, clock: &mut Clock, ms: u64, payload: Payload) -> EventHash {
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        set.insert(Event { time: clock.now(ms), seq, prev, payload }).0
    }

    #[test]
    fn books_belong_to_described_parties() {
        use crate::party::tests::{organisation, two_euro_shares};
        use crate::{Person, Role, RoleKind};
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        assert_eq!(set.review_parties(), [PartyProblem::NoOwner]);

        let added = append(&mut set, &mut clock, 1, Payload::OwnerAdded { owner: party(1) });
        assert_eq!(set.review_parties(), [PartyProblem::UnknownOwner { event: added, owner: party(1) }]);

        append(&mut set, &mut clock, 2, Payload::OrganisationDescribed { organisation: organisation(1) });
        let role = |n| Role { organisation: party(n), kind: RoleKind::Shareholder, shares: Some(two_euro_shares(257)) };
        let person = Person { id: party(2), name: "Erika Beispiel".into(), tax_ids: vec![], roles: vec![role(1)] };
        append(&mut set, &mut clock, 3, Payload::PersonDescribed { person: person.clone() });
        assert_eq!(set.review_parties(), []);
        assert_eq!(set.owners(), [&party(1)]);

        let renamed = Organisation { name: "Example Trading AG".into(), ..organisation(1) };
        append(&mut set, &mut clock, 4, Payload::OrganisationDescribed { organisation: renamed });
        assert_eq!(set.parties().0[&party(1)].name, "Example Trading AG", "the later description replaces");

        let elsewhere = Person { roles: vec![role(9)], ..person };
        append(&mut set, &mut clock, 5, Payload::PersonDescribed { person: elsewhere });
        assert_eq!(
            set.review_parties(),
            [PartyProblem::UnknownOrganisation { person: party(2), organisation: party(9) }]
        );
        let more = Role { shares: Some(two_euro_shares(513)), ..role(1) };
        let second = Person { id: party(3), name: "Max Beispiel".into(), tax_ids: vec![], roles: vec![more] };
        append(&mut set, &mut clock, 6, Payload::PersonDescribed { person: second });
        let too_many = set.review_parties();
        assert!(too_many.iter().any(|p| matches!(p, PartyProblem::Shares { .. })), "513 of 512: {too_many:?}");

        append(&mut set, &mut clock, 7, Payload::OwnerRemoved { owner: party(1) });
        record(&mut set, &mut clock, "a", 8, 5);
        let problems = set.review_parties();
        assert!(problems.contains(&PartyProblem::NoOwner));
        assert!(problems.iter().any(|p| matches!(p, PartyProblem::NotAnOwner { owner, .. } if *owner == party(1))));
    }

    #[test]
    fn the_state_changes_with_every_event_and_not_with_order() {
        let (mut a, mut b) = (EventSet::new(), EventSet::new());
        let (mut ca, mut cb) = (Clock::new("a".parse().unwrap()), Clock::new("b".parse().unwrap()));
        let empty = a.state();
        record(&mut a, &mut ca, "a", 1000, 1);
        record(&mut b, &mut cb, "b", 1500, 2);
        assert_ne!(a.state(), empty);
        let before = a.state();
        let mut ab = a.clone();
        ab.merge(&b);
        let mut ba = b.clone();
        ba.merge(&a);
        assert_eq!(ab.state(), ba.state());
        assert_ne!(ab.state(), before);
    }

    #[test]
    fn rejects_duplicate_members() {
        let json =
            r#"{"seq":1,"seq":2,"time":{"wall_ms":1,"counter":0,"device":"a"},"prev":null,"type":"event","kind":"x"}"#;
        assert!(matches!(EventSet::new().receive(json.as_bytes()), Err(ObjectError::Canonical(_))));
    }

    #[test]
    fn rejects_integers_beyond_the_canonical_range() {
        let json = r#"{"time":{"wall_ms":9007199254740992,"counter":0,"device":"a"},"seq":1,"prev":null,"type":"event","kind":"booking_recorded","booking":{"date":"2026-10-24","text":"x","postings":[]}}"#;
        assert!(serde_json::from_str::<Event>(json).is_err());
    }

    #[test]
    fn merging_is_order_independent_and_idempotent() {
        let (mut a, mut b) = (EventSet::new(), EventSet::new());
        let (mut ca, mut cb) = (Clock::new("a".parse().unwrap()), Clock::new("b".parse().unwrap()));
        record(&mut a, &mut ca, "a", 1000, 1);
        record(&mut a, &mut ca, "a", 2000, 2);
        record(&mut b, &mut cb, "b", 1500, 3);

        let mut ab = a.clone();
        assert_eq!(ab.merge(&b), 1);
        let mut ba = b.clone();
        assert_eq!(ba.merge(&a), 2);
        assert_eq!(ab, ba);
        assert_eq!(ab.merge(&b), 0);
        let order: Vec<u64> = ab.ordered().iter().map(|(_, e)| e.time.wall_ms).collect();
        assert_eq!(order, [1000, 1500, 2000]);
        assert!(ab.verify().is_empty());
    }

    #[test]
    fn finds_forks_gaps_and_broken_links() {
        let device: DeviceId = "a".parse().unwrap();
        let mut clock = Clock::new(device.clone());
        let mut set = EventSet::new();
        let first = record(&mut set, &mut clock, "a", 1000, 1);

        // Same position twice: a fork.
        let fork = Event {
            time: clock.now(1001),
            seq: 1,
            prev: None,
            payload: Payload::BookingRecorded { booking: booking(9) },
        };
        let mut forked = set.clone();
        forked.insert(fork);
        assert_eq!(forked.verify(), [ChainProblem::Fork { device: device.clone(), seq: 1 }]);

        // Position 3 without 2: a gap.
        let late = Event {
            time: clock.now(1002),
            seq: 3,
            prev: None,
            payload: Payload::BookingRecorded { booking: booking(3) },
        };
        let mut gapped = set.clone();
        gapped.insert(late);
        assert_eq!(gapped.verify(), [ChainProblem::Gap { device: device.clone(), from: 2, to: 2 }]);

        // Position 2 pointing elsewhere: a broken link.
        let wrong = Event {
            time: clock.now(1003),
            seq: 2,
            prev: Some(format!("sha256:{}", "00".repeat(32)).parse().unwrap()),
            payload: Payload::BookingRecorded { booking: booking(2) },
        };
        let hash = set.insert(wrong).0;
        assert_ne!(Some(first), set.get(&hash).unwrap().prev);
        assert_eq!(set.verify(), [ChainProblem::BrokenLink { device, event: hash }]);
    }
}
