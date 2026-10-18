//! Every previously-unbound Exchange method, on the assembled production host.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
include!("support/private_binding_frames.rs");
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::Store;
use safe_cex_types::*;
use serde_json::{Value, json};
use support::{Harness, harness, options};
const MARKET: &str = "/openApi/swap/v2/quote/contracts";
const BALANCE: &str = "/openApi/swap/v3/user/balance";
const POSITIONS: &str = "/openApi/swap/v2/user/positions";
const ORDERS: &str = "/openApi/swap/v2/trade/openOrders";
const ORDER: &str = "/openApi/swap/v2/trade/order";
fn raw_order() -> Value {
    json!({"orderId":"9007199254740993","symbol":"BTC-USDT","side":"BUY","positionSide":"BOTH","type":"LIMIT","status":"NEW","origQty":"1","executedQty":"0","price":"100","stopPrice":"0","timeInForce":"GTC","reduceOnly":false,"closePosition":false})
}
fn fixture() -> Harness {
    fixture_options(options())
}

#[test]
fn stale_close_during_held_listen_key_rebuild_does_not_connect_twice() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::rc::Rc;
    const KEYS: &str = "/openApi/user/auth/userDataStream";
    for held_ms in [0.0, 3_100.0] {
        let h = fixture();
        h.drive(h.venue().start()).unwrap();
        for _ in 0..8 {
            h.runtime.pump();
        }
        let stale = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url.contains("listenKey="))
            .unwrap()
            .1
            .clone();
        let gate = Rc::new(support::Release::default());
        *h.http.held_request.borrow_mut() = Some(("POST".into(), KEYS.into(), gate.clone()));
        h.http
            .routes
            .borrow_mut()
            .retain(|(method, path, _)| method != "POST" || path != KEYS);
        h.http
            .route("POST", KEYS, 200.0, json!({"listenKey":"fresh-key"}));
        stale
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"e":"listenKeyExpired"}).to_string(),
            )));
        for _ in 0..8 {
            h.runtime.pump();
        }
        assert_eq!(
            h.http.calls(KEYS).len(),
            2,
            "initial key plus the held replacement"
        );
        assert!(
            stale.closed.get(),
            "source rebuild closes its detached socket"
        );
        let delivered = stale.delivered_events.get();
        stale.events.borrow_mut().push_back(WsEvent::Close {
            code: Some(1000),
            reason: "stale socket".into(),
        });
        h.runtime.pump();
        assert_eq!(
            stale.delivered_events.get(),
            delivered + 1,
            "stale Close was delivered through the real event loop"
        );
        if held_ms > 0.0 {
            h.runtime.advance(held_ms);
        }
        assert_eq!(
            h.http.calls(KEYS).len(),
            2,
            "removed close callback cannot request another key"
        );
        gate.set(true);
        for _ in 0..8 {
            h.runtime.pump();
        }
        h.runtime.advance(3_100.0);
        assert_eq!(h.http.calls(KEYS).len(), 2);
        let sockets = h.runtime.sockets.borrow();
        let private: Vec<_> = sockets
            .iter()
            .filter(|(url, _)| url.contains("listenKey="))
            .collect();
        assert_eq!(private.len(), 2, "exactly one replacement socket");
        assert!(private[1].0.contains("listenKey=fresh-key"));
        drop(sockets);
        done(&h);
    }
}
fn fixture_options(opts: ExchangeOptions) -> Harness {
    let h = harness(ExchangeName::Bingx, opts, |http| {
        http.ok("GET",MARKET,json!([{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"tradeMinLimit":0.01,"tradeMaxLimit":100.0,"quantityPrecision":3,"pricePrecision":2,"size":0.001,"tradeMinQuantity":0.01,"tradeMaxQuantity":100,"tradeMinUSDT":1,"maxLongLeverage":50,"maxShortLeverage":50}]));
        http.ok("GET",BALANCE,json!([{"asset":"USDT","userId":"account-1","balance":"10","availableMargin":"8","usedMargin":"2","unrealizedProfit":"1"}]));
        http.ok("GET","/openApi/swap/v2/quote/ticker",json!([{"symbol":"BTC-USDT","lastPrice":"100","priceChangePercent":"2","volume":"9","quoteVolume":"900"}]));
        http.ok("GET","/openApi/swap/v2/quote/premiumIndex",json!([{"symbol":"BTC-USDT","markPrice":"101","indexPrice":"100","lastFundingRate":"0.01","nextFundingTime":1700000100000_f64}]));
        http.ok(
            "GET",
            "/openApi/swap/v1/positionSide/dual",
            json!({"dualSidePosition":"false"}),
        );
        http.ok("POST", "/openApi/swap/v1/positionSide/dual", json!({}));
        http.ok("GET",POSITIONS,json!([{"symbol":"BTC-USDT","positionId":"p1","positionSide":"BOTH","isolated":false,"positionAmt":"1","availableAmt":"1","avgPrice":"90","unrealizedProfit":"10","realisedProfit":"0","initialMargin":"10","margin":"10","liquidationPrice":1,"leverage":10,"positionValue":"100","markPrice":"100","riskRate":"0.01","maxMarginReduction":"0","pnlRatio":"1","updateTime":1700000000000_f64}]));
        http.ok("GET", ORDERS, json!({"orders":[raw_order()]}));
        http.route(
            "POST",
            "/openApi/user/auth/userDataStream",
            200.0,
            json!({"listenKey":"fixture-listen-key"}),
        );
        http.ok("GET","/openApi/swap/v2/trade/leverage",json!({"longLeverage":10,"shortLeverage":10,"maxLongLeverage":50,"maxShortLeverage":50}));
        http.ok("POST", "/openApi/swap/v2/trade/leverage", json!({}));
        http.ok(
            "GET",
            "/openApi/swap/v2/trade/marginType",
            json!({"marginType":"CROSSED"}),
        );
        http.ok("POST", "/openApi/swap/v2/trade/marginType", json!({}));
        let mut wire_order = raw_order();
        wire_order["orderId"] = json!(9007199254740993_u64);
        http.ok("POST", ORDER, json!({"order":wire_order}));
        http.ok("DELETE", ORDER, json!({}));
        http.ok(
            "POST",
            "/openApi/swap/v1/trade/amend",
            json!({"orderId":"9007199254740993","symbol":"BTC-USDT","quantity":0.5}),
        );
    });
    Store::update(h.store.as_ref(),serde_json::from_value(json!({"markets":[{"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.01},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":50}}}]})).unwrap(),false).unwrap();
    h
}
fn order() -> Order {
    let mut wire = raw_order();
    wire["orderId"] = json!("9007199254740993");
    safe_cex_venue_bingx::private_events::map_order(&wire, &[], 1700000000000.0)
        .unwrap()
        .unwrap()
}
fn placed() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":1.2345,"price":100.127,"timeInForce":"GoodTillCancel","clientOrderId":"tscaller-1"})).unwrap()
}
fn done(h: &Harness) {
    h.exchange.dispose().unwrap();
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "host exceptions: {:?}",
        h.runtime.reports.borrow()
    );
}
#[test]
fn bound_start() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let l = h.store.loaded().clone();
    assert!(l.markets && l.tickers && l.balance && l.positions && l.orders);
    assert!(
        h.runtime
            .sockets
            .borrow()
            .iter()
            .any(|(u, _)| u.contains("fixture-listen-key"))
    );
    done(&h);
}
#[test]
fn bound_get_account() {
    let h = fixture();
    assert_eq!(
        h.drive(h.venue().get_account()).unwrap().user_id,
        "account-1"
    );
    done(&h);
}
#[test]
fn bound_validate_account() {
    let h = fixture();
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap()["balance"]["total"],
        10.0
    );
    done(&h);
}
#[test]
fn bound_fetch_positions() {
    let h = fixture();
    let before = h.store.positions().to_vec();
    let p = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].symbol, "BTCUSDT");
    assert_eq!(p[0].contracts, 1.0);
    assert_eq!(h.store.positions().to_vec(), before, "read must be pure");
    done(&h);
}
#[test]
fn bound_fetch_orders() {
    let h = fixture();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "9007199254740993");
    assert!(h.store.orders().is_empty(), "read must be pure");
    done(&h);
}
#[test]
fn bound_fetch_markets() {
    let h = fixture();
    let m = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].precision.amount, 0.001);
    assert_eq!(m[0].precision.price, 0.01);
    done(&h);
}
#[test]
fn bound_fetch_tickers() {
    let h = fixture();
    let t = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].mark, 101.0);
    done(&h);
}
#[test]
fn bound_fetch_symbol_max_leverage() {
    let h = fixture();
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(h.http.requests.borrow().is_empty());
    done(&h);
}
#[test]
fn bound_place_order() {
    let h = fixture();
    let ids = h
        .drive(h.exchange.base().place_order(h.exchange.as_ref(), placed()))
        .unwrap();
    assert_eq!(ids, ["9007199254740993"]);
    let calls = h.http.calls(ORDER);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].data["clientOrderId"], "tscaller-1");
    assert_eq!(
        calls[0].data["quantity"], 1.235,
        "wire quantity uses amount step"
    );
    assert_eq!(calls[0].data["price"], 100.13, "wire price uses price step");
    assert_eq!(calls[0].headers["X-SOURCE-KEY"], "Tealstreet");
    assert!(
        calls[0].data["signature"]
            .as_str()
            .is_some_and(|s| s.len() == 64)
    );
    done(&h);
}
#[test]
fn bound_set_leverage() {
    let h = fixture();
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 7.0, None))
        .unwrap();
    let calls = h.http.calls("/openApi/swap/v2/trade/leverage");
    assert!(
        calls
            .iter()
            .any(|r| r.method == "POST" && r.data["leverage"] == 7.0)
    );
    done(&h);
}
#[test]
fn bound_change_position_mode() {
    let h = fixture();
    h.drive(h.venue().change_position_mode(true, None)).unwrap();
    assert!(
        h.http
            .calls("/openApi/swap/v1/positionSide/dual")
            .iter()
            .any(|r| r.method == "POST" && r.data["dualSidePosition"] == "true")
    );
    done(&h);
}
#[test]
fn bound_change_margin_type() {
    let h = fixture();
    h.drive(h.venue().change_margin_type(true, Some("BTCUSDT".into())))
        .unwrap();
    assert!(
        h.http
            .calls("/openApi/swap/v2/trade/marginType")
            .iter()
            .any(|r| r.method == "POST" && r.data["marginType"] == "ISOLATED")
    );
    done(&h);
}
#[test]
fn bound_update_order() {
    let h = fixture();
    let input: UpdateOrderOpts =
        serde_json::from_value(json!({"order":order(),"update":{"amount":0.5}})).unwrap();
    let result = h.drive(h.venue().update_order(input)).unwrap();
    assert_eq!(result, ["9007199254740993"]);
    assert_eq!(h.http.calls("/openApi/swap/v1/trade/amend").len(), 1);
    done(&h);
}
#[test]
fn bound_cancel_orders() {
    let h = fixture();
    h.store.add_order(order()).unwrap();
    h.drive(h.venue().cancel_orders(vec![order()])).unwrap();
    assert_eq!(h.http.calls(ORDER)[0].method, "DELETE");
    assert_eq!(h.http.calls(ORDER)[0].params["orderId"], "9007199254740993");
    assert_eq!(h.store.orders()[0].status, OrderStatus::Canceled);
    done(&h);
}
#[test]
fn bound_schedule_cancel() {
    let h = fixture();
    assert_eq!(
        h.drive(h.venue().schedule_cancel(Some(Some(3.0))))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(h.http.requests.borrow().is_empty());
    done(&h);
}
#[test]
fn bound_fetch_public_trades() {
    let h = fixture();
    assert_eq!(
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), Some(10.0)))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(h.http.requests.borrow().is_empty());
    done(&h);
}

