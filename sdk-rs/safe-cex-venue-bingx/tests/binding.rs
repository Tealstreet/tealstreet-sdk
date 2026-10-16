//! Every previously-unbound Exchange method, on the assembled production host.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
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
    for status in [200.0, 400.0] {
        let h = fixture();
        h.http.advance_ms.set(1.0);
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
    let h = fixture();
    h.http.advance_ms.set(1.0);
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
