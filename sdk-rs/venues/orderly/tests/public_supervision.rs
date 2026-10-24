mod support;
use safe_cex_engine::{
    Action, Command, Event, PublicFeedPolicy, Response, SocketChannel, SocketId,
};
use safe_cex_venue_orderly::{Options, Orderly};
use support::{Clock, Driver, POSITIONS, options};

const PING: &str = r#"{"event":"ping"}"#;
const PONG: &str = r#"{"event":"pong"}"#;
const AUTH: &str = r#"{"event":"auth","success":true}"#;
const FUNDING: &str = r#"{"topic":"PERP_BTC_USDC@estfundingrate","data":{"symbol":"PERP_BTC_USDC","fundingRate":0.001,"fundingTs":12345}}"#;

fn ticker(close: f64) -> String {
    format!(
        r#"{{"topic":"tickers","data":[{{"symbol":"PERP_BTC_USDC","close":{close},"volume":1,"amount":1}}]}}"#
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Feed {
    Market,
    Funding,
    Private,
}

struct Harness {
    e: Driver,
    clock: Clock,
    log: Vec<(f64, Action)>,
}

impl Harness {
    fn public(policy: PublicFeedPolicy) -> Self {
        let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        e.set_public_feed_policy(policy).unwrap();
        let mut h = Self {
            e,
            clock: Clock::new(1000.0),
            log: vec![],
        };
        h.e.start(1000.0);
        h.bootstrap(|_| false);
        h
    }
    fn private() -> Self {
        let mut h = Self {
            e: safe_cex_engine::test_support::seeded_engine(Orderly::new(options())),
            clock: Clock::new(1000.0),
            log: vec![],
        };
        h.e.start(1000.0);
        h.bootstrap(|a| {
            matches!(
                a,
                Action::WsConnect {
                    channel: Some(SocketChannel::Private),
                    ..
                }
            )
        });
        h
    }
    fn bootstrap(&mut self, done: impl Fn(&Action) -> bool) {
        for _ in 0..10 {
            let out = self.clock.drain(&mut self.e);
            if out.is_empty() {
                return;
            }
            let mut finished = false;
            for action in out {
                if let Action::Http(r) = &action
                    && !finished
                {
                    let body = support::account_body(&r.url, support::ORDER, "[]", POSITIONS);
                    self.e.handle(
                        self.clock.now,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: &body,
                            },
                        },
                    );
                }
                finished |= done(&action);
                self.log.push((self.clock.now, action));
            }
            if finished {
                let now = self.clock.now;
                let out = self.clock.drain(&mut self.e);
                self.log.extend(out.into_iter().map(|a| (now, a)));
                return;
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        let now = self.clock.now;
        let out = self.clock.handle(&mut self.e, event);
        self.log.extend(out.into_iter().map(|a| (now, a)));
    }
    fn advance(&mut self, until: f64) {
        let out = self.clock.advance(&mut self.e, until);
        self.log.extend(out);
    }
    fn connects(&self, feed: Feed) -> Vec<(f64, SocketId)> {
        self.log
            .iter()
            .filter_map(|(at, a)| match a {
                Action::WsConnect {
                    socket,
                    channel,
                    context,
                    ..
                } => {
                    let kind = match (channel, context.as_ref().map(|c| c.feed.as_str())) {
                        (Some(SocketChannel::Private), _) => Feed::Private,
                        (_, Some("market")) => Feed::Market,
                        _ => Feed::Funding,
                    };
                    (kind == feed).then_some((*at, *socket))
                }
                _ => None,
            })
            .collect()
    }
    fn recover(&self, socket: SocketId) -> bool {
        self.log
            .iter()
            .find_map(|(_, a)| match a {
                Action::WsConnect {
                    socket: s,
                    context: Some(context),
                    ..
                } if *s == socket => Some(context.recover),
                _ => None,
            })
            .unwrap()
    }
    fn socket(&self, feed: Feed) -> SocketId {
        self.connects(feed).last().unwrap().1
    }
    fn open(&mut self, feed: Feed) -> SocketId {
        let socket = self.socket(feed);
        self.event(Event::WsOpened { socket });
        if feed == Feed::Private {
            self.message(socket, AUTH);
        }
        socket
    }
    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }
    // Liveness plus the payload each feed must keep delivering.
    fn keepalive(&mut self, feed: Feed, socket: SocketId) {
        self.message(socket, PONG);
        match feed {
            Feed::Market => self.message(socket, &ticker(52000.0)),
            Feed::Funding => self.message(socket, FUNDING),
            Feed::Private => {}
        }
    }
    fn closed(&self, socket: SocketId) -> Option<f64> {
        self.log.iter().find_map(|(at, a)| match a {
            Action::WsClose { socket: s } if *s == socket => Some(*at),
            _ => None,
        })
    }
    fn sent(&self, socket: SocketId) -> Vec<&str> {
        self.log
            .iter()
            .filter_map(|(_, a)| match a {
                Action::WsSend { socket: s, text } if *s == socket => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
    fn pings(&self, socket: SocketId) -> usize {
        self.sent(socket).iter().filter(|t| **t == PING).count()
    }
    /// Closes the socket and returns the delay until its feed reconnects.
    fn close_and_reconnect(&mut self, feed: Feed, socket: SocketId) -> f64 {
        let closed = self.clock.now;
        let before = self.connects(feed).len();
        self.event(Event::WsClosed { socket, code: 1006 });
        while self.connects(feed).len() == before {
            assert!(
                self.clock.now < closed + 30_000.0,
                "{feed:?} never reconnected"
            );
            let out = self.clock.next(&mut self.e);
            self.log.extend(out);
        }
        let connects = self.connects(feed);
        assert_eq!(connects.len(), before + 1, "one reconnect per close");
        connects[before].0 - closed
    }
    fn for_feed(feed: Feed) -> Self {
        match feed {
            Feed::Private => Self::private(),
            _ => Self::public(PublicFeedPolicy::Direct),
        }
    }
}

const FEEDS: [Feed; 3] = [Feed::Market, Feed::Funding, Feed::Private];

fn assert_flapping_backs_off(feed: Feed) {
    let mut h = Harness::for_feed(feed);
    for attempt in 0..7 {
        let socket = h.open(feed);
        h.keepalive(feed, socket);
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(30_000.0);
        let delay = h.close_and_reconnect(feed, socket);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "{feed:?} attempt {attempt}: {delay}"
        );
    }
}

#[test]
fn flapping_market_socket_backs_off_although_each_attempt_delivers_tickers() {
    assert_flapping_backs_off(Feed::Market);
}

#[test]
fn flapping_funding_socket_backs_off_although_each_attempt_delivers_funding() {
    assert_flapping_backs_off(Feed::Funding);
}

#[test]
fn flapping_private_socket_backs_off_although_each_attempt_authenticates() {
    assert_flapping_backs_off(Feed::Private);
}

#[test]
fn market_reconnect_resubscribes_live_topics_and_ignores_retired_socket() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    let old = h.open(Feed::Market);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDC".into(),
    }));
    h.event(Event::Command(Command::SubscribeCandles {
        symbol: "ETHUSDC".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    h.close_and_reconnect(Feed::Market, old);
    let socket = h.open(Feed::Market);
    assert_ne!(socket, old);
    let sent = h.sent(socket);
    for topic in [
        "tickers",
        "bbos",
        "markprices",
        "PERP_BTC_USDC@orderbookupdate",
        "PERP_ETH_USDC@kline_1m",
    ] {
        let frame = format!(r#"{{"event":"subscribe","topic":"{topic}"}}"#);
        assert!(sent.contains(&frame.as_str()), "{topic}: {sent:?}");
    }
    assert!(sent.iter().any(|t| t.contains(r#""type":"orderbook""#)));
    assert_eq!(sent[0], PING);
    h.message(old, &ticker(60000.0));
    assert_eq!(h.e.store().ticker("BTCUSDC").unwrap().last, 51000.0);
    h.message(old, PING);
    assert!(!h.sent(old).contains(&PONG));
    h.message(socket, &ticker(61000.0));
    assert_eq!(h.e.store().ticker("BTCUSDC").unwrap().last, 61000.0);
}

#[test]
fn funding_reconnect_resubscribes_its_shard_and_ignores_retired_socket() {
    let mut h = Harness::public(PublicFeedPolicy::Direct);
    let old = h.open(Feed::Funding);
    h.close_and_reconnect(Feed::Funding, old);
    let socket = h.open(Feed::Funding);
    assert_ne!(socket, old);
    assert_eq!(h.sent(socket)[0], PING);
    assert_eq!(h.sent(socket)[1..], h.sent(old)[1..]);
    h.message(old, FUNDING);
    assert_eq!(h.e.store().ticker("BTCUSDC").unwrap().funding_rate, 0.0);
    h.message(socket, FUNDING);
    assert_eq!(h.e.store().ticker("BTCUSDC").unwrap().funding_rate, 0.001);
}

#[test]
fn private_reconnect_reauthenticates_and_ignores_retired_socket() {
    let mut h = Harness::private();
    let old = h.open(Feed::Private);
    h.close_and_reconnect(Feed::Private, old);
    let socket = h.socket(Feed::Private);
    assert_ne!(socket, old);
    h.event(Event::WsOpened { socket });
    assert!(h.sent(socket)[0].contains(r#""event":"auth""#));
    h.message(old, AUTH);
    h.message(old, PING);
    assert_eq!(h.sent(old).len(), 5, "auth, three subscriptions, ping");
    h.message(socket, AUTH);
    let topics: Vec<_> = h.sent(socket)[1..4].to_vec();
    assert_eq!(topics, h.sent(old)[1..4]);
    assert_eq!(h.sent(socket)[4], PING);
}

#[test]
fn open_sockets_ping_every_ten_seconds_and_silence_retires_them_at_twenty() {
    for feed in FEEDS {
        let mut h = Harness::for_feed(feed);
        let socket = h.open(feed);
        let opened = h.clock.now;
        for step in 1..=6 {
            h.advance(opened + f64::from(step) * 5_000.0);
            h.keepalive(feed, socket);
        }
        assert_eq!(h.pings(socket), 4, "{feed:?}: immediate ping plus three");
        let quiet = h.clock.now;
        h.advance(quiet + 19_999.0);
        assert_eq!(h.closed(socket), None, "{feed:?}");
        h.advance(quiet + 20_000.0);
        assert_eq!(h.closed(socket), Some(quiet + 20_000.0), "{feed:?}");
        let before = h.connects(feed).len();
        let until = h.clock.now + 1_000.0;
        h.advance(until);
        assert_eq!(h.connects(feed).len(), before + 1, "{feed:?} idle retries");
        if feed != Feed::Private {
            assert!(!h.recover(h.socket(feed)), "payload was still fresh");
        }
    }
}

#[test]
fn never_opened_sockets_expire_after_ten_seconds() {
    for feed in FEEDS {
        let mut h = Harness::for_feed(feed);
        let (connected, socket) = *h.connects(feed).last().unwrap();
        h.advance(connected + 9_999.0);
        assert_eq!(h.closed(socket), None, "{feed:?}");
        h.advance(connected + 10_000.0);
        assert_eq!(h.closed(socket), Some(connected + 10_000.0), "{feed:?}");
        let until = h.clock.now + 1_000.0;
        h.advance(until);
        if feed != Feed::Private {
            assert!(h.recover(h.socket(feed)), "a stalled handshake recovers");
        }
    }
}

#[test]
fn unauthenticated_private_socket_expires_ten_seconds_after_open() {
    let mut h = Harness::private();
    let socket = h.socket(Feed::Private);
    h.advance(2_000.0);
    h.event(Event::WsOpened { socket });
    h.advance(5_000.0);
    h.message(socket, PONG);
    h.advance(11_999.0);
    assert_eq!(h.closed(socket), None);
    h.advance(12_000.0);
    assert_eq!(h.closed(socket), Some(12_000.0));
}

#[test]
fn stable_traffic_resets_backoff() {
    for feed in FEEDS {
        let mut h = Harness::for_feed(feed);
        for _ in 0..3 {
            let socket = h.open(feed);
            h.close_and_reconnect(feed, socket);
        }
        let socket = h.open(feed);
        let opened = h.clock.now;
        for step in 1..=7 {
            h.advance(opened + f64::from(step) * 5_000.0);
            h.keepalive(feed, socket);
        }
        let delay = h.close_and_reconnect(feed, socket);
        assert!((750.0..=1_000.0).contains(&delay), "{feed:?}: {delay}");
    }
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::public(PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDC".into(),
    }));
    h.advance(600_000.0);
    assert!(h.connects(Feed::Market).is_empty());
    assert!(h.connects(Feed::Funding).is_empty());
    let seed_deadlines: Vec<_> = h.log.iter().filter_map(|(at, action)| match action {
        Action::SetTimer { timer, at_ms } if *at == 1000.0 && *at_ms == 21_000.0
            && h.log.iter().any(|(cleared_at, action)| *cleared_at == *at
                && matches!(action, Action::ClearTimer { timer: cleared } if cleared == timer)) => Some(*timer),
        _ => None,
    }).collect();
    assert_eq!(
        seed_deadlines.len(),
        2,
        "both admitted seed deadlines retire at bootstrap"
    );
    let churn: Vec<_> = h
        .log
        .iter()
        .filter(|(_, a)| {
            matches!(a, Action::WsClose { .. })
                || matches!(a, Action::SetTimer { timer, .. } if !seed_deadlines.contains(timer))
        })
        .collect();
    assert!(churn.is_empty(), "no public retries: {churn:?}");
}
