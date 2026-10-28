//! Socket liveness and retry through the shared supervisor.
mod support;

use safe_cex_engine::{Command, Event, PublicFeedPolicy, SocketId, TimerId};
use safe_cex_venue_bybit::BybitLinear;
use support::Harness;
use support::Options;

const PUBLIC: &str = "/v5/public/linear";
const PRIVATE: &str = "/v5/private";
const TICKER: &str =
    r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","lastPrice":"70001"}}"#;
const SNAPSHOT: &str = r#"{"topic":"orderbook.1000.BTCUSDT","type":"snapshot","ts":1687940967465,"data":{"s":"BTCUSDT","b":[["30247.20","1.000"]],"a":[["30248.70","0.250"]],"u":1,"seq":1},"cts":1687940967464}"#;
const PING: &str = r#"{"op":"ping"}"#;
/// `load_public` opens the public socket at this time.
const OPENED: f64 = 1_000.0;

fn private() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.load_public();
    h
}

fn pings(h: &Harness, path: &str) -> usize {
    h.sent_on(path).iter().filter(|text| *text == PING).count()
}

/// Close `socket` and return the delay of the single retry it schedules.
fn close(h: &mut Harness, socket: SocketId) -> f64 {
    let before: Vec<TimerId> = h.timers.iter().map(|(timer, _)| *timer).collect();
    h.event(Event::WsClosed { socket, code: 1006 });
    let fresh: Vec<f64> = h
        .timers
        .iter()
        .filter(|(timer, _)| !before.contains(timer))
        .map(|(_, at)| at - h.now)
        .collect();
    assert_eq!(fresh.len(), 1, "one retry timer: {fresh:?}");
    fresh[0]
}

fn assert_backoff(attempt: i32, delay: f64) {
    let nominal = (500.0 * 2f64.powi(attempt)).min(30_000.0);
    assert!(
        (nominal * 0.75..=nominal).contains(&delay),
        "attempt {attempt}: {delay} outside {nominal} jitter"
    );
}

/// The reconnect notice names the venue's close code, as the legacy socket did.
#[test]
fn a_close_names_its_code_in_the_reconnect_notice() {
    let mut h = Harness::new();
    let socket = h.socket(PUBLIC).unwrap();
    h.event(Event::WsClosed { socket, code: 1013 });
    assert!(
        h.logs
            .iter()
            .any(|log| log == "Bybit socket closed (1013); reconnecting"),
        "{:?}",
        h.logs
    );
}

#[test]
fn a_flapping_public_socket_backs_off_exponentially() {
    let mut h = Harness::new();
    for attempt in 0..10 {
        let socket = h.socket(PUBLIC).unwrap();
        if attempt > 0 {
            h.event(Event::WsOpened { socket });
        }
        let delay = close(&mut h, socket);
        assert_backoff(attempt, delay);
        h.advance(delay);
        assert_ne!(h.socket(PUBLIC), Some(socket), "attempt {attempt}");
    }
}

#[test]
fn a_flapping_private_socket_backs_off_exponentially() {
    let mut h = private();
    for attempt in 0..10 {
        let socket = h.socket(PRIVATE).unwrap();
        h.event(Event::WsOpened { socket });
        let delay = close(&mut h, socket);
        assert_backoff(attempt, delay);
        h.advance(delay);
        assert_ne!(h.socket(PRIVATE), Some(socket), "attempt {attempt}");
    }
}

#[test]
fn reconnect_resubscribes_live_topics_and_ignores_the_retired_socket() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.flush_subscriptions();
    let old = h.socket(PUBLIC).unwrap();
    let delay = close(&mut h, old);
    h.advance(delay);
    let socket = h.socket(PUBLIC).unwrap();
    assert_ne!(socket, old);
    h.event(Event::WsOpened { socket });
    h.flush_subscriptions();
    let sent = h.sent_on(PUBLIC).join("\n");
    assert!(sent.contains("tickers.BTCUSDT") && sent.contains("tickers.ETHUSDT"));
    assert!(sent.contains("orderbook.1000.BTCUSDT"));
    h.event(Event::WsMessage {
        socket: old,
        text: SNAPSHOT,
    });
    assert!(h.engine.book("BTCUSDT").is_none());
    h.event(Event::WsMessage {
        socket,
        text: SNAPSHOT,
    });
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids(), [(30247.2, 1.0)]);
}

#[test]
fn an_open_public_socket_is_pinged_every_ten_seconds() {
    let mut h = Harness::new();
    let socket = h.socket(PUBLIC).unwrap();
    let advance_to = |h: &mut Harness, at: f64| {
        let ms = at - h.now;
        h.advance(ms);
        h.message(PUBLIC, TICKER);
    };
    advance_to(&mut h, OPENED + 9_999.0);
    assert_eq!(pings(&h, PUBLIC), 0);
    advance_to(&mut h, OPENED + 10_000.0);
    assert_eq!(pings(&h, PUBLIC), 1);
    advance_to(&mut h, OPENED + 15_000.0);
    advance_to(&mut h, OPENED + 19_999.0);
    assert_eq!(pings(&h, PUBLIC), 1);
    advance_to(&mut h, OPENED + 20_000.0);
    assert_eq!(pings(&h, PUBLIC), 2);
    assert!(!h.closed.contains(&socket));
}

