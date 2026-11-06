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

    /// The Lohnsteuerbescheinigung of an employment for a calendar year (§ 41b EStG; the form of 2026, BMF letter of
    /// 29 August 2025): its numbered lines from the payslips for periods ending in the year, in cents. `None` for an
    /// employment taxed at a flat rate, which has none (§ 41b Abs. 6 EStG), or without payslips in the year.
    pub fn wage_tax_certificate(
        &self,
        employment: &super::EmploymentId,
        year: u16,
    ) -> Option<BTreeMap<&'static str, i128>> {
        let last = Date::new(year, 12, 31)?;
        let described = self.employment(employment, last)?;
        let de = described.de.as_ref()?;
        if de.tax.flat || de.tax.short_term {
            return None;
        }
        let payslips: Vec<Payslip> = self
            .payslips(&described.owner)
            .into_iter()
            .filter(|p| p.employment == *employment && p.to.year() == year)
            .collect();
        if payslips.is_empty() {
            return None;
        }
        let mut lines: BTreeMap<&'static str, i128> = BTreeMap::new();
        for payslip in &payslips {
            let figure = |name: &str| payslip.figures.get(name).map_or(0, Money::base_units);
            let items = |kinds: &[super::PayItemKind]| -> i128 {
                payslip.items.iter().filter(|i| kinds.contains(&i.kind)).map(|i| i.amount.base_units()).sum()
            };
            use super::PayItemKind::{Compensation, InKind, OneOff, Pension, Wage};
            let mut add = |line: &'static str, units: i128| *lines.entry(line).or_insert(0) += units;
            // 3: wages with benefits in kind, without those of line 10; 10: compensation (§ 34 EStG).
            // Wages free of tax as Aktivrente are not part of it.
            add("3", items(&[Wage, InKind, OneOff, Pension]) - figure("active_pension_free"));
            // 8: pensions, contained in 3.
            add("8", items(&[Pension]));
            add("10", items(&[Compensation]));
            add("4", figure("wage_tax"));
            add("5", figure("solidarity"));
            add("6", figure("church"));
            add("22a", figure("pension_employer"));
            add("23a", figure("pension_employee"));
            add("24b", figure("private_subsidy"));
            add("25", figure("health_employee") + figure("health_additional_employee"));
            add("26", figure("care_employee"));
            add("27", figure("unemployment_employee"));
        }
        lines.retain(|_, units| *units != 0);
        Some(lines)
    }

    /// How many employees `owner` paid wages to for periods ending from `from` to `to` (Lohnsteuer-Anmeldung, Kz 86).
    pub fn employees_paid(&self, owner: &PartyId, from: Date, to: Date) -> usize {
        let paid: BTreeSet<_> = self.payslips_for(owner, from, to).into_iter().map(|(p, _)| p.employment).collect();
        paid.len()
    }

    /// The Beitragsnachweis of `owner` to the insurer `insurer` (its company number) for the months from `from` to
    /// `to`: the contributions by the groups of the record (spec part 0013), in cents – of the payslips for periods
    /// ending in them, and the differences of the corrections paid in them (Gemeinsame Grundsätze, Ziffer 6).
    pub fn contribution_statement(
        &self,
        owner: &PartyId,
        insurer: &str,
        from: Date,
        to: Date,
    ) -> BTreeMap<&'static str, i128> {
        let recorded = self.recorded_payslips(owner);
        let mut fields: BTreeMap<&'static str, i128> = BTreeMap::new();
        for payslip in &recorded {
            let (counted, sign) = match &payslip.replaces {
                None if from <= payslip.to && payslip.to <= to => (payslip, 1),
                Some(_) if from <= payslip.paid && payslip.paid <= to => (payslip, 1),
                _ => continue,
            };
            self.add_contributions(&mut fields, counted, insurer, sign);
            if let Some(replaced) = payslip.replaces.as_ref().and_then(|id| recorded.iter().find(|p| p.id == *id)) {
                self.add_contributions(&mut fields, replaced, insurer, -1);
            }
        }
        fields.retain(|_, units| *units != 0);
        fields
    }

    /// Adds the contributions of one payslip to an insurer, times `sign`, by the groups of the record.
    fn add_contributions(
        &self,
        fields: &mut BTreeMap<&'static str, i128>,
        payslip: &Payslip,
        insurer: &str,
        sign: i128,
    ) {
        let employment = self.employment(&payslip.employment, payslip.to);
        let Some(insurance) = employment.as_ref().and_then(|e| e.de.as_ref()).map(|de| &de.insurance) else {
            return;
        };
        if insurance.insurer != insurer {
            return;
        }
        let figure = |name: &str| payslip.figures.get(name).map_or(0, Money::base_units);
        let both = |name: &str| figure(&format!("{name}_employee")) + figure(&format!("{name}_employer"));
        let digit = |i: usize| insurance.contribution_group.as_bytes().get(i).copied().unwrap_or(b'0');
        let mut add = |field: &'static str, units: i128| *fields.entry(field).or_insert(0) += sign * units;
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
        // Contributions on pensions, which the employer as paying agency transmits in the procedure of paying
        // agencies (§ 256 SGB V).
        add("zahlstelle_kv", figure("pension_health"));
        add("zahlstelle_pv", figure("pension_care"));
    }
}
