use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_grvt::{Grvt, Options};
use serde::Deserialize;
use std::collections::VecDeque;
const NOW: f64 = 1_700_000_000_000.;
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
/// Releases managed work (the session login) through the immediate fixture.
fn drain(e: &mut Engine<Grvt>) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(e, NOW + 3.).into_iter()
}
fn boot() -> Engine<Grvt> {
    boot_with_socket().0
}
fn boot_with_socket() -> (Engine<Grvt>, SocketId) {
    boot_with_catalog(CATALOG)
}
const AUTHORIZED: &str = r#"{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}"#;
fn boot_with_catalog(catalog: &str) -> (Engine<Grvt>, SocketId) {
    let (e, socket, _) = boot_with_builders(catalog, Some(AUTHORIZED));
    (e, socket)
}
/// `None` leaves the startup builder check unanswered and returns it.
fn boot_with_builders(
    catalog: &str,
    builders: Option<&str>,
) -> (Engine<Grvt>, SocketId, Option<HttpRequest>) {
    let mut private_socket = None;
    let mut held = None;
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        key: Some("grvt-test-key".into()),
        secret: Some(format!("0x{}", "01".repeat(32))),
        trading_account_id: Some("9007199254740993".into()),
        session_proxy: true,
        ..Options::default()
    }));
    e.start(NOW);
    let mut pending: VecDeque<_> = drain(&mut e)
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
            ("", catalog)
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
            let Some(builders) = builders else {
                held = Some(request);
                continue;
            };
            ("", builders)
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
        for action in drain(&mut e) {
            match action {
                Action::Http(request) => pending.push_back(request),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => private_socket = Some(socket),
                _ => {}
            }
        }
    }
    assert!(pending.is_empty());
    (e, private_socket.expect("GRVT private socket"), held)
}
fn authenticated() -> (Engine<Grvt>, SocketId) {
    let (mut engine, socket) = boot_with_socket();
    engine.handle(NOW + 1., Event::WsOpened { socket });
    engine.handle(
        NOW + 1.,
        Event::WsMessage {
            socket,
            text: r#"{"type":"auth_success"}"#,
        },
    );
    let mut pending: VecDeque<_> = drain(&mut engine)
        .filter_map(|action| {
            if let Action::Http(request) = action {
                Some(request)
            } else {
                None
            }
        })
        .collect();
    while let Some(request) = pending.pop_front() {
        let body = if request.url.ends_with("open_orders") {
            OPEN_ORDERS
        } else if request.url.ends_with("aggregated_account_summary") {
            r#"{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"9007199254740993"}]}}"#
        } else {
            r#"{"r":[]}"#
        };
        engine.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        pending.extend(drain(&mut engine).filter_map(|action| {
            if let Action::Http(request) = action {
                Some(request)
            } else {
                None
            }
        }));
    }
    (engine, socket)
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
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
        close: None,
        stp_mode: None,
    }
}
fn command(e: &mut Engine<Grvt>, c: Command) -> Vec<Action> {
    e.handle(NOW + 2., Event::Command(c));
    drain(e).collect()
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
    drain(e).collect()
}
fn done(actions: &[Action], id: u32) -> Option<Result<String, String>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command, result }) => {
            (*command == id).then(|| Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command, error }) => {
            (*command == id).then(|| Err(error.legacy_text()))
        }
        _ => None,
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
const OPEN_ORDERS: &str = r#"{"r":[{"sa":"9007199254740993","oi":"0x1234","im":false,"ti":"GOOD_TILL_TIME","po":true,"ro":false,"l":[{"i":"BTC_USDT_Perp","s":"1","lp":"100","ib":true}],"m":{"co":"42","ct":"1699999999000000000"},"s1":{"s":"OPEN","ts":["0.2"],"ut":"1700000000000000000"}}]}"#;
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
fn selected_caller_and_generated_identities_precede_their_signed_requests() {
    for generated in [false, true] {
        let mut e = boot();
        let mut order = opts();
        if generated {
            order.client_order_id = None;
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 93,
                orders: vec![order],
            },
        );
        let (progress_at, receipts) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                Action::Notice(Notice::CommandProgress {
                    command: 93,
                    receipts,
                }) => Some((index, receipts)),
                _ => None,
            })
            .expect("native identity before dispatch");
        let (write_at, request) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                Action::Http(request) if request.url.ends_with("create_order") => {
                    Some((index, request))
                }
                _ => None,
            })
            .unwrap();
        assert!(progress_at < write_at);
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].symbol, "BTC_USDT_PERP");
        assert_eq!(receipts[0].kind, OrderIdentityKind::ClientOrderId);
        assert_eq!(receipts[0].identity, body(request).o.m.co);
        if !generated {
            assert_eq!(receipts[0].identity, "9223372036854775807");
        }
    }
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
fn retired_product_identities_reject_every_symbol_write_without_touching_perpetuals() {
    for (kind, instrument) in [
        ("FUTURE", "BTC_USDT_30DEC26_Future"),
        ("CALL", "BTC_USDT_30DEC26_100000_Call"),
        ("PUT", "BTC_USDT_30DEC26_100000_Put"),
    ] {
        let catalog = format!(
            r#"{},{{"i":"{instrument}","ih":"0x030502","b":"BTC","q":"USDT","k":"{kind}","ts":"0.1","ms":"0.001","bd":9,"qd":6}}]}}"#,
            CATALOG.strip_suffix("]}").unwrap()
        );
        let (mut e, _) = boot_with_catalog(&catalog);
        assert!(e.store().market_by_id(instrument).is_none());
        for symbol in [instrument.to_owned(), instrument.to_ascii_uppercase()] {
            let mut unsupported = opts();
            unsupported.symbol = symbol.clone();
            unsupported.client_order_id = Some("43".into());
            for request in [
                Command::PlaceOrders {
                    command: 35,
                    orders: vec![opts(), unsupported],
                },
                Command::CancelSymbolOrders {
                    command: 35,
                    symbol: symbol.clone(),
                },
                Command::SetLeverage {
                    command: 35,
                    symbol,
                    leverage: 5.,
                    side: None,
                },
            ] {
                let actions = command(&mut e, request);
                assert!(done(&actions, 35).unwrap().is_err());
                assert!(
                    !actions
                        .iter()
                        .any(|action| matches!(action, Action::Http(_)))
                );
                assert_eq!(e.store().order("42").unwrap().status, OrderStatus::Open);
            }
        }
        for request in [
            Command::CancelOrders {
                command: 36,
                ids: vec!["42".into(), instrument.into()],
            },
            Command::AmendOrder {
                command: 36,
                id: instrument.into(),
                price: Some(110.),
                amount: None,
            },
        ] {
            let actions = command(&mut e, request);
            assert!(done(&actions, 36).unwrap().is_err());
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_)))
            );
        }
        let request = http(
            command(
                &mut e,
                Command::PlaceOrders {
                    command: 37,
                    orders: vec![opts()],
                },
            ),
            "create_order",
        );
        assert_eq!(body(&request).o.l[0].i, "BTC_USDT_Perp");
    }
}
#[test]
fn bybit_specific_options_reject_the_entire_batch_before_any_write() {
    for bbo in [true, false] {
        let mut e = boot();
        let mut invalid = opts();
        invalid.client_order_id = Some("2".into());
        if bbo {
            invalid.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.,
            });
        } else {
            invalid.bybit_slippage_tolerance_ticks = Some(1.);
        }
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 31,
                orders: vec![opts(), invalid],
            },
        );
        assert!(
            done(&actions, 31)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Bybit-specific")
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}
#[test]
fn unsupported_self_trade_prevention_rejects_the_entire_batch_before_any_write() {
    for mode in [
        StpMode::ExpireTaker,
        StpMode::ExpireMaker,
        StpMode::ExpireBoth,
    ] {
        let mut e = boot();
        let mut unsupported = opts();
        unsupported.client_order_id = Some("2".into());
        unsupported.stp_mode = Some(mode);
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 32,
                orders: vec![opts(), unsupported],
            },
        );
        assert!(
            done(&actions, 32)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("self-trade prevention")
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}

