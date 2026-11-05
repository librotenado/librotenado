// SPDX-License-Identifier: MIT
//! Currencies, how they divide, and amounts of money.

use std::{fmt, ops::Neg, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Date, Decimal, Object, ParseError};

/// A currency by its code: three upper-case letters for an ISO 4217 currency (`EUR`, `CHF`), `x-` and 1–62 characters
/// of `a-z`, `0-9` and `-` for a currency defined outside ISO 4217 (`x-btc`, `x-galleon`).
///
/// The two forms cannot collide. Only the form is checked; whether a code denotes an existing currency, and how it
/// divides, is reference data ([`CurrencyDefinition`]), not protocol.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Currency(String);

impl Currency {
    /// The code as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is an ISO 4217 code (three upper-case letters) rather than one beginning with `x-`.
    pub fn is_iso(&self) -> bool {
        !self.0.starts_with("x-")
    }
}

impl FromStr for Currency {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let iso = s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase());
        let custom = s.strip_prefix("x-").is_some_and(|name| {
            (1..=62).contains(&name.len())
                && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
        if iso || custom { Ok(Currency(s.to_owned())) } else { Err(ParseError::Currency(s.to_owned())) }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// An amount in a currency, counted in the currency's smallest unit: `{"currency": "EUR", "base_units": "-1240"}`
/// is −12.40 euros.
///
/// The count is exact and has one written form only – an integer as a string (`"-1240"`), never a JSON number –
/// so the same amount always has the same bytes and the same hash. Its magnitude is at most
/// [`Money::MAX_BASE_UNITS`]. Reading, checking and summing amounts never needs the currency's definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    currency: Currency,
    #[serde(with = "base_units")]
    base_units: i128,
}

impl Money {
    /// The largest magnitude of an amount in base units: 10^38 − 1. Every implementation can represent it.
    pub const MAX_BASE_UNITS: i128 = 10i128.pow(38) - 1;

    /// `base_units` of the currency's smallest unit; `None` if the magnitude is above [`Self::MAX_BASE_UNITS`].
    pub fn new(base_units: i128, currency: Currency) -> Option<Self> {
        (base_units.unsigned_abs() <= Self::MAX_BASE_UNITS.unsigned_abs()).then_some(Money { currency, base_units })
    }

    /// The currency.
    pub fn currency(&self) -> &Currency {
        &self.currency
    }

    /// The amount in the currency's smallest unit; its sign is given by its use (see [`crate::Posting`]).
    pub fn base_units(&self) -> i128 {
        self.base_units
    }
}

impl Object for Money {
    const TYPE: &'static str = "money";
}

impl Neg for Money {
    type Output = Money;

    /// The same amount with the opposite sign; the range is symmetric, so this cannot overflow.
    fn neg(self) -> Money {
        Money { currency: self.currency, base_units: -self.base_units }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} base units of {}", self.base_units, self.currency)
    }
}

/// The written form of an amount in base units: an integer as a string, in its one canonical form.
mod base_units {
    use serde::{Deserialize, Deserializer, Serializer, de};

    use super::Money;

