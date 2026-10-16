#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{NativeSizeInputs, NativeSizeMarket, native_position_size, oldest_cursor_id};
use serde_json::Value;
fn number(v: &Value) -> Option<f64> {
    v.as_str().map(|s| match s {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => s.parse().unwrap(),
    })
}
#[test]
#[ignore = "requires Node; run with --include-ignored for the differential gate"]
fn selection_typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/selection-oracle.cjs"
        ))
        .output()
        .expect("Node required for TypeScript differential tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(cases, 20022, "selection", |c| {
        if c["op"] == "cursor" {
            assert_eq!(
                oldest_cursor_id(
                    &c["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| id.as_str().unwrap())
                        .collect::<Vec<_>>()
                )
                .as_deref(),
                c["expected"].as_str(),
                "{c}"
            );
        } else {
            let p = NativeSizeInputs {
                contracts: number(&c["contracts"]).unwrap_or(f64::NAN),
                notional: number(&c["notional"]),
                contracts_native: number(&c["native"]),
            };
            let m = NativeSizeMarket {
                contract_size: number(&c["size"]),
                is_inverse: c["inverse"].as_bool(),
            };
            let actual = native_position_size(&p, &m);
            let expected = number(&c["expected"]).unwrap();
            assert!(
                if expected.is_nan() {
                    actual.is_nan()
                } else {
                    actual.to_bits() == expected.to_bits()
                },
                "{c}: actual {actual:?}"
            );
        }
    });
}
