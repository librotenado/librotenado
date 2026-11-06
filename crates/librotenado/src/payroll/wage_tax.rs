// SPDX-License-Identifier: MIT
//! The wage tax an employer withholds in Germany (§ 39b EStG), by the Programmablaufplan of the BMF (spec part 0013).

use super::dec::Dec;
use super::pap2025::Pap2025;
use super::pap2026::Pap2026;

/// The largest amount, in cents, the wage tax is computed for – far above any wage, so that the arithmetic stays exact.
pub const MAX_AMOUNT: i128 = 10_000_000_000_000;

/// The period wages are paid for (Lohnzahlungszeitraum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayPeriod {
    /// A year.
    Year,
    /// A month.
    Month,
    /// A week.
    Week,
    /// A day.
    Day,
}

/// How an employee is insured for health and care, as far as the wage tax asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthCover {
    /// In the statutory insurance, with the additional rate of the insurer in hundredths of a percent (2.90 % = 290).
    Statutory {
        /// The additional rate.
        additional_rate: u32,
    },
    /// Privately insured only: the monthly premium for basic health and compulsory care insurance, and the employer's
    /// subsidy to it, in cents (§ 10 Abs. 1 Nr. 3 EStG).
    Private {
        /// The premium.
        premium: i128,
        /// The subsidy.
        subsidy: i128,
    },
}

/// The employee's features for the wage tax: the electronic ones (ELStAM, § 39e EStG) and how they are insured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WageTaxFacts {
    /// The tax class, 1 to 6.
    pub class: u8,
    /// The factor of class 4 in thousandths (0.913 = 913, § 39f EStG); `None` without.
    pub factor: Option<u32>,
    /// The number of child allowances in tenths (1.5 = 15), classes 1 to 4.
    pub children: u32,
    /// Whether church tax is withheld.
    pub church: bool,
    /// The allowance (Freibetrag) for a year and for the period, in cents.
    pub allowance: (i128, i128),
    /// The addition (Hinzurechnungsbetrag) for a year and for the period, in cents.
    pub addition: (i128, i128),
    /// The calendar year after the one in which the employee turned 64, where that was before the year of the wages
    /// (§ 24a EStG); `None` otherwise.
    pub age_relief_from: Option<u16>,
    /// Health and care.
    pub health: HealthCover,
    /// Whether the employee pays into the statutory pension insurance or a professional scheme in its place.
    pub pension: bool,
    /// Whether the employee pays into the unemployment insurance.
    pub unemployment: bool,
    /// Whether the employee pays the surcharge to the care insurance for having no children (§ 55 Abs. 3 SGB XI).
    pub childless: bool,
    /// The reductions of the care insurance for the second to fifth child under 25, 0 to 4 (§ 55 Abs. 3 SGB XI).
    pub care_reductions: u8,
    /// Whether the employee works in Saxony, which splits the care insurance otherwise (§ 58 Abs. 3 SGB XI).
    pub saxony: bool,
    /// For pensions: the year they began and the pension of their first whole month, in cents (§ 19 Abs. 2 Satz 3,
    /// 4 EStG – the base of the allowance for pensions).
    pub pension_start: Option<(u16, i128)>,
}

impl Default for WageTaxFacts {
    fn default() -> Self {
        WageTaxFacts {
            class: 1,
            factor: None,
            children: 0,
            church: false,
            allowance: (0, 0),
            addition: (0, 0),
            age_relief_from: None,
            health: HealthCover::Statutory { additional_rate: 0 },
            pension: true,
            unemployment: true,
            childless: false,
            care_reductions: 0,
            saxony: false,
            pension_start: None,
        }
    }
}

