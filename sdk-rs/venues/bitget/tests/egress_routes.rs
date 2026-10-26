//! Private-WS egress refusal (TS #8093; `docs/plans/RUST_BITGET_MANAGED_SOCKET_ROUTING.md`):
//! 40018/40024 before login leaves the route, only a streak's first refusal
//! skips backoff, and route exhaustion reports once and stops reconnecting.
mod support;
use safe_cex_engine::{Event, SocketContext, SocketId};
use support::*;

const IP_BOUND: &str = r#"{"event":"error","code":40018,"msg":"Invalid IP"}"#;
const LOCATION: &str = r#"{"event":"error","code":40024,"msg":"Restricted location"}"#;
const WHITELIST: &str =
    "the API key's IP whitelist allows none of Tealstreet's WebSocket egress IPs";

/// Classic private startup loaded over REST, with the private socket open but not logged in.
fn pre_login() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    let socket = private(&h).last().unwrap().0;
    h.event(Event::WsOpened { socket });
    h
}

fn private(h: &Harness) -> Vec<(SocketId, SocketContext)> {
    h.socket_contexts
        .iter()
        .filter(|(_, url, _)| url.ends_with("/private"))
        .map(|(socket, _, context)| (*socket, context.clone().expect("private feed context")))
        .collect()
}

fn count(h: &Harness, text: &str) -> usize {
    h.logs.iter().filter(|log| log.contains(text)).count()
}

/// Fires every armed timer up to `until`, including timers they arm.
fn run_until(h: &mut Harness, until: f64) {
    for _ in 0..10_000 {
        let Some((timer, at)) = h
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            h.now = h.now.max(until);
            return;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    panic!("timers did not settle");
}

#[test]
fn an_egress_refusal_redials_at_once_with_reject_route_and_reports_no_error() {
    for refusal in [IP_BOUND, LOCATION] {
        let mut h = pre_login();
        let before = private(&h).len();
        h.private_message(refusal);
        let now = h.now;
        run_until(&mut h, now);
        let sockets = private(&h);
        assert_eq!(
            sockets.len(),
            before + 1,
            "a streak's first refusal skips backoff"
        );
        let (_, context) = sockets.last().unwrap();
        assert!(context.reject_route, "the redial leaves the refused route");
        assert_eq!(count(&h, "trying the next route"), 1);
        assert_eq!(count(&h, "login failed"), 0, "not a generic login failure");
        assert!(h.startup_failures.is_empty());
        assert_eq!(h.deactivations, 0);
    }
}

#[test]
fn later_refusals_in_a_streak_wait_for_backoff_and_still_leave_the_route() {
    let mut h = pre_login();
    h.private_message(IP_BOUND);
    let now = h.now;
    run_until(&mut h, now);
    let socket = private(&h).last().unwrap().0;
    h.event(Event::WsOpened { socket });
    let before = private(&h).len();
    h.private_message(IP_BOUND);
    assert_eq!(
        private(&h).len(),
        before,
        "a repeated refusal never redials hot, even if the host ignores reject_route"
    );
    let refused_at = h.now;
    while private(&h).len() == before {
        let (timer, at) = h
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a backoff retry is armed");
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    assert!(
        h.now - refused_at >= 750.,
        "the next attempt waits for backoff: {}",
        h.now - refused_at
    );
    assert!(private(&h).last().unwrap().1.reject_route);
}

#[test]
fn route_exhaustion_reports_once_stops_reconnecting_and_startup_settles() {
    let mut h = pre_login();
    h.private_message(IP_BOUND);
    let now = h.now;
    run_until(&mut h, now);
    let socket = private(&h).last().unwrap().0;
    h.event(Event::WsRouteExhausted { socket });
    assert_eq!(count(&h, WHITELIST), 1);
    assert!(
        h.logs.iter().any(|log| log.contains("40018 Invalid IP")),
        "the native refusal is in the report: {:?}",
        h.logs
    );
    h.event(Event::WsRouteExhausted { socket });
    assert_eq!(count(&h, WHITELIST), 1, "one report per streak");
    let connects = private(&h).len();
    let until = h.now + 600_000.;
    run_until(&mut h, until);
    assert_eq!(
        private(&h).len(),
        connects,
        "zero reconnects after exhaustion"
    );
    assert!(h.startup_failures.is_empty(), "never a startup failure");
    assert_eq!(h.deactivations, 0);
    let loaded = h.engine.store().loaded();
    assert!(
        loaded.balance && loaded.positions && loaded.orders,
        "REST-gated startup still settles"
    );
}

#[test]
fn exhaustion_for_a_stale_private_socket_is_inert() {
    let mut h = pre_login();
    let stale = private(&h).last().unwrap().0;
    h.private_message(IP_BOUND);
    let now = h.now;
    run_until(&mut h, now);
    h.event(Event::WsRouteExhausted { socket: stale });
    assert_eq!(count(&h, WHITELIST), 0);
    let socket = private(&h).last().unwrap().0;
    h.event(Event::WsOpened { socket });
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    assert!(
        h.sent
            .iter()
            .any(|(s, text)| *s == socket && text.contains("subscribe")),
        "the live socket still logs in and subscribes"
    );
}

#[test]
fn other_login_refusals_keep_the_native_generic_failure() {
    let mut h = pre_login();
    let before = private(&h).len();
    h.private_message(r#"{"event":"error","code":30005,"msg":"other"}"#);
    assert_eq!(count(&h, "trying the next route"), 0);
    assert_eq!(count(&h, "login failed"), 1);
    assert_eq!(
        private(&h).len(),
        before,
        "native backoff, no immediate redial"
    );
    assert!(private(&h).iter().all(|(_, context)| !context.reject_route));
}

/// Closes the newest private socket without any login answer, then lets the
/// supervisor redial and the new socket open.
fn silent_close(h: &mut Harness) {
    let socket = private(h).last().unwrap().0;
    let dialed = private(h).len();
    h.event(Event::WsClosed { socket, code: 1006 });
    // Fire timers one at a time until exactly the redial appears.
    while private(h).len() == dialed {
        let (timer, at) = h
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a redial timer");
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    let socket = private(h).last().unwrap().0;
    h.event(Event::WsOpened { socket });
}

#[test]
fn two_silent_closes_before_login_leave_the_route() {
    let mut h = pre_login();
    let before = private(&h).len();
    silent_close(&mut h);
    assert!(
        !private(&h).last().unwrap().1.reject_route,
        "one silent close keeps the route"
    );
    silent_close(&mut h);
    let sockets = private(&h);
    assert_eq!(sockets.len(), before + 2);
    assert!(
        sockets.last().unwrap().1.reject_route,
        "the second silent close in a row leaves the route"
    );
    assert_eq!(count(&h, "keeps closing before login"), 1);
    assert_eq!(h.deactivations, 0);
}

#[test]
fn a_login_answer_means_the_route_works_and_resets_the_streak() {
    let mut h = pre_login();
    silent_close(&mut h);
    // The venue answered this connection: its close says nothing about the route.
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    silent_close(&mut h);
    silent_close(&mut h);
    assert!(
        !private(&h).last().unwrap().1.reject_route,
        "an answered login resets the streak, so one later silent close keeps the route"
    );
    assert_eq!(count(&h, "keeps closing before login"), 0);
}
