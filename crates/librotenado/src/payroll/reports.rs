// SPDX-License-Identifier: MIT
//! The filings of an employer from its payslips (spec part 0013): in Germany the Lohnsteuer-Anmeldung and the
//! Beitragsnachweis.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Date, EventSet, Money, PartyId, ReportMapping};

use super::{Employment, Payslip};

impl EventSet {
    /// The payslips of `owner` for periods ending from `from` to `to`, each with its employment as described on the
    /// last day of its period.
    pub fn payslips_for(&self, owner: &PartyId, from: Date, to: Date) -> Vec<(Payslip, Option<Employment>)> {
        self.payslips(owner)
            .into_iter()
            .filter(|p| from <= p.to && p.to <= to)
            .map(|p| {
                let employment = self.employment(&p.employment, p.to);
                (p, employment)
            })
            .collect()
    }

    /// The fields of a report of the payroll (`payroll_fields`): per field the sum of the figures it names, over the
    /// payslips of `owner` for periods ending from `from` to `to`, in base units.
    pub fn payroll_report(
        &self,
        mapping: &ReportMapping,
        owner: &PartyId,
        from: Date,
        to: Date,
    ) -> BTreeMap<String, i128> {
        let mut fields = BTreeMap::new();
        for (payslip, employment) in self.payslips_for(owner, from, to) {
            let church = employment.as_ref().and_then(|e| e.de.as_ref()).and_then(|de| de.tax.church.clone());
            for (field, names) in &mapping.payroll_fields {
                for name in names {
                    let (figure, community) = match name.split_once('@') {
                        Some((figure, community)) => (figure, Some(community)),
                        None => (name.as_str(), None),
                    };
                    if community.is_some() && community != church.as_deref() {
                        continue;
                    }
                    let units = payslip.figures.get(figure).map_or(0, Money::base_units);
                    *fields.entry(field.clone()).or_insert(0) += units;
                }
            }
        }
        fields.retain(|_, units| *units != 0);
        fields
    }

    /// How many employees `owner` paid wages to for periods ending from `from` to `to` (Lohnsteuer-Anmeldung, Kz 86).
    pub fn employees_paid(&self, owner: &PartyId, from: Date, to: Date) -> usize {
        let paid: BTreeSet<_> = self.payslips_for(owner, from, to).into_iter().map(|(p, _)| p.employment).collect();
        paid.len()
    }

    /// The Beitragsnachweis of `owner` to the insurer `insurer` (its company number) for the months from `from` to
    /// `to`: the contributions by the groups of the record (spec part 0013), in cents.
    pub fn contribution_statement(
        &self,
        owner: &PartyId,
        insurer: &str,
        from: Date,
        to: Date,
    ) -> BTreeMap<&'static str, i128> {
        let mut fields: BTreeMap<&'static str, i128> = BTreeMap::new();
        for (payslip, employment) in self.payslips_for(owner, from, to) {
            let Some(insurance) = employment.as_ref().and_then(|e| e.de.as_ref()).map(|de| &de.insurance) else {
                continue;
            };
            if insurance.insurer != insurer {
                continue;
            }
            let figure = |name: &str| payslip.figures.get(name).map_or(0, Money::base_units);
            let both = |name: &str| figure(&format!("{name}_employee")) + figure(&format!("{name}_employer"));
            let digit = |i: usize| insurance.contribution_group.as_bytes().get(i).copied().unwrap_or(b'0');
            let mut add = |field: &'static str, units: i128| *fields.entry(field).or_insert(0) += units;
            match digit(0) {
                b'1' => add("1000", both("health")),
                b'3' => add("3000", both("health")),
                b'6' => add("6000", figure("health_flat")),
                _ => {}
            }
            add("zusatzbeitrag", both("health_additional"));
            add("0001", both("care"));
            match digit(1) {
                b'1' => add("0100", both("pension") + figure("pension_flat")),
                b'3' => add("0300", both("pension")),
                b'5' => add("0500", figure("pension_flat")),
                _ => {}
            }
            match digit(2) {
                b'1' => add("0010", both("unemployment")),
                b'2' => add("0020", both("unemployment")),
                _ => {}
            }
            add("0050", figure("u3"));
            add("u1", figure("u1"));
            add("u2", figure("u2"));
            add("pauschsteuer", figure("flat_tax"));
        }
        fields.retain(|_, units| *units != 0);
        fields
    }
}
