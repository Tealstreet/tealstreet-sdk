#[path = "support/private_socket.rs"]
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod market_socket;
#[path = "support/adapter_runtime.rs"]
mod support;
use safe_cex_exchange::{
    capabilities::{BaseCapabilityMethod, is_base_capability_fallback},
    exchange::Exchange,
    socket_lifecycle::SocketSlot,
};
use safe_cex_types::{OrderSide, OrderType};
use safe_cex_venue_backpack::native_value::NativeValue;
use safe_cex_venue_backpack::{
    native_value::Value as Native, public_adapter::Throttle, public_control::JsString, rest_reads,
};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::ready;
fn units(s: &str) -> JsString {
    s.encode_utf16().collect()
}

#[test]
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn native_market_frames_reach_sdk_tickers_books_and_candles() {
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_exchange::{
        exchange::{BaseExchange, ExchangeSettings},
        websocket::WsEvent,
    };
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::{RestResponseBody, socket::SocketFrame};
    use std::sync::Arc;
    let runtime = Rc::new(support::Runtime::default());
    let wire = Arc::new(support::Wire::default());
    let store = Rc::new(DefaultStore::new());
    let inputs = safe_cex_venue_backpack::native_host::native_inputs(runtime.clone(), wire.clone());
    let owner = safe_cex_venue_backpack::adapter::BackpackExchange::new(
        BaseExchange::new(
            safe_cex_types::ExchangeName::Backpack,
            serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap(),
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        inputs.build_http,
        inputs.install_transport,
        inputs.runtime,
        inputs.date_parse,
    )
    .unwrap();
    let put = |path: &str, body: serde_json::Value| {
        wire.bodies.lock().unwrap().insert(
            path.into(),
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(body).unwrap(),
            ),
        );
    };
    put(
        "/api/v1/markets",
        json!([{"symbol":"BTC_USDC","marketType":"PERP","quantityIncrement":"0.25","priceIncrement":"0.125","minQuantity":"0.01","maxQuantity":"100","imfFunction":{"base":"0.05"}}]),
    );
    put(
        "/api/v1/tickers",
        json!([{"symbol":"BTC_USDC","lastPrice":"100","priceChangePercent":"0.05","volume":"2","quoteVolume":"200"}]),
    );
    put(
        "/api/v1/markPrices",
        json!([{"symbol":"BTC_USDC","markPrice":"101","indexPrice":"102","fundingRate":"0.0001","nextFundingTimestamp":7}]),
    );
    let markets = ready(&runtime, Exchange::fetch_markets(owner.as_ref())).unwrap();
    assert_eq!(markets[0].precision.price, 0.125);
    assert_eq!(markets[0].limits.leverage.max, 20.0);
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let tickers = ready(&runtime, Exchange::fetch_tickers(owner.as_ref())).unwrap();
    assert_eq!(
        (tickers[0].mark, tickers[0].index, tickers[0].percentage),
        (101.0, 102.0, 5.0)
    );
    Store::update(
        store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let socket = Rc::new(market_socket::Socket::default());
    socket.events.borrow_mut().push_back(WsEvent::Open);
    runtime.dial_sockets.borrow_mut().push(socket.clone());
    let public = owner.public_websocket.get().unwrap();
    ready(
        &runtime,
        public
            .base
            .create_websocket("ws://market-fixture".into(), vec![], None),
    )
    .unwrap();
    let pump = || {
        for _ in 0..12 {
            runtime.pump();
        }
    };
    pump();
    let books = Rc::new(RefCell::new(Vec::new()));
    let seen = books.clone();
    let snapshot = json!({"lastUpdateId":100,"bids":[["98","1"]],"asks":[["102","2"],["103","1"]]});
    put("/api/v1/depth", snapshot.clone());
    put("/api/v1/depth?symbol=BTC_USDC", snapshot);
    let close_book = Exchange::listen_order_book(
        owner.as_ref(),
        "BTC_USDC".into(),
        Rc::new(move |book| seen.borrow_mut().push(book)),
    )
    .unwrap();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let seen = candles.clone();
    let close_candle = Exchange::listen_ohlcv(
        owner.as_ref(),
        serde_json::from_value(json!({"symbol":"BTC_USDC","interval":"1m"})).unwrap(),
        Rc::new(move |candle| seen.borrow_mut().push(candle)),
    )
    .unwrap();
    pump();
    assert_eq!(books.borrow().len(), 1, "native snapshot callback");
    let frame = |stream: &str, data: serde_json::Value| {
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"stream":stream,"data":data}).to_string(),
            )));
        pump();
        assert!(
            runtime.errors.borrow().is_empty(),
            "{:?}",
            runtime.errors.borrow()
        );
    };
    frame(
        "ticker.BTC_USDC",
        json!({"e":"ticker","s":"BTC_USDC","c":"100.5","P":"0.02","v":"3","q":"301.5"}),
    );
    assert_eq!(
        (
            store.tickers().unwrap()[0].last,
            store.tickers().unwrap()[0].percentage
        ),
        (100.5, 2.0)
    );
    frame(
        "kline.1m.BTC_USDC",
        json!({"e":"kline","s":"BTC_USDC","t":"2024-01-01T00:00:00","o":"99","h":"103","l":"98","c":"101","v":"7"}),
    );
    assert_eq!(candles.borrow().len(), 1);
    assert_eq!(
        (
            candles.borrow()[0].timestamp,
            candles.borrow()[0].close,
            candles.borrow()[0].volume
        ),
        (1704067200000.0, 101.0, 7.0)
    );
    frame(
        "depth.BTC_USDC",
        json!({"e":"depth","s":"BTC_USDC","U":101,"u":101,"b":[["98","3"]],"a":[["102","0"]]}),
    );
    assert_eq!(books.borrow().len(), 2);
    assert_eq!(
        (
            books.borrow()[1].bids[0].amount,
            books.borrow()[1].bids[0].total,
            books.borrow()[1].asks[0].price
        ),
        (3.0, 3.0, 103.0)
    );
    let snapshot = json!({"lastUpdateId":102,"bids":[["97","8"]],"asks":[["104","2"]]});
    put("/api/v1/depth", snapshot.clone());
    put("/api/v1/depth?symbol=BTC_USDC", snapshot);
    frame(
        "depth.BTC_USDC",
        json!({"e":"depth","s":"BTC_USDC","U":103,"u":103,"b":[["98","4"]],"a":[]}),
    );
    assert_eq!(
        books.borrow().len(),
        3,
        "gap refetches and bridges the snapshot"
    );
    assert_eq!(
        (
            books.borrow()[2].bids[0].price,
            books.borrow()[2].bids[1].total
        ),
        (98.0, 12.0)
    );
    close_book().unwrap();
    close_candle().unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|s| s.contains("UNSUBSCRIBE") && s.contains("depth.BTC_USDC"))
    );
    frame(
        "kline.1m.BTC_USDC",
        json!({"e":"kline","s":"BTC_USDC","t":"2024-01-01T00:01:00","o":"99","h":"103","l":"98","c":"101","v":"7"}),
    );
    assert_eq!(candles.borrow().len(), 1);
    owner.dispose().unwrap();
}

