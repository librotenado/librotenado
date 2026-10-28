// SPDX-License-Identifier: MIT
//! Report mappings (spec part 0009): how the harmonised accounts and the tax codes of a set of books fall into the
//! positions of a national report – a balance sheet, a VAT return – in one version of that report.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, AccountKind, BalanceError, Chart, Country, Date, EventSet, Object, PartyId, Payload, TaxCode,
    TaxDirection, TaxTable,
};

/// Whether a mapping has been reviewed by tax advisers of its country.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    /// Not yet reviewed.
    Draft,
    /// Reviewed.
    Reviewed,
}

/// The accounts that hold VAT, for a report computed from tax codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatAccounts {
    /// Accounts of VAT owed: collected, and owed by the recipient (reverse charge). Their refinements count, too.
    pub output: Vec<AccountId>,
    /// Accounts of deductible VAT. Their refinements count, too.
    pub input: Vec<AccountId>,
}

/// The fields for settling VAT with the tax office: postings on VAT accounts without a tax code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatSettlement {
    /// The field for a debit: VAT paid to the tax office.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub debit: Option<String>,
    /// The field for a credit: VAT refunded by the tax office.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub credit: Option<String>,
}

/// Which fields of a report a tax code fills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxRule {
    /// The tax code.
    pub tax: TaxCode,
    /// The field for the net amounts: postings with the code on accounts that hold no VAT.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub base: Option<String>,
    /// The field for VAT owed: postings with the code on output VAT accounts.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub output: Option<String>,
    /// The field for deductible VAT: postings with the code on input VAT accounts.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub input: Option<String>,
    /// Where the rule applies only to postings on accounts of one position: that position. Its positions below –
    /// written with it and a dot before them – count, too. E.g. sales by tax code within the net sales.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub within: Option<String>,
}

impl TaxRule {
    /// Whether the rule applies to a posting on an account of this position.
    fn applies(&self, position: Option<&str>) -> bool {
        match (&self.within, position) {
            (None, _) => true,
            (Some(within), Some(position)) => {
                position == within || position.strip_prefix(within.as_str()).is_some_and(|rest| rest.starts_with('.'))
            }
            (Some(_), None) => false,
        }
    }
}

/// How the harmonised chart and the tax codes fall into one version of a national report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportMapping {
    /// The report, e.g. `de-ustva`, `de-hgb-266`.
    pub report: String,
    /// The country.
    pub country: Country,
    /// The version of the report, e.g. a year or a taxonomy version.
    pub version: String,
    /// The first day of the periods it applies to.
    pub valid_from: Date,
    /// The harmonised chart it maps, as `name/version`.
    pub into: String,
    /// Whether tax advisers have reviewed it.
    pub status: MappingStatus,
    /// Account → position. A mapping holds for the account and every account that refines it, unless a refinement
    /// has its own; left out when the report is computed from tax codes alone.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "crate::non_empty_map")]
    pub accounts: BTreeMap<AccountId, String>,
    /// Account → position for a credit balance, where it differs (a bank account in overdraft).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "crate::non_empty_map")]
    pub credit_accounts: BTreeMap<AccountId, String>,
    /// The accounts that hold VAT; with `taxes`.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub vat_accounts: Option<VatAccounts>,
    /// The fields for postings on VAT accounts without a tax code: payments to and refunds from the tax office.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub vat_settlement: Option<VatSettlement>,
    /// The fields each tax code fills.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub taxes: Vec<TaxRule>,
    /// The positions a report must state even without an amount – as empty (NIL), not as zero –, e.g. the
    /// Mussfelder of the E-Bilanz.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub mandatory: Vec<String>,
}

impl Object for ReportMapping {
    const TYPE: &'static str = "report_mapping";
}

