use super::*;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, Ticker};
use serde_json::json;
fn class(n: f64) -> &'static str {
    if n.is_nan() {
        "NaN"
    } else if n == f64::INFINITY {
        "Infinity"
    } else if n == f64::NEG_INFINITY {
        "-Infinity"
    } else if n == 0. && n.is_sign_negative() {
        "-0"
    } else {
        "finite"
    }
}
fn norm(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            let value = n.as_f64().unwrap();
            json!(if value == 0. { 0. } else { value })
        }
        Value::Array(a) => Value::Array(a.into_iter().map(norm).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, norm(v))).collect()),
        v => v,
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn inverse_position_arithmetic_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-position-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, c) in cases.iter().enumerate() {
        let ticker = if c["ticker"].is_null() {
            None
        } else {
            Some(serde_json::from_value::<Ticker>(json!({"id":"venue-ticker-id","symbol":"BTCUSD","bid":0,"ask":0,"mark":c["ticker"]["mark"],"last":c["ticker"]["last"],"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap())
        };
        let store = DefaultStore::new();
        let market: Market = serde_json::from_value(json!({"id":"venue-market-id","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":100000},"leverage":{"min":1,"max":100}},"isInverse":c["inverse"]})).unwrap();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market]),
                tickers: Some(ticker.into_iter().collect()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let mut logs = vec![];
        let mut clock_calls = 0;
        let result = map_position(
            &c["row"],
            &store,
            Some(serde_json::from_value(c["margin"].clone()).unwrap()),
            || {
                clock_calls += 1;
                123456789.
            },
            |s| {
                logs.push(s);
                Store::update(
                    &store,
                    StoreUpdate {
                        tickers: Some(vec![]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            },
        );
        if c["boundary"] == "typed-symbol" {
            assert!(
                c.get("error").is_none(),
                "source accepts raw symbol case{i}: {c}"
            );
            assert_eq!(c["result"].get("symbol"), c["row"].get("symbol"));
            assert_eq!(
                result.unwrap_err().to_string(),
                "Bybit typed Position requires string symbol"
            );
            assert_eq!(json!(logs), c["logs"]);
            assert_eq!(json!(clock_calls), c["clockCalls"]);
            continue;
        }
        if c["boundary"] == "inherited-side" {
            assert!(
                c.get("error").is_none(),
                "source preserves inherited side case{i}: {c}"
            );
            let key = c["row"]["side"].as_str().unwrap();
            assert_eq!(
                c["sideTruthy"], true,
                "inherited side suppresses Long fallback"
            );
            if key == "__proto__" {
                assert_eq!(c["sideKind"], "object");
                assert_eq!(c["result"]["side"], json!({}));
            } else {
                assert_eq!(c["sideKind"], "function", "not an undefined table miss");
                assert!(
                    c["result"].get("side").is_none(),
                    "JSON omits inherited functions"
                );
            }
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("Bybit typed Position cannot represent inherited side key {key}")
            );
            assert_eq!(json!(logs), c["logs"]);
            assert_eq!(json!(clock_calls), c["clockCalls"]);
            continue;
        }
        assert_eq!(json!(clock_calls), c["clockCalls"], "clock case{i}");
        assert_eq!(
            logs,
            c["logs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "logs case{i}"
        );
        if let Some(error) = c.get("error") {
            assert_eq!(
                result.unwrap_err().to_string(),
                error.as_str().unwrap(),
                "error case{i}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("case{i}: {e}"));
        assert_eq!(
            norm(serde_json::to_value(&result).unwrap()),
            norm(c["result"].clone()),
            "position case{i}: {c}"
        );
        let classes = [
            result.entry_price,
            result.notional,
            result.leverage,
            result.unrealized_pnl,
            result.realized_pnl.unwrap(),
            result.contracts,
            result.liquidation_price,
            result.margin_size.unwrap_or(0.),
        ]
        .map(class);
        assert_eq!(json!(classes), c["classes"], "numeric classes case{i}");
    }
    assert_eq!(cases.len(), 496);
}
