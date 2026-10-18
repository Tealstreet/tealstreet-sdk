//! Constructed production Orderly family over scripted HTTP and shared sockets.
#[path = "support/exchange_runtime.rs"]
mod support;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, from_json},
};
use serde_json::json;
use support::drive;

#[test]
fn public_books_and_candles_reach_each_family_binding() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use safe_cex_types::ExchangeName;
    use std::{cell::RefCell, rc::Rc};
    for name in [
        ExchangeName::Kodiakfi,
        ExchangeName::Mode,
        ExchangeName::Salsadex,
        ExchangeName::TealstreetDex,
        ExchangeName::What,
        ExchangeName::Woofi,
    ] {
        let (venue, rt, _, wire) = support::fixture_for(
            name.clone(),
            json!({
                "key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"
            }),
        );
        script(&wire);
        let errors = Rc::new(RefCell::new(Vec::new()));
        let capture = errors.clone();
        venue.base().on(
            "error",
            Rc::new(move |_, error| {
                capture.borrow_mut().push(error.clone());
                Ok(())
            }),
        );
        wire.responses.borrow_mut().insert("/v1/orderbook/PERP_BTC_USDC".into(), json!({"success":true,"data":{
            "timestamp":100,"bids":[{"price":99,"quantity":2}],"asks":[{"price":101,"quantity":4}]
        }}));
        drive(&rt, venue.start()).unwrap();
        // Startup fills the real 10-RPS limiter; advance its deterministic clock.
        let pump = || {
            for _ in 0..16 {
                rt.now.set(rt.now.get() + 100.0);
                rt.pump();
            }
        };
        pump();
        let socket = rt
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| !url.contains("private"))
            .unwrap()
            .1
            .clone();
        let books = Rc::new(RefCell::new(Vec::new()));
        let seen = books.clone();
        let close_book = venue
            .listen_order_book(
                "BTCUSDC".into(),
                Rc::new(move |book| seen.borrow_mut().push(book)),
            )
            .unwrap();
        let candles = Rc::new(RefCell::new(Vec::new()));
        let seen = candles.clone();
        let close_candle = venue
            .listen_ohlcv(
                serde_json::from_value(json!({"symbol":"BTCUSDC","interval":"1m"})).unwrap(),
                Rc::new(move |candle| seen.borrow_mut().push(candle)),
            )
            .unwrap();
        assert!(
            rt.sent
                .borrow()
                .iter()
                .any(|frame| frame.contains("PERP_BTC_USDC@orderbookupdate"))
        );
        assert!(
            rt.sent
                .borrow()
                .iter()
                .any(|frame| frame.contains("PERP_BTC_USDC@kline_1m"))
        );
        let frame = |topic: &str, ts: u64, data: serde_json::Value| {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    json!({"topic":topic,"ts":ts,"data":data}).to_string(),
                )));
            pump();
            assert!(
                rt.errors.borrow().is_empty(),
                "{name:?}: {:?}",
                rt.errors.borrow()
            );
        };
        frame(
            "PERP_BTC_USDC@kline_1m",
            103,
            json!({"startTime":1700000000000u64,"open":100,"high":104,"low":99,"close":103,"volume":9}),
        );
        assert_eq!(candles.borrow().len(), 1);
        assert_eq!(
            (
                candles.borrow()[0].timestamp,
                candles.borrow()[0].close,
                candles.borrow()[0].volume
            ),
            (1700000000.0, 103.0, 9.0)
        );
        frame(
            "PERP_BTC_USDC@orderbookupdate",
            99,
            json!({"bids":[[99,999]],"asks":[]}),
        );
        assert_eq!(
            books.borrow().len(),
            1,
            "{name:?} snapshot errors: {:?}; runtime errors: {:?}",
            errors.borrow(),
            rt.errors.borrow(),
        );
        assert_eq!(
            books.borrow()[0].bids[0].amount,
            2.0,
            "buffered older update is discarded against snapshot timestamp100"
        );
        frame(
            "PERP_BTC_USDC@orderbookupdate",
            101,
            json!({"bids":[[99,3],[98,5]],"asks":[]}),
        );
        assert_eq!(books.borrow().len(), 2);
        assert_eq!(
            (
                books.borrow()[1].bids[0].amount,
                books.borrow()[1].bids[1].total
            ),
            (3.0, 8.0)
        );
        frame(
            "PERP_BTC_USDC@orderbookupdate",
            102,
            json!({"bids":[[99,0]],"asks":[[101,6]]}),
        );
        assert_eq!(books.borrow().len(), 3);
        assert_eq!(
            (
                books.borrow()[2].bids.len(),
                books.borrow()[2].bids[0].price,
                books.borrow()[2].asks[0].amount
            ),
            (1, 98.0, 6.0)
        );
        close_book().unwrap();
        close_candle().unwrap();
        assert!(
            rt.sent
                .borrow()
                .iter()
                .any(|frame| frame.contains("unsubscribe")
                    && frame.contains("PERP_BTC_USDC@orderbookupdate"))
        );
        frame(
            "PERP_BTC_USDC@orderbookupdate",
            104,
            json!({"bids":[[97,1]],"asks":[]}),
        );
        frame(
            "PERP_BTC_USDC@kline_1m",
            105,
            json!({"startTime":1700000060000u64,"open":100,"high":104,"low":99,"close":104,"volume":10}),
        );
        assert_eq!((books.borrow().len(), candles.borrow().len()), (3, 1));
        venue.dispose().unwrap();
    }
}

