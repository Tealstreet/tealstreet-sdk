mod support;
use safe_cex_engine::model::{Interval, MarginType};
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, Command, CredentialSlot, Event, HttpRequest, Notice,
    Response, SocketId, TimerId,
};
use support::*;
const SETTINGS: &str = r#"{"code":0,"data":{"leverage":50,"marginMode":"CROSS"}}"#;
fn ready(secondary: bool) -> (TestEngine, SocketId) {
    let e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    let (mut e, socket) = startup_with_engine(e);
    let mut markets = e.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.symbol = "ETHUSDT".into();
    eth.id = "ETH/USDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    e.store_mut().set_markets(markets);
    e.store_mut().update_setting("ETHUSDT", |s| {
        s.leverage = 10.0;
        s.margin_type = Some(MarginType::Cross)
    });
    (e, socket)
}
fn settings(actions: &[Action]) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("get_leverage_margin_mode") => Some(r.clone()),
            _ => None,
        })
        .expect("actual signed settings read")
}
fn begin(e: &mut TestEngine) -> HttpRequest {
    e.handle(
        100.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "ETHUSDT".into(),
            interval: Interval::M1,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    settings(&actions)
}
fn notify(e: &mut TestEngine, socket: SocketId, at: f64, symbol: &str) {
    let text = format!(
        r#"{{"ch":"position","data":{{"event":"UPDATE","symbol":"{symbol}","positionId":"ws-position","side":"SELL"}}}}"#
    );
    e.handle(
        at,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    e.drain_actions().for_each(drop);
}
fn answer(e: &mut TestEngine, r: &HttpRequest, at: f64, body: &str) -> Vec<Action> {
    e.handle(
        at,
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
fn auth(actions: &[Action]) -> Vec<(CredentialSlot, AuthOutcome)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::AccountAuth {
                credential,
                scope: AccountReadScope::Settings,
                outcome,
            }) => Some((*credential, *outcome)),
            _ => None,
        })
        .collect()
}
fn timer_between(actions: &[Action], lo: f64, hi: f64) -> (TimerId, f64) {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= lo && *at_ms <= hi => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("actual settings retry deadline")
}
fn fire(e: &mut TestEngine, timer: TimerId, at: f64) -> Vec<Action> {
    e.handle(at, Event::TimerFired { timer });
    e.drain_actions().collect()
}
#[test]
fn current_flat_settings_and_scoped_auth_advance_across_same_or_other_symbol_notifications() {
    for secondary in [false, true] {
        for symbol in ["BTCUSDT", "ETHUSDT"] {
            let (mut e, socket) = ready(secondary);
            let r = begin(&mut e);
            for at in [120.0, 140.0, 180.0] {
                notify(&mut e, socket, at, symbol);
            }
            let actions = answer(&mut e, &r, 200.0, SETTINGS);
            let s = e.store().setting("ETHUSDT").unwrap();
            assert_eq!(
                (s.leverage, s.margin_type),
                (50.0, Some(MarginType::Cross)),
                "flat metadata must not starve on {symbol}"
            );
            assert_eq!(
                auth(&actions),
                vec![(
                    if secondary {
                        CredentialSlot::Secondary
                    } else {
                        CredentialSlot::Primary
                    },
                    AuthOutcome::Accepted
                )]
            );
            assert!(
                auth(&answer(&mut e, &r, 201.0, SETTINGS)).is_empty(),
                "duplicate callback cannot emit evidence"
            );
        }
    }
}
#[test]
fn current_settings_refusal_recovers_with_real_backoff_despite_position_notifications() {
    let (mut e, socket) = ready(true);
    let r = begin(&mut e);
    notify(&mut e, socket, 150.0, "BTCUSDT");
    let failed = answer(&mut e, &r, 200.0, r#"{"code":10004}"#);
    assert_eq!(
        auth(&failed),
        vec![(CredentialSlot::Secondary, AuthOutcome::Rejected)]
    );
    assert!(failed.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::REQUEST_FAILED)));
    let (timer, at) = timer_between(&failed, 950.0, 1200.0);
    let current = fire(&mut e, timer, at);
    let r = settings(&current);
    notify(&mut e, socket, at + 1.0, "BTCUSDT");
    let recovered = answer(&mut e, &r, at + 2.0, SETTINGS);
    assert_eq!(
        auth(&recovered),
        vec![(CredentialSlot::Secondary, AuthOutcome::Accepted)]
    );
    assert_eq!(e.store().setting("ETHUSDT").unwrap().leverage, 50.0);
}
#[test]
fn failed_settings_attempts_still_increase_backoff_during_position_notifications() {
    for transport in [false, true] {
        let (mut e, socket) = ready(false);
        let r = begin(&mut e);
        notify(&mut e, socket, 150.0, "BTCUSDT");
        let failed = if transport {
            e.handle(
                200.0,
                Event::HttpFailed {
                    request: r.id,
                    error: "settings network unavailable",
                },
            );
            e.drain_actions().collect()
        } else {
            answer(&mut e, &r, 200.0, r#"{"code":0,"data":{"leverage":0}}"#)
        };
        assert!(auth(&failed).is_empty());
        assert!(failed.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::REQUEST_FAILED)),"overlap must not hide real failure");
        let (timer, at) = timer_between(&failed, 950.0, 1200.0);
        let current = fire(&mut e, timer, at);
        let r = settings(&current);
        notify(&mut e, socket, at + 1.0, "ETHUSDT");
        let again = answer(&mut e, &r, at + 2.0, r#"{"code":0,"data":{}}"#);
        let (timer, due) = timer_between(&again, at + 1502.0, at + 2002.0);
        let current = fire(&mut e, timer, due);
        let r = settings(&current);
        notify(&mut e, socket, due + 1.0, "BTCUSDT");
        answer(&mut e, &r, due + 2.0, SETTINGS);
        assert_eq!(e.store().setting("ETHUSDT").unwrap().leverage, 50.0);
    }
}

