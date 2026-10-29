// SPDX-License-Identifier: MIT
//! Reference library of librotenado, the open protocol for accounting data.
//!
//! Version 0.1 covers the base every other part builds on: amounts as exact counts of a currency's smallest unit,
//! calendar dates, bookings that must balance, and events – immutable, ordered by a hybrid logical clock, chained per
//! device and identified by the hash of their canonical encoding – that a set of books is made of. Two sets of events
//! merge by union; what cannot be merged silently is reported, never guessed. Around them: the harmonised chart of
//! accounts with translations of national charts, documents with their originals, and the organisations and persons
//! the books belong to.

/// Deserialises an optional member that, when present, is not `null` – absent is the one form of "none".
pub(crate) fn present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// Deserialises a map that, when present, is not empty – absent is the one form of "none".
pub(crate) fn non_empty_map<'de, D, K, V>(deserializer: D) -> Result<std::collections::BTreeMap<K, V>, D::Error>
where
    D: serde::Deserializer<'de>,
    K: serde::Deserialize<'de> + Ord,
    V: serde::Deserialize<'de>,
{
    let map = <std::collections::BTreeMap<K, V> as serde::Deserialize>::deserialize(deserializer)?;
    if map.is_empty() { Err(serde::de::Error::custom("an empty map is left out, not written")) } else { Ok(map) }
}

/// Deserialises an optional text that, when present, is neither `null` nor empty – absent is the one form of "none".
pub(crate) fn present_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let text = <String as serde::Deserialize>::deserialize(deserializer)?;
    if text.is_empty() {
        Err(serde::de::Error::custom("an empty text is left out, not written"))
    } else {
        Ok(Some(text))
    }
}

/// Whether `s` is a UUID in its one form: lower-case hexadecimal digits in groups of 8-4-4-4-12.
pub(crate) fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

/// The 32 bytes of a SHA-256 hash in its one written form: `sha256:` and 64 lower-case hexadecimal digits.
pub(crate) fn parse_sha256(s: &str) -> Option<[u8; 32]> {
    let hex = s.strip_prefix("sha256:")?;
    if hex.len() != 64 || hex.bytes().any(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let mut bytes = [0u8; 32];
    hex::decode_to_slice(hex, &mut bytes).ok()?;
    Some(bytes)
}

/// Deserialises a list that, when present, is not empty – absent is the one form of "none".
pub(crate) fn non_empty<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    let items = <Vec<T> as serde::Deserialize>::deserialize(deserializer)?;
    if items.is_empty() { Err(serde::de::Error::custom("an empty list is left out, not written")) } else { Ok(items) }
}

/// Deserialises an integer that the canonical encoding can represent (at most 2^53 − 1).
pub(crate) fn safe_u64<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = <u64 as serde::Deserialize>::deserialize(deserializer)?;
    if value > MAX_SAFE_INTEGER {
        return Err(serde::de::Error::custom(format!("{value} is larger than 2^53 - 1")));
    }
    Ok(value)
}

mod balance;
pub mod bank_csv;
mod booking;
mod books;
pub mod camt;
mod canonical;
mod chart;
mod clock;
mod date;
mod decimal;
mod document;
pub mod en16931;
mod encryption;
mod event;
mod filing;
pub mod gobd;
mod invoice;
mod money;
pub mod mt940;
mod names;
mod object;
mod party;
mod report;
mod statement;
mod tax;

