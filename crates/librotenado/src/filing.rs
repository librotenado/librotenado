// SPDX-License-Identifier: MIT
//! Filings (spec part 0007): what books end in – VAT returns, income statements, annual accounts – derived from the
//! books, with the state they were derived from, and the events of submitting them through an adapter.

use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{
    BalanceError, Bytes, Chart, ContentHash, Country, Currency, Date, DocumentId, EventSet, Money, Object,
    Organisation, Original, ParseError, PartyId, Payload, Person, ReportMapping, TaxTable,
};

/// The identity of a filing: a UUID in lower case, like the identities of documents.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilingId(String);

impl FilingId {
    /// The identity as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for FilingId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if crate::is_uuid(s) { Ok(FilingId(s.to_owned())) } else { Err(ParseError::Filing(s.to_owned())) }
    }
}

impl fmt::Display for FilingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for FilingId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FilingId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
    }
}

/// What a filing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilingKind {
    /// A VAT return for a month, a quarter or a year – in Germany the Umsatzsteuer-Voranmeldung.
    VatReturn,
    /// An income statement on a cash basis – in Germany the Anlage EÜR.
    CashBasisStatement,
    /// Annual accounts: balance sheet and profit and loss account – in Germany the E-Bilanz.
    AnnualAccounts,
    /// A kind of a later version; kept as received, not interpreted, and not written by this version.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// The fields of one report in a filing: positions and their amounts, as a report mapping (spec part 0009) derives
/// them; a mandatory position without an amount is left out, the adapter states it as empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportContent {
    /// The report, e.g. `de-ustva`, `de-ebilanz-bs`.
    pub report: String,
    /// The version of its mapping, e.g. `2026` or `6.10`.
    pub version: String,
    /// Position → amount, in the filer's functional currency; absent when no position has an amount – a VAT return
    /// with nothing to report is filed all the same.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "crate::non_empty_map")]
    pub fields: BTreeMap<String, Money>,
}

/// A filing: derived from the books, never changed; a correction is a new filing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Filing {
    /// Its identity.
    pub id: FilingId,
    /// What it is.
    pub kind: FilingKind,
    /// The country it is filed in.
    pub jurisdiction: Country,
    /// The organisation or person filing, an owner of the books.
    pub filer: PartyId,
    /// The first day of the period it covers.
    pub from: Date,
    /// The last day of the period it covers.
    pub to: Date,
    /// The state of the books it was derived from (spec part 0002).
    pub basis: ContentHash,
    /// The filing it corrects; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present")]
    pub corrects: Option<FilingId>,
    /// What it reports, one entry per report, at least one.
    #[serde(deserialize_with = "crate::non_empty")]
    pub content: Vec<ReportContent>,
}

impl Object for Filing {
    const TYPE: &'static str = "filing";
}

impl Object for ReportContent {
    const TYPE: &'static str = "report_content";
}

/// How a report is derived: over a period from the postings, or on the last day from the balances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Derivation {
    /// From the postings of the period – a VAT return, a profit and loss account, an income statement.
    Period,
    /// From the balances on the last day – a balance sheet.
    Balances,
}

impl EventSet {
    /// The content of one report for `filer` from `from` to `to`, in base units of `currency`, the filer's
    /// functional currency. Positions without an amount are left out.
    #[allow(clippy::too_many_arguments)]
    pub fn report_content(
        &self,
        mapping: &ReportMapping,
        derivation: Derivation,
        taxes: &TaxTable,
        chart: &Chart,
        filer: &PartyId,
        currency: &Currency,
        from: Date,
        to: Date,
    ) -> Result<ReportContent, BalanceError> {
        let units = match derivation {
            Derivation::Period => self.period_report(mapping, taxes, chart, filer, from, to)?,
            Derivation::Balances => self.account_report(mapping, chart, filer, Some(to))?,
        };
        let fields = units
            .into_iter()
            .filter(|(_, units)| *units != 0)
            .map(|(position, units)| Money::new(units, currency.clone()).map(|m| (position, m)))
            .collect::<Option<_>>()
            .ok_or(BalanceError::Overflow)?;
        Ok(ReportContent { report: mapping.report.clone(), version: mapping.version.clone(), fields })
    }