#[test]
fn accepted_settings_writes_keep_the_fence_while_unproved_writes_leave_the_read_eligible() {
    for kind in ["leverage", "margin", "mode"] {
        for result in ["accepted", "refused", "malformed", "transport"] {
            let (mut e, socket) = ready(false);
            e.store_mut().set_positions(vec![]);
            let old = begin(&mut e);
            let command = match kind {
                "leverage" => Command::SetLeverage {
                    command: 55,
                    symbol: "ETHUSDT".into(),
                    leverage: 30.0,
                    side: None,
                },
                "margin" => Command::SetMarginType {
                    command: 55,
                    symbol: "ETHUSDT".into(),
                    isolated: true,
                },
                _ => Command::SetPositionMode {
                    command: 55,
                    hedged: false,
                    symbol: None,
                },
            };
            e.handle(150.0, Event::Command(command));
            let actions: Vec<_> = e.drain_actions().collect();
            let write = actions
                .iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.method == safe_cex_engine::Method::Post => Some(r.clone()),
                    _ => None,
                })
                .expect("actual settings write");
            let complete = if result == "transport" {
                e.handle(
                    200.0,
                    Event::HttpFailed {
                        request: write.id,
                        error: "lost settings ACK",
                    },
                );
                e.drain_actions().collect()
            } else {
                answer(
                    &mut e,
                    &write,
                    200.0,
                    match result {
                        "accepted" => r#"{"code":0,"data":{}}"#,
                        "refused" => r#"{"code":20001,"msg":"refused"}"#,
                        _ => r#"{"code":0,"data":["#,
                    },
                )
            };
            assert!(complete.iter().any(|a| matches!(
                a,
                Action::Notice(
                    Notice::CommandDone { command: 55, .. }
                        | Notice::CommandFailed { command: 55, .. }
                )
            )));
            notify(&mut e, socket, 250.0, "BTCUSDT");
            let actions = answer(&mut e, &old, 300.0, SETTINGS);
            if result == "accepted" {
                let expected = match kind {
                    "leverage" => (30.0, Some(MarginType::Cross)),
                    "margin" => (10.0, Some(MarginType::Isolated)),
                    _ => (10.0, Some(MarginType::Cross)),
                };
                let actual = e.store().setting("ETHUSDT").unwrap();
                assert_eq!(
                    (actual.leverage, actual.margin_type),
                    expected,
                    "accepted {kind} must fence old metadata"
                );
                assert!(auth(&actions).is_empty());
                let (timer, at) = timer_between(&actions, 1300.0, 1300.0);
                let reads = fire(&mut e, timer, at);
                let fresh = settings(&reads);
                notify(&mut e, socket, at + 1.0, "BTCUSDT");
                let body = format!(
                    r#"{{"code":0,"data":{{"leverage":{},"marginMode":"{}"}}}}"#,
                    expected.0,
                    if kind == "margin" {
                        "ISOLATION"
                    } else {
                        "CROSS"
                    }
                );
                let actions = answer(&mut e, &fresh, at + 2.0, &body);
                assert_eq!(
                    auth(&actions),
                    vec![(CredentialSlot::Primary, AuthOutcome::Accepted)]
                );
                let actual = e.store().setting("ETHUSDT").unwrap();
                assert_eq!((actual.leverage, actual.margin_type), expected);
            } else {
                assert_eq!(
                    e.store().setting("ETHUSDT").unwrap().leverage,
                    50.0,
                    "unproved {kind} cannot fence current read"
                );
                assert_eq!(
                    auth(&actions),
                    vec![(CredentialSlot::Primary, AuthOutcome::Accepted)]
                );
            }
        }
    }
}
