// SPDX-License-Identifier: MIT
//! Report mappings (spec part 0009): how the harmonised accounts and the tax codes of a set of books fall into the
//! positions of a national report – a balance sheet, a VAT return – in one version of that report.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, AccountKind, BalanceError, Basis, Chart, Country, Date, EventHash, EventSet, Object, PartyId, Payload,
    TaxCode, TaxDirection, TaxTable,
};

/// How far a German balance sheet is shortened (§ 266 Abs. 1 Satz 3 and 4 HGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HgbSize {
    /// A small company (§ 267 Abs. 1): only the items with letters and Roman numerals.
    Small,
    /// A micro company (§ 267a): only the items with letters.
    Micro,
}

/// A balance sheet after § 266 HGB, its positions written as their place in the law (`aktiv.A.II.1`), shortened for a
/// small or micro company: each amount counted on its nearest position of a letter and – for a small company – a
/// Roman numeral (`aktiv.A.II`, `passiv.C`). Positions outside the scheme of the law (`aktiv.extra.…`) stay as they are.
pub fn hgb_shortened(fields: &BTreeMap<String, i128>, size: HgbSize) -> BTreeMap<String, i128> {
    let roman = |part: &str| !part.is_empty() && part.chars().all(|c| matches!(c, 'I' | 'V' | 'X'));
    let mut shortened: BTreeMap<String, i128> = BTreeMap::new();
    for (position, units) in fields {
        let parts: Vec<&str> = position.split('.').collect();
        let side = parts.first().copied().unwrap_or_default();
        let lettered = parts.get(1).is_some_and(|p| p.len() == 1 && p.chars().all(|c| c.is_ascii_uppercase()));
        let key = if !matches!(side, "aktiv" | "passiv") || !lettered {
            position.clone()
        } else {
            let keep = match size {
                HgbSize::Micro => 2,
                HgbSize::Small if parts.get(2).is_some_and(|p| roman(p)) => 3,
                HgbSize::Small => 2,
            };
            parts[..keep.min(parts.len())].join(".")
        };
        *shortened.entry(key).or_default() += units;
    }
    shortened.retain(|_, units| *units != 0);
    shortened
}

/// The remaining terms of the amounts on a position (§ 268 Abs. 4 and 5 HGB), in base units, debit positive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Terms {
    /// On accounts with a remaining term of up to one year.
    pub up_to_one_year: i128,
    /// On accounts with a remaining term of more than one year – those of more than five years included.
    pub over_one_year: i128,
    /// Of those, on accounts with a remaining term of more than five years (§ 285 Nr. 1 Buchstabe a HGB).
    pub over_five_years: i128,
    /// On accounts without a remaining term.
    pub unknown: i128,
}

/// A booking a report per customer cannot place (spec part 0009): never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CustomerProblem {
    /// The documents of the booking name no VAT identification number of the customer.
    #[error("the booking of event {event} names no VAT identification number of its customer")]
    NoVatId {
        /// The event of the booking.
        event: EventHash,
    },
    /// The documents of the booking name several VAT identification numbers of customers.
    #[error("the booking of event {event} names several VAT identification numbers of customers")]
    SeveralVatIds {
        /// The event of the booking.
        event: EventHash,
    },
}

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
    /// Accounts the report leaves out – with every account that refines them, unless a refinement has a mapping of its
    /// own: imputed costs, or provisions in an income statement on a cash basis.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub excluded: Vec<AccountId>,
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
    /// Groups of legal forms, by the codes of `legal_form` (spec part 0008), whose further positions are in
    /// `mandatory_by_form`: the E-Bilanz's sole proprietors, partnerships and corporations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legal_forms: BTreeMap<String, Vec<String>>,
    /// Per group of legal forms, the positions its reports state as well as `mandatory`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mandatory_by_form: BTreeMap<String, Vec<String>>,
    /// The report counts receipts and payments in the year they count in (§ 11 EStG): on the day a booking is
    /// `attributed` to, where it has one, else on its date.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cash_basis: bool,
    /// Accounts whose position applies before the `base` of a tax rule – the proceeds of fixed assets sold, which the
    /// Anlage EÜR wants on a line of their own whatever their VAT. An account holds for every account that refines it.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub accounts_first: Vec<AccountId>,
    /// A report of the fixed assets (spec part 0011): account → group, for the assets depreciated on a straight
    /// line or not at all. An account holds for every account that refines it, unless a refinement has its own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub asset_groups: BTreeMap<AccountId, String>,
    /// Per group, its fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub asset_fields: BTreeMap<String, crate::AssetFields>,
    /// The fields of the pools of low-value assets (§ 6 Abs. 2a EStG): of the pool of the year of the report, then
    /// of each year before it; a pool older than the last has dissolved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pools: Vec<crate::AssetFields>,
    /// For a report of the payroll (spec part 0013): per field, the figures of the payslips it sums – a name, or a
    /// name, `@` and the religious community church tax goes to (`church@ev`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub payroll_fields: BTreeMap<String, Vec<String>>,
}

impl Object for ReportMapping {
    const TYPE: &'static str = "report_mapping";
}

impl ReportMapping {
    /// The positions a report of an owner of the legal form `legal_form` states even without an amount: `mandatory`,
    /// and those of the group of legal forms that holds it.
    pub fn mandatory_for(&self, legal_form: Option<&str>) -> Vec<&str> {
        let group = legal_form.and_then(|form| {
            self.legal_forms.iter().find(|(_, forms)| forms.iter().any(|f| f == form)).map(|(group, _)| group)
        });
        let by_form = group.and_then(|group| self.mandatory_by_form.get(group)).into_iter().flatten();
        let mut positions: Vec<&str> = self.mandatory.iter().chain(by_form).map(String::as_str).collect();
        positions.sort_unstable();
        positions.dedup();
        positions
    }

