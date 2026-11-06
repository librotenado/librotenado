// SPDX-License-Identifier: MIT
//! Writes the pages of the rulebook on librotenado.dev from the library itself – its chart, translations, attributes,
//! report mappings and tax codes –, so that what a reviewer reads is what the library computes with.
//!   cargo run --example rulebook -- <directory>
use std::{collections::BTreeMap, fmt::Write as _, fs, path::Path};

use librotenado::{AccountId, Chart, ChartTranslation, EventSet, Names, ReportMapping, TaxDirection, TaxTable};

/// The reports whose positions the account pages show, with a short column title.
fn reports() -> Vec<(&'static str, ReportMapping)> {
    vec![
        ("UStVA 2026", ReportMapping::de_ustva_2026()),
        ("EÜR 2026", ReportMapping::de_euer_2026()),
        ("HGB § 266", ReportMapping::de_hgb_266_2026()),
        ("HGB § 275", ReportMapping::de_hgb_275_gkv_2026()),
        ("E-Bilanz 6.10 (Bilanz)", ReportMapping::de_ebilanz_bs_6_10()),
        ("E-Bilanz 6.10 (GuV)", ReportMapping::de_ebilanz_is_gkv_6_10()),
    ]
}

/// Text for a Markdown table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn front(title: &str, parent: Option<&str>, order: usize, permalink: &str) -> String {
    let mut out = format!("---\ntitle: \"{title}\"\nnav_order: {order}\npermalink: {permalink}\n");
    match parent {
        Some(parent) => writeln!(out, "parent: \"{parent}\"").unwrap(),
        None => out.push_str("has_children: true\n"),
    }
    out.push_str("---\n\n");
    out
}

