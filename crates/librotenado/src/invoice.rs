// SPDX-License-Identifier: MIT
//! The data of an invoice or credit note (spec part 0005): parties, dates, VAT breakdown and totals – as a record of
//! their own, whether entered by hand or translated from an electronic invoice.

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, Booking, Country, Currency, Date, Decimal, Document, DocumentKind, Money, Object, Original, PartyId,
    Posting, TaxCode,
};

/// The VAT category of an amount, as the codes of EN 16931 (UNCL 5305) name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum VatCategory {
    /// Standard or reduced rate.
    #[serde(rename = "S")]
    Standard,
    /// Zero rate.
    #[serde(rename = "Z")]
    ZeroRated,
    /// Exempt.
    #[serde(rename = "E")]
    Exempt,
    /// Reverse charge: the buyer owes the VAT.
    #[serde(rename = "AE")]
    ReverseCharge,
    /// Intra-Community supply, exempt.
    #[serde(rename = "K")]
    IntraCommunity,
    /// Export outside the EU, exempt.
    #[serde(rename = "G")]
    Export,
    /// Not subject to VAT.
    #[serde(rename = "O")]
    NotSubject,
    /// Canary Islands general indirect tax.
    #[serde(rename = "L")]
    CanaryIslands,
    /// Tax for production, services and importation in Ceuta and Melilla.
    #[serde(rename = "M")]
    CeutaMelilla,
}

/// An identifier with the scheme it belongs to, where stated – e.g. a GLN with scheme `0088`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identifier {
    /// The identifier.
    pub id: String,
    /// The scheme, as the code list of ISO/IEC 6523 or the invoice names it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub scheme: Option<String>,
}

/// A postal address on an invoice (BG-5, BG-8, BG-12, BG-15).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PostalAddress {
    /// Up to three address lines, the street first (BT-35, BT-36, BT-162 and their counterparts).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub lines: Vec<String>,
    /// The city (BT-37).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub city: Option<String>,
    /// The postcode (BT-38).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub postcode: Option<String>,
    /// The region or state (BT-39).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub subdivision: Option<String>,
    /// The country (BT-40).
    pub country: Country,
}

/// A party named on an invoice – the seller, the buyer, the payee or the seller's tax representative – as the
/// invoice names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceParty {
    /// The name (BT-27, BT-44, BT-59, BT-62).
    pub name: String,
    /// The name it trades under, where different (BT-28, BT-45).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub trading_name: Option<String>,
    /// Identifiers of the party, e.g. a GLN or the buyer's customer number (BT-29, BT-46, BT-60).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub identifiers: Vec<Identifier>,
    /// Its legal registration, e.g. a commercial register number (BT-30, BT-47, BT-61).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub registration: Option<Identifier>,
    /// The VAT identification number, with its country prefix (BT-31, BT-48, BT-63).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub vat_id: Option<String>,
    /// A national tax number (BT-32).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub tax_number: Option<String>,
    /// Further legal information, e.g. share capital or managing directors (BT-33).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub legal_info: Option<String>,
    /// The postal address (BG-5, BG-8, BG-12).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub address: Option<PostalAddress>,
    /// Whom to contact (BG-6, BG-9).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub contact: Option<InvoiceContact>,
    /// The electronic address invoices are sent to (BT-34, BT-49).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub endpoint: Option<Endpoint>,
    /// The organisation or person of the books this party is (spec part 0008), once known.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub party: Option<PartyId>,
}

impl InvoiceParty {
    /// A party known only by its name.
    pub fn named(name: &str) -> Self {
        InvoiceParty {
            name: name.to_owned(),
            trading_name: None,
            identifiers: Vec::new(),
            registration: None,
            vat_id: None,
            tax_number: None,
            legal_info: None,
            address: None,
            contact: None,
            endpoint: None,
            party: None,
        }
    }

    /// The country of its address.
    pub fn country(&self) -> Option<Country> {
        self.address.as_ref().map(|a| a.country)
    }
}

