use support::Options;
mod support;
use safe_cex_engine::{Event, PublicFeedPolicy, SocketId};
use safe_cex_venue_binance::Variant;
use support::Harness;

const SPOT_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8}]}"#;
const SPOT_DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"2","volume":"10","quoteVolume":"1000"}]"#;
const PROBE: &str = r#"{"id":0,"method":"LIST_SUBSCRIPTIONS"}"#;
const TRAFFIC: &str = r#"{"result":null,"id":1}"#;

fn spot(policy: PublicFeedPolicy) -> Harness {
    let mut h = Harness::started_with_policy(
        Options {
            variant: Variant::Spot,
            public_data_only: true,
            ..Options::default()
        },
        policy,
    );
    h.respond("/api/v3/exchangeInfo", 200, SPOT_INFO);
    h.respond("/api/v3/ticker/24hr", 200, SPOT_DAILY);
    for socket in h.sockets.iter().map(|(s, _)| *s).collect::<Vec<_>>() {
        h.event(Event::WsOpened { socket });
    }
    h
}

/// Closes `socket` and returns the delay of the single retry it schedules.
fn close(h: &mut Harness, socket: SocketId) -> f64 {
    let before = h.timers.clone();
    h.event(Event::WsClosed { socket, code: 1006 });
    let fresh: Vec<_> = h
        .timers
        .iter()
        .filter(|t| !before.contains(t))
        .map(|(_, at)| at - h.now)
        .collect();
    assert_eq!(fresh.len(), 1, "one retry timer: {fresh:?}");
    fresh[0]
}

fn sent_on(h: &Harness, socket: SocketId) -> Vec<&str> {
    h.sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect()
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    let mut h = Harness::new();
    let mut socket = h.socket("/public/ws").unwrap();
    for attempt in 0..8 {
        let nominal = (500.0 * 2f64.powi(attempt)).min(30_000.0);
        let delay = close(&mut h, socket);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let public = |h: &Harness| {
            h.sockets
                .iter()
                .filter(|(_, u)| u.contains("/public/ws"))
                .count()
        };
        let sockets = public(&h);
        h.advance(delay);
        assert_eq!(public(&h), sockets + 1, "one reconnect per close");
        socket = h.socket("/public/ws").unwrap();
        h.event(Event::WsOpened { socket });
    }
}

#[test]
fn flapping_spot_ticker_shard_backs_off_independently() {
    let mut h = spot(PublicFeedPolicy::Direct);
    let books = h.sockets[0].0;
    let mut shard = h.sockets[1].0;
    for attempt in 0..4 {
        let nominal = 500.0 * 2f64.powi(attempt);
        let delay = close(&mut h, shard);
        assert!((nominal * 0.75..=nominal).contains(&delay), "{delay}");
        h.advance(delay);
        shard = h.sockets.last().unwrap().0;
        h.event(Event::WsOpened { socket: shard });
    }
    assert!((375.0..=500.0).contains(&close(&mut h, books)));
}

#[test]
fn reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::new();
    let old = h.socket("/public/ws").unwrap();
    let delay = close(&mut h, old);
    h.advance(delay);
    let socket = h.socket("/public/ws").unwrap();
    assert_ne!(socket, old);
    h.event(Event::WsOpened { socket });
    assert!(
        sent_on(&h, socket)
            .iter()
            .any(|t| t.contains("SUBSCRIBE") && t.contains("btcusdt@depth")),
        "{:?}",
        sent_on(&h, socket)
    );

    let snapshots = h.snapshot_requests();
    let diff = r#"{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":1,"u":2,"pu":0,"b":[["100","1"]],"a":[]}"#;
    h.event(Event::WsMessage {
        socket: old,
        text: diff,
    });
    assert_eq!(h.snapshot_requests(), snapshots, "retired frames ignored");
    h.diff(1, 2, 0, "100");
    assert_eq!(h.snapshot_requests(), snapshots + 1);

    let market = h.socket("/market/ws").unwrap();
    let delay = close(&mut h, market);
    h.advance(delay);
    let market = h.socket("/market/ws").unwrap();
    h.event(Event::WsOpened { socket: market });
    let sent = sent_on(&h, market);
    assert!(
        sent.iter()
            .any(|t| t.contains("!ticker@arr") && t.contains("!markPrice@arr@1s")),
        "{sent:?}"
    );
}

