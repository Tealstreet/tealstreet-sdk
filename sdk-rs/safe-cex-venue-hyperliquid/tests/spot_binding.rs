//! Production Spot class through every required Exchange method.
#[allow(dead_code)]
#[path = "support/binding_runtime.rs"]
mod host;
use host::{Host, Routes};
use safe_cex_exchange::{exchange::Exchange, js_runtime::value::Js, runtime::ExchangeSigners};
use safe_cex_store::store::DefaultStore;
use safe_cex_venue_hyperliquid::{
    binding::{HyperliquidDependencies, RuntimeClock},
    rate_limit::RateLimiter,
    spot_binding::HyperliquidSpotExchange,
};
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};
const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
fn address() -> String {
    safe_cex_crypto::wallet_utils::create_wallet_from_private_key(KEY)
        .unwrap()
        .address()
        .to_string()
}
struct Fixture {
    host: Rc<Host>,
    routes: Arc<Routes>,
    exchange: Rc<HyperliquidSpotExchange>,
}
#[allow(clippy::arc_with_non_send_sync)]
fn fixture() -> Fixture {
    let host = Rc::new(Host::default());
    let routes = Routes::new(vec![
        (
            "info:spotMetaAndAssetCtxs",
            json!([
                {"tokens":[{"name":"USDC","szDecimals":6,"weiDecimals":8,"index":0},{"name":"PURR","szDecimals":2,"weiDecimals":8,"index":1}],
                 "universe":[{"name":"@0","tokens":[1,0],"index":0,"isCanonical":true}]},
                [{"coin":"@0","midPx":"1.2","markPx":"1.25","prevDayPx":"1.0","dayNtlVlm":"120","dayBaseVlm":"100"}]
            ]),
        ),
        (
            "info:spotClearinghouseState",
            json!({"balances":[
            {"coin":"USDC","token":0,"hold":"2","total":"100","entryNtl":"0"},
            {"coin":"PURR","token":1,"hold":"0","total":"10","entryNtl":"8"}
        ], "tokenToAvailableAfterMaintenance":[]}),
        ),
        (
            "exchange:order",
            json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":777}}]}}}),
        ),
        (
            "exchange:cancel",
            json!({"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}),
        ),
        (
            "exchange:updateLeverage",
            json!({"status":"ok","response":{"type":"default"}}),
        ),
    ]);
    let exchange = HyperliquidSpotExchange::new(
        serde_json::from_value(json!({"key":address(),"secret":KEY,"applicationId":address()}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: host.clone(),
            http: routes.clone(),
            limiter: Rc::new(RateLimiter::new(
                Arc::new(RuntimeClock::new(host.clone())),
                1200.0,
                60000.0,
            )),
            signers: ExchangeSigners::default(),
            external_public_data: None,
        },
    )
    .unwrap();
    Fixture {
        host,
        routes,
        exchange,
    }
}
fn started() -> Fixture {
    let f = fixture();
    f.host.block_on(f.exchange.start()).unwrap_or_else(|e| {
        panic!(
            "start {e:?}; reports {:?}; console {:?}",
            f.host.reports.borrow(),
            f.host.console.borrow()
        )
    });
    f
}
fn body(f: &Fixture, route: &str) -> Value {
    f.routes
        .requests
        .borrow()
        .iter()
        .rev()
        .find(|(r, _)| r == route)
        .unwrap()
        .1
        .clone()
}
fn order() -> safe_cex_types::Order {
    serde_json::from_value(json!({"id":"555","symbol":"PURR/USDC","status":"open","type":"limit","side":"buy","price":1.2,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":1700000000000u64})).unwrap()
}
#[test]
fn bound_start_loads_spot_markets_balance_and_both_sockets() {
    let f = started();
    assert_eq!(
        f.exchange.name(),
        &safe_cex_types::ExchangeName::Hyperliquidspot
    );
    assert!(f.exchange.base().is_spot);
    let store = &f.exchange.base().store;
    assert_eq!(store.markets().len(), 1);
    assert_eq!(store.markets()[0].symbol, "PURR/USDC");
    assert_eq!(
        store.markets()[0].info.as_ref().unwrap()["hyperliquidId"].as_f64(),
        Some(10000.0)
    );
    assert!(store.loaded().markets && store.loaded().tickers);
    assert!(
        store.tickers()[0].index.is_nan(),
        "source parseFloat(undefined) survives native Store"
    );
    assert!(
        store.tickers()[0].funding_rate.is_nan(),
        "source parseFloat(undefined) survives native Store"
    );
    assert_eq!(f.routes.count("info:spotMetaAndAssetCtxs"), 1);
    assert_eq!(f.routes.count("info:spotClearinghouseState"), 1);
    assert_eq!(f.host.sockets.borrow().len(), 2);
    assert!(
        f.host.reports.borrow().is_empty(),
        "{:?}",
        f.host.reports.borrow()
    );
}
#[test]
fn bound_validate_account_checks_real_wallet_pair() {
    let f = fixture();
    let result = f.host.block_on(f.exchange.validate_account()).unwrap();
    assert_eq!(serde_json::to_value(result.error).unwrap(), json!(""));
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_fetch_positions_preserves_source_store_snapshot_without_writes() {
    let f = started();
    let old = f.exchange.base().store.positions().to_vec();
    assert!(!old.is_empty());
    let result = f
        .host
        .block_on(f.exchange.fetch_positions(Some(vec!["OTHER".into()])))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::to_value(&old).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&*f.exchange.base().store.positions()).unwrap(),
        serde_json::to_value(old).unwrap()
    );
}
#[test]
fn bound_fetch_orders_preserves_source_empty_result_without_http() {
    let f = fixture();
    assert!(
        f.host
            .block_on(f.exchange.fetch_orders(None))
            .unwrap()
            .is_empty()
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_place_order_signs_shifted_asset_and_preserves_client_id() {
    let f = started();
    let opts: safe_cex_types::PlaceOrderOpts=serde_json::from_value(json!({"symbol":"PURR/USDC","type":"limit","side":"buy","amount":1,"price":1.2,"clientOrderId":"0x11111111111111111111111111111111"})).unwrap();
    let ids = f
        .host
        .block_on(f.exchange.base().place_order(f.exchange.as_ref(), opts))
        .unwrap();
    assert_eq!(
        ids,
        ["777"],
        "requests {:?}; reports {:?}; console {:?}",
        f.routes.requests.borrow(),
        f.host.reports.borrow(),
        f.host.console.borrow()
    );
    let b = body(&f, "exchange:order");
    assert_eq!(b["action"]["orders"][0]["a"], 10000);
    assert_eq!(
        b["action"]["orders"][0]["c"],
        "0x11111111111111111111111111111111"
    );
    assert!(b["signature"]["r"].is_string());
}
#[test]
fn bound_set_leverage_dispatches_source_update() {
    let f = started();
    f.host
        .block_on(f.exchange.set_leverage("PURR/USDC".into(), 3.0, None))
        .unwrap();
    let b = body(&f, "exchange:updateLeverage");
    assert_eq!(b["action"]["asset"], 10000);
    assert_eq!(b["action"]["leverage"], 3);
}
#[test]
fn bound_change_margin_type_preserves_source_absent_active_leverage() {
    let f = started();
    f.host
        .block_on(
            f.exchange
                .change_margin_type(true, Some("PURR/USDC".into())),
        )
        .unwrap();
    let b = body(&f, "exchange:updateLeverage");
    assert_eq!(b["action"]["isCross"], false);
    assert!(b["action"].get("leverage").is_none());
}
#[test]
fn bound_cancel_orders_uses_shifted_asset() {
    let f = started();
    f.host
        .block_on(f.exchange.cancel_orders(vec![order()]))
        .unwrap();
    let b = body(&f, "exchange:cancel");
    assert_eq!(b["action"]["cancels"][0], json!({"a":10000,"o":555}));
}
#[test]
fn bound_update_order_cancels_then_returns_new_order_ids() {
    let f = started();
    let opts = serde_json::from_value(json!({"order":order(),"update":{"price":1.1}})).unwrap();
    let ids = f.host.block_on(f.exchange.update_order(opts)).unwrap();
    assert_eq!(
        ids,
        ["777"],
        "requests {:?}; reports {:?}; console {:?}",
        f.routes.requests.borrow(),
        f.host.reports.borrow(),
        f.host.console.borrow()
    );
    assert_eq!(
        body(&f, "exchange:order")["action"]["orders"][0]["p"],
        "1.1"
    );
    let sent = f.routes.requests.borrow();
    let cancel = sent
        .iter()
        .position(|(r, _)| r == "exchange:cancel")
        .unwrap();
    let place = sent
        .iter()
        .position(|(r, _)| r == "exchange:order")
        .unwrap();
    assert!(cancel < place);
}
// These seven methods inherit base.ts's exact Promise rejection: the Spot
// class declares none of them (including scheduleCancel), so no network call.
#[test]
fn bound_get_account_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.get_account())
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_fetch_markets_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.fetch_markets())
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_fetch_tickers_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.fetch_tickers())
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_fetch_symbol_max_leverage_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.fetch_symbol_max_leverage("PURR/USDC".into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_change_position_mode_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.change_position_mode(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_schedule_cancel_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.schedule_cancel(None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}
#[test]
fn bound_fetch_public_trades_preserves_base_error() {
    let f = fixture();
    assert_eq!(
        f.host
            .block_on(f.exchange.fetch_public_trades("PURR/USDC".into(), None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert!(f.routes.requests.borrow().is_empty());
}

#[test]
fn bound_order_id_boundary_preserves_numeric_keys_holes_and_source_zero() {
    let f = started();
    f.routes
        .routes
        .borrow_mut()
        .iter_mut()
        .find(|(key, _)| key == "exchange:order")
        .unwrap()
        .1 = json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":0}},{"filled":{"oid":99}},{"resting":{"oid":"last"}}]}}});
    let opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"PURR/USDC","type":"limit","side":"buy","amount":1,"price":1.2}),
    )
    .unwrap();
    assert_eq!(
        f.host
            .block_on(f.exchange.base().place_order(f.exchange.as_ref(), opts))
            .unwrap(),
        ["0", "last"]
    );
}

#[test]
fn invalid_client_id_and_invalid_batch_fail_before_any_order_request() {
    let f = started();
    let opts: safe_cex_types::PlaceOrderOpts=serde_json::from_value(json!({"symbol":"PURR/USDC","type":"limit","side":"buy","amount":1,"price":1.2,"clientOrderId":"bad"})).unwrap();
    assert!(
        f.host
            .block_on(
                f.exchange
                    .base()
                    .place_order(f.exchange.as_ref(), opts.clone())
            )
            .is_err()
    );
    let mut valid = opts.clone();
    valid.client_order_id = None;
    assert!(
        f.host
            .block_on(
                f.exchange
                    .base()
                    .place_orders(f.exchange.as_ref(), vec![valid, opts])
            )
            .is_err()
    );
    assert_eq!(f.routes.count("exchange:order"), 0);
}

#[test]
fn bound_ohlcv_reads_candles_with_source_interval_and_times() {
    let f = started();
    f.routes.routes.borrow_mut().push((
        "info:candleSnapshot".into(),
        json!({"data":[{"t":1700000000000u64,"o":"1","h":"2","l":"0.5","c":"1.2","v":"4"}]}),
    ));
    let opts = serde_json::from_value(
        json!({"symbol":"PURR/USDC","interval":"1m","limit":2,"to":1700000000000u64}),
    )
    .unwrap();
    let candles = f.host.block_on(f.exchange.fetch_ohlcv(opts)).unwrap();
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].timestamp, 1700000000.0);
    assert_eq!(candles[0].close, 1.2);
    assert_eq!(
        body(&f, "info:candleSnapshot"),
        json!({"type":"candleSnapshot","req":{"coin":"@0","interval":"1m","startTime":1699999880000u64,"endTime":1700000000000u64}})
    );
}

#[test]
fn bound_public_candle_listener_dispatches_source_event_and_cleanup() {
    use safe_cex_exchange::websocket::WsEvent;
    use std::cell::RefCell;
    let f = started();
    let wire = f.host.sockets.borrow()[0].clone();
    wire.push(WsEvent::Open);
    f.host.settle();
    let values = Rc::new(RefCell::new(Vec::new()));
    let seen = values.clone();
    let opts = serde_json::from_value(json!({"symbol":"PURR/USDC","interval":"1m"})).unwrap();
    let cleanup = f
        .exchange
        .listen_ohlcv(opts, Rc::new(move |v| seen.borrow_mut().push(v)))
        .unwrap();
    assert!(
        wire.sent.borrow().iter().any(|v| v.contains("candle")),
        "{:?}",
        wire.sent.borrow()
    );
    let event=json!({"channel":"candle","data":[{"s":"@0","i":"1m","t":123000,"o":"1","h":"2","l":"0","c":"1.5","v":"4"}]}).to_string();
    wire.push(WsEvent::Message(
        safe_cex_transport::socket::SocketFrame::Text(event),
    ));
    f.host.settle();
    assert_eq!(
        values.borrow().len(),
        1,
        "reports {:?}",
        f.host.reports.borrow()
    );
    assert_eq!(values.borrow()[0].timestamp, 123.0);
    assert_eq!(values.borrow()[0].close, 1.5);
    let sent = wire.sent.borrow().len();
    cleanup().unwrap();
    // Source candle cleanup removes the parser and statistics without sending
    // an unsubscribe. Subsequent matching frames must not reach the listener.
    wire.push(WsEvent::Message(safe_cex_transport::socket::SocketFrame::Text(json!({"channel":"candle","data":[{"s":"@0","i":"1m","t":124000,"o":"1","h":"2","l":"0","c":"1.5","v":"4"}]}).to_string())));
    f.host.settle();
    assert_eq!(values.borrow().len(), 1);
    assert_eq!(wire.sent.borrow().len(), sent);
}

#[test]
fn bound_public_book_listener_dispatches_source_levels_and_cleanup() {
    use safe_cex_exchange::websocket::WsEvent;
    use std::cell::RefCell;
    let f = started();
    let wire = f.host.sockets.borrow()[0].clone();
    wire.push(WsEvent::Open);
    f.host.settle();
    let values = Rc::new(RefCell::new(Vec::new()));
    let seen = values.clone();
    let cleanup = f
        .exchange
        .listen_order_book(
            "PURR/USDC".into(),
            Rc::new(move |v| seen.borrow_mut().push(v)),
        )
        .unwrap();
    wire.push(WsEvent::Message(safe_cex_transport::socket::SocketFrame::Text(json!({"channel":"l2Book","data":{"coin":"@0","levels":[[{"px":"1.1","sz":"2"},{"px":"1.0","sz":"1"}],[{"px":"1.2","sz":"3"},{"px":"1.3","sz":"1"}]]}}).to_string())));
    f.host.settle();
    assert_eq!(
        values.borrow().len(),
        1,
        "reports {:?}",
        f.host.reports.borrow()
    );
    assert_eq!(values.borrow()[0].bids[0].price, 1.1);
    assert_eq!(values.borrow()[0].asks[0].amount, 3.0);
    cleanup().unwrap();
    assert!(
        wire.sent
            .borrow()
            .iter()
            .any(|v| v.contains("unsubscribe") && v.contains("l2Book"))
    );
}

fn private_socket(f: &Fixture) -> Rc<host::Wire> {
    for wire in f.host.sockets.borrow().iter() {
        wire.push(safe_cex_exchange::websocket::WsEvent::Open);
    }
    f.host.settle();
    f.host
        .sockets
        .borrow()
        .iter()
        .find(|wire| {
            wire.sent.borrow().iter().any(|text| {
                serde_json::from_str::<Value>(text)
                    .is_ok_and(|v| v["subscription"]["type"] == "userFills")
            })
        })
        .expect("actual Spot private subscription")
        .clone()
}

fn private_frame(f: &Fixture, wire: &host::Wire, value: Value) {
    wire.push(safe_cex_exchange::websocket::WsEvent::Message(
        safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
    ));
    f.host.settle();
    assert!(
        f.host.reports.borrow().is_empty(),
        "{:?}",
        f.host.reports.borrow()
    );
}

fn observe_private_fills(f: &Fixture) -> Rc<std::cell::RefCell<Vec<Js>>> {
    let values = Rc::new(std::cell::RefCell::new(vec![]));
    let saved = values.clone();
    f.exchange.base().on(
        "fill",
        Rc::new(move |_, value| {
            saved.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    values
}

#[test]
fn private_spot_order_updates_preserve_rest_owned_orders_without_emitting_fills() {
    let f = started();
    let wire = private_socket(&f);
    let store = &f.exchange.base().store;
    store.add_or_update_orders(vec![order()]).unwrap();
    let orders = store.orders().to_vec();
    let positions = store.positions().to_vec();
    let balance = store.balance().clone();
    let fills = observe_private_fills(&f);
    private_frame(
        &f,
        &wire,
        json!({"channel":"orderUpdates","data":[{
            "order":{"oid":555,"coin":"@0","side":"B","limitPx":"9","sz":"0","origSz":"1"},
            "status":"filled","statusTimestamp":1700000000000u64
        }]}),
    );
    assert_eq!(&*store.orders(), orders.as_slice());
    assert_eq!(&*store.positions(), positions.as_slice());
    assert_eq!(&*store.balance(), &balance);
    assert!(
        fills.borrow().is_empty(),
        "source has no orderUpdates handler"
    );
}

#[test]
fn private_spot_live_fills_emit_exact_payload_and_refresh_real_balance() {
    let f = started();
    let wire = private_socket(&f);
    let store = &f.exchange.base().store;
    store.add_or_update_orders(vec![order()]).unwrap();
    let orders = store.orders().to_vec();
    let fills = observe_private_fills(&f);
    let fill = json!({"tid":123,"coin":"@0","side":"B","px":"1.375","sz":"0.25"});
    private_frame(
        &f,
        &wire,
        json!({"channel":"userFills","data":{
            "isSnapshot":true,"user":address(),"fills":[fill.clone()]
        }}),
    );
    assert!(fills.borrow().is_empty(), "snapshot fills do not emit");
    f.host.advance(1500.0);
    let before = f.routes.count("info:spotClearinghouseState");
    for (route, data) in f.routes.routes.borrow_mut().iter_mut() {
        if route == "info:spotClearinghouseState" {
            *data = json!({"balances":[{"coin":"USDC","token":0,"hold":"5","total":"200"}]});
        }
    }
    private_frame(
        &f,
        &wire,
        json!({"channel":"userFills","data":{
            "isSnapshot":false,"user":address(),"fills":[fill]
        }}),
    );
    {
        let fills = fills.borrow();
        assert_eq!(fills.len(), 1);
        let fill = &fills[0];
        assert_eq!(fill.own_keys().len(), 5);
        for (key, expected) in [("id", "123"), ("side", "buy"), ("symbol", "PURR/USDC")] {
            assert!(fill.has_own(key));
            assert!(fill.get(key).unwrap().strict_equals(&Js::str(expected)));
        }
        for (key, expected) in [("price", 1.375_f64), ("amount", 0.25_f64)] {
            assert!(fill.has_own(key));
            assert_eq!(
                fill.get(key).unwrap().as_f64().unwrap().to_bits(),
                expected.to_bits()
            );
        }
    }
    assert_eq!(f.routes.count("info:spotClearinghouseState"), before + 1);
    assert_eq!(
        (
            store.balance().total,
            store.balance().used,
            store.balance().free
        ),
        (200.0, 5.0, 195.0)
    );
    assert_eq!(
        &*store.orders(),
        orders.as_slice(),
        "Spot fills do not patch orders"
    );
}

#[test]
fn private_spot_balance_channels_publish_native_amounts_and_notional() {
    let f = started();
    let wire = private_socket(&f);
    let fills = observe_private_fills(&f);
    for (channel, nested, usdc, purr_amount) in [
        ("spotClearinghouseState", false, 200.0, 20.0),
        ("spotState", true, 300.0, 30.0),
    ] {
        let balances = json!([
            {"coin":"USDC","token":0,"hold":"5","total":usdc.to_string()},
            {"coin":"PURR","token":1,"hold":"2","total":purr_amount.to_string()}
        ]);
        let data = if nested {
            json!({"spotState":{"balances":balances}})
        } else {
            json!({"balances":balances})
        };
        private_frame(&f, &wire, json!({"channel":channel,"data":data}));
        let balance = f.exchange.base().store.balance();
        let total = usdc + purr_amount * 1.25;
        assert_eq!(
            (balance.total, balance.used, balance.free, balance.upnl),
            (total, 7.5, total - 7.5, 0.0),
            "{channel}"
        );
        let purr = balance.currencies.as_ref().unwrap()["PURR"]
            .as_balance()
            .unwrap();
        assert_eq!(
            (purr.total, purr.used, purr.free),
            (purr_amount, 2.0, purr_amount - 2.0)
        );
    }
    assert!(fills.borrow().is_empty());
}

#[test]
fn private_spot_asset_data_updates_closed_position_configuration() {
    let f = started();
    let wire = private_socket(&f);
    let store = &f.exchange.base().store;
    let mut positions = store.positions().to_vec();
    assert_eq!(positions.len(), 1);
    positions[0].leverage = 7.0;
    positions[0].margin_type = safe_cex_types::MarginType::Isolated.into();
    positions[0].position_mode = safe_cex_types::PositionMode::Hedged.into();
    store.add_or_update_positions_full(positions).unwrap();
    let fills = observe_private_fills(&f);
    private_frame(
        &f,
        &wire,
        json!({"channel":"activeAssetData","data":{
            "coin":"@0","leverage":{"type":"isolated","value":50}
        }}),
    );
    let positions = store.positions();
    assert_eq!(positions[0].symbol, "PURR/USDC");
    assert_eq!(positions[0].contracts, 0.0);
    assert_eq!(positions[0].leverage, 1.0);
    assert_eq!(positions[0].margin_type, safe_cex_types::MarginType::Cross);
    assert_eq!(
        positions[0].position_mode,
        safe_cex_types::PositionMode::OneWay
    );
    assert!(fills.borrow().is_empty());
}
