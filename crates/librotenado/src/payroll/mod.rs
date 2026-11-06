// SPDX-License-Identifier: MIT
//! Payroll (spec part 0013): employments, payslips and what an employer withholds and pays on wages – in Germany the
//! wage tax by the BMF's Programmablaufplan and the social insurance.

mod booking;
mod dec;
mod employment;
mod figures;
mod pap2025;
mod pap2026;
mod register;
mod reports;
mod social;
mod wage_tax;

pub use employment::{
    Employment, EmploymentId, GermanEmployment, GermanInsurance, GermanTax, PayItem, PayItemKind, Payslip, PayslipId,
    PensionStart, PrivateInsurance, YearAndMonth,
};
pub use figures::PayrollError;
pub use register::PayrollProblem;
pub use wage_tax::{HealthCover, MAX_AMOUNT, PayPeriod, WageTax, WageTaxError, WageTaxFacts, Wages, wage_tax};