    /// Where each filing stands: the latest of its events, by filing.
    pub fn filings(&self) -> BTreeMap<FilingId, (Filing, FilingState)> {
        let mut filings = BTreeMap::new();
        for (_, event) in self.ordered() {
            match &event.payload {
                Payload::FilingPrepared { filing } => {
                    filings.entry(filing.id.clone()).or_insert((filing.clone(), FilingState::Prepared));
                }
                Payload::FilingSubmitted { filing, .. } => set_state(&mut filings, filing, FilingState::Submitted),
                Payload::FilingAccepted { filing, .. } => set_state(&mut filings, filing, FilingState::Accepted),
                Payload::FilingRejected { filing, .. } => set_state(&mut filings, filing, FilingState::Rejected),
                _ => {}
            }
        }
        filings
    }

    /// Checks the filings: an authority's rejection, events about a filing that was never prepared, and a filing
    /// recorded twice under one identity – none is resolved silently (spec part 0002).
    pub fn review_filings(&self) -> Vec<FilingProblem> {
        let mut problems = Vec::new();
        let mut prepared: BTreeMap<&FilingId, usize> = BTreeMap::new();
        for (_, event) in self.ordered() {
            if let Payload::FilingPrepared { filing } = &event.payload {
                *prepared.entry(&filing.id).or_default() += 1;
            }
        }
        for (id, count) in &prepared {
            if *count > 1 {
                problems.push(FilingProblem::Duplicate((*id).clone()));
            }
        }
        for (_, event) in self.ordered() {
            if let Payload::FilingSubmitted { filing, .. }
            | Payload::FilingAccepted { filing, .. }
            | Payload::FilingRejected { filing, .. } = &event.payload
                && !prepared.contains_key(filing)
            {
                problems.push(FilingProblem::Unknown(filing.clone()));
            }
        }
        for (id, (_, state)) in self.filings() {
            if state == FilingState::Rejected {
                problems.push(FilingProblem::Rejected(id));
            }
        }
        problems
    }
}

fn set_state(filings: &mut BTreeMap<FilingId, (Filing, FilingState)>, id: &FilingId, state: FilingState) {
    if let Some((_, current)) = filings.get_mut(id) {
        // Each event moves the filing on; after a rejection it may be submitted again.
        if *current < state || *current == FilingState::Rejected && state == FilingState::Submitted {
            *current = state;
        }
    }
}

/// Where a filing stands, in the order its events follow each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FilingState {
    /// Derived, not yet submitted.
    Prepared,
    /// Submitted; the authority has not answered yet.
    Submitted,
    /// Accepted by the authority.
    Accepted,
    /// Rejected by the authority.
    Rejected,
}

/// The organisation or person filing, as an adapter needs it: with its tax identities. Written as an object with one
/// member, `organisation` or `person`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filer {
    /// An organisation (spec part 0008).
    Organisation(Organisation),
    /// A person (spec part 0008).
    Person(Person),
}

impl Filer {
    /// The filer's identity.
    pub fn id(&self) -> &PartyId {
        match self {
            Filer::Organisation(organisation) => &organisation.id,
            Filer::Person(person) => &person.id,
        }
    }
}

/// What an implementation sends to an adapter: a prepared filing and who files it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Submission {
    /// The filing, as prepared.
    pub filing: Filing,
    /// The organisation or person filing; the filing's `filer`.
    pub filer: Filer,
    /// A test submission the authority does not process.
    pub test: bool,
}

impl Object for Submission {
    const TYPE: &'static str = "submission";
}

impl Submission {
    /// A submission of `filing` by `filer`, which must be the filing's filer.
    pub fn new(filing: Filing, filer: Filer, test: bool) -> Result<Self, SubmissionError> {
        let submission = Submission { filing, filer, test };
        submission.check()?;
        Ok(submission)
    }

    /// Checks what an adapter cannot submit: a filer other than the filing's.
    pub fn check(&self) -> Result<(), SubmissionError> {
        if self.filer.id() == &self.filing.filer {
            Ok(())
        } else {
            Err(SubmissionError::Filer { filing: self.filing.filer.clone(), filer: self.filer.id().clone() })
        }
    }
}

/// Why a submission cannot be submitted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubmissionError {
    /// The filer is not the filing's filer.
    #[error("the filing is filed by {filing}, the submission names {filer}")]
    Filer {
        /// The filing's filer.
        filing: PartyId,
        /// The filer of the submission.
        filer: PartyId,
    },
}

/// Where a submitted filing stands at the authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionState {
    /// Submitted; the authority has not answered yet.
    Submitted,
    /// Accepted by the authority.
    Accepted,
    /// Rejected by the authority.
    Rejected,
}

/// Something the authority returned – a confirmation of receipt, a protocol, a notice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Receipt {
    /// Its media type, for example `application/pdf`.
    pub media_type: String,
    /// Its file name.
    pub name: String,
    /// Its bytes.
    pub content: Bytes,
}

