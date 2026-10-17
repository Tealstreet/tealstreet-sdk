//! Production OkxExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::{Value, json};

const MARKETS: &str = "/api/v5/public/instruments";
const TICKERS: &str = "/api/v5/market/tickers";
const CONFIG: &str = "/api/v5/account/config";
const BALANCE: &str = "/api/v5/account/account-position-risk";
const POSITIONS: &str = "/api/v5/account/positions";
const LEVERAGE: &str = "/api/v5/account/leverage-info";
const ORDERS: &str = "/api/v5/trade/orders-pending";
const ALGO_ORDERS: &str = "/api/v5/trade/orders-algo-pending";
const PLACE: &str = "/api/v5/trade/batch-orders";
const AMEND: &str = "/api/v5/trade/amend-order";
const CANCEL: &str = "/api/v5/trade/cancel-batch-orders";
const SET_LEVERAGE: &str = "/api/v5/account/set-leverage";
const SET_MODE: &str = "/api/v5/account/set-position-mode";
const FILLS: &str = "/api/v5/trade/fills";
const FILLS_HISTORY: &str = "/api/v5/trade/fills-history";

fn instrument() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "instType": "SWAP", "ctType": "linear", "ctVal": "0.01",
        "ctValCcy": "BTC", "settleCcy": "USDT", "uly": "BTC-USDT", "instFamily": "BTC-USDT",
        "baseCcy": "", "quoteCcy": "", "lotSz": "1", "tickSz": "0.1", "minSz": "1",
        "maxLmtSz": "100000", "maxMktSz": "5000", "maxStopSz": "5000", "maxTriggerSz": "100000",
        "maxTwapSz": "100000", "maxIcebergSz": "100000", "lever": "100", "state": "live"
    })
}

fn ticker() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "last": "50000", "askPx": "50001", "bidPx": "49999",
        "open24h": "49000", "volCcy24h": "10", "vol24h": "1000"
    })
}

fn position() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "instType": "SWAP", "posSide": "net", "pos": "10",
        "avgPx": "50000", "markPx": "50100", "upl": "1", "liqPx": "40000", "lever": "10",
        "mgnMode": "cross", "imr": "50", "mmr": "5", "ccy": "USDT", "notionalUsd": "5010",
        "cTime": "1700000000000", "uTime": "1700000000000", "posId": "1"
    })
}

fn order() -> Value {
    json!({
        "instId": "BTC-USDT-SWAP", "instType": "SWAP", "ordId": "o1", "clOrdId": "c1",
        "side": "buy", "posSide": "net", "ordType": "limit", "px": "49000", "sz": "10",
        "accFillSz": "0", "state": "live", "reduceOnly": "false", "lever": "10",
        "ccy": "", "cTime": "1700000000000", "uTime": "1700000000000"
    })
}

fn okx(http: &Http) {
    http.ok("GET", MARKETS, json!([instrument()]));
    http.ok("GET", TICKERS, json!([ticker()]));
    http.ok(
        "GET",
        CONFIG,
        json!([{"posMode": "net_mode", "acctLv": "2"}]),
    );
    http.ok(
        "GET",
        BALANCE,
        json!([{"adjEq": "1000", "balData": [{"ccy": "USDT", "eq": "1000"}], "posData": []}]),
    );
    http.ok("GET", POSITIONS, json!([position()]));
    http.ok(
        "GET",
        LEVERAGE,
        json!([{"instId": "BTC-USDT-SWAP", "lever": "10", "mgnMode": "cross"}]),
    );
    http.ok("GET", ORDERS, json!([order()]));
    http.ok("GET", ALGO_ORDERS, json!([]));
    http.ok(
        "POST",
        PLACE,
        json!([{"ordId": "p1", "clOrdId": "cp1", "sCode": "0", "sMsg": ""}]),
    );
    http.ok(
        "POST",
        AMEND,
        json!([{"ordId": "o1", "clOrdId": "c1", "sCode": "0", "sMsg": ""}]),
    );
    http.ok(
        "POST",
        CANCEL,
        json!([{"ordId": "o1", "clOrdId": "c1", "sCode": "0", "sMsg": ""}]),
    );
    http.ok(
        "POST",
        SET_LEVERAGE,
        json!([{"instId": "BTC-USDT-SWAP", "lever": "5", "mgnMode": "cross"}]),
    );
    http.ok("POST", SET_MODE, json!([{"posMode": "long_short_mode"}]));
    http.ok(
        "GET",
        FILLS,
        json!([{
            "instId": "BTC-USDT-SWAP", "tradeId": "t1", "ordId": "o1", "billId": "b1", "side": "buy",
            "posSide": "net", "fillPx": "50000", "fillSz": "2", "fee": "-0.5", "feeCcy": "USDT",
            "fillPnl": "0", "execType": "T", "ts": "1700000000000"
        }]),
    );
    http.ok("GET", FILLS_HISTORY, json!([]));
}

fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("markets");
    Store::update(
        &*h.store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .expect("store markets");
}

fn limit_order(amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": amount, "price": 49000
    }))
    .expect("PlaceOrderOpts")
}

#[test]
fn fetch_markets_maps_instruments() {
    let h = harness(ExchangeName::Okx, options(), okx);
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].id, "BTC-USDT-SWAP");
    assert_eq!(markets[0].precision.amount, 1.0);
    assert_eq!(markets[0].limits.amount.min, 0.01);
    assert_eq!(h.http.calls(MARKETS)[0].params["instType"], "SWAP");
}

#[test]
fn fetch_tickers_parses_against_loaded_markets() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].symbol, "BTCUSDT");
    assert_eq!(tickers[0].last, 50000.0);
    assert_eq!(tickers[0].bid, 49999.0);
}

#[test]
fn validate_account_signs_the_balance_read() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(serde_json::to_value(&result.error).unwrap(), json!(""));
    let calls = h.http.calls(BALANCE);
    assert_eq!(calls[0].headers["OK-ACCESS-KEY"], "fixture-key");
    assert_eq!(calls[0].headers["OK-ACCESS-PASSPHRASE"], "fixture-pass");
    assert!(calls[0].headers["OK-ACCESS-SIGN"].is_string());
}

#[test]
fn fetch_positions_maps_net_positions() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].contracts, 0.1);
    assert_eq!(positions[0].side, PositionSide::Long);
    assert_eq!(h.http.calls(CONFIG).len(), 1);
}

#[test]
fn fetch_orders_reads_normal_and_algo_pages() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o1");
    assert_eq!(orders[0].price, 49000.0);
    assert!(!h.http.calls(ALGO_ORDERS).is_empty());
}

