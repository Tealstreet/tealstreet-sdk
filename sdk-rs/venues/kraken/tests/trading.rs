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
    let all = safe_cex_engine::test_support::prepared_actions(e, NOW);
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
    e.drain_plain()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
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
fn each_futures_bracket_step_receipts_its_actual_cli_ord_id_before_write() {
    let (mut e, _) = ready();
    let mut order = limit();
    order.stop_loss = Some(90.);
    order.take_profit = Some(110.);
    place(&mut e, vec![order]);
    let mut identities = std::collections::HashSet::new();
    for index in 0..3 {
        let actions = e.drain_plain().collect::<Vec<_>>();
        let (at, receipts) = actions
            .iter()
            .enumerate()
            .find_map(|(i, action)| match action {
                Action::Notice(Notice::CommandProgress {
                    command: 7,
                    receipts,
                }) => Some((i, receipts)),
                _ => None,
            })
            .unwrap();
        let (write_at, request) = actions
            .iter()
            .enumerate()
            .find_map(|(i, action)| match action {
                Action::Http(request) if request.url.ends_with("sendorder") => Some((i, request)),
                _ => None,
            })
            .unwrap();
        assert!(at < write_at);
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].symbol, "PF_XBTUSD");
        assert_eq!(receipts[0].kind, OrderIdentityKind::ClientOrderId);
        assert_eq!(receipts[0].identity, param(request, "cliOrdId").unwrap());
        assert!(identities.insert(receipts[0].identity.clone()));
        ack(&mut e, request, &format!("accepted-{index}"), "placed");
    }
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
        let all = e.drain_plain().collect::<Vec<_>>();
        assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            all.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
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
        let all = e.drain_plain().collect::<Vec<_>>();
        assert!(
            !all.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        );
        assert!(
            all.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
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
    let all = e.drain_plain().collect::<Vec<_>>();
    assert!(
        all.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
    );
    assert_eq!(
        all.iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("sendorder")))
            .count(),
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
            side: None,
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
                side: None,
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
            side: None,
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
        command(
            &mut e,
            Command::SetPositionMode {
                command: 7,
                hedged,
                symbol: None,
            },
        );
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
        let all = e.drain_plain().collect::<Vec<_>>();
        assert!(
            !all.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        );
        assert!(
            all.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
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
    let all = e.drain_plain().collect::<Vec<_>>();
    assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        all.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })))
    );
}
#[test]
fn fetch_leverage_missing_preferences_is_a_failure() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Kraken::new(options()));
    e.start(NOW);
    let r = http(&mut e, "instruments");
    reply(&mut e, &r, 200, META);
    let r = http(&mut e, "tickers");
    reply(&mut e, &r, 200, TICKERS);
    e.drain_plain().for_each(drop);
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
            side: None,
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
            side: None,
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
        let all = e.drain_plain().collect::<Vec<_>>();
        assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(all.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { error, .. })
                if error.legacy_text().contains("Bybit execution controls")
        )));
    }
}

#[test]
fn batch_rejects_foreign_stp_controls_before_writes() {
    let (mut e, _) = ready();
    let mut bad = limit();
    bad.stp_mode = Some(StpMode::ExpireTaker);
    place(&mut e, vec![limit(), bad]);
    let all = e.drain_plain().collect::<Vec<_>>();
    assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(all.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { error, .. })
            if error.legacy_text().contains("self-trade prevention")
    )));
}

