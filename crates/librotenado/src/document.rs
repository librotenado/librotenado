// SPDX-License-Identifier: MIT
//! Documents (spec part 0005): receipts, invoices and statements as records of their own, each with the file it came
//! as – the original – kept as an artefact.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest, Sha256};

use crate::{Date, InvoiceData, Object, ParseError};

/// The identity of a document: a UUID in its one form (spec part 0001), like an account's. It carries no meaning.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(String);

impl DocumentId {
    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for DocumentId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(DocumentId(s.to_owned())) } else { Err(ParseError::Document(s.to_owned())) }
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for DocumentId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DocumentId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// The SHA-256 hash of a file's bytes: `sha256:` and 64 lower-case hexadecimal digits. The bytes themselves are
/// stored outside the events, in any store addressed by this hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// The hash of these bytes.
    pub fn of(bytes: &[u8]) -> Self {
        ContentHash(Sha256::digest(bytes).into())
    }
}

impl FromStr for ContentHash {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        crate::parse_sha256(s).map(ContentHash).ok_or_else(|| ParseError::Hash(s.to_owned()))
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sha256:{}", hex::encode(self.0))
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// What kind of document it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    /// An invoice.
    Invoice,
    /// A credit note, cancelling an invoice in whole or in part.
    CreditNote,
    /// A receipt: a till receipt, a ticket, a confirmation of payment.
    Receipt,
    /// A statement of a bank or payment account.
    BankStatement,
    /// An authority's confirmation that a filing was received (spec part 0007).
    Acknowledgement,
    /// An authority's notice or assessment, e.g. a tax assessment.
    Notice,
    /// A kind of a later version; kept as received, not interpreted, and not written by this version.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// The file a document came as – an artefact: the document's data never depends on reading it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Original {
    /// The hash of its bytes.
    pub hash: ContentHash,
    /// Its size in bytes.
    #[serde(deserialize_with = "crate::safe_u64")]
    pub size: u64,
    /// Its media type, for example `application/pdf` or `application/xml`.
    pub media_type: String,
    /// The structured format it carries, if any, for example `en16931-cii` (also a PDF with an embedded invoice),
    /// `en16931-ubl` or `iso20022-camt.053`. Names of formats are listed in the specification.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub format: Option<String>,
    /// The file name it arrived with, if any.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub name: Option<String>,
}

impl Original {
    /// Describes these bytes; the hash and size are taken from them.
    pub fn of(bytes: &[u8], media_type: &str) -> Self {
        Original {
            hash: ContentHash::of(bytes),
            size: bytes.len() as u64,
            media_type: media_type.to_owned(),
            format: None,
            name: None,
        }
    }

    /// Whether these bytes are this original: the same size and the same hash.
    pub fn is(&self, bytes: &[u8]) -> bool {
        bytes.len() as u64 == self.size && ContentHash::of(bytes) == self.hash
    }
}

/// A document: what a booking is based on, as a record of its own. There is no document without its original.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Document {
    /// Its identity.
    pub id: DocumentId,
    /// What kind of document it is.
    pub kind: DocumentKind,
    /// Its number as printed on it (for example the invoice number), if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub number: Option<String>,
    /// The date it bears (for example the invoice date).
    pub date: Date,
    /// The day it was received – which may be earlier than the day it was recorded, e.g. a letter scanned later.
    pub received: Date,
    /// The file it came as.
    pub original: Original,
    /// The data of an invoice or credit note: parties, VAT and totals; absent when not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub invoice: Option<Box<InvoiceData>>,
    /// The data of a bank statement: account, balances, entries; absent when not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub statement: Option<Box<crate::StatementData>>,
}

impl Object for Document {
    const TYPE: &'static str = "document";
}

impl Object for Original {
    const TYPE: &'static str = "original";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(original: Original) -> Document {
        Document {
            id: "0192f1e4-0000-7000-8000-0000000000d1".parse().unwrap(),
            kind: DocumentKind::Invoice,
            number: Some("RE-2026-0815".into()),
            date: "2026-10-20".parse().unwrap(),
            received: "2026-10-24".parse().unwrap(),
            original,
            invoice: None,
            statement: None,
        }
    }

    #[test]
    fn the_original_is_an_artefact_checked_by_size_and_hash() {
        let pdf = b"%PDF-1.7 an invoice";
        let original = Original::of(pdf, "application/pdf");
        assert!(original.is(pdf));
        assert!(!original.is(b"%PDF-1.7 an invoice, changed"));
        let written = original.hash.to_string();
        assert!(written.starts_with("sha256:") && written.len() == 71);
        assert_eq!(written.parse::<ContentHash>().unwrap(), original.hash);
        assert!("sha1:00".parse::<ContentHash>().is_err());
    }

    #[test]
    fn the_data_stands_on_its_own() {
        let doc = document(Original::of(b"x", "text/plain"));
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains(r#""kind":"invoice""#) && json.contains(r#""number":"RE-2026-0815""#), "{json}");
        assert!(!json.contains("format") && !json.contains(r#""name""#));
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), doc);
        let empty = json.replace("RE-2026-0815", "");
        assert!(serde_json::from_str::<Document>(&empty).is_err(), "an empty number is left out");
    }
}
