// SPDX-License-Identifier: MIT
//! A set of books as a whole (spec part 0002): its identity, transactions it shares with other sets of books, and
//! the periods closed in it.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Booking, ContentHash, Date, EventHash, EventSet, Object, ParseError, PartyId, Payload};

/// The identity of a set of books: a UUID in its one form, given by its first event, `books_founded`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BooksId(String);

impl BooksId {
    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for BooksId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(BooksId(s.to_owned())) } else { Err(ParseError::Books(s.to_owned())) }
    }
}

impl fmt::Display for BooksId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for BooksId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BooksId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// The identity of a business transaction recorded in several sets of books: a UUID in its one form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionId(String);

impl TransactionId {
    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for TransactionId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(TransactionId(s.to_owned())) } else { Err(ParseError::Transaction(s.to_owned())) }
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for TransactionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TransactionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// A business transaction a booking shares with bookings in other sets of books – a bill split between a business
/// and its owner's private books.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transaction {
    /// The identity, the same in every set of books that records the transaction.
    pub id: TransactionId,
    /// The other sets of books that record it; at least one.
    #[serde(deserialize_with = "crate::non_empty")]
    pub books: Vec<BooksId>,
}

impl Object for Transaction {
    const TYPE: &'static str = "transaction";
}

/// An owner's books closed up to a date: the period's figures are final.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Closing {
    /// The organisation or person whose books are closed.
    pub owner: PartyId,
    /// The last day closed; everything up to it is closed.
    pub to: Date,
    /// The state of the books the closing was decided on: the events before it in the agreed order.
    pub state: ContentHash,
    /// The implementation and modules the figures were derived with, name → version; left out when not given.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "crate::non_empty_map")]
    pub software: BTreeMap<String, String>,
}

impl Object for Closing {
    const TYPE: &'static str = "closing";
}

/// Something about the identity of a set of books that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BooksProblem {
    /// Events without a `books_founded` among them – perhaps not arrived yet.
    #[error("the books have not been founded")]
    NotFounded,
    /// A further `books_founded`: two sets of books were merged, or one was founded twice.
    #[error("event {event} founds the books again, as {books}")]
    FoundedTwice {
        /// The later founding.
        event: EventHash,
        /// The identity it gives.
        books: BooksId,
    },
}

/// Something about a transaction across sets of books that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, thiserror::Error)]
pub enum TransactionProblem {
    /// Two bookings with the same transaction in one set of books.
    #[error("event {event} records transaction {transaction} again in books {books}")]
    Duplicate {
        /// The set of books.
        books: BooksId,
        /// The later booking's event.
        event: EventHash,
        /// The transaction.
        transaction: TransactionId,
    },
    /// A booking names a set of books that is held but has no booking of the transaction.
    #[error("event {event} in books {books} names books {other}, which do not record transaction {transaction}")]
    Missing {
        /// The set of books of the booking.
        books: BooksId,
        /// The booking's event.
        event: EventHash,
        /// The set of books named.
        other: BooksId,
        /// The transaction.
        transaction: TransactionId,
    },
    /// The bookings of a transaction do not name each other's sets of books.
    #[error("the bookings of transaction {transaction} do not name each other's books")]
    BooksMismatch {
        /// The transaction.
        transaction: TransactionId,
    },
    /// The bookings of a transaction have different dates.
    #[error("the bookings of transaction {transaction} have different dates")]
    DateMismatch {
        /// The transaction.
        transaction: TransactionId,
    },
    /// The booking of a transaction is reversed in one set of books and not in another.
    #[error("transaction {transaction} is reversed in some books and not in others")]
    ReversalMismatch {
        /// The transaction.
        transaction: TransactionId,
    },
}

/// Something about a closed period that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClosingProblem {
    /// The events before a closing do not have the state it names: events arrived that whoever closed had not seen.
    #[error("the books before closing {closing} are not the books it was decided on")]
    StateMismatch {
        /// The closing's event.
        closing: EventHash,
    },
    /// A booking after a closing, in the books of its owner, dated in the closed period.
    #[error("event {event} books into the period closed by {closing}")]
    LateBooking {
        /// The booking's event.
        event: EventHash,
        /// The closing's event.
        closing: EventHash,
    },
    /// A configuration after a closing, for its owner, from a date in the closed period.
    #[error("event {event} configures the period closed by {closing}")]
    LateConfiguration {
        /// The configuration's event.
        event: EventHash,
        /// The closing's event.
        closing: EventHash,
    },
}

