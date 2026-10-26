mod support;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Notice, RequestId, Response, SocketId, TimerId,
};
use support::*;
const FRESH: &str = r#"{"code":0,"data":[{"marginCoin":"USDT","available":"700","margin":"10","frozen":"2","positionMode":"HEDGE","crossUnrealizedPNL":"2","isolationUnrealizedPNL":"1","unrealizedPNL":"999"}]}"#;
fn notify(e: &mut TestEngine, socket: SocketId, at: f64) -> Vec<Action> {
    e.handle(
        at,
        Event::WsMessage {
            socket,
            text: r#"{"ch":"balance","data":{"available":"1"}}"#,
        },
    );
    e.drain_actions().collect()
}
fn timer_at(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing deadline{at}: {actions:?}"))
}
fn fire(e: &mut TestEngine, timer: TimerId, at: f64) -> Vec<Action> {
    e.handle(at, Event::TimerFired { timer });
    e.drain_actions().collect()
}
fn balance(actions: &[Action]) -> Option<HttpRequest> {
    actions.iter().find_map(|a| match a {
        Action::Http(r) if r.url.contains("/futures/account?marginCoin=USDT") => Some(r.clone()),
        _ => None,
    })
}
fn begin(e: &mut TestEngine, socket: SocketId) -> HttpRequest {
    let actions = notify(e, socket, 100.0);
    let actions = fire(e, timer_at(&actions, 1100.0), 1100.0);
    balance(&actions).expect("notification balance read")
}
fn answer(e: &mut TestEngine, id: RequestId, at: f64, body: &str) -> Vec<Action> {
    e.handle(
        at,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn first_retry(actions: &[Action], now: f64) -> (TimerId, f64) {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= now + 750.0 && *at_ms <= now + 1000.0 =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("actual first jittered balance retry")
}
fn balance_auth(actions: &[Action]) -> Vec<safe_cex_engine::AuthOutcome> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::AccountAuth {
                credential: safe_cex_engine::CredentialSlot::Primary,
                scope: safe_cex_engine::AccountReadScope::Balance,
                outcome,
            }) => Some(*outcome),
            _ => None,
        })
        .collect()
}
fn assert_fresh(e: &TestEngine) {
    let b = e.store().balance();
    assert_eq!((b.free, b.used, b.total, b.upnl), (700.0, 10.0, 712.0, 3.0));
}
#[test]
fn continuous_balance_notifications_keep_the_first_repair_deadline() {
    let (mut e, socket) = startup();
    let actions = notify(&mut e, socket, 100.0);
    let first = timer_at(&actions, 1100.0);
    for at in [200.0, 500.0, 1000.0] {
        let actions = notify(&mut e, socket, at);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::ClearTimer{timer}if *timer==first)),
            "notification postponed first repair at{at}"
        );
    }
    assert!(balance(&fire(&mut e, first, 1100.0)).is_some());
}
#[test]
fn current_balance_reply_advances_financial_state_during_continuous_notifications() {
    let (mut e, socket) = startup();
    let read = begin(&mut e, socket);
    for at in [1200.0, 1400.0, 1600.0, 1900.0] {
        notify(&mut e, socket, at);
    }
    let actions = answer(&mut e, read.id, 2000.0, FRESH);
    assert_eq!(
        balance_auth(&actions),
        vec![safe_cex_engine::AuthOutcome::Accepted],
        "WS-only overlap cannot suppress current Balance auth evidence"
    );
    assert_fresh(&e);
}
#[test]
fn notification_repair_cannot_supersede_a_slow_balance_read() {
    let (mut e, socket) = startup();
    let read = begin(&mut e, socket);
    let actions = notify(&mut e, socket, 1200.0);
    let repair = timer_at(&actions, 2200.0);
    let actions = fire(&mut e, repair, 2200.0);
    assert!(
        balance(&actions).is_none(),
        "an ordinary refresh must retain the in-flight serial"
    );
    answer(&mut e, read.id, 2500.0, FRESH);
    assert_fresh(&e);
}
#[test]
fn lost_read_replacement_and_late_callbacks_keep_the_new_balance_read_owned() {
    for late_failure in [false, true] {
        let (mut e, socket) = startup();
        let old = begin(&mut e, socket);
        let actions = notify(&mut e, socket, 10100.0);
        let actions = fire(&mut e, timer_at(&actions, 11100.0), 11100.0);
        let new = balance(&actions).expect("replacement at ten-second issue bound");
        assert_ne!(new.id, old.id);
        let mut actions = if late_failure {
            e.handle(
                11200.0,
                Event::HttpFailed {
                    request: old.id,
                    error: "late old read failure",
                },
            );
            e.drain_actions().collect()
        } else {
            answer(&mut e, old.id, 11200.0, FRESH)
        };
        assert_eq!(e.store().balance().free, 900.0, "old reply cannot publish");
        actions.extend(notify(&mut e, socket, 11300.0));
        let (timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms >= 12200.0 && *at_ms <= 12300.0 => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("coalesced notification repair");
        let actions = fire(&mut e, timer, at);
        assert!(
            balance(&actions).is_none(),
            "old callback must not release the replacement"
        );
        answer(&mut e, new.id, 13000.0, FRESH);
        assert_fresh(&e);
    }
}
#[test]
fn accepted_mode_ack_fences_old_balance_while_refused_or_unproved_writes_do_not() {
    for result in ["accepted", "refused", "malformed", "transport"] {
        let (mut e, socket) = startup();
        e.store_mut().set_positions(vec![]);
        let old = begin(&mut e, socket);
        e.handle(
            1200.0,
            Event::Command(Command::SetPositionMode {
                command: 91,
                hedged: false,
                symbol: None,
            }),
        );
        let write = requests(&mut e)
            .into_iter()
            .find(|r| r.method == safe_cex_engine::Method::Post)
            .expect("mode write");
        let actions = if result == "transport" {
            e.handle(
                1300.0,
                Event::HttpFailed {
                    request: write.id,
                    error: "lost mode ACK",
                },
            );
            e.drain_actions().collect()
        } else {
            answer(
                &mut e,
                write.id,
                1300.0,
                match result {
                    "accepted" => r#"{"code":0,"data":{}}"#,
                    "refused" => r#"{"code":20001,"msg":"refused"}"#,
                    _ => r#"{"code":0,"data":["#,
                },
            )
        };
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: 91, .. } | Notice::CommandFailed { command: 91, .. }
            )
        )));
        let actions = answer(&mut e, old.id, 1400.0, FRESH);
        if result == "accepted" {
            assert!(
                balance_auth(&actions).is_empty(),
                "pre-mode-ACK read cannot emit accepted credentials or reset recovery"
            );
            assert!(!e.store().hedged());
            assert_eq!(
                e.store().balance().free,
                900.0,
                "old mode read must be fenced"
            );
            let repair = timer_at(&actions, 2400.0);
            let actions = fire(&mut e, repair, 2400.0);
            let fresh = balance(&actions).expect("current mode repair");
            let fresh_actions =
                answer(&mut e, fresh.id, 2500.0, &FRESH.replace("HEDGE", "ONE_WAY"));
            assert_eq!(
                balance_auth(&fresh_actions),
                vec![safe_cex_engine::AuthOutcome::Accepted]
            );
            assert_fresh(&e);
            assert!(!e.store().hedged());
        } else {
            assert_fresh(&e);
            assert!(e.store().hedged(), "unproved write must not change mode");
        }
    }
}
#[test]
fn synchronous_balance_signing_failure_releases_before_the_normal_repair() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let unavailable = Arc::new(AtomicBool::new(false));
    let toggle = unavailable.clone();
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    });
    e.set_signing_entropy(Box::new(move |bytes| {
        if toggle.load(Ordering::SeqCst) {
            Err("controlled signing unavailable".into())
        } else {
            bytes.fill(7);
            Ok(())
        }
    }))
    .unwrap();
    let (mut e, socket) = startup_with_engine(e);
    unavailable.store(true, Ordering::SeqCst);
    let actions = notify(&mut e, socket, 100.0);
    let actions = fire(&mut e, timer_at(&actions, 1100.0), 1100.0);
    assert!(balance(&actions).is_none());
    let (retry, at) = first_retry(&actions, 1100.0);
    unavailable.store(false, Ordering::SeqCst);
    let actions = fire(&mut e, retry, at);
    let current =
        balance(&actions).expect("no fabricated ten-second in-flight wait after signing failure");
    answer(&mut e, current.id, 2300.0, FRESH);
    assert_fresh(&e);
}

