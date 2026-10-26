//! Actual native startup and physical ingress, before managed socket adoption.
use super::*;

fn duplicate_open<V: Venue>(mut h: H<V>, auth: fn(&mut H<V>, SocketId)) {
    let socket = ready_socket(&mut h, auth);
    let sent = h.sent.clone();
    let timers = h.timers.clone();
    h.now += 1.;
    h.event(Event::WsOpened { socket });
    assert_eq!(
        h.sent, sent,
        "a duplicate OPEN cannot replay subscriptions or auth"
    );
    assert_eq!(h.timers, timers, "a duplicate OPEN cannot renew any bound");
}

fn open_at_expiry<V: Venue>(mut h: H<V>, timeout: f64) {
    let socket = h.socket();
    h.now += timeout;
    h.event(Event::WsOpened { socket });
    assert!(
        h.closed.contains(&socket),
        "delayed timer delivery cannot admit an expired OPEN"
    );
    assert!(!h.sent.iter().any(|(id, _)| *id == socket));
}

fn receive_at_expiry<V: Venue>(mut h: H<V>, auth: fn(&mut H<V>, SocketId), idle: f64, pong: &str) {
    let socket = ready_socket(&mut h, auth);
    h.now += idle;
    h.message(socket, pong);
    assert!(
        h.closed.contains(&socket),
        "a frame at idle equality cannot rescue an expired feed"
    );
}

#[test]
fn futures_public_duplicate_open_is_inert() {
    duplicate_open(futures(true, PublicFeedPolicy::Direct), noop);
}
#[test]
fn futures_private_duplicate_open_is_inert() {
    duplicate_open(futures(false, PublicFeedPolicy::Direct), authenticate);
}
#[test]
fn spot_public_duplicate_open_is_inert() {
    duplicate_open(spot(true, PublicFeedPolicy::Direct), noop);
}
#[test]
fn spot_private_duplicate_open_is_inert() {
    duplicate_open(spot(false, PublicFeedPolicy::Direct), spot_subscribed);
}
#[test]
fn futures_public_open_at_connect_equality_refuses() {
    open_at_expiry(futures(true, PublicFeedPolicy::Direct), 45_000.);
}
#[test]
fn futures_private_open_at_connect_equality_refuses() {
    open_at_expiry(futures(false, PublicFeedPolicy::Direct), 15_000.);
}
#[test]
fn spot_public_open_at_connect_equality_refuses() {
    open_at_expiry(spot(true, PublicFeedPolicy::Direct), 45_000.);
}
#[test]
fn spot_private_open_at_connect_equality_refuses() {
    open_at_expiry(spot(false, PublicFeedPolicy::Direct), 45_000.);
}
#[test]
fn futures_public_receive_at_idle_equality_refuses() {
    receive_at_expiry(
        futures(true, PublicFeedPolicy::Direct),
        noop,
        45_000.,
        FUTURES_PONG,
    );
}
#[test]
fn futures_private_receive_at_idle_equality_refuses() {
    receive_at_expiry(
        futures(false, PublicFeedPolicy::Direct),
        authenticate,
        60_000.,
        FUTURES_PONG,
    );
}
#[test]
fn spot_public_receive_at_idle_equality_refuses() {
    receive_at_expiry(
        spot(true, PublicFeedPolicy::Direct),
        noop,
        45_000.,
        SPOT_PONG,
    );
}
#[test]
fn spot_private_receive_at_idle_equality_refuses() {
    receive_at_expiry(
        spot(false, PublicFeedPolicy::Direct),
        spot_subscribed,
        45_000.,
        SPOT_PONG,
    );
}

#[test]
fn futures_challenge_at_auth_equality_signs_nothing() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    h.sent.clear();
    h.now += 15_000.;
    h.message(
        socket,
        r#"{"event":"challenge","message":"expired-challenge"}"#,
    );
    assert!(
        !h.sent
            .iter()
            .any(|(id, text)| *id == socket && text.contains("signed_challenge"))
    );
    assert!(h.closed.contains(&socket));
}

#[test]
fn futures_three_empty_payloads_before_final_ack_complete_only_the_payload_bound() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    let opened = h.now;
    h.message(
        socket,
        r#"{"event":"challenge","message":"native-challenge"}"#,
    );
    for text in [
        r#"{"feed":"fills_snapshot","fills":[]}"#,
        r#"{"feed":"open_orders_snapshot","orders":[]}"#,
        r#"{"feed":"open_positions","positions":[]}"#,
    ] {
        h.message(socket, text);
    }
    for feed in ["fills", "open_orders", "open_positions"] {
        h.message(
            socket,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    for step in 1..=19 {
        h.advance(opened + f64::from(step) * 10_000.);
        h.message(socket, FUTURES_PONG);
    }
    assert!(
        !h.closed.contains(&socket),
        "validated empty payloads survive OPEN+180s"
    );
}

#[test]
fn futures_permissive_decoded_empty_envelope_refreshes_receive() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    let opened = h.now;
    h.advance(opened + 59_999.);
    h.message(socket, "{}");
    h.advance(opened + 60_000.);
    assert!(!h.closed.contains(&socket));
}

#[test]
fn futures_malformed_envelope_does_not_refresh_receive() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    let opened = h.now;
    h.advance(opened + 59_999.);
    h.message(socket, "{malformed");
    h.advance(opened + 60_000.);
    assert!(h.closed.contains(&socket));
}

#[test]
fn unopened_spot_balance_frame_cannot_create_http_work() {
    let mut h = spot(false, PublicFeedPolicy::Direct);
    let socket = h.socket();
    h.engine.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"channel":"balances","data":[]}"#,
        },
    );
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
        "unopened financial ingress must not create account HTTP: {actions:?}"
    );
}
