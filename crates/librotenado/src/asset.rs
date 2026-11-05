// SPDX-License-Identifier: MIT
//! Fixed assets (spec part 0011): what an owner acquired to use for longer than a year, how it is depreciated, and
//! when it left the business – the register a balance sheet and the German Anlage AVEÜR are derived from.

use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{AccountId, Date, EventHash, EventSet, Money, Object, ParseError, PartyId, Payload};

/// The identity of a fixed asset: a UUID in its one form (spec part 0001). It carries no meaning.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(String);

impl AssetId {
    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AssetId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(AssetId(s.to_owned())) } else { Err(ParseError::Asset(s.to_owned())) }
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for AssetId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AssetId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// How an asset is depreciated (spec part 0011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Depreciation {
    /// In equal amounts per month over its useful life, from the month it was acquired (§ 7 Abs. 1 EStG).
    StraightLine {
        /// The useful life in months, at least 1.
        months: u16,
    },
    /// In full in the year it was acquired – a low-value asset (§ 6 Abs. 2 EStG).
    Immediate,
    /// In a pool of the year it was acquired, a fifth in that year and each of the four after it, whether it is
    /// disposed of or not (§ 6 Abs. 2a EStG).
    Pool,
    /// Not at all – land, financial assets.
    None,
}

/// A fixed asset: acquired by an owner, kept on an account of fixed assets (spec part 0011).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset {
    /// Its identity.
    pub id: AssetId,
    /// The owner whose asset it is.
    pub owner: PartyId,
    /// What it is.
    #[serde(deserialize_with = "crate::text")]
    pub name: String,
    /// The account it is kept on.
    pub account: AccountId,
    /// The day it was acquired, or made or brought into the business.
    pub acquired: Date,
    /// Its cost – of acquisition or production, or the value it was brought in at –, in the owner's functional
    /// currency, without deductible VAT.
    pub cost: Money,
    /// How it is depreciated.
    pub depreciation: Depreciation,
}

/// An asset leaving the business: sold, withdrawn, scrapped (spec part 0011).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Disposal {
    /// The asset.
    pub asset: AssetId,
    /// The day it left.
    pub date: Date,
    /// What it was sold for, without VAT; absent when it was not sold.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub proceeds: Option<Money>,
}

impl Object for Asset {
    const TYPE: &'static str = "asset";
}

impl Object for Disposal {
    const TYPE: &'static str = "disposal";
}

/// A fixed asset as the books know it: acquired, and disposed of if it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetState {
    /// The asset.
    pub asset: Asset,
    /// When it left, if it did.
    pub disposal: Option<Disposal>,
}

/// Something about the fixed assets that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssetProblem {
    /// An asset of someone who is not an owner of the books.
    #[error("event {event} registers an asset of {owner}, who is not an owner")]
    NotAnOwner {
        /// The event.
        event: EventHash,
        /// The owner named.
        owner: PartyId,
    },
    /// An asset registered a second time under one identity.
    #[error("event {event} registers asset {asset} a second time")]
    Duplicate {
        /// The event.
        event: EventHash,
        /// The asset.
        asset: AssetId,
    },
    /// An asset whose depreciation cannot be computed: no useful life, or a cost not above zero.
    #[error("event {event} registers asset {asset}, which cannot be depreciated as given")]
    Invalid {
        /// The event.
        event: EventHash,
        /// The asset.
        asset: AssetId,
    },
    /// A disposal of an asset that is not registered – perhaps not arrived yet.
    #[error("event {event} disposes of asset {asset}, which is not registered")]
    UnknownAsset {
        /// The event.
        event: EventHash,
        /// The asset.
        asset: AssetId,
    },
    /// A further disposal of an asset disposed of already; the first in the agreed order applies.
    #[error("event {event} disposes of asset {asset} again")]
    DisposedTwice {
        /// The event.
        event: EventHash,
        /// The asset.
        asset: AssetId,
    },
    /// A disposal dated before the asset was acquired.
    #[error("event {event} disposes of asset {asset} before it was acquired")]
    DisposedBeforeAcquired {
        /// The event.
        event: EventHash,
        /// The asset.
        asset: AssetId,
    },
}

