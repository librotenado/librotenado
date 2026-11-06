// SPDX-License-Identifier: MIT
//! Calculators over the rules of the library (the rulebook on librotenado.dev, spec part 0009): a case as JSON in, the
//! result with its way of computing as JSON out – a payslip, income tax, corporation and trade tax. The cases of the
//! rulebook are computed and tested with it.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::payroll::{
    Employment, GermanEmployment, GermanInsurance, GermanTax, HealthCover, PayItem, PayItemKind, PayPeriod,
    WageTaxFacts, Wages, wage_tax_steps,
};
use crate::{
    Booking, Chart, ChartTranslation, Clock, Configuration, Currency, Date, Event, EventSet, IncomeTaxFacts, Money,
    Names, Payload, Posting, ProfitTaxFacts, TaxSubject,
};

type Failure = String;

fn eur() -> Currency {
    "EUR".parse().expect("a currency")
}

/// An amount in euros from the case (`"4000.00"` or `4000`), in cents.
fn cents(case: &Value, key: &str) -> Result<i128, Failure> {
    let text = match &case[key] {
        Value::Null => return Ok(0),
        Value::String(s) if s.trim().is_empty() => return Ok(0),
        Value::String(s) => s.trim().replace(',', "."),
        other => other.to_string(),
    };
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    if fraction.len() > 2 {
        return Err(format!("{key}: at most two places"));
    }
    let digits = format!("{whole}{fraction:0<2}");
    digits.parse::<i128>().map_err(|_| format!("{key}: not an amount: {text:?}"))
}

fn number<T: std::str::FromStr>(case: &Value, key: &str, default: T) -> Result<T, Failure> {
    match &case[key] {
        Value::Null => Ok(default),
        Value::String(s) if s.trim().is_empty() => Ok(default),
        Value::String(s) => s.trim().parse().map_err(|_| format!("{key}: not a number")),
        other => other.to_string().parse().map_err(|_| format!("{key}: not a number")),
    }
}