#[test]
fn concrete_exchange_owns_api_and_sdk_bindings() {
    let (o, runtime, store, wire) = support::fixture();
    assert_eq!(
        o.account_leverage_limit.get().to_bits(),
        1.0f64.to_bits(),
        "CLASS native exchange leverage default"
    );
    assert_eq!(
        o.grafana_endpoints_every,
        safe_cex_venue_backpack::constants::GRAFANA_ENDPOINTS_EVERY
    );
    assert!(
        !is_base_capability_fallback(&o.capabilities().start, BaseCapabilityMethod::Start),
        "CLASS native startup capability binding"
    );
    assert!(
        !is_base_capability_fallback(
            &o.capabilities().place_order,
            BaseCapabilityMethod::PlaceOrder
        ),
        "CLASS native placement capability binding"
    );
    assert!(
        Rc::ptr_eq(
            o.base.get_socket(SocketSlot::Public).as_ref().unwrap(),
            &o.public_websocket.get().unwrap().base
        ),
        "CLASS native public socket slot"
    );
    assert!(
        Rc::ptr_eq(
            o.base.get_socket(SocketSlot::Private).as_ref().unwrap(),
            &o.private_websocket.get().unwrap().base
        ),
        "CLASS native private socket slot"
    );
    let errors = Rc::new(RefCell::new(vec![]));
    let out = errors.clone();
    o.base.on(
        "error",
        Rc::new(move |_, v| {
            out.borrow_mut().push(v.clone());
            Ok(())
        }),
    );
    wire.bodies.lock().unwrap().insert(
        "/api/v1/markets".into(),
        safe_cex_transport::RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                json!({"error":{"msg":"same-base"}}),
            )
            .unwrap(),
        ),
    );
    let e = ready(&runtime, o.fetch_markets()).err().unwrap();
    assert!(e.message.contains("same-base"), "CLASS native API error");
    assert_eq!(
        errors.borrow().len(),
        1,
        "CLASS native API/base emitter identity"
    );
    let mut opts = safe_cex_venue_backpack::order_actions::Options {
        symbol: units("BTC_USDC"),
        r#type: safe_cex_types::ModelField::Value(OrderType::Limit),
        side: safe_cex_types::ModelField::Value(OrderSide::Buy),
        amount: 1.25,
        price: Some(10.0),
        trigger_price: None,
        take_profit: Some(11.0),
        stop_loss: None,
        reduce_only: false.into(),
        time_in_force: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: Some(units("1")),
        ..Default::default()
    };
    let before = wire.requests.lock().unwrap().len();
    assert!(
        ready(&runtime, o.place_order(opts.clone()))
            .unwrap_err()
            .message
            .contains("plain Limit/Market"),
        "CLASS native caller ID shape rejection"
    );
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        before,
        "CLASS preflight before network"
    );
    opts.take_profit = None;
    let ids = ready(&runtime, o.place_order(opts)).unwrap();
    assert_eq!(
        ids,
        [units("native-id")],
        "CLASS native bound order placement"
    );
    assert_eq!(store.markets().unwrap()[0].symbol, "BTC_USDC");
    o.dispose().unwrap();
    assert!(o.base.is_disposed(), "CLASS native disposal");
}
#[test]
fn concrete_public_throttle_and_weak_listener_lifetime() {
    let (o, rt, store, _) = support::fixture();
    let p = o.public_websocket.get().unwrap().clone();
    let data = |last| {
        Native::from_prepared(json!({"s":"BTC_USDC","c":last,"P":"0.5","v":"8","q":"80"}).into())
    };
    p.ticker_handler.call(data("10")).unwrap();
    assert_eq!(
        store.tickers().unwrap()[0].last.to_bits(),
        10.0f64.to_bits(),
        "PUBLIC CLASS native ticker binding"
    );
    assert_eq!(
        store.tickers().unwrap()[0].percentage.to_bits(),
        50.0f64.to_bits(),
        "PUBLIC CLASS native percentage binding"
    );
    rt.now.set(100.0);
    p.ticker_handler.call(data("20")).unwrap();
    rt.now.set(1000.0);
    p.ticker_handler.call(data("30")).unwrap();
    assert_eq!(
        store.tickers().unwrap()[0].last.to_bits(),
        30.0f64.to_bits(),
        "PUBLIC CLASS native throttle leading value"
    );
    rt.now.set(2000.0);
    rt.pump();
    assert_eq!(
        store.tickers().unwrap()[0].last.to_bits(),
        30.0f64.to_bits(),
        "PUBLIC CLASS native stale trailing canceled"
    );
    p.ticker_handler.cancel();
    rt.now.set(2100.0);
    p.ticker_handler.call(data("40")).unwrap();
    assert_eq!(
        store.tickers().unwrap()[0].last.to_bits(),
        40.0f64.to_bits(),
        "PUBLIC CLASS native cancel resets timestamp"
    );
    let stop = p
        .listen_ohlcv(units("BTC_USDC"), units("1m"), Rc::new(|_| Ok(())))
        .unwrap();
    let weak = Rc::downgrade(&p);
    drop(p);
    drop(o);
    assert!(
        weak.upgrade().is_none(),
        "PUBLIC CLASS native stored listener weak ownership"
    );
    assert!(
        stop().is_err(),
        "PUBLIC CLASS detached cleanup visibly fails"
    );
}
#[test]
fn concrete_public_depth_ticker_throttle_uses_source_100ms_window() {
    let (o, rt, store, _) = support::fixture();
    let p = o.public_websocket.get().unwrap().clone();
    let depth = Rc::new(Throttle {
        owner: Rc::downgrade(&p),
        depth: true,
        last: Cell::new(None),
        pending: RefCell::new(None),
        task: RefCell::new(None),
    });
    let data =
        |bid: &str| Native::from_prepared(json!({"s":"BTC_USDC","b":[[bid,"1"]],"a":[]}).into());
    depth.call(data("10")).unwrap();
    assert_eq!(
        store.tickers().unwrap()[0].bid.to_bits(),
        10.0f64.to_bits(),
        "DEPTH leading"
    );
    rt.now.set(50.0);
    depth.call(data("15")).unwrap();
    assert_eq!(
        store.tickers().unwrap()[0].bid.to_bits(),
        10.0f64.to_bits(),
        "DEPTH inside window deferred"
    );
    rt.now.set(100.0);
    rt.pump();
    assert_eq!(
        store.tickers().unwrap()[0].bid.to_bits(),
        15.0f64.to_bits(),
        "DEPTH trailing fires at the source throttle(updateTickerFromDepth, 100) window"
    );
    rt.now.set(250.0);
    depth.call(data("20")).unwrap();
    assert_eq!(
        store.tickers().unwrap()[0].bid.to_bits(),
        20.0f64.to_bits(),
        "DEPTH leading after window"
    );
}
#[test]
fn concrete_private_handler_and_throttle_state() {
    let (o, rt, store, _) = support::fixture();
    let p = o.private_websocket.get().unwrap();
    for state in [&p.state.positions, &p.state.balance] {
        assert_eq!(
            state.throttle_ms.get().to_bits(),
            500.0f64.to_bits(),
            "PRIVATE CLASS native reconciliation interval"
        );
        assert_eq!(state.last.get().to_bits(), 0.0f64.to_bits());
        assert!(!state.scheduled.get());
        assert!(!state.pending.get());
    }
    assert!(p.state.orders_timeout.borrow().is_none());
    p.on_message(&safe_cex_transport::socket::SocketFrame::Text(
        "broken JSON".into(),
    ))
    .unwrap();
    assert!(
        rt.errors.borrow().is_empty(),
        "PRIVATE CLASS malformed JSON source null"
    );
    let raw=json!({"stream":"account.orderUpdate","data":{"e":"orderUpdate","s":"BTC_USDC","i":"private-id","S":"Bid","o":"Limit","p":"10","q":"3","z":"1","X":"New","E":1000000,"f":"GTC"}}).to_string();
    p.on_message(&safe_cex_transport::socket::SocketFrame::Text(raw))
        .unwrap();
    assert_eq!(
        store.orders().unwrap().len(),
        1,
        "PRIVATE CLASS native dispatcher order count"
    );
    let order = store.orders().unwrap()[0].clone();
    assert_eq!(order.id, "private-id", "PRIVATE CLASS native dispatcher ID");
    assert_eq!(
        order.amount.to_bits(),
        3.0f64.to_bits(),
        "PRIVATE CLASS native order amount"
    );
    assert_eq!(
        order.remaining.to_bits(),
        2.0f64.to_bits(),
        "PRIVATE CLASS native order remaining"
    );
    assert!(
        p.state.orders_timeout.borrow().is_some(),
        "PRIVATE CLASS native reconciliation timer bound"
    );
    o.dispose().unwrap();
}
#[test]
fn concrete_transport_preserves_successful_text() {
    let (o, runtime, _, wire) = support::fixture();
    wire.bodies.lock().unwrap().insert(
        "/api/v1/klines".into(),
        safe_cex_transport::RestResponseBody::Encoded(b"<html>no klines</html>".to_vec()),
    );
    let c = ready(
        &runtime,
        o.fetch_ohlcv(rest_reads::OHLCVOptions {
            symbol: units("BTC_USDC"),
            interval: units("1m"),
            from: None,
            to: None,
        }),
    )
    .unwrap();
    assert!(
        c.is_empty(),
        "CLASS native non-array text OHLCV returns empty"
    );
    o.dispose().unwrap();
}
#[test]
fn concrete_exchange_limits_to_ten_requests_per_second() {
    let (o, rt, _, wire) = support::fixture();
    for _ in 0..10 {
        ready(
            &rt,
            o.xhr.request(
                "GET",
                "/api/v1/time",
                safe_cex_num::JsJsonValue::Undefined,
                safe_cex_num::JsJsonValue::Undefined,
            ),
        )
        .unwrap();
    }
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        10,
        "CLASS independent native RPS floor"
    );
    let mut eleventh = o.xhr.request(
        "GET",
        "/api/v1/time",
        safe_cex_num::JsJsonValue::Undefined,
        safe_cex_num::JsJsonValue::Undefined,
    );
    assert!(
        rt.poll_checkpoint(eleventh.as_mut()).is_pending(),
        "CLASS native eleventh request throttled"
    );
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        10,
        "CLASS native throttle before transport"
    );
    rt.now.set(1000.0);
    ready(&rt, eleventh).unwrap();
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        11,
        "CLASS native RPS resumes after interval"
    );
    o.dispose().unwrap();
}