/// Rounds `units × part / whole` half away from zero.
fn share(units: i128, part: i128, whole: i128) -> i128 {
    let product = units * part;
    let half = whole / 2;
    if product >= 0 { (product + half) / whole } else { (product - half) / whole }
}

/// The months from the month of `from` to the month of `to`, both counted; zero when `to` is before `from`'s month.
fn months_through(from: Date, to: Date) -> i128 {
    let count = (i128::from(to.year()) * 12 + i128::from(to.month()))
        - (i128::from(from.year()) * 12 + i128::from(from.month()))
        + 1;
    count.max(0)
}

impl AssetState {
    /// The depreciation accumulated up to and including `day`, in base units: for a straight line, the months from
    /// the month acquired through the month of `day` – and no further than the month the asset left –, rounded half
    /// away from zero on the whole; for a pool, a fifth for each calendar year from the year acquired, whether the
    /// asset left or not; for a low-value asset, all of it from the day acquired.
    pub fn accumulated(&self, day: Date) -> i128 {
        let cost = self.asset.cost.base_units();
        if day < self.asset.acquired {
            return 0;
        }
        match self.asset.depreciation {
            Depreciation::StraightLine { months } if months > 0 => {
                let until = self.disposal.as_ref().map_or(day, |d| d.date.min(day));
                let elapsed = months_through(self.asset.acquired, until).min(i128::from(months));
                share(cost, elapsed, i128::from(months))
            }
            Depreciation::StraightLine { .. } | Depreciation::None => 0,
            Depreciation::Immediate => cost,
            Depreciation::Pool => {
                let years = (i128::from(day.year()) - i128::from(self.asset.acquired.year()) + 1).clamp(0, 5);
                share(cost, years, 5)
            }
        }
    }

    /// The depreciation of the period from `from` to `to`, both included.
    pub fn depreciation(&self, from: Date, to: Date) -> i128 {
        let before = from.previous().map_or(0, |day| self.accumulated(day));
        self.accumulated(to) - before
    }

    /// The book value at the end of `day`: the cost less the depreciation accumulated; zero once the asset has left
    /// – except in a pool, which keeps it until dissolved.
    pub fn book_value(&self, day: Date) -> i128 {
        let left = self.disposal.as_ref().is_some_and(|d| d.date <= day);
        if left && self.asset.depreciation != Depreciation::Pool {
            return 0;
        }
        self.asset.cost.base_units() - self.accumulated(day)
    }

    /// The book value it left at: the cost less the depreciation up to the day it left; none if it did not.
    pub fn value_at_disposal(&self) -> Option<i128> {
        let disposal = self.disposal.as_ref()?;
        Some(self.asset.cost.base_units() - self.accumulated(disposal.date))
    }
}

/// The fields of a group of assets in a report of the register (spec part 0011): each absent where the form has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFields {
    /// The cost of the assets held at the beginning of the period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<String>,
    /// Their book value at the beginning of the period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opening: Option<String>,
    /// The cost of the assets acquired in the period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additions: Option<String>,
    /// The depreciation of the period – or, for a pool, the part dissolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depreciation: Option<String>,
    /// The book value of the assets that left in the period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposals: Option<String>,
    /// The book value at the end of the period.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closing: Option<String>,
}

impl EventSet {
    /// The fixed assets of `owner` in the agreed order of their registration, each with its first disposal.
    pub fn assets(&self, owner: &PartyId) -> Vec<AssetState> {
        let mut states: Vec<AssetState> = Vec::new();
        for (_, event) in self.ordered() {
            match &event.payload {
                Payload::AssetAcquired { asset } if asset.owner == *owner => {
                    if !states.iter().any(|s| s.asset.id == asset.id) {
                        states.push(AssetState { asset: asset.clone(), disposal: None });
                    }
                }
                Payload::AssetDisposed { disposal } => {
                    if let Some(state) = states.iter_mut().find(|s| s.asset.id == disposal.asset)
                        && state.disposal.is_none()
                    {
                        state.disposal = Some(disposal.clone());
                    }
                }
                _ => {}
            }
        }
        states
    }

