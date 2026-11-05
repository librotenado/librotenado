// SPDX-License-Identifier: MIT
//! Taxes on profit in Germany (spec part 0012): the taxable income from the tax balance sheet, corporation tax with
//! the solidarity surcharge, and trade tax with its additions and reductions.

use std::collections::BTreeMap;

use crate::{AccountId, AccountKind, BalanceError, Basis, Chart, Date, EventSet, PartyId, Payload};

/// Who pays the taxes on profit: a corporation (Körperschaftsteuer), a natural person running a business or a
/// partnership of them (Einkommensteuer of the partners), or an association or other body with a business
/// (Körperschaftsteuer, trade tax allowance of 5,000 euros).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxSubject {
    /// A corporation – GmbH, UG, AG, KGaA (§ 1 Abs. 1 Nr. 1 KStG).
    Corporation,
    /// A sole proprietor – a natural person (§ 15 EStG).
    Individual,
    /// A partnership of natural persons – OHG, KG, GbR (§ 15 Abs. 1 Satz 1 Nr. 2 EStG).
    Partnership,
    /// An association, foundation or other body subject to corporation tax with a business (§ 11 Abs. 1 Satz 3 Nr. 2
    /// GewStG).
    Association,
}

impl TaxSubject {
    fn is_corporate(self) -> bool {
        matches!(self, TaxSubject::Corporation | TaxSubject::Association)
    }
}

/// The size of the holdings in corporations whose dividends the books receive – one for all (OPEN: per holding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Holding {
    /// At least 15 % at the beginning of the year: dividends exempt from trade tax as well (§ 9 Nr. 2a GewStG).
    #[default]
    AtLeast15,
    /// At least 10 %, below 15 %: dividends added back for trade tax (§ 8 Nr. 5 GewStG).
    From10To15,
    /// Below 10 % at the beginning of the year: dividends taxable for a corporation (§ 8b Abs. 4 KStG) and added back
    /// for trade tax.
    Below10,
}

/// What the books do not say: the trade tax multiplier of the municipality, losses carried forward, and figures
/// whoever files supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfitTaxFacts {
    /// Who pays.
    pub subject: TaxSubject,
    /// The trade tax multiplier of the municipality in percent, at least 200 (§ 16 Abs. 4 GewStG); none for a business
    /// not subject to trade tax – a freelancer (§ 18 EStG).
    pub multiplier: Option<u32>,
    /// Losses carried forward for corporation or income tax, not yet used (§ 10d EStG, § 8 Abs. 1 KStG), in base units.
    pub income_losses: i128,
    /// Trade tax losses carried forward (Fehlbeträge, § 10a GewStG), in base units.
    pub trade_losses: i128,
    /// A reduction for business land stated beside the books, in base units – the extended reduction of § 9 Nr. 1
    /// Satz 2 GewStG; the land tax booked as an expense (Satz 1, from 2025) comes from the accounts `land-tax`.
    pub land_reduction: i128,
    /// The sum of turnover and wages of the year, for the limit of donations of 4 per mille (§ 9 Abs. 1 Nr. 2 KStG,
    /// § 9 Nr. 5 GewStG); none to apply the limit of 20 % alone.
    pub turnover_and_wages: Option<i128>,
    /// The holdings dividends come from.
    pub holding: Holding,
    /// The profit is determined by receipts and payments (§ 4 Abs. 3 EStG): a booking counts on the day it is
    /// `attributed` to, where it has one (§ 11 EStG, spec part 0001).
    pub cash_basis: bool,
}

/// Taxable profit, corporation tax and trade tax of a financial year, in base units of the euro.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfitTax {
    /// The result of the year in the tax balance sheet: income less expenses.
    pub result: i128,
    /// Expenses not deductible and income exempt (`profit` `excluded`), as they changed the result.
    pub excluded: i128,
    /// What of income from and losses on holdings in corporations stays out (§ 8b KStG, § 3 Nr. 40 and § 3c Abs. 2
    /// EStG), as it changed the result.
    pub participation: i128,
    /// Remunerations of partners booked as expenses, which are their profit (§ 15 Abs. 1 Satz 1 Nr. 2 EStG).
    pub partner_remuneration: i128,
    /// Donations booked (positive).
    pub donations: i128,
    /// Donations deducted (§ 9 Abs. 1 Nr. 2 KStG); none for persons, who deduct them privately (§ 10b EStG).
    pub donations_deducted: i128,
    /// The profit for tax: income, or for a business of persons its profit (§ 7 GewStG starts from it).
    pub income: i128,
    /// Losses carried forward used (§ 10d Abs. 2 EStG).
    pub loss_deduction: i128,
    /// The taxable income (zu versteuerndes Einkommen) of a corporation; for persons the profit after losses.
    pub taxable: i128,
    /// Corporation tax (§ 23 KStG); zero for persons.
    pub corporation_tax: i128,
    /// The solidarity surcharge on it (§ 4 SolZG).
    pub solidarity: i128,
    /// Trade tax, if the business is subject to it.
    pub trade: Option<TradeTax>,
}

