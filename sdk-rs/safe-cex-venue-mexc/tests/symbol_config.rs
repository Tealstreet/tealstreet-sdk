mod support;
use safe_cex_types::{PlaceOrderOpts, Position, PositionMode, PositionSide};
use safe_cex_venue_mexc::{
    symbol_config::{MexcSymbolConfig, derive_symbol_config, get_leverage_for_order},
    utils::{fake_positions_for_market, normalize_market},
};
use serde_json::{Value, json};
use support::{decode_number, encode};

fn decode_json(v: &Value) -> Value {
    if v.get("$number").is_some() {
        return json!(decode_number(v));
    }
    match v {
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), decode_json(v))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(decode_json).collect()),
        _ => v.clone(),
    }
}
fn config(v: &Value) -> Option<MexcSymbolConfig> {
    if v.get("$undefined").is_some() {
        return None;
    }
    Some(MexcSymbolConfig {
        margin_type: serde_json::from_value(v["marginType"].clone()).unwrap(),
        buy_leverage: decode_number(&v["buyLeverage"]),
        sell_leverage: decode_number(&v["sellLeverage"]),
        position_mode: serde_json::from_value(v["positionMode"].clone()).unwrap(),
    })
}
fn positions(rows: &Value) -> Vec<Position> {
    let market = normalize_market(
        json!({"symbol":"BTC_USDT","contractSize":1})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    let base = fake_positions_for_market(&market, None, None, None).remove(0);
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut p = base.clone();
            p.id = r["id"].as_str().unwrap().into();
            p.symbol = r["symbol"].as_str().unwrap().into();
            p.side = if r["side"] == "long" {
                safe_cex_types::ModelField::Value(PositionSide::Long)
            } else {
                safe_cex_types::ModelField::Value(PositionSide::Short)
            };
            p.contracts = decode_number(&r["contracts"]);
            p.leverage = decode_number(&r["leverage"]);
            p.position_mode = (r
                .get("positionMode")
                .map(|v| serde_json::from_value(v.clone()).unwrap()))
            .into();
            p.margin_type = (r
                .get("marginType")
                .map(|v| serde_json::from_value(v.clone()).unwrap()))
            .into();
            p.info = (r.get("info").map(decode_json)).into();
            p
        })
        .collect()
}

#[test]
fn mexc_symbol_config_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/symbol_config_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 180);
    let mut getters = 0;
    for (index, case) in cases.iter().enumerate() {
        let positions = positions(&case["rows"]);
        let existing = config(&case["config"]);
        let mode: PositionMode = serde_json::from_value(case["accountMode"].clone()).unwrap();
        let result = derive_symbol_config("BTCUSDT", &positions, existing.as_ref(), mode);
        match result {
            Ok(config) => assert_eq!(encode(&config), case["output"], "derive case {index}"),
            Err(e) => assert_eq!(
                json!({"name":e.name,"message":e.message}),
                case["error"],
                "derive error {index}"
            ),
        }
        for getter in case["getters"].as_array().unwrap() {
            let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDT","side":getter["side"],"reduceOnly":getter["reduceOnly"],"type":"limit","amount":1,"price":2})).unwrap();
            match get_leverage_for_order(&opts, &positions, existing.as_ref(), mode) {
                Ok(value) => assert_eq!(
                    encode(&value),
                    getter["output"],
                    "get case {index}: {getter}"
                ),
                Err(e) => assert_eq!(
                    json!({"name":e.name,"message":e.message}),
                    getter["error"],
                    "get error {index}"
                ),
            }
            getters += 1;
        }
    }
    assert_eq!(getters, 720);
}

#[test]
fn mexc_symbol_config_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "mexc_symbol_config_live_source", "--nocapture"])
        .env("MEXC_TS_CONTROL", "config-oneway")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_symbol_config_live_source ... FAILED"),
        "{text}"
    );
    assert!(
        text.contains("derive case") && text.contains("assertion"),
        "{text}"
    );
}
