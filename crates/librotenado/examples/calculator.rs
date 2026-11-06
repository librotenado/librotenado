// SPDX-License-Identifier: MIT
//! The calculators of the rulebook as a program: reads a case as JSON from standard input and writes the result with
//! its way of computing as JSON (`librotenado::calculator`). Built for WebAssembly (`wasm32-wasip1`) it runs in the
//! browser on librotenado.dev, with the same code as everywhere else.
//!   echo '{"calculation": "income_tax", "profit": "60000"}' | cargo run --example calculator
use std::io::Read;

fn main() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let out = match serde_json::from_str::<serde_json::Value>(&input) {
        Ok(case) => librotenado::calculator::calculate(&case),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    };
    println!("{out}");
}