#[test]
fn fetch_orders_retains_all_five_groups_for_symbol_cancellation() {
    const CANCEL_ALGO: &str = "/api/v5/trade/cancel-algos";
    let h = harness(ExchangeName::Okx, options(), |http| {
        okx(http);
        http.routes
            .borrow_mut()
            .retain(|(_, path, _)| path != ALGO_ORDERS);
        // Source loads OCO, conditional, trigger, trailing; output moves trailing
        // ahead of conditional/trigger. Give every branch a distinct native ID.
        for (kind, id) in [
            ("oco", "oco"),
            ("conditional", "conditional"),
            ("trigger", "trigger"),
            ("move_order_stop", "trailing"),
        ] {
            http.ok(
                "GET",
                ALGO_ORDERS,
                json!([{
                    "instId":"BTC-USDT-SWAP","algoId":id,"ordType":kind,
                    "side":"sell","sz":"10","reduceOnly":"true",
                    "slTriggerPx":"48000","triggerPx":"48000","ordPx":"-1",
                    "last":"50000","callbackSpread":"100","moveTriggerPx":"100"
                }]),
            );
        }
        http.ok("POST", CANCEL_ALGO, json!([]));
    });
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        [
            "o1",
            "oco_sl",
            "trailing_tsl",
            "conditional_sl",
            "trigger_sl"
        ],
        "OKX production retains all five order groups in source order"
    );
    assert_eq!(
        orders.iter().map(|o| o.r#type).collect::<Vec<_>>(),
        [
            OrderType::Limit,
            OrderType::StopLoss,
            OrderType::TrailingStopLoss,
            OrderType::StopLoss,
            OrderType::StopLoss
        ],
        "OKX production distinct native order variants"
    );
    Store::update(
        &*h.store,
        StoreUpdate {
            orders: Some(orders),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.drive(h.venue().cancel_symbol_orders("BTCUSDT".into()))
        .expect("cancelSymbolOrders");
    assert_eq!(
        h.http.calls(CANCEL).len(),
        1,
        "OKX cancellation normal batch"
    );
    assert_eq!(h.http.calls(CANCEL)[0].data[0]["ordId"], "o1");
    let calls = h.http.calls(CANCEL_ALGO);
    assert_eq!(calls.len(), 1, "OKX cancellation algo batch");
    assert_eq!(
        calls[0]
            .data
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["algoId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["oco", "trailing", "conditional", "trigger"],
        "OKX cancellation includes trailing conditional and trigger orders"
    );
}

#[test]
fn place_order_batches_a_broker_tagged_limit() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let ids = h
        .drive(Box::pin(
            h.venue().base().place_order(h.venue(), limit_order(10.0)),
        ))
        .expect("placeOrder");
    assert_eq!(ids, vec!["p1"]);
    let sent = &h.http.calls(PLACE)[0].data;
    assert_eq!(sent[0]["instId"], "BTC-USDT-SWAP");
    assert_eq!(sent[0]["sz"], "10");
    assert!(
        sent[0]["clOrdId"]
            .as_str()
            .unwrap()
            .starts_with("e847386590ce4dBC")
    );
}

#[test]
fn place_orders_sends_one_batch() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let ids = h
        .drive(Box::pin(
            h.venue()
                .base()
                .place_orders(h.venue(), vec![limit_order(10.0)]),
        ))
        .expect("placeOrders");
    assert_eq!(ids, vec!["p1"]);
    assert_eq!(h.http.calls(PLACE).len(), 1);
}

#[test]
fn set_leverage_posts_cross_leverage() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let positions = h.drive(h.venue().fetch_positions(None)).expect("positions");
    Store::add_or_update_positions(&*h.store, positions).expect("store positions");
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 5.0, None))
        .expect("setLeverage");
    let sent = &h.http.calls(SET_LEVERAGE)[0].data;
    assert_eq!(sent["instId"], "BTC-USDT-SWAP");
    assert_eq!(sent["lever"], "5");
    assert_eq!(sent["mgnMode"], "cross");
}

#[test]
fn change_position_mode_posts_long_short_mode() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    h.drive(h.venue().change_position_mode(true, None))
        .expect("changePositionMode");
    assert_eq!(h.http.calls(SET_MODE)[0].data["posMode"], "long_short_mode");
}

#[test]
fn change_margin_type_reports_cross_only_without_a_request() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, data| {
            sink.borrow_mut().push(data.to_display());
            Ok(())
        }),
    );
    let before = h.http.requests.borrow().len();
    h.drive(h.venue().change_margin_type(true, None))
        .expect("changeMarginType resolves");
    assert_eq!(h.http.requests.borrow().len(), before);
    assert!(
        errors
            .borrow()
            .iter()
            .any(|e| e.contains("Only cross mode is supported")),
        "{errors:?}"
    );
}

#[test]
fn update_and_cancel_orders_hit_amend_and_cancel_batch() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    Store::update(
        &*h.store,
        StoreUpdate {
            orders: Some(orders.clone()),
            ..Default::default()
        },
        false,
    )
    .expect("store orders");
    let update: UpdateOrderOpts = serde_json::from_value(json!({
        "order": orders[0], "update": {"price": 48000}
    }))
    .expect("UpdateOrderOpts");
    h.drive(h.venue().update_order(update))
        .expect("updateOrder");
    let amended = &h.http.calls(AMEND)[0].data;
    assert_eq!(amended["ordId"], "o1");
    assert_eq!(amended["newPx"], "48000");
    h.drive(h.venue().cancel_orders(orders))
        .expect("cancelOrders");
    assert_eq!(h.http.calls(CANCEL)[0].data[0]["ordId"], "o1");
}

#[test]
fn fetch_historical_fills_maps_executions() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let opts: ExecutionsOptions = serde_json::from_value(json!({"symbol": "BTCUSDT"})).unwrap();
    let fills = h
        .drive(h.venue().fetch_historical_fills(opts))
        .expect("fills");
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].price, 50000.0);
}

