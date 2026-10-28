// SPDX-License-Identifier: MIT
//! Translation of the test invoices of the XRechnung test suite (KoSIT) and of CEN into the model.

use librotenado::{
    CurrencyTable, DocumentKind, VatCategory,
    en16931::{self, TranslatedInvoice},
};

fn read(name: &str) -> TranslatedInvoice {
    let path = format!("{}/tests/data/en16931/{name}", env!("CARGO_MANIFEST_DIR"));
    en16931::read(&std::fs::read(&path).unwrap(), &CurrencyTable::v0_1()).unwrap()
}

/// The data must not depend on the syntax: UBL and CII of the same test case give the same invoice.
#[test]
fn ubl_and_cii_give_the_same_invoice() {
    for case in ["01.01a", "01.21a", "03.01a"] {
        let (ubl, cii) = (read(&format!("{case}-INVOICE_ubl.xml")), read(&format!("{case}-INVOICE_uncefact.xml")));
        assert_eq!((ubl.format, cii.format), ("en16931-ubl", "en16931-cii"));
        assert_eq!((&ubl.number, ubl.date, ubl.kind), (&cii.number, cii.date, cii.kind), "{case}");
        assert_eq!(ubl.invoice.totals, cii.invoice.totals, "{case}");
        // The test suite words the exemption reason (BT-120) differently in its two syntaxes.
        let without_reason = |t: &TranslatedInvoice| {
            t.invoice.taxes.iter().map(|b| (b.category, b.rate, b.base.clone(), b.tax.clone())).collect::<Vec<_>>()
        };
        assert_eq!(without_reason(&ubl), without_reason(&cii), "{case}");
        // The seller's VAT identifier differs between the two syntaxes of the test suite itself; both must be read.
        assert!(ubl.invoice.seller.vat_id.is_some() && cii.invoice.seller.vat_id.is_some(), "{case}");
        assert_eq!(ubl.invoice.lines.len(), cii.invoice.lines.len(), "{case}");
    }
}

#[test]
fn totals_agree_with_the_breakdown() {
    for name in [
        "01.01a-INVOICE_ubl.xml",
        "01.21a-INVOICE_ubl.xml",
        "03.01a-INVOICE_ubl.xml",
        "03.01a-INVOICE_uncefact.xml",
        "03.06a-INVOICE_ubl.xml",
        "ubl-tc434-creditnote1.xml",
    ] {
        let invoice = read(name).invoice;
        let sum = |f: fn(&librotenado::VatBreakdown) -> i128| invoice.taxes.iter().map(f).sum::<i128>();
        assert_eq!(sum(|t| t.base.base_units()), invoice.totals.net.base_units(), "{name}: net");
        assert_eq!(sum(|t| t.tax.base_units()), invoice.totals.tax.base_units(), "{name}: VAT");
        assert_eq!(
            invoice.totals.net.base_units() + invoice.totals.tax.base_units(),
            invoice.totals.gross.base_units(),
            "{name}: gross"
        );
    }
}

#[test]
fn categories_and_rates() {
    let categories =
        |name| read(name).invoice.taxes.iter().map(|t| (t.category, t.rate.map(|r| r.to_string()))).collect::<Vec<_>>();
    assert_eq!(categories("01.01a-INVOICE_ubl.xml"), [(VatCategory::Standard, Some("7".into()))]);
    assert_eq!(categories("01.21a-INVOICE_ubl.xml"), [(VatCategory::ReverseCharge, None)]);
    let several = categories("03.01a-INVOICE_ubl.xml");
    assert!(several.contains(&(VatCategory::Standard, Some("19".into()))));
    assert!(several.contains(&(VatCategory::Standard, Some("7".into()))));
    assert!(categories("03.06a-INVOICE_ubl.xml").iter().any(|(c, _)| *c == VatCategory::ZeroRated));
    let credit_note = read("ubl-tc434-creditnote1.xml");
    assert_eq!(credit_note.kind, DocumentKind::CreditNote);
    assert!(credit_note.invoice.taxes.iter().all(|t| t.category == VatCategory::Exempt));
}

#[test]
fn rejects_what_is_no_invoice() {
    let currencies = CurrencyTable::v0_1();
    assert!(en16931::read(b"<x/>", &currencies).is_err());
    assert!(en16931::read(b"not xml", &currencies).is_err());
}

