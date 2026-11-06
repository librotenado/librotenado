// SPDX-License-Identifier: MIT
//! The review cases of the rulebook (`spec/data/cases/`): each case computed by the calculators gives the figures it
//! expects. A tax adviser who confirms or corrects a case changes the expectation; the library must follow.

use serde_json::Value;

#[test]
fn the_review_cases_of_germany() {
    let data: Value = serde_json::from_str(include_str!("../data/cases/de.json")).unwrap();
    for case in data["cases"].as_array().unwrap() {
        let result = librotenado::calculator::calculate(&case["case"]);
        let ok = &result["ok"];
        assert!(!ok.is_null(), "{}: {result}", case["id"]);
        let mut figures = ok["figures"].as_array().unwrap().clone();
        figures.extend(ok["trade"].as_array().cloned().unwrap_or_default());
        for (name, expected) in case["expected"].as_object().unwrap() {
            let got = figures.iter().find(|f| f["name"] == *name).map_or("0.00", |f| f["value"].as_str().unwrap());
            assert_eq!(got, expected.as_str().unwrap(), "{} {name}", case["id"]);
        }
    }
}
