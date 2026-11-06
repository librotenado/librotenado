// SPDX-License-Identifier: MIT
//! Income tax in Germany of a person running a business (spec part 0012): the profit of the books with what the person
//! states beside them, the tariff, the reduction for trade tax, the solidarity surcharge, church tax and the advance
//! payments.

use crate::{Chart, Date, EventSet, PartyId, ProfitTaxError, ProfitTaxFacts, TaxSubject};

/// What the books do not hold of a person's income tax.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IncomeTaxFacts {
    /// Assessed jointly with a spouse (§ 26b EStG): the splitting tariff (§ 32a Abs. 5 EStG).
    pub joint: bool,
    /// The sum of the other positive income of the person, or of both spouses – wages, rents, capital not taxed at the
    /// flat rate – in base units.
    pub other_income: i128,
    /// What is deducted from the total income: special expenses, extraordinary burdens, allowances (§§ 10 to 10c, 33
    /// to 33b EStG), in base units.
    pub deductions: i128,
    /// Losses carried forward, not yet used (§ 10d EStG), in base units.
    pub losses: i128,
    /// The rate of church tax in percent where the person belongs to a church that levies it – 8 in Bavaria and
    /// Baden-Württemberg, 9 elsewhere (the church tax laws of the states).
    pub church_rate: Option<u8>,
    /// The allowances for children in tenths (§ 32 Abs. 6 EStG): 10 for a child whose allowances – for the child and
    /// for care, education and training – are the person's in full, or the spouses' together; 5 for half of them.
    pub child_allowances: u32,
    /// The child benefit (Kindergeld) for the year for those children, in base units (§ 31 EStG).
    pub child_benefit: i128,
    /// Extraordinary income contained in the other income or the profit – compensation, pay for work of several
    /// years – taxed by the fifth (§ 34 Abs. 1, 2 Nr. 2 and 4 EStG), in base units.
    pub extraordinary: i128,
    /// Shares of profits of partnerships (Mitunternehmer, § 15 Abs. 1 Satz 1 Nr. 2 EStG), as their separate
    /// determination states them, in base units; part of the business income.
    pub partnership_income: i128,
    /// The person's shares of the trade tax base amounts and of the trade tax of those partnerships (§ 35 Abs. 2 EStG),
    /// in base units.
    pub partnership_trade: (i128, i128),
    /// Capital income not yet taxed at source (§ 20 EStG), before the saver's allowance, in base units: taxed
    /// separately at 25 % (§ 32d Abs. 1 EStG).
    pub capital_income: i128,
    /// The number of children for which a single parent gets the relief (§ 24b EStG); none otherwise.
    pub single_parent_children: u8,
}

/// The income tax of a person for a calendar year, in base units of the euro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IncomeTax {
    /// The profit of the business (§ 15 or § 18 EStG): the income of the taxes on profit.
    pub business: i128,
    /// The other income stated.
    pub other_income: i128,
    /// Losses carried forward used (§ 10d Abs. 2 EStG).
    pub loss_deduction: i128,
    /// The deductions stated.
    pub deductions: i128,
    /// The taxable income, rounded down to full euros (§ 32a Abs. 1 Satz 1 EStG).
    pub taxable: i128,
    /// The allowances for children deducted where the child benefit is worth less (§ 31 EStG).
    pub child_allowances: i128,
    /// The tariff tax (§ 32a EStG, with § 34 for extraordinary income), in full euros.
    pub tariff: i128,
    /// The child benefit added to the tax where the allowances were deducted (§ 2 Abs. 6 Satz 3 EStG).
    pub child_benefit_added: i128,
    /// The relief for single parents deducted (§ 24b EStG).
    pub single_parent_relief: i128,
    /// Capital income after the saver's allowance (§ 20 Abs. 9 EStG).
    pub capital_income: i128,
    /// The tax on it at 25 %, less a quarter of the church tax on it (§ 32d Abs. 1 EStG).
    pub capital_tax: i128,
    /// The reduction for trade tax (§ 35 EStG).
    pub trade_tax_credit: i128,
    /// The income tax assessed: the tariff tax less the reduction.
    pub assessed: i128,
    /// The solidarity surcharge (§§ 3, 4 SolZG).
    pub solidarity: i128,
    /// Church tax, on the tariff tax without the reduction for trade tax (§ 51a Abs. 2 Satz 3 EStG).
    pub church: i128,
    /// Each advance payment, due on 10 March, June, September and December (§ 37 Abs. 1 EStG); zero below 400 euros a
    /// year or 100 euros a payment (Abs. 5).
    pub advance_payment: i128,
}

