//! Documented g-API payloads and mutation lifecycle regressions.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Response, SocketId};
use safe_cex_venue_phemex::PhemexPerpetuals;
use serde_json::{Value, json};
use support::{Options, PreparedEngine as Engine};

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
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
            _ => None,
        })
        .expect("command completed")
}

fn failure(actions: &[Action]) -> &safe_cex_engine::VenueError {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("failed command")
}

fn command(e: &mut Engine<PhemexPerpetuals>, c: Command) -> Vec<Action> {
    e.handle(1000., Event::Command(c));
    e.drain_actions().collect()
}

fn engine_with_socket(mode: &str) -> (Engine<PhemexPerpetuals>, SocketId) {
    let mut e = support::prepared_engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    e.start(0.);
    let a: Vec<_> = e.drain_actions().collect();
    let mut a = raw(&mut e, &requests(&a)[0], PRODUCTS);
    let ticker = requests(&a)
        .into_iter()
        .find(|r| r.url.contains("24hr/all"))
        .unwrap();
    a.extend(raw(&mut e, &ticker, DAILY));
    a.retain(|action| !matches!(action, Action::Http(r) if r.url.contains("24hr/all")));
    let socket = a
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
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

#[test]
fn caller_and_generated_order_identities_precede_mutation() {
    for generated in [false, true] {
        let mut e = engine();
        let mut order = opts("caller-identity");
        if generated {
            order.client_order_id = None;
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 94,
                orders: vec![order],
            },
        );
        let (progress_at, receipts) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                Action::Notice(Notice::CommandProgress {
                    command: 94,
                    receipts,
                }) => Some((index, receipts)),
                _ => None,
            })
            .expect("native placement identity");
        let (write_at, request) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                Action::Http(request) if request.url.contains("/g-orders/create") => {
                    Some((index, request))
                }
                _ => None,
            })
            .unwrap();
        assert!(progress_at < write_at);
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].symbol, "BTCUSDT");
        assert_eq!(receipts[0].kind, OrderIdentityKind::ClientOrderId);
        assert_eq!(receipts[0].identity, p(request, "clOrdID").unwrap());
        if !generated {
            assert_eq!(receipts[0].identity, "caller-identity");
        }
    }
}
fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
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
fn bybit_controls_reject_the_whole_batch_before_any_write() {
    for control in 0..2 {
        let mut e = engine();
        let mut invalid = opts("second");
        if control == 0 {
            invalid.bybit_bbo = Some(safe_cex_engine::model::BybitBbo {
                side_type: "Queue".into(),
                level: 1.,
            });
        } else {
            invalid.bybit_slippage_tolerance_ticks = Some(1.);
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), invalid],
            },
        );
        assert!(writes(&actions).is_empty());
        assert!(
            done(&actions)
                .unwrap_err()
                .contains("Bybit execution controls")
        );
    }
}

#[test]
fn explicit_stp_rejects_the_whole_batch_before_any_io() {
    use safe_cex_engine::model::StpMode;
    for mode in [
        StpMode::ExpireTaker,
        StpMode::ExpireMaker,
        StpMode::ExpireBoth,
    ] {
        let mut e = engine();
        let mut invalid = opts("second");
        invalid.stp_mode = Some(mode);
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts("first"), invalid],
            },
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert!(
            done(&actions)
                .unwrap_err()
                .contains("self-trade prevention is unsupported")
        );
        assert!(e.store().orders().is_empty());
    }
}

#[test]
fn unsupported_shared_commands_complete_with_error_without_io() {
    use safe_cex_engine::model::{CancelOrderRef, StpMode};
    for unsupported in [
        Command::CancelOrdersWithSymbols {
            command: 1,
            orders: vec![CancelOrderRef {
                id: "unreconciled".into(),
                symbol: "BTCUSDT".into(),
            }],
        },
        Command::FetchStpMode { command: 1 },
        Command::SetStpMode {
            command: 1,
            mode: StpMode::ExpireTaker,
        },
        Command::FetchOrderBook {
            command: 1,
            symbol: "BTCUSDT".into(),
            limit: Some(100),
        },
        Command::FetchLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
        },
    ] {
        let mut e = engine();
        let actions = command(&mut e, unsupported);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert!(done(&actions).unwrap_err().contains("unsupported"));
        assert!(e.store().orders().is_empty());
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
fn index_price_is_encoded_for_standalone_and_attached_protection() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut e = engine();
        let mut o = opts("");
        o.kind = kind;
        o.side = OrderSide::Sell;
        o.price = Some(if kind == OrderType::StopLoss {
            68000.
        } else {
            70000.
        });
        o.trigger_type = Some(StopTriggerType::IndexPrice);
        o.sl_trigger_type = Some(StopTriggerType::IndexPrice);
        o.tp_trigger_type = Some(StopTriggerType::IndexPrice);
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 95,
                orders: vec![o],
            },
        );
        let sent = writes(&a);
        assert_eq!(sent.len(), 1, "{a:?}");
        assert_eq!(p(&sent[0], "triggerType").as_deref(), Some("ByIndexPrice"));
        let mut reply = ack(&sent[0], "indexed-stop");
        reply["data"]["triggerType"] = json!("ByIndexPrice");
        reply["data"]["stopPxRp"] = json!(p(&sent[0], "stopPxRp").unwrap());
        let complete = respond(&mut e, &sent[0], reply);
        assert!(done(&complete).is_ok());
        assert_eq!(
            e.store().order("indexed-stop").unwrap().trigger_type,
            Some(StopTriggerType::IndexPrice)
        );
    }
    let mut e = engine();
    let mut o = opts("");
    o.stop_loss = Some(68000.);
    o.take_profit = Some(70000.);
    o.sl_trigger_type = Some(StopTriggerType::IndexPrice);
    o.tp_trigger_type = Some(StopTriggerType::IndexPrice);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 96,
            orders: vec![o],
        },
    );
    let sent = writes(&a);
    assert_eq!(sent.len(), 1, "{a:?}");
    assert_eq!(p(&sent[0], "slTrigger").as_deref(), Some("ByIndexPrice"));
    assert_eq!(p(&sent[0], "tpTrigger").as_deref(), Some("ByIndexPrice"));
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
            symbol: Some("BTCUSDT".into()),
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
            side: None,
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
    let mut e = support::prepared_engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller")],
        },
    );
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty());
    let mut e = support::prepared_engine(Options {
        public_data_only: true,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
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

#[test]
fn cancel_all_waits_for_creating_order_and_cancels_native_ack_even_when_init() {
    for status in ["New", "Init"] {
        let (mut e, socket) = authenticated("OneWay");
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
            Command::PlaceOrders {
                command: 41,
                orders: vec![opts("pending-caller")],
            },
        );
        let create = writes(&a)[0].clone();
        assert!(
            e.store()
                .orders()
                .iter()
                .any(|o| o.status == OrderStatus::Creating)
        );
        let a = command(
            &mut e,
            Command::CancelSymbolOrders {
                command: 42,
                symbol: "BTCUSDT".into(),
            },
        );
        assert!(writes(&a).is_empty());
        assert!(!a.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: 42, .. } | Notice::CommandFailed { command: 42, .. }
            )
        )));
        let mut reply = ack(&create, "native-accepted");
        reply["data"]["ordStatus"] = json!(status);
        let a = respond(&mut e, &create, reply);
        let cancel = writes(&a)
            .into_iter()
            .find(|r| r.method == Method::Delete)
            .expect("deferred cancel dispatched");
        assert_eq!(p(&cancel, "orderID").as_deref(), Some("native-accepted"));
        assert_eq!(p(&cancel, "posSide").as_deref(), Some("Merged"));
        let a = respond(
            &mut e,
            &cancel,
            json!({"code":0,"data":{"orderID":"native-accepted","ordStatus":"Canceled"}}),
        );
        assert!(a.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 42,
                result: _
            })
        )));
    }
}

