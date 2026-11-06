// SPDX-License-Identifier: MIT
//! The capital accounts of the partners of a partnership and how they developed in a year (spec part 0009): the
//! Kapitalkontenentwicklung of the German E-Bilanz.

use std::collections::BTreeMap;

use crate::{AccountId, AccountKind, BalanceError, Basis, Chart, Date, EventSet, PartyId, Payload};

/// Whether a partner is liable without limit (Vollhafter – a general partner) or limited (Teilhafter – a limited
/// partner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Liability {
    /// Liable without limit.
    Unlimited,
    /// Liable up to the contribution.
    Limited,
}

impl Liability {
    /// The branch of the kinds of equity accounts in the taxonomy: `unlimitedPartners` or `limitedPartners`.
    pub fn branch(self) -> &'static str {
        match self {
            Liability::Unlimited => "unlimitedPartners",
            Liability::Limited => "limitedPartners",
        }
    }
}

/// How one capital account of one partner developed in a period, in base units, credit positive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapitalAccount {
    /// The balance at the beginning.
    pub begin: i128,
    /// Deposits.
    pub deposits: i128,
    /// Withdrawals, positive.
    pub withdrawals: i128,
    /// The partner's share of the result of the period.
    pub income_share: i128,
    /// Transfers from and to other capital accounts of the partners.
    pub movements: i128,
    /// The balance at the end.
    pub end: i128,
}

/// The capital accounts of a partnership: per partner, liability and kind of account (`FK`, `VK`, … – the suffix of
/// the taxonomy).
pub type CapitalAccounts = BTreeMap<(PartyId, Liability, String), CapitalAccount>;

impl EventSet {
    /// How the capital accounts of the partners of `owner` developed from `from` to `to`, in the tax balance sheet
    /// (spec part 0009). A capital account is an account opened in the books with the attributes `partner` (the
    /// partner's identity), `capital` (its kind) and `liability` (`unlimited` or `limited`). Each partner's share of
    /// the result – of the period, and of the years before it for the beginning – goes to the partner's variable
    /// capital (`VK`), by `shares` (partner → numerator, denominator); remainders of the division to the last partner.
    pub fn capital_accounts(
        &self,
        chart: &Chart,
        owner: &PartyId,
        from: Date,
        to: Date,
        shares: &BTreeMap<PartyId, (i128, i128)>,
    ) -> Result<CapitalAccounts, BalanceError> {
        // The capital accounts opened in the books.
        let mut accounts: BTreeMap<AccountId, (PartyId, Liability, String)> = BTreeMap::new();
        for (_, event) in self.ordered() {
            if let Payload::AccountOpened { account } = &event.payload {
                let get = |key: &str| account.attributes.get(key);
                let partner = get("partner").and_then(|p| p.parse::<PartyId>().ok());
                let liability = match get("liability").map(String::as_str) {
                    Some("unlimited") => Some(Liability::Unlimited),
                    Some("limited") => Some(Liability::Limited),
                    _ => None,
                };
                if let (Some(partner), Some(liability), Some(kind)) = (partner, liability, get("capital")) {
                    accounts.entry(account.id.clone()).or_insert((partner, liability, kind.clone()));
                }
            }
        }
        let kinds = self.account_kinds(chart);
        let mut out: CapitalAccounts = BTreeMap::new();
        let (mut result_before, mut result) = (0i128, 0i128);
        for (_, event) in self.ordered() {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            if booking.date > to || !booking.counts_in(Basis::Tax) {
                continue;
            }
            let before = booking.date < from;
            let ours: Vec<_> = booking.postings.iter().filter(|p| p.owner == *owner).collect();
            // A booking only between capital accounts moves capital; any other is a deposit or a withdrawal.
            let transfer = ours.iter().all(|p| accounts.contains_key(&p.account));
            for posting in ours {
                let credit = -posting.functional().base_units();
                if matches!(kinds.get(&posting.account), Some(AccountKind::Income | AccountKind::Expense)) {
                    if before {
                        result_before += credit
                    } else {
                        result += credit
                    }
                    continue;
                }
                let Some(key) = accounts.get(&posting.account) else { continue };
                let entry = out.entry(key.clone()).or_default();
                if before {
                    entry.begin += credit;
                } else if transfer {
                    entry.movements += credit;
                } else if credit > 0 {
                    entry.deposits += credit;
                } else {
                    entry.withdrawals -= credit;
                }
            }
        }
        // The shares of the result on the variable capital, the remainder of the division with the last partner.
        let total: Vec<(&PartyId, i128, i128)> = shares.iter().map(|(p, (n, d))| (p, *n, *d)).collect();
        let split = |amount: i128| -> Vec<i128> {
            let mut parts: Vec<i128> = total.iter().map(|(_, n, d)| if *d == 0 { 0 } else { amount * n / d }).collect();
            let sum: i128 = parts.iter().sum();
            if let Some(last) = parts.last_mut() {
                *last += amount - sum;
            }
            parts
        };
        let liability_of = |partner: &PartyId, out: &CapitalAccounts| {
            out.keys()
                .chain(accounts.values())
                .find(|(p, _, _)| p == partner)
                .map_or(Liability::Unlimited, |(_, l, _)| *l)
        };
        for ((partner, _, _), (before_share, share)) in
            total.iter().zip(split(result_before).into_iter().zip(split(result)))
        {
            let liability = liability_of(partner, &out);
            let entry = out.entry(((*partner).clone(), liability, "VK".to_owned())).or_default();
            entry.begin += before_share;
            entry.income_share += share;
        }
        for account in out.values_mut() {
            account.end =
                account.begin + account.deposits - account.withdrawals + account.income_share + account.movements;
        }
        Ok(out)
    }
}