const EURO: i128 = 100;

/// The tariff income tax in full euros for a taxable income in full euros (§ 32a Abs. 1 EStG), by the values of the
/// year (`LawValues::de`, keys `est.*`): 2025 and 2026; `None` for a year without them (OPEN).
pub fn income_tax_tariff(year: u16, taxable_euros: i128) -> Option<i128> {
    let law = crate::LawValues::de();
    let day = Date::new(year, 1, 1)?;
    // The tariff is set for each year: without its basic allowance, no tariff.
    law.of_year("est.basic_allowance", day)?;
    let euros = |key: &str| law.cents(key, day).map(|c| c / EURO);
    let frac = |key: &str| law.fraction(key, day);
    let (gfb, z2, z3, z4) =
        (euros("est.basic_allowance")?, euros("est.zone2_end")?, euros("est.zone3_end")?, euros("est.zone4_end")?);
    // A value a × 10^-s in ten-billionths of a euro, times n.
    let scaled = |(units, denominator): (i128, i128), n: i128| units * n * 10_000_000_000 / denominator;
    let x = taxable_euros.max(0);
    // Each zone in ten-billionths of a euro, then rounded down to full euros; y and z are ten-thousandths of the
    // income above the zone's start.
    let tenth_billions: i128 = if x <= gfb {
        0
    } else if x <= z2 {
        let d = x - gfb;
        scaled(frac("est.zone2_a")?, d * d) / 100_000_000 + scaled(frac("est.zone2_b")?, d) / 10_000
    } else if x <= z3 {
        let d = x - z2;
        scaled(frac("est.zone3_a")?, d * d) / 100_000_000
            + scaled(frac("est.zone3_b")?, d) / 10_000
            + scaled(frac("est.zone3_c")?, 1)
    } else {
        let (rate, minus) =
            if x <= z4 { ("est.zone4_rate", "est.zone4_minus") } else { ("est.zone5_rate", "est.zone5_minus") };
        scaled(frac(rate)?, x) / 100 - scaled(frac(minus)?, 1)
    };
    Some(tenth_billions / 10_000_000_000)
}

/// The tariff for a taxable income in base units: on the income rounded down to full euros, by the splitting tariff
/// for spouses (§ 32a Abs. 5 EStG); in base units.
fn tariff(year: u16, taxable: i128, joint: bool) -> Option<i128> {
    let euros = taxable.max(0) / EURO;
    let tax = if joint { 2 * income_tax_tariff(year, euros / 2)? } else { income_tax_tariff(year, euros)? };
    Some(tax * EURO)
}

/// The tariff for a taxable income with extraordinary income in it taxed by the fifth (§ 34 Abs. 1 EStG): the tax on
/// the rest plus five times the tax on a fifth of the extraordinary income added to it; where the rest is negative,
/// five times the tax on a fifth of the whole; in base units.
fn fifth(year: u16, taxable: i128, extraordinary: i128, joint: bool) -> Option<i128> {
    let x = extraordinary.min(taxable);
    if x <= 0 {
        return tariff(year, taxable, joint);
    }
    let rest = taxable - x;
    let base = tariff(year, rest, joint)?;
    Some(base + 5 * (tariff(year, rest + x / 5, joint)? - base))
}