    /// Checks the fixed assets: each registered once, of an owner, with a cost above zero and – on a straight line
    /// – a useful life; each disposal of a registered asset, once, not before it was acquired.
    pub fn review_assets(&self) -> Vec<AssetProblem> {
        let owners = self.owners();
        let mut problems = Vec::new();
        let mut registered: BTreeMap<&AssetId, Date> = BTreeMap::new();
        let mut disposed: Vec<&AssetId> = Vec::new();
        for (hash, event) in self.ordered() {
            match &event.payload {
                Payload::AssetAcquired { asset } => {
                    if registered.insert(&asset.id, asset.acquired).is_some() {
                        problems.push(AssetProblem::Duplicate { event: hash, asset: asset.id.clone() });
                        continue;
                    }
                    if !owners.contains(&&asset.owner) {
                        problems.push(AssetProblem::NotAnOwner { event: hash, owner: asset.owner.clone() });
                    }
                    let no_life = matches!(asset.depreciation, Depreciation::StraightLine { months: 0 });
                    if no_life || asset.cost.base_units() <= 0 {
                        problems.push(AssetProblem::Invalid { event: hash, asset: asset.id.clone() });
                    }
                }
                Payload::AssetDisposed { disposal } => match registered.get(&disposal.asset) {
                    None => problems.push(AssetProblem::UnknownAsset { event: hash, asset: disposal.asset.clone() }),
                    Some(acquired) => {
                        if disposed.contains(&&disposal.asset) {
                            problems.push(AssetProblem::DisposedTwice { event: hash, asset: disposal.asset.clone() });
                        } else {
                            disposed.push(&disposal.asset);
                        }
                        if disposal.date < *acquired {
                            problems.push(AssetProblem::DisposedBeforeAcquired {
                                event: hash,
                                asset: disposal.asset.clone(),
                            });
                        }
                    }
                },
                _ => {}
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(acquired: &str, cost: i128, depreciation: Depreciation) -> AssetState {
        AssetState {
            asset: Asset {
                id: "01a1a976-0000-7000-8000-000000000001".parse().unwrap(),
                owner: "01a1a976-0000-7000-8000-0000000000a1".parse().unwrap(),
                name: "Laptop".into(),
                account: "01a18406-ad76-7351-9e54-cfc3b49a15a1".parse().unwrap(),
                acquired: acquired.parse().unwrap(),
                cost: Money::new(cost, "EUR".parse().unwrap()).unwrap(),
                depreciation,
            },
            disposal: None,
        }
    }

    fn date(text: &str) -> Date {
        text.parse().unwrap()
    }

    /// A laptop of 1,800 euros, acquired in October over 36 months: three months in the first year, by month.
    #[test]
    fn straight_line_by_month_from_the_month_acquired() {
        let laptop = asset("2026-10-15", 180_000, Depreciation::StraightLine { months: 36 });
        assert_eq!(laptop.depreciation(date("2026-01-01"), date("2026-12-31")), 15_000);
        assert_eq!(laptop.depreciation(date("2027-01-01"), date("2027-12-31")), 60_000);
        assert_eq!(laptop.book_value(date("2027-12-31")), 105_000);
        // Fully depreciated after 36 months, the last in September 2029.
        assert_eq!(laptop.accumulated(date("2029-09-30")), 180_000);
        assert_eq!(laptop.depreciation(date("2029-10-01"), date("2030-12-31")), 0);
        // Sold in March 2028: depreciated up to and including March, then gone at its book value.
        let mut sold = laptop.clone();
        sold.disposal = Some(Disposal { asset: sold.asset.id.clone(), date: date("2028-03-10"), proceeds: None });
        assert_eq!(sold.depreciation(date("2028-01-01"), date("2028-12-31")), 15_000);
        assert_eq!(sold.value_at_disposal(), Some(90_000));
        assert_eq!(sold.book_value(date("2028-12-31")), 0);
    }

    /// A pool dissolves a fifth a year, whether its assets leave or not; a low-value asset goes at once.
    #[test]
    fn pools_by_the_fifth_and_low_value_assets_at_once() {
        let mut pool = asset("2026-06-01", 99_999, Depreciation::Pool);
        assert_eq!(pool.depreciation(date("2026-01-01"), date("2026-12-31")), 20_000);
        pool.disposal = Some(Disposal { asset: pool.asset.id.clone(), date: date("2027-02-01"), proceeds: None });
        assert_eq!(pool.depreciation(date("2027-01-01"), date("2027-12-31")), 20_000);
        assert_eq!(pool.accumulated(date("2030-12-31")), 99_999);
        let gwg = asset("2026-06-01", 79_900, Depreciation::Immediate);
        assert_eq!(gwg.depreciation(date("2026-01-01"), date("2026-12-31")), 79_900);
        assert_eq!(gwg.depreciation(date("2027-01-01"), date("2027-12-31")), 0);
        let land = asset("2026-06-01", 5_000_000, Depreciation::None);
        assert_eq!(land.book_value(date("2030-12-31")), 5_000_000);
    }

    /// The register through its events: a laptop on office equipment and a pool, reported by the groups and pools of
    /// the Anlage AVEÜR; a disposal of an asset not registered is reported.
    #[test]
    fn the_register_is_reported_by_group_and_pool() {
        use crate::{Chart, ChartTranslation, Clock, Event, ReportMapping};
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let device: crate::DeviceId = "a".parse().unwrap();
        let mut add = |set: &mut EventSet, payload: Payload| {
            let (seq, prev) = set.chain_head(&device);
            set.insert(Event { time: clock.now(seq), seq, prev, payload }).0
        };
        let skr = ChartTranslation::de_skr03();
        let mut laptop = asset("2026-10-15", 180_000, Depreciation::StraightLine { months: 36 });
        laptop.asset.account = skr.accounts["0420"].clone();
        let mut chairs = asset("2026-03-01", 60_000, Depreciation::Pool);
        chairs.asset.id = "01a1a976-0000-7000-8000-000000000002".parse().unwrap();
        chairs.asset.account = skr.accounts["0485"].clone();
        let owner = laptop.asset.owner.clone();
        add(&mut set, Payload::OwnerAdded { owner: owner.clone() });
        for state in [&laptop, &chairs] {
            add(&mut set, Payload::AssetAcquired { asset: state.asset.clone() });
        }
        let report = |set: &EventSet, year: u16| {
            set.asset_report(
                &ReportMapping::de_aveuer_2026(),
                &Chart::v0_1(),
                &owner,
                Date::new(year, 1, 1).unwrap(),
                Date::new(year, 12, 31).unwrap(),
            )
        };
        let first = report(&set, 2026);
        let field = |r: &BTreeMap<String, i128>, kz: &str| r.get(kz).copied();
        assert_eq!(
            (field(&first, "412"), field(&first, "414"), field(&first, "416")),
            (Some(180_000), Some(15_000), Some(165_000))
        );
        assert_eq!(
            (field(&first, "432"), field(&first, "434"), field(&first, "436")),
            (Some(60_000), Some(12_000), Some(48_000))
        );
        let second = report(&set, 2027);
        assert_eq!(
            (field(&second, "410"), field(&second, "411"), field(&second, "414"), field(&second, "416")),
            (Some(180_000), Some(165_000), Some(60_000), Some(105_000))
        );
        assert_eq!(
            (field(&second, "440"), field(&second, "441"), field(&second, "444"), field(&second, "446")),
            (Some(60_000), Some(48_000), Some(12_000), Some(36_000))
        );
        // Sold in March 2028: three months more, then gone at its book value.
        let disposal = Disposal { asset: laptop.asset.id.clone(), date: date("2028-03-10"), proceeds: None };
        add(&mut set, Payload::AssetDisposed { disposal });
        let third = report(&set, 2028);
        assert_eq!(
            (field(&third, "414"), field(&third, "415"), field(&third, "416")),
            (Some(15_000), Some(90_000), None)
        );
        assert_eq!(set.review_assets(), []);
        let unknown: AssetId = "01a1a976-0000-7000-8000-000000000009".parse().unwrap();
        let disposal = Disposal { asset: unknown.clone(), date: date("2028-01-01"), proceeds: None };
        let event = add(&mut set, Payload::AssetDisposed { disposal });
        assert_eq!(set.review_assets(), [AssetProblem::UnknownAsset { event, asset: unknown }]);
    }
}