fn rejection_engine(rows: Vec<Value>) -> Engine<PhemexPerpetuals> {
    let (mut e, socket) = authenticated("OneWay");
    let text = json!({"type":"snapshot","orders_p":rows}).to_string();
    e.handle(
        1000.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    e.drain_actions().for_each(drop);
    assert!(e.store().loaded().orders);
    e
}

fn cancel_all_after_create_result(value: impl FnOnce(&HttpRequest) -> Value) -> Vec<Action> {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 71,
            orders: vec![opts("rejected-caller")],
        },
    );
    let create = writes(&a)[0].clone();
    let a = respond(&mut e, &create, value(&create));
    assert!(done(&a).is_err());
    assert!(writes(&a).is_empty(), "never retry a rejected placement");
    assert!(e.store().order("old-live").is_some());
    command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 72,
            symbol: "BTCUSDT".into(),
        },
    )
}

#[test]
fn definitive_create_rejection_does_not_fence_cancel_all_of_existing_orders() {
    for biz_error in [0, 11001] {
        let a = cancel_all_after_create_result(|create| {
            let mut value = ack(create, "rejected-native");
            value["data"]["ordStatus"] = json!("Rejected");
            value["data"]["execStatus"] = json!("Rejected");
            value["data"]["bizError"] = json!(biz_error);
            value
        });
        let cancel = writes(&a)
            .into_iter()
            .find(|r| r.method == Method::Delete)
            .expect("a rejected placement must not block canceling the prior live order");
        assert_eq!(p(&cancel, "orderID").as_deref(), Some("old-live"));
    }
}

#[test]
fn documented_create_business_errors_do_not_fence_cancel_all() {
    // Official business-rejection definitions, never duplicate/routing/internal errors:
    // https://github.com/phemex/phemex-api-docs/blob/master/TradingErrorCode.md
    for code in [
        11001, 11027, 11034, 11035, 11036, 11053, 11055, 11056, 11058, 11081, 11082,
    ] {
        for null in [false, true] {
            let a = cancel_all_after_create_result(|_| {
                if null {
                    json!({"code":code,"msg":"explicit business rejection","data":null})
                } else {
                    json!({"code":code,"msg":"explicit business rejection"})
                }
            });
            assert!(
                writes(&a).iter().any(|r| r.method == Method::Delete),
                "code {code}: {a:?}"
            );
        }
    }
}

#[test]
fn unproven_create_results_still_cancel_every_known_order() {
    for case in [
        "missing-client",
        "wrong-client",
        "wrong-symbol",
        "wrong-side",
        "wrong-qty",
        "missing-qty",
        "wrong-position",
        "missing-position",
        "wrong-type",
        "suspended",
        "malformed",
        "duplicate",
        "routing",
        "unknown",
        "contradictory",
        "error-success",
        "error-business",
    ] {
        let a = cancel_all_after_create_result(|create| {
            let mut value = ack(create, "rejected-native");
            value["data"]["ordStatus"] = json!("Rejected");
            match case {
                "missing-client" => {
                    value["data"].as_object_mut().unwrap().remove("clOrdID");
                }
                "wrong-client" => value["data"]["clOrdID"] = json!("other-caller"),
                "wrong-symbol" => value["data"]["symbol"] = json!("ETHUSDT"),
                "wrong-side" => value["data"]["side"] = json!("Sell"),
                "wrong-qty" => value["data"]["orderQtyRq"] = json!("0.02"),
                "missing-qty" => {
                    value["data"].as_object_mut().unwrap().remove("orderQtyRq");
                }
                "wrong-position" => value["data"]["posSide"] = json!("Long"),
                "missing-position" => {
                    value["data"].as_object_mut().unwrap().remove("posSide");
                }
                "wrong-type" => value["data"]["orderType"] = json!("Market"),
                "suspended" => {
                    value["data"]["ordStatus"] = json!("Suspended");
                    value["data"]["execStatus"] = json!("PendingAllocate");
                }
                "malformed" => value = json!({"code":0,"data":{}}),
                "duplicate" => value = json!({"code":11085,"msg":"duplicate client ID"}),
                "routing" => value = json!({"code":11107,"msg":"cross engine connection failed"}),
                "unknown" => value = json!({"code":100,"msg":"unknown"}),
                "contradictory" => {
                    value = ack(create, "accepted-native");
                    value["code"] = json!(11001);
                }
                "error-success" => {
                    value = ack(create, "accepted-native");
                    value["error"] = json!({"code":500,"message":"unknown outcome"});
                }
                "error-business" => {
                    value = json!({"code":11001,"msg":"balance","data":null,"error":{"code":500,"message":"unknown outcome"}})
                }
                _ => unreachable!(),
            }
            value
        });
        let cancel = writes(&a)
            .into_iter()
            .find(|r| r.method == Method::Delete)
            .unwrap_or_else(|| panic!("case {case}: cancel-all must go out: {a:?}"));
        assert_eq!(
            p(&cancel, "orderID").as_deref(),
            Some("old-live"),
            "case {case}"
        );
    }
}

