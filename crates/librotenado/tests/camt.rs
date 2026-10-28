// SPDX-License-Identifier: MIT
//! Bank statements in camt.053: read in both versions, recorded, booked entry by entry, and reviewed.

use std::collections::BTreeMap;

use librotenado::{
    Account, AccountKind, Booking, ChartTranslation, Clock, CurrencyTable, DeviceId, Document, EntryReference, Event,
    EventHash, EventSet, Money, Original, Payload, Posting, StatementProblem, camt, from_root, to_root,
};

const OCTOBER: &[u8] = include_bytes!("data/camt/statement-001.08.xml");
const MID_OCTOBER: &[u8] = include_bytes!("data/camt/statement-001.02.xml");

fn eur(units: i128) -> Money {
    Money::new(units, "EUR".parse().unwrap()).unwrap()
}

#[test]
fn version_08_is_read_with_balances_entries_and_their_details() {
    let statements = camt::read(OCTOBER, &CurrencyTable::v0_1()).unwrap();
    assert_eq!(statements.len(), 1);
    let read = &statements[0];
    assert_eq!(
        (read.format, read.number.as_str(), read.date.to_string()),
        ("iso20022-camt.053", "2026-10-001", "2026-11-01".into())
    );
    let statement = &read.statement;
    assert_eq!(statement.account.iban, "DE89370400440532013000");
    assert_eq!(statement.account.holder.as_deref(), Some("Ada Beispiel"));
    assert_eq!(
        (statement.from.unwrap().to_string(), statement.to.unwrap().to_string()),
        ("2026-10-01".into(), "2026-10-31".into())
    );
    assert_eq!((statement.opening.clone(), statement.closing.clone()), (Some(eur(100_000)), Some(eur(316_050))));
    // The pending entry is left out.
    let amounts: Vec<i128> = statement.entries.iter().map(|e| e.amount.base_units()).collect();
    assert_eq!(amounts, [238_000, -5_950, -16_000]);
    let income = &statement.entries[0];
    assert_eq!((income.reference.as_deref(), income.code.as_deref()), (Some("2026100500001"), Some("PMNT/RCDT/ESCT")));
    let detail = &income.details[0];
    let counterparty = detail.counterparty.as_ref().unwrap();
    assert_eq!(
        (counterparty.name.as_deref(), counterparty.iban.as_deref(), counterparty.bic.as_deref()),
        (Some("Kunde A GmbH"), Some("DE02120300000000202051"), Some("BYLADEM1001"))
    );
    assert_eq!(detail.purpose.as_deref(), Some("Rechnung RE-2026-001 Honorar September"));
    assert_eq!(detail.end_to_end.as_deref(), Some("RE-2026-001"));
    let debit = &statement.entries[1].details[0];
    assert_eq!((debit.end_to_end.as_deref(), debit.mandate.as_deref()), (None, Some("TK-4711")));
    assert_eq!(debit.counterparty.as_ref().unwrap().name.as_deref(), Some("Telefon AG"), "the creditor of money out");
    let batch: Vec<Option<i128>> =
        statement.entries[2].details.iter().map(|d| d.amount.as_ref().map(Money::base_units)).collect();
    assert_eq!(batch, [Some(-10_000), Some(-6_000)]);
}

#[test]
fn version_02_is_read_too() {
    let statements = camt::read(MID_OCTOBER, &CurrencyTable::v0_1()).unwrap();
    let entry = &statements[0].statement.entries[0];
    assert_eq!(entry.amount, eur(-5_950));
    let counterparty = entry.details[0].counterparty.as_ref().unwrap();
    assert_eq!((counterparty.name.as_deref(), counterparty.bic.as_deref()), (Some("Telefon AG"), Some("COBADEFFXXX")));
    assert!(matches!(
        camt::read(b"<Document><Other/></Document>", &CurrencyTable::v0_1()),
        Err(camt::CamtError::Syntax)
    ));
}

/// Books on one device.
struct Desk {
    set: EventSet,
    clock: Clock,
    device: DeviceId,
}

impl Desk {
    fn write(&mut self, payload: Payload) -> EventHash {
        let (seq, prev) = self.set.chain_head(&self.device);
        self.set.insert(Event { time: self.clock.now(1_000 + seq), seq, prev, payload }).0
    }
}

fn document(id: &str, bytes: &[u8]) -> Document {
    let read = camt::read(bytes, &CurrencyTable::v0_1()).unwrap().remove(0);
    let mut original = Original::of(bytes, "application/xml");
    original.format = Some(read.format.into());
    Document {
        id: id.parse().unwrap(),
        kind: read.kind,
        number: Some(read.number),
        date: read.date,
        received: read.date,
        original,
        invoice: None,
        statement: Some(Box::new(read.statement)),
    }
}