/// A contact of a party on an invoice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceContact {
    /// A person or department (BT-41, BT-56).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub name: Option<String>,
    /// A telephone number (BT-42, BT-57).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub phone: Option<String>,
    /// An e-mail address (BT-43, BT-58).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub email: Option<String>,
}

/// An electronic address: its scheme (e.g. `EM` for e-mail, `0204` for a Leitweg-ID) and the address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Endpoint {
    /// The scheme, as the code list of the Electronic Address Scheme names it.
    pub scheme: String,
    /// The address.
    pub address: String,
}

/// The references of an invoice to other documents and objects of the business (BT-11 to BT-19).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceReferences {
    /// The project (BT-11).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub project: Option<String>,
    /// The contract (BT-12).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub contract: Option<String>,
    /// The buyer's purchase order (BT-13).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub order: Option<String>,
    /// The seller's sales order (BT-14).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub sales_order: Option<String>,
    /// The receiving advice (BT-15).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub receiving_advice: Option<String>,
    /// The despatch advice (BT-16).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub despatch_advice: Option<String>,
    /// The tender or lot (BT-17).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub tender: Option<String>,
    /// The object invoiced, e.g. a meter or a subscription (BT-18).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub object: Option<Identifier>,
    /// The buyer's accounting reference, e.g. a cost centre (BT-19).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub accounting: Option<String>,
}

/// A note on an invoice (BG-1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceNote {
    /// What it is about, a code of UNTDID 4451, e.g. `AAK` (BT-21).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub subject: Option<String>,
    /// The note (BT-22).
    pub text: String,
}

/// An invoice an invoice or credit note refers to, e.g. the one a credit note corrects (BG-3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PrecedingInvoice {
    /// Its number (BT-25).
    pub number: String,
    /// Its date (BT-26).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub date: Option<Date>,
}

/// A period of time, both ends included (BG-14, BG-26).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Period {
    /// The first day (BT-73, BT-134).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub start: Option<Date>,
    /// The last day (BT-74, BT-135).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub end: Option<Date>,
}

/// Where the supply went (BG-13 without its dates).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delivery {
    /// The party delivered to (BT-70).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub name: Option<String>,
    /// The location, e.g. a GLN (BT-71).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub location: Option<Identifier>,
    /// The address delivered to (BG-15).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub address: Option<PostalAddress>,
}

/// How an invoice is paid (BG-16).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Payment {
    /// The means, a code of UNTDID 4461 (BT-81), e.g. `58` SEPA credit transfer, `59` SEPA direct debit.
    pub means: String,
    /// The means as text (BT-82).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub means_text: Option<String>,
    /// The text to give with the payment, the remittance information (BT-83).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub remittance: Option<String>,
    /// The accounts to transfer to (BG-17).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub transfers: Vec<CreditTransfer>,
    /// The card paid with (BG-18).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub card: Option<PaymentCard>,
    /// The direct debit (BG-19).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub direct_debit: Option<DirectDebit>,
}

/// An account to transfer to (BG-17).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreditTransfer {
    /// The account, an IBAN for SEPA (BT-84).
    pub account: String,
    /// The account's name (BT-85).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub name: Option<String>,
    /// The payment service provider, a BIC (BT-86).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub provider: Option<String>,
}

/// A payment card (BG-18).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentCard {
    /// The last digits of the card number – never the whole number (BT-87).
    pub number: String,
    /// The card holder (BT-88).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub holder: Option<String>,
}

/// A SEPA direct debit (BG-19).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DirectDebit {
    /// The mandate reference (BT-89).
    pub mandate: String,
    /// The creditor identifier of the seller (BT-90).
    pub creditor: String,
    /// The account debited, an IBAN (BT-91).
    pub debited: String,
}

