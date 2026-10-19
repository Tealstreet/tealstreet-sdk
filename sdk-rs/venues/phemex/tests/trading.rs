//! Documented g-API payloads and mutation lifecycle regressions.
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, SocketId,
};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};
use serde_json::{Value, json};

const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"69000","askRp":"69001","lastRp":"69000","markRp":"69000","indexRp":"69000","openRp":"69000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;
fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}
fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    requests(actions)
        .into_iter()
        .filter(|r| r.method != Method::Get)
        .collect()
}
fn raw(e: &mut Engine<PhemexPerpetuals>, r: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
        1001.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn respond(e: &mut Engine<PhemexPerpetuals>, r: &HttpRequest, body: Value) -> Vec<Action> {
    raw(e, r, &body.to_string())
}
fn done(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result.clone())
            } else {
                None
            }
        })
        .expect("command completed")
}
fn command(e: &mut Engine<PhemexPerpetuals>, c: Command) -> Vec<Action> {
    e.handle(1000., Event::Command(c));
    e.drain_actions().collect()
}
fn engine_with_socket(mode: &str) -> (Engine<PhemexPerpetuals>, SocketId) {
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }));
    e.start(0.);
    let a: Vec<_> = e.drain_actions().collect();
    let a = raw(&mut e, &requests(&a)[0], PRODUCTS);
    let a = raw(&mut e, &requests(&a)[0], DAILY);
    let socket = a
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    for r in requests(&a) {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let rows = if currency == "USDT" {
            vec![
                json!({"symbol":"BTCUSDT","currency":"USDT","posMode":mode,"posSide":if mode=="OneWay"{"Merged"}else{"Long"},"size":"0","leverageRr":"-10","crossMargin":true}),
            ]
        } else {
            vec![]
        };
        respond(
            &mut e,
            &r,
            json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":rows}}),
        );
    }
    e.drain_actions().for_each(drop);
    (e, socket)
}
fn engine_mode(mode: &str) -> Engine<PhemexPerpetuals> {
    engine_with_socket(mode).0
}
fn engine() -> Engine<PhemexPerpetuals> {
    engine_mode("OneWay")
}
fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.into()),
        close: None,
    }
}
fn params(r: &HttpRequest) -> Vec<(String, String)> {
    r.url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (k.into(), v.replace("%2C", ","))
        })
        .collect()
}
fn p(r: &HttpRequest, key: &str) -> Option<String> {
    params(r)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}
fn ack(r: &HttpRequest, id: &str) -> Value {
    json!({"code":0,"data":{"orderID":id,"clOrdID":p(r,"clOrdID").unwrap(),"symbol":"BTCUSDT","side":p(r,"side").unwrap(),"posSide":p(r,"posSide").unwrap(),"orderType":p(r,"ordType").unwrap(),"orderQtyRq":p(r,"orderQtyRq").unwrap(),"priceRp":p(r,"priceRp").unwrap_or("0".into()),"ordStatus":"New","execStatus":"New","timeInForce":"GoodTillCancel"}})
}
fn order(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 69000.,
        amount: 0.1,
        filled: 0.,
        remaining: 0.1,
        reduce_only: false,
        close: None,
        client_order_id: Some("old".into()),
        time_in_force: Some(TimeInForce::PostOnly),
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

#[test]
fn documented_put_query_and_rest_create_upsert() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller-1")],
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(r.method, Method::Put);
    assert_eq!(r.body, None);
    assert!(r.url.ends_with("/g-orders/create?clOrdID=caller-1&symbol=BTCUSDT&posSide=Merged&side=Buy&ordType=Limit&orderQtyRq=0.01&priceRp=69000&timeInForce=GoodTillCancel"));
    assert_eq!(e.store().orders()[0].status, OrderStatus::Creating);
    let a = respond(&mut e, r, ack(r, "real-1"));
    assert_eq!(done(&a).unwrap(), "[\"real-1\"]");
    assert_eq!(e.store().order("real-1").unwrap().status, OrderStatus::Open);
    assert_eq!(
        e.store()
            .order("real-1")
            .unwrap()
            .client_order_id
            .as_deref(),
        Some("caller-1")
    );
}