#[test]
fn inherited_base_methods_reject_without_a_request() {
    let h = harness(ExchangeName::Okx, options(), okx);
    loaded(&h);
    let before = h.http.requests.borrow().len();
    for result in [
        h.drive(h.venue().get_account()).map(|_| ()),
        h.drive(h.venue().fetch_symbol_max_leverage("BTCUSDT".into()))
            .map(|_| ()),
        h.drive(h.venue().schedule_cancel(None)),
        h.drive(h.venue().fetch_public_trades("BTCUSDT".into(), None))
            .map(|_| ()),
    ] {
        assert_eq!(result.unwrap_err().message, "Not implemented");
    }
    assert_eq!(h.http.requests.borrow().len(), before);
}

#[test]
fn start_loads_markets_tickers_account_and_sockets() {
    let h = harness(ExchangeName::Okx, options(), okx);
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(50.0);
    {
        let loaded = h.store.loaded();
        assert!(loaded.markets && loaded.tickers);
        assert!(loaded.positions && loaded.orders && loaded.balance);
    }
    let urls: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert!(
        urls.iter().any(|u| u.ends_with("/ws/v5/public")),
        "{urls:?}"
    );
    assert!(
        urls.iter().any(|u| u.ends_with("/ws/v5/private")),
        "{urls:?}"
    );
    h.venue().dispose().expect("dispose");
}

#[test]
fn source_identity_flags_survive_demo_facade_construction() {
    for (name, demo) in [
        (ExchangeName::Okx, false),
        (ExchangeName::Okxdemo, true),
        (ExchangeName::Okxeu, false),
        (ExchangeName::Okxeudemo, true),
    ] {
        let h = harness(name, options(), |_| {});
        assert_eq!(
            h.venue().base().is_demo,
            demo,
            "OKX source isDemo flag reaches typed base"
        );
        assert!(!h.venue().base().is_spot);
        h.venue().dispose().unwrap();
    }
}

fn private_topic_names() -> [ExchangeName; 4] {
    [
        ExchangeName::Okx,
        ExchangeName::Okxdemo,
        ExchangeName::Okxeu,
        ExchangeName::Okxeudemo,
    ]
}
fn private_topic_scene(name: &ExchangeName) -> Harness {
    harness(name.clone(), options(), |http| {
        okx(http);
        for (path, rows) in [
            (MARKETS, json!([instrument()])),
            ("/api/v5/account/instruments", json!([instrument()])),
            (TICKERS, json!([ticker()])),
            (ORDERS, json!([order()])),
            (POSITIONS, json!([position()])),
        ] {
            http.ok_for_instrument(path, "SWAP", rows);
            http.ok_for_instrument(path, "FUTURES", json!([]));
        }
    })
}

fn private_frame(h: &Harness, channel: &str, rows: Value) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/ws/v5/private"))
        .expect("production private socket")
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"arg":{"channel":channel,"instType":"SWAP"},"data":rows}).to_string(),
        )));
    h.runtime.pump();
}

fn record_private_fills(h: &Harness) -> std::rc::Rc<std::cell::RefCell<Vec<Value>>> {
    let fills = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, payload| {
            sink.borrow_mut().push(
                safe_cex_exchange::js_runtime::value::to_json(payload)
                    .expect("finite fixture fill"),
            );
            Ok(())
        }),
    );
    fills
}