impl ReportMapping {
    /// The German VAT return of 2026 (Umsatzsteuer-Voranmeldung), from tax codes (`spec/data/reports/`).
    pub fn de_ustva_2026() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-ustva-2026.json"))
            .expect("the published mapping is valid")
    }

    /// The German balance sheet of 2026 (HGB § 266).
    pub fn de_hgb_266_2026() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-hgb-266-2026.json"))
            .expect("the published mapping is valid")
    }

    /// The German profit and loss account of 2026, by nature of expense (HGB § 275 Abs. 2).
    pub fn de_hgb_275_gkv_2026() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-hgb-275-gkv-2026.json"))
            .expect("the published mapping is valid")
    }

    /// The balance sheet of the German E-Bilanz (§ 5b EStG) in the taxonomy 6.10: positions of `de-gaap-ci`.
    pub fn de_ebilanz_bs_6_10() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-ebilanz-bs-6.10.json"))
            .expect("the published mapping is valid")
    }

    /// The income statement by nature of expense of the German E-Bilanz (§ 5b EStG) in the taxonomy 6.10.
    pub fn de_ebilanz_is_gkv_6_10() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-ebilanz-is-gkv-6.10.json"))
            .expect("the published mapping is valid")
    }

    /// The German income statement on a cash basis of 2026 (Anlage EÜR, § 4 Abs. 3 EStG).
    pub fn de_euer_2026() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-euer-2026.json"))
            .expect("the published mapping is valid")
    }

    /// The German income statement on a cash basis of 2025 (Anlage EÜR, § 4 Abs. 3 EStG).
    pub fn de_euer_2025() -> Self {
        crate::from_root(include_bytes!("../../../spec/data/reports/de-euer-2025.json"))
            .expect("the published mapping is valid")
    }

    /// The position of an account: its own mapping or the nearest of the accounts it refines; for a credit balance
    /// the credit mapping, where there is one.
    pub fn position(
        &self,
        account: &AccountId,
        parents: &BTreeMap<AccountId, AccountId>,
        credit: bool,
    ) -> Option<&str> {
        if credit && let Some(position) = nearest(&self.credit_accounts, account, parents) {
            return Some(position);
        }
        nearest(&self.accounts, account, parents)
    }
}

/// The mapping of an account or of the nearest account it refines.
fn nearest<'m>(
    map: &'m BTreeMap<AccountId, String>,
    account: &AccountId,
    parents: &BTreeMap<AccountId, AccountId>,
) -> Option<&'m str> {
    ancestry(account, parents).into_iter().find_map(|id| map.get(id)).map(String::as_str)
}

/// An account and the accounts it refines, nearest first.
fn ancestry<'a>(account: &'a AccountId, parents: &'a BTreeMap<AccountId, AccountId>) -> Vec<&'a AccountId> {
    let mut chain = vec![account];
    while let Some(parent) = parents.get(chain[chain.len() - 1]) {
        if chain.len() > parents.len() {
            break; // a cycle; the review of accounts reports it
        }
        chain.push(parent);
    }
    chain
}

impl EventSet {
    /// The fields of a report over a period – a VAT return, a profit and loss account, an income statement on a
    /// cash basis – for `owner`, from the bookings dated from `from` to `to` inclusive (spec part 0009). Per posting:
    ///
    /// - with a tax code that has a rule: on VAT accounts its `output` or `input` field, elsewhere its `base` field;
    /// - without a tax code on a VAT account: the `vat_settlement` field of its side – a payment to or a refund from
    ///   the tax office;
    /// - otherwise, and where the rule has no `base`: the position of its account.
    ///
    /// Amounts are in base units of the functional currency – converted where a posting has an exchange – and positive as the report expects them: sales and VAT owed as credit, purchases
    /// and deductible VAT as debit; positions of accounts on the normal side of their kind, or – where the mapping
    /// distinguishes the sides – on the side mapped.
    pub fn period_report(
        &self,
        mapping: &ReportMapping,
        taxes: &TaxTable,
        chart: &Chart,
        owner: &PartyId,
        from: Date,
        to: Date,
    ) -> Result<BTreeMap<String, i128>, BalanceError> {
        let parents = self.account_parents(chart);
        let kinds = self.account_kinds(chart);
        let none = VatAccounts { output: vec![], input: vec![] };
        let vat = mapping.vat_accounts.as_ref().unwrap_or(&none);
        let within = |account: &AccountId, roots: &[AccountId]| {
            ancestry(account, &parents).into_iter().any(|id| roots.contains(id))
        };
        let mut fields: BTreeMap<String, i128> = BTreeMap::new();
        let mut add = |field: &str, amount: i128| -> Result<(), BalanceError> {
            let sum = fields.entry(field.to_owned()).or_default();
            *sum = sum.checked_add(amount).ok_or(BalanceError::Overflow)?;
            Ok(())
        };
        for (_, event) in self.ordered() {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            if booking.date < from || booking.date > to {
                continue;
            }
            for posting in booking.postings.iter().filter(|p| p.owner == *owner) {
                let units = posting.functional().base_units();
                let on_output = within(&posting.account, &vat.output);
                let on_input = within(&posting.account, &vat.input);
                let credit = units < 0;
                let position = mapping.position(&posting.account, &parents, credit);
                let rule = posting
                    .tax
                    .as_ref()
                    .and_then(|code| mapping.taxes.iter().find(|rule| rule.tax == *code && rule.applies(position)));
                if let Some(rule) = rule {
                    let sales = taxes.get(&rule.tax).is_some_and(|d| d.direction == TaxDirection::Output);
                    let (field, amount) = if on_output {
                        (rule.output.as_ref(), -units)
                    } else if on_input {
                        (rule.input.as_ref(), units)
                    } else {
                        (rule.base.as_ref(), if sales { -units } else { units })
                    };
                    if let Some(field) = field {
                        add(field, amount)?;
                    }
                    if on_output || on_input || rule.base.is_some() {
                        continue;
                    }
                } else if on_output || on_input {
                    if let Some(settlement) = &mapping.vat_settlement {
                        let field = if units < 0 { &settlement.credit } else { &settlement.debit };
                        if let Some(field) = field {
                            add(field, units.abs())?;
                        }
                    }
                    continue;
                }
                let sided =
                    ancestry(&posting.account, &parents).into_iter().any(|id| mapping.credit_accounts.contains_key(id));
                if let Some(position) = position {
                    let debit_positive = if sided {
                        !credit
                    } else {
                        kinds.get(&posting.account).is_none_or(|kind| kind.debit_normal() || *kind == AccountKind::Memo)
                    };
                    add(position, if debit_positive { units } else { -units })?;
                }
            }
        }
        Ok(fields)
    }

