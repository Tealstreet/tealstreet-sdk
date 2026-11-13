// Native REST payload and response contracts from Kraken's Futures order docs.
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
// TS: packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts:452-462,514-529.
// KNOWN-ISSUE: TS sends above-max; venue refuses.
#[test]
fn legacy_above_max_sends_one_rounded_payload_and_surfaces_native_refusal() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (amount, maximum, step, expected) in [
        (26.0, 5.0, 1.0, "26"),
        (11.0, 5.0, 1.0, "11"),
        (12.0, 5.0, 1.0, "12"),
        (10.0, 5.0, 1.0, "10"),
        (7.0, 3.0, 0.5, "7"),
        (0.0264, 0.005, 0.001, "0.026"),
        (5.6, 5.0, 1.0, "6"),
    ] {
        for (kind, native) in [
            (OrderType::Limit, "lmt"),
            (OrderType::Market, "mkt"),
            (OrderType::StopLoss, "stp"),
            (OrderType::TakeProfit, "take_profit"),
            (OrderType::TrailingStopLoss, "trailing_stop"),
        ] {
            let (mut e, _) = ready();
            e.store_mut().update_markets(|market| {
                market.precision.amount = step;
                market.limits.amount.min = step;
                market.limits.amount.max = maximum;
            });
            let mut order = limit();
            order.kind = kind;
            order.amount = amount;
            order.price = Some(90.0);
            place(&mut e, vec![order]);
            let writes = requests(&mut e);
            assert_eq!(writes.len(), 1, "{amount}/{maximum}/{step}: {kind:?}");
            let request = &writes[0];
            assert_eq!(request.method, Method::Post);
            assert!(request.url.ends_with("sendorder"));
            assert_eq!(param(request, "size").as_deref(), Some(expected));
            assert_eq!(param(request, "orderType").as_deref(), Some(native));
            reply(
                &mut e,
                request,
                200,
                r#"{"result":"success","sendStatus":{"status":"invalidSize"}}"#,
            );
            let terminal = e.drain_plain().collect::<Vec<_>>();
            assert!(!terminal.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(
                !terminal
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            );
            let errors = terminal
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].kind, Some(ErrorKind::Rejected));
            assert_eq!(errors[0].outcome, Some(Outcome::Rejected));
            assert_eq!(errors[0].venue_code.as_deref(), Some("invalidSize"));
            assert!(errors[0].legacy_text().contains("invalidSize"));
        }
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
fn ambiguous_write_is_not_retried_and_queued_command_is_not_sent() {
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
    let all = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(all.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error })
                if error.outcome == Some(safe_cex_engine::Outcome::Unknown))
    ));
    assert!(all.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 9, error })
                if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                    && error.message.contains("Check Open Orders"))
    ));
    assert_eq!(
        all.iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("sendorder")))
            .count(),
        0
    );
}
#[test]
fn batch_cancellation_json_strict_count_status_and_identity() {
    for body in [
        json!({"result":"success","batchStatus":[]}),
        json!({"result":"success","batchStatus":[{"status":"cancelled","order_id":"a"},{"status":"cancelled","order_id":"a"}]}),
        json!({"result":"success","batchStatus":[{"status":"cancelled","order_id":"a"},{"status":"marketSuspended","order_id":"b"}]}),
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
        r#"{"feed":"open_orders","order":{"order_id":"new","symbol":"PF_XBTUSD","type":"lmt","direction":0,"qty":0.15,"filled":0.05,"limit_price":102}}"#,
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
        let amount = if changed { 0.25 } else { 0.073 };
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
fn write_failures_preserve_definitive_refusals_and_explain_ambiguity() {
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
            "Kraken write outcome is unknown. Check Open Orders",
        ),
        (
            503,
            r#"{"result":"error"}"#.to_owned(),
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
            "Kraken write outcome is unknown. Check Open Orders",
        ),
        (
            429,
            r#"{"result":"error"}"#.to_owned(),
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
            "Kraken write outcome is unknown. Check Open Orders",
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
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
            "Kraken write outcome is unknown. Check Open Orders",
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
            "Kraken write outcome is unknown. Check Open Orders: Unavailable",
        ),
        (
            200,
            r#"{"result":"error","error":"somethingNew"}"#.to_owned(),
            unknown,
            "Kraken write outcome is unknown. Check Open Orders: somethingNew",
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
            "Kraken write outcome is unknown. Check Open Orders: Kraken placement rejected or incomplete: partiallyFilled",
        ),
        (
            200,
            r#"{"result":"success"}"#.to_owned(),
            unknown,
            "Kraken write outcome is unknown. Check Open Orders: Kraken placement missing sendStatus",
        ),
    ] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit()]);
        let r = http(&mut e, "sendorder");
        reply(&mut e, &r, status, &body);
        let [error] = failures(&mut e).try_into().unwrap();
        if body == r#"{"result":"success"}"# {
            // Legacy leaves malformed ACK kind unspecified; managed custody types it Internal.
            assert!(matches!(error.kind, None | Some(ErrorKind::Internal)));
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(error.legacy_text(), text);
        } else if classified.1 == Some(Outcome::Unknown) && text.contains("Check Open Orders") {
            // Managed custody uses the engine's ambiguity wording.
            assert_eq!((error.kind, error.outcome), classified);
            assert!(error.legacy_text().contains("Check Open Orders"));
        } else {
            assert_eq!(
                ((error.kind, error.outcome), error.legacy_text().as_str()),
                (classified, text),
                "{status} {body}"
            );
        }
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
        r#"{"message":"Kraken write outcome is unknown. Check Open Orders: insufficientFunds","partialOrderIds":["accepted"],"partialOrderCount":1}"#
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
        r#"{"result":"success","batchStatus":[{"status":"cancelled"},{"status":"marketSuspended","order_id":"b"}]}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.legacy_text(),
        "Kraken write outcome is unknown. Check Open Orders: Kraken cancellation rejected or identity mismatch: cancelled"
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
            Outcome::Unknown,
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
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {command:81, error}) if error.message.contains("uncertain") && error.outcome == Some(safe_cex_engine::Outcome::NotSent))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    reply(&mut e, &fresh, 200, OPEN);
    command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 82,
            symbol: "PF_XBTUSD".into(),
        },
    );
    let cancel = http(&mut e, "cancelallorders");
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"result":"success","cancelStatus":{"status":"noOrdersToCancel"}}"#,
    );
    assert!(
        e.drain_plain()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 82, .. })))
    );
}