/// The solidarity surcharge on an income tax assessed (§ 3 Abs. 3, § 4 SolZG) of 2026: none up to the exemption
/// limit of 20,350 euros, or 40,700 euros for spouses; 5.5 %, but no more than 11.9 % of what exceeds the limit;
/// fractions of a cent left out. The values are those of `LawValues::de` (`soli.*`).
pub fn solidarity_surcharge(assessed: i128, joint: bool) -> i128 {
    solidarity_of(2026, assessed, joint)
}

/// The solidarity surcharge by the values of `year`.
fn solidarity_of(year: u16, assessed: i128, joint: bool) -> i128 {
    let law = crate::LawValues::de();
    let day = Date::new(year, 1, 1).expect("a date");
    let single = law.cents("soli.limit", day).expect("published");
    let limit = if joint { 2 * single } else { single };
    if assessed <= limit {
        return 0;
    }
    let rate = law.millionths("soli.rate", day).expect("published");
    let mitigation = law.millionths("soli.mitigation", day).expect("published");
    (assessed * rate / 1_000_000).min((assessed - limit) * mitigation / 1_000_000)
}

impl EventSet {
    /// The income tax of the person `owner` for the calendar year from `from` to `to` (spec part 0012): the profit of
    /// its books as for the taxes on profit (`profit`), with what the person states (`facts`). `None` for a year whose
    /// tariff this version does not have.
    pub fn income_tax(
        &self,
        chart: &Chart,
        owner: &PartyId,
        from: Date,
        to: Date,
        profit: &ProfitTaxFacts,
        facts: &IncomeTaxFacts,
    ) -> Result<Option<IncomeTax>, ProfitTaxError> {
        let business = self.profit_tax(
            chart,
            owner,
            from,
            to,
            &ProfitTaxFacts { subject: TaxSubject::Individual, ..profit.clone() },
        )?;
        let mut tax = IncomeTax {
            business: business.income + facts.partnership_income,
            other_income: facts.other_income,
            deductions: facts.deductions,
            ..IncomeTax::default()
        };
        let total = tax.business + tax.other_income;
        // Losses: up to 1 million euros, 2 million for spouses, in full, above it up to 70 % (§ 10d Abs. 2 EStG).
        let law = crate::LawValues::de();
        let day = Date::new(to.year(), 1, 1).expect("a year of a date");
        let value = |key: &str| law.cents(key, day).expect("published");
        let percent = |key: &str| law.millionths(key, day).expect("published") / 10_000;
        let full = value("est.loss_full");
        let threshold = if facts.joint { 2 * full } else { full };
        if total > 0 && facts.losses > 0 {
            let rest = percent("est.loss_rest");
            let limit = if total <= threshold { total } else { threshold + (total - threshold) * rest / 100 };
            tax.loss_deduction = facts.losses.min(limit);
        }
        // The relief for single parents: 4,260 euros and 240 for each further child (§ 24b Abs. 2 EStG).
        if facts.single_parent_children > 0 {
            tax.single_parent_relief = value("est.single_parent")
                + value("est.single_parent_more") * i128::from(facts.single_parent_children - 1);
        }
        let before_children = (total - tax.loss_deduction - tax.deductions - tax.single_parent_relief).max(0);
        let year = to.year();
        // The allowances for one child, both parents together (`est.child_allowance`, § 32 Abs. 6 EStG).
        let law = crate::LawValues::de();
        let day = Date::new(year, 1, 1).expect("a year of a date");
        let child = law.cents("est.child_allowance", day).unwrap_or(0);
        let allowances = child * i128::from(facts.child_allowances) / 10;
        let with = (before_children - allowances).max(0) / EURO * EURO;
        let without = before_children / EURO * EURO;
        let extraordinary = facts.extraordinary.max(0);
        let Some(tax_with) = fifth(year, with, extraordinary, facts.joint) else { return Ok(None) };
        let Some(tax_without) = fifth(year, without, extraordinary, facts.joint) else { return Ok(None) };
        // The allowances only where they save more than the child benefit; then the benefit is added (§ 31 EStG).
        if allowances > 0 && tax_without - tax_with > facts.child_benefit {
            tax.child_allowances = allowances;
            tax.taxable = with;
            tax.tariff = tax_with;
            tax.child_benefit_added = facts.child_benefit;
        } else {
            tax.taxable = without;
            tax.tariff = tax_without;
        }
        // Solidarity surcharge and church tax always as if the allowances were deducted (§ 3 Abs. 2 SolZG, § 51a
        // Abs. 2 EStG).
        let base_tariff = tax_with;
        // Trade tax: four times the base amount, no more than the trade tax, nor than the share of the tariff tax that
        // falls on the profit of the business among the positive income (§ 35 Abs. 1 EStG).
        let (partner_base, partner_tax) = facts.partnership_trade;
        let (own_base, own_tax) = business.trade.as_ref().map_or((0, 0), |t| (t.base, t.tax));
        let (base, paid) = (own_base + partner_base, own_tax + partner_tax);
        let positive = tax.business.max(0) + tax.other_income.max(0);
        let credit = |tariff: i128| {
            let share = if positive > 0 { tariff * tax.business.max(0) / positive } else { 0 };
            (law.fraction("est.trade_tax_credit", day).map_or(4, |(n, d)| n / d) * base).min(paid).min(share).max(0)
        };
        if base > 0 {
            tax.trade_tax_credit = credit(tax.tariff);
        }
        // Capital income: after the saver's allowance of 1,000 euros, 2,000 for spouses (§ 20 Abs. 9 EStG), 25 %; with
        // church tax (e − 4q) / (4 + k), q the foreign tax credited, here none (§ 32d Abs. 1 Satz 3 to 5 EStG).
        let saver = value("est.saver_allowance");
        let allowance = if facts.joint { 2 * saver } else { saver };
        tax.capital_income = (facts.capital_income - allowance).max(0);
        let k = facts.church_rate.map_or(0, i128::from);
        // 25 % as e / (4 + k): with the rate r, e · r / (1 + r · k).
        let r = percent("est.capital_rate");
        tax.capital_tax = tax.capital_income * r * 100 / (10_000 + r * k);
        tax.assessed = tax.tariff - tax.trade_tax_credit + tax.child_benefit_added + tax.capital_tax;
        // The solidarity surcharge: the exemption limit tested without the tax on capital income, which bears 5.5 %
        // in any case (§ 3 Abs. 3, § 4 Satz 4 SolZG).
        let surcharge_base = base_tariff - if base > 0 { credit(base_tariff) } else { 0 };
        tax.solidarity = solidarity_of(to.year(), surcharge_base, facts.joint)
            + tax.capital_tax * law.millionths("soli.rate", day).expect("published") / 1_000_000;
        tax.church = facts.church_rate.map_or(0, |rate| (base_tariff + tax.capital_tax) * i128::from(rate) / 100);
        let quarter = tax.assessed / 4;
        let due = tax.assessed >= value("est.advance_year") && quarter >= value("est.advance_quarter");
        tax.advance_payment = if due { quarter } else { 0 };
        Ok(Some(tax))
    }
}