/// A PDF with an invoice attached, as ZUGFeRD and Factur-X carry it.
#[cfg(feature = "pdf")]
#[test]
fn reads_the_invoice_attached_to_a_pdf() {
    use lopdf::{Document, Object, Stream, dictionary};
    let xml = std::fs::read(format!("{}/tests/data/en16931/03.01a-INVOICE_uncefact.xml", env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    let mut pdf = Document::with_version("1.7");
    let pages = pdf.new_object_id();
    pdf.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![], "Count" => 0 }));
    let file = pdf.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, xml));
    let spec = pdf.add_object(dictionary! { "Type" => "Filespec", "F" => Object::string_literal("factur-x.xml"), "EF" => dictionary! { "F" => file } });
    let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages, "AF" => vec![spec.into()] });
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    let invoice = en16931::read_pdf(&bytes, &CurrencyTable::v0_1()).unwrap();
    assert_eq!(invoice, read("03.01a-INVOICE_uncefact.xml"));
}

mod proposals {
    use super::*;
    use librotenado::{
        Chart, Clock, Document, DocumentKind, Event, EventSet, InvoiceSide, Original, PartyId, Payload,
        ProposalAccounts, ReportMapping, TaxTable, propose_booking,
    };

    const OWNER: &str = "0192f1e4-0000-7000-8000-0000000000a1";

    fn document(name: &str) -> Document {
        let translated = read(name);
        Document {
            id: "0192f1e4-0000-7000-8000-0000000000d1".parse().unwrap(),
            kind: translated.kind,
            number: Some(translated.number),
            date: translated.date,
            received: translated.date,
            original: Original::of(name.as_bytes(), "application/xml"),
            invoice: Some(Box::new(translated.invoice)),
            statement: None,
        }
    }

    /// The VAT return of the month of the document, after booking the proposal.
    fn vat_return(document: &Document, side: InvoiceSide) -> std::collections::BTreeMap<String, i128> {
        let owner: PartyId = OWNER.parse().unwrap();
        let booking = propose_booking(document, side, &owner, &ProposalAccounts::v0_1(side)).unwrap();
        booking.validate().expect("a proposal balances");
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let date = booking.date;
        set.insert(Event { time: clock.now(1), seq: 1, prev: None, payload: Payload::BookingRecorded { booking } });
        set.period_report(&ReportMapping::de_ustva_2026(), &TaxTable::de(), &Chart::v0_1(), &owner, date, date).unwrap()
    }

    #[test]
    fn a_purchase_at_two_rates() {
        let doc = document("03.01a-INVOICE_ubl.xml");
        let invoice = doc.invoice.clone().unwrap();
        let report = vat_return(&doc, InvoiceSide::Purchase);
        assert_eq!(report.get("66").copied(), Some(invoice.totals.tax.base_units()), "all VAT deductible");
    }

    #[test]
    fn a_purchase_under_reverse_charge() {
        let doc = document("01.21a-INVOICE_ubl.xml");
        let base = doc.invoice.as_ref().unwrap().totals.net.base_units();
        let report = vat_return(&doc, InvoiceSide::Purchase);
        let owed = (base * 19 + 50) / 100;
        assert_eq!((report.get("84"), report.get("85"), report.get("67")), (Some(&base), Some(&owed), Some(&owed)));
    }

    #[test]
    fn a_sale_at_seven_per_cent() {
        let doc = document("01.01a-INVOICE_ubl.xml");
        let net = doc.invoice.as_ref().unwrap().totals.net.base_units();
        assert_eq!(vat_return(&doc, InvoiceSide::Sale).get("86"), Some(&net));
    }

    #[test]
    fn a_credit_note_books_the_other_way() {
        let doc = document("ubl-tc434-creditnote1.xml");
        assert_eq!(doc.kind, DocumentKind::CreditNote);
        let owner: PartyId = OWNER.parse().unwrap();
        let accounts = ProposalAccounts::v0_1(InvoiceSide::Purchase);
        let booking = propose_booking(&doc, InvoiceSide::Purchase, &owner, &accounts).unwrap();
        booking.validate().unwrap();
        let on_supplier: i128 =
            booking.postings.iter().filter(|p| p.account == accounts.party).map(|p| p.amount.base_units()).sum();
        assert!(on_supplier > 0, "a credit note from a supplier reduces what is owed");
    }
}

mod data_transfer {
    use super::*;
    use librotenado::{
        Chart, Clock, Document, Event, EventSet, InvoiceSide, Names, Original, PartyId, Payload, ProposalAccounts,
        TaxTable, gobd, propose_booking,
    };

