// SPDX-License-Identifier: MIT
//! Bank statements (spec part 0005): the account, the balances and the entries of a statement, the references of
//! bookings to its entries, and their review.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Currency, Date, DocumentId, EventHash, EventSet, Money, Object, Payload, booking::Booking, chart::Account,
};

/// The data of a bank statement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatementData {
    /// The account.
    pub account: StatementAccount,
    /// The first day it covers.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub from: Option<Date>,
    /// The last day it covers.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub to: Option<Date>,
    /// The booked balance before the first entry, positive in the holder's favour.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub opening: Option<Money>,
    /// The booked balance after the last entry, positive in the holder's favour.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub closing: Option<Money>,
    /// The entries, in the order of the statement; numbered by position from 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub entries: Vec<StatementEntry>,
}

/// The account a statement is of.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatementAccount {
    /// Its IBAN (ISO 13616).
    pub iban: String,
    /// Its currency.
    pub currency: Currency,
    /// The holder, as the bank names them.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub holder: Option<String>,
}

/// One entry of a statement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatementEntry {
    /// The day it was booked.
    pub booked: Date,
    /// The value date.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub value: Option<Date>,
    /// Positive when credited – money in –, negative when debited.
    pub amount: Money,
    /// Whether the bank marks it as the reversal of an earlier entry; left out when not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not", deserialize_with = "marked")]
    pub reversal: bool,
    /// The bank's own reference.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub reference: Option<String>,
    /// The bank's transaction code, as given.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub code: Option<String>,
    /// The transactions it consists of – usually one.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub details: Vec<EntryDetail>,
}

/// `true`, written only when it is: absent is the one form of `false`.
fn marked<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    if bool::deserialize(deserializer)? {
        Ok(true)
    } else {
        Err(serde::de::Error::custom("false is left out, not written"))
    }
}

/// One transaction of an entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryDetail {
    /// The other party: the payer of money in, the payee of money out.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub counterparty: Option<Counterparty>,
    /// The remittance information.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub purpose: Option<String>,
    /// The payer's end-to-end reference.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub end_to_end: Option<String>,
    /// The reference of a SEPA direct debit mandate.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub mandate: Option<String>,
    /// Its amount, where an entry bundles several.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub amount: Option<Money>,
}

/// The other party of a transaction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Counterparty {
    /// Its name.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub name: Option<String>,
    /// The IBAN of its account.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub iban: Option<String>,
    /// The BIC of its bank.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub bic: Option<String>,
}

/// A reference of a booking to an entry of a bank statement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntryReference {
    /// The statement.
    pub document: DocumentId,
    /// The position of the entry, from 1.
    #[serde(deserialize_with = "position")]
    pub entry: u32,
}

fn position<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let position = u32::deserialize(deserializer)?;
    if position == 0 { Err(serde::de::Error::custom("entries are numbered from 1")) } else { Ok(position) }
}

impl Object for StatementData {
    const TYPE: &'static str = "statement_data";
}

impl Object for StatementAccount {
    const TYPE: &'static str = "statement_account";
}

impl Object for StatementEntry {
    const TYPE: &'static str = "statement_entry";
}

impl Object for EntryDetail {
    const TYPE: &'static str = "entry_detail";
}

impl Object for Counterparty {
    const TYPE: &'static str = "counterparty";
}

impl Object for EntryReference {
    const TYPE: &'static str = "entry_reference";
}

/// A statement read from a file, with what its document needs besides its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadStatement {
    /// The name of the format: `iso20022-camt.053`, `swift-mt940`, `csv`.
    pub format: &'static str,
    /// Always a bank statement.
    pub kind: crate::DocumentKind,
    /// The statement's number, as the file gives it.
    pub number: Option<String>,
    /// The day it was made, or the last day it covers.
    pub date: Date,
    /// Its data.
    pub statement: StatementData,
}