fn private_orders_commit_terminal_states_before_final_fill_and_reject_stale_snapshots_for_name(
    name: ExchangeName,
) {
    let mut checked = 0;
    for (state, status) in [
        ("filled", OrderStatus::Closed),
        ("canceled", OrderStatus::Canceled),
        ("mmp_canceled", OrderStatus::Canceled),
    ] {
        let h = private_topic_scene(&name);
        h.drive(h.venue().start()).unwrap();
        h.runtime.advance(50.);
        let stale = h.store.orders().to_vec();
        assert_eq!(stale.len(), 1, "OKX private initial order floor");
        assert_eq!(stale[0].status, OrderStatus::Open);
        let fills = record_private_fills(&h);
        let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let failures = errors.clone();
        h.venue().base().on(
            "error",
            std::rc::Rc::new(move |_, error| {
                failures.borrow_mut().push(error.clone());
                Ok(())
            }),
        );
        let statuses = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let observed = statuses.clone();
        let store = h.store.clone();
        h.venue().base().on(
            "fill",
            std::rc::Rc::new(move |_, _| {
                observed.borrow_mut().push(store.orders()[0].status);
                Ok(())
            }),
        );
        let mut row = order();
        row["state"] = json!(state);
        row["accFillSz"] = json!("10");
        row["fillSz"] = json!("2");
        row["fillPx"] = json!("50100");
        row["tradeId"] = json!("execution-1");
        row["fillTime"] = json!("1700000000001");
        private_frame(&h, "orders", json!([row.clone()]));
        assert_eq!(
            h.store.orders().len(),
            1,
            "OKX private terminal order retained"
        );
        assert_eq!(h.store.orders()[0].id, "o1");
        assert_eq!(
            h.store.orders()[0].status,
            status,
            "OKX private terminal frame commits native status {state}; errors={:?}",
            errors.borrow()
        );
        let expected_fills = usize::from(state == "filled");
        assert_eq!(
            fills.borrow().len(),
            expected_fills,
            "OKX private final fill count {state}"
        );
        if state == "filled" {
            assert_eq!(
                *statuses.borrow(),
                [OrderStatus::Closed],
                "OKX private terminal commit precedes final fill"
            );
            assert_eq!(
                fills.borrow()[0],
                json!({"id":"o1","side":"buy","symbol":"BTCUSDT","price":50100.0,"amount":0.02}),
                "OKX private native fill object and converted amount"
            );
            private_frame(&h, "orders", json!([row]));
            assert_eq!(
                fills.borrow().len(),
                1,
                "OKX private duplicate execution does not emit twice"
            );
        }
        Store::update(
            &*h.store,
            StoreUpdate {
                orders: Some(stale),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            h.store.orders()[0].status,
            status,
            "OKX private terminal memory refuses stale open REST snapshot {state}"
        );
        assert!(
            errors.borrow().is_empty(),
            "OKX private terminal frame emits no error"
        );
        assert!(
            h.runtime.reports.borrow().is_empty(),
            "OKX private frame has no uncaught runtime error"
        );
        h.venue().dispose().unwrap();
        checked += 1;
    }
    assert_eq!(checked, 3, "OKX private terminal state execution floor");
}

#[test]
fn private_orders_commit_terminal_states_before_final_fill_and_reject_stale_snapshots() {
    for name in private_topic_names() {
        private_orders_commit_terminal_states_before_final_fill_and_reject_stale_snapshots_for_name(
            name,
        );
    }
}

fn private_positions_frame_updates_native_position_and_balance_without_a_fill_for_name(
    name: ExchangeName,
) {
    let h = private_topic_scene(&name);
    h.drive(h.venue().start()).unwrap();
    h.runtime.advance(50.);
    let fills = record_private_fills(&h);
    let mut row = position();
    row["pos"] = json!("15");
    row["avgPx"] = json!("47000");
    row["markPx"] = json!("48000");
    row["liqPx"] = json!("39000");
    row["upl"] = json!("1.75");
    row["mmr"] = json!("7.25");
    row["imr"] = json!("75");
    private_frame(&h, "positions", json!([row]));
    let positions = h.store.positions();
    assert_eq!(positions.len(), 1, "OKX private position floor");
    let p = &positions[0];
    assert_eq!(p.symbol, "BTCUSDT");
    assert_eq!(p.side, PositionSide::Long);
    assert_eq!(p.contracts, 0.15, "OKX private position converts contracts");
    assert_eq!(
        p.entry_price, 47000.,
        "OKX private position native entry price"
    );
    assert_eq!(
        p.liquidation_price, 39000.,
        "OKX private position native liquidation price"
    );
    assert_eq!(
        p.unrealized_pnl, 1.8,
        "OKX private position native PnL rounded to price tick"
    );
    assert_eq!(
        h.store.balance().used,
        7.25,
        "OKX private position updates used margin"
    );
    assert_eq!(
        h.store.balance().upnl,
        1.8,
        "OKX private position updates balance PnL"
    );
    assert!(
        fills.borrow().is_empty(),
        "OKX position topic emits no fill"
    );
    assert!(h.runtime.reports.borrow().is_empty());
    drop(positions);
    h.venue().dispose().unwrap();
}

#[test]
fn private_positions_frame_updates_native_position_and_balance_without_a_fill() {
    for name in private_topic_names() {
        private_positions_frame_updates_native_position_and_balance_without_a_fill_for_name(name);
    }
}

fn private_account_frame_updates_balance_and_emits_no_fill_for_name(name: ExchangeName) {
    let h = private_topic_scene(&name);
    h.drive(h.venue().start()).unwrap();
    h.runtime.advance(50.);
    let fills = record_private_fills(&h);
    let mut seed = h.store.balance().clone();
    seed.used = 7.25;
    seed.upnl = 1.75;
    Store::update(
        &*h.store,
        StoreUpdate {
            balance: Some(seed),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    private_frame(
        &h,
        "account",
        json!([{"totalEq":"1200.25"},{"totalEq":"9.75"}]),
    );
    let balance = h.store.balance();
    assert_eq!(
        balance.free, 1202.75,
        "OKX private account free after used margin"
    );
    assert_eq!(
        balance.total, 1208.25,
        "OKX private account total after PnL"
    );
    assert_eq!(balance.used, 7.25);
    assert_eq!(balance.upnl, 1.75);
    assert!(fills.borrow().is_empty(), "OKX account topic emits no fill");
    assert!(h.runtime.reports.borrow().is_empty());
    drop(balance);
    h.venue().dispose().unwrap();
}

#[test]
fn private_account_frame_updates_balance_and_emits_no_fill() {
    for name in private_topic_names() {
        private_account_frame_updates_balance_and_emits_no_fill_for_name(name);
    }
}

fn record_errors(h: &Harness) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
    let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, data| {
            sink.borrow_mut().push(data.to_display());
            Ok(())
        }),
    );
    errors
}

