// SPDX-License-Identifier: MIT
//! The German data transfer for tax audits (spec part 0010): the books of one owner for a period as CSV files with an
//! `index.xml` after the "Beschreibungsstandard für die Datenträgerüberlassung" (GoBD Rz. 167, § 147 Abs. 6 AO).

use std::{collections::BTreeMap, fmt::Write as _};

use crate::{
    AccountId, AccountKind, Chart, CurrencyTable, Date, Document, EventHash, EventSet, Names, PartyId, Payload,
    TaxTable,
};

/// A file of an export: its path within the export and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    /// The path, relative to the export's directory.
    pub path: String,
    /// The content.
    pub content: Vec<u8>,
}

/// What a column holds, as the Beschreibungsstandard types it.
enum Column {
    Text(&'static str, &'static str),
    Number(&'static str, &'static str, u32),
    Day(&'static str, &'static str),
}

/// A table: its file, name, description, columns and rows.
struct Table {
    file: &'static str,
    name: &'static str,
    description: &'static str,
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

/// The export of the books of `owner` for the bookings dated from `from` to `to`: `index.xml`, `konten.csv`
/// (accounts), `buchungen.csv` (the journal, one row per posting) and `belege.csv` (documents). Each document's
/// original is listed as `belege/<hash>`; the caller adds those files from its store. Names of accounts come from
/// `names`.
#[allow(clippy::too_many_arguments)]
pub fn export(
    books: &EventSet,
    chart: &Chart,
    names: &Names,
    currencies: &CurrencyTable,
    taxes: &TaxTable,
    owner: &PartyId,
    from: Date,
    to: Date,
) -> Vec<ExportFile> {
    let kinds = books.account_kinds(chart);
    let parents = books.account_parents(chart);
    let documents: BTreeMap<_, &Document> = books
        .ordered()
        .into_iter()
        .filter_map(|(_, event)| match &event.payload {
            Payload::DocumentRecorded { document } => Some((document.id.clone(), document)),
            _ => None,
        })
        .collect();

    // The number of decimals of the amounts: the most any currency of the export has.
    let decimals_of = |code: &crate::Currency| currencies.get(code).and_then(|d| d.decimals()).unwrap_or(0);
    let mut used_accounts: BTreeMap<AccountId, ()> = BTreeMap::new();
    let mut used_documents: BTreeMap<crate::DocumentId, ()> = BTreeMap::new();
    let mut accuracy = 2;
    let mut journal = Vec::new();
    for (hash, event) in books.ordered() {
        let (booking, reverses): (_, Option<&EventHash>) = match &event.payload {
            Payload::BookingRecorded { booking } => (booking, None),
            Payload::BookingReversed { booking, reverses } => (booking, Some(reverses)),
            _ => continue,
        };
        if booking.date < from || booking.date > to {
            continue;
        }
        let numbers: Vec<String> = booking
            .documents
            .iter()
            .map(|id| documents.get(id).and_then(|d| d.number.clone()).unwrap_or_default())
            .collect();
        let dates: Vec<String> =
            booking.documents.iter().filter_map(|id| documents.get(id)).map(|d| day(d.date)).collect();
        for (line, posting) in booking.postings.iter().enumerate().filter(|(_, p)| p.owner == *owner) {
            let units = posting.amount.base_units();
            let places = decimals_of(posting.amount.currency());
            let functional = posting.functional();
            let functional_places = decimals_of(functional.currency());
            let functional_units = functional.base_units();
            accuracy = accuracy.max(places).max(functional_places);
            used_accounts.insert(posting.account.clone(), ());
            used_documents.extend(booking.documents.iter().map(|id| (id.clone(), ())));
            let kind = kinds.get(&posting.account).copied();
            let rate = posting
                .tax
                .as_ref()
                .and_then(|code| taxes.get(code))
                .and_then(|d| d.rate)
                .map(|r| r.to_string().replace('.', ","));
            journal.push((
                places,
                functional_places,
                vec![
                    hash.to_string(),
                    (line + 1).to_string(),
                    day(booking.date),
                    timestamp(event.time.wall_ms),
                    event.time.device.as_str().to_owned(),
                    booking.text.clone(),
                    posting.account.to_string(),
                    names.get(posting.account.as_str()).unwrap_or_default().to_owned(),
                    kind.map(account_kind).unwrap_or("sonstige").to_owned(),
                    kind.map(account_type).unwrap_or("sonstige").to_owned(),
                    if units > 0 { units.to_string() } else { String::new() },
                    if units < 0 { (-units).to_string() } else { String::new() },
                    posting.amount.currency().to_string(),
                    posting.exchange.as_ref().map(|e| e.rate.to_string().replace('.', ",")).unwrap_or_default(),
                    if functional_units > 0 { functional_units.to_string() } else { String::new() },
                    if functional_units < 0 { (-functional_units).to_string() } else { String::new() },
                    functional.currency().to_string(),
                    posting.tax.as_ref().map(ToString::to_string).unwrap_or_default(),
                    rate.unwrap_or_default(),
                    booking.documents.iter().map(ToString::to_string).collect::<Vec<_>>().join(","),
                    numbers.join(","),
                    dates.join(","),
                    reverses.map(ToString::to_string).unwrap_or_default(),
                    posting.note.clone().unwrap_or_default(),
                ],
            ));
        }
    }
    // Amounts in a common number of places.
    let journal: Vec<Vec<String>> = journal
        .into_iter()
        .map(|(places, functional_places, mut row)| {
            for (column, places) in [(10, places), (11, places), (14, functional_places), (15, functional_places)] {
                if let Ok(units) = row[column].parse::<i128>() {
                    row[column] = amount(units * 10i128.pow(accuracy - places), accuracy);
                }
            }
            row
        })
        .collect();

    // Accounts: those posted on, and the accounts they refine.
    let mut accounts: Vec<AccountId> = Vec::new();
    for account in used_accounts.keys() {
        let mut next = Some(account);
        while let Some(id) = next {
            if !accounts.contains(id) {
                accounts.push(id.clone());
            }
            next = parents.get(id);
        }
    }
    accounts.sort();
    let in_chart = |id: &AccountId| chart.get(id).is_some();
    let account_rows = accounts
        .iter()
        .map(|id| {
            let kind = kinds.get(id).copied();
            vec![
                id.to_string(),
                names.get(id.as_str()).unwrap_or_default().to_owned(),
                kind.map(account_kind).unwrap_or("sonstige").to_owned(),
                kind.map(account_type).unwrap_or("sonstige").to_owned(),
                parents.get(id).map(ToString::to_string).unwrap_or_default(),
                if in_chart(id) { "Kontenrahmen" } else { "eigenes Konto" }.to_owned(),
            ]
        })
        .collect();

    let document_rows = used_documents
        .keys()
        .filter_map(|id| documents.get(id))
        .map(|d| {
            let invoice = d.invoice.as_deref();
            let party = invoice.map(|i| [&i.seller, &i.buyer]);
            let places = invoice.map_or(0, |i| decimals_of(&i.currency));
            let money = |m: Option<&crate::Money>| m.map(|m| amount(m.base_units(), places)).unwrap_or_default();
            vec![
                d.id.to_string(),
                format!("{:?}", d.kind).to_lowercase(),
                d.number.clone().unwrap_or_default(),
                day(d.date),
                day(d.received),
                format!("belege/{}", d.original.hash.to_string().trim_start_matches("sha256:")),
                d.original.hash.to_string(),
                d.original.media_type.clone(),
                d.original.name.clone().unwrap_or_default(),
                party.map(|[s, _]| s.name.clone()).unwrap_or_default(),
                party.and_then(|[s, _]| s.vat_id.clone()).unwrap_or_default(),
                party.and_then(|[s, _]| s.tax_number.clone()).unwrap_or_default(),
                party.map(|[_, b]| b.name.clone()).unwrap_or_default(),
                party.and_then(|[_, b]| b.vat_id.clone()).unwrap_or_default(),
                money(invoice.map(|i| &i.totals.net)),
                money(invoice.map(|i| &i.totals.tax)),
                money(invoice.map(|i| &i.totals.gross)),
                invoice.map(|i| i.currency.to_string()).unwrap_or_default(),
            ]
        })
        .collect();

    use Column::*;
    let tables = [
        Table {
            file: "konten.csv",
            name: "Konten",
            description: "Konten, auf die gebucht wurde, und die Konten, die sie verfeinern",
            columns: vec![
                Text("KONTO_ID", "Identität des Kontos (UUID)"),
                Text("BEZEICHNUNG", "Bezeichnung"),
                Text("KONTOART", "Kontoart (Aktiva, Passiva, Kapital, Aufwand, Ertrag, sonstige)"),
                Text("KONTOTYP", "Kontotyp (Bilanz, GuV, sonstige)"),
                Text("UEBERGEORDNET", "Konto, das dieses Konto verfeinert"),
                Text("HERKUNFT", "Kontenrahmen oder in den Büchern eröffnetes Konto"),
            ],
            rows: account_rows,
        },
        Table {
            file: "buchungen.csv",
            name: "Buchungen",
            description: "Journal: je Posten einer Buchung eine Zeile",
            columns: vec![
                Text("BUCHUNG_ID", "Identität der Buchung (Hash des Ereignisses)"),
                Number("POSTEN", "Nummer des Postens in der Buchung", 0),
                Day("BUCHUNGSDATUM", "Buchungsdatum"),
                Text("ERFASSUNGSZEITPUNKT", "Zeitpunkt der Erfassung (UTC)"),
                Text("ERFASST_AUF", "Gerät, auf dem erfasst wurde"),
                Text("BUCHUNGSTEXT", "Buchungstext"),
                Text("KONTO_ID", "Konto"),
                Text("KONTOBEZEICHNUNG", "Bezeichnung des Kontos"),
                Text("KONTOART", "Kontoart"),
                Text("KONTOTYP", "Kontotyp"),
                Number("SOLL", "Soll-Betrag", accuracy),
                Number("HABEN", "Haben-Betrag", accuracy),
                Text("WAEHRUNG", "Währung (ISO 4217)"),
                Text("KURS", "Bei Fremdwährung: Kurs, Einheiten der Hauswährung je Einheit der Währung"),
                Number("SOLL_HAUSWAEHRUNG", "Soll-Betrag in der Hauswährung", accuracy),
                Number("HABEN_HAUSWAEHRUNG", "Haben-Betrag in der Hauswährung", accuracy),
                Text("HAUSWAEHRUNG", "Hauswährung (ISO 4217)"),
                Text("STEUERSCHLUESSEL", "Steuerschlüssel"),
                Text("STEUERSATZ", "Umsatzsteuersatz in Prozent"),
                Text("BELEG_IDS", "Belege der Buchung"),
                Text("BELEGNUMMERN", "Belegnummern"),
                Text("BELEGDATEN", "Belegdaten (TT.MM.JJJJ)"),
                Text("STORNIERT", "Bei einem Storno: die stornierte Buchung"),
                Text("NOTIZ", "Vermerk zum Posten"),
            ],
            rows: journal,
        },
        Table {
            file: "belege.csv",
            name: "Belege",
            description: "Belege der exportierten Buchungen, mit ihren Originalen",
            columns: vec![
                Text("BELEG_ID", "Identität des Belegs (UUID)"),
                Text("ART", "Art des Belegs"),
                Text("NUMMER", "Belegnummer"),
                Day("BELEGDATUM", "Belegdatum"),
                Day("EINGANG", "Eingangstag"),
                Text("DATEI", "Original im Export"),
                Text("HASH", "SHA-256 des Originals"),
                Text("MEDIENTYP", "Medientyp des Originals"),
                Text("DATEINAME", "Dateiname des Originals"),
                Text("VERKAEUFER", "Verkäufer"),
                Text("VERKAEUFER_UST_ID", "USt-IdNr. des Verkäufers"),
                Text("VERKAEUFER_STEUERNUMMER", "Steuernummer des Verkäufers"),
                Text("KAEUFER", "Käufer"),
                Text("KAEUFER_UST_ID", "USt-IdNr. des Käufers"),
                Text("NETTO", "Nettobetrag"),
                Text("UMSATZSTEUER", "Umsatzsteuer"),
                Text("BRUTTO", "Bruttobetrag"),
                Text("BELEGWAEHRUNG", "Währung des Belegs"),
            ],
            rows: document_rows,
        },
    ];

    let mut files = vec![ExportFile { path: "index.xml".into(), content: index(&tables, from, to).into_bytes() }];
    for table in &tables {
        files.push(ExportFile { path: table.file.into(), content: csv(table).into_bytes() });
    }
    files
}

fn account_kind(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset => "Aktiva",
        AccountKind::Liability => "Passiva",
        AccountKind::Equity => "Kapital",
        AccountKind::Expense => "Aufwand",
        AccountKind::Income => "Ertrag",
        AccountKind::Memo => "sonstige",
    }
}

