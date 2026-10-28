// SPDX-License-Identifier: MIT
//! Translation of electronic invoices according to EN 16931 into the model (spec part 0005): both syntaxes, UBL and
//! UN/CEFACT CII – XRechnung, Peppol BIS Billing, and the XML embedded in ZUGFeRD and Factur-X PDFs.

use roxmltree::{Document as Xml, Node};

use crate::{
    Country, Currency, CurrencyTable, Date, Decimal, DocumentKind, InvoiceData, InvoiceLine, InvoiceParty,
    InvoiceTotals, Money, VatBreakdown, VatCategory,
};

const UBL_INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
const UBL_CREDIT_NOTE: &str = "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2";
const CII: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";

/// What a translation of an electronic invoice yields: the data of a document of the model. The document's own
/// identity, day received and original are the caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslatedInvoice {
    /// The name of the format (spec part 0005): `en16931-ubl` or `en16931-cii`.
    pub format: &'static str,
    /// An invoice or a credit note.
    pub kind: DocumentKind,
    /// The invoice number (BT-1).
    pub number: String,
    /// The issue date (BT-2).
    pub date: Date,
    /// The data of the invoice.
    pub invoice: InvoiceData,
}

/// Why an electronic invoice cannot be translated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranslationError {
    /// Not XML.
    #[error("not XML: {0}")]
    Xml(String),
    /// Neither a UBL invoice or credit note nor a CII invoice.
    #[error("not an invoice according to EN 16931 in UBL or CII")]
    Syntax,
    /// A term the model needs is missing.
    #[error("{0} is missing")]
    Missing(&'static str),
    /// A value that cannot be read.
    #[error("{term} is not valid: {value:?}")]
    Invalid {
        /// The term.
        term: &'static str,
        /// The value.
        value: String,
    },
    /// A currency the currency table does not divide by decimals.
    #[error("currency {0} is not in the currency table")]
    Currency(String),
    /// A PDF without an embedded invoice.
    #[error("the PDF has no embedded invoice: {0}")]
    Pdf(String),
}

type Result<T> = std::result::Result<T, TranslationError>;

/// Translates an electronic invoice in UBL or CII.
pub fn read(xml: &[u8], currencies: &CurrencyTable) -> Result<TranslatedInvoice> {
    let text = std::str::from_utf8(xml).map_err(|e| TranslationError::Xml(e.to_string()))?;
    let doc = Xml::parse(text).map_err(|e| TranslationError::Xml(e.to_string()))?;
    let root = doc.root_element();
    match (root.tag_name().namespace(), root.tag_name().name()) {
        (Some(UBL_INVOICE), "Invoice") => ubl(root, false, currencies),
        (Some(UBL_CREDIT_NOTE), "CreditNote") => ubl(root, true, currencies),
        (Some(CII), "CrossIndustryInvoice") => cii(root, currencies),
        _ => Err(TranslationError::Syntax),
    }
}

/// Translates the invoice embedded in a PDF – ZUGFeRD, Factur-X: the attached file of an EN 16931 syntax.
#[cfg(feature = "pdf")]
pub fn read_pdf(pdf: &[u8], currencies: &CurrencyTable) -> Result<TranslatedInvoice> {
    let file = lopdf::Document::load_mem(pdf).map_err(|e| TranslationError::Pdf(e.to_string()))?;
    let mut last = TranslationError::Pdf("no attached file".into());
    for object in file.objects.values() {
        let Ok(stream) = object.as_stream() else { continue };
        let is_embedded = stream.dict.get(b"Type").and_then(|t| t.as_name()).is_ok_and(|n| n == b"EmbeddedFile");
        if !is_embedded {
            continue;
        }
        let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        match read(&content, currencies) {
            Ok(invoice) => return Ok(invoice),
            Err(error) => last = error,
        }
    }
    Err(last)
}

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

fn required(node: Node, path: &[&str], term: &'static str) -> Result<String> {
    text(node, path).ok_or(TranslationError::Missing(term))
}

/// A date written `YYYY-MM-DD` (UBL) or `YYYYMMDD` (CII, format 102).
fn date(value: &str, term: &'static str) -> Result<Date> {
    let iso = if value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit()) {
        format!("{}-{}-{}", &value[..4], &value[4..6], &value[6..])
    } else {
        value.to_owned()
    };
    iso.parse().map_err(|_| TranslationError::Invalid { term, value: value.to_owned() })
}