fn script(wire: &support::Wire) {
    let mut responses = wire.responses.borrow_mut();
    responses.insert(
        "/v1/client/info".into(),
        json!({"success":true,"data":{"account_id":"orderly-account","max_leverage":10}}),
    );
    responses.insert("/v1/public/info".into(),json!({"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","min_leverage":1,"base_imr":0.02,"max_leverage":50,"base_tick":0.001,"quote_tick":0.25,"base_min":0.001,"base_max":10}]}}));
    responses.insert("/v1/public/futures".into(),json!({"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","mark_price":102,"index_price":101,"last_price":102,"volume":50,"amount":5000,"open_interest":8,"est_funding_rate":0.0001,"last_funding_rate":0.0001,"next_funding_time":1700000100000u64,"24h_open":100,"24h_close":102,"24h_high":110,"24h_low":90,"24h_volume":50,"24h_amount":5000}]}}));
    responses.insert("/v1/positions".into(),json!({"success":true,"data":{"total_collateral_value":100,"free_collateral":70,"rows":[]}}));
    responses.insert(
        "/v1/orders".into(),
        json!({"success":true,"data":{"rows":[]}}),
    );
    responses.insert(
        "/v1/algo/orders".into(),
        json!({"success":true,"data":{"rows":[]}}),
    );
    responses.insert(
        "/v1/order".into(),
        json!({"success":true,"data":{"order_id":1234567,"status":"EDIT_SENT"}}),
    );
    responses.insert(
        "/v1/client/margin_mode".into(),
        json!({"success":true,"data":{"rows":[]}}),
    );
}
#[test]
fn constructed_exchange_starts_and_signs_a_real_account_read() {
    let (venue, rt, store, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    assert!(
        store.loaded().markets
            && store.loaded().tickers
            && store.loaded().positions
            && store.loaded().orders
    );
    assert_eq!(
        drive(&rt, venue.get_account()).unwrap().user_id,
        "orderly-account"
    );
    assert_eq!(store.markets()[0].symbol, "BTCUSDC");
    assert!(!rt.urls.borrow().is_empty());
    let requests = wire.requests.borrow();
    let read = requests
        .iter()
        .find(|r| r.url == "/v1/client/info")
        .unwrap();
    assert_eq!(
        read.headers.get("orderly-key").map(String::as_str),
        Some("ed25519:fixture-key")
    );
    venue.dispose().unwrap();
}
fn sent(wire: &support::Wire, method: &str, path: &str) -> bool {
    wire.requests
        .borrow()
        .iter()
        .any(|r| r.method.eq_ignore_ascii_case(method) && r.url.split('?').next() == Some(path))
}
fn take(wire: &support::Wire) {
    wire.requests.borrow_mut().clear();
}
#[test]
fn all_sixteen_methods_execute_the_source_bodies_on_the_constructed_exchange() {
    use safe_cex_types::{PlaceOrderOpts, UpdateOrderOpts};
    let (venue, rt, store, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    let raw = json!({"order_id":9001,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":95.25,"quantity":0.02,"total_executed_quantity":0,"visible_quantity":0.02,"reduce_only":false,"margin_mode":"CROSS","time_in_force":"GTC"});
    wire.responses.borrow_mut().insert(
        "/v1/orders".into(),
        json!({"success":true,"data":{"rows":[raw]}}),
    );
    wire.responses
        .borrow_mut()
        .insert("/v1/client/leverages".into(), json!({"success":true}));
    wire.responses
        .borrow_mut()
        .insert("/v1/client/margin_mode".into(), json!({"success":true}));
    // 1 source startup, Store initialization, real HTTP signing and socket assembly.
    drive(&rt, venue.start()).expect("start binding");
    assert!(
        store.loaded().markets
            && store.loaded().tickers
            && store.loaded().orders
            && store.loaded().positions
    );
    take(&wire);
    // 2 account.
    assert_eq!(
        drive(&rt, venue.get_account())
            .expect("get_account binding")
            .user_id,
        "orderly-account"
    );
    assert!(sent(&wire, "GET", "/v1/client/info"));
    take(&wire);
    // 3 validation.
    assert_eq!(
        drive(&rt, venue.validate_account())
            .expect("validate_account binding")
            .error
            .as_value(),
        &json!("")
    );
    assert!(sent(&wire, "GET", "/v1/client/info"));
    take(&wire);
    // 4 positions retain the TS-visible balance side effect.
    wire.responses.borrow_mut().insert("/v1/positions".into(),json!({"success":true,"data":{"total_collateral_value":123,"free_collateral":81,"rows":[]}}));
    let positions = drive(&rt, venue.fetch_positions(None)).expect("fetch_positions binding");
    assert_eq!(positions.len(), 1);
    assert!(positions.iter().all(|p| p.symbol == "BTCUSDC"));
    assert_eq!(store.balance().free.to_bits(), 81f64.to_bits());
    assert_eq!(store.balance().used.to_bits(), 42f64.to_bits());
    assert!(sent(&wire, "GET", "/v1/positions"));
    take(&wire);
    // 5 limit + algo order reads, including snake-case get-key conversion.
    let orders = drive(&rt, venue.fetch_orders(None)).expect("fetch_orders binding");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "9001");
    assert_eq!(orders[0].amount.to_bits(), 0.02f64.to_bits());
    assert!(sent(&wire, "GET", "/v1/orders") && sent(&wire, "GET", "/v1/algo/orders"));
    take(&wire);
    // 6 market axes are intentionally distinct.
    let markets = drive(&rt, venue.fetch_markets()).expect("fetch_markets binding");
    assert_eq!(markets[0].precision.price.to_bits(), 0.25f64.to_bits());
    assert_eq!(markets[0].precision.amount.to_bits(), 0.001f64.to_bits());
    assert!(sent(&wire, "GET", "/v1/public/info"));
    take(&wire);
    // 7 ticker.
    let tickers = drive(&rt, venue.fetch_tickers()).expect("fetch_tickers binding");
    assert_eq!(tickers[0].last.to_bits(), 102f64.to_bits());
    assert_eq!(tickers[0].percentage.to_bits(), 2f64.to_bits());
    assert!(sent(&wire, "GET", "/v1/public/futures"));
    take(&wire);
    // 8 inherited unsupported maximum leverage, base.ts:799.
    assert_eq!(
        drive(&rt, venue.fetch_symbol_max_leverage("BTCUSDC".into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(wire.requests.borrow().is_empty());
    // 9 leverage POST and Store metadata.
    drive(&rt, venue.set_leverage("BTCUSDC".into(), 5.4, None)).expect("set_leverage binding");
    assert!(sent(&wire, "POST", "/v1/client/leverages"));
    let request = wire
        .requests
        .borrow()
        .iter()
        .find(|r| r.url == "/v1/client/leverages")
        .unwrap()
        .clone();
    assert_eq!(
        request
            .body
            .unwrap()
            .untransformed()
            .unwrap()
            .clone()
            .into_value()
            .unwrap()["leverage"]
            .as_f64()
            .unwrap()
            .to_bits(),
        5f64.to_bits()
    );
    take(&wire);
    // 10 inherited unsupported mode change, base.ts:832.
    assert_eq!(
        drive(&rt, venue.change_position_mode(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(wire.requests.borrow().is_empty());
    // 11 margin cache changes after a confirmed POST.
    drive(&rt, venue.change_margin_type(true, Some("BTCUSDC".into())))
        .expect("change_margin_type binding");
    assert!(sent(&wire, "POST", "/v1/client/margin_mode"));
    take(&wire);
    // 12 one regular order honors the caller ID verbatim and keeps both precision axes.
    let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":0.012,"price":101.37,"clientOrderId":"caller-order-12"})).unwrap();
    assert_eq!(
        drive(&rt, venue.base().place_order(venue.as_ref(), opts)).expect("place_order binding"),
        ["1234567"]
    );
    let request = wire
        .requests
        .borrow()
        .iter()
        .find(|r| r.method == "post" && r.url == "/v1/order")
        .unwrap()
        .clone();
    let body = request
        .body
        .as_ref()
        .unwrap()
        .untransformed()
        .unwrap()
        .clone()
        .into_value()
        .unwrap();
    assert_eq!(body["client_order_id"], json!("caller-order-12"));
    assert_eq!(body["margin_mode"], json!("ISOLATED"));
    assert_eq!(body["order_tag"], json!("TEALSTREET"));
    assert_eq!(
        body["broker_id"],
        json!(safe_cex_venue_orderly::types::BROKER_ID)
    );
    assert_eq!(
        body["order_quantity"].as_f64().unwrap().to_bits(),
        0.012f64.to_bits()
    );
    assert_eq!(
        body["order_price"].as_f64().unwrap().to_bits(),
        101.25f64.to_bits()
    );
    take(&wire);
    // 13 regular amend keeps the reconciled ID and uses PUT without cancel/recreate.
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order":orders[0],"update":{"price":96.25}})).unwrap();
    assert_eq!(
        drive(&rt, venue.update_order(update)).expect("update_order binding"),
        ["9001"]
    );
    assert!(sent(&wire, "PUT", "/v1/order"));
    assert!(!sent(&wire, "DELETE", "/v1/order") && !sent(&wire, "POST", "/v1/order"));
    take(&wire);
    // 14 query cancellation.
    drive(&rt, venue.cancel_orders(orders)).expect("cancel_orders binding");
    assert!(sent(&wire, "DELETE", "/v1/order"));
    assert!(wire.requests.borrow().iter().any(|r| {
        r.url
            .starts_with("/v1/order?order_id=9001&symbol=PERP_BTC_USDC")
    }));
    take(&wire);
    // 15 and 16 inherited base.ts:989 and1138 rejections.
    assert_eq!(
        drive(&rt, venue.schedule_cancel(None)).unwrap_err().message,
        "Not implemented"
    );
    assert_eq!(
        drive(&rt, venue.fetch_public_trades("BTCUSDC".into(), None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(wire.requests.borrow().is_empty());
    venue.dispose().unwrap();
}
#[test]
fn private_stream_waits_for_the_auth_reply_before_subscriptions() {
    let (venue, rt, _, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    assert!(
        rt.sent
            .borrow()
            .iter()
            .any(|frame| serde_json::from_str::<serde_json::Value>(frame)
                .is_ok_and(|v| v["event"] == "auth")),
        "private open sends an actual auth frame; frames={:?}, errors={:?}, console={:?}, urls={:?}",
        rt.sent.borrow(),
        rt.errors.borrow(),
        rt.console.borrow(),
        rt.urls.borrow()
    );
    let subscribed = || {
        rt.sent
            .borrow()
            .iter()
            .filter_map(|frame| serde_json::from_str::<serde_json::Value>(frame).ok())
            .any(|v| v["event"] == "subscribe" && v["topic"] == "executionreport")
    };
    assert!(!subscribed(), "private channels wait for server auth");
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"event":"auth","success":false}).to_string(),
        )));
    rt.pump();
    for _ in 0..8 {
        rt.pump();
    }
    assert!(
        !subscribed(),
        "auth rejection sends no private subscriptions"
    );
    assert!(
        !rt.console
            .borrow()
            .iter()
            .chain(rt.errors.borrow().iter())
            .any(|message| message.contains("auth failed"))
    );
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"event":"auth","success":true}).to_string(),
        )));
    rt.pump();
    for _ in 0..8 {
        rt.pump();
    }
    assert!(
        subscribed(),
        "auth success dispatches the source subscription body"
    );
    venue.dispose().unwrap();
}
#[test]
fn definitive_auth_rejection_and_transient_statuses_stop_before_sockets() {
    for status in [401, 408, 429, 503] {
        let (venue, rt, _, wire) = support::fixture(
            json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
        );
        script(&wire);
        wire.statuses
            .borrow_mut()
            .insert("/v1/client/info".into(), status);
        wire.responses.borrow_mut().insert(
            "/v1/client/info".into(),
            json!({"message":"upstream unavailable","msg":"upstream unavailable"}),
        );
        let result = drive(&rt, venue.start());
        if status == 401 {
            result.expect("definitive source rejection returns without starting");
        } else {
            assert!(
                result.unwrap_err().message.contains("transiently"),
                "status-only classification for {status}"
            );
        }
        assert!(
            rt.urls.borrow().is_empty(),
            "failed validation precedes every socket dial"
        );
        venue.dispose().unwrap();
    }
}

#[test]
fn caller_id_preflight_matches_the_source_regex_before_http() {
    let (venue, rt, _, wire) =
        support::fixture(json!({"key":"key","secret":"11111111111111111111111111111111"}));
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    take(&wire);
    for invalid in [
        "".to_string(),
        "-leading".into(),
        "a".repeat(37),
        "a\n".into(),
        "a\r".into(),
        "a\u{2028}".into(),
        "a\u{2029}".into(),
        "😀".repeat(19),
    ] {
        let opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":invalid})).unwrap();
        let error = drive(&rt, venue.base().place_order(venue.as_ref(), opts)).unwrap_err();
        assert!(
            error.message.contains("clientOrderId"),
            "invalid caller id {invalid:?}: {}",
            error.message
        );
        assert!(!(venue
            .base()
            .settings
            .client_order_id_rule
            .as_ref()
            .unwrap()
            .is_valid)(&invalid));
        assert!(
            wire.requests.borrow().is_empty(),
            "caller ID rejected before HTTP"
        );
    }
    let rule = venue.base().settings.client_order_id_rule.as_ref().unwrap();
    for valid in ["a".repeat(36), "😀".repeat(18), "a-b c".into()] {
        assert!((rule.is_valid)(&valid), "valid caller id: {valid:?}");
    }
}

#[test]
fn signed_requests_verify_over_exact_wire_method_url_and_body() {
    use base64::Engine;
    use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    let (venue, rt, _, wire) = support::fixture(
        json!({"key":"key","secret":"11111111111111111111111111111111","applicationId":"account"}),
    );
    script(&wire);
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    drive(&rt, venue.get_account()).unwrap();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let requests = wire.requests.borrow();
    let request = &requests[0];
    // orderly.api.ts signs the synced timestamp, method, URL and optional body.
    let timestamp = &request.headers["orderly-timestamp"];
    let native_timestamp: f64 = timestamp.parse().unwrap();
    assert!(
        native_timestamp >= before && native_timestamp <= after,
        "signed timestamp comes from the synced virtual clock"
    );
    let preimage = format!("{timestamp}GET/v1/client/info");
    assert_eq!(request.headers["orderly-textsign"], preimage);
    assert_eq!(request.headers["orderly-account-id"], "account");
    let signature = base64::engine::general_purpose::URL_SAFE
        .decode(&request.headers["orderly-signature"])
        .unwrap();
    let public_key = [
        0x3b, 0x6a, 0x27, 0xbc, 0xce, 0xb6, 0xa4, 0x2d, 0x62, 0xa3, 0xa8, 0xd0, 0x2a, 0x6f, 0x0d,
        0x73, 0x65, 0x32, 0x15, 0x77, 0x1d, 0xe2, 0x43, 0xa6, 0x3a, 0xc0, 0x48, 0xa1, 0x8b, 0x59,
        0xda, 0x29,
    ];
    assert!(
        safe_cex_crypto::ed25519::verify(preimage.as_bytes(), &public_key, &signature).unwrap()
    );
    assert!(
        !safe_cex_crypto::ed25519::verify(b"wrong-method-or-path", &public_key, &signature)
            .unwrap()
    );
}

#[test]
fn each_family_constructor_preserves_its_source_order_tag_and_normalized_key() {
    use safe_cex_types::ExchangeName;
    for (name, tag) in [
        (ExchangeName::Kodiakfi, "TEALKODIAK"),
        (ExchangeName::Mode, "TEALMODE"),
        (ExchangeName::Salsadex, "TEALSALSA"),
        (ExchangeName::TealstreetDex, "TEALSTREET"),
        (ExchangeName::What, "TEALWHAT"),
        (ExchangeName::Woofi, "TEALSTREET"),
    ] {
        let (venue, rt, _, wire) = support::fixture_for(
            name.clone(),
            json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"account"}),
        );
        script(&wire);
        drive(&rt, venue.start()).unwrap();
        let opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":0.012,"price":101.37,"clientOrderId":"family-id"})).unwrap();
        drive(&rt, venue.base().place_order(venue.as_ref(), opts)).unwrap();
        let request = wire
            .requests
            .borrow()
            .iter()
            .find(|r| r.url == "/v1/order" && r.method == "post")
            .unwrap()
            .clone();
        assert_eq!(
            request
                .body
                .as_ref()
                .unwrap()
                .untransformed()
                .unwrap()
                .clone()
                .into_value()
                .unwrap()["order_tag"],
            json!(tag),
            "{name:?} source subclass"
        );
        assert_eq!(request.headers["orderly-key"], "ed25519:fixture-key");
        venue.dispose().unwrap();
    }
}

#[test]
fn disposal_updates_the_shared_flag_and_preserves_source_listeners() {
    use std::{cell::Cell, rc::Rc};
    let (venue, rt, _, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    for _ in 0..8 {
        rt.pump();
    }
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    venue.base().on(
        "fill",
        Rc::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    assert!(!venue.base().is_disposed());
    assert!(
        rt.sockets
            .borrow()
            .iter()
            .any(|(_, socket)| !socket.closed.get())
    );
    venue.dispose().unwrap();
    assert!(
        venue.base().is_disposed(),
        "source write reaches the shared disposal flag"
    );
    assert!(
        rt.sockets
            .borrow()
            .iter()
            .all(|(_, socket)| socket.closed.get()),
        "source disposes both sockets"
    );
    venue
        .base()
        .emit("fill", from_json(&json!({"source":"disposal witness"})))
        .unwrap();
    assert_eq!(calls.get(), 1, "source disposal retains emitter listeners");
}

#[test]
fn trait_cancel_all_reaches_both_endpoints_with_an_empty_store() {
    let (venue, rt, store, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    assert!(store.orders().is_empty());
    drive(&rt, venue.cancel_all_orders()).unwrap();
    assert!(
        sent(&wire, "DELETE", "/v1/orders"),
        "cancel-all reaches venue orders absent from Store"
    );
    assert!(sent(&wire, "DELETE", "/v1/algo/orders"));
    let calls = wire.requests.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].url, "/v1/orders");
    assert_eq!(calls[1].url, "/v1/algo/orders");
}

#[test]
fn trait_cancel_symbol_attempts_algo_after_a_normal_cancel_failure() {
    use std::{cell::RefCell, rc::Rc};
    let (venue, rt, _, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    wire.statuses.borrow_mut().insert("/v1/orders".into(), 400);
    wire.responses.borrow_mut().insert(
        "/v1/orders".into(),
        json!({"message":"normal cancellation denied"}),
    );
    let errors = Rc::new(RefCell::new(vec![]));
    let seen = errors.clone();
    venue.base().on(
        "error",
        Rc::new(move |_, e| {
            seen.borrow_mut().push(e.clone());
            Ok(())
        }),
    );
    drive(&rt, venue.cancel_symbol_orders("BTCUSDC".into())).unwrap();
    assert!(
        sent(&wire, "DELETE", "/v1/orders"),
        "symbol cancellation is venue-wide"
    );
    assert!(
        sent(&wire, "DELETE", "/v1/algo/orders"),
        "algo cancel continues after the ordinary cancel fails"
    );
    let calls = wire.requests.borrow();
    assert_eq!(calls.len(), 2);
    for call in calls.iter() {
        assert!(call.url.ends_with("?symbol=PERP_BTC_USDC"));
        assert!(
            call.params.is_none(),
            "signer consumes the query before transport"
        );
    }
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        errors.borrow()[0]
            .to_display()
            .contains("normal cancellation denied")
    );
}

fn private_topic_names() -> [safe_cex_types::ExchangeName; 6] {
    use safe_cex_types::ExchangeName::*;
    [Kodiakfi, Mode, Salsadex, TealstreetDex, What, Woofi]
}
fn private_frame(
    venue: &safe_cex_venue_orderly::native_exchange::OrderlyExchange,
    rt: &support::Runtime,
    frame: serde_json::Value,
) {
    assert_eq!(venue.base().get_private_sockets().len(), 1);
    for _ in 0..16 {
        rt.pump();
    }
    let socket = rt
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(frame.to_string()),
        ));
    for _ in 0..16 {
        rt.pump();
    }
    assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
}
fn acknowledge_private(
    venue: &safe_cex_venue_orderly::native_exchange::OrderlyExchange,
    rt: &support::Runtime,
) {
    private_frame(venue, rt, json!({"event":"auth","success":true}));
    for topic in ["executionreport", "algoexecutionreportv2", "position"] {
        assert!(
            rt.sent
                .borrow()
                .iter()
                .filter_map(|frame| serde_json::from_str::<serde_json::Value>(frame).ok())
                .any(|frame| frame["event"] == "subscribe" && frame["topic"] == topic)
        );
    }
}

