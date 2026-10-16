#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{JsPrimitive, js_bigint, js_bigint_primitive};
use serde_json::{Value, json};
#[test]
fn bigint_matches_live_node_error_classes() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js_bigint.cjs"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(rows, 10156, "bigint", |row| {
        let actual = if let Some(p) = row["primitive"].as_str() {
            js_bigint_primitive(&match p {
                "Symbol()" => JsPrimitive::Symbol,
                "10n" => JsPrimitive::BigInt(10.into()),
                _ => JsPrimitive::Number(p.parse().unwrap()),
            })
        } else {
            let value = row["raw"]
                .as_str()
                .map(|r| serde_json::from_str::<Value>(r).unwrap());
            js_bigint(value.as_ref())
        };
        let actual = match actual {
            Ok(v) => json!({"ok":v.to_string()}),
            Err(e) => {
                json!({"error":{"name":e.kind.name(),"message":e.message,"units":e.message_utf16}})
            }
        };
        assert_eq!(actual, row["expected"], "{row}");
    });
}