/// An allowance or a charge on the invoice (BG-20, BG-21) or on a line (BG-27, BG-28).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AllowanceCharge {
    /// The amount, without VAT (BT-92, BT-99, BT-136, BT-141).
    pub amount: Money,
    /// The amount it is a percentage of (BT-93, BT-100, BT-137, BT-142).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub base: Option<Money>,
    /// The percentage (BT-94, BT-101, BT-138, BT-143).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub percent: Option<Decimal>,
    /// Its VAT category – on the invoice only; on a line, the line's (BT-95, BT-102).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub category: Option<VatCategory>,
    /// Its VAT rate in percent – on the invoice only, absent where the category has none (BT-96, BT-103).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rate: Option<Decimal>,
    /// The reason as text (BT-97, BT-104, BT-139, BT-144).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub reason: Option<String>,
    /// The reason as a code of UNTDID 5189 for allowances, 7161 for charges (BT-98, BT-105, BT-140, BT-145).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub reason_code: Option<String>,
}

/// The VAT of one category and rate on an invoice (BG-23).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VatBreakdown {
    /// The category (BT-118).
    pub category: VatCategory,
    /// The rate in percent; absent where the category has none (BT-119).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rate: Option<Decimal>,
    /// The taxable amount (BT-116).
    pub base: Money,
    /// The VAT (BT-117).
    pub tax: Money,
    /// Why the amount is exempt or not taxed, as the invoice states it (BT-120).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub exemption: Option<String>,
    /// The same as a code of the VATEX list, e.g. `VATEX-EU-IC` (BT-121).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub exemption_code: Option<String>,
}

/// A document supporting an invoice, e.g. a timesheet (BG-24).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SupportingDocument {
    /// Its reference (BT-122).
    pub reference: String,
    /// What it is (BT-123).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub description: Option<String>,
    /// Where it can be fetched (BT-124).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub uri: Option<String>,
    /// The file attached to the invoice (BT-125): its hash, size, media type and name. Its bytes are stored like
    /// any original, outside the events.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub file: Option<Original>,
}

/// A classification of an item, e.g. by CPV or the customs tariff (BT-158).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Classification {
    /// The code.
    pub code: String,
    /// The list, a code of UNTDID 7143, e.g. `STI` CPV.
    pub list: String,
    /// The version of the list.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub version: Option<String>,
}

/// A property of an item, e.g. its colour (BG-32).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemAttribute {
    /// The name (BT-160).
    pub name: String,
    /// The value (BT-161).
    pub value: String,
}

/// A line of an invoice (BG-25).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceLine {
    /// The line's identifier (BT-126).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub id: Option<String>,
    /// A note on the line (BT-127).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub note: Option<String>,
    /// The object invoiced on the line (BT-128).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub object: Option<Identifier>,
    /// The line of the buyer's purchase order (BT-132).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub order_line: Option<String>,
    /// The buyer's accounting reference (BT-133).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub accounting: Option<String>,
    /// The period invoiced on the line (BG-26).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub period: Option<Period>,
    /// What was supplied (BT-153).
    pub name: String,
    /// A description of the item (BT-154).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub description: Option<String>,
    /// The seller's item number (BT-155).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub seller_item: Option<String>,
    /// The buyer's item number (BT-156).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub buyer_item: Option<String>,
    /// A standard identifier of the item, e.g. a GTIN with scheme `0160` (BT-157).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub standard_item: Option<Identifier>,
    /// Classifications of the item (BT-158).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub classifications: Vec<Classification>,
    /// The country of origin of the item (BT-159).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub origin: Option<Country>,
    /// Properties of the item (BG-32).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub attributes: Vec<ItemAttribute>,
    /// How much (BT-129).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub quantity: Option<Decimal>,
    /// The unit of the quantity, a code of UN/ECE Recommendation 20 (BT-130), e.g. `C62` one, `HUR` hour.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub unit: Option<String>,
    /// The net price per unit in units of the currency, which may have more places than money (BT-146).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub price: Option<Decimal>,
    /// The quantity the price is for, where not one unit (BT-149, in the unit BT-150 – the line's).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub price_quantity: Option<Decimal>,
    /// The price before the discount on it (BT-148).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub gross_price: Option<Decimal>,
    /// The discount on the gross price, per unit (BT-147).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub price_discount: Option<Decimal>,
    /// Allowances on the line (BG-27).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub allowances: Vec<AllowanceCharge>,
    /// Charges on the line (BG-28).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub charges: Vec<AllowanceCharge>,
    /// The net amount of the line, after its allowances and charges (BT-131).
    pub net: Money,
    /// Its VAT category (BT-151).
    pub category: VatCategory,
    /// Its VAT rate in percent; absent where the category has none (BT-152).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rate: Option<Decimal>,
}

