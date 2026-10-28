// SPDX-License-Identifier: MIT
//! Exact decimal numbers for prices, quantities and rates – never a float.

use std::{cmp::Ordering, fmt, ops::Neg, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::ParseError;

/// An exact decimal number `units · 10^−scale` – a price per unit, a quantity, a rate. Money is [`crate::Money`].
///
/// A value has one form only: the smallest scale (`units` not divisible by ten unless the scale is 0), written as
/// a string in exactly one way (`"1.5"`, never `"1.50"`; `"0"`, never `"-0"`). Reading rejects any other form, so
/// the same value always has the same bytes and the same hash. Addition and subtraction are exact; rounding is a
/// rule of the law that applies, not of the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Decimal {
    units: i128,
    scale: u8,
}

impl Decimal {
    /// The most decimal places a number may have.
    pub const MAX_SCALE: u8 = 18;
    /// Zero.
    pub const ZERO: Decimal = Decimal { units: 0, scale: 0 };

    /// `units · 10^−scale`, reduced to its one form; `None` if the scale is above [`Self::MAX_SCALE`].
    pub fn new(units: i128, scale: u8) -> Option<Self> {
        (scale <= Self::MAX_SCALE).then(|| Decimal { units, scale }.reduced())
    }

    /// The number of decimal places.
    pub fn scale(self) -> u8 {
        self.scale
    }

    /// Whether the value is zero.
    pub fn is_zero(self) -> bool {
        self.units == 0
    }

    /// Whether the value is below zero.
    pub fn is_negative(self) -> bool {
        self.units < 0
    }

    /// The exact sum, or `None` on overflow.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let scale = self.scale.max(other.scale);
        let a = self.units_at(scale)?;
        let b = other.units_at(scale)?;
        Some(Decimal { units: a.checked_add(b)?, scale }.reduced())
    }

    /// The exact difference, or `None` on overflow.
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.checked_add(-other)
    }

    /// The units at a scale at least as large as the own one.
    fn units_at(self, scale: u8) -> Option<i128> {
        10i128.checked_pow(u32::from(scale - self.scale))?.checked_mul(self.units)
    }

    /// The same value at the smallest scale.
    fn reduced(self) -> Self {
        let (mut units, mut scale) = (self.units, self.scale);
        while scale > 0 && units % 10 == 0 {
            units /= 10;
            scale -= 1;
        }
        Decimal { units, scale }
    }
}

impl Neg for Decimal {
    type Output = Decimal;

    /// The same value with the opposite sign.
    fn neg(self) -> Decimal {
        Decimal { units: -self.units, scale: self.scale }
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let scale = self.scale.max(other.scale);
        match (self.units_at(scale), other.units_at(scale)) {
            (Some(a), Some(b)) => a.cmp(&b),
            // Only reachable near the limits of i128: compare the integer parts, then the fractions.
            _ => (self.units / 10i128.pow(u32::from(self.scale)))
                .cmp(&(other.units / 10i128.pow(u32::from(other.scale))))
                .then_with(|| (self.units, self.scale).cmp(&(other.units, other.scale))),
        }
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for Decimal {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseError::Decimal(s.to_owned());
        let (negative, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let valid = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
        // The one form: no leading zero in the integer part, no trailing zero in the places, no "-0".
        let canonical = (int == "0" || !int.starts_with('0'))
            && !frac.ends_with('0')
            && (digits.contains('.') == !frac.is_empty())
            && !(negative && int == "0" && frac.is_empty());
        if int.is_empty() || !valid(int) || !valid(frac) || !canonical {
            return Err(err());
        }
        let scale = u8::try_from(frac.len()).ok().filter(|&n| n <= Self::MAX_SCALE).ok_or_else(err)?;
        let units: i128 = format!("{int}{frac}").parse().map_err(|_| err())?;
        Ok(Decimal { units: if negative { -units } else { units }, scale })
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.units < 0 { "-" } else { "" };
        let digits = self.units.unsigned_abs().to_string();
        let scale = usize::from(self.scale);
        if scale == 0 {
            return write!(f, "{sign}{digits}");
        }
        let digits = format!("{digits:0>width$}", width = scale + 1);
        let (int, frac) = digits.split_at(digits.len() - scale);
        write!(f, "{sign}{int}.{frac}")
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn parses_and_prints_its_one_form() {
        for s in ["0", "12", "100", "-12.3", "0.05", "-0.5", "1.799", "1234567.891"] {
            assert_eq!(d(s).to_string(), s);
        }
    }

    #[test]
    fn rejects_every_other_form() {
        for s in ["", "-", ".5", "5.", "1e3", "+1", "1,5", " 1", "0.1234567890123456789"] {
            assert!(s.parse::<Decimal>().is_err(), "{s:?}");
        }
        for s in ["1.50", "1.0", "01.5", "00", "-0", "-0.0", "0.10"] {
            assert!(s.parse::<Decimal>().is_err(), "{s:?} is not in its one form");
        }
    }

    #[test]
    fn construction_reduces_to_the_one_form() {
        assert_eq!(Decimal::new(150, 2).unwrap(), d("1.5"));
        assert_eq!(Decimal::new(150, 2).unwrap().scale(), 1);
        assert_eq!(Decimal::new(0, 5).unwrap(), Decimal::ZERO);
        assert_eq!(-Decimal::ZERO, Decimal::ZERO);
    }

    #[test]
    fn compares_by_value() {
        assert!(d("-0.01") < d("0"));
        assert!(d("2") > d("1.99"));
    }

    #[test]
    fn adds_and_subtracts_exactly() {
        assert_eq!(d("0.1").checked_add(d("0.2")).unwrap(), d("0.3"));
        assert_eq!(d("0.25").checked_add(d("0.25")).unwrap().to_string(), "0.5");
        assert_eq!(d("10").checked_sub(d("0.01")).unwrap().to_string(), "9.99");
        assert!(Decimal::new(i128::MAX, 0).unwrap().checked_add(d("1")).is_none());
    }

    #[test]
    fn is_a_string_in_json() {
        assert_eq!(serde_json::to_string(&d("-12.3")).unwrap(), r#""-12.3""#);
        assert_eq!(serde_json::from_str::<Decimal>(r#""4.2""#).unwrap(), d("4.2"));
        assert!(serde_json::from_str::<Decimal>(r#""4.20""#).is_err());
        assert!(serde_json::from_str::<Decimal>("4.2").is_err());
    }
}
