// SPDX-License-Identifier: MIT
//! The data of an invoice or credit note (spec part 0005): parties, dates, VAT breakdown and totals – as a record of
//! their own, whether entered by hand or translated from an electronic invoice.

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, Booking, Country, Currency, Date, Decimal, Document, DocumentKind, Money, Object, PartyId, Posting,
    TaxCode,
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

/// A party named on an invoice: the seller or the buyer, as the invoice names them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceParty {
    /// The name.
    pub name: String,
    /// The VAT identification number, with its country prefix.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub vat_id: Option<String>,
    /// A national tax number.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub tax_number: Option<String>,
    /// The country of the address.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub country: Option<Country>,
    /// The organisation or person of the books this party is (spec part 0008), once known.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub party: Option<PartyId>,
}

/// The VAT of one category and rate on an invoice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VatBreakdown {
    /// The category.
    pub category: VatCategory,
    /// The rate in percent; absent where the category has none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rate: Option<Decimal>,
    /// The taxable amount.
    pub base: Money,
    /// The VAT.
    pub tax: Money,
    /// Why the amount is exempt or not taxed, as the invoice states it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub exemption: Option<String>,
}

/// A line of an invoice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceLine {
    /// What was supplied.
    pub name: String,
    /// The net amount of the line.
    pub net: Money,
    /// Its VAT category.
    pub category: VatCategory,
    /// Its VAT rate in percent; absent where the category has none.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub rate: Option<Decimal>,
}

/// The totals of an invoice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceTotals {
    /// The total without VAT.
    pub net: Money,
    /// The total VAT.
    pub tax: Money,
    /// The total with VAT.
    pub gross: Money,
    /// The amount due for payment, after prepaid amounts and rounding.
    pub due: Money,
}

/// The data of an invoice or credit note.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceData {
    /// The currency of the invoice.
    pub currency: Currency,
    /// Who supplies.
    pub seller: InvoiceParty,
    /// Who is supplied.
    pub buyer: InvoiceParty,
    /// When payment is due.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub due_date: Option<Date>,
    /// When the supply was made.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub delivery_date: Option<Date>,
    /// The reference the buyer asked for, e.g. a routing identifier of a public authority (Leitweg-ID).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub buyer_reference: Option<String>,
    /// The account to pay to (IBAN).
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub iban: Option<String>,
    /// The VAT per category and rate, at least one.
    #[serde(deserialize_with = "crate::non_empty")]
    pub taxes: Vec<VatBreakdown>,
    /// The lines; absent when not recorded.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub lines: Vec<InvoiceLine>,
    /// The totals.
    pub totals: InvoiceTotals,
}

impl Object for InvoiceData {
    const TYPE: &'static str = "invoice_data";
}

impl Object for InvoiceParty {
    const TYPE: &'static str = "invoice_party";
}

impl Object for VatBreakdown {
    const TYPE: &'static str = "vat_breakdown";
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
            });
        }
        Ok(())
    };
    let mut total: i128 = 0;
    for breakdown in &invoice.taxes {
        let code = de_tax_code(breakdown.category, breakdown.rate, side, invoice.seller.country);
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
    Ok(Booking { date: document.date, text, postings, documents: vec![document.id.clone()] })
}