#[test]
fn shared_runtime_graph_reaches_backpack_throttled_ticker_without_projection() {
    use safe_cex_exchange::js_runtime::value::Js;
    use safe_cex_num::JsJsonValue;

    let (owner, runtime, store, _) = support::fixture();
    let socket = owner.public_websocket.get().unwrap();
    let data = Js::object(vec![
        ("s", Js::str("BTC_USDC")),
        ("c", Js::str("-0")),
        ("P", Js::Number(f64::NAN)),
        ("v", Js::Number(f64::INFINITY)),
        ("q", Js::Number(f64::NEG_INFINITY)),
    ]);
    let prior_percentage = store.tickers().unwrap()[0].percentage;
    assert!(data.field("P").unwrap().as_f64().unwrap().is_nan());
    socket.ticker_handler.call(data.clone()).unwrap();
    let ticker = store.tickers().unwrap()[0].clone();
    assert_eq!(ticker.last.to_bits(), (-0.0f64).to_bits());
    assert_eq!(ticker.percentage.to_bits(), prior_percentage.to_bits());
    assert_eq!(ticker.volume, f64::INFINITY);
    assert_eq!(ticker.quote_volume, f64::NEG_INFINITY);

    runtime.now.set(50.0);
    socket.ticker_handler.call(data.clone()).unwrap();
    assert!(
        socket
            .ticker_handler
            .pending
            .borrow()
            .as_ref()
            .unwrap()
            .same_reference(&data),
        "SHARED GRAPH throttle retains the caller's runtime object"
    );
    data.set("c", Js::Number(37.5)).unwrap();
    runtime.now.set(1000.0);
    runtime.pump();
    assert_eq!(
        store.tickers().unwrap()[0].last,
        37.5,
        "SHARED GRAPH deferred reader sees runtime mutation"
    );

    let child = Js::holey_array(vec![None, Some(Js::Number(-0.0))]);
    let input = Js::object(vec![("child", child.clone())]);
    let spread = Native::spread(&[input]).unwrap();
    assert!(spread.field("child").unwrap().same_reference(&child));
    assert!(
        matches!(&spread.prepared().unwrap(), JsJsonValue::Object(rows)
        if matches!(&rows[0].1, JsJsonValue::Array(items)
            if matches!(items[0], JsJsonValue::Undefined)
                && matches!(items[1], JsJsonValue::Number(n) if n.to_bits() == (-0.0f64).to_bits())))
    );
    let lone = Js::utf16(&[0xd800]);
    assert_eq!(lone.string().unwrap(), [0xd800]);
    assert!(
        lone.prepared().is_err(),
        "shared storage keeps the explicit UTF-8 projection refusal"
    );
}

