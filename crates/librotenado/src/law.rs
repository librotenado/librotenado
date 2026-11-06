// SPDX-License-Identifier: MIT
//! The values of the law a country's rules compute with – limits, rates, allowances, tariffs –, each with the day it
//! applies from, its provision and the state of its review (spec part 0009). The library reads them from here, and
//! so does the rulebook: what is shown is what is computed with.

use serde::{Deserialize, Serialize};

use crate::{Date, Decimal, Object};

/// One value of the law.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawValue {
    /// What it is, e.g. `sv.health_ceiling`.
    pub key: String,
    /// The first day it applies; it applies until the next value of the same key.
    pub from: Date,
    /// The value, in `unit`.
    pub value: Decimal,
    /// `EUR`, `EUR/year`, `EUR/month`, `EUR/day`, `percent`, `factor`, `days`.
    pub unit: String,
    /// The provision it comes from.
    pub law: String,
    /// Where the provision can be read.
    pub url: String,
    /// The state of its review: `draft` until reviewed, then who reviewed it and when.
    pub status: String,
}

/// The values of the law of one country.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawValues {
    /// The country.
    pub country: crate::Country,
    /// The version of the protocol they are for.
    pub into: String,
    /// The values, each key in the order of its days.
    pub values: Vec<LawValue>,
}

impl Object for LawValues {
    const TYPE: &'static str = "law_values";
}

impl LawValues {
    /// The values of Germany (`spec/data/law/de.json`), read once.
    pub fn de() -> Self {
        static VALUES: std::sync::OnceLock<LawValues> = std::sync::OnceLock::new();
        VALUES
            .get_or_init(|| {
                crate::from_root(include_bytes!("../data/law/de.json")).expect("the published values are valid")
            })
            .clone()
    }

    /// The value of `key` that applies on `date`: the one with the latest `from` not after it.
    pub fn get(&self, key: &str, date: Date) -> Option<&LawValue> {
        self.values.iter().filter(|v| v.key == key && v.from <= date).max_by_key(|v| v.from)
    }

    /// The value of `key` set for the year of `date` itself: values the law sets anew each year – contribution
    /// ceilings, the basic allowance – do not carry into a year they are not yet published for.
    pub fn of_year(&self, key: &str, date: Date) -> Option<&LawValue> {
        self.get(key, date).filter(|v| v.from.year() == date.year())
    }

    /// An amount in euros as cents.
    pub(crate) fn cents(&self, key: &str, date: Date) -> Option<i128> {
        let v = &self.get(key, date)?.value;
        Some(v.units() * 100 / 10i128.pow(u32::from(v.scale())))
    }

    /// A percentage in millionths (1 % = 10,000).
    pub(crate) fn millionths(&self, key: &str, date: Date) -> Option<i128> {
        let v = &self.get(key, date)?.value;
        Some(v.units() * 10_000 / 10i128.pow(u32::from(v.scale())))
    }

    /// A value as a fraction: numerator and denominator.
    pub(crate) fn fraction(&self, key: &str, date: Date) -> Option<(i128, i128)> {
        let v = &self.get(key, date)?.value;
        Some((v.units(), 10i128.pow(u32::from(v.scale()))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_values_of_germany() {
        let law = LawValues::de();
        let day = |d: &str| d.parse::<Date>().unwrap();
        assert_eq!(law.cents("sv.health_ceiling", day("2026-06-30")), Some(6_975_000));
        assert_eq!(law.cents("sv.health_ceiling", day("2025-12-31")), Some(6_615_000));
        assert_eq!(law.cents("sv.health_ceiling", day("2024-12-31")), None);
        assert_eq!(law.millionths("sv.insolvency", day("2026-01-01")), Some(1_500));
        assert_eq!(law.fraction("sv.factor_f", day("2026-01-01")), Some((6_619, 10_000)));
        // Every value names its provision and where to read it.
        assert!(law.values.iter().all(|v| !v.law.is_empty() && v.url.starts_with("https://")));
    }
}
