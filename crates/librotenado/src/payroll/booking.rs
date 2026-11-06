// SPDX-License-Identifier: MIT
//! The postings of a payslip on the accounts of the harmonised chart (spec part 0013).

use crate::{AccountId, Money, PartyId, Posting};

use super::{PayItemKind, Payslip};

/// The accounts of the chart of version 0.1 a German payslip is posted to; in SKR 03 / SKR 04.
mod account {
    /// Gehälter – 4120 / 6020.
    pub const WAGES: &str = "01a18406-adb5-76e9-b217-1b9b3ed5a012";
    /// Aushilfslöhne – 4190 / 6030: the wages of a minor employment.
    pub const MINOR_WAGES: &str = "01a18406-adb5-76e9-b217-1ba13d2cf8ae";
    /// Gesetzliche soziale Aufwendungen – 4130 / 6110.
    pub const SOCIAL: &str = "01a18406-adb7-77d0-85cc-0754e147e181";
    /// Pauschale Steuer für Aushilfen – 4199 / 6036.
    pub const FLAT_TAX: &str = "01a18406-adb6-719c-81ec-c9394a6a9b39";
    /// Verbindlichkeiten aus Lohn- und Kirchensteuer – 1741 / 3730.
    pub const TAX_LIABILITY: &str = "01a18406-ad99-7582-97b7-0a5170b605c8";
    /// Verbindlichkeiten im Rahmen der sozialen Sicherheit – 1742 / 3740.
    pub const SOCIAL_LIABILITY: &str = "01a18406-ad99-7582-97b7-0a52f892d8b0";
    /// Verbindlichkeiten aus Lohn und Gehalt – 1740 / 3720.
    pub const NET_LIABILITY: &str = "01a18406-ad99-7582-97b7-0a4e3d9f7aef";
    /// Lohn- und Gehaltsverrechnungskonto – 1755 / 3790: benefits in kind and deductions, to be settled.
    pub const CLEARING: &str = "01a18406-ad9a-73c4-be5f-29507bbb7d94";
}

/// The figures the employer pays on top of the wages, to the insurers.
const EMPLOYER: [&str; 11] = [
    "health_employer",
    "health_additional_employer",
    "care_employer",
    "pension_employer",
    "unemployment_employer",
    "health_flat",
    "pension_flat",
    "u1",
    "u2",
    "u3",
    "accident",
];

/// What the employee pays to the insurers out of the wages.
const EMPLOYEE: [&str; 7] = [
    "health_employee",
    "health_additional_employee",
    "care_employee",
    "pension_employee",
    "unemployment_employee",
    "pension_health",
    "pension_care",
];

impl Payslip {
    /// The postings of a German payslip of `owner` (spec part 0013): the wages – of a minor employment if `minor` – and
    /// what the employer pays on them as expenses; the taxes, the contributions and the net pay as liabilities. They
    /// balance; amounts without a figure are left out.
    pub fn postings(&self, owner: &PartyId, minor: bool) -> Vec<Posting> {
        let figure = |name: &str| self.figures.get(name).map_or(0, Money::base_units);
        let currency =
            self.figures.get("gross").map_or_else(|| "EUR".parse().expect("a currency"), |m| m.currency().clone());
        let items = |kinds: &[PayItemKind]| -> i128 {
            self.items.iter().filter(|i| kinds.contains(&i.kind)).map(|i| i.amount.base_units()).sum()
        };
        let employer: i128 = EMPLOYER.iter().map(|n| figure(n)).sum::<i128>() + figure("private_subsidy");
        let contributions: i128 =
            EMPLOYER.iter().chain(&EMPLOYEE).map(|n| figure(n)).sum::<i128>() + figure("flat_tax");
        // The flat taxes: 2 % to the Minijob-Zentrale with the contributions, 25 % and its surcharge to the tax office.
        let flat_office = figure("flat_wage_tax") + figure("flat_solidarity");
        let taxes = figure("wage_tax") + figure("solidarity") + figure("church") + flat_office;
        let clearing = items(&[PayItemKind::InKind, PayItemKind::Deduction]);
        let lines = [
            (if minor { account::MINOR_WAGES } else { account::WAGES }, figure("gross")),
            (account::SOCIAL, employer),
            (account::FLAT_TAX, figure("flat_tax") + flat_office),
            (account::TAX_LIABILITY, -taxes),
            (account::SOCIAL_LIABILITY, -contributions),
            (account::NET_LIABILITY, -figure("payout")),
            (account::CLEARING, -clearing),
        ];
        lines
            .into_iter()
            .filter(|(_, units)| *units != 0)
            .map(|(account, units)| Posting {
                owner: owner.clone(),
                account: account.parse::<AccountId>().expect("an account of the chart"),
                amount: Money::new(units, currency.clone()).expect("within range"),
                note: None,
                tax: None,
                exchange: None,
            })
            .collect()
    }