#[test]
fn native_message_receiver_compares_utf16_without_coercion() {
    use safe_cex_venue_backpack::socket_events::MessageValue;
    let text = "account.orderUpdate";
    for (value, expected) in [
        (Native::str(text), true),
        (Native::str("account.positionUpdate"), false),
        (Native::utf16(&[0xd800]), false),
        (Native::Number(1.0), false),
        (Native::Bool(true), false),
        (Native::Null, false),
        (Native::Undefined, false),
        (Native::object(vec![("toString", Native::str(text))]), false),
    ] {
        assert_eq!(MessageValue::equals_string(&value, text), expected);
    }
    for text in ["", "日本語", "🦀", "é", "e\u{301}"] {
        let value = Native::str(text);
        assert!(MessageValue::equals_string(&value, text));
        assert!(!MessageValue::equals_string(&value, "different"));
    }
}

#[test]
fn native_field_receiver_forwards_own_cells_without_projection() {
    let child = Native::object(vec![
        ("text", Native::utf16(&[0xd800, 0xdc00, 0xdfff])),
        ("number", Native::Number(f64::NAN)),
    ]);
    let row = Native::object(vec![
        ("child", child.clone()),
        ("ownUndefined", Native::Undefined),
        ("negativeZero", Native::Number(-0.0)),
    ]);
    assert!(row.field("child").unwrap().same_reference(&child));
    assert!(row.field("ownUndefined").unwrap().is_undefined());
    assert!(row.has_own("ownUndefined"));
    assert!(row.field("absent").unwrap().is_undefined());
    assert!(!row.has_own("absent"));
    assert!(
        matches!(row.field("negativeZero").unwrap(), Native::Number(n) if n.to_bits() == (-0.0f64).to_bits())
    );
    assert_eq!(
        child.field("text").unwrap().string().unwrap(),
        [0xd800, 0xdc00, 0xdfff]
    );
    assert!(matches!(child.field("number").unwrap(), Native::Number(n) if n.is_nan()));
    assert!(row.field("toString").unwrap().is_undefined());
    for (value, kind) in [(Native::Null, "null"), (Native::Undefined, "undefined")] {
        assert_eq!(
            value.field("x").unwrap_err().message,
            format!("Cannot read properties of {kind} (reading 'x')")
        );
    }
    for value in [
        Native::Number(2.0),
        Native::Bool(false),
        Native::str("text"),
    ] {
        assert!(value.field("x").unwrap().is_undefined());
    }
}