#[test]
fn symbol_cancel_hits_the_venue_with_no_order_in_the_store() {
    let (mut e, _) = ready();
    command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 83,
            symbol: "PF_XBTUSD".into(),
        },
    );
    let cancel = http(&mut e, "cancelallorders");
    assert!(
        cancel
            .body
            .as_deref()
            .unwrap_or_default()
            .contains("symbol=PF_XBTUSD")
    );
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"result":"success","cancelStatus":{"status":"cancelled","cancelledOrders":[]}}"#,
    );
    assert!(
        e.drain_plain()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 83, .. })))
    );
}

#[test]
fn beta_mixed_cancel_keeps_confirmed_ids_and_unknown() {
    for factory in [ready, ready_provider] {
        let (mut e, _) = factory();
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
            r#"{"result":"success","batchStatus":[{"status":"cancelled","order_id":"a"},{"status":"marketSuspended","order_id":"b"}]}"#,
        );
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["a"]);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(e.store().order("a").is_none());
        assert!(e.store().order("b").is_some());
    }
}

#[test]
fn provider_partial_placement_keeps_unknown_and_fences_until_reconciled() {
    let (mut e, _) = ready_provider();
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
    let settled = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let error = settled
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
    let read = settled
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("openorders") => Some(request.clone()),
            _ => None,
        })
        .expect("partial placement must reconcile");
    command(
        &mut e,
        Command::PlaceOrders {
            command: 8,
            orders: vec![limit()],
        },
    );
    assert!(
        !requests(&mut e)
            .iter()
            .any(|r| r.url.ends_with("sendorder"))
    );
    reply(&mut e, &read, 200, OPEN);
    command(
        &mut e,
        Command::PlaceOrders {
            command: 9,
            orders: vec![limit()],
        },
    );
    http(&mut e, "sendorder");
}

