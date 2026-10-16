include!("cardinality.rs.inc");
use safe_cex_venue_bitrue::utils::reverse_ws_symbol;
use serde_json::Value;
use std::{collections::BTreeMap, process::Command};

#[test]
fn reverse_ws_symbol_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/utils-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "utility oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Preserve the escaped lone surrogate separately: Value intentionally refuses it.
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("\\ud800"), "source retains utility surrogate");
    assert!(
        serde_json::from_str::<Value>(&text).is_err(),
        "chosen utility UTF-8 refusal"
    );
    let capture: Value = serde_json::from_str(&text.replace("\\ud800", "SURROGATE")).unwrap();
    source_cardinality(&capture, "utils");
    println!("capture {}", capture["header"]);
    assert!(capture["header"]["node"].as_str().unwrap().starts_with('v'));
    assert!(!capture["header"]["arch"].as_str().unwrap().is_empty());
    for row in capture["rows"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        let native = reverse_ws_symbol(input);
        native_cardinality(&row["output"], &Value::String(native.clone()), "utils");
        assert_eq!(
            native,
            row["output"].as_str().unwrap(),
            "reverseWsSymbol parity: {input:?}"
        );
    }
    let changes: BTreeMap<u32, String> =
        serde_json::from_value::<Vec<(u32, String)>>(capture["uppercase"].clone())
            .unwrap()
            .into_iter()
            .collect();
    let mut checked = 0;
    for code in 0..=0x10ffff {
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        let input = c.to_string();
        let expected = changes.get(&code).map(String::as_str).unwrap_or(&input);
        assert_eq!(
            input.to_uppercase(),
            expected,
            "Unicode uppercase parity U+{code:04X}"
        );
        checked += 1;
    }
    assert_eq!(
        checked, 1_112_064,
        "BITRUE-NATIVE-CARDINALITY Unicode comparisons"
    );
    println!(
        "{} utility cases; {checked} Unicode scalars",
        capture["rows"].as_array().unwrap().len()
    );
    native_comparisons(23, "utils");
}
