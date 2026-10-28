// SPDX-License-Identifier: MIT
//! Bookings: postings on accounts that balance in every currency.

use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Currency, Date, DocumentId, Money, Object, ParseError, PartyId, TaxCode};

/// The identity of an account: a UUID (RFC 9562) in its one form, lower case `8-4-4-4-12` hexadecimal digits, e.g.
/// `0192f1e4-7a3c-7b1e-9c2d-5e8f0a1b2c3d`. It carries no meaning – what an account is follows from its kind, parent
/// and attributes, its name from translation data – so it never has to change. Version 7 is recommended.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AccountId(String);

impl AccountId {
    /// The account as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AccountId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(AccountId(s.to_owned())) } else { Err(ParseError::Account(s.to_owned())) }
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for AccountId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AccountId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// One line of a booking: an amount on an account. Positive is debit, negative is credit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Posting {
    /// The organisation or person whose books the posting is in (spec part 0008).
    pub owner: PartyId,
    /// The account.
    pub account: AccountId,
    /// The amount; positive is debit, negative is credit.
    pub amount: Money,
    /// A remark on the posting – for example why there is no document for it; left out when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub note: Option<String>,
    /// How the posting is treated for VAT, as a tax code of its country (`de-out-19`); left out when it is not.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub tax: Option<TaxCode>,
}

/// A booking: a business transaction on a date, as postings that balance for every owner in every currency. One
/// booking may concern several owners – an invoice split between a business and its owner's private books.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Booking {
    /// The date the transaction belongs to.
    pub date: Date,
    /// What it is about.
    pub text: String,
    /// At least two postings.
    pub postings: Vec<Posting>,
    /// The documents it is based on (receipts, invoices, statements); left out of JSON when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub documents: Vec<DocumentId>,
}

impl Object for Posting {
    const TYPE: &'static str = "posting";
}

impl Object for Booking {
    const TYPE: &'static str = "booking";
}

/// Why a booking is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookingError {
    /// Fewer than two postings.
    #[error("a booking needs at least two postings")]
    TooFewPostings,
    /// A posting of zero.
    #[error("posting {0} is zero")]
    ZeroPosting(usize),
    /// Debit and credit differ for an owner in a currency.
    #[error("debit and credit of {owner} differ by {difference}")]
    Unbalanced {
        /// The owner whose postings do not balance.
        owner: PartyId,
        /// By how much.
        difference: Money,
    },
    /// The sums do not fit the number range.
    #[error("the amounts are too large")]
    Overflow,
}

impl Booking {
    /// Checks that the booking has at least two postings, none of them zero, and balances for every owner in every
    /// currency – each owner's books stay balanced on their own.
    pub fn validate(&self) -> Result<(), BookingError> {
        if self.postings.len() < 2 {
            return Err(BookingError::TooFewPostings);
        }
        let mut sums: BTreeMap<(&PartyId, Currency), i128> = BTreeMap::new();
        for (i, posting) in self.postings.iter().enumerate() {
            if posting.amount.base_units() == 0 {
                return Err(BookingError::ZeroPosting(i));
            }
            let sum = sums.entry((&posting.owner, posting.amount.currency().clone())).or_insert(0);
            *sum = sum.checked_add(posting.amount.base_units()).ok_or(BookingError::Overflow)?;
        }
        match sums.into_iter().find(|&(_, sum)| sum != 0) {
            Some(((owner, currency), sum)) => match Money::new(sum, currency) {
                Some(difference) => Err(BookingError::Unbalanced { owner: owner.clone(), difference }),
                None => Err(BookingError::Overflow),
            },
            None => Ok(()),
        }
    }

