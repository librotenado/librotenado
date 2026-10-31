// SPDX-License-Identifier: MIT
//! The harmonised chart of accounts (spec part 0004) and translations of national charts into it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{AccountId, Object};

/// What an account is, in the sense of double-entry bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// What the organisation owns or is owed.
    Asset,
    /// What it owes.
    Liability,
    /// What belongs to its owners or members.
    Equity,
    /// What increases the result.
    Income,
    /// What decreases the result.
    Expense,
    /// Outside the balance sheet and the result: commitments, opening and closing balance sheet accounts.
    Memo,
}

impl AccountKind {
    /// Whether the account normally has a debit balance (assets and expenses) – postings there are positive. Memo
    /// accounts have no normal side.
    pub fn debit_normal(self) -> bool {
        matches!(self, AccountKind::Asset | AccountKind::Expense)
    }
}

/// An account: of the harmonised chart, or opened in a set of books to refine one (spec parts 0002, 0004).
///
/// Names are not part of an account, as they differ by language: translation data names it by its identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Account {
    /// Its identity, stable across versions of the chart.
    pub id: AccountId,
    /// What it is.
    pub kind: AccountKind,
    /// The account it refines; none for a top-level account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<AccountId>,
    /// Further distinctions the account carries (for example a tax treatment or a non-profit sphere), as
    /// attribute → value. Attributes are inherited by refining accounts unless they set their own value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

/// A chart of accounts: a hierarchy of accounts with attributes, under a name and version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chart {
    /// The chart's name, for example `librotenado`.
    pub name: String,
    /// Its version.
    pub version: String,
    /// The accounts.
    pub accounts: Vec<Account>,
}

impl Chart {
    /// The harmonised chart of version 0.1 (`spec/data/chart.json`).
    pub fn v0_1() -> Self {
        crate::from_root(include_bytes!("../data/chart.json")).expect("the published chart is valid")
    }

    /// The opening balance sheet of the chart 0.1, a memo account: the other side of opening balances (part 0004).
    pub fn opening_balances_v0_1() -> AccountId {
        "01a182f7-2145-7351-b0f0-548a57054ddf".parse().expect("an account identity")
    }
}

impl ChartTranslation {
    /// The translation of the French Plan comptable général, version of 1 January 2026, into the harmonised chart
    /// 0.1 (`spec/data/translations/fr-pcg.json`).
    pub fn fr_pcg() -> Self {
        crate::from_root(include_bytes!("../data/translations/fr-pcg.json"))
            .expect("the published translation is valid")
    }

    /// The translation of the DATEV standard chart SKR 03 (process structure), valid for 2026, into the harmonised
    /// chart 0.1 (`spec/data/translations/de-skr03.json`), reversible (spec part 0004).
    pub fn de_skr03() -> Self {
        crate::from_root(include_bytes!("../data/translations/de-skr03.json"))
            .expect("the published translation is valid")
    }

    /// The translation of the DATEV standard chart SKR 04 (closing statement structure), valid for 2026, into the
    /// harmonised chart 0.1 (`spec/data/translations/de-skr04.json`), reversible.
    pub fn de_skr04() -> Self {
        crate::from_root(include_bytes!("../data/translations/de-skr04.json"))
            .expect("the published translation is valid")
    }
}

impl Object for Account {
    const TYPE: &'static str = "account";
}

impl Object for Chart {
    const TYPE: &'static str = "chart";
}

impl Object for ChartTranslation {
    const TYPE: &'static str = "chart_translation";
}

/// Why a chart is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChartError {
    /// Two accounts with the same identity.
    #[error("account {0} is defined twice")]
    Duplicate(AccountId),
    /// A parent that is not in the chart.
    #[error("account {0} refines {1}, which is not in the chart")]
    UnknownParent(AccountId, AccountId),
    /// An account that refines one of a different kind.
    #[error("account {0} is of a different kind than its parent")]
    KindMismatch(AccountId),
    /// Accounts that refine each other in a circle.
    #[error("account {0} is part of a cycle")]
    Cycle(AccountId),
}