    pub fn serialize<S: Serializer>(base_units: &i128, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(base_units)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i128, D::Error> {
        let s = String::deserialize(deserializer)?;
        parse(&s).ok_or_else(|| de::Error::custom(format!("not an amount in base units: {s:?}")))
    }

    /// An optional `-`, then `0` or a digit 1–9 followed by digits; not `-0`; at most [`Money::MAX_BASE_UNITS`].
    pub(super) fn parse(s: &str) -> Option<i128> {
        let digits = s.strip_prefix('-').unwrap_or(s);
        let canonical = !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && (digits == "0" || !digits.starts_with('0'))
            && s != "-0";
        let base_units: i128 = if canonical { s.parse().ok()? } else { return None };
        (base_units.unsigned_abs() <= Money::MAX_BASE_UNITS.unsigned_abs()).then_some(base_units)
    }
}

/// How a currency divides into its base unit – reference data, published in versions with the protocol.
///
/// `subdivisions` lists, from the unit down, how many of the next smaller stage one stage contains: `[100]` for EUR
/// (euro, cent), `[]` for JPY, `[17, 29]` for Galleons (Galleon, Sickle, Knut), `[1000000000, 1000000000]` for
/// `x-eth` (ether, gwei, wei). Stage `n` is the unit after `n` subdivisions; stage 0 is the unit itself, the last
/// stage the base unit. Names are not part of a definition: translation data names stage `n` by the key
/// `<code>/<n>` (`EUR/0` euro, `EUR/1` cent).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawDefinition", into = "RawDefinition")]
pub struct CurrencyDefinition {
    code: Currency,
    subdivisions: Vec<u64>,
    withdrawn: Option<Withdrawn>,
    fixed_rate: Option<FixedRate>,
}

/// The month a currency was withdrawn from ISO 4217 (list three), written `YYYY-MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Withdrawn {
    year: u16,
    month: u8,
}

impl Withdrawn {
    /// The month, if `month` is 1 to 12 and `year` 1 to 9999.
    pub fn new(year: u16, month: u8) -> Option<Self> {
        ((1..=9999).contains(&year) && (1..=12).contains(&month)).then_some(Withdrawn { year, month })
    }

    /// The year.
    pub fn year(self) -> u16 {
        self.year
    }

    /// The month, 1 to 12.
    pub fn month(self) -> u8 {
        self.month
    }

    /// Whether `day` lies after the month of withdrawal.
    pub fn is_before(self, day: Date) -> bool {
        (day.year(), day.month()) > (self.year, self.month)
    }
}

impl fmt::Display for Withdrawn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl FromStr for Withdrawn {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let error = || ParseError::Month(s.to_owned());
        let b = s.as_bytes();
        if b.len() != 7 || b[4] != b'-' || !s[..4].bytes().chain(s[5..].bytes()).all(|c| c.is_ascii_digit()) {
            return Err(error());
        }
        Withdrawn::new(s[..4].parse().map_err(|_| error())?, s[5..].parse().map_err(|_| error())?).ok_or_else(error)
    }
}

impl Serialize for Withdrawn {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Withdrawn {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// A rate fixed by law between a currency and the one that replaced it: `rate` units of the currency are one unit of
/// `currency` – 1.95583 Deutsche Mark per euro.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedRate {
    /// The currency that replaced it.
    pub currency: Currency,
    /// Units of the replaced currency per unit of `currency`, above zero.
    pub rate: Decimal,
}

impl Object for CurrencyDefinition {
    const TYPE: &'static str = "currency";
}

/// Why a currency definition is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DefinitionError {
    /// A stage containing fewer than 2 of the next, or more than 2^53 − 1.
    #[error("every subdivision is between 2 and 2^53 - 1")]
    Stage,
    /// More base units per unit than 10^38, the range of amounts.
    #[error("more than 10^38 base units per unit")]
    TooLarge,
    /// An ISO 4217 currency not divided as ISO 4217 does: once by a power of ten, or not at all.
    #[error("an ISO 4217 currency is divided once by a power of ten, or not at all")]
    Iso,
    /// A fixed rate that is not above zero, or to the currency itself.
    #[error("a fixed rate is above zero and to another currency")]
    FixedRate,
}

impl CurrencyDefinition {
    /// A definition, checked: every subdivision between 2 and 2^53 − 1, at most 10^38 base units per unit, and an
    /// ISO 4217 currency divided at most once, by a power of ten.
    pub fn new(code: Currency, subdivisions: Vec<u64>) -> Result<Self, DefinitionError> {
        let definition = CurrencyDefinition { code, subdivisions, withdrawn: None, fixed_rate: None };
        if definition.subdivisions.iter().any(|&s| !(2..=crate::MAX_SAFE_INTEGER).contains(&s)) {
            return Err(DefinitionError::Stage);
        }
        definition.base_units_per_unit().ok_or(DefinitionError::TooLarge)?;
        if definition.code.is_iso() && (definition.subdivisions.len() > 1 || definition.decimals().is_none()) {
            return Err(DefinitionError::Iso);
        }
        Ok(definition)
    }

