// SPDX-License-Identifier: MIT
//! Two devices record bookings, exchange their events and arrive at the same books.

use librotenado::{Booking, Clock, DeviceId, Event, EventSet, Money, Payload, Posting};

// The organisation or person whose books these are, and the accounts – all identified by UUIDs. What an account is
// follows from the chart, its name from translation data.
const OWNER: &str = "0192f1e4-0000-7000-8000-0000000000a1";
const CASH: &str = "0192f1e4-0000-7000-8000-000000000001";
const OFFICE: &str = "0192f1e4-0000-7000-8000-000000000002";
const TRAVEL: &str = "0192f1e4-0000-7000-8000-000000000003";

/// A booking of `cents` from cash to an expense account – money counts the currency's smallest unit.
fn booking(text: &str, account: &str, cents: i128) -> Booking {
    let money = |base_units: i128| Money::new(base_units, "EUR".parse().unwrap()).expect("within range");
    Booking {
        date: "2026-10-24".parse().unwrap(),
        text: text.into(),
        postings: vec![
            Posting {
                owner: OWNER.parse().unwrap(),
                account: account.parse().unwrap(),
                amount: money(cents),
                note: None,
                tax: None,
                exchange: None,
            },
            Posting {
                owner: OWNER.parse().unwrap(),
                account: CASH.parse().unwrap(),
                amount: money(-cents),
                note: None,
                tax: None,
                exchange: None,
            },
        ],
        documents: vec![],
    }
}

/// Appends a booking to the chain of `device` in `books`.
fn record(books: &mut EventSet, device: &DeviceId, clock: &mut Clock, wall_ms: u64, booking: Booking) {
    booking.validate().expect("debit and credit balance");
    let (seq, prev) = books.chain_head(device);
    let event = Event { time: clock.now(wall_ms), seq, prev, payload: Payload::BookingRecorded { booking } };
    books.insert(event);
}

fn main() {
    let (mut laptop, mut phone) = (EventSet::new(), EventSet::new());
    let (laptop_id, phone_id): (DeviceId, DeviceId) = ("laptop".parse().unwrap(), "phone".parse().unwrap());
    let (mut laptop_clock, mut phone_clock) = (Clock::new(laptop_id.clone()), Clock::new(phone_id.clone()));

    record(&mut laptop, &laptop_id, &mut laptop_clock, 1_000, booking("Office paper", OFFICE, 1240));
    record(&mut phone, &phone_id, &mut phone_clock, 1_500, booking("Train ticket", TRAVEL, 2990));

    // Synchronisation is a union of sets: order and repetition do not matter. Events travel as their canonical
    // bytes and are hashed as received.
    for (hash, _) in phone.ordered() {
        laptop.receive(phone.canonical(&hash).expect("in the set")).expect("an event");
    }
    phone.merge(&laptop);
    assert_eq!(laptop, phone);
    assert!(laptop.verify().is_empty(), "no forks, gaps or broken links");

    // Each event is identified by the SHA-256 hash of its canonical encoding (RFC 8785).
    for (hash, _) in laptop.ordered() {
        let canonical = laptop.canonical(&hash).expect("in the set");
        println!("{hash}\n{}\n", String::from_utf8_lossy(canonical));
    }
}