#[test]
fn partial_batch_rejection_preserves_prior_ack_routing() {
    let mut e = rejection_engine(vec![]);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 81,
            orders: vec![opts("accepted-caller"), opts("rejected-caller")],
        },
    );
    let first = writes(&a)[0].clone();
    let mut value = ack(&first, "first-accepted");
    value["data"]["ordStatus"] = json!("Init");
    let a = respond(&mut e, &first, value);
    let second = writes(&a)[0].clone();
    let mut value = ack(&second, "second-rejected");
    value["data"]["ordStatus"] = json!("Rejected");
    let a = respond(&mut e, &second, value);
    assert!(done(&a).is_err());
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 82,
            symbol: "BTCUSDT".into(),
        },
    );
    let cancel = writes(&a)
        .into_iter()
        .find(|r| r.method == Method::Delete)
        .expect("earlier accepted batch order remains cancelable");
    assert_eq!(p(&cancel, "orderID").as_deref(), Some("first-accepted"));
    assert_eq!(p(&cancel, "posSide").as_deref(), Some("Merged"));
}

#[test]
fn proven_rejection_cannot_clear_older_transport_uncertainty() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 91,
            orders: vec![opts("unknown-caller")],
        },
    );
    e.handle(
        1001.,
        Event::HttpFailed {
            request: writes(&a)[0].id,
            error: "transport outcome unknown",
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(done(&a).is_err());
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 92,
            orders: vec![opts("rejected-caller")],
        },
    );
    let create = writes(&a)[0].clone();
    let mut value = ack(&create, "rejected-native");
    value["data"]["ordStatus"] = json!("Rejected");
    assert!(done(&respond(&mut e, &create, value)).is_err());
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 93,
            symbol: "BTCUSDT".into(),
        },
    );
    let cancel = writes(&a)
        .into_iter()
        .find(|r| r.method == Method::Delete)
        .expect("cancel-all goes out despite the unknown placement");
    assert_eq!(p(&cancel, "orderID").as_deref(), Some("old-live"));
    let armed = warnings(&a);
    assert!(
        armed.iter().any(|m| m.contains("unknown-caller"))
            && !armed.iter().any(|m| m.contains("rejected-caller")),
        "only the unknown placement stays owed a late cancel: {armed:?}"
    );
}

#[test]
fn non_success_http_cannot_prove_create_rejection() {
    for status in [400, 408, 500] {
        let mut e = rejection_engine(vec![live_row("old-live")]);
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 101,
                orders: vec![opts("caller")],
            },
        );
        let body = json!({"code":11001,"msg":"insufficient balance","data":null}).to_string();
        e.handle(
            1001.,
            Event::HttpResponse {
                request: writes(&a)[0].id,
                response: Response {
                    status,
                    headers: "",
                    body: &body,
                },
            },
        );
        let a: Vec<_> = e.drain_actions().collect();
        assert!(done(&a).is_err());
        assert!(writes(&a).is_empty());
        let a = command(
            &mut e,
            Command::CancelSymbolOrders {
                command: 102,
                symbol: "BTCUSDT".into(),
            },
        );
        let cancel = writes(&a)
            .into_iter()
            .find(|r| r.method == Method::Delete)
            .unwrap_or_else(|| panic!("HTTP {status}: cancel-all must go out"));
        assert_eq!(p(&cancel, "orderID").as_deref(), Some("old-live"));
        assert!(
            warnings(&a).iter().any(|m| m.contains("caller")),
            "HTTP {status}: unknown placement stays owed a late cancel"
        );
    }
}

#[test]
fn non_success_write_keeps_typed_native_diagnostics_without_proving_rejection() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, body, kind, native_code, native_message) in [
        (
            400,
            r#"{"code":11001,"msg":"insufficient balance","data":null}"#,
            Some(ErrorKind::Rejected),
            Some("11001"),
            Some("insufficient balance"),
        ),
        (
            401,
            r#"{"msg":"invalid credentials"}"#,
            Some(ErrorKind::AuthInvalid),
            None,
            Some("invalid credentials"),
        ),
        (
            403,
            "<html>forbidden</html>",
            Some(ErrorKind::AuthPermission),
            None,
            None,
        ),
        (
            429,
            r#"{"msg":"slow down"}"#,
            Some(ErrorKind::RateLimited),
            None,
            Some("slow down"),
        ),
        (
            408,
            r#"{"code":11001,"msg":"insufficient balance"}"#,
            Some(ErrorKind::Timeout),
            Some("11001"),
            Some("insufficient balance"),
        ),
        (
            500,
            r#"{"code":11001,"msg":"insufficient balance"}"#,
            Some(ErrorKind::VenueUnavailable),
            Some("11001"),
            Some("insufficient balance"),
        ),
        (
            500,
            "<html>upstream unavailable</html>",
            Some(ErrorKind::VenueUnavailable),
            None,
            None,
        ),
        (
            400,
            r#"{"code":11001,"msg":"insufficient balance","data":{"orderID":"unproven-id"}}"#,
            Some(ErrorKind::Rejected),
            Some("11001"),
            Some("insufficient balance"),
        ),
    ] {
        let mut e = rejection_engine(vec![live_row("old-live")]);
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 101,
                orders: vec![opts("caller")],
            },
        );
        e.handle(
            1001.,
            Event::HttpResponse {
                request: writes(&a)[0].id,
                response: Response {
                    status,
                    headers: "retry-after: 2\r\n",
                    body,
                },
            },
        );
        let a: Vec<_> = e.drain_actions().collect();
        let error = failure(&a);
        assert_eq!(error.kind, kind, "HTTP {status}: {error:?}");
        assert_eq!(error.venue_code.as_deref(), native_code, "HTTP {status}");
        assert_eq!(
            error.venue_message.as_deref(),
            native_message,
            "HTTP {status}"
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown), "HTTP {status}");
        assert_eq!(error.retry_after_ms, (status == 429).then_some(2000.));
        assert!(writes(&a).is_empty());
        assert!(e.store().order("unproven-id").is_none());
        assert_eq!(
            a.iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 101, .. })
                ))
                .count(),
            1
        );
        let serialized = serde_json::to_value(error).unwrap();
        assert_eq!(serialized["outcome"], "unknown");
    }
}

