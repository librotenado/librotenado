// SPDX-License-Identifier: MIT
//! Translation of electronic invoices according to EN 16931 into the model (spec part 0005): both syntaxes, UBL and
//! UN/CEFACT CII – XRechnung, Peppol BIS Billing, and the XML embedded in ZUGFeRD and Factur-X PDFs.

use roxmltree::{Document as Xml, Node};

use crate::{
    Country, Currency, CurrencyTable, Date, Decimal, DirectDebit, DocumentKind, Endpoint, InvoiceContact, InvoiceData,
    InvoiceLine, InvoiceParty, InvoiceTotals, Money, VatBreakdown, VatCategory,
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

/// An electronic address from an element with a `schemeID`.
fn endpoint(node: Option<Node>) -> Option<Endpoint> {
    let node = node?;
    Some(Endpoint { scheme: node.attribute("schemeID")?.to_owned(), address: node.text()?.trim().to_owned() })
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
    let optional = |node: Node, path: &[&str], term| -> Result<Option<Money>> {
        text(node, path).map(|value| money(&value, &currency, decimals, term)).transpose()
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
        let contact = at(party, &["Contact"]).map(|c| InvoiceContact {
            name: text(c, &["Name"]),
            phone: text(c, &["Telephone"]),
            email: text(c, &["ElectronicMail"]),
        });
        Ok(InvoiceParty {
            name,
            vat_id: tax_id(true),
            tax_number: tax_id(false),
            street: text(party, &["PostalAddress", "StreetName"]),
            postcode: text(party, &["PostalAddress", "PostalZone"]),
            city: text(party, &["PostalAddress", "CityName"]),
            country: country(text(party, &["PostalAddress", "Country", "IdentificationCode"])),
            contact,
            endpoint: endpoint(at(party, &["EndpointID"])),
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
            let quantity = at(line, &[if credit_note { "CreditedQuantity" } else { "InvoicedQuantity" }]);
            Ok(InvoiceLine {
                id: text(line, &["ID"]),
                name: required(line, &["Item", "Name"], "BT-153")?,
                quantity: quantity.and_then(|q| q.text()).map(|q| decimal(q.trim(), "BT-129")).transpose()?,
                unit: quantity.and_then(|q| q.attribute("unitCode")).map(str::to_owned),
                price: text(line, &["Price", "PriceAmount"]).map(|p| decimal(&p, "BT-146")).transpose()?,
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
            payment_means: text(root, &["PaymentMeans", "PaymentMeansCode"]),
            iban: text(root, &["PaymentMeans", "PayeeFinancialAccount", "ID"]),
            direct_debit: text(root, &["PaymentMeans", "PaymentMandate", "ID"]).map(|mandate| DirectDebit {
                mandate,
                creditor: at(root, &["AccountingSupplierParty", "Party"])
                    .into_iter()
                    .flat_map(|p| all(p, "PartyIdentification"))
                    .filter_map(|i| at(i, &["ID"]))
                    .find(|id| id.attribute("schemeID") == Some("SEPA"))
                    .and_then(|id| id.text())
                    .map(|t| t.trim().to_owned())
                    .unwrap_or_default(),
                debited: text(root, &["PaymentMeans", "PaymentMandate", "PayerFinancialAccount", "ID"])
                    .unwrap_or_default(),
            }),
            payment_terms: text(root, &["PaymentTerms", "Note"]),
            taxes,
            lines,
            totals: InvoiceTotals {
                net: amount(totals, &["TaxExclusiveAmount"], "BT-109")?,
                tax: amount(tax_total, &["TaxAmount"], "BT-110")?,
                gross: amount(totals, &["TaxInclusiveAmount"], "BT-112")?,
                prepaid: optional(totals, &["PrepaidAmount"], "BT-113")?,
                rounding: optional(totals, &["PayableRoundingAmount"], "BT-114")?,
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
    let optional = |node: Node, path: &[&str], term| -> Result<Option<Money>> {
        text(node, path).map(|value| money(&value, &currency, decimals, term)).transpose()
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
        let contact = at(party, &["DefinedTradeContact"]).map(|c| InvoiceContact {
            name: text(c, &["PersonName"]).or_else(|| text(c, &["DepartmentName"])),
            phone: text(c, &["TelephoneUniversalCommunication", "CompleteNumber"]),
            email: text(c, &["EmailURIUniversalCommunication", "URIID"]),
        });
        Ok(InvoiceParty {
            name: required(party, &["Name"], "BT-27/BT-44")?,
            vat_id: registration(&["VA", "VAT"]),
            tax_number: registration(&["FC"]),
            street: text(party, &["PostalTradeAddress", "LineOne"]),
            postcode: text(party, &["PostalTradeAddress", "PostcodeCode"]),
            city: text(party, &["PostalTradeAddress", "CityName"]),
            country: country(text(party, &["PostalTradeAddress", "CountryID"])),
            contact,
            endpoint: endpoint(at(party, &["URIUniversalCommunication", "URIID"])),
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
            let quantity = at(line, &["SpecifiedLineTradeDelivery", "BilledQuantity"]);
            Ok(InvoiceLine {
                id: text(line, &["AssociatedDocumentLineDocument", "LineID"]),
                name: required(line, &["SpecifiedTradeProduct", "Name"], "BT-153")?,
                quantity: quantity.and_then(|q| q.text()).map(|q| decimal(q.trim(), "BT-129")).transpose()?,
                unit: quantity.and_then(|q| q.attribute("unitCode")).map(str::to_owned),
                price: text(line, &["SpecifiedLineTradeAgreement", "NetPriceProductTradePrice", "ChargeAmount"])
                    .map(|p| decimal(&p, "BT-146"))
                    .transpose()?,
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
            payment_means: text(settlement, &["SpecifiedTradeSettlementPaymentMeans", "TypeCode"]),
            iban: means.and_then(|m| text(m, &["IBANID"]).or_else(|| text(m, &["ProprietaryID"]))),
            direct_debit: text(settlement, &["SpecifiedTradePaymentTerms", "DirectDebitMandateID"]).map(|mandate| {
                DirectDebit {
                    mandate,
                    creditor: text(settlement, &["CreditorReferenceID"]).unwrap_or_default(),
                    debited: text(
                        settlement,
                        &["SpecifiedTradeSettlementPaymentMeans", "PayerPartyDebtorFinancialAccount", "IBANID"],
                    )
                    .unwrap_or_default(),
                }
            }),
            payment_terms: text(settlement, &["SpecifiedTradePaymentTerms", "Description"]),
            taxes,
            lines,
            totals: InvoiceTotals {
                net: amount(totals, &["TaxBasisTotalAmount"], "BT-109")?,
                tax: money(tax_total.trim(), &currency, decimals, "BT-110")?,
                gross: amount(totals, &["GrandTotalAmount"], "BT-112")?,
                prepaid: optional(totals, &["TotalPrepaidAmount"], "BT-113")?,
                rounding: optional(totals, &["RoundingAmount"], "BT-114")?,
                due: amount(totals, &["DuePayableAmount"], "BT-115")?,
            },
        },
    })
}

/// The specification identifier (BT-24) of XRechnung 3.0.
pub const XRECHNUNG: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
/// The business process (BT-23) of Peppol BIS Billing 3.0, which XRechnung uses.
const PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

/// Why an invoice cannot be written as XRechnung.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    /// Terms XRechnung requires are missing, by their identifiers.
    #[error("missing for XRechnung: {}", .0.join(", "))]
    Missing(Vec<&'static str>),
    /// The lines do not add up to the net total: allowances and charges on the document are not supported yet.
    #[error("the lines do not add up to the net total")]
    Lines,
    /// The amount due is not the gross amount less prepaid amounts plus rounding.
    #[error("the amount due is not gross less prepaid plus rounding")]
    Totals,
    /// A currency the currency table does not divide by decimals.
    #[error("currency {0} is not in the currency table")]
    Currency(String),
}

/// Checks that an invoice carries what XRechnung 3.0 requires beyond EN 16931 and the model's own terms.
pub fn check_xrechnung(invoice: &TranslatedInvoice) -> std::result::Result<(), WriteError> {
    let data = &invoice.invoice;
    let mut missing = Vec::new();
    let mut need = |present: bool, term: &'static str| {
        if !present {
            missing.push(term);
        }
    };
    let (seller, buyer) = (&data.seller, &data.buyer);
    need(data.buyer_reference.is_some(), "BT-10");
    need(data.payment_means.is_some(), "BT-81");
    need(seller.street.is_some(), "BT-35");
    need(seller.city.is_some(), "BT-37");
    need(seller.postcode.is_some(), "BT-38");
    need(seller.country.is_some(), "BT-40");
    let contact = seller.contact.as_ref();
    need(contact.is_some_and(|c| c.name.is_some()), "BT-41");
    need(contact.is_some_and(|c| c.phone.is_some()), "BT-42");
    need(contact.is_some_and(|c| c.email.is_some()), "BT-43");
    need(seller.vat_id.is_some() || seller.tax_number.is_some(), "BT-31/BT-32");
    need(seller.endpoint.is_some(), "BT-34");
    need(buyer.endpoint.is_some(), "BT-49");
    need(buyer.city.is_some(), "BT-52");
    need(buyer.postcode.is_some(), "BT-53");
    need(buyer.country.is_some(), "BT-55");
    if matches!(data.payment_means.as_deref(), Some("30" | "58")) {
        need(data.iban.is_some(), "BT-84");
    }
    if data.payment_means.as_deref() == Some("59") {
        let debit = data.direct_debit.as_ref();
        need(debit.is_some_and(|d| !d.mandate.is_empty()), "BT-89");
        need(debit.is_some_and(|d| !d.creditor.is_empty()), "BT-90");
        need(debit.is_some_and(|d| !d.debited.is_empty()), "BT-91");
    }
    need(!data.lines.is_empty(), "BG-25");
    for line in &data.lines {
        need(line.id.is_some(), "BT-126");
        need(line.quantity.is_some(), "BT-129");
        need(line.unit.is_some(), "BT-130");
        need(line.price.is_some(), "BT-146");
    }
    for tax in &data.taxes {
        let exempt = !matches!(tax.category, VatCategory::Standard | VatCategory::ZeroRated);
        need(!exempt || tax.exemption.is_some(), "BT-120");
    }
    missing.dedup();
    if !missing.is_empty() {
        return Err(WriteError::Missing(missing));
    }
    let lines: i128 = data.lines.iter().map(|l| l.net.base_units()).sum();
    if lines != data.totals.net.base_units() {
        return Err(WriteError::Lines);
    }
    let totals = &data.totals;
    let units = |m: &Option<Money>| m.as_ref().map_or(0, Money::base_units);
    if totals.gross.base_units() - units(&totals.prepaid) + units(&totals.rounding) != totals.due.base_units() {
        return Err(WriteError::Totals);
    }
    Ok(())
}

/// Writes XML with indentation.
struct Writer {
    out: String,
    depth: usize,
}

impl Writer {
    fn new() -> Self {
        Writer { out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"), depth: 0 }
    }

    fn open(&mut self, tag: &str, attributes: &str) {
        self.out.push_str(&format!("{}<{tag}{attributes}>\n", "  ".repeat(self.depth)));
        self.depth += 1;
    }

    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.out.push_str(&format!("{}</{tag}>\n", "  ".repeat(self.depth)));
    }

    fn leaf(&mut self, tag: &str, attributes: &str, value: &str) {
        self.out.push_str(&format!("{}<{tag}{attributes}>{}</{tag}>\n", "  ".repeat(self.depth), escape(value)));
    }

    fn some(&mut self, tag: &str, value: Option<&str>) {
        if let Some(value) = value {
            self.leaf(tag, "", value);
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn attribute(name: &str, value: &str) -> String {
    format!(" {name}=\"{}\"", escape(value))
}

/// Money as written in EN 16931: base units at the currency's decimals, `11900` EUR as `119.00`.
fn written(money: &Money, decimals: u32) -> String {
    let units = money.base_units();
    let sign = if units < 0 { "-" } else { "" };
    let digits = units.unsigned_abs().to_string();
    if decimals == 0 {
        return format!("{sign}{digits}");
    }
    let places = decimals as usize;
    let digits = format!("{digits:0>width$}", width = places + 1);
    let (int, frac) = digits.split_at(digits.len() - places);
    format!("{sign}{int}.{frac}")
}

fn category_code(category: VatCategory) -> &'static str {
    match category {
        VatCategory::Standard => "S",
        VatCategory::ZeroRated => "Z",
        VatCategory::Exempt => "E",
        VatCategory::ReverseCharge => "AE",
        VatCategory::IntraCommunity => "K",
        VatCategory::Export => "G",
        VatCategory::NotSubject => "O",
        VatCategory::CanaryIslands => "L",
        VatCategory::CeutaMelilla => "M",
    }
}

/// The rate as written: the category's rate, 0 for exempt categories, none for amounts not subject to VAT.
fn written_rate(category: VatCategory, rate: Option<Decimal>) -> Option<String> {
    match category {
        VatCategory::NotSubject => None,
        _ => Some(rate.map_or_else(|| "0".to_owned(), |r| r.to_string())),
    }
}

fn type_code(kind: DocumentKind) -> &'static str {
    if kind == DocumentKind::CreditNote { "381" } else { "380" }
}

fn iso(date: Date) -> String {
    date.to_string()
}

fn compact(date: Date) -> String {
    date.to_string().replace('-', "")
}

/// Writes an invoice or credit note as XRechnung 3.0 in the UBL syntax.
pub fn write_ubl(invoice: &TranslatedInvoice, currencies: &CurrencyTable) -> std::result::Result<String, WriteError> {
    check_xrechnung(invoice)?;
    let data = &invoice.invoice;
    let code = data.currency.as_str();
    let decimals = currencies
        .get(&data.currency)
        .and_then(|d| d.decimals())
        .ok_or_else(|| WriteError::Currency(code.to_owned()))?;
    let amount = |m: &Money| written(m, decimals);
    let currency = attribute("currencyID", code);
    let credit_note = invoice.kind == DocumentKind::CreditNote;
    let root = if credit_note { "ubl:CreditNote" } else { "ubl:Invoice" };
    let namespace = if credit_note { UBL_CREDIT_NOTE } else { UBL_INVOICE };
    let mut x = Writer::new();
    x.open(
        root,
        &format!(
            "{}{}{}",
            attribute("xmlns:ubl", namespace),
            attribute("xmlns:cac", "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2"),
            attribute("xmlns:cbc", "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2"),
        ),
    );
    x.leaf("cbc:CustomizationID", "", XRECHNUNG);
    x.leaf("cbc:ProfileID", "", PROCESS);
    x.leaf("cbc:ID", "", &invoice.number);
    x.leaf("cbc:IssueDate", "", &iso(invoice.date));
    if !credit_note && let Some(due) = data.due_date {
        x.leaf("cbc:DueDate", "", &iso(due));
    }
    x.leaf(if credit_note { "cbc:CreditNoteTypeCode" } else { "cbc:InvoiceTypeCode" }, "", type_code(invoice.kind));
    x.leaf("cbc:DocumentCurrencyCode", "", code);
    x.some("cbc:BuyerReference", data.buyer_reference.as_deref());
    for (tag, party) in [("cac:AccountingSupplierParty", &data.seller), ("cac:AccountingCustomerParty", &data.buyer)] {
        x.open(tag, "");
        x.open("cac:Party", "");
        if let Some(endpoint) = &party.endpoint {
            x.leaf("cbc:EndpointID", &attribute("schemeID", &endpoint.scheme), &endpoint.address);
        }
        if tag == "cac:AccountingSupplierParty"
            && let Some(debit) = &data.direct_debit
        {
            x.open("cac:PartyIdentification", "");
            x.leaf("cbc:ID", &attribute("schemeID", "SEPA"), &debit.creditor);
            x.close("cac:PartyIdentification");
        }
        x.open("cac:PostalAddress", "");
        x.some("cbc:StreetName", party.street.as_deref());
        x.some("cbc:CityName", party.city.as_deref());
        x.some("cbc:PostalZone", party.postcode.as_deref());
        x.open("cac:Country", "");
        x.some("cbc:IdentificationCode", party.country.as_ref().map(Country::as_str));
        x.close("cac:Country");
        x.close("cac:PostalAddress");
        for (id, scheme) in [(&party.vat_id, "VAT"), (&party.tax_number, "FC")] {
            if let Some(id) = id {
                x.open("cac:PartyTaxScheme", "");
                x.leaf("cbc:CompanyID", "", id);
                x.open("cac:TaxScheme", "");
                x.leaf("cbc:ID", "", scheme);
                x.close("cac:TaxScheme");
                x.close("cac:PartyTaxScheme");
            }
        }
        x.open("cac:PartyLegalEntity", "");
        x.leaf("cbc:RegistrationName", "", &party.name);
        x.close("cac:PartyLegalEntity");
        if let Some(contact) = &party.contact {
            x.open("cac:Contact", "");
            x.some("cbc:Name", contact.name.as_deref());
            x.some("cbc:Telephone", contact.phone.as_deref());
            x.some("cbc:ElectronicMail", contact.email.as_deref());
            x.close("cac:Contact");
        }
        x.close("cac:Party");
        x.close(tag);
    }
    if let Some(delivery) = data.delivery_date {
        x.open("cac:Delivery", "");
        x.leaf("cbc:ActualDeliveryDate", "", &iso(delivery));
        x.close("cac:Delivery");
    }
    if let Some(means) = &data.payment_means {
        x.open("cac:PaymentMeans", "");
        x.leaf("cbc:PaymentMeansCode", "", means);
        if credit_note && let Some(due) = data.due_date {
            x.leaf("cbc:PaymentDueDate", "", &iso(due));
        }
        if let Some(iban) = &data.iban {
            x.open("cac:PayeeFinancialAccount", "");
            x.leaf("cbc:ID", "", iban);
            x.close("cac:PayeeFinancialAccount");
        }
        if let Some(debit) = &data.direct_debit {
            x.open("cac:PaymentMandate", "");
            x.leaf("cbc:ID", "", &debit.mandate);
            x.open("cac:PayerFinancialAccount", "");
            x.leaf("cbc:ID", "", &debit.debited);
            x.close("cac:PayerFinancialAccount");
            x.close("cac:PaymentMandate");
        }
        x.close("cac:PaymentMeans");
    }
    if let Some(terms) = &data.payment_terms {
        x.open("cac:PaymentTerms", "");
        x.leaf("cbc:Note", "", terms);
        x.close("cac:PaymentTerms");
    }
    x.open("cac:TaxTotal", "");
    x.leaf("cbc:TaxAmount", &currency, &amount(&data.totals.tax));
    for tax in &data.taxes {
        x.open("cac:TaxSubtotal", "");
        x.leaf("cbc:TaxableAmount", &currency, &amount(&tax.base));
        x.leaf("cbc:TaxAmount", &currency, &amount(&tax.tax));
        x.open("cac:TaxCategory", "");
        x.leaf("cbc:ID", "", category_code(tax.category));
        x.some("cbc:Percent", written_rate(tax.category, tax.rate).as_deref());
        x.some("cbc:TaxExemptionReason", tax.exemption.as_deref());
        x.open("cac:TaxScheme", "");
        x.leaf("cbc:ID", "", "VAT");
        x.close("cac:TaxScheme");
        x.close("cac:TaxCategory");
        x.close("cac:TaxSubtotal");
    }
    x.close("cac:TaxTotal");
    x.open("cac:LegalMonetaryTotal", "");
    x.leaf("cbc:LineExtensionAmount", &currency, &amount(&data.totals.net));
    x.leaf("cbc:TaxExclusiveAmount", &currency, &amount(&data.totals.net));
    x.leaf("cbc:TaxInclusiveAmount", &currency, &amount(&data.totals.gross));
    if let Some(prepaid) = &data.totals.prepaid {
        x.leaf("cbc:PrepaidAmount", &currency, &amount(prepaid));
    }
    if let Some(rounding) = &data.totals.rounding {
        x.leaf("cbc:PayableRoundingAmount", &currency, &amount(rounding));
    }
    x.leaf("cbc:PayableAmount", &currency, &amount(&data.totals.due));
    x.close("cac:LegalMonetaryTotal");
    let (line_tag, quantity_tag) = if credit_note {
        ("cac:CreditNoteLine", "cbc:CreditedQuantity")
    } else {
        ("cac:InvoiceLine", "cbc:InvoicedQuantity")
    };
    for line in &data.lines {
        x.open(line_tag, "");
        x.some("cbc:ID", line.id.as_deref());
        let unit = attribute("unitCode", line.unit.as_deref().unwrap_or_default());
        x.leaf(quantity_tag, &unit, &line.quantity.map(|q| q.to_string()).unwrap_or_default());
        x.leaf("cbc:LineExtensionAmount", &currency, &amount(&line.net));
        x.open("cac:Item", "");
        x.leaf("cbc:Name", "", &line.name);
        x.open("cac:ClassifiedTaxCategory", "");
        x.leaf("cbc:ID", "", category_code(line.category));
        x.some("cbc:Percent", written_rate(line.category, line.rate).as_deref());
        x.open("cac:TaxScheme", "");
        x.leaf("cbc:ID", "", "VAT");
        x.close("cac:TaxScheme");
        x.close("cac:ClassifiedTaxCategory");
        x.close("cac:Item");
        x.open("cac:Price", "");
        x.leaf("cbc:PriceAmount", &currency, &line.price.map(|p| p.to_string()).unwrap_or_default());
        x.close("cac:Price");
        x.close(line_tag);
    }
    x.close(root);
    Ok(x.out)
}

/// Writes an invoice or credit note as XRechnung 3.0 in the UN/CEFACT CII syntax – also the XML a ZUGFeRD or
/// Factur-X PDF carries in its profile XRECHNUNG.
pub fn write_cii(invoice: &TranslatedInvoice, currencies: &CurrencyTable) -> std::result::Result<String, WriteError> {
    check_xrechnung(invoice)?;
    let data = &invoice.invoice;
    let code = data.currency.as_str();
    let decimals = currencies
        .get(&data.currency)
        .and_then(|d| d.decimals())
        .ok_or_else(|| WriteError::Currency(code.to_owned()))?;
    let amount = |m: &Money| written(m, decimals);
    let mut x = Writer::new();
    x.open(
        "rsm:CrossIndustryInvoice",
        &format!(
            "{}{}{}{}",
            attribute("xmlns:rsm", CII),
            attribute(
                "xmlns:ram",
                "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100"
            ),
            attribute("xmlns:qdt", "urn:un:unece:uncefact:data:standard:QualifiedDataType:100"),
            attribute("xmlns:udt", "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100"),
        ),
    );
    x.open("rsm:ExchangedDocumentContext", "");
    x.open("ram:BusinessProcessSpecifiedDocumentContextParameter", "");
    x.leaf("ram:ID", "", PROCESS);
    x.close("ram:BusinessProcessSpecifiedDocumentContextParameter");
    x.open("ram:GuidelineSpecifiedDocumentContextParameter", "");
    x.leaf("ram:ID", "", XRECHNUNG);
    x.close("ram:GuidelineSpecifiedDocumentContextParameter");
    x.close("rsm:ExchangedDocumentContext");
    x.open("rsm:ExchangedDocument", "");
    x.leaf("ram:ID", "", &invoice.number);
    x.leaf("ram:TypeCode", "", type_code(invoice.kind));
    x.open("ram:IssueDateTime", "");
    x.leaf("udt:DateTimeString", &attribute("format", "102"), &compact(invoice.date));
    x.close("ram:IssueDateTime");
    x.close("rsm:ExchangedDocument");
    x.open("rsm:SupplyChainTradeTransaction", "");
    for line in &data.lines {
        x.open("ram:IncludedSupplyChainTradeLineItem", "");
        x.open("ram:AssociatedDocumentLineDocument", "");
        x.some("ram:LineID", line.id.as_deref());
        x.close("ram:AssociatedDocumentLineDocument");
        x.open("ram:SpecifiedTradeProduct", "");
        x.leaf("ram:Name", "", &line.name);
        x.close("ram:SpecifiedTradeProduct");
        x.open("ram:SpecifiedLineTradeAgreement", "");
        x.open("ram:NetPriceProductTradePrice", "");
        x.leaf("ram:ChargeAmount", "", &line.price.map(|p| p.to_string()).unwrap_or_default());
        x.close("ram:NetPriceProductTradePrice");
        x.close("ram:SpecifiedLineTradeAgreement");
        x.open("ram:SpecifiedLineTradeDelivery", "");
        let unit = attribute("unitCode", line.unit.as_deref().unwrap_or_default());
        x.leaf("ram:BilledQuantity", &unit, &line.quantity.map(|q| q.to_string()).unwrap_or_default());
        x.close("ram:SpecifiedLineTradeDelivery");
        x.open("ram:SpecifiedLineTradeSettlement", "");
        x.open("ram:ApplicableTradeTax", "");
        x.leaf("ram:TypeCode", "", "VAT");
        x.leaf("ram:CategoryCode", "", category_code(line.category));
        x.some("ram:RateApplicablePercent", written_rate(line.category, line.rate).as_deref());
        x.close("ram:ApplicableTradeTax");
        x.open("ram:SpecifiedTradeSettlementLineMonetarySummation", "");
        x.leaf("ram:LineTotalAmount", "", &amount(&line.net));
        x.close("ram:SpecifiedTradeSettlementLineMonetarySummation");
        x.close("ram:SpecifiedLineTradeSettlement");
        x.close("ram:IncludedSupplyChainTradeLineItem");
    }
    x.open("ram:ApplicableHeaderTradeAgreement", "");
    x.some("ram:BuyerReference", data.buyer_reference.as_deref());
    for (tag, party) in [("ram:SellerTradeParty", &data.seller), ("ram:BuyerTradeParty", &data.buyer)] {
        x.open(tag, "");
        x.leaf("ram:Name", "", &party.name);
        if let Some(contact) = &party.contact {
            x.open("ram:DefinedTradeContact", "");
            x.some("ram:PersonName", contact.name.as_deref());
            if let Some(phone) = &contact.phone {
                x.open("ram:TelephoneUniversalCommunication", "");
                x.leaf("ram:CompleteNumber", "", phone);
                x.close("ram:TelephoneUniversalCommunication");
            }
            if let Some(email) = &contact.email {
                x.open("ram:EmailURIUniversalCommunication", "");
                x.leaf("ram:URIID", "", email);
                x.close("ram:EmailURIUniversalCommunication");
            }
            x.close("ram:DefinedTradeContact");
        }
        x.open("ram:PostalTradeAddress", "");
        x.some("ram:PostcodeCode", party.postcode.as_deref());
        x.some("ram:LineOne", party.street.as_deref());
        x.some("ram:CityName", party.city.as_deref());
        x.some("ram:CountryID", party.country.as_ref().map(Country::as_str));
        x.close("ram:PostalTradeAddress");
        if let Some(endpoint) = &party.endpoint {
            x.open("ram:URIUniversalCommunication", "");
            x.leaf("ram:URIID", &attribute("schemeID", &endpoint.scheme), &endpoint.address);
            x.close("ram:URIUniversalCommunication");
        }
        for (id, scheme) in [(&party.tax_number, "FC"), (&party.vat_id, "VA")] {
            if let Some(id) = id {
                x.open("ram:SpecifiedTaxRegistration", "");
                x.leaf("ram:ID", &attribute("schemeID", scheme), id);
                x.close("ram:SpecifiedTaxRegistration");
            }
        }
        x.close(tag);
    }
    x.close("ram:ApplicableHeaderTradeAgreement");
    x.open("ram:ApplicableHeaderTradeDelivery", "");
    if let Some(delivery) = data.delivery_date {
        x.open("ram:ActualDeliverySupplyChainEvent", "");
        x.open("ram:OccurrenceDateTime", "");
        x.leaf("udt:DateTimeString", &attribute("format", "102"), &compact(delivery));
        x.close("ram:OccurrenceDateTime");
        x.close("ram:ActualDeliverySupplyChainEvent");
    }
    x.close("ram:ApplicableHeaderTradeDelivery");
    x.open("ram:ApplicableHeaderTradeSettlement", "");
    if let Some(debit) = &data.direct_debit {
        x.leaf("ram:CreditorReferenceID", "", &debit.creditor);
    }
    x.leaf("ram:InvoiceCurrencyCode", "", code);
    if let Some(means) = &data.payment_means {
        x.open("ram:SpecifiedTradeSettlementPaymentMeans", "");
        x.leaf("ram:TypeCode", "", means);
        if let Some(debit) = &data.direct_debit {
            x.open("ram:PayerPartyDebtorFinancialAccount", "");
            x.leaf("ram:IBANID", "", &debit.debited);
            x.close("ram:PayerPartyDebtorFinancialAccount");
        }
        if let Some(iban) = &data.iban {
            x.open("ram:PayeePartyCreditorFinancialAccount", "");
            x.leaf("ram:IBANID", "", iban);
            x.close("ram:PayeePartyCreditorFinancialAccount");
        }
        x.close("ram:SpecifiedTradeSettlementPaymentMeans");
    }
    for tax in &data.taxes {
        x.open("ram:ApplicableTradeTax", "");
        x.leaf("ram:CalculatedAmount", "", &amount(&tax.tax));
        x.leaf("ram:TypeCode", "", "VAT");
        x.some("ram:ExemptionReason", tax.exemption.as_deref());
        x.leaf("ram:BasisAmount", "", &amount(&tax.base));
        x.leaf("ram:CategoryCode", "", category_code(tax.category));
        x.some("ram:RateApplicablePercent", written_rate(tax.category, tax.rate).as_deref());
        x.close("ram:ApplicableTradeTax");
    }
    if data.payment_terms.is_some() || data.due_date.is_some() || data.direct_debit.is_some() {
        x.open("ram:SpecifiedTradePaymentTerms", "");
        x.some("ram:Description", data.payment_terms.as_deref());
        if let Some(due) = data.due_date {
            x.open("ram:DueDateDateTime", "");
            x.leaf("udt:DateTimeString", &attribute("format", "102"), &compact(due));
            x.close("ram:DueDateDateTime");
        }
        x.some("ram:DirectDebitMandateID", data.direct_debit.as_ref().map(|d| d.mandate.as_str()));
        x.close("ram:SpecifiedTradePaymentTerms");
    }
    x.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation", "");
    x.leaf("ram:LineTotalAmount", "", &amount(&data.totals.net));
    x.leaf("ram:TaxBasisTotalAmount", "", &amount(&data.totals.net));
    x.leaf("ram:TaxTotalAmount", &attribute("currencyID", code), &amount(&data.totals.tax));
    if let Some(rounding) = &data.totals.rounding {
        x.leaf("ram:RoundingAmount", "", &amount(rounding));
    }
    x.leaf("ram:GrandTotalAmount", "", &amount(&data.totals.gross));
    if let Some(prepaid) = &data.totals.prepaid {
        x.leaf("ram:TotalPrepaidAmount", "", &amount(prepaid));
    }
    x.leaf("ram:DuePayableAmount", "", &amount(&data.totals.due));
    x.close("ram:SpecifiedTradeSettlementHeaderMonetarySummation");
    x.close("ram:ApplicableHeaderTradeSettlement");
    x.close("rsm:SupplyChainTradeTransaction");
    x.close("rsm:CrossIndustryInvoice");
    Ok(x.out)
}

/// Attaches an invoice in CII to a PDF, as ZUGFeRD and Factur-X carry it: an embedded file `xrechnung.xml`
/// (profile XRECHNUNG) related to the document as its alternative, listed in the catalogue's associated files and
/// embedded files, and described in the document's XMP metadata. The PDF must already be a PDF/A-3 – its fonts,
/// colours and output intent are the caller's; this adds the invoice to it.
#[cfg(feature = "pdf")]
pub fn attach_to_pdf(pdf: &[u8], cii: &str) -> std::result::Result<Vec<u8>, TranslationError> {
    use lopdf::{Dictionary, Object, Stream, StringFormat, dictionary};
    const NAME: &str = "xrechnung.xml";
    let pdf_error = |e: lopdf::Error| TranslationError::Pdf(e.to_string());
    let mut document = lopdf::Document::load_mem(pdf).map_err(pdf_error)?;
    let text = |s: &str| Object::String(s.as_bytes().to_vec(), StringFormat::Literal);

    let size = i64::try_from(cii.len()).map_err(|_| TranslationError::Pdf("the invoice is too large".into()))?;
    let mut file = Stream::new(
        dictionary! { "Type" => "EmbeddedFile", "Subtype" => Object::Name(b"text/xml".to_vec()), "Params" => dictionary! { "Size" => size } },
        cii.as_bytes().to_vec(),
    );
    file.compress().map_err(pdf_error)?;
    let file = document.add_object(file);
    let spec = document.add_object(dictionary! {
        "Type" => "Filespec",
        "F" => text(NAME),
        "UF" => text(NAME),
        "Desc" => text("XRechnung"),
        "AFRelationship" => "Alternative",
        "EF" => dictionary! { "F" => file, "UF" => file },
    });

    let catalog_id = document.trailer.get(b"Root").and_then(Object::as_reference).map_err(pdf_error)?;
    // The XMP metadata: the Factur-X description added to what the document has.
    let xmp = facturx_xmp(
        document
            .get_dictionary(catalog_id)
            .ok()
            .and_then(|c| c.get(b"Metadata").and_then(Object::as_reference).ok())
            .and_then(|id| document.get_object(id).ok())
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok().or_else(|| Some(s.content.clone())))
            .and_then(|bytes| String::from_utf8(bytes).ok()),
    );
    let metadata =
        document.add_object(Stream::new(dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, xmp.into_bytes()));

    let catalog = document.get_object_mut(catalog_id).and_then(Object::as_dict_mut).map_err(pdf_error)?;
    let mut associated = catalog.get(b"AF").and_then(Object::as_array).cloned().unwrap_or_default();
    associated.push(Object::Reference(spec));
    catalog.set("AF", associated);
    catalog.set("Metadata", metadata);
    let mut names = catalog.get(b"Names").and_then(Object::as_dict).cloned().unwrap_or_else(|_| Dictionary::new());
    let mut embedded =
        names.get(b"EmbeddedFiles").and_then(Object::as_dict).cloned().unwrap_or_else(|_| Dictionary::new());
    let mut entries = embedded.get(b"Names").and_then(Object::as_array).cloned().unwrap_or_default();
    entries.push(text(NAME));
    entries.push(Object::Reference(spec));
    embedded.set("Names", entries);
    names.set("EmbeddedFiles", embedded);
    catalog.set("Names", names);

    let mut out = Vec::new();
    document.save_to(&mut out).map_err(|e| TranslationError::Pdf(e.to_string()))?;
    Ok(out)
}

/// XMP metadata with the Factur-X description of an attached XRechnung, inserted into `existing` where there is one.
#[cfg(feature = "pdf")]
fn facturx_xmp(existing: Option<String>) -> String {
    const DESCRIPTION: &str = r#"<rdf:Description rdf:about="" xmlns:fx="urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#">
<fx:DocumentType>INVOICE</fx:DocumentType>
<fx:DocumentFileName>xrechnung.xml</fx:DocumentFileName>
<fx:Version>1.0</fx:Version>
<fx:ConformanceLevel>XRECHNUNG</fx:ConformanceLevel>
</rdf:Description>
<rdf:Description rdf:about="" xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#" xmlns:pdfaProperty="http://www.aiim.org/pdfa/ns/property#">
<pdfaExtension:schemas><rdf:Bag><rdf:li rdf:parseType="Resource">
<pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>
<pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>
<pdfaSchema:prefix>fx</pdfaSchema:prefix>
<pdfaSchema:property><rdf:Seq>
<rdf:li rdf:parseType="Resource"><pdfaProperty:name>DocumentFileName</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The name of the embedded XML document</pdfaProperty:description></rdf:li>
<rdf:li rdf:parseType="Resource"><pdfaProperty:name>DocumentType</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The type of the hybrid document</pdfaProperty:description></rdf:li>
<rdf:li rdf:parseType="Resource"><pdfaProperty:name>Version</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The version of the XML schema</pdfaProperty:description></rdf:li>
<rdf:li rdf:parseType="Resource"><pdfaProperty:name>ConformanceLevel</pdfaProperty:name><pdfaProperty:valueType>Text</pdfaProperty:valueType><pdfaProperty:category>external</pdfaProperty:category><pdfaProperty:description>The conformance level of the embedded XML document</pdfaProperty:description></rdf:li>
</rdf:Seq></pdfaSchema:property>
</rdf:li></rdf:Bag></pdfaExtension:schemas>
</rdf:Description>
"#;
    match existing.filter(|x| x.contains("</rdf:RDF>")) {
        Some(xmp) => xmp.replacen("</rdf:RDF>", &format!("{DESCRIPTION}</rdf:RDF>"), 1),
        None => format!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n{DESCRIPTION}</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>"
        ),
    }
}