    /// The kind of every account: of `chart`, and opened in the books.
    pub fn account_kinds(&self, chart: &Chart) -> BTreeMap<AccountId, AccountKind> {
        let mut kinds: BTreeMap<AccountId, AccountKind> =
            chart.accounts.iter().map(|account| (account.id.clone(), account.kind)).collect();
        for (_, event) in self.ordered() {
            if let Payload::AccountOpened { account } = &event.payload {
                kinds.entry(account.id.clone()).or_insert(account.kind);
            }
        }
        kinds
    }

    /// The positions of a report computed from accounts – a balance sheet, a profit and loss account – for `owner`
    /// on `until`: per position, the sum of the balances of the accounts that fall into it, in base units of the
    /// owner's functional currency, debit positive. Accounts without a position are left out.
    pub fn account_report(
        &self,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        until: Option<Date>,
    ) -> Result<BTreeMap<String, i128>, BalanceError> {
        let parents = self.account_parents(chart);
        let mut positions: BTreeMap<String, i128> = BTreeMap::new();
        for (party, account, _, units) in self.functional_balances(until)?.iter() {
            if party != owner {
                continue;
            }
            if let Some(position) = mapping.position(account, &parents, units < 0) {
                let sum = positions.entry(position.to_owned()).or_default();
                *sum = sum.checked_add(units).ok_or(BalanceError::Overflow)?;
            }
        }
        Ok(positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Booking, ChartTranslation, Clock, Event, Money, Posting, party::tests::party};

    /// A posting of `units` cents for owner 1 on the PCG account `number`, with an optional tax code.
    fn posting(number: &str, units: i128, tax: Option<&str>) -> Posting {
        Posting {
            owner: party(1),
            account: ChartTranslation::fr_pcg().accounts[number].clone(),
            amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
            note: None,
            tax: tax.map(|t| t.parse().unwrap()),
            exchange: None,
        }
    }

    fn books(bookings: Vec<Vec<Posting>>) -> EventSet {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        for (ms, postings) in bookings.into_iter().enumerate() {
            let booking = Booking {
                date: "2026-03-15".parse().unwrap(),
                text: "test".into(),
                postings,
                documents: vec![],
                transaction: None,
            };
            booking.validate().unwrap();
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event { time: clock.now(ms as u64), seq, prev, payload: Payload::BookingRecorded { booking } });
        }
        set
    }

    #[test]
    fn a_vat_return_from_tax_codes() {
        let set = books(vec![
            // A sale: 1,000 + 190 VAT.
            vec![
                posting("411", 119_000, None),
                posting("706", -100_000, Some("de-out-19")),
                posting("44571", -19_000, Some("de-out-19")),
            ],
            // A purchase: 200 + 38 VAT.
            vec![
                posting("6064", 20_000, Some("de-in-19")),
                posting("44566", 3_800, Some("de-in-19")),
                posting("401", -23_800, None),
            ],
            // A service from another EU country, reverse charge: 500, VAT owed and deducted.
            vec![
                posting("6226", 50_000, Some("de-in-13b-eu-service")),
                posting("4452", -9_500, Some("de-in-13b-eu-service")),
                posting("44566", 9_500, Some("de-in-13b-eu-service")),
                posting("401", -50_000, None),
            ],
        ]);
        let report = set
            .period_report(
                &ReportMapping::de_ustva_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                "2026-03-01".parse().unwrap(),
                "2026-03-31".parse().unwrap(),
            )
            .unwrap();
        let field = |kz: &str| report.get(kz).copied().unwrap_or(0);
        assert_eq!(field("81"), 100_000, "net sales at 19 %");
        assert_eq!(field("66"), 3_800, "deductible VAT from invoices");
        assert_eq!((field("46"), field("47"), field("67")), (50_000, 9_500, 9_500), "reverse charge");
        assert_eq!(report.len(), 5, "81, 66, 46, 47, 67");
    }

