// SPDX-License-Identifier: MIT
//! The contributions to the German social insurance on the wages of a month, and the levies of the employer (spec part
//! 0013). Amounts in cents, rates in millionths (1 % = 10,000).

use std::collections::BTreeMap;

/// The values of a year, from the law of that year (sources in spec part 0013).
#[derive(Debug, Clone, Copy)]
pub(super) struct Year {
    /// Contribution ceilings per year: health and care, pension and unemployment, in cents.
    pub health_ceiling: i128,
    pub pension_ceiling: i128,
    /// Rates: health general and reduced, care, the surcharge without children, the reduction per child, the
    /// employee's extra in Saxony, pension, unemployment, insolvency levy.
    pub health: i128,
    pub health_reduced: i128,
    pub care: i128,
    pub childless: i128,
    pub per_child: i128,
    pub saxony: i128,
    pub pension: i128,
    pub unemployment: i128,
    pub insolvency: i128,
    /// The average additional rate of health insurance (§ 242a SGB V), and the monthly limit of trainees whose
    /// contributions the employer bears alone (§ 20 Abs. 3 Satz 1 Nr. 1 SGB IV), in cents; a twentieth of the monthly
    /// reference value, below which pensions bear no contributions (§ 226 Abs. 2 SGB V), in cents.
    pub average_additional: i128,
    pub trainee_limit: i128,
    pub pension_threshold: i128,
    /// Minor employment: the limit per month in cents; the employer's flat rates for health and pension; the levies of
    /// the Minijob-Zentrale; the minimum base of the pension insurance in cents.
    pub minor_limit: i128,
    pub minor_health: i128,
    pub minor_pension: i128,
    pub minor_u1: i128,
    pub minor_u2: i128,
    pub minor_minimum: i128,
    /// A minor employment in a private household: the employer's flat rates for health and pension, and the
    /// accident insurance.
    pub household_health: i128,
    pub household_pension: i128,
    pub household_accident: i128,
    /// The transition range: its upper limit per month in cents and the factor F as a fraction.
    pub transition_limit: i128,
    pub factor: (i128, i128),
}

/// The values of `year` from the values of the law of Germany (`LawValues::de`), as they apply on its first day;
/// `None` for a year this version has not all of them for.
pub(super) fn year(year: u16) -> Option<Year> {
    let law = crate::LawValues::de();
    let day = crate::Date::new(year, 1, 1)?;
    let cents = |key: &str| law.cents(key, day);
    let rate = |key: &str| law.millionths(key, day);
    Some(Year {
        health_ceiling: cents("sv.health_ceiling")?,
        pension_ceiling: cents("sv.pension_ceiling")?,
        health: rate("sv.health")?,
        health_reduced: rate("sv.health_reduced")?,
        care: rate("sv.care")?,
        childless: rate("sv.care_childless")?,
        per_child: rate("sv.care_per_child")?,
        saxony: rate("sv.care_saxony")?,
        pension: rate("sv.pension")?,
        unemployment: rate("sv.unemployment")?,
        insolvency: rate("sv.insolvency")?,
        // The average additional rate of 2026 for the years before it too: only trainees use it (OPEN).
        average_additional: law
            .millionths("sv.average_additional", day)
            .or_else(|| law.millionths("sv.average_additional", crate::Date::new(2026, 1, 1).expect("a date")))?,
        trainee_limit: cents("sv.trainee_limit")?,
        // The twentieth of the reference value of 2026 for the years before it too: pensions in a one-off payment
        // attributed to 2025 do not occur (OPEN).
        pension_threshold: cents("sv.pension_threshold")
            .or_else(|| law.cents("sv.pension_threshold", crate::Date::new(2026, 1, 1).expect("a date")))?,
        minor_limit: cents("sv.minor_limit")?,
        minor_health: rate("sv.minor_health")?,
        minor_pension: rate("sv.minor_pension")?,
        minor_u1: rate("sv.minor_u1")?,
        minor_u2: rate("sv.minor_u2")?,
        minor_minimum: cents("sv.minor_minimum")?,
        household_health: rate("sv.household_health")?,
        household_pension: rate("sv.household_pension")?,
        household_accident: rate("sv.household_accident")?,
        transition_limit: cents("sv.transition_limit")?,
        factor: law.fraction("sv.factor_f", day)?,
    })
}

