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
