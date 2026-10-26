//! Supervised liveness and backoff for the futures and spot public/private sockets.
#[path = "spot_support/mod.rs"]
mod spot_support;
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
    Venue,
};
use safe_cex_venue_kraken::{Kraken, KrakenSpot, Options};

const SPOT_TOKEN: &str = r#"{"error":[],"result":{"token":"fresh-token","expires":900}}"#;
const PRIVATE_SUBSCRIBED: &str =
    r#"{"method":"subscribe","success":true,"result":{"channel":"executions"}}"#;

struct H<V: Venue> {
    engine: Engine<V>,
    now: f64,
    channel: SocketChannel,
    respond: fn(&str) -> Option<&'static str>,
    ping: &'static str,
    timers: Vec<(TimerId, f64)>,
    sockets: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    fired: usize,
}

fn futures_rest(url: &str) -> Option<&'static str> {
    Some(if url.contains("/instruments") {
        support::META
    } else if url.contains("/tickers") {
        support::TICKERS
    } else if url.contains("/openorders") {
        support::OPEN
    } else if url.contains("/openpositions") {
        r#"{"result":"success","openPositions":[]}"#
    } else if url.contains("/accounts") {
        support::ACCOUNT
    } else if url.contains("/leveragepreferences") {
        r#"{"result":"success","leveragePreferences":[]}"#
    } else {
        return None;
    })
}

fn spot_rest(url: &str) -> Option<&'static str> {
    Some(if url.ends_with("AssetPairs") {
        spot_support::META
    } else if url.ends_with("Ticker") {
        spot_support::TICKERS
    } else if url.ends_with("Balance") {
        spot_support::BALANCE
    } else if url.ends_with("OpenOrders") {
        spot_support::OPEN
    } else if url.ends_with("GetWebSocketsToken") {
        SPOT_TOKEN
    } else {
        return None;
    })
}

fn futures(public: bool, policy: PublicFeedPolicy) -> H<Kraken> {
    let options = Options {
        public_data_only: public,
        ..support::options()
    };
    H::start(
        Kraken::new(options),
        policy,
        if public {
            SocketChannel::Public
        } else {
            SocketChannel::Private
        },
        futures_rest,
        r#"{"event":"ping"}"#,
    )
}

fn spot(public: bool, policy: PublicFeedPolicy) -> H<KrakenSpot> {
    let options = Options {
        public_data_only: public,
        ..spot_support::options()
    };
    H::start(
        KrakenSpot::new(options),
        policy,
        if public {
            SocketChannel::Public
        } else {
            SocketChannel::Private
        },
        spot_rest,
        r#"{"method":"ping"}"#,
    )
}

impl<V: Venue> H<V> {
    fn start(
        venue: V,
        policy: PublicFeedPolicy,
        channel: SocketChannel,
        respond: fn(&str) -> Option<&'static str>,
        ping: &'static str,
    ) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(support::NOW);
        let mut h = Self {
            engine,
            now: support::NOW,
            channel,
            respond,
            ping,
            timers: vec![],
            sockets: vec![],
            sent: vec![],
            closed: vec![],
            fired: 0,
        };
        h.collect();
        h
    }
    fn collect(&mut self) {
        loop {
            let actions =
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now);
            if actions.is_empty() {
                return;
            }
            for action in actions {
                match action {
                    Action::Http(request) => {
                        if let Some(body) = (self.respond)(&request.url) {
                            self.engine.handle(
                                self.now,
                                Event::HttpResponse {
                                    request: request.id,
                                    response: Response {
                                        status: 200,
                                        headers: "",
                                        body,
                                    },
                                },
                            );
                        }
                    }
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                    Action::WsConnect {
                        socket,
                        channel: Some(channel),
                        ..
                    } if channel == self.channel => self.sockets.push(socket),
                    Action::WsSend { socket, text } => self.sent.push((socket, text)),
                    Action::WsClose { socket } => self.closed.push(socket),
                    _ => (),
                }
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn socket(&self) -> SocketId {
        *self.sockets.last().expect("supervised socket")
    }
    fn open(&mut self) -> SocketId {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
        socket
    }
    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }
    fn advance(&mut self, until: f64) {
        while let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
            self.fired += 1;
        }
        self.now = until;
    }
    fn wait(&mut self, ms: f64) {
        let until = self.now + ms;
        self.advance(until);
    }
    fn close(&mut self, socket: SocketId) -> f64 {
        let before = self.timers.clone();
        self.event(Event::WsClosed { socket, code: 1006 });
        let fresh: Vec<_> = self
            .timers
            .iter()
            .filter(|t| !before.contains(t))
            .map(|(_, at)| at - self.now)
            .collect();
        assert_eq!(fresh.len(), 1, "one retry timer: {fresh:?}");
        fresh[0]
    }
    fn pings(&self, socket: SocketId) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t == self.ping)
            .count()
    }
}