    /// Books with one purchase invoice and its booking, exported for a tax audit.
    pub(crate) fn exported() -> Vec<gobd::ExportFile> {
        let owner: PartyId = "0192f1e4-0000-7000-8000-0000000000a1".parse().unwrap();
        let translated = read("03.01a-INVOICE_ubl.xml");
        let document = Document {
            id: "0192f1e4-0000-7000-8000-0000000000d1".parse().unwrap(),
            kind: translated.kind,
            number: Some(translated.number.clone()),
            date: translated.date,
            received: translated.date,
            original: Original::of(b"the XML", "application/xml"),
            invoice: Some(Box::new(translated.invoice)),
            statement: None,
        };
        let booking =
            propose_booking(&document, InvoiceSide::Purchase, &owner, &ProposalAccounts::v0_1(InvoiceSide::Purchase))
                .unwrap();
        let date = booking.date;
        let mut set = EventSet::new();
        let mut clock = Clock::new("laptop".parse().unwrap());
        set.insert(Event { time: clock.now(1), seq: 1, prev: None, payload: Payload::DocumentRecorded { document } });
        let (seq, prev) = set.chain_head(&"laptop".parse().unwrap());
        set.insert(Event { time: clock.now(2), seq, prev, payload: Payload::BookingRecorded { booking } });
        gobd::export(
            &set,
            &Chart::v0_1(),
            &Names::v0_1("de").unwrap(),
            &CurrencyTable::v0_1(),
            &TaxTable::de(),
            &owner,
            date,
            date,
        )
    }

    /// Writes the export to the directory in `LIBROTENADO_EXPORT`, to check it with other tools.
    #[test]
    fn writes_the_export_where_asked() {
        if let Ok(dir) = std::env::var("LIBROTENADO_EXPORT") {
            for file in exported() {
                std::fs::write(format!("{dir}/{}", file.path), file.content).unwrap();
            }
        }
    }

    #[test]
    fn an_export_for_a_tax_audit() {
        let files = exported();
        let names: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, ["index.xml", "konten.csv", "buchungen.csv", "belege.csv"]);
        let text =
            |name: &str| String::from_utf8(files.iter().find(|f| f.path == name).unwrap().content.clone()).unwrap();
        let index = text("index.xml");
        let doc = roxmltree::Document::parse_with_options(
            &index,
            roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() },
        )
        .unwrap();
        assert_eq!(doc.root_element().tag_name().name(), "DataSet");
        for table in doc.descendants().filter(|n| n.has_tag_name("Table")) {
            let url = table.children().find(|c| c.has_tag_name("URL")).unwrap().text().unwrap();
            let columns = table.descendants().filter(|n| n.has_tag_name("VariableColumn")).count();
            for line in text(url).lines() {
                let cells = line.split(';').count();
                assert!(cells >= columns, "{url}: {cells} cells for {columns} columns in {line}");
            }
        }
        let journal = text("buchungen.csv");
        assert_eq!(journal.lines().count(), 1 + 5, "header, two net amounts, two VAT amounts, the supplier");
        assert!(journal.contains("\"de-in-19\";\"19\""), "{journal}");
        assert!(journal.contains("\"Sonstiges Material\";\"Aufwand\""), "German account names: {journal}");
        assert!(text("belege.csv").contains(&format!("\"{}\"", read("03.01a-INVOICE_ubl.xml").number)));
    }
}

mod writing {
    use super::*;

    /// Reads a written XRechnung back.
    fn again(xml: &str) -> TranslatedInvoice {
        en16931::read(xml.as_bytes(), &CurrencyTable::v0_1()).unwrap()
    }