impl IncomeTax {
    /// The figures as fields of the report `de-est` (names under `de-est/<field>`), in base units.
    pub fn fields(&self) -> std::collections::BTreeMap<String, i128> {
        [
            ("business", self.business),
            ("other_income", self.other_income),
            ("loss_deduction", self.loss_deduction),
            ("deductions", self.deductions),
            ("taxable", self.taxable),
            ("child_allowances", self.child_allowances),
            ("tariff", self.tariff),
            ("child_benefit_added", self.child_benefit_added),
            ("single_parent_relief", self.single_parent_relief),
            ("capital_income", self.capital_income),
            ("capital_tax", self.capital_tax),
            ("trade_tax_credit", self.trade_tax_credit),
            ("assessed", self.assessed),
            ("solidarity", self.solidarity),
            ("church", self.church),
            ("advance_payment", self.advance_payment),
        ]
        .into_iter()
        .filter(|(_, units)| *units != 0)
        .map(|(field, units)| (field.to_owned(), units))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tariff_of_2026() {
        let t = |x| income_tax_tariff(2026, x).unwrap();
        assert_eq!((t(0), t(12_348)), (0, 0), "the basic allowance");
        // The zones meet: no jump of more than a euro at their borders.
        for (below, above) in [(17_799, 17_800), (69_878, 69_879), (277_825, 277_826)] {
            assert!((t(above) - t(below)).abs() <= 1, "{below}: {} {}", t(below), t(above));
        }
        // 0.42 × 100,000 − 11,135.63 = 30,864.37; 0.45 × 300,000 − 19,470.38 = 115,529.62.
        assert_eq!((t(100_000), t(300_000)), (30_864, 115_529));
        // (914.51 × 0.1652 + 1,400) × 0.1652 = 256.23; (173.10 × 2.2201 + 2,397) × 2.2201 + 1,034.87 = 7,209.63.
        assert_eq!(t(14_000), 256);
        assert_eq!(t(40_000), 7_209);
        // 2025: (176.64 × 2.2557 + 2,397) × 2.2557 + 1,015.13 = 7,320.82; no tariff before it, nor for 2027 yet.
        assert_eq!(income_tax_tariff(2025, 40_000), Some(7_320));
        assert_eq!((income_tax_tariff(2024, 40_000), income_tax_tariff(2027, 40_000)), (None, None));
        // Splitting: twice the tax on half.
        assert_eq!(tariff(2026, 8_000_000 * 100 / 100, true), Some(2 * t(40_000) * 100));
    }

    #[test]
    fn the_solidarity_surcharge() {
        let e = 100;
        assert_eq!(solidarity_surcharge(20_350 * e, false), 0);
        // Just above the limit, 11.9 % of the excess; far above it, 5.5 %.
        assert_eq!(solidarity_surcharge(21_000 * e, false), 650 * e * 119 / 1000);
        assert_eq!(solidarity_surcharge(100_000 * e, false), 5_500 * e);
        assert_eq!(solidarity_surcharge(40_000 * e, true), 0);
    }

    /// A sole proprietor with a profit of 120,000 euros and trade tax at a multiplier of 400 %: the reduction of § 35
    /// is four times the base amount, no more than the trade tax; church tax on the tariff tax before it.
    #[test]
    fn a_sole_proprietor() {
        use crate::{Chart, party::tests::party, profit_tax::tests};
        let e = 100;
        let set = tests::books(&[("8400", -120_000 * e)]);
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let facts = IncomeTaxFacts {
            other_income: 10_000 * e,
            deductions: 5_000 * e,
            church_rate: Some(9),
            ..Default::default()
        };
        let tax = set
            .income_tax(&Chart::v0_1(), &party(1), from, to, &tests::facts(TaxSubject::Individual), &facts)
            .unwrap()
            .unwrap();
        assert_eq!(tax.business, 120_000 * e);
        assert_eq!(tax.taxable, 125_000 * e);
        let tariff = income_tax_tariff(2026, 125_000).unwrap() * e;
        assert_eq!(tax.tariff, tariff);
        // Trade income 120,000 less 24,500 = 95,500 × 3.5 % = 3,342.50 base, × 4 = 13,370; trade tax 13,370.
        assert_eq!(tax.trade_tax_credit, 13_370 * e);
        assert_eq!(tax.assessed, tariff - 13_370 * e);
        assert_eq!(tax.solidarity, solidarity_surcharge(tax.assessed, false));
        assert_eq!(tax.church, tariff * 9 / 100);
        assert_eq!(tax.advance_payment, tax.assessed / 4);
        // A freelancer: no trade tax, no reduction.
        let freelancer = ProfitTaxFacts { multiplier: None, ..tests::facts(TaxSubject::Individual) };
        let tax = set.income_tax(&Chart::v0_1(), &party(1), from, to, &freelancer, &facts).unwrap().unwrap();
        assert_eq!((tax.trade_tax_credit, tax.assessed), (0, tariff));
    }

    /// Children: the allowances only where they save more than the child benefit, which is then added; solidarity
    /// surcharge and church tax always with them. Extraordinary income by the fifth.
    #[test]
    fn children_and_the_fifth() {
        use crate::{Chart, party::tests::party, profit_tax::tests};
        let e = 100;
        let set = tests::books(&[("8400", -150_000 * e)]);
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let freelancer = ProfitTaxFacts { multiplier: None, ..tests::facts(TaxSubject::Individual) };
        let run = |facts: IncomeTaxFacts| {
            set.income_tax(&Chart::v0_1(), &party(1), from, to, &freelancer, &facts).unwrap().unwrap()
        };
        let none = run(IncomeTaxFacts { church_rate: Some(9), ..Default::default() });
        // 150,000: the allowances of 9,756 save about 42 % of them, more than a child benefit of 3,108.
        let child = run(IncomeTaxFacts {
            church_rate: Some(9),
            child_allowances: 10,
            child_benefit: 3_108 * e,
            ..Default::default()
        });
        assert_eq!(child.child_allowances, 9_756 * e);
        assert_eq!(child.taxable, 150_000 * e - 9_756 * e);
        assert_eq!(child.assessed, child.tariff + 3_108 * e);
        assert!(child.church < none.church);
        // 20,000: the allowances save less than the benefit – not deducted; church tax still with them.
        let set = tests::books(&[("8400", -20_000 * e)]);
        let small = set
            .income_tax(
                &Chart::v0_1(),
                &party(1),
                from,
                to,
                &freelancer,
                &IncomeTaxFacts {
                    church_rate: Some(9),
                    child_allowances: 10,
                    child_benefit: 3_108 * e,
                    ..Default::default()
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!((small.child_allowances, small.child_benefit_added), (0, 0));
        assert_eq!(small.church, 0, "church tax on 10,244 after the allowances: nothing");
        // § 34: 40,000 of which 30,000 extraordinary: T(10,000) + 5 × (T(16,000) − T(10,000)).
        let set = tests::books(&[("8400", -40_000 * e)]);
        let fifth = set
            .income_tax(
                &Chart::v0_1(),
                &party(1),
                from,
                to,
                &freelancer,
                &IncomeTaxFacts { extraordinary: 30_000 * e, ..Default::default() },
            )
            .unwrap()
            .unwrap();
        let t = |x| income_tax_tariff(2026, x).unwrap() * e;
        assert_eq!(fifth.tariff, t(10_000) + 5 * (t(16_000) - t(10_000)));
    }

    /// Capital income at 25 % after the saver's allowance, with church tax by the formula; the relief for single
    /// parents.
    #[test]
    fn capital_income_and_single_parents() {
        use crate::{Chart, party::tests::party, profit_tax::tests};
        let e = 100;
        let set = tests::books(&[("8400", -50_000 * e)]);
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let freelancer = ProfitTaxFacts { multiplier: None, ..tests::facts(TaxSubject::Individual) };
        let run = |facts: IncomeTaxFacts| {
            set.income_tax(&Chart::v0_1(), &party(1), from, to, &freelancer, &facts).unwrap().unwrap()
        };
        let plain = run(IncomeTaxFacts::default());
        let capital = run(IncomeTaxFacts { capital_income: 5_090 * e, church_rate: Some(9), ..Default::default() });
        // 4,090 × 100 / 409 = 1,000; church 9 % of it on top, solidarity 5.5 %.
        assert_eq!((capital.capital_income, capital.capital_tax), (4_090 * e, 1_000 * e));
        assert_eq!(capital.assessed, plain.assessed + 1_000 * e);
        assert_eq!(capital.solidarity, 55 * e);
        let single = run(IncomeTaxFacts { single_parent_children: 2, ..Default::default() });
        assert_eq!(single.single_parent_relief, 4_500 * e);
        assert_eq!(single.taxable, 45_500 * e);
    }
}