// ts: okx.api.ts formatOkxApiErrorMessage through the createAPI response interceptor,
// observed by setLeverage's `err?.response?.data?.msg || err?.message`.
fn rejected_set_leverage(body: Value) -> (String, Vec<String>) {
    let h = harness(ExchangeName::Okx, options(), |http| {
        http.route("POST", SET_LEVERAGE, 200.0, body.clone());
        okx(http);
    });
    loaded(&h);
    let positions = h.drive(h.venue().fetch_positions(None)).expect("positions");
    Store::add_or_update_positions(&*h.store, positions).expect("store positions");
    let errors = record_errors(&h);
    let error = h
        .drive(h.venue().set_leverage("BTCUSDT".into(), 5.0, None))
        .expect_err("setLeverage rejects");
    let emitted = errors.borrow().clone();
    (error.message, emitted)
}

#[test]
fn okx_api_error_without_msg_names_the_full_code() {
    let (message, emitted) = rejected_set_leverage(json!({"code": "50011", "msg": "", "data": []}));
    assert_eq!(message, "OKX API error (code 50011)");
    assert!(
        emitted
            .iter()
            .any(|e| e.contains("OKX API error (code 50011)")),
        "{emitted:?}"
    );
}

#[test]
fn okx_api_error_row_message_carries_its_code() {
    let (message, _) = rejected_set_leverage(json!({
        "code": "1", "msg": "",
        "data": [{"sCode": "0", "sMsg": ""}, {"sCode": "51008", "sMsg": " Insufficient margin "}]
    }));
    assert_eq!(message, "Insufficient margin (code 51008)");
}

// setLeverage reads the attached response's raw msg before the trimmed error message.
#[test]
fn okx_api_error_top_level_msg_reaches_the_caller_through_response() {
    let (message, _) = rejected_set_leverage(
        json!({"code": "51603", "msg": " Order does not exist ", "data": []}),
    );
    assert_eq!(message, " Order does not exist ");
}

#[test]
fn okx_place_rejection_reads_the_attached_response() {
    let h = harness(ExchangeName::Okx, options(), |http| {
        http.route(
            "POST",
            PLACE,
            200.0,
            json!({"code": "1", "msg": "All operations failed",
                   "data": [{"sCode": "51008", "sMsg": "Insufficient margin"}]}),
        );
        okx(http);
    });
    loaded(&h);
    let errors = record_errors(&h);
    let error = h
        .drive(Box::pin(
            h.venue().base().place_order(h.venue(), limit_order(10.0)),
        ))
        .expect_err("placeOrder rejects");
    assert_eq!(error.message, "Insufficient margin (code 51008)");
    assert!(
        errors
            .borrow()
            .iter()
            .any(|e| e.contains("Insufficient margin (code 51008)")),
        "{:?}",
        errors.borrow()
    );
}