    /// Every test invoice of the XRechnung test suite: its name and what it translates to.
    fn every_test_invoice() -> Vec<(String, TranslatedInvoice)> {
        let dir = format!("{}/tests/data/en16931", env!("CARGO_MANIFEST_DIR"));
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with('0') && n.ends_with(".xml"))
            .collect();
        names.sort();
        names.into_iter().map(|name| (name.clone(), read(&name))).collect()
    }

    /// Writing and reading again keeps every term of the model, in both syntaxes and across them, for every test
    /// invoice of the XRechnung test suite.
    #[test]
    fn written_invoices_read_back_the_same() {
        let currencies = CurrencyTable::v0_1();
        let invoices = every_test_invoice();
        assert!(invoices.len() > 70);
        for (name, original) in invoices {
            en16931::check_xrechnung(&original).unwrap_or_else(|e| panic!("{name}: {e}"));
            let ubl = en16931::write_ubl(&original, &currencies).unwrap();
            let cii = match en16931::write_cii(&original, &currencies) {
                Ok(cii) => Some(cii),
                // CII carries one preceding invoice only.
                Err(en16931::WriteError::Syntax("BG-3")) if original.invoice.preceding.len() > 1 => None,
                Err(error) => panic!("{name}: {error}"),
            };
            // Written files go where LIBROTENADO_EXPORT asks, to check them with other tools.
            if let Ok(dir) = std::env::var("LIBROTENADO_EXPORT") {
                std::fs::write(format!("{dir}/{name}.ubl.xml"), &ubl).unwrap();
                if let Some(cii) = &cii {
                    std::fs::write(format!("{dir}/{name}.cii.xml"), cii).unwrap();
                }
            }
            for xml in std::iter::once(ubl).chain(cii) {
                let back = again(&xml);
                assert_eq!(
                    (&back.number, back.date, back.kind),
                    (&original.number, original.date, original.kind),
                    "{name}"
                );
                assert_eq!(back.invoice, original.invoice, "{name}");
                assert_eq!(back.files, original.files, "{name}");
            }
        }
    }

    /// The model keeps the terms of EN 16931 beyond those XRechnung requires.
    #[test]
    fn all_terms_are_kept() {
        let invoice = read("01.01_comprehensive_test_ubl.xml");
        let data = &invoice.invoice;
        assert!(!data.notes.is_empty() && !data.preceding.is_empty() && !data.attachments.is_empty());
        assert!(data.payee.is_some() && data.tax_representative.is_some() && data.delivery.is_some());
        assert!(data.period.is_some() && !data.allowances.is_empty() && !data.charges.is_empty());
        let references = data.references.as_ref().unwrap();
        assert!(references.order.is_some() && references.contract.is_some() && references.project.is_some());
        let file = data.attachments.iter().find_map(|a| a.file.as_ref()).unwrap();
        assert!(file.is(&invoice.files[&file.hash]), "the attached file is kept by its hash");
        let line = &data.lines[0];
        assert!(line.seller_item.is_some() && line.period.is_some());
        assert_eq!(read("01.01_comprehensive_test_uncefact.xml").invoice.notes, data.notes);
    }

    #[test]
    fn a_credit_note_is_written_as_one() {
        let currencies = CurrencyTable::v0_1();
        let mut credit_note = read("03.01a-INVOICE_ubl.xml");
        credit_note.kind = DocumentKind::CreditNote;
        let ubl = en16931::write_ubl(&credit_note, &currencies).unwrap();
        assert!(ubl.contains("<ubl:CreditNote") && ubl.contains("<cac:CreditNoteLine>"));
        if let Ok(dir) = std::env::var("LIBROTENADO_EXPORT") {
            std::fs::write(format!("{dir}/credit-note.ubl.xml"), &ubl).unwrap();
        }
        for xml in [ubl, en16931::write_cii(&credit_note, &currencies).unwrap()] {
            let back = again(&xml);
            assert_eq!(back.kind, DocumentKind::CreditNote);
            assert_eq!(back.invoice, credit_note.invoice);
        }
    }

    #[test]
    fn missing_terms_are_named() {
        let mut invoice = read("01.01a-INVOICE_ubl.xml");
        invoice.invoice.buyer_reference = None;
        invoice.invoice.seller.contact = None;
        let error = en16931::check_xrechnung(&invoice).unwrap_err();
        assert_eq!(error, en16931::WriteError::Missing(vec!["BT-10", "BT-41", "BT-42", "BT-43"]));
    }
}

#[cfg(feature = "pdf")]
#[test]
fn an_xrechnung_attached_to_a_pdf_reads_back() {
    use lopdf::{Document, Object, dictionary};
    let mut pdf = Document::with_version("1.7");
    let pages = pdf.new_object_id();
    pdf.objects.insert(pages, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![], "Count" => 0 }));
    let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    pdf.trailer.set("Root", catalog);
    let mut base = Vec::new();
    pdf.save_to(&mut base).unwrap();

    let original = read("03.01a-INVOICE_ubl.xml");
    let cii = en16931::write_cii(&original, &CurrencyTable::v0_1()).unwrap();
    let hybrid = en16931::attach_to_pdf(&base, &cii).unwrap();
    let back = en16931::read_pdf(&hybrid, &CurrencyTable::v0_1()).unwrap();
    assert_eq!((back.format, &back.invoice), ("en16931-cii", &original.invoice));
    let hybrid = Document::load_mem(&hybrid).unwrap();
    let catalog = hybrid.catalog().unwrap();
    let spec = catalog.get(b"AF").unwrap().as_array().unwrap()[0].as_reference().unwrap();
    let spec = hybrid.get_dictionary(spec).unwrap();
    assert_eq!(spec.get(b"AFRelationship").unwrap().as_name().unwrap(), b"Alternative");
    assert_eq!(spec.get(b"F").unwrap().as_str().unwrap(), b"xrechnung.xml");
    let metadata = hybrid.get_object(catalog.get(b"Metadata").unwrap().as_reference().unwrap()).unwrap();
    let xmp = String::from_utf8(metadata.as_stream().unwrap().content.clone()).unwrap();
    assert!(xmp.contains("<fx:ConformanceLevel>XRECHNUNG</fx:ConformanceLevel>"));
}