fn private_execution_report_updates_orders_in_the_real_store_for_name(
    name: safe_cex_types::ExchangeName,
) {
    let (venue, rt, store, wire) = support::fixture_for(
        name,
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    acknowledge_private(&venue, &rt);
    private_frame(
        &venue,
        &rt,
        json!({"topic":"executionreport","data":{"orderId":777,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":97.5,"quantity":2,"totalExecutedQuantity":0,"reduceOnly":false}}),
    );
    let orders = store.orders();
    let actual = orders.iter().find(|o| o.id == "777").unwrap();
    assert_eq!(actual.symbol, "BTCUSDC");
    assert_eq!(actual.price, 97.5);
    assert_eq!(actual.remaining, 2.0);
    drop(orders);
    venue.dispose().unwrap();
}

#[test]
fn private_execution_report_updates_orders_in_the_real_store() {
    for name in private_topic_names() {
        private_execution_report_updates_orders_in_the_real_store_for_name(name);
    }
}

fn private_partial_execution_updates_store_and_emits_fill_for_name(
    name: safe_cex_types::ExchangeName,
) {
    use std::{cell::RefCell, rc::Rc};
    let (venue, rt, store, wire) = support::fixture_for(
        name,
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    acknowledge_private(&venue, &rt);
    let fills = Rc::new(RefCell::new(vec![]));
    let seen = fills.clone();
    venue.base().on(
        "fill",
        Rc::new(move |_, fill| {
            seen.borrow_mut().push(fill.clone());
            Ok(())
        }),
    );
    private_frame(
        &venue,
        &rt,
        json!({"topic":"executionreport","data":{"orderId":778,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"PARTIAL_FILLED","price":101.5,"quantity":2,"totalExecutedQuantity":0.5,"executedPrice":101.5,"executedQuantity":0.5,"reduceOnly":false}}),
    );
    let orders = store.orders();
    let actual = orders.iter().find(|o| o.id == "778").unwrap();
    assert_eq!(actual.filled, 0.5);
    assert_eq!(actual.remaining, 1.5);
    assert_eq!(fills.borrow().len(), 1);
    assert_eq!(fills.borrow()[0].get("id").unwrap().as_f64(), Some(778.0));
    assert!(
        fills.borrow()[0]
            .get("symbol")
            .unwrap()
            .strict_equals(&Js::str("BTCUSDC"))
    );
    assert_eq!(
        fills.borrow()[0].get("price").unwrap().as_f64(),
        Some(101.5)
    );
    assert_eq!(fills.borrow()[0].get("amount").unwrap().as_f64(), Some(0.5));
    drop(orders);
    venue.dispose().unwrap();
}

#[test]
fn private_partial_execution_updates_store_and_emits_fill() {
    for name in private_topic_names() {
        private_partial_execution_updates_store_and_emits_fill_for_name(name);
    }
}

fn private_position_report_updates_store_and_keeps_rest_account_balance_for_name(
    name: safe_cex_types::ExchangeName,
) {
    let (venue, rt, store, wire) = support::fixture_for(
        name,
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    acknowledge_private(&venue, &rt);
    assert_eq!(store.balance().total, 100.0);
    private_frame(
        &venue,
        &rt,
        json!({"topic":"position","data":{"positions":[{"symbol":"PERP_BTC_USDC","position_qty":"3","average_open_price":"100","mark_price":"102","margin_mode":"CROSS","leverage":"20","est_liq_price":"50"}]}}),
    );
    let positions = store.positions();
    let actual = positions.iter().find(|p| p.contracts == 3.0).unwrap();
    assert_eq!(actual.symbol, "BTCUSDC");
    assert_eq!(actual.entry_price, 100.0);
    assert_eq!(actual.unrealized_pnl, 6.0);
    // Orderly has no private balance topic: fetchPositions owns its REST balance write.
    assert_eq!(store.balance().total, 100.0);
    assert_eq!(store.balance().free, 70.0);
    drop(positions);
    venue.dispose().unwrap();
}

#[test]
fn private_position_report_updates_store_and_keeps_rest_account_balance() {
    for name in private_topic_names() {
        private_position_report_updates_store_and_keeps_rest_account_balance_for_name(name);
    }
}

fn private_algo_execution_report_updates_the_real_store_for_name(
    name: safe_cex_types::ExchangeName,
) {
    let (venue, rt, store, wire) = support::fixture_for(
        name,
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    drive(&rt, venue.start()).unwrap();
    acknowledge_private(&venue, &rt);
    private_frame(
        &venue,
        &rt,
        json!({"topic":"algoexecutionreportv2","data":{"algoOrderId":779,"rootAlgoOrderId":779,"symbol":"PERP_BTC_USDC","algoType":"TAKE_PROFIT","algoStatus":"NEW","side":"SELL","quantity":2,"triggerPrice":108.5,"totalExecutedQuantity":0,"reduceOnly":true}}),
    );
    let orders = store.orders();
    let actual = orders.iter().find(|o| o.id == "779").unwrap();
    assert_eq!(actual.symbol, "BTCUSDC");
    assert_eq!(actual.price, 108.5);
    assert_eq!(actual.amount, 2.0);
    assert_eq!(actual.parent_id.as_deref(), Some("779"));
    drop(orders);
    venue.dispose().unwrap();
}

#[test]
fn private_algo_execution_report_updates_the_real_store() {
    for name in private_topic_names() {
        private_algo_execution_report_updates_the_real_store_for_name(name);
    }
}

#[test]
fn trait_set_all_leverage_updates_account_once_and_clamps_without_positions() {
    let (venue, rt, store, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    wire.responses
        .borrow_mut()
        .insert("/v1/client/leverages".into(), json!({"success":true}));
    assert!(store.positions().is_empty());
    drive(&rt, venue.set_all_leverage(20.0)).unwrap();
    assert!(
        sent(&wire, "POST", "/v1/client/leverages"),
        "account leverage updates even with no positions"
    );
    drive(&rt, venue.set_all_leverage(20.0)).unwrap();
    assert_eq!(
        wire.requests.borrow().len(),
        1,
        "unchanged account leverage is skipped"
    );
    drive(&rt, venue.set_all_leverage(125.0)).unwrap();
    drive(&rt, venue.set_all_leverage(0.2)).unwrap();
    drive(&rt, venue.set_all_leverage(1.0)).unwrap();
    let requests = wire.requests.borrow();
    assert_eq!(requests.len(), 3);
    for (request, leverage) in requests.iter().zip([20.0, 100.0, 1.0]) {
        let payload = request
            .body
            .as_ref()
            .unwrap()
            .untransformed()
            .unwrap()
            .clone()
            .into_value()
            .unwrap();
        assert_eq!(payload["leverage"].as_f64(), Some(leverage));
        assert_eq!(payload["margin_mode"], "CROSS");
        assert!(payload.get("symbol").is_none());
    }
    drop(requests);
    assert_eq!(
        drive(&rt, venue.set_all_leverage(f64::NAN))
            .unwrap_err()
            .message,
        "Orderly leverage must be a finite number"
    );
    assert_eq!(wire.requests.borrow().len(), 3);
}

#[test]
fn trait_set_all_leverage_emits_and_swallows_failed_account_write() {
    use std::{cell::RefCell, rc::Rc};
    let (venue, rt, _, wire) = support::fixture(
        json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
    );
    script(&wire);
    wire.responses.borrow_mut().insert(
        "/v1/client/leverages".into(),
        json!({"success":false,"message":"account leverage denied"}),
    );
    let errors = Rc::new(RefCell::new(vec![]));
    let seen = errors.clone();
    venue.base().on(
        "error",
        Rc::new(move |_, error| {
            seen.borrow_mut().push(error.clone());
            Ok(())
        }),
    );
    drive(&rt, venue.set_all_leverage(20.0)).unwrap();
    assert_eq!(
        wire.requests.borrow().len(),
        1,
        "failed account leverage still attempted the venue write"
    );
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        errors.borrow()[0]
            .to_display()
            .contains("account leverage denied")
    );
    wire.responses
        .borrow_mut()
        .insert("/v1/client/leverages".into(), json!({"success":true}));
    drive(&rt, venue.set_all_leverage(20.0)).unwrap();
    assert_eq!(
        wire.requests.borrow().len(),
        2,
        "failed write did not advance the account leverage cache"
    );
}

#[test]
fn private_balance_topic_preserves_rest_owned_account_for_each_subclass() {
    for name in private_topic_names() {
        let (venue, rt, store, wire) = support::fixture_for(
            name,
            json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
        );
        script(&wire);
        drive(&rt, venue.start()).unwrap();
        acknowledge_private(&venue, &rt);
        let before = store.balance().clone();
        let orders = store.orders().to_vec();
        let positions = store.positions().to_vec();
        let fills = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let observed = fills.clone();
        venue.base().on(
            "fill",
            std::rc::Rc::new(move |_, _| {
                observed.set(observed.get() + 1);
                Ok(())
            }),
        );
        private_frame(
            &venue,
            &rt,
            json!({"topic":"balance","data":{
            "total_collateral_value":"999999","free_collateral":"888888"}}),
        );
        assert_eq!(
            *store.balance(),
            before,
            "source has no private balance handler"
        );
        assert_eq!(store.orders().to_vec(), orders);
        assert_eq!(store.positions().to_vec(), positions);
        assert_eq!(fills.get(), 0);
        venue.dispose().unwrap();
    }
}

fn ticker_frame_keeps_valid_rows(
    topic: &str,
    source: &str,
    target: &str,
    source_error: bool,
    inherited: bool,
) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    for name in private_topic_names() {
        let (venue, rt, store, wire) = support::fixture_for(
            name.clone(),
            json!({
                "key":"fixture-key", "secret":"11111111111111111111111111111111", "applicationId":"fixture-account"
            }),
        );
        script(&wire);
        drive(&rt, venue.start()).unwrap();
        let btc = store.tickers()[0].clone();
        let mut eth = btc.clone();
        eth.id = "PERP_ETH_USDC".into();
        eth.symbol = "ETHUSDC".into();
        let mut ltc = btc.clone();
        ltc.id = "PERP_LTC_USDC".into();
        ltc.symbol = "LTCUSDC".into();
        let mut ada = btc.clone();
        ada.id = "PERP_ADA_USDC".into();
        ada.symbol = "ADAUSDC".into();
        let unchanged_btc = serde_json::to_value(&btc).unwrap();
        let unchanged_ltc = serde_json::to_value(&ltc).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                tickers: Some(vec![btc, eth, ltc, ada]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let mut rows = vec![
            json!({"symbol":"PERP_BTC_USDC", "close":30,"amount":300,"volume":3,"bid":29,"ask":31,"price":30}),
            json!({"symbol":"PERP_ETH_USDC", "close":40,"amount":400,"volume":4,"bid":39,"ask":41,"price":40}),
            json!({"symbol":"PERP_LTC_USDC", "close":50,"amount":500,"volume":5,"bid":49,"ask":51,"price":50}),
            json!({"symbol":"PERP_ADA_USDC", "close":60,"amount":600,"volume":6,"bid":59,"ask":61,"price":60}),
        ];
        if inherited {
            rows[0]["symbol"] = json!("PERP_constructor_");
            rows[2]["symbol"] = json!("PERP_toString_");
        }
        rows[0][source] = serde_json::Value::Null;
        rows[2][source] = json!("123");
        if source_error {
            rows.push(serde_json::Value::Null);
        }
        for _ in 0..16 {
            rt.pump();
        }
        let socket = rt
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| !url.contains("private"))
            .unwrap()
            .1
            .clone();
        rt.console.borrow_mut().clear();
        rt.errors.borrow_mut().clear();
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"topic":topic,"data":rows}).to_string(),
            )));
        for _ in 0..16 {
            rt.pump();
        }
        let tickers = store.tickers();
        let projected = serde_json::to_value(&*tickers).unwrap();
        let expected_ada = rows[3][source].as_f64().unwrap();
        let expected_eth = rows[1][source].as_f64().unwrap();
        let btc = projected
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["symbol"] == "BTCUSDC")
            .unwrap();
        let eth = projected
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["symbol"] == "ETHUSDC")
            .unwrap();
        let ltc = projected
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["symbol"] == "LTCUSDC")
            .unwrap();
        let ada = projected
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["symbol"] == "ADAUSDC")
            .unwrap();
        assert_eq!(
            ada[target], expected_ada,
            "{name:?}/{topic}: later valid ADA row survives"
        );
        assert_eq!(
            *btc, unchanged_btc,
            "bad null row cannot partially update BTC"
        );
        assert_eq!(
            *ltc, unchanged_ltc,
            "bad string row cannot partially update LTC"
        );
        assert_eq!(
            eth[target], expected_eth,
            "{name:?}/{topic}: valid ETH sibling survives"
        );
        let reports = rt
            .errors
            .borrow()
            .iter()
            .chain(rt.console.borrow().iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if inherited {
            assert!(
                reports.is_empty(),
                "inherited targets remain source no-ops: {reports}"
            );
        } else if source_error {
            assert!(
                reports.contains("Cannot read properties of null (reading 'symbol')"),
                "{reports}"
            );
            assert!(
                !reports.contains("typed field"),
                "source error takes precedence: {reports}"
            );
        } else {
            assert!(
                reports.contains("BTCUSDC")
                    && reports.contains("LTCUSDC")
                    && reports.contains(target)
                    && reports.contains("null")
                    && reports.contains("\"123\""),
                "{name:?}/{topic}: {reports}"
            );
        }
        venue.dispose().unwrap();
    }
}
#[test]
fn ticker_rows_refuse_only_null_and_string_values() {
    ticker_frame_keeps_valid_rows("tickers", "close", "last", false, false);
}
#[test]
fn bbo_rows_refuse_only_null_and_string_values() {
    ticker_frame_keeps_valid_rows("bbos", "bid", "bid", false, false);
}
#[test]
fn mark_rows_refuse_only_null_and_string_values() {
    ticker_frame_keeps_valid_rows("markprices", "price", "mark", false, false);
}

#[test]
fn ticker_row_refusals_preserve_later_source_errors() {
    for (topic, source, target) in [
        ("tickers", "close", "last"),
        ("bbos", "bid", "bid"),
        ("markprices", "price", "mark"),
    ] {
        ticker_frame_keeps_valid_rows(topic, source, target, true, false);
    }
}

#[test]
fn ticker_row_admission_preserves_inherited_target_no_ops() {
    for (topic, source, target) in [
        ("tickers", "close", "last"),
        ("bbos", "bid", "bid"),
        ("markprices", "price", "mark"),
    ] {
        ticker_frame_keeps_valid_rows(topic, source, target, false, true);
    }
}