    /// The definition of a currency withdrawn in the month `withdrawn` and, if given, replaced at a rate fixed by law.
    pub fn withdrawn(mut self, withdrawn: Withdrawn, fixed_rate: Option<FixedRate>) -> Result<Self, DefinitionError> {
        if fixed_rate.as_ref().is_some_and(|f| f.rate.units() <= 0 || f.currency == self.code) {
            return Err(DefinitionError::FixedRate);
        }
        self.withdrawn = Some(withdrawn);
        self.fixed_rate = fixed_rate;
        Ok(self)
    }

    /// The month the currency was withdrawn, if it was.
    pub fn withdrawal(&self) -> Option<Withdrawn> {
        self.withdrawn
    }

    /// The rate fixed by law to the currency that replaced it, if there is one.
    pub fn fixed_rate(&self) -> Option<&FixedRate> {
        self.fixed_rate.as_ref()
    }

    /// The code.
    pub fn code(&self) -> &Currency {
        &self.code
    }

    /// How many of the next smaller stage each stage contains, from the unit down.
    pub fn subdivisions(&self) -> &[u64] {
        &self.subdivisions
    }

    /// How many base units make one unit: 100 for EUR, 17 · 29 = 493 for Galleons in Knuts; `None` above 10^38.
    pub fn base_units_per_unit(&self) -> Option<u128> {
        let limit = Money::MAX_BASE_UNITS.unsigned_abs() + 1;
        self.subdivisions
            .iter()
            .try_fold(1u128, |product, &s| product.checked_mul(u128::from(s)).filter(|&p| p <= limit))
    }

    /// The number of decimal places if one unit is a power of ten base units – `Some(2)` for EUR, `Some(0)` for JPY,
    /// `None` for Galleons. Only then does money convert to and from decimal numbers.
    pub fn decimals(&self) -> Option<u32> {
        let mut per_unit = self.base_units_per_unit()?;
        let mut decimals = 0;
        while per_unit % 10 == 0 {
            per_unit /= 10;
            decimals += 1;
        }
        (per_unit == 1).then_some(decimals)
    }
}

/// A published table of currency definitions – reference data, versioned with the protocol (spec part 0001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrencyTable {
    /// The version of the table.
    pub version: String,
    /// The date of the ISO 4217 list the table follows.
    pub iso_4217: Date,
    /// The definitions, each code once.
    pub currencies: Vec<CurrencyDefinition>,
}

impl Object for CurrencyTable {
    const TYPE: &'static str = "currency_table";
}

impl CurrencyTable {
    /// The table of version 0.1: every currency of ISO 4217 with a minor unit, and `x-btc` and `x-eth`
    /// (`spec/data/currencies.json`).
    pub fn v0_1() -> Self {
        crate::from_root(include_bytes!("../data/currencies.json")).expect("the published table is valid")
    }

    /// The definition of this currency, if the table has it.
    pub fn get(&self, code: &Currency) -> Option<&CurrencyDefinition> {
        self.currencies.iter().find(|definition| definition.code() == code)
    }

