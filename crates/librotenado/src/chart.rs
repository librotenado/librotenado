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
}

impl AccountKind {
    /// Whether the account normally has a debit balance (assets and expenses) – postings there are positive.
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
}