#[test]
fn provider_native_refusals_keep_documented_status_and_effect_semantics() {
    use safe_cex_engine::Outcome;
    for status in [
        "invalidOrderType",
        "invalidSide",
        "clientOrderIdTooLong",
        "invalidDomainConfiguration",
    ] {
        let (mut e, _) = ready_provider();
        place(&mut e, vec![limit()]);
        let request = http(&mut e, "sendorder");
        ack(&mut e, &request, "", status);
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(error.outcome, Some(Outcome::Rejected), "{status}");
        assert_eq!(error.venue_code.as_deref(), Some(status));
        command(
            &mut e,
            Command::PlaceOrders {
                command: 8,
                orders: vec![limit()],
            },
        );
        http(&mut e, "sendorder");
    }
    for (status, expected) in [
        ("invalidSize", Outcome::Rejected),
        ("postWouldExecute", Outcome::Unknown),
        ("iocWouldNotExecute", Outcome::Unknown),
    ] {
        let (mut e, _) = ready_provider();
        inject(&mut e, "old", OrderType::Limit, None);
        command(
            &mut e,
            Command::AmendOrder {
                command: 8,
                id: "old".into(),
                price: Some(101.),
                amount: Some(0.2),
            },
        );
        let request = http(&mut e, "editorder");
        reply(
            &mut e,
            &request,
            200,
            &json!({"result":"success","editStatus":{"orderId":"old","status":status}}).to_string(),
        );
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(error.outcome, Some(expected), "{status}");
        assert_eq!(error.venue_code.as_deref(), Some(status));
    }
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
fn beta_a_rejected_row_on_another_market_releases_the_fence() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let unknown = http(&mut e, "sendorder");
    reply(&mut e, &unknown, 503, "<html>");
    let fresh = http(&mut e, "openorders");
    reply(
        &mut e,
        &fresh,
        200,
        r#"{"result":"success","openOrders":[{"order_id":"77","symbol":"PF_ETHUSD"}]}"#,
    );
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
    slow_reconcile(false);
}

#[test]
fn beta_slow_fresh_read_releases_after_three_engine_reasks() {
    slow_reconcile(true);
}

fn slow_reconcile(held_completes_cleanly: bool) {
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
    if held_completes_cleanly {
        place(&mut e, NOW + 7050., 181);
        refused(&collect(&mut e));
        respond(&mut e, NOW + 7100., &held, 200, OPEN);
        let completed = collect(&mut e);
        assert!(
            !completed.iter().any(|a| matches!(a,
            Action::Http(r) if r.url.ends_with("openorders"))),
            "the original clean read must not owe a spurious successor: {completed:?}"
        );
        place(&mut e, NOW + 7200., 182);
        assert!(
            collect(&mut e).iter().any(|a| matches!(a,
            Action::Http(r) if r.url.ends_with("sendorder"))),
            "the original held read releases the fence"
        );
        return;
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

// deaws2's live bodies follow.
#[test]
fn a_stop_amend_edits_in_place_with_its_stop_price() {
    let (mut e, _) = ready();
    inject(
        &mut e,
        "old",
        OrderType::StopLoss,
        Some(StopTriggerType::LastPrice),
    );
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(85.0),
            amount: None,
        },
    );
    let r = http(&mut e, "editorder");
    assert_eq!(param(&r, "stopPrice").unwrap(), "85");
    assert_eq!(param(&r, "limitPrice"), None);
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","editStatus":{"status":"edited","orderId":"old"}}"#,
    );
    assert!(done(&mut e).is_ok());
    assert_eq!(e.store().order("old").unwrap().price, 85.0);
}

#[test]
fn a_trailing_stop_amend_still_refuses() {
    let (mut e, _) = ready();
    inject(&mut e, "old", OrderType::TrailingStopLoss, None);
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: Some(85.0),
            amount: None,
        },
    );
    assert!(done(&mut e).is_err());
}

#[test]
fn an_amend_sends_the_total_under_absolute_quantity_mode() {
    let (mut e, socket) = ready();
    inject(&mut e, "old", OrderType::Limit, None);
    message(&mut e, socket, NOW, &json!({"feed":"open_orders","order":{"order_id":"old","symbol":"PF_XBTUSD","type":"lmt","direction":1,"qty":8.0,"filled":5.0,"limit_price":90}}).to_string());
    assert_eq!(e.store().order("old").unwrap().amount, 13.0);
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: None,
            amount: Some(13.0),
        },
    );
    let r = http(&mut e, "editorder");
    assert_eq!(param(&r, "qtyMode").unwrap(), "ABSOLUTE");
    assert_eq!(param(&r, "size").unwrap(), "13");
}

