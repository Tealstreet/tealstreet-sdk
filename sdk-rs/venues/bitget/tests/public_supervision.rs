use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::{Event, PublicFeedPolicy, SocketId, TimerId};

use support::Harness;

const TICKER: &str = r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTCUSDT","lastPr":"101"}]}"#;

/// Fires every timer due by `until` and returns how many fired.
fn advance(h: &mut Harness, until: f64) -> usize {
    let mut fired = 0;
    while let Some((timer, at)) = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
        fired += 1;
    }
    h.now = until;
    fired
}

fn feed(h: &Harness, name: &str) -> SocketId {
    h.socket_contexts
        .iter()
        .rev()
        .find(|(_, _, context)| context.as_ref().is_some_and(|c| c.feed == name))
        .unwrap()
        .0
}

fn feed_count(h: &Harness, name: &str) -> usize {
    h.socket_contexts
        .iter()
        .filter(|(_, _, context)| context.as_ref().is_some_and(|c| c.feed == name))
        .count()
}

fn primary(h: &Harness) -> SocketId {
    feed(h, "primary")
}

fn primary_count(h: &Harness) -> usize {
    feed_count(h, "primary")
}

fn retry_delay(h: &Harness, before: &[(TimerId, f64)]) -> f64 {
    let fresh: Vec<_> = h
        .timers
        .iter()
        .filter(|timer| !before.contains(timer))
        .map(|(_, at)| at - h.now)
        .collect();
    assert_eq!(fresh.len(), 1, "one retry timer: {fresh:?}");
    fresh[0]
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    let mut h = Harness::public();
    h.book();
    for attempt in 0..7 {
        let socket = primary(&h);
        h.event(Event::WsOpened { socket });
        let before = h.timers.clone();
        h.event(Event::WsClosed { socket, code: 1006 });
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(30_000.0);
        let delay = retry_delay(&h, &before);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let sockets = primary_count(&h);
        let until = h.now + delay;
        advance(&mut h, until);
        assert_eq!(primary_count(&h), sockets + 1, "one reconnect per close");
    }
}

#[test]
fn reconnected_primary_resubscribes_tickers_and_books_and_ignores_retired_socket() {
    let mut h = Harness::public();
    h.book();
    let old = primary(&h);
    let before = h.timers.clone();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let until = h.now + retry_delay(&h, &before);
    advance(&mut h, until);
    let socket = primary(&h);
    assert_ne!(socket, old);
    h.sent.clear();
    h.event(Event::WsOpened { socket });
    let subscribe: Vec<_> = h.sent.iter().filter(|(s, _)| *s == socket).collect();
    assert!(
        subscribe
            .iter()
            .any(|(_, t)| t.contains(r#""channel":"ticker""#))
    );
    assert!(
        subscribe
            .iter()
            .any(|(_, t)| t.contains(r#""channel":"books""#))
    );
    assert!(subscribe.iter().any(|(_, t)| t == "ping"));
    h.event(Event::WsMessage {
        socket: old,
        text: TICKER,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 100.0);
    h.event(Event::WsMessage {
        socket,
        text: TICKER,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 101.0);
}

#[test]
fn open_socket_pings_every_five_seconds_and_silence_retires_it_at_twenty() {
    let mut h = Harness::public();
    let socket = primary(&h);
    let opened = h.now;
    h.sent.clear();
    advance(&mut h, opened + 15_000.0);
    let pings = h
        .sent
        .iter()
        .filter(|(s, t)| *s == socket && t == "ping")
        .count();
    assert_eq!(pings, 3);
    assert!(!h.closed.contains(&socket));
    advance(&mut h, opened + 20_000.0);
    assert!(h.closed.contains(&socket));
    let sockets = primary_count(&h);
    // First retry backoff is at most the 1s base.
    let until = h.now + 1_000.0;
    advance(&mut h, until);
    assert_eq!(
        primary_count(&h),
        sockets + 1,
        "idle timeout reconnects once"
    );
    assert_ne!(primary(&h), socket);
}

#[test]
fn silent_secondary_public_feed_times_out_and_reconnects() {
    let mut h = Harness::public();
    let socket = feed(&h, "secondary-1");
    let opened = h.now;
    advance(&mut h, opened + 19_999.0);
    assert!(!h.closed.contains(&socket));
    advance(&mut h, opened + 20_000.0);
    assert!(h.closed.contains(&socket));
    let sockets = feed_count(&h, "secondary-1");
    // First retry backoff is at most the 1s base.
    let until = h.now + 1_000.0;
    advance(&mut h, until);
    assert_eq!(feed_count(&h, "secondary-1"), sockets + 1);
    assert_ne!(feed(&h, "secondary-1"), socket);
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut engine = crate::support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    let mut h = Harness {
        engine,
        now: 1_000_000.0,
        requests: vec![],
        sockets: vec![],
        socket_contexts: vec![],
        sent: vec![],
        closed: vec![],
        timers: vec![],
        done: vec![],
        failures: vec![],
        logs: vec![],
        command_errors: vec![],
        command_notices: vec![],
        progress: vec![],
        progress_at_request: vec![],
        expect_startup_failure: false,
        startup_failures: vec![],
        deactivations: 0,
    };
    h.engine.start(h.now);
    h.collect();
    h.load_public();
    h.book();
    let until = h.now + 600_000.0;
    let fired = advance(&mut h, until);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(fired, 0, "no supervision timers fire");
}

#[test]
fn traffic_keeps_socket_alive_and_never_opened_socket_expires_at_twenty_seconds() {
    let mut h = Harness::public();
    let socket = primary(&h);
    let start = h.now;
    for step in 1..=8 {
        advance(&mut h, start + f64::from(step) * 5_000.0);
        h.event(Event::WsMessage {
            socket,
            text: "pong",
        });
    }
    assert!(!h.closed.contains(&socket));

    let before = h.timers.clone();
    h.event(Event::WsClosed { socket, code: 1006 });
    let until = h.now + retry_delay(&h, &before);
    advance(&mut h, until);
    let pending = primary(&h);
    let connecting = h.now;
    advance(&mut h, connecting + 19_999.0);
    assert!(!h.closed.contains(&pending));
    advance(&mut h, connecting + 20_000.0);
    assert!(h.closed.contains(&pending));
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::public();
    for _ in 0..3 {
        let socket = primary(&h);
        h.event(Event::WsOpened { socket });
        let before = h.timers.clone();
        h.event(Event::WsClosed { socket, code: 1006 });
        let until = h.now + retry_delay(&h, &before);
        advance(&mut h, until);
    }
    let socket = primary(&h);
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    for step in 1..=7 {
        advance(&mut h, opened + f64::from(step) * 5_000.0);
        h.event(Event::WsMessage {
            socket,
            text: "pong",
        });
    }
    let before = h.timers.clone();
    h.event(Event::WsClosed { socket, code: 1006 });
    assert!((750.0..=1_000.0).contains(&retry_delay(&h, &before)));
}
