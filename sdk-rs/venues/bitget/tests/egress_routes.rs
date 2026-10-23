//! TS #8093 egress-refusal walk: a pre-login 40018/40024 asks the host for the
//! next route; only the host's typed exhaustion reports, once per streak, and
//! the socket keeps retrying the last route as TS does.
mod support;

use safe_cex_engine::{Event, SocketId};
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

/// Fire timers until the next private connect; returns that socket.
fn next_private(h: &mut Harness) -> SocketId {
    let connects = private_connects(h);
    while private_connects(h) == connects {
        let next = h.timers.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1));
        let (timer, at) = next.expect("a paced reconnect");
        h.timers.retain(|(t, _)| *t != timer);
        h.now = at;
        h.event(Event::TimerFired { timer });
    }
    private(h)
}

/// Deliver a refusal and pace to the next private connect; returns its delay.
fn refuse(h: &mut Harness, refusal: &str) -> f64 {
    let from = h.now;
    h.private_message(refusal);
    next_private(h);
    h.now - from
}

fn route_warnings(h: &Harness) -> usize {
    h.logs
        .iter()
        .filter(|m| m.contains("trying the next route"))
        .count()
}

#[test]
fn egress_refusal_before_login_asks_for_the_next_route_without_reporting() {
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
        let delay = refuse(&mut h, refusal);
        assert!(
            h.closed.contains(&first),
            "{refusal}: refused socket closed"
        );
        assert!(
            delay <= 1_000.,
            "{refusal}: first walk step is prompt: {delay}"
        );
        assert!(
            rejects_route(&h, private(&h)),
            "{refusal}: next route asked"
        );
        assert!(h.startup_failures.is_empty(), "{refusal}: nothing reported");
        assert_eq!(auth_rejections(&h), 0);
        assert_eq!(route_warnings(&h), 1, "{refusal}: TS-style walk warning");
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
fn a_host_without_routing_paces_repeated_refusals_with_backoff() {
    // Such a host ignores reject_route and never reports exhaustion.
    let mut h = to_login();
    let mut delays = vec![];
    for _ in 0..6 {
        let socket = private(&h);
        h.event(Event::WsOpened { socket });
        delays.push(refuse(&mut h, IP_BOUND));
    }
    assert!(delays.windows(2).all(|d| d[1] >= d[0] * 0.75), "{delays:?}");
    assert!(delays[5] >= 8_000., "backoff grows: {delays:?}");
    assert!(h.startup_failures.is_empty());
}

#[test]
fn exhaustion_before_startup_reports_once_then_redials_until_a_login() {
    let mut h = to_login();
    refuse(&mut h, IP_BOUND);
    let refused = private(&h);
    h.event(Event::WsRouteExhausted { socket: refused });
    assert_eq!(auth_rejections(&h), 1);
    assert!(
        h.logs
            .iter()
            .any(|m| m.contains("(40018): Current IP does not match the bound IP"))
    );
    // Duplicate exhaustion for the spent socket is inert.
    h.event(Event::WsRouteExhausted { socket: refused });
    // The last route is redialled, paced, and a repeat walk stays silent.
    let socket = next_private(&mut h);
    assert!(!rejects_route(&h, socket));
    h.event(Event::WsOpened { socket });
    refuse(&mut h, IP_BOUND);
    h.event(Event::WsRouteExhausted {
        socket: private(&h),
    });
    assert_eq!(auth_rejections(&h), 1);
    // The user fixes the whitelist: the redial logs in, with no second report.
    let socket = next_private(&mut h);
    h.event(Event::WsOpened { socket });
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    assert!(
        h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains("subscribe"))
    );
    assert_eq!(auth_rejections(&h), 1);
    assert!(h.startup_failures.is_empty());
    assert_eq!(
        h.deactivations, 0,
        "a whitelist refusal is not a refused key"
    );
}

#[test]
fn walk_across_three_routes_reports_only_at_the_end() {
    let mut h = to_login();
    for route in 1..=2 {
        refuse(&mut h, IP_BOUND);
        assert_eq!(private_connects(&h), route + 1);
        let socket = private(&h);
        assert!(rejects_route(&h, socket));
        h.event(Event::WsOpened { socket });
        assert!(h.startup_failures.is_empty(), "route {route}: still silent");
    }
    h.event(Event::WsRouteExhausted {
        socket: private(&h),
    });
    assert_eq!(auth_rejections(&h), 1);
}

#[test]
fn established_session_exhaustion_reports_once_per_streak_and_keeps_retrying() {
    let mut h = Harness::classic();
    let exhaust = |h: &mut Harness| {
        let socket = private(h);
        h.event(Event::WsClosed { socket, code: 1006 });
        let socket = next_private(h);
        h.event(Event::WsOpened { socket });
        refuse(h, IP_BOUND);
        h.event(Event::WsRouteExhausted { socket: private(h) });
        let socket = next_private(h);
        h.event(Event::WsOpened { socket });
    };
    exhaust(&mut h);
    assert_eq!(auth_rejections(&h), 1);
    let socket = private(&h);
    assert!(!rejects_route(&h, socket), "redials the last route like TS");
    // A second spent walk in the same streak stays silent.
    exhaust(&mut h);
    assert_eq!(auth_rejections(&h), 1);
    // A whitelist fix recovers: login ends the streak, so a later one reports.
    let socket = private(&h);
    h.event(Event::WsOpened { socket });
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    exhaust(&mut h);
    assert_eq!(auth_rejections(&h), 2);
    assert_eq!(h.deactivations, 0);
}

#[test]
fn stale_exhaustion_from_an_old_socket_is_inert() {
    let mut h = to_login();
    let old = private(&h);
    refuse(&mut h, IP_BOUND);
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

#[test]
fn a_pending_route_rejection_never_leaks_onto_the_promoted_uta_socket() {
    let mut h = Harness::classic();
    let socket = private(&h);
    h.event(Event::WsClosed { socket, code: 1006 });
    let socket = next_private(&mut h);
    h.event(Event::WsOpened { socket });
    // The v2 refusal waits out the backoff while the account turns out to be UTA.
    h.private_message(IP_BOUND);
    h.respond(
        "/account/accounts",
        200,
        r#"{"code":"40085","msg":"Unified Account mode","data":null}"#,
    );
    h.respond(
        SETTINGS,
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    while !h.sockets.iter().any(|(_, u)| u.contains("/v3/ws/private")) {
        next_private(&mut h);
    }
    let uta = private(&h);
    assert!(
        !rejects_route(&h, uta),
        "the v2 refusal says nothing about v3"
    );
}