/// What the contributions of a month depend on.
#[derive(Debug, Clone, Default)]
pub(super) struct Month {
    /// The digits of the contribution groups: health, pension, unemployment, care.
    pub groups: [u8; 4],
    /// The employment is minor (Personengruppe 109), in a private household; short-term (Personengruppe 110),
    /// lasting more than four weeks.
    pub minor: bool,
    pub household: bool,
    pub short_term: bool,
    pub over_four_weeks: bool,
    /// The current wages of the month subject to contributions, and the one-off payments.
    pub current: i128,
    pub one_off: i128,
    /// The days of social insurance of the month (30 for a whole one), and of the months before in the year.
    pub days: i128,
    pub days_before: i128,
    /// The bases of health and of pension insurance in the months before in the year.
    pub health_before: i128,
    pub pension_before: i128,
    /// The insurer's additional rate; the employee pays the surcharge without children; children the care insurance
    /// counts; works in Saxony; the rates of U1 and U2.
    pub additional: i128,
    /// A trainee with low pay (person group 121); pensions paid in the month (Versorgungsbezüge), whose
    /// contributions the pensioner bears alone.
    pub trainee: bool,
    pub pension: i128,
    pub childless: bool,
    pub children: u8,
    pub saxony: bool,
    pub u1: Option<i128>,
    pub u2: Option<i128>,
}

/// `units × rate` (millionths), rounded half up to a cent (§ 1 Abs. 2 BVV); `units` a fraction `num / den`.
fn share(num: i128, den: i128, rate: i128) -> i128 {
    let n = num * rate;
    let d = den * 1_000_000;
    (2 * n + d) / (2 * d)
}