#[test]
fn signature_refusal_replays_serialized_write_once() {
    let _clock = ResetModelClock::acquire();
    for status in [200.0, 400.0] {
        let h = fixture();
        // VirtualClock ignores corrections below one second.
        h.http.advance_ms.set(2_000.0);
        *h.http
            .routes
            .borrow_mut()
            .iter_mut()
            .find(|(m, p, _)| m == "POST" && p == ORDER)
            .unwrap()
            .2
            .front_mut()
            .unwrap() = (
            status,
            json!({"code":100001,"msg":"signature verification failed"}),
        );
        h.http.ok("POST", ORDER, json!({"order":raw_order()}));
        let ids = h
            .drive(h.exchange.base().place_order(h.venue(), placed()))
            .unwrap();
        assert_eq!(ids, vec!["9007199254740993"]);
        let calls = h.http.calls(ORDER);
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].data["timestamp"], calls[1].data["timestamp"]);
        assert!(
            calls[1].data["timestamp"].as_f64().unwrap()
                - calls[0].data["timestamp"].as_f64().unwrap()
                >= 2_000.0,
            "retry signing must observe the deterministic two-second clock step"
        );
        assert_ne!(calls[0].data["signature"], calls[1].data["signature"]);
        for call in calls {
            assert!(call.data_raw.is_string());
            assert_eq!(call.data["clientOrderId"], "tscaller-1");
            assert_eq!(call.data["signature"].as_str().unwrap().len(), 64);
        }
        done(&h);
    }
}
#[test]
fn second_signature_refusal_is_not_replayed() {
    let h = fixture();
    *h.http
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(m, p, _)| m == "POST" && p == ORDER)
        .unwrap()
        .2
        .front_mut()
        .unwrap() = (400.0, json!({"code":100001}));
    assert!(
        h.drive(h.exchange.base().place_order(h.venue(), placed()))
            .is_err()
    );
    assert_eq!(h.http.calls(ORDER).len(), 2);
    assert_eq!(
        h.runtime.reports.borrow().as_slice(),
        &["Failed to switch margin type: Request failed with status code 400"]
    );
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}
#[test]
fn ambiguous_write_failure_is_not_replayed() {
    let h = fixture();
    *h.http
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(m, p, _)| m == "POST" && p == ORDER)
        .unwrap()
        .2
        .front_mut()
        .unwrap() = (503.0, json!({"code":500}));
    assert!(
        h.drive(h.exchange.base().place_order(h.venue(), placed()))
            .is_err()
    );
    assert_eq!(h.http.calls(ORDER).len(), 1);
    assert_eq!(
        h.runtime.reports.borrow().as_slice(),
        &["Failed to switch margin type: Request failed with status code 503"]
    );
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}