#[test]
fn queued_cancel_all_continues_after_proven_rejection() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 111,
            orders: vec![opts("caller")],
        },
    );
    let create = writes(&a)[0].clone();
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 112,
            symbol: "BTCUSDT".into(),
        },
    );
    assert!(writes(&a).is_empty());
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: 112, .. } | Notice::CommandFailed { command: 112, .. }
        )
    )));
    let mut reply = ack(&create, "rejected-native");
    reply["data"]["ordStatus"] = json!("Rejected");
    let a = respond(&mut e, &create, reply);
    assert!(a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 111, .. })
    )));
    let cancel = writes(&a)
        .into_iter()
        .find(|r| r.method == Method::Delete)
        .expect("queued cancel proceeds after known rejection");
    assert_eq!(p(&cancel, "orderID").as_deref(), Some("old-live"));
    let a = respond(
        &mut e,
        &cancel,
        json!({"code":0,"data":{"orderID":"old-live","ordStatus":"Canceled"}}),
    );
    assert!(a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 112,
            result: _
        })
    )));
}

#[test]
fn per_side_leverage_preserves_opposite_signed_rail_and_publishes_distinct_settings() {
    for isolated in [false, true] {
        for side in [Some(PositionSide::Long), Some(PositionSide::Short), None] {
            let (mut e, socket) = authenticated("Hedged");
            let sign = if isolated { 1. } else { -1. };
            let text = json!({"type":"incremental","positions_p":[
                {"symbol":"BTCUSDT","currency":"USDT","posMode":"Hedged","posSide":"Long","size":"0","leverageRr":10.*sign,"crossMargin":!isolated},
                {"symbol":"BTCUSDT","currency":"USDT","posMode":"Hedged","posSide":"Short","size":"0","leverageRr":20.*sign,"crossMargin":!isolated}
            ]}).to_string();
            e.handle(
                1000.,
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
            e.drain_actions().for_each(drop);
            let a = command(
                &mut e,
                Command::SetLeverage {
                    command: 501,
                    symbol: "BTCUSDT".into(),
                    leverage: 37.,
                    side,
                },
            );
            let r = &writes(&a)[0];
            let long = if side == Some(PositionSide::Short) {
                10.
            } else {
                37.
            };
            let short = if side == Some(PositionSide::Long) {
                20.
            } else {
                37.
            };
            assert_eq!(p(r, "longLeverageRr"), Some((long * sign).to_string()));
            assert_eq!(p(r, "shortLeverageRr"), Some((short * sign).to_string()));
            let a = respond(&mut e, r, json!({"code":0,"data":null}));
            assert!(done(&a).is_ok());
            let setting = e.store().setting("BTCUSDT").unwrap();
            assert_eq!(setting.buy_leverage, Some(long));
            assert_eq!(setting.sell_leverage, Some(short));
            assert_eq!(
                setting.margin_type,
                Some(if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                })
            );
            // The cached opposite rail must survive into a subsequent margin request.
            let a = command(
                &mut e,
                Command::SetMarginType {
                    command: 502,
                    symbol: "BTCUSDT".into(),
                    isolated: !isolated,
                },
            );
            let r = &writes(&a)[0];
            assert_eq!(p(r, "longLeverageRr"), Some((-long * sign).to_string()));
            assert_eq!(p(r, "shortLeverageRr"), Some((-short * sign).to_string()));
        }
    }
}

#[test]
fn rejected_phemex_side_leverage_and_oneway_side_do_not_corrupt_context() {
    let (mut e, _) = authenticated("Hedged");
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 503,
            symbol: "BTCUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Short),
        },
    );
    let a = respond(&mut e, &writes(&a)[0], json!({"code":100,"msg":"rejected"}));
    assert!(done(&a).is_err());
    assert_eq!(e.store().setting("BTCUSDT").unwrap().sell_leverage, None);
    let mut e = engine_mode("OneWay");
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 504,
            symbol: "BTCUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Short),
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "leverageRr"), Some("-37".into()));
    assert_eq!(p(r, "shortLeverageRr"), None);
    respond(&mut e, r, json!({"code":0,"data":null}));
    let s = e.store().setting("BTCUSDT").unwrap();
    assert_eq!((s.buy_leverage, s.sell_leverage), (Some(37.), Some(37.)));
}

#[test]
fn symbol_mode_switch_sends_one_put_and_leaves_other_market_mode_unchanged() {
    let (mut e, socket) = authenticated("Hedged");
    let mut markets = e.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.id = "ETHUSDT".into();
    eth.symbol = "ETHUSDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    e.store_mut().set_markets(markets);
    e.handle(1000.,Event::WsMessage{socket,text:r#"{"type":"incremental","positions_p":[{"symbol":"ETHUSDT","currency":"USDT","posMode":"Hedged","posSide":"Long","size":"0","leverageRr":"-10","crossMargin":true},{"symbol":"ETHUSDT","currency":"USDT","posMode":"Hedged","posSide":"Short","size":"0","leverageRr":"-20","crossMargin":true}]}"#});
    e.drain_actions().for_each(drop);
    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 701,
            hedged: false,
            symbol: Some("BTCUSDT".into()),
        },
    );
    assert_eq!(writes(&a).len(), 1);
    let r = &writes(&a)[0];
    assert!(r.url.contains("/g-positions/switch-pos-mode-sync"));
    assert_eq!(p(r, "symbol"), Some("BTCUSDT".into()));
    assert_eq!(p(r, "targetPosMode"), Some("OneWay".into()));
    let a = respond(&mut e, r, json!({"code":0,"data":null}));
    assert!(done(&a).is_ok());
    assert!(writes(&a).is_empty());
    assert!(e.store().hedged(), "ETH remains in hedge mode");
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 702,
            symbol: "ETHUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Short),
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "longLeverageRr"), Some("-10".into()));
    assert_eq!(p(r, "shortLeverageRr"), Some("-37".into()));
    assert_eq!(p(r, "leverageRr"), None);
}

