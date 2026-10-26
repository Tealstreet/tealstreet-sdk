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

#[test]
fn repeated_challenge_and_duplicate_ack_do_not_extend_authentication() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    let opened = h.now;
    let challenge = r#"{"event":"challenge","message":"once-only-challenge"}"#;
    h.message(socket, challenge);
    let frames: Vec<_> = h
        .sent
        .iter()
        .filter(|(_, text)| text.contains("signed_challenge"))
        .cloned()
        .collect();
    assert_eq!(frames.len(), 3);
    h.now = opened + 1_000.;
    h.message(socket, challenge);
    assert_eq!(
        h.sent
            .iter()
            .filter(|(_, text)| text.contains("signed_challenge"))
            .count(),
        3
    );
    h.now = opened + 5_000.;
    let ack = r#"{"event":"subscribed","feed":"fills"}"#;
    h.message(socket, ack);
    h.advance(opened + 19_000.);
    h.message(socket, ack);
    h.advance(opened + 20_000.);
    assert!(
        h.closed.contains(&socket),
        "a duplicate ACK cannot renew the distinct-ACK bound"
    );
}

#[test]
fn authenticated_pongs_cannot_discharge_missing_initial_payloads() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    let opened = h.now;
    for step in 1..18 {
        h.advance(opened + f64::from(step) * 10_000.);
        h.message(socket, FUTURES_PONG);
    }
    h.advance(opened + 180_000.);
    assert!(h.closed.contains(&socket));
}

fn take_spot_token(h: &mut H<KrakenSpot>, socket: SocketId) -> safe_cex_engine::RequestId {
    h.engine.handle(h.now, Event::WsOpened { socket });
    let mut token = None;
    for action in safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now) {
        match action {
            Action::Http(request) => {
                assert!(request.url.ends_with("GetWebSocketsToken"));
                assert!(token.replace(request.id).is_none());
            }
            Action::SetTimer { timer, at_ms } => h.timers.push((timer, at_ms)),
            Action::ClearTimer { timer } => h.timers.retain(|(t, _)| *t != timer),
            Action::WsSend { .. } => {
                panic!("token consumers cannot subscribe before a token result")
            }
            _ => (),
        }
    }
    token.expect("real native token acquisition")
}

#[test]
fn a_retired_epoch_token_cannot_subscribe_the_replacement() {
    let mut h = spot(false, PublicFeedPolicy::Direct);
    let old_socket = h.socket();
    let old_token = take_spot_token(&mut h, old_socket);
    let delay = h.close(old_socket);
    h.wait(delay);
    let socket = h.socket();
    assert_ne!(socket, old_socket);
    let token = take_spot_token(&mut h, socket);
    h.sent.clear();
    h.event(Event::HttpResponse {
        request: old_token,
        response: Response {
            status: 200,
            headers: "",
            body: SPOT_TOKEN,
        },
    });
    assert!(
        h.sent.is_empty(),
        "a late token has no authority over either epoch"
    );
    h.event(Event::HttpResponse {
        request: token,
        response: Response {
            status: 200,
            headers: "",
            body: SPOT_TOKEN,
        },
    });
    assert_eq!(h.sent.len(), 2);
    assert!(h.sent.iter().all(|(id, _)| *id == socket));
    assert!(h.sent[0].1.contains("executions"));
    assert!(h.sent[1].1.contains("balances"));
}

#[test]
fn authentication_rejection_retires_without_publishing_financial_data() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    h.sent.clear();
    h.message(socket, r#"{"event":"subscribed_failed","feed":"fills"}"#);
    assert!(h.closed.contains(&socket));
    assert!(h.sent.is_empty());
    h.message(socket, r#"{"event":"challenge","message":"late"}"#);
    assert!(h.sent.is_empty());
}

fn invalid_time<V: Venue>(mut h: H<V>, auth: fn(&mut H<V>, SocketId)) {
    let socket = ready_socket(&mut h, auth);
    h.sent.clear();
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        h.engine
            .handle(now, Event::WsMessage { socket, text: "{}" });
        let actions: Vec<_> = h.engine.drain_actions().collect();
        assert!(
            actions.iter().all(|a| !matches!(
                a,
                Action::WsSend { .. } | Action::Http(_) | Action::ManagedHttp(_)
            )),
            "nonfinite ingress emitted work: {actions:?}"
        );
    }
}
#[test]
fn nonfinite_socket_receipts_produce_no_native_work() {
    invalid_time(futures(true, PublicFeedPolicy::Direct), noop);
    invalid_time(futures(false, PublicFeedPolicy::Direct), authenticate);
    invalid_time(spot(true, PublicFeedPolicy::Direct), noop);
    invalid_time(spot(false, PublicFeedPolicy::Direct), spot_subscribed);
}

#[test]
fn a_token_result_at_idle_equality_releases_no_subscription_frames() {
    let mut h = spot(false, PublicFeedPolicy::Direct);
    let socket = h.socket();
    let request = take_spot_token(&mut h, socket);
    h.sent.clear();
    h.now += 45_000.;
    h.event(Event::HttpResponse {
        request,
        response: Response {
            status: 200,
            headers: "",
            body: SPOT_TOKEN,
        },
    });
    assert!(h.sent.is_empty());
    assert!(
        h.closed.contains(&socket),
        "one-shot final release must retire an expired epoch"
    );
}