/// The trade tax of a financial year (§§ 7 to 11, 16 GewStG), in base units.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeTax {
    /// The profit it starts from (§ 7 GewStG).
    pub profit: i128,
    /// The parts of § 8 Nr. 1 GewStG before the allowance, by letter: `a` interest, `b` annuities, `c` silent
    /// partners, `d` rents of movables (one fifth; one tenth for electric vehicles and bicycles), `e` rents of land
    /// (one half), `f` licences (one quarter).
    pub financing: BTreeMap<char, i128>,
    /// The addition of § 8 Nr. 1: a quarter of their sum above 200,000 euros.
    pub financing_addition: i128,
    /// Dividends added back (§ 8 Nr. 5).
    pub dividends: i128,
    /// Shares of losses of partnerships added back (§ 8 Nr. 8).
    pub partnership_losses: i128,
    /// Donations added back (§ 8 Nr. 9).
    pub donations_added: i128,
    /// The reduction for business land (§ 9 Nr. 1): the land tax booked, and what is stated beside it.
    pub land: i128,
    /// Shares of profits of partnerships deducted (§ 9 Nr. 2).
    pub partnership_profits: i128,
    /// Donations deducted (§ 9 Nr. 5).
    pub donations: i128,
    /// The trade income before losses.
    pub trade_income: i128,
    /// Losses carried forward used (§ 10a).
    pub loss_deduction: i128,
    /// The trade income, rounded down to full 100 euros (§ 11 Abs. 1 Satz 3).
    pub rounded: i128,
    /// The allowance (§ 11 Abs. 1 Satz 3 Nr. 1 and 2).
    pub allowance: i128,
    /// The base amount: 3.5 % of the rest (§ 11 Abs. 2).
    pub base: i128,
    /// The multiplier in percent.
    pub multiplier: u32,
    /// The trade tax: the base amount times the multiplier.
    pub tax: i128,
}

impl ProfitTax {
    /// The lines of a person's profit in the forms of the income tax return (spec part 0012), by their Kennzahlen of
    /// the forms of 2025 (Formulare 2025AnlG231 and 2025AnlS), in base units: of a trade the Anlage G – Kz 10, the
    /// profit of the sole proprietorship; Kz 64, the base amount of trade tax, and Kz 66, the trade tax to pay, for the
    /// reduction of § 35 EStG –, of a freelancer the Anlage S – Kz 100. Names under `de-anlage-g/<Kz>` and
    /// `de-anlage-s/<Kz>`. Zero lines are left out.
    pub fn income_tax_lines(&self) -> (&'static str, BTreeMap<String, i128>) {
        let mut lines = BTreeMap::new();
        let form = match &self.trade {
            Some(trade) => {
                lines.insert("10".to_owned(), self.income);
                lines.insert("64".to_owned(), trade.base);
                lines.insert("66".to_owned(), trade.tax);
                "de-anlage-g"
            }
            None => {
                lines.insert("100".to_owned(), self.income);
                "de-anlage-s"
            }
        };
        lines.retain(|_, units| *units != 0);
        (form, lines)
    }

    /// The figures of corporation tax as fields of the report `de-kst` (names in the translation data under
    /// `de-kst/<field>`), in base units.
    pub fn fields(&self) -> BTreeMap<String, i128> {
        [
            ("result", self.result),
            ("excluded", self.excluded),
            ("participation", self.participation),
            ("partner_remuneration", self.partner_remuneration),
            ("donations", self.donations),
            ("donations_deducted", self.donations_deducted),
            ("income", self.income),
            ("loss_deduction", self.loss_deduction),
            ("taxable", self.taxable),
            ("corporation_tax", self.corporation_tax),
            ("solidarity", self.solidarity),
        ]
        .into_iter()
        .filter(|(_, units)| *units != 0)
        .map(|(field, units)| (field.to_owned(), units))
        .collect()
    }
}

