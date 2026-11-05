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
    /// The tariff tax (§ 32a EStG), in full euros.
    pub tariff: i128,
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

/// The tariff income tax in full euros for a taxable income in full euros (§ 32a Abs. 1 EStG), from 2026; `None` for
/// earlier years (OPEN).
pub fn income_tax_tariff(year: u16, taxable_euros: i128) -> Option<i128> {
    if year < 2026 {
        return None;
    }
    let x = taxable_euros.max(0);
    // Each zone in hundred-millionths of a euro, then rounded down to full euros.
    let scaled: i128 = match x {
        ..=12_348 => 0,
        12_349..=17_799 => {
            let d = x - 12_348;
            91_451 * d * d + 1_400 * 1_000_000 * d
        }
        17_800..=69_878 => {
            let d = x - 17_799;
            17_310 * d * d + 2_397 * 1_000_000 * d + 103_487 * 100_000_000
        }
        69_879..=277_825 => (42 * x - 1_113_563) * 100_000_000,
        _ => (45 * x - 1_947_038) * 100_000_000,
    };
    Some(scaled / 10_000_000_000)
}

/// The tariff for a taxable income in base units: on the income rounded down to full euros, by the splitting tariff
/// for spouses (§ 32a Abs. 5 EStG); in base units.
fn tariff(year: u16, taxable: i128, joint: bool) -> Option<i128> {
    let euros = taxable.max(0) / EURO;
    let tax = if joint { 2 * income_tax_tariff(year, euros / 2)? } else { income_tax_tariff(year, euros)? };
    Some(tax * EURO)
}

/// The solidarity surcharge on an income tax assessed (§ 3 Abs. 3, § 4 SolZG): none up to the exemption limit of
/// 20,350 euros, or 40,700 euros for spouses; 5.5 %, but no more than 11.9 % of what exceeds the limit; fractions of a
/// cent left out.
pub fn solidarity_surcharge(assessed: i128, joint: bool) -> i128 {
    let limit = if joint { 40_700 * EURO } else { 20_350 * EURO };
    if assessed <= limit {
        return 0;
    }
    (assessed * 55 / 1000).min((assessed - limit) * 119 / 1000)
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
            business: business.income,
            other_income: facts.other_income,
            deductions: facts.deductions,
            ..IncomeTax::default()
        };
        let total = tax.business + tax.other_income;
        // Losses: up to 1 million euros, 2 million for spouses, in full, above it up to 70 % (§ 10d Abs. 2 EStG).
        let threshold = if facts.joint { 2_000_000 * EURO } else { 1_000_000 * EURO };
        if total > 0 && facts.losses > 0 {
            let limit = if total <= threshold { total } else { threshold + (total - threshold) * 70 / 100 };
            tax.loss_deduction = facts.losses.min(limit);
        }
        tax.taxable = (total - tax.loss_deduction - tax.deductions).max(0) / EURO * EURO;
        let Some(tariff) = tariff(to.year(), tax.taxable, facts.joint) else { return Ok(None) };
        tax.tariff = tariff;
        // Trade tax: four times the base amount, no more than the trade tax, nor than the share of the tariff tax that
        // falls on the profit of the business among the positive income (§ 35 Abs. 1 EStG).
        if let Some(trade) = &business.trade {
            let positive = tax.business.max(0) + tax.other_income.max(0);
            let share = if positive > 0 { tax.tariff * tax.business.max(0) / positive } else { 0 };
            tax.trade_tax_credit = (4 * trade.base).min(trade.tax).min(share).max(0);
        }
        tax.assessed = tax.tariff - tax.trade_tax_credit;
        tax.solidarity = solidarity_surcharge(tax.assessed, facts.joint);
        tax.church = facts.church_rate.map_or(0, |rate| tax.tariff * i128::from(rate) / 100);
        let quarter = tax.assessed / 4;
        tax.advance_payment = if tax.assessed >= 400 * EURO && quarter >= 100 * EURO { quarter } else { 0 };
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
            ("tariff", self.tariff),
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
        assert_eq!(income_tax_tariff(2025, 40_000), None);
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
}
