// SPDX-License-Identifier: MIT
//! Bank statements in MT940 and in the CSV files of banks, read through the one entry point that tells the formats
//! apart.

use librotenado::{CurrencyTable, ReadError, read_statements};

const MT940: &[u8] = include_bytes!("data/bank/statement.mt940");
const SPARKASSE: &[u8] = include_bytes!("data/bank/sparkasse-style.csv");
const ENGLISH: &[u8] = include_bytes!("data/bank/english-style.csv");
const CAMT: &[u8] = include_bytes!("data/camt/statement-001.08.xml");

fn amounts(statement: &librotenado::StatementData) -> Vec<i128> {
    statement.entries.iter().map(|e| e.amount.base_units()).collect()
}

#[test]
fn mt940_with_the_structured_field_of_german_banks() {
    let read = read_statements(MT940, &CurrencyTable::v0_1(), None).unwrap().remove(0);
    assert_eq!((read.format, read.number.as_deref()), ("swift-mt940", Some("00010/001")));
    let statement = &read.statement;
    assert_eq!(statement.account.iban, "DE89370400440532013000", "from BLZ and account number");
    assert_eq!(
        (statement.opening.as_ref().unwrap().base_units(), statement.closing.as_ref().unwrap().base_units()),
        (100_000, 333_250)
    );
    assert_eq!(amounts(statement), [238_000, -5_950, 1_200]);
    let first = &statement.entries[0];
    assert_eq!(
        (first.booked.to_string(), first.reference.as_deref(), first.code.as_deref()),
        ("2026-12-31".into(), Some("2026123100001"), Some("NTRF/166"))
    );
    let detail = &first.details[0];
    assert_eq!(
        (detail.end_to_end.as_deref(), detail.purpose.as_deref()),
        (Some("RE-2026-001"), Some("Rechnung RE-2026-001 Honorar Dezember"))
    );
    let party = detail.counterparty.as_ref().unwrap();
    assert_eq!(
        (party.name.as_deref(), party.iban.as_deref(), party.bic.as_deref()),
        (Some("Kunde A GmbH"), Some("DE02120300000000202051"), Some("BYLADEM1001"))
    );
    let debit = &statement.entries[1];
    assert_eq!(debit.booked.to_string(), "2027-01-02", "the entry date in the year of the value date");
    let detail = &debit.details[0];
    assert_eq!(
        (detail.end_to_end.as_deref(), detail.mandate.as_deref(), detail.purpose.as_deref()),
        (None, Some("TK-4711"), Some("Kundennummer 123 Januar"))
    );
    let reversal = &statement.entries[2];
    assert!(reversal.reversal, "RD: the reversal of a debit, money in");
    assert_eq!(reversal.details[0].purpose.as_deref(), Some("Rueckbuchung Gebuehr Bueromaterial Müller"), "ISO 8859-1");
    assert_eq!(
        (statement.from.unwrap().to_string(), statement.to.unwrap().to_string()),
        ("2026-12-31".into(), "2027-01-03".into())
    );
}

#[test]
fn csv_files_of_banks_are_read_by_their_column_names() {
    let read = read_statements(SPARKASSE, &CurrencyTable::v0_1(), None).unwrap().remove(0);
    assert_eq!(read.format, "csv");
    let statement = &read.statement;
    assert_eq!(statement.account.iban, "DE89370400440532013000", "from the column Auftragskonto");
    assert_eq!(amounts(statement), [238_000, -5_950], "in the order booked, the entry not yet booked left out");
    let debit = &statement.entries[1].details[0];
    assert_eq!((debit.mandate.as_deref(), debit.end_to_end.as_deref()), (Some("TK-4711"), None));
    assert_eq!(debit.counterparty.as_ref().unwrap().bic.as_deref(), Some("COBADEFFXXX"));
    assert_eq!(statement.entries[1].code.as_deref(), Some("FOLGELASTSCHRIFT"));

    // Another bank: lines before the headers, commas, English names, the account given by whoever reads it.
    let missing = read_statements(ENGLISH, &CurrencyTable::v0_1(), None);
    assert_eq!(missing, Err(ReadError::Missing("the account's IBAN")));
    let read = read_statements(ENGLISH, &CurrencyTable::v0_1(), Some("DE89 3704 0044 0532 0130 00")).unwrap().remove(0);
    assert_eq!(read.statement.account.iban, "DE89370400440532013000");
    assert_eq!(amounts(&read.statement), [238_000, -16_000]);
    let paper = &read.statement.entries[1].details[0];
    assert_eq!(paper.counterparty.as_ref().unwrap().name.as_deref(), Some("Büro Müller, Inh. M."));
    assert_eq!(paper.purpose.as_deref(), Some("Papier, Toner"));
}

#[test]
fn the_format_is_told_by_the_content() {
    let formats: Vec<&str> = [CAMT, MT940, SPARKASSE]
        .iter()
        .map(|bytes| read_statements(bytes, &CurrencyTable::v0_1(), None).unwrap()[0].format)
        .collect();
    assert_eq!(formats, ["iso20022-camt.053", "swift-mt940", "csv"]);
    assert_eq!(read_statements(b"just text\n", &CurrencyTable::v0_1(), None), Err(ReadError::Syntax("csv")));
}