    /// `money` converted at a rate fixed by law, between a replaced currency and the one that replaced it or between
    /// two currencies replaced by the same one (Regulation (EC) No 1103/97, Articles 4 and 5): rounded half away from
    /// zero to the base unit of the target, between two replaced currencies through an amount in the replacing
    /// currency rounded to three decimal places. `None` without such a rate, for a currency that is not decimal, or
    /// out of range.
    pub fn convert_fixed(&self, money: &Money, to: &Currency) -> Option<Money> {
        let from = self.get(money.currency())?;
        let target = self.get(to)?;
        let (from_decimals, to_decimals) = (from.decimals()?, target.decimals()?);
        let (units, rate_from) = (money.base_units(), from.fixed_rate.as_ref());
        // An amount in the replacing currency, `units · 10^-scale`.
        let (common, scale, decimals) = match (rate_from, target.fixed_rate.as_ref()) {
            (Some(rate), _) if &rate.currency == to => {
                return Money::new(divide(units, from_decimals, rate, to_decimals)?, to.clone());
            }
            (_, Some(rate)) if &rate.currency == money.currency() => {
                return Money::new(multiply(units, from_decimals, rate, to_decimals)?, to.clone());
            }
            (Some(a), Some(b)) if a.currency == b.currency => (divide(units, from_decimals, a, 3)?, 3, b),
            _ => return None,
        };
        Money::new(multiply(common, scale, decimals, to_decimals)?, to.clone())
    }

    /// Whether every code occurs once.
    pub fn is_valid(&self) -> bool {
        let mut codes: Vec<&Currency> = self.currencies.iter().map(CurrencyDefinition::code).collect();
        codes.sort();
        codes.windows(2).all(|pair| pair[0] != pair[1])
    }
}

/// `n · 10^-scale`, rounded half away from zero to `decimals` places, as an integer of them; `n · 10^(decimals -
/// scale) · numerator / denominator` in general.
fn scaled(n: i128, scale: u32, decimals: u32, numerator: i128, denominator: i128) -> Option<i128> {
    let (mut top, mut bottom) = (n.checked_mul(numerator)?, denominator);
    if decimals >= scale {
        top = top.checked_mul(10i128.checked_pow(decimals - scale)?)?;
    } else {
        bottom = bottom.checked_mul(10i128.checked_pow(scale - decimals)?)?;
    }
    let half = bottom / 2;
    Some(if top >= 0 { (top + half) / bottom } else { (top - half) / bottom })
}

/// Base units of the replaced currency, divided by the rate, in units of `decimals` places of the replacing one.
fn divide(units: i128, scale: u32, rate: &FixedRate, decimals: u32) -> Option<i128> {
    let factor = 10i128.checked_pow(u32::from(rate.rate.scale()))?;
    scaled(units, scale, decimals, factor, rate.rate.units())
}

/// An amount of the replacing currency, multiplied by the rate, in units of `decimals` places of the replaced one.
fn multiply(units: i128, scale: u32, rate: &FixedRate, decimals: u32) -> Option<i128> {
    let factor = 10i128.checked_pow(u32::from(rate.rate.scale()))?;
    scaled(units, scale, decimals, rate.rate.units(), factor)
}

/// The written form of a definition: `{"code": …, "subdivisions": […]}`, and for a withdrawn currency `withdrawn` and
/// `fixed_rate`.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDefinition {
    code: Currency,
    subdivisions: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    withdrawn: Option<Withdrawn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fixed_rate: Option<FixedRate>,
}

impl TryFrom<RawDefinition> for CurrencyDefinition {
    type Error = DefinitionError;

    fn try_from(raw: RawDefinition) -> Result<Self, Self::Error> {
        let definition = CurrencyDefinition::new(raw.code, raw.subdivisions)?;
        match raw.withdrawn {
            Some(withdrawn) => definition.withdrawn(withdrawn, raw.fixed_rate),
            None if raw.fixed_rate.is_some() => Err(DefinitionError::FixedRate),
            None => Ok(definition),
        }
    }
}