#[test]
fn retryable_read_gets_a_fresh_signature() {
    let _clock = ResetModelClock::acquire();
    let h = fixture();
    // VirtualClock ignores corrections below one second.
    h.http.advance_ms.set(2_000.0);
    let original = h
        .http
        .routes
        .borrow()
        .iter()
        .find(|(m, p, _)| m == "GET" && p == BALANCE)
        .unwrap()
        .2[0]
        .clone();
    *h.http
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(m, p, _)| m == "GET" && p == BALANCE)
        .unwrap()
        .2
        .front_mut()
        .unwrap() = (503.0, json!({"code":500}));
    h.http.route("GET", BALANCE, original.0, original.1);
    h.drive(h.venue().get_account()).unwrap();
    let calls = h.http.calls(BALANCE);
    assert_eq!(calls.len(), 2);
    assert_ne!(calls[0].params["timestamp"], calls[1].params["timestamp"]);
    let timestamp = |index: usize| match &calls[index].params["timestamp"] {
        Value::String(value) => value.parse::<f64>().unwrap(),
        value => value.as_f64().unwrap(),
    };
    assert!(
        timestamp(1) - timestamp(0) >= 2_000.0,
        "read retry signing must observe the deterministic two-second clock step"
    );
    assert_ne!(calls[0].params["signature"], calls[1].params["signature"]);
    done(&h);
}