#[test]
fn whole_command_caller_preflight_before_any_write() {
    for bad in ["", &"x".repeat(41), "line\nbreak"] {
        let mut e = engine();
        let mut o = opts("valid");
        o.client_order_id = Some(bad.into());
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), o],
            },
        );
        assert!(writes(&a).is_empty());
        assert!(done(&a).is_err());
    }
    for variant in 0..4 {
        let mut e = engine();
        let mut o = opts("same");
        match variant {
            0 => o.client_order_id = Some("first".into()),
            1 => o.amount = 1.001,
            2 => o.take_profit = Some(70000.),
            _ => o.kind = OrderType::Conditional,
        };
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), o],
            },
        );
        assert!(writes(&a).is_empty());
        assert!(done(&a).is_err());
    }
}

#[test]
fn split_orders_are_sequential_exact_and_unique() {
    let mut e = engine();
    let mut o = opts("");
    o.amount = 2.123;
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    let mut r = writes(&a).pop().unwrap();
    let mut ids = Vec::new();
    for (i, qty) in ["1", "1", "0.123"].iter().enumerate() {
        assert_eq!(p(&r, "orderQtyRq").as_deref(), Some(*qty));
        let id = p(&r, "clOrdID").unwrap();
        assert!(id.starts_with("Tealstreet_"));
        assert!(!ids.contains(&id));
        ids.push(id);
        let a = respond(&mut e, &r, ack(&r, &format!("real-{i}")));
        if i < 2 {
            assert_eq!(writes(&a).len(), 1);
            r = writes(&a).pop().unwrap();
        } else {
            assert_eq!(done(&a).unwrap(), "[\"real-0\",\"real-1\",\"real-2\"]");
        }
    }
}

#[test]
fn stop_brackets_and_hedged_reduce_side() {
    let mut e = engine_mode("Hedged");
    let mut o = opts("");
    o.side = OrderSide::Sell;
    o.reduce_only = Some(true);
    o.stop_loss = Some(68000.);
    o.take_profit = Some(71000.);
    o.time_in_force = Some(TimeInForce::PostOnly);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "posSide").as_deref(), Some("Long"));
    assert_eq!(p(r, "reduceOnly"), None);
    assert_eq!(p(r, "stopLossRp").as_deref(), Some("68000"));
    assert_eq!(p(r, "slTrigger").as_deref(), Some("ByMarkPrice"));
    assert_eq!(p(r, "tpTrigger").as_deref(), Some("ByLastPrice"));
    assert_eq!(p(r, "timeInForce").as_deref(), Some("PostOnly"));
    let mut o = opts("");
    o.side = OrderSide::Sell;
    o.kind = OrderType::StopLoss;
    o.price = Some(68000.);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![o],
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "ordType").as_deref(), Some("Stop"));
    assert_eq!(p(r, "stopPxRp").as_deref(), Some("68000"));
    assert_eq!(p(r, "closeOnTrigger").as_deref(), Some("true"));
    assert_eq!(p(r, "triggerType").as_deref(), Some("ByMarkPrice"));
}

#[test]
fn trailing_has_full_position_quantity_and_signed_distance() {
    let mut e = engine();
    e.store_mut().set_positions(vec![Position {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 69000.,
        notional: 34500.,
        leverage: 10.,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.,
        realized_pnl: None,
        contracts: 0.5,
        liquidation_price: 0.,
        margin_size: None,
        last_update: None,
    }]);
    let mut o = opts("");
    o.kind = OrderType::TrailingStopLoss;
    o.side = OrderSide::Sell;
    o.price = Some(68000.);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "orderQtyRq").as_deref(), Some("0.5"));
    assert_eq!(p(r, "pegOffsetValueRp").as_deref(), Some("-1000"));
    assert_eq!(p(r, "pegPriceType").as_deref(), Some("TrailingStopPeg"));
    assert_eq!(p(r, "ordType").as_deref(), Some("Stop"));
    assert_eq!(p(r, "priceRp"), None);
}