/// The contributions and levies of a month, by name (spec part 0013): the bases, each share of employee and employer,
/// the flat rates of a minor employment, the levies.
pub(super) fn contributions(y: &Year, m: &Month) -> BTreeMap<&'static str, i128> {
    let mut out = BTreeMap::new();
    let [health, pension, unemployment, care] = m.groups;
    let wages = m.current + m.one_off;
    if m.short_term {
        // No contributions (Beitragsgruppe 0000); at the Minijob-Zentrale U1 only for more than four weeks, U2 and the
        // insolvency levy always (Geringfügigkeits-Richtlinien 2026, G).
        if m.over_four_weeks {
            out.insert("u1", share(m.current, 1, y.minor_u1));
        }
        out.insert("u2", share(m.current, 1, y.minor_u2));
        out.insert("u3", share(wages, 1, y.insolvency));
        out.retain(|_, v| *v != 0);
        return out;
    }
    if m.minor {
        let (health_rate, pension_rate) =
            if m.household { (y.household_health, y.household_pension) } else { (y.minor_health, y.minor_pension) };
        // § 249b SGB V, § 168 Abs. 1 Nr. 1b SGB VI: the employer's flat rates on the wages; with compulsory pension
        // insurance the employee tops up to the full rate on at least the minimum base (§ 163 Abs. 8 SGB VI), the
        // employer's share computed first (§ 2 Abs. 3 BVV).
        if health == 6 {
            out.insert("health_flat", share(wages, 1, health_rate));
        }
        match pension {
            5 => {
                out.insert("pension_flat", share(wages, 1, pension_rate));
            }
            1 => {
                let employer = share(wages, 1, pension_rate);
                let total = share(wages.max(y.minor_minimum), 1, y.pension);
                out.insert("pension_flat", employer);
                out.insert("pension_employee", total - employer);
            }
            _ => {}
        }
        out.insert("pension_base", wages);
        out.insert("u1", share(m.current, 1, y.minor_u1));
        out.insert("u2", share(m.current, 1, y.minor_u2));
        // A private household pays the accident insurance with the levies and no insolvency levy (§ 8a SGB IV;
        // Minijob-Zentrale).
        if m.household {
            out.insert("accident", share(wages, 1, y.household_accident));
        } else {
            out.insert("u3", share(wages, 1, y.insolvency));
        }
        out.retain(|_, v| *v != 0);
        return out;
    }
    // Current wages up to the ceiling of the days; one-off payments up to what the ceiling of the year so far leaves
    // (§ 23a Abs. 3 SGB IV).
    let base = |ceiling: i128, before: i128| {
        let month = m.current.min(ceiling * m.days / 360);
        let room = ceiling * (m.days_before + m.days) / 360 - before - month;
        month + m.one_off.min(room.max(0))
    };
    let health_base = base(y.health_ceiling, m.health_before);
    let pension_base = base(y.pension_ceiling, m.pension_before);
    out.insert("health_base", health_base);
    out.insert("pension_base", pension_base);
    // The transition range (§ 20 Abs. 2, 2a SGB IV): a whole month with wages above the minor limit up to its upper
    // limit; the total on the reduced base BE, the employee's share on BE_AN, the employer's the difference (§ 2 Abs. 2
    // BVV).
    let transition = m.days == 30 && wages > y.minor_limit && wages <= y.transition_limit;
    let (f, fd) = y.factor;
    let (g, o) = (y.minor_limit, y.transition_limit);
    let total_base = |b: i128| -> (i128, i128) {
        if transition { (f * g * (o - g) + (o * fd - g * f) * (b - g), fd * (o - g)) } else { (b, 1) }
    };
    let employee_base = |b: i128| -> (i128, i128) { if transition { (o * (b - g), o - g) } else { (b, 1) } };
    let bases = |b: i128| (total_base(b), employee_base(b));
    match health {
        1 | 3 => {
            let rate = if health == 1 { y.health } else { y.health_reduced };
            branch(&mut out, "health", bases(health_base), rate, true, transition);
            branch(&mut out, "health_additional", bases(health_base), m.additional, true, transition);
        }
        _ => {}
    }
    if care == 1 || care == 2 {
        // Saxony: the employee pays one point more, the employer one less (§ 58 Abs. 3 SGB XI); without children a
        // surcharge, with more than one a reduction, both the employee's alone (§ 55 Abs. 3 SGB XI).
        let mut extra = if m.saxony { y.saxony / 2 } else { 0 };
        let mut employer_less = if m.saxony { y.saxony / 2 } else { 0 };
        if care == 2 {
            extra = 0;
            employer_less = 0;
        }
        let surcharge = if m.childless { y.childless } else { 0 };
        let reduction = if m.children >= 2 { i128::from(m.children.min(5) - 1) * y.per_child } else { 0 };
        branch(&mut out, "care", bases(health_base), y.care - 2 * employer_less, care == 1, transition);
        if care == 1 {
            let adjust = 2 * extra + surcharge;
            let (tn, td) = total_base(health_base);
            let (an, ad) = employee_base(health_base);
            let more = share(tn, td, adjust) - share(an, ad, reduction);
            *out.entry("care_employee").or_insert(0) += more;
        }
    }
    match pension {
        1 | 3 => branch(&mut out, "pension", bases(pension_base), y.pension, pension == 1, transition),
        _ => {}
    }
    match unemployment {
        1 | 2 => branch(&mut out, "unemployment", bases(pension_base), y.unemployment, unemployment == 1, transition),
        _ => {}
    }
    // Levies (§ 7 AAG, § 358 SGB III): U1 and U2 on the current wages up to the pension ceiling, U3 on the base of the
    // pension insurance; the employer's alone, on the wages themselves in the transition range.
    let levy_base = m.current.min(y.pension_ceiling * m.days / 360);
    if let Some(rate) = m.u1 {
        out.insert("u1", share(levy_base, 1, rate));
    }
    if let Some(rate) = m.u2 {
        out.insert("u2", share(levy_base, 1, rate));
    }
    out.insert("u3", share(pension_base, 1, y.insolvency));
    // A trainee with pay up to the limit: the employer bears every contribution, health with the average additional
    // rate (§ 20 Abs. 3 SGB IV, § 242 Abs. 3 Nr. 6 SGB V); above it, by a one-off payment, the employee bears half of
    // the part above the limit (§ 20 Abs. 3 Satz 2).
    if m.trainee && !transition {
        let above = (wages - y.trainee_limit).max(0);
        for name in ["health", "health_additional", "care", "pension", "unemployment"] {
            let employee = employee_key(name);
            let Some(paid) = out.remove(employee) else { continue };
            let rate = match name {
                "health" => {
                    if health == 3 {
                        y.health_reduced
                    } else {
                        y.health
                    }
                }
                "health_additional" => m.additional,
                "care" => y.care,
                "pension" => y.pension,
                _ => y.unemployment,
            };
            let stays = share(above, 1, rate / 2).min(paid);
            if stays != 0 {
                out.insert(employee, stays);
            }
            *out.entry(employer_key(name)).or_insert(0) += paid - stays;
        }
    }
    // Pensions: health at the general rate with the additional one and care in full, borne by the pensioner and
    // withheld by the employer as paying agency (§§ 248, 250, 256 SGB V, § 59 SGB XI); nothing up to a twentieth of
    // the reference value, and for health that twentieth is deducted from company pensions (§ 226 Abs. 2 SGB V).
    if m.pension > y.pension_threshold && matches!(health, 1 | 3) {
        let base = m.pension.min(y.health_ceiling / 12);
        out.insert("pension_health", share(base - y.pension_threshold, 1, y.health + m.additional));
        let care_rate = y.care + if m.childless { y.childless } else { 0 }
            - if m.children >= 2 { i128::from(m.children.min(5) - 1) * y.per_child } else { 0 };
        out.insert("pension_care", share(base, 1, care_rate));
    }
    out.retain(|_, v| *v != 0);
    out
}