impl TradeTax {
    /// The figures of trade tax as fields of the report `de-gewst` (names under `de-gewst/<field>`), in base units;
    /// the parts of § 8 Nr. 1 as `financing_a` to `financing_f`.
    pub fn fields(&self) -> BTreeMap<String, i128> {
        let mut fields: BTreeMap<String, i128> =
            self.financing.iter().map(|(letter, units)| (format!("financing_{letter}"), *units)).collect();
        for (field, units) in [
            ("profit", self.profit),
            ("financing_addition", self.financing_addition),
            ("dividends", self.dividends),
            ("partnership_losses", self.partnership_losses),
            ("donations_added", self.donations_added),
            ("land", self.land),
            ("partnership_profits", self.partnership_profits),
            ("donations", self.donations),
            ("trade_income", self.trade_income),
            ("loss_deduction", self.loss_deduction),
            ("rounded", self.rounded),
            ("allowance", self.allowance),
            ("base", self.base),
            ("tax", self.tax),
        ] {
            fields.insert(field.to_owned(), units);
        }
        fields.retain(|_, units| *units != 0);
        fields
    }
}

/// Why taxes on profit cannot be computed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfitTaxError {
    /// A sum out of range.
    #[error(transparent)]
    Balance(#[from] BalanceError),
    /// A multiplier below 200 % (§ 16 Abs. 4 Satz 2 GewStG).
    #[error("a trade tax multiplier is at least 200 %, not {0} %")]
    Multiplier(u32),
}

const EURO: i128 = 100;

/// `units × numerator / denominator`, rounded towards zero.
fn part(units: i128, numerator: i128, denominator: i128) -> i128 {
    units * numerator / denominator
}

/// The rate of corporation tax for a year, in tenths of a per cent (§ 23 Abs. 1 KStG).
pub fn corporation_tax_rate(year: u16) -> i128 {
    match year {
        ..=2027 => 150,
        2028 => 140,
        2029 => 130,
        2030 => 120,
        2031 => 110,
        _ => 100,
    }
}

/// Losses usable against `income`: all of it up to 1 million euros, above that `percent` of the rest (§ 10d Abs. 2
/// EStG: 70; § 10a GewStG: 60).
fn usable_losses(income: i128, losses: i128, percent: i128) -> i128 {
    if income <= 0 || losses <= 0 {
        return 0;
    }
    let million = 1_000_000 * EURO;
    let limit = if income <= million { income } else { million + part(income - million, percent, 100) };
    losses.min(limit)
}

