// SPDX-License-Identifier: MIT
//! Names by language (spec part 0001): translation data that names accounts, the stages of currencies, tax codes and
//! the positions of reports.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Object;

/// Names in one language, by key: an account's identity, `<currency code>/<stage>` (`EUR/1`), a tax code
/// (`de-out-19`), or `<report>/<position>` (`de-hgb-266/aktiv.B.II.1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Names {
    /// The language, as a BCP 47 tag (`en`, `fr`, `de`).
    pub language: String,
    /// Key → name.
    pub names: BTreeMap<String, String>,
}

impl Object for Names {
    const TYPE: &'static str = "names";
}

impl Names {
    /// The names of version 0.1 in `language`, if published (`spec/data/names/`): `de`, `en` and `fr`.
    pub fn v0_1(language: &str) -> Option<Self> {
        let json: &[u8] = match language {
            "de" => include_bytes!("../../../spec/data/names/de.json"),
            "en" => include_bytes!("../../../spec/data/names/en.json"),
            "fr" => include_bytes!("../../../spec/data/names/fr.json"),
            _ => return None,
        };
        Some(crate::from_root(json).expect("the published names are valid"))
    }

    /// The name for this key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.names.get(key).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Chart, ChartTranslation};

    #[test]
    fn every_account_of_the_chart_has_a_name_in_german_english_and_french() {
        let chart = Chart::v0_1();
        for language in ["de", "en", "fr"] {
            let names = Names::v0_1(language).unwrap();
            assert_eq!(names.language, language);
            assert!(chart.accounts.iter().all(|a| names.get(a.id.as_str()).is_some_and(|n| !n.is_empty())));
            assert!(names.get("EUR/0").is_some() && names.get("EUR/1").is_some());
        }
        let pcg = ChartTranslation::fr_pcg();
        let fr = Names::v0_1("fr").unwrap();
        assert_eq!(fr.get(pcg.accounts["512"].as_str()), Some("Banques"));
        assert_eq!(Names::v0_1("en").unwrap().get(pcg.accounts["512"].as_str()), Some("Banks"));
        assert_eq!(Names::v0_1("de").unwrap().get(pcg.accounts["512"].as_str()), Some("Bank"));
        assert!(Names::v0_1("es").is_none());
    }

    #[test]
    fn tax_codes_and_german_positions_have_german_names() {
        let de = Names::v0_1("de").unwrap();
        let en = Names::v0_1("en").unwrap();
        for code in crate::TaxTable::de().codes {
            assert!(de.get(code.code.as_str()).is_some() && en.get(code.code.as_str()).is_some(), "{}", code.code);
        }
        for mapping in [
            crate::ReportMapping::de_hgb_266_2026(),
            crate::ReportMapping::de_hgb_275_gkv_2026(),
            crate::ReportMapping::de_ustva_2026(),
            crate::ReportMapping::de_euer_2025(),
        ] {
            let taxes = mapping.taxes.iter().flat_map(|r| [&r.base, &r.output, &r.input]).flatten();
            let settlement = mapping.vat_settlement.iter().flat_map(|s| [&s.debit, &s.credit]).flatten();
            for position in
                mapping.accounts.values().chain(mapping.credit_accounts.values()).chain(taxes).chain(settlement)
            {
                let key = format!("{}/{position}", mapping.report);
                assert!(de.get(&key).is_some(), "{key}");
            }
        }
    }
}