/// A decimal number as written in XML (`"19.00"`) in its one form (`"19"`).
fn decimal(value: &str, term: &'static str) -> Result<Decimal> {
    let invalid = || TranslationError::Invalid { term, value: value.to_owned() };
    let (negative, digits) = value.strip_prefix('-').map_or((false, value), |rest| (true, rest));
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let digits_only = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() || !digits_only(int) || !digits_only(frac) || frac.len() > usize::from(Decimal::MAX_SCALE) {
        return Err(invalid());
    }
    let units: i128 = format!("{int}{frac}").parse().map_err(|_| invalid())?;
    let scale = u8::try_from(frac.len()).map_err(|_| invalid())?;
    Decimal::new(if negative { -units } else { units }, scale).ok_or_else(invalid)
}

/// An amount as money of a currency divided by decimals: `"119.00"` EUR is 11 900 cents.
fn money(value: &str, currency: &Currency, decimals: u32, term: &'static str) -> Result<Money> {
    let invalid = || TranslationError::Invalid { term, value: value.to_owned() };
    let number = decimal(value, term)?;
    let shift = decimals.checked_sub(u32::from(number.scale())).ok_or_else(invalid)?;
    let units = number.units().checked_mul(10i128.checked_pow(shift).ok_or_else(invalid)?).ok_or_else(invalid)?;
    Money::new(units, currency.clone()).ok_or_else(invalid)
}

fn category(value: &str, term: &'static str) -> Result<VatCategory> {
    Ok(match value {
        "S" => VatCategory::Standard,
        "Z" => VatCategory::ZeroRated,
        "E" => VatCategory::Exempt,
        "AE" => VatCategory::ReverseCharge,
        "K" => VatCategory::IntraCommunity,
        "G" => VatCategory::Export,
        "O" => VatCategory::NotSubject,
        "L" => VatCategory::CanaryIslands,
        "M" => VatCategory::CeutaMelilla,
        _ => return Err(TranslationError::Invalid { term, value: value.to_owned() }),
    })
}

/// The rate of a category: absent where it has none (zero on an exempt or not taxed amount).
fn rate(value: Option<String>, category: VatCategory, term: &'static str) -> Result<Option<Decimal>> {
    let rate = value.map(|v| decimal(&v, term)).transpose()?;
    let rated = matches!(category, VatCategory::Standard | VatCategory::ZeroRated);
    Ok(rate.filter(|r| rated || !r.is_zero()))
}

fn country(value: Option<String>) -> Option<Country> {
    value.and_then(|v| v.parse().ok())
}

/// The currency of the invoice and its number of decimals.
fn currency(code: &str, currencies: &CurrencyTable) -> Result<(Currency, u32)> {
    let currency: Currency = code.parse().map_err(|_| TranslationError::Currency(code.to_owned()))?;
    let decimals = currencies
        .get(&currency)
        .and_then(|d| d.decimals())
        .ok_or_else(|| TranslationError::Currency(code.to_owned()))?;
    Ok((currency, decimals))
}

fn kind(type_code: &str) -> DocumentKind {
    if type_code == "381" { DocumentKind::CreditNote } else { DocumentKind::Invoice }
}

