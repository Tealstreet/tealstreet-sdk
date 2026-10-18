include!("cardinality.rs.inc");
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Balance;
use safe_cex_venue_bitrue::{balance::parse_balance, wire::OpaqueValue};
use serde_json::{Value, json};
use std::process::Command;
fn number(n: f64) -> Value {
    json!({"type":"number","bits":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn native(value: &(impl serde::Serialize + ?Sized)) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let value = safe_cex_num::numeric_ingress::js_capture_output(value).unwrap();
    match &value {
        Input::Undefined => json!({"type":"undefined"}),
        Input::Null => json!({"type":"null"}),
        Input::Bool(v) => json!({"type":"boolean","value":v}),
        Input::Text(v) => json!({"type":"string","value":String::from_utf16(v).unwrap()}),
        Input::Float(_) | Input::Lexeme(_) => number(value.as_f64().unwrap()),
        Input::Array(v) => json!({"type":"array","items":v.iter().map(native).collect::<Vec<_>>()}),
        Input::Object(v) => {
            json!({"type":"object","entries":v.iter().map(|(k,v)|json!([String::from_utf16(k).unwrap(),native(v)])).collect::<Vec<_>>()})
        }
    }
}
fn fields(b: &Balance) -> Value {
    json!({"total":number(b.total),"free":number(b.free),"used":number(b.used),"upnl":number(b.upnl)})
}
#[test]
fn balance_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/balance-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual balance oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality(&capture, "balance");
    println!(
        "capture {}; {} balance cases, {} typed-output refusals",
        capture["header"],
        capture["rows"].as_array().unwrap().len(),
        capture["refusals"].as_array().unwrap().len()
    );
    assert!(capture["header"]["node"].as_str().unwrap().starts_with('v'));
    assert!(!capture["header"]["arch"].as_str().unwrap().is_empty());
    let store = DefaultStore::default();
    let initial = js_from_str::<Balance>(&capture["stored"].to_string()).unwrap();
    Store::update(
        &store,
        StoreUpdate {
            balance: Some(initial),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let stored = store.balance();
    for row in capture["rows"].as_array().unwrap() {
        let input = row["raw"]
            .as_str()
            .map(|s| js_from_str::<OpaqueValue>(s).unwrap().value);
        let mut warnings = Vec::new();
        let actual = match parse_balance(input.as_ref(), &stored, |warning| {
            warnings.push(warning.to_owned())
        }) {
            Err(e) => json!({"type":"error","name":e.name,"message":e.message}),
            Ok(b) => {
                json!({"type":"balance","stored":std::ptr::eq(b.as_ref(),&*stored),"fields":fields(&b),"currencies":b.currencies.as_ref().map(|c|c.iter().map(|(k,v)|json!([k,fields(v.as_balance().unwrap())])).collect::<Vec<_>>()),"info":b.info.as_ref().map(native)})
            }
        };
        native_cardinality(&row["result"], &actual, "balance");
        assert_eq!(
            actual, row["result"],
            "balance native parity: parseBalance; {row}"
        );
    }
    for row in capture["refusals"].as_array().unwrap() {
        let input = js_from_str::<OpaqueValue>(row["raw"].as_str().unwrap()).unwrap();
        assert_eq!(row["result"]["type"], "balance");
        let mut warnings = Vec::new();
        let balance = parse_balance(Some(&input.value), &stored, |warning| {
            warnings.push(warning.to_owned())
        })
        .unwrap();
        assert_eq!(
            (balance.total, balance.free, balance.used, balance.upnl),
            (0., 0., 0., 0.)
        );
        assert!(balance.currencies.as_ref().unwrap().is_empty());
        assert_eq!(balance.info.as_ref(), Some(&input.value.into()));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Balance.upnl"));
    }
    for key in ["utf8Raw", "depthRaw"] {
        assert!(
            js_from_str::<OpaqueValue>(capture[key].as_str().unwrap()).is_err(),
            "balance chosen ingress refusal: {key}"
        );
    }
    native_comparisons(149, "balance");
}
