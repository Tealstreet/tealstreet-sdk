//! Production BitgetExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

#[path = "support/account_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn get_account_signs_and_maps_parent() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    let account = h.drive(h.venue().get_account()).expect("getAccount");
    assert_eq!(account.user_id, "123");
    let calls = h.http.calls("/api/v2/spot/account/info");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].headers["ACCESS-KEY"], "fixture-key");
    assert!(
        calls[0].headers["ACCESS-SIGN"].is_string(),
        "request interceptor signed the call"
    );
}

#[test]
fn fetch_markets_maps_contracts() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].limits.leverage.max, 125.0);
    assert_eq!(
        h.http.calls(MARKETS)[0].params["productType"],
        "usdt-futures"
    );
}

#[test]
fn fetch_tickers_maps_known_markets() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].last, 50000.0);
    assert_eq!(tickers[0].bid, 49999.0);
}

#[test]
fn inherited_base_methods_reject_like_ts() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    for result in [
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .map(|_| ()),
        h.drive(h.venue().schedule_cancel(Some(Some(1000.0)))),
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), None))
            .map(|_| ()),
    ] {
        assert_eq!(
            result.expect_err("TS base rejects").message,
            "Not implemented"
        );
    }
}

fn with_positions(h: &Harness) {
    loaded(h);
    let positions = h.drive(h.venue().fetch_positions(None)).expect("positions");
    Store::update(
        &*h.store,
        StoreUpdate {
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .expect("store positions");
}

fn posted(h: &Harness, path: &str) -> Value {
    h.http
        .calls(path)
        .last()
        .unwrap_or_else(|| panic!("no POST {path}"))
        .data
        .clone()
}

#[test]
fn validate_account_reads_balance_through_detection() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(serde_json::to_value(&result.error).unwrap(), json!(""));
    assert_eq!(result.balance.expect("balance").free, 100.0);
    assert_eq!(
        h.http.calls("/api/v3/account/settings").len(),
        1,
        "account type detected first"
    );
}

#[test]
fn fetch_positions_maps_classic_rows() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].side, PositionSide::Long);
    assert_eq!(positions[0].contracts, 0.01);
    assert_eq!(positions[0].liquidation_price, 40000.0);
}

#[test]
fn fetch_orders_reads_normal_algo_and_trailing() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o1");
    assert_eq!(orders[0].price, 49000.0);
    let plans = h.http.calls("/api/v2/mix/order/orders-plan-pending");
    let kinds: Vec<_> = plans.iter().map(|r| r.params["planType"].clone()).collect();
    assert_eq!(kinds, vec![json!("profit_loss"), json!("track_plan")]);
}

#[test]
fn set_leverage_needs_a_stored_position_then_posts_and_refetches() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let missing = h
        .drive(h.venue().set_leverage("BTCUSDT".into(), 20.0, None))
        .expect_err("TS throws");
    assert_eq!(missing.message, "Position BTCUSDT not found");
    with_positions(&h);
    let before = h.http.calls("/api/v2/mix/position/all-position").len();
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 500.0, None))
        .expect("setLeverage");
    assert_eq!(
        posted(&h, "/api/v2/mix/account/set-leverage")["leverage"],
        json!(125.0),
        "clamped to market max"
    );
    assert_eq!(
        h.http.calls("/api/v2/mix/position/all-position").len(),
        before + 1,
        "positions refetched"
    );
}

#[test]
fn change_position_mode_posts_hedge_mode() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    h.drive(h.venue().change_position_mode(true, None))
        .expect("changePositionMode");
    assert_eq!(
        posted(&h, "/api/v2/mix/account/set-position-mode")["posMode"],
        "hedge_mode"
    );
}

#[test]
fn change_margin_type_posts_isolated_for_symbol() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    with_positions(&h);
    h.drive(h.venue().change_margin_type(true, Some("BTCUSDT".into())))
        .expect("changeMarginType");
    let body = posted(&h, "/api/v2/mix/account/set-margin-mode");
    assert_eq!(body["marginMode"], "isolated");
    assert_eq!(body["symbol"], "BTCUSDT");
}