/// The totals of an invoice (BG-22). The sum of the lines and of the allowances and charges (BT-106 to BT-108) are
/// not recorded: they follow from the lines and from the allowances and charges.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceTotals {
    /// The total without VAT (BT-109).
    pub net: Money,
    /// The total VAT (BT-110).
    pub tax: Money,
    /// The total VAT in the currency VAT is accounted in, where it differs from the invoice's: its currency is
    /// BT-6, its amount BT-111.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub tax_accounting: Option<Money>,
    /// The total with VAT (BT-112).
    pub gross: Money,
    /// Amounts paid in advance (BT-113); absent when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub prepaid: Option<Money>,
    /// The rounding of the amount due (BT-114); absent when none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rounding: Option<Money>,
    /// The amount due for payment: gross, less prepaid, plus rounding (BT-115).
    pub due: Money,
}

/// The data of an invoice or credit note, after the semantic model of EN 16931. The number, date and kind are the
/// document's (BT-1 to BT-3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceData {
    /// The type of the invoice as a code of UNTDID 1001 (BT-3), where it is neither `380` for an invoice nor `381`
    /// for a credit note – e.g. `384` corrected invoice, `389` self-billed invoice, `326` partial invoice.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub type_code: Option<String>,
    /// The currency of the invoice (BT-5).
    pub currency: Currency,
    /// The notes (BG-1).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub notes: Vec<InvoiceNote>,
    /// Who supplies (BG-4).
    pub seller: InvoiceParty,
    /// Who is supplied (BG-7).
    pub buyer: InvoiceParty,
    /// Who is paid, where not the seller (BG-10).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub payee: Option<InvoiceParty>,
    /// The seller's tax representative (BG-11).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub tax_representative: Option<InvoiceParty>,
    /// The reference the buyer asked for, e.g. the Leitweg-ID of a German public authority (BT-10).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub buyer_reference: Option<String>,
    /// References to other documents of the business (BT-11 to BT-19).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub references: Option<InvoiceReferences>,
    /// Invoices it refers to (BG-3).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub preceding: Vec<PrecedingInvoice>,
    /// When the supply was made (BT-72).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub delivery_date: Option<Date>,
    /// The period invoiced (BG-14).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub period: Option<Period>,
    /// Where the supply went (BG-13, BG-15).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub delivery: Option<Delivery>,
    /// When VAT becomes chargeable, where it is known and differs from the invoice date (BT-7).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub tax_point_date: Option<Date>,
    /// The same as a code of UNTDID 2005: `3` the invoice date, `35` the delivery date, `432` the date paid (BT-8).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub tax_point_code: Option<String>,
    /// When payment is due (BT-9).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub due_date: Option<Date>,
    /// The terms of payment as text (BT-20).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub payment_terms: Option<String>,
    /// How it is paid (BG-16).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub payment: Option<Payment>,
    /// Allowances on the invoice (BG-20).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub allowances: Vec<AllowanceCharge>,
    /// Charges on the invoice (BG-21).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub charges: Vec<AllowanceCharge>,
    /// The VAT per category and rate, at least one (BG-23).
    #[serde(deserialize_with = "crate::non_empty")]
    pub taxes: Vec<VatBreakdown>,
    /// Supporting documents (BG-24).
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub attachments: Vec<SupportingDocument>,
    /// The lines (BG-25); absent when not recorded.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub lines: Vec<InvoiceLine>,
    /// The totals (BG-22).
    pub totals: InvoiceTotals,
}

impl Object for InvoiceData {
    const TYPE: &'static str = "invoice_data";
}

impl Object for InvoiceParty {
    const TYPE: &'static str = "invoice_party";
}

impl Object for Identifier {
    const TYPE: &'static str = "identifier";
}

impl Object for PostalAddress {
    const TYPE: &'static str = "postal_address";
}

impl Object for InvoiceContact {
    const TYPE: &'static str = "invoice_contact";
}

