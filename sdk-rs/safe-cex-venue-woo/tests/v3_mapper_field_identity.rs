#[path = "support/number_bits.rs"]
mod capture_bits;
use safe_cex_venue_woo::v3_public::*;
use serde_json::{Value, json};

fn number(actual: f64, expected: &Value, field: &str) {
    let expected = expected["$bits"].as_str().expect("native source bits");
    let actual = capture_bits::number_bits(actual);
    assert_eq!(actual, expected, "{field} native field identity");
}
fn info_identity(actual: &safe_cex_types::retained_json::RetainedJsonInput, expected: &Value) {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match (actual, expected) {
        (Input::Float(_) | Input::Lexeme(_), Value::Object(_)) => {
            number(actual.as_f64().unwrap(), expected, "info")
        }
        (Input::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in a {
                info_identity(value, &b[&String::from_utf16(key).unwrap()]);
            }
        }
        (Input::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                info_identity(a, b);
            }
        }
        (Input::Null, Value::Null) => {}
        (Input::Bool(actual), Value::Bool(expected)) => {
            assert_eq!(actual, expected, "info native boolean")
        }
        (Input::Text(actual), Value::String(expected)) => assert_eq!(
            actual,
            &expected.encode_utf16().collect::<Vec<_>>(),
            "info native UTF16 text"
        ),
        _ => panic!("info native kind mismatch: {actual:?} vs {expected:?}"),
    }
}
macro_rules! numbers {
    ($value:ident, $input:ident, $($field:ident => $wire:literal),+ $(,)?) => {$(
        number($value.$field, &$input[$wire], stringify!($field));
    )+};
}

#[test]
fn public_mapper_native_field_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-oracle.cjs"
        ))
        .arg("--mapper-identity")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mapper oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(capture["capture"]["architecture"].is_string());
    let cases = &capture["fixture"];
    assert_eq!(
        cases.as_array().unwrap().len(),
        215,
        "WOO-MAPPER source total"
    );
    let mut counts = [0; 4];
    for case in cases
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case.get("ok").is_some())
    {
        let input = &case["input"];
        let expected = &case["ok"];
        match case["name"].as_str().unwrap() {
            "mapWooV3Markets" => {
                let rows: Vec<safe_cex_venue_woo::v3_contracts::WooV3Instrument> =
                    serde_json::from_value(input.clone()).unwrap();
                let values = map_woo_v3_markets(&rows).unwrap();
                let outputs = expected.as_array().unwrap();
                assert_eq!(values.len(), outputs.len());
                for (value, input) in values.iter().zip(outputs) {
                    for (actual, wire) in [
                        (&value.id, "id"),
                        (&value.symbol, "symbol"),
                        (&value.base, "base"),
                        (&value.quote, "quote"),
                    ] {
                        assert_eq!(
                            Some(actual.as_str()),
                            input[wire].as_str(),
                            "market {wire} native field identity"
                        );
                    }
                    assert_eq!(
                        Some(value.active),
                        input["active"].as_bool(),
                        "active native field identity"
                    );
                    for (actual, expected, field) in [
                        (
                            value.precision.amount,
                            &input["precision"]["amount"],
                            "precision.amount",
                        ),
                        (
                            value.precision.price,
                            &input["precision"]["price"],
                            "precision.price",
                        ),
                        (
                            value.limits.amount.min,
                            &input["limits"]["amount"]["min"],
                            "limits.amount.min",
                        ),
                        (
                            value.limits.amount.max,
                            &input["limits"]["amount"]["max"],
                            "limits.amount.max",
                        ),
                        (
                            value.limits.leverage.min,
                            &input["limits"]["leverage"]["min"],
                            "limits.leverage.min",
                        ),
                        (
                            value.limits.leverage.max,
                            &input["limits"]["leverage"]["max"],
                            "limits.leverage.max",
                        ),
                    ] {
                        number(actual, expected, field);
                    }
                    info_identity(value.info.as_ref().unwrap(), &input["info"]);
                    assert!(
                        value.pretty_symbol.is_none()
                            && value.contract_size.is_none()
                            && value.is_expiring.is_none()
                            && value.is_inverse.is_none()
                            && value.is_spot.is_none()
                            && value.is_read_only.is_none()
                            && value.taker_fee.is_none()
                            && value.maker_fee.is_none(),
                        "unpopulated market fields stay absent"
                    );
                }
                counts[0] += 1;
            }
            "mapWooV3FuturesStats" => {
                let markets = case["extra"][0].as_object().unwrap().iter().map(|(key, value)| {
                    let mut complete = json!({"base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}});
                    complete.as_object_mut().unwrap().extend(value.as_object().unwrap().clone());
                    (key.clone(), serde_json::from_value(complete).unwrap())
                }).collect();
                let rows: Vec<WooV3FuturesRow> = serde_json::from_value(input.clone()).unwrap();
                let values = map_woo_v3_futures_stats(&rows, &markets).unwrap();
                let outputs = expected.as_array().unwrap();
                assert_eq!(values.len(), outputs.len());
                for (value, input) in values.iter().zip(outputs) {
                    assert_eq!(
                        value.id.as_deref(),
                        input["id"].as_str(),
                        "stats id native field identity"
                    );
                    assert_eq!(
                        value.symbol.as_deref(),
                        input["symbol"].as_str(),
                        "stats symbol native field identity"
                    );
                    numbers!(value, input, index => "index", mark => "mark", open24h => "open24h", close24h => "close24h", high24h => "high24h", low24h => "low24h", percentage24h => "percentage24h", open_interest => "openInterest", estimated_funding_rate => "estimatedFundingRate", last_funding_rate => "lastFundingRate", funding_time => "fundingTime", volume24h => "volume24h", quote_volume24h => "quoteVolume24h");
                }
                counts[1] += 1;
            }
            "mapWooV3OrderBook" => {
                let value =
                    map_woo_v3_order_book(&serde_json::from_value(input.clone()).unwrap()).unwrap();
                number(value.timestamp, &expected["timestamp"], "timestamp");
                for (side, wire) in [(&value.book.asks, "asks"), (&value.book.bids, "bids")] {
                    let outputs = expected[wire].as_array().unwrap();
                    assert_eq!(side.len(), outputs.len());
                    for (value, input) in side.iter().zip(outputs) {
                        numbers!(value, input, price => "price", amount => "amount", total => "total");
                    }
                }
                counts[2] += 1;
            }
            "mapWooV3Candles" => {
                if case["nonfinite"] == true {
                    continue;
                }
                let rows: Vec<WooV3KlineRow> = serde_json::from_value(input.clone()).unwrap();
                let values = map_woo_v3_candles(&rows).unwrap();
                let outputs = expected.as_array().unwrap();
                assert_eq!(values.len(), outputs.len());
                for (value, input) in values.iter().zip(outputs) {
                    numbers!(value, input, timestamp => "timestamp", open => "open", high => "high", low => "low", close => "close", volume => "volume");
                    assert!(value.last_side.is_none(), "last_side native absence");
                }
                counts[3] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        counts,
        [28, 56, 13, 26],
        "WOO-MAPPER compared cases per family"
    );
}