impl EventSet {
    /// The taxes on profit of `owner` for the financial year from `from` to `to` (spec part 0012): from the bookings
    /// of the tax balance sheet in it and the attributes `profit` and `trade_tax` of their accounts, own or inherited.
    pub fn profit_tax(
        &self,
        chart: &Chart,
        owner: &PartyId,
        from: Date,
        to: Date,
        facts: &ProfitTaxFacts,
    ) -> Result<ProfitTax, ProfitTaxError> {
        if let Some(multiplier) = facts.multiplier.filter(|m| *m < 200) {
            return Err(ProfitTaxError::Multiplier(multiplier));
        }
        let kinds = self.account_kinds(chart);
        let attribute = self.account_attributes(chart);
        // What each account added to the result: credits of income positive, debits of expenses negative.
        let mut contributions: BTreeMap<&AccountId, i128> = BTreeMap::new();
        for (_, event) in self.ordered() {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            let day = if facts.cash_basis { booking.attributed.unwrap_or(booking.date) } else { booking.date };
            if day < from || day > to || !booking.counts_in(Basis::Tax) {
                continue;
            }
            for posting in booking.postings.iter().filter(|p| p.owner == *owner) {
                if matches!(kinds.get(&posting.account), Some(AccountKind::Income | AccountKind::Expense)) {
                    let sum = contributions.entry(&posting.account).or_default();
                    *sum = sum.checked_sub(posting.functional().base_units()).ok_or(BalanceError::Overflow)?;
                }
            }
        }
        let corporate = facts.subject.is_corporate();
        let mut tax = ProfitTax::default();
        let mut trade = TradeTax::default();
        let mut dividends_exempt = 0;
        let mut land_tax = 0;
        for (account, units) in &contributions {
            let units = *units;
            tax.result += units;
            match attribute(account, "profit").as_deref() {
                Some("excluded") => tax.excluded += units,
                Some("partner-remuneration") if facts.subject == TaxSubject::Partnership => {
                    tax.partner_remuneration += units;
                }
                Some("donation") => tax.donations -= units,
                Some("participation-dividend") => {
                    let exempt = match (corporate, facts.holding) {
                        (true, Holding::Below10) => 0,
                        (true, _) => part(units, 95, 100),
                        (false, _) => part(units, 40, 100),
                    };
                    tax.participation += exempt;
                    if facts.holding != Holding::AtLeast15 {
                        dividends_exempt += exempt;
                    }
                }
                Some("participation-income") => {
                    tax.participation += if corporate { part(units, 95, 100) } else { part(units, 40, 100) };
                }
                Some("participation-loss") => {
                    tax.participation += if corporate { units } else { part(units, 40, 100) };
                }
                Some("participation-cost") if !corporate => tax.participation += part(units, 40, 100),
                _ => {}
            }
            let expense = -units;
            match attribute(account, "trade_tax").as_deref() {
                Some("interest") => *trade.financing.entry('a').or_default() += expense,
                Some("annuity") => *trade.financing.entry('b').or_default() += expense,
                Some("silent-partner") => *trade.financing.entry('c').or_default() += expense,
                Some("rent-movable") => *trade.financing.entry('d').or_default() += part(expense, 1, 5),
                Some("rent-movable-half") => *trade.financing.entry('d').or_default() += part(expense, 1, 10),
                Some("rent-immovable") => *trade.financing.entry('e').or_default() += part(expense, 1, 2),
                Some("licence") => *trade.financing.entry('f').or_default() += part(expense, 1, 4),
                Some("partnership-loss") => trade.partnership_losses += expense,
                Some("partnership-share") => trade.partnership_profits += units,
                Some("land-tax") => land_tax += expense,
                _ => {}
            }
        }
        // The income: the result without what stays out; donations of a corporation up to their limit.
        let before_donations = tax.result - tax.excluded - tax.participation - tax.partner_remuneration + tax.donations;
        let donation_limit = |base: i128| {
            let by_income = part(base.max(0), 20, 100);
            let by_turnover = facts.turnover_and_wages.map_or(0, |sum| part(sum, 4, 1000));
            by_income.max(by_turnover)
        };
        if corporate {
            tax.donations_deducted = tax.donations.min(donation_limit(before_donations)).max(0);
        }
        tax.income = before_donations - tax.donations_deducted;
        tax.loss_deduction = usable_losses(tax.income, facts.income_losses, 70);
        tax.taxable = tax.income - tax.loss_deduction;
        if corporate && tax.taxable > 0 {
            // Fractions of a cent stay out (§ 4 Satz 3 SolZG; OPEN for corporation tax).
            tax.corporation_tax = part(tax.taxable, corporation_tax_rate(to.year()), 1000);
            tax.solidarity = part(tax.corporation_tax, 55, 1000);
        }
        if let Some(multiplier) = facts.multiplier {
            trade.profit = tax.income;
            trade.multiplier = multiplier;
            let financing: i128 = trade.financing.values().sum();
            trade.financing_addition = part((financing - 200_000 * EURO).max(0), 1, 4);
            trade.dividends = dividends_exempt;
            trade.donations_added = tax.donations_deducted;
            // § 9 Nr. 1 Satz 1 GewStG from 2025: the land tax booked as an expense; with what is stated beside it.
            trade.land = land_tax.max(0) + facts.land_reduction;
            let with_additions = trade.profit
                + trade.financing_addition
                + trade.dividends
                + trade.partnership_losses
                + trade.donations_added;
            trade.donations = tax.donations.min(donation_limit(with_additions)).max(0);
            trade.trade_income = with_additions - trade.land - trade.partnership_profits - trade.donations;
            trade.loss_deduction = usable_losses(trade.trade_income, facts.trade_losses, 60);
            let after_losses = trade.trade_income - trade.loss_deduction;
            trade.rounded = if after_losses > 0 { after_losses / (100 * EURO) * (100 * EURO) } else { 0 };
            trade.allowance = match facts.subject {
                TaxSubject::Individual | TaxSubject::Partnership => 24_500 * EURO,
                TaxSubject::Association => 5_000 * EURO,
                TaxSubject::Corporation => 0,
            }
            .min(trade.rounded);
            trade.base = part(trade.rounded - trade.allowance, 35, 1000);
            trade.tax = part(trade.base, i128::from(multiplier), 100);
            tax.trade = Some(trade);
        }
        Ok(tax)
    }

