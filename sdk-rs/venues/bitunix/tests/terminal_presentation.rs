//! Terminal wording follows the journal's final outcome across all write decoders.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Outcome, Response};
use support::*;

fn respond(e: &mut TestEngine, write: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: write.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn begin(path: &str) -> (TestEngine, HttpRequest, f64, usize) {
    let (mut e, socket) = startup();
    for id in ["two", "three"] {
        event(
            &mut e,
            socket,
            &serde_json::json!({"ch":"order","data":{
                "event":"CREATE","symbol":"BTCUSDT","orderId":id,"side":"Buy",
                "type":"LIMIT","qty":"1","price":"90","dealAmount":"0",
            }})
            .to_string(),
        );
    }
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500,"amount":0.01}"#,
    )
    .unwrap();
    let command = match path {
        "place" => Command::PlaceOrders {
            command: 7,
            orders: vec![order],
        },
        "batch place" | "batch place after refusal" => Command::PlaceOrders {
            command: 7,
            orders: vec![order; if path.ends_with("refusal") { 7 } else { 2 }],
        },
        "cancel" => Command::CancelOrders {
            command: 7,
            ids: vec!["two".into()],
        },
        "batch cancel" => Command::CancelOrders {
            command: 7,
            ids: vec!["two".into(), "three".into()],
        },
        "amend" => Command::AmendOrder {
            command: 7,
            id: "two".into(),
            price: Some(89.0),
            amount: None,
        },
        "cancel after amend refusal" | "place after amend refusal" => Command::AmendOrder {
            command: 7,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        },
        _ => unreachable!(),
    };
    e.handle(100.0, Event::Command(command));
    let mut writes = requests(&mut e);
    assert_eq!(writes.len(), 1, "{path}: {writes:?}");
    let mut write = writes.remove(0);
    let mut now = 110.0;
    if path == "batch place after refusal" {
        assert!(write.url.ends_with("batch_order"));
        let refused = serde_json::json!({"code":0,"data":{
            "successList":[],"failureList":vec![serde_json::json!({
                "errorCode":20003,"errorMsg":"earlier refused chunk",
            });5],
        }})
        .to_string();
        respond(&mut e, &write, now, &refused);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
        )));
        let (timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("later chunk cooldown");
        e.handle(at, Event::TimerFired { timer });
        let mut writes = requests(&mut e);
        writes.retain(|r| r.method != Method::Get);
        assert_eq!(writes.len(), 1, "later chunk must dispatch");
        write = writes.remove(0);
        now = at + 1.0;
    } else if path.ends_with("amend refusal") {
        assert!(write.url.ends_with("modify_order"));
        respond(
            &mut e,
            &write,
            now,
            r#"{"code":20003,"msg":"The order does not support this operation"}"#,
        );
        write = requests(&mut e).remove(0);
        assert!(write.url.ends_with("cancel_orders"));
        if path.starts_with("place") {
            respond(
                &mut e,
                &write,
                now,
                r#"{"code":0,"data":{"successList":[{"orderId":"one"},{"orderId":"one__tp"},{"orderId":"one__sl"}],"failureList":[]}}"#,
            );
            write = requests(&mut e).remove(0);
            assert_eq!(write.method, Method::Post);
            assert!(write.url.ends_with("place_order"));
        }
    }
    assert_ne!(write.method, Method::Get, "{path}");
    let rows = match path {
        "batch place" | "batch place after refusal" | "batch cancel" => 2,
        "cancel" => 1,
        "cancel after amend refusal" => 3,
        _ => 0,
    };
    (e, write, now, rows)
}

