include!("cardinality.rs.inc");
use safe_cex_num::{JsCoercionError, js_number, numeric_ingress::js_from_str};
use safe_cex_venue_bitrue::{account_helpers::*, wire::OpaqueValue};
use serde_json::{Value, json};
use std::process::Command;
fn native(value: &Value) -> Value {
    match value {
        Value::Null => json!({"type":"null"}),
        Value::Bool(v) => json!({"type":"boolean","value":v}),
        Value::String(v) => json!({"type":"string","value":v}),
        Value::Number(_) => {
            let n = js_number(Some(value)).unwrap();
            json!({"type":"number","bits":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
        }
        Value::Array(values) => {
            json!({"type":"array","items":values.iter().map(native).collect::<Vec<_>>()})
        }
        Value::Object(values) => {
            json!({"type":"object","entries":values.iter().map(|(k,v)|json!([k,native(v)])).collect::<Vec<_>>()})
        }
    }
}
fn error(error: JsCoercionError) -> Value {
    json!({"type":"error","name":error.name})
}
#[test]
fn accounts_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual account oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&capture, "accounts");
    println!(
        "capture {}; {} account inputs, two actual methods each",
        capture["header"],
        capture["rows"].as_array().unwrap().len()
    );
    assert!(capture["header"]["node"].as_str().unwrap().starts_with('v'));
    assert!(!capture["header"]["arch"].as_str().unwrap().is_empty());
    assert!(
        js_from_str::<OpaqueValue>(capture["surrogateRaw"].as_str().unwrap()).is_err(),
        "chosen account/snapshot UTF-8 refusal"
    );
    assert!(
        js_from_str::<OpaqueValue>(capture["depthRaw"].as_str().unwrap()).is_err(),
        "chosen account/snapshot bounded-depth refusal"
    );
    for row in capture["rows"].as_array().unwrap() {
        let data = row["raw"]
            .as_str()
            .map(|raw| js_from_str::<OpaqueValue>(raw).unwrap().value);
        let actual = match get_position_accounts(data.as_ref()) {
            Ok(None) => json!({"type":"undefined"}),
            Err(e) => error(e),
            Ok(Some(accounts)) => {
                let original = data.as_ref().unwrap()["data"]["account"]
                    .as_array()
                    .unwrap();
                let indices: Vec<_> = accounts
                    .into_iter()
                    .map(|account| {
                        original
                            .iter()
                            .position(|entry| std::ptr::eq(entry, account))
                            .expect("native account retains original identity")
                    })
                    .collect();
                json!({"type":"accounts","indices":indices})
            }
        };
        native_cardinality(&row["accounts"], &actual, "accounts");
        assert_eq!(
            actual, row["accounts"],
            "account native parity: getPositionAccounts; {row}"
        );
        let actual = match get_account_position_snapshots(data.as_ref()) {
            Ok(None) => json!({"type":"undefined"}),
            Err(e) => error(e),
            Ok(Some(snapshots)) => {
                json!({"type":"snapshots","values":snapshots.iter().map(|p|json!({"symbol":native(&Value::String(p.symbol.into())),"side":native(&Value::String(match p.side {safe_cex_types::PositionSide::Long=>"long",safe_cex_types::PositionSide::Short=>"short"}.into())),"leverageLevel":native(p.leverage_level),"positionType":native(p.position_type)})).collect::<Vec<_>>()})
            }
        };
        native_cardinality(&row["snapshots"], &actual, "snapshots");
        assert_eq!(
            actual, row["snapshots"],
            "account native parity: getAccountPositionSnapshots; {row}"
        );
    }
    native_comparisons(304, "accounts");
}