// ts: okx.exchange.ts fetchIncomeHistory `rowsSeen += rows.length`.
#[test]
fn fetch_income_history_maps_a_nonempty_bills_page() {
    let h = harness(ExchangeName::Okx, options(), |http| {
        okx(http);
        http.ok(
            "GET",
            "/api/v5/account/bills",
            json!([{"billId": "1", "ts": "1700000000000", "balChg": "-0.1", "fee": "-0.1",
                    "pnl": "0", "type": "2", "instId": "BTC-USDT-SWAP", "ccy": "USDT"}]),
        );
    });
    loaded(&h);
    let errors = record_errors(&h);
    let records = h
        .drive(h.venue().fetch_income_history(None))
        .expect("fetchIncomeHistory");
    assert!(errors.borrow().is_empty(), "{:?}", errors.borrow());
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].income_type, "COMMISSION");
    assert_eq!(records[0].income, -0.1);
    assert_eq!(records[0].symbol, "BTCUSDT");
}

// ts: okx.ws-business.ts resyncsPrivateStateOnReopen -> base.ws.ts trackConnected runs
// fetchAndSetPrivateSnapshot when only the business (algo orders) socket re-opens.
#[test]
fn business_socket_reopen_alone_resyncs_the_private_snapshot() {
    use safe_cex_exchange::websocket::WsEvent;
    let h = harness(ExchangeName::Okx, options(), okx);
    h.drive(h.venue().start()).expect("start");
    for _ in 0..100 {
        h.runtime.advance(50.0);
    }
    let business = |h: &Harness| {
        h.runtime
            .sockets
            .borrow()
            .iter()
            .filter(|(url, _)| url.ends_with("/ws/v5/business"))
            .count()
    };
    assert_eq!(business(&h), 1, "one business socket after start");
    let before = h.http.calls(ALGO_ORDERS).len();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/ws/v5/business"))
        .expect("business socket")
        .1
        .clone();
    socket.closed.set(true);
    socket.events.borrow_mut().push_back(WsEvent::Close {
        code: Some(1006),
        reason: "abnormal".into(),
    });
    for _ in 0..200 {
        h.runtime.advance(50.0);
    }
    assert_eq!(business(&h), 2, "business socket re-dialled");
    assert!(
        h.http.calls(ALGO_ORDERS).len() > before,
        "business re-open fetched the private snapshot ({before} algo reads before)"
    );
    h.venue().dispose().expect("dispose");
}

#[test]
fn eu_order_reads_start_both_instruments_before_first_reply() {
    use std::{
        cell::Cell,
        rc::Rc,
        task::{Context, Poll, Waker},
    };
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/parallel-reads-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual source parallel reads: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let mut observations = vec![];
    for path in [ORDERS, ALGO_ORDERS] {
        let h = harness(ExchangeName::Okxeu, options(), |http| {
            okx(http);
            http.routes
                .borrow_mut()
                .retain(|(_, path, _)| path != ORDERS);
            http.ok("GET", ORDERS, json!([]));
        });
        loaded(&h);
        h.http.requests.borrow_mut().clear();
        let gate = Rc::new(Cell::new(false));
        *h.http.held_instrument.borrow_mut() = Some((path.into(), "SWAP".into(), gate.clone()));
        let mut future = h.venue().fetch_orders(None);
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        h.runtime.pump();
        let instruments = h
            .http
            .calls(path)
            .iter()
            .map(|r| r.params["instType"].clone())
            .collect::<Vec<_>>();
        observations.push(json!({"path":path,"instruments":instruments}));
        gate.set(true);
        let orders = h.drive(future).expect("both instrument requests settle");
        assert!(
            orders.is_empty(),
            "empty instrument replies complete without an error"
        );
        h.venue().dispose().unwrap();
    }
    assert_eq!(
        json!(observations),
        source,
        "OKX EU both source parallel reads start before SWAP completes for normal and algo orders"
    );
}

