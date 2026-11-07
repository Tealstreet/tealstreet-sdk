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
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 110.0);
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "{case}/{status}/{code}: {actions:?}");
        let error = failures[0];
        assert_eq!(
            error.outcome,
            Some(if unknown {
                Outcome::Unknown
            } else {
                Outcome::Rejected
            }),
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
        // Drive every timer produced by this completion, including recovery/backoff,
        // so a delayed resend or put-back cannot hide behind the terminal notice.
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