/// Completes the futures challenge handshake and every private feed ack.
fn authenticate(h: &mut H<Kraken>, socket: SocketId) {
    h.message(
        socket,
        r#"{"event":"challenge","message":"c100b894-1729-464d-ace1-52dbce11db42"}"#,
    );
    for feed in ["fills", "open_orders", "open_positions"] {
        h.message(
            socket,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
}

fn ready_socket<V: Venue>(h: &mut H<V>, authenticate_with: fn(&mut H<V>, SocketId)) -> SocketId {
    let socket = h.open();
    authenticate_with(h, socket);
    socket
}

fn noop<V: Venue>(_: &mut H<V>, _: SocketId) {}

fn spot_subscribed(h: &mut H<KrakenSpot>, socket: SocketId) {
    h.message(socket, PRIVATE_SUBSCRIBED);
}

fn flapping_socket_backs_off<V: Venue>(mut h: H<V>, cap: f64) {
    for attempt in 0..8 {
        let socket = h.open();
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(cap);
        let delay = h.close(socket);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay} vs {nominal}"
        );
        let sockets = h.sockets.len();
        h.wait(delay);
        assert_eq!(h.sockets.len(), sockets + 1, "one reconnect per close");
    }
}

fn pings_and_idle_timeout<V: Venue>(
    mut h: H<V>,
    authenticate_with: fn(&mut H<V>, SocketId),
    ping: f64,
    idle: f64,
) {
    let socket = ready_socket(&mut h, authenticate_with);
    let opened = h.now;
    let before = h.pings(socket);
    h.advance(opened + idle - 1.0);
    assert_eq!(h.pings(socket) - before, ((idle - 1.0) / ping) as usize);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + idle);
    assert!(h.closed.contains(&socket), "silence retires the socket");
    let sockets = h.sockets.len();
    h.wait(1_000.0);
    assert_eq!(h.sockets.len(), sockets + 1, "idle timeout retries");
}

fn never_opened_socket_expires<V: Venue>(mut h: H<V>, deadline: f64) {
    let socket = h.open();
    let delay = h.close(socket);
    h.wait(delay);
    let pending = h.socket();
    let connecting = h.now;
    h.advance(connecting + deadline - 1.0);
    assert!(!h.closed.contains(&pending));
    h.advance(connecting + deadline);
    assert!(h.closed.contains(&pending), "connect deadline");
}

fn stable_traffic_resets_backoff<V: Venue>(
    mut h: H<V>,
    authenticate_with: fn(&mut H<V>, SocketId),
    keepalive: &str,
) {
    for _ in 0..3 {
        let socket = h.open();
        let delay = h.close(socket);
        h.wait(delay);
    }
    let socket = ready_socket(&mut h, authenticate_with);
    for _ in 0..6 {
        h.wait(6_000.0);
        h.message(socket, keepalive);
    }
    assert!(!h.closed.contains(&socket));
    let delay = h.close(socket);
    assert!((750.0..=1_000.0).contains(&delay), "{delay}");
}

const FUTURES_PONG: &str = r#"{"event":"pong"}"#;
const SPOT_PONG: &str = r#"{"method":"pong"}"#;

#[test]
fn futures_public_flapping_socket_backs_off_although_each_attempt_opens() {
    flapping_socket_backs_off(futures(true, PublicFeedPolicy::Direct), 16_000.0);
}

#[test]
fn futures_private_flapping_socket_backs_off_up_to_thirty_seconds() {
    flapping_socket_backs_off(futures(false, PublicFeedPolicy::Direct), 30_000.0);
}

#[test]
fn spot_public_flapping_socket_backs_off_although_each_attempt_opens() {
    flapping_socket_backs_off(spot(true, PublicFeedPolicy::Direct), 16_000.0);
}

#[test]
fn spot_private_flapping_socket_backs_off_although_each_token_subscribes() {
    flapping_socket_backs_off(spot(false, PublicFeedPolicy::Direct), 16_000.0);
}

#[test]
fn futures_public_pings_every_fifteen_seconds_and_silence_retires_at_forty_five() {
    pings_and_idle_timeout(
        futures(true, PublicFeedPolicy::Direct),
        noop,
        15_000.0,
        45_000.0,
    );
}

#[test]
fn futures_private_pings_every_ten_seconds_and_silence_retires_at_sixty() {
    pings_and_idle_timeout(
        futures(false, PublicFeedPolicy::Direct),
        authenticate,
        10_000.0,
        60_000.0,
    );
}

#[test]
fn spot_public_pings_every_fifteen_seconds_and_silence_retires_at_forty_five() {
    pings_and_idle_timeout(
        spot(true, PublicFeedPolicy::Direct),
        noop,
        15_000.0,
        45_000.0,
    );
}

