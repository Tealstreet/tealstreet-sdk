//! Production binding with genuine installed-provider signing and captured IDs.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Event, SocketId, Venue};
use safe_cex_venue_blofin::Blofin;
use support::Harness;

fn sends(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: own, text } if *own == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
fn public(h: &Harness) -> SocketId {
    h.actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .expect("actual public connection")
}
#[test]
fn blofin_declares_managed_sockets_for_the_production_binding() {
    assert!(<Blofin as Venue>::managed_sockets());
}
#[test]
fn duplicate_public_open_has_no_subscriptions_or_supervision_effects() {
    let mut h = Harness::new();
    let socket = public(&h);
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert!(
        sends(&h.actions, socket)
            .iter()
            .any(|s| s.contains("tickers"))
    );
    assert!(sends(&h.actions, socket).contains(&"ping"));
    h.actions.clear();
    h.handle(2., Event::WsOpened { socket });
    h.collect();
    assert!(
        h.actions.is_empty(),
        "duplicate Opened must own no work: {:?}",
        h.actions
    );
}
#[test]
fn duplicate_private_open_cannot_resign_or_repeat_ping() {
    let mut h = Harness::new();
    let socket = h.private;
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert_eq!(
        sends(&h.actions, socket)
            .iter()
            .filter(|s| s.contains("\"op\":\"login\""))
            .count(),
        1
    );
    assert!(sends(&h.actions, socket).contains(&"ping"));
    h.actions.clear();
    h.handle(2., Event::WsOpened { socket });
    h.collect();
    assert!(
        h.actions.is_empty(),
        "duplicate Opened cannot sign or reschedule: {:?}",
        h.actions
    );
}
#[test]
fn private_subscriptions_need_the_real_ack_and_duplicate_ack_is_inert() {
    let mut h = Harness::new();
    let socket = h.private;
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert!(
        sends(&h.actions, socket)
            .iter()
            .all(|s| !s.contains("subscribe"))
    );
    h.actions.clear();
    h.handle(
        2.,
        Event::WsMessage {
            socket,
            text: "pong",
        },
    );
    h.collect();
    assert!(sends(&h.actions, socket).is_empty());
    h.handle(
        3.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    h.collect();
    assert_eq!(
        sends(&h.actions, socket)
            .iter()
            .filter(|s| s.contains("subscribe"))
            .count(),
        4
    );
    h.actions.clear();
    h.handle(
        4.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    h.collect();
    assert!(h.actions.is_empty());
}
