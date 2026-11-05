// SPDX-License-Identifier: MIT
//! The figures of a payslip under German law (spec part 0013): the wage tax by the PAP, the social insurance, and what
//! is paid out.

use std::collections::BTreeMap;

use crate::{Currency, Date, Decimal, EventSet, Money};

use super::social::{self, Month};
use super::{EmploymentId, HealthCover, PayItem, PayItemKind, PayPeriod, WageTaxError, WageTaxFacts, Wages, wage_tax};

/// Why the figures of a payslip cannot be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PayrollError {
    /// The employment is not described on the last day of the period.
    #[error("employment {0} is not described for the period")]
    UnknownEmployment(EmploymentId),
    /// The employment is not under German law – the only one of this version.
    #[error("employment {0} has no rules this version knows")]
    NotGerman(EmploymentId),
    /// A year whose values this version does not have.
    #[error("no rules of payroll for the year {0}")]
    Year(u16),
    /// A period that ends before it begins or spans two calendar months.
    #[error("a period of a payslip lies within one calendar month")]
    Period,
    /// The wages are taxed by the ELStAM but there is no tax class.
    #[error("no tax class")]
    Class,
    /// A key of the contribution or person groups this version does not know.
    #[error("unknown key {0:?}")]
    Group(String),
    /// An amount not in euros.
    #[error("amounts of German payroll are in euros")]
    Currency,
    /// Church tax is due but the state of the employer's establishment is not configured (`region`).
    #[error("church tax needs the region of the employer")]
    Region,
    /// Wages taxed as a short-term employment that is not one: longer than 18 days, without an end, or above 150 euros
    /// a day (§ 40a Abs. 1 EStG).
    #[error("not a short-term employment for the flat tax of 25 %")]
    ShortTerm,
    /// The wage tax cannot be computed.
    #[error(transparent)]
    WageTax(#[from] WageTaxError),
}

/// The euro.
fn euro() -> Currency {
    "EUR".parse().expect("a currency")
}

/// A percentage as given (`"2.9"`) in millionths.
fn millionths(rate: &Decimal) -> i128 {
    rate.units() * 10i128.pow(6) / 100 / 10i128.pow(u32::from(rate.scale()))
}

/// The days of social insurance of a period within a month: 30 for the whole month, else its calendar days (§ 1 Abs. 1
/// BVV) – and whether it is the whole month.
fn days(from: Date, to: Date) -> (i128, bool) {
    let last = Date::new(to.year(), to.month(), to.day() + 1).is_none();
    if from.day() == 1 && last { (30, true) } else { (i128::from(to.day() - from.day() + 1).min(30), false) }
}

/// The number of a day, counted from 1 March of year 0: differences of two are the days between them.
fn day_number(date: Date) -> i64 {
    let (y, m, d) = (i64::from(date.year()), i64::from(date.month()), i64::from(date.day()));
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + d
}

/// The sums of the items of a payslip by kind, in cents.
#[derive(Debug, Default)]
struct Sums {
    wage: i128,
    one_off: i128,
    compensation: i128,
    in_kind: i128,
    tax_free: i128,
    deduction: i128,
}

fn sums(items: &[PayItem]) -> Result<Sums, PayrollError> {
    let mut s = Sums::default();
    for item in items {
        if item.amount.currency() != &euro() {
            return Err(PayrollError::Currency);
        }
        let units = item.amount.base_units();
        match item.kind {
            PayItemKind::Wage => s.wage += units,
            PayItemKind::OneOff => s.one_off += units,
            PayItemKind::Compensation => s.compensation += units,
            PayItemKind::InKind => s.in_kind += units,
            PayItemKind::TaxFree => s.tax_free += units,
            PayItemKind::Deduction => s.deduction += units,
        }
    }
    Ok(s)
}

fn group_digits(key: &str) -> Result<[u8; 4], PayrollError> {
    let digits: Vec<u8> = key.bytes().map(|b| b.wrapping_sub(b'0')).collect();
    let valid = digits.len() == 4
        && matches!(digits[0], 0 | 1 | 3 | 6 | 9)
        && matches!(digits[1], 0 | 1 | 3 | 5)
        && matches!(digits[2], 0..=2)
        && matches!(digits[3], 0..=2);
    if valid { Ok([digits[0], digits[1], digits[2], digits[3]]) } else { Err(PayrollError::Group(key.to_owned())) }
}