fn account_type(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset | AccountKind::Liability | AccountKind::Equity => "Bilanz",
        AccountKind::Expense | AccountKind::Income => "GuV",
        AccountKind::Memo => "sonstige",
    }
}

/// A date as `DD.MM.YYYY`, the default of the Beschreibungsstandard.
fn day(date: Date) -> String {
    format!("{:02}.{:02}.{:04}", date.day(), date.month(), date.year())
}

/// Base units as a decimal number with a comma: 1240 at 2 places is `12,40`.
fn amount(units: i128, places: u32) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let digits = units.unsigned_abs().to_string();
    if places == 0 {
        return format!("{sign}{digits}");
    }
    let places = places as usize;
    let digits = format!("{digits:0>width$}", width = places + 1);
    let (int, frac) = digits.split_at(digits.len() - places);
    format!("{sign}{int},{frac}")
}

/// Milliseconds since 1970 as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn timestamp(ms: u64) -> String {
    let (days, rest) = (ms / 86_400_000, ms % 86_400_000);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (h, min, s, milli) = (rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{milli:03}Z")
}

fn csv(table: &Table) -> String {
    let field = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    let mut out = String::new();
    let header: Vec<&str> = table
        .columns
        .iter()
        .map(|c| match c {
            Column::Text(name, _) | Column::Number(name, _, _) | Column::Day(name, _) => *name,
        })
        .collect();
    out.push_str(&header.join(";"));
    out.push_str("\r\n");
    for row in &table.rows {
        let cells: Vec<String> = row
            .iter()
            .zip(&table.columns)
            .map(|(value, column)| match column {
                Column::Text(..) => field(value),
                Column::Number(..) | Column::Day(..) => value.clone(),
            })
            .collect();
        out.push_str(&cells.join(";"));
        out.push_str("\r\n");
    }
    out
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn index(tables: &[Table], from: Date, to: Date) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<!DOCTYPE DataSet SYSTEM \"gdpdu-01-09-2004.dtd\">\r\n<DataSet>\r\n  \
         <Version>1.0</Version>\r\n  <DataSupplier>\r\n    <Name>librotenado</Name>\r\n    <Location />\r\n    \
         <Comment>Datenüberlassung nach GoBD Rz. 167</Comment>\r\n  </DataSupplier>\r\n  <Media>\r\n    \
         <Name>Buchhaltung</Name>\r\n",
    );
    for table in tables {
        let _ = write!(
            out,
            "    <Table>\r\n      <URL>{}</URL>\r\n      <Name>{}</Name>\r\n      <Description>{}</Description>\r\n      \
             <Validity>\r\n        <Range>\r\n          <From>{}</From>\r\n          <To>{}</To>\r\n        </Range>\r\n        \
             <Format>DD.MM.YYYY</Format>\r\n      </Validity>\r\n      <UTF8 />\r\n      \
             <DecimalSymbol>,</DecimalSymbol>\r\n      <DigitGroupingSymbol>.</DigitGroupingSymbol>\r\n      \
             <Range>\r\n        <From>2</From>\r\n      </Range>\r\n      <VariableLength>\r\n        \
             <ColumnDelimiter>;</ColumnDelimiter>\r\n        <RecordDelimiter>&#xD;&#xA;</RecordDelimiter>\r\n        \
             <TextEncapsulator>\"</TextEncapsulator>\r\n",
            table.file,
            table.name,
            xml(table.description),
            day(from),
            day(to)
        );
        for column in &table.columns {
            let (name, description, kind) = match column {
                Column::Text(n, d) => (n, d, "<AlphaNumeric />".to_owned()),
                Column::Number(n, d, accuracy) => {
                    (n, d, format!("<Numeric>\r\n            <Accuracy>{accuracy}</Accuracy>\r\n          </Numeric>"))
                }
                Column::Day(n, d) => {
                    (n, d, "<Date>\r\n            <Format>DD.MM.YYYY</Format>\r\n          </Date>".to_owned())
                }
            };
            let _ = write!(
                out,
                "        <VariableColumn>\r\n          <Name>{name}</Name>\r\n          <Description>{}</Description>\r\n          \
                 {kind}\r\n        </VariableColumn>\r\n",
                xml(description)
            );
        }
        out.push_str("      </VariableLength>\r\n    </Table>\r\n");
    }
    out.push_str("  </Media>\r\n</DataSet>\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_dates_and_times_in_the_german_form() {
        assert_eq!(amount(1240, 2), "12,40");
        assert_eq!(amount(-5, 2), "-0,05");
        assert_eq!(amount(100, 0), "100");
        assert_eq!(day("2026-03-05".parse().unwrap()), "05.03.2026");
        assert_eq!(timestamp(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(timestamp(1_793_177_071_143), "2026-10-28T08:44:31.143Z");
    }
}