#[test]
fn place_order_through_the_client_order_id_contract() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    with_positions(&h);
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 0.01, "price": 49000}),
    )
    .unwrap();
    let base = safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref());
    let ids = h
        .drive(Box::pin(base.place_order(h.exchange.as_ref(), opts)))
        .expect("placeOrder");
    assert_eq!(ids, vec!["p1".to_owned()]);
    let body = posted(&h, "/api/v2/mix/order/batch-place-order");
    assert_eq!(body["orderList"][0]["price"], "49000");
    assert_eq!(body["orderList"][0]["side"], "buy");

    let invalid: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 0.01, "price": 49000,
        "clientOrderId": "not valid because it is far longer than thirty two characters"
    }))
    .unwrap();
    let posts = h.http.calls("/api/v2/mix/order/batch-place-order").len();
    let error = h
        .drive(Box::pin(base.place_order(h.exchange.as_ref(), invalid)))
        .expect_err("contract rejects");
    assert!(error.message.contains("clientOrderId"), "{}", error.message);
    assert_eq!(
        h.http.calls("/api/v2/mix/order/batch-place-order").len(),
        posts,
        "nothing sent"
    );
}

#[test]
fn update_order_posts_modify() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("orders");
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": orders[0], "update": {"price": 48000}})).unwrap();
    let ids = h
        .drive(h.venue().update_order(update))
        .expect("updateOrder");
    assert_eq!(ids, vec!["o1".to_owned()]);
    assert_eq!(
        posted(&h, "/api/v2/mix/order/modify-order")["newPrice"],
        "48000"
    );
}

#[test]
fn cancel_orders_posts_batch_cancel() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("orders");
    h.drive(h.venue().cancel_orders(orders))
        .expect("cancelOrders");
    assert_eq!(
        posted(&h, "/api/v2/mix/order/batch-cancel-orders")["orderIdList"][0]["orderId"],
        "o1"
    );
}

#[test]
fn start_loads_public_data_and_logs_in_before_subscribing() {
    let h = harness(ExchangeName::Bitget, options(), classic);
    h.drive(h.venue().start()).expect("start");
    let loaded = h.store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers,
        "markets and tickers loaded"
    );
    let sockets = h.runtime.sockets.borrow();
    let private = sockets
        .iter()
        .find(|(url, _)| url == "wss://ws.bitget.com/v2/ws/private")
        .map(|(_, s)| s.clone())
        .expect("private socket dialed");
    assert!(
        sockets
            .iter()
            .any(|(url, _)| url == "wss://ws.bitget.com/v2/ws/public"),
        "public socket dialed"
    );
    drop(sockets);
    let sent = private.sent.borrow().clone();
    assert!(
        sent[0].contains("\"op\":\"login\""),
        "login first: {sent:?}"
    );
    assert!(
        !sent.iter().any(|m| m.contains("subscribe")),
        "no subscribe before the login ack: {sent:?}"
    );
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(r#"{"event":"login","code":0}"#.into()),
        ));
    h.runtime.advance(10.0);
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("subscribe")),
        "subscribes after login ack: {:?}",
        private.sent.borrow()
    );
}

fn signature_refusal() -> Value {
    json!({"code":"40009", "msg":"sign signature error", "data":null})
}

#[test]
fn signature_retry_non_2xx_replays_each_family_read_once() {
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        let path = "/api/v2/spot/account/info";
        let h = harness(name.clone(), options(), |http| {
            http.route("GET", path, 401.0, signature_refusal());
            http.ok(
                "GET",
                path,
                json!({"userId":"retried-account", "parentId":null}),
            );
        });
        let account = h
            .drive(h.venue().get_account())
            .expect("signature refusal must retry");
        assert_eq!(account.user_id, "retried-account", "{name:?}");
        let calls = h.http.calls(path);
        assert_eq!(calls.len(), 2, "signature_retry::non_2xx {name:?}");
        assert!(calls.iter().all(|r| r.headers["ACCESS-SIGN"].is_string()));
    }
}

