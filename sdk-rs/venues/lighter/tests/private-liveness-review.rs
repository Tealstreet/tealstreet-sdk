//! Heartbeats do not prove that account subscriptions are producing payloads.
mod support;
use safe_cex_engine::{Action, Event, SocketId, TimerId};
use support::*;

fn opened() -> (
    safe_cex_engine::Engine<safe_cex_venue_lighter::Lighter>,
    SocketId,
    TimerId,
) {
    let (mut e, socket) = ready();
    e.handle(NOW, Event::WsOpened { socket });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 10_000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    (e, socket, timer)
}

#[test]
fn heartbeat_only_private_socket_resubscribes_then_reconnects() {
    let (mut e, socket, mut timer) = opened();
    let mut subscriptions = 0;
    let mut closed = false;
    for step in 1..=20 {
        let now = NOW + f64::from(step) * 10_000.0;
        message(
            &mut e,
            socket,
            now,
            if step % 2 == 0 {
                r#"{"type":"pong"}"#
            } else {
                r#"{"type":"error","channel":"account_all:718895","message":"subscription rejected"}"#
            },
        );
        e.handle(now, Event::TimerFired { timer });
        for action in e.drain_actions() {
            match action {
                Action::WsSend { text, .. } if text.contains("\"type\":\"subscribe\"") => {
                    subscriptions += 1
                }
                Action::WsClose {
                    socket: closed_socket,
                } if closed_socket == socket => closed = true,
                Action::SetTimer { timer: next, at_ms } if at_ms == now + 10_000.0 => timer = next,
                _ => {}
            }
        }
    }
    assert_eq!(
        subscriptions, 2,
        "missing account payloads retry both subscriptions"
    );
    assert!(
        closed,
        "pongs must not conceal a dead account subscription indefinitely"
    );
}

#[test]
fn empty_account_payloads_keep_private_protocol_healthy() {
    let (mut e, socket, mut timer) = opened();
    for step in 1..=40 {
        let now = NOW + f64::from(step) * 10_000.0;
        message(
            &mut e,
            socket,
            now,
            r#"{"type":"update/account_all","channel":"account_all:718895","data":{"orders":[],"positions":[],"fills":[]}}"#,
        );
        e.handle(now, Event::TimerFired { timer });
        for action in e.drain_actions() {
            match action {
                Action::WsClose { .. } => {
                    panic!("valid empty account payloads were treated as dead")
                }
                Action::WsSend { text, .. } if text.contains("\"type\":\"subscribe\"") => {
                    panic!("healthy empty account subscription was restarted")
                }
                Action::SetTimer { timer: next, at_ms } if at_ms == now + 10_000.0 => timer = next,
                _ => {}
            }
        }
    }
}

#[test]
fn resumed_empty_subscription_snapshot_cancels_stale_stream_reconnect() {
    let (mut e, socket, mut timer) = opened();
    let mut subscriptions = 0;
    for step in 1..=16 {
        let now = NOW + f64::from(step) * 10_000.0;
        let text = if step == 1 || step == 14 {
            r#"{"type":"subscribed/account_all","channel":"account_all:718895","data":{"orders":[],"positions":[],"fills":[]}}"#
        } else {
            r#"{"type":"pong"}"#
        };
        message(&mut e, socket, now, text);
        e.handle(now, Event::TimerFired { timer });
        for action in e.drain_actions() {
            match action {
                Action::WsClose { .. } => {
                    panic!("resumed account payload did not cancel the reconnect grace")
                }
                Action::WsSend { text, .. } if text.contains("\"type\":\"subscribe\"") => {
                    subscriptions += 1
                }
                Action::SetTimer { timer: next, at_ms } if at_ms == now + 10_000.0 => timer = next,
                _ => {}
            }
        }
    }
    assert_eq!(
        subscriptions, 2,
        "stale account data retries subscriptions after 120 seconds"
    );
}
