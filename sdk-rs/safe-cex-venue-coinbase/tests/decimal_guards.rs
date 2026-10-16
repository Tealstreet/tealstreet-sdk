use safe_cex_venue_coinbase::decimal::{is_coinbase_unsigned_decimal, is_coinbase_zero_decimal};
use serde_json::Value;
use std::{process::Command, str};

const INPUTS: &[&str] = &[
    "undefined",
    "null",
    "false",
    "0",
    "1",
    "[]",
    "{}",
    "\"\"",
    "\"0\"",
    "\"00\"",
    "\"1\"",
    "\"01\"",
    "\"9\"",
    "\"10\"",
    "\"0.0\"",
    "\"0.000\"",
    "\"00.0\"",
    "\"1.0\"",
    "\"1.\"",
    "\"1.00\"",
    "\".1\"",
    "\"-0\"",
    "\"+0\"",
    "\"NaN\"",
    "\"Infinity\"",
    "\"1e3\"",
    "\"１２\"",
    "\"١\"",
    "\"1\\n\"",
    "\" 0\"",
    "\"\\ufeff0\"",
    "\"0x10\"",
    "\"0001\"",
    "\"000.\"",
    "\"0.00.0\"",
];

#[test]
fn coinbase_decimal_guards_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/decimal-guards.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected["unsignedPattern"],
        "/^(?:0|[1-9]\\d*)(?:\\.\\d+)?$/"
    );
    assert_eq!(expected["zeroPattern"], "/^0(?:\\.0+)?$/");
    let results = expected["results"].as_array().unwrap();
    assert_eq!(results.len(), INPUTS.len());
    for (index, input) in INPUTS.iter().enumerate() {
        let value = if *input == "undefined" {
            None
        } else {
            Some(serde_json::from_str::<Value>(input).unwrap())
        };
        assert_eq!(
            [
                is_coinbase_unsigned_decimal(value.as_ref()),
                is_coinbase_zero_decimal(value.as_ref()),
            ],
            [
                results[index][0].as_bool().unwrap(),
                results[index][1].as_bool().unwrap()
            ],
            "decimal guard result at input {input}"
        );
    }
}