    /// An attribute of an account, own or inherited: of the chart, and of accounts opened in the books.
    fn account_attributes<'a>(&'a self, chart: &'a Chart) -> impl Fn(&AccountId, &str) -> Option<String> + 'a {
        let parents = self.account_parents(chart);
        let mut own: BTreeMap<AccountId, BTreeMap<String, String>> =
            chart.accounts.iter().map(|a| (a.id.clone(), a.attributes.clone())).collect();
        for (_, event) in self.ordered() {
            if let Payload::AccountOpened { account } = &event.payload {
                own.entry(account.id.clone()).or_insert_with(|| account.attributes.clone());
            }
        }
        move |account: &AccountId, key: &str| {
            let mut next = Some(account);
            for _ in 0..=parents.len() {
                let id = next?;
                if let Some(value) = own.get(id).and_then(|attributes| attributes.get(key)) {
                    return Some(value.clone());
                }
                next = parents.get(id);
            }
            None
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Booking, ChartTranslation, Clock, Event, Money, Posting, party::tests::party};

    /// Books of owner 1 in SKR 03: each booking `number`, units (debit positive), against the bank.
    pub(crate) fn books(lines: &[(&str, i128)]) -> EventSet {
        let skr03 = ChartTranslation::de_skr03();
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        for (ms, (number, units)) in lines.iter().enumerate() {
            let posting = |number: &str, units: i128| Posting {
                owner: party(1),
                account: skr03.accounts[number].clone(),
                amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
                note: None,
                tax: None,
                exchange: None,
            };
            let booking = Booking {
                basis: None,
                attributed: None,
                date: "2026-06-30".parse().unwrap(),
                text: "test".into(),
                postings: vec![posting(number, *units), posting("1200", -*units)],
                documents: vec![],
                entries: vec![],
                transaction: None,
            };
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event { time: clock.now(ms as u64), seq, prev, payload: Payload::BookingRecorded { booking } });
        }
        set
    }

    pub(crate) fn facts(subject: TaxSubject) -> ProfitTaxFacts {
        ProfitTaxFacts {
            subject,
            multiplier: Some(400),
            income_losses: 0,
            trade_losses: 0,
            land_reduction: 0,
            turnover_and_wages: None,
            holding: Holding::AtLeast15,
            cash_basis: false,
        }
    }

    const E: i128 = 100;

    /// A GmbH: sales of 1.5 million, interest of 120,000, rent of land of 300,000, licences of 40,000, the leasing
    /// of cars of 100,000; business entertainment not deductible of 10,000, dividends of 50,000 from a holding of
    /// 20 %, donations of 30,000; corporation tax and trade tax booked as provisions of 100,000.
    #[test]
    fn a_corporation() {
        let set = books(&[
            ("8400", -1_500_000 * E),
            ("2120", 120_000 * E),
            ("4210", 300_000 * E),
            ("4964", 40_000 * E),
            ("4570", 100_000 * E),
            ("4654", 10_000 * E),
            ("2615", -50_000 * E),
            ("2383", 30_000 * E),
            ("2200", 60_000 * E),
            ("4320", 40_000 * E),
        ]);
        let tax = set
            .profit_tax(
                &Chart::v0_1(),
                &party(1),
                "2026-01-01".parse().unwrap(),
                "2026-12-31".parse().unwrap(),
                &facts(TaxSubject::Corporation),
            )
            .unwrap();
        assert_eq!(tax.result, 850_000 * E);
        assert_eq!(tax.excluded, -110_000 * E, "entertainment, corporation tax, trade tax");
        assert_eq!(tax.participation, 47_500 * E, "95 % of the dividends");
        assert_eq!(tax.donations, 30_000 * E);
        // 850,000 + 110,000 − 47,500 + 30,000 = 942,500; donations up to 20 % of it.
        assert_eq!(tax.donations_deducted, 30_000 * E);
        assert_eq!(tax.income, 912_500 * E);
        assert_eq!(tax.corporation_tax, 136_875 * E, "15 %");
        assert_eq!(tax.solidarity, 752_812, "5.5 %, fractions of a cent left out");
        let trade = tax.trade.unwrap();
        assert_eq!(trade.financing[&'a'], 120_000 * E);
        assert_eq!(trade.financing[&'e'], 150_000 * E);
        assert_eq!(trade.financing[&'f'], 10_000 * E);
        assert_eq!(trade.financing[&'d'], 20_000 * E);
        // (300,000 − 200,000) / 4.
        assert_eq!(trade.financing_addition, 25_000 * E);
        assert_eq!(trade.dividends, 0, "a holding of at least 15 %");
        // 912,500 + 25,000 + 30,000 (Nr. 9) − 30,000 (Nr. 5).
        assert_eq!(trade.trade_income, 937_500 * E);
        assert_eq!((trade.rounded, trade.allowance), (937_500 * E, 0));
        assert_eq!(trade.base, 32_812 * E + 50);
        assert_eq!(trade.tax, 131_250 * E);
    }