/// The fields of the Kapitalkontenentwicklung of HGB taxonomy 6.10: per account
/// `<partner>/<branch>.<kind>/<movement>`, the movements named by the elements below
/// `table.kke.sumEquityAccounts.sumYearEnd` (`begin`, `deposits`, `withdrawals`, `incomeShare`, `capitalMovements`)
/// and `sumYearEnd` itself; zero movements are left out, the end never.
pub fn capital_account_fields(accounts: &CapitalAccounts) -> BTreeMap<String, i128> {
    let mut fields = BTreeMap::new();
    for ((partner, liability, kind), a) in accounts {
        let key = |movement: &str| format!("{partner}/{}.{kind}/{movement}", liability.branch());
        for (movement, units) in [
            ("begin", a.begin),
            ("deposits", a.deposits),
            ("withdrawals", a.withdrawals),
            ("incomeShare", a.income_share),
            ("capitalMovements", a.movements),
        ] {
            if units != 0 {
                fields.insert(key(movement), units);
            }
        }
        fields.insert(key("sumYearEnd"), a.end);
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, Booking, ChartTranslation, Clock, Event, Money, Posting, party::tests::party};

    /// Two partners, a general and a limited one, half each: deposits, a withdrawal, a result of 10,000.
    #[test]
    fn the_capital_accounts_of_two_partners() {
        let skr03 = ChartTranslation::de_skr03();
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let mut add = |set: &mut EventSet, payload: Payload, ms: u64| {
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event { time: clock.now(ms), seq, prev, payload });
        };
        let capital = |id: &str, partner: u8, liability: &str, kind: &str| Account {
            id: id.parse().unwrap(),
            kind: AccountKind::Equity,
            parent: Some(skr03.accounts["0870"].clone()),
            attributes: [
                ("partner".to_owned(), party(partner).to_string()),
                ("liability".to_owned(), liability.to_owned()),
                ("capital".to_owned(), kind.to_owned()),
            ]
            .into(),
        };
        let a = capital("01a18406-0000-7000-8000-00000000ca01", 2, "unlimited", "FK");
        let b = capital("01a18406-0000-7000-8000-00000000ca02", 3, "limited", "FK");
        let b_vk = capital("01a18406-0000-7000-8000-00000000ca03", 3, "limited", "VK");
        for (i, account) in [&a, &b, &b_vk].into_iter().enumerate() {
            add(&mut set, Payload::AccountOpened { account: account.clone() }, i as u64);
        }
        let posting = |account: &AccountId, units: i128| Posting {
            owner: party(1),
            account: account.clone(),
            amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
            note: None,
            tax: None,
            exchange: None,
        };
        let bank = skr03.accounts["1200"].clone();
        let book = |date: &str, lines: Vec<Posting>| Booking {
            basis: None,
            attributed: None,
            date: date.parse().unwrap(),
            text: "t".into(),
            postings: lines,
            documents: vec![],
            entries: vec![],
            transaction: None,
        };
        let bookings = [
            book("2025-12-01", vec![posting(&bank, 2_000_000), posting(&a.id, -1_000_000), posting(&b.id, -1_000_000)]),
            book("2026-03-01", vec![posting(&bank, -50_000), posting(&b_vk.id, 50_000)]),
            book("2026-06-01", vec![posting(&bank, 1_000_000), posting(&skr03.accounts["8400"], -1_000_000)]),
        ];
        for (i, booking) in bookings.into_iter().enumerate() {
            add(&mut set, Payload::BookingRecorded { booking }, 10 + i as u64);
        }
        let shares = [(party(2), (1, 2)), (party(3), (1, 2))].into();
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let accounts = set.capital_accounts(&Chart::v0_1(), &party(1), from, to, &shares).unwrap();
        let get = |p: u8, l: Liability, k: &str| accounts[&(party(p), l, k.to_owned())].clone();
        assert_eq!(get(2, Liability::Unlimited, "FK").begin, 1_000_000);
        assert_eq!(get(2, Liability::Unlimited, "VK").income_share, 500_000);
        let vk = get(3, Liability::Limited, "VK");
        assert_eq!((vk.withdrawals, vk.income_share, vk.end), (50_000, 500_000, 450_000));
        // The ends add up to the equity: capital 2,000,000 less 500 withdrawn plus the result 10,000.
        let total: i128 = accounts.values().map(|a| a.end).sum();
        assert_eq!(total, 2_000_000 - 50_000 + 1_000_000);
        let fields = capital_account_fields(&accounts);
        assert_eq!(fields[&format!("{}/limitedPartners.VK/withdrawals", party(3))], 50_000);
    }
}