impl Chart {
    /// The account with this identity.
    pub fn get(&self, id: &AccountId) -> Option<&Account> {
        self.accounts.iter().find(|a| &a.id == id)
    }

    /// Checks that identities are unique, parents exist and are of the same kind, and there are no cycles.
    pub fn validate(&self) -> Result<(), ChartError> {
        let mut by_id = BTreeMap::new();
        for account in &self.accounts {
            if by_id.insert(&account.id, account).is_some() {
                return Err(ChartError::Duplicate(account.id.clone()));
            }
        }
        for account in &self.accounts {
            let Some(parent_id) = &account.parent else { continue };
            let parent =
                by_id.get(parent_id).ok_or_else(|| ChartError::UnknownParent(account.id.clone(), parent_id.clone()))?;
            if parent.kind != account.kind {
                return Err(ChartError::KindMismatch(account.id.clone()));
            }
            let mut seen = BTreeSet::from([&account.id]);
            let mut next = account.parent.as_ref();
            while let Some(id) = next {
                if !seen.insert(id) {
                    return Err(ChartError::Cycle(account.id.clone()));
                }
                next = by_id.get(id).and_then(|a| a.parent.as_ref());
            }
        }
        Ok(())
    }

    /// The account's attributes including those inherited from its parents (its own values win).
    pub fn attributes(&self, id: &AccountId) -> BTreeMap<String, String> {
        let mut chain = Vec::new();
        let mut next = self.get(id);
        while let Some(account) = next {
            if chain.len() > self.accounts.len() {
                break; // a cycle; validate() reports it
            }
            chain.push(account);
            next = account.parent.as_ref().and_then(|p| self.get(p));
        }
        chain.iter().rev().flat_map(|a| a.attributes.clone()).collect()
    }
}

/// How a national chart of accounts translates into the harmonised one: each national account onto exactly one
/// harmonised account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartTranslation {
    /// The national chart, for example `de-skr04`.
    pub chart: String,
    /// Its version.
    pub version: String,
    /// The harmonised chart and version it translates into, as `name/version`.
    pub into: String,
    /// National account number → harmonised account.
    pub accounts: BTreeMap<String, AccountId>,
}

/// Why a translation does not fit its chart.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranslationError {
    /// The translation targets a different chart or version.
    #[error("the translation is into {0}, not into this chart")]
    WrongChart(String),
    /// A national account maps onto an account the chart does not have.
    #[error("national account {0} maps onto {1}, which is not in the chart")]
    UnknownAccount(String, AccountId),
}

impl ChartTranslation {
    /// Checks the translation against the harmonised chart. Every national account maps onto exactly one account
    /// by construction (unambiguous); this checks that those accounts exist.
    pub fn validate(&self, chart: &Chart) -> Result<(), TranslationError> {
        let target = format!("{}/{}", chart.name, chart.version);
        if self.into != target {
            return Err(TranslationError::WrongChart(self.into.clone()));
        }
        match self.accounts.iter().find(|(_, id)| chart.get(id).is_none()) {
            Some((code, id)) => Err(TranslationError::UnknownAccount(code.clone(), id.clone())),
            None => Ok(()),
        }
    }

    /// Whether the translation can be reversed: no two national accounts map onto the same harmonised account.
    /// Where this fails, the harmonised chart is coarser than the national one and has to grow (the model grows,
    /// translations do not bend).
    pub fn coarser_accounts(&self) -> Vec<AccountId> {
        let mut seen = BTreeSet::new();
        let mut shared = BTreeSet::new();
        for id in self.accounts.values() {
            if !seen.insert(id) {
                shared.insert(id.clone());
            }
        }
        shared.into_iter().collect()
    }

