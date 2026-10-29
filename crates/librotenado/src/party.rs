// SPDX-License-Identifier: MIT
//! Parties (spec part 0008): the organisations and natural persons a set of books belongs to.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Currency, Date, DeviceId, Endpoint, Money, Object, ParseError, PostalAddress};

/// The identity of an organisation or a person: a UUID in its one form (spec part 0001). It carries no meaning.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartyId(String);

impl PartyId {
    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PartyId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(PartyId(s.to_owned())) } else { Err(ParseError::Party(s.to_owned())) }
    }
}

impl fmt::Display for PartyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for PartyId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PartyId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// A country by its ISO 3166-1 alpha-2 code, e.g. `DE`, `FR`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Country([u8; 2]);

impl Country {
    /// The code as text.
    pub fn as_str(&self) -> &str {
        // Only ASCII upper-case letters get in (see from_str).
        std::str::from_utf8(&self.0).unwrap_or("??")
    }
}

impl FromStr for Country {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.as_bytes() {
            &[a, b] if a.is_ascii_uppercase() && b.is_ascii_uppercase() => Ok(Country([a, b])),
            _ => Err(ParseError::Country(s.to_owned())),
        }
    }
}

impl fmt::Display for Country {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Country {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Country {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// A subdivision of a country by its ISO 3166-2 code, e.g. `DE-BY`: the country, `-`, and one to three upper-case
/// letters or digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Region(String);

impl Region {
    /// The code as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The country it is part of.
    pub fn country(&self) -> Country {
        self.0[..2].parse().expect("checked when read")
    }
}

impl FromStr for Region {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let fits = s.get(..2).is_some_and(|c| c.parse::<Country>().is_ok())
            && s.get(2..3) == Some("-")
            && (1..=3).contains(&(s.len() - 3))
            && s[3..].bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        if fits { Ok(Region(s.to_owned())) } else { Err(ParseError::Region(s.to_owned())) }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Region {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Region {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// An entry in a public register, e.g. a commercial register.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Registration {
    /// The register, e.g. `de-hra`, `de-hrb`, `de-vr` (names of registers are listed in the specification).
    pub register: String,
    /// The authority that keeps it, e.g. a local court; left out if the register has only one.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub authority: Option<String>,
    /// The number in the register.
    pub number: String,
}

/// A number under which a tax authority knows a party.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaxId {
    /// The scheme, e.g. `vat` (an EU VAT identification number, with its country prefix) or `de-steuernummer`.
    pub scheme: String,
    /// The number as the authority writes it.
    pub number: String,
}

/// A number of shares of one nominal amount: `{"count": 512, "nominal": 2 EUR}` – a share capital of 1,024 euros in
/// 512 shares of 2 euros. Shares are counted in whole numbers; fractions of the capital follow from the counts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shares {
    /// How many, at least 1.
    #[serde(deserialize_with = "count")]
    pub count: u64,
    /// The nominal amount of each, above zero.
    #[serde(deserialize_with = "nominal")]
    pub nominal: Money,
}

fn count<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    match crate::safe_u64(deserializer)? {
        0 => Err(de::Error::custom("at least one share")),
        count => Ok(count),
    }
}

fn nominal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Money, D::Error> {
    let nominal = Money::deserialize(deserializer)?;
    if nominal.base_units() <= 0 {
        return Err(de::Error::custom("a nominal amount is above zero"));
    }
    Ok(nominal)
}

/// An organisation: a company, an association, a foundation, a partnership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Organisation {
    /// Its identity.
    pub id: PartyId,
    /// Its name, as registered.
    pub name: String,
    /// Its legal form, e.g. `de-gmbh`, `de-ev`, `fr-sas` (codes are listed in the specification).
    pub legal_form: String,
    /// The country whose law it is formed under.
    pub country: Country,
    /// Its entries in public registers; left out when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub registrations: Vec<Registration>,
    /// Its tax numbers; left out when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub tax_ids: Vec<TaxId>,
    /// How its capital is divided into shares, one entry per nominal amount; left out when it has no shares.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub shares: Vec<Shares>,
    /// How to reach it; left out when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
}

impl Organisation {
    /// The share capital: the sum of count × nominal amount, if all nominal amounts are in one currency.
    pub fn capital(&self) -> Option<Money> {
        let currency = self.shares.first()?.nominal.currency().clone();
        let mut total: i128 = 0;
        for shares in &self.shares {
            if *shares.nominal.currency() != currency {
                return None;
            }
            total = total.checked_add(i128::from(shares.count).checked_mul(shares.nominal.base_units())?)?;
        }
        Money::new(total, currency)
    }
}

/// What a person is to an organisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    /// Sole owner of an organisation without shares, e.g. a sole trader's business.
    Owner,
    /// Holds shares of a company.
    Shareholder,
    /// Partner of a partnership.
    Partner,
    /// Manages and represents it, e.g. a managing director.
    ManagingDirector,
    /// Member of its board.
    BoardMember,
    /// Member of an association or a cooperative.
    Member,
    /// May sign for it, e.g. holds a commercial power of attorney.
    AuthorisedSignatory,
    /// A role of a later version; kept as received, not interpreted, and not written by this version.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// A person's role in an organisation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Role {
    /// The organisation.
    pub organisation: PartyId,
    /// What the person is to it.
    pub kind: RoleKind,
    /// The shares held in this role, of one nominal amount the organisation defines; left out where the role has
    /// none. A person holding shares of several nominal amounts has one role per amount.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "held")]
    pub shares: Option<Shares>,
}