    #[test]
    fn every_account_of_the_chart_has_a_german_position() {
        use crate::AccountKind::*;
        let chart = Chart::v0_1();
        let parents = EventSet::new().account_parents(&chart);
        let (bs, gkv) = (ReportMapping::de_hgb_266_2026(), ReportMapping::de_hgb_275_gkv_2026());
        let refined: std::collections::BTreeSet<&AccountId> = parents.values().collect();
        for account in chart.accounts.iter().filter(|a| !refined.contains(&a.id)) {
            let mapping = match account.kind {
                Asset | Liability | Equity => &bs,
                Income | Expense => &gkv,
                Memo => continue,
            };
            assert!(mapping.position(&account.id, &parents, false).is_some(), "{} has no position", account.id);
        }
        let pcg = ChartTranslation::fr_pcg();
        let position =
            |m: &ReportMapping, n: &str, credit| m.position(&pcg.accounts[n], &parents, credit).map(str::to_owned);
        assert_eq!(position(&bs, "5121", false), Some("aktiv.B.IV".into()));
        assert_eq!(position(&bs, "5121", true), Some("passiv.C.2".into()));
        assert_eq!(position(&bs, "44566", false), Some("aktiv.B.II.4".into()));
        assert_eq!(position(&gkv, "6411", false), Some("gkv.6.a".into()));
        assert_eq!(position(&gkv, "706", false), Some("gkv.1".into()));
    }

    /// The E-Bilanz takes every account the HGB places, a level deeper where the taxonomy requires it.
    #[test]
    fn the_e_bilanz_refines_the_hgb() {
        let chart = Chart::v0_1();
        let parents = EventSet::new().account_parents(&chart);
        for (hgb, ebilanz, prefix) in [
            (ReportMapping::de_hgb_266_2026(), ReportMapping::de_ebilanz_bs_6_10(), "bs."),
            (ReportMapping::de_hgb_275_gkv_2026(), ReportMapping::de_ebilanz_is_gkv_6_10(), "is."),
        ] {
            assert!(!ebilanz.mandatory.is_empty());
            for (account, position) in hgb.accounts.iter().chain(&hgb.credit_accounts) {
                if position.contains(".extra.") {
                    continue; // French positions a German set of books does not use
                }
                for credit in [false, true] {
                    let position = ebilanz.position(account, &parents, credit).unwrap_or_else(|| panic!("{account}"));
                    assert!(position.starts_with(prefix), "{account}: {position}");
                }
            }
        }
        let pcg = ChartTranslation::fr_pcg();
        let bs = ReportMapping::de_ebilanz_bs_6_10();
        let position = |n: &str, credit| bs.position(&pcg.accounts[n], &parents, credit).map(str::to_owned);
        assert_eq!(position("53", false), Some("bs.ass.currAss.cashEquiv.cash".into()));
        assert_eq!(position("5121", false), Some("bs.ass.currAss.cashEquiv.bank".into()));
        assert_eq!(position("5121", true), Some("bs.eqLiab.liab.bank.misc".into()));
    }

    /// Sales fall into the net sales by their VAT, as the E-Bilanz requires.
    #[test]
    fn e_bilanz_sales_by_tax_code() {
        let set = books(vec![
            vec![
                posting("5121", 119_000, None),
                posting("706", -100_000, Some("de-out-19")),
                posting("44571", -19_000, Some("de-out-19")),
            ],
            vec![
                posting("5121", 10_700, None),
                posting("706", -10_000, Some("de-out-7")),
                posting("44571", -700, None),
            ],
            vec![posting("5121", 5_000, None), posting("706", -5_000, None)],
        ]);
        let mapping = ReportMapping::de_ebilanz_is_gkv_6_10();
        let day: Date = "2026-03-15".parse().unwrap();
        let report = set.period_report(&mapping, &TaxTable::de(), &Chart::v0_1(), &party(1), day, day).unwrap();
        let sales = "is.netIncome.regular.operatingTC.grossTradingProfit.totalOutput.netSales.grossSales";
        let field = |name: &str| report.get(&format!("{sales}.{name}")).copied();
        assert_eq!(field("generalRateVAT"), Some(100_000));
        assert_eq!(field("reducedRateVAT"), Some(10_000));
        assert_eq!(field("unknownVAT"), Some(5_000), "without a tax code: the catch-all position");
        assert_eq!(report.len(), 3, "VAT accounts are not in the income statement: {report:?}");
    }