#[test]
fn native_signing_record_alias_preserves_shared_ordered_cells() {
    use safe_cex_venue_backpack::signing::{BackpackSigningParams, SigningRecord};
    let value = Native::object(vec![
        ("text", Native::utf16(&[0xd800])),
        ("nan", Native::Number(f64::NAN)),
        ("infinity", Native::Number(f64::INFINITY)),
        ("undefined", Native::Undefined),
    ]);
    let record: SigningRecord<Native> = Rc::new(RefCell::new(vec![
        (vec![0xdfff], value.clone()),
        (vec![0xdfff], Native::Number(-0.0)),
    ]));
    let BackpackSigningParams::Object(shared) = BackpackSigningParams::object(record.clone())
    else {
        panic!("source object arm")
    };
    assert!(Rc::ptr_eq(&record, &shared));
    assert_eq!(
        shared
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>(),
        [vec![0xdfff], vec![0xdfff]]
    );
    assert!(shared.borrow()[0].1.same_reference(&value));
    assert!(matches!(shared.borrow()[1].1, Native::Number(n) if n.to_bits()==(-0.0f64).to_bits()));
    record.borrow_mut().push((vec![97], Native::Bool(true)));
    assert_eq!(shared.borrow().len(), 3);
    assert_eq!(
        shared.borrow()[0]
            .1
            .field("text")
            .unwrap()
            .string()
            .unwrap(),
        [0xd800]
    );
    assert!(matches!(shared.borrow()[0].1.field("nan").unwrap(), Native::Number(n) if n.is_nan()));
    assert!(
        matches!(shared.borrow()[0].1.field("infinity").unwrap(), Native::Number(n) if n==f64::INFINITY)
    );
    assert!(shared.borrow()[0].1.has_own("undefined"));
    assert!(
        shared.borrow()[0]
            .1
            .field("undefined")
            .unwrap()
            .is_undefined()
    );
}