#[test]
fn secondary_keys_read_while_primary_keys_write() {
    let mut opts = options();
    opts.secondary_key = Some("read-key".into());
    opts.secondary_secret = Some("read-secret".into());
    let h = fixture_options(opts);
    h.drive(h.venue().get_account()).unwrap();
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 7.0, None))
        .unwrap();
    assert_eq!(h.http.calls(BALANCE)[0].headers["X-BX-APIKEY"], "read-key");
    assert_ne!(
        h.http
            .calls("/openApi/swap/v2/trade/leverage")
            .iter()
            .find(|r| r.method == "POST")
            .unwrap()
            .headers["X-BX-APIKEY"],
        "read-key"
    );
    done(&h);
}
#[test]
fn invalid_caller_id_fails_before_any_request() {
    let h = fixture();
    let mut opts = placed();
    opts.client_order_id = Some("INVALID-ID".into());
    assert!(
        h.drive(h.exchange.base().place_order(h.venue(), opts))
            .is_err()
    );
    assert!(h.http.requests.borrow().is_empty());
    assert_eq!(
        h.runtime.reports.borrow().as_slice(),
        &["Exchange Error: BingX clientOrderId must be 1-40 characters with no uppercase letters"]
    );
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}

#[test]
fn compressed_binary_frames_reach_both_protocols() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    for bytes in [
        &[
            31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 11, 200, 204, 75, 7, 0, 195, 146, 231, 133, 4, 0, 0,
            0,
        ][..],
        &[120, 156, 11, 200, 204, 75, 7, 0, 3, 194, 1, 143],
    ] {
        for (_, socket) in h.runtime.sockets.borrow().iter() {
            socket
                .events
                .borrow_mut()
                .push_back(safe_cex_exchange::websocket::WsEvent::Message(
                    safe_cex_transport::socket::SocketFrame::Binary(bytes.to_vec()),
                ));
        }
        h.runtime.pump();
    }
    for (_, socket) in h.runtime.sockets.borrow().iter() {
        assert_eq!(
            socket
                .sent
                .borrow()
                .iter()
                .filter(|frame| frame.as_str() == "Pong")
                .count(),
            2,
            "gzip and zlib ping each get a protocol pong"
        );
    }
    done(&h);
}

#[test]
fn bound_listen_ohlcv() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let rows = observed.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            std::rc::Rc::new(move |c| rows.borrow_mut().push(c)),
        )
        .unwrap();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("ws-market-swap"))
        .unwrap()
        .1
        .clone();
    socket.events.borrow_mut().push_back(safe_cex_exchange::websocket::WsEvent::Message(safe_cex_transport::socket::SocketFrame::Text(json!({"dataType":"market.kline.BTC-USDT.1min.trade.utc+8","data":{"klineInfosVo":[{"time":1700000000000_f64,"open":"99","high":"103","low":"98","close":"101","volume":"7"}]}}).to_string())));
    h.runtime.pump();
    assert_eq!(observed.borrow().len(), 1);
    assert_eq!(observed.borrow()[0].close, 101.0);
    cleanup().unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("unsub") && frame.contains("market.kline.BTC-USDT"))
    );
    done(&h);
}
#[test]
fn bound_listen_order_book() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let rows = observed.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            std::rc::Rc::new(move |book| rows.borrow_mut().push(book)),
        )
        .unwrap();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("ws-market-swap"))
        .unwrap()
        .1
        .clone();
    socket.events.borrow_mut().push_back(safe_cex_exchange::websocket::WsEvent::Message(safe_cex_transport::socket::SocketFrame::Text(json!({"dataType":"market.depth.BTC-USDT.step0.level100","data":{"bids":[{"price":"99","volume":"4"}],"asks":[{"price":"101","volume":"6"}]}}).to_string())));
    h.runtime.pump();
    assert_eq!(observed.borrow().len(), 1);
    assert_eq!(observed.borrow()[0].bids[0].price, 99.0);
    assert_eq!(observed.borrow()[0].asks[0].amount, 6.0);
    cleanup().unwrap();
    assert_eq!(
        observed.borrow()[0].bids.len(),
        1,
        "the shared trait callback retains an owned depth snapshot after cleanup"
    );
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("unsub") && frame.contains("market.depth.BTC-USDT"))
    );
    done(&h);
}

