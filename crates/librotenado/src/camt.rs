// SPDX-License-Identifier: MIT
//! Reading bank statements in ISO 20022 camt.053 (spec part 0005): versions 001.02 and 001.08, as German banks
//! deliver them. Elements are matched by their local names, whatever their namespace.

use roxmltree::{Document as Xml, Node};

use crate::{
    Counterparty, Currency, CurrencyTable, Date, DocumentKind, EntryDetail, Money, StatementAccount, StatementData,
    StatementEntry,
};

/// Why a camt.053 file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CamtError {
    /// Not XML.
    #[error("not XML: {0}")]
    Xml(String),
    /// Not a BankToCustomerStatement.
    #[error("not a bank statement in camt.053")]
    Syntax,
    /// An element the model needs is missing.
    #[error("{0} is missing")]
    Missing(&'static str),
    /// A value that cannot be read.
    #[error("{element} is not valid: {value:?}")]
    Invalid {
        /// The element.
        element: &'static str,
        /// The value.
        value: String,
    },
}

/// A statement read from a camt.053 file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadStatement {
    /// The name of the format: `iso20022-camt.053`.
    pub format: &'static str,
    /// Always a bank statement.
    pub kind: DocumentKind,
    /// The statement's identification (`Stmt/Id`).
    pub number: String,
    /// The day it was created.
    pub date: Date,
    /// Its data.
    pub statement: StatementData,
}

type Result<T> = std::result::Result<T, CamtError>;

/// The first descendant along a path of local names.
fn at<'a, 'i>(node: Node<'a, 'i>, path: &[&str]) -> Option<Node<'a, 'i>> {
    let mut current = node;
    for name in path {
        current = current.children().find(|c| c.is_element() && c.tag_name().name() == *name)?;
    }
    Some(current)
}

