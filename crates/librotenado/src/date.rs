// SPDX-License-Identifier: MIT
//! Calendar dates without time or time zone, as in `2026-10-24`.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::ParseError;

/// A day of the Gregorian calendar, written `YYYY-MM-DD` (years 0001 to 9999).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    /// The date, or `None` if it does not exist.
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let valid = (1..=9999).contains(&year) && (1..=12).contains(&month) && day >= 1 && day <= days_in(year, month);
        valid.then_some(Date { year, month, day })
    }

    /// The year.
    pub fn year(self) -> u16 {
        self.year
    }

    /// The month, 1 to 12.
    pub fn month(self) -> u8 {
        self.month
    }

    /// The day of the month.
    pub fn day(self) -> u8 {
        self.day
    }
}

fn days_in(year: u16, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        2 => 28,
        _ => 31,
    }
}

impl FromStr for Date {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseError::Date(s.to_owned());
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return Err(err());
        }
        let num = |part: &str| part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse::<u16>().ok()).flatten();
        let (year, month, day) = (num(&s[0..4]), num(&s[5..7]), num(&s[8..10]));
        match (year, month, day) {
            (Some(y), Some(m), Some(d)) => Date::new(y, m as u8, d as u8).ok_or_else(err),
            _ => Err(err()),
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_iso_dates() {
        assert_eq!("2026-10-24".parse::<Date>().unwrap().to_string(), "2026-10-24");
        assert!("2024-02-29".parse::<Date>().is_ok());
        assert!("2000-02-29".parse::<Date>().is_ok());
    }

    #[test]
    fn rejects_days_that_do_not_exist() {
        for s in ["2026-02-29", "1900-02-29", "2026-04-31", "2026-13-01", "0000-01-01", "2026-1-01", "26-10-24"] {
            assert!(s.parse::<Date>().is_err(), "{s:?}");
        }
    }

    #[test]
    fn orders_chronologically() {
        assert!("2026-01-31".parse::<Date>().unwrap() < "2026-02-01".parse().unwrap());
    }
}
