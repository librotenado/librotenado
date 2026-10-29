// SPDX-License-Identifier: MIT
//! Reading the CSV files banks offer for download (spec part 0005): the columns are found by their names in the line of
//! headers, wherever it stands, whatever the separator.

use crate::{
    Counterparty, Currency, CurrencyTable, Date, DocumentKind, EntryDetail, Money, ReadError, ReadStatement,
    StatementAccount, StatementData, StatementEntry,
};

/// The name of the format.
pub const FORMAT: &str = "csv";

type Result<T> = std::result::Result<T, ReadError>;

/// What a column holds, and the names it goes by – compared without case, spaces and the umlauts' spelling.
const COLUMNS: &[(Role, &[&str])] = &[
    (Role::Booked, &["buchungstag", "buchungsdatum", "datum", "bookingdate", "date"]),
    (Role::Value, &["valutadatum", "wertstellung", "valuta", "valuedate"]),
    (Role::Amount, &["betrag", "betrag(€)", "betrag(eur)", "umsatz", "amount", "amount(eur)"]),
    (Role::Currency, &["waehrung", "währung", "currency"]),
    (
        Role::Name,
        &[
            "beguenstigter/zahlungspflichtiger",
            "begünstigter/zahlungspflichtiger",
            "zahlungsempfänger*in",
            "zahlungspflichtige*r",
            "auftraggeber/empfänger",
            "empfänger",
            "name",
            "partnername",
            "payee",
        ],
    ),
    (Role::Iban, &["kontonummer/iban", "iban", "partneriban", "gegenkonto"]),
    (Role::Bic, &["bic(swift-code)", "bic", "bankleitzahl/bic"]),
    (Role::Purpose, &["verwendungszweck", "paymentreference", "purpose", "reference"]),
    (Role::Code, &["buchungstext", "umsatzart", "transactiontype", "type"]),
    (Role::EndToEnd, &["kundenreferenz(end-to-end)", "kundenreferenz", "end-to-endreferenz"]),
    (Role::Mandate, &["mandatsreferenz"]),
    (Role::Account, &["auftragskonto"]),
    (Role::Status, &["info", "status"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Booked,
    Value,
    Amount,
    Currency,
    Name,
    Iban,
    Bic,
    Purpose,
    Code,
    EndToEnd,
    Mandate,
    Account,
    Status,
}

/// A name as compared: lower case, without spaces and quotes.
fn key(name: &str) -> String {
    name.trim().trim_matches('"').to_lowercase().chars().filter(|c| !c.is_whitespace()).collect()
}

/// The fields of a line, split by `separator`, quotes respected (`""` is a quote).
fn split(line: &str, separator: char) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().expect("a field").push('"');
            }
            '"' => quoted = !quoted,
            c if c == separator && !quoted => fields.push(String::new()),
            c => fields.last_mut().expect("a field").push(c),
        }
    }
    fields.into_iter().map(|f| f.trim().to_owned()).collect()
}

/// A date: `31.10.2026`, `31.10.26` or `2026-10-31`.
fn date(value: &str) -> Result<Date> {
    let invalid = || ReadError::Invalid { element: "date", value: value.to_owned() };
    if let Ok(date) = value.parse() {
        return Ok(date);
    }
    let mut parts = value.split('.');
    let (day, month, year) = (parts.next(), parts.next(), parts.next());
    let (Some(day), Some(month), Some(year)) = (day, month, year) else { return Err(invalid()) };
    let year: u16 = year.parse().map_err(|_| invalid())?;
    let year = if year < 100 { 2000 + year } else { year };
    Date::new(year, month.parse().map_err(|_| invalid())?, day.parse().map_err(|_| invalid())?).ok_or_else(invalid)
}

/// An amount: `-1.234,56`, `1234,5` or `-1,234.56` – the last of `,` and `.` is the decimal mark, when followed by
/// at most the places of the currency.
fn amount(value: &str, currency: &Currency, currencies: &CurrencyTable) -> Result<Money> {
    let invalid = || ReadError::Invalid { element: "amount", value: value.to_owned() };
    let places = currencies.get(currency).and_then(|c| c.decimals()).ok_or_else(invalid)? as usize;
    let text: String = value.chars().filter(|c| !c.is_whitespace() && *c != '€').collect();
    let (negative, digits) = text.strip_prefix('-').map_or((false, text.as_str()), |rest| (true, rest));
    let digits = digits.strip_prefix('+').unwrap_or(digits);
    let mark = digits.rfind([',', '.']).filter(|at| digits.len() - at - 1 <= places && digits.len() - at - 1 > 0);
    let (whole, fraction) = match mark {
        Some(at) => (digits[..at].replace([',', '.'], ""), &digits[at + 1..]),
        None => (digits.replace([',', '.'], ""), ""),
    };
    if whole.is_empty() || !whole.bytes().chain(fraction.bytes()).all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    let units: i128 = format!("{whole}{fraction:0<places$}").parse().map_err(|_| invalid())?;
    Money::new(if negative { -units } else { units }, currency.clone()).ok_or_else(invalid)
}