impl From<CurrencyDefinition> for RawDefinition {
    fn from(definition: CurrencyDefinition) -> Self {
        RawDefinition {
            code: definition.code,
            subdivisions: definition.subdivisions,
            withdrawn: definition.withdrawn,
            fixed_rate: definition.fixed_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eur() -> Currency {
        "EUR".parse().unwrap()
    }

    #[test]
    fn currency_codes_are_iso_or_begin_with_x() {
        for s in ["EUR", "XAU", "x-btc", "x-galleon", "x-bonus-points-2026"] {
            assert_eq!(s.parse::<Currency>().unwrap().to_string(), s);
        }
        assert!(eur().is_iso());
        assert!(!"x-btc".parse::<Currency>().unwrap().is_iso());
        for s in ["eur", "EU", "EURO", "E1R", "", "x-", "X-btc", "x-BTC", "x-b c", "btc"] {
            assert!(s.parse::<Currency>().is_err(), "{s:?}");
        }
    }

    #[test]
    fn money_is_base_units_as_a_string_in_json() {
        let money = Money::new(-1240, eur()).unwrap();
        let json = serde_json::to_string(&money).unwrap();
        assert_eq!(json, r#"{"currency":"EUR","base_units":"-1240"}"#);
        assert_eq!(serde_json::from_str::<Money>(&json).unwrap(), money);
    }

    #[test]
    fn base_units_have_one_written_form() {
        for s in ["0", "7", "-1240", "99999999999999999999999999999999999999"] {
            assert_eq!(base_units::parse(s).unwrap().to_string(), s);
        }
        for s in
            ["", "-", "-0", "+1", "01", "-01", "1.0", "12.40", "1e3", " 1", "100000000000000000000000000000000000000"]
        {
            assert!(base_units::parse(s).is_none(), "{s:?}");
        }
        assert!(serde_json::from_str::<Money>(r#"{"currency":"EUR","base_units":1240}"#).is_err());
    }

    #[test]
    fn the_range_is_symmetric() {
        assert!(Money::new(Money::MAX_BASE_UNITS + 1, eur()).is_none());
        assert_eq!((-Money::new(-Money::MAX_BASE_UNITS, eur()).unwrap()).base_units(), Money::MAX_BASE_UNITS);
    }

    fn definition(json: &str) -> Result<CurrencyDefinition, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn decimal_currencies() {
        let json = r#"{"code":"EUR","subdivisions":[100]}"#;
        let eur = definition(json).unwrap();
        assert_eq!((eur.base_units_per_unit(), eur.decimals()), (Some(100), Some(2)));
        assert_eq!(serde_json::to_string(&eur).unwrap(), json);
        let jpy = definition(r#"{"code":"JPY","subdivisions":[]}"#).unwrap();
        assert_eq!((jpy.base_units_per_unit(), jpy.decimals()), (Some(1), Some(0)));
        let eth = definition(r#"{"code":"x-eth","subdivisions":[1000000000,1000000000]}"#).unwrap();
        assert_eq!(eth.decimals(), Some(18));
    }

    #[test]
    fn other_currencies() {
        let galleon = definition(r#"{"code":"x-galleon","subdivisions":[17,29]}"#).unwrap();
        assert_eq!((galleon.base_units_per_unit(), galleon.decimals()), (Some(493), None));
        assert_eq!(galleon.subdivisions(), [17, 29]);
    }

    #[test]
    fn the_table_of_version_0_1() {
        let table = CurrencyTable::v0_1();
        assert!(table.is_valid());
        assert_eq!(table.currencies.len(), 167 + 21, "list one, bitcoin, ether, and the currencies the euro replaced");
        let decimals = |code: &str| table.get(&code.parse().unwrap()).and_then(CurrencyDefinition::decimals);
        assert_eq!(
            [decimals("EUR"), decimals("JPY"), decimals("KWD"), decimals("CLF"), decimals("MGA")],
            [Some(2), Some(0), Some(3), Some(4), Some(2)]
        );
        assert_eq!((decimals("x-btc"), decimals("x-eth")), (Some(8), Some(18)));
        assert!(table.get(&"XAU".parse().unwrap()).is_none(), "units without a minor unit are not in the table");
    }

    #[test]
    fn invalid_definitions() {
        let x = || "x-test".parse::<Currency>().unwrap();
        for stages in [vec![1], vec![17, 0], vec![20, 1, 12], vec![1 << 53]] {
            assert_eq!(CurrencyDefinition::new(x(), stages), Err(DefinitionError::Stage));
        }
        let too_large = vec![1_000_000_000_000_000; 3];
        assert_eq!(CurrencyDefinition::new(x(), too_large), Err(DefinitionError::TooLarge));
        for stages in [vec![20, 12], vec![10, 10], vec![5]] {
            assert_eq!(CurrencyDefinition::new(eur(), stages), Err(DefinitionError::Iso));
        }
        for json in [
            r#"{"code":"EUR"}"#,
            r#"{"code":"EUR","decimals":2}"#,
            r#"{"code":"EUR","subdivisions":[100],"number":"978"}"#,
            r#"{"code":"EUR","subdivisions":[100.0]}"#,
        ] {
            assert!(definition(json).is_err(), "{json}");
        }
    }

    #[test]
    fn replaced_currencies_convert_at_the_fixed_rate() {
        let table = CurrencyTable::v0_1();
        assert!(table.is_valid());
        let (dem, eur, frf, itl): (Currency, Currency, Currency, Currency) =
            ("DEM".parse().unwrap(), eur(), "FRF".parse().unwrap(), "ITL".parse().unwrap());
        let dm = table.get(&dem).unwrap();
        assert_eq!(dm.withdrawal().unwrap().to_string(), "2002-03");
        assert_eq!(dm.fixed_rate().unwrap().rate.to_string(), "1.95583");
        assert!(dm.withdrawal().unwrap().is_before(Date::new(2002, 4, 1).unwrap()));
        assert!(!dm.withdrawal().unwrap().is_before(Date::new(2002, 3, 31).unwrap()));
        assert!(table.get(&eur).unwrap().withdrawal().is_none());
        // 100 DM = 51.129188… euros; 1,000 euros = 1,955.83 DM; −0.01 DM = −0.005… euros, half away from zero.
        let convert = |base_units, from: &Currency, to: &Currency| {
            table.convert_fixed(&Money::new(base_units, from.clone()).unwrap(), to).map(|m| m.base_units())
        };
        assert_eq!(convert(10000, &dem, &eur), Some(5113));
        assert_eq!(convert(100000, &eur, &dem), Some(195583));
        assert_eq!(convert(-1, &dem, &eur), Some(-1));
        // 1,000 lire = 0.516457 euros → 0.516 (three places) → 3.38 francs (Article 4(4)).
        assert_eq!(convert(1000, &itl, &eur), Some(52));
        assert_eq!(convert(1000, &itl, &frf), Some(338));
        // No rate between currencies that are not replaced.
        assert_eq!(convert(100, &eur, &"USD".parse().unwrap()), None);
        assert_eq!(convert(100, &eur, &eur), None);
        for json in [
            r#"{"code":"DEM","subdivisions":[100],"fixed_rate":{"currency":"EUR","rate":"1.95583"}}"#,
            r#"{"code":"DEM","subdivisions":[100],"withdrawn":"2002-3"}"#,
            r#"{"code":"DEM","subdivisions":[100],"withdrawn":"2002-13"}"#,
            r#"{"code":"DEM","subdivisions":[100],"withdrawn":"2002-03","fixed_rate":{"currency":"EUR","rate":"0"}}"#,
            r#"{"code":"DEM","subdivisions":[100],"withdrawn":"2002-03","fixed_rate":{"currency":"DEM","rate":"1"}}"#,
        ] {
            assert!(definition(json).is_err(), "{json}");
        }
        let json = r#"{"code":"DEM","subdivisions":[100],"withdrawn":"2002-03","fixed_rate":{"currency":"EUR","rate":"1.95583"}}"#;
        assert_eq!(serde_json::to_string(&definition(json).unwrap()).unwrap(), json);
    }
}