#[test]
fn every_balance_issuer_anchors_the_lost_read_bound_at_actual_dispatch() {
    let (mut e, socket) = startup();
    let old = begin(&mut e, socket); // dispatched1100
    let actions = notify(&mut e, socket, 9600.0);
    let actions = fire(&mut e, timer_at(&actions, 10600.0), 10600.0);
    assert!(
        balance(&actions).is_none(),
        "9500ms old notification read cannot inherit the earlier tick/startup timestamp"
    );
    let actions = notify(&mut e, socket, 10700.0);
    let actions = fire(&mut e, timer_at(&actions, 11700.0), 11700.0);
    let current = balance(&actions).expect("replace only after own ten-second bound");
    assert_ne!(old.id, current.id);
    answer(&mut e, current.id, 11800.0, FRESH);
    assert_fresh(&e);
}
#[test]
fn private_reconnect_coalesces_against_a_current_balance_read() {
    let (mut e, socket) = startup();
    let old = begin(&mut e, socket);
    e.handle(1200.0, Event::WsClosed { socket, code: 1006 });
    let actions: Vec<_> = e.drain_actions().collect();
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= 1950.0 && *at_ms <= 2200.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("supervisor reconnect");
    let actions = fire(&mut e, timer, at);
    let replacement = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.contains("/private/") => Some(*socket),
            _ => None,
        })
        .expect("new private epoch");
    e.handle(
        at + 1.0,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        balance(&actions).is_none(),
        "reconnect refresh must retain slow current balance serial"
    );
    assert!(
        notify(&mut e, socket, at + 2.0).is_empty(),
        "retired socket has no authority"
    );
    notify(&mut e, replacement, at + 3.0);
    answer(&mut e, old.id, at + 4.0, FRESH);
    assert_fresh(&e);
}

#[test]
fn current_balance_refusal_and_recovery_survive_ws_overlap_without_bypassing_backoff() {
    let (mut e, socket) = startup();
    let read = begin(&mut e, socket);
    let overlap = notify(&mut e, socket, 1110.0);
    let notification_repair = timer_at(&overlap, 2110.0);
    let refused = answer(&mut e, read.id, 1500.0, r#"{"code":10004}"#);
    assert_eq!(
        balance_auth(&refused),
        vec![safe_cex_engine::AuthOutcome::Rejected]
    );
    let (timer, at) = first_retry(&refused, 1500.0);
    notify(&mut e, socket, 1600.0);
    assert!(
        balance(&fire(&mut e, notification_repair, 2110.0)).is_none(),
        "an actually due notification repair cannot bypass the failed-read backoff"
    );
    let actions = fire(&mut e, timer, at);
    let repair = balance(&actions).expect("ordinary retry after actual backoff");
    notify(&mut e, socket, at + 1.0);
    let recovered = answer(&mut e, repair.id, at + 2.0, FRESH);
    assert_eq!(
        balance_auth(&recovered),
        vec![safe_cex_engine::AuthOutcome::Accepted]
    );
    assert_fresh(&e);
}