    #[test]
    fn an_income_statement_on_a_cash_basis() {
        let names = crate::Names::v0_1("en").unwrap();
        let grown =
            |name: &str| names.names.iter().find(|(_, n)| n.as_str() == name).map(|(id, _)| id.clone()).unwrap();
        let on = |id: String, units: i128, tax: Option<&str>| Posting {
            account: id.parse().unwrap(),
            ..posting("512", units, tax)
        };
        let set = books(vec![
            // A sale at 19 %, received in the bank.
            vec![
                posting("5121", 119_000, None),
                posting("706", -100_000, Some("de-out-19")),
                posting("44571", -19_000, Some("de-out-19")),
            ],
            // A phone bill, paid.
            vec![
                on(grown("Telecommunications"), 5_000, Some("de-in-19")),
                posting("44566", 950, Some("de-in-19")),
                posting("5121", -5_950, None),
            ],
            // The VAT paid to the tax office, and a withdrawal by the proprietor.
            vec![posting("44551", 18_050, None), posting("5121", -18_050, None)],
            vec![posting("108", 30_000, None), posting("5121", -30_000, None)],
        ]);
        let report = set
            .period_report(
                &ReportMapping::de_euer_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                "2026-01-01".parse().unwrap(),
                "2026-12-31".parse().unwrap(),
            )
            .unwrap();
        let field = |f: &str| report.get(f).copied().unwrap_or(0);
        assert_eq!((field("1.112"), field("1.140")), (100_000, 19_000), "sales and VAT received");
        assert_eq!((field("2.280"), field("2.185")), (5_000, 950), "telecommunications and input VAT paid");
        assert_eq!(field("2.186"), 18_050, "VAT paid to the tax office");
        assert_eq!(field("5.122"), 30_000, "withdrawal");
        assert_eq!(report.len(), 6, "the bank account is no line of the EÜR: {report:?}");
    }

    #[test]
    fn every_expense_has_a_line_of_the_euer() {
        let chart = Chart::v0_1();
        let parents = EventSet::new().account_parents(&chart);
        let refined: std::collections::BTreeSet<&AccountId> = parents.values().collect();
        for euer in [ReportMapping::de_euer_2025(), ReportMapping::de_euer_2026()] {
            let expenses = chart.accounts.iter().filter(|a| a.kind == AccountKind::Expense && !refined.contains(&a.id));
            for account in expenses {
                assert!(euer.position(&account.id, &parents, false).is_some(), "{} has no line", account.id);
            }
        }
        // The form of 2026 has the Kennzahlen of 2025.
        let (old, new) = (ReportMapping::de_euer_2025(), ReportMapping::de_euer_2026());
        assert_eq!((new.version.as_str(), new.valid_from), ("2026", "2026-01-01".parse().unwrap()));
        assert_eq!((&old.accounts, &old.taxes, &old.vat_settlement), (&new.accounts, &new.taxes, &new.vat_settlement));
    }

    #[test]
    fn positions_follow_the_nearest_mapping_and_the_credit_side() {
        let pcg = ChartTranslation::fr_pcg();
        let mut accounts = BTreeMap::new();
        accounts.insert(pcg.accounts["51"].clone(), "aktiv.B.IV".to_owned());
        accounts.insert(pcg.accounts["41"].clone(), "aktiv.B.II.1".to_owned());
        let mut credit_accounts = BTreeMap::new();
        credit_accounts.insert(pcg.accounts["512"].clone(), "passiv.C.2".to_owned());
        let mapping = ReportMapping {
            report: "test".into(),
            country: "DE".parse().unwrap(),
            version: "1".into(),
            valid_from: "2026-01-01".parse().unwrap(),
            into: "librotenado/0.1".into(),
            status: MappingStatus::Draft,
            accounts,
            credit_accounts,
            vat_accounts: None,
            vat_settlement: None,
            taxes: vec![],
            mandatory: vec![],
        };
        let set = books(vec![vec![posting("4111", 5_000, None), posting("5121", -5_000, None)]]);
        let report = set.account_report(&mapping, &Chart::v0_1(), &party(1), None).unwrap();
        assert_eq!(report.get("aktiv.B.II.1"), Some(&5_000), "4111 refines 41");
        assert_eq!(report.get("passiv.C.2"), Some(&-5_000), "an overdrawn bank account is a liability");
    }
}