    /// The postings of a correction (spec part 0013): its own postings less those of the payslip it replaces, per
    /// account; accounts whose difference is zero are left out.
    pub fn correction_postings(&self, replaced: &Payslip, owner: &PartyId, minor: bool) -> Vec<Posting> {
        let mut sums: Vec<(AccountId, Money)> = Vec::new();
        let new = self.postings(owner, minor);
        let old = replaced.postings(owner, minor).into_iter().map(|p| Posting { amount: -p.amount, ..p });
        for posting in new.into_iter().chain(old) {
            match sums.iter_mut().find(|(account, _)| *account == posting.account) {
                Some((_, sum)) => {
                    *sum = Money::new(sum.base_units() + posting.amount.base_units(), sum.currency().clone())
                        .expect("within range");
                }
                None => sums.push((posting.account.clone(), posting.amount.clone())),
            }
        }
        sums.into_iter()
            .filter(|(_, sum)| sum.base_units() != 0)
            .map(|(account, amount)| Posting {
                owner: owner.clone(),
                account,
                amount,
                note: None,
                tax: None,
                exchange: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::payroll::figures::tests::{add, employer, employment, employment_id, item};
    use crate::payroll::{PayItemKind, Payslip};
    use crate::{Chart, ChartTranslation, Payload, party::tests::party};

    fn balanced(group: &str, person: &str, items: Vec<crate::payroll::PayItem>) {
        let mut set = employer();
        add(&mut set, Payload::EmploymentDescribed { employment: Box::new(employment(group, person)) });
        let (from, to) = ("2026-05-01".parse().unwrap(), "2026-05-31".parse().unwrap());
        let figures = set.payslip_figures(&employment_id(), from, to, &items).unwrap();
        let payslip = Payslip {
            id: "01a18406-0000-7000-8000-0000000000b1".parse().unwrap(),
            employment: employment_id(),
            from,
            to,
            paid: to,
            replaces: None,
            items,
            figures,
        };
        let postings = payslip.postings(&party(1), person == "109");
        let sum: i128 = postings.iter().map(|p| p.amount.base_units()).sum();
        assert_eq!(sum, 0, "{postings:?}");
        let chart = Chart::v0_1();
        let skr03 = ChartTranslation::de_skr03();
        for p in &postings {
            assert!(chart.accounts.iter().any(|a| a.id == p.account), "{}", p.account);
            assert!(skr03.accounts.values().any(|a| *a == p.account), "{}", p.account);
        }
    }

    #[test]
    fn the_postings_balance() {
        balanced(
            "1111",
            "101",
            vec![
                item(PayItemKind::Wage, 350_000),
                item(PayItemKind::OneOff, 50_000),
                item(PayItemKind::InKind, 5_000),
                item(PayItemKind::TaxFree, 3_000),
                item(PayItemKind::Deduction, 20_000),
            ],
        );
        balanced("1111", "101", vec![item(PayItemKind::Wage, 120_000)]);
        balanced("6100", "109", vec![item(PayItemKind::Wage, 60_000)]);
        balanced("6500", "109", vec![item(PayItemKind::Wage, 60_000)]);
    }
}