/// Why a statement could not be read from a file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// Not text in the encoding the format uses, or not XML.
    #[error("cannot be read: {0}")]
    Encoding(String),
    /// Not a bank statement in this format.
    #[error("not a bank statement in {0}")]
    Syntax(&'static str),
    /// An element or field the model needs is missing.
    #[error("{0} is missing")]
    Missing(&'static str),
    /// A value that cannot be read.
    #[error("{element} is not valid: {value:?}")]
    Invalid {
        /// The element or field.
        element: &'static str,
        /// The value.
        value: String,
    },
}

/// Reads the statements of a file in whichever format it is: camt.053, MT940, or the CSV file of a bank. `account`
/// is the IBAN of the account for a CSV file that does not name it.
pub fn read_statements(
    bytes: &[u8],
    currencies: &crate::CurrencyTable,
    account: Option<&str>,
) -> Result<Vec<ReadStatement>, ReadError> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.trim_start_matches('\u{feff}').trim_start();
    if start.starts_with('<') {
        crate::camt::read(bytes, currencies)
    } else if start.starts_with(":20:") || start.starts_with("{1:") || start.contains("\n:20:") {
        crate::mt940::read(bytes, currencies)
    } else {
        crate::bank_csv::read(bytes, currencies, account)
    }
}

/// Something about bank statements and the bookings of their entries that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatementProblem {
    /// A reference to a document that is not a recorded bank statement, or has no such entry.
    #[error("event {event} books entry {} of {}, which is not an entry of a recorded statement", .entry.entry, .entry.document)]
    UnknownEntry {
        /// The booking's event.
        event: EventHash,
        /// The reference.
        entry: EntryReference,
    },
    /// Two bookings that stand name the same entry.
    #[error("entry {} of {} is booked again by event {event}", .entry.entry, .entry.document)]
    BookedTwice {
        /// The later booking's event.
        event: EventHash,
        /// The entry.
        entry: EntryReference,
    },
    /// Two statements of the same account show the same entry.
    #[error("entry {} of {} is shown again as entry {} of {}", .first.entry, .first.document, .again.entry, .again.document)]
    ShownTwice {
        /// Where it was shown first.
        first: EntryReference,
        /// Where it is shown again.
        again: EntryReference,
    },
    /// The closing balance of a statement differs from the balance the books show for its account.
    #[error("statement {document} closes at {closing}, the books show {booked}")]
    BalanceMismatch {
        /// The statement.
        document: DocumentId,
        /// Its closing balance.
        closing: Money,
        /// The balance of the account in the books on its last day.
        booked: Money,
    },
}

/// What makes an entry the same as one in another statement: the account, and the bank's reference – or, without
/// one, the day, amount, counterparty and purpose.
fn identity(statement: &StatementData, entry: &StatementEntry) -> (String, String) {
    let account = statement.account.iban.replace(' ', "");
    match &entry.reference {
        Some(reference) => (account, format!("ref:{reference}")),
        None => {
            let detail = entry.details.first();
            let counterparty = detail.and_then(|d| d.counterparty.as_ref());
            let party = counterparty.and_then(|c| c.iban.clone().or_else(|| c.name.clone())).unwrap_or_default();
            let purpose = detail.and_then(|d| d.purpose.clone()).unwrap_or_default();
            (
                account,
                format!("{}|{}|{}|{party}|{purpose}", entry.booked, entry.amount.base_units(), entry.amount.currency()),
            )
        }
    }
}

impl EventSet {
    /// The bank statements recorded, by document, in the agreed order.
    pub fn statements(&self) -> Vec<(DocumentId, &StatementData)> {
        self.ordered()
            .into_iter()
            .filter_map(|(_, event)| match &event.payload {
                Payload::DocumentRecorded { document } => {
                    document.statement.as_deref().map(|statement| (document.id.clone(), statement))
                }
                _ => None,
            })
            .collect()
    }