/// Reads the entries of a bank's CSV file as one statement. `account`, the account's IBAN, is needed where the file
/// does not name it in a column.
pub fn read(bytes: &[u8], currencies: &CurrencyTable, account: Option<&str>) -> Result<Vec<ReadStatement>> {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.trim_start_matches('\u{feff}').to_owned(),
        // Windows-1252 in its common part, ISO 8859-1.
        Err(_) => bytes.iter().map(|b| char::from(*b)).collect(),
    };
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    // The line of headers: the first that names a day and an amount.
    let (header_at, separator, columns) = lines
        .iter()
        .enumerate()
        .find_map(|(i, line)| {
            [';', ',', '\t'].into_iter().find_map(|separator| {
                let names: Vec<String> = split(line, separator).iter().map(|n| key(n)).collect();
                let column = |role: Role| -> Option<usize> {
                    let aliases = COLUMNS.iter().find(|(r, _)| *r == role)?.1;
                    aliases.iter().find_map(|alias| names.iter().position(|n| n == alias))
                };
                let roles: Vec<(Role, usize)> =
                    COLUMNS.iter().filter_map(|(role, _)| Some((*role, column(*role)?))).collect();
                let has = |role| roles.iter().any(|(r, _)| *r == role);
                (has(Role::Booked) && has(Role::Amount)).then_some((i, separator, roles))
            })
        })
        .ok_or(ReadError::Syntax(FORMAT))?;
    let column = |role: Role| columns.iter().find(|(r, _)| *r == role).map(|(_, i)| *i);
    let mut entries = Vec::new();
    let mut iban = account.map(|a| a.replace(' ', ""));
    let mut currency_seen: Option<Currency> = None;
    for line in lines.iter().skip(header_at + 1).filter(|l| !l.trim().is_empty()) {
        let fields = split(line, separator);
        let get = |role: Role| column(role).and_then(|i| fields.get(i)).map(String::as_str).filter(|v| !v.is_empty());
        let Some(booked) = get(Role::Booked) else { continue };
        // Entries a bank marks as not yet booked are left out.
        if get(Role::Status).is_some_and(|s| s.to_lowercase().contains("vorgemerkt") || s.to_lowercase() == "pending") {
            continue;
        }
        let currency: Currency = match get(Role::Currency) {
            Some(code) => {
                code.parse().map_err(|_| ReadError::Invalid { element: "currency", value: code.to_owned() })?
            }
            None => "EUR".parse().expect("a currency"),
        };
        if iban.is_none() {
            iban = get(Role::Account).map(|a| a.replace(' ', ""));
        }
        let counterparty = Counterparty {
            name: get(Role::Name).map(str::to_owned),
            iban: get(Role::Iban).map(|i| i.replace(' ', "")),
            bic: get(Role::Bic).map(str::to_owned),
        };
        let detail = EntryDetail {
            counterparty: (counterparty != Counterparty::default()).then_some(counterparty),
            purpose: get(Role::Purpose).map(str::to_owned),
            end_to_end: get(Role::EndToEnd).filter(|e| *e != "NOTPROVIDED").map(str::to_owned),
            mandate: get(Role::Mandate).map(str::to_owned),
            amount: None,
        };
        let amount_text = get(Role::Amount).ok_or(ReadError::Missing("amount"))?;
        entries.push(StatementEntry {
            booked: date(booked)?,
            value: get(Role::Value).map(date).transpose()?,
            amount: amount(amount_text, &currency, currencies)?,
            reversal: false,
            reference: None,
            code: get(Role::Code).map(str::to_owned),
            details: if detail == EntryDetail::default() { Vec::new() } else { vec![detail] },
        });
        currency_seen.get_or_insert(currency);
    }
    // Banks list the newest first; a statement lists in the order booked.
    entries.sort_by_key(|e| e.booked);
    let iban = iban.ok_or(ReadError::Missing("the account's IBAN"))?;
    let currency = currency_seen.unwrap_or_else(|| "EUR".parse().expect("a currency"));
    let (from, to) = (entries.iter().map(|e| e.booked).min(), entries.iter().map(|e| e.booked).max());
    let date = to.ok_or(ReadError::Missing("entries"))?;
    Ok(vec![ReadStatement {
        format: FORMAT,
        kind: DocumentKind::BankStatement,
        number: None,
        date,
        statement: StatementData {
            account: StatementAccount { iban, currency, holder: None },
            from,
            to,
            opening: None,
            closing: None,
            entries,
        },
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_in_both_notations() {
        let (eur, table) = ("EUR".parse().unwrap(), CurrencyTable::v0_1());
        let units = |text: &str| amount(text, &eur, &table).map(|m| m.base_units());
        assert_eq!(units("-1.234,56"), Ok(-123_456));
        assert_eq!(units("1,234.56"), Ok(123_456));
        assert_eq!(units("59,5"), Ok(5_950));
        assert_eq!(units("1.000"), Ok(100_000), "a thousand, not one");
        assert_eq!(units("+12"), Ok(1_200));
        assert!(units("12,345").is_ok_and(|u| u == 1_234_500), "three places are no cents");
        assert!(units("abc").is_err());
    }

    #[test]
    fn quoted_fields_keep_their_separators() {
        assert_eq!(split(r#""a;b";"c ""d""";e"#, ';'), ["a;b", r#"c "d""#, "e"]);
        assert_eq!(date("31.10.26").unwrap().to_string(), "2026-10-31");
    }
}
