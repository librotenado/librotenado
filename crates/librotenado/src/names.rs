// SPDX-License-Identifier: MIT
//! Names by language (spec part 0003): translation data that names accounts and the stages of currencies.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Object;

/// Names in one language, by key: an account's identity, or `<currency code>/<stage>` (`EUR/1`).
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
    /// The names of version 0.1 in `language`, if published (`spec/data/names/`): `en` and `fr`.
    pub fn v0_1(language: &str) -> Option<Self> {
        let json: &[u8] = match language {
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
    fn every_account_of_the_chart_has_a_name_in_english_and_french() {
        let chart = Chart::v0_1();
        for language in ["en", "fr"] {
            let names = Names::v0_1(language).unwrap();
            assert_eq!(names.language, language);
            assert!(chart.accounts.iter().all(|a| names.get(a.id.as_str()).is_some_and(|n| !n.is_empty())));
            assert_eq!(names.names.len(), chart.accounts.len());
        }
        let pcg = ChartTranslation::fr_pcg();
        let fr = Names::v0_1("fr").unwrap();
        assert_eq!(fr.get(pcg.accounts["512"].as_str()), Some("Banques"));
        assert_eq!(Names::v0_1("en").unwrap().get(pcg.accounts["512"].as_str()), Some("Banks"));
        assert!(Names::v0_1("de").is_none());
    }
}
