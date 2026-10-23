//! TS #8093 egress-refusal walk: a pre-login 40018/40024 asks the host for the
//! next route; only the host's typed exhaustion reports, once, and stops.
mod support;

use safe_cex_engine::{ErrorKind, Event, SocketId};
use safe_cex_venue_bitget::Options;
use support::*;

const SETTINGS: &str = "/account/settings";
const IP_BOUND: &str =
    r#"{"event":"error","code":40018,"msg":"Current IP does not match the bound IP"}"#;

fn keyed() -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    }
}

/// Classic account with the private socket open and awaiting its login answer.
fn to_login() -> Harness {
    let mut h = Harness::started_expecting(keyed(), true);
    h.load_public();
    h.respond(
        SETTINGS,
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    let socket = private(&h);
    h.event(Event::WsOpened { socket });
    h
}

fn private(h: &Harness) -> SocketId {
    h.sockets
        .iter()
        .rev()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0
}

fn private_connects(h: &Harness) -> usize {
    h.sockets
        .iter()
        .filter(|(_, u)| u.ends_with("/private"))
        .count()
}

fn rejects_route(h: &Harness, socket: SocketId) -> bool {
    h.socket_contexts
        .iter()
        .find(|(s, _, _)| *s == socket)
        .and_then(|(_, _, context)| context.as_ref())
        .is_some_and(|context| context.reject_route)
}

fn run_for(h: &mut Harness, ms: f64) {
    let horizon = h.now + ms;
    while let Some((timer, at)) = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= horizon)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(t, _)| *t != timer);
        h.now = at;
        h.event(Event::TimerFired { timer });
    }
}

fn auth_rejections(h: &Harness) -> usize {
    h.logs
        .iter()
        .filter(|m| m.contains("every available egress IP"))
        .count()
}

#[test]
fn egress_refusal_before_login_asks_for_the_next_route_silently() {
    for refusal in [
        IP_BOUND,
        r#"{"event":"error","code":"40018","msg":"Invalid IP"}"#,
        r#"{"event":"error","code":40024,"msg":"restricted location"}"#,
    ] {
        let mut h = to_login();
        let first = private(&h);
        assert!(
            !rejects_route(&h, first),
            "first connect carries no refusal"
        );
        h.private_message(refusal);
        assert!(
            h.closed.contains(&first),
            "{refusal}: refused socket closed"
        );
        assert_eq!(private_connects(&h), 2, "{refusal}: reconnects at once");
        assert!(
            rejects_route(&h, private(&h)),
            "{refusal}: next route asked"
        );
        assert!(h.startup_failures.is_empty(), "{refusal}: nothing reported");
        assert_eq!(auth_rejections(&h), 0);
        // The edge is one connect: an ordinary reconnect after it is plain.
        let second = private(&h);
        h.event(Event::WsClosed {
            socket: second,
            code: 1006,
        });
        run_for(&mut h, 60_000.);
        assert!(private_connects(&h) >= 3);
        assert!(
            !rejects_route(&h, private(&h)),
            "{refusal}: edge not sticky"
        );
    }
}

#[test]
fn route_exhaustion_reports_once_and_stops_private_reconnects() {
    let mut h = to_login();
    h.private_message(IP_BOUND);
    let refused = private(&h);
    h.event(Event::WsRouteExhausted { socket: refused });
    assert_eq!(h.startup_failures.len(), 1);
    let failure = &h.startup_failures[0];
    assert_eq!(failure.kind, Some(ErrorKind::AuthPermission));
    assert_eq!(failure.venue_code.as_deref(), Some("40018"));
    assert!(failure.message.contains("Current IP does not match"));
    // Duplicate exhaustion for the spent socket is inert.
    h.event(Event::WsRouteExhausted { socket: refused });
    run_for(&mut h, 600_000.);
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(
        private_connects(&h),
        2,
        "no private reconnect after exhaustion"
    );
}

#[test]
fn walk_across_three_routes_reports_only_at_the_end() {
    let mut h = to_login();
    for route in 1..=2 {
        h.private_message(IP_BOUND);
        assert_eq!(private_connects(&h), route + 1);
        let socket = private(&h);
        assert!(rejects_route(&h, socket));
        h.event(Event::WsOpened { socket });
        assert!(h.startup_failures.is_empty(), "route {route}: still silent");
    }
    h.event(Event::WsRouteExhausted {
        socket: private(&h),
    });
    assert_eq!(h.startup_failures.len(), 1);
    run_for(&mut h, 600_000.);
    assert_eq!(private_connects(&h), 3);
}

#[test]
fn stale_exhaustion_from_an_old_socket_is_inert() {
    let mut h = to_login();
    let old = private(&h);
    h.private_message(IP_BOUND);
    h.event(Event::WsRouteExhausted { socket: old });
    assert!(h.startup_failures.is_empty());
    assert_eq!(private_connects(&h), 2);
}

#[test]
fn login_on_a_later_route_clears_the_refusal() {
    let mut h = to_login();
    h.private_message(IP_BOUND);
    let socket = private(&h);
    h.event(Event::WsOpened { socket });
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    assert!(h.startup_failures.is_empty());
    h.event(Event::WsClosed { socket, code: 1006 });
    run_for(&mut h, 60_000.);
    let next = private(&h);
    assert_ne!(next, socket, "reconnects after a plain close");
    assert!(!rejects_route(&h, next));
    // Without held refusal evidence, exhaustion is only a close.
    h.event(Event::WsRouteExhausted { socket: next });
    assert!(h.startup_failures.is_empty());
    run_for(&mut h, 60_000.);
    assert_ne!(private(&h), next, "reconnects as after a close");
}

#[test]
fn egress_refusal_after_login_is_not_a_route_walk() {
    let mut h = to_login();
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    let socket = private(&h);
    h.private_message(IP_BOUND);
    assert_eq!(private(&h), socket, "an authenticated socket is kept");
    assert!(
        !h.socket_contexts
            .iter()
            .any(|(_, _, c)| c.as_ref().is_some_and(|c| c.reject_route))
    );
}

#[test]
fn other_login_refusals_are_unchanged() {
    for refusal in [
        r#"{"event":"error","code":"40018.5","msg":"x"}"#,
        r#"{"event":"error","code":30005,"msg":"Login failed"}"#,
        r#"{"event":"login","code":40018,"msg":"login event, not error"}"#,
    ] {
        let mut h = to_login();
        h.private_message(refusal);
        assert!(
            !h.socket_contexts
                .iter()
                .any(|(_, _, c)| c.as_ref().is_some_and(|c| c.reject_route)),
            "{refusal}: no route walk"
        );
    }
}
