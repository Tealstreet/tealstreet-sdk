//! Heartbeats do not prove that account subscriptions are producing payloads.
mod support;
use safe_cex_engine::{Action, Event, SocketId};
use support::*;

fn opened() -> (
    safe_cex_engine::Engine<safe_cex_venue_lighter::Lighter>,
    SocketId,
    Timers,
) {
    let (mut e, socket, mut timers) = ready_tracked(options());
    e.handle(NOW, Event::WsOpened { socket });
    drain(&mut e, &mut timers);
    (e, socket, timers)
}

#[test]
fn heartbeat_only_private_socket_resubscribes_then_reconnects() {
    let (mut e, socket, mut timers) = opened();
    let mut subscriptions = 0;
    let mut closed = false;
    for step in 1..=20 {
        let now = NOW + f64::from(step) * 10_000.0;
        let mut actions = advance(&mut e, &mut timers, now - 1.0);
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
        actions.extend(advance(&mut e, &mut timers, now));
        for action in actions {
            match action {
                Action::WsSend { text, .. } if text.contains("\"type\":\"subscribe\"") => {
                    subscriptions += 1
                }
                Action::WsClose {
                    socket: closed_socket,
                } if closed_socket == socket => closed = true,
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
    let (mut e, socket, mut timers) = opened();
    for step in 1..=40 {
        let now = NOW + f64::from(step) * 10_000.0;
        let mut actions = advance(&mut e, &mut timers, now - 1.0);
        message(
            &mut e,
            socket,
            now,
            r#"{"type":"update/account_all","channel":"account_all:718895","data":{"orders":[],"positions":[],"fills":[]}}"#,
        );
        actions.extend(advance(&mut e, &mut timers, now));
        for action in actions {
            match action {
                Action::WsClose { socket: closed } if closed == socket => {
                    panic!("valid empty account payloads were treated as dead")
                }
                Action::WsSend { text, .. } if text.contains("\"type\":\"subscribe\"") => {
                    panic!("healthy empty account subscription was restarted")
                }
                _ => {}
            }
        }
    }
}

#[test]
fn resumed_empty_subscription_snapshot_cancels_stale_stream_reconnect() {
    let (mut e, socket, mut timers) = opened();
    let mut subscriptions = 0;
    for step in 1..=16 {
        let now = NOW + f64::from(step) * 10_000.0;
        let mut actions = advance(&mut e, &mut timers, now - 1.0);
        let text = if step == 1 || step == 14 {
            r#"{"type":"subscribed/account_all","channel":"account_all:718895","data":{"orders":[],"positions":[],"fills":[]}}"#
        } else {
            r#"{"type":"pong"}"#
        };
        message(&mut e, socket, now, text);
        actions.extend(advance(&mut e, &mut timers, now));
        for action in actions {
            match action {
                Action::WsClose { socket: closed } if closed == socket => {
                    panic!("resumed account payload did not cancel the reconnect grace")
                }
                Action::WsSend { text, .. } if text.contains("\"type\":\"subscribe\"") => {
                    subscriptions += 1
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        subscriptions, 2,
        "stale account data retries subscriptions after 120 seconds"
    );
}