#[test]
fn live_refusal_codes_with_detail_are_refusals_and_leave_no_fence() {
    for body in [
        r#"{"result":"error","error":"nonceBelowThreshold: TOO_SMALL"}"#,
        r#"{"result":"error","error":"invalidArgument: 0"}"#,
        r#"{"result":"error","error":"invalidSide"}"#,
        r#"{"status":"BAD_REQUEST","result":"error","errors":[{"code":98,"message":"MAX_LEVERAGE_OUT_OF_BOUNDS"}]}"#,
    ] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit()]);
        let r = http(&mut e, "sendorder");
        reply(&mut e, &r, 200, body);
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(
            error.outcome,
            Some(safe_cex_engine::Outcome::Rejected),
            "{body}"
        );
        place(&mut e, vec![limit()]);
        http(&mut e, "sendorder");
    }
}

#[test]
fn a_market_order_placed_with_executions_does_not_rest() {
    let (mut e, _) = ready();
    let mut order = limit();
    order.kind = OrderType::Market;
    order.price = None;
    place(&mut e, vec![order]);
    let r = http(&mut e, "sendorder");
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","sendStatus":{"status":"placed","order_id":"m1","orderEvents":[{"type":"EXECUTION","amount":2}]}}"#,
    );
    assert!(done(&mut e).is_ok());
    assert!(e.store().order("m1").is_none());
}

#[test]
fn cancelling_a_gone_order_is_done() {
    let (mut e, _) = ready();
    inject(&mut e, "a", OrderType::Limit, None);
    command(
        &mut e,
        Command::CancelOrders {
            command: 8,
            ids: vec!["a".into()],
        },
    );
    let r = http(&mut e, "cancelorder");
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","cancelStatus":{"status":"notFound","order_id":"a"}}"#,
    );
    assert!(done(&mut e).is_ok());
    assert!(e.store().order("a").is_none());
}

#[test]
fn order_events_of_any_shape_never_make_a_placed_order_invalid() {
    for events in [
        r#"null"#,
        r#"[{"type":"PLACE","order":{"orderId":"o"},"reducedQuantity":null}]"#,
        r#"[{"type":"EXECUTION","amount":null},{"type":null}]"#,
    ] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit()]);
        let r = http(&mut e, "sendorder");
        let body = format!(
            r#"{{"result":"success","sendStatus":{{"status":"placed","order_id":"n","orderEvents":{events}}}}}"#
        );
        reply(&mut e, &r, 200, &body);
        assert!(done(&mut e).is_ok(), "{events}");
    }
}

fn answer(status: &str) -> (Engine<Kraken>, HttpRequest) {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let write = http(&mut e, "sendorder");
    let body = format!(r#"{{"result":"success","sendStatus":{{"status":"{status}"}}}}"#);
    reply(&mut e, &write, 200, &body);
    (e, write)
}
fn outcome_of(actions: &[Action]) -> Option<safe_cex_engine::Outcome> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { error, .. }) => error.outcome,
        _ => None,
    })
}

#[test]
fn documented_send_statuses_refuse_and_leave_no_fence() {
    for status in [
        "invalidOrderType",
        "invalidSide",
        "clientOrderIdTooLong",
        "invalidDomainConfiguration",
    ] {
        let (mut e, _) = answer(status);
        let settled = e.drain_plain().collect::<Vec<_>>();
        assert_eq!(
            outcome_of(&settled),
            Some(safe_cex_engine::Outcome::Rejected),
            "{status}"
        );
        place(&mut e, vec![limit()]);
        http(&mut e, "sendorder");
    }
}

#[test]
fn a_duplicate_client_id_stays_unknown() {
    let (mut e, _) = answer("clientOrderIdAlreadyExist");
    let settled = e.drain_plain().collect::<Vec<_>>();
    assert_eq!(
        outcome_of(&settled),
        Some(safe_cex_engine::Outcome::Unknown)
    );
}

#[test]
fn a_json_parse_error_in_a_200_refuses_and_leaves_no_fence() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let write = http(&mut e, "sendorder");
    reply(
        &mut e,
        &write,
        200,
        r#"{"result":"error","error":"Json Parse Error"}"#,
    );
    let settled = e.drain_plain().collect::<Vec<_>>();
    assert_eq!(
        outcome_of(&settled),
        Some(safe_cex_engine::Outcome::Rejected)
    );
    place(&mut e, vec![limit()]);
    http(&mut e, "sendorder");
}

