use safe_cex_venue_binance::public_subscriptions::subscribe_binance_public;
use serde_json::{Map, Value, json};
use std::cell::RefCell;

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn subscriptions_match_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-subscriptions.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 5505);
    for (index, c) in cases.iter().enumerate() {
        let topics = RefCell::new(
            c["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    let value = pair[1]
                        .get("raw")
                        .and_then(Value::as_str)
                        .map_or_else(|| pair[1].clone(), |raw| serde_json::from_str(raw).unwrap());
                    (pair[0].as_str().unwrap().to_owned(), value)
                })
                .collect::<Map<_, _>>(),
        );
        let keys = topics.borrow().keys().cloned().collect::<Vec<_>>();
        let count = |key: &str| {
            let topics = topics.borrow();
            let value = topics.get(key)?;
            match value.get("native").and_then(Value::as_str) {
                Some("NaN") => Some(f64::NAN),
                Some("Infinity") => Some(f64::INFINITY),
                Some("-Infinity") => Some(f64::NEG_INFINITY),
                _ => value.as_f64(),
            }
        };
        let mut sent = Vec::new();
        let send = |frame: &str| -> Result<(), &'static str> {
            sent.push(frame.to_owned());
            if sent.len() == 1 {
                match c["action"].as_str().unwrap() {
                    "remove" => {
                        topics.borrow_mut().shift_remove("last");
                    }
                    "activate" => {
                        topics.borrow_mut().insert("later".into(), json!(1));
                    }
                    "append" => {
                        topics.borrow_mut().insert("added".into(), json!(1));
                    }
                    "throw" => return Err("fixture send error"),
                    "none" => (),
                    _ => unreachable!(),
                }
            }
            Ok(())
        };
        // Every concrete PublicState uses the same production subscription body.
        let result = match c["variant"].as_str().unwrap() {
            "binance:BinanceMarketWebsocket"
            | "binance:BinancePublicWebsocket"
            | "binancepm:BinancePMMarketWebsocket"
            | "binancepm:BinancePMPublicWebsocket"
            | "binancespot:BinancePublicWebsocket" => subscribe_binance_public(keys, count, send),
            _ => unreachable!(),
        };
        assert_eq!(
            json!(sent),
            c["sent"],
            "subscription frames case {index}: {c}"
        );
        assert_eq!(
            json!(result.err()),
            c["error"],
            "subscription error case {index}: {c}"
        );
    }
}