#[test]
fn cancel_deduplicates_children_and_chunks_ten() {
    let mut e = engine();
    for i in 0..11 {
        e.store_mut().upsert_order(order(&format!("order-{i}")));
    }
    let mut child = order("order-0__stop_loss");
    child.parent_id = Some("order-0".into());
    child.kind = OrderType::StopLoss;
    e.store_mut().upsert_order(child);
    let mut ids: Vec<_> = (0..11).map(|i| format!("order-{i}")).collect();
    ids.push("order-0__stop_loss".into());
    let a = command(&mut e, Command::CancelOrders { command: 1, ids });
    let r = &writes(&a)[0];
    assert!(r.url.contains("/g-orders?"));
    assert_eq!(p(r, "orderID").unwrap().split(',').count(), 10);
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":(0..10).map(|i|json!({"orderID":format!("order-{i}"),"ordStatus":"Canceled","bizError":0})).collect::<Vec<_>>()}),
    );
    let r = &writes(&a)[0];
    assert!(r.url.contains("/g-orders?"));
    assert_eq!(p(r, "orderID").as_deref(), Some("order-10"));
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"order-10","ordStatus":"Canceled","bizError":0}}),
    );
    assert_eq!(done(&a).unwrap(), "[]");
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Canceled)
    );
}

#[test]
fn cancel_row_rejection_rolls_back_and_does_not_retry() {
    let mut e = engine();
    e.store_mut().upsert_order(order("old"));
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["old".into()],
        },
    );
    let r = &writes(&a)[0];
    assert!(r.url.contains("/g-orders/cancel?"));
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"old","bizError":11001}}),
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
}

#[test]
fn replacement_preserves_brackets_postonly_and_remaining() {
    let mut e = engine();
    let mut parent = order("old");
    parent.amount = 0.1;
    parent.filled = 0.04;
    parent.remaining = 0.06;
    parent.stop_loss = Some(68000.);
    e.store_mut().upsert_order(parent);
    let mut sl = order("old__stop_loss");
    sl.parent_id = Some("old".into());
    sl.kind = OrderType::StopLoss;
    sl.trigger_type = Some(StopTriggerType::LastPrice);
    sl.price = 68000.;
    e.store_mut().upsert_order(sl);
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(69500.),
            amount: None,
        },
    );
    let cancel = &writes(&a)[0];
    let a = respond(
        &mut e,
        cancel,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Canceled"}}),
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "orderQtyRq").as_deref(), Some("0.06"));
    assert_eq!(p(r, "priceRp").as_deref(), Some("69500"));
    assert_eq!(p(r, "stopLossRp").as_deref(), Some("68000"));
    assert_eq!(p(r, "slTrigger").as_deref(), Some("ByLastPrice"));
    assert_eq!(p(r, "timeInForce").as_deref(), Some("PostOnly"));
    assert_ne!(p(r, "clOrdID").as_deref(), Some("old"));
}

#[test]
fn imported_unknown_bracket_type_rejects_before_cancel() {
    let mut e = engine();
    let mut o = order("old");
    o.stop_loss = Some(68000.);
    e.store_mut().upsert_order(o);
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(69500.),
            amount: None,
        },
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
}

#[test]
fn fill_during_cancel_prevents_replacement() {
    let mut e = engine();
    e.store_mut().upsert_order(order("old"));
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(69500.),
            amount: None,
        },
    );
    let r = &writes(&a)[0];
    e.store_mut().update_order("old", |o| {
        o.filled = 0.05;
        o.remaining = 0.05;
    });
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Canceled"}}),
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
}