#[test]
fn invalid_dynamic_leverage_limits_reject_without_native_write() {
    for (min, max) in [
        (30.0, 20.0),
        (f64::NAN, 20.0),
        (1.0, 0.0),
        (1.0, f64::INFINITY),
    ] {
        let (mut e, _) = ready();
        e.drain_plain().for_each(drop);
        e.store_mut().update_markets(|market| {
            market.limits.leverage.min = min;
            market.limits.leverage.max = max;
        });
        command(
            &mut e,
            Command::SetLeverage {
                command: 99,
                symbol: "PF_XBTUSD".into(),
                leverage: 5.0,
                side: None,
            },
        );
        let out = e.drain_plain().collect::<Vec<_>>();
        assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
        assert!(out.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 99, error }) if error.legacy_text().contains("leverage limits"))));
    }
}
#[test]
fn market_entry_with_tpsl_names_the_unsupported_legs() {
    let (mut e, _) = ready();
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 41,
            orders: vec![serde_json::from_str(
                r#"{"symbol":"PF_XBTUSD","type":"market","side":"buy","amount":1,"stopLoss":90}"#,
            )
            .unwrap()],
        }),
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 41, error: m }) if m.legacy_text().contains("take-profit/stop-loss legs on a market entry"))));
}
#[test]
fn app_protective_payload_with_matching_trigger_price_places_like_price_only() {
    let app = |kind: &str, price: f64, trigger: Option<f64>| {
        let mut o = opts(
            json!({"symbol":"PF_XBTUSD","side":"sell","type":kind,"notional":12,"timeInForce":"GoodTillCancel","reduceOnly":true,"price":price,"amount":0.1234}),
        );
        o.trigger_price = trigger;
        o
    };
    for (kind, price) in [("stop_market", 90.5), ("take_profit_market", 110.5)] {
        let mut sent = vec![];
        for trigger in [Some(price), None] {
            let (mut e, _) = ready();
            place(&mut e, vec![app(kind, price, trigger)]);
            let r = http(&mut e, "sendorder");
            let mut params = r
                .body
                .unwrap()
                .split('&')
                .filter(|p| !p.starts_with("cliOrdId="))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            params.sort();
            sent.push(params);
        }
        assert!(sent[0].contains(&format!("stopPrice={price}")), "{kind}");
        assert_eq!(sent[0], sent[1], "{kind}");
    }
    let (mut e, _) = ready();
    place(&mut e, vec![app("stop_market", 90.5, Some(90.0))]);
    let all = e.drain_plain().collect::<Vec<_>>();
    assert!(!all.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        all.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}
#[test]
fn zero_bracket_prices_mean_no_leg_on_market_and_limit_entries() {
    for (command, kind) in [
        (42, r#""type":"market""#),
        (43, r#""type":"limit","price":100"#),
    ] {
        let (mut e, _) = ready();
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![serde_json::from_str(&format!(
                    r#"{{"symbol":"PF_XBTUSD",{kind},"side":"buy","amount":1,"stopLoss":0,"takeProfit":0}}"#
                ))
                .unwrap()],
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        let writes = actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
            .count();
        assert_eq!(writes, 1, "{kind}: only the entry is sent");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. }))),
            "{kind}"
        );
    }
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    let unknown = (None, Some(Outcome::Unknown));
    let send = |status: &str| {
        json!({"result":"success","sendStatus":{"order_id":"","status":status}}).to_string()
    };
    for (status, body, classified, text) in [
        (
            503,
            "<html>".to_owned(),
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
            "Invalid Kraken trading response",
        ),
        (
            503,
            r#"{"result":"error"}"#.to_owned(),
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
            "Kraken HTTP 503",
        ),
        (
            429,
            r#"{"result":"error"}"#.to_owned(),
            rejected(ErrorKind::RateLimited),
            "Kraken HTTP 429",
        ),
        (
            400,
            r#"{"result":"error","error":"invalidArgument"}"#.to_owned(),
            rejected(ErrorKind::Rejected),
            "Kraken HTTP 400: invalidArgument",
        ),
        (
            200,
            r#"{"result":"error","error":"apiLimitExceeded"}"#.to_owned(),
            rejected(ErrorKind::RateLimited),
            "apiLimitExceeded",
        ),
        (
            200,
            r#"{"result":"error","error":"insufficientFunds"}"#.to_owned(),
            rejected(ErrorKind::Rejected),
            "insufficientFunds",
        ),
        (
            200,
            r#"{"result":"error","error":"Unavailable"}"#.to_owned(),
            unknown,
            "Unavailable",
        ),
        (
            200,
            r#"{"result":"error","error":"somethingNew"}"#.to_owned(),
            unknown,
            "somethingNew",
        ),
        (
            200,
            send("insufficientAvailableFunds"),
            rejected(ErrorKind::Rejected),
            "Kraken placement rejected or incomplete: insufficientAvailableFunds",
        ),
        (
            200,
            send("partiallyFilled"),
            unknown,
            "Kraken placement rejected or incomplete: partiallyFilled",
        ),
        (
            200,
            r#"{"result":"success"}"#.to_owned(),
            unknown,
            "Kraken placement missing sendStatus",
        ),
    ] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit()]);
        let r = http(&mut e, "sendorder");
        reply(&mut e, &r, status, &body);
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(
            ((error.kind, error.outcome), error.legacy_text().as_str()),
            (classified, text),
            "{status} {body}"
        );
    }
    // Transport loss on a write is Unknown.
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let r = http(&mut e, "sendorder");
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "reset",
        },
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    // Preflight refusals were never sent.
    let (mut e, _) = ready();
    let mut invalid = limit();
    invalid.client_order_id = Some("mine".into());
    place(&mut e, vec![invalid]);
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn a_refusal_after_an_accepted_placement_is_never_rejected() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit(), limit()]);
    let first = http(&mut e, "sendorder");
    ack(&mut e, &first, "accepted", "placed");
    let second = http(&mut e, "sendorder");
    reply(
        &mut e,
        &second,
        200,
        r#"{"result":"error","error":"insufficientFunds"}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(
        error.legacy_text(),
        r#"{"message":"insufficientFunds","partialOrderIds":["accepted"],"partialOrderCount":1}"#
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn edit_and_cancel_statuses_never_claim_a_refusal_after_an_effect() {
    use safe_cex_engine::Outcome;
    for (status, outcome) in [
        ("invalidSize", Some(Outcome::Rejected)),
        // These cancel the resting order, possibly after fills.
        ("postWouldExecute", Some(Outcome::Unknown)),
        ("iocWouldNotExecute", Some(Outcome::Unknown)),
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
        reply(
            &mut e,
            &r,
            200,
            &json!({"result":"success","editStatus":{"orderId":"old","status":status}}).to_string(),
        );
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(error.outcome, outcome, "{status}");
    }
    // An unproven batch entry keeps the command Unknown after a later refusal.
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
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","batchStatus":[{"status":"cancelled"},{"status":"notFound","order_id":"b"}]}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.legacy_text(),
        "Kraken cancellation rejected or identity mismatch: notFound"
    );
}

