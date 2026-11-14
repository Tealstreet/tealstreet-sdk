//! Released writes cannot prove refusal from an ambiguous status or native rate code.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, Method, Notice, Outcome, Response};
use support::*;

fn command(case: &str) -> Command {
    let id = if case.contains("protective") {
        "protect__position-1__sl"
    } else {
        "one"
    };
    match case {
        "create" | "protective create" => {
            let mut order: PlaceOrderOpts = serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500,"amount":0.01}"#,
            )
            .unwrap();
            if case == "protective create" {
                order.kind = OrderType::StopLoss;
                order.price = Some(100500.0);
            }
            Command::PlaceOrders {
                command: 7,
                orders: vec![order],
            }
        }
        "amend" | "protective amend" => Command::AmendOrder {
            command: 7,
            id: id.into(),
            price: Some(100100.0),
            amount: None,
        },
        "cancel" | "protective cancel" => Command::CancelOrders {
            command: 7,
            ids: vec![id.into()],
        },
        "leverage" => Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        },
        "margin" => Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDT".into(),
            isolated: false,
        },
        _ => unreachable!(),
    }
}

fn probe(status: u16, code: i64, unknown: bool) {
    for case in [
        "create",
        "protective create",
        "amend",
        "protective amend",
        "cancel",
        "protective cancel",
        "leverage",
        "margin",
    ] {
        let (mut e, _) = startup();
        e.handle(100.0, Event::Command(command(case)));
        let writes = requests(&mut e);
        assert_eq!(
            writes.len(),
            1,
            "{case}: must physically dispatch one write"
        );
        let write = &writes[0];
        assert_ne!(write.method, Method::Get, "{case}: {write:?}");
        let body = format!(r#"{{"code":{code},"msg":"scripted native response"}}"#);
        e.handle(
            110.0,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status,
                    headers: "",
                    body: &body,
                },
            },
        );
        let mut actions = safe_cex_engine::test_support::prepared_actions(&mut e, 110.0);
        let restores_kept_leg =
            case == "protective cancel" && (200..300).contains(&status) && !unknown;
        if restores_kept_leg {
            let index = actions.iter().position(|action| matches!(action, Action::Http(request) if request.method == Method::Post))
                .expect("TS cancellation continues after a resolved native refusal");
            let Action::Http(kept) = actions.remove(index) else {
                unreachable!()
            };
            assert!(kept.url.ends_with("/tpsl/place_order"));
            e.handle(
                110.0,
                Event::HttpResponse {
                    request: kept.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"code":0,"data":{"orderId":"kept"}}"#,
                    },
                },
            );
            actions.extend(safe_cex_engine::test_support::prepared_actions(
                &mut e, 110.0,
            ));
        }
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(
            failures.len(),
            1,
            "{case}/{status}/{code}: one failure completion"
        );
        let error = failures[0];
        assert_eq!(
            error.outcome,
            if unknown {
                Some(Outcome::Unknown)
            } else if restores_kept_leg {
                None
            } else {
                Some(Outcome::Rejected)
            },
            "{case}/{status}/{code}: {error:?}"
        );
        if unknown {
            let message = error.message.to_ascii_lowercase();
            assert!(
                message.contains("unknown") && message.contains("check open orders"),
                "{case}/{status}/{code}: {error:?}"
            );
            assert!(
                !message.contains("refused") && !message.contains("no stop"),
                "{error:?}"
            );
        }
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))),
            "{actions:?}"
        );
        // After the expected legacy put-back, completion cannot hide a resend
        // or another placement behind a recovery/backoff timer.
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        let mut later = actions;
        for (timer, at) in timers {
            e.handle(at, Event::TimerFired { timer });
            later.extend(safe_cex_engine::test_support::prepared_actions(&mut e, at));
        }
        assert!(
            !later
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
            "{case}/{status}/{code}: no resend or put-back: {later:?}"
        );
    }
}

#[test]
fn ambiguous_http_statuses_seal_all_dispatched_writes() {
    for status in [429, 408, 500, 503] {
        // Signature errors must not mint a resend grant on ambiguous HTTP statuses.
        for code in [10005, 10006, 10007, 20003] {
            probe(status, code, true);
        }
    }
}

#[test]
fn native_rates_at_http_200_are_unknown_for_all_dispatched_writes() {
    for code in [10005, 10006] {
        probe(200, code, true);
    }
}

