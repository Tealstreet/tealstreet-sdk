#[path = "support/inverse_rest_runtime.rs"]
mod support;
use safe_cex_exchange::{
    js_runtime::{
        promise::await_js,
        value::{Js, Thrown, from_json},
    },
    runtime::ExchangeFuture,
};
use safe_cex_store::Store;
use serde_json::json;
use support::*;

fn drive(h: &Harness, value: Result<Js, Thrown>) -> Js {
    let pending = value.unwrap();
    let future: ExchangeFuture<'_, Js> =
        Box::pin(async move { await_js(pending).await.map_err(|e| e.to_exchange_error()) });
    h.drive(future).unwrap()
}
fn call(h: &Harness, method: &str, args: Vec<Js>) -> Js {
    drive(h, h.exchange.receiver().call_method(method, args))
}
fn raw_market() -> serde_json::Value {
    json!({"symbol":"BTCUSD_PERP","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractSize":100,"contractType":"PERPETUAL","contractStatus":"TRADING","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"10"},{"filterType":"MARKET_LOT_SIZE","minQty":"2","maxQty":"20"}]})
}
fn seed(h: &Harness) {
    Store::update(h.store.as_ref(), serde_json::from_value(json!({
        "markets":[{"id":"BTC/USD:BTC","symbol":"BTCUSD_PERP","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10},"leverage":{"min":1,"max":50}},"contractSize":100,"isInverse":true}],
        "tickers":[{"id":"BTC/USD:BTC","symbol":"BTCUSD_PERP","bid":4999,"ask":5001,"last":5000,"mark":5000,"index":5000,"percentage":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":5000,"openInterest":0}]
    })).unwrap(), false).unwrap();
}
#[test]
fn constructor_installs_source_clients_and_live_read_methods() {
    let h = harness(options(), |_| {});
    let receiver = h.exchange.receiver();
    for key in [
        "xhr",
        "_primaryXhr",
        "_secondaryXhr",
        "unlimitedXHR",
        "publicXhr",
    ] {
        assert!(
            receiver
                .get(key)
                .unwrap()
                .get("get")
                .is_ok_and(|v| v.is_callable()),
            "{key}: source Axios client must be assembled"
        );
    }
    for key in [
        "fetchMarkets",
        "fetchTickers",
        "fetchPositions",
        "fetchOrders",
        "fetchOHLCV",
        "placeOrder",
        "cancelOrders",
    ] {
        assert!(receiver.get(key).unwrap().is_callable(), "{key}");
    }
    assert!(h.http.sent.lock().unwrap().is_empty());
}

#[test]
fn signed_response_error_keeps_native_utf16_message_and_response() {
    let h = harness(options(), |wire| {
        wire.encoded(
            "GET",
            "/dapi/v1/openOrders",
            br#"{"code":-2011,"msg":"\ud800"}"#,
        )
    });
    let pending = h
        .exchange
        .receiver()
        .get("xhr")
        .unwrap()
        .call_method("get", vec![Js::str("/dapi/v1/openOrders")])
        .unwrap();
    let error = h
        .drive(Box::pin(async move { Ok(await_js(pending).await) }))
        .unwrap()
        .unwrap_err();
    let message = error.value.get("message").unwrap();
    assert_eq!(
        message.as_str().unwrap().as_ref(),
        &[0xd800_u16],
        "native Error must retain source message code units"
    );
    assert!(
        message.strict_equals(
            &error
                .value
                .get("response")
                .unwrap()
                .get("data")
                .unwrap()
                .get("msg")
                .unwrap()
        )
    );
    assert!(
        error
            .value
            .get("name")
            .unwrap()
            .strict_equals(&Js::str("Error"))
    );
    assert_eq!(error.value.get("code").unwrap().as_f64(), Some(-2011.0));
    assert!(error.value.get("__binanceEmitted").unwrap().truthy());
    assert_eq!(h.http.calls("/dapi/v1/openOrders").len(), 1);
}

#[test]
fn ohlcv_uses_source_regex_interval_and_preserves_missing_volume_nan() {
    let h = harness(options(), |wire| {
        wire.ok(
            "GET",
            "/dapi/v1/klines",
            json!([[1_700_000_000_000_u64, "1", "3", "0", "2"]]),
        )
    });
    let rows = call(
        &h,
        "fetchOHLCV",
        vec![from_json(&json!({
            "symbol":"BTCUSD_PERP","interval":"15m","limit":2,"to":1_700_000_000_000_u64,
        }))],
    );
    assert_eq!(rows.get("length").unwrap().as_f64(), Some(1.0));
    let candle = rows.get("0").unwrap();
    for (key, expected) in [
        ("timestamp", 1_700_000_000.0_f64),
        ("open", 1.0),
        ("high", 3.0),
        ("low", 0.0),
        ("close", 2.0),
    ] {
        assert_eq!(
            candle.get(key).unwrap().as_f64().unwrap().to_bits(),
            expected.to_bits(),
            "native {key}"
        );
    }
    assert!(candle.get("volume").unwrap().as_f64().unwrap().is_nan());
    let requests = h.http.calls("/dapi/v1/klines");
    assert_eq!(requests.len(), 1);
    for pair in [
        "interval=15m",
        "limit=2",
        "startTime=1699998200000",
        "endTime=1700000000000",
    ] {
        assert!(
            requests[0].url.contains(pair),
            "actual public candle query: {pair}"
        );
    }
    assert!(!requests[0].url.contains("signature="));
    assert!(!requests[0].headers.contains_key("X-MBX-APIKEY"));
}
#[test]
fn markets_use_coin_m_endpoints_filters_and_signed_account_brackets() {
    let h = harness(options(), |wire| {
        wire.ok("GET", "/dapi/v1/exchangeInfo", json!({"symbols":[raw_market(), {"contractType":"CURRENT_QUARTER","contractStatus":"TRADING"}]}));
        wire.ok(
            "GET",
            "/dapi/v2/leverageBracket",
            json!([{"symbol":"BTCUSD_PERP","brackets":[{"initialLeverage":75}]}]),
        );
    });
    let result = call(&h, "fetchMarkets", vec![]);
    assert_eq!(result.get("length").unwrap().as_f64(), Some(1.0));
    let m = result.get("0").unwrap();
    assert!(m.get("id").unwrap().strict_equals(&Js::str("BTC/USD:BTC")));
    assert_eq!(m.get("contractSize").unwrap().as_f64(), Some(100.0));
    assert_eq!(
        m.get("limits")
            .unwrap()
            .get("amount")
            .unwrap()
            .get("min")
            .unwrap()
            .as_f64(),
        Some(2.0)
    );
    assert_eq!(
        m.get("limits")
            .unwrap()
            .get("amount")
            .unwrap()
            .get("max")
            .unwrap()
            .as_f64(),
        Some(10.0)
    );
    assert_eq!(
        m.get("limits")
            .unwrap()
            .get("leverage")
            .unwrap()
            .get("max")
            .unwrap()
            .as_f64(),
        Some(75.0)
    );
    assert!(m.get("isInverse").unwrap().truthy());
    let calls = h.http.calls("/dapi/v1/exchangeInfo");
    assert_eq!(calls.len(), 1);
    assert!(!calls[0].headers.contains_key("X-MBX-APIKEY"));
    assert_eq!(
        calls[0].base_url.as_deref(),
        Some("https://dapi.binance.com")
    );
    let calls = h.http.calls("/dapi/v2/leverageBracket");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].url.contains("signature="));
    assert_eq!(
        calls[0].headers.get("X-MBX-APIKEY").map(String::as_str),
        Some("synthetic-key")
    );
}
#[test]
fn positions_convert_native_contracts_without_changing_source_info() {
    let h = harness(options(), |wire| {
        wire.ok("GET", "/dapi/v1/positionRisk", json!([{
        "symbol":"BTCUSD_PERP","positionAmt":"100","positionSide":"BOTH","entryPrice":"4000","markPrice":"5000","unRealizedProfit":"0.25","liquidationPrice":"2500","leverage":"10","isolatedMargin":"0","marginType":"cross"
    }]))
    });
    seed(&h);
    let positions = call(&h, "fetchPositions", vec![]);
    assert_eq!(positions.get("length").unwrap().as_f64(), Some(1.0));
    let p = positions.get("0").unwrap();
    assert_eq!(p.get("contracts").unwrap().as_f64(), Some(2.0));
    assert_eq!(p.get("notional").unwrap().as_f64(), Some(10000.0));
    assert_eq!(p.get("marginSize").unwrap().as_f64(), Some(0.2));
    assert_eq!(
        p.get("info")
            .unwrap()
            .get("contractsNative")
            .unwrap()
            .as_f64(),
        Some(100.0)
    );
    assert!(
        p.get("info")
            .unwrap()
            .get("positionAmt")
            .unwrap()
            .strict_equals(&Js::str("100"))
    );
    assert!(
        p.get("positionMode")
            .unwrap()
            .strict_equals(&Js::str("oneway"))
    );
}
#[test]
fn listen_key_is_api_key_authenticated_and_unsigned_on_demo() {
    let mut opts = options();
    opts.testnet = Some(true);
    let h = harness(opts, |wire| {
        wire.ok(
            "POST",
            "/dapi/v1/listenKey",
            json!({"listenKey":"native-listen-key"}),
        )
    });
    let xhr = h.exchange.receiver().get("xhr").unwrap();
    let result = drive(
        &h,
        xhr.call_method("post", vec![Js::str("/dapi/v1/listenKey")]),
    );
    assert!(
        result
            .get("data")
            .unwrap()
            .get("listenKey")
            .unwrap()
            .strict_equals(&Js::str("native-listen-key"))
    );
    let calls = h.http.calls("/dapi/v1/listenKey");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].url, "/dapi/v1/listenKey");
    assert_eq!(
        calls[0].base_url.as_deref(),
        Some("https://demo-dapi.binance.com")
    );
    assert_eq!(
        calls[0].headers.get("X-MBX-APIKEY").map(String::as_str),
        Some("synthetic-key")
    );
    assert!(calls[0].skip_remote_signing);
}
#[test]
fn real_order_formatter_and_terminal_place_use_contract_quantity() {
    let h = harness(options(), |wire| {
        wire.ok(
            "POST",
            "/dapi/v1/order",
            json!({"orderId":9007199254740993_u64}),
        )
    });
    seed(&h);
    let opts = from_json(
        &json!({"symbol":"BTCUSD_PERP","side":"buy","type":"limit","amount":2,"price":5000.16}),
    );
    let ids = call(&h, "placeOrder", vec![opts]);
    assert!(
        ids.get("0")
            .unwrap()
            .strict_equals(&Js::str("9007199254740993")),
        "bigint transform must run before binary64 parsing"
    );
    let calls = h.http.calls("/dapi/v1/order");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].url.contains("quantity=2"));
    assert!(calls[0].url.contains("positionSide=BOTH"));
    assert!(calls[0].url.contains("price=5000.2"));
    assert!(calls[0].url.contains("signature="));
    assert!(
        calls[0].body.is_none(),
        "signed data is moved to params before transmission"
    );
}
#[test]
fn malformed_position_envelope_reports_and_retains_existing_snapshot() {
    let h = harness(options(), |wire| {
        wire.ok(
            "GET",
            "/dapi/v1/positionRisk",
            json!({"code":0,"message":"wrong-shape"}),
        )
    });
    seed(&h);
    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = errors.clone();
    h.exchange.base().on(
        "error",
        std::rc::Rc::new(move |_, value| {
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let positions = call(&h, "fetchPositions", vec![]);
    assert_eq!(positions.get("length").unwrap().as_f64(), Some(0.0));
    assert!(h.http.calls("/dapi/v1/positionRisk").len() == 1);
    // The source handles this malformed response in fetchPositions' catch, rather
    // than allowing an object to masquerade as an empty successful array.
    let errors = errors.borrow();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].strict_equals(&Js::str("Exchange Error: Binance COIN-M /dapi/v1/positionRisk response malformed: expected array, received object(code,message) (code=0 wrong-shape)")), "actual event: {}", errors[0].to_display());
}

#[test]
#[ignore = "executes the complete maintained TypeScript venue with installed oracle dependencies"]
fn maintained_rest_class_matches_native_market_position_and_order_results() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-rest-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let h = harness(options(), |wire| {
        wire.ok("GET", "/dapi/v1/exchangeInfo", json!({"symbols":[raw_market(), {"contractType":"CURRENT_QUARTER","contractStatus":"TRADING"}]}));
        wire.ok(
            "GET",
            "/dapi/v2/leverageBracket",
            json!([{"symbol":"BTCUSD_PERP","brackets":[{"initialLeverage":75}]}]),
        );
        wire.ok("GET", "/dapi/v1/positionRisk", json!([{
            "symbol":"BTCUSD_PERP","positionAmt":"100","positionSide":"BOTH","entryPrice":"4000","markPrice":"5000","unRealizedProfit":"0.25","liquidationPrice":"2500","leverage":"10","isolatedMargin":"0","marginType":"cross"
        }]));
        wire.ok(
            "POST",
            "/dapi/v1/order",
            json!({"orderId":9007199254740993_u64}),
        );
    });
    seed(&h);
    let markets = call(&h, "fetchMarkets", vec![]);
    h.exchange
        .receiver()
        .get("store")
        .unwrap()
        .call_method(
            "update",
            vec![Js::object(vec![("markets", markets.clone())])],
        )
        .unwrap();
    let positions = call(&h, "fetchPositions", vec![]);
    // Observe native numeric outputs before the secondary JSON comparison.
    assert_eq!(
        positions
            .get("0")
            .unwrap()
            .get("contracts")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        2.0_f64.to_bits()
    );
    let ids = call(
        &h,
        "placeOrder",
        vec![from_json(
            &json!({"symbol":"BTCUSD_PERP","side":"buy","type":"limit","amount":2,"price":5000.16}),
        )],
    );
    let requests = h.http.calls("/dapi/v1/order");
    assert_eq!(requests.len(), 1);
    let query: std::collections::BTreeMap<_, _> = requests[0]
        .url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|pair| pair.split_once('=').unwrap())
        .collect();
    let native = Js::object(vec![
        ("markets", markets),
        ("positions", positions),
        ("ids", ids),
        (
            "order",
            Js::object(vec![
                ("quantity", Js::str(query["quantity"])),
                ("positionSide", Js::str(query["positionSide"])),
                ("price", Js::str(query["price"])),
            ]),
        ),
    ]);
    fn finite_numbers(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Number(number) => json!(number.as_f64().unwrap()),
            serde_json::Value::Array(rows) => rows.into_iter().map(finite_numbers).collect(),
            serde_json::Value::Object(fields) => fields
                .into_iter()
                .map(|(key, value)| (key, finite_numbers(value)))
                .collect(),
            other => other,
        }
    }
    let source = serde_json::from_slice(&source.stdout).unwrap();
    let native = safe_cex_exchange::js_runtime::value::to_json(&native).unwrap();
    assert_eq!(finite_numbers(native), finite_numbers(source));
    h.exchange.base().dispose().unwrap();
}