/// All children with this local name.
fn all<'a, 'i>(node: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

/// The trimmed text along a path, if present and not empty.
fn text(node: Node, path: &[&str]) -> Option<String> {
    at(node, path).and_then(|n| n.text()).map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// A date, from `Dt` or the day of `DtTm`.
fn date(node: Node, element: &'static str) -> Result<Option<Date>> {
    let Some(value) = text(node, &["Dt"]).or_else(|| text(node, &["DtTm"])) else { return Ok(None) };
    value.get(..10).and_then(|day| day.parse().ok()).map(Some).ok_or(CamtError::Invalid { element, value })
}

/// An amount with its sign by `CdtDbtInd`: credit positive, debit negative.
fn amount(node: Node, currencies: &CurrencyTable, element: &'static str) -> Result<Money> {
    let amt = at(node, &["Amt"]).ok_or(CamtError::Missing(element))?;
    let value = amt.text().map(str::trim).unwrap_or("").to_owned();
    let invalid = || CamtError::Invalid { element, value: value.clone() };
    let currency: Currency = amt.attribute("Ccy").ok_or(CamtError::Missing("Ccy"))?.parse().map_err(|_| invalid())?;
    let places = currencies.get(&currency).and_then(|c| c.decimals()).ok_or_else(invalid)? as usize;
    let (whole, fraction) = value.split_once('.').unwrap_or((value.as_str(), ""));
    if whole.is_empty()
        || fraction.len() > places
        || !(whole.bytes().chain(fraction.bytes())).all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let units: i128 = format!("{whole}{fraction:0<places$}").parse().map_err(|_| invalid())?;
    let sign = match text(node, &["CdtDbtInd"]).as_deref() {
        Some("CRDT") => 1,
        Some("DBIT") => -1,
        _ => return Err(CamtError::Missing("CdtDbtInd")),
    };
    Money::new(sign * units, currency).ok_or_else(invalid)
}

/// A party: its name directly (001.02) or under `Pty` (001.08).
fn party_name(node: Node) -> Option<String> {
    text(node, &["Nm"]).or_else(|| text(node, &["Pty", "Nm"]))
}

fn detail(tx: Node, money_in: bool, bundled: bool, currencies: &CurrencyTable) -> Result<EntryDetail> {
    let parties = at(tx, &["RltdPties"]);
    let agents = at(tx, &["RltdAgts"]);
    // The other party: the debtor of money in, the creditor of money out.
    let (party, account, agent) =
        if money_in { ("Dbtr", "DbtrAcct", "DbtrAgt") } else { ("Cdtr", "CdtrAcct", "CdtrAgt") };
    let counterparty = Counterparty {
        name: parties.and_then(|p| at(p, &[party])).and_then(party_name),
        iban: parties.and_then(|p| text(p, &[account, "Id", "IBAN"])),
        bic: agents
            .and_then(|a| text(a, &[agent, "FinInstnId", "BICFI"]).or_else(|| text(a, &[agent, "FinInstnId", "BIC"]))),
    };
    let purpose: Vec<String> = at(tx, &["RmtInf"])
        .map(|rmt| {
            all(rmt, "Ustrd").filter_map(|n| n.text()).map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect()
        })
        .unwrap_or_default();
    let amount = match at(tx, &["AmtDtls", "TxAmt"]) {
        Some(node) if bundled => {
            let mut money = amount_of(node, currencies)?;
            if !money_in {
                money = -money;
            }
            Some(money)
        }
        _ => None,
    };
    Ok(EntryDetail {
        counterparty: (counterparty != Counterparty::default()).then_some(counterparty),
        purpose: (!purpose.is_empty()).then(|| purpose.join(" ")),
        end_to_end: text(tx, &["Refs", "EndToEndId"]).filter(|id| id != "NOTPROVIDED"),
        mandate: text(tx, &["Refs", "MndtId"]),
        amount,
    })
}

/// An amount without a sign of its own (`AmtDtls/TxAmt`).
fn amount_of(node: Node, currencies: &CurrencyTable) -> Result<Money> {
    let amt = at(node, &["Amt"]).ok_or(CamtError::Missing("TxAmt"))?;
    let value = amt.text().map(str::trim).unwrap_or("");
    let currency: Currency = amt
        .attribute("Ccy")
        .and_then(|c| c.parse().ok())
        .ok_or(CamtError::Invalid { element: "TxAmt", value: value.to_owned() })?;
    let places = currencies.get(&currency).and_then(|c| c.decimals()).unwrap_or(2) as usize;
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let units: i128 = format!("{whole}{fraction:0<places$}")
        .parse()
        .map_err(|_| CamtError::Invalid { element: "TxAmt", value: value.to_owned() })?;
    Money::new(units, currency).ok_or(CamtError::Invalid { element: "TxAmt", value: value.to_owned() })
}

fn entry(ntry: Node, currencies: &CurrencyTable) -> Result<Option<StatementEntry>> {
    // Only booked entries: `Sts` holds `BOOK` as text (001.02) or as `Cd` (001.08).
    let status = text(ntry, &["Sts", "Cd"]).or_else(|| text(ntry, &["Sts"]));
    if status.as_deref() != Some("BOOK") {
        return Ok(None);
    }
    let amount = amount(ntry, currencies, "Ntry/Amt")?;
    let booked = at(ntry, &["BookgDt"])
        .map(|n| date(n, "BookgDt"))
        .transpose()?
        .flatten()
        .ok_or(CamtError::Missing("BookgDt"))?;
    let value = at(ntry, &["ValDt"]).map(|n| date(n, "ValDt")).transpose()?.flatten();
    let code = at(ntry, &["BkTxCd", "Domn"]).and_then(|domain| {
        let family = at(domain, &["Fmly"])?;
        Some(format!("{}/{}/{}", text(domain, &["Cd"])?, text(family, &["Cd"])?, text(family, &["SubFmlyCd"])?))
    });
    let transactions: Vec<Node> = all(ntry, "NtryDtls").flat_map(|d| all(d, "TxDtls")).collect();
    let bundled = transactions.len() > 1;
    let details = transactions
        .into_iter()
        .map(|tx| detail(tx, amount.base_units() > 0, bundled, currencies))
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(StatementEntry {
        booked,
        value,
        amount,
        reversal: text(ntry, &["RvslInd"]).as_deref() == Some("true"),
        reference: text(ntry, &["AcctSvcrRef"]),
        code,
        details,
    }))
}

fn statement(stmt: Node, currencies: &CurrencyTable) -> Result<ReadStatement> {
    let number = text(stmt, &["Id"]).ok_or(CamtError::Missing("Stmt/Id"))?;
    let created = text(stmt, &["CreDtTm"]).ok_or(CamtError::Missing("CreDtTm"))?;
    let date = created
        .get(..10)
        .and_then(|day| day.parse().ok())
        .ok_or(CamtError::Invalid { element: "CreDtTm", value: created.clone() })?;
    let currency_text = text(stmt, &["Acct", "Ccy"]).ok_or(CamtError::Missing("Acct/Ccy"))?;
    let account = StatementAccount {
        iban: text(stmt, &["Acct", "Id", "IBAN"]).ok_or(CamtError::Missing("Acct/Id/IBAN"))?,
        currency: currency_text
            .parse()
            .map_err(|_| CamtError::Invalid { element: "Acct/Ccy", value: currency_text })?,
        holder: text(stmt, &["Acct", "Ownr", "Nm"]),
    };
    let period = at(stmt, &["FrToDt"]);
    let day =
        |path: &[&str]| -> Option<Date> { period.and_then(|p| text(p, path)).and_then(|v| v.get(..10)?.parse().ok()) };
    let (mut opening, mut closing) = (None, None);
    for bal in all(stmt, "Bal") {
        let kind = text(bal, &["Tp", "CdOrPrtry", "Cd"]);
        match kind.as_deref() {
            Some("PRCD" | "OPBD") if opening.is_none() => opening = Some(amount(bal, currencies, "Bal/Amt")?),
            Some("CLBD") => closing = Some(amount(bal, currencies, "Bal/Amt")?),
            _ => {}
        }
    }
    let entries =
        all(stmt, "Ntry").map(|n| entry(n, currencies)).collect::<Result<Vec<_>>>()?.into_iter().flatten().collect();
    Ok(ReadStatement {
        format: "iso20022-camt.053",
        kind: DocumentKind::BankStatement,
        number,
        date,
        statement: StatementData {
            account,
            from: day(&["FrDtTm"]).or_else(|| day(&["FrDt"])),
            to: day(&["ToDtTm"]).or_else(|| day(&["ToDt"])),
            opening,
            closing,
            entries,
        },
    })
}

/// Reads the statements of a camt.053 file – one per `Stmt`.
pub fn read(xml: &[u8], currencies: &CurrencyTable) -> Result<Vec<ReadStatement>> {
    let text = std::str::from_utf8(xml).map_err(|e| CamtError::Xml(e.to_string()))?;
    let doc = Xml::parse(text).map_err(|e| CamtError::Xml(e.to_string()))?;
    let message = at(doc.root_element(), &["BkToCstmrStmt"]).ok_or(CamtError::Syntax)?;
    all(message, "Stmt").map(|stmt| statement(stmt, currencies)).collect()
}