#[test]
fn phemex_missing_unknown_and_rejected_symbol_modes_never_switch_other_markets() {
    for symbol in [None, Some(String::new()), Some("UNKNOWN".into())] {
        let mut e = engine();
        let a = command(
            &mut e,
            Command::SetPositionMode {
                command: 703,
                hedged: true,
                symbol,
            },
        );
        assert!(writes(&a).is_empty());
        assert!(done(&a).is_err());
        assert!(!e.store().hedged());
    }
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 704,
            hedged: true,
            symbol: Some("BTCUSDT".into()),
        },
    );
    let a = respond(
        &mut e,
        &writes(&a)[0],
        json!({"code":39201,"msg":"positions not closed"}),
    );
    assert!(done(&a).is_err());
    assert!(!e.store().hedged());
    assert!(writes(&a).is_empty());
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 705,
            symbol: "BTCUSDT".into(),
            leverage: 37.,
            side: None,
        },
    );
    assert_eq!(p(&writes(&a)[0], "leverageRr"), Some("-37".into()));
}

#[test]
fn uncached_market_mode_ack_needs_no_invented_margin_and_fences_stale_account_reads() {
    let (mut e, socket) = authenticated("OneWay");
    let mut markets = e.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.id = "ETHUSDT".into();
    eth.symbol = "ETHUSDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    e.store_mut().set_markets(markets);
    assert!(e.store().setting("ETHUSDT").is_none());

    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 710,
            hedged: true,
            symbol: Some("ETHUSDT".into()),
        },
    );
    assert_eq!(writes(&a).len(), 1);
    let r = &writes(&a)[0];
    assert_eq!(p(r, "symbol"), Some("ETHUSDT".into()));
    assert_eq!(p(r, "targetPosMode"), Some("Hedged".into()));
    let a = respond(&mut e, r, json!({"code":0,"data":null}));
    assert!(done(&a).is_ok());
    assert!(e.store().hedged());
    assert!(
        e.store().setting("ETHUSDT").is_none(),
        "a mode ACK cannot invent leverage or margin"
    );
    let stale = requests(&a);
    assert_eq!(stale.len(), 2);
    // A delayed untimed OneWay row must not erase the acknowledged Hedged mode.
    e.handle(1001., Event::WsMessage { socket, text: r#"{"type":"incremental","positions_p":[{"symbol":"ETHUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","size":"0","leverageRr":"8"},{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","size":"0","leverageRr":"-10","crossMargin":true}]}"# });
    e.drain_actions().for_each(drop);
    assert!(e.store().hedged());
    assert!(e.store().setting("ETHUSDT").is_none());

    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 711,
            symbol: "ETHUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Long),
        },
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
    let mut order = opts("no-unknown-margin");
    order.symbol = "ETHUSDT".into();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 712,
            orders: vec![order],
        },
    );
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());

    let a = command(
        &mut e,
        Command::SetPositionMode {
            command: 713,
            hedged: false,
            symbol: Some("ETHUSDT".into()),
        },
    );
    let a = respond(&mut e, &writes(&a)[0], json!({"code":0,"data":null}));
    assert!(done(&a).is_ok());
    assert!(!e.store().hedged());
    let mut completed = vec![];
    for r in stale {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let rows = if currency == "USDT" {
            json!([
                {"symbol":"ETHUSDT","currency":"USDT","posMode":"Hedged","posSide":"Long","size":"0","leverageRr":"10"},
                {"symbol":"ETHUSDT","currency":"USDT","posMode":"Hedged","posSide":"Short","size":"0","leverageRr":"20"}
            ])
        } else {
            json!([])
        };
        completed.extend(respond(&mut e, &r, json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":rows}})));
    }
    assert!(
        !e.store().hedged(),
        "reads started before the last ACK cannot replace its mode"
    );
    assert!(e.store().setting("ETHUSDT").is_none());

    let (timer, at) = completed
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= 3000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("the next authoritative account refresh is scheduled");
    e.handle(at, Event::TimerFired { timer });
    let fresh: Vec<_> = e.drain_actions().collect();
    assert_eq!(requests(&fresh).len(), 2);
    for r in requests(&fresh) {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let rows = if currency == "USDT" {
            json!([
                {"symbol":"ETHUSDT","currency":"USDT","posMode":"Hedged","posSide":"Long","size":"0","leverageRr":"10"},
                {"symbol":"ETHUSDT","currency":"USDT","posMode":"Hedged","posSide":"Short","size":"0","leverageRr":"20"}
            ])
        } else {
            json!([])
        };
        respond(
            &mut e,
            &r,
            json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":rows}}),
        );
    }
    assert!(
        e.store().hedged(),
        "a fresh authoritative row supersedes the mode-only ACK"
    );
    let s = e.store().setting("ETHUSDT").unwrap();
    assert_eq!((s.buy_leverage, s.sell_leverage), (Some(10.), Some(20.)));
    assert_eq!(s.margin_type, Some(MarginType::Isolated));
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 714,
            symbol: "ETHUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Short),
        },
    );
    let r = &writes(&a)[0];
    assert_eq!(p(r, "longLeverageRr"), Some("10".into()));
    assert_eq!(p(r, "shortLeverageRr"), Some("37".into()));
}

#[test]
fn app_protective_payload_with_matching_trigger_price_places_like_price_only() {
    let app = |kind: &str, price: f64, trigger: Option<f64>| {
        let mut o: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDT","side":"sell","type":kind,"notional":690,"timeInForce":"GoodTillCancel","reduceOnly":true,"price":price,"amount":0.01})).unwrap();
        o.trigger_price = trigger;
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        }
    };
    for (kind, price) in [("stop_market", 68000.), ("take_profit_market", 71000.)] {
        let mut sent = vec![];
        for trigger in [Some(price), None] {
            let mut e = engine();
            let a = command(&mut e, app(kind, price, trigger));
            let w = writes(&a);
            assert_eq!(w.len(), 1, "{kind}: {:?}", done(&a));
            let mut params = params(&w[0]);
            params.retain(|(k, _)| k != "clOrdID");
            params.sort();
            sent.push((
                w[0].method,
                w[0].url.split_once('?').unwrap().0.to_owned(),
                params,
            ));
        }
        assert!(
            sent[0].2.contains(&("stopPxRp".into(), price.to_string())),
            "{kind}"
        );
        assert_eq!(sent[0], sent[1], "{kind}");
    }
    let mut e = engine();
    let a = command(&mut e, app("stop_market", 68000., Some(67900.)));
    assert!(writes(&a).is_empty());
    assert!(done(&a).is_err());
}

fn at(e: &mut Engine<PhemexPerpetuals>, now: f64, event: Event<'_>) -> Vec<Action> {
    e.handle(now, event);
    e.drain_actions().collect()
}

