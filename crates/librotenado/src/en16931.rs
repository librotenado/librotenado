// SPDX-License-Identifier: MIT
//! Translation of electronic invoices according to EN 16931 into the model (spec part 0005): both syntaxes, UBL and
//! UN/CEFACT CII – XRechnung, Peppol BIS Billing, and the XML embedded in ZUGFeRD and Factur-X PDFs.

use roxmltree::{Document as Xml, Node};

use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use crate::{
    AllowanceCharge, Classification, ContentHash, Country, CreditTransfer, Currency, CurrencyTable, Date, Decimal,
    Delivery, DirectDebit, DocumentKind, Endpoint, Identifier, InvoiceContact, InvoiceData, InvoiceLine, InvoiceNote,
    InvoiceParty, InvoiceReferences, InvoiceTotals, ItemAttribute, Money, Original, Payment, PaymentCard, Period,
    PostalAddress, PrecedingInvoice, SupportingDocument, VatBreakdown, VatCategory,
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
    /// The bytes of the files attached to it (BT-125), by their hash – stored like originals, outside the events.
    pub files: BTreeMap<ContentHash, Vec<u8>>,
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

/// An identifier from an element with an optional scheme attribute.
fn identifier(node: Option<Node>, scheme: &str) -> Option<Identifier> {
    let node = node?;
    let id = node.text().map(str::trim).filter(|t| !t.is_empty())?.to_owned();
    Some(Identifier { id, scheme: node.attribute(scheme).map(str::to_owned) })
}

/// The kind of a document from its type code (BT-3) and the type code the model keeps: none for `380` and `381`.
fn kind(type_code: &str, credit_note: bool) -> (DocumentKind, Option<String>) {
    let kind = if credit_note || type_code == "381" { DocumentKind::CreditNote } else { DocumentKind::Invoice };
    let kept = (!matches!(type_code, "380" | "381")).then(|| type_code.to_owned());
    (kind, kept)
}

/// A note as UBL writes it: its subject code between `#` before the text (Peppol BIS Billing 3.0).
fn ubl_note(text: String) -> InvoiceNote {
    if let Some(rest) = text.strip_prefix('#')
        && let Some((subject, note)) = rest.split_once('#')
        && !subject.is_empty()
        && subject.bytes().all(|b| b.is_ascii_uppercase())
    {
        return InvoiceNote { subject: Some(subject.to_owned()), text: note.to_owned() };
    }
    InvoiceNote { subject: None, text }
}

/// The code of the date VAT becomes chargeable (BT-8) in UNTDID 2005, from UNTDID 2475 as CII writes it.
fn tax_point_from_cii(code: &str) -> String {
    match code {
        "5" => "3",
        "29" => "35",
        "72" => "432",
        other => other,
    }
    .to_owned()
}

fn tax_point_to_cii(code: &str) -> &str {
    match code {
        "3" => "5",
        "35" => "29",
        "432" => "72",
        other => other,
    }
}

/// The file attached as base64, with its media type and name.
fn attachment(node: Option<Node>, files: &mut BTreeMap<ContentHash, Vec<u8>>) -> Result<Option<Original>> {
    let Some(node) = node else { return Ok(None) };
    let encoded: String = node.text().unwrap_or_default().chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = BASE64
        .decode(&encoded)
        .map_err(|_| TranslationError::Invalid { term: "BT-125", value: encoded.chars().take(32).collect() })?;
    let mut original = Original::of(&bytes, node.attribute("mimeCode").unwrap_or("application/octet-stream"));
    original.name = node.attribute("filename").map(str::to_owned);
    files.insert(original.hash, bytes);
    Ok(Some(original))
}

fn references_or_none(references: InvoiceReferences) -> Option<InvoiceReferences> {
    (references != InvoiceReferences::default()).then_some(references)
}

fn period(start: Option<Date>, end: Option<Date>) -> Option<Period> {
    (start.is_some() || end.is_some()).then_some(Period { start, end })
}

/// A postal address in UBL.
fn ubl_address(node: Option<Node>, term: &'static str) -> Result<Option<PostalAddress>> {
    let Some(a) = node else { return Ok(None) };
    let lines = [text(a, &["StreetName"]), text(a, &["AdditionalStreetName"]), text(a, &["AddressLine", "Line"])];
    Ok(Some(PostalAddress {
        lines: lines.into_iter().flatten().collect(),
        city: text(a, &["CityName"]),
        postcode: text(a, &["PostalZone"]),
        subdivision: text(a, &["CountrySubentity"]),
        country: country(text(a, &["Country", "IdentificationCode"])).ok_or(TranslationError::Missing(term))?,
    }))
}

/// A party in UBL: the seller, buyer, payee or tax representative.
fn ubl_party(party: Node, terms: [&'static str; 2]) -> Result<InvoiceParty> {
    let tax_id = |vat: bool| {
        all(party, "PartyTaxScheme")
            .find(|s| (text(*s, &["TaxScheme", "ID"]).as_deref() == Some("VAT")) == vat)
            .and_then(|s| text(s, &["CompanyID"]))
    };
    let registered = text(party, &["PartyLegalEntity", "RegistrationName"]);
    let trading = text(party, &["PartyName", "Name"]);
    let (name, trading_name) = match registered {
        Some(name) => (name, trading),
        None => (trading.ok_or(TranslationError::Missing(terms[0]))?, None),
    };
    let contact = at(party, &["Contact"]).map(|c| InvoiceContact {
        name: text(c, &["Name"]),
        phone: text(c, &["Telephone"]),
        email: text(c, &["ElectronicMail"]),
    });
    Ok(InvoiceParty {
        name,
        trading_name,
        identifiers: all(party, "PartyIdentification")
            .filter_map(|i| identifier(at(i, &["ID"]), "schemeID"))
            .filter(|i| i.scheme.as_deref() != Some("SEPA"))
            .collect(),
        registration: identifier(at(party, &["PartyLegalEntity", "CompanyID"]), "schemeID"),
        vat_id: tax_id(true),
        tax_number: tax_id(false),
        legal_info: text(party, &["PartyLegalEntity", "CompanyLegalForm"]),
        address: ubl_address(at(party, &["PostalAddress"]), terms[1])?,
        contact,
        endpoint: endpoint(at(party, &["EndpointID"])),
        party: None,
    })
}

fn ubl(root: Node, credit_note: bool, currencies: &CurrencyTable) -> Result<TranslatedInvoice> {
    let (currency, decimals) = currency(&required(root, &["DocumentCurrencyCode"], "BT-5")?, currencies)?;
    let amount = |node: Node, path: &[&str], term| -> Result<Money> {
        money(&required(node, path, term)?, &currency, decimals, term)
    };
    let optional = |node: Node, path: &[&str], term| -> Result<Option<Money>> {
        text(node, path).map(|value| money(&value, &currency, decimals, term)).transpose()
    };
    let optional_date = |node: Node, path: &[&str], term| -> Result<Option<Date>> {
        text(node, path).map(|d| date(&d, term)).transpose()
    };
    let decimal_at = |node: Node, path: &[&str], term| -> Result<Option<Decimal>> {
        text(node, path).map(|d| decimal(&d, term)).transpose()
    };
    let allowance_charge = |node: Node, document: bool| -> Result<(bool, AllowanceCharge)> {
        let charge = text(node, &["ChargeIndicator"]).as_deref() == Some("true");
        let category = match text(node, &["TaxCategory", "ID"]) {
            Some(code) if document => Some(category(&code, "BT-95")?),
            _ => None,
        };
        Ok((
            charge,
            AllowanceCharge {
                amount: amount(node, &["Amount"], "BT-92")?,
                base: optional(node, &["BaseAmount"], "BT-93")?,
                percent: decimal_at(node, &["MultiplierFactorNumeric"], "BT-94")?,
                category,
                rate: match category {
                    Some(c) => rate(text(node, &["TaxCategory", "Percent"]), c, "BT-96")?,
                    None => None,
                },
                reason: text(node, &["AllowanceChargeReason"]),
                reason_code: text(node, &["AllowanceChargeReasonCode"]),
            },
        ))
    };
    let split = |items: Vec<(bool, AllowanceCharge)>| -> (Vec<AllowanceCharge>, Vec<AllowanceCharge>) {
        let (charges, allowances): (Vec<_>, Vec<_>) = items.into_iter().partition(|(charge, _)| *charge);
        (allowances.into_iter().map(|(_, a)| a).collect(), charges.into_iter().map(|(_, c)| c).collect())
    };
    let type_code = text(root, &[if credit_note { "CreditNoteTypeCode" } else { "InvoiceTypeCode" }])
        .ok_or(TranslationError::Missing("BT-3"))?;
    let (kind, type_code) = kind(&type_code, credit_note);
    let due = text(root, &["DueDate"]).or_else(|| text(root, &["PaymentMeans", "PaymentDueDate"]));
    let tax_totals: Vec<Node> = all(root, "TaxTotal").collect();
    let tax_total = tax_totals
        .iter()
        .copied()
        .find(|t| at(*t, &["TaxAmount"]).and_then(|a| a.attribute("currencyID")) == Some(currency.as_str()));
    let tax_accounting = match text(root, &["TaxCurrencyCode"]).filter(|c| c != currency.as_str()) {
        Some(code) => {
            let (tax_currency, tax_decimals) = self::currency(&code, currencies)?;
            tax_totals
                .iter()
                .filter_map(|t| at(*t, &["TaxAmount"]))
                .find(|a| a.attribute("currencyID") == Some(code.as_str()))
                .and_then(|a| a.text())
                .map(|value| money(value.trim(), &tax_currency, tax_decimals, "BT-111"))
                .transpose()?
        }
        None => None,
    };
    let taxes = tax_total
        .into_iter()
        .flat_map(|t| all(t, "TaxSubtotal"))
        .map(|sub| {
            let category = category(&required(sub, &["TaxCategory", "ID"], "BT-118")?, "BT-118")?;
            Ok(VatBreakdown {
                category,
                rate: rate(text(sub, &["TaxCategory", "Percent"]), category, "BT-119")?,
                base: amount(sub, &["TaxableAmount"], "BT-116")?,
                tax: amount(sub, &["TaxAmount"], "BT-117")?,
                exemption: text(sub, &["TaxCategory", "TaxExemptionReason"]),
                exemption_code: text(sub, &["TaxCategory", "TaxExemptionReasonCode"]),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let line_name = if credit_note { "CreditNoteLine" } else { "InvoiceLine" };
    let lines = all(root, line_name)
        .map(|line| {
            let item = at(line, &["Item"]).ok_or(TranslationError::Missing("BG-31"))?;
            let category = category(&required(item, &["ClassifiedTaxCategory", "ID"], "BT-151")?, "BT-151")?;
            let quantity = at(line, &[if credit_note { "CreditedQuantity" } else { "InvoicedQuantity" }]);
            let price = at(line, &["Price"]);
            let price_allowance = price.and_then(|p| at(p, &["AllowanceCharge"]));
            let (allowances, charges) =
                split(all(line, "AllowanceCharge").map(|a| allowance_charge(a, false)).collect::<Result<Vec<_>>>()?);
            Ok(InvoiceLine {
                id: text(line, &["ID"]),
                note: text(line, &["Note"]),
                object: all(line, "DocumentReference")
                    .find(|d| text(*d, &["DocumentTypeCode"]).as_deref() == Some("130"))
                    .and_then(|d| identifier(at(d, &["ID"]), "schemeID")),
                order_line: text(line, &["OrderLineReference", "LineID"]),
                accounting: text(line, &["AccountingCost"]),
                period: at(line, &["InvoicePeriod"])
                    .map(|p| -> Result<_> {
                        Ok(period(
                            optional_date(p, &["StartDate"], "BT-134")?,
                            optional_date(p, &["EndDate"], "BT-135")?,
                        ))
                    })
                    .transpose()?
                    .flatten(),
                name: required(item, &["Name"], "BT-153")?,
                description: text(item, &["Description"]),
                seller_item: text(item, &["SellersItemIdentification", "ID"]),
                buyer_item: text(item, &["BuyersItemIdentification", "ID"]),
                standard_item: identifier(at(item, &["StandardItemIdentification", "ID"]), "schemeID"),
                classifications: all(item, "CommodityClassification")
                    .filter_map(|c| at(c, &["ItemClassificationCode"]))
                    .filter_map(|c| {
                        Some(Classification {
                            code: c.text()?.trim().to_owned(),
                            list: c.attribute("listID")?.to_owned(),
                            version: c.attribute("listVersionID").map(str::to_owned),
                        })
                    })
                    .collect(),
                origin: country(text(item, &["OriginCountry", "IdentificationCode"])),
                attributes: all(item, "AdditionalItemProperty")
                    .filter_map(|p| Some(ItemAttribute { name: text(p, &["Name"])?, value: text(p, &["Value"])? }))
                    .collect(),
                quantity: quantity.and_then(|q| q.text()).map(|q| decimal(q.trim(), "BT-129")).transpose()?,
                unit: quantity.and_then(|q| q.attribute("unitCode")).map(str::to_owned),
                price: price.map(|p| decimal_at(p, &["PriceAmount"], "BT-146")).transpose()?.flatten(),
                price_quantity: price.map(|p| decimal_at(p, &["BaseQuantity"], "BT-149")).transpose()?.flatten(),
                gross_price: price_allowance.map(|a| decimal_at(a, &["BaseAmount"], "BT-148")).transpose()?.flatten(),
                price_discount: price_allowance.map(|a| decimal_at(a, &["Amount"], "BT-147")).transpose()?.flatten(),
                allowances,
                charges,
                net: amount(line, &["LineExtensionAmount"], "BT-131")?,
                category,
                rate: rate(text(item, &["ClassifiedTaxCategory", "Percent"]), category, "BT-152")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (allowances, charges) =
        split(all(root, "AllowanceCharge").map(|a| allowance_charge(a, true)).collect::<Result<Vec<_>>>()?);
    let mut files = BTreeMap::new();
    let mut attachments = Vec::new();
    let mut object = None;
    let mut project = text(root, &["ProjectReference", "ID"]);
    for reference in all(root, "AdditionalDocumentReference") {
        match text(reference, &["DocumentTypeCode"]).as_deref() {
            Some("130") => object = identifier(at(reference, &["ID"]), "schemeID"),
            Some("50") if credit_note => project = text(reference, &["ID"]),
            _ => attachments.push(SupportingDocument {
                reference: required(reference, &["ID"], "BT-122")?,
                description: text(reference, &["DocumentDescription"]),
                uri: text(reference, &["Attachment", "ExternalReference", "URI"]),
                file: attachment(at(reference, &["Attachment", "EmbeddedDocumentBinaryObject"]), &mut files)?,
            }),
        }
    }
    let order = text(root, &["OrderReference", "ID"]);
    let sales_order = text(root, &["OrderReference", "SalesOrderID"]);
    let references = InvoiceReferences {
        project,
        contract: text(root, &["ContractDocumentReference", "ID"]),
        order: order.filter(|o| !(o == "NA" && sales_order.is_some())),
        sales_order,
        receiving_advice: text(root, &["ReceiptDocumentReference", "ID"]),
        despatch_advice: text(root, &["DespatchDocumentReference", "ID"]),
        tender: text(root, &["OriginatorDocumentReference", "ID"]),
        object,
        accounting: text(root, &["AccountingCost"]),
    };
    let means: Vec<Node> = all(root, "PaymentMeans").collect();
    let payment = means
        .first()
        .map(|first| -> Result<Payment> {
            let first = *first;
            Ok(Payment {
                means: required(first, &["PaymentMeansCode"], "BT-81")?,
                means_text: at(first, &["PaymentMeansCode"]).and_then(|c| c.attribute("name")).map(str::to_owned),
                remittance: text(first, &["PaymentID"]),
                transfers: means
                    .iter()
                    .filter_map(|m| at(*m, &["PayeeFinancialAccount"]))
                    .filter_map(|a| {
                        Some(CreditTransfer {
                            account: text(a, &["ID"])?,
                            name: text(a, &["Name"]),
                            provider: text(a, &["FinancialInstitutionBranch", "ID"]),
                        })
                    })
                    .collect(),
                card: at(first, &["CardAccount"]).and_then(|c| {
                    Some(PaymentCard {
                        number: text(c, &["PrimaryAccountNumberID"])?,
                        holder: text(c, &["HolderName"]),
                    })
                }),
                direct_debit: text(first, &["PaymentMandate", "ID"]).map(|mandate| DirectDebit {
                    mandate,
                    creditor: at(root, &["AccountingSupplierParty", "Party"])
                        .into_iter()
                        .flat_map(|p| all(p, "PartyIdentification"))
                        .filter_map(|i| at(i, &["ID"]))
                        .find(|id| id.attribute("schemeID") == Some("SEPA"))
                        .and_then(|id| id.text())
                        .map(|t| t.trim().to_owned())
                        .unwrap_or_default(),
                    debited: text(first, &["PaymentMandate", "PayerFinancialAccount", "ID"]).unwrap_or_default(),
                }),
            })
        })
        .transpose()?;
    let delivery = at(root, &["Delivery"]);
    let delivery_place = delivery.and_then(|d| {
        let location = at(d, &["DeliveryLocation"]);
        let place = Delivery {
            name: text(d, &["DeliveryParty", "PartyName", "Name"]),
            location: identifier(location.and_then(|l| at(l, &["ID"])), "schemeID"),
            address: location.and_then(|l| at(l, &["Address"])).and_then(|a| ubl_address(Some(a), "BT-80").ok()?),
        };
        (place.name.is_some() || place.location.is_some() || place.address.is_some()).then_some(place)
    });
    let invoice_period = at(root, &["InvoicePeriod"]);
    let totals = at(root, &["LegalMonetaryTotal"]).ok_or(TranslationError::Missing("BG-22"))?;
    let seller = at(root, &["AccountingSupplierParty", "Party"]).ok_or(TranslationError::Missing("BG-4"))?;
    let buyer = at(root, &["AccountingCustomerParty", "Party"]).ok_or(TranslationError::Missing("BG-7"))?;
    Ok(TranslatedInvoice {
        format: "en16931-ubl",
        kind,
        number: required(root, &["ID"], "BT-1")?,
        date: date(&required(root, &["IssueDate"], "BT-2")?, "BT-2")?,
        invoice: InvoiceData {
            type_code,
            currency: currency.clone(),
            notes: all(root, "Note").filter_map(|n| n.text()).map(|t| ubl_note(t.trim().to_owned())).collect(),
            seller: ubl_party(seller, ["BT-27", "BT-40"])?,
            buyer: ubl_party(buyer, ["BT-44", "BT-55"])?,
            payee: at(root, &["PayeeParty"]).map(|p| ubl_party(p, ["BT-59", "BT-40"])).transpose()?,
            tax_representative: at(root, &["TaxRepresentativeParty"])
                .map(|p| ubl_party(p, ["BT-62", "BT-69"]))
                .transpose()?,
            buyer_reference: text(root, &["BuyerReference"]),
            references: references_or_none(references),
            preceding: all(root, "BillingReference")
                .filter_map(|b| at(b, &["InvoiceDocumentReference"]))
                .map(|r| {
                    Ok(PrecedingInvoice {
                        number: required(r, &["ID"], "BT-25")?,
                        date: optional_date(r, &["IssueDate"], "BT-26")?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            delivery_date: delivery.map(|d| optional_date(d, &["ActualDeliveryDate"], "BT-72")).transpose()?.flatten(),
            period: invoice_period
                .map(|p| -> Result<_> {
                    Ok(period(optional_date(p, &["StartDate"], "BT-73")?, optional_date(p, &["EndDate"], "BT-74")?))
                })
                .transpose()?
                .flatten(),
            delivery: delivery_place,
            tax_point_date: optional_date(root, &["TaxPointDate"], "BT-7")?,
            tax_point_code: invoice_period.and_then(|p| text(p, &["DescriptionCode"])),
            due_date: due.map(|d| date(&d, "BT-9")).transpose()?,
            payment_terms: text(root, &["PaymentTerms", "Note"]),
            payment,
            allowances,
            charges,
            taxes,
            attachments,
            lines,
            totals: InvoiceTotals {
                net: amount(totals, &["TaxExclusiveAmount"], "BT-109")?,
                tax: match tax_total {
                    Some(t) => amount(t, &["TaxAmount"], "BT-110")?,
                    None => Money::new(0, currency.clone()).ok_or(TranslationError::Missing("BT-110"))?,
                },
                tax_accounting,
                gross: amount(totals, &["TaxInclusiveAmount"], "BT-112")?,
                prepaid: optional(totals, &["PrepaidAmount"], "BT-113")?,
                rounding: optional(totals, &["PayableRoundingAmount"], "BT-114")?,
                due: amount(totals, &["PayableAmount"], "BT-115")?,
            },
        },
        files,
    })
}

/// A postal address in CII.
fn cii_address(node: Option<Node>, term: &'static str) -> Result<Option<PostalAddress>> {
    let Some(a) = node else { return Ok(None) };
    let lines = [text(a, &["LineOne"]), text(a, &["LineTwo"]), text(a, &["LineThree"])];
    Ok(Some(PostalAddress {
        lines: lines.into_iter().flatten().collect(),
        city: text(a, &["CityName"]),
        postcode: text(a, &["PostcodeCode"]),
        subdivision: text(a, &["CountrySubDivisionName"]),
        country: country(text(a, &["CountryID"])).ok_or(TranslationError::Missing(term))?,
    }))
}

/// The identifiers of a party in CII: `ID` without a scheme, `GlobalID` with one.
fn cii_identifiers(party: Node) -> Vec<Identifier> {
    party
        .children()
        .filter(|c| c.is_element() && matches!(c.tag_name().name(), "ID" | "GlobalID"))
        .filter_map(|c| identifier(Some(c), "schemeID"))
        .collect()
}

/// A party in CII: the seller, buyer, payee or tax representative.
fn cii_party(party: Node, terms: [&'static str; 2]) -> Result<InvoiceParty> {
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
        name: required(party, &["Name"], terms[0])?,
        trading_name: text(party, &["SpecifiedLegalOrganization", "TradingBusinessName"]),
        identifiers: cii_identifiers(party),
        registration: identifier(at(party, &["SpecifiedLegalOrganization", "ID"]), "schemeID"),
        vat_id: registration(&["VA", "VAT"]),
        tax_number: registration(&["FC"]),
        legal_info: text(party, &["Description"]),
        address: cii_address(at(party, &["PostalTradeAddress"]), terms[1])?,
        contact,
        endpoint: endpoint(at(party, &["URIUniversalCommunication", "URIID"])),
        party: None,
    })
}

fn cii(root: Node, currencies: &CurrencyTable) -> Result<TranslatedInvoice> {
    let document = at(root, &["ExchangedDocument"]).ok_or(TranslationError::Missing("BT-1"))?;
    let transaction = at(root, &["SupplyChainTradeTransaction"]).ok_or(TranslationError::Syntax)?;
    let agreement = at(transaction, &["ApplicableHeaderTradeAgreement"]).ok_or(TranslationError::Missing("BG-4"))?;
    let delivery = at(transaction, &["ApplicableHeaderTradeDelivery"]);
    let settlement = at(transaction, &["ApplicableHeaderTradeSettlement"]).ok_or(TranslationError::Missing("BG-22"))?;
    let (currency, decimals) = currency(&required(settlement, &["InvoiceCurrencyCode"], "BT-5")?, currencies)?;
    let amount = |node: Node, path: &[&str], term| -> Result<Money> {
        money(&required(node, path, term)?, &currency, decimals, term)
    };
    let optional = |node: Node, path: &[&str], term| -> Result<Option<Money>> {
        text(node, path).map(|value| money(&value, &currency, decimals, term)).transpose()
    };
    let date_time = |node: Node, path: &[&str], term| -> Result<Option<Date>> {
        text(node, path).map(|d| date(&d, term)).transpose()
    };
    let decimal_at = |node: Node, path: &[&str], term| -> Result<Option<Decimal>> {
        text(node, path).map(|d| decimal(&d, term)).transpose()
    };
    let billing_period = |node: Node, terms: [&'static str; 2]| -> Result<Option<Period>> {
        let Some(p) = at(node, &["BillingSpecifiedPeriod"]) else { return Ok(None) };
        Ok(period(
            date_time(p, &["StartDateTime", "DateTimeString"], terms[0])?,
            date_time(p, &["EndDateTime", "DateTimeString"], terms[1])?,
        ))
    };
    let allowances_charges = |node: Node, document: bool| -> Result<(Vec<AllowanceCharge>, Vec<AllowanceCharge>)> {
        let (mut allowances, mut charges) = (Vec::new(), Vec::new());
        for a in all(node, "SpecifiedTradeAllowanceCharge") {
            let category = match text(a, &["CategoryTradeTax", "CategoryCode"]) {
                Some(code) if document => Some(category(&code, "BT-95")?),
                _ => None,
            };
            let item = AllowanceCharge {
                amount: amount(a, &["ActualAmount"], "BT-92")?,
                base: optional(a, &["BasisAmount"], "BT-93")?,
                percent: decimal_at(a, &["CalculationPercent"], "BT-94")?,
                category,
                rate: match category {
                    Some(c) => rate(text(a, &["CategoryTradeTax", "RateApplicablePercent"]), c, "BT-96")?,
                    None => None,
                },
                reason: text(a, &["Reason"]),
                reason_code: text(a, &["ReasonCode"]),
            };
            if text(a, &["ChargeIndicator", "Indicator"]).as_deref() == Some("true") {
                charges.push(item);
            } else {
                allowances.push(item);
            }
        }
        Ok((allowances, charges))
    };
    let (kind, type_code) = kind(&required(document, &["TypeCode"], "BT-3")?, false);
    let taxes = all(settlement, "ApplicableTradeTax")
        .map(|tax| {
            let category = category(&required(tax, &["CategoryCode"], "BT-118")?, "BT-118")?;
            Ok(VatBreakdown {
                category,
                rate: rate(text(tax, &["RateApplicablePercent"]), category, "BT-119")?,
                base: amount(tax, &["BasisAmount"], "BT-116")?,
                tax: amount(tax, &["CalculatedAmount"], "BT-117")?,
                exemption: text(tax, &["ExemptionReason"]),
                exemption_code: text(tax, &["ExemptionReasonCode"]),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let first_tax = at(settlement, &["ApplicableTradeTax"]);
    let lines = all(transaction, "IncludedSupplyChainTradeLineItem")
        .map(|line| {
            let product = at(line, &["SpecifiedTradeProduct"]).ok_or(TranslationError::Missing("BG-31"))?;
            let line_agreement = at(line, &["SpecifiedLineTradeAgreement"]);
            let line_settlement =
                at(line, &["SpecifiedLineTradeSettlement"]).ok_or(TranslationError::Missing("BG-30"))?;
            let tax = at(line_settlement, &["ApplicableTradeTax"]);
            let category = category(
                &tax.and_then(|t| text(t, &["CategoryCode"])).ok_or(TranslationError::Missing("BT-151"))?,
                "BT-151",
            )?;
            let quantity = at(line, &["SpecifiedLineTradeDelivery", "BilledQuantity"]);
            let net_price = line_agreement.and_then(|a| at(a, &["NetPriceProductTradePrice"]));
            let gross_price = line_agreement.and_then(|a| at(a, &["GrossPriceProductTradePrice"]));
            let (allowances, charges) = allowances_charges(line_settlement, false)?;
            Ok(InvoiceLine {
                id: text(line, &["AssociatedDocumentLineDocument", "LineID"]),
                note: text(line, &["AssociatedDocumentLineDocument", "IncludedNote", "Content"]),
                object: all(line_settlement, "AdditionalReferencedDocument")
                    .find(|d| text(*d, &["TypeCode"]).as_deref() == Some("130"))
                    .and_then(|d| {
                        let mut id = identifier(at(d, &["IssuerAssignedID"]), "schemeID")?;
                        id.scheme = text(d, &["ReferenceTypeCode"]);
                        Some(id)
                    }),
                order_line: line_agreement.and_then(|a| text(a, &["BuyerOrderReferencedDocument", "LineID"])),
                accounting: text(line_settlement, &["ReceivableSpecifiedTradeAccountingAccount", "ID"]),
                period: billing_period(line_settlement, ["BT-134", "BT-135"])?,
                name: required(product, &["Name"], "BT-153")?,
                description: text(product, &["Description"]),
                seller_item: text(product, &["SellerAssignedID"]),
                buyer_item: text(product, &["BuyerAssignedID"]),
                standard_item: identifier(at(product, &["GlobalID"]), "schemeID"),
                classifications: all(product, "DesignatedProductClassification")
                    .filter_map(|c| at(c, &["ClassCode"]))
                    .filter_map(|c| {
                        Some(Classification {
                            code: c.text()?.trim().to_owned(),
                            list: c.attribute("listID")?.to_owned(),
                            version: c.attribute("listVersionID").map(str::to_owned),
                        })
                    })
                    .collect(),
                origin: country(text(product, &["OriginTradeCountry", "ID"])),
                attributes: all(product, "ApplicableProductCharacteristic")
                    .filter_map(|p| {
                        Some(ItemAttribute { name: text(p, &["Description"])?, value: text(p, &["Value"])? })
                    })
                    .collect(),
                quantity: quantity.and_then(|q| q.text()).map(|q| decimal(q.trim(), "BT-129")).transpose()?,
                unit: quantity.and_then(|q| q.attribute("unitCode")).map(str::to_owned),
                price: net_price.map(|p| decimal_at(p, &["ChargeAmount"], "BT-146")).transpose()?.flatten(),
                price_quantity: net_price
                    .and_then(|p| at(p, &["BasisQuantity"]))
                    .or_else(|| gross_price.and_then(|p| at(p, &["BasisQuantity"])))
                    .and_then(|q| q.text())
                    .map(|q| decimal(q.trim(), "BT-149"))
                    .transpose()?,
                gross_price: gross_price.map(|p| decimal_at(p, &["ChargeAmount"], "BT-148")).transpose()?.flatten(),
                price_discount: gross_price
                    .and_then(|p| at(p, &["AppliedTradeAllowanceCharge"]))
                    .map(|a| decimal_at(a, &["ActualAmount"], "BT-147"))
                    .transpose()?
                    .flatten(),
                allowances,
                charges,
                net: amount(
                    line_settlement,
                    &["SpecifiedTradeSettlementLineMonetarySummation", "LineTotalAmount"],
                    "BT-131",
                )?,
                category,
                rate: rate(tax.and_then(|t| text(t, &["RateApplicablePercent"])), category, "BT-152")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (allowances, charges) = allowances_charges(settlement, true)?;
    let totals = at(settlement, &["SpecifiedTradeSettlementHeaderMonetarySummation"])
        .ok_or(TranslationError::Missing("BG-22"))?;
    let tax_total = all(totals, "TaxTotalAmount")
        .find(|t| t.attribute("currencyID").is_none_or(|c| c == currency.as_str()))
        .and_then(|t| t.text());
    let tax_accounting = match text(settlement, &["TaxCurrencyCode"]).filter(|c| c != currency.as_str()) {
        Some(code) => {
            let (tax_currency, tax_decimals) = self::currency(&code, currencies)?;
            all(totals, "TaxTotalAmount")
                .find(|t| t.attribute("currencyID") == Some(code.as_str()))
                .and_then(|t| t.text())
                .map(|value| money(value.trim(), &tax_currency, tax_decimals, "BT-111"))
                .transpose()?
        }
        None => None,
    };
    let mut files = BTreeMap::new();
    let mut attachments = Vec::new();
    let (mut tender, mut object) = (None, None);
    for reference in all(agreement, "AdditionalReferencedDocument") {
        match text(reference, &["TypeCode"]).as_deref() {
            Some("50") => tender = text(reference, &["IssuerAssignedID"]),
            Some("130") => {
                object = text(reference, &["IssuerAssignedID"])
                    .map(|id| Identifier { id, scheme: text(reference, &["ReferenceTypeCode"]) })
            }
            _ => attachments.push(SupportingDocument {
                reference: required(reference, &["IssuerAssignedID"], "BT-122")?,
                description: text(reference, &["Name"]),
                uri: text(reference, &["URIID"]),
                file: attachment(at(reference, &["AttachmentBinaryObject"]), &mut files)?,
            }),
        }
    }
    let references = InvoiceReferences {
        project: text(agreement, &["SpecifiedProcuringProject", "ID"]),
        contract: text(agreement, &["ContractReferencedDocument", "IssuerAssignedID"]),
        order: text(agreement, &["BuyerOrderReferencedDocument", "IssuerAssignedID"]),
        sales_order: text(agreement, &["SellerOrderReferencedDocument", "IssuerAssignedID"]),
        receiving_advice: delivery.and_then(|d| text(d, &["ReceivingAdviceReferencedDocument", "IssuerAssignedID"])),
        despatch_advice: delivery.and_then(|d| text(d, &["DespatchAdviceReferencedDocument", "IssuerAssignedID"])),
        tender,
        object,
        accounting: text(settlement, &["ReceivableSpecifiedTradeAccountingAccount", "ID"]),
    };
    let means: Vec<Node> = all(settlement, "SpecifiedTradeSettlementPaymentMeans").collect();
    let terms = at(settlement, &["SpecifiedTradePaymentTerms"]);
    let payment = means
        .first()
        .map(|first| -> Result<Payment> {
            let first = *first;
            Ok(Payment {
                means: required(first, &["TypeCode"], "BT-81")?,
                means_text: text(first, &["Information"]),
                remittance: text(settlement, &["PaymentReference"]),
                transfers: means
                    .iter()
                    .filter_map(|m| {
                        let account = at(*m, &["PayeePartyCreditorFinancialAccount"])?;
                        Some(CreditTransfer {
                            account: text(account, &["IBANID"]).or_else(|| text(account, &["ProprietaryID"]))?,
                            name: text(account, &["AccountName"]),
                            provider: text(*m, &["PayeeSpecifiedCreditorFinancialInstitution", "BICID"]),
                        })
                    })
                    .collect(),
                card: at(first, &["ApplicableTradeSettlementFinancialCard"]).and_then(|c| {
                    Some(PaymentCard { number: text(c, &["ID"])?, holder: text(c, &["CardholderName"]) })
                }),
                direct_debit: terms.and_then(|t| text(t, &["DirectDebitMandateID"])).map(|mandate| DirectDebit {
                    mandate,
                    creditor: text(settlement, &["CreditorReferenceID"]).unwrap_or_default(),
                    debited: text(first, &["PayerPartyDebtorFinancialAccount", "IBANID"]).unwrap_or_default(),
                }),
            })
        })
        .transpose()?;
    let ship_to = delivery.and_then(|d| at(d, &["ShipToTradeParty"]));
    let delivery_place = ship_to
        .map(|s| -> Result<Delivery> {
            Ok(Delivery {
                name: text(s, &["Name"]),
                location: cii_identifiers(s).into_iter().next(),
                address: cii_address(at(s, &["PostalTradeAddress"]), "BT-80")?,
            })
        })
        .transpose()?;
    let seller = at(agreement, &["SellerTradeParty"]).ok_or(TranslationError::Missing("BG-4"))?;
    let buyer = at(agreement, &["BuyerTradeParty"]).ok_or(TranslationError::Missing("BG-7"))?;
    Ok(TranslatedInvoice {
        format: "en16931-cii",
        kind,
        number: required(document, &["ID"], "BT-1")?,
        date: date(&required(document, &["IssueDateTime", "DateTimeString"], "BT-2")?, "BT-2")?,
        invoice: InvoiceData {
            type_code,
            currency: currency.clone(),
            notes: all(document, "IncludedNote")
                .filter_map(|n| Some(InvoiceNote { subject: text(n, &["SubjectCode"]), text: text(n, &["Content"])? }))
                .collect(),
            seller: cii_party(seller, ["BT-27", "BT-40"])?,
            buyer: cii_party(buyer, ["BT-44", "BT-55"])?,
            payee: at(settlement, &["PayeeTradeParty"]).map(|p| cii_party(p, ["BT-59", "BT-40"])).transpose()?,
            tax_representative: at(agreement, &["SellerTaxRepresentativeTradeParty"])
                .map(|p| cii_party(p, ["BT-62", "BT-69"]))
                .transpose()?,
            buyer_reference: text(agreement, &["BuyerReference"]),
            references: references_or_none(references),
            preceding: all(settlement, "InvoiceReferencedDocument")
                .map(|r| {
                    Ok(PrecedingInvoice {
                        number: required(r, &["IssuerAssignedID"], "BT-25")?,
                        date: date_time(r, &["FormattedIssueDateTime", "DateTimeString"], "BT-26")?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            delivery_date: delivery
                .map(|d| {
                    date_time(d, &["ActualDeliverySupplyChainEvent", "OccurrenceDateTime", "DateTimeString"], "BT-72")
                })
                .transpose()?
                .flatten(),
            period: billing_period(settlement, ["BT-73", "BT-74"])?,
            delivery: delivery_place,
            tax_point_date: first_tax
                .map(|t| date_time(t, &["TaxPointDate", "DateString"], "BT-7"))
                .transpose()?
                .flatten(),
            tax_point_code: first_tax.and_then(|t| text(t, &["DueDateTypeCode"])).map(|c| tax_point_from_cii(&c)),
            due_date: terms
                .map(|t| date_time(t, &["DueDateDateTime", "DateTimeString"], "BT-9"))
                .transpose()?
                .flatten(),
            payment_terms: terms.and_then(|t| text(t, &["Description"])),
            payment,
            allowances,
            charges,
            taxes,
            attachments,
            lines,
            totals: InvoiceTotals {
                net: amount(totals, &["TaxBasisTotalAmount"], "BT-109")?,
                tax: money(tax_total.map_or("0", str::trim), &currency, decimals, "BT-110")?,
                tax_accounting,
                gross: amount(totals, &["GrandTotalAmount"], "BT-112")?,
                prepaid: optional(totals, &["TotalPrepaidAmount"], "BT-113")?,
                rounding: optional(totals, &["RoundingAmount"], "BT-114")?,
                due: amount(totals, &["DuePayableAmount"], "BT-115")?,
            },
        },
        files,
    })
}

/// The specification identifier (BT-24) of XRechnung 3.0.
pub const XRECHNUNG: &str = "urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0";
/// The business process (BT-23) of Peppol BIS Billing 3.0, which XRechnung uses.
const PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";
/// The types of invoice XRechnung 3.0 allows (BR-DE-17).
const XRECHNUNG_TYPES: [&str; 8] = ["326", "380", "384", "389", "381", "875", "876", "877"];

/// Why an invoice cannot be written as XRechnung.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    /// Terms XRechnung requires are missing, by their identifiers.
    #[error("missing for XRechnung: {}", .0.join(", "))]
    Missing(Vec<&'static str>),
    /// The net total is not the sum of the lines less the allowances plus the charges on the invoice.
    #[error("the net total is not the lines less allowances plus charges")]
    Lines,
    /// The amount due is not the gross amount less prepaid amounts plus rounding.
    #[error("the amount due is not gross less prepaid plus rounding")]
    Totals,
    /// A type of invoice XRechnung does not allow.
    #[error("XRechnung does not allow the type of invoice {0}")]
    Type(String),
    /// A term repeated more often than the syntax carries it: CII D16B has one preceding invoice (BG-3).
    #[error("the syntax carries {0} only once")]
    Syntax(&'static str),
    /// A currency the currency table does not divide by decimals.
    #[error("currency {0} is not in the currency table")]
    Currency(String),
    /// The bytes of an attached file are not among the invoice's files.
    #[error("the bytes of the attached file {0} are missing")]
    File(String),
}

/// The type code of an invoice (BT-3).
fn type_code(invoice: &TranslatedInvoice) -> &str {
    match (&invoice.invoice.type_code, invoice.kind) {
        (Some(code), _) => code,
        (None, DocumentKind::CreditNote) => "381",
        (None, _) => "380",
    }
}

/// Checks that an invoice carries what XRechnung 3.0 requires beyond EN 16931 and the model's own terms, and that
/// its totals agree.
pub fn check_xrechnung(invoice: &TranslatedInvoice) -> std::result::Result<(), WriteError> {
    let data = &invoice.invoice;
    let code = type_code(invoice);
    if !XRECHNUNG_TYPES.contains(&code) {
        return Err(WriteError::Type(code.to_owned()));
    }
    let mut missing = Vec::new();
    let mut need = |present: bool, term: &'static str| {
        if !present {
            missing.push(term);
        }
    };
    let (seller, buyer) = (&data.seller, &data.buyer);
    need(data.buyer_reference.is_some(), "BT-10");
    need(data.payment.is_some(), "BG-16");
    need(seller.address.is_some(), "BG-5");
    need(seller.address.as_ref().is_some_and(|a| a.city.is_some()), "BT-37");
    need(seller.address.as_ref().is_some_and(|a| a.postcode.is_some()), "BT-38");
    let contact = seller.contact.as_ref();
    need(contact.is_some_and(|c| c.name.is_some()), "BT-41");
    need(contact.is_some_and(|c| c.phone.is_some()), "BT-42");
    need(contact.is_some_and(|c| c.email.is_some()), "BT-43");
    need(
        !seller.identifiers.is_empty() || seller.registration.is_some() || seller.vat_id.is_some(),
        "BT-29/BT-30/BT-31",
    );
    let taxed = data.taxes.iter().any(|t| t.category != VatCategory::NotSubject);
    let representative_vat = data.tax_representative.as_ref().is_some_and(|r| r.vat_id.is_some());
    need(!taxed || seller.vat_id.is_some() || seller.tax_number.is_some() || representative_vat, "BT-31/BT-32");
    need(seller.endpoint.is_some(), "BT-34");
    need(buyer.endpoint.is_some(), "BT-49");
    need(buyer.address.is_some(), "BG-8");
    need(buyer.address.as_ref().is_some_and(|a| a.city.is_some()), "BT-52");
    need(buyer.address.as_ref().is_some_and(|a| a.postcode.is_some()), "BT-53");
    if let Some(address) = data.delivery.as_ref().and_then(|d| d.address.as_ref()) {
        need(address.city.is_some(), "BT-77");
        need(address.postcode.is_some(), "BT-78");
    }
    if let Some(payment) = &data.payment {
        match payment.means.as_str() {
            "30" | "58" => need(!payment.transfers.is_empty(), "BG-17"),
            "48" | "54" | "55" => need(payment.card.is_some(), "BG-18"),
            "59" => {
                let debit = payment.direct_debit.as_ref();
                need(debit.is_some_and(|d| !d.mandate.is_empty()), "BT-89");
                need(debit.is_some_and(|d| !d.creditor.is_empty()), "BT-90");
                need(debit.is_some_and(|d| !d.debited.is_empty()), "BT-91");
            }
            _ => {}
        }
    }
    for allowance in data.allowances.iter().chain(&data.charges) {
        need(allowance.category.is_some(), "BT-95/BT-102");
        need(allowance.reason.is_some() || allowance.reason_code.is_some(), "BT-97/BT-104");
    }
    need(!data.lines.is_empty(), "BG-25");
    for line in &data.lines {
        need(line.id.is_some(), "BT-126");
        need(line.quantity.is_some(), "BT-129");
        need(line.unit.is_some(), "BT-130");
        need(line.price.is_some(), "BT-146");
        for allowance in line.allowances.iter().chain(&line.charges) {
            need(allowance.reason.is_some() || allowance.reason_code.is_some(), "BT-139/BT-144");
        }
    }
    for tax in &data.taxes {
        let stated = !matches!(
            tax.category,
            VatCategory::Standard | VatCategory::ZeroRated | VatCategory::CanaryIslands | VatCategory::CeutaMelilla
        );
        need(!stated || tax.exemption.is_some() || tax.exemption_code.is_some(), "BT-120/BT-121");
    }
    for file in data.attachments.iter().filter_map(|a| a.file.as_ref()) {
        need(file.name.is_some(), "BT-125");
    }
    missing.sort_unstable();
    missing.dedup();
    if !missing.is_empty() {
        return Err(WriteError::Missing(missing));
    }
    for file in data.attachments.iter().filter_map(|a| a.file.as_ref()) {
        if !invoice.files.get(&file.hash).is_some_and(|bytes| file.is(bytes)) {
            return Err(WriteError::File(file.hash.to_string()));
        }
    }
    let sum = |items: &[AllowanceCharge]| items.iter().map(|a| a.amount.base_units()).sum::<i128>();
    let lines: i128 = data.lines.iter().map(|l| l.net.base_units()).sum();
    if lines - sum(&data.allowances) + sum(&data.charges) != data.totals.net.base_units() {
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

    /// An element holding one element: `<a><b>value</b></a>`.
    fn wrapped(&mut self, outer: &str, tag: &str, attributes: &str, value: &str) {
        self.open(outer, "");
        self.leaf(tag, attributes, value);
        self.close(outer);
    }

    /// An identifier with its scheme, if it has one, in the attribute named.
    fn identifier(&mut self, tag: &str, scheme: &str, id: &Identifier) {
        let attributes = id.scheme.as_deref().map(|s| attribute(scheme, s)).unwrap_or_default();
        self.leaf(tag, &attributes, &id.id);
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

/// The rate of a VAT breakdown, which XRechnung requires also where the category has none (BR-DE-14): 0 there.
fn breakdown_rate(rate: Option<Decimal>) -> String {
    rate.map_or_else(|| "0".to_owned(), |r| r.to_string())
}

/// The terms of payment as written: XRechnung ends every structured line such as `#SKONTO#…#` with a line break
/// (BR-DE-18), which reading trims.
fn written_terms(terms: &str) -> String {
    if terms.lines().last().is_some_and(|l| l.starts_with('#') && l.ends_with('#')) {
        format!("{terms}\n")
    } else {
        terms.to_owned()
    }
}

fn iso(date: Date) -> String {
    date.to_string()
}

fn compact(date: Date) -> String {
    date.to_string().replace('-', "")
}

/// The decimals of the invoice's currency.
fn decimals_of(currency: &Currency, currencies: &CurrencyTable) -> std::result::Result<u32, WriteError> {
    currencies.get(currency).and_then(|d| d.decimals()).ok_or_else(|| WriteError::Currency(currency.to_string()))
}

/// Whether an account is written as an IBAN: two letters, two digits, then letters and digits.
fn is_iban(account: &str) -> bool {
    let bytes = account.as_bytes();
    bytes.len() > 4
        && bytes[..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..4].iter().all(u8::is_ascii_digit)
        && bytes[4..].iter().all(u8::is_ascii_alphanumeric)
}

/// The role a party has on an invoice, which decides how UBL writes its name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Seller,
    Buyer,
    Payee,
    TaxRepresentative,
}

fn write_ubl_address(x: &mut Writer, tag: &str, address: &PostalAddress) {
    x.open(tag, "");
    x.some("cbc:StreetName", address.lines.first().map(String::as_str));
    x.some("cbc:AdditionalStreetName", address.lines.get(1).map(String::as_str));
    x.some("cbc:CityName", address.city.as_deref());
    x.some("cbc:PostalZone", address.postcode.as_deref());
    x.some("cbc:CountrySubentity", address.subdivision.as_deref());
    if let Some(line) = address.lines.get(2) {
        x.wrapped("cac:AddressLine", "cbc:Line", "", line);
    }
    x.wrapped("cac:Country", "cbc:IdentificationCode", "", address.country.as_str());
    x.close(tag);
}

fn write_ubl_party(x: &mut Writer, party: &InvoiceParty, role: Role, creditor: Option<&str>) {
    let legal = matches!(role, Role::Seller | Role::Buyer);
    if let Some(endpoint) = &party.endpoint {
        x.leaf("cbc:EndpointID", &attribute("schemeID", &endpoint.scheme), &endpoint.address);
    }
    for id in &party.identifiers {
        x.open("cac:PartyIdentification", "");
        x.identifier("cbc:ID", "schemeID", id);
        x.close("cac:PartyIdentification");
    }
    if let Some(creditor) = creditor {
        x.wrapped("cac:PartyIdentification", "cbc:ID", &attribute("schemeID", "SEPA"), creditor);
    }
    match (legal, &party.trading_name) {
        (true, Some(trading)) => x.wrapped("cac:PartyName", "cbc:Name", "", trading),
        (true, None) => {}
        (false, _) => x.wrapped("cac:PartyName", "cbc:Name", "", &party.name),
    }
    if let Some(address) = &party.address {
        write_ubl_address(x, "cac:PostalAddress", address);
    }
    for (id, scheme) in [(&party.vat_id, "VAT"), (&party.tax_number, "FC")] {
        if let Some(id) = id {
            x.open("cac:PartyTaxScheme", "");
            x.leaf("cbc:CompanyID", "", id);
            x.wrapped("cac:TaxScheme", "cbc:ID", "", scheme);
            x.close("cac:PartyTaxScheme");
        }
    }
    if legal || party.registration.is_some() {
        x.open("cac:PartyLegalEntity", "");
        if legal {
            x.leaf("cbc:RegistrationName", "", &party.name);
        }
        if let Some(registration) = &party.registration {
            x.identifier("cbc:CompanyID", "schemeID", registration);
        }
        x.some("cbc:CompanyLegalForm", party.legal_info.as_deref());
        x.close("cac:PartyLegalEntity");
    }
    if let Some(contact) = &party.contact {
        x.open("cac:Contact", "");
        x.some("cbc:Name", contact.name.as_deref());
        x.some("cbc:Telephone", contact.phone.as_deref());
        x.some("cbc:ElectronicMail", contact.email.as_deref());
        x.close("cac:Contact");
    }
}

fn write_ubl_allowance_charge(x: &mut Writer, item: &AllowanceCharge, charge: bool, amount: &dyn Fn(&Money) -> String) {
    let currency = attribute("currencyID", item.amount.currency().as_str());
    x.open("cac:AllowanceCharge", "");
    x.leaf("cbc:ChargeIndicator", "", if charge { "true" } else { "false" });
    x.some("cbc:AllowanceChargeReasonCode", item.reason_code.as_deref());
    x.some("cbc:AllowanceChargeReason", item.reason.as_deref());
    x.some("cbc:MultiplierFactorNumeric", item.percent.map(|p| p.to_string()).as_deref());
    x.leaf("cbc:Amount", &currency, &amount(&item.amount));
    if let Some(base) = &item.base {
        x.leaf("cbc:BaseAmount", &currency, &amount(base));
    }
    if let Some(category) = item.category {
        x.open("cac:TaxCategory", "");
        x.leaf("cbc:ID", "", category_code(category));
        x.some("cbc:Percent", written_rate(category, item.rate).as_deref());
        x.wrapped("cac:TaxScheme", "cbc:ID", "", "VAT");
        x.close("cac:TaxCategory");
    }
    x.close("cac:AllowanceCharge");
}

/// Writes an invoice or credit note as XRechnung 3.0 in the UBL syntax.
pub fn write_ubl(invoice: &TranslatedInvoice, currencies: &CurrencyTable) -> std::result::Result<String, WriteError> {
    check_xrechnung(invoice)?;
    let data = &invoice.invoice;
    let code = data.currency.as_str();
    let decimals = decimals_of(&data.currency, currencies)?;
    let amount = |m: &Money| written(m, decimals);
    let currency = attribute("currencyID", code);
    let credit_note = invoice.kind == DocumentKind::CreditNote;
    let root = if credit_note { "ubl:CreditNote" } else { "ubl:Invoice" };
    let namespace = if credit_note { UBL_CREDIT_NOTE } else { UBL_INVOICE };
    let references = data.references.clone().unwrap_or_default();
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
    if credit_note {
        x.some("cbc:TaxPointDate", data.tax_point_date.map(iso).as_deref());
        x.leaf("cbc:CreditNoteTypeCode", "", type_code(invoice));
    } else {
        x.some("cbc:DueDate", data.due_date.map(iso).as_deref());
        x.leaf("cbc:InvoiceTypeCode", "", type_code(invoice));
    }
    for note in &data.notes {
        let text = match &note.subject {
            Some(subject) => format!("#{subject}#{}", note.text),
            None => note.text.clone(),
        };
        x.leaf("cbc:Note", "", &text);
    }
    if !credit_note {
        x.some("cbc:TaxPointDate", data.tax_point_date.map(iso).as_deref());
    }
    x.leaf("cbc:DocumentCurrencyCode", "", code);
    if let Some(tax) = &data.totals.tax_accounting {
        x.leaf("cbc:TaxCurrencyCode", "", tax.currency().as_str());
    }
    x.some("cbc:AccountingCost", references.accounting.as_deref());
    x.some("cbc:BuyerReference", data.buyer_reference.as_deref());
    if data.period.is_some() || data.tax_point_code.is_some() {
        let period = data.period.as_ref();
        x.open("cac:InvoicePeriod", "");
        x.some("cbc:StartDate", period.and_then(|p| p.start).map(iso).as_deref());
        x.some("cbc:EndDate", period.and_then(|p| p.end).map(iso).as_deref());
        x.some("cbc:DescriptionCode", data.tax_point_code.as_deref());
        x.close("cac:InvoicePeriod");
    }
    if references.order.is_some() || references.sales_order.is_some() {
        x.open("cac:OrderReference", "");
        x.leaf("cbc:ID", "", references.order.as_deref().unwrap_or("NA"));
        x.some("cbc:SalesOrderID", references.sales_order.as_deref());
        x.close("cac:OrderReference");
    }
    for preceding in &data.preceding {
        x.open("cac:BillingReference", "");
        x.open("cac:InvoiceDocumentReference", "");
        x.leaf("cbc:ID", "", &preceding.number);
        x.some("cbc:IssueDate", preceding.date.map(iso).as_deref());
        x.close("cac:InvoiceDocumentReference");
        x.close("cac:BillingReference");
    }
    if let Some(despatch) = &references.despatch_advice {
        x.wrapped("cac:DespatchDocumentReference", "cbc:ID", "", despatch);
    }
    if let Some(receipt) = &references.receiving_advice {
        x.wrapped("cac:ReceiptDocumentReference", "cbc:ID", "", receipt);
    }
    let originator = |x: &mut Writer| {
        if let Some(tender) = &references.tender {
            x.wrapped("cac:OriginatorDocumentReference", "cbc:ID", "", tender);
        }
    };
    if !credit_note {
        originator(&mut x);
    }
    if let Some(contract) = &references.contract {
        x.wrapped("cac:ContractDocumentReference", "cbc:ID", "", contract);
    }
    for document in &data.attachments {
        x.open("cac:AdditionalDocumentReference", "");
        x.leaf("cbc:ID", "", &document.reference);
        x.some("cbc:DocumentDescription", document.description.as_deref());
        if document.file.is_some() || document.uri.is_some() {
            x.open("cac:Attachment", "");
            if let Some(file) = &document.file {
                let bytes = invoice.files.get(&file.hash).map(|b| BASE64.encode(b)).unwrap_or_default();
                let attributes = format!(
                    "{}{}",
                    attribute("mimeCode", &file.media_type),
                    attribute("filename", file.name.as_deref().unwrap_or_default())
                );
                x.leaf("cbc:EmbeddedDocumentBinaryObject", &attributes, &bytes);
            }
            if let Some(uri) = &document.uri {
                x.wrapped("cac:ExternalReference", "cbc:URI", "", uri);
            }
            x.close("cac:Attachment");
        }
        x.close("cac:AdditionalDocumentReference");
    }
    if let Some(object) = &references.object {
        x.open("cac:AdditionalDocumentReference", "");
        x.identifier("cbc:ID", "schemeID", object);
        x.leaf("cbc:DocumentTypeCode", "", "130");
        x.close("cac:AdditionalDocumentReference");
    }
    if credit_note {
        if let Some(project) = &references.project {
            x.open("cac:AdditionalDocumentReference", "");
            x.leaf("cbc:ID", "", project);
            x.leaf("cbc:DocumentTypeCode", "", "50");
            x.close("cac:AdditionalDocumentReference");
        }
        originator(&mut x);
    } else if let Some(project) = &references.project {
        x.wrapped("cac:ProjectReference", "cbc:ID", "", project);
    }
    let payment = data.payment.as_ref();
    let creditor = payment.and_then(|p| p.direct_debit.as_ref()).map(|d| d.creditor.as_str());
    for (tag, party, role) in [
        ("cac:AccountingSupplierParty", &data.seller, Role::Seller),
        ("cac:AccountingCustomerParty", &data.buyer, Role::Buyer),
    ] {
        x.open(tag, "");
        x.open("cac:Party", "");
        write_ubl_party(&mut x, party, role, if role == Role::Seller { creditor } else { None });
        x.close("cac:Party");
        x.close(tag);
    }
    if let Some(payee) = &data.payee {
        x.open("cac:PayeeParty", "");
        write_ubl_party(&mut x, payee, Role::Payee, None);
        x.close("cac:PayeeParty");
    }
    if let Some(representative) = &data.tax_representative {
        x.open("cac:TaxRepresentativeParty", "");
        write_ubl_party(&mut x, representative, Role::TaxRepresentative, None);
        x.close("cac:TaxRepresentativeParty");
    }
    if data.delivery_date.is_some() || data.delivery.is_some() {
        x.open("cac:Delivery", "");
        x.some("cbc:ActualDeliveryDate", data.delivery_date.map(iso).as_deref());
        if let Some(delivery) = &data.delivery {
            if delivery.location.is_some() || delivery.address.is_some() {
                x.open("cac:DeliveryLocation", "");
                if let Some(location) = &delivery.location {
                    x.identifier("cbc:ID", "schemeID", location);
                }
                if let Some(address) = &delivery.address {
                    write_ubl_address(&mut x, "cac:Address", address);
                }
                x.close("cac:DeliveryLocation");
            }
            if let Some(name) = &delivery.name {
                x.open("cac:DeliveryParty", "");
                x.wrapped("cac:PartyName", "cbc:Name", "", name);
                x.close("cac:DeliveryParty");
            }
        }
        x.close("cac:Delivery");
    }
    if let Some(payment) = payment {
        // One payment means per account to transfer to, at least one.
        let accounts: Vec<Option<&CreditTransfer>> =
            if payment.transfers.is_empty() { vec![None] } else { payment.transfers.iter().map(Some).collect() };
        for (index, transfer) in accounts.into_iter().enumerate() {
            let first = index == 0;
            x.open("cac:PaymentMeans", "");
            let name = payment.means_text.as_deref().filter(|_| first).map(|n| attribute("name", n));
            x.leaf("cbc:PaymentMeansCode", &name.unwrap_or_default(), &payment.means);
            if credit_note && first {
                x.some("cbc:PaymentDueDate", data.due_date.map(iso).as_deref());
            }
            x.some("cbc:PaymentID", payment.remittance.as_deref().filter(|_| first));
            if let Some(card) = payment.card.as_ref().filter(|_| first) {
                x.open("cac:CardAccount", "");
                x.leaf("cbc:PrimaryAccountNumberID", "", &card.number);
                x.leaf("cbc:NetworkID", "", "NA");
                x.some("cbc:HolderName", card.holder.as_deref());
                x.close("cac:CardAccount");
            }
            if let Some(transfer) = transfer {
                x.open("cac:PayeeFinancialAccount", "");
                x.leaf("cbc:ID", "", &transfer.account);
                x.some("cbc:Name", transfer.name.as_deref());
                if let Some(provider) = &transfer.provider {
                    x.wrapped("cac:FinancialInstitutionBranch", "cbc:ID", "", provider);
                }
                x.close("cac:PayeeFinancialAccount");
            }
            if let Some(debit) = payment.direct_debit.as_ref().filter(|_| first) {
                x.open("cac:PaymentMandate", "");
                x.leaf("cbc:ID", "", &debit.mandate);
                x.wrapped("cac:PayerFinancialAccount", "cbc:ID", "", &debit.debited);
                x.close("cac:PaymentMandate");
            }
            x.close("cac:PaymentMeans");
        }
    }
    if let Some(terms) = &data.payment_terms {
        x.wrapped("cac:PaymentTerms", "cbc:Note", "", &written_terms(terms));
    }
    for (items, charge) in [(&data.allowances, false), (&data.charges, true)] {
        for item in items {
            write_ubl_allowance_charge(&mut x, item, charge, &amount);
        }
    }
    x.open("cac:TaxTotal", "");
    x.leaf("cbc:TaxAmount", &currency, &amount(&data.totals.tax));
    for tax in &data.taxes {
        x.open("cac:TaxSubtotal", "");
        x.leaf("cbc:TaxableAmount", &currency, &amount(&tax.base));
        x.leaf("cbc:TaxAmount", &currency, &amount(&tax.tax));
        x.open("cac:TaxCategory", "");
        x.leaf("cbc:ID", "", category_code(tax.category));
        x.leaf("cbc:Percent", "", &breakdown_rate(tax.rate));
        x.some("cbc:TaxExemptionReasonCode", tax.exemption_code.as_deref());
        x.some("cbc:TaxExemptionReason", tax.exemption.as_deref());
        x.wrapped("cac:TaxScheme", "cbc:ID", "", "VAT");
        x.close("cac:TaxCategory");
        x.close("cac:TaxSubtotal");
    }
    x.close("cac:TaxTotal");
    if let Some(tax) = &data.totals.tax_accounting {
        let tax_decimals = decimals_of(tax.currency(), currencies)?;
        x.open("cac:TaxTotal", "");
        x.leaf("cbc:TaxAmount", &attribute("currencyID", tax.currency().as_str()), &written(tax, tax_decimals));
        x.close("cac:TaxTotal");
    }
    let sum = |items: &[AllowanceCharge]| {
        Money::new(items.iter().map(|a| a.amount.base_units()).sum(), data.currency.clone())
            .ok_or_else(|| WriteError::Currency(code.to_owned()))
    };
    let lines = Money::new(data.lines.iter().map(|l| l.net.base_units()).sum(), data.currency.clone())
        .ok_or_else(|| WriteError::Currency(code.to_owned()))?;
    x.open("cac:LegalMonetaryTotal", "");
    x.leaf("cbc:LineExtensionAmount", &currency, &amount(&lines));
    x.leaf("cbc:TaxExclusiveAmount", &currency, &amount(&data.totals.net));
    x.leaf("cbc:TaxInclusiveAmount", &currency, &amount(&data.totals.gross));
    if !data.allowances.is_empty() {
        x.leaf("cbc:AllowanceTotalAmount", &currency, &amount(&sum(&data.allowances)?));
    }
    if !data.charges.is_empty() {
        x.leaf("cbc:ChargeTotalAmount", &currency, &amount(&sum(&data.charges)?));
    }
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
        x.some("cbc:Note", line.note.as_deref());
        let unit = attribute("unitCode", line.unit.as_deref().unwrap_or_default());
        x.leaf(quantity_tag, &unit, &line.quantity.map(|q| q.to_string()).unwrap_or_default());
        x.leaf("cbc:LineExtensionAmount", &currency, &amount(&line.net));
        x.some("cbc:AccountingCost", line.accounting.as_deref());
        if let Some(period) = &line.period {
            x.open("cac:InvoicePeriod", "");
            x.some("cbc:StartDate", period.start.map(iso).as_deref());
            x.some("cbc:EndDate", period.end.map(iso).as_deref());
            x.close("cac:InvoicePeriod");
        }
        if let Some(order_line) = &line.order_line {
            x.wrapped("cac:OrderLineReference", "cbc:LineID", "", order_line);
        }
        if let Some(object) = &line.object {
            x.open("cac:DocumentReference", "");
            x.identifier("cbc:ID", "schemeID", object);
            x.leaf("cbc:DocumentTypeCode", "", "130");
            x.close("cac:DocumentReference");
        }
        for (items, charge) in [(&line.allowances, false), (&line.charges, true)] {
            for item in items {
                write_ubl_allowance_charge(&mut x, item, charge, &amount);
            }
        }
        x.open("cac:Item", "");
        x.some("cbc:Description", line.description.as_deref());
        x.leaf("cbc:Name", "", &line.name);
        if let Some(id) = &line.buyer_item {
            x.wrapped("cac:BuyersItemIdentification", "cbc:ID", "", id);
        }
        if let Some(id) = &line.seller_item {
            x.wrapped("cac:SellersItemIdentification", "cbc:ID", "", id);
        }
        if let Some(id) = &line.standard_item {
            x.open("cac:StandardItemIdentification", "");
            x.identifier("cbc:ID", "schemeID", id);
            x.close("cac:StandardItemIdentification");
        }
        if let Some(origin) = &line.origin {
            x.wrapped("cac:OriginCountry", "cbc:IdentificationCode", "", origin.as_str());
        }
        for class in &line.classifications {
            let mut attributes = attribute("listID", &class.list);
            if let Some(version) = &class.version {
                attributes.push_str(&attribute("listVersionID", version));
            }
            x.wrapped("cac:CommodityClassification", "cbc:ItemClassificationCode", &attributes, &class.code);
        }
        x.open("cac:ClassifiedTaxCategory", "");
        x.leaf("cbc:ID", "", category_code(line.category));
        x.some("cbc:Percent", written_rate(line.category, line.rate).as_deref());
        x.wrapped("cac:TaxScheme", "cbc:ID", "", "VAT");
        x.close("cac:ClassifiedTaxCategory");
        for property in &line.attributes {
            x.open("cac:AdditionalItemProperty", "");
            x.leaf("cbc:Name", "", &property.name);
            x.leaf("cbc:Value", "", &property.value);
            x.close("cac:AdditionalItemProperty");
        }
        x.close("cac:Item");
        x.open("cac:Price", "");
        x.leaf("cbc:PriceAmount", &currency, &line.price.map(|p| p.to_string()).unwrap_or_default());
        if let Some(quantity) = line.price_quantity {
            x.leaf("cbc:BaseQuantity", &unit, &quantity.to_string());
        }
        if line.gross_price.is_some() || line.price_discount.is_some() {
            x.open("cac:AllowanceCharge", "");
            x.leaf("cbc:ChargeIndicator", "", "false");
            x.leaf("cbc:Amount", &currency, &line.price_discount.map_or_else(|| "0".to_owned(), |d| d.to_string()));
            if let Some(gross) = line.gross_price {
                x.leaf("cbc:BaseAmount", &currency, &gross.to_string());
            }
            x.close("cac:AllowanceCharge");
        }
        x.close("cac:Price");
        x.close(line_tag);
    }
    x.close(root);
    Ok(x.out)
}

fn write_cii_address(x: &mut Writer, address: &PostalAddress) {
    x.open("ram:PostalTradeAddress", "");
    x.some("ram:PostcodeCode", address.postcode.as_deref());
    for (tag, line) in ["ram:LineOne", "ram:LineTwo", "ram:LineThree"].iter().zip(&address.lines) {
        x.leaf(tag, "", line);
    }
    x.some("ram:CityName", address.city.as_deref());
    x.leaf("ram:CountryID", "", address.country.as_str());
    x.some("ram:CountrySubDivisionName", address.subdivision.as_deref());
    x.close("ram:PostalTradeAddress");
}

fn write_cii_identifiers(x: &mut Writer, identifiers: &[Identifier]) {
    for id in identifiers.iter().filter(|i| i.scheme.is_none()) {
        x.leaf("ram:ID", "", &id.id);
    }
    for id in identifiers.iter().filter(|i| i.scheme.is_some()) {
        x.identifier("ram:GlobalID", "schemeID", id);
    }
}

fn write_cii_party(x: &mut Writer, tag: &str, party: &InvoiceParty) {
    x.open(tag, "");
    write_cii_identifiers(x, &party.identifiers);
    x.leaf("ram:Name", "", &party.name);
    x.some("ram:Description", party.legal_info.as_deref());
    if party.registration.is_some() || party.trading_name.is_some() {
        x.open("ram:SpecifiedLegalOrganization", "");
        if let Some(registration) = &party.registration {
            x.identifier("ram:ID", "schemeID", registration);
        }
        x.some("ram:TradingBusinessName", party.trading_name.as_deref());
        x.close("ram:SpecifiedLegalOrganization");
    }
    if let Some(contact) = &party.contact {
        x.open("ram:DefinedTradeContact", "");
        x.some("ram:PersonName", contact.name.as_deref());
        if let Some(phone) = &contact.phone {
            x.wrapped("ram:TelephoneUniversalCommunication", "ram:CompleteNumber", "", phone);
        }
        if let Some(email) = &contact.email {
            x.wrapped("ram:EmailURIUniversalCommunication", "ram:URIID", "", email);
        }
        x.close("ram:DefinedTradeContact");
    }
    if let Some(address) = &party.address {
        write_cii_address(x, address);
    }
    if let Some(endpoint) = &party.endpoint {
        x.wrapped(
            "ram:URIUniversalCommunication",
            "ram:URIID",
            &attribute("schemeID", &endpoint.scheme),
            &endpoint.address,
        );
    }
    for (id, scheme) in [(&party.tax_number, "FC"), (&party.vat_id, "VA")] {
        if let Some(id) = id {
            x.wrapped("ram:SpecifiedTaxRegistration", "ram:ID", &attribute("schemeID", scheme), id);
        }
    }
    x.close(tag);
}

fn write_cii_allowance_charge(x: &mut Writer, item: &AllowanceCharge, charge: bool, amount: &dyn Fn(&Money) -> String) {
    x.open("ram:SpecifiedTradeAllowanceCharge", "");
    x.wrapped("ram:ChargeIndicator", "udt:Indicator", "", if charge { "true" } else { "false" });
    x.some("ram:CalculationPercent", item.percent.map(|p| p.to_string()).as_deref());
    x.some("ram:BasisAmount", item.base.as_ref().map(amount).as_deref());
    x.leaf("ram:ActualAmount", "", &amount(&item.amount));
    x.some("ram:ReasonCode", item.reason_code.as_deref());
    x.some("ram:Reason", item.reason.as_deref());
    if let Some(category) = item.category {
        x.open("ram:CategoryTradeTax", "");
        x.leaf("ram:TypeCode", "", "VAT");
        x.leaf("ram:CategoryCode", "", category_code(category));
        x.some("ram:RateApplicablePercent", written_rate(category, item.rate).as_deref());
        x.close("ram:CategoryTradeTax");
    }
    x.close("ram:SpecifiedTradeAllowanceCharge");
}

fn write_cii_period(x: &mut Writer, period: &Period) {
    x.open("ram:BillingSpecifiedPeriod", "");
    for (tag, day) in [("ram:StartDateTime", period.start), ("ram:EndDateTime", period.end)] {
        if let Some(day) = day {
            x.wrapped(tag, "udt:DateTimeString", &attribute("format", "102"), &compact(day));
        }
    }
    x.close("ram:BillingSpecifiedPeriod");
}

/// Writes an invoice or credit note as XRechnung 3.0 in the UN/CEFACT CII syntax – also the XML a ZUGFeRD or
/// Factur-X PDF carries in its profile XRECHNUNG.
pub fn write_cii(invoice: &TranslatedInvoice, currencies: &CurrencyTable) -> std::result::Result<String, WriteError> {
    check_xrechnung(invoice)?;
    let data = &invoice.invoice;
    if data.preceding.len() > 1 {
        return Err(WriteError::Syntax("BG-3"));
    }
    let code = data.currency.as_str();
    let decimals = decimals_of(&data.currency, currencies)?;
    let amount = |m: &Money| written(m, decimals);
    let references = data.references.clone().unwrap_or_default();
    let date_102 = attribute("format", "102");
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
    x.wrapped("ram:BusinessProcessSpecifiedDocumentContextParameter", "ram:ID", "", PROCESS);
    x.wrapped("ram:GuidelineSpecifiedDocumentContextParameter", "ram:ID", "", XRECHNUNG);
    x.close("rsm:ExchangedDocumentContext");
    x.open("rsm:ExchangedDocument", "");
    x.leaf("ram:ID", "", &invoice.number);
    x.leaf("ram:TypeCode", "", type_code(invoice));
    x.wrapped("ram:IssueDateTime", "udt:DateTimeString", &date_102, &compact(invoice.date));
    for note in &data.notes {
        x.open("ram:IncludedNote", "");
        x.leaf("ram:Content", "", &note.text);
        x.some("ram:SubjectCode", note.subject.as_deref());
        x.close("ram:IncludedNote");
    }
    x.close("rsm:ExchangedDocument");
    x.open("rsm:SupplyChainTradeTransaction", "");
    for line in &data.lines {
        x.open("ram:IncludedSupplyChainTradeLineItem", "");
        x.open("ram:AssociatedDocumentLineDocument", "");
        x.some("ram:LineID", line.id.as_deref());
        if let Some(note) = &line.note {
            x.wrapped("ram:IncludedNote", "ram:Content", "", note);
        }
        x.close("ram:AssociatedDocumentLineDocument");
        x.open("ram:SpecifiedTradeProduct", "");
        if let Some(id) = &line.standard_item {
            x.identifier("ram:GlobalID", "schemeID", id);
        }
        x.some("ram:SellerAssignedID", line.seller_item.as_deref());
        x.some("ram:BuyerAssignedID", line.buyer_item.as_deref());
        x.leaf("ram:Name", "", &line.name);
        x.some("ram:Description", line.description.as_deref());
        for property in &line.attributes {
            x.open("ram:ApplicableProductCharacteristic", "");
            x.leaf("ram:Description", "", &property.name);
            x.leaf("ram:Value", "", &property.value);
            x.close("ram:ApplicableProductCharacteristic");
        }
        for class in &line.classifications {
            let mut attributes = attribute("listID", &class.list);
            if let Some(version) = &class.version {
                attributes.push_str(&attribute("listVersionID", version));
            }
            x.wrapped("ram:DesignatedProductClassification", "ram:ClassCode", &attributes, &class.code);
        }
        if let Some(origin) = &line.origin {
            x.wrapped("ram:OriginTradeCountry", "ram:ID", "", origin.as_str());
        }
        x.close("ram:SpecifiedTradeProduct");
        let unit = attribute("unitCode", line.unit.as_deref().unwrap_or_default());
        x.open("ram:SpecifiedLineTradeAgreement", "");
        if let Some(order_line) = &line.order_line {
            x.wrapped("ram:BuyerOrderReferencedDocument", "ram:LineID", "", order_line);
        }
        if line.gross_price.is_some() || line.price_discount.is_some() {
            x.open("ram:GrossPriceProductTradePrice", "");
            let gross = line.gross_price.or(line.price).map(|p| p.to_string()).unwrap_or_default();
            x.leaf("ram:ChargeAmount", "", &gross);
            if let Some(quantity) = line.price_quantity {
                x.leaf("ram:BasisQuantity", &unit, &quantity.to_string());
            }
            if let Some(discount) = line.price_discount {
                x.open("ram:AppliedTradeAllowanceCharge", "");
                x.wrapped("ram:ChargeIndicator", "udt:Indicator", "", "false");
                x.leaf("ram:ActualAmount", "", &discount.to_string());
                x.close("ram:AppliedTradeAllowanceCharge");
            }
            x.close("ram:GrossPriceProductTradePrice");
        }
        x.open("ram:NetPriceProductTradePrice", "");
        x.leaf("ram:ChargeAmount", "", &line.price.map(|p| p.to_string()).unwrap_or_default());
        if let Some(quantity) = line.price_quantity {
            x.leaf("ram:BasisQuantity", &unit, &quantity.to_string());
        }
        x.close("ram:NetPriceProductTradePrice");
        x.close("ram:SpecifiedLineTradeAgreement");
        x.open("ram:SpecifiedLineTradeDelivery", "");
        x.leaf("ram:BilledQuantity", &unit, &line.quantity.map(|q| q.to_string()).unwrap_or_default());
        x.close("ram:SpecifiedLineTradeDelivery");
        x.open("ram:SpecifiedLineTradeSettlement", "");
        x.open("ram:ApplicableTradeTax", "");
        x.leaf("ram:TypeCode", "", "VAT");
        x.leaf("ram:CategoryCode", "", category_code(line.category));
        x.some("ram:RateApplicablePercent", written_rate(line.category, line.rate).as_deref());
        x.close("ram:ApplicableTradeTax");
        if let Some(period) = &line.period {
            write_cii_period(&mut x, period);
        }
        for (items, charge) in [(&line.allowances, false), (&line.charges, true)] {
            for item in items {
                write_cii_allowance_charge(&mut x, item, charge, &amount);
            }
        }
        x.wrapped("ram:SpecifiedTradeSettlementLineMonetarySummation", "ram:LineTotalAmount", "", &amount(&line.net));
        if let Some(object) = &line.object {
            x.open("ram:AdditionalReferencedDocument", "");
            x.leaf("ram:IssuerAssignedID", "", &object.id);
            x.leaf("ram:TypeCode", "", "130");
            x.some("ram:ReferenceTypeCode", object.scheme.as_deref());
            x.close("ram:AdditionalReferencedDocument");
        }
        if let Some(accounting) = &line.accounting {
            x.wrapped("ram:ReceivableSpecifiedTradeAccountingAccount", "ram:ID", "", accounting);
        }
        x.close("ram:SpecifiedLineTradeSettlement");
        x.close("ram:IncludedSupplyChainTradeLineItem");
    }
    x.open("ram:ApplicableHeaderTradeAgreement", "");
    x.some("ram:BuyerReference", data.buyer_reference.as_deref());
    write_cii_party(&mut x, "ram:SellerTradeParty", &data.seller);
    write_cii_party(&mut x, "ram:BuyerTradeParty", &data.buyer);
    if let Some(representative) = &data.tax_representative {
        write_cii_party(&mut x, "ram:SellerTaxRepresentativeTradeParty", representative);
    }
    if let Some(sales_order) = &references.sales_order {
        x.wrapped("ram:SellerOrderReferencedDocument", "ram:IssuerAssignedID", "", sales_order);
    }
    if let Some(order) = &references.order {
        x.wrapped("ram:BuyerOrderReferencedDocument", "ram:IssuerAssignedID", "", order);
    }
    if let Some(contract) = &references.contract {
        x.wrapped("ram:ContractReferencedDocument", "ram:IssuerAssignedID", "", contract);
    }
    for document in &data.attachments {
        x.open("ram:AdditionalReferencedDocument", "");
        x.leaf("ram:IssuerAssignedID", "", &document.reference);
        x.some("ram:URIID", document.uri.as_deref());
        x.leaf("ram:TypeCode", "", "916");
        x.some("ram:Name", document.description.as_deref());
        if let Some(file) = &document.file {
            let bytes = invoice.files.get(&file.hash).map(|b| BASE64.encode(b)).unwrap_or_default();
            let attributes = format!(
                "{}{}",
                attribute("mimeCode", &file.media_type),
                attribute("filename", file.name.as_deref().unwrap_or_default())
            );
            x.leaf("ram:AttachmentBinaryObject", &attributes, &bytes);
        }
        x.close("ram:AdditionalReferencedDocument");
    }
    if let Some(tender) = &references.tender {
        x.open("ram:AdditionalReferencedDocument", "");
        x.leaf("ram:IssuerAssignedID", "", tender);
        x.leaf("ram:TypeCode", "", "50");
        x.close("ram:AdditionalReferencedDocument");
    }
    if let Some(object) = &references.object {
        x.open("ram:AdditionalReferencedDocument", "");
        x.leaf("ram:IssuerAssignedID", "", &object.id);
        x.leaf("ram:TypeCode", "", "130");
        x.some("ram:ReferenceTypeCode", object.scheme.as_deref());
        x.close("ram:AdditionalReferencedDocument");
    }
    if let Some(project) = &references.project {
        x.open("ram:SpecifiedProcuringProject", "");
        x.leaf("ram:ID", "", project);
        x.leaf("ram:Name", "", "Project reference");
        x.close("ram:SpecifiedProcuringProject");
    }
    x.close("ram:ApplicableHeaderTradeAgreement");
    x.open("ram:ApplicableHeaderTradeDelivery", "");
    if let Some(delivery) = &data.delivery {
        x.open("ram:ShipToTradeParty", "");
        if let Some(location) = &delivery.location {
            write_cii_identifiers(&mut x, std::slice::from_ref(location));
        }
        x.some("ram:Name", delivery.name.as_deref());
        if let Some(address) = &delivery.address {
            write_cii_address(&mut x, address);
        }
        x.close("ram:ShipToTradeParty");
    }
    if let Some(delivery) = data.delivery_date {
        x.open("ram:ActualDeliverySupplyChainEvent", "");
        x.wrapped("ram:OccurrenceDateTime", "udt:DateTimeString", &date_102, &compact(delivery));
        x.close("ram:ActualDeliverySupplyChainEvent");
    }
    if let Some(despatch) = &references.despatch_advice {
        x.wrapped("ram:DespatchAdviceReferencedDocument", "ram:IssuerAssignedID", "", despatch);
    }
    if let Some(receipt) = &references.receiving_advice {
        x.wrapped("ram:ReceivingAdviceReferencedDocument", "ram:IssuerAssignedID", "", receipt);
    }
    x.close("ram:ApplicableHeaderTradeDelivery");
    x.open("ram:ApplicableHeaderTradeSettlement", "");
    let payment = data.payment.as_ref();
    if let Some(debit) = payment.and_then(|p| p.direct_debit.as_ref()) {
        x.leaf("ram:CreditorReferenceID", "", &debit.creditor);
    }
    x.some("ram:PaymentReference", payment.and_then(|p| p.remittance.as_deref()));
    if let Some(tax) = &data.totals.tax_accounting {
        x.leaf("ram:TaxCurrencyCode", "", tax.currency().as_str());
    }
    x.leaf("ram:InvoiceCurrencyCode", "", code);
    if let Some(payee) = &data.payee {
        write_cii_party(&mut x, "ram:PayeeTradeParty", payee);
    }
    if let Some(payment) = payment {
        let accounts: Vec<Option<&CreditTransfer>> =
            if payment.transfers.is_empty() { vec![None] } else { payment.transfers.iter().map(Some).collect() };
        for (index, transfer) in accounts.into_iter().enumerate() {
            let first = index == 0;
            x.open("ram:SpecifiedTradeSettlementPaymentMeans", "");
            x.leaf("ram:TypeCode", "", &payment.means);
            x.some("ram:Information", payment.means_text.as_deref().filter(|_| first));
            if let Some(card) = payment.card.as_ref().filter(|_| first) {
                x.open("ram:ApplicableTradeSettlementFinancialCard", "");
                x.leaf("ram:ID", "", &card.number);
                x.some("ram:CardholderName", card.holder.as_deref());
                x.close("ram:ApplicableTradeSettlementFinancialCard");
            }
            if let Some(debit) = payment.direct_debit.as_ref().filter(|_| first) {
                x.wrapped("ram:PayerPartyDebtorFinancialAccount", "ram:IBANID", "", &debit.debited);
            }
            if let Some(transfer) = transfer {
                x.open("ram:PayeePartyCreditorFinancialAccount", "");
                let tag = if is_iban(&transfer.account) { "ram:IBANID" } else { "ram:ProprietaryID" };
                x.leaf(tag, "", &transfer.account);
                x.some("ram:AccountName", transfer.name.as_deref());
                x.close("ram:PayeePartyCreditorFinancialAccount");
                if let Some(provider) = &transfer.provider {
                    x.wrapped("ram:PayeeSpecifiedCreditorFinancialInstitution", "ram:BICID", "", provider);
                }
            }
            x.close("ram:SpecifiedTradeSettlementPaymentMeans");
        }
    }
    for (index, tax) in data.taxes.iter().enumerate() {
        let first = index == 0;
        x.open("ram:ApplicableTradeTax", "");
        x.leaf("ram:CalculatedAmount", "", &amount(&tax.tax));
        x.leaf("ram:TypeCode", "", "VAT");
        x.some("ram:ExemptionReason", tax.exemption.as_deref());
        x.leaf("ram:BasisAmount", "", &amount(&tax.base));
        x.leaf("ram:CategoryCode", "", category_code(tax.category));
        x.some("ram:ExemptionReasonCode", tax.exemption_code.as_deref());
        if let Some(day) = data.tax_point_date.filter(|_| first) {
            x.wrapped("ram:TaxPointDate", "udt:DateString", &date_102, &compact(day));
        }
        x.some("ram:DueDateTypeCode", data.tax_point_code.as_deref().filter(|_| first).map(tax_point_to_cii));
        x.leaf("ram:RateApplicablePercent", "", &breakdown_rate(tax.rate));
        x.close("ram:ApplicableTradeTax");
    }
    if let Some(period) = &data.period {
        write_cii_period(&mut x, period);
    }
    for (items, charge) in [(&data.allowances, false), (&data.charges, true)] {
        for item in items {
            write_cii_allowance_charge(&mut x, item, charge, &amount);
        }
    }
    let debit = payment.and_then(|p| p.direct_debit.as_ref());
    if data.payment_terms.is_some() || data.due_date.is_some() || debit.is_some() {
        x.open("ram:SpecifiedTradePaymentTerms", "");
        x.some("ram:Description", data.payment_terms.as_deref().map(written_terms).as_deref());
        if let Some(due) = data.due_date {
            x.wrapped("ram:DueDateDateTime", "udt:DateTimeString", &date_102, &compact(due));
        }
        x.some("ram:DirectDebitMandateID", debit.map(|d| d.mandate.as_str()));
        x.close("ram:SpecifiedTradePaymentTerms");
    }
    let sum = |items: &[AllowanceCharge]| -> i128 { items.iter().map(|a| a.amount.base_units()).sum() };
    let units = |u: i128| -> std::result::Result<String, WriteError> {
        Money::new(u, data.currency.clone()).map(|m| amount(&m)).ok_or_else(|| WriteError::Currency(code.to_owned()))
    };
    x.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation", "");
    x.leaf("ram:LineTotalAmount", "", &units(data.lines.iter().map(|l| l.net.base_units()).sum())?);
    if !data.charges.is_empty() {
        x.leaf("ram:ChargeTotalAmount", "", &units(sum(&data.charges))?);
    }
    if !data.allowances.is_empty() {
        x.leaf("ram:AllowanceTotalAmount", "", &units(sum(&data.allowances))?);
    }
    x.leaf("ram:TaxBasisTotalAmount", "", &amount(&data.totals.net));
    x.leaf("ram:TaxTotalAmount", &attribute("currencyID", code), &amount(&data.totals.tax));
    if let Some(tax) = &data.totals.tax_accounting {
        let tax_decimals = decimals_of(tax.currency(), currencies)?;
        x.leaf("ram:TaxTotalAmount", &attribute("currencyID", tax.currency().as_str()), &written(tax, tax_decimals));
    }
    if let Some(rounding) = &data.totals.rounding {
        x.leaf("ram:RoundingAmount", "", &amount(rounding));
    }
    x.leaf("ram:GrandTotalAmount", "", &amount(&data.totals.gross));
    if let Some(prepaid) = &data.totals.prepaid {
        x.leaf("ram:TotalPrepaidAmount", "", &amount(prepaid));
    }
    x.leaf("ram:DuePayableAmount", "", &amount(&data.totals.due));
    x.close("ram:SpecifiedTradeSettlementHeaderMonetarySummation");
    for preceding in &data.preceding {
        x.open("ram:InvoiceReferencedDocument", "");
        x.leaf("ram:IssuerAssignedID", "", &preceding.number);
        if let Some(day) = preceding.date {
            x.wrapped("ram:FormattedIssueDateTime", "qdt:DateTimeString", &date_102, &compact(day));
        }
        x.close("ram:InvoiceReferencedDocument");
    }
    if let Some(accounting) = &references.accounting {
        x.wrapped("ram:ReceivableSpecifiedTradeAccountingAccount", "ram:ID", "", accounting);
    }
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
