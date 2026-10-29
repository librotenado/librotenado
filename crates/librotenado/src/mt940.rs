// SPDX-License-Identifier: MIT
//! Reading bank statements in SWIFT MT940 (spec part 0005), with the structured field `:86:` German banks write.

use crate::{
    Counterparty, Currency, CurrencyTable, Date, DocumentKind, EntryDetail, Money, ReadError, ReadStatement,
    StatementAccount, StatementData, StatementEntry,
};

/// The name of the format.
pub const FORMAT: &str = "swift-mt940";

type Result<T> = std::result::Result<T, ReadError>;

/// The text of a file: UTF-8, or else ISO 8859-1, in which German banks often write MT940.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|b| char::from(*b)).collect(),
    }
}

/// The fields of a file, as (tag, content): a field begins with `:tag:` at the start of a line and runs to the next.
fn fields(text: &str) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let tag = line
            .strip_prefix(':')
            .and_then(|rest| rest.split_once(':'))
            .filter(|(tag, _)| (2..=3).contains(&tag.len()) && tag.chars().all(|c| c.is_ascii_alphanumeric()));
        match tag {
            Some((tag, content)) => fields.push((tag.to_owned(), content.to_owned())),
            None if line == "-" || line.starts_with('{') || line.starts_with('}') => {}
            None => {
                if let Some((_, content)) = fields.last_mut() {
                    content.push('\n');
                    content.push_str(line);
                }
            }
        }
    }
    fields
}

/// A date written `YYMMDD`, in the years 2000 to 2099.
fn date(value: &str, element: &'static str) -> Result<Date> {
    let invalid = || ReadError::Invalid { element, value: value.to_owned() };
    let digits = value.get(..6).filter(|d| d.bytes().all(|b| b.is_ascii_digit())).ok_or_else(invalid)?;
    let (year, month, day) = (&digits[..2], &digits[2..4], &digits[4..]);
    Date::new(
        2000 + year.parse::<u16>().map_err(|_| invalid())?,
        month.parse().map_err(|_| invalid())?,
        day.parse().map_err(|_| invalid())?,
    )
    .ok_or_else(invalid)
}

/// An amount written with a decimal comma (`1190,00`), in base units of `currency`.
fn units(value: &str, currency: &Currency, currencies: &CurrencyTable, element: &'static str) -> Result<i128> {
    let invalid = || ReadError::Invalid { element, value: value.to_owned() };
    let places = currencies.get(currency).and_then(|c| c.decimals()).ok_or_else(invalid)? as usize;
    let (whole, fraction) = value.split_once(',').ok_or_else(invalid)?;
    if whole.is_empty() || fraction.len() > places || !whole.bytes().chain(fraction.bytes()).all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    format!("{whole}{fraction:0<places$}").parse().map_err(|_| invalid())
}

/// A balance (`:60F:`, `:62F:` …): `C` or `D`, the date, the currency, the amount.
fn balance(value: &str, currencies: &CurrencyTable) -> Result<(Date, Money)> {
    let invalid = || ReadError::Invalid { element: "balance", value: value.to_owned() };
    let sign = match value.get(..1) {
        Some("C") => 1,
        Some("D") => -1,
        _ => return Err(invalid()),
    };
    let day = date(value.get(1..7).ok_or_else(invalid)?, "balance")?;
    let currency: Currency = value.get(7..10).ok_or_else(invalid)?.parse().map_err(|_| invalid())?;
    let units = units(value.get(10..).ok_or_else(invalid)?.trim(), &currency, currencies, "balance")?;
    Ok((day, Money::new(sign * units, currency).ok_or_else(invalid)?))
}

/// The IBAN of a German account given as `BLZ/Kontonummer` (ISO 13616, check digits after ISO 7064 MOD 97-10).
fn german_iban(account: &str) -> Option<String> {
    let (bank, number) = account.split_once('/')?;
    let number = number.split(|c: char| !c.is_ascii_digit()).next()?;
    if bank.len() != 8 || !bank.bytes().all(|b| b.is_ascii_digit()) || number.is_empty() || number.len() > 10 {
        return None;
    }
    let bban = format!("{bank}{number:0>10}");
    // The BBAN, then D = 13, E = 14 and 00, modulo 97.
    let remainder = format!("{bban}131400").bytes().fold(0u32, |r, b| (r * 10 + u32::from(b - b'0')) % 97);
    Some(format!("DE{:02}{bban}", 98 - remainder))
}