#[test]
fn entries_are_booked_once_and_statements_close_at_the_books() {
    let skr03 = ChartTranslation::de_skr03();
    let bank_account: librotenado::AccountId = "0192f1e4-0000-7000-8000-00000000b001".parse().unwrap();
    let device: DeviceId = "laptop".parse().unwrap();
    let mut desk = Desk { set: EventSet::new(), clock: Clock::new(device.clone()), device };
    let attributes = BTreeMap::from([("iban".to_owned(), "DE89 3704 0044 0532 0130 00".to_owned())]);
    desk.write(Payload::AccountOpened {
        account: Account {
            id: bank_account.clone(),
            kind: AccountKind::Asset,
            parent: Some(skr03.accounts["1200"].clone()),
            attributes,
        },
    });
    let october = document("0192f1e4-0000-7000-8000-0000000000d1", OCTOBER);
    let mid = document("0192f1e4-0000-7000-8000-0000000000d2", MID_OCTOBER);
    // A statement in the form of the protocol: written and read back unchanged.
    assert_eq!(from_root::<Document>(&to_root(&october).unwrap()).unwrap(), october);
    desk.write(Payload::DocumentRecorded { document: october.clone() });
    desk.write(Payload::DocumentRecorded { document: mid.clone() });
    let entry = |document: &Document, entry: u32| EntryReference { document: document.id.clone(), entry };
    let booking = |date: &str, number: &str, units: i128, entries: Vec<EntryReference>| Booking {
        date: date.parse().unwrap(),
        text: "test".into(),
        postings: vec![
            Posting {
                owner: owner_of(),
                account: bank_account.clone(),
                amount: eur(units),
                note: None,
                tax: None,
                exchange: None,
            },
            Posting {
                owner: owner_of(),
                account: skr03.accounts[number].clone(),
                amount: eur(-units),
                note: None,
                tax: None,
                exchange: None,
            },
        ],
        documents: vec![],
        entries,
        transaction: None,
    };
    desk.write(Payload::BookingRecorded { booking: booking("2026-09-30", "0800", 100_000, vec![]) });
    desk.write(Payload::BookingRecorded { booking: booking("2026-10-05", "8400", 238_000, vec![entry(&october, 1)]) });
    let telephone = desk
        .write(Payload::BookingRecorded { booking: booking("2026-10-15", "4920", -5_950, vec![entry(&october, 2)]) });
    // The same debit, shown again by the statement of 15 October: reported, not to be booked again.
    let shown_twice = StatementProblem::ShownTwice { first: entry(&october, 2), again: entry(&mid, 1) };
    let problems = desk.set.review_statements();
    assert!(problems.contains(&shown_twice), "{problems:?}");
    // October closes at 3,160.50; the batch of 160.00 is not booked yet.
    assert!(problems.contains(&StatementProblem::BalanceMismatch {
        document: october.id.clone(),
        closing: eur(316_050),
        booked: eur(332_050),
    }));
    desk.write(Payload::BookingRecorded { booking: booking("2026-10-20", "4930", -16_000, vec![entry(&october, 3)]) });
    assert_eq!(desk.set.review_statements(), std::slice::from_ref(&shown_twice));
    assert_eq!(desk.set.booked_entries().len(), 3);

    // Booked again – and once more, of an entry no statement has.
    let again =
        desk.write(Payload::BookingRecorded { booking: booking("2026-10-15", "4920", -1, vec![entry(&october, 2)]) });
    let unknown =
        desk.write(Payload::BookingRecorded { booking: booking("2026-10-15", "4920", -1, vec![entry(&october, 9)]) });
    let problems = desk.set.review_statements();
    assert!(problems.contains(&StatementProblem::BookedTwice { event: again, entry: entry(&october, 2) }));
    assert!(problems.contains(&StatementProblem::UnknownEntry { event: unknown, entry: entry(&october, 9) }));
    // A reversed booking frees its entry.
    let original = match &desk.set.get(&telephone).unwrap().payload {
        Payload::BookingRecorded { booking } => booking.clone(),
        _ => unreachable!(),
    };
    desk.write(Payload::BookingReversed {
        reverses: telephone,
        booking: original.reversed("2026-10-16".parse().unwrap(), "wrong".into()),
    });
    assert!(!desk.set.review_statements().iter().any(|p| matches!(p, StatementProblem::BookedTwice { .. })));
}

fn owner_of() -> librotenado::PartyId {
    "0192f1e4-0000-7000-8000-0000000000a1".parse().unwrap()
}
