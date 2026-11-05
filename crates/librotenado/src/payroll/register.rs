// SPDX-License-Identifier: MIT
//! The employments and payslips of a set of books, and what about them needs review (spec part 0013).

use std::collections::BTreeMap;

use crate::{Date, EventHash, EventSet, PartyId, Payload};

use super::{Employment, EmploymentId, Payslip, PayslipId};

/// Something about the payroll that needs review: never resolved silently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PayrollProblem {
    /// An employment with someone who is not an owner of the books.
    #[error("event {event} describes employment {employment} with {owner}, who is not an owner")]
    NotAnOwner {
        /// The event.
        event: EventHash,
        /// The employment.
        employment: EmploymentId,
        /// The employer named.
        owner: PartyId,
    },
    /// A later description of an employment that names another employer or employee.
    #[error("event {event} describes employment {employment} with another employer or employee")]
    Changed {
        /// The event.
        event: EventHash,
        /// The employment.
        employment: EmploymentId,
    },
    /// A payslip of an employment not described (it may not have arrived yet).
    #[error("event {event} records payslip {payslip} of unknown employment {employment}")]
    UnknownEmployment {
        /// The event.
        event: EventHash,
        /// The payslip.
        payslip: PayslipId,
        /// The employment named.
        employment: EmploymentId,
    },
    /// A payslip recorded a second time under one identity; the first in the agreed order applies.
    #[error("event {event} records payslip {payslip} a second time")]
    Duplicate {
        /// The event.
        event: EventHash,
        /// The payslip.
        payslip: PayslipId,
    },
    /// A payslip whose period overlaps that of another of the same employment.
    #[error("event {event}: payslip {payslip} overlaps payslip {other}")]
    Overlap {
        /// The event.
        event: EventHash,
        /// The payslip.
        payslip: PayslipId,
        /// The payslip it overlaps.
        other: PayslipId,
    },
    /// A payslip whose figures the rules compute otherwise from the books as they are – recorded with other rules or
    /// before an earlier payslip arrived.
    #[error("event {event}: the figures of payslip {payslip} are not those the rules give")]
    Figures {
        /// The event.
        event: EventHash,
        /// The payslip.
        payslip: PayslipId,
    },
    /// A correction of a payslip not recorded, of another employment or period, or replaced already.
    #[error("event {event}: payslip {payslip} corrects a payslip it cannot replace")]
    Correction {
        /// The event.
        event: EventHash,
        /// The payslip.
        payslip: PayslipId,
    },
    /// A payslip whose period ends before it begins, lies outside the employment or spans two calendar months.
    #[error("event {event}: payslip {payslip} has a period it cannot have")]
    Period {
        /// The event.
        event: EventHash,
        /// The payslip.
        payslip: PayslipId,
    },
}

impl EventSet {
    /// The employments of `owner` as described on `day`: per employment the latest description in the agreed order
    /// whose `from` is not after it, of those that began by then.
    pub fn employments(&self, owner: &PartyId, day: Date) -> BTreeMap<EmploymentId, Employment> {
        let mut current: BTreeMap<EmploymentId, Employment> = BTreeMap::new();
        for (_, event) in self.ordered() {
            if let Payload::EmploymentDescribed { employment } = &event.payload
                && employment.owner == *owner
                && employment.from <= day
                && current.get(&employment.id).is_none_or(|e| e.from <= employment.from)
            {
                current.insert(employment.id.clone(), (**employment).clone());
            }
        }
        current
    }

    /// The description of one employment that applies on `day`.
    pub fn employment(&self, id: &EmploymentId, day: Date) -> Option<Employment> {
        let mut found: Option<Employment> = None;
        for (_, event) in self.ordered() {
            if let Payload::EmploymentDescribed { employment } = &event.payload
                && employment.id == *id
                && employment.from <= day
                && found.as_ref().is_none_or(|e| e.from <= employment.from)
            {
                found = Some((**employment).clone());
            }
        }
        found
    }

    /// Every payslip of the employments of `owner`, each once, in the agreed order – corrections and those they
    /// replace alike.
    pub fn recorded_payslips(&self, owner: &PartyId) -> Vec<Payslip> {
        let mut employer: BTreeMap<EmploymentId, PartyId> = BTreeMap::new();
        let mut seen: Vec<PayslipId> = Vec::new();
        let mut payslips = Vec::new();
        for (_, event) in self.ordered() {
            match &event.payload {
                Payload::EmploymentDescribed { employment } => {
                    employer.entry(employment.id.clone()).or_insert_with(|| employment.owner.clone());
                }
                Payload::PayslipRecorded { payslip } if !seen.contains(&payslip.id) => {
                    seen.push(payslip.id.clone());
                    payslips.push(payslip.clone());
                }
                _ => {}
            }
        }
        payslips.retain(|p| employer.get(&p.employment) == Some(owner));
        payslips
    }