    /// The national account for a harmonised one, if the translation is reversible there.
    pub fn national(&self, id: &AccountId) -> Option<&str> {
        let mut matches = self.accounts.iter().filter(|(_, target)| *target == id);
        match (matches.next(), matches.next()) {
            (Some((code, _)), None) => Some(code),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn opening_balances_are_a_memo_account_of_the_chart() {
        let chart = Chart::v0_1();
        let account = chart.get(&Chart::opening_balances_v0_1()).expect("in the chart");
        assert_eq!(account.kind, AccountKind::Memo);
    }
    use super::*;

    use crate::booking::tests::account as id;

    // Test accounts by number: 1 bank, 2 office expenses, 3 office software; others as the tests need.
    const BANK: u8 = 1;
    const OFFICE: u8 = 2;
    const SOFTWARE: u8 = 3;

    fn account(n: u8, kind: AccountKind, parent: Option<u8>, attrs: &[(&str, &str)]) -> Account {
        Account {
            id: id(n),
            kind,
            parent: parent.map(id),
            attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn chart() -> Chart {
        Chart {
            name: "librotenado".into(),
            version: "0.1".into(),
            accounts: vec![
                account(BANK, AccountKind::Asset, None, &[]),
                account(OFFICE, AccountKind::Expense, None, &[("tax", "input-vat-deductible")]),
                account(SOFTWARE, AccountKind::Expense, Some(OFFICE), &[("useful-life", "3y")]),
            ],
        }
    }

    #[test]
    fn a_valid_chart_inherits_attributes() {
        let c = chart();
        assert_eq!(c.validate(), Ok(()));
        let attrs = c.attributes(&id(SOFTWARE));
        assert_eq!(attrs.get("tax").map(String::as_str), Some("input-vat-deductible"));
        assert_eq!(attrs.get("useful-life").map(String::as_str), Some("3y"));
        assert!(AccountKind::Expense.debit_normal() && !AccountKind::Income.debit_normal());
    }

    #[test]
    fn the_chart_of_version_0_1_with_the_pcg() {
        let chart = Chart::v0_1();
        assert_eq!(chart.validate(), Ok(()));
        let roots: Vec<AccountKind> = chart.accounts.iter().filter(|a| a.parent.is_none()).map(|a| a.kind).collect();
        assert_eq!(roots.len(), 6, "one root per kind");

        let pcg = ChartTranslation::fr_pcg();
        assert_eq!(pcg.validate(&chart), Ok(()));
        assert!(pcg.coarser_accounts().is_empty(), "every PCG account has an account of its own");
        let kind = |number: &str| chart.get(&pcg.accounts[number]).map(|a| a.kind);
        assert_eq!(kind("512"), Some(AccountKind::Asset), "banks");
        assert_eq!(kind("519"), Some(AccountKind::Liability), "bank overdrafts, under banks in the PCG");
        assert_eq!(kind("401"), Some(AccountKind::Liability));
        assert_eq!(kind("409"), Some(AccountKind::Asset), "suppliers with debit balances");
        assert_eq!(kind("44566"), Some(AccountKind::Asset), "deductible VAT");
        assert_eq!(kind("44571"), Some(AccountKind::Liability), "VAT collected");
        assert_eq!(kind("101"), Some(AccountKind::Equity));
        assert_eq!(kind("6064"), Some(AccountKind::Expense));
        assert_eq!(kind("706"), Some(AccountKind::Income));
        assert_eq!(kind("890"), Some(AccountKind::Memo));
        assert_eq!(pcg.national(&pcg.accounts["6064"]), Some("6064"), "reversible");
    }

    #[test]
    fn finds_broken_hierarchies() {
        let mut c = chart();
        c.accounts.push(account(BANK, AccountKind::Asset, None, &[]));
        assert_eq!(c.validate(), Err(ChartError::Duplicate(id(BANK))));

        let mut c = chart();
        c.accounts.push(account(10, AccountKind::Asset, Some(11), &[]));
        assert_eq!(c.validate(), Err(ChartError::UnknownParent(id(10), id(11))));

        let mut c = chart();
        c.accounts.push(account(12, AccountKind::Expense, Some(BANK), &[]));
        assert_eq!(c.validate(), Err(ChartError::KindMismatch(id(12))));

        let mut c = chart();
        c.accounts.push(account(13, AccountKind::Asset, Some(14), &[]));
        c.accounts.push(account(14, AccountKind::Asset, Some(13), &[]));
        assert_eq!(c.validate(), Err(ChartError::Cycle(id(13))));
    }

    #[test]
    fn translations_must_hit_the_chart_and_show_where_the_model_must_grow() {
        let c = chart();
        let mut t = ChartTranslation {
            chart: "example-national".into(),
            version: "2026".into(),
            into: "librotenado/0.1".into(),
            accounts: BTreeMap::from([("1800".into(), id(BANK)), ("6815".into(), id(OFFICE))]),
        };
        assert_eq!(t.validate(&c), Ok(()));
        assert!(t.coarser_accounts().is_empty());
        assert_eq!(t.national(&id(BANK)), Some("1800"));

        t.accounts.insert("6816".into(), id(OFFICE));
        assert_eq!(t.coarser_accounts(), [id(OFFICE)]);
        assert_eq!(t.national(&id(OFFICE)), None);

        t.accounts.insert("9999".into(), id(99));
        assert_eq!(t.validate(&c), Err(TranslationError::UnknownAccount("9999".into(), id(99))));
    }

    /// The German standard charts land where German books need them: cash, bank, VAT, receivables, payables and
    /// sales in the HGB positions, VAT in the accounts the VAT return reads.
    #[test]
    fn the_german_standard_charts() {
        let chart = Chart::v0_1();
        let parents = crate::EventSet::new().account_parents(&chart);
        let (bs, gkv) = (crate::ReportMapping::de_hgb_266_2026(), crate::ReportMapping::de_hgb_275_gkv_2026());
        let ustva = crate::ReportMapping::de_ustva_2026();
        let vat = ustva.vat_accounts.as_ref().unwrap();
        let holds = |roots: &[AccountId], account: &AccountId| {
            let mut next = Some(account);
            while let Some(a) = next {
                if roots.contains(a) {
                    return true;
                }
                next = parents.get(a);
            }
            false
        };
        for (translation, cash, bank, receivables, payables, input, output, sales) in [
            (ChartTranslation::de_skr03(), "1000", "1200", "1400", "1600", "1576", "1776", "8400"),
            (ChartTranslation::de_skr04(), "1600", "1800", "1200", "3300", "1406", "3806", "4400"),
        ] {
            assert_eq!(translation.validate(&chart), Ok(()), "{}", translation.chart);
            assert!(translation.accounts.len() > 1_200, "{}", translation.chart);
            assert!(translation.coarser_accounts().is_empty(), "{} is reversible", translation.chart);
            let at = |n: &str| &translation.accounts[n];
            let position = |m: &crate::ReportMapping, n: &str| m.position(at(n), &parents, false).map(str::to_owned);
            assert_eq!(position(&bs, cash), Some("aktiv.B.IV".into()));
            assert_eq!(position(&bs, bank), Some("aktiv.B.IV".into()));
            assert_eq!(bs.position(at(bank), &parents, true), Some("passiv.C.2"), "an overdraft");
            assert_eq!(position(&bs, receivables), Some("aktiv.B.II.1".into()));
            assert_eq!(position(&bs, payables), Some("passiv.C.4".into()));
            assert_eq!(position(&gkv, sales), Some("gkv.1".into()));
            assert!(holds(&vat.input, at(input)) && holds(&vat.output, at(output)), "{}", translation.chart);
            // Every account falls into a position of the balance sheet or the profit and loss account.
            for (number, account) in &translation.accounts {
                let placed = bs.position(account, &parents, false).or(gkv.position(account, &parents, false));
                assert!(placed.is_some(), "{} {number}", translation.chart);
            }
        }
    }
}