impl Object for Endpoint {
    const TYPE: &'static str = "endpoint";
}

impl Object for InvoiceReferences {
    const TYPE: &'static str = "invoice_references";
}

impl Object for InvoiceNote {
    const TYPE: &'static str = "invoice_note";
}

impl Object for PrecedingInvoice {
    const TYPE: &'static str = "preceding_invoice";
}

impl Object for Period {
    const TYPE: &'static str = "period";
}

impl Object for Delivery {
    const TYPE: &'static str = "delivery";
}

impl Object for Payment {
    const TYPE: &'static str = "payment";
}

impl Object for CreditTransfer {
    const TYPE: &'static str = "credit_transfer";
}

impl Object for PaymentCard {
    const TYPE: &'static str = "payment_card";
}

impl Object for DirectDebit {
    const TYPE: &'static str = "direct_debit";
}

impl Object for AllowanceCharge {
    const TYPE: &'static str = "allowance_charge";
}

impl Object for VatBreakdown {
    const TYPE: &'static str = "vat_breakdown";
}

impl Object for SupportingDocument {
    const TYPE: &'static str = "supporting_document";
}

impl Object for Classification {
    const TYPE: &'static str = "classification";
}

impl Object for ItemAttribute {
    const TYPE: &'static str = "item_attribute";
}

impl Object for InvoiceLine {
    const TYPE: &'static str = "invoice_line";
}

impl Object for InvoiceTotals {
    const TYPE: &'static str = "invoice_totals";
}

/// Whether the books' owner bought or sold with an invoice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceSide {
    /// The owner is the buyer.
    Purchase,
    /// The owner is the seller.
    Sale,
}

/// The accounts a booking proposal posts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalAccounts {
    /// The account of the net amounts: an expense for a purchase, income for a sale.
    pub counter: AccountId,
    /// The account of the other party: a supplier for a purchase, a customer for a sale.
    pub party: AccountId,
    /// Output VAT.
    pub vat_output: AccountId,
    /// Deductible input VAT.
    pub vat_input: AccountId,
    /// VAT the buyer owes under reverse charge or on an intra-Community acquisition.
    pub vat_reverse_charge: AccountId,
}

impl ProposalAccounts {
    /// Accounts of the harmonised chart 0.1: other purchases (PCG 6068) or services sold (706), suppliers (4011) or
    /// customers (4111), VAT collected (44571), deductible VAT (44566), VAT due on intra-Community transactions
    /// (4452).
    pub fn v0_1(side: InvoiceSide) -> Self {
        let pcg = crate::ChartTranslation::fr_pcg();
        let account = |number: &str| pcg.accounts[number].clone();
        let (counter, party) = match side {
            InvoiceSide::Purchase => ("6068", "4011"),
            InvoiceSide::Sale => ("706", "4111"),
        };
        ProposalAccounts {
            counter: account(counter),
            party: account(party),
            vat_output: account("44571"),
            vat_input: account("44566"),
            vat_reverse_charge: account("4452"),
        }
    }
}

/// The German tax code (spec data `tax/de.json`) for an amount of a category and rate: `None` where the amount is
/// not reported, or where the category does not tell enough (a rate other than 19 or 7 per cent).
pub fn de_tax_code(
    category: VatCategory,
    rate: Option<Decimal>,
    side: InvoiceSide,
    seller: Option<Country>,
) -> Option<TaxCode> {
    let percent = rate.map(|r| r.to_string());
    let code = match (side, category, percent.as_deref()) {
        (InvoiceSide::Sale, VatCategory::Standard, Some("19")) => "de-out-19",
        (InvoiceSide::Sale, VatCategory::Standard, Some("7")) => "de-out-7",
        (InvoiceSide::Sale, VatCategory::ZeroRated, _) => "de-out-0",
        (InvoiceSide::Sale, VatCategory::Exempt, _) => "de-out-exempt",
        (InvoiceSide::Sale, VatCategory::ReverseCharge, _) => "de-out-13b",
        (InvoiceSide::Sale, VatCategory::IntraCommunity, _) => "de-out-ig-supply",
        (InvoiceSide::Sale, VatCategory::Export, _) => "de-out-export",
        (InvoiceSide::Sale, VatCategory::NotSubject, _) => "de-out-not-taxable",
        (InvoiceSide::Purchase, VatCategory::Standard, Some("19")) => "de-in-19",
        (InvoiceSide::Purchase, VatCategory::Standard, Some("7")) => "de-in-7",
        (InvoiceSide::Purchase, VatCategory::ReverseCharge, _) if seller.is_some_and(|c| c.as_str() != "DE") => {
            "de-in-13b-eu-service"
        }
        (InvoiceSide::Purchase, VatCategory::ReverseCharge, _) => "de-in-13b-domestic",
        (InvoiceSide::Purchase, VatCategory::IntraCommunity, _) => "de-in-ig-acquisition-19",
        _ => return None,
    };
    Some(code.parse().expect("a valid code"))
}

