//! Production socket pump -> source private receiver -> real Store and emitter.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_exchange::{
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, WsEvent},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, OrderStatus};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn setup(
    name: ExchangeName,
    opts: safe_cex_types::ExchangeOptions,
) -> (support::Harness, Rc<BaseWebSocket>) {
    let h = support::harness_as(name, opts, |_| {});
    let market = serde_json::from_value(json!({
        "id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.001,"price":0.1},
        "limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":100}}
    }))
    .unwrap();
    let mut balance = h.store.balance().clone();
    balance.total = 1000.;
    balance.free = 1000.;
    balance.upnl = 0.;
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            balance: Some(balance),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let socket = h.venue().base().get_socket(SocketSlot::Private).unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    (h, socket)
}
fn feed(h: &support::Harness, value: Value) {
    h.runtime.sockets.borrow()[0]
        .1
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(value.to_string())));
    h.runtime.pump();
}
fn order(id: &str, symbol: &str, status: &str) -> Value {
    json!({"orderId":id,"orderLinkId":"caller-1","symbol":symbol,"side":"Buy",
        "orderType":"Limit","price":"25000","qty":"0.01","leavesQty":"0.01",
        "cumExecQty":"0","orderStatus":status,"timeInForce":"GTC","reduceOnly":false,
        "closeOnTrigger":false,"stopOrderType":"","triggerPrice":"","stopLoss":"24000",
        "takeProfit":"","tpslMode":"","createType":"CreateByUser","positionIdx":0,
        "triggerBy":"","slTriggerBy":"MarkPrice","rejectReason":"EC_NoError"})
}
fn position(symbol: &str, upnl: &str, size: &str) -> Value {
    json!({"symbol":symbol,"side":"Buy","size":size,"avgPrice":"25000","positionValue":"12500",
        "leverage":"10","unrealisedPnl":upnl,"curRealisedPnl":"-1","liqPrice":"20000",
        "positionIdx":0,"positionIM":"1250","updatedTime":"1700000000000","seq":7,"positionStatus":"Normal"})
}

