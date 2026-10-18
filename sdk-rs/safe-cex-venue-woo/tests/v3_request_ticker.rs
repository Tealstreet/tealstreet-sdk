use safe_cex_types::Market;
use safe_cex_venue_woo::{
    v3_public::WooV3FuturesRow,
    v3_ticker::{
        WooV3AuthoritativeTicker, WooV3AuthoritativeTickerInfo,
        compose_woo_v3_authoritative_ticker, normalize_perpetual_symbol,
        require_positive_finite_price,
    },
    v3_ws_quotes::{WooV3WsBbo, WooV3WsTrade},
};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};
fn source() -> &'static Value {
    static SOURCE: OnceLock<Value> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/v3-request-ticker-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "V3 source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn raw(name: &str) -> &str {
    source()["records"][name]["raw"].as_str().unwrap()
}
fn bits(number: f64, expected: &Value, assertion: &str) {
    let actual = if number.is_nan() {
        "7ff8000000000000".into()
    } else {
        format!("{:016x}", number.to_bits())
    };
    assert_eq!(actual, expected["$bits"].as_str().unwrap(), "{assertion}");
}
fn bbo(value: &Value) -> WooV3WsBbo {
    WooV3WsBbo {
        symbol: value["symbol"].as_str().unwrap().into(),
        ask_price: value["askPrice"].as_str().unwrap().into(),
        ask_quantity: value["askQuantity"].as_str().unwrap().into(),
        bid_price: value["bidPrice"].as_str().unwrap().into(),
        bid_quantity: value["bidQuantity"].as_str().unwrap().into(),
        timestamp: safe_cex_num::js_number(value.get("timestamp")).unwrap(),
    }
}
fn trade(value: &Value) -> WooV3WsTrade {
    WooV3WsTrade {
        symbol: value["symbol"].as_str().unwrap().into(),
        price: value["price"].as_str().unwrap().into(),
        quantity: value["quantity"].as_str().unwrap().into(),
        side: value["side"].as_str().unwrap().into(),
        source: safe_cex_num::js_number(value.get("source")).unwrap(),
        matched_rpi: value["matchedRpi"].as_bool().unwrap(),
        timestamp: safe_cex_num::js_number(value.get("timestamp")).unwrap(),
    }
}
fn check_info(actual: &WooV3AuthoritativeTickerInfo, expected: &Value) {
    let stats = &actual.futures_stats;
    let e = &expected["futuresStats"];
    assert_eq!(
        stats.id.as_deref(),
        e["id"].as_str(),
        "INFO native futures id"
    );
    assert_eq!(
        stats.symbol.as_deref(),
        e["symbol"].as_str(),
        "INFO native futures symbol"
    );
    for (value, key) in [
        (stats.index, "index"),
        (stats.mark, "mark"),
        (stats.open24h, "open24h"),
        (stats.close24h, "close24h"),
        (stats.high24h, "high24h"),
        (stats.low24h, "low24h"),
        (stats.percentage24h, "percentage24h"),
        (stats.open_interest, "openInterest"),
        (stats.estimated_funding_rate, "estimatedFundingRate"),
        (stats.last_funding_rate, "lastFundingRate"),
        (stats.funding_time, "fundingTime"),
        (stats.volume24h, "volume24h"),
        (stats.quote_volume24h, "quoteVolume24h"),
    ] {
        bits(value, &e[key], &format!("INFO native {key}"));
    }
    check_bbo(&actual.bbo, &expected["bbo"]);
    check_trade(&actual.trade, &expected["trade"]);
}
fn check_bbo(actual: &WooV3WsBbo, expected: &Value) {
    assert_eq!(
        actual.ask_price, expected["askPrice"],
        "BBO native ask price"
    );
    assert_eq!(
        actual.bid_price, expected["bidPrice"],
        "BBO native bid price"
    );
    assert_eq!(
        actual.ask_quantity, expected["askQuantity"],
        "BBO native ask quantity"
    );
    assert_eq!(
        actual.bid_quantity, expected["bidQuantity"],
        "BBO native bid quantity"
    );
    assert_eq!(actual.symbol, expected["symbol"], "BBO native symbol");
    bits(
        actual.timestamp,
        &expected["timestamp"],
        "BBO native timestamp",
    );
}
fn check_trade(actual: &WooV3WsTrade, expected: &Value) {
    assert_eq!(actual.price, expected["price"], "TRADE native price");
    assert_eq!(
        actual.quantity, expected["quantity"],
        "TRADE native quantity"
    );
    assert_eq!(actual.symbol, expected["symbol"], "TRADE native symbol");
    assert_eq!(actual.side, expected["side"], "TRADE native side");
    assert_eq!(
        actual.matched_rpi,
        expected["matchedRpi"].as_bool().unwrap(),
        "TRADE native RPI"
    );
    bits(actual.source, &expected["source"], "TRADE native source");
    bits(
        actual.timestamp,
        &expected["timestamp"],
        "TRADE native timestamp",
    );
}
fn check_ticker(actual: &WooV3AuthoritativeTicker, expected: &Value) {
    assert_eq!(actual.id, expected["id"], "TICKER native id");
    assert_eq!(actual.symbol, expected["symbol"], "TICKER native symbol");
    assert!(actual.mid_price.is_none(), "TICKER absent midpoint");
    bits(actual.bid, &expected["bid"], "COMPOSE native bid");
    for (value, key) in [
        (actual.ask, "ask"),
        (actual.last, "last"),
        (actual.mark, "mark"),
        (actual.index, "index"),
        (actual.percentage, "percentage"),
        (actual.open_interest, "openInterest"),
        (actual.funding_rate, "fundingRate"),
        (actual.funding_time, "fundingTime"),
        (actual.volume, "volume"),
        (actual.quote_volume, "quoteVolume"),
    ] {
        bits(value, &expected[key], &format!("TICKER native {key}"));
    }
    check_info(&actual.info, &expected["info"]);
}
#[test]
fn quote_declarations_decode_native_fields() {
    let actual: WooV3WsBbo = serde_json::from_str(raw("bbo")).expect("BBO native decode");
    check_bbo(&actual, &source()["records"]["bbo"]["native"]);
    let actual: WooV3WsTrade = serde_json::from_str(raw("trade")).expect("TRADE native decode");
    check_trade(&actual, &source()["records"]["trade"]["native"]);
    for text in ["[]", "null"] {
        assert!(
            serde_json::from_str::<WooV3WsBbo>(text).is_err(),
            "BBO map-only"
        );
        assert!(
            serde_json::from_str::<WooV3WsTrade>(text).is_err(),
            "TRADE map-only"
        );
    }
    let mut value: Value = serde_json::from_str(raw("trade")).unwrap();
    value["side"] = Value::String("buy".into());
    assert!(
        serde_json::from_value::<WooV3WsTrade>(value).is_err(),
        "TRADE closed side"
    );
    let mut value: Value = serde_json::from_str(raw("trade")).unwrap();
    value["source"] = Value::from(2);
    assert!(
        serde_json::from_value::<WooV3WsTrade>(value).is_err(),
        "TRADE closed source"
    );
    let edge = raw("trade").replace("\"source\":1", "\"source\":-0");
    let actual: WooV3WsTrade = serde_json::from_str(&edge).unwrap();
    assert_eq!(
        actual.source.to_bits(),
        (-0.0_f64).to_bits(),
        "TRADE literal zero sign"
    );
}
#[test]
fn ticker_declarations_decode_native_fields() {
    let info: WooV3AuthoritativeTickerInfo =
        serde_json::from_str(raw("info")).expect("INFO native decode");
    check_info(&info, &source()["records"]["info"]["native"]);
    let ticker: WooV3AuthoritativeTicker =
        serde_json::from_str(raw("ticker")).expect("TICKER native decode");
    check_ticker(&ticker, &source()["records"]["ticker"]["native"]);
    for text in ["[]", "null"] {
        assert!(
            serde_json::from_str::<WooV3AuthoritativeTickerInfo>(text).is_err(),
            "INFO map-only"
        );
        assert!(
            serde_json::from_str::<WooV3AuthoritativeTicker>(text).is_err(),
            "TICKER map-only"
        );
    }
}
#[test]
fn ticker_operations_match_source_native_values() {
    let cases = source()["normalized"].as_array().unwrap();
    assert_eq!(cases.len(), 10, "NORMALIZE independent corpus floor");
    for case in cases {
        let actual = normalize_perpetual_symbol(case["input"].as_str().unwrap(), "symbol-path");
        if let Some(error) = case.get("error") {
            assert_eq!(
                actual.unwrap_err(),
                error["message"],
                "NORMALIZE native error"
            );
        } else {
            assert_eq!(actual.unwrap(), case["ok"], "NORMALIZE native symbol");
        }
    }
    let cases = source()["priceCases"].as_array().unwrap();
    assert_eq!(cases.len(), 15, "PRICE independent corpus floor");
    for case in cases {
        let actual = require_positive_finite_price(case["input"].as_str().unwrap(), "price-path");
        if let Some(error) = case.get("error") {
            assert_eq!(actual.unwrap_err(), error["message"], "PRICE native error");
        } else {
            bits(actual.unwrap(), &case["ok"], "PRICE native bits");
        }
    }
    let cases = source()["tickerCases"].as_array().unwrap();
    assert_eq!(cases.len(), 14, "COMPOSE independent corpus floor");
    for case in cases {
        let args = case["args"].as_array().unwrap();
        assert_eq!(args.len(), 4, "COMPOSE independent argument count");
        let futures: WooV3FuturesRow = serde_json::from_str(args[0].as_str().unwrap()).unwrap();
        let mut bbo = bbo(&serde_json::from_str(args[1].as_str().unwrap()).unwrap());
        let mut trade = trade(&serde_json::from_str(args[2].as_str().unwrap()).unwrap());
        let market: Market = serde_json::from_str(args[3].as_str().unwrap()).unwrap();
        let result = compose_woo_v3_authoritative_ticker(&futures, &bbo, &trade, &market);
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error["message"],
                "COMPOSE native error priority"
            );
        } else {
            let mut actual = result.unwrap();
            check_ticker(&actual, &case["ok"]);
            if case["label"] == "ordinary" {
                bbo.ask_price = "changed-after-composition".into();
                trade.quantity = "changed-after-composition".into();
                assert_eq!(
                    actual.info.bbo.ask_price, "131",
                    "INFO native owned BBO snapshot"
                );
                assert_eq!(
                    actual.info.trade.quantity, "23",
                    "INFO native owned Trade snapshot"
                );
                actual.bid = 999.0;
                assert_eq!(actual.bid, 999.0, "TICKER native mutable descriptor");
            }
        }
    }
    assert_eq!(
        source()["alias"]["after"]["info"]["bbo"]["askPrice"],
        "changed-after-composition",
        "INFO source live BBO alias"
    );
    assert_eq!(
        source()["alias"]["after"]["info"]["trade"]["quantity"],
        "changed-after-composition",
        "INFO source live Trade alias"
    );
    assert_eq!(
        source()["alias"]["freezeError"],
        "TypeError",
        "TICKER source frozen write"
    );
}