impl Object for Receipt {
    const TYPE: &'static str = "receipt";
}

impl Receipt {
    /// The original of the document an implementation records it as (spec part 0005).
    pub fn original(&self) -> Original {
        Original { name: Some(self.name.clone()), ..Original::of(&self.content.0, &self.media_type) }
    }
}

/// What an adapter answers about a submitted filing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubmissionStatus {
    /// The filing.
    pub filing: FilingId,
    /// Where it stands.
    pub state: SubmissionState,
    /// What the authority returned; left out when nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "crate::non_empty")]
    pub receipts: Vec<Receipt>,
    /// How the authority explained a rejection; left out otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::present_text")]
    pub message: Option<String>,
}

impl Object for SubmissionStatus {
    const TYPE: &'static str = "submission_status";
}

impl SubmissionStatus {
    /// The event that records this state, with the receipts recorded as `documents`.
    pub fn payload(&self, documents: Vec<DocumentId>) -> Payload {
        let filing = self.filing.clone();
        match self.state {
            SubmissionState::Submitted => Payload::FilingSubmitted { filing, documents },
            SubmissionState::Accepted => Payload::FilingAccepted { filing, documents },
            SubmissionState::Rejected => Payload::FilingRejected { filing, documents },
        }
    }
}

/// What an adapter answers with `4xx` or `5xx` when it cannot process a request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterError {
    /// Why, in words.
    pub message: String,
}

impl Object for AdapterError {
    const TYPE: &'static str = "adapter_error";
}

