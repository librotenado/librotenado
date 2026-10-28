// SPDX-License-Identifier: MIT
//! Tax codes (spec part 0001): how a posting is treated for value added tax, per country.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Date, Decimal, Object, ParseError};

/// A tax code: the country in lower case, `-`, and a name of `a-z`, `0-9` and `-` (`de-out-19`). It says how a
/// posting is treated for VAT; what that means is reference data ([`TaxTable`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaxCode(String);

impl TaxCode {
    /// The code as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for TaxCode {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = s.len() <= 64
            && s.as_bytes().get(2) == Some(&b'-')
            && s[..2].bytes().all(|b| b.is_ascii_lowercase())
            && s.len() > 3
            && s[3..].bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if valid { Ok(TaxCode(s.to_owned())) } else { Err(ParseError::TaxCode(s.to_owned())) }
    }
}

impl fmt::Display for TaxCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for TaxCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TaxCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// Whether a tax code is used on what the books' owner supplies or on what it receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxDirection {
    /// Supplies and services rendered: sales.
    Output,
    /// Supplies and services received: purchases.
    Input,
}

/// What a tax code means.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaxCodeDefinition {
    /// The code.
    pub code: TaxCode,
    /// Sales or purchases.
    pub direction: TaxDirection,
    /// The rate in percent (`"19"`), if the code taxes at one; absent for exempt or non-taxable transactions.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rate: Option<Decimal>,
    /// The provisions it rests on, e.g. `UStG § 4 Nr. 1 Buchst. b`.
    pub law: String,
}

/// The tax codes of one country, valid from a date – reference data, published with the protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxTable {
    /// The country, ISO 3166-1 alpha-2.
    pub country: crate::Country,
    /// The first day the table applies to.
    pub valid_from: Date,
    /// The codes, each once.
    pub codes: Vec<TaxCodeDefinition>,
}

impl Object for TaxCodeDefinition {
    const TYPE: &'static str = "tax_code";
}

impl Object for TaxTable {
    const TYPE: &'static str = "tax_table";
}

impl TaxTable {
    /// The German tax codes (`spec/data/tax/de.json`).
    pub fn de() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/tax/de.json")).expect("the published table is valid")
    }

    /// The definition of a code.
    pub fn get(&self, code: &TaxCode) -> Option<&TaxCodeDefinition> {
        self.codes.iter().find(|definition| definition.code == *code)
    }

    /// Whether every code occurs once and starts with the table's country.
    pub fn is_valid(&self) -> bool {
        let prefix = format!("{}-", self.country.as_str().to_ascii_lowercase());
        let mut codes: Vec<&TaxCode> = self.codes.iter().map(|d| &d.code).collect();
        codes.sort();
        codes.windows(2).all(|pair| pair[0] != pair[1]) && codes.iter().all(|c| c.as_str().starts_with(&prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_codes_name_their_country() {
        for s in ["de-out-19", "de-in-13b-eu-service", "fr-tva-20"] {
            assert_eq!(s.parse::<TaxCode>().unwrap().as_str(), s);
        }
        for s in ["DE-19", "de19", "de-", "de-Out", "deu-19", ""] {
            assert!(s.parse::<TaxCode>().is_err(), "{s}");
        }
    }

    #[test]
    fn the_german_table() {
        let table = TaxTable::de();
        assert!(table.is_valid());
        let rate = |code: &str| table.get(&code.parse().unwrap()).and_then(|d| d.rate);
        assert_eq!(rate("de-out-19"), Some("19".parse().unwrap()));
        assert_eq!(rate("de-in-7"), Some("7".parse().unwrap()));
        assert_eq!(rate("de-out-ig-supply"), None);
        assert_eq!(table.get(&"de-out-small-business".parse().unwrap()).unwrap().direction, TaxDirection::Output);
    }
}