#[test]
fn validation_keeps_http_envelope_hint() {
    let h = fixture();
    *h.http
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(m, p, _)| m == "GET" && p == BALANCE)
        .unwrap()
        .2
        .front_mut()
        .unwrap() = (403.0, json!({"code":100202,"msg":"permission denied"}));
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap()["error"],
        "permission denied"
    );
    assert_eq!(h.runtime.reports.borrow().len(), 1);
    assert!(h.runtime.reports.borrow()[0].contains("permission denied"));
    h.runtime.reports.borrow_mut().clear(); // validateAccount intentionally emits the returned error.
    done(&h);
}
#[test]
fn account_http_error_keeps_status_and_request() {
    let h = fixture();
    *h.http
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(m, p, _)| m == "GET" && p == BALANCE)
        .unwrap()
        .2
        .front_mut()
        .unwrap() = (403.0, json!({"code":100202,"msg":"permission denied"}));
    let error = h.drive(h.venue().get_account()).unwrap_err();
    assert!(error.is_axios_error);
    assert_eq!(error.response.as_ref().unwrap()["status"], 403.0);
    assert_eq!(
        error.response.as_ref().unwrap()["data"]["msg"],
        "permission denied"
    );
    assert_eq!(error.config.as_ref().unwrap()["url"], BALANCE);
    done(&h);
}

include!("support/binding_factory.rs");

#[test]
fn bound_batch_attempts_stop_loss_after_take_profit_rejection() {
    let h = fixture();
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.http
        .routes
        .borrow_mut()
        .retain(|(m, p, _)| !(m == "POST" && p == ORDER));
    let mut accepted = raw_order();
    accepted["orderId"] = json!("entry-accepted");
    h.http.ok("POST", ORDER, json!({"order":accepted}));
    h.http.route(
        "POST",
        ORDER,
        200.0,
        json!({"code":10002,"msg":"take profit rejected"}),
    );
    accepted["orderId"] = json!("stop-accepted");
    accepted["type"] = json!("STOP_MARKET");
    accepted["side"] = json!("SELL");
    accepted["stopPrice"] = json!("90");
    h.http.ok("POST", ORDER, json!({"order":accepted}));
    let entry = placed();
    let mut tp = entry.clone();
    tp.client_order_id = None;
    tp.r#type = OrderType::TakeProfit;
    tp.side = OrderSide::Sell;
    tp.reduce_only = Some(true);
    tp.amount = 0.5;
    tp.price = Some(110.0);
    let mut sl = tp.clone();
    sl.r#type = OrderType::StopLoss;
    sl.price = Some(90.0);
    let error = h
        .drive(
            h.venue()
                .base()
                .place_orders(h.venue(), vec![entry, tp, sl]),
        )
        .unwrap_err();
    assert_eq!(error.message, "take profit rejected");
    let calls = h.http.calls(ORDER);
    assert_eq!(
        calls.len(),
        3,
        "a rejected take profit must not suppress the stop loss"
    );
    assert_eq!(calls[1].data["type"], "TAKE_PROFIT_MARKET");
    assert_eq!(calls[2].data["type"], "STOP_MARKET");
    assert!(h.store.orders().iter().any(|o| o.id == "stop-accepted"));
    assert_eq!(h.http.calls("/openApi/swap/v1/positionSide/dual").len(), 1);
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}

#[test]
fn bound_batch_formats_every_order_before_sending_the_first_payload() {
    let h = fixture();
    let entry = placed();
    let mut stop = entry.clone();
    stop.client_order_id = None;
    stop.r#type = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = Some(90.0);
    stop.amount = 0.0;
    stop.close = Some(true);
    let error = h
        .drive(h.venue().base().place_orders(h.venue(), vec![entry, stop]))
        .unwrap_err();
    assert!(error.message.contains("without an open position"));
    assert!(
        h.http.calls(ORDER).is_empty(),
        "whole-batch formatting must finish before any submission"
    );
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}

