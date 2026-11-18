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
fn futures_public_uses_heartbeat_and_silence_retires_at_twenty_seconds() {
    pings_and_idle_timeout(
        futures(true, PublicFeedPolicy::Direct),
        noop,
        f64::INFINITY,
        20_000.0,
    );
}

#[test]
fn futures_private_uses_heartbeat_and_silence_retires_at_twenty_seconds() {
    pings_and_idle_timeout(
        futures(false, PublicFeedPolicy::Direct),
        authenticate,
        f64::INFINITY,
        20_000.0,
    );
}

#[test]
fn futures_subscribe_to_heartbeat_on_every_open_before_private_auth() {
    for public in [true, false] {
        let mut h = futures(public, PublicFeedPolicy::Direct);
        for _ in 0..2 {
            let socket = h.open();
            assert!(
                h.sent.iter().any(|(s, text)| *s == socket
                    && text == r#"{"event":"subscribe","feed":"heartbeat"}"#)
            );
            let delay = h.close(socket);
            h.wait(delay);
        }
    }
}

#[test]
fn futures_heartbeat_and_expected_payloads_keep_public_and_private_sockets_alive() {
    for public in [true, false] {
        let mut h = futures(public, PublicFeedPolicy::Direct);
        let socket = h.open();
        if !public {
            authenticate(&mut h, socket);
        }
        for _ in 0..12 {
            h.wait(10_000.);
            h.message(socket, r#"{"feed":"heartbeat","time":1700000000000}"#);
            if public {
                ticker(&mut h, socket, "PF_XBTUSD");
                ticker(&mut h, socket, "PF_ETHUSD");
            }
        }
        assert!(!h.closed.contains(&socket));
        assert_eq!(h.pings(socket), 0, "Futures rejects JSON event:ping");
    }
}

#[derive(Debug)]
struct Recovery {
    first_frame_ms: f64,
    data_ms: f64,
    redial_ms: f64,
}

fn initial_private_payloads(h: &mut H<Kraken>, socket: SocketId) {
    for text in [
        r#"{"feed":"fills_snapshot","fills":[]}"#,
        r#"{"feed":"open_orders_snapshot","orders":[]}"#,
        r#"{"feed":"open_positions_snapshot","positions":[]}"#,
    ] {
        h.message(socket, text);
    }
}

fn quiet_window_execution(simulate_gap: bool) -> (u32, u32, f64, f64) {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    initial_private_payloads(&mut h, socket);
    let before = h.sent.len();
    for _ in 0..30 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
    }
    let unsubscribed = h.sent[before..].iter().any(|(s, text)| {
        *s == socket
            && text.contains(r#""event":"unsubscribe""#)
            && text.contains(r#""feed":"fills""#)
    });
    h.message(socket, &format!(r#"{{"feed":"open_orders","order":{{"instrument":"PF_XBTUSD","order_id":"quiet-order","qty":2,"filled":0,"type":"limit","direction":0,"limit_price":100,"last_update_time":{}}}}}"#, h.now));
    h.now += 1.;
    let fill = format!(
        r#"{{"fill_id":"quiet-window-fill","order_id":"quiet-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"remaining_order_qty":1.5,"time":{}}}"#,
        h.now
    );
    // The venue sends an execution during an unsubscribe gap in the replacement
    // snapshot; an uninterrupted subscription receives the same execution live.
    let feed = if simulate_gap && unsubscribed {
        "fills_snapshot"
    } else {
        "fills"
    };
    h.message(socket, &format!(r#"{{"feed":"{feed}","fills":[{fill}]}}"#));
    let first = h.engine.take_frame(h.now).map_or(0, support::fill_count);
    let order = h
        .engine
        .store()
        .orders()
        .iter()
        .find(|order| order.id == "quiet-order")
        .unwrap();
    let progress = (order.filled, order.remaining);
    h.message(socket, &format!(r#"{{"feed":"fills","fills":[{fill}]}}"#));
    let duplicate = h.engine.take_frame(h.now).map_or(0, support::fill_count);
    (first, duplicate, progress.0, progress.1)
}

#[test]
fn futures_execution_during_the_quiet_probe_window_is_not_lost() {
    let (first, duplicate, filled, remaining) = quiet_window_execution(true);
    assert_eq!(first, 1, "a quiet-window execution must emit once");
    assert_eq!(duplicate, 0, "a replay must not emit twice");
    assert_eq!(filled, 0.5);
    assert_eq!(remaining, 1.5);
}

#[test]
fn futures_live_execution_control_at_the_quiet_window_emits_once() {
    assert_eq!(quiet_window_execution(false), (1, 0, 0.5, 1.5));
}

fn reconnect_execution_delivery(snapshot_first: bool) -> (u32, u32, f64) {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let old = ready_socket(&mut h, authenticate);
    initial_private_payloads(&mut h, old);
    h.wait(10_000.);
    heartbeat(&mut h, old);
    let last_live = h.now;
    let _ = h.engine.take_frame(h.now);
    h.wait(20_000.);
    assert!(h.closed.contains(&old));
    let executed_at = h.now + 1.;
    h.wait(1_000.);
    let replacement = h.open();
    assert_ne!(old, replacement);
    h.message(
        replacement,
        r#"{"event":"challenge","message":"fresh-execution-recovery"}"#,
    );
    for feed in ["fills", "open_orders", "open_positions"] {
        h.message(
            replacement,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    assert!(executed_at > last_live && executed_at < h.now);
    h.message(replacement, &format!(r#"{{"feed":"open_orders_snapshot","orders":[{{"instrument":"PF_XBTUSD","order_id":"reconnect-order","qty":1.5,"filled":0.5,"type":"limit","direction":0,"limit_price":100,"last_update_time":{executed_at}}}]}}"#));
    let _ = h.engine.take_frame(h.now);
    let fill = format!(
        r#"{{"fill_id":"new-offline-execution","order_id":"reconnect-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"remaining_order_qty":1.5,"time":{executed_at}}}"#
    );
    let feed = if snapshot_first {
        "fills_snapshot"
    } else {
        "fills"
    };
    h.message(
        replacement,
        &format!(r#"{{"feed":"{feed}","fills":[{fill}]}}"#),
    );
    let first = h.engine.take_frame(h.now).map_or(0, support::fill_count);
    h.now += 1.;
    h.message(
        replacement,
        &format!(r#"{{"feed":"fills","fills":[{fill}]}}"#),
    );
    let replay = h.engine.take_frame(h.now).map_or(0, support::fill_count);
    let remaining = h
        .engine
        .store()
        .orders()
        .iter()
        .find(|order| order.id == "reconnect-order")
        .unwrap()
        .remaining;
    println!(
        "RECONNECT snapshot_first={snapshot_first} first={first} replay={replay} remaining={remaining}"
    );
    (first, replay, remaining)
}

#[test]
fn futures_reconnect_snapshot_execution_emits_once() {
    assert_eq!(reconnect_execution_delivery(true), (1, 0, 1.5));
}

#[test]
fn futures_reconnect_live_execution_control_emits_once() {
    assert_eq!(reconnect_execution_delivery(false), (1, 0, 1.5));
}

fn review_reconnect_boundary(offset: f64) -> (u32, u32) {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let old = ready_socket(&mut h, authenticate);
    initial_private_payloads(&mut h, old);
    h.wait(100.);
    let emitted_at = h.now;
    let emitted = format!(
        r#"{{"fill_id":"review-earlier","order_id":"review-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"time":{emitted_at}}}"#
    );
    h.message(old, &format!(r#"{{"feed":"fills","fills":[{emitted}]}}"#));
    assert_eq!(h.engine.take_frame(h.now).map_or(0, support::fill_count), 1);
    h.wait(20_000.);
    assert!(h.closed.contains(&old));
    h.wait(1_000.);
    let replacement = h.open();
    assert_ne!(old, replacement);
    authenticate(&mut h, replacement);
    let _ = h.engine.take_frame(h.now);
    let time = emitted_at + offset;
    assert!(time > support::NOW);
    let unseen = format!(
        r#"{{"fill_id":"review-unseen","order_id":"review-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"time":{time}}}"#
    );
    h.message(
        replacement,
        &format!(r#"{{"feed":"fills_snapshot","fills":[{emitted},{unseen}]}}"#),
    );
    let first = h.engine.take_frame(h.now).map_or(0, support::fill_count);
    h.message(
        replacement,
        &format!(r#"{{"feed":"fills_snapshot","fills":[{unseen},{emitted}]}}"#),
    );
    assert_eq!(
        h.engine.take_frame(h.now).map_or(0, support::fill_count),
        0,
        "snapshot replay must not re-emit either execution"
    );
    h.now += 1.;
    h.message(
        replacement,
        &format!(r#"{{"feed":"fills","fills":[{emitted},{unseen}]}}"#),
    );
    let replay = h.engine.take_frame(h.now).map_or(0, support::fill_count);
    println!("REVIEW offset={offset} snapshot_first={first} live_replay={replay}");
    (first, replay)
}

#[test]
fn review_reconnect_unseen_same_timestamp() {
    assert_eq!(review_reconnect_boundary(0.), (1, 0));
}

#[test]
fn review_reconnect_unseen_older_timestamp() {
    assert_eq!(review_reconnect_boundary(-1.), (1, 0));
}

#[test]
fn review_reconnect_unseen_newer_timestamp_control() {
    assert_eq!(review_reconnect_boundary(1.), (1, 0));
}

#[test]
fn futures_reconnect_recovers_if_the_initial_fill_snapshot_never_arrived() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let old = ready_socket(&mut h, authenticate);
    // The socket dies before it can establish startup fill history.
    h.wait(20_000.);
    assert!(h.closed.contains(&old));
    h.wait(1_000.);
    let replacement = h.open();
    assert_ne!(old, replacement);
    authenticate(&mut h, replacement);
    let _ = h.engine.take_frame(h.now);
    let fill = r#"{"fill_id":"missed-startup","order_id":"new-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true}"#;
    h.message(
        replacement,
        &format!(r#"{{"feed":"fills_snapshot","fills":[{fill}]}}"#),
    );
    assert_eq!(h.engine.take_frame(h.now).map_or(0, support::fill_count), 1);
    h.message(
        replacement,
        &format!(r#"{{"feed":"fills","fills":[{fill}]}}"#),
    );
    assert_eq!(h.engine.take_frame(h.now).map_or(0, support::fill_count), 0);
}

fn ticker(h: &mut H<Kraken>, socket: SocketId, symbol: &str) {
    h.message(
        socket,
        &format!(r#"{{"feed":"ticker","product_id":"{symbol}","last":125}}"#),
    );
}

fn heartbeat(h: &mut H<Kraken>, socket: SocketId) {
    h.message(socket, r#"{"feed":"heartbeat","time":1700000000000}"#);
}

#[test]
fn futures_stalled_ticker_recovers_despite_heartbeat_and_another_live_symbol() {
    let mut h = futures(true, PublicFeedPolicy::Direct);
    let socket = h.open();
    ticker(&mut h, socket, "PF_XBTUSD");
    ticker(&mut h, socket, "PF_ETHUSD");
    for _ in 0..3 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
        ticker(&mut h, socket, "PF_ETHUSD");
    }
    assert!(
        h.closed.contains(&socket),
        "a busy sibling and heartbeat cannot mask XBT ticker silence"
    );
    h.wait(1_000.);
    let replacement = h.open();
    assert_ne!(replacement, socket);
    assert!(h.sent.iter().any(|(s, text)| *s == replacement
        && text.contains(r#""feed":"ticker""#)
        && text.contains("PF_XBTUSD")));
    ticker(&mut h, replacement, "PF_XBTUSD");
}

#[test]
fn futures_only_live_subscribed_tickers_refresh_their_deadlines() {
    let mut h = futures(true, PublicFeedPolicy::Direct);
    let socket = h.open();
    h.event(Event::Command(Command::SetTickerSymbols {
        symbols: vec!["PF_XBTUSD".into()],
    }));
    for _ in 0..18 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
        ticker(&mut h, socket, "PF_XBTUSD");
    }
    assert!(
        !h.closed.contains(&socket),
        "unsubscribed ETH cannot expire active XBT"
    );
    h.event(Event::Command(Command::SetTickerSymbols {
        symbols: vec!["PF_XBTUSD".into(), "PF_ETHUSD".into()],
    }));
    for _ in 0..2 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
        ticker(&mut h, socket, "PF_XBTUSD");
    }
    assert!(
        !h.closed.contains(&socket),
        "new ETH interest gets its own first-payload window"
    );
    h.wait(10_000.);
    heartbeat(&mut h, socket);
    assert!(
        h.closed.contains(&socket),
        "a subscribed ticker that never starts must expire"
    );
}

#[test]
fn futures_empty_ticker_envelopes_do_not_prove_data_progress() {
    let mut h = futures(true, PublicFeedPolicy::Direct);
    let socket = h.open();
    h.event(Event::Command(Command::SetTickerSymbols {
        symbols: vec!["PF_XBTUSD".into()],
    }));
    ticker(&mut h, socket, "PF_XBTUSD");
    for _ in 0..3 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
        h.message(socket, r#"{"feed":"ticker","product_id":"PF_XBTUSD"}"#);
    }
    assert!(h.closed.contains(&socket));
}

#[test]
fn futures_book_silence_repairs_while_tickers_and_heartbeat_are_live() {
    let mut h = futures(true, PublicFeedPolicy::Direct);
    let socket = h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "PF_XBTUSD".into(),
    }));
    h.message(socket, r#"{"feed":"book_snapshot","product_id":"PF_XBTUSD","seq":1,"bids":[{"price":99,"qty":2}],"asks":[]}"#);
    let before = h.sent.len();
    for _ in 0..13 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
        ticker(&mut h, socket, "PF_XBTUSD");
        ticker(&mut h, socket, "PF_ETHUSD");
    }
    assert!(!h.closed.contains(&socket));
    for event in ["unsubscribe", "subscribe"] {
        assert!(h.sent[before..].iter().any(|(s, text)| *s == socket
            && text.contains(&format!(r#""event":"{event}""#))
            && text.contains(r#""feed":"book""#)));
    }
}

#[test]
fn futures_quiet_private_feeds_keep_their_live_subscriptions() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    initial_private_payloads(&mut h, socket);
    let before = h.sent.len();
    for _ in 0..180 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
    }
    assert!(
        !h.closed.contains(&socket),
        "quiet private feeds must not cause a reconnect"
    );
    assert_eq!(
        h.sent.len(),
        before,
        "quiet private feeds must not unsubscribe or resubscribe"
    );
}

#[test]
fn futures_quiet_private_socket_still_recovers_when_heartbeats_stop() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    initial_private_payloads(&mut h, socket);
    for _ in 0..30 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
    }
    assert!(!h.closed.contains(&socket));
    h.wait(20_000.);
    assert!(h.closed.contains(&socket));
    h.wait(1_000.);
    let replacement = h.open();
    assert_ne!(replacement, socket);
    authenticate(&mut h, replacement);
    for feed in ["fills", "open_orders", "open_positions"] {
        assert!(h.sent.iter().any(|(s, text)| *s == replacement
            && text.contains(r#""event":"subscribe""#)
            && text.contains(&format!(r#""feed":"{feed}""#))));
    }
}

#[test]
fn futures_late_ticker_after_a_timer_jump_cannot_rescue_an_expired_feed() {
    let mut h = futures(true, PublicFeedPolicy::Direct);
    let socket = h.open();
    for _ in 0..2 {
        h.wait(10_000.);
        heartbeat(&mut h, socket);
    }
    h.now += 10_000.;
    ticker(&mut h, socket, "PF_XBTUSD");
    assert!(
        h.closed.contains(&socket),
        "a late ticker must not reset the expired data clock before its callback"
    );
}

fn wake_recovery(public: bool, sleep_ms: f64) -> Recovery {
    let mut h = futures(public, PublicFeedPolicy::Direct);
    let socket = h.open();
    if public {
        h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "PF_XBTUSD".into(),
        }));
    } else {
        authenticate(&mut h, socket);
        initial_private_payloads(&mut h, socket);
    }
    h.message(socket, r#"{"feed":"heartbeat","time":1700000000000}"#);
    let sockets = h.sockets.len();
    // Timers do not run while asleep. Deliver overdue callbacks at the new time.
    h.now += sleep_ms;
    let wake = h.now;
    h.advance(wake);
    while h.sockets.len() == sockets {
        let next = h
            .timers
            .iter()
            .map(|(_, at)| *at)
            .min_by(f64::total_cmp)
            .unwrap();
        assert!(next < wake + 65_000., "recovery must remain bounded");
        h.advance(next);
    }
    assert!(h.closed.contains(&socket));
    let redial_ms = h.now - wake;
    h.wait(50.); // Simulated websocket handshake.
    let replacement = h.open();
    assert_ne!(replacement, socket);
    h.wait(50.); // Simulated first server response.
    let first_frame_ms = h.now - wake;
    if public {
        for feed in ["ticker", "book"] {
            assert!(
                h.sent
                    .iter()
                    .any(|(s, text)| *s == replacement
                        && text.contains(&format!(r#""feed":"{feed}""#))),
                "public interest must be replayed: {feed}"
            );
        }
        h.message(
            replacement,
            r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":125}"#,
        );
        assert_eq!(h.engine.store().tickers()[0].last, 125.);
    } else {
        assert!(
            h.sent
                .iter()
                .any(|(s, text)| *s == replacement && text.contains(r#""event":"challenge""#))
        );
        h.message(
            replacement,
            r#"{"event":"challenge","message":"fresh-wake-challenge"}"#,
        );
        h.wait(50.); // Challenge signing/subscription round trip.
        for feed in ["fills", "open_orders", "open_positions"] {
            assert!(h.sent.iter().any(|(s, text)| *s == replacement
                && text.contains(&format!(r#""feed":"{feed}""#))
                && text.contains("fresh-wake-challenge")));
            h.message(
                replacement,
                &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
            );
        }
        h.wait(50.); // Initial account payloads, without an account reload.
        initial_private_payloads(&mut h, replacement);
    }
    Recovery {
        first_frame_ms,
        data_ms: h.now - wake,
        redial_ms,
    }
}

#[test]
fn futures_wake_recovery_measurement() {
    for sleep_ms in [35_000., 14_400_000.] {
        for public in [true, false] {
            let recovery = wake_recovery(public, sleep_ms);
            println!(
                "WAKE sleep_ms={sleep_ms} channel={} {recovery:?}",
                if public { "public" } else { "private" }
            );
            assert!(recovery.first_frame_ms >= recovery.redial_ms);
            assert!(recovery.data_ms >= recovery.first_frame_ms);
        }
    }
}

#[test]
fn futures_idle_recovery_finishes_within_five_seconds_of_wake() {
    for public in [true, false] {
        let recovery = wake_recovery(public, 35_000.);
        assert!(
            recovery.data_ms < 5_000.,
            "wake recovery: public={public} {recovery:?}"
        );
    }
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
    let mut public = futures(true, PublicFeedPolicy::Direct);
    public.event(Event::Command(Command::SetTickerSymbols {
        symbols: vec!["PF_XBTUSD".into()],
    }));
    stable_traffic_resets_backoff(
        public,
        noop,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":125}"#,
    );
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

// Bodies from wss://futures.kraken.com/ws/v1, 2026-11-02.
#[test]
fn a_venue_alert_about_a_frame_does_not_refuse_the_private_socket() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = ready_socket(&mut h, authenticate);
    assert!(
        h.sent
            .iter()
            .any(|(s, t)| *s == socket && t.contains(r#""feed":"heartbeat""#))
    );
    h.message(
        socket,
        r#"{"event":"alert","message":"Bad websocket message"}"#,
    );
    h.message(
        socket,
        r#"{"event":"alert","message":"Couldn't subscribe to invalid feed"}"#,
    );
    h.wait(1_000.0);
    assert!(!h.closed.contains(&socket));
    assert_eq!(h.sockets.len(), 1);
}

#[test]
fn a_refused_signature_still_refuses_the_private_socket() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    h.message(
        socket,
        r#"{"event":"challenge","message":"c100b894-1729-464d-ace1-52dbce11db42"}"#,
    );
    h.message(
        socket,
        r#"{"event":"alert","message":"Failed to subscribe to authenticated feed"}"#,
    );
    assert!(h.closed.contains(&socket));
}

#[test]
fn a_restored_network_redials_a_socket_waiting_out_backoff() {
    let mut h = futures(false, PublicFeedPolicy::Direct);
    let socket = h.open();
    for _ in 0..3 {
        let waiting = h.close(h.socket());
        assert!(waiting > 0.0);
        h.wait(waiting);
    }
    let _ = socket;
    let sockets = h.sockets.len();
    let waiting = h.close(h.socket());
    assert!(waiting > 500.0, "{waiting}");
    h.event(Event::NetworkRestored);
    h.wait(0.0);
    assert_eq!(
        h.sockets.len(),
        sockets + 1,
        "redials without the {waiting} ms"
    );
}