    /// The German VAT return of 2026 (Umsatzsteuer-Voranmeldung), from tax codes (`spec/data/reports/`).
    pub fn de_ustva_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-ustva-2026.json")).expect("the published mapping is valid")
    }

    /// The German balance sheet of 2026 (HGB § 266).
    pub fn de_hgb_266_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-hgb-266-2026.json"))
            .expect("the published mapping is valid")
    }

    /// The German profit and loss account of 2026, by nature of expense (HGB § 275 Abs. 2).
    pub fn de_hgb_275_gkv_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-hgb-275-gkv-2026.json"))
            .expect("the published mapping is valid")
    }

    /// The balance sheet of the German E-Bilanz (§ 5b EStG) in the taxonomy 6.10: positions of `de-gaap-ci`.
    pub fn de_ebilanz_bs_6_10() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-ebilanz-bs-6.10.json"))
            .expect("the published mapping is valid")
    }

    /// The income statement by nature of expense of the German E-Bilanz (§ 5b EStG) in the taxonomy 6.10.
    pub fn de_ebilanz_is_gkv_6_10() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-ebilanz-is-gkv-6.10.json"))
            .expect("the published mapping is valid")
    }

    /// The German register of fixed assets of 2026 for the Anlage EÜR (Anlage AVEÜR), from the assets
    /// (`EventSet::asset_report`, spec part 0011).
    pub fn de_aveuer_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-aveuer-2026.json")).expect("the published mapping is valid")
    }

    /// The German annual VAT return of 2026 (Umsatzsteuererklärung, § 18 Abs. 3 UStG), from tax codes.
    pub fn de_ust_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-ust-2026.json")).expect("the published mapping is valid")
    }

    /// The German recapitulative statement of 2026 (Zusammenfassende Meldung, § 18a UStG), from tax codes, per
    /// customer (`EventSet::customer_report`).
    pub fn de_zm_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-zm-2026.json")).expect("the published mapping is valid")
    }

    /// The German wage tax return of 2026 (Lohnsteuer-Anmeldung, § 41a EStG), from the payslips (spec part 0013).
    pub fn de_lsta_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-lsta-2026.json")).expect("the published mapping is valid")
    }

    /// The German income statement on a cash basis of 2026 (Anlage EÜR, § 4 Abs. 3 EStG).
    pub fn de_euer_2026() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-euer-2026.json")).expect("the published mapping is valid")
    }

    /// The German income statement on a cash basis of 2025 (Anlage EÜR, § 4 Abs. 3 EStG).
    pub fn de_euer_2025() -> Self {
        crate::from_root(include_bytes!("../data/reports/de-euer-2025.json")).expect("the published mapping is valid")
    }

    /// The position of an account: its own mapping or the nearest of the accounts it refines; for a credit balance
    /// the credit mapping, where there is one. None for an account the report leaves out, or that has no position.
    pub fn position(
        &self,
        account: &AccountId,
        parents: &BTreeMap<AccountId, AccountId>,
        credit: bool,
    ) -> Option<&str> {
        // An exclusion ends the search: mappings of the accounts above it do not reach below it.
        let mut chain = ancestry(account, parents);
        if let Some(excluded) = chain.iter().position(|id| self.excluded.contains(id)) {
            chain.truncate(excluded);
        }
        let nearest = |map: &'_ BTreeMap<AccountId, String>| chain.iter().any(|id| map.contains_key(*id));
        let map = if credit && nearest(&self.credit_accounts) { &self.credit_accounts } else { &self.accounts };
        chain.iter().find_map(|id| map.get(*id)).map(String::as_str)
    }
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
    /// The report of the fixed assets of `owner` from `from` to `to` (spec part 0011), in base units: per group of
    /// `mapping.asset_groups` the cost and book value of the assets held at the beginning, the additions, the
    /// depreciation, the book value of those that left and the book value at the end; per pool of a year the same.
    /// Low-value assets depreciated at once are not part of it. Fields without an amount are left out.
    pub fn asset_report(
        &self,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        from: Date,
        to: Date,
    ) -> BTreeMap<String, i128> {
        let parents = self.account_parents(chart);
        let group_of =
            |account: &AccountId| ancestry(account, &parents).into_iter().find_map(|id| mapping.asset_groups.get(id));
        let before = from.previous();
        let mut fields: BTreeMap<String, i128> = BTreeMap::new();
        let mut add = |field: &Option<String>, units: i128| {
            if let Some(field) = field
                && units != 0
            {
                *fields.entry(field.clone()).or_default() += units;
            }
        };
        for state in self.assets(owner) {
            let asset = &state.asset;
            if asset.acquired > to {
                continue;
            }
            let held = asset.acquired < from && state.disposal.as_ref().is_none_or(|d| d.date >= from);
            let opening = before.map_or(0, |day| state.book_value(day));
            let depreciation = state.depreciation(from, to);
            let cost = asset.cost.base_units();
            let fields_of = match asset.depreciation {
                crate::Depreciation::Immediate => continue,
                crate::Depreciation::Pool => {
                    let age = usize::from(from.year().saturating_sub(asset.acquired.year()));
                    match mapping.pools.get(age) {
                        Some(pool) => pool,
                        None => continue,
                    }
                }
                _ => match group_of(&asset.account).and_then(|g| mapping.asset_fields.get(g)) {
                    Some(group) => group,
                    None => continue,
                },
            };
            let pool = asset.depreciation == crate::Depreciation::Pool;
            if held || (pool && asset.acquired < from) {
                add(&fields_of.cost, cost);
                add(&fields_of.opening, opening);
            }
            if asset.acquired >= from {
                add(&fields_of.additions, cost);
            }
            add(&fields_of.depreciation, depreciation);
            let left = state.disposal.as_ref().filter(|d| d.date >= from && d.date <= to);
            if let (Some(_), false) = (left, pool) {
                add(&fields_of.disposals, state.value_at_disposal().unwrap_or(0));
            }
            add(&fields_of.closing, state.book_value(to));
        }
        fields.retain(|_, units| *units != 0);
        fields
    }

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
            let (booking, reversal) = match &event.payload {
                Payload::BookingRecorded { booking } => (booking, false),
                Payload::BookingReversed { booking, .. } => (booking, true),
                _ => continue,
            };
            let day = if mapping.cash_basis { booking.attributed.unwrap_or(booking.date) } else { booking.date };
            if day < from || day > to {
                continue;
            }
            for posting in booking.postings.iter().filter(|p| p.owner == *owner) {
                let units = posting.functional().base_units();
                let on_output = within(&posting.account, &vat.output);
                let on_input = within(&posting.account, &vat.input);
                // The side of the posting – for a reversal, the side of the posting it cancels, so that it cancels
                // in the same field.
                let credit = if reversal { units > 0 } else { units < 0 };
                let position = mapping.position(&posting.account, &parents, credit);
                let first = !on_output
                    && !on_input
                    && ancestry(&posting.account, &parents).into_iter().any(|id| mapping.accounts_first.contains(id));
                let rule = posting
                    .tax
                    .as_ref()
                    .filter(|_| !first)
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
                        let field = if credit { &settlement.credit } else { &settlement.debit };
                        if let Some(field) = field {
                            add(field, if credit { -units } else { units })?;
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

    /// A report per customer over a period – in Germany the recapitulative statement (Zusammenfassende Meldung,
    /// § 18a UStG) – for `owner`, from the bookings dated from `from` to `to` inclusive (spec part 0009). Every
    /// posting with a tax code whose rule has a `base` field, on an account that holds no VAT, counts on the field
    /// `<VAT identification number>/<base>` – e.g. `FR40303265045/S` –, positive as a sale; reversals and credit
    /// notes reduce it. The customer's VAT identification number is the buyer's on the invoices and credit notes the
    /// booking is based on (BT-48), or else the `vat` tax number of the organisation the buyer is.
    ///
    /// A booking whose documents name no such number, or several, is reported and left out.
    pub fn customer_report(
        &self,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        from: Date,
        to: Date,
    ) -> Result<(BTreeMap<String, i128>, Vec<CustomerProblem>), BalanceError> {
        let parents = self.account_parents(chart);
        let none = VatAccounts { output: vec![], input: vec![] };
        let vat = mapping.vat_accounts.as_ref().unwrap_or(&none);
        let on_vat = |account: &AccountId| {
            ancestry(account, &parents).into_iter().any(|id| vat.output.contains(id) || vat.input.contains(id))
        };
        let (organisations, _) = self.parties();
        let documents = self.documents();
        let vat_id_of = |buyer: &crate::InvoiceParty| -> Option<String> {
            buyer.vat_id.clone().or_else(|| {
                let organisation = organisations.get(buyer.party.as_ref()?)?;
                organisation.tax_ids.iter().find(|t| t.scheme == "vat").map(|t| t.number.clone())
            })
        };
        let mut fields: BTreeMap<String, i128> = BTreeMap::new();
        let mut problems = Vec::new();
        for (hash, event) in self.ordered() {
            let booking = match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => booking,
                _ => continue,
            };
            if booking.date < from || booking.date > to {
                continue;
            }
            let counted: Vec<(&str, i128)> = booking
                .postings
                .iter()
                .filter(|p| p.owner == *owner && !on_vat(&p.account))
                .filter_map(|p| {
                    let code = p.tax.as_ref()?;
                    let rule = mapping.taxes.iter().find(|rule| rule.tax == *code)?;
                    Some((rule.base.as_deref()?, -p.functional().base_units()))
                })
                .collect();
            if counted.is_empty() {
                continue;
            }
            let vat_ids: BTreeSet<String> = booking
                .documents
                .iter()
                .filter_map(|id| documents.get(id)?.invoice.as_ref())
                .filter_map(|invoice| vat_id_of(&invoice.buyer))
                .collect();
            let mut vat_ids = vat_ids.into_iter();
            let vat_id = match (vat_ids.next(), vat_ids.next()) {
                (Some(vat_id), None) => vat_id,
                (None, _) => {
                    problems.push(CustomerProblem::NoVatId { event: hash });
                    continue;
                }
                (Some(_), Some(_)) => {
                    problems.push(CustomerProblem::SeveralVatIds { event: hash });
                    continue;
                }
            };
            for (base, amount) in counted {
                let sum = fields.entry(format!("{vat_id}/{base}")).or_default();
                *sum = sum.checked_add(amount).ok_or(BalanceError::Overflow)?;
            }
        }
        fields.retain(|_, amount| *amount != 0);
        Ok((fields, problems))
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
    ///
    /// Income and expense accounts without a position of their own – on a balance sheet – show their result on the
    /// chart's result accounts (spec part 0004): what was booked in the financial year that holds `until` on the
    /// result for the year, what was booked before it on the retained earnings. Without `until`, the year is the
    /// one of the owner's latest booking.
    pub fn account_report(
        &self,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        until: Option<Date>,
    ) -> Result<BTreeMap<String, i128>, BalanceError> {
        self.account_report_in(Basis::Commercial, mapping, chart, owner, until)
    }

    /// The reconciliation of the commercial to the tax balance sheet (spec part 0009, Überleitungsrechnung): per
    /// position, what the bookings that count in only one of them change – the positions of the tax balance sheet
    /// less those of the commercial one; positions without a difference are left out.
    pub fn reconciliation(
        &self,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        until: Option<Date>,
    ) -> Result<BTreeMap<String, i128>, BalanceError> {
        let mut differences = self.account_report_in(Basis::Tax, mapping, chart, owner, until)?;
        for (position, units) in self.account_report_in(Basis::Commercial, mapping, chart, owner, until)? {
            let difference = differences.entry(position).or_default();
            *difference = difference.checked_sub(units).ok_or(BalanceError::Overflow)?;
        }
        differences.retain(|_, units| *units != 0);
        Ok(differences)
    }

    /// The report from the accounts of the balance sheet of `basis` (see [`EventSet::account_report`]).
    pub fn account_report_in(
        &self,
        basis: Basis,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        until: Option<Date>,
    ) -> Result<BTreeMap<String, i128>, BalanceError> {
        let parents = self.account_parents(chart);
        let kinds = self.account_kinds(chart);
        let is_result =
            |account: &AccountId| matches!(kinds.get(account), Some(AccountKind::Income | AccountKind::Expense));
        let mut positions: BTreeMap<String, i128> = BTreeMap::new();
        let mut add = |account: &AccountId, units: i128| -> Result<bool, BalanceError> {
            let Some(position) = mapping.position(account, &parents, units < 0) else { return Ok(false) };
            let sum = positions.entry(position.to_owned()).or_default();
            *sum = sum.checked_add(units).ok_or(BalanceError::Overflow)?;
            Ok(true)
        };
        let mut result: i128 = 0;
        for (party, account, _, units) in self.functional_balances_in(basis, until)?.iter() {
            if party != owner {
                continue;
            }
            if !add(account, units)? && is_result(account) {
                result = result.checked_add(units).ok_or(BalanceError::Overflow)?;
            }
        }
        let date = until.or_else(|| self.latest_booking(owner));
        if let (Some((current, earlier)), Some(date), true) = (chart.result_accounts(), date, result != 0) {
            let start = self.financial_year_start(owner, date);
            let mut before: i128 = 0;
            for (party, account, _, units) in self.functional_balances_before(basis, start)?.iter() {
                if party == owner && is_result(account) && mapping.position(account, &parents, units < 0).is_none() {
                    before = before.checked_add(units).ok_or(BalanceError::Overflow)?;
                }
            }
            let this_year = result.checked_sub(before).ok_or(BalanceError::Overflow)?;
            if before != 0 {
                add(&earlier, before)?;
            }
            if this_year != 0 {
                add(&current, this_year)?;
            }
        }
        Ok(positions)
    }

    /// The remaining terms of the amounts on each position of `mapping` on `until` (spec part 0009; § 268 Abs. 4
    /// and 5 HGB): by the attribute `term` of the account, own or inherited – `up-to-1-year`, `over-1-year`,
    /// `1-to-5-years`, `over-5-years` –, unknown without one. Amounts in the owner's functional currency, debit
    /// positive; positions without an amount are left out.
    pub fn term_report(
        &self,
        mapping: &ReportMapping,
        chart: &Chart,
        owner: &PartyId,
        until: Option<Date>,
    ) -> Result<BTreeMap<String, Terms>, BalanceError> {
        let parents = self.account_parents(chart);
        let mut own: BTreeMap<&AccountId, &str> = chart
            .accounts
            .iter()
            .filter_map(|account| Some((&account.id, account.attributes.get("term")?.as_str())))
            .collect();
        let ordered = self.ordered();
        for (_, event) in &ordered {
            if let Payload::AccountOpened { account } = &event.payload
                && let Some(term) = account.attributes.get("term")
            {
                own.entry(&account.id).or_insert(term.as_str());
            }
        }
        let term = |account: &AccountId| {
            let mut next = Some(account);
            for _ in 0..=parents.len() {
                let id = next?;
                if let Some(term) = own.get(id) {
                    return Some(*term);
                }
                next = parents.get(id);
            }
            None
        };
        let mut positions: BTreeMap<String, Terms> = BTreeMap::new();
        for (party, account, _, units) in self.functional_balances(until)?.iter() {
            if party != owner || units == 0 {
                continue;
            }
            let Some(position) = mapping.position(account, &parents, units < 0) else { continue };
            let terms = positions.entry(position.to_owned()).or_default();
            let add = |sum: &mut i128| -> Result<(), BalanceError> {
                *sum = sum.checked_add(units).ok_or(BalanceError::Overflow)?;
                Ok(())
            };
            match term(account) {
                Some("up-to-1-year") => add(&mut terms.up_to_one_year)?,
                Some("over-5-years") => {
                    add(&mut terms.over_one_year)?;
                    add(&mut terms.over_five_years)?;
                }
                Some("over-1-year" | "1-to-5-years") => add(&mut terms.over_one_year)?,
                _ => add(&mut terms.unknown)?,
            }
        }
        positions.retain(|_, terms| *terms != Terms::default());
        Ok(positions)
    }

    /// The first day of the financial year of `owner` that holds `date` (see [`EventSet::financial_year`]).
    pub fn financial_year_start(&self, owner: &PartyId, date: Date) -> Date {
        self.financial_year(owner, date).0
    }

    /// The financial year of `owner` that holds `date`, first and last day (spec part 0008): twelve months from the
    /// latest day `year_start`/`year_start_day` of the configuration that applies on `date` – the first of January
    /// without one –, shorter where the start changes: the year before a change ends the day before the configuration
    /// that changes it, the year after it begins on that configuration's `from`.
    pub fn financial_year(&self, owner: &PartyId, date: Date) -> (Date, Date) {
        // The configurations of the owner by `from`; of two with the same `from`, the later in the agreed order.
        let mut by_from: BTreeMap<Date, (u8, u8)> = BTreeMap::new();
        for (_, event) in self.ordered() {
            if let Payload::OwnerConfigured { configuration } = &event.payload
                && configuration.owner == *owner
            {
                by_from.insert(configuration.from, configuration.year_start_date());
            }
        }
        let (month, day) = by_from.range(..=date).next_back().map_or((1, 1), |(_, start)| *start);
        let year = if (date.month(), date.day()) >= (month, day) { date.year() } else { date.year() - 1 };
        let mut start = Date::new(year, month, day).expect("a day of every month");
        let next = Date::new(year + 1, month, day).expect("a day of every month");
        let mut end = next.previous().expect("after the first day of the calendar");
        // A change of the start on or before `date`, after the start: the year begins with it.
        let mut previous: Option<(u8, u8)> = None;
        for (from, year_start) in by_from.range(..=date) {
            if previous.is_some_and(|p| p != *year_start) && *from > start {
                start = *from;
            }
            previous = Some(*year_start);
        }
        // The next change of the start after `date`, before the year ends: the year ends the day before it.
        let mut current = (month, day);
        for (from, year_start) in by_from.range(date..).filter(|(from, _)| **from > date) {
            if *year_start != current {
                if *from <= end {
                    end = from.previous().expect("after the first day of the calendar");
                }
                break;
            }
            current = *year_start;
        }
        (start, end)
    }

    /// The date of the latest booking with a posting of `owner`.
    fn latest_booking(&self, owner: &PartyId) -> Option<Date> {
        self.ordered()
            .into_iter()
            .filter_map(|(_, event)| match &event.payload {
                Payload::BookingRecorded { booking } | Payload::BookingReversed { booking, .. } => Some(booking),
                _ => None,
            })
            .filter(|booking| booking.postings.iter().any(|posting| posting.owner == *owner))
            .map(|booking| booking.date)
            .max()
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
                basis: None,
                attributed: None,
                date: "2026-03-15".parse().unwrap(),
                text: "test".into(),
                postings,
                documents: vec![],
                entries: vec![],
                transaction: None,
            };
            booking.validate().unwrap();
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event { time: clock.now(ms as u64), seq, prev, payload: Payload::BookingRecorded { booking } });
        }
        set
    }

    /// A VAT payment for the last quarter, made on 8 January, counts in the year before on a cash basis (§ 11 Abs. 2
    /// Satz 2 EStG); attributed to a year not within ten days, it is reported.
    #[test]
    fn a_payment_within_ten_days_counts_in_the_year_it_belongs_to() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let mut add = |set: &mut EventSet, date: &str, attributed: Option<&str>| {
            let booking = Booking {
                basis: None,
                attributed: attributed.map(|d| d.parse().unwrap()),
                date: date.parse().unwrap(),
                text: "VAT for the fourth quarter".into(),
                postings: vec![posting("44571", 19_000, None), posting("512", -19_000, None)],
                documents: vec![],
                entries: vec![],
                transaction: None,
            };
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            let time = clock.now(seq);
            set.insert(Event { time, seq, prev, payload: Payload::BookingRecorded { booking } }).0
        };
        add(&mut set, "2027-01-08", Some("2026-12-31"));
        let euer = |set: &EventSet, year: u16| {
            set.period_report(
                &ReportMapping::de_euer_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                Date::new(year, 1, 1).unwrap(),
                Date::new(year, 12, 31).unwrap(),
            )
            .unwrap()
        };
        assert_eq!(euer(&set, 2026).get("2.186"), Some(&19_000));
        assert_eq!(euer(&set, 2027).get("2.186"), None);
        // The VAT return goes by the date: the payment is of January.
        let ustva = set
            .period_report(
                &ReportMapping::de_ustva_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                "2026-10-01".parse().unwrap(),
                "2026-12-31".parse().unwrap(),
            )
            .unwrap();
        assert!(ustva.is_empty());
        assert!(set.review_bookings().is_empty());
        let late = add(&mut set, "2027-02-01", Some("2026-12-31"));
        assert_eq!(set.review_bookings(), [crate::BookingProblem::Attribution { event: late }]);
    }

    /// A vehicle sold for 5,000 + 950 VAT with a book value of 3,000: the proceeds on line 19 of the Anlage EÜR
    /// without VAT, the VAT on line 21, the book value on line 39 – whatever the tax code says of the proceeds.
    #[test]
    fn an_asset_sold_reaches_its_own_lines_of_the_euer() {
        let skr = ChartTranslation::de_skr03();
        let posting = |number: &str, units: i128, tax: Option<&str>| Posting {
            owner: party(1),
            account: skr.accounts[number].clone(),
            amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
            note: None,
            tax: tax.map(|t| t.parse().unwrap()),
            exchange: None,
        };
        let set = books(vec![
            vec![
                posting("1200", 595_000, None),
                posting("8820", -500_000, Some("de-out-19")),
                posting("1776", -95_000, Some("de-out-19")),
            ],
            vec![posting("2315", 300_000, None), posting("0320", -300_000, None)],
        ]);
        let euer = set
            .period_report(
                &ReportMapping::de_euer_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                "2026-01-01".parse().unwrap(),
                "2026-12-31".parse().unwrap(),
            )
            .unwrap();
        let field = |kz: &str| euer.get(kz).copied();
        assert_eq!((field("1.102"), field("1.112"), field("1.140")), (Some(500_000), None, Some(95_000)));
        assert_eq!(field("2.135"), Some(300_000));
    }

    /// A balance sheet shortened for a small and a micro company (§ 266 Abs. 1 Satz 3 and 4 HGB).
    #[test]
    fn balance_sheets_are_shortened_by_size() {
        let fields: BTreeMap<String, i128> = [
            ("aktiv.A.II.1", 500),
            ("aktiv.A.II.3", 200),
            ("aktiv.B.IV", 300),
            ("passiv.A.I", 400),
            ("passiv.C.2", 250),
            ("passiv.C.4", 350),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let small = crate::hgb_shortened(&fields, crate::HgbSize::Small);
        let expected: BTreeMap<String, i128> =
            [("aktiv.A.II", 700), ("aktiv.B.IV", 300), ("passiv.A.I", 400), ("passiv.C", 600)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
        assert_eq!(small, expected);
        let micro = crate::hgb_shortened(&fields, crate::HgbSize::Micro);
        let expected: BTreeMap<String, i128> =
            [("aktiv.A", 700), ("aktiv.B", 300), ("passiv.A", 400), ("passiv.C", 600)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
        assert_eq!(micro, expected);
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

        // The annual return of the year takes the same postings into its own Kennzahlen.
        let year = set
            .period_report(
                &ReportMapping::de_ust_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                "2026-01-01".parse().unwrap(),
                "2026-12-31".parse().unwrap(),
            )
            .unwrap();
        let field = |kz: &str| year.get(kz).copied().unwrap_or(0);
        assert_eq!((field("177"), field("320")), (100_000, 3_800));
        assert_eq!((field("846"), field("847"), field("467")), (50_000, 9_500, 9_500));
        assert_eq!(year.len(), 5, "177, 320, 846, 847, 467");
    }

    /// Every German output and input code reaches the advance return, the annual return and the Anlage EÜR – a new
    /// code is not left out of one of them.
    #[test]
    fn every_german_tax_code_reaches_the_vat_returns() {
        let table = TaxTable::de();
        for mapping in [ReportMapping::de_ustva_2026(), ReportMapping::de_ust_2026()] {
            for code in &table.codes {
                if code.code.as_str() == "de-in-no-deduction" {
                    continue;
                }
                assert!(
                    mapping.taxes.iter().any(|rule| rule.tax == code.code),
                    "{} has no rule for {}",
                    mapping.report,
                    code.code
                );
            }
        }
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
    fn a_freelancers_expenses_in_skr_03_fall_into_their_lines_of_the_euer() {
        let skr03 = ChartTranslation::de_skr03();
        let posting = |number: &str, units: i128| Posting {
            owner: party(1),
            account: skr03.accounts[number].clone(),
            amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
            note: None,
            tax: None,
            exchange: None,
        };
        let paid = |number: &str, units: i128| vec![posting(number, units), posting("1200", -units)];
        let set = books(vec![
            paid("4920", 4_000),  // telephone
            paid("4925", 2_000),  // internet
            paid("4930", 1_500),  // office supplies
            paid("4650", 7_000),  // entertainment, the deductible 70 %
            paid("4676", 9_000),  // the proprietor's hotel on a business trip
            paid("4955", 30_000), // bookkeeping
            paid("4530", 6_000),  // fuel
            // Imputed costs and their offset are no expenses on a cash basis.
            vec![posting("4990", 50_000), posting("2890", -50_000)],
            // Commuting: the actual costs (Haben) out, the flat rate in, the rest not deductible.
            vec![posting("4678", 1_000), posting("4679", 500), posting("4680", -1_500)],
            // A car sold at a loss: the proceeds are income of line 19.
            vec![posting("1200", 300_000), posting("8800", -300_000)],
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
        let expected: BTreeMap<String, i128> = [
            ("2.280", 6_000),
            ("2.229", 1_500),
            ("2.175", 7_000),
            ("2.221", 9_000),
            ("2.194", 30_000),
            ("2.146", 6_000),
            ("2.176", 1_000),
            ("2.142", 1_500),
            ("1.102", 300_000),
        ]
        .into_iter()
        .map(|(field, units)| (field.to_owned(), units))
        .collect();
        assert_eq!(report, expected);
    }

    #[test]
    fn a_reversal_cancels_in_the_field_of_what_it_reverses() {
        let skr03 = ChartTranslation::de_skr03();
        let posting = |number: &str, units: i128| Posting {
            owner: party(1),
            account: skr03.accounts[number].clone(),
            amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
            note: None,
            tax: None,
            exchange: None,
        };
        // A car sold at a loss (a field of the credit side), a VAT payment and a withdrawal – each reversed.
        let mut set = books(vec![
            vec![posting("1200", 300_000), posting("8800", -300_000)],
            vec![posting("1780", 18_050), posting("1200", -18_050)],
            vec![posting("1800", 30_000), posting("1200", -30_000)],
        ]);
        let period = ("2026-01-01".parse().unwrap(), "2026-12-31".parse().unwrap());
        let euer = |set: &EventSet| {
            set.period_report(
                &ReportMapping::de_euer_2026(),
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                period.0,
                period.1,
            )
            .unwrap()
        };
        let fields: Vec<String> = euer(&set).into_keys().collect();
        assert_eq!(fields, ["1.102", "2.186", "5.122"], "proceeds, VAT paid, withdrawal");
        let mut clock = Clock::new("b".parse().unwrap());
        for (hash, event) in set.clone().ordered() {
            let Payload::BookingRecorded { booking } = &event.payload else { continue };
            let reversal = booking.reversed(booking.date, "reversed".into());
            let (seq, prev) = set.chain_head(&"b".parse().unwrap());
            let payload = Payload::BookingReversed { reverses: hash, booking: reversal };
            set.insert(Event { time: clock.now(10_000 + seq), seq, prev, payload });
        }
        let report = euer(&set);
        assert!(report.values().all(|units| *units == 0), "{report:?}");
    }

    #[test]
    fn every_expense_has_a_line_of_the_euer() {
        let chart = Chart::v0_1();
        let parents = EventSet::new().account_parents(&chart);
        let refined: std::collections::BTreeSet<&AccountId> = parents.values().collect();
        for euer in [ReportMapping::de_euer_2025(), ReportMapping::de_euer_2026()] {
            let expenses = chart.accounts.iter().filter(|a| a.kind == AccountKind::Expense && !refined.contains(&a.id));
            for account in expenses {
                let id = &account.id;
                let own = euer.accounts.contains_key(id)
                    || euer.credit_accounts.contains_key(id)
                    || euer.excluded.contains(id);
                if own {
                    continue;
                }
                // An account without a mapping of its own takes its group's line – never by accident the catch-all
                // line of other expenses (Kz 183), which an account reaches only by its own mapping.
                let line = euer.position(id, &parents, false);
                assert!(line.is_some_and(|line| line != "2.183"), "{id} falls to {line:?}");
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
            excluded: vec![],
            vat_accounts: None,
            vat_settlement: None,
            taxes: vec![],
            mandatory: vec![],
            legal_forms: BTreeMap::new(),
            mandatory_by_form: BTreeMap::new(),
            cash_basis: false,
            accounts_first: vec![],
            asset_groups: BTreeMap::new(),
            asset_fields: BTreeMap::new(),
            pools: vec![],
            payroll_fields: BTreeMap::new(),
        };
        let set = books(vec![vec![posting("4111", 5_000, None), posting("5121", -5_000, None)]]);
        let report = set.account_report(&mapping, &Chart::v0_1(), &party(1), None).unwrap();
        assert_eq!(report.get("aktiv.B.II.1"), Some(&5_000), "4111 refines 41");
        assert_eq!(report.get("passiv.C.2"), Some(&-5_000), "an overdrawn bank account is a liability");
        // An exclusion keeps the mapping above it from reaching the accounts below it, but not their own mappings.
        let mut mapping = mapping;
        mapping.excluded.push(pcg.accounts["411"].clone());
        let parents = EventSet::new().account_parents(&Chart::v0_1());
        assert_eq!(mapping.position(&pcg.accounts["4111"], &parents, false), None);
        assert_eq!(mapping.position(&pcg.accounts["41"], &parents, false), Some("aktiv.B.II.1"));
        mapping.accounts.insert(pcg.accounts["4111"].clone(), "aktiv.B.II.4".into());
        assert_eq!(mapping.position(&pcg.accounts["4111"], &parents, false), Some("aktiv.B.II.4"));
    }

    /// A balance sheet shows the result of the year and of earlier years on equity, without a closing booking.
    #[test]
    fn a_balance_sheet_shows_the_result_on_equity() {
        let dated = |date: &str, postings: Vec<Posting>| Booking {
            basis: None,
            attributed: None,
            date: date.parse().unwrap(),
            text: "test".into(),
            postings,
            documents: vec![],
            entries: vec![],
            transaction: None,
        };
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let mut write = |set: &mut EventSet, ms: u64, payload: Payload| {
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event { time: clock.now(ms), seq, prev, payload });
        };
        // The financial year begins on 1 July.
        let configuration = serde_json::from_value(serde_json::json!({
            "owner": party(1).to_string(), "from": "2025-01-01", "currency": "EUR", "year_start": 7,
        }))
        .unwrap();
        write(&mut set, 0, Payload::OwnerConfigured { configuration });
        // Sales of 1,000 in the year 2025/26, costs of 300 and sales of 500 in 2026/27.
        for (ms, booking) in [
            dated("2026-05-10", vec![posting("5121", 100_000, None), posting("706", -100_000, None)]),
            dated("2026-07-01", vec![posting("6064", 30_000, None), posting("5121", -30_000, None)]),
            dated("2026-09-30", vec![posting("5121", 50_000, None), posting("706", -50_000, None)]),
        ]
        .into_iter()
        .enumerate()
        {
            write(&mut set, ms as u64 + 1, Payload::BookingRecorded { booking });
        }
        let chart = Chart::v0_1();
        let bs = ReportMapping::de_hgb_266_2026();
        let report = set.account_report(&bs, &chart, &party(1), Some("2026-12-31".parse().unwrap())).unwrap();
        assert_eq!(report.get("aktiv.B.IV"), Some(&120_000));
        assert_eq!(report.get("passiv.A.IV"), Some(&-100_000), "retained: the year 2025/26");
        assert_eq!(report.get("passiv.A.V"), Some(&-20_000), "the result of 2026/27 so far");
        assert_eq!(report.values().sum::<i128>(), 0, "the balance sheet balances");
        // In the earlier year, all of it is the result for the year.
        let report = set.account_report(&bs, &chart, &party(1), Some("2026-06-30".parse().unwrap())).unwrap();
        assert_eq!(report.get("passiv.A.IV"), None);
        assert_eq!(report.get("passiv.A.V"), Some(&-100_000));
        // Without a date, the year of the latest booking.
        let report = set.account_report(&bs, &chart, &party(1), None).unwrap();
        assert_eq!(report.get("passiv.A.V"), Some(&-20_000));
        // The same on the E-Bilanz, and the profit and loss account keeps its own positions.
        let report = set.account_report(&ReportMapping::de_ebilanz_bs_6_10(), &chart, &party(1), None).unwrap();
        assert_eq!(report.get("bs.eqLiab.equity.netIncome"), Some(&-20_000));
        assert_eq!(report.get("bs.eqLiab.equity.retainedEarnings"), Some(&-100_000));
        let gkv = set.account_report(&ReportMapping::de_hgb_275_gkv_2026(), &chart, &party(1), None).unwrap();
        assert!(gkv.keys().all(|position| position.starts_with("gkv.")));
        assert_eq!(set.financial_year_start(&party(1), "2026-06-30".parse().unwrap()), "2025-07-01".parse().unwrap());
        assert_eq!(set.financial_year_start(&party(2), "2026-06-30".parse().unwrap()), "2026-01-01".parse().unwrap());
    }

    /// The recapitulative statement: per customer's VAT identification number and kind, from the invoices.
    #[test]
    fn a_recapitulative_statement_per_customer() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let device: crate::DeviceId = "a".parse().unwrap();
        let mut add = |set: &mut EventSet, payload: Payload| {
            let (seq, prev) = set.chain_head(&device);
            let ms = seq;
            set.insert(Event { time: clock.now(ms), seq, prev, payload }).0
        };
        let invoice = |id: u8, vat_id: Option<&str>, buyer: Option<PartyId>| -> crate::Document {
            let mut document: crate::Document = serde_json::from_value(serde_json::json!({
                "id": format!("01a18406-0000-7000-8000-0000000000{id:02}"), "kind": "invoice", "date": "2026-03-15",
                "received": "2026-03-15",
                "original": {"hash": format!("sha256:{}", "ab".repeat(32)), "size": 1, "media_type": "application/pdf"},
            }))
            .unwrap();
            let mut buyer_party = crate::InvoiceParty::named("Client");
            buyer_party.vat_id = vat_id.map(str::to_owned);
            buyer_party.party = buyer;
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/en16931/01.01a-INVOICE_ubl.xml");
            let read = crate::en16931::read(&std::fs::read(path).unwrap(), &crate::CurrencyTable::v0_1()).unwrap();
            let mut data = read.invoice;
            data.buyer = buyer_party;
            document.invoice = Some(Box::new(data));
            document
        };
        let sale = |documents: Vec<crate::DocumentId>, tax: &str, units: i128| Booking {
            basis: None,
            attributed: None,
            date: "2026-03-15".parse().unwrap(),
            text: "sale".into(),
            postings: vec![posting("411", units, None), posting("706", -units, Some(tax))],
            documents,
            entries: vec![],
            transaction: None,
        };
        // A customer in France by the invoice, one in Austria by its description; a domestic sale counts nowhere.
        let austrian: PartyId = party(2);
        let organisation = serde_json::from_value(serde_json::json!({
            "id": austrian.to_string(), "name": "Kunde GmbH", "legal_form": "", "country": "AT",
            "tax_ids": [{"scheme": "vat", "number": "ATU12345678"}],
        }))
        .unwrap();
        add(&mut set, Payload::OrganisationDescribed { organisation });
        let documents =
            [invoice(1, Some("FR40303265045"), None), invoice(2, None, Some(austrian)), invoice(3, None, None)];
        for document in &documents {
            add(&mut set, Payload::DocumentRecorded { document: document.clone() });
        }
        let id = |i: usize| documents[i].id.clone();
        let first = sale(vec![id(0)], "de-out-eu-service", 100_000);
        let recorded = add(&mut set, Payload::BookingRecorded { booking: first.clone() });
        add(&mut set, Payload::BookingRecorded { booking: sale(vec![id(0)], "de-out-eu-service", 50_000) });
        add(&mut set, Payload::BookingRecorded { booking: sale(vec![id(1)], "de-out-ig-supply", 70_000) });
        add(&mut set, Payload::BookingRecorded { booking: sale(vec![id(1)], "de-out-19", 10_000) });
        // A reversal reduces the sum; a booking without a number, or with two, is reported.
        add(
            &mut set,
            Payload::BookingReversed {
                reverses: recorded,
                booking: first.reversed("2026-03-20".parse().unwrap(), "storno".into()),
            },
        );
        let without = add(&mut set, Payload::BookingRecorded { booking: sale(vec![id(2)], "de-out-ig-supply", 1) });
        let both = add(&mut set, Payload::BookingRecorded { booking: sale(vec![id(0), id(1)], "de-out-ig-supply", 1) });
        let (fields, problems) = set
            .customer_report(
                &ReportMapping::de_zm_2026(),
                &Chart::v0_1(),
                &party(1),
                "2026-01-01".parse().unwrap(),
                "2026-03-31".parse().unwrap(),
            )
            .unwrap();
        assert_eq!(fields, BTreeMap::from([("ATU12345678/L".into(), 70_000), ("FR40303265045/S".into(), 50_000)]));
        assert_eq!(
            problems,
            vec![CustomerProblem::NoVatId { event: without }, CustomerProblem::SeveralVatIds { event: both }]
        );
        // As the content of a filing: the same fields, in money of the functional currency.
        let eur: crate::Currency = "EUR".parse().unwrap();
        let content = set
            .report_content(
                &ReportMapping::de_zm_2026(),
                crate::Derivation::Customers,
                &crate::TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                &eur,
                "2026-01-01".parse().unwrap(),
                "2026-03-31".parse().unwrap(),
            )
            .unwrap();
        assert_eq!((content.report.as_str(), content.version.as_str()), ("de-zm", "2026"));
        let units: BTreeMap<String, i128> =
            content.fields.iter().map(|(field, money)| (field.clone(), money.base_units())).collect();
        assert_eq!(units, fields);
    }

    /// A change of `year_start` makes a short financial year (§ 8b Satz 2 Nr. 2 EStDV).
    #[test]
    fn a_change_of_the_financial_year_makes_a_short_one() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let date = |d: &str| -> Date { d.parse().unwrap() };
        for (ms, (from, year_start)) in
            [("2025-01-01", 1), ("2026-03-01", 1), ("2026-07-01", 7)].into_iter().enumerate()
        {
            let configuration = serde_json::from_value(serde_json::json!({
                "owner": party(1).to_string(), "from": from, "currency": "EUR", "year_start": year_start,
            }))
            .unwrap();
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event {
                time: clock.now(ms as u64),
                seq,
                prev,
                payload: Payload::OwnerConfigured { configuration },
            });
        }
        let year = |d: &str| set.financial_year(&party(1), date(d));
        assert_eq!(year("2025-12-31"), (date("2025-01-01"), date("2025-12-31")));
        // A configuration that keeps `year_start` changes nothing; the change ends the year early.
        assert_eq!(year("2026-04-15"), (date("2026-01-01"), date("2026-06-30")), "short");
        assert_eq!(year("2026-07-01"), (date("2026-07-01"), date("2027-06-30")));
        assert_eq!(year("2027-06-30"), (date("2026-07-01"), date("2027-06-30")));
        assert_eq!(year("2027-07-01"), (date("2027-07-01"), date("2028-06-30")));
        assert!(set.review_parties().iter().all(|p| !matches!(p, crate::PartyProblem::YearChangedWithin { .. })));
        // A change from a day that does not begin the new year is reported.
        let configuration = serde_json::from_value(serde_json::json!({
            "owner": party(1).to_string(), "from": "2028-03-01", "currency": "EUR", "year_start": 4,
        }))
        .unwrap();
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        let event = Event { time: clock.now(10), seq, prev, payload: Payload::OwnerConfigured { configuration } };
        let (changed, _) = set.insert(event);
        assert!(
            set.review_parties().contains(&crate::PartyProblem::YearChangedWithin { event: changed, owner: party(1) })
        );
        // Without a configuration, the calendar year; before the first one, too.
        assert_eq!(set.financial_year(&party(2), date("2026-02-28")), (date("2026-01-01"), date("2026-12-31")));
        assert_eq!(date("2024-03-01").previous(), Some(date("2024-02-29")));
        assert_eq!(date("0001-01-01").previous(), None);
    }

    /// A financial year that begins in the middle of a month.
    #[test]
    fn a_financial_year_from_the_middle_of_a_month() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let date = |d: &str| -> Date { d.parse().unwrap() };
        for (ms, (from, month, day)) in [("2025-01-01", 1, None), ("2026-07-16", 7, Some(16))].into_iter().enumerate() {
            let mut json = serde_json::json!({
                "owner": party(1).to_string(), "from": from, "currency": "EUR", "year_start": month,
            });
            if let Some(day) = day {
                json["year_start_day"] = serde_json::json!(day);
            }
            let configuration = serde_json::from_value(json).unwrap();
            let (seq, prev) = set.chain_head(&"a".parse().unwrap());
            set.insert(Event {
                time: clock.now(ms as u64),
                seq,
                prev,
                payload: Payload::OwnerConfigured { configuration },
            });
        }
        let year = |d: &str| set.financial_year(&party(1), date(d));
        assert_eq!(year("2026-07-15"), (date("2026-01-01"), date("2026-07-15")), "short");
        assert_eq!(year("2026-07-16"), (date("2026-07-16"), date("2027-07-15")));
        assert_eq!(year("2027-07-15"), (date("2026-07-16"), date("2027-07-15")));
        assert_eq!(year("2028-01-01"), (date("2027-07-16"), date("2028-07-15")));
        assert!(set.review_parties().iter().all(|p| !matches!(p, crate::PartyProblem::YearChangedWithin { .. })));
        for day in [0, 1, 29, 31] {
            let json = serde_json::json!({
                "owner": party(1).to_string(), "from": "2026-01-01", "currency": "EUR", "year_start": 7,
                "year_start_day": day,
            });
            assert!(serde_json::from_value::<crate::Configuration>(json).is_err(), "{day}");
        }
    }

    /// The remaining terms of receivables and liabilities (§ 268 Abs. 4 and 5 HGB), from the attribute `term`.
    #[test]
    fn remaining_terms_per_position() {
        let skr03 = ChartTranslation::de_skr03();
        let on = |number: &str, units: i128| Posting {
            account: skr03.accounts[number].clone(),
            ..posting("512", units, None)
        };
        // Receivables up to and over a year, without a term; advance payments received over one and five years.
        let set = books(vec![vec![
            on("1451", 10_000),
            on("1455", 20_000),
            on("1400", 5_000),
            on("1720", -30_000),
            on("1721", -40_000),
            on("1200", 35_000),
        ]]);
        let chart = Chart::v0_1();
        let bs = ReportMapping::de_hgb_266_2026();
        let positions = set.account_report(&bs, &chart, &party(1), None).unwrap();
        let terms = set.term_report(&bs, &chart, &party(1), None).unwrap();
        let receivables = bs.position(&skr03.accounts["1455"], &set.account_parents(&chart), false).unwrap();
        assert_eq!(
            terms[receivables],
            Terms { up_to_one_year: 10_000, over_one_year: 20_000, over_five_years: 0, unknown: 5_000 }
        );
        let advances = bs.position(&skr03.accounts["1720"], &set.account_parents(&chart), true).unwrap();
        assert_eq!(
            terms[advances],
            Terms { up_to_one_year: 0, over_one_year: -70_000, over_five_years: -40_000, unknown: 0 }
        );
        // The terms add up to the positions.
        for (position, t) in &terms {
            assert_eq!(t.up_to_one_year + t.over_one_year + t.unknown, positions[position], "{position}");
        }
        assert_eq!(chart.attributes(&skr03.accounts["1455"]).get("term").map(String::as_str), Some("over-1-year"));
    }

    /// The Mussfelder of the E-Bilanz per legal form (taxonomy 6.10, `legalFormEU`, `legalFormPG`, `legalFormKSt`).
    #[test]
    fn mandatory_positions_by_legal_form() {
        let bs = ReportMapping::de_ebilanz_bs_6_10();
        let all = bs.mandatory_for(None);
        assert_eq!(all.len(), bs.mandatory.len());
        let (gmbh, kg, ek) =
            (bs.mandatory_for(Some("de-gmbh")), bs.mandatory_for(Some("de-kg")), bs.mandatory_for(Some("de-ek")));
        assert!(
            gmbh.contains(&"bs.ass.currAss.receiv.shareholders.gmbh")
                && !kg.contains(&"bs.ass.currAss.receiv.shareholders.gmbh")
        );
        assert!(kg.contains(&"bs.ass.currAss.receiv.shareholders.limitedLiable"));
        assert!(ek.len() > all.len() && all.iter().all(|p| ek.contains(p)));
        assert_eq!(bs.mandatory_for(Some("fr-sas")), all, "a legal form outside the groups");
        for mapping in [bs, ReportMapping::de_ebilanz_is_gkv_6_10()] {
            for positions in mapping.mandatory_by_form.values() {
                assert!(positions.iter().all(|p| !mapping.mandatory.contains(p)));
            }
        }
    }

    /// Bookings in one balance sheet only make the reconciliation to the tax balance sheet.
    #[test]
    fn the_reconciliation_to_the_tax_balance_sheet() {
        let mut set = books(vec![vec![posting("512", 100_000, None), posting("101", -100_000, None)]]);
        let mut clock = Clock::new("b".parse().unwrap());
        // A provision for impending losses, in the commercial balance sheet only (§ 5 Abs. 4a EStG); a tax-only
        // write-down of a receivable.
        for (ms, (basis, postings)) in [
            (Basis::Commercial, vec![posting("6815", 30_000, None), posting("1518", -30_000, None)]),
            (Basis::Tax, vec![posting("6817", 5_000, None), posting("491", -5_000, None)]),
        ]
        .into_iter()
        .enumerate()
        {
            let booking = Booking {
                basis: Some(basis),
                attributed: None,
                date: "2026-12-31".parse().unwrap(),
                text: "test".into(),
                postings,
                documents: vec![],
                entries: vec![],
                transaction: None,
            };
            let (seq, prev) = set.chain_head(&"b".parse().unwrap());
            set.insert(Event { time: clock.now(ms as u64), seq, prev, payload: Payload::BookingRecorded { booking } });
        }
        let chart = Chart::v0_1();
        let bs = ReportMapping::de_hgb_266_2026();
        let commercial = set.account_report(&bs, &chart, &party(1), None).unwrap();
        let tax = set.account_report_in(Basis::Tax, &bs, &chart, &party(1), None).unwrap();
        let reconciliation = set.reconciliation(&bs, &chart, &party(1), None).unwrap();
        assert_eq!(commercial.values().sum::<i128>(), 0);
        assert_eq!(tax.values().sum::<i128>(), 0);
        for (position, units) in &reconciliation {
            assert_eq!(commercial.get(position).copied().unwrap_or(0) + units, tax.get(position).copied().unwrap_or(0));
        }
        // The provision is gone, the receivables are lower, and the result differs by both.
        assert!(reconciliation.values().any(|units| *units == 30_000), "{reconciliation:?}");
        assert!(reconciliation.values().any(|units| *units == -5_000), "{reconciliation:?}");
        assert!(!reconciliation.is_empty() && reconciliation.values().sum::<i128>() == 0);
        assert!(
            set.functional_balances(None)
                .unwrap()
                .iter()
                .all(|(_, account, _, _)| { *account != ChartTranslation::fr_pcg().accounts["6817"] })
        );
    }
}