#[test]
fn cancel_ack_cumulative_fill_prevents_replacement_without_ws_echo() {
    let mut e = engine();
    e.store_mut().upsert_order(order("old"));
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(69500.),
            amount: None,
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Canceled","cumQtyRq":"0.04"}}),
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
    let old = e.store().order("old").unwrap();
    assert_eq!(old.status, OrderStatus::Canceled);
    assert_eq!(old.filled, 0.04);
}

#[test]
fn cancel_array_checks_every_requested_identity() {
    let mut e = engine();
    for id in ["one", "two"] {
        e.store_mut().upsert_order(order(id));
    }
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["one".into(), "two".into()],
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":[{"orderID":"one","code":0}]}),
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    assert_eq!(e.store().order("two").unwrap().status, OrderStatus::Open);
}

#[test]
fn documented_init_ack_returns_real_identity_without_open_order() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller")],
        },
    );
    let r = &writes(&a)[0];
    let mut value = ack(r, "actual-id");
    value["data"]["ordStatus"] = json!("Init");
    let a = respond(&mut e, r, value);
    assert_eq!(done(&a).unwrap(), "[\"actual-id\"]");
    assert!(e.store().orders().is_empty());
}

#[test]
fn zero_quantity_protective_omits_quantity_and_closes_on_trigger() {
    let mut e = engine();
    let mut o = opts("");
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.amount = 0.;
    o.price = Some(68000.);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "orderQtyRq"), None);
    assert_eq!(p(r, "closeOnTrigger").as_deref(), Some("true"));
}

#[test]
fn partial_cancel_array_preserves_success_and_rolls_back_rejection() {
    let mut e = engine();
    for id in ["one", "two"] {
        e.store_mut().upsert_order(order(id));
    }
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["one".into(), "two".into()],
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":[{"orderID":"one","code":0,"ordStatus":"Canceled"},{"orderID":"two","code":10001}]}),
    );
    assert!(done(&a).is_err());
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(e.store().order("two").unwrap().status, OrderStatus::Open);
}

#[test]
fn mode_ack_installs_confirmed_context_and_fences_dependent_placements() {
    let (mut e, socket) = engine_with_socket("OneWay");
    e.handle(1000., Event::WsOpened { socket });
    e.handle(
        1000.,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"result":{"status":"success"}}"#,
        },
    );
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 1,
            hedged: true,
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "targetPosMode").as_deref(), Some("Hedged"));
    let blocked = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![opts("blocked")],
        },
    );
    assert!(writes(&blocked).is_empty());
    assert!(done(&blocked).is_err());
    e.handle(1001.,Event::WsMessage{socket,text:r#"{"type":"incremental","positions_p":[{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","size":"0","leverageRr":"-10","crossMargin":true}]}"#});
    e.drain_actions().for_each(drop);
    let a = respond(&mut e, r, json!({"code":0,"data":null}));
    assert_eq!(done(&a).unwrap(), "[]");
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 3,
            orders: vec![opts("after")],
        },
    );
    assert_eq!(p(&writes(&a)[0], "posSide").as_deref(), Some("Long"));
}

#[test]
fn cancel_symbol_waits_for_initial_order_snapshot() {
    let mut e = engine();
    assert!(!e.store().loaded().orders);
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
}

#[test]
fn escaped_client_id_uses_ts_decoded_query_signature() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller &+/")],
        },
    );
    let r = &writes(&a)[0];
    assert!(r.url.contains("clOrdID=caller%20%26%2B%2F&"));
    let payload = "/g-orders/createclOrdID=caller &+/&symbol=BTCUSDT&posSide=Merged&side=Buy&ordType=Limit&orderQtyRq=0.01&priceRp=69000&timeInForce=GoodTillCancel61";
    let expected = safe_cex_crypto::hmac_sha256(
        b"secret",
        payload.as_bytes(),
        safe_cex_crypto::Encoding::Hex,
    );
    assert!(
        r.headers
            .iter()
            .any(|(key, value)| *key == "X-PHEMEX-REQUEST-SIGNATURE" && value == &expected)
    );
}