#[test]
fn signature_retry_resolved_envelope_replays_each_family_read_once() {
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        let path = "/api/v2/spot/account/info";
        let h = harness(name.clone(), options(), |http| {
            http.route("GET", path, 200.0, signature_refusal());
            http.ok(
                "GET",
                path,
                json!({"userId":"resolved-retry", "parentId":null}),
            );
        });
        let account = h
            .drive(h.venue().get_account())
            .expect("resolved signature refusal must resend");
        assert_eq!(account.user_id, "resolved-retry", "{name:?}");
        assert_eq!(
            h.http.calls(path).len(),
            2,
            "signature_retry::resolved {name:?}"
        );
    }
}

#[test]
fn signature_retry_posts_run_signing_again_and_stop_after_second_refusal() {
    use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    struct RestoreClock(f64);
    impl Drop for RestoreClock {
        fn drop(&mut self) {
            VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + self.0);
        }
    }
    let _restore = RestoreClock(VIRTUAL_CLOCK.get_current_time_ms() - (VIRTUAL_CLOCK.now)());
    for status in [200.0, 401.0] {
        let path = "/api/v2/mix/account/set-position-mode";
        let h = harness(ExchangeName::Bitget, options(), |http| {
            http.route("POST", path, status, signature_refusal());
            classic(http);
        });
        let first_lower = (VIRTUAL_CLOCK.now)() + 90_000.0;
        VIRTUAL_CLOCK.sync_from_server_time(first_lower);
        h.http.advance_after_send_ms.set(17.0);
        h.http.advance_signing_clock_after_send_ms.set(60_000.0);
        h.drive(h.venue().change_position_mode(true, None)).unwrap();
        let calls = h.http.calls(path);
        assert_eq!(calls.len(), 2, "signature_retry::write {status}");
        for (index, call) in calls.iter().enumerate() {
            let timestamp = call.headers["ACCESS-TIMESTAMP"].as_f64().unwrap();
            let lower = if index == 0 {
                first_lower
            } else {
                calls[0].signing_clock_at_send + 60_000.0
            };
            assert!(
                (lower..=call.signing_clock_at_send).contains(&timestamp),
                "signature_retry::shared_clock_timestamp attempt {index}: {timestamp} outside [{lower}, {}]",
                call.signing_clock_at_send
            );
            let body = r#"{"productType":"usdt-futures","posMode":"hedge_mode"}"#;
            assert_eq!(call.data, serde_json::from_str::<Value>(body).unwrap());
            let timestamp = call.headers["ACCESS-TIMESTAMP"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!("{:.0}", call.headers["ACCESS-TIMESTAMP"].as_f64().unwrap())
                });
            let preimage = format!("{timestamp}POST{path}{body}");
            assert_eq!(
                call.headers["ACCESS-SIGN"],
                safe_cex_crypto::hmac_sha256(
                    b"fixture-secret",
                    preimage.as_bytes(),
                    safe_cex_crypto::Encoding::Base64
                ),
                "signature_retry::write_signature"
            );
        }
        assert_eq!(
            calls[0].data, calls[1].data,
            "signature_retry::write_payload"
        );
        let path = "/api/v2/spot/account/info";
        let h = harness(ExchangeName::Bitget, options(), |http| {
            http.route("GET", path, status, signature_refusal());
        });
        let result = h.drive(h.venue().get_account());
        if status != 200.0 {
            assert!(result.is_err());
        }
        assert_eq!(
            h.http.calls(path).len(),
            2,
            "signature_retry::one_retry_stamp {status}"
        );
    }
}

#[test]
fn signature_retry_does_not_replay_unrelated_http_or_resolved_errors() {
    for status in [200.0, 403.0] {
        let path = "/api/v2/spot/account/info";
        let h = harness(ExchangeName::Bitget, options(), |http| {
            http.route(
                "GET",
                path,
                status,
                json!({"code":"40017","msg":"unrelated refusal","data":null}),
            );
        });
        let _ = h.drive(h.venue().get_account());
        assert_eq!(
            h.http.calls(path).len(),
            1,
            "signature_retry::unrelated {status}"
        );
    }
}