    /// The booking that cancels this one: same accounts, opposite amounts.
    pub fn reversed(&self, date: Date, text: String) -> Booking {
        let postings = self
            .postings
            .iter()
            .map(|p| Posting {
                owner: p.owner.clone(),
                account: p.account.clone(),
                amount: -p.amount.clone(),
                note: None,
                tax: p.tax.clone(),
            })
            .collect();
        Booking { date, text, postings, documents: self.documents.clone() }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A test account: `n` in the last digits of a UUID.
    pub(crate) fn account(n: u8) -> AccountId {
        format!("00000000-0000-7000-8000-{n:012x}").parse().unwrap()
    }

    fn posting(n: u8, base_units: i128, currency: &str) -> Posting {
        let amount = Money::new(base_units, currency.parse().unwrap()).unwrap();
        Posting { owner: crate::party::tests::party(1), account: account(n), amount, note: None, tax: None }
    }

    #[test]
    fn every_owner_must_balance_on_their_own() {
        // An internet bill of 50 euros: 30 for the business, 20 private, each paid from its own bank account.
        let mut business = vec![posting(1, 3000, "EUR"), posting(3, -3000, "EUR")];
        let mut private = vec![posting(2, 2000, "EUR"), posting(4, -2000, "EUR")];
        for p in &mut private {
            p.owner = crate::party::tests::party(2);
        }
        let mut postings = business.clone();
        postings.extend(private.clone());
        assert_eq!(booking(postings).validate(), Ok(()));

        // The business pays it all: balanced in total, but not for each owner.
        business[1].amount = Money::new(-5000, "EUR".parse().unwrap()).unwrap();
        private.pop();
        business.extend(private);
        assert!(matches!(
            booking(business).validate(),
            Err(BookingError::Unbalanced { owner, .. }) if owner == crate::party::tests::party(1)
        ));
    }

    #[test]
    fn account_identities_are_uuids_in_lower_case() {
        let id = "0192f1e4-7a3c-7b1e-9c2d-5e8f0a1b2c3d";
        assert_eq!(id.parse::<AccountId>().unwrap().as_str(), id);
        for bad in ["0192F1E4-7A3C-7B1E-9C2D-5E8F0A1B2C3D", "0192f1e47a3c7b1e9c2d5e8f0a1b2c3d", "bank", ""] {
            assert!(bad.parse::<AccountId>().is_err(), "{bad}");
        }
    }

    fn booking(postings: Vec<Posting>) -> Booking {
        Booking { date: "2026-10-24".parse().unwrap(), text: "test".into(), postings, documents: vec![] }
    }

    #[test]
    fn a_balanced_booking_is_valid() {
        let b = booking(vec![posting(1, 8403, "EUR"), posting(2, 1597, "EUR"), posting(3, -10000, "EUR")]);
        assert_eq!(b.validate(), Ok(()));
    }

    #[test]
    fn every_currency_must_balance_on_its_own() {
        let b = booking(vec![posting(4, -10000, "EUR"), posting(5, 9500, "CHF")]);
        assert!(matches!(b.validate(), Err(BookingError::Unbalanced { .. })));
        let b = booking(vec![
            posting(4, -10000, "EUR"),
            posting(6, 10000, "EUR"),
            posting(6, -9500, "CHF"),
            posting(5, 9500, "CHF"),
        ]);
        assert_eq!(b.validate(), Ok(()));
    }

    #[test]
    fn rejects_single_and_zero_postings() {
        assert_eq!(booking(vec![posting(7, 100, "EUR")]).validate(), Err(BookingError::TooFewPostings));
        let b = booking(vec![posting(7, 0, "EUR"), posting(8, 0, "EUR")]);
        assert_eq!(b.validate(), Err(BookingError::ZeroPosting(0)));
    }

    #[test]
    fn a_reversal_cancels_the_booking() {
        let b = booking(vec![posting(7, 1250, "EUR"), posting(8, -1250, "EUR")]);
        let r = b.reversed("2026-10-25".parse().unwrap(), "reversal".into());
        assert_eq!(r.validate(), Ok(()));
        assert_eq!(r.postings[0].amount.base_units(), -1250);
    }

    #[test]
    fn sums_beyond_the_range_are_an_overflow() {
        let b = booking(vec![
            posting(7, Money::MAX_BASE_UNITS, "EUR"),
            posting(8, Money::MAX_BASE_UNITS, "EUR"),
            posting(9, 1, "EUR"),
        ]);
        assert_eq!(b.validate(), Err(BookingError::Overflow));
    }
}