/// The position of an account in a report: debit and credit where they differ.
fn position(mapping: &ReportMapping, account: &AccountId, parents: &BTreeMap<AccountId, AccountId>) -> String {
    let debit = mapping.position(account, parents, false);
    let credit = mapping.position(account, parents, true);
    match (debit, credit) {
        (Some(d), Some(c)) if d != c => format!("{d} / Haben: {c}"),
        (Some(d), _) => d.to_owned(),
        (None, Some(c)) => format!("Haben: {c}"),
        (None, None) => String::new(),
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: rulebook <directory>");
    let dir = Path::new(&dir).join("rules");
    fs::create_dir_all(&dir).unwrap();
    let chart = Chart::v0_1();
    let parents = EventSet::new().account_parents(&chart);
    let de = Names::v0_1("de").unwrap();
    let version = env!("CARGO_PKG_VERSION");

    let mut index = front("Rulebook", None, 3, "/rules/");
    write!(
        index,
        "The rules of librotenado {version} as the reference library computes with them, written from the library \
         itself when this site is built: every account, its attributes, the position it takes in each report, and \
         every tax code. Nothing here is maintained by hand, so nothing here can differ from what the library does.\n\n\
         All of it is a **draft** until it has been reviewed by tax advisers. Whoever finds an assignment that does \
         not hold is asked to report it with the account number, the report and the reason \
         ([issues](https://github.com/librotenado/librotenado/issues)).\n\n\
         The attributes of accounts are those of the specification: `profit` and `trade_tax` for the taxes on profit \
         ([0012](../spec/0012-taxes-on-profit.html)), `tax` the code of an automatic account, `term` the remaining \
         term, `interest` the interest income of the interest barrier ([0004](../spec/0004-chart-of-accounts.html)).\n\n\
         - [Tax codes of Germany](de-tax-codes.html)\n"
    )
    .unwrap();

    // The accounts of each national chart, by class.
    let mut order = 2;
    for (code, title, translation) in
        [("de-skr03", "SKR 03", ChartTranslation::de_skr03()), ("de-skr04", "SKR 04", ChartTranslation::de_skr04())]
    {
        writeln!(index, "- {title}, accounts by class:").unwrap();
        let mut classes: BTreeMap<char, Vec<(&String, &AccountId)>> = BTreeMap::new();
        for (number, id) in &translation.accounts {
            classes.entry(number.chars().next().unwrap_or('0')).or_default().push((number, id));
        }
        for (class, accounts) in classes {
            let name = format!("{code}-{class}");
            let page_title = format!("{title}, Klasse {class}");
            write!(index, " [{class}]({name}.html)").unwrap();
            let mut page = front(&page_title, Some("Rulebook"), order, &format!("/rules/{name}.html"));
            order += 1;
            write!(
                page,
                "The accounts of {title} of class {class} (DATEV, valid for 2026) and what librotenado makes of \
                 them: the harmonised account they translate into, its attributes – own or inherited – and its \
                 position in each report. Where a credit balance takes another position, it follows after \
                 \"Haben\". Sales and purchases reach the VAT returns, the EÜR and the split of sales in the \
                 E-Bilanz by their tax code, not by their account: see the [tax codes](de-tax-codes.html).\n\n"
            )
            .unwrap();
            let reports = reports();
            write!(page, "| Konto | Bezeichnung | Merkmale |").unwrap();
            for (column, _) in &reports {
                write!(page, " {column} |").unwrap();
            }
            write!(page, "\n|---|---|---|").unwrap();
            for _ in &reports {
                page.push_str("---|");
            }
            page.push('\n');
            for (number, id) in accounts {
                let attributes = chart
                    .attributes(id)
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(page, "| {number} | {} | {} |", cell(de.get(id.as_str()).unwrap_or("")), cell(&attributes))
                    .unwrap();
                for (_, mapping) in &reports {
                    write!(page, " {} |", cell(&position(mapping, id, &parents))).unwrap();
                }
                page.push('\n');
            }
            fs::write(dir.join(format!("{name}.md")), page).unwrap();
        }
        index.push('\n');
    }

    // The tax codes of Germany and where the advance VAT return counts them.
    let table = TaxTable::de();
    let ustva = ReportMapping::de_ustva_2026();
    let euer = ReportMapping::de_euer_2026();
    let mut page = front("Tax codes of Germany", Some("Rulebook"), 1, "/rules/de-tax-codes.html");
    write!(
        page,
        "The tax codes of Germany, valid from {}, with their rate and provision, and the fields of the advance VAT \
         return 2026 and of the EÜR 2026 a posting with the code fills: the net amount (base), the VAT owed (output), the \
         input VAT (input).\n\n| Code | Bezeichnung | Richtung | Satz | Vorschrift | UStVA: Bemessung | UStVA: USt | \
         UStVA: Vorsteuer | EÜR: Betrag | EÜR: USt/VSt |\n|---|---|---|---|---|---|---|---|---|---|\n",
        table.valid_from
    )
    .unwrap();
    for code in &table.codes {
        let rule = ustva.taxes.iter().find(|r| r.tax == code.code);
        let field = |f: fn(&librotenado::TaxRule) -> &Option<String>| rule.and_then(|r| f(r).clone()).unwrap_or_default();
        let euer_rule = euer.taxes.iter().find(|r| r.tax == code.code);
        let euer_base = euer_rule.and_then(|r| r.base.clone()).unwrap_or_default();
        let euer_tax = euer_rule.and_then(|r| r.output.clone().or_else(|| r.input.clone())).unwrap_or_default();
        let direction = match code.direction {
            TaxDirection::Output => "Umsatz",
            TaxDirection::Input => "Eingang",
        };
        writeln!(
            page,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            code.code,
            cell(de.get(code.code.as_str()).unwrap_or("")),
            direction,
            code.rate.map(|r| format!("{r} %")).unwrap_or_default(),
            cell(&code.law),
            field(|r| &r.base),
            field(|r| &r.output),
            field(|r| &r.input),
            euer_base,
            euer_tax,
        )
        .unwrap();
    }
    fs::write(dir.join("de-tax-codes.md"), page).unwrap();
    fs::write(dir.join("index.md"), index).unwrap();
}
