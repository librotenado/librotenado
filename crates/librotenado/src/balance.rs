// SPDX-License-Identifier: MIT
//! Balances (spec part 0004): what the postings of a set of books add up to, per owner, account and currency.

use std::collections::BTreeMap;

use crate::{AccountId, Chart, Currency, Date, EventSet, PartyId, Payload};

/// Balances: per owner, account and currency, the sum of the base units of the postings. Positive is a debit
/// balance, negative a credit balance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Balances(BTreeMap<(PartyId, AccountId, Currency), i128>);

/// Why balances cannot be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BalanceError {
    /// A sum does not fit the number range.
    #[error("the balances are too large")]
    Overflow,
}

impl Balances {
    /// The balance of `account` in the books of `owner` in `currency`; zero where nothing was posted.
    pub fn get(&self, owner: &PartyId, account: &AccountId, currency: &Currency) -> i128 {
        self.0.get(&(owner.clone(), account.clone(), currency.clone())).copied().unwrap_or(0)
    }

    /// Every balance: owner, account, currency, base units – in that order.
    pub fn iter(&self) -> impl Iterator<Item = (&PartyId, &AccountId, &Currency, i128)> {
        self.0.iter().map(|((owner, account, currency), units)| (owner, account, currency, *units))
    }

    /// The balances with every account's balance also added to each account it refines, so that an account's
    /// balance includes all accounts below it. `parents` maps an account to the account it refines.
    pub fn rolled_up(&self, parents: &BTreeMap<AccountId, AccountId>) -> Result<Balances, BalanceError> {
        let mut rolled = BTreeMap::new();
        for ((owner, account, currency), units) in &self.0 {
            let mut next = Some(account);
            let mut steps = 0;
            while let Some(id) = next {
                let sum: &mut i128 = rolled.entry((owner.clone(), id.clone(), currency.clone())).or_default();
                *sum = sum.checked_add(*units).ok_or(BalanceError::Overflow)?;
                next = parents.get(id);
                steps += 1;
                if steps > parents.len() {
                    break; // a cycle; the review of accounts reports it
                }
            }
        }
        Ok(Balances(rolled))
    }
}

impl EventSet {
    /// The balances of all bookings – recorded and reversing – dated on or before `until`, or of all bookings.
    pub fn balances(&self, until: Option<Date>) -> Result<Balances, BalanceError> {
        let mut sums: BTreeMap<(PartyId, AccountId, Currency), i128> = BTreeMap::new();
        for (_, event) in self.ordered() {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            if until.is_some_and(|until| booking.date > until) {
                continue;
            }
            for posting in &booking.postings {
                let key = (posting.owner.clone(), posting.account.clone(), posting.amount.currency().clone());
                let sum = sums.entry(key).or_default();
                *sum = sum.checked_add(posting.amount.base_units()).ok_or(BalanceError::Overflow)?;
            }
        }
        Ok(Balances(sums))
    }

    /// Which account each account refines: the accounts of `chart` and those opened in the books.
    pub fn account_parents(&self, chart: &Chart) -> BTreeMap<AccountId, AccountId> {
        let mut parents: BTreeMap<AccountId, AccountId> =
            chart.accounts.iter().filter_map(|account| Some((account.id.clone(), account.parent.clone()?))).collect();
        for (_, event) in self.ordered() {
            if let Payload::AccountOpened { account } = &event.payload
                && let Some(parent) = &account.parent
            {
                parents.entry(account.id.clone()).or_insert_with(|| parent.clone());
            }
        }
        parents
    }

    /// A trial balance: the balances on or before `until`, rolled up the hierarchy of `chart` and the opened
    /// accounts.
    pub fn trial_balance(&self, chart: &Chart, until: Option<Date>) -> Result<Balances, BalanceError> {
        self.balances(until)?.rolled_up(&self.account_parents(chart))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Account, AccountKind, Booking, Clock, Event, Money, Posting, booking::tests::account, party::tests::party,
    };

    fn eur() -> Currency {
        "EUR".parse().unwrap()
    }

    /// A booking on `date` of `cents` from account 2 (bank) to account `to`, for owner 1.
    fn booking(date: &str, to: u8, cents: i128) -> Booking {
        let posting = |n, units| Posting {
            owner: party(1),
            account: account(n),
            amount: Money::new(units, eur()).unwrap(),
            note: None,
            tax: None,
        };
        Booking {
            date: date.parse().unwrap(),
            text: "test".into(),
            postings: vec![posting(to, cents), posting(2, -cents)],
            documents: vec![],
        }
    }

    fn append(set: &mut EventSet, clock: &mut Clock, ms: u64, payload: Payload) {
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        set.insert(Event { time: clock.now(ms), seq, prev, payload });
    }

    #[test]
    fn balances_per_owner_account_and_currency_up_to_a_date() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        append(&mut set, &mut clock, 1, Payload::BookingRecorded { booking: booking("2026-10-01", 3, 1240) });
        append(&mut set, &mut clock, 2, Payload::BookingRecorded { booking: booking("2026-11-01", 3, 2990) });

        let all = set.balances(None).unwrap();
        assert_eq!(all.get(&party(1), &account(3), &eur()), 4230);
        assert_eq!(all.get(&party(1), &account(2), &eur()), -4230);
        assert_eq!(all.get(&party(2), &account(3), &eur()), 0, "another owner's books");
        let october = set.balances(Some("2026-10-31".parse().unwrap())).unwrap();
        assert_eq!(october.get(&party(1), &account(3), &eur()), 1240);
        assert_eq!(all.iter().map(|(_, _, _, units)| units).sum::<i128>(), 0, "debit equals credit");
    }

    #[test]
    fn a_reversal_brings_the_balance_back() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let recorded = booking("2026-10-01", 3, 1240);
        let reversal = recorded.reversed("2026-10-02".parse().unwrap(), "reversal".into());
        append(&mut set, &mut clock, 1, Payload::BookingRecorded { booking: recorded });
        let hash = set.ordered()[0].0;
        append(&mut set, &mut clock, 2, Payload::BookingReversed { reverses: hash, booking: reversal });
        assert_eq!(set.balances(None).unwrap().get(&party(1), &account(3), &eur()), 0);
    }

    #[test]
    fn a_trial_balance_rolls_up_the_hierarchy() {
        // Chart: 10 expenses ← 3 office; opened in the books: 11 software ← 3.
        let node = |n, parent: Option<u8>| Account {
            id: account(n),
            kind: AccountKind::Expense,
            parent: parent.map(account),
            attributes: Default::default(),
        };
        let chart =
            Chart { name: "test".into(), version: "1".into(), accounts: vec![node(10, None), node(3, Some(10))] };
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        append(&mut set, &mut clock, 1, Payload::AccountOpened { account: node(11, Some(3)) });
        append(&mut set, &mut clock, 2, Payload::BookingRecorded { booking: booking("2026-10-01", 3, 1000) });
        append(&mut set, &mut clock, 3, Payload::BookingRecorded { booking: booking("2026-10-02", 11, 500) });

        let trial = set.trial_balance(&chart, None).unwrap();
        assert_eq!(trial.get(&party(1), &account(11), &eur()), 500);
        assert_eq!(trial.get(&party(1), &account(3), &eur()), 1500, "office includes software");
        assert_eq!(trial.get(&party(1), &account(10), &eur()), 1500, "expenses include office");
    }
}