#[test]
fn native_signing_record_container_decodes_ordered_utf16_pairs() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    use safe_cex_venue_backpack::signing::SigningRecord;
    let record: SigningRecord<Input> =
        serde_json::from_str("[[[55296],-0.0],[[55296],true]]").unwrap();
    let rows = record.borrow();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, [0xd800]);
    assert_eq!(rows[1].0, [0xd800]);
    assert_eq!(rows[0].1.as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    assert!(matches!(rows[1].1, Input::Bool(true)));
    for raw in ["null", "{}", "[[[65536],0]]", "[[[-1],0]]", "[[[97]]]"] {
        assert!(
            serde_json::from_str::<SigningRecord<Input>>(raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn native_typed_container_aliases_keep_shared_cells() {
    use safe_cex_num::JsJsonValue as J;
    use safe_cex_venue_backpack::{
        account_reads, api_factory, order_actions, orderbook_state as book, public_control,
        public_listeners,
    };
    let failure: account_reads::ValidationFailure = Rc::new(RefCell::new(J::Number(f64::NAN)));
    let other = failure.clone();
    assert!(Rc::ptr_eq(&failure, &other));
    assert!(matches!(*other.borrow(), J::Number(n) if n.is_nan()));
    *failure.borrow_mut() = J::Undefined;
    assert!(matches!(*other.borrow(), J::Undefined));

    let headers: api_factory::Headers = Rc::new(RefCell::new(vec![
        ("x-first".into(), J::Number(-0.0)),
        ("x-next".into(), J::Undefined),
    ]));
    let other = headers.clone();
    assert!(Rc::ptr_eq(&headers, &other));
    assert_eq!(
        other
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>(),
        ["x-first", "x-next"]
    );
    assert!(matches!(other.borrow()[0].1, J::Number(n) if n.to_bits()==(-0.0f64).to_bits()));
    headers.borrow_mut().push(("x-last".into(), J::Bool(true)));
    assert_eq!(other.borrow().len(), 3);

    let request: order_actions::Request = vec![
        ("price", order_actions::Field::Number(f64::NAN)),
        ("text", order_actions::Field::Text(vec![0xd800])),
        ("reduceOnly", order_actions::Field::Bool(true)),
    ];
    assert_eq!(
        request.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        ["price", "text", "reduceOnly"]
    );
    assert!(matches!(request[0].1, order_actions::Field::Number(n) if n.is_nan()));
    assert!(matches!(&request[1].1, order_actions::Field::Text(v) if v==&[0xd800]));

    let subscriptions: public_control::SubscriptionState =
        Rc::new(RefCell::new(public_control::Subscriptions::default()));
    let other = subscriptions.clone();
    assert!(Rc::ptr_eq(&subscriptions, &other));
    subscriptions.borrow_mut().insert(vec![0xd800]);
    assert!(other.borrow().contains(&[0xd800]));
    other.borrow_mut().remove(&[0xd800]);
    assert!(subscriptions.borrow().values().is_empty());

    let state: book::StateRef<bool, bool> = Rc::new(RefCell::new(book::create_order_book_state()));
    let other = state.clone();
    assert!(Rc::ptr_eq(&state, &other));
    state.borrow_mut().updates.extend([true, false]);
    state.borrow_mut().last_update_id = -0.0;
    assert_eq!(other.borrow().updates, [true, false]);
    assert_eq!(other.borrow().last_update_id.to_bits(), (-0.0f64).to_bits());
    let handlers: book::TerminalHandlers<bool> = state.borrow().terminal_handlers.clone();
    let other = handlers.clone();
    assert!(Rc::ptr_eq(&handlers, &other));
    let handler = Rc::new(true);
    handlers.borrow_mut().insert(handler.clone());
    assert!(Rc::ptr_eq(other.borrow().iter().next().unwrap(), &handler));
    other.borrow_mut().remove(&handler);
    assert!(handlers.borrow().is_empty());

    let listener: public_listeners::State<bool> =
        Rc::new(RefCell::new(book::create_order_book_state()));
    let other = listener.clone();
    assert!(Rc::ptr_eq(&listener, &other));
    listener.borrow_mut().is_terminal = true;
    assert!(other.borrow().is_terminal);
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let terminal = Rc::new(public_listeners::Terminal {
        call: Rc::new(move || {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    });
    listener
        .borrow()
        .terminal_handlers
        .borrow_mut()
        .insert(terminal);
    (other
        .borrow()
        .terminal_handlers
        .borrow()
        .iter()
        .next()
        .unwrap()
        .call)()
    .unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn native_data_container_aliases_preserve_holes_keys_and_mutation() {
    use safe_cex_types::{OrderBook, OrderBookOrders};
    use safe_cex_venue_backpack::{
        order_utils, orderbook_state, private_auth, public_control, public_listeners,
        startup_recovery,
    };
    let text: public_control::JsString = vec![0xd800, 0xdc00, 0xdfff];
    assert_eq!(text, [0xd800, 0xdc00, 0xdfff]);
    let value = Native::object(vec![("nan", Native::Number(f64::NAN))]);
    let requests: order_utils::BatchRequests<Native> = Rc::new(RefCell::new(vec![value.clone()]));
    let other = requests.clone();
    assert!(Rc::ptr_eq(&requests, &other));
    assert!(other.borrow()[0].same_reference(&value));
    requests.borrow_mut().push(Native::Undefined);
    assert!(other.borrow()[1].is_undefined());
    let map: startup_recovery::NativeMap<Native> = Rc::new(RefCell::new(vec![
        (vec![0xd800], value.clone()),
        (vec![0xd800], Native::Undefined),
    ]));
    let other = map.clone();
    assert!(Rc::ptr_eq(&map, &other));
    assert_eq!(
        other
            .borrow()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>(),
        [vec![0xd800], vec![0xd800]]
    );
    assert!(other.borrow()[0].1.same_reference(&value));
    map.borrow_mut().pop();
    assert_eq!(other.borrow().len(), 1);
    let array = Native::holey_array(vec![
        None,
        Some(Native::Null),
        Some(Native::Undefined),
        Some(value.clone()),
    ]);
    let other = array.clone();
    assert!(array.same_reference(&other));
    assert!(!other.has_own("0"));
    assert!(matches!(other.get("1").unwrap(), Native::Null));
    assert!(other.get("2").unwrap().is_undefined());
    assert!(other.get("3").unwrap().same_reference(&value));
    array.call_method("pop", vec![]).unwrap();
    assert_eq!(other.get("length").unwrap().to_number().unwrap(), 3.0);

    let streams: public_control::Streams = Rc::new(RefCell::new(vec![None, Some(vec![0xd800])]));
    let other = streams.clone();
    assert!(Rc::ptr_eq(&streams, &other));
    assert_eq!(other.borrow()[1], Some(vec![0xd800]));
    streams.borrow_mut().push(Some(vec![97]));
    assert_eq!(other.borrow().len(), 3);
    let depth: orderbook_state::DepthLevels = Rc::new(RefCell::new(vec![[vec![0xd800], vec![49]]]));
    let other = depth.clone();
    assert!(Rc::ptr_eq(&depth, &other));
    depth.borrow_mut()[0][1].push(50);
    assert_eq!(other.borrow()[0], [vec![0xd800], vec![49, 50]]);
    let signature: private_auth::AuthSignature =
        Rc::new(RefCell::new([vec![0xd800], vec![], vec![97], vec![98]]));
    let other = signature.clone();
    assert!(Rc::ptr_eq(&signature, &other));
    let snapshot = private_auth::AuthMessage::snapshot(&[], &signature);
    signature.borrow_mut()[0].push(99);
    assert_eq!(snapshot.signature[0], [0xd800]);
    assert_eq!(other.borrow()[0], [0xd800, 99]);
    let book: public_listeners::Book = Rc::new(RefCell::new(OrderBook {
        bids: vec![OrderBookOrders {
            price: f64::NAN,
            amount: f64::INFINITY,
            total: -0.0,
        }],
        asks: vec![],
    }));
    let other = book.clone();
    assert!(Rc::ptr_eq(&book, &other));
    assert!(other.borrow().bids[0].price.is_nan());
    assert_eq!(other.borrow().bids[0].amount, f64::INFINITY);
    assert_eq!(other.borrow().bids[0].total.to_bits(), (-0.0f64).to_bits());
    book.borrow_mut().bids.clear();
    assert!(other.borrow().bids.is_empty());
}

#[test]
fn native_callable_aliases_preserve_arguments_results_and_shared_effects() {
    use safe_cex_types::{OrderBook, OrderBookOrders};
    use safe_cex_venue_backpack::{
        api_factory as api, private_events, public_listeners as listeners, socket_events,
        startup_recovery,
    };
    let error = Native::object(vec![("message", Native::utf16(&[0xd800]))]);
    let data = Native::object(vec![("number", Native::Number(-0.0))]);
    let original = data.clone();
    let failure = error.clone();
    let ticker: socket_events::TickerCallback<Native, Native> = Rc::new(move |row| {
        assert!(row.same_reference(&original));
        Err(failure.clone())
    });
    assert!(ticker(&data).unwrap_err().same_reference(&error));
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let failure = error.clone();
    let timeout: private_events::TimeoutCallback<Native> = Box::new(move || {
        observed.set(observed.get() + 1);
        Err(failure)
    });
    assert!(timeout().unwrap_err().same_reference(&error));
    let observed = calls.clone();
    let failure = error.clone();
    let recovery: startup_recovery::Callback<Native> = Box::new(move || {
        observed.set(observed.get() + 1);
        Err(failure)
    });
    assert!(recovery().unwrap_err().same_reference(&error));
    let observed = calls.clone();
    let action: listeners::Action<Native> = Rc::new(move || {
        observed.set(observed.get() + 1);
        Ok(())
    });
    let shared = action.clone();
    assert!(Rc::ptr_eq(&action, &shared));
    action().unwrap();
    shared().unwrap();
    assert_eq!(calls.get(), 4);

    let book: listeners::Book = Rc::new(RefCell::new(OrderBook {
        bids: vec![OrderBookOrders {
            price: -0.0,
            amount: f64::NAN,
            total: f64::INFINITY,
        }],
        asks: vec![],
    }));
    let original = book.clone();
    let callback: listeners::BookCallback<Native> = Rc::new(move |row| {
        assert!(Rc::ptr_eq(&row, &original));
        assert!(row.borrow().bids[0].amount.is_nan());
        row.borrow_mut().asks.push(OrderBookOrders {
            price: 1.0,
            amount: 2.0,
            total: -0.0,
        });
        Ok(())
    });
    callback(book.clone()).unwrap();
    assert_eq!(book.borrow().asks[0].total.to_bits(), (-0.0f64).to_bits());
    let bits = Rc::new(RefCell::new(vec![]));
    let observed = bits.clone();
    let candle: listeners::CandleCallback<Native> = Rc::new(move |row| {
        observed.borrow_mut().extend(
            [
                row.timestamp,
                row.open,
                row.high,
                row.low,
                row.close,
                row.volume,
            ]
            .map(f64::to_bits),
        );
        Ok(())
    });
    candle(rest_reads::Candle {
        timestamp: -0.0,
        open: f64::NAN,
        high: f64::INFINITY,
        low: f64::NEG_INFINITY,
        close: 1.0,
        volume: 2.0,
    })
    .unwrap();
    assert_eq!(
        *bits.borrow(),
        [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1.0, 2.0].map(f64::to_bits)
    );
    let (owner, runtime, _, _) = support::fixture();
    let config = Rc::new(api::Config {
        url: Some("/api/v1/time".into()),
        method: Some("GET".into()),
        params: Rc::new(safe_cex_num::JsJsonValue::Undefined),
        data: Rc::new(safe_cex_num::JsJsonValue::Undefined),
        headers: None,
        skip_remote_signing: false,
        extra: vec![],
    });
    let request: api::RequestHandler<Native> = Rc::new(|row| Box::pin(async move { Ok(row) }));
    let shared = request.clone();
    assert!(Rc::ptr_eq(&request, &shared));
    assert!(Rc::ptr_eq(
        &ready(&runtime, shared(config.clone())).unwrap(),
        &config
    ));
    let response = Rc::new(api::Response {
        data: safe_cex_types::retained_json::RetainedJsonInput::Text(vec![0xd800]),
        config: Some(config),
        status: Some(-0.0),
    });
    let success: api::ResponseHandler<Native> = Rc::new(|row| Box::pin(async move { Ok(row) }));
    let returned = ready(&runtime, success(response.clone())).unwrap();
    assert!(Rc::ptr_eq(&returned, &response));
    assert_eq!(returned.data.as_text().unwrap(), [0xd800]);
    assert_eq!(returned.status.unwrap().to_bits(), (-0.0f64).to_bits());
    let failure: api::FailureHandler<Native> = Rc::new(|error| Box::pin(async move { Err(error) }));
    assert!(
        ready(&runtime, failure(error.clone()))
            .err()
            .unwrap()
            .same_reference(&error)
    );
    let observed = calls.clone();
    let failure = error.clone();
    let fetcher: listeners::Fetcher<Native> = Rc::new(move || {
        observed.set(observed.get() + 1);
        let error = failure.clone();
        Box::pin(async move { Err(error) })
    });
    let task = fetcher();
    assert_eq!(calls.get(), 5);
    assert!(ready(&runtime, task).unwrap_err().same_reference(&error));
    owner.dispose().unwrap();
}

#[test]
fn native_future_and_prepared_aliases_retain_pending_state_and_values() {
    use safe_cex_venue_backpack::{api_factory, order_actions, public_listeners};
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    let value = Native::object(vec![
        ("opaque", Native::utf16(&[0xd800])),
        ("zero", Native::Number(-0.0)),
    ]);
    let release = Rc::new(Cell::new(false));
    let continuation = || {
        let value = value.clone();
        let release = release.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if release.get() {
                Poll::Ready(Ok(value.clone()))
            } else {
                Poll::Pending
            }
        })) as Pin<Box<dyn Future<Output = Result<Native, Native>>>>
    };
    let reply: rest_reads::Reply<Native, Native> = continuation();
    let read: rest_reads::Task<Native, Native> = continuation();
    let action: order_actions::Task<Native, Native> = continuation();
    let api: api_factory::Task<Native, Native> = continuation();
    let listen: public_listeners::Task<Native, Native> = continuation();
    for mut task in [reply, read, action, api, listen] {
        release.set(false);
        assert!(
            task.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        release.set(true);
        let Poll::Ready(Ok(output)) = task.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("released native continuation")
        };
        assert!(output.same_reference(&value));
        assert_eq!(output.field("opaque").unwrap().string().unwrap(), [0xd800]);
    }
    let opts: order_actions::Options = order_actions::Options {
        symbol: vec![0xd800],
        amount: -0.0,
        r#type: safe_cex_types::ModelField::Undefined,
        reduce_only: safe_cex_types::ModelField::Undefined,
        ..Default::default()
    };
    assert_eq!(opts.symbol, [0xd800]);
    assert_eq!(opts.amount.to_bits(), (-0.0f64).to_bits());
    assert!(matches!(opts.r#type, safe_cex_types::ModelField::Undefined));
    assert!(matches!(
        opts.reduce_only,
        safe_cex_types::ModelField::Undefined
    ));
    let opts: api_factory::Options =
        serde_json::from_value(json!({"key":"public","secret":"private","publicDataOnly":true}))
            .unwrap();
    assert_eq!(opts.key, "public");
    assert_eq!(opts.secret, "private");
    assert_eq!(opts.public_data_only, Some(true));
}

#[test]
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn native_http_callable_aliases_execute_the_production_factory() {
    use safe_cex_venue_backpack::{adapter, api_factory as api, native_host, native_http};
    let (_, runtime, _, wire) = support::fixture();
    let inputs = native_host::native_inputs(runtime.clone(), wire.clone());
    let build: native_http::BuildHttp = inputs.build_http;
    let install: native_http::InstallTransport = inputs.install_transport;
    let date: adapter::DateParser = inputs.date_parse;
    assert_eq!(
        date(units("1970-01-01T00:00:00.000Z")).unwrap().to_bits(),
        0.0f64.to_bits()
    );
    let options = Rc::new(RefCell::new(
        serde_json::from_value::<api::Options>(json!({"key":"","secret":"","publicDataOnly":true}))
            .unwrap(),
    ));
    let client = build(
        api::ClientConfig {
            base_url: "https://api.backpack.exchange",
            timeout: 20_000.0,
            headers: [
                ("content-type", "application/json"),
                ("accept", "application/json"),
            ],
            serialize: Rc::new(|_| Ok(String::new())),
        },
        options.clone(),
    )
    .unwrap();
    install(&client, options).unwrap();
    let result = ready(
        &runtime,
        client.execute(safe_cex_transport::RestRequest {
            url: "/api/v1/time".into(),
            method: "GET".into(),
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(result.status, 200);
    assert_eq!(wire.requests.lock().unwrap().len(), 1);
    assert_eq!(wire.requests.lock().unwrap()[0].timeout_ms, Some(20_000));
}