#[test]
fn malformed_create_quantity_or_symbol_never_upserts_foreign_order() {
    for field in ["symbol", "orderQtyRq"] {
        let mut e = engine();
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("caller")],
            },
        );
        let r = &writes(&a)[0];
        let mut value = ack(r, "foreign");
        value["data"][field] = json!(if field == "symbol" { "ETHUSDT" } else { "0.2" });
        let a = respond(&mut e, r, value);
        assert!(done(&a).is_err());
        assert!(e.store().orders().is_empty());
    }
}

#[test]
fn cancel_symbol_rejects_retained_loaded_snapshot_while_disconnected() {
    let (mut e, socket) = engine_with_socket("OneWay");
    e.handle(1000., Event::WsOpened { socket });
    e.handle(
        1000.,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"result":{"status":"success"}}"#,
        },
    );
    e.handle(
        1000.,
        Event::WsMessage {
            socket,
            text: r#"{"type":"snapshot","orders_p":[]}"#,
        },
    );
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDT".into(),
        },
    );
    assert_eq!(done(&a).unwrap(), "[]");
    assert!(e.store().loaded().orders);
    e.handle(1001., Event::WsClosed { socket, code: 1006 });
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 2,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    assert!(e.store().loaded().orders);
}

fn authenticated(mode: &str) -> (Engine<PhemexPerpetuals>, SocketId) {
    let (mut e, socket) = engine_with_socket(mode);
    e.handle(1000., Event::WsOpened { socket });
    e.handle(
        1000.,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"result":{"status":"success"}}"#,
        },
    );
    e.drain_actions().for_each(drop);
    (e, socket)
}
fn ws_orders(e: &mut Engine<PhemexPerpetuals>, socket: SocketId, rows: Vec<Value>) {
    let text =
        json!({"type":"incremental","timestamp":2_000_000_000_u64,"orders_p":rows}).to_string();
    e.handle(
        1002.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    e.drain_actions().for_each(drop);
}
fn live_row(id: &str) -> Value {
    json!({"orderID":id,"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","ordType":"Limit","ordStatus":"New","orderQty":"0.1","cumQty":"0","leavesQty":"0.1","priceRp":"69000"})
}

#[test]
fn standalone_stop_loss_amend_preserves_last_price_trigger() {
    let mut e = engine();
    let mut o = order("old");
    o.kind = OrderType::StopLoss;
    o.reduce_only = true;
    o.side = OrderSide::Sell;
    o.trigger_type = Some(StopTriggerType::LastPrice);
    o.price = 68000.;
    e.store_mut().upsert_order(o);
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(67900.),
            amount: None,
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Canceled","cumQtyRq":"0"}}),
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "ordType").as_deref(), Some("Stop"));
    assert_eq!(p(r, "triggerType").as_deref(), Some("ByLastPrice"));
}

#[test]
fn standalone_take_profit_amend_preserves_mark_price_trigger() {
    let mut e = engine();
    let mut o = order("old");
    o.kind = OrderType::TakeProfit;
    o.reduce_only = true;
    o.side = OrderSide::Sell;
    o.trigger_type = Some(StopTriggerType::MarkPrice);
    o.price = 71000.;
    e.store_mut().upsert_order(o);
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(71100.),
            amount: None,
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Canceled","cumQtyRq":"0"}}),
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "ordType").as_deref(), Some("MarketIfTouched"));
    assert_eq!(p(r, "triggerType").as_deref(), Some("ByMarkPrice"));
}