#[test]
fn spot_private_pings_every_fifteen_seconds_and_silence_retires_at_forty_five() {
    pings_and_idle_timeout(
        spot(false, PublicFeedPolicy::Direct),
        spot_subscribed,
        15_000.0,
        45_000.0,
    );
}

#[test]
fn never_opened_sockets_expire_at_their_connect_deadline() {
    never_opened_socket_expires(futures(true, PublicFeedPolicy::Direct), 45_000.0);
    never_opened_socket_expires(futures(false, PublicFeedPolicy::Direct), 15_000.0);
    never_opened_socket_expires(spot(true, PublicFeedPolicy::Direct), 45_000.0);
    never_opened_socket_expires(spot(false, PublicFeedPolicy::Direct), 45_000.0);
}

#[test]
fn stable_traffic_resets_backoff_on_every_socket() {
    stable_traffic_resets_backoff(futures(true, PublicFeedPolicy::Direct), noop, FUTURES_PONG);
    stable_traffic_resets_backoff(
        futures(false, PublicFeedPolicy::Direct),
        authenticate,
        FUTURES_PONG,
    );
    stable_traffic_resets_backoff(spot(true, PublicFeedPolicy::Direct), noop, SPOT_PONG);
    stable_traffic_resets_backoff(
        spot(false, PublicFeedPolicy::Direct),
        spot_subscribed,
        SPOT_PONG,
    );
}

#[test]
fn futures_private_auth_stages_keep_their_fifteen_second_deadlines() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    let opened = h.now;
    h.advance(opened + 14_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 15_000.0);
    assert!(h.closed.contains(&socket), "challenge reply deadline");

    h.wait(1_000.0);
    let socket = h.open();
    assert!(!h.closed.contains(&socket));
    h.wait(10_000.0);
    h.message(socket, r#"{"event":"challenge","message":"late"}"#);
    let challenged = h.now;
    h.advance(challenged + 14_999.0);
    assert!(
        !h.closed.contains(&socket),
        "challenge restarts the ack stage"
    );
    h.advance(challenged + 15_000.0);
    assert!(h.closed.contains(&socket), "subscribed ack deadline");
}

#[test]
fn futures_public_reconnect_resubscribes_and_ignores_retired_socket() {
    let mut h = futures(true, PublicFeedPolicy::Direct);
    let old = h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "PF_XBTUSD".into(),
    }));
    let delay = h.close(old);
    h.wait(delay);
    h.sent.clear();
    let socket = h.open();
    assert_ne!(socket, old);
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    for feed in ["ticker", "book"] {
        assert!(
            sent.iter().any(|t| t.contains(r#""event":"subscribe""#)
                && t.contains(&format!(r#""feed":"{feed}""#))
                && t.contains("PF_XBTUSD")),
            "{feed}: {sent:?}"
        );
    }
    h.message(
        old,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":900}"#,
    );
    assert_eq!(h.engine.store().ticker("PF_XBTUSD").unwrap().last, 100.0);
    h.message(
        socket,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":901}"#,
    );
    assert_eq!(h.engine.store().ticker("PF_XBTUSD").unwrap().last, 901.0);
}

#[test]
fn spot_public_reconnect_resubscribes_and_ignores_retired_socket() {
    let mut h = spot(true, PublicFeedPolicy::Direct);
    let old = h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "ETH/USD".into(),
    }));
    let delay = h.close(old);
    h.wait(delay);
    h.sent.clear();
    let socket = h.open();
    assert_ne!(socket, old);
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    for channel in ["ticker", "book"] {
        assert!(
            sent.iter().any(|t| t.contains(r#""method":"subscribe""#)
                && t.contains(&format!(r#""channel":"{channel}""#))
                && t.contains("ETH/USD")),
            "{channel}: {sent:?}"
        );
    }
    h.message(
        old,
        r#"{"channel":"ticker","data":[{"symbol":"ETH/USD","last":777}]}"#,
    );
    assert_eq!(h.engine.store().ticker("ETH/USD").unwrap().last, 50.0);
    h.message(
        socket,
        r#"{"channel":"ticker","data":[{"symbol":"ETH/USD","last":778}]}"#,
    );
    assert_eq!(h.engine.store().ticker("ETH/USD").unwrap().last, 778.0);
}

#[test]
fn suppressed_futures_public_feed_is_not_supervised() {
    let mut h = futures(true, PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "PF_XBTUSD".into(),
    }));
    h.wait(600_000.0);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 120, "only the five-second poll fires");
    assert_eq!(h.timers.len(), 1);
}

#[test]
fn suppressed_spot_public_feed_is_not_supervised() {
    let mut h = spot(true, PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "ETH/USD".into(),
    }));
    h.wait(600_000.0);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 120, "only the five-second poll fires");
    assert_eq!(h.timers.len(), 1);
}

#[path = "support/managed_socket_context.rs"]
mod managed_socket_context;