#[test]
fn an_unlisted_send_status_stays_unknown_and_clears_on_the_next_read() {
    let (mut e, _) = answer("wouldProcessAfterSpecifiedTime");
    let settled = e.drain_plain().collect::<Vec<_>>();
    assert_eq!(
        outcome_of(&settled),
        Some(safe_cex_engine::Outcome::Unknown)
    );
    let read = settled
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("openorders") => Some(r.clone()),
            _ => None,
        })
        .expect("the read starts with the answer");
    command(
        &mut e,
        Command::PlaceOrders {
            command: 8,
            orders: vec![limit()],
        },
    );
    let fenced = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !fenced
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("sendorder")))
    );
    reply(&mut e, &read, 200, OPEN);
    command(
        &mut e,
        Command::PlaceOrders {
            command: 9,
            orders: vec![limit()],
        },
    );
    http(&mut e, "sendorder");
}

/// Kraken's sendStatus enum lists partiallyFilled: the entry is live, so its brackets follow.
#[test]
fn a_partially_filled_entry_still_gets_its_stop() {
    let (mut e, _) = ready();
    let mut o = limit();
    o.stop_loss = Some(90.0);
    place(&mut e, vec![o]);
    let entry = http(&mut e, "sendorder");
    ack(&mut e, &entry, "entry", "partiallyFilled");
    let sl = http(&mut e, "sendorder");
    assert_eq!(param(&sl, "orderType").unwrap(), "stp");
    assert_eq!(param(&sl, "reduceOnly").unwrap(), "true");
    ack(&mut e, &sl, "stop", "placed");
    assert_eq!(done(&mut e).unwrap(), r#"["entry","stop"]"#);
}

#[test]
fn an_amended_size_is_rounded_to_the_step() {
    let (mut e, _) = ready();
    inject(&mut e, "old", OrderType::Limit, None);
    let mut order = e.store().order("old").unwrap().clone();
    order.amount = 0.3;
    order.filled = 0.1;
    order.remaining = 0.2;
    e.store_mut().upsert_order(order);
    command(
        &mut e,
        Command::AmendOrder {
            command: 8,
            id: "old".into(),
            price: None,
            amount: Some(0.3),
        },
    );
    let r = http(&mut e, "editorder");
    // Kraken's qtyMode ABSOLUTE: the total, fills included, so a fill not yet seen cannot grow it.
    assert_eq!(param(&r, "qtyMode").unwrap(), "ABSOLUTE");
    assert_eq!(param(&r, "size").unwrap(), "0.3");
}

/// An order already filled or gone cannot be cancelled, and that is not a failure.
#[test]
fn a_cancel_of_an_order_already_gone_is_done() {
    for body in [
        json!({"result":"success","cancelStatus":{"status":"notFound"}}),
        json!({"result":"success","cancelStatus":{"status":"filled","order_id":"a"}}),
    ] {
        let (mut e, _) = ready();
        inject(&mut e, "a", OrderType::Limit, None);
        command(
            &mut e,
            Command::CancelOrders {
                command: 8,
                ids: vec!["a".into()],
            },
        );
        let r = http(&mut e, "cancelorder");
        reply(&mut e, &r, 200, &body.to_string());
        assert!(done(&mut e).is_ok(), "{body}");
    }
}

#[test]
fn dispatched_write_ambiguity_matrix() {
    for (status, body) in [
        (429, "{}"),
        (429, r#"{"result":"error","error":"insufficientFunds"}"#),
        (429, r#"{"result":"error","error":"authenticationError"}"#),
        (429, r#"{"result":"error","error":"apiLimitExceeded"}"#),
        (200, r#"{"result":"error","error":"apiLimitExceeded"}"#),
        (408, r#"{"result":"error","error":"insufficientFunds"}"#),
        (500, r#"{"result":"error","error":"insufficientFunds"}"#),
        (503, r#"{"result":"error","error":"apiLimitExceeded"}"#),
        (408, r#"{"result":"error","error":"authenticationError"}"#),
        (500, r#"{"result":"error","error":"authenticationError"}"#),
        (599, r#"{"result":"error","error":"authenticationError"}"#),
        (400, r#"{"result":"error","error":"apiLimitExceeded"}"#),
        (404, r#"{"result":"error","error":"apiLimitExceeded"}"#),
    ] {
        for operation in [
            "create",
            "stop_create",
            "bracket",
            "amend",
            "amend_stop",
            "cancel",
            "cancel_stop",
            "setting",
        ] {
            let (mut e, _) = ready();
            let path = match operation {
                "create" | "stop_create" | "bracket" => {
                    let mut o = limit();
                    if operation == "stop_create" {
                        o.kind = OrderType::StopLoss;
                        o.price = Some(90.0);
                    }
                    if operation == "bracket" {
                        o.stop_loss = Some(90.0);
                        o.take_profit = Some(110.0);
                    }
                    place(&mut e, vec![o]);
                    "sendorder"
                }
                "amend" | "amend_stop" => {
                    inject(
                        &mut e,
                        "old",
                        if operation == "amend_stop" {
                            OrderType::StopLoss
                        } else {
                            OrderType::Limit
                        },
                        Some(StopTriggerType::LastPrice),
                    );
                    command(
                        &mut e,
                        Command::AmendOrder {
                            command: 7,
                            id: "old".into(),
                            price: Some(85.0),
                            amount: None,
                        },
                    );
                    "editorder"
                }
                "cancel" | "cancel_stop" => {
                    inject(
                        &mut e,
                        "old",
                        if operation == "cancel_stop" {
                            OrderType::StopLoss
                        } else {
                            OrderType::Limit
                        },
                        None,
                    );
                    command(
                        &mut e,
                        Command::CancelOrders {
                            command: 7,
                            ids: vec!["old".into()],
                        },
                    );
                    "cancelorder"
                }
                _ => {
                    command(
                        &mut e,
                        Command::SetLeverage {
                            command: 7,
                            symbol: "PF_XBTUSD".into(),
                            leverage: 5.0,
                            side: None,
                        },
                    );
                    "leveragepreferences"
                }
            };
            let mut r = http(&mut e, path);
            if operation == "bracket" {
                ack(&mut e, &r, "accepted-entry", "placed");
                r = http(&mut e, path);
                assert_eq!(param(&r, "orderType").as_deref(), Some("stp"));
            }
            assert_ne!(r.method, Method::Get);
            reply(&mut e, &r, status, body);
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            let errors: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1, "{operation} {status}: {actions:?}");
            let error = errors[0];
            let case = format!("{operation} {status} {body}: {error:?}");
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "{case}"
            );
            let text = error.legacy_text().to_lowercase();
            assert!(
                text.contains("unknown") && text.contains("check open orders"),
                "{case}"
            );
            assert!(
                !text.contains("refused") && !text.contains("no stop"),
                "{case}"
            );
            assert_eq!(error.protection, None, "{case}");
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
                "resend/put-back: {case}"
            );
        }
    }
}

fn futures_queued_write_after_failure(
    first_kind: &str,
    next_kind: &str,
    status: u16,
    body: &str,
    unknown: bool,
) {
    use safe_cex_engine::Outcome;
    let (mut e, _) = ready();
    inject(&mut e, "active-edit", OrderType::Limit, None);
    inject(&mut e, "active-cancel", OrderType::Limit, None);
    inject(&mut e, "queued-edit", OrderType::Limit, None);
    let first_command = match first_kind {
        "sendorder" => Command::PlaceOrders {
            command: 901,
            orders: vec![limit()],
        },
        "editorder" => Command::AmendOrder {
            command: 901,
            id: "active-edit".into(),
            price: Some(94.),
            amount: None,
        },
        "cancelorder" => Command::CancelOrders {
            command: 901,
            ids: vec!["active-cancel".into()],
        },
        _ => unreachable!(),
    };
    command(&mut e, first_command);
    let first = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with(first_kind))
        .expect("first physical write");
    let next = if next_kind == "place" {
        Command::PlaceOrders {
            command: 902,
            orders: vec![limit()],
        }
    } else {
        Command::AmendOrder {
            command: 902,
            id: "queued-edit".into(),
            price: Some(95.),
            amount: None,
        }
    };
    command(&mut e, next);
    let queued = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(
        !queued.iter().any(|a| matches!(a, Action::Http(r) if
        r.url.ends_with("sendorder") || r.url.ends_with("editorder"))),
        "second command must queue"
    );
    assert!(
        !queued.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 902, .. })
        )),
        "queued command was genuinely admitted"
    );
    if status == 0 {
        e.handle(
            NOW,
            Event::HttpFailed {
                request: first.id,
                error: body,
            },
        );
    } else {
        reply(&mut e, &first, status, body);
    }
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 901,
                error,
            }) => Some(error),
            _ => None,
        })
        .expect("original write completes");
    assert_eq!(
        error.outcome,
        Some(if unknown {
            Outcome::Unknown
        } else {
            Outcome::Rejected
        })
    );
    let writes: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.ends_with("sendorder") || r.url.ends_with("editorder") => {
                Some(r.url.clone())
            }
            _ => None,
        })
        .collect();
    if unknown {
        if first_kind != "cancelorder" {
            command(
                &mut e,
                Command::PlaceOrders {
                    command: 903,
                    orders: vec![limit()],
                },
            );
            let blocked = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            assert!(blocked.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 903, error })
                    if error.outcome == Some(Outcome::NotSent) && error.message.contains("uncertain"))),
                "the original uncertainty fence must remain held");
        }
        assert!(
            writes.is_empty(),
            "{first_kind} released queued {next_kind} before reconciliation: {writes:?}"
        );
        assert!(actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 902, error })
                if error.outcome == Some(Outcome::NotSent) && error.message.contains("Check Open Orders"))));
        if first_kind != "cancelorder" {
            let read = actions
                .iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.url.ends_with("openorders") => Some(r),
                    _ => None,
                })
                .expect("the original uncertainty starts reconciliation");
            reply(&mut e, read, 200, OPEN);
            assert!(
                requests(&mut e)
                    .iter()
                    .all(|r| { !r.url.ends_with("sendorder") && !r.url.ends_with("editorder") }),
                "reconciliation must not replay refused jobs"
            );
            command(
                &mut e,
                Command::PlaceOrders {
                    command: 908,
                    orders: vec![limit()],
                },
            );
            http(&mut e, "sendorder");
        }
    } else {
        assert_eq!(writes.len(), 1, "definitive refusals may advance the queue");
    }
}