pub use balance::{BalanceError, Balances};
pub use booking::{AccountId, Booking, BookingError, Exchange, Posting};
pub use books::{
    BooksId, BooksProblem, Closing, ClosingProblem, Transaction, TransactionId, TransactionProblem, review_transactions,
};
pub use canonical::{CanonicalError, MAX_SAFE_INTEGER, canonicalize, to_canonical_vec};
pub use chart::{Account, AccountKind, Chart, ChartError, ChartTranslation, TranslationError};
pub use clock::{Clock, DeviceId, Hlc};
pub use date::Date;
pub use decimal::Decimal;
pub use document::{ContentHash, Document, DocumentId, DocumentKind, Original};
pub use encryption::{
    BooksKey, Bytes, EncryptionError, Envelope, EnvelopeContent, Grant, Invitation, JoinRequest, KeyChange, Member,
    MemberId, MemberKeys, MemberList, MemberRecord, MemberRole, ReceivingKey, RecordProblem, RecoveryCode, Rights,
    request_signature, verify, verify_request,
};
pub use event::{
    AccountProblem, BookingProblem, ChainProblem, DocumentProblem, Event, EventHash, EventSet, PartyProblem, Payload,
};
pub use filing::{Derivation, Filing, FilingId, FilingKind, FilingProblem, FilingState, ReportContent};
pub use invoice::{
    AllowanceCharge, Classification, CreditTransfer, Delivery, DirectDebit, Endpoint, Identifier, InvoiceContact,
    InvoiceData, InvoiceLine, InvoiceNote, InvoiceParty, InvoiceReferences, InvoiceSide, InvoiceTotals, ItemAttribute,
    Payment, PaymentCard, Period, PostalAddress, PrecedingInvoice, ProposalAccounts, ProposalError, SupportingDocument,
    VatBreakdown, VatCategory, de_tax_code, propose_booking,
};
pub use money::{Currency, CurrencyDefinition, CurrencyTable, DefinitionError, Money};
pub use names::Names;
pub use object::{Object, ObjectError, from_root, to_root};
pub use party::{
    Configuration, Country, Organisation, PartyId, Person, RegimeCode, Region, Registration, ReturnPeriod, Role,
    RoleKind, Shares, TaxId, VatBasis, VatScheme,
};
pub use report::{MappingStatus, ReportMapping, TaxRule, VatAccounts, VatSettlement};
pub use statement::{
    Counterparty, EntryDetail, EntryReference, ReadError, ReadStatement, StatementAccount, StatementData,
    StatementEntry, StatementProblem, read_statements,
};
pub use tax::{TaxCode, TaxCodeDefinition, TaxDirection, TaxTable};

/// A value in the protocol that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// A decimal number not in its one form, or with more than [`Decimal::MAX_SCALE`] places.
    #[error("not a decimal number: {0:?}")]
    Decimal(String),
    /// A currency code that is neither three upper-case letters (ISO 4217) nor `x-` and a name.
    #[error("not a currency code: {0:?}")]
    Currency(String),
    /// A date that is not a valid `YYYY-MM-DD`.
    #[error("not a date: {0:?}")]
    Date(String),
    /// A device name that is not 1–64 characters of `a-z`, `0-9` and `-`.
    #[error("not a device id: {0:?}")]
    Device(String),
    /// An account identity that is not a UUID in lower case (`8-4-4-4-12` hexadecimal digits).
    #[error("not an account id: {0:?}")]
    Account(String),
    /// A document identity that is not a UUID in lower case.
    #[error("not a document id: {0:?}")]
    Document(String),
    /// A party identity that is not a UUID in lower case.
    #[error("not a party id: {0:?}")]
    Party(String),
    /// A country that is not two upper-case letters (ISO 3166-1 alpha-2).
    #[error("not a country code: {0:?}")]
    Country(String),
    /// A region that is not a country, `-` and one to three upper-case letters or digits (ISO 3166-2).
    #[error("not a region code: {0:?}")]
    Region(String),
    /// A tax code that is not a country in lower case, `-` and a name.
    #[error("not a tax code: {0:?}")]
    TaxCode(String),
    /// A filing identity that is not a UUID in lower case.
    #[error("not a filing id: {0:?}")]
    Filing(String),
    /// An identity of a set of books that is not a UUID in lower case.
    #[error("not a books id: {0:?}")]
    Books(String),
    /// A transaction identity that is not a UUID in lower case.
    #[error("not a transaction id: {0:?}")]
    Transaction(String),
    /// A code of a configuration that is not a country in lower case, `-` and a name.
    #[error("not a configuration code: {0:?}")]
    RegimeCode(String),
    /// A hash that is not `sha256:` and 64 lower-case hexadecimal digits.
    #[error("not a hash: {0:?}")]
    Hash(String),
}