fn held<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Shares>, D::Error> {
    Shares::deserialize(deserializer).map(Some)
}

/// A natural person.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Person {
    /// Their identity.
    pub id: PartyId,
    /// Their name, as they use it.
    pub name: String,
    /// Their tax numbers, e.g. a personal tax identification number; left out when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub tax_ids: Vec<TaxId>,
    /// Their roles in organisations; left out when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub roles: Vec<Role>,
    /// How to reach them; left out when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
}

/// How to reach an organisation or a person – for invoices to and from them, and filings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Contact {
    /// The postal address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<PostalAddress>,
    /// Whom to contact.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub name: Option<String>,
    /// The telephone number.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub phone: Option<String>,
    /// The e-mail address.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub email: Option<String>,
    /// Where electronic invoices go, as EN 16931 writes it (BT-34, BT-49): a scheme and an address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<Endpoint>,
    /// What it asks to be named on invoices to it – the Leitweg-ID of a German authority (BT-10).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub buyer_reference: Option<String>,
}

impl Object for Contact {
    const TYPE: &'static str = "contact";
}

/// How an owner writes invoices (spec part 0008).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Invoicing {
    /// The owner.
    pub owner: PartyId,
    /// What invoice numbers start with, before the year: `RE-`.
    pub prefix: String,
    /// The days until an invoice is due; left out when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_days: Option<u16>,
    /// The account invoices are paid to, an IBAN; left out when not given.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub account: Option<String>,
}

impl Object for Invoicing {
    const TYPE: &'static str = "invoicing";
}

/// The series of invoice numbers a device writes for an owner, so that devices working offline never write the
/// same number (spec part 0008): empty, or one to four of `A-Z` and `0-9`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Series {
    /// The owner.
    pub owner: PartyId,
    /// The device.
    pub device: DeviceId,
    /// The series.
    #[serde(deserialize_with = "series_name")]
    pub series: String,
}

impl Object for Series {
    const TYPE: &'static str = "series";
}

fn series_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let name = String::deserialize(deserializer)?;
    let valid = name.len() <= 4 && name.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    if valid { Ok(name) } else { Err(de::Error::custom("a series is up to four of A-Z and 0-9")) }
}

/// A code of a configuration: the country in lower case, `-`, and a name of `a-z`, `0-9` and `-`
/// (`de-cash-basis`, `de-small-business`, `de-skr03`). What it means is listed per country in the specification.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegimeCode(String);

impl RegimeCode {
    /// The code as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RegimeCode {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // The form of a tax code.
        match s.parse::<crate::TaxCode>() {
            Ok(_) => Ok(RegimeCode(s.to_owned())),
            Err(_) => Err(ParseError::RegimeCode(s.to_owned())),
        }
    }
}

impl fmt::Display for RegimeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RegimeCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RegimeCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// How an owner keeps its books from a date on, until its next configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Configuration {
    /// The organisation or person, an owner of the books.
    pub owner: PartyId,
    /// The first day it applies.
    pub from: Date,
    /// The functional currency the owner's statements are in.
    pub currency: Currency,
    /// The month, 1 to 12, on whose first day the owner's financial year begins.
    #[serde(deserialize_with = "month")]
    pub year_start: u8,
    /// The national chart of accounts the owner works with (`de-skr03`), whose translation valid on a date applies;
    /// left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub chart: Option<RegimeCode>,
    /// How profit is determined (`de-cash-basis`, `de-double-entry`); left out when none is – a private household.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub accounting: Option<RegimeCode>,
    /// How the owner is treated for VAT; left out when it is not liable.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub vat: Option<VatScheme>,
    /// The subdivision of the country whose public holidays move the owner's deadlines – where its tax authority
    /// sits (`DE-BY`); left out when not known.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub region: Option<Region>,
}

fn month<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    let month = u8::deserialize(deserializer)?;
    if (1..=12).contains(&month) { Ok(month) } else { Err(de::Error::custom("a month is 1 to 12")) }
}

/// How an owner is treated for VAT.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VatScheme {
    /// The scheme (`de-regular`, `de-small-business`).
    pub scheme: RegimeCode,
    /// When VAT is due.
    pub basis: VatBasis,
    /// The period of advance returns.
    pub period: ReturnPeriod,
    /// Whether the tax authority has extended the deadlines of advance returns for good (`de`: § 46 UStDV); left
    /// out when not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not", deserialize_with = "only_true")]
    pub extended: bool,
}

/// A flag that, when present, is `true` – absent is the one form of `false`.
fn only_true<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    if bool::deserialize(deserializer)? { Ok(true) } else { Err(de::Error::custom("false is written by leaving out")) }
}