type Fraction = (i128, i128);

/// One branch of the insurance: the rate split in halves, or the employer's half alone; in the transition range the
/// total on the reduced base, the employee's share on its own base, the employer's the difference (§ 2 BVV).
fn branch(
    out: &mut BTreeMap<&'static str, i128>,
    name: &str,
    ((tn, td), (an, ad)): (Fraction, Fraction),
    rate: i128,
    employee: bool,
    transition: bool,
) {
    let half = share(tn, td, rate / 2);
    let (employee_share, employer_share) = if !employee {
        (0, half)
    } else if transition {
        let employee_share = share(an, ad, rate / 2);
        (employee_share, 2 * half - employee_share)
    } else {
        (half, half)
    };
    if employee_share != 0 {
        out.insert(employee_key(name), employee_share);
    }
    if employer_share != 0 {
        out.insert(employer_key(name), employer_share);
    }
}

fn employee_key(name: &str) -> &'static str {
    match name {
        "health" => "health_employee",
        "health_additional" => "health_additional_employee",
        "care" => "care_employee",
        "pension" => "pension_employee",
        _ => "unemployment_employee",
    }
}

fn employer_key(name: &str) -> &'static str {
    match name {
        "health" => "health_employer",
        "health_additional" => "health_additional_employer",
        "care" => "care_employer",
        "pension" => "pension_employer",
        _ => "unemployment_employer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn month(current: i128) -> Month {
        Month { groups: [1, 1, 1, 1], current, days: 30, additional: 29_000, ..Month::default() }
    }

    #[test]
    fn a_month_of_3000_euros() {
        let c = contributions(&year(2026).unwrap(), &month(300_000));
        // 7.3 % = 219.00, 1.45 % = 43.50, 1.8 % = 54.00, 9.3 % = 279.00, 1.3 % = 39.00, U3 0.15 % = 4.50.
        assert_eq!((c["health_employee"], c["health_employer"]), (21_900, 21_900));
        assert_eq!((c["health_additional_employee"], c["health_additional_employer"]), (4_350, 4_350));
        assert_eq!((c["care_employee"], c["care_employer"]), (5_400, 5_400));
        assert_eq!((c["pension_employee"], c["unemployment_employee"]), (27_900, 3_900));
        assert_eq!(c["u3"], 450);
        // Without children 0.6 % more for the employee; with three children two reductions of 0.25 %.
        let c = contributions(&year(2026).unwrap(), &Month { childless: true, ..month(300_000) });
        assert_eq!(c["care_employee"], 5_400 + 1_800);
        let c = contributions(&year(2026).unwrap(), &Month { children: 3, ..month(300_000) });
        assert_eq!(c["care_employee"], 5_400 - 1_500);
        // Saxony: 2.3 % and 1.3 %.
        let c = contributions(&year(2026).unwrap(), &Month { saxony: true, ..month(300_000) });
        assert_eq!((c["care_employee"], c["care_employer"]), (6_900, 3_900));
    }

    #[test]
    fn ceilings_and_one_off_payments() {
        // 7,000 euros: health up to 5,812.50, pension below 8,450.
        let c = contributions(&year(2026).unwrap(), &month(700_000));
        assert_eq!((c["health_base"], c["pension_base"]), (581_250, 700_000));
        // A bonus of 10,000 in the third month after two of 7,000: health has no room left, pension 3 × 8,450 −
        // 3 × 7,000 = 4,350.
        let m = Month {
            one_off: 1_000_000,
            days_before: 60,
            health_before: 2 * 581_250,
            pension_before: 2 * 700_000,
            ..month(700_000)
        };
        let c = contributions(&year(2026).unwrap(), &m);
        assert_eq!((c["health_base"], c["pension_base"]), (581_250, 700_000 + 435_000));
        // Half a month: the ceiling of 15 days.
        let c = contributions(&year(2026).unwrap(), &Month { days: 15, ..month(700_000) });
        assert_eq!(c["health_base"], 6_975_000 * 15 / 360);
    }

    #[test]
    fn the_transition_range() {
        // 1,000 euros: BE = 0.6619 × 603 + (2000 − 603 × 0.6619) / 1397 × 397 = 854.06…;
        // BE_AN = 2000 / 1397 × 397 = 568.36…
        let c = contributions(&year(2026).unwrap(), &month(100_000));
        // Pension: total 2 × round(9.3 % × BE), employee 9.3 % × BE_AN = 52.86.
        assert_eq!(c["pension_employee"], 5_286);
        let total = c["pension_employee"] + c["pension_employer"];
        assert_eq!(total, 2 * 7_943);
        // At the upper limit the shares are equal again.
        let c = contributions(&year(2026).unwrap(), &month(200_000));
        assert_eq!(c["pension_employee"], c["pension_employer"]);
    }

    #[test]
    fn a_minor_employment() {
        let m = Month { groups: [6, 1, 0, 0], minor: true, current: 60_000, days: 30, ..Month::default() };
        let c = contributions(&year(2026).unwrap(), &m);
        // 13 % = 78.00; pension 15 % = 90.00 and the employee 18.6 % − that = 111.60 − 90.00 = 21.60.
        assert_eq!((c["health_flat"], c["pension_flat"], c["pension_employee"]), (7_800, 9_000, 2_160));
        assert_eq!((c["u1"], c["u2"], c["u3"]), (480, 132, 90));
        // In a private household: 5 % and 5 %, the employee 13.6 %, accident insurance 1.6 %, no insolvency levy.
        let c = contributions(&year(2026).unwrap(), &Month { household: true, ..m });
        assert_eq!((c["health_flat"], c["pension_flat"], c["pension_employee"]), (3_000, 3_000, 8_160));
        assert_eq!((c["accident"], c.get("u3")), (960, None));
    }

    #[test]
    fn a_short_term_employment() {
        let m = Month { short_term: true, current: 150_000, days: 10, ..Month::default() };
        let c = contributions(&year(2026).unwrap(), &m);
        assert_eq!((c.get("u1"), c["u2"], c["u3"]), (None, 330, 225));
        let c = contributions(&year(2026).unwrap(), &Month { over_four_weeks: true, ..m });
        assert_eq!(c["u1"], 1_200);
        assert!(!c.keys().any(|k| k.contains("employee")));
    }
}
