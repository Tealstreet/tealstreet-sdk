//! Exercise genuine signed dispatch and journal settlement, including an accepted cancel prefix.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Outcome};
use safe_cex_venue_lighter::Lighter;
use serde_json::json;
use support::*;

fn take_http(e: &mut Engine<Lighter>, now: f64, path: &str) -> HttpRequest {
    let actions = safe_cex_engine::test_support::prepared_actions(e, now);
    let writes: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(writes.len(), 1, "missing {path}: {actions:?}");
    writes[0].clone()
}

fn probe(status: u16, code: u64, unknown: bool) {
    for case in [
        "create",
        "protective create",
        "amend cancel",
        "amend replacement",
        "protective amend",
        "cancel",
        "protective cancel",
        "leverage",
        "margin",
    ] {
        let mut opts = options();
        opts.testnet = true;
        let (mut e, feeds, _) = ready_feeds(opts, true);
        if case == "protective amend" || case == "protective cancel" {
            message(
                &mut e,
                feeds.private,
                NOW,
                &json!({"channel":"account_all_orders","orders":[{
                    "order_index":"42","client_order_index":"123","market_id":1,
                    "status":"open","type":2,"time_in_force":0,"is_ask":true,
                    "reduce_only":true,"initial_base_amount":"2","filled_base_amount":"0",
                    "remaining_base_amount":"2","trigger_price":"80","price":"79",
                    "order_expiry":NOW as u64 + 1_000_000,"trigger_status":"mark-price",
                    "parent_order_index":0,"parent_order_id":"0","to_trigger_order_id_0":"0",
                    "to_trigger_order_id_1":"0","to_cancel_order_id_0":"0","order_flags":0
                }]})
                .to_string(),
            );
            message(
                &mut e,
                feeds.public,
                NOW,
                r#"{"channel":"market_stats/all","market_stats":[{"market_id":1,"mark_price":"100"}]}"#,
            );
            safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        }
        let command = match case {
            "create" | "protective create" => {
                let order: PlaceOrderOpts = serde_json::from_str(if case == "create" {
                    r#"{"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}"#
                } else {
                    r#"{"symbol":"ETH","type":"stop_market","side":"sell","amount":0.2,"price":80}"#
                })
                .unwrap();
                Command::PlaceOrders {
                    command: 7,
                    orders: vec![order],
                }
            }
            "amend cancel" | "amend replacement" | "protective amend" => Command::AmendOrder {
                command: 7,
                id: "42".into(),
                price: Some(92.0),
                amount: None,
            },
            "cancel" | "protective cancel" => Command::CancelOrders {
                command: 7,
                ids: vec!["42".into()],
            },
            "leverage" => Command::SetLeverage {
                command: 7,
                symbol: "ETH".into(),
                leverage: 12.0,
                side: None,
            },
            "margin" => Command::SetMarginType {
                command: 7,
                symbol: "ETH".into(),
                isolated: true,
            },
            _ => unreachable!(),
        };
        e.handle(NOW, Event::Command(command));
        let nonce = take_http(&mut e, NOW, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let mut write = take_http(&mut e, NOW, "sendTx");
        let mut now = NOW;
        if case == "amend replacement" {
            reply(
                &mut e,
                &write,
                200,
                r#"{"code":200,"tx_hash":"accepted-cancel"}"#,
            );
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
            let timer = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms < NOW + 500.0 => {
                        Some((*timer, *at_ms))
                    }
                    _ => None,
                })
                .expect("replacement pacing timer");
            now = timer.1;
            e.handle(now, Event::TimerFired { timer: timer.0 });
            write = take_http(&mut e, now, "sendTx");
        }
        assert_eq!(write.method, Method::Post, "{case}: physical write");
        let body = format!(r#"{{"code":{code},"message":"scripted native response"}}"#);
        reply_at(&mut e, now, &write, status, &body);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "{case}/{status}/{code}: {actions:?}");
        let error = failures[0];
        // A definitive replacement refusal after accepted cancel is a known partial.
        let expected = if unknown {
            Some(Outcome::Unknown)
        } else if case == "amend replacement" {
            None
        } else {
            Some(Outcome::Rejected)
        };
        assert_eq!(error.outcome, expected, "{case}/{status}/{code}: {error:?}");
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
        for code in [23000, 23100, 21104, 21702] {
            probe(status, code, true);
        }
    }
}

#[test]
fn native_rates_at_http_200_are_unknown_for_all_dispatched_writes() {
    for code in [23000, 23100] {
        probe(200, code, true);
    }
}

#[test]
fn documented_definitive_refusal_stays_rejected() {
    probe(200, 21702, false);
}