#[test]
fn accepted_init_cancel_never_replaces_or_tombstones_before_private_finality() {
    let (mut e, socket) = authenticated("OneWay");
    e.store_mut().upsert_order(order("old"));
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(67900.),
            amount: None,
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Init","execStatus":"Init","bizError":0,"cumQtyRq":"0"}}),
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
    assert_eq!(
        e.store().order("old").unwrap().status,
        OrderStatus::Canceling
    );
    ws_orders(&mut e, socket, vec![live_row("old")]);
    assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
}

#[test]
fn accepted_bulk_init_leaves_every_order_pending_without_tombstones() {
    let (mut e, socket) = authenticated("OneWay");
    for id in ["one", "two"] {
        e.store_mut().upsert_order(order(id));
    }
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["one".into(), "two".into()],
        },
    );
    let r = &writes(&a)[0];
    assert!(r.url.contains("/g-orders?"));
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"one","ordStatus":"Init","execStatus":"Init","bizError":0}}),
    );
    assert_eq!(done(&a).unwrap(), "[]");
    assert!(writes(&a).is_empty());
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Canceling)
    );
    ws_orders(&mut e, socket, vec![live_row("one"), live_row("two")]);
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Open)
    );
}

#[test]
fn partial_cancel_rejection_does_not_roll_back_an_accepted_pending_sibling() {
    let mut e = engine();
    for id in ["one", "two"] {
        e.store_mut().upsert_order(order(id));
    }
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["one".into(), "two".into()],
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":[{"orderID":"one","ordStatus":"Init","execStatus":"Init","bizError":0},{"orderID":"two","ordStatus":"Rejected","bizError":11001}]}),
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceling
    );
    assert_eq!(e.store().order("two").unwrap().status, OrderStatus::Open);
}

#[test]
fn missing_or_rejected_cancel_results_restore_previous_state() {
    for value in [
        json!({"code":0}),
        json!({"code":0,"data":null}),
        json!({"code":0,"data":{"orderID":"old"}}),
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Rejected"}}),
    ] {
        let mut e = engine();
        e.store_mut().upsert_order(order("old"));
        let a = command(
            &mut e,
            Command::AmendOrder {
                command: 1,
                id: "old".into(),
                price: Some(67900.),
                amount: None,
            },
        );
        let r = &writes(&a)[0];
        let a = respond(&mut e, r, value);
        assert!(done(&a).is_err());
        assert!(writes(&a).is_empty());
        assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn duplicate_cancel_identity_cannot_prove_terminal_outcome() {
    let mut e = engine();
    e.store_mut().upsert_order(order("old"));
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["old".into()],
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":[{"orderID":"old","ordStatus":"Canceled"},{"orderID":"old","ordStatus":"Init"}]}),
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
}

#[test]
fn single_bulk_final_row_does_not_finalize_other_requested_ids() {
    let mut e = engine();
    for id in ["one", "two"] {
        e.store_mut().upsert_order(order(id));
    }
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["one".into(), "two".into()],
        },
    );
    let r = &writes(&a)[0];
    let a = respond(
        &mut e,
        r,
        json!({"code":0,"data":{"orderID":"one","ordStatus":"Canceled"}}),
    );
    assert_eq!(done(&a).unwrap(), "[]");
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        e.store().order("two").unwrap().status,
        OrderStatus::Canceling
    );
}

#[test]
fn margin_only_hedged_update_preserves_distinct_side_leverages_after_ack() {
    let (mut e, socket) = authenticated("Hedged");
    let text = r#"{"type":"incremental","positions_p":[{"symbol":"BTCUSDT","currency":"USDT","posMode":"Hedged","posSide":"Short","size":"0","leverageRr":"-20","crossMargin":true}]}"#;
    e.handle(1000., Event::WsMessage { socket, text });
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::SetMarginType {
            command: 1,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "longLeverageRr").as_deref(), Some("10"));
    assert_eq!(p(r, "shortLeverageRr").as_deref(), Some("20"));
    let a = respond(&mut e, r, json!({"code":0,"data":null}));
    assert_eq!(done(&a).unwrap(), "[]");
    let a = command(
        &mut e,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: false,
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "longLeverageRr").as_deref(), Some("-10"));
    assert_eq!(p(r, "shortLeverageRr").as_deref(), Some("-20"));
}