/// Why no booking can be proposed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProposalError {
    /// The document carries no invoice data.
    #[error("the document has no invoice data")]
    NoInvoice,
    /// An amount beyond the range of money.
    #[error("the amounts are too large")]
    Overflow,
}

/// A booking proposed for an invoice or credit note: per VAT category and rate, the net amount on the counter
/// account and the VAT on the VAT account, each with its German tax code; the gross amount on the party's account.
/// Under reverse charge and for intra-Community acquisitions the VAT owed and deducted is computed at 19 per cent.
/// A credit note books the other way round. Whoever books reviews and may change every posting.
pub fn propose_booking(
    document: &Document,
    side: InvoiceSide,
    owner: &PartyId,
    accounts: &ProposalAccounts,
) -> Result<Booking, ProposalError> {
    let invoice = document.invoice.as_deref().ok_or(ProposalError::NoInvoice)?;
    // Purchases are debits, sales credits; a credit note reverses.
    let sign: i128 = match (side, document.kind == DocumentKind::CreditNote) {
        (InvoiceSide::Purchase, false) | (InvoiceSide::Sale, true) => 1,
        (InvoiceSide::Purchase, true) | (InvoiceSide::Sale, false) => -1,
    };
    let mut postings = Vec::new();
    let mut post = |account: &AccountId, units: i128, tax: Option<&TaxCode>| -> Result<(), ProposalError> {
        if units != 0 {
            let amount = Money::new(units, invoice.currency.clone()).ok_or(ProposalError::Overflow)?;
            postings.push(Posting {
                owner: owner.clone(),
                account: account.clone(),
                amount,
                note: None,
                tax: tax.cloned(),
                exchange: None,
            });
        }
        Ok(())
    };
    let mut total: i128 = 0;
    for breakdown in &invoice.taxes {
        let code = de_tax_code(breakdown.category, breakdown.rate, side, invoice.seller.country());
        let (base, tax) = (breakdown.base.base_units(), breakdown.tax.base_units());
        post(&accounts.counter, sign * base, code.as_ref())?;
        let vat = match side {
            InvoiceSide::Purchase => &accounts.vat_input,
            InvoiceSide::Sale => &accounts.vat_output,
        };
        post(vat, sign * tax, code.as_ref())?;
        total = total.checked_add(base).and_then(|t| t.checked_add(tax)).ok_or(ProposalError::Overflow)?;
        let self_assessed = matches!(breakdown.category, VatCategory::ReverseCharge | VatCategory::IntraCommunity);
        if side == InvoiceSide::Purchase && self_assessed {
            // 19 per cent, rounded half away from zero.
            let owed = (base * 19 + base.signum() * 50) / 100;
            post(&accounts.vat_input, sign * owed, code.as_ref())?;
            post(&accounts.vat_reverse_charge, -sign * owed, code.as_ref())?;
        }
    }
    post(&accounts.party, -sign * total, None)?;
    let name = match side {
        InvoiceSide::Purchase => &invoice.seller.name,
        InvoiceSide::Sale => &invoice.buyer.name,
    };
    let text = match &document.number {
        Some(number) => format!("{name} {number}"),
        None => name.clone(),
    };
    Ok(Booking { date: document.date, text, postings, documents: vec![document.id.clone()], transaction: None })
}