#[test]
fn a_silent_public_socket_times_out_at_twenty_seconds_and_reconnects() {
    let mut h = Harness::new();
    let socket = h.socket(PUBLIC).unwrap();
    h.advance(OPENED + 19_999.0 - h.now);
    assert!(!h.closed.contains(&socket));
    h.advance(1.0);
    assert!(h.closed.contains(&socket));
    h.advance(500.0);
    assert_ne!(h.socket(PUBLIC), Some(socket));
}

#[test]
fn a_silent_private_socket_times_out_at_twenty_seconds_and_reconnects() {
    let mut h = private();
    let socket = h.socket(PRIVATE).unwrap();
    h.event(Event::WsOpened { socket });
    let opened = h.now;
    for _ in 0..19 {
        h.advance(1_000.0);
        h.message(PUBLIC, TICKER);
    }
    h.advance(opened + 19_999.0 - h.now);
    assert_eq!(pings(&h, PRIVATE), 1);
    assert!(!h.closed.contains(&socket));
    h.advance(1.0);
    assert!(h.closed.contains(&socket));
    assert!(!h.closed.contains(&h.socket(PUBLIC).unwrap()));
    h.advance(500.0);
    assert_ne!(h.socket(PRIVATE), Some(socket));
}

#[test]
fn a_socket_that_never_opens_times_out_after_ten_seconds() {
    let mut h = Harness::new();
    let first = h.socket(PUBLIC).unwrap();
    let delay = close(&mut h, first);
    h.advance(delay);
    let socket = h.socket(PUBLIC).unwrap();
    let connected = h.now;
    h.advance(9_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(1.0);
    assert!(h.closed.contains(&socket));
    assert_eq!(h.now, connected + 10_000.0);

    let mut h = private();
    let socket = h.socket(PRIVATE).unwrap();
    h.advance(9_999.0 - (h.now - OPENED));
    assert!(!h.closed.contains(&socket));
    h.advance(1.0);
    assert!(h.closed.contains(&socket));
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::new();
    for attempt in 0..4 {
        let socket = h.socket(PUBLIC).unwrap();
        if attempt > 0 {
            h.event(Event::WsOpened { socket });
        }
        let delay = close(&mut h, socket);
        h.advance(delay);
    }
    let socket = h.socket(PUBLIC).unwrap();
    h.event(Event::WsOpened { socket });
    for _ in 0..6 {
        h.advance(5_000.0);
        h.message(PUBLIC, TICKER);
    }
    // Retry entropy can produce a fractional opening time. Cross the stable
    // threshold explicitly rather than depend on exact floating-point sums.
    h.advance(1.0);
    h.message(PUBLIC, TICKER);
    assert_backoff(0, close(&mut h, socket));
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(BybitLinear::new(
        safe_cex_venue_bybit::Options {
            public_data_only: true,
            ..safe_cex_venue_bybit::Options::default()
        },
    ));
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    let mut h = Harness {
        engine,
        now: 1_000.0,
        requests: vec![],
        sockets: vec![],
        socket_channels: vec![],
        sent: vec![],
        closed: vec![],
        timers: vec![],
        done: vec![],
        unknown: vec![],
        outcomes: vec![],
        logs: vec![],
        notices: vec![],
        command_errors: vec![],
        command_notices: vec![],
        progress: vec![],
        progress_at_request: vec![],
        startup_failures: vec![],
        deactivated: false,
    };
    h.engine.start(h.now);
    h.collect();
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    assert!(h.timers.is_empty(), "no supervision timers: {:?}", h.timers);
    h.advance(600_000.0);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert!(h.timers.is_empty());
}
#[test]
fn hyro_early_and_retired_watch_timers_do_not_send_early_pings() {
    use safe_cex_venue_bybit::{Endpoints, Variant};
    let mut h = Harness::started(Options {
        variant: Variant::Hyrotrader,
        public_data_only: true,
        endpoints: Some(Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    });
    h.load_public();
    let socket = h.socket(PUBLIC).unwrap();
    let watch = h
        .timers
        .iter()
        .find(|(_, at)| *at == OPENED + 5_000.0)
        .unwrap()
        .0;
    h.now = OPENED + 2_000.0;
    h.event(Event::TimerFired { timer: watch });
    assert_eq!(pings(&h, PUBLIC), 0);
    h.event(Event::TimerFired { timer: watch });
    assert_eq!(pings(&h, PUBLIC), 0);
    h.advance(OPENED + 9_999.0 - h.now);
    assert_eq!(pings(&h, PUBLIC), 0);
    h.advance(1.0);
    assert_eq!(pings(&h, PUBLIC), 1);
    // The first ping rearms the current watch for five seconds later. Closing
    // must clear that active watch, not just ignore the already consumed one.
    let active_watch = h
        .timers
        .iter()
        .find(|(_, at)| *at == OPENED + 15_000.0)
        .expect("watch rearmed after the first ping")
        .0;
    assert_ne!(active_watch, watch);
    h.event(Event::WsClosed { socket, code: 1006 });
    assert!(!h.timers.iter().any(|(timer, _)| *timer == active_watch));
    let pending_after_close = h.timers.clone();
    h.event(Event::TimerFired {
        timer: active_watch,
    });
    h.event(Event::TimerFired { timer: watch });
    assert_eq!(pings(&h, PUBLIC), 1);
    assert_eq!(
        h.timers, pending_after_close,
        "retired watches cannot rearm"
    );
}
