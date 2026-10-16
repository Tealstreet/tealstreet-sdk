use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_woo::{
    v3_ws_public::*,
    v3_ws_quotes::{WooV3WsBbo, WooV3WsTrade},
    v3_ws_topics::{WooV3WsKlineInterval, parse_woo_v3_ws_topic},
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-public-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WS-PUBLIC source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value) -> Option<JsNumberInput> {
    row["input"]
        .as_str()
        .map(|raw| serde_json::from_str(raw).unwrap())
}
fn number(actual: f64, expected: &Value, name: &str) {
    assert_eq!(
        actual.to_bits(),
        expected["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "{name}"
    );
}
fn error<T: std::fmt::Debug>(actual: Result<T, String>, expected: &Value) {
    assert_eq!(
        actual.unwrap_err(),
        expected["error"]["message"].as_str().unwrap(),
        "WS-PUBLIC native first validation error"
    );
    assert_eq!(expected["error"]["name"], "Error");
}
fn levels(actual: &[WooV3WsBookLevel], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "WS-PUBLIC native level count versus source"
    );
    for (index, actual) in actual.iter().enumerate() {
        assert_eq!(
            actual[0],
            expected[index][0].as_str().unwrap(),
            "WS-PUBLIC native level price"
        );
        assert_eq!(
            actual[1],
            expected[index][1].as_str().unwrap(),
            "WS-PUBLIC native level quantity"
        );
    }
}
fn order_book_update(actual: &WooV3WsOrderBookUpdate, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native order_book_update symbol"
    );
    number(
        actual.previous_timestamp,
        &expected["previousTimestamp"],
        "WS-PUBLIC native order_book_update previousTimestamp",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native order_book_update timestamp",
    );
    levels(&actual.asks, &expected["asks"]);
    levels(&actual.bids, &expected["bids"]);
}
fn order_book10(actual: &WooV3WsOrderBook10, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native order_book10 symbol"
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native order_book10 timestamp",
    );
    levels(&actual.asks, &expected["asks"]);
    levels(&actual.bids, &expected["bids"]);
}
fn ticker(actual: &WooV3WsTicker, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native ticker symbol"
    );
    assert_eq!(
        actual.open,
        expected["open"].as_str().unwrap(),
        "WS-PUBLIC native ticker open"
    );
    assert_eq!(
        actual.close,
        expected["close"].as_str().unwrap(),
        "WS-PUBLIC native ticker close"
    );
    assert_eq!(
        actual.high,
        expected["high"].as_str().unwrap(),
        "WS-PUBLIC native ticker high"
    );
    assert_eq!(
        actual.low,
        expected["low"].as_str().unwrap(),
        "WS-PUBLIC native ticker low"
    );
    assert_eq!(
        actual.volume,
        expected["volume"].as_str().unwrap(),
        "WS-PUBLIC native ticker volume"
    );
    assert_eq!(
        actual.amount,
        expected["amount"].as_str().unwrap(),
        "WS-PUBLIC native ticker amount"
    );
    assert_eq!(
        actual.aggregated_volume,
        expected["aggregatedVolume"].as_str().unwrap(),
        "WS-PUBLIC native ticker aggregatedVolume"
    );
    assert_eq!(
        actual.aggregated_amount,
        expected["aggregatedAmount"].as_str().unwrap(),
        "WS-PUBLIC native ticker aggregatedAmount"
    );
    number(
        actual.trade_count,
        &expected["tradeCount"],
        "WS-PUBLIC native ticker tradeCount",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native ticker timestamp",
    );
    number(
        actual.last_trade_timestamp,
        &expected["lastTradeTimestamp"],
        "WS-PUBLIC native ticker lastTradeTimestamp",
    );
}
fn kline(actual: &WooV3WsKline, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native kline symbol"
    );
    assert_eq!(expected["interval"], "5m");
    assert_eq!(
        actual.interval,
        WooV3WsKlineInterval::M5,
        "WS-PUBLIC native kline interval"
    );
    assert_eq!(
        actual.open,
        expected["open"].as_str().unwrap(),
        "WS-PUBLIC native kline open"
    );
    assert_eq!(
        actual.close,
        expected["close"].as_str().unwrap(),
        "WS-PUBLIC native kline close"
    );
    assert_eq!(
        actual.high,
        expected["high"].as_str().unwrap(),
        "WS-PUBLIC native kline high"
    );
    assert_eq!(
        actual.low,
        expected["low"].as_str().unwrap(),
        "WS-PUBLIC native kline low"
    );
    assert_eq!(
        actual.volume,
        expected["volume"].as_str().unwrap(),
        "WS-PUBLIC native kline volume"
    );
    assert_eq!(
        actual.amount,
        expected["amount"].as_str().unwrap(),
        "WS-PUBLIC native kline amount"
    );
    number(
        actual.start_timestamp,
        &expected["startTimestamp"],
        "WS-PUBLIC native kline startTimestamp",
    );
    number(
        actual.end_timestamp,
        &expected["endTimestamp"],
        "WS-PUBLIC native kline endTimestamp",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native kline timestamp",
    );
    number(
        actual.last_trade_timestamp,
        &expected["lastTradeTimestamp"],
        "WS-PUBLIC native kline lastTradeTimestamp",
    );
}
fn price(actual: &WooV3WsPrice, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native price symbol"
    );
    assert_eq!(
        actual.price,
        expected["price"].as_str().unwrap(),
        "WS-PUBLIC native price price"
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native price timestamp",
    );
}
fn funding_rate(actual: &WooV3WsFundingRate, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native funding_rate symbol"
    );
    assert_eq!(
        actual.rate,
        expected["rate"].as_str().unwrap(),
        "WS-PUBLIC native funding_rate rate"
    );
    number(
        actual.next_funding_timestamp,
        &expected["nextFundingTimestamp"],
        "WS-PUBLIC native funding_rate nextFundingTimestamp",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native funding_rate timestamp",
    );
}
fn open_interest(actual: &WooV3WsOpenInterest, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native open_interest symbol"
    );
    assert_eq!(
        actual.open_interest,
        expected["openInterest"].as_str().unwrap(),
        "WS-PUBLIC native open_interest openInterest"
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native open_interest timestamp",
    );
}
fn bbo(actual: &WooV3WsBbo, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native quote symbol"
    );
    assert_eq!(
        actual.ask_price,
        expected["askPrice"].as_str().unwrap(),
        "WS-PUBLIC native quote ask price"
    );
    assert_eq!(
        actual.ask_quantity,
        expected["askQuantity"].as_str().unwrap(),
        "WS-PUBLIC native quote ask quantity"
    );
    assert_eq!(
        actual.bid_price,
        expected["bidPrice"].as_str().unwrap(),
        "WS-PUBLIC native quote bid price"
    );
    assert_eq!(
        actual.bid_quantity,
        expected["bidQuantity"].as_str().unwrap(),
        "WS-PUBLIC native quote bid quantity"
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native quote timestamp",
    );
}
fn trade(actual: &WooV3WsTrade, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PUBLIC native trade symbol"
    );
    assert_eq!(
        actual.price,
        expected["price"].as_str().unwrap(),
        "WS-PUBLIC native trade price"
    );
    assert_eq!(
        actual.quantity,
        expected["quantity"].as_str().unwrap(),
        "WS-PUBLIC native trade quantity"
    );
    assert_eq!(
        actual.side,
        expected["side"].as_str().unwrap(),
        "WS-PUBLIC native trade side"
    );
    number(
        actual.source,
        &expected["source"],
        "WS-PUBLIC native trade source",
    );
    assert_eq!(
        actual.matched_rpi,
        expected["matchedRpi"].as_bool().unwrap(),
        "WS-PUBLIC native trade rpi"
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PUBLIC native trade timestamp",
    );
}
fn payload(actual: &WooV3WsPublicPayload, kind: &str, expected: &Value) {
    match (kind, actual) {
        ("orderbookupdate" | "orderbookupdaterpi", WooV3WsPublicPayload::OrderBookUpdate(row)) => {
            order_book_update(row, expected)
        }
        ("orderbook10", WooV3WsPublicPayload::OrderBook10(row)) => order_book10(row, expected),
        ("bbo" | "bborpi", WooV3WsPublicPayload::Bbo(row)) => bbo(row, expected),
        ("trade", WooV3WsPublicPayload::Trade(row)) => trade(row, expected),
        ("ticker", WooV3WsPublicPayload::Ticker(row)) => ticker(row, expected),
        ("kline", WooV3WsPublicPayload::Kline(row)) => kline(row, expected),
        ("indexprice" | "markprice", WooV3WsPublicPayload::Price(row)) => price(row, expected),
        ("estfundingrate", WooV3WsPublicPayload::FundingRate(row)) => funding_rate(row, expected),
        ("openinterest", WooV3WsPublicPayload::OpenInterest(row)) => open_interest(row, expected),
        _ => panic!("WS-PUBLIC native union variant for {kind}: {actual:?}"),
    }
}
#[test]
fn native_level_and_symbol_helpers_match_source() {
    let data = source();
    assert_eq!(data["levels"].as_array().unwrap().len(), 30);
    for index in 0..30 {
        let row = &data["levels"][index];
        let value = input(row);
        let actual = decode_levels(
            value.as_ref(),
            "test.levels",
            row["zero"].as_bool().unwrap(),
        );
        if let Some(expected) = row.get("ok") {
            levels(&actual.unwrap(), expected);
        } else {
            error(actual, row);
        }
    }
    let tuple: WooV3WsBookLevel = js_from_str(r#"["price-literal","quantity-literal"]"#).unwrap();
    assert_eq!(
        tuple[0], "price-literal",
        "WS-PUBLIC native tuple declaration price"
    );
    assert_eq!(
        tuple[1], "quantity-literal",
        "WS-PUBLIC native tuple declaration quantity"
    );
    for raw in [
        r#"["x"]"#,
        r#"["x","y","z"]"#,
        r#"{"0":"x","1":"y"}"#,
        r#"[1,"y"]"#,
    ] {
        assert!(
            js_from_str::<WooV3WsBookLevel>(raw).is_err(),
            "WS-PUBLIC strict tuple declaration {raw}"
        );
    }
    assert_eq!(data["symbols"].as_array().unwrap().len(), 6);
    for index in 0..6 {
        let row = &data["symbols"][index];
        let native: JsObjectMap<JsNumberInput> =
            serde_json::from_str(row["input"].as_str().unwrap()).unwrap();
        let topic = Value::String(row["topic"].as_str().unwrap().to_owned());
        let descriptor = parse_woo_v3_ws_topic(Some(&topic)).unwrap();
        let actual = assert_payload_symbol(&native, &descriptor, "test.symbol");
        if let Some(expected) = row.get("ok") {
            assert_eq!(
                actual.unwrap(),
                expected.as_str().unwrap(),
                "WS-PUBLIC native validated symbol"
            );
        } else {
            error(actual, row);
        }
    }
}
#[test]
fn decoded_native_public_declarations_and_union() {
    let data = source();
    assert_eq!(data["declarations"].as_array().unwrap().len(), 7);
    assert_eq!(data["unionNames"].as_array().unwrap().len(), 9);
    let names = [
        "WooV3WsOrderBookUpdate",
        "WooV3WsOrderBook10",
        "WooV3WsTicker",
        "WooV3WsKline",
        "WooV3WsPrice",
        "WooV3WsFundingRate",
        "WooV3WsOpenInterest",
    ];
    for index in 0..7 {
        let row = &data["declarations"][index];
        assert_eq!(
            row["name"], names[index],
            "WS-PUBLIC independent declaration floor"
        );
        let raw = row["input"].as_str().unwrap();
        let expected = &row["expected"];
        match names[index] {
            "WooV3WsOrderBookUpdate" => order_book_update(
                &js_from_str::<WooV3WsOrderBookUpdate>(raw).unwrap(),
                expected,
            ),
            "WooV3WsOrderBook10" => {
                order_book10(&js_from_str::<WooV3WsOrderBook10>(raw).unwrap(), expected)
            }
            "WooV3WsTicker" => ticker(&js_from_str::<WooV3WsTicker>(raw).unwrap(), expected),
            "WooV3WsKline" => kline(&js_from_str::<WooV3WsKline>(raw).unwrap(), expected),
            "WooV3WsPrice" => price(&js_from_str::<WooV3WsPrice>(raw).unwrap(), expected),
            "WooV3WsFundingRate" => {
                funding_rate(&js_from_str::<WooV3WsFundingRate>(raw).unwrap(), expected)
            }
            "WooV3WsOpenInterest" => {
                open_interest(&js_from_str::<WooV3WsOpenInterest>(raw).unwrap(), expected)
            }
            _ => unreachable!(),
        }
        let actual: WooV3WsPublicPayload = js_from_str(raw).unwrap();
        payload(
            &actual,
            [
                "orderbookupdate",
                "orderbook10",
                "ticker",
                "kline",
                "indexprice",
                "estfundingrate",
                "openinterest",
            ][index],
            expected,
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsOrderBookUpdate>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsOrderBookUpdate record {raw}"
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsOrderBook10>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsOrderBook10 record {raw}"
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsTicker>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsTicker record {raw}"
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsKline>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsKline record {raw}"
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsPrice>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsPrice record {raw}"
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsFundingRate>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsFundingRate record {raw}"
        );
    }
    for raw in ["[]", "null", "{}", r#"["PERP_BTC_USDT",1]"#] {
        assert!(
            js_from_str::<WooV3WsOpenInterest>(raw).is_err(),
            "WS-PUBLIC strict WooV3WsOpenInterest record {raw}"
        );
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsPublicPayload>(raw).is_err(),
            "WS-PUBLIC strict union record {raw}"
        );
    }
    assert!(js_from_str::<WooV3WsKline>(r#"{"symbol":"S","interval":"bad","open":"1","close":"2","high":"3","low":"4","volume":"5","amount":"6","startTimestamp":1,"endTimestamp":2,"timestamp":3,"lastTradeTimestamp":4}"#).is_err(),"WS-PUBLIC strict native interval");
}
#[test]
fn native_public_dispatch_preserves_fields_variants_and_first_errors() {
    let data = source();
    let kinds = [
        "orderbookupdate",
        "orderbookupdaterpi",
        "orderbook10",
        "bbo",
        "bborpi",
        "trade",
        "ticker",
        "kline",
        "indexprice",
        "markprice",
        "estfundingrate",
        "openinterest",
    ];
    assert_eq!(data["kinds"].as_array().unwrap().len(), 12);
    assert_eq!(data["payloads"].as_array().unwrap().len(), 46);
    for index in 0..12 {
        assert_eq!(
            data["kinds"][index], kinds[index],
            "WS-PUBLIC independent branch floor"
        );
    }
    for index in 0..46 {
        let row = &data["payloads"][index];
        let topic = row["topic"].as_str().unwrap();
        let kind = topic.split('@').next().unwrap();
        let descriptor = parse_woo_v3_ws_topic(Some(&Value::String(topic.to_owned()))).unwrap();
        let value = input(row);
        let actual = decode_public_payload(&descriptor, value.as_ref());
        if let Some(expected) = row.get("ok") {
            payload(&actual.unwrap(), kind, expected);
            let declaration: WooV3WsPublicPayload =
                js_from_str(row["decoded"].as_str().unwrap()).unwrap();
            payload(&declaration, kind, expected);
        } else {
            error(actual, row);
        }
    }
    // The source signature admits public descriptors; the shared Rust enum also carries private ones.
    let private = parse_woo_v3_ws_topic(Some(&Value::String("account".into()))).unwrap();
    assert_eq!(
        decode_public_payload(&private, None).unwrap_err(),
        "WOO V3 websocket payload requires a public topic descriptor",
        "WS-PUBLIC chosen private descriptor refusal"
    );
}