    /// The payslips of the employments of `owner` that apply, in the agreed order: those no correction replaces.
    pub fn payslips(&self, owner: &PartyId) -> Vec<Payslip> {
        let recorded = self.recorded_payslips(owner);
        let replaced: Vec<PayslipId> = recorded.iter().filter_map(|p| p.replaces.clone()).collect();
        recorded.into_iter().filter(|p| !replaced.contains(&p.id)).collect()
    }

    /// Checks the payroll: employments of owners that keep their employer and employee, payslips of known
    /// employments, each once, within the employment and a calendar month, not overlapping another, with the figures
    /// the rules give.
    pub fn review_payroll(&self) -> Vec<PayrollProblem> {
        let owners = self.owners();
        let mut problems = Vec::new();
        let mut parties: BTreeMap<EmploymentId, (PartyId, PartyId)> = BTreeMap::new();
        let mut spans: BTreeMap<EmploymentId, Vec<(Date, Option<Date>)>> = BTreeMap::new();
        let mut recorded: BTreeMap<PayslipId, ()> = BTreeMap::new();
        let mut periods: BTreeMap<EmploymentId, Vec<(PayslipId, Date, Date)>> = BTreeMap::new();
        let mut checked: Vec<(EventHash, Payslip)> = Vec::new();
        for (hash, event) in self.ordered() {
            match &event.payload {
                Payload::EmploymentDescribed { employment } => {
                    let pair = (employment.owner.clone(), employment.employee.clone());
                    match parties.get(&employment.id) {
                        Some(known) if *known != pair => {
                            problems.push(PayrollProblem::Changed { event: hash, employment: employment.id.clone() })
                        }
                        _ => {
                            parties.insert(employment.id.clone(), pair);
                        }
                    }
                    if !owners.contains(&&employment.owner) {
                        problems.push(PayrollProblem::NotAnOwner {
                            event: hash,
                            employment: employment.id.clone(),
                            owner: employment.owner.clone(),
                        });
                    }
                    spans.entry(employment.id.clone()).or_default().push((employment.start, employment.end));
                }
                Payload::PayslipRecorded { payslip } => {
                    if recorded.insert(payslip.id.clone(), ()).is_some() {
                        problems.push(PayrollProblem::Duplicate { event: hash, payslip: payslip.id.clone() });
                        continue;
                    }
                    let Some(span) = spans.get(&payslip.employment) else {
                        problems.push(PayrollProblem::UnknownEmployment {
                            event: hash,
                            payslip: payslip.id.clone(),
                            employment: payslip.employment.clone(),
                        });
                        continue;
                    };
                    let within = span
                        .iter()
                        .any(|(start, end)| payslip.from >= *start && end.is_none_or(|end| payslip.to <= end));
                    let one_month =
                        (payslip.from.year(), payslip.from.month()) == (payslip.to.year(), payslip.to.month());
                    if payslip.to < payslip.from || !within || !one_month {
                        problems.push(PayrollProblem::Period { event: hash, payslip: payslip.id.clone() });
                    }
                    let others = periods.entry(payslip.employment.clone()).or_default();
                    // A correction replaces the payslip of the same employment and period that applies.
                    if let Some(replaced) = &payslip.replaces {
                        match others.iter().position(|(id, _, _)| id == replaced) {
                            Some(i) if (others[i].1, others[i].2) == (payslip.from, payslip.to) => {
                                others.remove(i);
                            }
                            _ => problems.push(PayrollProblem::Correction { event: hash, payslip: payslip.id.clone() }),
                        }
                    }
                    if let Some((other, _, _)) =
                        others.iter().find(|(_, from, to)| payslip.from <= *to && *from <= payslip.to)
                    {
                        problems.push(PayrollProblem::Overlap {
                            event: hash,
                            payslip: payslip.id.clone(),
                            other: other.clone(),
                        });
                    }
                    others.push((payslip.id.clone(), payslip.from, payslip.to));
                    checked.push((hash, payslip.clone()));
                }
                _ => {}
            }
        }
        // The figures of the payslips that apply, from the payslips before them that apply; only figures the rules
        // can compute are checked – an employment under other law is not.
        let replaced: Vec<PayslipId> = checked.iter().filter_map(|(_, p)| p.replaces.clone()).collect();
        for (hash, payslip) in checked {
            if replaced.contains(&payslip.id) {
                continue;
            }
            if let Ok(figures) = self.payslip_figures(&payslip.employment, payslip.from, payslip.to, &payslip.items)
                && figures != payslip.figures
            {
                problems.push(PayrollProblem::Figures { event: hash, payslip: payslip.id.clone() });
            }
        }
        problems
    }
}