#[test]
fn decimal_maximum_accepts_one_exact_caller_order_without_oversized_lots() {
    let mut e = engine();
    let mut markets = e.store().markets().to_vec();
    markets[0].precision.amount = 0.1;
    markets[0].limits.amount.max = 0.3;
    e.store_mut().set_markets(markets);
    let mut o = opts("decimal-caller");
    o.amount = 0.3;
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "orderQtyRq").as_deref(), Some("0.3"));
    assert_eq!(p(r, "clOrdID").as_deref(), Some("decimal-caller"));
    let a = respond(&mut e, r, ack(r, "actual"));
    assert!(done(&a).is_ok());
    assert!(writes(&a).is_empty());
    let mut markets = e.store().markets().to_vec();
    markets[0].limits.amount.max = 0.29;
    e.store_mut().set_markets(markets);
    let mut o = opts("");
    o.amount = 0.3;
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![o],
        },
    );
    assert_eq!(p(&writes(&a)[0], "orderQtyRq").as_deref(), Some("0.2"));
}

#[test]
fn imported_limit_trigger_amend_fails_before_destructive_cancel() {
    for kind in ["StopLimit", "LimitIfTouched"] {
        let (mut e, socket) = authenticated("OneWay");
        let mut row = live_row("imported");
        row["ordType"] = json!(kind);
        row["side"] = json!("Sell");
        row["reduceOnly"] = json!(true);
        row["triggerType"] = json!("ByLastPrice");
        row["stopDirection"] = json!("Falling");
        row["stopPxRp"] = json!("68000");
        row["priceRp"] = json!("67950");
        ws_orders(&mut e, socket, vec![row]);
        let a = command(
            &mut e,
            Command::AmendOrder {
                command: 1,
                id: "imported".into(),
                price: Some(68100.),
                amount: None,
            },
        );
        assert!(done(&a).is_err());
        assert!(writes(&a).is_empty());
        assert_eq!(
            e.store().order("imported").unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn flat_cross_leverage_margin_and_overlapping_settings() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.,
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "leverageRr").as_deref(), Some("-20"));
    let b = command(
        &mut e,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    );
    assert!(writes(&b).is_empty());
    assert!(done(&b).is_err());
    let a = respond(&mut e, r, json!({"code":0,"data":null}));
    assert_eq!(done(&a).unwrap(), "[]");
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.);
    let a = command(
        &mut e,
        Command::SetMarginType {
            command: 3,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
    );
    assert_eq!(p(&writes(&a)[0], "leverageRr").as_deref(), Some("20"));
}

#[test]
fn malformed_rejected_and_ambiguous_create_never_retry() {
    for value in [
        json!({"code":0,"data":{}}),
        json!({"code":100,"msg":"rejected"}),
        json!({"code":0,"data":{"orderID":"suspended","ordStatus":"Suspended","execStatus":"PendingAllocate"}}),
    ] {
        let mut e = engine();
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("caller")],
            },
        );
        let r = &writes(&a)[0];
        let a = respond(&mut e, r, value);
        assert!(done(&a).is_err());
        assert!(writes(&a).is_empty());
        assert!(e.store().orders().is_empty());
    }
    let mut e = engine();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller")],
        },
    );
    let r = &writes(&a)[0];
    e.handle(
        1001.,
        Event::HttpFailed {
            request: r.id,
            error: "network outcome unknown",
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    assert!(e.store().orders().is_empty());
}

#[test]
fn unknown_context_and_public_only_fail_closed() {
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }));
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller")],
        },
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        public_data_only: true,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }));
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec![],
        },
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
}
