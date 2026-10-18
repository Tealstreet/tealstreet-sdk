//! E2E calls the production PhemexExchange through the typed Exchange contract.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_exchange::js_runtime::value::{Js, from_json};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};
use support::{Harness, harness, options};
const SYMBOL: &str = "BTCUSDT";
fn instruments() -> Value {
    json!({"data":{"perpProductsV2":[{"symbol":SYMBOL,"type":"PerpetualV2","baseCurrency":"BTC","quoteCurrency":"USDT","settleCurrency":"USDT","qtyStepSize":"0.003","tickSize":"0.25","maxOrderQtyRq":"100","maxLeverage":"50"},{"symbol":"IGNORE","type":"Perpetual","settleCurrency":"USDT"}]}})
}
fn tickers() -> Value {
    json!({"error":null,"result":[{"symbol":SYMBOL,"bidRp":"99.5","askRp":"100.5","lastRp":"100","markRp":"101","indexRp":"99","openRp":"80","volumeRq":"7","openInterestRv":"800","fundingRateRr":"0.001"}]})
}
fn account() -> Value {
    json!({"code":0,"data":{"account":{"userID":123,"accountBalanceRv":"1000","totalUsedBalanceRv":"200"},"positions":[{"symbol":SYMBOL,"size":"2","avgEntryPriceRp":"90","markPriceRp":"101","leverageRr":"-10","posSide":"Long","side":"Buy","posMode":"Hedged","crossMargin":true,"liquidationPriceRp":"50"}]}})
}
fn created() -> Value {
    json!({"code":0,"data":{"orderID":"o-created","symbol":SYMBOL,"orderStatus":"New","orderType":"Limit","side":"Buy","priceRp":"99.5","orderQtyRq":"0.009","leavesQty":"0.009","execQty":"0","timeInForce":"GoodTillCancel","reduceOnly":false}})
}
fn venue() -> Harness {
    harness(ExchangeName::Phemex, options(), |http| {
        http.ok("GET", "/public/products", instruments());
        http.ok("GET", "/md/v3/ticker/24hr/all", tickers());
        http.ok("GET", "/g-accounts/accountPositions", account());
        http.ok("PUT", "/g-orders/create", created());
        http.ok("DELETE", "/g-orders/cancel", json!({"code":0}));
        http.ok("PUT", "/g-positions/leverage", json!({"code":0}));
        http.ok(
            "PUT",
            "/g-positions/switch-pos-mode-sync",
            json!({"code":0}),
        );
    })
}
fn seed(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("markets");
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let tickers = h.drive(h.venue().fetch_tickers()).expect("tickers");
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn order() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":SYMBOL,"side":"buy","type":"limit","amount":0.01,"price":99.9,"clientOrderId":"caller-id"})).unwrap()
}
fn existing() -> Order {
    serde_json::from_value(json!({"id":"o-old","symbol":SYMBOL,"side":"buy","type":"limit","status":"open","amount":0.01,"filled":0,"remaining":0.01,"price":99.9,"reduceOnly":false,"lastUpdate":1700000000000.0,"info":{"posSide":"Long"}})).unwrap()
}