/// The wages of a period, in cents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wages {
    /// The current wages of the period (laufender Arbeitslohn), taxable.
    pub current: i128,
    /// One-off payments in the period (sonstige Bezüge) – a bonus, holiday or Christmas pay.
    pub one_off: i128,
    /// Of those, compensation taxed by the fifth (§ 24 Nr. 1, § 34 EStG).
    pub compensation: i128,
    /// Of the current wages, pensions (Versorgungsbezüge, § 19 Abs. 2 EStG).
    pub pension: i128,
    /// For one-off payments: the wages expected for the year without them, with the one-off payments of earlier periods
    /// of the year (§ 39b Abs. 3 EStG).
    pub expected_year: i128,
}

/// The wage tax of a period, in cents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WageTax {
    /// The wage tax on the current wages.
    pub tax: i128,
    /// The solidarity surcharge on it.
    pub solidarity: i128,
    /// The base of church tax on it.
    pub church_base: i128,
    /// The wage tax on the one-off payments.
    pub one_off_tax: i128,
    /// The solidarity surcharge on it.
    pub one_off_solidarity: i128,
    /// The base of church tax on it.
    pub one_off_church_base: i128,
}

/// Why the wage tax cannot be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WageTaxError {
    /// A year without a Programmablaufplan in this version.
    #[error("no Programmablaufplan for the year {0}")]
    Year(u16),
    /// A tax class that does not exist.
    #[error("tax class {0} – classes are 1 to 6")]
    Class(u8),
    /// An amount below zero or above [`MAX_AMOUNT`].
    #[error("an amount is negative or too large")]
    Amount,
}

fn cents(units: i128) -> Result<Dec, WageTaxError> {
    if (0..=MAX_AMOUNT).contains(&units) { Ok(Dec::lit(units, 0)) } else { Err(WageTaxError::Amount) }
}

/// The wage tax, solidarity surcharge and base of church tax of a period (§ 39b EStG), by the Programmablaufplan of the
/// year: 2025 and 2026 (their XML pseudocode, of 17 September and 23 October 2025); other years are not part of
/// this version.
pub fn wage_tax(year: u16, period: PayPeriod, wages: &Wages, facts: &WageTaxFacts) -> Result<WageTax, WageTaxError> {
    compute(year, period, wages, facts).map(|(tax, _)| tax)
}

/// One value of the PAP after it ran: its name in the PAP, the PAP's explanation of it, and the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WageTaxStep {
    /// The name of the variable in the PAP (`ZVE`, `VSP`, `LSTLZZ`).
    pub name: &'static str,
    /// The PAP's explanation of it.
    pub description: &'static str,
    /// Its value, as the PAP holds it – cents or euros as the explanation says.
    pub value: String,
}

/// The way the wage tax of a period is computed: every input, internal field and output of the Programmablaufplan
/// after it ran, in the PAP's order, with the PAP's explanation of each.
pub fn wage_tax_steps(
    year: u16,
    period: PayPeriod,
    wages: &Wages,
    facts: &WageTaxFacts,
) -> Result<Vec<WageTaxStep>, WageTaxError> {
    compute(year, period, wages, facts).map(|(_, steps)| steps)
}

fn compute(
    year: u16,
    period: PayPeriod,
    wages: &Wages,
    facts: &WageTaxFacts,
) -> Result<(WageTax, Vec<WageTaxStep>), WageTaxError> {
    if !(1..=6).contains(&facts.class) {
        return Err(WageTaxError::Class(facts.class));
    }
    match year {
        2025 => run!(Pap2025, period, wages, facts, |_pap: &mut Pap2025| Ok(())),
        2026 => run!(Pap2026, period, wages, facts, |pap: &mut Pap2026| -> Result<(), WageTaxError> {
            // From 2026 the PAP asks for the unemployment insurance and the employer's subsidy to a private
            // insurance.
            pap.ALV = i64::from(!facts.unemployment);
            if let HealthCover::Private { subsidy, .. } = facts.health {
                pap.PKPVAGZ = cents(subsidy)?;
            }
            Ok(())
        }),
        _ => Err(WageTaxError::Year(year)),
    }
}