impl Month {
    /// The month with the days and bases of the months before it in the year.
    fn with_before(self, before: &Month) -> Month {
        Month {
            days_before: before.days_before,
            health_before: before.health_before,
            pension_before: before.pension_before,
            ..self
        }
    }
}

impl EventSet {
    /// The days and bases of the payslips of an employment in `year` before `until`, with their current wages and
    /// one-off payments.
    fn months_before(
        &self,
        owner: &crate::PartyId,
        employment: &EmploymentId,
        year: u16,
        until: Date,
        attributed_until: Option<Date>,
    ) -> Result<(Month, i128, i128), PayrollError> {
        let mut before = Month::default();
        let (mut current, mut one_off) = (0, 0);
        for payslip in self.payslips(owner) {
            if payslip.employment != *employment || payslip.to >= until || payslip.to.year() != year {
                continue;
            }
            let earlier = sums(&payslip.items)?;
            current += earlier.wage + earlier.in_kind;
            one_off += earlier.one_off + earlier.compensation;
            before.days_before += days(payslip.from, payslip.to).0;
            let figure = |name: &str| payslip.figures.get(name).map_or(0, Money::base_units);
            before.health_before += figure("health_base");
            before.pension_before += figure("pension_base");
        }
        // One-off payments of January to March attributed to December count in the year before.
        for payslip in self.payslips(owner) {
            let attributed = attributed_until.is_some_and(|limit| payslip.to < limit);
            if payslip.employment == *employment && payslip.to.year() == year + 1 && attributed {
                let figure = |name: &str| payslip.figures.get(name).map_or(0, Money::base_units);
                before.health_before += figure("health_base_previous");
                before.pension_before += figure("pension_base_previous");
            }
        }
        Ok((before, current, one_off))
    }

