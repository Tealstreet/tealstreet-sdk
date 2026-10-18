use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::Market;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_weex::{
    market_data::{normalize_market, normalize_ticker},
    utils::{WeexUtilityError, WeexUtilityInput},
};
use serde_json::Value;
use std::{path::Path, process::Command};

fn input(raw: &str, route: usize) -> WeexUtilityInput {
    match route {
        0 => serde_json::from_str(raw).unwrap(),
        1 => js_from_str(raw).unwrap(),
        2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
        _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
    }
}
fn number(n: f64, expected: &Value, label: &str) {
    assert_eq!(
        format!("{:016x}", n.to_bits()),
        expected["bits"].as_str().unwrap(),
        "{label}"
    );
}
fn error(error: WeexUtilityError, case: &Value, label: &str) {
    assert_eq!(
        error.name,
        case["error"]["name"].as_str().unwrap(),
        "{label}::error_name"
    );
    assert_eq!(
        error.message,
        case["error"]["message"].as_str().unwrap(),
        "{label}::error_message"
    );
}
#[test]
fn market_data_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/market-data-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["node"].is_string());
    let market_ids = [
        "distinct",
        "defaults",
        "aliases",
        "zero",
        "invalid_precision",
        "hex_precision",
        "coercion",
        "underflow",
        "price_error",
        "eager_fallback",
        "amount_error",
        "null",
    ];
    let cases = corpus["marketCases"].as_array().unwrap();
    assert_eq!(cases.len(), market_ids.len(), "market_data::market_cases");
    let (mut market_success, mut market_errors, mut fields) = (0, 0, 0);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            case["id"], market_ids[index],
            "market_data::market_identity"
        );
        for route in 0..4 {
            let raw = input(case["raw"].as_str().unwrap(), route);
            let label = format!("market_data::market::{}::route_{route}", market_ids[index]);
            match normalize_market(&raw) {
                Err(e) => {
                    error(e, case, &label);
                    market_errors += 1;
                }
                Ok(value) => {
                    assert!(case.get("error").is_none(), "{label}::unexpected_success");
                    for (key, text) in [
                        ("id", &value.id),
                        ("symbol", &value.symbol),
                        ("base", &value.base),
                        ("quote", &value.quote),
                    ] {
                        assert_eq!(
                            text,
                            case["strings"][key].as_str().unwrap(),
                            "{label}::{key}"
                        );
                    }
                    assert_eq!(
                        value.active,
                        case["active"].as_bool().unwrap(),
                        "{label}::active"
                    );
                    for (key, n) in [
                        ("amount", value.precision.amount),
                        ("price", value.precision.price),
                        ("minAmount", value.limits.amount.min),
                        (
                            "maxAmount",
                            safe_cex_num::numeric_value_to_number(&value.limits.amount.max),
                        ),
                        ("minLeverage", value.limits.leverage.min),
                        ("maxLeverage", value.limits.leverage.max),
                        ("contractSize", value.contract_size.unwrap()),
                        ("makerFee", value.maker_fee.unwrap()),
                        ("takerFee", value.taker_fee.unwrap()),
                    ] {
                        number(n, &case["numbers"][key], &format!("{label}::{key}"));
                        fields += 1;
                    }
                    assert_eq!(
                        value.info.as_ref().unwrap(),
                        &raw.as_value().clone(),
                        "{label}::native_info"
                    );
                    assert!(
                        value.pretty_symbol.is_none()
                            && value.is_expiring.is_none()
                            && value.is_inverse.is_none()
                            && value.is_spot.is_none()
                            && value.is_read_only.is_none(),
                        "{label}::absent_fields"
                    );
                    market_success += 1;
                }
            }
        }
    }
    assert_eq!(market_success, 32, "market_data::market_successes");
    assert_eq!(market_errors, 16, "market_data::market_errors");
    let market: Market = js_from_str(&corpus["market"].to_string()).unwrap();
    assert_eq!(market.id, "venue-id");
    assert_eq!(market.symbol, "ETHUSDT");
    let ticker_ids = [
        "distinct",
        "aliases",
        "nullish",
        "absent",
        "null",
        "scalar",
        "zero",
        "fallback",
        "percentage_ratio",
        "percentage_points",
        "coercion",
        "open_error",
        "bid_error",
        "book_priority",
    ];
    let cases = corpus["tickerCases"].as_array().unwrap();
    assert_eq!(cases.len(), ticker_ids.len(), "market_data::ticker_cases");
    let (mut ticker_success, mut ticker_errors) = (0, 0);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            case["id"], ticker_ids[index],
            "market_data::ticker_identity"
        );
        for route in 0..4 {
            let raw = case.get("raw").map(|v| input(v.as_str().unwrap(), route));
            let book = case.get("book").map(|v| input(v.as_str().unwrap(), route));
            let label = format!("market_data::ticker::{}::route_{route}", ticker_ids[index]);
            match normalize_ticker(raw.as_ref(), &market, book.as_ref()) {
                Err(e) => {
                    error(e, case, &label);
                    ticker_errors += 1;
                }
                Ok(value) => {
                    assert!(case.get("error").is_none(), "{label}::unexpected_success");
                    assert_eq!(value.id, "venue-id", "{label}::id");
                    assert_eq!(value.symbol, "ETHUSDT", "{label}::symbol");
                    assert_eq!(value.id, case["strings"]["id"].as_str().unwrap());
                    assert_eq!(value.symbol, case["strings"]["symbol"].as_str().unwrap());
                    for (key, n) in [
                        ("bid", value.bid),
                        ("ask", value.ask),
                        ("last", value.last),
                        ("mark", value.mark),
                        ("index", value.index),
                        ("percentage", value.percentage),
                        ("openInterest", value.open_interest),
                        ("fundingRate", value.funding_rate),
                        ("fundingTime", value.funding_time),
                        ("volume", value.volume),
                        ("quoteVolume", value.quote_volume),
                    ] {
                        number(n, &case["numbers"][key], &format!("{label}::{key}"));
                        fields += 1;
                    }
                    let info = Input::Object(vec![
                        (
                            "ticker".encode_utf16().collect(),
                            raw.as_ref()
                                .map_or(Input::Undefined, |raw| raw.as_value().clone()),
                        ),
                        (
                            "bookTicker".encode_utf16().collect(),
                            book.as_ref()
                                .map_or(Input::Undefined, |raw| raw.as_value().clone()),
                        ),
                    ]);
                    assert_eq!(value.info, Some(info), "{label}::native_info");
                    assert!(value.mid_price.is_none(), "{label}::mid_price_absent");
                    assert!(value.timestamp.is_none(), "{label}::timestamp_absent");
                    ticker_success += 1;
                }
            }
        }
    }
    assert_eq!(ticker_success, 48, "market_data::ticker_successes");
    assert_eq!(ticker_errors, 8, "market_data::ticker_errors");
    assert_eq!(fields, 816, "market_data::numeric_comparisons");
    let refusals = corpus["shapeRefusals"].as_array().unwrap();
    assert_eq!(refusals.len(), 3, "market_data::shape_refusal_count");
    for (index, case) in refusals.iter().enumerate() {
        assert_eq!(case["symbolType"], ["undefined", "number", "string"][index]);
        assert_eq!(case["baseIsNull"], index == 2);
        assert!(
            normalize_market(&input(case["raw"].as_str().unwrap(), 1)).is_err(),
            "market_data::shape_refusal"
        );
    }
    assert_eq!(corpus["callable"]["bits"], "401d000000000000");
    assert!(
        js_from_str::<WeexUtilityInput>(r#"{"extra":"\ud800"}"#)
            .unwrap()
            .as_value()
            .get("extra")
            .unwrap()
            .as_text()
            .unwrap()
            == [0xd800],
        "market_data::utf16_refusal"
    );
}

#[test]
fn precision_overflow_is_refused_before_scientific_notation() {
    // Do not run the enormous CCXT concatenation loop on the shared host.
    // These assert the native guard at the shared materialization ceiling;
    // they are not enormous-input live-source oracle comparisons.
    for count in ["1000000000000000000000".to_owned(), "9".repeat(400)] {
        for key in ["pricePrecision", "quantityPrecision"] {
            let raw = format!(
                r#"{{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","{key}":"{count}","minOrderSize":1}}"#
            );
            let error =
                normalize_market(&input(&raw, 1)).expect_err("market_data::precision_upper_bound");
            assert_eq!(
                error.name, "RangeError",
                "market_data::precision_upper_bound"
            );
            assert_eq!(
                error.message, "Invalid string length",
                "market_data::precision_upper_bound"
            );
        }
    }
}