fn reply(
    e: &mut Engine<PhemexPerpetuals>,
    now: f64,
    r: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    at(
        e,
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    )
}

fn place_at(
    e: &mut Engine<PhemexPerpetuals>,
    now: f64,
    command: safe_cex_engine::CommandId,
    id: &str,
) -> HttpRequest {
    let a = at(
        e,
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![opts(id)],
        }),
    );
    writes(&a).remove(0)
}

fn open_order_reads(actions: &[Action]) -> Vec<HttpRequest> {
    requests(actions)
        .into_iter()
        .filter(|r| r.method == Method::Get && r.url.contains("/g-orders/activeList?"))
        .collect()
}

fn latest_timer(actions: &[Action]) -> Option<(safe_cex_engine::TimerId, f64)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

fn error_logs(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    level: safe_cex_engine::Level::Error,
                    ..
                }))
            )
        })
        .count()
}

fn warnings(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Warn,
                message,
                ..
            })) => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn deletes(actions: &[Action]) -> Vec<HttpRequest> {
    writes(actions)
        .into_iter()
        .filter(|r| r.method == Method::Delete)
        .collect()
}

fn rest_row(id: &str, client: &str) -> Value {
    json!({"orderID":id,"clOrdID":client,"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","orderType":"Limit","ordStatus":"New","orderQtyRq":"0.01","cumQtyRq":"0","leavesQtyRq":"0.01","priceRp":"69000","transactTimeNs":3_000_000_000_u64})
}

fn rows(rows: Vec<Value>) -> String {
    json!({"code":0,"data":{"rows":rows}}).to_string()
}

fn cancel_all_ids(
    e: &mut Engine<PhemexPerpetuals>,
    now: f64,
    command: safe_cex_engine::CommandId,
) -> Vec<String> {
    let a = at(
        e,
        now,
        Event::Command(Command::CancelSymbolOrders {
            command,
            symbol: "BTCUSDT".into(),
        }),
    );
    let mut ids: Vec<_> = writes(&a)
        .iter()
        .filter(|r| r.method == Method::Delete)
        .flat_map(|r| {
            p(r, "orderID")
                .unwrap()
                .split(',')
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        !ids.is_empty() || done(&a).is_ok(),
        "cancel-all is never refused: {a:?}"
    );
    ids.sort();
    ids
}

/// A create ACK that proves nothing: the fence rises, but the venue answered.
const UNPROVEN_ACK: &str = r#"{"code":0,"data":{}}"#;

/// Places `id` at `now`, answers it with an unproven ACK and fires the owed read
/// (signed expiry plus clock skew); returns the read and when it was sent.
fn unproven_then_read(
    e: &mut Engine<PhemexPerpetuals>,
    now: f64,
    command: safe_cex_engine::CommandId,
    id: &str,
) -> (HttpRequest, f64) {
    let create = place_at(e, now, command, id);
    let a = reply(e, now + 1., &create, 200, UNPROVEN_ACK);
    assert!(done(&a).is_err());
    assert!(
        open_order_reads(&a).is_empty(),
        "an unproven answer may still be followed by a late landing"
    );
    let (timer, wake) = latest_timer(&a).expect("read scheduled");
    assert_eq!(wake, ((now + 60_000.) / 1000.).round() * 1000. + 5_000.);
    let read = open_order_reads(&at(e, wake, Event::TimerFired { timer })).remove(0);
    (read, wake)
}

#[test]
fn transport_uncertain_placement_reads_open_orders_after_its_signed_expiry() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let create = place_at(&mut e, 1000., 121, "unknown-caller");
    let a = at(
        &mut e,
        1500.,
        Event::HttpFailed {
            request: create.id,
            error: "transport outcome unknown",
        },
    );
    assert!(done(&a).is_err());
    assert!(
        open_order_reads(&a).is_empty(),
        "a late write could still land after an immediate read"
    );
    let (timer, wake) = latest_timer(&a).expect("read scheduled");
    // Signed expiry is send time + 60 s (rounded to seconds), plus 5 s clock skew.
    assert_eq!(wake, 66_000.);
    let a = at(&mut e, wake, Event::TimerFired { timer });
    let read = open_order_reads(&a);
    assert_eq!(read.len(), 1);
    assert_eq!(p(&read[0], "symbol").as_deref(), Some("BTCUSDT"));
    reply(
        &mut e,
        wake + 1.,
        &read[0],
        200,
        &rows(vec![
            live_row("old-live"),
            rest_row("revealed", "unknown-caller"),
        ]),
    );
    assert!(e.store().order("revealed").is_some());
    assert_eq!(
        cancel_all_ids(&mut e, wake + 2., 123),
        ["old-live", "revealed"]
    );
}

#[test]
fn open_order_read_started_before_a_later_failure_cannot_settle_it() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let (stale, now) = unproven_then_read(&mut e, 1000., 131, "first-unknown");
    let second = place_at(&mut e, now + 1., 132, "second-unknown");
    let a = reply(&mut e, now + 2., &second, 200, UNPROVEN_ACK);
    assert!(
        open_order_reads(&a).is_empty(),
        "one read per symbol at a time"
    );
    let a = reply(
        &mut e,
        now + 3.,
        &stale,
        200,
        &rows(vec![
            live_row("old-live"),
            rest_row("first", "first-unknown"),
        ]),
    );
    assert!(
        open_order_reads(&a).is_empty(),
        "second write is not expired yet"
    );
    let (timer, wake) = latest_timer(&a).expect("a newer failure owes a newer read");
    assert_eq!(cancel_all_ids(&mut e, now + 4., 133), ["first", "old-live"]);
    let fresh = open_order_reads(&at(&mut e, wake, Event::TimerFired { timer }));
    assert_eq!(fresh.len(), 1);
    let a = reply(
        &mut e,
        wake + 1.,
        &fresh[0],
        200,
        &rows(vec![
            live_row("old-live"),
            rest_row("first", "first-unknown"),
            rest_row("second", "second-unknown"),
        ]),
    );
    let late = deletes(&a);
    assert_eq!(late.len(), 1, "only the order the stale read missed: {a:?}");
    assert_eq!(p(&late[0], "orderID").as_deref(), Some("second"));
}