    /// The figures of a payslip of `employment` for `from` to `to` with `items` (spec part 0013): computed from the
    /// employment as described on `to` and the payslips of the year recorded for it before `from`.
    pub fn payslip_figures(
        &self,
        employment: &EmploymentId,
        from: Date,
        to: Date,
        items: &[PayItem],
    ) -> Result<BTreeMap<String, Money>, PayrollError> {
        let described =
            self.employment(employment, to).ok_or_else(|| PayrollError::UnknownEmployment(employment.clone()))?;
        let de = described.de.as_ref().ok_or_else(|| PayrollError::NotGerman(employment.clone()))?;
        if to < from || (from.year(), from.month()) != (to.year(), to.month()) {
            return Err(PayrollError::Period);
        }
        let year = to.year();
        let rules = social::year(year).ok_or(PayrollError::Year(year))?;
        let s = sums(items)?;
        let (days, whole) = days(from, to);

        let (before, current_before, one_off_before) =
            self.months_before(&described.owner, employment, year, from, None)?;

        let insurance = &de.insurance;
        let groups = group_digits(&insurance.contribution_group)?;
        if insurance.person_group.len() != 3 || !insurance.person_group.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PayrollError::Group(insurance.person_group.clone()));
        }
        let minor = insurance.person_group == "109";
        let short_term = insurance.person_group == "110";
        let current = s.wage + s.in_kind;
        let facts = |insurance: &super::GermanInsurance, groups: [u8; 4]| Month {
            groups,
            minor,
            household: insurance.household,
            short_term,
            over_four_weeks: described.end.is_none_or(|end| day_number(end) - day_number(described.start) >= 28),
            additional: insurance.additional_rate.as_ref().map_or(0, millionths),
            childless: insurance.childless,
            children: insurance.children,
            saxony: insurance.saxony,
            u1: insurance.u1.as_ref().map(millionths),
            u2: insurance.u2.as_ref().map(millionths),
            ..Month::default()
        };
        let mut month = Month { current, one_off: s.one_off, days, ..facts(insurance, groups) }.with_before(&before);

        // The Märzklausel (§ 23a Abs. 4, 5 SGB IV): a one-off payment from January to March that, with the wages of
        // the year so far, exceeds the ceiling of those months – of health where the employee is insured in it, else
        // of pension – belongs to the last month of the year before, if the employment had payslips in it.
        let mut march: BTreeMap<&'static str, i128> = BTreeMap::new();
        if to.month() <= 3 && s.one_off > 0 && !minor && !short_term {
            let health_insured = matches!(groups[0], 1 | 3);
            let (ceiling, so_far) = if health_insured {
                (rules.health_ceiling, before.health_before)
            } else {
                (rules.pension_ceiling, before.pension_before)
            };
            let month_days = before.days_before + days;
            let current_base = current.min(ceiling * days / 360);
            let exceeds = so_far + current_base + s.one_off > ceiling * month_days / 360;
            let last = Date::new(year - 1, 12, 31).expect("a date");
            let (previous, _, _) = self.months_before(
                &described.owner,
                employment,
                year - 1,
                Date::new(year, 1, 1).expect("a date"),
                Some(from),
            )?;
            if exceeds && previous.days_before > 0 {
                let earlier = social::year(year - 1).ok_or(PayrollError::Year(year - 1))?;
                let then = self.employment(employment, last).and_then(|e| e.de).map(|de| de.insurance);
                let then = then.as_ref().unwrap_or(insurance);
                let then_groups = group_digits(&then.contribution_group)?;
                let attributed = Month { one_off: s.one_off, ..facts(then, then_groups) }.with_before(&previous);
                for (name, units) in social::contributions(&earlier, &attributed) {
                    let name = match name {
                        "health_base" => "health_base_previous",
                        "pension_base" => "pension_base_previous",
                        "u1" | "u2" => continue,
                        other => other,
                    };
                    march.insert(name, units);
                }
                month.one_off = 0;
            }
        }
        let mut figures: BTreeMap<&'static str, i128> = social::contributions(&rules, &month);
        for (name, units) in march {
            *figures.entry(name).or_insert(0) += units;
        }

        // The wage tax.
        let tax = &de.tax;
        if tax.flat {
            // 2 % of the wages, with solidarity surcharge and church tax (§ 40a Abs. 2 EStG).
            figures.insert("flat_tax", (current + s.one_off) * 2 / 100);
        } else if tax.short_term {
            // 25 % of the wages of an employment of at most 18 days with at most 150 euros a day (§ 40a Abs. 1
            // EStG); the solidarity surcharge 5.5 % of it (§ 3 Abs. 1 Nr. 3, § 4 SolZG). The days are counted as
            // calendar days of the employment, the hourly limit of 19 euros (Abs. 4 Nr. 1) is not checked (OPEN).
            let span = described.end.map(|end| day_number(end) - day_number(described.start) + 1);
            let wages = current + s.one_off;
            match span {
                Some(span) if span <= 18 && wages <= 15_000 * i128::from(span) => {}
                _ => return Err(PayrollError::ShortTerm),
            }
            let flat = wages * 25 / 100;
            figures.insert("flat_wage_tax", flat);
            figures.insert("flat_solidarity", flat * 55 / 1000);
        } else {
            let class = tax.class.ok_or(PayrollError::Class)?;
            let private = match (&insurance.private, groups[0]) {
                (Some(p), 0) => {
                    if p.premium.currency() != &euro() || p.subsidy.currency() != &euro() {
                        return Err(PayrollError::Currency);
                    }
                    Some((p.premium.base_units(), p.subsidy.base_units()))
                }
                _ => None,
            };
            let age_relief_from = described.born.and_then(|born| {
                // The 64th year of life is completed the day before the 64th birthday.
                let completed = if (born.month(), born.day()) == (1, 1) { born.year() + 63 } else { born.year() + 64 };
                (completed < year).then_some(completed + 1)
            });
            let year_and_month = |amounts: &Option<super::YearAndMonth>| -> Result<(i128, i128), PayrollError> {
                match amounts {
                    None => Ok((0, 0)),
                    Some(a) if a.year.currency() != &euro() || a.month.currency() != &euro() => {
                        Err(PayrollError::Currency)
                    }
                    Some(a) => Ok((a.year.base_units(), a.month.base_units())),
                }
            };
            let (allowance, addition) = (year_and_month(&tax.allowance)?, year_and_month(&tax.addition)?);
            let facts = WageTaxFacts {
                class,
                factor: tax.factor.as_ref().map(|f| (f.units() * 1000 / 10i128.pow(u32::from(f.scale()))) as u32),
                children: tax
                    .children
                    .as_ref()
                    .map_or(0, |c| (c.units() * 10 / 10i128.pow(u32::from(c.scale()))) as u32),
                church: tax.church.is_some(),
                allowance,
                addition,
                age_relief_from,
                health: match private {
                    Some((premium, subsidy)) => HealthCover::Private { premium, subsidy },
                    None => {
                        HealthCover::Statutory { additional_rate: (month.additional / 100).try_into().unwrap_or(0) }
                    }
                },
                pension: groups[1] != 0,
                unemployment: groups[2] != 0,
                childless: insurance.childless,
                care_reductions: insurance.children.saturating_sub(1).min(4),
                saxony: insurance.saxony,
            };
            // A whole month by the monthly tables; part of one by the daily ones, the wages per day (§ 39b Abs. 2
            // Satz 6 EStG).
            let (period, factor) = if whole { (PayPeriod::Month, 1) } else { (PayPeriod::Day, days) };
            let per = |amount: i128| amount / factor;
            let facts = WageTaxFacts {
                allowance: (facts.allowance.0, if whole { facts.allowance.1 } else { facts.allowance.0 / 360 }),
                addition: (facts.addition.0, if whole { facts.addition.1 } else { facts.addition.0 / 360 }),
                ..facts
            };
            let wages = Wages {
                current: per(current),
                one_off: s.one_off + s.compensation,
                compensation: s.compensation,
                // The wages expected for the year: those paid, the current ones for the rest of it, and the one-off
                // payments before (§ 39b Abs. 3 EStG).
                expected_year: current_before + current * i128::from(13 - to.month()) + one_off_before,
            };
            let result = wage_tax(year, period, &wages, &facts)?;
            let wage_tax = result.tax * factor + result.one_off_tax;
            let solidarity = result.solidarity * factor + result.one_off_solidarity;
            let church_base = result.church_base * factor + result.one_off_church_base;
            figures.insert("wage_tax", wage_tax);
            figures.insert("solidarity", solidarity);
            if tax.church.is_some() {
                let region = self
                    .configuration(&described.owner, to)
                    .and_then(|c| c.region.clone())
                    .ok_or(PayrollError::Region)?;
                let rate = if matches!(region.as_str(), "DE-BY" | "DE-BW") { 8 } else { 9 };
                figures.insert("church", church_base * rate / 100);
            }
            if let Some((_, subsidy)) = private {
                figures.insert("private_subsidy", subsidy);
            }
        }

        // Gross, net and what is paid out.
        let gross = current + s.one_off + s.compensation + s.tax_free;
        let withheld: i128 = ["wage_tax", "solidarity", "church"]
            .iter()
            .chain(&[
                "health_employee",
                "health_additional_employee",
                "care_employee",
                "pension_employee",
                "unemployment_employee",
            ])
            .map(|name| figures.get(name).copied().unwrap_or(0))
            .sum();
        let net = gross - withheld + figures.get("private_subsidy").copied().unwrap_or(0);
        figures.insert("gross", gross);
        figures.insert("net", net);
        figures.insert("payout", net - s.in_kind - s.deduction);
        let currency = euro();
        Ok(figures
            .into_iter()
            .filter(|(_, units)| *units != 0)
            .map(|(name, units)| (name.to_owned(), Money::new(units, currency.clone()).expect("within range")))
            .collect())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::payroll::{Employment, GermanEmployment, GermanInsurance, GermanTax, Payslip};
    use crate::{Clock, Configuration, Event, Payload, party::tests::party};

    pub(crate) fn eur(units: i128) -> Money {
        Money::new(units, euro()).unwrap()
    }

    pub(crate) fn add(set: &mut EventSet, payload: Payload) {
        let device = "a".parse().unwrap();
        let (seq, prev) = set.chain_head(&device);
        let time = Clock::new(device).now(seq);
        set.insert(Event { time, seq, prev, payload });
    }

    pub(crate) fn employment_id() -> EmploymentId {
        "01a18406-0000-7000-8000-000000000001".parse().unwrap()
    }

    /// Books of party 1 in North Rhine-Westphalia, employing party 2 from January 2026 in class 1, church tax `ev`.
    pub(crate) fn employer() -> EventSet {
        let mut set = EventSet::new();
        add(&mut set, Payload::OwnerAdded { owner: party(1) });
        add(
            &mut set,
            Payload::OwnerConfigured {
                configuration: Configuration {
                    owner: party(1),
                    from: "2025-01-01".parse().unwrap(),
                    currency: euro(),
                    year_start: 1,
                    year_start_day: None,
                    chart: None,
                    accounting: None,
                    vat: None,
                    trade_tax_multiplier: None,
                    region: Some("DE-NW".parse().unwrap()),
                },
            },
        );
        add(&mut set, Payload::EmploymentDescribed { employment: Box::new(employment("1111", "101")) });
        set
    }

    pub(crate) fn employment(group: &str, person: &str) -> Employment {
        Employment {
            id: employment_id(),
            owner: party(1),
            employee: party(2),
            from: "2026-01-01".parse().unwrap(),
            start: "2026-01-01".parse().unwrap(),
            end: None,
            number: None,
            born: Some("1990-05-17".parse().unwrap()),
            de: Some(GermanEmployment {
                tax: GermanTax {
                    class: Some(1),
                    factor: None,
                    children: None,
                    church: Some("ev".into()),
                    allowance: None,
                    addition: None,
                    flat: person == "109",
                    short_term: false,
                },
                insurance: GermanInsurance {
                    insurer: "01234567".into(),
                    contribution_group: group.into(),
                    person_group: person.into(),
                    additional_rate: Some("2.9".parse().unwrap()),
                    private: None,
                    childless: true,
                    children: 0,
                    saxony: false,
                    u1: Some("1.6".parse().unwrap()),
                    u2: Some("0.44".parse().unwrap()),
                    household: false,
                },
            }),
        }
    }

    pub(crate) fn item(kind: PayItemKind, units: i128) -> PayItem {
        PayItem { kind, name: "item".into(), amount: eur(units) }
    }

    fn get(figures: &BTreeMap<String, Money>, name: &str) -> i128 {
        figures.get(name).map_or(0, Money::base_units)
    }

    #[test]
    fn a_salary_and_a_bonus() {
        let mut set = employer();
        let (jan, jan_end) = ("2026-01-01".parse().unwrap(), "2026-01-31".parse().unwrap());
        let items = vec![item(PayItemKind::Wage, 400_000)];
        let figures = set.payslip_figures(&employment_id(), jan, jan_end, &items).unwrap();
        // The wage tax of the PAP itself, church tax 9 % of its base.
        let facts = WageTaxFacts {
            church: true,
            childless: true,
            health: HealthCover::Statutory { additional_rate: 290 },
            ..WageTaxFacts::default()
        };
        let tax = wage_tax(2026, PayPeriod::Month, &Wages { current: 400_000, ..Wages::default() }, &facts).unwrap();
        assert_eq!(get(&figures, "wage_tax"), tax.tax);
        assert_eq!(get(&figures, "church"), tax.church_base * 9 / 100);
        // 7.3 % + 1.45 % + 1.8 % + 0.6 % + 9.3 % + 1.3 % of 4,000 = 870.00.
        let social: i128 = ["health_employee", "health_additional_employee", "care_employee", "pension_employee"]
            .iter()
            .chain(&["unemployment_employee"])
            .map(|n| get(&figures, n))
            .sum();
        assert_eq!(social, 87_000);
        assert_eq!(get(&figures, "net"), 400_000 - 87_000 - tax.tax - tax.solidarity - get(&figures, "church"));
        assert_eq!((get(&figures, "u1"), get(&figures, "u2"), get(&figures, "u3")), (6_400, 1_760, 600));
        add(
            &mut set,
            Payload::PayslipRecorded {
                payslip: Payslip {
                    id: "01a18406-0000-7000-8000-0000000000a1".parse().unwrap(),
                    employment: employment_id(),
                    from: jan,
                    to: jan_end,
                    paid: jan_end,
                    replaces: None,
                    items,
                    figures: figures.clone(),
                },
            },
        );
        // February with a bonus of 10,000: taxed as a one-off payment; it is insured up to what the ceilings of two
        // months leave – health 2 × 5,812.50 − 2 × 4,000, pension 2 × 8,450 − 2 × 4,000.
        let items = vec![item(PayItemKind::Wage, 400_000), item(PayItemKind::OneOff, 1_000_000)];
        let feb = set
            .payslip_figures(&employment_id(), "2026-02-01".parse().unwrap(), "2026-02-28".parse().unwrap(), &items)
            .unwrap();
        assert_eq!(get(&feb, "health_base"), 400_000 + 2 * 581_250 - 2 * 400_000);
        assert_eq!(get(&feb, "pension_base"), 400_000 + 2 * 845_000 - 2 * 400_000);
        let bonus = wage_tax(
            2026,
            PayPeriod::Month,
            &Wages { current: 400_000, one_off: 1_000_000, compensation: 0, expected_year: 400_000 * 12 },
            &facts,
        )
        .unwrap();
        assert_eq!(get(&feb, "wage_tax"), bonus.tax + bonus.one_off_tax);
        assert!(set.review_payroll().is_empty(), "{:?}", set.review_payroll());
        // The Lohnsteuer-Anmeldung of January: the payslip's wage tax, solidarity surcharge and church tax (ev).
        let (from, to) = (jan, jan_end);
        let lsta = set.payroll_report(&crate::ReportMapping::de_lsta_2026(), &party(1), from, to);
        assert_eq!(lsta["42"], get(&figures, "wage_tax"));
        assert_eq!(lsta["61"], get(&figures, "church"));
        assert!(!lsta.contains_key("62"));
        assert_eq!(lsta["83"], get(&figures, "wage_tax") + get(&figures, "solidarity") + get(&figures, "church"));
        assert_eq!(set.employees_paid(&party(1), from, to), 1);
        // The Beitragsnachweis to the insurer.
        let bn = set.contribution_statement(&party(1), "01234567", from, to);
        assert_eq!((bn["1000"], bn["zusatzbeitrag"], bn["0100"], bn["0010"]), (58_400, 11_600, 74_400, 10_400));
        // Care: 1.8 % each and the employee's 0.6 % without children.
        assert_eq!(bn["0001"], 7_200 + 2_400 + 7_200);
        assert!(set.contribution_statement(&party(1), "99999999", from, to).is_empty());
    }

    #[test]
    fn a_minor_employment_and_part_of_a_month() {
        let mut set = employer();
        add(&mut set, Payload::EmploymentDescribed { employment: Box::new(employment("6100", "109")) });
        let items = vec![item(PayItemKind::Wage, 50_000)];
        let f = set
            .payslip_figures(&employment_id(), "2026-03-01".parse().unwrap(), "2026-03-31".parse().unwrap(), &items)
            .unwrap();
        assert_eq!((get(&f, "flat_tax"), get(&f, "health_flat"), get(&f, "pension_flat")), (1_000, 6_500, 7_500));
        // 18.6 % of 500 = 93.00 less 75.00.
        assert_eq!(get(&f, "pension_employee"), 1_800);
        assert_eq!(get(&f, "payout"), 50_000 - 1_800);
        assert_eq!(get(&f, "wage_tax"), 0);
        // Half a month of a regular employment: the ceiling of 16 days, the daily tables.
        let mut set = employer();
        let items = vec![item(PayItemKind::Wage, 400_000)];
        let f = set
            .payslip_figures(&employment_id(), "2026-04-15".parse().unwrap(), "2026-04-30".parse().unwrap(), &items)
            .unwrap();
        assert_eq!(get(&f, "health_base"), 6_975_000 * 16 / 360);
        assert!(get(&f, "wage_tax") > 0);
        // Church tax needs the region.
        add(
            &mut set,
            Payload::OwnerConfigured {
                configuration: Configuration {
                    owner: party(1),
                    from: "2026-04-01".parse().unwrap(),
                    currency: euro(),
                    year_start: 1,
                    year_start_day: None,
                    chart: None,
                    accounting: None,
                    vat: None,
                    trade_tax_multiplier: None,
                    region: None,
                },
            },
        );
        let err =
            set.payslip_figures(&employment_id(), "2026-04-15".parse().unwrap(), "2026-04-30".parse().unwrap(), &items);
        assert_eq!(err, Err(PayrollError::Region));
    }

    fn record(
        set: &mut EventSet,
        id: &str,
        from: &str,
        to: &str,
        items: Vec<PayItem>,
        replaces: Option<&str>,
    ) -> Payslip {
        let (from, to) = (from.parse().unwrap(), to.parse().unwrap());
        let figures = set.payslip_figures(&employment_id(), from, to, &items).unwrap();
        let payslip = Payslip {
            id: id.parse().unwrap(),
            employment: employment_id(),
            from,
            to,
            paid: to,
            replaces: replaces.map(|r| r.parse().unwrap()),
            items,
            figures,
        };
        add(set, Payload::PayslipRecorded { payslip: payslip.clone() });
        payslip
    }

    /// A bonus in February 2026 above what the ceiling of two months leaves goes to December 2025 (Märzklausel),
    /// with the values of 2025 and the room left there.
    #[test]
    fn the_march_clause() {
        let mut set = employer();
        let mut described = employment("1111", "101");
        described.start = "2025-01-01".parse().unwrap();
        described.from = described.start;
        add(&mut set, Payload::EmploymentDescribed { employment: Box::new(described) });
        let wage = || vec![item(PayItemKind::Wage, 400_000)];
        for month in 1..=12 {
            let to = Date::new(2025, month, 1).unwrap();
            let last = (28..=31).rev().find_map(|d| Date::new(2025, month, d)).unwrap();
            let id = format!("01a18406-0000-7000-8000-0000000025{month:02}");
            record(&mut set, &id, &to.to_string(), &last.to_string(), wage(), None);
        }
        record(&mut set, "01a18406-0000-7000-8000-000000002601", "2026-01-01", "2026-01-31", wage(), None);
        let items = vec![item(PayItemKind::Wage, 400_000), item(PayItemKind::OneOff, 1_000_000)];
        let f = set
            .payslip_figures(&employment_id(), "2026-02-01".parse().unwrap(), "2026-02-28".parse().unwrap(), &items)
            .unwrap();
        // 2 × 5,812.50 − 2 × 4,000 = 3,625 < 10,000: to 2025, where 66,150 − 48,000 = 18,150 for health and
        // 96,600 − 48,000 for pension are left.
        assert_eq!(get(&f, "health_base"), 400_000);
        assert_eq!((get(&f, "health_base_previous"), get(&f, "pension_base_previous")), (1_000_000, 1_000_000));
        // Health on the current wages and the bonus: 7.3 % of 4,000 + 7.3 % of 10,000.
        assert_eq!(get(&f, "health_employee"), 29_200 + 73_000);
        // A small bonus stays in 2026.
        let items = vec![item(PayItemKind::Wage, 400_000), item(PayItemKind::OneOff, 100_000)];
        let f = set
            .payslip_figures(&employment_id(), "2026-02-01".parse().unwrap(), "2026-02-28".parse().unwrap(), &items)
            .unwrap();
        assert_eq!((get(&f, "health_base"), get(&f, "health_base_previous")), (500_000, 0));
    }

    #[test]
    fn a_short_term_employment_and_a_household() {
        let mut set = employer();
        let mut short = employment("0000", "110");
        short.start = "2026-07-01".parse().unwrap();
        short.end = Some("2026-07-10".parse().unwrap());
        short.de.as_mut().unwrap().tax =
            GermanTax { class: None, short_term: true, flat: false, ..short.de.as_ref().unwrap().tax.clone() };
        add(&mut set, Payload::EmploymentDescribed { employment: Box::new(short.clone()) });
        let (from, to) = ("2026-07-01".parse().unwrap(), "2026-07-10".parse().unwrap());
        let f = set.payslip_figures(&employment_id(), from, to, &[item(PayItemKind::Wage, 120_000)]).unwrap();
        assert_eq!((get(&f, "flat_wage_tax"), get(&f, "flat_solidarity")), (30_000, 1_650));
        assert_eq!((get(&f, "u1"), get(&f, "u2"), get(&f, "u3")), (0, 264, 180));
        assert_eq!(get(&f, "payout"), 120_000);
        // Above 150 euros a day: not a short-term employment for the flat tax.
        let err = set.payslip_figures(&employment_id(), from, to, &[item(PayItemKind::Wage, 160_000)]);
        assert_eq!(err, Err(PayrollError::ShortTerm));
        // A minor employment in a private household.
        let mut household = employment("6500", "109");
        household.de.as_mut().unwrap().insurance.household = true;
        add(&mut set, Payload::EmploymentDescribed { employment: Box::new(household) });
        let f = set
            .payslip_figures(
                &employment_id(),
                "2026-08-01".parse().unwrap(),
                "2026-08-31".parse().unwrap(),
                &[item(PayItemKind::Wage, 50_000)],
            )
            .unwrap();
        assert_eq!((get(&f, "health_flat"), get(&f, "pension_flat"), get(&f, "accident")), (2_500, 2_500, 800));
        assert_eq!(get(&f, "u3"), 0);
    }

    /// A correction replaces a payslip: the Lohnsteuer-Anmeldung of its month shows the corrected figures, the
    /// Beitragsnachweis of the month it is paid the difference, its postings the difference; the certificate counts
    /// the corrected one.
    #[test]
    fn a_correction_and_the_certificate() {
        let mut set = employer();
        let jan = record(
            &mut set,
            "01a18406-0000-7000-8000-0000000000c1",
            "2026-01-01",
            "2026-01-31",
            vec![item(PayItemKind::Wage, 300_000)],
            None,
        );
        let feb = record(
            &mut set,
            "01a18406-0000-7000-8000-0000000000c2",
            "2026-02-01",
            "2026-02-28",
            vec![item(PayItemKind::Wage, 300_000)],
            None,
        );
        // January was 3,500, corrected with the March payroll.
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-01-31".parse().unwrap());
        let items = vec![item(PayItemKind::Wage, 350_000)];
        let figures = set.payslip_figures(&employment_id(), from, to, &items).unwrap();
        let fixed = Payslip {
            id: "01a18406-0000-7000-8000-0000000000c3".parse().unwrap(),
            employment: employment_id(),
            from,
            to,
            paid: "2026-03-31".parse().unwrap(),
            replaces: Some(jan.id.clone()),
            items,
            figures,
        };
        add(&mut set, Payload::PayslipRecorded { payslip: fixed.clone() });
        assert!(set.review_payroll().is_empty(), "{:?}", set.review_payroll());
        let lsta = set.payroll_report(&crate::ReportMapping::de_lsta_2026(), &party(1), from, to);
        assert_eq!(lsta["42"], get(&fixed.figures, "wage_tax"));
        let march = set.contribution_statement(
            &party(1),
            "01234567",
            "2026-03-01".parse().unwrap(),
            "2026-03-31".parse().unwrap(),
        );
        assert_eq!(march["1000"], 2 * (25_550 - 21_900));
        let january = set.contribution_statement(&party(1), "01234567", from, to);
        assert_eq!(january["1000"], 2 * 21_900);
        let postings = fixed.correction_postings(&jan, &party(1), false);
        assert_eq!(postings.iter().map(|p| p.amount.base_units()).sum::<i128>(), 0);
        assert!(postings.iter().any(|p| p.amount.base_units() == 50_000));
        // The certificate of 2026: both months as they apply.
        let lines = set.wage_tax_certificate(&employment_id(), 2026).unwrap();
        assert_eq!(lines["3"], 650_000);
        assert_eq!(lines["4"], get(&fixed.figures, "wage_tax") + get(&feb.figures, "wage_tax"));
        assert_eq!(
            lines["25"],
            get(&fixed.figures, "health_employee")
                + get(&fixed.figures, "health_additional_employee")
                + get(&feb.figures, "health_employee")
                + get(&feb.figures, "health_additional_employee")
        );
        // A correction of something that is not there is reported.
        let mut stray = fixed.clone();
        stray.id = "01a18406-0000-7000-8000-0000000000c4".parse().unwrap();
        stray.replaces = Some("01a18406-0000-7000-8000-0000000000ff".parse().unwrap());
        add(&mut set, Payload::PayslipRecorded { payslip: stray });
        assert!(set.review_payroll().iter().any(|p| matches!(p, crate::payroll::PayrollProblem::Correction { .. })));
    }
}