    /// The entries booked by bookings that stand – neither reversed nor reversals – with the event that booked them.
    pub fn booked_entries(&self) -> BTreeMap<EntryReference, EventHash> {
        let reversed: BTreeSet<EventHash> = self
            .ordered()
            .into_iter()
            .filter_map(|(_, event)| match &event.payload {
                Payload::BookingReversed { reverses, .. } => Some(*reverses),
                _ => None,
            })
            .collect();
        let mut booked = BTreeMap::new();
        for (hash, event) in self.ordered() {
            if let Payload::BookingRecorded { booking } = &event.payload
                && !reversed.contains(&hash)
            {
                for entry in &booking.entries {
                    booked.entry(entry.clone()).or_insert(hash);
                }
            }
        }
        booked
    }

    /// Checks the statements and the bookings of their entries: every entry booked is an entry of a recorded
    /// statement and booked once, no entry is shown by two statements, and each statement closes at the balance the
    /// books show for its account – the one account opened with its IBAN.
    pub fn review_statements(&self) -> Vec<StatementProblem> {
        let statements: BTreeMap<DocumentId, &StatementData> = self.statements().into_iter().collect();
        let mut problems = Vec::new();
        // Bookings: known entries, each booked once by bookings that stand.
        let reversed: BTreeSet<EventHash> = self
            .ordered()
            .into_iter()
            .filter_map(|(_, event)| match &event.payload {
                Payload::BookingReversed { reverses, .. } => Some(*reverses),
                _ => None,
            })
            .collect();
        let mut booked: BTreeSet<&EntryReference> = BTreeSet::new();
        for (hash, event) in self.ordered() {
            let (booking, stands): (&Booking, bool) = match &event.payload {
                Payload::BookingRecorded { booking } => (booking, !reversed.contains(&hash)),
                Payload::BookingReversed { booking, .. } => (booking, false),
                _ => continue,
            };
            for entry in &booking.entries {
                let known = statements.get(&entry.document).is_some_and(|s| entry.entry as usize <= s.entries.len());
                if !known {
                    problems.push(StatementProblem::UnknownEntry { event: hash, entry: entry.clone() });
                } else if stands && !booked.insert(entry) {
                    problems.push(StatementProblem::BookedTwice { event: hash, entry: entry.clone() });
                }
            }
        }
        // Entries shown by two statements.
        let mut seen: BTreeMap<(String, String), EntryReference> = BTreeMap::new();
        for (document, statement) in &statements {
            for (index, entry) in statement.entries.iter().enumerate() {
                let here = EntryReference { document: document.clone(), entry: index as u32 + 1 };
                match seen.get(&identity(statement, entry)) {
                    Some(first) if first.document != here.document => {
                        problems.push(StatementProblem::ShownTwice { first: first.clone(), again: here });
                    }
                    Some(_) => {}
                    None => {
                        seen.insert(identity(statement, entry), here);
                    }
                }
            }
        }
        // Closing balances against the books.
        let mut by_iban: BTreeMap<String, Vec<&Account>> = BTreeMap::new();
        for (_, event) in self.ordered() {
            if let Payload::AccountOpened { account } = &event.payload
                && let Some(iban) = account.attributes.get("iban")
            {
                by_iban.entry(iban.replace(' ', "")).or_default().push(account);
            }
        }
        for (document, statement) in &statements {
            let (Some(closing), Some(to)) = (&statement.closing, statement.to) else { continue };
            let Some([account]) = by_iban.get(&statement.account.iban.replace(' ', "")).map(Vec::as_slice) else {
                continue;
            };
            let Ok(balances) = self.balances(Some(to)) else { continue };
            let units: i128 = balances
                .iter()
                .filter(|(_, id, currency, _)| **id == account.id && **currency == statement.account.currency)
                .map(|(.., units)| units)
                .sum();
            let booked = Money::new(units, statement.account.currency.clone()).expect("a balance");
            if booked != *closing {
                problems.push(StatementProblem::BalanceMismatch {
                    document: document.clone(),
                    closing: closing.clone(),
                    booked,
                });
            }
        }
        problems
    }
}