#[test]
fn beta_partial_placement_retains_ids_and_reconciles_uncertainty() {
    for body in [
        r#"{"result":"error","error":"insufficientFunds"}"#,
        "<html>",
    ] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit(), limit(), limit()]);
        let first = http(&mut e, "sendorder");
        ack(&mut e, &first, "accepted", "placed");
        let second = http(&mut e, "sendorder");
        reply(
            &mut e,
            &second,
            if body == "<html>" { 503 } else { 200 },
            body,
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("openorders")))
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("sendorder")))
        );
    }
}

#[test]
fn beta_non_success_status_keeps_native_error_classification() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, code, kind, outcome) in [
        (
            400,
            "apiLimitExceeded",
            Some(ErrorKind::RateLimited),
            Outcome::Rejected,
        ),
        (
            403,
            "authenticationError",
            Some(ErrorKind::AuthInvalid),
            Outcome::Rejected,
        ),
        (400, "unknownError", None, Outcome::Unknown),
        (
            503,
            "insufficientFunds",
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
        ),
        (
            408,
            "invalidArgument",
            Some(ErrorKind::Timeout),
            Outcome::Unknown,
        ),
    ] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit()]);
        let request = http(&mut e, "sendorder");
        reply(
            &mut e,
            &request,
            status,
            &json!({"result":"error","error":code}).to_string(),
        );
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(
            (error.kind, error.outcome),
            (kind, Some(outcome)),
            "{status} {code}"
        );
        assert_eq!(error.venue_code.as_deref(), Some(code));
    }
}

#[test]
fn beta_an_old_inflight_read_cannot_clear_a_new_uncertain_write() {
    let (mut e, _) = ready();
    let mut other = limit();
    other.symbol = "PF_ETHUSD".into();
    place(&mut e, vec![other]);
    let request = http(&mut e, "sendorder");
    reply(&mut e, &request, 503, "<html>");
    let old = http(&mut e, "openorders");
    place(&mut e, vec![limit()]);
    let request = http(&mut e, "sendorder");
    reply(&mut e, &request, 503, "<html>");
    assert!(
        !requests(&mut e)
            .iter()
            .any(|r| r.url.ends_with("openorders"))
    );
    reply(&mut e, &old, 200, OPEN);
    let fresh = http(&mut e, "openorders");
    assert_ne!(old.id, fresh.id);
    command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 81,
            symbol: "PF_XBTUSD".into(),
        },
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {command:81, error}) if error.message.contains("uncertain"))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    reply(&mut e, &fresh, 200, OPEN);
    command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 82,
            symbol: "PF_XBTUSD".into(),
        },
    );
    assert!(
        e.drain_plain()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 82, .. })))
    );
}

