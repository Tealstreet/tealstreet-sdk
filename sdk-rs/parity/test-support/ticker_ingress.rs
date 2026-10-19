use safe_cex_exchange::js_runtime::value::Js;
use serde_json::{Value, json};

pub fn source(venue: &str, profile: &str) -> Value {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            "sdk-rs/parity/scripts/ticker-ingress-source.cjs",
            venue,
            profile,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

pub fn observe(store: std::rc::Rc<dyn safe_cex_store::Store>) -> Value {
    let facade = safe_cex_exchange::native_store::facade(store).unwrap();
    let tickers = facade.get("tickers").unwrap();
    assert!(matches!(tickers.get("length").unwrap(), Js::Number(1.0)));
    let row = tickers.get("0").unwrap();
    let observed = observe_row(&row);
    let snapshot = facade.call_method("serialize", vec![]).unwrap();
    let payload = safe_cex_exchange::store_payload::StorePayloadPreparer::default()
        .prepare(&snapshot, 0.0, 0.0, false, Some(true))
        .unwrap();
    let output = safe_cex_exchange::js_runtime::value::owned::OwnedJsValue::from_js(&payload)
        .unwrap()
        .to_js();
    let delivered = output.get("tickers").unwrap().get("0").unwrap();
    assert_eq!(
        observed,
        observe_row(&delivered),
        "STORE_TICKER_PAYLOAD_ORDER"
    );
    observed
}

fn observe_row(row: &Js) -> Value {
    let keys: Vec<String> = row
        .own_keys()
        .iter()
        .map(|key| String::from_utf16(key).unwrap())
        .collect();
    let string = |field| {
        let Js::String(units) = row.get(field).unwrap() else {
            panic!("ticker {field} must be a native string")
        };
        String::from_utf16(&units).unwrap()
    };
    let number = |field| {
        let Js::Number(value) = row.get(field).unwrap() else {
            panic!("ticker {field} must be a native number")
        };
        value
    };
    json!({"keys": keys, "id": string("id"), "symbol": string("symbol"),
        "lastBits": format!("{:016x}", number("last").to_bits()),
        "volumeBits": format!("{:016x}", number("volume").to_bits())})
}