#[test]
fn definite_rejection_owes_no_open_order_read() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let create = place_at(&mut e, 1000., 151, "rejected-caller");
    let a = reply(
        &mut e,
        1001.,
        &create,
        200,
        r#"{"code":11001,"msg":"insufficient available balance","data":null}"#,
    );
    assert!(done(&a).is_err());
    assert_eq!(
        failure(&a).outcome,
        Some(safe_cex_engine::Outcome::Rejected)
    );
    assert!(open_order_reads(&a).is_empty());
    assert_eq!(cancel_all_ids(&mut e, 1002., 152), ["old-live"]);
}

#[test]
fn unproven_and_lost_writes_report_an_unknown_outcome() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let create = place_at(&mut e, 1000., 141, "unknown-caller");
    let a = reply(&mut e, 1001., &create, 200, UNPROVEN_ACK);
    assert_eq!(failure(&a).outcome, Some(safe_cex_engine::Outcome::Unknown));

    let mut e = rejection_engine(vec![live_row("old-live")]);
    let create = place_at(&mut e, 1000., 142, "lost-caller");
    let a = reply(&mut e, 1001., &create, 502, "Bad Gateway");
    assert_eq!(failure(&a).outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn failed_read_retries_and_empty_code_reconciles() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let (read, now) = unproven_then_read(&mut e, 1000., 141, "unknown-caller");
    let a = reply(&mut e, now + 1., &read, 500, "{}");
    let (timer, wake) = latest_timer(&a).expect("failed read schedules a retry");
    let delay = wake - (now + 1.);
    assert!(
        (0.75 * 2000. ..=2000.).contains(&delay),
        "retry {delay} outside jittered [0.75, 1.0] x 2000"
    );
    let a = at(&mut e, wake, Event::TimerFired { timer });
    let read = open_order_reads(&a).remove(0);
    reply(
        &mut e,
        wake + 1.,
        &read,
        200,
        r#"{"code":10002,"msg":"OM_ORDER_NOT_FOUND","data":null}"#,
    );
    assert_eq!(cancel_all_ids(&mut e, wake + 2., 143), ["old-live"]);
}

#[test]
fn persistently_unmappable_row_stops_retrying_with_one_error() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let (mut read, now) = unproven_then_read(&mut e, 1000., 161, "unknown-caller");
    let mut bad = rest_row("unmappable", "unknown-caller");
    bad["side"] = json!("Sideways");
    let mut now = now + 1.;
    let mut errors = 0;
    for attempt in 1..=3 {
        let a = reply(&mut e, now, &read, 200, &rows(vec![bad.clone()]));
        errors += error_logs(&a);
        if attempt == 3 {
            assert!(latest_timer(&a).is_none(), "no retry after the cap");
            assert!(open_order_reads(&a).is_empty());
            break;
        }
        let (timer, wake) = latest_timer(&a).expect("incomplete read retries");
        now = wake;
        read = open_order_reads(&at(&mut e, now, Event::TimerFired { timer })).remove(0);
    }
    assert_eq!(errors, 1, "one Error once retrying is pointless");
    assert_eq!(
        cancel_all_ids(&mut e, now + 1., 162),
        ["old-live"],
        "an unreadable row cannot hold back canceling the known orders"
    );
}

#[test]
fn transport_timeout_cancel_all_goes_out_now_and_cancels_the_late_landing_after_expiry() {
    let mut e = rejection_engine(vec![live_row("old-live")]);
    let create = place_at(&mut e, 1000., 171, "lost-caller");
    let a = at(
        &mut e,
        1500.,
        Event::HttpFailed {
            request: create.id,
            error: "transport timeout",
        },
    );
    assert!(
        latest_timer(&a).is_some(),
        "read scheduled after signed expiry"
    );
    let a = at(
        &mut e,
        2000.,
        Event::Command(Command::CancelSymbolOrders {
            command: 172,
            symbol: "BTCUSDT".into(),
        }),
    );
    let cancel = deletes(&a);
    assert_eq!(cancel.len(), 1, "cancel-all is sent immediately: {a:?}");
    assert_eq!(p(&cancel[0], "orderID").as_deref(), Some("old-live"));
    assert!(warnings(&a).iter().any(|m| m.contains("lost-caller")));
    let a = reply(
        &mut e,
        2001.,
        &cancel[0],
        200,
        r#"{"code":0,"data":{"orderID":"old-live","ordStatus":"Canceled"}}"#,
    );
    assert!(a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 172,
            result: _
        })
    )));
    let (timer, wake) = latest_timer(&a).expect("read still owed");
    assert_eq!(wake, 66_000.);
    let a = at(&mut e, wake, Event::TimerFired { timer });
    let read = open_order_reads(&a).remove(0);
    let a = reply(
        &mut e,
        wake + 1.,
        &read,
        200,
        &rows(vec![
            rest_row("landed", "lost-caller"),
            rest_row("other", "fresh-caller"),
        ]),
    );
    let late = deletes(&a);
    assert_eq!(late.len(), 1, "{a:?}");
    assert_eq!(p(&late[0], "orderID").as_deref(), Some("landed"));
    assert!(warnings(&a).iter().any(|m| m.contains("landed")));
    assert_eq!(
        e.store().order("other").map(|o| o.status),
        Some(OrderStatus::Open),
        "only the uncertain placement's clOrdID is canceled"
    );
    let a = reply(
        &mut e,
        wake + 2.,
        &late[0],
        200,
        r#"{"code":0,"data":{"orderID":"landed","ordStatus":"Canceled"}}"#,
    );
    assert!(
        warnings(&a)
            .iter()
            .any(|m| m.contains("canceled late-landing order landed"))
    );
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. }))),
        "a venue-originated cancel completes no caller command"
    );
    assert_eq!(
        e.store().order("landed").map(|o| o.status),
        Some(OrderStatus::Canceled)
    );
}