/// Sets the inputs both PAPs share, runs the PAP of the year, and reads its outputs.
macro_rules! run {
    ($pap:ident, $period:expr, $wages:expr, $facts:expr, $more:expr) => {{
        let (period, wages, facts) = ($period, $wages, $facts);
        let mut pap = $pap {
            LZZ: match period {
                PayPeriod::Year => 1,
                PayPeriod::Month => 2,
                PayPeriod::Week => 3,
                PayPeriod::Day => 4,
            },
            STKL: i64::from(facts.class),
            RE4: cents(wages.current)?,
            SONSTB: cents(wages.one_off)?,
            SONSTENT: cents(wages.compensation)?,
            JRE4: cents(wages.expected_year)?,
            ZKF: Dec::lit(i128::from(facts.children), 1),
            R: i64::from(facts.church),
            JFREIB: cents(facts.allowance.0)?,
            LZZFREIB: cents(facts.allowance.1)?,
            JHINZU: cents(facts.addition.0)?,
            LZZHINZU: cents(facts.addition.1)?,
            KRV: i64::from(!facts.pension),
            PVZ: i64::from(facts.childless),
            PVA: Dec::lit(i128::from(facts.care_reductions.min(4)), 0),
            PVS: i64::from(facts.saxony),
            VBEZ: cents(wages.pension)?,
            ..$pap::default()
        };
        if let Some((year, monthly)) = facts.pension_start {
            pap.VJAHR = i64::from(year);
            pap.VBEZM = cents(monthly)?;
            // Pensions in the wages expected for the year, for one-off payments: as many months as the year has.
            pap.JVBEZ = cents(wages.pension * 12)?;
        }
        if let Some(factor) = facts.factor {
            pap.af = 1;
            pap.f = Dec::lit(i128::from(factor), 3);
        } else {
            pap.af = 0;
        }
        if let Some(from) = facts.age_relief_from {
            pap.ALTER1 = 1;
            pap.AJAHR = i64::from(from);
        }
        match facts.health {
            HealthCover::Statutory { additional_rate } => pap.KVZ = Dec::lit(i128::from(additional_rate), 2),
            HealthCover::Private { premium, .. } => {
                pap.PKV = 1;
                pap.PKPV = cents(premium)?;
            }
        }
        ($more)(&mut pap)?;
        pap.run();
        let steps = pap
            .trace()
            .into_iter()
            .map(|(name, description, value)| WageTaxStep { name, description, value: value.to_string() })
            .collect();
        Ok((
            WageTax {
                tax: pap.LSTLZZ.units(0),
                solidarity: pap.SOLZLZZ.units(0),
                church_base: pap.BK.units(0),
                one_off_tax: pap.STS.units(0),
                one_off_solidarity: pap.SOLZS.units(0),
                one_off_church_base: pap.BKS.units(0),
            },
            steps,
        ))
    }};
}
use run;

#[cfg(test)]
mod tests {
    use super::super::dec::Round;
    use super::*;