/// Something about the filings that needs review.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FilingProblem {
    /// The authority rejected the filing.
    #[error("filing {0} was rejected")]
    Rejected(FilingId),
    /// An event about a filing that was never prepared.
    #[error("filing {0} was never prepared")]
    Unknown(FilingId),
    /// A filing prepared twice under the same identity.
    #[error("filing {0} was prepared twice")]
    Duplicate(FilingId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Booking, ChartTranslation, Clock, Event, Posting, party::tests::party};

    fn posting(number: &str, units: i128, tax: Option<&str>) -> Posting {
        Posting {
            owner: party(1),
            account: ChartTranslation::fr_pcg().accounts[number].clone(),
            amount: Money::new(units, "EUR".parse().unwrap()).unwrap(),
            note: None,
            tax: tax.map(|t| t.parse().unwrap()),
            exchange: None,
        }
    }

    fn record(set: &mut EventSet, clock: &mut Clock, payload: Payload) {
        let (seq, prev) = set.chain_head(&"a".parse().unwrap());
        let time = clock.now(seq);
        set.insert(Event { time, seq, prev, payload });
    }

    /// A VAT return derived from the books, rejected, submitted again and accepted.
    #[test]
    fn a_vat_return_through_its_events() {
        let mut set = EventSet::new();
        let mut clock = Clock::new("a".parse().unwrap());
        let booking = Booking {
            date: "2026-03-15".parse().unwrap(),
            text: "sale".into(),
            postings: vec![
                posting("411", 119_000, None),
                posting("706", -100_000, Some("de-out-19")),
                posting("44571", -19_000, Some("de-out-19")),
            ],
            documents: vec![],
            entries: vec![],
            transaction: None,
        };
        record(&mut set, &mut clock, Payload::BookingRecorded { booking });
        let (from, to) = ("2026-03-01".parse().unwrap(), "2026-03-31".parse().unwrap());
        let eur = "EUR".parse().unwrap();
        let content = set
            .report_content(
                &ReportMapping::de_ustva_2026(),
                Derivation::Period,
                &TaxTable::de(),
                &Chart::v0_1(),
                &party(1),
                &eur,
                from,
                to,
            )
            .unwrap();
        assert_eq!(content.fields["81"].base_units(), 100_000);
        let filing = Filing {
            id: "0192f1e4-0000-7000-8000-0000000000f1".parse().unwrap(),
            kind: FilingKind::VatReturn,
            jurisdiction: "DE".parse().unwrap(),
            filer: party(1),
            from,
            to,
            basis: set.state(),
            corrects: None,
            content: vec![content],
        };
        let id = filing.id.clone();
        let event = Payload::FilingPrepared { filing };
        record(&mut set, &mut clock, event);
        record(&mut set, &mut clock, Payload::FilingSubmitted { filing: id.clone(), documents: vec![] });
        record(&mut set, &mut clock, Payload::FilingRejected { filing: id.clone(), documents: vec![] });
        assert_eq!(set.review_filings(), [FilingProblem::Rejected(id.clone())]);
        record(&mut set, &mut clock, Payload::FilingSubmitted { filing: id.clone(), documents: vec![] });
        record(&mut set, &mut clock, Payload::FilingAccepted { filing: id.clone(), documents: vec![] });
        assert_eq!(set.filings()[&id].1, FilingState::Accepted);
        assert!(set.review_filings().is_empty());
        // Every event reads back as written.
        for (_, event) in set.ordered() {
            let json = crate::to_root(event).unwrap();
            assert_eq!(&crate::from_root::<Event>(&json).unwrap(), event);
        }
        let unknown = "0192f1e4-0000-7000-8000-0000000000f2".parse::<FilingId>().unwrap();
        record(&mut set, &mut clock, Payload::FilingAccepted { filing: unknown.clone(), documents: vec![] });
        assert_eq!(set.review_filings(), [FilingProblem::Unknown(unknown)]);
    }

    /// A filing goes to an adapter as a submission with its filer; the adapter's answers become events.
    #[test]
    fn a_submission_and_its_status() {
        let eur: Currency = "EUR".parse().unwrap();
        let filing = Filing {
            id: "0192f1e4-0000-7000-8000-0000000000f1".parse().unwrap(),
            kind: FilingKind::VatReturn,
            jurisdiction: "DE".parse().unwrap(),
            filer: party(1),
            from: "2026-03-01".parse().unwrap(),
            to: "2026-03-31".parse().unwrap(),
            basis: ContentHash::of(b"books"),
            corrects: None,
            content: vec![ReportContent {
                report: "de-ustva".into(),
                version: "2026".into(),
                fields: [("81".to_owned(), Money::new(100_000, eur).unwrap())].into(),
            }],
        };
        let filer = Filer::Organisation(crate::party::tests::organisation(1));
        let submission = Submission::new(filing.clone(), filer.clone(), true).unwrap();
        let json = String::from_utf8(crate::to_root(&submission).unwrap()).unwrap();
        assert!(json.contains(r#""filer":{"organisation":{"#), "{json}");
        assert_eq!(crate::from_root::<Submission>(json.as_bytes()).unwrap(), submission);

        // Someone else cannot file it, and an adapter finds out when it checks what it received.
        let other = Filer::Organisation(crate::party::tests::organisation(2));
        let error = SubmissionError::Filer { filing: party(1), filer: party(2) };
        assert_eq!(Submission::new(filing.clone(), other.clone(), true), Err(error.clone()));
        assert_eq!(Submission { filing, filer: other, test: false }.check(), Err(error));

        let status = SubmissionStatus {
            filing: submission.filing.id.clone(),
            state: SubmissionState::Accepted,
            receipts: vec![Receipt {
                media_type: "application/pdf".into(),
                name: "protocol.pdf".into(),
                content: Bytes(b"%PDF-1.7".to_vec()),
            }],
            message: None,
        };
        let json = String::from_utf8(crate::to_root(&status).unwrap()).unwrap();
        assert!(json.contains(r#""content":"JVBERi0xLjc""#), "{json}");
        assert_eq!(crate::from_root::<SubmissionStatus>(json.as_bytes()).unwrap(), status);
        let original = status.receipts[0].original();
        assert!(original.is(b"%PDF-1.7"));
        assert_eq!(original.name.as_deref(), Some("protocol.pdf"));
        let document: DocumentId = "0192f1e4-0000-7000-8000-0000000000d1".parse().unwrap();
        assert_eq!(
            status.payload(vec![document.clone()]),
            Payload::FilingAccepted { filing: status.filing.clone(), documents: vec![document] }
        );

        // A filing with nothing to report leaves its fields out, and reads back as written.
        let nil = Submission {
            filing: Filing {
                content: vec![ReportContent { fields: BTreeMap::new(), ..submission.filing.content[0].clone() }],
                ..submission.filing.clone()
            },
            ..submission.clone()
        };
        let json = crate::to_root(&nil).unwrap();
        assert!(String::from_utf8_lossy(&json).contains(r#""content":[{"report":"de-ustva","version":"2026"}]"#));
        assert_eq!(crate::from_root::<Submission>(&json).unwrap(), nil);

        // Empty receipts and messages are left out, not written.
        let empty = r#"{"filing":"0192f1e4-0000-7000-8000-0000000000f1","message":"","state":"rejected","type":"submission_status"}"#;
        assert!(crate::from_root::<SubmissionStatus>(empty.as_bytes()).is_err());
        let error = AdapterError { message: "no certificate".into() };
        assert_eq!(crate::to_root(&error).unwrap(), br#"{"message":"no certificate","type":"adapter_error"}"#);
    }
}
