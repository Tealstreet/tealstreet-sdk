//! Constructed production Orderly family over scripted HTTP and shared sockets.
#[path = "support/exchange_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use serde_json::json;
use support::drive;
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
        request.body.unwrap().untransformed().unwrap()["leverage"]
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
    let body = request.body.as_ref().unwrap().untransformed().unwrap();
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
            request.body.as_ref().unwrap().untransformed().unwrap()["order_tag"],
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
        .emit("fill", json!({"source":"disposal witness"}))
        .unwrap();
    assert_eq!(calls.get(), 1, "source disposal retains emitter listeners");
}