fn chart_venue() -> Harness {
    let opts = serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap();
    harness(ExchangeName::Phemex, opts, |http| {
        http.ok("GET", "/public/products", instruments());
        http.ok("GET", "/md/v3/ticker/24hr/all", tickers());
        http.ok("GET", "/md/v2/fullbook", json!({"result":{"sequence":100,"orderbook_p":{"bids":[["100","2"]],"asks":[["102","3"]]}}}));
    })
}
fn chart_options() -> OHLCVOptions {
    serde_json::from_value(
        json!({"symbol":SYMBOL,"interval":"1m","from":1700000000000_f64,"to":1700000060000_f64}),
    )
    .unwrap()
}
fn public_frame(h: &Harness, frame: Value) {
    h.runtime.sockets.borrow()[0]
        .1
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(frame.to_string()),
        ));
    h.runtime.pump();
}
#[test]
fn bound_chart_history_fetches_and_maps_real_rest_rows() {
    let h = chart_venue();
    seed(&h);
    h.http.ok(
        "GET",
        "/exchange/public/md/v2/kline/list",
        json!({"data":{"rows":[[1700000000,60,0,"10","15","9","12","3"]]}}),
    );
    let candles = h.drive(h.venue().fetch_ohlcv(chart_options())).unwrap();
    assert_eq!(
        candles.len(),
        1,
        "trait must dispatch to the venue candle reader"
    );
    assert_eq!(
        (
            candles[0].timestamp,
            candles[0].open,
            candles[0].high,
            candles[0].low,
            candles[0].close,
            candles[0].volume
        ),
        (1700000000.0, 10.0, 15.0, 9.0, 12.0, 3.0)
    );
    let calls = h.http.calls("/exchange/public/md/v2/kline/list");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].params,
        json!({"symbol":SYMBOL,"resolution":"60","from":1700000000_f64,"to":1700000060_f64})
    );
}
#[test]
fn bound_daily_chart_history_ends_at_previous_utc_midnight() {
    use std::{cell::RefCell, rc::Rc};
    let h = chart_venue();
    seed(&h);
    let errors = Rc::new(RefCell::new(Vec::new()));
    let seen = errors.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, v| {
            seen.borrow_mut().push(v.clone());
            Ok(())
        }),
    );
    h.http.ok(
        "GET",
        "/exchange/public/md/v2/kline/list",
        json!({"data":{"rows":[[1709856000,86400,0,"10","15","9","12","3"]]}}),
    );
    let opts = serde_json::from_value(
        json!({"symbol":SYMBOL,"interval":"1d","from":1709856000000_f64,"to":1710083045678_f64}),
    )
    .unwrap();
    let candles = h.drive(h.venue().fetch_ohlcv(opts)).unwrap();
    // TS: dayjs((dayjs(to).utcOffset(0).startOf('date').unix() - 86400) * 1000) => 1709942400000.
    let calls = h.http.calls("/exchange/public/md/v2/kline/list");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].params,
        json!({"symbol":SYMBOL,"resolution":"86400","from":1709856000_f64,"to":1709942400_f64})
    );
    assert_eq!(
        (candles.len(), candles[0].timestamp, candles[0].close),
        (1, 1709856000.0, 12.0)
    );
    assert!(
        errors.borrow().is_empty(),
        "no error emitted: {:?}",
        errors.borrow()
    );
}
#[test]
fn bound_chart_history_failure_emits_venue_message_and_returns_no_candles() {
    use std::{cell::RefCell, rc::Rc};
    let h = chart_venue();
    seed(&h);
    let errors = Rc::new(RefCell::new(Vec::new()));
    let seen = errors.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, v| {
            seen.borrow_mut().push(v.clone());
            Ok(())
        }),
    );
    h.http.route(
        "GET",
        "/exchange/public/md/v2/kline/list",
        400.0,
        json!({"code":10001,"msg":"kline backend busy"}),
    );
    let opts = serde_json::from_value(json!({"symbol":SYMBOL,"interval":"1m"})).unwrap();
    // TS catch: this.emitter.emit('error', err?.response?.data?.msg || err.message); return [].
    let candles = h.drive(h.venue().fetch_ohlcv(opts)).unwrap();
    assert!(candles.is_empty());
    let errors = errors.borrow();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].strict_equals(&Js::str("kline backend busy")));
}
#[test]
fn bound_chart_candles_subscribe_deliver_and_unsubscribe() {
    use std::{cell::RefCell, rc::Rc};
    let h = chart_venue();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let received = Rc::new(RefCell::new(Vec::<Candle>::new()));
    let seen = received.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(chart_options(), Rc::new(move |c| seen.borrow_mut().push(c)))
        .unwrap();
    h.runtime.pump();
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("kline_p.subscribe")),
        "trait must subscribe the production public socket"
    );
    let frame = json!({"symbol":SYMBOL,"kline_p":[[1700000060,60,0,12,16,11,14,5],[1700000000,60,0,10,15,9,12,3]]});
    public_frame(&h, frame.clone());
    assert_eq!(received.borrow().len(), 2);
    assert_eq!(received.borrow()[0].timestamp, 1700000000.0);
    assert_eq!(received.borrow()[1].close, 14.0);
    cleanup().unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("kline_p.unsubscribe"))
    );
    public_frame(&h, frame);
    assert_eq!(
        received.borrow().len(),
        2,
        "cleanup removes the actual parser"
    );
    h.venue().dispose().unwrap();
}
#[test]
fn bound_chart_book_subscribes_snapshots_and_unsubscribes() {
    use std::{cell::RefCell, rc::Rc};
    let h = chart_venue();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let received = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let seen = received.clone();
    let cleanup = h
        .venue()
        .listen_order_book(SYMBOL.into(), Rc::new(move |b| seen.borrow_mut().push(b)))
        .unwrap();
    h.runtime.pump();
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("orderbook_p.subscribe")),
        "trait must subscribe the production public socket"
    );
    let frame =
        json!({"symbol":SYMBOL,"orderbook_p":{"sequence":101,"bids":[["100","4"]],"asks":[]}});
    public_frame(&h, frame.clone());
    assert_eq!(h.http.calls("/md/v2/fullbook").len(), 1);
    assert_eq!(received.borrow().len(), 1);
    assert_eq!(received.borrow()[0].bids[0].amount, 4.0);
    assert_eq!(received.borrow()[0].bids[0].total, 4.0);
    assert_eq!(received.borrow()[0].asks[0].price, 102.0);
    cleanup().unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("orderbook_p.unsubscribe"))
    );
    public_frame(&h, frame);
    assert_eq!(received.borrow().len(), 1);
    h.venue().dispose().unwrap();
}