#[test]
fn failed_late_cancel_is_reported_as_an_error() {
    let mut e = rejection_engine(vec![]);
    let create = place_at(&mut e, 1000., 181, "lost-caller");
    let a = at(
        &mut e,
        1500.,
        Event::HttpFailed {
            request: create.id,
            error: "transport timeout",
        },
    );
    assert!(done(&a).is_err());
    let a = at(
        &mut e,
        2000.,
        Event::Command(Command::CancelSymbolOrders {
            command: 182,
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(done(&a), Ok("[]".into()), "nothing known to cancel yet");
    let (timer, wake) = latest_timer(&a).unwrap();
    let a = at(&mut e, wake, Event::TimerFired { timer });
    let read = open_order_reads(&a).remove(0);
    let a = reply(
        &mut e,
        wake + 1.,
        &read,
        200,
        &rows(vec![rest_row("landed", "lost-caller")]),
    );
    let late = deletes(&a).remove(0);
    let a = at(
        &mut e,
        wake + 2.,
        Event::HttpFailed {
            request: late.id,
            error: "transport timeout",
        },
    );
    assert_eq!(error_logs(&a), 1, "{a:?}");
}

#[test]
fn placement_lost_after_cancel_all_is_not_canceled_by_it() {
    let mut e = rejection_engine(vec![]);
    assert_eq!(cancel_all_ids(&mut e, 900., 191), Vec::<String>::new());
    let create = place_at(&mut e, 1000., 192, "after-cancel");
    let a = at(
        &mut e,
        1500.,
        Event::HttpFailed {
            request: create.id,
            error: "transport timeout",
        },
    );
    let (timer, wake) = latest_timer(&a).unwrap();
    let a = at(&mut e, wake, Event::TimerFired { timer });
    let read = open_order_reads(&a).remove(0);
    let a = reply(
        &mut e,
        wake + 1.,
        &read,
        200,
        &rows(vec![rest_row("kept", "after-cancel")]),
    );
    assert!(deletes(&a).is_empty(), "{a:?}");
    assert!(e.store().order("kept").is_some());
}

fn armed_engine(mode: &str) -> (Engine<PhemexPerpetuals>, SocketId) {
    let (mut e, socket) = authenticated(mode);
    at(
        &mut e,
        1000.,
        Event::WsMessage {
            socket,
            text: r#"{"type":"snapshot","orders_p":[]}"#,
        },
    );
    (e, socket)
}

/// Loses `id` to a transport timeout at `now`, then sends cancel-all; returns
/// the owed read's timer.
fn lose_then_cancel_all(
    e: &mut Engine<PhemexPerpetuals>,
    now: f64,
    command: safe_cex_engine::CommandId,
    order: PlaceOrderOpts,
) -> (safe_cex_engine::TimerId, f64) {
    let a = at(
        e,
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![order],
        }),
    );
    let create = writes(&a).remove(0);
    at(
        e,
        now + 1.,
        Event::HttpFailed {
            request: create.id,
            error: "transport timeout",
        },
    );
    let a = at(
        e,
        now + 2.,
        Event::Command(Command::CancelSymbolOrders {
            command: command + 1,
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(done(&a), Ok("[]".into()));
    latest_timer(&a).expect("read owed after signed expiry")
}

#[test]
fn lost_client_order_id_cannot_be_reused_until_its_read_settles_it() {
    let (mut e, _) = armed_engine("OneWay");
    let (timer, wake) = lose_then_cancel_all(&mut e, 1000., 201, opts("lost-caller"));
    let a = at(
        &mut e,
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 203,
            orders: vec![opts("lost-caller")],
        }),
    );
    assert!(
        writes(&a).is_empty(),
        "a retry would be canceled as the late landing"
    );
    assert!(done(&a).unwrap_err().contains("unknown outcome"), "{a:?}");
    let read = open_order_reads(&at(&mut e, wake, Event::TimerFired { timer })).remove(0);
    let a = reply(&mut e, wake + 1., &read, 200, &rows(vec![]));
    assert!(deletes(&a).is_empty());
    let a = at(
        &mut e,
        wake + 2.,
        Event::Command(Command::PlaceOrders {
            command: 204,
            orders: vec![opts("lost-caller")],
        }),
    );
    assert_eq!(
        writes(&a).len(),
        1,
        "the read proved the lost write never landed"
    );
}

#[test]
fn late_landing_seen_on_the_private_stream_is_canceled_immediately() {
    let (mut e, socket) = armed_engine("OneWay");
    lose_then_cancel_all(&mut e, 1000., 211, opts("lost-caller"));
    let mut row = live_row("landed");
    row["clOrdID"] = json!("lost-caller");
    let text =
        json!({"type":"incremental","timestamp":2_000_000_000_u64,"orders_p":[row]}).to_string();
    let a = at(
        &mut e,
        2000.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    let late = deletes(&a);
    assert_eq!(late.len(), 1, "no wait for the post-expiry read: {a:?}");
    assert_eq!(p(&late[0], "orderID").as_deref(), Some("landed"));
    assert!(warnings(&a).iter().any(|m| m.contains("landed")));
    let a = at(
        &mut e,
        2001.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    assert!(deletes(&a).is_empty(), "one cancel per landing");
}

#[test]
fn late_cancels_in_two_groups_are_sequenced_without_false_errors() {
    let (mut e, _) = armed_engine("Hedged");
    lose_then_cancel_all(&mut e, 1000., 221, opts("long-lost"));
    let mut short = opts("short-lost");
    short.side = OrderSide::Sell;
    let (timer, wake) = lose_then_cancel_all(&mut e, 1010., 223, short);
    let read = open_order_reads(&at(&mut e, wake, Event::TimerFired { timer })).remove(0);
    let mut long = rest_row("long", "long-lost");
    long["posSide"] = json!("Long");
    let mut short = rest_row("short", "short-lost");
    short["posSide"] = json!("Short");
    short["side"] = json!("Sell");
    let a = reply(&mut e, wake + 1., &read, 200, &rows(vec![long, short]));
    let first = deletes(&a);
    assert_eq!(first.len(), 1, "one late cancel in flight at a time: {a:?}");
    let id = p(&first[0], "orderID").unwrap();
    let body = json!({"code":0,"data":{"orderID":id,"ordStatus":"Canceled"}}).to_string();
    let a = reply(&mut e, wake + 2., &first[0], 200, &body);
    assert_eq!(error_logs(&a), 0, "{a:?}");
    let second = deletes(&a);
    assert_eq!(second.len(), 1);
    let other = p(&second[0], "orderID").unwrap();
    assert_ne!(other, id);
    let body = json!({"code":0,"data":{"orderID":other,"ordStatus":"Canceled"}}).to_string();
    let a = reply(&mut e, wake + 3., &second[0], 200, &body);
    assert_eq!(error_logs(&a), 0, "{a:?}");
    for id in ["long", "short"] {
        assert_eq!(
            e.store().order(id).map(|o| o.status),
            Some(OrderStatus::Canceled)
        );
    }
}