fn signature_type_error(status: f64) {
    let h = fixture();
    *h.http
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(m, p, _)| m == "GET" && p == BALANCE)
        .unwrap()
        .2
        .front_mut()
        .unwrap() = (status, json!({"code":{"toString":0}}));
    let error = h.drive(h.exchange.get_account()).unwrap_err();
    assert_eq!(
        error.name, "TypeError",
        "BINGX-SIGNATURE-TYPEERROR native account request retains coercion kind for status {status}"
    );
    assert_eq!(error.message, "Cannot convert object to primitive value");
    assert_eq!(
        h.http.calls(BALANCE).len(),
        1,
        "BINGX-SIGNATURE-TYPEERROR coercion failure must not resend"
    );
    done(&h);
}
#[test]
fn resolved_signature_type_error_reaches_the_bingx_exchange() {
    signature_type_error(200.0);
}
#[test]
fn rejected_signature_type_error_reaches_the_bingx_exchange() {
    signature_type_error(400.0);
}

struct ResetModelClock {
    _guard: std::sync::MutexGuard<'static, ()>,
}
impl ResetModelClock {
    fn acquire() -> Self {
        // The three tests that change the process-wide signing clock cannot overlap.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        Self {
            _guard: LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }
}
impl Drop for ResetModelClock {
    fn drop(&mut self) {
        let clock = &safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
        clock.sync_from_server_time((clock.now)());
    }
}