impl EventSet {
    /// The identity of the books: that of the first `books_founded` in the agreed order.
    pub fn books(&self) -> Option<&BooksId> {
        self.ordered().into_iter().find_map(|(_, event)| match &event.payload {
            Payload::BooksFounded { books } => Some(books),
            _ => None,
        })
    }

    /// Checks the identity: the books are founded, and founded once.
    pub fn review_books(&self) -> Vec<BooksProblem> {
        let mut problems = Vec::new();
        let mut founded = false;
        for (hash, event) in self.ordered() {
            if let Payload::BooksFounded { books } = &event.payload {
                if founded {
                    problems.push(BooksProblem::FoundedTwice { event: hash, books: books.clone() });
                }
                founded = true;
            }
        }
        if !founded && !self.is_empty() {
            problems.push(BooksProblem::NotFounded);
        }
        problems
    }

    /// The closings of the books, in the agreed order, with their events.
    pub fn closings(&self) -> Vec<(EventHash, &Closing)> {
        self.ordered()
            .into_iter()
            .filter_map(|(hash, event)| match &event.payload {
                Payload::PeriodClosed { closing } => Some((hash, closing)),
                _ => None,
            })
            .collect()
    }

    /// The last day closed in the books of `owner`, if any.
    pub fn closed_until(&self, owner: &PartyId) -> Option<Date> {
        self.closings().into_iter().filter(|(_, c)| c.owner == *owner).map(|(_, c)| c.to).max()
    }

    /// Checks the closings: each was decided on the events before it, and nothing written after it falls into the
    /// period it closed for its owner.
    pub fn review_closings(&self) -> Vec<ClosingProblem> {
        let ordered = self.ordered();
        let mut problems = Vec::new();
        for (position, (closing_hash, event)) in ordered.iter().enumerate() {
            let Payload::PeriodClosed { closing } = &event.payload else { continue };
            let before: Vec<EventHash> = ordered[..position].iter().map(|(hash, _)| *hash).collect();
            if crate::event::state_of(&before) != closing.state {
                problems.push(ClosingProblem::StateMismatch { closing: *closing_hash });
            }
            for (hash, later) in &ordered[position + 1..] {
                match &later.payload {
                    Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. }
                        if booking.date <= closing.to && booking.postings.iter().any(|p| p.owner == closing.owner) =>
                    {
                        problems.push(ClosingProblem::LateBooking { event: *hash, closing: *closing_hash });
                    }
                    Payload::OwnerConfigured { configuration }
                        if configuration.owner == closing.owner && configuration.from <= closing.to =>
                    {
                        problems.push(ClosingProblem::LateConfiguration { event: *hash, closing: *closing_hash });
                    }
                    _ => {}
                }
            }
        }
        problems
    }
}

/// A booking of a transaction as found in one set of books.
struct Side<'a> {
    event: EventHash,
    booking: &'a Booking,
    names: &'a [BooksId],
    reversed: bool,
}