    /// A sole proprietor: profit of 80,000, rent of land of 500,000 – the allowance of 24,500 and the rounding to 100.
    #[test]
    fn a_sole_proprietor_with_losses() {
        let set = books(&[("8400", -580_050 * E), ("4210", 500_000 * E), ("2383", 1_000 * E), ("4320", 5_000 * E)]);
        let mut facts = facts(TaxSubject::Individual);
        facts.trade_losses = 10_000 * E;
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let tax = set.profit_tax(&Chart::v0_1(), &party(1), from, to, &facts).unwrap();
        // Trade tax and donations stay out of the profit of a person; no corporation tax.
        assert_eq!(tax.income, 74_050 * E + 5_000 * E + 1_000 * E);
        assert_eq!((tax.corporation_tax, tax.donations_deducted), (0, 0));
        let trade = tax.trade.unwrap();
        // (250,000 − 200,000) / 4 = 12,500; donations deducted under § 9 Nr. 5.
        assert_eq!(trade.financing_addition, 12_500 * E);
        assert_eq!(trade.trade_income, 80_050 * E + 12_500 * E - 1_000 * E);
        assert_eq!(trade.loss_deduction, 10_000 * E);
        assert_eq!(trade.rounded, 81_500 * E);
        assert_eq!(trade.allowance, 24_500 * E);
        assert_eq!(trade.base, 1_995 * E);
        assert_eq!(trade.tax, 7_980 * E);
        // A freelancer pays no trade tax; a multiplier below 200 % is refused.
        facts.multiplier = None;
        assert!(set.profit_tax(&Chart::v0_1(), &party(1), from, to, &facts).unwrap().trade.is_none());
        facts.multiplier = Some(150);
        assert_eq!(set.profit_tax(&Chart::v0_1(), &party(1), from, to, &facts), Err(ProfitTaxError::Multiplier(150)));
    }

    #[test]
    fn losses_and_rates() {
        let million = 1_000_000 * E;
        assert_eq!(usable_losses(3 * million, 10 * million, 70), million + 2 * million * 7 / 10);
        assert_eq!(usable_losses(3 * million, 10 * million, 60), million + 2 * million * 6 / 10);
        assert_eq!(usable_losses(500_000 * E, 100 * E, 70), 100 * E);
        assert_eq!(usable_losses(-5, 100, 70), 0);
        assert_eq!([2026, 2027, 2028, 2031, 2032, 2040].map(corporation_tax_rate), [150, 150, 140, 110, 100, 100]);
    }

    /// Land tax booked on SKR 03 2375 reduces the trade income from 2025 (§ 9 Nr. 1 Satz 1 GewStG).
    #[test]
    fn land_tax_reduces_the_trade_income() {
        let set = books(&[("8400", -100_000 * E), ("2375", 2_000 * E)]);
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let tax = set.profit_tax(&Chart::v0_1(), &party(1), from, to, &facts(TaxSubject::Corporation)).unwrap();
        let trade = tax.trade.unwrap();
        assert_eq!(trade.land, 2_000 * E);
        assert_eq!(trade.trade_income, tax.income - 2_000 * E);
    }

    /// A trade fills the Anlage G with its profit and trade tax, a freelancer the Anlage S.
    #[test]
    fn the_lines_of_the_income_tax_forms() {
        let set = books(&[("8400", -100_000 * E)]);
        let (from, to) = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let tax = set.profit_tax(&Chart::v0_1(), &party(1), from, to, &facts(TaxSubject::Individual)).unwrap();
        let (form, lines) = tax.income_tax_lines();
        assert_eq!(form, "de-anlage-g");
        let trade = tax.trade.clone().unwrap();
        assert_eq!((lines["10"], lines["64"], lines["66"]), (100_000 * E, trade.base, trade.tax));
        let freelancer = ProfitTaxFacts { multiplier: None, ..facts(TaxSubject::Individual) };
        let tax = set.profit_tax(&Chart::v0_1(), &party(1), from, to, &freelancer).unwrap();
        assert_eq!(tax.income_tax_lines(), ("de-anlage-s", [("100".to_owned(), 100_000 * E)].into()));
    }
}