/// When VAT is due: on what was agreed, or on what was received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VatBasis {
    /// On the consideration agreed – with the invoice or the supply.
    Accrual,
    /// On the consideration received – with the payment.
    Cash,
}

/// The period of advance VAT returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnPeriod {
    /// Every month.
    Month,
    /// Every quarter.
    Quarter,
    /// None: only the annual return.
    Year,
}

impl Object for Configuration {
    const TYPE: &'static str = "configuration";
}

impl Object for VatScheme {
    const TYPE: &'static str = "vat_scheme";
}

impl Object for Organisation {
    const TYPE: &'static str = "organisation";
}

impl Object for Person {
    const TYPE: &'static str = "person";
}

impl Object for Registration {
    const TYPE: &'static str = "registration";
}

impl Object for TaxId {
    const TYPE: &'static str = "tax_id";
}

impl Object for Shares {
    const TYPE: &'static str = "shares";
}

impl Object for Role {
    const TYPE: &'static str = "role";
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Money, from_root, to_root};

    pub(crate) fn party(n: u8) -> PartyId {
        format!("00000000-0000-7000-8000-0000000000{n:02x}").parse().unwrap()
    }

    pub(crate) fn organisation(n: u8) -> Organisation {
        Organisation {
            id: party(n),
            name: "Example Trading GmbH".into(),
            legal_form: "de-gmbh".into(),
            country: "DE".parse().unwrap(),
            registrations: vec![Registration {
                register: "de-hrb".into(),
                authority: Some("Amtsgericht Musterstadt".into()),
                number: "HRB 12345".into(),
            }],
            tax_ids: vec![TaxId { scheme: "vat".into(), number: "DE000000000".into() }],
            shares: vec![two_euro_shares(512)],
            contact: None,
        }
    }

    pub(crate) fn two_euro_shares(count: u64) -> Shares {
        Shares { count, nominal: Money::new(200, "EUR".parse().unwrap()).unwrap() }
    }

    #[test]
    fn the_capital_follows_from_the_shares() {
        assert_eq!(organisation(1).capital(), Money::new(102_400, "EUR".parse().unwrap()));
        let none = Organisation { shares: vec![], ..organisation(1) };
        assert_eq!(none.capital(), None);
    }

    #[test]
    fn an_organisation_as_a_root_object() {
        let org = organisation(1);
        let json = to_root(&org).unwrap();
        let text = String::from_utf8_lossy(&json);
        assert!(text.starts_with(r#"{"country":"DE","id":"#) && text.ends_with(r#""type":"organisation"}"#), "{text}");
        assert_eq!(from_root::<Organisation>(&json).unwrap(), org);
    }

    #[test]
    fn a_person_with_roles_and_shares() {
        let json = br#"{"type":"person","id":"00000000-0000-7000-8000-000000000002","name":"Erika Beispiel",
            "roles":[{"organisation":"00000000-0000-7000-8000-000000000001","kind":"shareholder",
                      "shares":{"count":257,"nominal":{"currency":"EUR","base_units":"200"}}},
                     {"organisation":"00000000-0000-7000-8000-000000000001","kind":"managing_director"}]}"#;
        let person: Person = from_root(json).unwrap();
        assert_eq!(person.roles[0].shares, Some(two_euro_shares(257)));
        assert_eq!(person.roles[1].kind, RoleKind::ManagingDirector);
        for (from, to) in [("257", "0"), ("257", "2.5"), (r#""200""#, r#""0""#), (r#""200""#, r#""-200""#)] {
            let bad = String::from_utf8_lossy(json).replace(from, to);
            assert!(from_root::<Person>(bad.as_bytes()).is_err(), "{to}");
        }
    }

    #[test]
    fn countries_are_two_upper_case_letters() {
        assert_eq!("DE".parse::<Country>().unwrap().as_str(), "DE");
        for bad in ["de", "DEU", "D", ""] {
            assert!(bad.parse::<Country>().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_configuration_with_a_region_and_extended_deadlines() {
        let json = r#"{"type":"configuration","owner":"01a182f7-0000-7000-8000-000000000001","from":"2026-01-01","currency":"EUR","year_start":1,"vat":{"scheme":"de-regular","basis":"cash","period":"quarter","extended":true},"region":"DE-BY"}"#;
        let configuration: Configuration = crate::from_root(json.as_bytes()).unwrap();
        assert_eq!(configuration.region.as_ref().map(Region::as_str), Some("DE-BY"));
        assert_eq!(configuration.region.as_ref().map(Region::country), Some("DE".parse().unwrap()));
        assert!(configuration.vat.as_ref().unwrap().extended);
        let again: Configuration = crate::from_root(&crate::to_root(&configuration).unwrap()).unwrap();
        assert_eq!(again, configuration);
        // Absent is the one form of no extension; a region has its form.
        assert!(crate::from_root::<Configuration>(json.replace("true", "false").as_bytes()).is_err());
        for region in ["DE", "de-BY", "DE-", "DE-BYXX", "DE-by"] {
            assert!(region.parse::<Region>().is_err(), "{region}");
        }
    }
}