fn ubl(root: Node, credit_note: bool, currencies: &CurrencyTable) -> Result<TranslatedInvoice> {
    let (currency, decimals) = currency(&required(root, &["DocumentCurrencyCode"], "BT-5")?, currencies)?;
    let amount = |node: Node, path: &[&str], term| -> Result<Money> {
        money(&required(node, path, term)?, &currency, decimals, term)
    };
    let party = |path: &str| -> Result<InvoiceParty> {
        let party = at(root, &[path, "Party"]).ok_or(TranslationError::Missing("BG-4/BG-7"))?;
        let tax_id = |vat: bool| {
            all(party, "PartyTaxScheme")
                .find(|s| (text(*s, &["TaxScheme", "ID"]).as_deref() == Some("VAT")) == vat)
                .and_then(|s| text(s, &["CompanyID"]))
        };
        let name = text(party, &["PartyLegalEntity", "RegistrationName"])
            .or_else(|| text(party, &["PartyName", "Name"]))
            .ok_or(TranslationError::Missing("BT-27/BT-44"))?;
        Ok(InvoiceParty {
            name,
            vat_id: tax_id(true),
            tax_number: tax_id(false),
            country: country(text(party, &["PostalAddress", "Country", "IdentificationCode"])),
            party: None,
        })
    };
    let type_code = text(root, &[if credit_note { "CreditNoteTypeCode" } else { "InvoiceTypeCode" }])
        .ok_or(TranslationError::Missing("BT-3"))?;
    let due = text(root, &["DueDate"]).or_else(|| text(root, &["PaymentMeans", "PaymentDueDate"]));
    let tax_total = all(root, "TaxTotal")
        .find(|t| at(*t, &["TaxAmount"]).and_then(|a| a.attribute("currencyID")) == Some(currency.as_str()))
        .ok_or(TranslationError::Missing("BT-110"))?;
    let taxes = all(tax_total, "TaxSubtotal")
        .map(|sub| {
            let category = category(&required(sub, &["TaxCategory", "ID"], "BT-118")?, "BT-118")?;
            Ok(VatBreakdown {
                category,
                rate: rate(text(sub, &["TaxCategory", "Percent"]), category, "BT-119")?,
                base: amount(sub, &["TaxableAmount"], "BT-116")?,
                tax: amount(sub, &["TaxAmount"], "BT-117")?,
                exemption: text(sub, &["TaxCategory", "TaxExemptionReason"]),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let line_name = if credit_note { "CreditNoteLine" } else { "InvoiceLine" };
    let lines = all(root, line_name)
        .map(|line| {
            let category = category(&required(line, &["Item", "ClassifiedTaxCategory", "ID"], "BT-151")?, "BT-151")?;
            Ok(InvoiceLine {
                name: required(line, &["Item", "Name"], "BT-153")?,
                net: amount(line, &["LineExtensionAmount"], "BT-131")?,
                category,
                rate: rate(text(line, &["Item", "ClassifiedTaxCategory", "Percent"]), category, "BT-152")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let totals = at(root, &["LegalMonetaryTotal"]).ok_or(TranslationError::Missing("BG-22"))?;
    Ok(TranslatedInvoice {
        format: "en16931-ubl",
        kind: if credit_note { DocumentKind::CreditNote } else { kind(&type_code) },
        number: required(root, &["ID"], "BT-1")?,
        date: date(&required(root, &["IssueDate"], "BT-2")?, "BT-2")?,
        invoice: InvoiceData {
            currency: currency.clone(),
            seller: party("AccountingSupplierParty")?,
            buyer: party("AccountingCustomerParty")?,
            due_date: due.map(|d| date(&d, "BT-9")).transpose()?,
            delivery_date: text(root, &["Delivery", "ActualDeliveryDate"]).map(|d| date(&d, "BT-72")).transpose()?,
            buyer_reference: text(root, &["BuyerReference"]),
            iban: text(root, &["PaymentMeans", "PayeeFinancialAccount", "ID"]),
            taxes,
            lines,
            totals: InvoiceTotals {
                net: amount(totals, &["TaxExclusiveAmount"], "BT-109")?,
                tax: amount(tax_total, &["TaxAmount"], "BT-110")?,
                gross: amount(totals, &["TaxInclusiveAmount"], "BT-112")?,
                due: amount(totals, &["PayableAmount"], "BT-115")?,
            },
        },
    })
}

fn cii(root: Node, currencies: &CurrencyTable) -> Result<TranslatedInvoice> {
    let document = at(root, &["ExchangedDocument"]).ok_or(TranslationError::Missing("BT-1"))?;
    let transaction = at(root, &["SupplyChainTradeTransaction"]).ok_or(TranslationError::Syntax)?;
    let agreement = at(transaction, &["ApplicableHeaderTradeAgreement"]).ok_or(TranslationError::Missing("BG-4"))?;
    let settlement = at(transaction, &["ApplicableHeaderTradeSettlement"]).ok_or(TranslationError::Missing("BG-22"))?;
    let (currency, decimals) = currency(&required(settlement, &["InvoiceCurrencyCode"], "BT-5")?, currencies)?;
    let amount = |node: Node, path: &[&str], term| -> Result<Money> {
        money(&required(node, path, term)?, &currency, decimals, term)
    };
    let party = |name: &str| -> Result<InvoiceParty> {
        let party = at(agreement, &[name]).ok_or(TranslationError::Missing("BG-4/BG-7"))?;
        let registration = |schemes: &[&str]| {
            all(party, "SpecifiedTaxRegistration")
                .filter_map(|r| at(r, &["ID"]))
                .find(|id| id.attribute("schemeID").is_some_and(|s| schemes.contains(&s)))
                .and_then(|id| id.text())
                .map(|t| t.trim().to_owned())
        };
        Ok(InvoiceParty {
            name: required(party, &["Name"], "BT-27/BT-44")?,
            vat_id: registration(&["VA", "VAT"]),
            tax_number: registration(&["FC"]),
            country: country(text(party, &["PostalTradeAddress", "CountryID"])),
            party: None,
        })
    };
    let date_time = |node: Node, path: &[&str], term| -> Result<Option<Date>> {
        text(node, path).map(|d| date(&d, term)).transpose()
    };
    let taxes = all(settlement, "ApplicableTradeTax")
        .map(|tax| {
            let category = category(&required(tax, &["CategoryCode"], "BT-118")?, "BT-118")?;
            Ok(VatBreakdown {
                category,
                rate: rate(text(tax, &["RateApplicablePercent"]), category, "BT-119")?,
                base: amount(tax, &["BasisAmount"], "BT-116")?,
                tax: amount(tax, &["CalculatedAmount"], "BT-117")?,
                exemption: text(tax, &["ExemptionReason"]),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let lines = all(transaction, "IncludedSupplyChainTradeLineItem")
        .map(|line| {
            let tax = at(line, &["SpecifiedLineTradeSettlement", "ApplicableTradeTax"]);
            let category = category(
                &tax.and_then(|t| text(t, &["CategoryCode"])).ok_or(TranslationError::Missing("BT-151"))?,
                "BT-151",
            )?;
            Ok(InvoiceLine {
                name: required(line, &["SpecifiedTradeProduct", "Name"], "BT-153")?,
                net: amount(
                    line,
                    &[
                        "SpecifiedLineTradeSettlement",
                        "SpecifiedTradeSettlementLineMonetarySummation",
                        "LineTotalAmount",
                    ],
                    "BT-131",
                )?,
                category,
                rate: rate(tax.and_then(|t| text(t, &["RateApplicablePercent"])), category, "BT-152")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let totals = at(settlement, &["SpecifiedTradeSettlementHeaderMonetarySummation"])
        .ok_or(TranslationError::Missing("BG-22"))?;
    let tax_total = all(totals, "TaxTotalAmount")
        .find(|t| t.attribute("currencyID").is_none_or(|c| c == currency.as_str()))
        .and_then(|t| t.text())
        .ok_or(TranslationError::Missing("BT-110"))?;
    let means = at(settlement, &["SpecifiedTradeSettlementPaymentMeans", "PayeePartyCreditorFinancialAccount"]);
    Ok(TranslatedInvoice {
        format: "en16931-cii",
        kind: kind(&required(document, &["TypeCode"], "BT-3")?),
        number: required(document, &["ID"], "BT-1")?,
        date: date(&required(document, &["IssueDateTime", "DateTimeString"], "BT-2")?, "BT-2")?,
        invoice: InvoiceData {
            currency: currency.clone(),
            seller: party("SellerTradeParty")?,
            buyer: party("BuyerTradeParty")?,
            due_date: date_time(
                settlement,
                &["SpecifiedTradePaymentTerms", "DueDateDateTime", "DateTimeString"],
                "BT-9",
            )?,
            delivery_date: date_time(
                transaction,
                &[
                    "ApplicableHeaderTradeDelivery",
                    "ActualDeliverySupplyChainEvent",
                    "OccurrenceDateTime",
                    "DateTimeString",
                ],
                "BT-72",
            )?,
            buyer_reference: text(agreement, &["BuyerReference"]),
            iban: means.and_then(|m| text(m, &["IBANID"]).or_else(|| text(m, &["ProprietaryID"]))),
            taxes,
            lines,
            totals: InvoiceTotals {
                net: amount(totals, &["TaxBasisTotalAmount"], "BT-109")?,
                tax: money(tax_total.trim(), &currency, decimals, "BT-110")?,
                gross: amount(totals, &["GrandTotalAmount"], "BT-112")?,
                due: amount(totals, &["DuePayableAmount"], "BT-115")?,
            },
        },
    })
}