#[test]
fn fetch_markets_preserves_distinct_steps_and_filters_products() {
    let h = venue();
    let m = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(m.len(), 1, "markets census");
    assert_eq!((m[0].precision.amount, m[0].precision.price), (0.003, 0.25));
    assert_eq!(m[0].limits.leverage.max, 50.0);
    assert!(h.store.markets().is_empty(), "read stays pure");
    let req = &h.http.calls("/public/products")[0];
    assert_eq!(req.base_url, "https://api.phemex.com");
    assert_eq!(req.headers["X-PHEMEX-ACCESS-TOKEN"], "fixture-key");
}
#[test]
fn fetch_tickers_maps_source_numbers_without_store_writes() {
    let h = venue();
    seed(&h);
    let t = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].last, 100.0);
    assert_eq!(t[0].percentage, 25.0);
    assert_eq!(t[0].quote_volume, 700.0);
}
#[test]
fn get_account_uses_signed_account_positions() {
    let h = venue();
    let a = h.drive(h.venue().get_account()).unwrap();
    assert_eq!(serde_json::to_value(a).unwrap()["userId"], "123");
    let req = &h.http.calls("/g-accounts/accountPositions")[0];
    assert_eq!(req.params, json!({"currency":"USDT"}));
    let expiry = req.headers["X-PHEMEX-REQUEST-EXPIRY"].as_str().unwrap();
    let payload = format!("/g-accounts/accountPositionscurrency=USDT{expiry}");
    assert_eq!(
        req.headers["X-PHEMEX-REQUEST-SIGNATURE"],
        safe_cex_crypto::hmac_sha256(
            b"fixture-secret",
            payload.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        )
    );
}
#[test]
fn validate_account_returns_balance_and_keeps_store_pure() {
    let h = venue();
    let v = h.drive(h.venue().validate_account()).unwrap();
    let b = v.balance.unwrap();
    assert_eq!((b.total, b.used, b.free), (1000.0, 200.0, 800.0));
    assert_eq!(h.store.balance().total, 0.0);
    assert_eq!(serde_json::to_value(v.error).unwrap(), "");
}
#[test]
fn fetch_positions_supports_symbol_recovery_and_pure_full_snapshots() {
    let h = venue();
    seed(&h);
    let p = h
        .drive(h.venue().fetch_positions(Some(vec![SYMBOL.into()])))
        .unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].contracts, 2.0);
    assert_eq!(p[0].unrealized_pnl, 20.0);
    assert_eq!(p[0].margin_type, Some(MarginType::Cross));
    assert!(h.store.positions().is_empty());
    let all = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(all.len(), 2, "both settlement replies mapped");
    assert!(h.store.positions().is_empty());
}
#[test]
fn fetch_orders_returns_the_store_snapshot_without_http() {
    let h = venue();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            orders: Some(vec![existing()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o-old");
    assert!(h.http.requests.borrow().is_empty());
}
#[test]
fn start_populates_snapshots_and_opens_both_sockets() {
    let h = venue();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert_eq!(h.store.markets().len(), 1);
    assert_eq!(h.store.tickers().len(), 1);
    assert_eq!(h.store.balance().total, 2000.0);
    assert!(h.store.loaded().positions);
    assert_eq!(h.runtime.sockets.borrow().len(), 2);
    let private = h.runtime.sockets.borrow()[1].1.clone();
    let sent = private.sent.borrow().clone();
    assert!(sent.iter().any(|m| m.contains("user.auth")), "{sent:?}");
    assert!(!sent.iter().any(|m| m.contains("aop_p.subscribe")));
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                "{\"id\":999,\"result\":{\"status\":\"success\"}}".into(),
            ),
        ));
    h.runtime.pump();
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("aop_p.subscribe"))
    );
    h.venue().dispose().unwrap();
    assert!(
        h.runtime
            .sockets
            .borrow()
            .iter()
            .all(|(_, s)| s.closed.get())
    );
}
#[test]
fn place_order_honors_id_and_distinct_amount_price_precision() {
    let h = venue();
    seed(&h);
    let result = h
        .drive(h.venue().base().place_order(h.venue(), order()))
        .unwrap();
    assert_eq!(result, vec!["o-created"]);
    let calls = h.http.calls("/g-orders/create");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "PUT");
    assert_eq!(calls[0].params["clOrdID"], "caller-id");
    assert_eq!(calls[0].params["orderQtyRq"], "0.009");
    assert_eq!(calls[0].params["priceRp"], "100");
    assert_eq!(calls[0].data, Value::Null);
    assert_eq!(h.store.orders().len(), 1);
}
#[test]
fn update_order_cancels_then_replaces() {
    let h = venue();
    seed(&h);
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            orders: Some(vec![existing()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let opts: UpdateOrderOpts =
        serde_json::from_value(json!({"order":existing(),"update":{"price":98.5}})).unwrap();
    let ids = h.drive(h.venue().update_order(opts)).unwrap();
    assert_eq!(ids, vec!["o-created"]);
    assert_eq!(h.http.calls("/g-orders/cancel").len(), 1);
    assert_eq!(
        h.http.calls("/g-orders/create")[0].params["priceRp"],
        "98.5"
    );
}
#[test]
fn cancel_orders_deduplicates_parent_and_updates_status() {
    let h = venue();
    seed(&h);
    let o = existing();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            orders: Some(vec![o.clone()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let mut child = o.clone();
    child.id = "o-old__stop_loss".into();
    h.drive(h.venue().cancel_orders(vec![o, child])).unwrap();
    let calls = h.http.calls("/g-orders/cancel");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["orderID"], "o-old");
    assert_eq!(h.store.orders()[0].status, OrderStatus::Canceled);
}
#[test]
fn set_leverage_retains_cross_margin_and_records_confirmed_metadata() {
    let h = venue();
    seed(&h);
    h.drive(h.venue().set_leverage(SYMBOL.into(), 7.0, None))
        .unwrap();
    let calls = h.http.calls("/g-positions/leverage");
    assert_eq!(calls[0].params["longLeverageRr"], -7.0);
    assert_eq!(calls[0].params["shortLeverageRr"], -7.0);
    let all = serde_json::to_value(&*h.store.fetched_leverage()).unwrap();
    assert_eq!(all[SYMBOL]["buyLeverage"], 7.0);
    assert_eq!(all[SYMBOL]["marginType"], "cross");
}
#[test]
fn change_position_mode_sends_symbol_and_records_raw_mode() {
    let h = venue();
    seed(&h);
    h.drive(h.venue().change_position_mode(false, Some(SYMBOL.into())))
        .unwrap();
    let calls = h.http.calls("/g-positions/switch-pos-mode-sync");
    assert_eq!(
        calls[0].params,
        json!({"symbol":SYMBOL,"targetPosMode":"OneWay"})
    );
    let all = serde_json::to_value(&*h.store.fetched_leverage()).unwrap();
    assert_eq!(all[SYMBOL]["positionMode"], "hedged");
}
#[test]
fn change_margin_type_retains_existing_leverage() {
    let h = venue();
    seed(&h);
    h.drive(h.venue().change_margin_type(true, Some(SYMBOL.into())))
        .unwrap();
    assert_eq!(
        h.http.calls("/g-positions/leverage")[0].params["longLeverageRr"],
        10.0
    );
    let all = serde_json::to_value(&*h.store.fetched_leverage()).unwrap();
    assert_eq!(all[SYMBOL]["marginType"], "isolated");
}
#[test]
fn inherited_symbol_max_leverage_rejects() {
    let h = venue();
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage(SYMBOL.into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
}
#[test]
fn inherited_schedule_cancel_rejects() {
    let h = venue();
    assert_eq!(
        h.drive(h.venue().schedule_cancel(None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
}
#[test]
fn inherited_public_trades_rejects() {
    let h = venue();
    assert_eq!(
        h.drive(h.venue().fetch_public_trades(SYMBOL.into(), None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
}

#[test]
fn public_only_start_has_no_private_requests_or_socket() {
    let opts: ExchangeOptions =
        serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap();
    let h = harness(ExchangeName::Phemex, opts, |http| {
        http.ok("GET", "/public/products", instruments());
        http.ok("GET", "/md/v3/ticker/24hr/all", tickers());
    });
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    assert!(h.http.calls("/g-accounts/accountPositions").is_empty());
    assert_eq!(h.store.balance().total, 0.0);
    assert!(h.store.loaded().balance);
    assert!(h.store.loaded().orders);
    assert!(h.store.loaded().positions);
}
#[test]
fn invalid_client_id_rejects_before_any_http_or_store_effect() {
    let h = venue();
    let mut opts = order();
    opts.client_order_id = Some("x".repeat(41));
    let error = h
        .drive(h.venue().base().place_order(h.venue(), opts))
        .unwrap_err();
    assert!(error.message.contains("Phemex"));
    assert!(h.http.requests.borrow().is_empty());
    assert!(h.store.orders().is_empty());
}
#[test]
fn disposal_during_start_ignores_late_market_reply() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let h = venue();
    let release = h.http.hold("/public/products");
    let mut future = Box::pin(h.venue().start());
    assert!(matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    h.runtime.pump();
    assert_eq!(h.http.calls("/public/products").len(), 1);
    h.venue().dispose().unwrap();
    release.set(true);
    h.drive(future).unwrap();
    assert!(h.store.markets().is_empty());
    assert!(h.store.tickers().is_empty());
    assert_eq!(h.store.balance().total, 0.0);
    assert!(h.runtime.sockets.borrow().is_empty());
}
#[derive(Default)]
struct Signer {
    released: std::rc::Rc<std::cell::Cell<bool>>,
    fail: std::cell::Cell<bool>,
    requests: std::cell::RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for Signer {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        self.requests
            .borrow_mut()
            .push(serde_json::to_value(request).unwrap());
        let ready = self.released.clone();
        let fail = self.fail.get();
        Box::pin(async move {
            std::future::poll_fn(move |_| {
                if ready.get() {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            })
            .await;
            Ok(safe_cex_transport::signing::RemoteSignerResponse::WsAuth(
                safe_cex_types::ws_auth::WsAuthResponse {
                    id: None,
                    ws_auth_signature: (!fail).then(|| "remote-signature".into()),
                    error: fail.then(|| "tab unavailable".into()),
                },
            ))
        })
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for Signer {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(
        &self,
        _: std::rc::Rc<dyn Fn(bool)>,
    ) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}
fn remote_venue(signer: std::rc::Rc<Signer>) -> Harness {
    harness(ExchangeName::Phemex, options(), |http| {
        *http.signer.borrow_mut() = Some((signer, Some("account".into())));
        http.ok("GET", "/public/products", instruments());
        http.ok("GET", "/md/v3/ticker/24hr/all", tickers());
        http.ok("GET", "/g-accounts/accountPositions", account());
    })
}
#[test]
fn private_socket_waits_for_remote_signature_then_auth_ack() {
    let signer = std::rc::Rc::new(Signer::default());
    let h = remote_venue(signer.clone());
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = h.runtime.sockets.borrow()[1].1.clone();
    assert_eq!(signer.requests.borrow().len(), 1);
    assert!(
        !socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("user.auth") || m.contains("aop_p.subscribe"))
    );
    let req = &signer.requests.borrow()[0];
    assert_eq!(req["exchangeName"], "phemex");
    assert_eq!(req["accountName"], "account");
    assert_eq!(req["wsAuth"]["outputFormat"], "hex");
    assert!(
        req["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .starts_with("fixture-key")
    );
    signer.released.set(true);
    h.runtime.pump();
    let sent = socket.sent.borrow();
    let auth: Value =
        serde_json::from_str(sent.iter().find(|m| m.contains("user.auth")).unwrap()).unwrap();
    assert_eq!(auth["params"][2], "remote-signature");
    assert!(!sent.iter().any(|m| m.contains("aop_p.subscribe")));
}
#[test]
fn remote_signing_failure_sends_no_auth_or_subscription_and_is_nonterminal() {
    let signer = std::rc::Rc::new(Signer::default());
    signer.fail.set(true);
    signer.released.set(true);
    let h = remote_venue(signer);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = h.runtime.sockets.borrow()[1].1.clone();
    assert!(
        !socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("user.auth") || m.contains("aop_p.subscribe"))
    );
    let logs = h.runtime.console.borrow();
    assert!(
        logs.iter()
            .any(|(_, m)| m.contains("Remote WS signing unavailable: tab unavailable")),
        "{logs:?}"
    );
    assert!(
        !logs
            .iter()
            .any(|(_, m)| m.to_lowercase().contains("auth failed"))
    );
}

#[test]
fn place_orders_preflights_and_places_each_caller_id_in_series() {
    let h = venue();
    seed(&h);
    let first = order();
    let mut second = first.clone();
    second.client_order_id = Some("second-id".into());
    let ids = h
        .drive(
            h.venue()
                .base()
                .place_orders(h.venue(), vec![first, second]),
        )
        .unwrap();
    assert_eq!(ids, vec!["o-created", "o-created"]);
    let calls = h.http.calls("/g-orders/create");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].params["clOrdID"], "caller-id");
    assert_eq!(calls[1].params["clOrdID"], "second-id");
}
#[test]
fn historical_fills_use_the_primary_symbol_scoped_route() {
    let h = venue();
    seed(&h);
    h.http.ok("GET","/exchange/order/v2/tradingList",json!({"data":{"rows":[{"execID":"fill-1","orderID":"o-1","symbol":SYMBOL,"execType":"Trade","side":"Buy","execPriceRp":"101","execQtyRq":"0.5","createdAt":1700000000000u64,"execFeeRv":"0.01","feeCurrency":"USDT"}]}}));
    let opts: ExecutionsOptions =
        serde_json::from_value(json!({"symbol":SYMBOL,"limit":2})).unwrap();
    let fills = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, 101.0);
    assert_eq!(fills[0].qty, 0.5);
    assert_eq!(fills[0].timestamp, 1700000000.0);
    assert_eq!(fills[0].commission, Some(0.01));
    assert!(h.http.calls("/exchange/order/trade").is_empty());
}

#[test]
fn dispose_runs_source_super_cleanup() {
    use std::{cell::Cell, rc::Rc};
    let h = venue();
    let observed = Rc::new(Cell::new(0));
    let seen = observed.clone();
    h.venue().base().on(
        "update",
        Rc::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    h.venue().dispose().expect("source dispose");
    assert!(
        h.venue().base().is_disposed(),
        "source super call sets shared state"
    );
    h.venue()
        .base()
        .emit("update", from_json(&json!({})))
        .unwrap();
    assert_eq!(
        observed.get(),
        0,
        "source super call removes inherited listeners"
    );
}

fn one_way_account() -> Value {
    let mut reply = account();
    let p = &mut reply["data"]["positions"][0];
    p["posMode"] = json!("OneWay");
    p["posSide"] = json!("Merged");
    reply
}
fn one_way_venue() -> Harness {
    harness(ExchangeName::Phemex, options(), |http| {
        http.ok("GET", "/public/products", instruments());
        http.ok("GET", "/md/v3/ticker/24hr/all", tickers());
        http.ok("GET", "/g-accounts/accountPositions", one_way_account());
        http.ok("PUT", "/g-orders/create", created());
        http.ok("DELETE", "/g-orders", json!({"code":0,"data":[]}));
    })
}
#[test]
fn cached_position_controls_placement_without_account_reads() {
    let h = one_way_venue();
    seed(&h);
    let positions = h
        .drive(h.venue().fetch_positions(Some(vec![SYMBOL.into()])))
        .unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.http.requests.borrow_mut().clear();
    let mut opts = order();
    opts.reduce_only = Some(true);
    h.drive(h.venue().base().place_order(h.venue(), opts))
        .unwrap();
    let calls = h.http.calls("/g-orders/create");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["posSide"], "Merged");
    assert_eq!(calls[0].params["reduceOnly"], "true");
    assert!(
        h.http.calls("/g-accounts/accountPositions").is_empty(),
        "both order-side and reduceOnly lookups must use the cache"
    );
}
#[test]
fn cancel_without_info_uses_cached_position_side_without_account_reads() {
    let h = one_way_venue();
    seed(&h);
    let positions = h
        .drive(h.venue().fetch_positions(Some(vec![SYMBOL.into()])))
        .unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.http.requests.borrow_mut().clear();
    let mut first = existing();
    first.info = safe_cex_types::ModelField::Absent;
    let mut second = first.clone();
    second.id = "second-order".into();
    h.drive(h.venue().cancel_orders(vec![first, second]))
        .unwrap();
    assert_eq!(h.http.calls("/g-orders")[0].params["posSide"], "Merged");
    assert!(
        h.http.calls("/g-accounts/accountPositions").is_empty(),
        "cancel side lookup must use the store"
    );
}
#[test]
fn unloaded_positions_omit_reduce_only_after_raw_mode_lookup() {
    // Hosted startup deliberately leaves loaded.positions false. Only the raw
    // mode recovery may read REST; it must not populate the positions cache.
    let h = one_way_venue();
    seed(&h);
    assert!(!h.store.loaded().positions);
    let mut opts = order();
    opts.reduce_only = Some(true);
    h.drive(h.venue().base().place_order(h.venue(), opts))
        .unwrap();
    let calls = h.http.calls("/g-orders/create");
    assert_eq!(calls[0].params["posSide"], "Merged");
    assert!(
        calls[0].params.get("reduceOnly").is_none(),
        "TS omits reduceOnly when the unloaded position helper returns []"
    );
    assert_eq!(
        h.http.calls("/g-accounts/accountPositions").len(),
        1,
        "only raw mode recovery reads the account"
    );
    assert!(h.store.positions().is_empty());
}
#[test]
fn loaded_missing_position_is_fetched_once_and_cached_for_formatting() {
    let h = one_way_venue();
    seed(&h);
    let mut loaded = h.store.loaded().clone();
    loaded.positions = true;
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            loaded: Some(loaded),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let mut opts = order();
    opts.reduce_only = Some(true);
    h.drive(h.venue().base().place_order(h.venue(), opts))
        .unwrap();
    assert_eq!(
        h.http.calls("/g-accounts/accountPositions").len(),
        1,
        "order-side fetch must be reused by reduceOnly formatting"
    );
    assert_eq!(
        h.store.positions().len(),
        1,
        "recovery commits the fetched row"
    );
    assert_eq!(
        h.http.calls("/g-orders/create")[0].params["reduceOnly"],
        "true"
    );
}

include!("support/private_binding_topics.rs");

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("phemex");
    assert_eq!(source["cases"].as_array().unwrap().len(), 72);
    let h = chart_venue();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("phemex")),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    frame.as_str().unwrap().into(),
                )));
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

#[test]
fn bound_chart_snapshot_warning_uses_error_message_and_retries() {
    use std::{cell::RefCell, rc::Rc};
    let h = chart_venue();
    // Reject terminal I/O with the real harness Error; all socket, HTTP-client,
    // snapshot and logging behavior stays in the constructed production venue.
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/md/v2/fullbook");
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let received = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
    let seen = received.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            SYMBOL.into(),
            Rc::new(move |book| seen.borrow_mut().push(book)),
        )
        .unwrap();
    h.runtime.pump();
    let frame =
        json!({"symbol":SYMBOL,"orderbook_p":{"sequence":101,"bids":[["100","4"]],"asks":[]}});
    let warning = "[Phemex] Orderbook snapshot request failed for BTCUSDT; waiting for next delta to retry: unscripted request GET /md/v2/fullbook";
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshot-warning-source.cjs"
        ))
        .output()
        .expect("maintained TS source oracle");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source["error"],
        json!([[warning, "warning"], [warning, "warning"]])
    );
    for attempt in 0..3 {
        if attempt == 1 {
            // Markets/tickers plus the first snapshot consumed the real
            // three-per-second read budget. Advance its clock, not its policy.
            h.runtime.advance(1_000.);
        }
        if attempt == 2 {
            h.runtime.advance(30_000.);
        }
        public_frame(&h, frame.clone());
        for _ in 0..8 {
            h.runtime.pump();
        }
        assert_eq!(
            h.http.calls("/md/v2/fullbook").len(),
            attempt + 1,
            "failed snapshot must retry on the next delta"
        );
        let logs: Vec<_> = h
            .runtime
            .console
            .borrow()
            .iter()
            .filter(|(_, msg)| msg.contains("Orderbook snapshot request failed"))
            .cloned()
            .collect();
        assert_eq!(
            logs.len(),
            if attempt == 2 { 2 } else { 1 },
            "PHEMEX-SNAPSHOT-WARNING must reach parent.log and throttle/reopen; reports: {:?}",
            h.runtime.reports.borrow()
        );
        for (severity, message) in logs {
            assert_eq!(severity, "warning");
            assert_eq!(
                message, warning,
                "PHEMEX-SNAPSHOT-WARNING must use Error.message, matching maintained TS"
            );
        }
        assert!(received.borrow().is_empty());
    }
    h.http.ok("GET", "/md/v2/fullbook", json!({"result":{"sequence":100,"orderbook_p":{"bids":[["100","2"]],"asks":[["102","3"]]}}}));
    public_frame(&h, frame);
    for _ in 0..8 {
        h.runtime.pump();
    }
    assert_eq!(
        received.borrow().len(),
        1,
        "a later valid snapshot must recover"
    );
    assert_eq!(received.borrow()[0].bids[0].amount, 4.);
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "snapshot failure must be handled by its warning path"
    );
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}