#[test]
fn spot_feeds_probe_every_five_seconds_and_futures_feeds_never_do() {
    let mut h = spot(PublicFeedPolicy::Direct);
    let (books, shard) = (h.sockets[0].0, h.sockets[1].0);
    let probes = |h: &Harness, socket| sent_on(h, socket).iter().filter(|t| **t == PROBE).count();
    h.advance(4_999.0);
    assert_eq!((probes(&h, books), probes(&h, shard)), (0, 0));
    h.advance(1.0);
    assert_eq!((probes(&h, books), probes(&h, shard)), (1, 1));
    for socket in [books, shard] {
        h.event(Event::WsMessage {
            socket,
            text: r#"{"result":[],"id":0}"#,
        });
    }
    h.advance(10_000.0);
    assert_eq!((probes(&h, books), probes(&h, shard)), (3, 3));

    let mut h = Harness::new();
    h.sent.clear();
    for _ in 0..4 {
        h.advance(4_000.0);
        h.message("/public/ws", TRAFFIC);
        h.message("/market/ws", TRAFFIC);
    }
    assert!(h.sent.is_empty(), "{:?}", h.sent);
}

#[test]
fn traffic_keeps_market_socket_alive_and_twenty_seconds_of_silence_retires_it() {
    let mut h = Harness::new();
    let market = h.socket("/market/ws").unwrap();
    for _ in 0..12 {
        h.advance(5_000.0);
        h.message("/market/ws", TRAFFIC);
        h.message("/public/ws", TRAFFIC);
    }
    assert!(!h.closed.contains(&market));
    h.advance(19_999.0);
    assert!(!h.closed.contains(&market));
    h.advance(1.0);
    assert!(h.closed.contains(&market));
    let sockets = h.sockets.len();
    h.advance(500.0);
    assert!(h.sockets.len() > sockets, "idle timeout retries");
    assert_ne!(h.socket("/market/ws"), Some(market));
}

#[test]
fn never_opened_public_socket_expires_at_ten_seconds() {
    let mut h = Harness::new();
    let old = h.socket("/public/ws").unwrap();
    let delay = close(&mut h, old);
    h.advance(delay);
    let pending = h.socket("/public/ws").unwrap();
    h.advance(9_999.0);
    assert!(!h.closed.contains(&pending));
    h.advance(1.0);
    assert!(h.closed.contains(&pending));
    h.advance(1_000.0);
    assert_ne!(
        h.socket("/public/ws"),
        Some(pending),
        "connect timeout retries"
    );
}

#[test]
fn stable_public_traffic_resets_backoff() {
    let mut h = Harness::new();
    let mut socket = h.socket("/public/ws").unwrap();
    for _ in 0..3 {
        let delay = close(&mut h, socket);
        h.advance(delay);
        socket = h.socket("/public/ws").unwrap();
        h.event(Event::WsOpened { socket });
    }
    for _ in 0..7 {
        h.advance(5_000.0);
        h.message("/public/ws", TRAFFIC);
        h.message("/market/ws", TRAFFIC);
    }
    assert!((375.0..=500.0).contains(&close(&mut h, socket)));
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    for policy in [PublicFeedPolicy::Shared, PublicFeedPolicy::Disabled] {
        let mut h = Harness::started_with_policy(
            Options {
                public_data_only: true,
                ..Options::default()
            },
            policy,
        );
        h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
        h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
        h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
        h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
        let mut spot = spot(policy);
        for h in [&mut h, &mut spot] {
            h.advance(600_000.0);
            assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
            assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
            assert!(h.timers.is_empty(), "no supervision timers: {:?}", h.timers);
            assert!(
                !h.logs.iter().any(|l| l.contains("socket closed")),
                "{:?}",
                h.logs
            );
        }
    }
}