#[test]
fn beta_mixed_cancel_keeps_confirmed_ids_and_unknown() {
    let (mut e, _) = ready();
    inject(&mut e, "a", OrderType::Limit, None);
    inject(&mut e, "b", OrderType::Limit, None);
    command(
        &mut e,
        Command::CancelOrders {
            command: 81,
            ids: vec!["a".into(), "b".into()],
        },
    );
    let request = http(&mut e, "batchorder");
    reply(
        &mut e,
        &request,
        200,
        r#"{"result":"success","batchStatus":[{"status":"cancelled","order_id":"a"},{"status":"notFound","order_id":"b"}]}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["a"]);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(e.store().order("a").is_none());
    assert!(e.store().order("b").is_some());
}

#[test]
fn beta_unknown_placement_fences_retry_and_amend_until_a_fresh_read() {
    let (mut e, _) = ready();
    let mut other = limit();
    other.symbol = "PF_ETHUSD".into();
    place(&mut e, vec![other]);
    let prior = http(&mut e, "sendorder");
    reply(&mut e, &prior, 503, "<html>");
    let old = http(&mut e, "openorders");
    inject(&mut e, "amend-target", OrderType::Limit, None);

    place(&mut e, vec![limit()]);
    let unknown = http(&mut e, "sendorder");
    reply(&mut e, &unknown, 503, "<html>");
    let sealed = e.drain_plain().collect::<Vec<_>>();
    assert!(sealed.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 7, error })
            if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
    assert!(
        !sealed
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("openorders")))
    );

    let refused = |e: &mut Engine<Kraken>, input: Command, id| {
        command(e, input);
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
            "an uncertain placement must not admit another write: {actions:?}"
        );
        assert!(
            actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command, error })
                if *command == id && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                && error.message.contains("uncertain"))),
            "{actions:?}"
        );
    };
    refused(
        &mut e,
        Command::PlaceOrders {
            command: 81,
            orders: vec![limit()],
        },
        81,
    );
    refused(
        &mut e,
        Command::AmendOrder {
            command: 82,
            id: "amend-target".into(),
            amount: Some(0.2),
            price: Some(98.),
        },
        82,
    );
    reply(&mut e, &old, 200, OPEN);
    let fresh = http(&mut e, "openorders");
    assert_ne!(old.id, fresh.id);
    refused(
        &mut e,
        Command::PlaceOrders {
            command: 83,
            orders: vec![limit()],
        },
        83,
    );
    reply(&mut e, &fresh, 200, OPEN);
    command(
        &mut e,
        Command::PlaceOrders {
            command: 84,
            orders: vec![limit()],
        },
    );
    http(&mut e, "sendorder");
}

#[test]
fn beta_slow_reconcile_joins_reasks_and_only_a_clean_retry_releases() {
    let (mut e, _) = ready();
    let collect = |e: &mut safe_cex_engine::Engine<Kraken>| e.drain_plain().collect::<Vec<_>>();
    let read = |actions: &[Action]| {
        actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("openorders") => Some(r.clone()),
                _ => None,
            })
            .expect("a reconcile read")
    };
    let place = |e: &mut safe_cex_engine::Engine<Kraken>, now, command| {
        e.handle(
            now,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![limit()],
            }),
        );
    };
    let respond = |e: &mut safe_cex_engine::Engine<Kraken>,
                   now,
                   request: &safe_cex_engine::HttpRequest,
                   status,
                   body: &str| {
        e.handle(
            now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    };
    let refused = |actions: &[Action]| {
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("sendorder")))
        );
        assert!(
            actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { error, .. })
                if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                    && error.message.contains("uncertain"))),
            "{actions:?}"
        );
    };
    place(&mut e, NOW, 180);
    let sent = collect(&mut e);
    let write = sent
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("sendorder") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    respond(&mut e, NOW, &write, 503, "<html>");
    let mut log = collect(&mut e);
    let held = read(&log);
    // These are the engine's captured re-ask wakes, while the original read is held.
    let fire = |e: &mut safe_cex_engine::Engine<Kraken>, log: &mut Vec<Action>, offset: f64| {
        let timer = log
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + offset => Some(*timer),
                _ => None,
            })
            .expect("the engine reconcile wake");
        e.handle(NOW + offset, Event::TimerFired { timer });
        let out = collect(e);
        log.extend(out.iter().cloned());
        out
    };
    for offset in [1000., 3000., 7000.] {
        let out = fire(&mut e, &mut log, offset);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("openorders"))),
            "a re-ask must join the original read: {out:?}"
        );
    }
    // Failure preserves the fence; a later engine wake supplies another read.
    respond(&mut e, NOW + 7100., &held, 503, "<html>");
    log.extend(collect(&mut e));
    place(&mut e, NOW + 7200., 181);
    refused(&collect(&mut e));
    let second = read(&fire(&mut e, &mut log, 15000.));
    respond(
        &mut e,
        NOW + 15100.,
        &second,
        200,
        r#"{"result":"success","openOrders":[{"order_id":"unknown"}]}"#,
    );
    log.extend(collect(&mut e));
    place(&mut e, NOW + 15200., 182);
    refused(&collect(&mut e));
    let third = read(&fire(&mut e, &mut log, 31000.));
    respond(&mut e, NOW + 31100., &third, 200, OPEN);
    collect(&mut e);
    place(&mut e, NOW + 31200., 183);
    assert!(
        collect(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("sendorder")))
    );
}