#[test]
fn foreign_commands_fail_without_bypassing_known_order_cancellation_identity() {
    let mut e = boot();
    for request in [
        Command::CancelOrdersWithSymbols {
            command: 33,
            orders: vec![CancelOrderRef {
                id: "42".into(),
                symbol: "BTC_USDT_PERP".into(),
            }],
        },
        Command::FetchStpMode { command: 33 },
        Command::SetStpMode {
            command: 33,
            mode: StpMode::ExpireBoth,
        },
    ] {
        let actions = command(&mut e, request);
        assert!(
            done(&actions, 33)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Unsupported GRVT command")
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
    let request = http(
        command(
            &mut e,
            Command::CancelOrders {
                command: 34,
                ids: vec!["42".into()],
            },
        ),
        "cancel_order",
    );
    assert_eq!(
        request.body.as_deref(),
        Some(r#"{"sa":"9007199254740993","oi":"0x1234"}"#)
    );
    let actions = reply(&mut e, &request, 200, r#"{"r":{"a":true}}"#);
    assert!(done(&actions, 34).unwrap().is_ok());
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
    assert_eq!(
        failure(&actions).outcome,
        None,
        "the entry was already created"
    );
    let error = done(&actions, 5).unwrap().unwrap_err();
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
    let actions: Vec<_> = drain(&mut e).collect();
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
fn cancellation_preflights_all_identities_and_symbol_snapshot_is_native() {
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
        "open_orders",
    );
    assert_eq!(r.body.as_deref(), Some(r#"{"sa":"9007199254740993"}"#));
    let r = http(reply(&mut e, &r, 200, OPEN_ORDERS), "cancel_order");
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"sa":"9007199254740993","oi":"0x1234"}"#)
    );
    let actions = reply(&mut e, &r, 200, r#"{"r":{"a":false}}"#);
    assert!(done(&actions, 8).unwrap().is_err());
    assert_eq!(e.store().order("42").unwrap().status, OrderStatus::Open);
}
#[test]
fn symbol_cancellation_ack_requires_reconciliation_without_inventing_terminal_state() {
    for body in [r#"{"r":{"a":true}}"#, r#"{"result":{"ack":"true"}}"#] {
        let mut e = boot();
        seed(&mut e);
        let r = http(
            command(
                &mut e,
                Command::CancelSymbolOrders {
                    command: 81,
                    symbol: "BTC_USDT_PERP".into(),
                },
            ),
            "open_orders",
        );
        let r = http(reply(&mut e, &r, 200, OPEN_ORDERS), "cancel_order");
        let actions = reply(&mut e, &r, 200, body);
        assert!(done(&actions, 81).unwrap().is_ok());
        assert_eq!(
            e.store().order("42").unwrap().status,
            OrderStatus::Canceling
        );
        assert!(actions.iter().any(
            |action| matches!(action,Action::Http(request) if request.url.ends_with("open_orders"))
        ));
    }
}
#[test]
fn global_cancellation_reaches_native_orders_without_a_local_snapshot() {
    for body in [r#"{"r":{"a":true}}"#, r#"{"r":{"a":false}}"#] {
        let mut e = boot();
        e.store_mut().set_orders(vec![]);
        let request = http(
            command(
                &mut e,
                Command::CancelSymbolOrders {
                    command: 82,
                    symbol: String::new(),
                },
            ),
            "cancel_all_orders",
        );
        assert_eq!(
            request.body.as_deref(),
            Some(r#"{"sa":"9007199254740993"}"#)
        );
        let actions = reply(&mut e, &request, 200, body);
        assert_eq!(done(&actions, 82).unwrap().is_ok(), body.contains("true"));
        assert!(e.store().orders().is_empty());
    }
}
#[test]
fn global_cancellation_tracks_all_known_symbols_until_reconciliation() {
    for ack in [true, false] {
        let mut e = boot();
        seed(&mut e);
        let mut other = e.store().order("42").unwrap().clone();
        other.id = "43".into();
        other.client_order_id = Some("43".into());
        other.symbol = "ETH_USDT_PERP".into();
        e.store_mut().upsert_order(other);
        let request = http(
            command(
                &mut e,
                Command::CancelSymbolOrders {
                    command: 83,
                    symbol: String::new(),
                },
            ),
            "cancel_all_orders",
        );
        for id in ["42", "43"] {
            assert_eq!(e.store().order(id).unwrap().status, OrderStatus::Canceling);
        }
        let actions = reply(
            &mut e,
            &request,
            200,
            if ack {
                r#"{"r":{"a":true}}"#
            } else {
                r#"{"r":{"a":false}}"#
            },
        );
        assert_eq!(done(&actions, 83).unwrap().is_ok(), ack);
        for id in ["42", "43"] {
            assert_eq!(
                e.store().order(id).unwrap().status,
                if ack {
                    OrderStatus::Canceling
                } else {
                    OrderStatus::Open
                }
            );
        }
    }
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
                side: None,
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
                side: None,
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
    let resent = http(drain(&mut e).collect(), "create_order");
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
            let got: Fetched = serde_json::from_str(&value).unwrap();
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
                side: None,
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

fn order_frame(status: &str, filled: &str, updated_ns: &str) -> String {
    OPEN.replace("{\"r\":", "{\"s\":\"v1.order\",\"f\":")
        .replace("\"OPEN\"", &format!("\"{status}\""))
        .replace("[\"0.2\"]", &format!("[\"{filled}\"]"))
        .replace("1700000000000000000", updated_ns)
}
fn execution_frame() -> &'static str {
    r#"{"s":"v1.fill","f":{"i":"BTC_USDT_Perp","ti":"9223372036854775806","co":"42","oi":"0x1234","ib":true,"s":"0.1","p":"110"}}"#
}
#[test]
fn terminal_execution_survives_store_removal_through_every_amendment_boundary() {
    for stage in 0..3 {
        let (mut e, socket) = authenticated();
        seed(&mut e);
        let get = http(
            command(
                &mut e,
                Command::AmendOrder {
                    command: 90,
                    id: "42".into(),
                    price: Some(110.),
                    amount: None,
                },
            ),
            "/order",
        );
        let pending = if stage == 0 {
            get
        } else {
            let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
            if stage == 1 {
                cancel
            } else {
                http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order")
            }
        };
        let filled = order_frame("FILLED", "1", "1700000000010000000");
        e.handle(
            NOW + 4.,
            Event::WsMessage {
                socket,
                text: &filled,
            },
        );
        assert!(
            e.store().order("42").is_none(),
            "terminal original is removed from Store"
        );
        drain(&mut e);
        let response = match stage {
            0 => OPEN,
            1 => r#"{"r":{"a":true}}"#,
            _ => CANCELED,
        };
        let actions = reply(&mut e, &pending, 200, response);
        assert!(done(&actions, 90).unwrap().is_err(), "stage {stage}");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order"))),
            "stage {stage}"
        );
        if stage == 0 {
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("cancel_order")))
            );
        }
    }
}
#[test]
fn fill_topic_only_execution_aborts_pending_inspection_and_final_proof() {
    for after_cancel in [false, true] {
        let (mut e, socket) = authenticated();
        seed(&mut e);
        let get = http(
            command(
                &mut e,
                Command::AmendOrder {
                    command: 91,
                    id: "42".into(),
                    price: Some(110.),
                    amount: None,
                },
            ),
            "/order",
        );
        let pending = if after_cancel {
            let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
            http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order")
        } else {
            get
        };
        e.handle(
            NOW + 4.,
            Event::WsMessage {
                socket,
                text: execution_frame(),
            },
        );
        assert_eq!(
            e.store().order("42").unwrap().filled,
            0.2,
            "fill feed does not update the order snapshot"
        );
        drain(&mut e);
        let actions = reply(
            &mut e,
            &pending,
            200,
            if after_cancel { CANCELED } else { OPEN },
        );
        assert!(
            done(&actions, 91)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("account execution")
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
        );
    }
}
#[test]
fn authoritative_unchanged_fill_cancellation_still_allows_replacement() {
    let (mut e, socket) = authenticated();
    seed(&mut e);
    let get = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: 92,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
    let verify = http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order");
    let canceled = order_frame("CANCELLED", "0.2", "1700000000001000000");
    e.handle(
        NOW + 4.,
        Event::WsMessage {
            socket,
            text: &canceled,
        },
    );
    assert!(e.store().order("42").is_none());
    drain(&mut e);
    let replacement = http(reply(&mut e, &verify, 200, CANCELED), "create_order");
    assert_eq!(body(&replacement).o.l[0].s, "0.8");
}
#[test]
fn unchanged_fill_but_older_inspection_or_cancellation_proof_is_rejected() {
    for final_proof in [false, true] {
        let (mut e, socket) = authenticated();
        seed(&mut e);
        let get = http(
            command(
                &mut e,
                Command::AmendOrder {
                    command: 93,
                    id: "42".into(),
                    price: Some(110.),
                    amount: None,
                },
            ),
            "/order",
        );
        let pending = if final_proof {
            let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
            http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order")
        } else {
            get
        };
        let newer = order_frame(
            if final_proof { "CANCELLED" } else { "OPEN" },
            "0.2",
            "1700000000010000000",
        );
        e.handle(
            NOW + 4.,
            Event::WsMessage {
                socket,
                text: &newer,
            },
        );
        drain(&mut e);
        let actions = reply(
            &mut e,
            &pending,
            200,
            if final_proof { CANCELED } else { OPEN },
        );
        assert!(
            done(&actions, 93)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("predates")
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
        );
    }
}

#[test]
fn cancellation_proof_cannot_predate_the_authoritative_inspection() {
    let mut e = boot();
    seed(&mut e);
    let get = http(
        command(
            &mut e,
            Command::AmendOrder {
                command: 94,
                id: "42".into(),
                price: Some(110.),
                amount: None,
            },
        ),
        "/order",
    );
    let inspected = OPEN.replace("1700000000000000000", "1700000000005000000");
    let cancel = http(reply(&mut e, &get, 200, &inspected), "cancel_order");
    let verify = http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order");
    let actions = reply(&mut e, &verify, 200, CANCELED);
    assert!(
        done(&actions, 94)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("predates")
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
    );
}
#[test]
fn authentication_resend_rechecks_execution_progress_for_pending_verify_and_place() {
    for pending_place in [false, true] {
        let (mut e, socket) = authenticated();
        seed(&mut e);
        let get = http(
            command(
                &mut e,
                Command::AmendOrder {
                    command: 95,
                    id: "42".into(),
                    price: Some(110.),
                    amount: None,
                },
            ),
            "/order",
        );
        let cancel = http(reply(&mut e, &get, 200, OPEN), "cancel_order");
        let verify = http(reply(&mut e, &cancel, 200, r#"{"r":{"a":true}}"#), "/order");
        let pending = if pending_place {
            http(reply(&mut e, &verify, 200, CANCELED), "create_order")
        } else {
            verify
        };
        // Start an independent account read while the amendment HTTP is in flight.
        let unrelated = order_frame("OPEN", "0", "1700000000002000000")
            .replace("\"co\":\"42\"", "\"co\":\"88\"")
            .replace("0x1234", "0x8888");
        e.handle(
            NOW + 4.,
            Event::WsMessage {
                socket,
                text: &unrelated,
            },
        );
        let timer = drain(&mut e)
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if at_ms == NOW + 1004. => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(NOW + 1004., Event::TimerFired { timer });
        let snapshot = http(drain(&mut e).collect(), "open_orders");
        let login = http(
            reply(&mut e, &pending, 401, r#"{"c":1000,"m":"expired"}"#),
            "auth/api_key/login",
        );
        let filled = OPEN
            .replace("[\"0.2\"]", "[\"0.3\"]")
            .replace("1700000000000000000", "1700000000010000000");
        let snapshot_body = filled
            .replace("\"r\":{", "\"r\":[{")
            .replacen("}}}", "}}]}", 1);
        reply(&mut e, &snapshot, 200, &snapshot_body);
        e.handle(NOW+1005.,Event::HttpResponse{request:login.id,response:Response{status:200,headers:"Set-Cookie: gravity=renewed-session; Path=/\nX-Grvt-Account-Id: renewed-account",body:r#"{"sub_account_id":"9007199254740993"}"#}});
        let actions: Vec<_> = drain(&mut e).collect();
        assert!(
            done(&actions, 95)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("execution")
        );
        assert!(!actions.iter().any(|a|matches!(a,Action::Http(r) if r.url.ends_with("create_order")||r.url.ends_with("/order"))));
    }
}
const PROXY_REJECTED: &str =
    r#"{"code":1008,"message":"Request IP is not whitelisted","status":403}"#;
fn timer_at(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing timer at {at}"))
}
/// Fails the held check and both TS-budget retries (2s, then 4s); returns the final actions.
fn exhaust_builder_check(e: &mut Engine<Grvt>, first: HttpRequest) -> Vec<Action> {
    assert_eq!(first.timeout_ms, Some(10_000));
    let mut check = first;
    for (attempt, delay) in [(1, 2_000.), (2, 4_000.)] {
        let actions = reply(e, &check, 403, PROXY_REJECTED);
        assert!(
            !actions.iter().any(
                |a| matches!(a, Action::Http(r) if r.url.ends_with("get_authorized_builders"))
            ),
            "attempt {attempt} retried immediately instead of backing off"
        );
        let timer = timer_at(&actions, NOW + 3. + delay);
        e.handle(NOW + 3. + delay, Event::TimerFired { timer });
        check = http(drain(e).collect(), "get_authorized_builders");
    }
    reply(e, &check, 403, PROXY_REJECTED)
}
#[test]
fn failed_builder_check_retries_within_budget_then_names_the_cause_and_rechecks() {
    let (mut e, _, held) = boot_with_builders(CATALOG, None);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 40,
            orders: vec![opts()],
        },
    );
    assert!(
        done(&actions, 40).is_none(),
        "order rejected while the check is in flight"
    );
    let actions = exhaust_builder_check(&mut e, held.unwrap());
    let error = done(&actions, 40).unwrap().unwrap_err();
    assert!(error.contains("could not be checked"), "{error}");
    assert!(error.contains("code 1008"), "{error}");
    assert!(error.contains("Request IP is not whitelisted"), "{error}");
    let mut next = opts();
    next.client_order_id = Some("43".into());
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 41,
            orders: vec![next],
        },
    );
    assert!(
        done(&actions, 41).is_none(),
        "failed check was cached for the session"
    );
    let check = http(actions, "get_authorized_builders");
    let create = http(reply(&mut e, &check, 200, AUTHORIZED), "create_order");
    assert_eq!(body(&create).o.bf, "0.03");
}
#[test]
fn cancel_queued_behind_a_held_place_runs_in_fifo_order_after_the_check() {
    let (mut e, _, held) = boot_with_builders(CATALOG, None);
    seed(&mut e);
    let mut actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 48,
            orders: vec![opts()],
        },
    );
    actions.extend(command(
        &mut e,
        Command::CancelOrders {
            command: 49,
            ids: vec!["42".into()],
        },
    ));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("create_order") || r.url.ends_with("cancel_order"))),
        "write overtook the held place"
    );
    let actions = reply(&mut e, &held.unwrap(), 200, AUTHORIZED);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("cancel_order")))
    );
    let create = http(actions, "create_order");
    let actions = reply(&mut e, &create, 200, r#"{"r":{"oi":"0x00"}}"#);
    assert!(done(&actions, 48).unwrap().is_ok());
    http(actions, "cancel_order");
}
#[test]
fn matching_builder_row_without_fee_cap_is_authorized() {
    let (mut e, _, _) = boot_with_builders(
        CATALOG,
        Some(r#"{"r":[{"ba":"0xee8f3559f298fffa79b8253437969d15b5dccff3"}]}"#),
    );
    http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 44,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
}
#[test]
fn builder_fee_cap_below_required_still_rejects() {
    let (mut e, _, _) = boot_with_builders(
        CATALOG,
        Some(r#"{"r":[{"ba":"0xee8f3559f298fffa79b8253437969d15b5dccff3","mfr":"0.01"}]}"#),
    );
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 45,
            orders: vec![opts()],
        },
    );
    let error = done(&actions, 45).unwrap().unwrap_err();
    assert!(error.contains("cap is below 300"), "{error}");
}
#[test]
fn definitive_rejection_surfaces_grvt_message_without_reconcile_advice() {
    let mut e = boot();
    let r = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 46,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
    let actions = reply(
        &mut e,
        &r,
        400,
        r#"{"code":2001,"message":"Insufficient margin","status":400}"#,
    );
    let error = done(&actions, 46).unwrap().unwrap_err();
    assert!(error.contains("code 2001"), "{error}");
    assert!(error.contains("Insufficient margin"), "{error}");
    assert!(!error.contains("reconcile"), "{error}");
}
#[test]
fn gateway_failure_on_create_keeps_reconcile_advice() {
    let mut e = boot();
    let r = http(
        command(
            &mut e,
            Command::PlaceOrders {
                command: 47,
                orders: vec![opts()],
            },
        ),
        "create_order",
    );
    let actions = reply(&mut e, &r, 502, "<html>bad gateway</html>");
    let error = done(&actions, 47).unwrap().unwrap_err();
    assert!(
        error.contains("reconcile GRVT client order ID 9223372036854775807"),
        "{error}"
    );
}
#[test]
fn unreadable_builder_fee_cap_on_matching_row_is_authorized() {
    for cap in [r#""""#, r#""n/a""#, "null", "{}"] {
        let (mut e, _, _) = boot_with_builders(
            CATALOG,
            Some(&format!(
                r#"{{"r":[{{"ba":"0xee8f3559f298fffa79b8253437969d15b5dccff3","mfr":{cap}}}]}}"#
            )),
        );
        http(
            command(
                &mut e,
                Command::PlaceOrders {
                    command: 50,
                    orders: vec![opts()],
                },
            ),
            "create_order",
        );
    }
}
#[test]
fn null_or_non_string_rejection_message_keeps_the_grvt_code() {
    for message in ["null", "42", r#"{"x":1}"#] {
        let mut e = boot();
        let r = http(
            command(
                &mut e,
                Command::PlaceOrders {
                    command: 51,
                    orders: vec![opts()],
                },
            ),
            "create_order",
        );
        let actions = reply(
            &mut e,
            &r,
            400,
            &format!(r#"{{"code":2001,"message":{message},"status":400}}"#),
        );
        let error = done(&actions, 51).unwrap().unwrap_err();
        assert!(error.contains("code 2001"), "{message}: {error}");
        assert!(!error.contains("reconcile"), "{message}: {error}");
    }
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

#[test]
fn write_failures_carry_grvt_outcomes_without_changing_their_text() {
    use safe_cex_engine::Outcome;
    for (status, body, outcome) in [
        (400, r#"{"c":2001,"m":"rejected"}"#, Some(Outcome::Rejected)),
        (502, "Bad Gateway", Some(Outcome::Unknown)),
    ] {
        let mut e = boot();
        let create = http(
            command(
                &mut e,
                Command::PlaceOrders {
                    command: 5,
                    orders: vec![opts()],
                },
            ),
            "create_order",
        );
        let actions = reply(&mut e, &create, status, body);
        assert_eq!(failure(&actions).outcome, outcome, "{status} {body}");
    }
}

fn structured_failure(actions: &[Action], id: u32) -> serde_json::Value {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(serde_json::to_value(error).unwrap())
            }
            _ => None,
        })
        .expect("expected structured GRVT failure")
}
#[test]
fn typed_builder_approval_failure_survives_placement_and_amend_inspection() {
    for (builders, expected) in [
        (
            r#"{"r":[]}"#,
            serde_json::json!({"code":"GRVT_BUILDER_UNAUTHORIZED","reason":"unauthorized","requiredRaw":300}),
        ),
        (
            r#"{"r":[{"ba":"0xee8f3559f298fffa79b8253437969d15b5dccff3","mfr":"0.01"}]}"#,
            serde_json::json!({"code":"GRVT_BUILDER_UNAUTHORIZED","reason":"cap-too-low","requiredRaw":300,"authorizedRaw":100}),
        ),
    ] {
        for amend in [false, true] {
            let (mut e, _, _) = boot_with_builders(CATALOG, Some(builders));
            seed(&mut e);
            let mut actions = command(
                &mut e,
                if amend {
                    Command::AmendOrder {
                        command: 92,
                        id: "42".into(),
                        price: Some(110.),
                        amount: None,
                    }
                } else {
                    Command::PlaceOrders {
                        command: 92,
                        orders: vec![opts()],
                    }
                },
            );
            if amend {
                let read = http(actions, "/order");
                actions = reply(&mut e, &read, 200, OPEN);
            }
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(r) if r.url.ends_with("create_order") || r.url.ends_with("cancel_order"))));
            let error = structured_failure(&actions, 92);
            assert_eq!(error["builderAuthorization"], expected, "amend={amend}");
            assert_eq!(error["outcome"], if amend { "unknown" } else { "notSent" });
            assert_eq!(e.store().order("42").unwrap().status, OrderStatus::Open);
        }
    }
}
#[test]
fn earlier_grvt_order_validation_does_not_become_an_approval_error() {
    let (mut e, _, _) = boot_with_builders(CATALOG, Some(r#"{"r":[]}"#));
    let mut order = opts();
    order.kind = OrderType::StopLoss;
    order.trigger_price = Some(90.);
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 93,
            orders: vec![order],
        },
    );
    let error = structured_failure(&actions, 93);
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("plain Limit/Market")
    );
    assert!(error.get("builderAuthorization").is_none());
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn failed_grvt_builder_checks_never_claim_missing_approval() {
    for (status, body) in [(403, PROXY_REJECTED), (200, r#"{"r":[null]}"#)] {
        let (mut e, _, held) = boot_with_builders(CATALOG, None);
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 94,
                orders: vec![opts()],
            },
        );
        assert!(done(&actions, 94).is_none());
        let mut check = held.unwrap();
        for delay in [2_000., 4_000.] {
            let actions = reply(&mut e, &check, status, body);
            assert!(done(&actions, 94).is_none());
            let timer = timer_at(&actions, NOW + 3. + delay);
            e.handle(NOW + 3. + delay, Event::TimerFired { timer });
            check = http(drain(&mut e).collect(), "get_authorized_builders");
        }
        let actions = reply(&mut e, &check, status, body);
        let error = structured_failure(&actions, 94);
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("could not be checked")
        );
        assert!(error.get("builderAuthorization").is_none());
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("create_order")))
        );
    }
}