fn futures_unknown_queued_writes(first_kind: &str) {
    for next_kind in ["place", "amend"] {
        for (status, body) in [
            (200, r#"{"result":"error","error":"apiLimitExceeded"}"#),
            (503, "<html>"),
            (0, "connection reset"),
        ] {
            futures_queued_write_after_failure(first_kind, next_kind, status, body, true);
        }
    }
}
#[test]
fn futures_unknown_sendorder_refuses_queued_place_and_edit() {
    futures_unknown_queued_writes("sendorder");
}
#[test]
fn futures_unknown_editorder_refuses_queued_place_and_edit() {
    futures_unknown_queued_writes("editorder");
}
#[test]
fn futures_unknown_cancel_refuses_queued_place_and_edit() {
    futures_unknown_queued_writes("cancelorder");
}
#[test]
fn futures_definitive_refusals_advance_queued_writes() {
    for first_kind in ["sendorder", "editorder", "cancelorder"] {
        for next_kind in ["place", "amend"] {
            futures_queued_write_after_failure(
                first_kind,
                next_kind,
                200,
                r#"{"result":"error","error":"insufficientFunds"}"#,
                false,
            );
        }
    }
}

#[test]
fn futures_unknown_or_partial_result_keeps_queued_cancels_and_settings() {
    use safe_cex_engine::Outcome;
    for partial in [false, true] {
        let (mut e, _) = ready();
        inject(&mut e, "queued-edit", OrderType::Limit, None);
        let orders = if partial {
            vec![limit(), limit()]
        } else {
            vec![limit()]
        };
        command(
            &mut e,
            Command::PlaceOrders {
                command: 901,
                orders,
            },
        );
        let mut first = http(&mut e, "sendorder");
        for next in [
            Command::PlaceOrders {
                command: 902,
                orders: vec![limit()],
            },
            Command::AmendOrder {
                command: 903,
                id: "queued-edit".into(),
                price: Some(95.),
                amount: None,
            },
            Command::CancelOrders {
                command: 904,
                ids: vec!["safety-cancel".into()],
            },
            Command::SetLeverage {
                command: 905,
                symbol: "PF_XBTUSD".into(),
                leverage: 10.,
                side: None,
            },
            Command::FetchLeverage {
                command: 906,
                symbol: "PF_XBTUSD".into(),
            },
        ] {
            command(&mut e, next);
        }
        assert!(
            requests(&mut e).is_empty(),
            "all later jobs wait for the active write"
        );
        if partial {
            ack(&mut e, &first, "accepted-entry", "placed");
            first = http(&mut e, "sendorder");
        }
        reply(
            &mut e,
            &first,
            200,
            if partial {
                r#"{"result":"error","error":"insufficientFunds"}"#
            } else {
                r#"{"result":"error","error":"apiLimitExceeded"}"#
            },
        );
        let settled = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let original = settled
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: 901,
                    error,
                }) => Some(error),
                _ => None,
            })
            .expect("original command settles");
        assert_eq!(original.outcome, Some(Outcome::Unknown));
        if partial {
            assert_eq!(
                original.partial.as_ref().unwrap().order_ids,
                ["accepted-entry"]
            );
        }
        for refused in [902, 903] {
            assert!(settled.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command, error })
                    if *command == refused && error.outcome == Some(Outcome::NotSent))));
        }
        let writes: Vec<_> = settled
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.method != Method::Get => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(writes.len(), 1);
        assert!(writes[0].url.ends_with("cancelorder"));
        cancel_ack(&mut e, writes[0], "safety-cancel");
        let after_cancel = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(
            after_cancel
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 904, .. })))
        );
        let setting = after_cancel
            .iter()
            .find_map(|a| match a {
                Action::Http(r)
                    if r.method == Method::Put && r.url.ends_with("leveragepreferences") =>
                {
                    Some(r)
                }
                _ => None,
            })
            .expect("queued setting survives");
        reply(&mut e, setting, 200, r#"{"result":"success"}"#);
        let after_setting = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        for completed in [905, 906] {
            assert!(after_setting.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandDone { command, .. }) if *command == completed)));
        }
        assert!(!after_setting.iter().any(|a| matches!(a, Action::Http(r)
            if r.url.ends_with("sendorder") || r.url.ends_with("editorder"))));
    }
}