fn text(case: &Value, key: &str) -> Option<String> {
    case[key].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn euros(units: i128) -> String {
    let sign = if units < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", units.abs() / 100, units.abs() % 100)
}

/// Fields of a report with their German names, in their order.
fn named(fields: BTreeMap<String, i128>, prefix: &str, order: &[&str]) -> Vec<Value> {
    let names = Names::v0_1("de").expect("published");
    let mut keys: Vec<&String> = fields.keys().collect();
    keys.sort_by_key(|k| order.iter().position(|o| o == k).unwrap_or(usize::MAX));
    keys.into_iter()
        .map(|k| {
            let label = names.get(&format!("{prefix}/{k}")).unwrap_or(k);
            json!({ "name": k, "label": label, "value": euros(fields[k]) })
        })
        .collect()
}

/// Books of one owner with one configuration – a set of events to compute on.
struct Books {
    set: EventSet,
    clock: Clock,
}

impl Books {
    fn new(region: Option<&str>, multiplier: Option<u32>) -> Result<Books, Failure> {
        let mut books = Books { set: EventSet::new(), clock: Clock::new("calculator".parse().expect("a device")) };
        let owner = owner();
        books.add(Payload::OwnerAdded { owner: owner.clone() });
        let region = match region {
            Some(r) => Some(r.parse().map_err(|_| format!("not a region: {r}"))?),
            None => None,
        };
        books.add(Payload::OwnerConfigured {
            configuration: Configuration {
                owner,
                from: Date::new(2025, 1, 1).expect("a date"),
                currency: eur(),
                year_start: 1,
                year_start_day: None,
                chart: None,
                accounting: None,
                vat: None,
                trade_tax_multiplier: multiplier,
                region,
            },
        });
        Ok(books)
    }

    fn add(&mut self, payload: Payload) {
        let device = "calculator".parse().expect("a device");
        let (seq, prev) = self.set.chain_head(&device);
        let time = self.clock.now(seq);
        self.set.insert(Event { time, seq, prev, payload });
    }

    /// Books an amount on an account of SKR 03 against the bank: debit positive.
    fn book(&mut self, year: u16, number: &str, units: i128) {
        if units == 0 {
            return;
        }
        let skr03 = ChartTranslation::de_skr03();
        let posting = |number: &str, units: i128| Posting {
            owner: owner(),
            account: skr03.accounts[number].clone(),
            amount: Money::new(units, eur()).expect("an amount"),
            note: None,
            tax: None,
            exchange: None,
        };
        let booking = Booking {
            basis: None,
            attributed: None,
            date: Date::new(year, 6, 30).expect("a date"),
            text: number.into(),
            postings: vec![posting(number, units), posting("1200", -units)],
            documents: vec![],
            entries: vec![],
            transaction: None,
        };
        self.add(Payload::BookingRecorded { booking });
    }
}

fn owner() -> crate::PartyId {
    "01a18406-0000-7000-8000-00000000c0de".parse().expect("an id")
}

fn payslip(case: &Value) -> Result<Value, Failure> {
    let year: u16 = number(case, "year", 2026)?;
    let month: u8 = number(case, "month", 1)?;
    let class: u8 = number(case, "class", 1)?;
    let church = text(case, "church");
    let region = text(case, "region").unwrap_or_else(|| "DE-NW".into());
    let groups = text(case, "groups").unwrap_or_else(|| "1111".into());
    let person = text(case, "person_group").unwrap_or_else(|| "101".into());
    let additional = text(case, "additional_rate").unwrap_or_else(|| "2.9".into());
    let childless = case["childless"].as_bool().unwrap_or(false);
    let children: u8 = number(case, "care_children", 0)?;
    let saxony = case["saxony"].as_bool().unwrap_or(false);
    let allowances: u32 = number(case, "child_allowances", 0)?;
    let (wage, one_off) = (cents(case, "wage")?, cents(case, "one_off")?);
    let mut books = Books::new(Some(&region), None)?;
    let employment = Employment {
        id: "01a18406-0000-7000-8000-0000000e0001".parse().expect("an id"),
        owner: owner(),
        employee: "01a18406-0000-7000-8000-0000000e0002".parse().expect("an id"),
        from: Date::new(2025, 1, 1).expect("a date"),
        start: Date::new(2025, 1, 1).expect("a date"),
        end: None,
        number: None,
        born: None,
        de: Some(GermanEmployment {
            tax: GermanTax {
                class: Some(class),
                factor: None,
                children: (allowances > 0).then(|| crate::Decimal::new(i128::from(allowances), 1)).flatten(),
                church: church.clone(),
                allowance: None,
                addition: None,
                flat: person == "109",
                short_term: false,
                pension_start: None,
                active_pension: false,
            },
            insurance: GermanInsurance {
                insurer: "00000000".into(),
                contribution_group: groups.clone(),
                person_group: person,
                additional_rate: additional.parse().ok(),
                private: None,
                childless,
                children,
                saxony,
                u1: None,
                u2: None,
                household: false,
            },
        }),
    };
    books.add(Payload::EmploymentDescribed { employment: Box::new(employment.clone()) });
    let from = Date::new(year, month, 1).ok_or("no such month")?;
    let to = (28..=31).rev().find_map(|d| Date::new(year, month, d)).ok_or("no such month")?;
    let mut items = vec![PayItem {
        kind: PayItemKind::Wage,
        name: "Lohn".into(),
        amount: Money::new(wage, eur()).expect("an amount"),
    }];
    if one_off > 0 {
        items.push(PayItem {
            kind: PayItemKind::OneOff,
            name: "Einmalzahlung".into(),
            amount: Money::new(one_off, eur()).expect("an amount"),
        });
    }
    let figures = books.set.payslip_figures(&employment.id, from, to, &items).map_err(|e| e.to_string())?;
    let order = [
        "gross",
        "wage_tax",
        "solidarity",
        "church",
        "health_base",
        "pension_base",
        "health_employee",
        "health_additional_employee",
        "care_employee",
        "pension_employee",
        "unemployment_employee",
        "net",
        "payout",
        "health_employer",
        "health_additional_employer",
        "care_employer",
        "pension_employer",
        "unemployment_employer",
        "u1",
        "u2",
        "u3",
    ];
    let fields = figures.into_iter().map(|(k, m)| (k, m.base_units())).collect();
    // The way of the wage tax: the PAP of the year with the same facts.
    let digit = |i: usize| groups.as_bytes().get(i).copied().unwrap_or(b'0');
    let facts = WageTaxFacts {
        class,
        children: allowances,
        church: church.is_some(),
        health: HealthCover::Statutory {
            additional_rate: additional
                .parse::<crate::Decimal>()
                .map(|r| (r.units() * 100 / 10i128.pow(u32::from(r.scale()))) as u32)
                .unwrap_or(0),
        },
        pension: digit(1) != b'0',
        unemployment: digit(2) != b'0',
        childless,
        care_reductions: children.saturating_sub(1).min(4),
        saxony,
        ..WageTaxFacts::default()
    };
    let wages = Wages { current: wage, one_off, expected_year: wage * 12, ..Wages::default() };
    let steps = wage_tax_steps(year, PayPeriod::Month, &wages, &facts).map_err(|e| e.to_string())?;
    Ok(json!({
        "figures": named(fields, "payroll", &order),
        "steps": steps.iter().map(|s| json!({ "name": s.name, "label": s.description, "value": s.value })).collect::<Vec<_>>(),
        "steps_title": format!("Programmablaufplan {year} des BMF"),
    }))
}

fn subject(case: &Value) -> Result<TaxSubject, Failure> {
    Ok(match text(case, "subject").as_deref() {
        None | Some("individual") => TaxSubject::Individual,
        Some("corporation") => TaxSubject::Corporation,
        Some("partnership") => TaxSubject::Partnership,
        Some("association") => TaxSubject::Association,
        Some(other) => return Err(format!("not a subject: {other}")),
    })
}

/// Books with the figures of a business: its result and the expenses the rules treat apart.
fn business(case: &Value, year: u16, multiplier: Option<u32>) -> Result<Books, Failure> {
    let mut books = Books::new(None, multiplier)?;
    let (excluded, interest, rent) =
        (cents(case, "excluded")?, cents(case, "interest")?, cents(case, "rent_immovable")?);
    let result = cents(case, "profit")?;
    // Sales of the result with the expenses booked, and the expenses: 4654 not deductible, 2100 interest, 4210 rent.
    books.book(year, "8400", -(result + excluded + interest + rent));
    books.book(year, "4654", excluded);
    books.book(year, "2100", interest);
    books.book(year, "4210", rent);
    Ok(books)
}

fn profit_facts(case: &Value, subject: TaxSubject, multiplier: Option<u32>) -> Result<ProfitTaxFacts, Failure> {
    Ok(ProfitTaxFacts {
        subject,
        multiplier,
        income_losses: cents(case, "income_losses")?,
        trade_losses: cents(case, "trade_losses")?,
        land_reduction: 0,
        turnover_and_wages: None,
        holding: crate::Holding::AtLeast15,
        cash_basis: false,
        hidden_distributions: cents(case, "hidden_distributions")?,
        hidden_contributions: 0,
        group: None,
        interest_carryforward: 0,
        ebitda_carryforward: 0,
        interest_exception: false,
    })
}

fn period(year: u16) -> (Date, Date) {
    (Date::new(year, 1, 1).expect("a date"), Date::new(year, 12, 31).expect("a date"))
}

fn profit_tax(case: &Value) -> Result<Value, Failure> {
    let year: u16 = number(case, "year", 2026)?;
    let multiplier: Option<u32> = Some(number(case, "multiplier", 400)?).filter(|m| *m > 0);
    let subject = subject(case)?;
    let books = business(case, year, multiplier)?;
    let (from, to) = period(year);
    let tax = books
        .set
        .profit_tax(&Chart::v0_1(), &owner(), from, to, &profit_facts(case, subject, multiplier)?)
        .map_err(|e| e.to_string())?;
    let kst = [
        "result",
        "excluded",
        "participation",
        "partner_remuneration",
        "net_interest",
        "interest_not_deductible",
        "hidden_distributions",
        "hidden_contributions",
        "donations",
        "donations_deducted",
        "group_income",
        "income",
        "loss_deduction",
        "taxable",
        "corporation_tax",
        "solidarity",
        "interest_carried",
        "ebitda_carried",
    ];
    let gewst = [
        "profit",
        "financing_a",
        "financing_b",
        "financing_c",
        "financing_d",
        "financing_e",
        "financing_f",
        "financing_addition",
        "dividends",
        "partnership_losses",
        "donations_added",
        "land",
        "partnership_profits",
        "donations",
        "trade_income",
        "loss_deduction",
        "rounded",
        "allowance",
        "base",
        "tax",
    ];
    let trade = tax.trade.as_ref().map(|t| named(t.fields(), "de-gewst", &gewst)).unwrap_or_default();
    Ok(json!({ "figures": named(tax.fields(), "de-kst", &kst), "trade": trade }))
}

fn income_tax(case: &Value) -> Result<Value, Failure> {
    let year: u16 = number(case, "year", 2026)?;
    let multiplier: Option<u32> = Some(number(case, "multiplier", 0)?).filter(|m| *m > 0);
    let books = business(case, year, multiplier)?;
    let (from, to) = period(year);
    let facts = IncomeTaxFacts {
        joint: case["joint"].as_bool().unwrap_or(false),
        other_income: cents(case, "other_income")?,
        deductions: cents(case, "deductions")?,
        losses: cents(case, "losses")?,
        church_rate: Some(number(case, "church_rate", 0u8)?).filter(|r| *r > 0),
        child_allowances: number(case, "child_allowances", 0)?,
        child_benefit: cents(case, "child_benefit")?,
        extraordinary: cents(case, "extraordinary")?,
        partnership_income: 0,
        partnership_trade: (0, 0),
        capital_income: cents(case, "capital_income")?,
        single_parent_children: number(case, "single_parent_children", 0)?,
    };
    let profit = profit_facts(case, TaxSubject::Individual, multiplier)?;
    let tax = books
        .set
        .income_tax(&Chart::v0_1(), &owner(), from, to, &profit, &facts)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no tariff for {year}"))?;
    let order = [
        "business",
        "other_income",
        "loss_deduction",
        "deductions",
        "single_parent_relief",
        "child_allowances",
        "taxable",
        "tariff",
        "trade_tax_credit",
        "child_benefit_added",
        "capital_income",
        "capital_tax",
        "assessed",
        "solidarity",
        "church",
        "advance_payment",
    ];
    Ok(json!({ "figures": named(tax.fields(), "de-est", &order) }))
}

/// Computes a case: `calculation` is `payslip`, `income_tax` or `profit_tax`, the other members its facts (see the
/// rulebook). The result has `ok` with the figures – and for a payslip the steps of the PAP –, or `error`.
pub fn calculate(case: &Value) -> Value {
    let result = match case["calculation"].as_str() {
        Some("payslip") => payslip(case),
        Some("income_tax") => income_tax(case),
        Some("profit_tax") => profit_tax(case),
        _ => Err("calculation: payslip, income_tax or profit_tax".into()),
    };
    match result {
        Ok(value) => json!({ "ok": value, "version": env!("CARGO_PKG_VERSION") }),
        Err(error) => json!({ "error": error }),
    }
}