#[test]
fn public_rest_book_headers_and_rejections_cannot_replace_private_session() {
    let mut e = boot();
    for (owner, body) in [
        (70, r#"{"r":{"b":[],"a":[]}}"#),
        (71, r#"{"c":1000,"m":"invalid session"}"#),
    ] {
        let public = http(
            command(
                &mut e,
                Command::FetchOrderBook {
                    command: owner,
                    symbol: "BTC_USDT_PERP".into(),
                    limit: None,
                },
            ),
            "/lite/v1/book",
        );
        assert_eq!(
            public.headers,
            vec![("Content-Type", "application/json".into())]
        );
        e.handle(NOW + 3.0, Event::HttpResponse { request: public.id, response: Response {
            status: 200,
            headers: "Set-Cookie: gravity=foreign-session; Path=/\nX-Grvt-Account-Id: foreign-account",
            body,
        }});
        let actions: Vec<_> = drain(&mut e).collect();
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::AccountAuth { .. })
        )));
        assert!(actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. }) if *command == owner)));
        let private = http(
            command(
                &mut e,
                Command::FetchAccount {
                    command: owner + 10,
                },
            ),
            "/lite/v1/account_summary",
        );
        assert!(
            private
                .headers
                .iter()
                .any(|(name, value)| *name == "X-Grvt-Account-Id" && value == "test-account")
        );
        assert!(
            private
                .headers
                .iter()
                .any(|(name, value)| *name == "Cookie" && value == "rm=true; gravity=test-session")
        );
        let actions = reply(&mut e, &private, 200, r#"{"r":{"te":"1000"}}"#);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command,result }) if *command==owner+10 && serde_json::from_str::<serde_json::Value>(result).unwrap()["userId"]=="test-account")));
    }
}