fn postmerge_cancel_failure(body: &str, batch: bool) -> safe_cex_engine::VenueError {
    let (mut e, _) = ready();
    if batch {
        inject(&mut e, "a", OrderType::Limit, None);
        inject(&mut e, "b", OrderType::Limit, None);
    }
    command(
        &mut e,
        if batch {
            Command::CancelOrders {
                command: 81,
                ids: vec!["a".into(), "b".into()],
            }
        } else {
            Command::CancelSymbolOrders {
                command: 81,
                symbol: "PF_XBTUSD".into(),
            }
        },
    );
    let request = http(
        &mut e,
        if batch {
            "batchorder"
        } else {
            "cancelallorders"
        },
    );
    reply(&mut e, &request, 200, body);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
    );
    out.into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("one failure")
}
#[test]
fn postmerge_native_batch_rate_failure_is_actionable() {
    let error = postmerge_cancel_failure(
        r#"{"result":"success","batchStatus":[{"status":"apiLimitExceeded","order_id":"a"},{"status":"invalidSize","order_id":"b"}]}"#,
        true,
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), Some("apiLimitExceeded"));
    assert!(error.message.contains("apiLimitExceeded"), "{error:?}");
    assert!(!error.message.contains("invalidSize"), "{error:?}");
    assert!(
        error.message.contains("unknown") && error.message.contains("Check Open Orders"),
        "{error:?}"
    );
}
#[test]
fn postmerge_cancel_all_rate_status_is_actionable() {
    let error = postmerge_cancel_failure(
        r#"{"result":"success","cancelStatus":{"status":"apiLimitExceeded"}}"#,
        false,
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        error.message.contains("unknown") && error.message.contains("Check Open Orders"),
        "{error:?}"
    );
}
#[test]
fn postmerge_envelope_rate_control_is_actionable() {
    for batch in [false, true] {
        let error =
            postmerge_cancel_failure(r#"{"result":"error","error":"apiLimitExceeded"}"#, batch);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(
            error.message.contains("unknown") && error.message.contains("Check Open Orders"),
            "{error:?}"
        );
    }
}

#[test]
fn postmerge_documented_server_error_is_actionable_in_batch_and_cancel_all() {
    let mut errors = Vec::new();
    for batch in [false, true] {
        let error = postmerge_cancel_failure(r#"{"result":"error","error":"Server Error"}"#, batch);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        errors.push((batch, error));
    }
    assert!(
        errors
            .iter()
            .all(|(_, error)| error.message.contains("unknown")
                && error.message.contains("Check Open Orders")),
        "{errors:?}"
    );
}