#[test]
fn documented_definitive_refusal_stays_rejected() {
    probe(200, 20003, false);
}

fn replacement_stale_refusal(status: u16, code: i64, unknown: bool) {
    let (mut e, _) = startup();
    e.handle(
        100.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let modify = requests(&mut e).remove(0);
    assert!(modify.url.ends_with("/trade/modify_order"));
    reply(
        &mut e,
        modify.id,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = requests(&mut e).remove(0);
    assert!(cancel.url.ends_with("/trade/cancel_orders"));
    reply(
        &mut e,
        cancel.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("/trade/place_order"));
    assert_eq!(replacement.method, Method::Post);
    assert!(
        replacement
            .body
            .as_deref()
            .unwrap()
            .contains(r#""qty":"3""#)
    );
    e.handle(
        110.0,
        Event::HttpResponse {
            request: replacement.id,
            response: Response {
                status,
                headers: "",
                body: &format!(r#"{{"code":{code},"msg":"replacement response"}}"#),
            },
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 110.0);
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "{actions:?}");
    let error = failures[0];
    assert_eq!(
        error.outcome,
        if unknown {
            Some(Outcome::Unknown)
        } else {
            None
        },
        "{error:?}"
    );
    if unknown {
        let text = error.message.lines().next().unwrap().to_ascii_lowercase();
        assert!(
            text.contains("unknown") && text.contains("check open orders"),
            "{error:?}"
        );
        assert!(
            !text.contains("does not support this operation"),
            "stale refusal: {error:?}"
        );
        assert_eq!(error.protection, None);
    }
    assert_eq!(
        e.store().order("one").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
    );
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    let mut later = actions;
    for (timer, at) in timers {
        e.handle(at, Event::TimerFired { timer });
        later.extend(safe_cex_engine::test_support::prepared_actions(&mut e, at));
    }
    assert!(
        !later
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
        "no retry or put-back: {later:?}"
    );
    assert!(
        !later
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))),
        "{later:?}"
    );
}

#[test]
fn stale_refusal_before_replacement_10005_is_replaced_by_unknown_guidance() {
    replacement_stale_refusal(200, 10005, true);
}
#[test]
fn stale_refusal_before_replacement_10006_is_replaced_by_unknown_guidance() {
    replacement_stale_refusal(200, 10006, true);
}
#[test]
fn stale_refusal_before_replacement_http_429_already_has_unknown_guidance() {
    replacement_stale_refusal(429, 10005, true);
}
#[test]
fn stale_refusal_definitive_replacement_response_stays_known_partial() {
    replacement_stale_refusal(200, 20003, false);
}

fn batch_failure_rows(code: i64, refused_prefix: bool) {
    let native_message = if code == 10001 {
        "network error"
    } else {
        "throttled"
    };
    let (mut e, _) = startup();
    let mut cmd = command("create");
    if let Command::PlaceOrders { orders, .. } = &mut cmd {
        orders.resize(if refused_prefix { 7 } else { 2 }, orders[0].clone());
    }
    e.handle(100.0, Event::Command(cmd));
    let mut writes = requests(&mut e);
    assert_eq!(writes.len(), 1);
    let mut now = 110.0;
    if refused_prefix {
        let body = serde_json::json!({"code": 0, "data": {
            "successList": [],
            "failureList": vec![serde_json::json!({
                "errorCode": 20003, "errorMsg": "earlier refused chunk",
            }); 5],
        }})
        .to_string();
        e.handle(
            now,
            Event::HttpResponse {
                request: writes[0].id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        let (timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("next chunk cooldown");
        e.handle(at, Event::TimerFired { timer });
        writes = requests(&mut e);
        writes.retain(|r| r.method != Method::Get);
        assert_eq!(writes.len(), 1, "one later chunk");
        now = at + 1.0;
    }
    let write = &writes[0];
    assert!(write.url.ends_with("batch_order"));
    assert_ne!(write.method, Method::Get);
    let body = serde_json::json!({"code": 0, "data": {
        "successList": [],
        "failureList": [
            {"errorCode": code, "errorMsg": native_message},
            {"errorCode": code, "errorMsg": native_message},
        ],
    }})
    .to_string();
    e.handle(
        now,
        Event::HttpResponse {
            request: write.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    assert_batch_unknown(&mut e, now, code, !refused_prefix);
}

fn assert_batch_unknown(e: &mut TestEngine, now: f64, code: i64, check_native: bool) {
    let actions = safe_cex_engine::test_support::prepared_actions(e, now);
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "{actions:?}");
    let error = failures[0];
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    if check_native {
        assert_eq!(
            error.kind,
            Some(if code == 10001 {
                safe_cex_engine::ErrorKind::Network
            } else {
                safe_cex_engine::ErrorKind::RateLimited
            })
        );
        assert_eq!(error.venue_code, Some(code.to_string()));
        assert_eq!(
            error.venue_message.as_deref(),
            Some(if code == 10001 {
                "network error"
            } else {
                "throttled"
            })
        );
    }
    let text = error.legacy_text().to_ascii_lowercase();
    assert!(
        text.contains("unknown") && text.contains("check open orders"),
        "{error:?}"
    );
    let headline = error.message.lines().next().unwrap().to_ascii_lowercase();
    assert!(
        !headline.contains("refused") && !headline.contains("no stop"),
        "{error:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
    );
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    let mut later = actions;
    for (timer, at) in timers {
        e.handle(at, Event::TimerFired { timer });
        later.extend(safe_cex_engine::test_support::prepared_actions(e, at));
    }
    assert!(
        !later
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get)),
        "no resend or put-back: {later:?}"
    );
}

#[test]
fn native_10005_batch_rows_present_unknown_guidance() {
    batch_failure_rows(10005, false);
}

#[test]
fn native_10006_batch_rows_present_unknown_guidance() {
    batch_failure_rows(10006, false);
}

#[test]
fn native_10005_batch_rows_replace_an_earlier_chunks_refusal() {
    batch_failure_rows(10005, true);
}

#[test]
fn native_10006_batch_rows_replace_an_earlier_chunks_refusal() {
    batch_failure_rows(10006, true);
}

#[test]
fn native_10001_batch_rows_replace_an_earlier_chunks_refusal() {
    batch_failure_rows(10001, true);
}

#[test]
fn native_10001_batch_rows_present_unknown_guidance() {
    batch_failure_rows(10001, false);
}

fn cancel_batch_failure_rows(code: i64) {
    let (mut e, _) = startup();
    e.handle(100.0, Event::Command(command("cancel")));
    let writes = requests(&mut e);
    assert_eq!(writes.len(), 1);
    let write = &writes[0];
    assert!(write.url.ends_with("cancel_orders"));
    assert_ne!(write.method, Method::Get);
    let body = serde_json::json!({"code": 0, "data": {
        "successList": [],
        "failureList": [{"orderId": "one", "errorCode": code, "errorMsg": "throttled"}],
    }})
    .to_string();
    e.handle(
        110.0,
        Event::HttpResponse {
            request: write.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    assert_batch_unknown(&mut e, 110.0, code, false);
}

#[test]
fn native_10005_cancel_rows_present_unknown_guidance() {
    cancel_batch_failure_rows(10005);
}

#[test]
fn native_10006_cancel_rows_present_unknown_guidance() {
    cancel_batch_failure_rows(10006);
}

#[test]
fn native_10001_cancel_rows_present_unknown_guidance() {
    cancel_batch_failure_rows(10001);
}

#[test]
fn native_10001_cancel_rows_with_refused_siblings_present_unknown_guidance() {
    let (mut e, socket) = startup();
    for id in ["two", "three"] {
        event(
            &mut e,
            socket,
            &serde_json::json!({"ch":"order", "data":{
                "event":"CREATE", "symbol":"BTCUSDT", "orderId":id,
                "side":"Buy", "type":"LIMIT", "qty":"1", "price":"90", "dealAmount":"0",
            }})
            .to_string(),
        );
        assert!(e.store().order(id).is_some());
    }
    e.handle(
        100.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["two".into(), "three".into()],
        }),
    );
    let writes = requests(&mut e);
    assert_eq!(writes.len(), 1);
    assert!(writes[0].url.ends_with("cancel_orders"));
    e.handle(
        110.0,
        Event::HttpResponse {
            request: writes[0].id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":{"successList":[],"failureList":[{"orderId":"two","errorCode":20003,"errorMsg":"earlier refused row"},{"orderId":"three","errorCode":10001,"errorMsg":"network error"}]}}"#,
            },
        },
    );
    assert_batch_unknown(&mut e, 110.0, 10001, true);
}