#[test]
fn final_outcome_controls_every_write_paths_terminal_presentation() {
    let paths = [
        "place",
        "batch place",
        "cancel",
        "batch cancel",
        "amend",
        "batch place after refusal",
        "cancel after amend refusal",
        "place after amend refusal",
    ];
    let answers = [
        "refused",
        "unknown",
        "empty message",
        "rate 10005",
        "rate 10006",
        "malformed data",
        "malformed envelope",
        "http rate",
        "http server",
        "lost response",
        "refused rows",
        "unknown rows",
        "rate rows",
        "mixed rows",
    ];
    let mut checked = 0;
    let mut missing_guidance = Vec::new();
    for path in paths {
        for answer in answers {
            let batch_rows = path.contains("batch") || path.starts_with("cancel");
            if answer.ends_with("rows") && !batch_rows {
                continue;
            }
            let (mut e, write, now, rows) = begin(path);
            if answer == "mixed rows" && rows < 2 {
                continue;
            }
            let body = match answer {
                "refused" => r#"{"code":20003,"msg":"definitive refusal"}"#.to_owned(),
                "unknown" => r#"{"code":10001,"msg":"network error"}"#.to_owned(),
                "empty message" => r#"{"code":99999,"msg":""}"#.to_owned(),
                "rate 10005" => r#"{"code":10005,"msg":"throttled"}"#.to_owned(),
                "rate 10006" => r#"{"code":10006,"msg":"throttled"}"#.to_owned(),
                "malformed data" => r#"{"code":0,"data":true}"#.to_owned(),
                "malformed envelope" => "{".to_owned(),
                "http rate" | "http server" | "lost response" => "{}".to_owned(),
                _ => {
                    let failures: Vec<_> = (0..rows)
                        .map(|i| {
                            serde_json::json!({
                                "orderId":format!("row-{i}"),
                                "errorCode":match answer {
                                    "refused rows" => 20003,
                                    "rate rows" => 10005,
                                    "mixed rows" if i == 0 => 20003,
                                    _ => 10001,
                                },
                                "errorMsg":"scripted row failure",
                            })
                        })
                        .collect();
                    serde_json::json!({"code":0,"data":{"successList":[],"failureList":failures}})
                        .to_string()
                }
            };
            match answer {
                "lost response" => e.handle(
                    now,
                    Event::HttpFailed {
                        request: write.id,
                        error: "response lost",
                    },
                ),
                "http rate" | "http server" => e.handle(
                    now,
                    Event::HttpResponse {
                        request: write.id,
                        response: Response {
                            status: if answer == "http rate" { 429 } else { 503 },
                            headers: "",
                            body: &body,
                        },
                    },
                ),
                _ => respond(&mut e, &write, now, &body),
            }
            let mut actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
            let continued_after_refusal =
                path == "cancel after amend refusal" && answer == "refused rows";
            if continued_after_refusal {
                // Master continues after resolved cancel-row refusals. Settle its
                // one trailing write before checking terminal publication;
                // Unknown cases must never enter this branch.
                assert_eq!(
                    actions
                        .iter()
                        .filter(|action| matches!(action,
                    Action::Http(request) if request.method != Method::Get))
                        .count(),
                    1
                );
                let index = actions
                    .iter()
                    .position(|action| {
                        matches!(action,
                    Action::Http(request) if request.method == Method::Post)
                    })
                    .expect("one approved trailing put-back");
                let Action::Http(kept) = actions.remove(index) else {
                    unreachable!()
                };
                assert!(kept.url.ends_with("place_order"));
                respond(
                    &mut e,
                    &kept,
                    now,
                    r#"{"code":0,"data":{"orderId":"kept"}}"#,
                );
                actions.extend(safe_cex_engine::test_support::prepared_actions(&mut e, now));
            }
            let errors: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1, "{path}/{answer}: {actions:?}");
            let error = errors[0];
            let expected = if answer == "refused" || answer == "refused rows" {
                if path == "place after amend refusal" || continued_after_refusal {
                    None
                } else if answer == "refused" && path.contains("batch place") {
                    Some(Outcome::Unknown)
                } else {
                    Some(Outcome::Rejected)
                }
            } else {
                Some(Outcome::Unknown)
            };
            assert_eq!(error.outcome, expected, "{path}/{answer}: {error:?}");
            let text = error.legacy_text().to_ascii_lowercase();
            if expected == Some(Outcome::Unknown) {
                if error.message.lines().next()
                    != Some("The outcome of this Bitunix write is unknown. Check Open Orders")
                {
                    missing_guidance.push(format!("{path}/{answer}: {error:?}"));
                }
                for action in &actions {
                    if let Action::Notice(Notice::CommandError {
                        command: 7,
                        message,
                    }) = action
                        && message.lines().next()
                            != Some(
                                "The outcome of this Bitunix write is unknown. Check Open Orders",
                            )
                    {
                        missing_guidance.push(format!("warning {path}/{answer}: {message}"));
                    }
                }
            } else {
                assert!(
                    !text.contains("outcome is unknown"),
                    "{path}/{answer}: {error:?}"
                );
            }
            let mut later = Vec::new();
            for action in &actions {
                if let Action::SetTimer { timer, at_ms } = action {
                    e.handle(*at_ms, Event::TimerFired { timer: *timer });
                    later.extend(safe_cex_engine::test_support::prepared_actions(
                        &mut e, *at_ms,
                    ));
                }
            }
            assert!(
                !actions
                    .iter()
                    .chain(&later)
                    .any(|a| matches!(a,Action::Http(r) if r.method != Method::Get)),
                "resend: {path}/{answer}"
            );
            assert!(
                !actions
                    .iter()
                    .chain(&later)
                    .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))),
                "{path}/{answer}"
            );
            assert!(
                !later
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. }))),
                "duplicate failure: {path}/{answer}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 99);
    assert!(
        missing_guidance.is_empty(),
        "{}",
        missing_guidance.join("\n")
    );
}