/// Checks the transactions across the sets of books held: within each set a transaction is booked once, and every
/// set of books a booking names – if held – records the transaction too, names the booking's set in return, on the
/// same date and reversed alike. Sets of books that are not founded cannot be named and are left out.
pub fn review_transactions(sets: &[&EventSet]) -> Vec<TransactionProblem> {
    let mut problems = Vec::new();
    // Per set of books held, per transaction: its booking there.
    let mut held: BTreeMap<&BooksId, BTreeMap<&TransactionId, Side>> = BTreeMap::new();
    for set in sets {
        let Some(books) = set.books() else { continue };
        let ordered = set.ordered();
        let reversed: BTreeSet<&EventHash> = ordered
            .iter()
            .filter_map(|(_, event)| match &event.payload {
                Payload::BookingReversed { reverses, .. } => Some(reverses),
                _ => None,
            })
            .collect();
        let sides = held.entry(books).or_default();
        for (hash, event) in &ordered {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            let Some(transaction) = &booking.transaction else { continue };
            if sides.contains_key(&transaction.id) {
                problems.push(TransactionProblem::Duplicate {
                    books: books.clone(),
                    event: *hash,
                    transaction: transaction.id.clone(),
                });
            } else {
                let side = Side { event: *hash, booking, names: &transaction.books, reversed: reversed.contains(hash) };
                sides.insert(&transaction.id, side);
            }
        }
    }
    for (books, sides) in &held {
        for (transaction, side) in sides {
            for other in side.names.iter().filter(|other| held.contains_key(other)) {
                let Some(counterpart) = held[other].get(transaction) else {
                    problems.push(TransactionProblem::Missing {
                        books: (*books).clone(),
                        event: side.event,
                        other: other.clone(),
                        transaction: (*transaction).clone(),
                    });
                    continue;
                };
                let transaction = (*transaction).clone();
                if !counterpart.names.contains(books) {
                    problems.push(TransactionProblem::BooksMismatch { transaction: transaction.clone() });
                }
                if counterpart.booking.date != side.booking.date {
                    problems.push(TransactionProblem::DateMismatch { transaction: transaction.clone() });
                }
                if counterpart.reversed != side.reversed {
                    problems.push(TransactionProblem::ReversalMismatch { transaction });
                }
            }
        }
    }
    // Each pair of bookings is seen from both sides; report what it shows once.
    problems.sort();
    problems.dedup();
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clock, Configuration, DeviceId, Event, Money, Posting, booking::tests::account, party::tests::party};

    pub(crate) fn books(n: u8) -> BooksId {
        format!("00000000-0000-7000-8000-0000000000{n:02x}").parse().unwrap()
    }

    fn transaction(n: u8) -> TransactionId {
        format!("00000000-0000-7000-8000-0000000001{n:02x}").parse().unwrap()
    }

    fn booking(owner: u8, date: &str, base_units: i128, shared: Option<(u8, &[u8])>) -> Booking {
        let money = |units: i128| Money::new(units, "EUR".parse().unwrap()).unwrap();
        let posting = |n: u8, units: i128| Posting {
            owner: party(owner),
            account: account(n),
            amount: money(units),
            note: None,
            tax: None,
            exchange: None,
        };
        Booking {
            date: date.parse().unwrap(),
            text: "internet".into(),
            postings: vec![posting(1, base_units), posting(2, -base_units)],
            documents: vec![],
            entries: vec![],
            transaction: shared.map(|(id, others)| Transaction {
                id: transaction(id),
                books: others.iter().map(|n| books(*n)).collect(),
            }),
        }
    }

    /// Books on one device, with a clock that moves on by one millisecond per event.
    struct Writer {
        set: EventSet,
        device: DeviceId,
        clock: Clock,
        ms: u64,
    }

    impl Writer {
        fn new(device: &str) -> Self {
            let device: DeviceId = device.parse().unwrap();
            Writer { set: EventSet::new(), clock: Clock::new(device.clone()), device, ms: 1000 }
        }

        fn founded(device: &str, id: u8) -> Self {
            let mut writer = Writer::new(device);
            writer.write(Payload::BooksFounded { books: books(id) });
            writer
        }

        fn write(&mut self, payload: Payload) -> EventHash {
            self.ms += 1;
            let (seq, prev) = self.set.chain_head(&self.device);
            self.set.insert(Event { time: self.clock.now(self.ms), seq, prev, payload }).0
        }

        fn book(&mut self, booking: Booking) -> EventHash {
            self.write(Payload::BookingRecorded { booking })
        }
    }

    #[test]
    fn the_first_founding_gives_the_identity() {
        let mut writer = Writer::new("a");
        writer.book(booking(1, "2026-10-28", 10, None));
        assert_eq!(writer.set.books(), None);
        assert_eq!(writer.set.review_books(), vec![BooksProblem::NotFounded]);
        let first = writer.write(Payload::BooksFounded { books: books(1) });
        assert_eq!(writer.set.books(), Some(&books(1)));
        assert!(writer.set.review_books().is_empty());
        let second = writer.write(Payload::BooksFounded { books: books(2) });
        assert_ne!(first, second);
        assert_eq!(writer.set.books(), Some(&books(1)));
        assert_eq!(writer.set.review_books(), vec![BooksProblem::FoundedTwice { event: second, books: books(2) }]);
        assert!(EventSet::new().review_books().is_empty());
    }

    #[test]
    fn a_transaction_is_written_with_its_books() {
        let booking = booking(1, "2026-10-28", 10, Some((1, &[2])));
        let json = serde_json::to_value(&booking).unwrap();
        assert_eq!(
            json["transaction"],
            serde_json::json!({"id": transaction(1).to_string(), "books": [books(2).to_string()]})
        );
        assert_eq!(serde_json::from_value::<Booking>(json.clone()).unwrap(), booking);
        let mut empty = json;
        empty["transaction"]["books"] = serde_json::json!([]);
        assert!(serde_json::from_value::<Booking>(empty).is_err());
        assert!("not-a-uuid".parse::<TransactionId>().is_err());
        assert!("0192F1E4-0000-7000-8000-000000000001".parse::<BooksId>().is_err());
    }

    #[test]
    fn a_bill_split_between_business_and_private_books() {
        let mut business = Writer::founded("laptop", 1);
        let mut private = Writer::founded("laptop", 2);
        business.book(booking(1, "2026-10-28", 1200, Some((1, &[2]))));
        private.book(booking(2, "2026-10-28", 1200, Some((1, &[1]))));
        assert!(review_transactions(&[&business.set, &private.set]).is_empty());
        // Who holds only the business books cannot check the other side, and is not told it is missing.
        assert!(review_transactions(&[&business.set]).is_empty());
    }

    #[test]
    fn a_missing_side_is_reported() {
        let mut business = Writer::founded("laptop", 1);
        let private = Writer::founded("laptop", 2);
        let event = business.book(booking(1, "2026-10-28", 1200, Some((1, &[2]))));
        assert_eq!(
            review_transactions(&[&business.set, &private.set]),
            vec![TransactionProblem::Missing { books: books(1), event, other: books(2), transaction: transaction(1) }]
        );
    }

    #[test]
    fn sides_that_do_not_fit_are_reported_once() {
        let mut business = Writer::founded("laptop", 1);
        let mut private = Writer::founded("laptop", 2);
        let recorded = business.book(booking(1, "2026-10-28", 1200, Some((1, &[2]))));
        private.book(booking(2, "2026-10-29", 1200, Some((1, &[1]))));
        let reversal = booking(1, "2026-10-30", -1200, Some((2, &[2])));
        business.write(Payload::BookingReversed { reverses: recorded, booking: reversal });
        let other = business.book(booking(1, "2026-10-28", 50, Some((1, &[3]))));
        assert_eq!(
            review_transactions(&[&business.set, &private.set]),
            vec![
                TransactionProblem::Duplicate { books: books(1), event: other, transaction: transaction(1) },
                TransactionProblem::Missing {
                    books: books(1),
                    event: business.set.ordered()[2].0,
                    other: books(2),
                    transaction: transaction(2),
                },
                TransactionProblem::DateMismatch { transaction: transaction(1) },
                TransactionProblem::ReversalMismatch { transaction: transaction(1) },
            ]
        );
        let mut one_sided = Writer::founded("laptop", 3);
        one_sided.book(booking(2, "2026-10-28", 1200, Some((3, &[4]))));
        let mut other_side = Writer::founded("laptop", 4);
        other_side.book(booking(2, "2026-10-28", 1200, Some((3, &[5]))));
        assert_eq!(
            review_transactions(&[&one_sided.set, &other_side.set]),
            vec![TransactionProblem::BooksMismatch { transaction: transaction(3) }]
        );
    }

    fn configuration(owner: u8, from: &str) -> Configuration {
        serde_json::from_value(serde_json::json!({
            "owner": party(owner).to_string(), "from": from, "currency": "EUR", "year_start": 1,
            "accounting": "de-cash-basis",
            "vat": {"scheme": "de-regular", "basis": "accrual", "period": "quarter"},
        }))
        .unwrap()
    }

    #[test]
    fn an_owner_is_configured_from_a_date_on() {
        let mut writer = Writer::founded("laptop", 1);
        writer.write(Payload::OwnerAdded { owner: party(1) });
        let early = writer.book(booking(1, "2025-12-31", 10, None));
        writer.write(Payload::OwnerConfigured { configuration: configuration(1, "2026-01-01") });
        let mut small = configuration(1, "2027-01-01");
        small.vat.as_mut().unwrap().scheme = "de-small-business".parse().unwrap();
        writer.write(Payload::OwnerConfigured { configuration: small.clone() });
        let stranger = writer.write(Payload::OwnerConfigured { configuration: configuration(2, "2026-01-01") });
        writer.book(booking(1, "2026-06-30", 10, None));
        let at = |date: &str| writer.set.configuration(&party(1), date.parse().unwrap());
        assert_eq!(at("2025-12-31"), None);
        assert_eq!(at("2026-12-31"), Some(&configuration(1, "2026-01-01")));
        assert_eq!(at("2027-03-01"), Some(&small));
        let problems: Vec<_> = writer
            .set
            .review_parties()
            .into_iter()
            .filter(|p| !matches!(p, crate::PartyProblem::UnknownOwner { .. }))
            .collect();
        assert_eq!(
            problems,
            vec![
                crate::PartyProblem::ConfigurationOfNonOwner { event: stranger, owner: party(2) },
                crate::PartyProblem::NotConfigured { event: early, owner: party(1) },
            ]
        );
    }

    #[test]
    fn a_configuration_is_read_in_its_one_form() {
        let json = serde_json::to_value(configuration(1, "2026-01-01")).unwrap();
        assert_eq!(json["vat"]["basis"], "accrual");
        let with = |member: &str, value: serde_json::Value| {
            let mut changed = json.clone();
            changed[member] = value;
            serde_json::from_value::<Configuration>(changed)
        };
        assert!(with("year_start", serde_json::json!(12)).is_ok());
        assert!(with("year_start", serde_json::json!(0)).is_err());
        assert!(with("year_start", serde_json::json!(13)).is_err());
        assert!(with("accounting", serde_json::json!("DE-cash")).is_err());
        assert!(with("accounting", serde_json::Value::Null).is_err());
        assert_eq!(with("chart", serde_json::json!("de-skr03")).unwrap().chart.unwrap().as_str(), "de-skr03");
        assert!(with("chart", serde_json::json!("SKR03")).is_err());
        let mut private = json.clone();
        private.as_object_mut().unwrap().remove("accounting");
        private.as_object_mut().unwrap().remove("vat");
        let private: Configuration = serde_json::from_value(private).unwrap();
        assert_eq!((private.accounting, private.vat), (None, None));
    }

    #[test]
    fn a_closed_period_takes_no_late_bookings() {
        let mut writer = Writer::founded("laptop", 1);
        writer.book(booking(1, "2026-03-31", 10, None));
        let state = writer.set.state();
        let closing = Closing {
            owner: party(1),
            to: "2026-03-31".parse().unwrap(),
            state,
            software: BTreeMap::from([("spezado-law-de".into(), "0.1.0".into())]),
        };
        let closed = writer.write(Payload::PeriodClosed { closing });
        assert!(writer.set.review_closings().is_empty());
        assert_eq!(writer.set.closed_until(&party(1)), Some("2026-03-31".parse().unwrap()));
        assert_eq!(writer.set.closed_until(&party(2)), None);
        // In the open period and in another owner's books, bookings are fine.
        writer.book(booking(1, "2026-04-01", 10, None));
        writer.book(booking(2, "2026-03-15", 10, None));
        let late = writer.book(booking(1, "2026-03-15", 10, None));
        let reconfigured = writer.write(Payload::OwnerConfigured { configuration: configuration(1, "2026-01-01") });
        writer.write(Payload::OwnerConfigured { configuration: configuration(1, "2026-04-01") });
        assert_eq!(
            writer.set.review_closings(),
            vec![
                ClosingProblem::LateBooking { event: late, closing: closed },
                ClosingProblem::LateConfiguration { event: reconfigured, closing: closed },
            ]
        );
    }

    #[test]
    fn a_closing_shows_events_it_had_not_seen() {
        let mut laptop = Writer::founded("laptop", 1);
        let mut phone = Writer::new("phone");
        phone.set.merge(&laptop.set);
        // The phone books offline, at a time before the laptop closes; the laptop has not seen it.
        phone.ms = 1001;
        phone.book(booking(1, "2026-03-10", 10, None));
        laptop.ms = 5000;
        let state = laptop.set.state();
        let closed = laptop.write(Payload::PeriodClosed {
            closing: Closing { owner: party(1), to: "2026-03-31".parse().unwrap(), state, software: BTreeMap::new() },
        });
        assert!(laptop.set.review_closings().is_empty());
        laptop.set.merge(&phone.set);
        assert_eq!(laptop.set.review_closings(), vec![ClosingProblem::StateMismatch { closing: closed }]);
    }
}
