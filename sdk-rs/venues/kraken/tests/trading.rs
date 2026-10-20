//! Native REST payload and response contracts from Kraken's Futures order docs.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice};
use safe_cex_venue_kraken::Kraken;
use serde_json::{Value, json};
use support::*;
fn opts(v: Value) -> PlaceOrderOpts {
    serde_json::from_value(v).unwrap()
}
fn limit() -> PlaceOrderOpts {
    opts(json!({"symbol":"PF_XBTUSD","type":"limit","side":"buy","amount":0.1234,"price":100.26}))
}
fn command(e: &mut Engine<Kraken>, c: Command) {
    e.handle(NOW, Event::Command(c));
}
fn place(e: &mut Engine<Kraken>, orders: Vec<PlaceOrderOpts>) {
    command(e, Command::PlaceOrders { command: 7, orders });
}
fn http(e: &mut Engine<Kraken>, path: &str) -> HttpRequest {
    let all = e.drain_actions().collect::<Vec<_>>();
    let debug = format!("{all:?}");
    all.into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with(path) => Some(r),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {path}: {debug}"))
}
fn decode(text: &str) -> String {
    let mut out = vec![];
    let mut b = text.bytes();
    while let Some(v) = b.next() {
        if v == b'%' {
            let p = [b.next().unwrap(), b.next().unwrap()];
            out.push(u8::from_str_radix(std::str::from_utf8(&p).unwrap(), 16).unwrap());
        } else {
            out.push(if v == b'+' { b' ' } else { v });
        }
    }
    String::from_utf8(out).unwrap()
}
fn param(r: &HttpRequest, key: &str) -> Option<String> {
    r.body.as_ref().unwrap().split('&').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        (k == key).then(|| decode(v))
    })
}
fn ack(e: &mut Engine<Kraken>, r: &HttpRequest, id: &str, status: &str) {
    reply(
        e,
        r,
        200,
        &json!({"result":"success","sendStatus":{"order_id":id,"status":status}}).to_string(),
    );
}
fn done(e: &mut Engine<Kraken>) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result),
            _ => None,
        })
        .expect("command completion")
}
fn inject(e: &mut Engine<Kraken>, id: &str, kind: OrderType, trigger: Option<StopTriggerType>) {
    e.store_mut().upsert_order(Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "PF_XBTUSD".into(),
        kind,
        side: OrderSide::Sell,
        price: 90.0,
        amount: 0.123,
        filled: 0.0,
        remaining: 0.123,
        reduce_only: true,
        close: None,
        client_order_id: Some("old-client".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: NOW,
        trigger_type: trigger,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
}
fn cancel_ack(e: &mut Engine<Kraken>, r: &HttpRequest, id: &str) {
    reply(
        e,
        r,
        200,
        &json!({"result":"success","cancelStatus":{"status":"cancelled","order_id":id}})
            .to_string(),
    );
}
#[test]
fn batch_prevalidates_every_caller_id_before_writes() {
    for kind in [
        OrderType::Limit,
        OrderType::Market,
        OrderType::StopLoss,
        OrderType::TakeProfit,
        OrderType::TrailingStopLoss,
    ] {
        let (mut e, _) = ready();
        let mut bad = limit();
        bad.kind = kind;
        bad.client_order_id = Some("caller".into());
        place(&mut e, vec![limit(), bad]);
        let all = e.drain_actions().collect::<Vec<_>>();
        assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(all.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}
#[test]
fn native_limit_precision_symbol_broker_fresh_ids_and_tif() {
    let (mut e, _) = ready();
    let mut first = String::new();
    for (i, tif) in [
        None,
        Some(TimeInForce::PostOnly),
        Some(TimeInForce::ImmediateOrCancel),
        Some(TimeInForce::FillOrKill),
    ]
    .into_iter()
    .enumerate()
    {
        let mut o = limit();
        o.time_in_force = tif;
        place(&mut e, vec![o]);
        let r = http(&mut e, "sendorder");
        assert_eq!(r.method, Method::Post);
        assert_eq!(param(&r, "symbol").unwrap(), "PF_XBTUSD");
        assert_eq!(param(&r, "size").unwrap(), "0.123");
        assert_eq!(param(&r, "limitPrice").unwrap(), "100.5");
        assert_eq!(param(&r, "broker").unwrap(), "AA73 N84G 7YOF J26Y");
        assert_eq!(
            param(&r, "orderType").unwrap(),
            ["lmt", "post", "ioc", "fok"][i]
        );
        let client = param(&r, "cliOrdId").unwrap();
        assert!(client.starts_with("ts_"));
        assert_ne!(client, first);
        first = client;
        ack(&mut e, &r, &format!("order-{i}"), "placed");
        assert_eq!(done(&mut e).unwrap(), format!("[\"order-{i}\"]"));
    }
}
#[test]
fn missing_or_rejected_semantic_ack_never_sends_bracket() {
    for body in [
        r#"{"result":"success"}"#,
        r#"{"result":"success","sendStatus":{"order_id":"x"}}"#,
        r#"{"result":"success","sendStatus":{"status":"placed"}}"#,
        r#"{"result":"success","sendStatus":{"status":"invalidSize","order_id":"x"}}"#,
    ] {
        let (mut e, _) = ready();
        let mut o = limit();
        o.stop_loss = Some(90.0);
        place(&mut e, vec![o]);
        let r = http(&mut e, "sendorder");
        reply(&mut e, &r, 200, body);
        let all = e.drain_actions().collect::<Vec<_>>();
        assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(all.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}
#[test]
fn bracket_legs_follow_accepted_entry_and_each_get_fresh_id() {
    let (mut e, _) = ready();
    let mut o = limit();
    o.stop_loss = Some(90.0);
    o.take_profit = Some(110.0);
    o.sl_trigger_type = Some(StopTriggerType::MarkPrice);
    o.tp_trigger_type = Some(StopTriggerType::LastPrice);
    place(&mut e, vec![o]);
    let entry = http(&mut e, "sendorder");
    let cid = param(&entry, "cliOrdId").unwrap();
    ack(&mut e, &entry, "entry", "placed");
    let sl = http(&mut e, "sendorder");
    assert_eq!(param(&sl, "orderType").unwrap(), "stp");
    assert_eq!(param(&sl, "side").unwrap(), "sell");
    assert_eq!(param(&sl, "reduceOnly").unwrap(), "true");
    assert_eq!(param(&sl, "triggerSignal").unwrap(), "mark");
    assert_ne!(param(&sl, "cliOrdId").unwrap(), cid);
    ack(&mut e, &sl, "stop", "placed");
    let tp = http(&mut e, "sendorder");
    assert_eq!(param(&tp, "orderType").unwrap(), "take_profit");
    assert_ne!(param(&tp, "cliOrdId"), param(&sl, "cliOrdId"));
    ack(&mut e, &tp, "profit", "placed");
    assert_eq!(done(&mut e).unwrap(), r#"["entry","stop","profit"]"#);
}
#[test]
fn invalid_late_bracket_leg_rejects_before_entry() {
    let (mut e, _) = ready();
    let mut o = limit();
    o.stop_loss = Some(90.0);
    o.take_profit = Some(f64::NAN);
    place(&mut e, vec![o]);
    assert!(done(&mut e).is_err());
    assert!(requests(&mut e).is_empty());
}
#[test]
fn trailing_uses_last_distance_and_no_stop_or_limit_price() {
    let (mut e, _) = ready();
    let mut o = limit();
    o.kind = OrderType::TrailingStopLoss;
    o.price = Some(90.1);
    o.sl_trigger_type = Some(StopTriggerType::MarkPrice);
    place(&mut e, vec![o]);
    let r = http(&mut e, "sendorder");
    assert_eq!(param(&r, "orderType").unwrap(), "trailing_stop");
    assert_eq!(param(&r, "trailingStopMaxDeviation").unwrap(), "10");
    assert_eq!(
        param(&r, "trailingStopDeviationUnit").unwrap(),
        "QUOTE_CURRENCY"
    );
    assert!(param(&r, "stopPrice").is_none());
    assert!(param(&r, "limitPrice").is_none());
    assert_eq!(param(&r, "reduceOnly").unwrap(), "true");
}
#[test]
fn trailing_rejects_missing_last_or_zero_distance() {
    for missing in [false, true] {
        let (mut e, _) = ready();
        e.store_mut().update_ticker("PF_XBTUSD", |t| {
            if missing {
                t.last = 0.0
            }
        });
        let mut o = limit();
        o.kind = OrderType::TrailingStopLoss;
        o.price = Some(100.0);
        place(&mut e, vec![o]);
        assert!(done(&mut e).is_err());
        assert!(requests(&mut e).is_empty());
    }
}
#[test]
fn protective_amend_prevalidates_and_waits_for_cancellation_ack() {
    let (mut e, _) = ready();
    inject(
        &mut e,
        "old",
        OrderType::StopLoss,
        Some(StopTriggerType::MarkPrice),
    );
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(91.1),
            amount: None,
        },
    );
    let cancel = http(&mut e, "cancelorder");
    assert_eq!(param(&cancel, "order_id").unwrap(), "old");
    assert_eq!(e.store().order("old").unwrap().price, 90.0);
    cancel_ack(&mut e, &cancel, "old");
    let replacement = http(&mut e, "sendorder");
    assert_eq!(param(&replacement, "stopPrice").unwrap(), "91");
    assert_eq!(param(&replacement, "triggerSignal").unwrap(), "mark");
    assert_ne!(param(&replacement, "cliOrdId").unwrap(), "old-client");
    ack(&mut e, &replacement, "new", "placed");
    assert_eq!(done(&mut e).unwrap(), r#"["new"]"#);
}
#[test]
fn protective_amend_without_provenance_trailing_or_invalid_price_never_cancels() {
    for (kind, trigger, price) in [
        (OrderType::StopLoss, None, 91.0),
        (
            OrderType::TrailingStopLoss,
            Some(StopTriggerType::MarkPrice),
            91.0,
        ),
        (
            OrderType::TakeProfit,
            Some(StopTriggerType::LastPrice),
            -1.0,
        ),
    ] {
        let (mut e, _) = ready();
        inject(&mut e, "old", kind, trigger);
        command(
            &mut e,
            Command::AmendOrder {
                command: 8,
                id: "old".into(),
                price: Some(price),
                amount: None,
            },
        );
        assert!(done(&mut e).is_err());
        assert!(requests(&mut e).is_empty());
        assert!(e.store().order("old").is_some());
    }
}
#[test]
fn rejected_cancel_keeps_old_protection_and_never_replaces() {
    let (mut e, _) = ready();
    inject(
        &mut e,
        "old",
        OrderType::StopLoss,
        Some(StopTriggerType::MarkPrice),
    );
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(91.0),
            amount: None,
        },
    );
    let r = http(&mut e, "cancelorder");
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","cancelStatus":{"status":"notFound","order_id":"old"}}"#,
    );
    assert!(done(&mut e).is_err());
    assert!(e.store().order("old").is_some());
    assert!(requests(&mut e).is_empty());
}
#[test]
fn ambiguous_write_is_not_retried_and_next_command_is_independent() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let r = http(&mut e, "sendorder");
    command(
        &mut e,
        Command::PlaceOrders {
            command: 9,
            orders: vec![limit()],
        },
    );
    assert!(requests(&mut e).is_empty());
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let all = e.drain_actions().collect::<Vec<_>>();
    assert!(all.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 7,
            result: Err(_)
        })
    )));
    assert_eq!(
        all.iter().filter(|a| matches!(a, Action::Http(_))).count(),
        1
    );
}
#[test]
fn batch_cancellation_json_strict_count_status_and_identity() {
    for body in [
        json!({"result":"success","batchStatus":[]}),
        json!({"result":"success","batchStatus":[{"status":"cancelled","order_id":"a"},{"status":"cancelled","order_id":"a"}]}),
        json!({"result":"success","batchStatus":[{"status":"cancelled","order_id":"a"},{"status":"notFound","order_id":"b"}]}),
    ] {
        let (mut e, _) = ready();
        inject(&mut e, "a", OrderType::Limit, None);
        inject(&mut e, "b", OrderType::Limit, None);
        command(
            &mut e,
            Command::CancelOrders {
                command: 8,
                ids: vec!["a".into(), "b".into()],
            },
        );
        let r = http(&mut e, "batchorder");
        assert_eq!(
            serde_json::from_str::<Value>(&param(&r, "json").unwrap()).unwrap(),
            json!({"batchOrder":[{"order":"cancel","order_id":"a"},{"order":"cancel","order_id":"b"}]})
        );
        reply(&mut e, &r, 200, &body.to_string());
        assert!(done(&mut e).is_err());
        assert!(e.store().order("b").is_some());
    }
}
#[test]
fn cancellation_success_removes_only_exact_owned_ids() {
    let (mut e, _) = ready();
    inject(&mut e, "a", OrderType::Limit, None);
    inject(&mut e, "b", OrderType::Limit, None);
    command(
        &mut e,
        Command::CancelOrders {
            command: 8,
            ids: vec!["a".into()],
        },
    );
    let r = http(&mut e, "cancelorder");
    cancel_ack(&mut e, &r, "other");
    assert!(done(&mut e).is_err());
    assert!(e.store().order("a").is_some());
    assert!(e.store().order("b").is_some());
}
#[test]
fn ordinary_edit_requires_semantic_status_and_matching_id() {
    for (id, status, good) in [
        ("old", "edited", true),
        ("old", "filled", true),
        ("other", "edited", false),
        ("old", "invalidSize", false),
        ("old", "", false),
    ] {
        let (mut e, _) = ready();
        inject(&mut e, "old", OrderType::Limit, None);
        command(
            &mut e,
            Command::AmendOrder {
                command: 8,
                id: "old".into(),
                price: Some(101.0),
                amount: Some(0.2),
            },
        );
        let r = http(&mut e, "editorder");
        assert_eq!(param(&r, "orderId").unwrap(), "old");
        assert_eq!(param(&r, "size").unwrap(), "0.2");
        reply(
            &mut e,
            &r,
            200,
            &json!({"result":"success","editStatus":{"orderId":id,"status":status}}).to_string(),
        );
        assert_eq!(done(&mut e).is_ok(), good);
    }
}
#[test]
fn setting_cache_changes_only_after_acceptance_and_cross_omits_leverage() {
    let (mut e, _) = ready();
    command(
        &mut e,
        Command::SetLeverage {
            command: 8,
            symbol: "PF_XBTUSD".into(),
            leverage: 7.0,
        },
    );
    let r = http(&mut e, "leveragepreferences");
    assert_eq!(r.method, Method::Put);
    assert_eq!(param(&r, "maxLeverage").unwrap(), "7");
    assert!(
        e.store()
            .setting("PF_XBTUSD")
            .is_none_or(|s| s.leverage != 7.0)
    );
    reply(&mut e, &r, 200, r#"{"result":"success"}"#);
    assert!(done(&mut e).is_ok());
    assert_eq!(e.store().setting("PF_XBTUSD").unwrap().leverage, 7.0);
    assert_eq!(
        e.store().setting("PF_XBTUSD").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    command(
        &mut e,
        Command::SetMarginType {
            command: 9,
            symbol: "PF_XBTUSD".into(),
            isolated: false,
        },
    );
    let r = http(&mut e, "leveragepreferences");
    assert!(param(&r, "maxLeverage").is_none());
    reply(&mut e, &r, 200, r#"{"result":"success"}"#);
    assert!(done(&mut e).is_ok());
    assert_eq!(
        e.store().setting("PF_XBTUSD").unwrap().margin_type,
        Some(MarginType::Cross)
    );
}
#[test]
fn setting_refusal_or_transport_failure_preserves_cache() {
    for body in [
        r#"{"result":"error","error":"denied"}"#,
        r#"{"result":"success","status":"BAD_REQUEST","errors":[{"message":"out of range"}]}"#,
    ] {
        let (mut e, _) = ready();
        command(
            &mut e,
            Command::SetLeverage {
                command: 8,
                symbol: "PF_XBTUSD".into(),
                leverage: 7.0,
            },
        );
        let r = http(&mut e, "leveragepreferences");
        reply(&mut e, &r, 200, body);
        assert!(done(&mut e).is_err());
        assert!(
            e.store()
                .setting("PF_XBTUSD")
                .is_none_or(|s| s.leverage != 7.0)
        );
    }
}
#[test]
fn leverage_lookup_returns_confirmed_two_sided_preferences() {
    let (mut e, _) = ready();
    command(
        &mut e,
        Command::SetLeverage {
            command: 8,
            symbol: "PF_XBTUSD".into(),
            leverage: 7.0,
        },
    );
    let r = http(&mut e, "leveragepreferences");
    reply(&mut e, &r, 200, r#"{"result":"success"}"#);
    done(&mut e).unwrap();
    command(
        &mut e,
        Command::FetchLeverage {
            command: 9,
            symbol: "PF_XBTUSD".into(),
        },
    );
    let v: Value = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(v["buyLeverage"], 7.0);
    assert_eq!(v["sellLeverage"], 7.0);
    assert_eq!(v["marginType"], "isolated");
}
#[test]
fn one_way_position_mode_is_noop_hedge_rejected() {
    let (mut e, _) = ready();
    for hedged in [false, true] {
        command(&mut e, Command::SetPositionMode { command: 7, hedged });
        assert_eq!(done(&mut e).is_ok(), !hedged);
        assert!(requests(&mut e).is_empty());
    }
}
#[test]
fn index_trigger_uses_documented_futures_enum() {
    let (mut e, _) = ready();
    let mut o = limit();
    o.kind = OrderType::StopLoss;
    o.sl_trigger_type = Some(StopTriggerType::IndexPrice);
    place(&mut e, vec![o]);
    let r = http(&mut e, "sendorder");
    assert_eq!(param(&r, "triggerSignal").unwrap(), "index");
}
#[test]
fn execution_or_full_fill_racing_cancel_suppresses_protective_replacement() {
    for fill in [
        r#"{"feed":"fills","fills":[{"fill_id":"exec","order_id":"old","instrument":"PF_XBTUSD","buy":false,"qty":0.01,"price":90}]}"#,
        r#"{"feed":"open_orders","order_id":"old","is_cancel":true,"reason":"full_fill"}"#,
        r#"{"feed":"open_orders","order_id":"old","is_cancel":true,"reason":"stop_order_triggered"}"#,
    ] {
        let (mut e, socket) = ready();
        inject(
            &mut e,
            "old",
            OrderType::StopLoss,
            Some(StopTriggerType::MarkPrice),
        );
        command(
            &mut e,
            Command::AmendOrder {
                command: 8,
                id: "old".into(),
                price: Some(91.0),
                amount: None,
            },
        );
        let r = http(&mut e, "cancelorder");
        message(&mut e, socket, NOW, fill);
        cancel_ack(&mut e, &r, "old");
        let all = e.drain_actions().collect::<Vec<_>>();
        assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(all.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}
#[test]
fn late_placement_ack_cannot_resurrect_a_ws_terminal_order() {
    let (mut e, socket) = ready();
    place(&mut e, vec![limit()]);
    let r = http(&mut e, "sendorder");
    message(
        &mut e,
        socket,
        NOW,
        r#"{"feed":"open_orders","order_id":"new","is_cancel":true,"reason":"full_fill"}"#,
    );
    ack(&mut e, &r, "new", "placed");
    assert!(done(&mut e).is_ok());
    assert!(e.store().order("new").is_none());
}
#[test]
fn queued_protective_amend_rejects_changed_order_before_cancelling() {
    let (mut e, _) = ready();
    inject(
        &mut e,
        "old",
        OrderType::StopLoss,
        Some(StopTriggerType::MarkPrice),
    );
    place(&mut e, vec![limit()]);
    let r = http(&mut e, "sendorder");
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(91.0),
            amount: None,
        },
    );
    e.store_mut().update_order("old", |o| o.filled = 0.01);
    ack(&mut e, &r, "entry", "placed");
    let all = e.drain_actions().collect::<Vec<_>>();
    assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(all.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 8,
            result: Err(_)
        })
    )));
}
#[test]
fn fetch_leverage_missing_preferences_is_a_failure() {
    let mut e = Engine::new(Kraken::new(options()));
    e.start(NOW);
    let r = http(&mut e, "instruments");
    reply(&mut e, &r, 200, META);
    let r = http(&mut e, "tickers");
    reply(&mut e, &r, 200, TICKERS);
    e.drain_actions().for_each(drop);
    command(
        &mut e,
        Command::FetchLeverage {
            command: 7,
            symbol: "PF_XBTUSD".into(),
        },
    );
    let r = http(&mut e, "leveragepreferences");
    reply(&mut e, &r, 200, r#"{"result":"success"}"#);
    assert!(done(&mut e).is_err());
}
#[test]
fn filled_ack_closes_order_without_inventing_fills() {
    let (mut e, socket) = ready();
    inject(&mut e, "old", OrderType::Limit, None);
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(101.0),
            amount: None,
        },
    );
    let r = http(&mut e, "editorder");
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","editStatus":{"status":"filled","orderId":"old"}}"#,
    );
    assert!(done(&mut e).is_ok());
    assert!(e.store().order("old").is_none());
    message(
        &mut e,
        socket,
        NOW,
        r#"{"feed":"open_orders","order":{"order_id":"old","symbol":"PF_XBTUSD","type":"lmt","direction":1,"qty":0.123,"filled":0,"limit_price":90}}"#,
    );
    assert!(e.store().order("old").is_none());
}
#[test]
fn protective_replacement_error_reports_canceled_original() {
    let (mut e, _) = ready();
    inject(
        &mut e,
        "old",
        OrderType::StopLoss,
        Some(StopTriggerType::MarkPrice),
    );
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(91.0),
            amount: None,
        },
    );
    let r = http(&mut e, "cancelorder");
    cancel_ack(&mut e, &r, "old");
    let r = http(&mut e, "sendorder");
    ack(&mut e, &r, "bad", "invalidSize");
    assert!(done(&mut e).unwrap_err().contains("canceled the original"));
    assert!(e.store().order("old").is_none());
}
#[test]
fn protective_amend_replaces_only_remaining_quantity() {
    let (mut e, _) = ready();
    inject(
        &mut e,
        "old",
        OrderType::StopLoss,
        Some(StopTriggerType::MarkPrice),
    );
    e.store_mut().update_order("old", |o| {
        o.filled = 0.023;
        o.remaining = 0.1;
    });
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(91.0),
            amount: None,
        },
    );
    let r = http(&mut e, "cancelorder");
    cancel_ack(&mut e, &r, "old");
    let r = http(&mut e, "sendorder");
    assert_eq!(param(&r, "size").unwrap(), "0.1");
}
#[test]
fn queued_margin_change_uses_latest_accepted_leverage() {
    let (mut e, _) = ready();
    command(
        &mut e,
        Command::SetLeverage {
            command: 7,
            symbol: "PF_XBTUSD".into(),
            leverage: 5.0,
        },
    );
    let first = http(&mut e, "leveragepreferences");
    command(
        &mut e,
        Command::SetMarginType {
            command: 8,
            symbol: "PF_XBTUSD".into(),
            isolated: true,
        },
    );
    reply(&mut e, &first, 200, r#"{"result":"success"}"#);
    let second = http(&mut e, "leveragepreferences");
    assert_eq!(param(&second, "maxLeverage").unwrap(), "5");
}
#[test]
fn placed_ack_preserves_preceding_ws_order_attributes() {
    let (mut e, socket) = ready();
    place(&mut e, vec![limit()]);
    let r = http(&mut e, "sendorder");
    message(
        &mut e,
        socket,
        NOW,
        r#"{"feed":"open_orders","order":{"order_id":"new","symbol":"PF_XBTUSD","type":"lmt","direction":0,"qty":0.2,"filled":0.05,"limit_price":102}}"#,
    );
    ack(&mut e, &r, "new", "placed");
    assert!(done(&mut e).is_ok());
    let order = e.store().order("new").unwrap();
    assert_eq!(
        (order.price, order.amount, order.filled),
        (102.0, 0.2, 0.05)
    );
}
#[test]
fn edit_ack_preserves_newer_ws_order_attributes_and_merges_fills() {
    for changed in [false, true] {
        let (mut e, socket) = ready();
        inject(&mut e, "old", OrderType::Limit, None);
        command(
            &mut e,
            Command::AmendOrder {
                command: 8,
                id: "old".into(),
                price: Some(101.0),
                amount: Some(0.2),
            },
        );
        let r = http(&mut e, "editorder");
        let price = if changed { 102.0 } else { 90.0 };
        let amount = if changed { 0.3 } else { 0.123 };
        message(&mut e,socket,NOW,&json!({"feed":"open_orders","order":{"order_id":"old","symbol":"PF_XBTUSD","type":"lmt","direction":1,"qty":amount,"filled":0.05,"limit_price":price}}).to_string());
        reply(
            &mut e,
            &r,
            200,
            r#"{"result":"success","editStatus":{"status":"edited","orderId":"old"}}"#,
        );
        assert!(done(&mut e).is_ok());
        let o = e.store().order("old").unwrap();
        assert_eq!(o.filled, 0.05);
        assert_eq!(
            (o.price, o.amount),
            if changed { (102.0, 0.3) } else { (101.0, 0.2) }
        );
    }
}
#[test]
fn contradictory_envelope_is_rejected_despite_semantic_ack() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let r = http(&mut e, "sendorder");
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","error":"denied","sendStatus":{"status":"placed","order_id":"bad"}}"#,
    );
    assert!(done(&mut e).is_err());
    assert!(e.store().order("bad").is_none());
}
#[test]
fn initial_position_snapshot_cannot_roll_back_accepted_settings() {
    let (mut e, socket) = ready();
    command(
        &mut e,
        Command::SetLeverage {
            command: 7,
            symbol: "PF_XBTUSD".into(),
            leverage: 7.0,
        },
    );
    let r = http(&mut e, "leveragepreferences");
    reply(&mut e, &r, 200, r#"{"result":"success"}"#);
    done(&mut e).unwrap();
    message(
        &mut e,
        socket,
        NOW,
        r#"{"feed":"open_positions_snapshot","positions":[{"instrument":"PF_XBTUSD","balance":2,"entry_price":100,"mark_price":120,"pnl":40,"maxFixedLeverage":10}]}"#,
    );
    let p = &e.store().positions()[0];
    assert_eq!((p.contracts, p.leverage), (2.0, 7.0));
    assert_eq!(p.margin_type, Some(MarginType::Isolated));
    assert_eq!(e.store().setting("PF_XBTUSD").unwrap().leverage, 7.0);
}

#[test]
fn batch_rejects_bybit_controls_before_any_write() {
    for field in ["bybitBbo", "bybitSlippageToleranceTicks"] {
        let (mut e, _) = ready();
        let mut bad = serde_json::to_value(limit()).unwrap();
        bad[field] = if field == "bybitBbo" {
            json!({"sideType":"Counterparty","level":1})
        } else {
            json!(5)
        };
        place(&mut e, vec![limit(), opts(bad)]);
        let all = e.drain_actions().collect::<Vec<_>>();
        assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(all.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandDone { result: Err(error), .. })
                if error.contains("Bybit execution controls")
        )));
    }
}