    /// Runs the cases of a year on the generated PAP and compares its outputs with those of the interpreter.
    macro_rules! check {
        ($pap:ident, $file:literal, [$($optional:ident),*]) => {{
            let cases: serde_json::Value = serde_json::from_str(include_str!($file)).unwrap();
            let dec = |v: &serde_json::Value| -> Dec {
                let text = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
                let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
                Dec::lit(format!("{whole}{frac}").parse().unwrap(), frac.len() as u32)
            };
            for (n, case) in cases.as_array().unwrap().iter().enumerate() {
                let mut pap = $pap::default();
                for (name, value) in case["inputs"].as_object().unwrap() {
                    let int = || value.as_i64().unwrap();
                    match name.as_str() {
                        "LZZ" => pap.LZZ = int(),
                        "STKL" => pap.STKL = int(),
                        "R" => pap.R = int(),
                        "PKV" => pap.PKV = int(),
                        "PVZ" => pap.PVZ = int(),
                        "PVS" => pap.PVS = int(),
                        "KRV" => pap.KRV = int(),
                        "af" => pap.af = int(),
                        "ALTER1" => pap.ALTER1 = int(),
                        "AJAHR" => pap.AJAHR = int(),
                        "VJAHR" => pap.VJAHR = int(),
                        "ZMVB" => pap.ZMVB = int(),
                        "RE4" => pap.RE4 = dec(value),
                        "ZKF" => pap.ZKF = dec(value),
                        "KVZ" => pap.KVZ = dec(value),
                        "PVA" => pap.PVA = dec(value),
                        "PKPV" => pap.PKPV = dec(value),
                        "f" => pap.f = dec(value),
                        "LZZFREIB" => pap.LZZFREIB = dec(value),
                        "LZZHINZU" => pap.LZZHINZU = dec(value),
                        "VBEZ" => pap.VBEZ = dec(value),
                        "VBEZM" => pap.VBEZM = dec(value),
                        "JRE4" => pap.JRE4 = dec(value),
                        "SONSTB" => pap.SONSTB = dec(value),
                        "SONSTENT" => pap.SONSTENT = dec(value),
                        "JFREIB" => pap.JFREIB = dec(value),
                        $(stringify!($optional) => set!(pap, $optional, value, int, dec),)*
                        other => panic!("input {other}"),
                    }
                }
                pap.run();
                let out = &case["outputs"];
                let got = [pap.LSTLZZ, pap.SOLZLZZ, pap.BK, pap.STS, pap.SOLZS, pap.BKS];
                for (name, value) in ["LSTLZZ", "SOLZLZZ", "BK", "STS", "SOLZS", "BKS"].iter().zip(got) {
                    assert_eq!(
                        value.compare_to(dec(&out[name])),
                        0,
                        "{} case {n} {name}: {} ≠ {}",
                        $file,
                        value.set_scale(2, Round::Down).units(2),
                        out[name]
                    );
                }
            }
        }};
    }

    /// Sets an input only one year has.
    macro_rules! set {
        ($pap:ident, ALV, $value:ident, $int:ident, $dec:ident) => {
            $pap.ALV = $int()
        };
        ($pap:ident, PKPVAGZ, $value:ident, $int:ident, $dec:ident) => {
            $pap.PKPVAGZ = $dec($value)
        };
    }

    /// The PAP as generated, against the same PAP run by the interpreter of `scripts/pap.py` on Python's decimals: 600
    /// cases a year of every period, class and insurance – which the BMF's interface for programme tests computes
    /// alike.
    #[test]
    fn the_paps_as_interpreted() {
        check!(Pap2026, "../../tests/data/payroll/de-lst-2026.json", [ALV, PKPVAGZ]);
        check!(Pap2025, "../../tests/data/payroll/de-lst-2025.json", []);
    }

    #[test]
    fn a_month_in_class_1() {
        let facts = WageTaxFacts { health: HealthCover::Statutory { additional_rate: 290 }, ..Default::default() };
        let wages = Wages { current: 300_000, ..Default::default() };
        let tax = wage_tax(2026, PayPeriod::Month, &wages, &facts).unwrap();
        assert!(tax.tax > 0 && tax.solidarity == 0 && tax.church_base == 0, "{tax:?}");
        // More children, less church tax base; the wage tax itself does not change (child benefit instead).
        let church = WageTaxFacts { church: true, children: 10, ..facts.clone() };
        let with = wage_tax(2026, PayPeriod::Month, &wages, &church).unwrap();
        assert_eq!(with.tax, tax.tax);
        assert!(with.church_base < with.tax);
        assert_eq!(wage_tax(2024, PayPeriod::Month, &wages, &facts), Err(WageTaxError::Year(2024)));
        assert_eq!(
            wage_tax(2026, PayPeriod::Month, &Wages { current: -1, ..Default::default() }, &facts),
            Err(WageTaxError::Amount)
        );
    }
}