#[test]
fn public_market_data_frames_reach_real_book_ticker_and_candle_bindings() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let h = harness(ExchangeName::Okx, options(), okx);
    h.drive(h.venue().start()).unwrap();
    h.runtime.advance(50.0);
    let public = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/ws/v5/public"))
        .unwrap()
        .1
        .clone();
    let business = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/ws/v5/business"))
        .unwrap()
        .1
        .clone();
    let books = Rc::new(RefCell::new(Vec::new()));
    let seen = books.clone();
    let stop_book = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |book| seen.borrow_mut().push(book)),
        )
        .unwrap();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let seen = candles.clone();
    let options: OHLCVOptions =
        serde_json::from_value(json!({"symbol":"BTCUSDT", "interval":"1m"})).unwrap();
    let stop_candle = h
        .venue()
        .listen_ohlcv(
            options,
            Rc::new(move |candle| seen.borrow_mut().push(candle)),
        )
        .unwrap();
    let frame = |socket: &Rc<harness::Socket>, value: Value| {
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(value.to_string())));
        for _ in 0..8 {
            h.runtime.pump();
        }
    };
    frame(
        &public,
        json!({"arg":{"channel":"tickers","instId":"BTC-USDT-SWAP"},"data":[{
            "instId":"BTC-USDT-SWAP","last":"50000","open24h":"40000", "bidPx":"49998", "askPx":"50002", "volCcy24h":"12", "vol24h":"34"
        }]}),
    );
    let ticker = h.store.tickers_by_symbol()["BTCUSDT"].clone();
    assert_eq!(
        (
            ticker.bid,
            ticker.ask,
            ticker.last,
            ticker.percentage,
            ticker.volume,
            ticker.quote_volume
        ),
        (49998.0, 50002.0, 50000.0, 25.0, 12.0, 34.0)
    );
    frame(
        &public,
        json!({"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"snapshot","data":[{
            "seqId":100,"checksum":-999,"bids":[["49999","2"],["49998","3"]],"asks":[["50001","4"],["50002","0"]]
        }]}),
    );
    assert_eq!(
        books.borrow().len(),
        1,
        "OKX real binding delivers snapshot"
    );
    assert_eq!(
        (
            books.borrow()[0].bids[1].amount,
            books.borrow()[0].bids[1].total
        ),
        (3.0, 5.0)
    );
    frame(
        &public,
        json!({"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"update","data":[{
            "seqId":101,"prevSeqId":100,"bids":[["49999","0"],["49997","7"]],"asks":[["50001","6"]]
        }]}),
    );
    assert_eq!(
        books.borrow().len(),
        2,
        "OKX real binding delivers contiguous delta"
    );
    assert_eq!(
        (
            books.borrow()[1].bids[0].price,
            books.borrow()[1].bids[1].total
        ),
        (49998.0, 10.0)
    );
    let before = public.sent.borrow().len();
    frame(
        &public,
        json!({"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"update","data":[{
            "seqId":103,"prevSeqId":102,"bids":[],"asks":[]
        }]}),
    );
    assert_eq!(
        books.borrow().len(),
        2,
        "sequence gap must suppress callback"
    );
    let sent = public.sent.borrow();
    assert_eq!(
        sent.len(),
        before + 2,
        "sequence gap resubscribes exactly once"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&sent[before]).unwrap()["op"],
        "unsubscribe"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&sent[before + 1]).unwrap()["op"],
        "subscribe"
    );
    drop(sent);
    frame(
        &public,
        json!({"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"snapshot","data":[{
            "seqId":200,"bids":[["49996","1"]],"asks":[["50003","2"]]
        }]}),
    );
    assert_eq!(books.borrow().len(), 3, "fresh snapshot ends recovery");
    assert_eq!(books.borrow()[2].bids.len(), 1);
    frame(
        &business,
        json!({"arg":{"channel":"candle1m","instId":"BTC-USDT-SWAP"},"data":[["1700000000000","10","13","9","12","999","888","77","1"]]}),
    );
    assert_eq!(
        candles.borrow().len(),
        1,
        "OKX real business binding delivers candle"
    );
    let candle = candles.borrow()[0].clone();
    assert_eq!(
        (
            candle.timestamp,
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume
        ),
        (1700000000.0, 10.0, 13.0, 9.0, 12.0, 77.0)
    );
    stop_book().unwrap();
    stop_candle().unwrap();
    h.venue().dispose().unwrap();
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
}