#[test]
fn native_synced_clock_reaches_all_order_mapping_paths() {
    let _reset = ResetModelClock::acquire();
    let clock = &safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    clock.sync_from_server_time((clock.now)() + 40_000.0);
    let h = started_private_fixture();
    let before = clock.get_current_time_ms();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    let mut stamps = vec![("REST", orders[0].last_update)];
    private_frame(&h, private_order("NEW", "0"));
    stamps.push((
        "private frame",
        h.store
            .orders()
            .iter()
            .find(|o| o.id == "frame-order")
            .unwrap()
            .last_update,
    ));
    let ids = h
        .drive(h.venue().base().place_order(h.venue(), placed()))
        .unwrap();
    assert_eq!(ids.len(), 1);
    stamps.push((
        "placement",
        h.store
            .orders()
            .iter()
            .find(|o| o.id == ids[0])
            .unwrap()
            .last_update,
    ));
    let mut raw = raw_order();
    raw["clientOrderId"] = json!("original-clock-client");
    let original = safe_cex_venue_bingx::private_events::map_order(&raw, &[], 0.0)
        .unwrap()
        .unwrap();
    h.store.add_or_update_order(original.clone()).unwrap();
    const REPLACE: &str = "/openApi/swap/v1/trade/cancelReplace";
    h.http.ok("POST", REPLACE, json!({"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"replacement-clock-id"}));
    let update = serde_json::from_value(json!({"order":original,"update":{"price":110}})).unwrap();
    assert_eq!(
        h.drive(h.venue().update_order(update)).unwrap(),
        ["replacement-clock-id"]
    );
    stamps.push((
        "replacement",
        h.store
            .orders()
            .iter()
            .find(|o| o.id == "replacement-clock-id")
            .unwrap()
            .last_update,
    ));
    let after = clock.get_current_time_ms();
    let failures: Vec<_> = stamps
        .into_iter()
        .filter(|(_, stamp)| !(before..=after).contains(stamp))
        .collect();
    assert!(
        h.http
            .calls(ORDERS)
            .iter()
            .any(|r| r.params["signature"].is_string())
    );
    assert_eq!(h.http.calls(REPLACE).len(), 1);
    done(&h);
    assert!(
        failures.is_empty(),
        "all real order mapper callers must use synced clock: {failures:?}"
    );
}

#[test]
fn historical_fills_use_host_local_time_conversion() {
    use std::{cell::RefCell, process::Command, rc::Rc};
    for zone in ["Asia/Singapore", "America/New_York"] {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/history_timezone_oracle.mjs"
            ))
            .env("TZ", zone)
            .output()
            .expect("execute actual TS historical fill mapper");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(source["zone"], zone);
        let conversions = source["conversions"].as_array().unwrap().clone();
        assert_eq!(conversions.len(), 2);
        let observed = Rc::new(RefCell::new(Vec::new()));
        let recorded = observed.clone();
        let conversion = Rc::new(move |ms: f64| {
            recorded.borrow_mut().push(ms);
            conversions
                .iter()
                .find(|row| row["wall"].as_f64() == Some(ms))
                .expect("callback receives the ISO civil date in milliseconds")["utc"]
                .as_f64()
                .unwrap()
        });
        let h =
            support::harness_with_local_time(ExchangeName::Bingx, options(), conversion, |http| {
                http.ok(
                    "GET",
                    "/openApi/swap/v2/trade/fillHistory",
                    json!({"fill_history_orders": source["fills"]}),
                );
            });
        let fills = h
            .drive(h.venue().fetch_historical_fills(
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
            ))
            .unwrap();
        assert_eq!(fills.len(), 6, "all local and zoned source fills");
        for (fill, expected) in fills.iter().zip(source["executions"].as_array().unwrap()) {
            assert_eq!(
                fill.timestamp,
                expected["timestamp"].as_f64().unwrap(),
                "real binding historical fill timestamp matches TS in {zone}: {}",
                expected["orderId"]
            );
            let expected: Execution = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(*fill, expected, "complete source execution in {zone}");
        }
        assert_eq!(
            *observed.borrow(),
            source["conversions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["wall"].as_f64().unwrap())
                .collect::<Vec<_>>(),
            "only offset-less timestamps invoke the host, once per date"
        );
        done(&h);
    }
}

#[test]
fn undefined_reduce_only_amendment_omits_the_signed_wire_field_like_source() {
    use safe_cex_exchange::js_runtime::value::{Js, from_json, to_typed};
    use std::process::Command;

    let h = fixture();
    let mut source_order = serde_json::to_value(order()).unwrap();
    let info = source_order["info"].as_object_mut().unwrap();
    info.insert("clientOrderId".into(), json!("original-presence-client"));
    for key in ["reduceOnly", "ro", "R", "positionSide", "ps"] {
        info.remove(key);
    }
    let raw = from_json(&source_order);
    raw.define("reduceOnly", Js::Undefined).unwrap();
    let original: Order = to_typed(&raw).expect("native Order admits own-undefined reduceOnly");
    h.store.add_or_update_order(original.clone()).unwrap();
    const REPLACE: &str = "/openApi/swap/v1/trade/cancelReplace";
    h.http.ok("POST", REPLACE, json!({"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"presence-replacement"}));
    let mut request: UpdateOrderOpts =
        serde_json::from_value(json!({"order":order(),"update":{"price":110}})).unwrap();
    request.order = original;
    assert_eq!(
        h.drive(h.venue().update_order(request)).unwrap(),
        ["presence-replacement"]
    );
    let calls = h.http.calls(REPLACE);
    assert_eq!(calls.len(), 1);
    let wire = &calls[0].data;
    assert!(
        wire.get("reduceOnly").is_none(),
        "undefined reduceOnly must be omitted, including before HMAC: {wire}"
    );
    // Control only clock and UUID entropy; execute the maintained class, actual
    // createAPI signer, installed qs and real HMAC over the source payload.
    let input = json!({"order":source_order,"options":options(),"markets":&*h.store.markets(),"price":110,"timestamp":wire["timestamp"],"uuid":wire["clientOrderId"].as_str().unwrap().strip_prefix("ts").unwrap()});
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/completion_oracle.cjs"
        ))
        .arg("--undefined-reduce-only")
        .arg(input.to_string())
        .output()
        .expect("execute actual TypeScript amendment and signer");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        wire, &expected["body"],
        "complete signed wire body matches source"
    );
    assert!(
        !expected["preimage"]
            .as_str()
            .unwrap()
            .contains("reduceOnly")
    );
    done(&h);
}

fn asset_market(symbol: &str, base: &str) -> Value {
    json!({"symbol":symbol,"asset":base,"currency":"USDT","status":1,"quantityPrecision":"3","pricePrecision":"2","tradeMinLimit":"0.01","maxLongLeverage":75})
}

#[test]
fn mixed_market_asset_refusals_keep_fresh_siblings() {
    use std::{cell::RefCell, rc::Rc};
    for (field, wire_field) in [("base", "asset"), ("quote", "currency")] {
        for value in [
            Some(json!({"asset":"wrong"})),
            Some(json!(false)),
            Some(Value::Null),
            Some(json!(17)),
            None,
        ] {
            let h = fixture();
            h.http
                .routes
                .borrow_mut()
                .retain(|(method, path, _)| method != "GET" || path != MARKET);
            let mut bad = asset_market("BAD-USDT", "BAD");
            if let Some(value) = &value {
                bad[wire_field] = value.clone();
            } else {
                bad.as_object_mut().unwrap().remove(wire_field);
            }
            h.http.ok(
                "GET",
                MARKET,
                json!([
                    asset_market("BTC-USDT", "BTC"),
                    bad,
                    asset_market("ETH-USDT", "ETH")
                ]),
            );
            let errors = Rc::new(RefCell::new(Vec::new()));
            let capture = errors.clone();
            h.exchange.base().on(
                "error",
                Rc::new(move |_, error| {
                    capture.borrow_mut().push(error.clone());
                    Ok(())
                }),
            );
            let markets = h.drive(h.venue().fetch_markets()).unwrap();
            assert_eq!(
                markets
                    .iter()
                    .map(|m| m.symbol.as_str())
                    .collect::<Vec<_>>(),
                ["BTCUSDT", "ETHUSDT"],
                "bad {field} must not discard fresh siblings"
            );
            let expected = value
                .as_ref()
                .map_or_else(|| "undefined".into(), Value::to_string);
            let reports = errors.borrow();
            assert_eq!(reports.len(), 1, "one visible refusal per bad row");
            let report = String::from_utf16(reports[0].as_str().unwrap())
                .expect("market refusal is a scalar Unicode message");
            assert!(
                report.contains("BAD-USDT") && report.contains(&format!("{field}={expected}")),
                "{report}"
            );
            drop(reports);
            assert_eq!(h.runtime.reports.borrow().len(), 1);
            h.runtime.reports.borrow_mut().clear();
            done(&h);
        }
    }
}

#[test]
fn refresh_markets_commits_valid_rows_around_bad_assets() {
    let h = fixture();
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "GET" || path != MARKET);
    let mut bad = asset_market("BAD-USDT", "BAD");
    bad["asset"] = json!(false);
    bad["maxLongLeverage"] = json!({"toString":null});
    h.http.ok(
        "GET",
        MARKET,
        json!([
            asset_market("BTC-USDT", "BTC"),
            bad,
            asset_market("ETH-USDT", "ETH")
        ]),
    );
    h.drive(h.venue().refresh_markets()).unwrap();
    assert_eq!(
        h.store
            .markets()
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["BTCUSDT", "ETHUSDT"]
    );
    assert!(
        h.runtime
            .reports
            .borrow()
            .iter()
            .any(|line| line.contains("base=false"))
    );
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}

fn market_leverage_refusal_case(refresh: bool) {
    use std::{cell::RefCell, rc::Rc};
    for value in [
        json!("75"),
        json!({"leverage":75}),
        json!([75]),
        json!(true),
    ] {
        let h = fixture();
        h.http
            .routes
            .borrow_mut()
            .retain(|(method, path, _)| method != "GET" || path != MARKET);
        let mut bad = asset_market("BAD-USDT", "BAD");
        bad["maxLongLeverage"] = value.clone();
        h.http.ok(
            "GET",
            MARKET,
            json!([
                asset_market("BTC-USDT", "BTC"),
                bad,
                asset_market("ETH-USDT", "ETH")
            ]),
        );
        let errors = Rc::new(RefCell::new(Vec::new()));
        let capture = errors.clone();
        h.exchange.base().on(
            "error",
            Rc::new(move |_, error| {
                capture.borrow_mut().push(error.clone());
                Ok(())
            }),
        );
        let markets = if refresh {
            h.drive(h.venue().refresh_markets()).unwrap();
            h.store.markets().to_vec()
        } else {
            h.drive(h.venue().fetch_markets()).unwrap()
        };
        assert_eq!(
            markets
                .iter()
                .map(|m| m.symbol.as_str())
                .collect::<Vec<_>>(),
            ["BTCUSDT", "ETHUSDT"]
        );
        assert_eq!(markets[0].limits.leverage.max, 75.0);
        assert_eq!(markets[1].limits.leverage.max, 75.0);
        let errors = errors.borrow();
        assert_eq!(errors.len(), 1);
        let message = String::from_utf16(errors[0].as_str().unwrap()).unwrap();
        assert!(
            message.contains("BAD-USDT")
                && message.contains(&format!("limits.leverage.max={value}")),
            "{message}"
        );
        drop(errors);
        h.runtime.reports.borrow_mut().clear();
        done(&h);
    }
}
#[test]
fn fetch_market_leverage_refusals_keep_siblings() {
    market_leverage_refusal_case(false);
}
#[test]
fn refresh_market_leverage_refusals_keep_siblings() {
    market_leverage_refusal_case(true);
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("bingx");
    assert_eq!(source["cases"].as_array().unwrap().len(), 140);
    for name in [ExchangeName::Bingx] {
        let mut opts = options();
        opts.public_data_only = Some(true);
        let h = harness(name, opts, |_| {});
        let seed = ticker_coercion::seed("bingx");
        let markets = seed
            .iter()
            .map(|ticker| {
                serde_json::from_value(serde_json::json!({
            "id":ticker.id,"symbol":ticker.symbol,"base":"fixture","quote":"USDT","active":true,
            "precision":{"amount":0.01,"price":0.25},
            "limits":{"amount":{"min":0.01,"max":1000},"leverage":{"min":1,"max":20}}
        })).unwrap()
            })
            .collect();
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                markets: Some(markets),
                tickers: Some(seed.clone()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for slot in [SocketSlot::Public] {
            let socket = h.venue().base().get_socket(slot).unwrap();
            socket.connect_and_subscribe(false).unwrap();
            for _ in 0..8 {
                h.runtime.pump();
            }
            assert!(
                socket.is_connected(),
                "production bingx ticker socket connects"
            );
        }
        for case in source["cases"].as_array().unwrap() {
            Store::update(
                h.store.as_ref(),
                StoreUpdate {
                    tickers: Some(seed.clone()),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            let socket = h.venue().base().get_socket(SocketSlot::Public).unwrap();
            for frame in case["frames"].as_array().unwrap() {
                socket
                    .on_message(&SocketFrame::Text(frame.as_str().unwrap().into()))
                    .unwrap();
                h.runtime.pump();
            }
            ticker_coercion::assert_native(&h.store.tickers(), case);
        }
        assert!(
            h.runtime.reports.borrow().is_empty(),
            "{:?}",
            h.runtime.reports.borrow()
        );
        h.venue().dispose().unwrap();
    }
}