/// The sub-fields of a structured `:86:`: the code of the business transaction (GVC) and `?nn` → text.
fn structured(info: &str) -> Option<(String, Vec<(String, String)>)> {
    let info: String = info.replace(['\n', '\r'], "");
    let code = info.get(..3).filter(|c| c.bytes().all(|b| b.is_ascii_digit()))?.to_owned();
    let separator = info.get(3..4)?.chars().next()?;
    let mut parts = Vec::new();
    for part in info[3..].split(separator).skip(1) {
        if part.len() >= 2 {
            parts.push((part[..2].to_owned(), part[2..].to_owned()));
        }
    }
    Some((code, parts))
}

/// The SEPA references of remittance text (`EREF+…SVWZ+…`): end-to-end, mandate, purpose.
fn sepa(text: &str) -> (Option<String>, Option<String>, Option<String>) {
    const KEYS: [&str; 10] = ["EREF+", "KREF+", "MREF+", "CRED+", "DEBT+", "COAM+", "OAMT+", "SVWZ+", "ABWA+", "ABWE+"];
    let mut marks: Vec<(usize, &str)> = KEYS.iter().filter_map(|key| text.find(key).map(|at| (at, *key))).collect();
    marks.sort();
    let value = |key: &str| {
        let i = marks.iter().position(|(_, k)| *k == key)?;
        let start = marks[i].0 + key.len();
        let end = marks.get(i + 1).map_or(text.len(), |(at, _)| *at);
        Some(text[start..end].trim().to_owned()).filter(|v| !v.is_empty())
    };
    if marks.is_empty() {
        return (None, None, Some(text.trim().to_owned()).filter(|t| !t.is_empty()));
    }
    (value("EREF+").filter(|e| e != "NOTPROVIDED"), value("MREF+"), value("SVWZ+"))
}

/// One line `:61:` and its `:86:`.
fn entry(line: &str, info: Option<&str>, currency: &Currency, currencies: &CurrencyTable) -> Result<StatementEntry> {
    let invalid = || ReadError::Invalid { element: ":61:", value: line.to_owned() };
    // The first line; a second holds supplementary details the model does not carry.
    let first = line.split_once('\n').map_or(line, |(first, _)| first);
    let value_date = date(first, ":61:")?;
    let mut rest = &first[6..];
    // An optional entry date MMDD, in the year of the value date – or the one next to it across the turn of a year.
    let mut booked = value_date;
    if rest.len() >= 4 && rest[..4].bytes().all(|b| b.is_ascii_digit()) {
        let (month, day): (u8, u8) =
            (rest[..2].parse().map_err(|_| invalid())?, rest[2..4].parse().map_err(|_| invalid())?);
        let year = match (value_date.month(), month) {
            (1, 12) => value_date.year() - 1,
            (12, 1) => value_date.year() + 1,
            _ => value_date.year(),
        };
        booked = Date::new(year, month, day).ok_or_else(invalid)?;
        rest = &rest[4..];
    }
    // C, D, or RC, RD: the reversal of a credit is a debit, the reversal of a debit a credit.
    let (sign, reversal, mark) = if let Some(r) = rest.strip_prefix("RC") {
        (-1, true, r)
    } else if let Some(r) = rest.strip_prefix("RD") {
        (1, true, r)
    } else if let Some(r) = rest.strip_prefix('C') {
        (1, false, r)
    } else if let Some(r) = rest.strip_prefix('D') {
        (-1, false, r)
    } else {
        return Err(invalid());
    };
    // An optional third letter of the currency, then the amount.
    let mark = mark.strip_prefix(|c: char| c.is_ascii_alphabetic()).unwrap_or(mark);
    let end = mark.find(|c: char| !(c.is_ascii_digit() || c == ',')).ok_or_else(invalid)?;
    let units = units(&mark[..end], currency, currencies, ":61:")?;
    let after = &mark[end..];
    let kind = after.get(..4).ok_or_else(invalid)?.to_owned();
    let references = &after[4..];
    let bank_reference = references.split_once("//").map(|(_, bank)| bank.trim().to_owned()).filter(|r| !r.is_empty());

    let mut detail = EntryDetail::default();
    let mut code = kind;
    if let Some(info) = info {
        match structured(info) {
            Some((gvc, parts)) => {
                code = format!("{code}/{gvc}");
                let joined = |range: &dyn Fn(u32) -> bool| -> String {
                    parts.iter().filter(|(n, _)| n.parse::<u32>().is_ok_and(range)).map(|(_, t)| t.as_str()).collect()
                };
                let remittance = joined(&|n| (20..=29).contains(&n) || (60..=63).contains(&n));
                let (end_to_end, mandate, purpose) = sepa(&remittance);
                let name = joined(&|n| (32..=33).contains(&n));
                let iban = joined(&|n| n == 31);
                let bic = joined(&|n| n == 30);
                let counterparty = Counterparty {
                    name: Some(name.trim().to_owned()).filter(|n| !n.is_empty()),
                    iban: Some(iban.trim().to_owned()).filter(|i| !i.is_empty()),
                    bic: Some(bic.trim().to_owned()).filter(|b| !b.is_empty()),
                };
                detail = EntryDetail {
                    counterparty: (counterparty != Counterparty::default()).then_some(counterparty),
                    purpose,
                    end_to_end,
                    mandate,
                    amount: None,
                };
            }
            None => detail.purpose = Some(info.replace('\n', " ").trim().to_owned()).filter(|p| !p.is_empty()),
        }
    }
    Ok(StatementEntry {
        booked,
        value: Some(value_date),
        amount: Money::new(sign * units, currency.clone()).ok_or_else(invalid)?,
        reversal,
        reference: bank_reference,
        code: Some(code),
        details: if detail == EntryDetail::default() { Vec::new() } else { vec![detail] },
    })
}

