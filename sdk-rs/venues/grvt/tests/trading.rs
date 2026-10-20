use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_grvt::{Grvt, Options};
use serde::Deserialize;
use std::collections::VecDeque;
const NOW: f64 = 1_700_000_000_000.;
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
fn boot() -> Engine<Grvt> {
    let mut e = Engine::new(Grvt::new(Options {
        key: Some("grvt-test-key".into()),
        secret: Some(format!("0x{}", "01".repeat(32))),
        trading_account_id: Some("9007199254740993".into()),
        ..Options::default()
    }));
    e.start(NOW);
    let mut pending: VecDeque<_> = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    for _ in 0..30 {
        let Some(request) = pending.pop_front() else {
            break;
        };
        let (headers, body) = if request.url.ends_with("all_instruments") {
            ("", CATALOG)
        } else if request.url.ends_with("auth/api_key/login") {
            (
                "Set-Cookie: gravity=test-session; Path=/\nX-Grvt-Account-Id: test-account",
                r#"{"sub_account_id":"9007199254740993"}"#,
            )
        } else if request.url.ends_with("aggregated_account_summary") {
            (
                "",
                r#"{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"9007199254740993"}]}}"#,
            )
        } else if request.url.ends_with("get_authorized_builders") {
            (
                "",
                r#"{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}"#,
            )
        } else if request.url.ends_with("get_all_initial_leverage") {
            ("", r#"{"r":[{"i":"BTC_USDT_Perp","l":"10"}]}"#)
        } else if request.url.ends_with("open_orders") {
            ("", OPEN_ORDERS)
        } else {
            ("", r#"{"r":[]}"#)
        };
        e.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers,
                    body,
                },
            },
        );
        pending.extend(e.drain_actions().filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        }));
    }
    assert!(pending.is_empty());
    e
}
fn opts() -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTC_USDT_PERP".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 1.,
        price: Some(100.),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: Some(false),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: Some("9223372036854775807".into()),
        close: None,
    }
}
fn command(e: &mut Engine<Grvt>, c: Command) -> Vec<Action> {
    e.handle(NOW + 2., Event::Command(c));
    e.drain_actions().collect()
}
fn http(actions: Vec<Action>, suffix: &str) -> HttpRequest {
    actions
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                r.url.ends_with(suffix).then_some(r)
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("Missing request {suffix}"))
}
fn reply(e: &mut Engine<Grvt>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        NOW + 3.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn done(actions: &[Action], id: u32) -> Option<&Result<String, String>> {
    actions.iter().find_map(|a| {
        if let Action::Notice(Notice::CommandDone { command, result }) = a {
            (*command == id).then_some(result)
        } else {
            None
        }
    })
}
#[derive(Deserialize)]
struct Create {
    o: WireOrder,
}
#[derive(Deserialize)]
struct WireOrder {
    sa: String,
    im: bool,
    ro: bool,
    l: Vec<WireLeg>,
    s: WireSignature,
    m: WireMetadata,
    b: String,
    bf: String,
}
#[derive(Deserialize)]
struct WireLeg {
    i: String,
    s: String,
    #[serde(default)]
    lp: Option<String>,
    ib: bool,
}
#[derive(Deserialize)]
struct WireSignature {
    s: String,
    r: String,
    s1: String,
    v: u8,
    e: String,
    n: u32,
    ci: String,
}
#[derive(Deserialize)]
struct WireMetadata {
    co: String,
    #[serde(default)]
    t: Option<WireTrigger>,
}
#[derive(Deserialize)]
struct WireTrigger {
    tt: String,
    t: WireTriggerDetails,
}
#[derive(Deserialize)]
struct WireTriggerDetails {
    tb: String,
    tp: String,
    cp: bool,
    is: bool,
}
fn body(r: &HttpRequest) -> Create {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
fn seed(e: &mut Engine<Grvt>) {
    e.store_mut().upsert_order(Order {
        id: "42".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTC_USDT_PERP".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 100.,
        amount: 1.,
        filled: 0.2,
        remaining: 0.8,
        reduce_only: false,
        close: None,
        client_order_id: Some("42".into()),
        time_in_force: Some(TimeInForce::PostOnly),
        last_update: NOW,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
}
const OPEN: &str = r#"{"r":{"oi":"0x1234","im":false,"ti":"GOOD_TILL_TIME","po":true,"ro":false,"l":[{"i":"BTC_USDT_Perp","s":"1","lp":"100","ib":true}],"m":{"co":"42","ct":"1699999999000000000"},"s1":{"s":"OPEN","ts":["0.2"],"ut":"1700000000000000000"}}}"#;
const OPEN_ORDERS: &str = r#"{"r":[{"oi":"0x1234","im":false,"ti":"GOOD_TILL_TIME","po":true,"ro":false,"l":[{"i":"BTC_USDT_Perp","s":"1","lp":"100","ib":true}],"m":{"co":"42","ct":"1699999999000000000"},"s1":{"s":"OPEN","ts":["0.2"],"ut":"1700000000000000000"}}]}"#;
const CANCELED: &str = r#"{"r":{"oi":"0x1234","im":false,"ti":"GOOD_TILL_TIME","po":true,"ro":false,"l":[{"i":"BTC_USDT_Perp","s":"1","lp":"100","ib":true}],"m":{"co":"42","ct":"1699999999000000000"},"s1":{"s":"CANCELLED","ts":["0.2"],"ut":"1700000000001000000"}}}"#;
#[test]
fn signed_lite_order_preserves_large_native_account_and_caller_identity() {
    let mut e = boot();
    let r = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 1,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
    assert_eq!(r.url, "https://trades.grvt.io/lite/v1/create_order");
    let o = body(&r).o;
    assert_eq!(o.sa, "9007199254740993");
    assert_eq!(o.m.co, "9223372036854775807");
    assert!(!o.im && !o.ro);
    assert_eq!(o.l[0].i, "BTC_USDT_Perp");
    assert_eq!(o.l[0].s, "1");
    assert_eq!(o.l[0].lp.as_deref(), Some("100"));
    assert!(o.l[0].ib);
    assert_eq!(o.b, "0xee8f3559f298fffa79b8253437969d15b5dccff3");
    assert_eq!(o.bf, "0.03");
    assert_eq!(o.s.ci, "325");
    assert_eq!(o.s.e, "1700003600002000000");
    assert_eq!(o.s.s, "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1");
    assert_eq!((o.s.r.len(), o.s.s1.len()), (66, 66));
    assert!([27, 28].contains(&o.s.v));
    let _ = o.s.n;
    let actions = reply(&mut e, &r, 200, r#"{"r":{"oi":"0x00"}}"#);
    assert_eq!(
        done(&actions, 1).unwrap().as_ref().unwrap(),
        "[\"9223372036854775807\"]"
    );
}
#[test]
fn entire_batch_preflight_prevents_partial_writes() {
    let mut e = boot();
    let mut invalid = opts();
    invalid.symbol = "missing".into();
    invalid.client_order_id = Some("2".into());
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![opts(), invalid],
        },
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(done(&actions, 2).unwrap().is_err());
    for id in ["0", "01", "9223372036854775808", "1.0", "-1"] {
        let mut o = opts();
        o.client_order_id = Some(id.into());
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 3,
                orders: vec![o],
            },
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(done(&actions, 3).unwrap().is_err());
    }
}
#[test]
fn both_brackets_are_distinct_reducing_opposite_side_market_triggers() {
    let mut e = boot();
    let mut o = opts();
    o.client_order_id = None;
    o.stop_loss = Some(90.);
    o.take_profit = Some(120.);
    o.sl_trigger_type = Some(StopTriggerType::MarkPrice);
    o.tp_trigger_type = Some(StopTriggerType::IndexPrice);
    let entry = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 4,
                orders: vec![o],
            },
        ),
        "create_order",
    );
    let entry_body = body(&entry).o;
    let entry_id = entry_body.m.co;
    assert!(entry_body.m.t.is_none());
    let sl = http(
        reply(&mut e, &entry, 200, r#"{"r":{"oi":"0x00"}}"#),
        "create_order",
    );
    let child = body(&sl).o;
    let sl_id = child.m.co.clone();
    assert!(child.im && child.ro);
    assert!(!child.l[0].ib);
    assert!(child.l[0].lp.is_none());
    let t = child.m.t.unwrap();
    assert_eq!(
        (t.tt.as_str(), t.t.tb.as_str(), t.t.tp.as_str()),
        ("STOP_LOSS", "MARK", "90")
    );
    assert!(!t.t.cp && !t.t.is);
    assert_eq!(
        e.store().order(&sl_id).unwrap().parent_id.as_deref(),
        Some(entry_id.as_str())
    );
    let tp = http(
        reply(&mut e, &sl, 200, r#"{"r":{"oi":"0x00"}}"#),
        "create_order",
    );
    let child = body(&tp).o;
    let tp_id = child.m.co.clone();
    assert_ne!(sl_id, tp_id);
    let t = child.m.t.unwrap();
    assert_eq!(
        (t.tt.as_str(), t.t.tb.as_str(), t.t.tp.as_str()),
        ("TAKE_PROFIT", "INDEX", "120")
    );
    assert!(child.ro);
    assert!(!child.l[0].ib);
    let actions = reply(&mut e, &tp, 200, r#"{"r":{"oi":"0x00"}}"#);
    let ids: Vec<String> =
        serde_json::from_str(done(&actions, 4).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(ids.len(), 3);
}
#[test]
fn bracket_failure_reports_partial_progress_and_never_sends_next_child() {
    let mut e = boot();
    let mut o = opts();
    o.client_order_id = None;
    o.stop_loss = Some(90.);
    o.take_profit = Some(120.);
    let entry = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 5,
                orders: vec![o],
            },
        ),
        "create_order",
    );
    let entry_id = body(&entry).o.m.co;
    let child = http(
        reply(&mut e, &entry, 200, r#"{"r":{"oi":"0x00"}}"#),
        "create_order",
    );
    let actions = reply(&mut e, &child, 400, r#"{"c":2001,"m":"rejected"}"#);
    let error = done(&actions, 5).unwrap().as_ref().unwrap_err();
    assert!(error.contains(&format!("already created GRVT order IDs: {entry_id}")));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
    );
}
#[test]
fn uncertain_write_is_not_retried_and_exposes_reconciliation_identity() {
    let mut e = boot();
    let r = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 6,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request: r.id,
            error: "connection reset",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        done(&actions, 6)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("9223372036854775807")
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
    );
}
#[test]
fn cancellation_preflights_all_identities_and_symbol_filter_is_native() {
    let mut e = boot();
    seed(&mut e);
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 7,
            ids: vec!["42".into(), "outsider".into()],
        },
    );
    assert!(done(&actions, 7).unwrap().is_err());
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let r = http(
        command(
            &mut e,
            Command::CancelSymbolOrders {
                command: 8,
                symbol: "BTC_USDT_PERP".into(),
            },
        ),
        "cancel_all_orders",
    );
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"sa":"9007199254740993","k":"PERPETUAL","b":"BTC","q":"USDT"}"#)
    );
    let actions = reply(&mut e, &r, 200, r#"{"r":{"a":false}}"#);
    assert!(done(&actions, 8).unwrap().is_err());
    assert_eq!(e.store().order("42").unwrap().status, OrderStatus::Open);
}
#[test]
fn amendment_waits_for_final_cancellation_and_preserves_post_only() {
    let mut e = boot();
    seed(&mut e);
    let get = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: 9,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
    assert_eq!(
        cancel.body.as_deref(),
        Some(r#"{"sa":"9007199254740993","oi":"0x1234"}"#)
    );
    let verify = http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order");
    let replacement = http(reply(&mut e, &verify, 200, CANCELED), "create_order");
    let o = body(&replacement).o;
    assert_ne!(o.m.co, "42");
    assert_eq!(o.l[0].lp.as_deref(), Some("110"));
    assert_eq!(o.l[0].s, "0.8");
    assert!(replacement.body.as_deref().unwrap().contains("\"po\":true"));
}
#[test]
fn cancellation_ack_without_final_state_or_with_new_fill_aborts_amendment() {
    let advanced = CANCELED.replace("[\"0.2\"]", "[\"0.3\"]");
    for confirmation in [OPEN, advanced.as_str()] {
        let mut e = boot();
        seed(&mut e);
        let get = http(
            command(
                &mut e,
                Command::AmendOrder {
                    command: 10,
                    id: "42".into(),
                    price: Some(110.),
                    amount: None,
                },
            ),
            "/order",
        );
        let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
        let verify = http(
            reply(&mut e, &cancel, 200, r#"{"r":{"a":"true"}}"#),
            "/order",
        );
        let actions = reply(&mut e, &verify, 200, confirmation);
        assert!(done(&actions, 10).unwrap().is_err());
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
        );
    }
}
#[test]
fn leverage_changes_only_on_explicit_success() {
    let mut e = boot();
    let initial = e.store().setting("BTC_USDT_PERP").unwrap().leverage;
    let r = http(
        command(
            &mut e,
            Command::SetLeverage {
                command: 11,
                symbol: "BTC_USDT_PERP".into(),
                leverage: 20.,
            },
        ),
        "set_initial_leverage",
    );
    let actions = reply(&mut e, &r, 200, r#"{"s":false}"#);
    assert!(done(&actions, 11).unwrap().is_err());
    assert_eq!(
        e.store().setting("BTC_USDT_PERP").unwrap().leverage,
        initial
    );
    let r = http(
        command(
            &mut e,
            Command::SetLeverage {
                command: 12,
                symbol: "BTC_USDT_PERP".into(),
                leverage: 20.,
            },
        ),
        "set_initial_leverage",
    );
    let actions = reply(&mut e, &r, 200, r#"{"s":true}"#);
    assert!(done(&actions, 12).unwrap().is_ok());
    assert_eq!(e.store().setting("BTC_USDT_PERP").unwrap().leverage, 20.);
}

#[test]
fn explicit_session_rejection_resends_identical_signed_order_once() {
    let mut e = boot();
    let original = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 13,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
    let login = http(
        reply(
            &mut e,
            &original,
            401,
            r#"{"c":1000,"m":"session expired"}"#,
        ),
        "auth/api_key/login",
    );
    e.handle(NOW+4.,Event::HttpResponse{request:login.id,response:Response{status:200,headers:"Set-Cookie: gravity=renewed-session; Path=/\nX-Grvt-Account-Id: renewed-account",body:r#"{"sub_account_id":"9007199254740993"}"#}});
    let resent = http(e.drain_actions().collect(), "create_order");
    assert_eq!(resent.body, original.body);
    assert!(
        resent
            .headers
            .iter()
            .any(|(k, v)| *k == "X-Grvt-Account-Id" && v == "renewed-account")
    );
    let actions = reply(
        &mut e,
        &resent,
        401,
        r#"{"c":1000,"m":"session expired again"}"#,
    );
    assert!(done(&actions, 13).unwrap().is_err());
    assert!(!actions.iter().any(|a|matches!(a,Action::Http(r) if r.url.ends_with("auth/api_key/login")||r.url.ends_with("create_order"))));
}

#[test]
fn amendment_rejects_incomplete_native_semantics_before_cancelling() {
    for source in [
        OPEN.replace("\"ti\":\"GOOD_TILL_TIME\",", ""),
        OPEN.replace("\"ti\":\"GOOD_TILL_TIME\"", "\"ti\":\"ALL_OR_NONE\""),
    ] {
        let mut e = boot();
        seed(&mut e);
        let get = http(
            command(
                &mut e,
                Command::AmendOrder {
                    command: 14,
                    id: "42".into(),
                    price: Some(110.),
                    amount: None,
                },
            ),
            "/order",
        );
        let actions = reply(&mut e, &get, 200, &source);
        assert!(done(&actions, 14).unwrap().is_err());
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("cancel_order")))
        );
    }
}

#[test]
fn caller_ids_reject_conditional_and_bracket_shapes_before_any_write() {
    let mut e = boot();
    for shape in 0..4 {
        let mut o = opts();
        match shape {
            0 => o.stop_loss = Some(90.),
            1 => o.take_profit = Some(120.),
            2 => o.kind = OrderType::StopLoss,
            _ => o.trigger_price = Some(90.),
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 15,
                orders: vec![o],
            },
        );
        assert!(done(&actions, 15).unwrap().is_err());
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn amendment_refuses_changed_creation_lifecycle_and_unassigned_native_identity() {
    let mut e = boot();
    seed(&mut e);
    let get = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: 16,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    let newer = OPEN.replace("1699999999000000000", "1700000000000000000");
    let actions = reply(&mut e, &get, 200, &newer);
    assert!(done(&actions, 16).unwrap().is_err());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("cancel_order")))
    );
    let mut o = opts();
    o.client_order_id = Some("77".into());
    let create = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 17,
                orders: vec![o],
            },
        ),
        "create_order",
    );
    reply(&mut e, &create, 200, r#"{"r":{"oi":"0x00"}}"#);
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 18,
            id: "77".into(),
            price: Some(110.),
            amount: None,
        },
    );
    assert!(done(&actions, 18).unwrap().is_err());
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn fetch_leverage_is_a_real_native_read_and_missing_or_malformed_rows_fail() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fetched {
        buy_leverage: f64,
        sell_leverage: f64,
    }
    for (response, expected) in [
        (r#"{"r":[{"i":"BTC_USDT_Perp","l":"25"}]}"#, true),
        (r#"{"r":[]}"#, false),
        (r#"{"r":[{"i":"BTC_USDT_Perp","l":"bad"}]}"#, false),
    ] {
        let mut e = boot();
        let r = http(
            command(
                &mut e,
                Command::FetchLeverage {
                    command: 19,
                    symbol: "BTC_USDT_PERP".into(),
                },
            ),
            "get_all_initial_leverage",
        );
        let actions = reply(&mut e, &r, 200, response);
        let result = done(&actions, 19).unwrap();
        assert_eq!(result.is_ok(), expected);
        if let Ok(value) = result {
            let got: Fetched = serde_json::from_str(value).unwrap();
            assert_eq!((got.buy_leverage, got.sell_leverage), (25., 25.));
        }
    }
}

#[test]
fn old_leverage_read_cannot_overwrite_a_successful_mutation() {
    let mut e = boot();
    let read = http(
        command(
            &mut e,
            Command::FetchLeverage {
                command: 20,
                symbol: "BTC_USDT_PERP".into(),
            },
        ),
        "get_all_initial_leverage",
    );
    let write = http(
        command(
            &mut e,
            Command::SetLeverage {
                command: 21,
                symbol: "BTC_USDT_PERP".into(),
                leverage: 20.,
            },
        ),
        "set_initial_leverage",
    );
    let actions = reply(&mut e, &write, 200, r#"{"s":true}"#);
    assert!(done(&actions, 21).unwrap().is_ok());
    let actions = reply(
        &mut e,
        &read,
        200,
        r#"{"r":[{"i":"BTC_USDT_Perp","l":"10"}]}"#,
    );
    assert!(done(&actions, 20).unwrap().is_err());
    assert_eq!(e.store().setting("BTC_USDT_PERP").unwrap().leverage, 20.);
}
