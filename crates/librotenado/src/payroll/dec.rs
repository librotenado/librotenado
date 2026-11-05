// SPDX-License-Identifier: MIT
//! The arithmetic of the PAP: exact decimals with the operations of Java's `BigDecimal` that its pseudocode uses.
//! Values stay small – amounts in cents, rates with a few places –, so a mantissa of 128 bits holds them; inputs are
//! bounded before the PAP runs (`super::MAX_AMOUNT`).

use std::cmp::Ordering;

/// A decimal `m × 10^-s`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Dec {
    m: i128,
    s: u32,
}

/// How a result is rounded to its scale: towards zero, or away from it.
#[derive(Debug, Clone, Copy)]
pub(super) enum Round {
    Down,
    Up,
}

/// A value the PAP turns into a decimal (`BigDecimal.valueOf`).
pub(super) trait Of {
    fn dec(self) -> Dec;
}

impl Of for i64 {
    fn dec(self) -> Dec {
        Dec { m: i128::from(self), s: 0 }
    }
}

impl Of for Dec {
    fn dec(self) -> Dec {
        self
    }
}

fn pow10(n: u32) -> i128 {
    10i128.pow(n)
}

/// `a ÷ b` rounded towards zero, or away from it.
fn div(a: i128, b: i128, round: Round) -> i128 {
    let q = a / b;
    match round {
        Round::Down => q,
        Round::Up if a % b != 0 => q + if (a < 0) == (b < 0) { 1 } else { -1 },
        Round::Up => q,
    }
}

impl Dec {
    pub const ZERO: Dec = Dec { m: 0, s: 0 };
    pub const ONE: Dec = Dec { m: 1, s: 0 };

    /// `m × 10^-s`.
    pub const fn lit(m: i128, s: u32) -> Dec {
        Dec { m, s }
    }

    pub fn of(value: impl Of) -> Dec {
        value.dec()
    }

    /// Both mantissas at the larger scale.
    fn align(self, other: Dec) -> (i128, i128, u32) {
        let s = self.s.max(other.s);
        (self.m * pow10(s - self.s), other.m * pow10(s - other.s), s)
    }

    /// Without trailing zeros, so that scales do not grow beyond need.
    fn normal(mut self) -> Dec {
        while self.s > 0 && self.m % 10 == 0 {
            self.m /= 10;
            self.s -= 1;
        }
        self
    }

    pub fn add(self, other: Dec) -> Dec {
        let (a, b, s) = self.align(other);
        Dec { m: a + b, s }.normal()
    }

    pub fn subtract(self, other: Dec) -> Dec {
        let (a, b, s) = self.align(other);
        Dec { m: a - b, s }.normal()
    }

    pub fn multiply(self, other: Dec) -> Dec {
        Dec { m: self.m * other.m, s: self.s + other.s }.normal()
    }

    /// `-1`, `0` or `1` as `self` is less than, equal to or greater than `other`.
    pub fn compare_to(self, other: Dec) -> i64 {
        let (a, b, _) = self.align(other);
        match a.cmp(&b) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    pub fn set_scale(self, s: u32, round: Round) -> Dec {
        if s >= self.s {
            Dec { m: self.m * pow10(s - self.s), s }
        } else {
            Dec { m: div(self.m, pow10(self.s - s), round), s }
        }
    }

    /// `self ÷ other` at the scale `s`, rounded.
    pub fn divide(self, other: Dec, s: u32, round: Round) -> Dec {
        // self.m / 10^self.s ÷ (other.m / 10^other.s) × 10^s = self.m × 10^(s + other.s - self.s) ÷ other.m
        let exponent = i64::from(s) + i64::from(other.s) - i64::from(self.s);
        let (a, b) = if exponent >= 0 {
            (self.m * pow10(exponent as u32), other.m)
        } else {
            (self.m, other.m * pow10((-exponent) as u32))
        };
        Dec { m: div(a, b, round), s }
    }

    /// `self ÷ other`, exact: the PAP divides without a scale only where the quotient ends.
    pub fn divide_exact(self, other: Dec) -> Dec {
        (0..=30)
            .map(|s| self.divide(other, s, Round::Down))
            .find(|q| q.multiply(other).compare_to(self) == 0)
            .unwrap_or_else(|| self.divide(other, 30, Round::Down))
            .normal()
    }

    /// The whole part, towards zero.
    pub fn long_value(self) -> i64 {
        (self.m / pow10(self.s)) as i64
    }

    /// The value in units of `10^-s`, rounded towards zero.
    pub fn units(self, s: u32) -> i128 {
        self.set_scale(s, Round::Down).m
    }
}

impl std::ops::Neg for Dec {
    type Output = Dec;
    fn neg(self) -> Dec {
        Dec { m: -self.m, s: self.s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_big_decimal() {
        let d = |m, s| Dec::lit(m, s);
        assert_eq!(d(12345, 3).set_scale(2, Round::Down).units(2), 1234);
        assert_eq!(d(12341, 3).set_scale(2, Round::Up).units(2), 1235);
        assert_eq!(d(-12341, 3).set_scale(2, Round::Up).units(2), -1235);
        assert_eq!(d(-12349, 3).set_scale(2, Round::Down).units(2), -1234);
        assert_eq!(d(1000, 0).divide(d(12, 0), 0, Round::Down).units(0), 83);
        assert_eq!(d(1000, 0).divide(d(12, 0), 2, Round::Up).units(2), 8334);
        assert_eq!(d(1050, 0).divide_exact(d(12, 0)).units(1), 875);
        assert_eq!(d(29, 1).divide_exact(d(2, 0)).divide_exact(d(100, 0)).units(4), 145);
        assert_eq!(d(15, 1).compare_to(d(150, 2)), 0);
        assert_eq!(d(-15, 1).compare_to(Dec::ZERO), -1);
        assert_eq!(d(1999, 2).long_value(), 19);
    }
}