/// Reads the statements of an MT940 file – one per `:20:`.
pub fn read(bytes: &[u8], currencies: &CurrencyTable) -> Result<Vec<ReadStatement>> {
    let text = decode(bytes);
    let fields = fields(&text);
    if !fields.iter().any(|(tag, _)| tag == "20") {
        return Err(ReadError::Syntax(FORMAT));
    }
    let mut statements = Vec::new();
    let mut groups: Vec<Vec<(String, String)>> = Vec::new();
    for field in fields {
        if field.0 == "20" || groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().expect("a group").push(field);
    }
    for group in groups {
        let get = |tag: &str| group.iter().find(|(t, _)| t == tag).map(|(_, c)| c.trim());
        let account_text = get("25").ok_or(ReadError::Missing(":25:"))?;
        let opening = group
            .iter()
            .find(|(t, _)| t == "60F" || t == "60M")
            .map(|(_, c)| balance(c.trim(), currencies))
            .transpose()?;
        let closing = group
            .iter()
            .find(|(t, _)| t == "62F" || t == "62M")
            .map(|(_, c)| balance(c.trim(), currencies))
            .transpose()?;
        let currency = opening
            .as_ref()
            .or(closing.as_ref())
            .map(|(_, m)| m.currency().clone())
            .ok_or(ReadError::Missing(":60F:"))?;
        let compact: String = account_text.chars().filter(|c| !c.is_whitespace()).collect();
        let iban = if compact.len() > 4 && compact[..2].chars().all(|c| c.is_ascii_uppercase()) {
            compact.split('/').next().unwrap_or(&compact).to_owned()
        } else {
            german_iban(&compact).ok_or(ReadError::Invalid { element: ":25:", value: account_text.to_owned() })?
        };
        let mut entries = Vec::new();
        for (i, (tag, content)) in group.iter().enumerate() {
            if tag == "61" {
                let info = group.get(i + 1).filter(|(t, _)| t == "86").map(|(_, c)| c.as_str());
                entries.push(entry(content.trim(), info, &currency, currencies)?);
            }
        }
        let first = opening.as_ref().map(|(day, _)| *day);
        let last = closing.as_ref().map(|(day, _)| *day).or_else(|| entries.iter().map(|e| e.booked).max());
        let date = last.or(first).ok_or(ReadError::Missing(":62F:"))?;
        statements.push(ReadStatement {
            format: FORMAT,
            kind: DocumentKind::BankStatement,
            number: get("28C").or(get("28")).map(str::to_owned),
            date,
            statement: StatementData {
                account: StatementAccount { iban, currency, holder: None },
                // The opening balance is the closing balance of the day before; the statement covers what follows.
                from: entries.iter().map(|e| e.booked).min().or(first),
                to: last,
                opening: opening.map(|(_, money)| money),
                closing: closing.map(|(_, money)| money),
                entries,
            },
        });
    }
    Ok(statements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_german_account_number_becomes_its_iban() {
        // The example of the Deutsche Bundesbank: BLZ 37040044, account 532013000.
        assert_eq!(german_iban("37040044/532013000").as_deref(), Some("DE89370400440532013000"));
        assert_eq!(german_iban("3704004/1"), None);
    }

    #[test]
    fn sepa_references_are_found_in_the_remittance_text() {
        let (e2e, mandate, purpose) = sepa("EREF+RE-2026-001MREF+TK-4711CRED+DE98ZZZ09999999999SVWZ+Kundennummer 123");
        assert_eq!(
            (e2e.as_deref(), mandate.as_deref(), purpose.as_deref()),
            (Some("RE-2026-001"), Some("TK-4711"), Some("Kundennummer 123"))
        );
        assert_eq!(sepa("Miete Oktober"), (None, None, Some("Miete Oktober".into())));
        assert_eq!(sepa("EREF+NOTPROVIDED SVWZ+x").0, None);
    }
}
