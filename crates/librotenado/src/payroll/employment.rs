// SPDX-License-Identifier: MIT
//! Employments and payslips (spec part 0013): who works for an owner, on what terms the law asks about, and what each
//! period's wages came to.

use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Date, Decimal, Money, Object, ParseError, PartyId};

macro_rules! uuid_id {
    ($name:ident, $error:ident, $what:literal) => {
        #[doc = concat!("The identity of ", $what, ": a UUID in its one form (spec part 0001). It carries no meaning.")]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// The identity as text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = ParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if crate::is_uuid(s) { Ok($name(s.to_owned())) } else { Err(ParseError::$error(s.to_owned())) }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
            }
        }
    };
}

uuid_id!(EmploymentId, Employment, "an employment");
uuid_id!(PayslipId, Payslip, "a payslip");

/// An employment: a person working for an owner of the books, with what the law of payroll asks about it (spec part
/// 0013). A later description with the same identity and a later `from` replaces the earlier one from that day.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Employment {
    /// Its identity.
    pub id: EmploymentId,
    /// The employer, an owner of the books.
    pub owner: PartyId,
    /// The employee, a person (spec part 0008).
    pub employee: PartyId,
    /// The first day this description applies.
    pub from: Date,
    /// The first day of the employment.
    pub start: Date,
    /// Its last day; left out while it lasts.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub end: Option<Date>,
    /// The employer's number for the employee (Personalnummer); left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub number: Option<String>,
    /// The employee's day of birth; left out when not given.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub born: Option<Date>,
    /// What German law asks about it; left out for an employment not under it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub de: Option<GermanEmployment>,
}

impl Object for Employment {
    const TYPE: &'static str = "employment";
}

/// An employment under German law: the features for the wage tax and the social insurance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GermanEmployment {
    /// The wage tax.
    pub tax: GermanTax,
    /// The social insurance.
    pub insurance: GermanInsurance,
}

/// The features of an employee for the wage tax: the electronic ones the tax authority gives (ELStAM, § 39e EStG),
/// or a flat tax instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GermanTax {
    /// The tax class, 1 to 6; left out where the wages are taxed at a flat rate (`flat`).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub class: Option<u8>,
    /// The factor of class 4 (§ 39f EStG), three places; left out without.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub factor: Option<Decimal>,
    /// The number of child allowances, one place (`"1.5"`); left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub children: Option<Decimal>,
    /// The religious community that levies church tax, as the ELStAM name it (`ev`, `rk`, …); left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub church: Option<String>,
    /// The allowance (Freibetrag) per year and per month; left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub allowance: Option<YearAndMonth>,
    /// The addition (Hinzurechnungsbetrag) per year and per month; left out when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub addition: Option<YearAndMonth>,
    /// The wages of a minor employment taxed at the flat rate of 2 % instead (§ 40a Abs. 2 EStG); left out otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flat: bool,
}

/// An amount per year and per month.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct YearAndMonth {
    /// Per year.
    pub year: Money,
    /// Per month.
    pub month: Money,
}

/// How an employee is insured, as the employer reports it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GermanInsurance {
    /// The company number (Betriebsnummer) of the health insurer that collects the contributions – for a minor
    /// employment the Minijob-Zentrale.
    pub insurer: String,
    /// The key of the contribution groups (Beitragsgruppenschlüssel): four digits for health, pension, unemployment
    /// and care insurance.
    pub contribution_group: String,
    /// The key of the group of persons (Personengruppenschlüssel), three digits.
    pub person_group: String,
    /// The additional rate of the health insurer in percent (Zusatzbeitragssatz, § 242 SGB V); left out when the
    /// employee is not in the statutory health insurance.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub additional_rate: Option<Decimal>,
    /// For an employee insured privately: the monthly premium for basic health and compulsory care insurance and the
    /// employer's subsidy to it (§ 257 SGB V, § 61 SGB XI); left out otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub private: Option<PrivateInsurance>,
    /// Whether the employee pays the surcharge of the care insurance for being without children (§ 55 Abs. 3 SGB XI).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub childless: bool,
    /// The number of children under 25 the care insurance counts, from which the second to the fifth reduce the
    /// employee's rate (§ 55 Abs. 3 SGB XI); left out when none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub children: u8,
    /// Whether the employee works in Saxony, where the employee pays more of the care insurance (§ 58 Abs. 3 SGB XI).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub saxony: bool,
    /// The rates of the levies for sick pay and maternity (U1, U2, § 7 AAG) of the insurer in percent; each left out
    /// when the employer does not take part.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub u1: Option<Decimal>,
    /// The rate of U2.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub u2: Option<Decimal>,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A private health and care insurance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrivateInsurance {
    /// The monthly premium for basic health and compulsory care insurance.
    pub premium: Money,
    /// The employer's monthly subsidy to it.
    pub subsidy: Money,
}

/// The wages of a period and what was withheld and paid on them (spec part 0013). It records the figures as they were
/// computed; an implementation reports one the rules would compute otherwise, and never changes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Payslip {
    /// Its identity.
    pub id: PayslipId,
    /// The employment.
    pub employment: EmploymentId,
    /// The first day of the period the wages are for.
    pub from: Date,
    /// Its last day.
    pub to: Date,
    /// The day the wages are paid.
    pub paid: Date,
    /// What the wages are made of.
    #[serde(deserialize_with = "crate::non_empty")]
    pub items: Vec<PayItem>,
    /// The figures: what was withheld and what the employer pays, by name (below); left out when there are none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub figures: BTreeMap<String, Money>,
}

impl Object for Payslip {
    const TYPE: &'static str = "payslip";
}

/// A part of the wages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PayItem {
    /// What it is.
    pub kind: PayItemKind,
    /// Its name on the payslip.
    #[serde(deserialize_with = "crate::text")]
    pub name: String,
    /// Its amount.
    pub amount: Money,
}

/// What a part of the wages is, and so how it is taxed and insured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayItemKind {
    /// Current wages of the period – salary, hourly wages, regular allowances: taxed and insured.
    Wage,
    /// A one-off payment – a bonus, holiday or Christmas pay: taxed as such (sonstiger Bezug), insured as such
    /// (Einmalzahlung).
    OneOff,
    /// Compensation for the loss of the employment (§ 24 Nr. 1 EStG): taxed as a one-off payment by the fifth, not
    /// insured.
    Compensation,
    /// A benefit in kind (Sachbezug): taxed and insured like wages, not paid out.
    InKind,
    /// Paid free of tax and contributions – a reimbursement of expenses.
    TaxFree,
    /// Deducted from the net pay – an advance, an attachment.
    Deduction,
}