#[test]
fn production_private_frames_publish_orders_incremental_fills_and_terminal_statuses() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitspot,
    ] {
        let (h, _) = setup(name, support::options());
        let fills = Rc::new(RefCell::new(vec![]));
        let observed = fills.clone();
        h.venue().base().on(
            "fill",
            Rc::new(move |_, value| {
                let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
                observed.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        feed(
            &h,
            json!({"topic":"order","data":[order("o1","BTCUSDT","New")]}),
        );
        assert_eq!(
            h.store.orders().len(),
            2,
            "private event creates parent and virtual stop"
        );
        assert_eq!(h.store.orders()[0].id, "o1");
        let mut partial = order("o1", "BTCUSDT", "PartiallyFilled");
        partial["cumExecQty"] = json!("0.004");
        partial["cumExecValue"] = json!("100");
        partial["avgPrice"] = json!("25000");
        feed(&h, json!({"topic":"order","data":[partial.clone()]}));
        feed(&h, json!({"topic":"order","data":[partial.clone()]}));
        assert_eq!(
            fills.borrow().len(),
            1,
            "replayed cumulative fill emits once"
        );
        partial["cumExecQty"] = json!("0.006");
        partial["cumExecValue"] = json!("152");
        feed(&h, json!({"topic":"order","data":[partial.clone()]}));
        assert_eq!(
            *fills.borrow(),
            vec![
                json!({"id":"o1","side":"buy","symbol":"BTCUSDT","price":25000.0,"amount":0.004}),
                json!({"id":"o1","side":"buy","symbol":"BTCUSDT","price":26000.0,"amount":0.002}),
            ],
            "source uses execution delta value/qty, never the limit qty"
        );
        partial["orderStatus"] = json!("Filled");
        partial["cumExecQty"] = json!("0.01");
        partial["cumExecValue"] = json!("256");
        feed(&h, json!({"topic":"order","data":[partial]}));
        assert_eq!(fills.borrow().len(), 3);
        assert!(
            h.store
                .orders()
                .iter()
                .all(|o| o.status == OrderStatus::Closed)
        );
        for (i, status) in ["Cancelled", "Deactivated", "Rejected"]
            .into_iter()
            .enumerate()
        {
            let mut row = order(&format!("terminal-{i}"), "BTCUSDT", status);
            if status == "Rejected" {
                row["rejectReason"] = json!("EC_TestReject");
            }
            feed(&h, json!({"topic":"order","data":[row]}));
            let rows: Vec<_> = h
                .store
                .orders()
                .iter()
                .filter(|o| o.id.starts_with(&format!("terminal-{i}")))
                .cloned()
                .collect();
            assert_eq!(
                rows.len(),
                2,
                "terminal parent and virtual stop were delivered"
            );
            assert!(rows.iter().all(|o| o.status == OrderStatus::Canceled));
        }
        assert!(
            h.runtime
                .logs
                .borrow()
                .iter()
                .any(|s| s.contains("EC_TestReject"))
        );
        assert!(
            h.wire.requests.borrow().is_empty(),
            "private events do not refetch HTTP"
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn production_position_frames_update_positions_balance_and_filter_untraded_categories() {
    let (h, socket) = setup(ExchangeName::Bybit, support::options());
    feed(
        &h,
        json!({"topic":"position","data":[position("BTCUSDT","12.5","0.5"), position("BTCUSD","99","200"), position("BTC-30DEC26-100000-C","88","2")]}),
    );
    assert_eq!(
        h.store.positions().len(),
        1,
        "default linear-only connection filters inverse/options"
    );
    assert_eq!(h.store.positions()[0].contracts, 0.5);
    assert_eq!(h.store.positions()[0].unrealized_pnl, 12.5);
    assert_eq!(
        h.store.balance().upnl,
        12.5,
        "private position synchronizes balance upnl"
    );
    assert!(
        socket
            .message_handlers_stats
            .borrow()
            .contains_key("position")
    );
    feed(
        &h,
        json!({"topic":"position","data":[position("BTCUSDT","0","0")]}),
    );
    assert_eq!(h.store.positions()[0].contracts, 0.);
    assert_eq!(h.store.balance().upnl, 0.);
    let before = serde_json::to_value(h.store.balance().clone()).unwrap();
    feed(
        &h,
        json!({"topic":"wallet","data":[{"totalEquity":"999999"}]}),
    );
    feed(
        &h,
        json!({"topic":"execution","data":[{"orderId":"not-an-order","execQty":"999","execPrice":"1"}]}),
    );
    assert_eq!(
        serde_json::to_value(h.store.balance().clone()).unwrap(),
        before,
        "source ignores wallet topic"
    );
    assert!(
        h.store.orders().is_empty(),
        "source ignores execution topic"
    );
    feed(&h, json!({"op":"pong"}));
    assert!(socket.message_handlers_stats.borrow().contains_key("pong"));
    h.venue().dispose().unwrap();
    socket
        .on_message(&SocketFrame::Text(
            json!({"topic":"order","data":[order("after-dispose","BTCUSDT","New")]}).to_string(),
        ))
        .unwrap();
    assert!(
        h.store.orders().is_empty(),
        "disposed delivery has no effects"
    );
}

#[test]
fn enabled_inverse_private_orders_retain_native_units_and_false_partial_is_ignored() {
    let mut opts = support::options();
    opts.enable_inverse_markets = Some(true);
    let (h, _) = setup(ExchangeName::Bybit, opts);
    let mut inverse = order("inverse", "BTCUSD", "New");
    inverse["qty"] = json!("250");
    inverse["leavesQty"] = json!("200");
    inverse["cumExecQty"] = json!("50");
    feed(&h, json!({"topic":"order","data":[inverse]}));
    assert_eq!(h.store.orders()[0].remaining, 0.008);
    let count = h.store.orders().len();
    let mut false_partial = order("false-positive", "BTCUSDT", "PartiallyFilled");
    false_partial["price"] = json!("0");
    false_partial["qty"] = json!("0");
    feed(&h, json!({"topic":"order","data":[false_partial]}));
    assert_eq!(
        h.store.orders().len(),
        count,
        "replacement false partial creates no order"
    );
    h.venue().dispose().unwrap();
}

fn numeric(value: &mut Value) {
    match value {
        Value::Number(n) => *n = serde_json::Number::from_f64(n.as_f64().unwrap()).unwrap(),
        Value::Array(a) => a.iter_mut().for_each(numeric),
        Value::Object(o) => o.values_mut().for_each(numeric),
        _ => {}
    }
}
fn pick(value: Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|k| value.get(*k).cloned().map(|v| ((*k).into(), v)))
            .collect(),
    )
}
fn snapshot(h: &support::Harness, fills: &RefCell<Vec<Value>>) -> Value {
    json!({"orders":h.store.orders().iter().map(|row|pick(serde_json::to_value(row).unwrap(), &["id","status","symbol","type","side","filled","remaining","amount"])).collect::<Vec<_>>(),
        "positions":h.store.positions().iter().map(|row|pick(serde_json::to_value(row).unwrap(), &["id","symbol","contracts","notional","unrealizedPnl","realizedPnl","leverage","marginSize","marginType","positionMode","option"])).collect::<Vec<_>>(),
        "upnl":h.store.balance().upnl,"fills":fills.borrow().clone()})
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn private_delivery_matches_actual_source_handler_mapper_fill_and_store_bodies() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut cases = vec![];
    let mut native = vec![];
    for mode in 0..8 {
        let mask = mode % 4;
        let spot = mode >= 4;
        let mut opts = support::options();
        opts.enable_inverse_markets = Some(mask & 1 != 0);
        opts.enable_option_markets = Some(mask & 2 != 0);
        for scene in 0..6 {
            let (h, _) = setup(
                if spot {
                    ExchangeName::Bybitspot
                } else if mask & 1 == 0 {
                    ExchangeName::Bybit
                } else {
                    ExchangeName::Bybitdemo
                },
                opts.clone(),
            );
            let mut partial = order("fill", "BTCUSDT", "PartiallyFilled");
            partial["cumExecQty"] = json!("0.004");
            partial["cumExecValue"] = json!("100");
            partial["avgPrice"] = json!("25000");
            let mut filled = partial.clone();
            filled["orderStatus"] = json!("Filled");
            filled["cumExecQty"] = json!("0.01");
            filled["cumExecValue"] = json!("256");
            let mut false_partial = partial.clone();
            false_partial["qty"] = json!("0");
            false_partial["price"] = json!("0");
            let frames = match scene {
                0 => vec![
                    json!({"topic":"order","data":[order("fill","BTCUSDT","New")]}),
                    json!({"topic":"order","data":[partial.clone()]}),
                    json!({"topic":"order","data":[partial]}),
                    json!({"topic":"order","data":[filled]}),
                ],
                1 => vec![
                    json!({"topic":"position","data":[position("BTCUSDT","12.5","0.5"), position("BTCUSD","0.01","200"), position("BTC-30DEC26-100000-C","8","2")]}),
                    json!({"topic":"position","data":[position("BTCUSDT","0","0")]}),
                ],
                2 => vec![
                    json!({"topic":"order","data":[order("c","BTCUSDT","Cancelled"), order("d","BTCUSDT","Deactivated"), order("r","BTCUSDT","Rejected")]}),
                ],
                3 => vec![
                    json!({"topic":"order","data":[false_partial]}),
                    json!({"topic":"order","data":[order("live","BTCUSDT","New")]}),
                ],
                4 => vec![
                    json!({"topic":"wallet","data":[{"totalEquity":"999"}]}),
                    json!({"topic":"execution","data":[{"execQty":"999"}]}),
                    json!("{invalid"),
                    json!({"op":"pong"}),
                ],
                5 => vec![
                    json!({"topic":"order","data":[order("linear","BTCUSDT","New"),order("inverse","BTCUSD","New"),order("option","BTC-30DEC26-100000-C","New")]}),
                    json!({"topic":"order","data":if spot { json!([]) } else { json!({}) }}),
                ],
                _ => unreachable!(),
            };
            let fills = Rc::new(RefCell::new(vec![]));
            let observed = fills.clone();
            h.venue().base().on(
                "fill",
                Rc::new(move |_, value| {
                    let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
                    observed.borrow_mut().push(value.clone());
                    Ok(())
                }),
            );
            cases
                .push(json!({"variant":if spot {"spot"} else {"primary"},"options":opts,"markets":h.store.markets().to_vec(),"frames":frames}));
            let mut snapshots = vec![];
            for frame in frames {
                if let Some(raw) = frame.as_str() {
                    h.runtime.sockets.borrow()[0]
                        .1
                        .events
                        .borrow_mut()
                        .push_back(WsEvent::Message(SocketFrame::Text(raw.into())));
                    h.runtime.pump();
                } else {
                    feed(&h, frame);
                }
                snapshots.push(snapshot(&h, &fills));
            }
            native.push(json!(snapshots));
            h.venue().dispose().unwrap();
        }
    }
    assert_eq!(
        native.len(),
        48,
        "native fixed two variants by four flag modes by six scenes"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private-event-delivery-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        48,
        "source fixed two variants by four flag modes by six scenes"
    );
    for (index, (mut native, mut source)) in native.into_iter().zip(source).enumerate() {
        numeric(&mut native);
        numeric(&mut source);
        assert_eq!(
            native, source,
            "actual source private delivery case {index}"
        );
    }
}

#[test]
fn private_auth_rejections_retry_four_times_without_inventing_wallet_or_execution_subscriptions() {
    let (h, _) = setup(ExchangeName::Bybit, support::options());
    for _ in 0..6 {
        feed(
            &h,
            json!({"op":"auth", "success":false, "ret_msg":"denied"}),
        );
    }
    assert_eq!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .filter(|line| line.contains("[WS] Syncing time with Bybit..."))
            .count(),
        4,
        "source authRetries <= 3 allows four starts"
    );
    assert!(h.store.orders().is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn private_order_sighting_clears_the_recent_placement_snapshot_grace() {
    let (h, _) = setup(ExchangeName::Bybit, support::options());
    h.wire
        .ok("POST", "/v5/order/create", json!({"orderId":"placed"}));
    h.wire.ok(
        "GET",
        "/v5/position/list",
        json!({"list":[],"nextPageCursor":""}),
    );
    h.wire.ok(
        "GET",
        "/v5/order/realtime",
        json!({"list":[], "nextPageCursor":""}),
    );
    let venue = h.exchange.as_ref();
    let opts = serde_json::from_value::<safe_cex_types::PlaceOrderOpts>(json!({
        "symbol":"BTCUSDT", "side":"buy", "type":"limit", "amount":0.01, "price":25000
    }))
    .unwrap();
    assert_eq!(
        h.drive(Box::pin(
            safe_cex_exchange::exchange::Exchange::base(venue).place_order(venue, opts)
        ))
        .unwrap(),
        ["placed"]
    );
    assert_eq!(
        h.drive(h.venue().fetch_orders(None)).unwrap().len(),
        1,
        "fresh REST omission preserves the unsighted placement"
    );
    feed(
        &h,
        json!({"topic":"order", "data":[order("placed","BTCUSDT","New")]}),
    );
    assert!(
        h.drive(h.venue().fetch_orders(None)).unwrap().is_empty(),
        "private sighting releases snapshot grace"
    );
    h.venue().dispose().unwrap();
}
