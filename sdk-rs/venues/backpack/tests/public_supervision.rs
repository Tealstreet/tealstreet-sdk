mod support;
use safe_cex_engine::{
    Action, Command, Event, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_backpack::{Backpack, Options};
use support::Driver;

const PING: &str = r#"{"method":"PING"}"#;
const PONG: &str = r#"{"error":{"code":4002,"message":"PONG"}}"#;
const MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#;
const TICKERS: &str = r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#;
const MARKS: &str = r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#;
const TICKER: &str = r#"{"stream":"ticker.SOL_USDC_PERP","data":{"s":"SOL_USDC_PERP","o":"100","c":"105","v":"1","V":"105"}}"#;

struct Harness {
    engine: Driver,
    channel: SocketChannel,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    sockets: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    depth_reads: usize,
    fired: usize,
}

impl Harness {
    fn public() -> Self {
        Self::public_with(PublicFeedPolicy::Direct)
    }
    fn public_with(policy: PublicFeedPolicy) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(0.0);
        let mut h = Self::wrap(engine, SocketChannel::Public, 0.0);
        let markets = h.request("markets");
        for (request, url) in h.reply(markets, MARKETS) {
            h.reply(
                request,
                if url.contains("tickers") {
                    TICKERS
                } else {
                    MARKS
                },
            );
        }
        h
    }
    fn private() -> Self {
        let (engine, socket) = support::ready();
        let mut h = Self::wrap(engine, SocketChannel::Private, 1000.0);
        h.sockets.push(socket);
        h
    }
    fn wrap(engine: Driver, channel: SocketChannel, now: f64) -> Self {
        Self {
            engine,
            channel,
            now,
            timers: Vec::new(),
            sockets: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            depth_reads: 0,
            fired: 0,
        }
    }
    fn requests(&mut self) -> Vec<(safe_cex_engine::RequestId, String)> {
        let mut requests = Vec::new();
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect {
                    socket,
                    channel: Some(channel),
                    ..
                } if channel == self.channel => self.sockets.push(socket),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::Http(r) => {
                    if r.url.contains("/depth") {
                        self.depth_reads += 1;
                    }
                    requests.push((r.id, r.url));
                }
                _ => (),
            }
        }
        requests
    }
    fn request(&mut self, needle: &str) -> safe_cex_engine::RequestId {
        self.requests()
            .into_iter()
            .find(|(_, url)| url.contains(needle))
            .expect(needle)
            .0
    }
    fn reply(
        &mut self,
        request: safe_cex_engine::RequestId,
        body: &str,
    ) -> Vec<(safe_cex_engine::RequestId, String)> {
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.requests()
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.requests();
    }
    fn socket(&self) -> SocketId {
        *self.sockets.last().unwrap()
    }
    fn release_snapshot_handoff(&mut self) {
        let Some(timer) = self
            .timers
            .iter()
            .find(|(_, at)| *at == self.now)
            .map(|(timer, _)| *timer)
        else {
            return;
        };
        self.timers.retain(|(issued, _)| *issued != timer);
        self.event(Event::TimerFired { timer });
    }
    fn open(&mut self) -> SocketId {
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
        if self.channel == SocketChannel::Public {
            self.release_snapshot_handoff();
        }
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
    fn reconnect(&mut self, delay: f64) {
        let sockets = self.sockets.len();
        let until = self.now + delay;
        self.advance(until);
        assert_eq!(self.sockets.len(), sockets + 1, "one reconnect per close");
    }
    fn pings(&self, socket: SocketId) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t == PING)
            .count()
    }
}

fn flapping_socket_backs_off(mut h: Harness, payload: Option<&str>) {
    for attempt in 0..8 {
        let socket = h.open();
        if let Some(text) = payload {
            h.message(socket, text);
        }
        let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
        let delay = h.close(socket);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        h.reconnect(delay);
    }
}

#[test]
fn flapping_public_socket_backs_off_although_each_attempt_opens() {
    flapping_socket_backs_off(Harness::public(), None);
}

#[test]
fn flapping_private_socket_backs_off_although_each_attempt_opens() {
    flapping_socket_backs_off(Harness::private(), None);
}

#[test]
fn flapping_private_socket_backs_off_although_each_attempt_streams_account_data() {
    let payload = r#"{"stream":"account.balanceUpdate","data":{}}"#;
    flapping_socket_backs_off(Harness::private(), Some(payload));
}

#[test]
fn reconnect_resubscribes_live_topics_resnapshots_and_ignores_retired_socket() {
    let mut h = Harness::public();
    let old = h.open();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "SOL_USDC_PERP".into(),
    }));
    h.release_snapshot_handoff();
    let delay = h.close(old);
    h.reconnect(delay);
    h.sent.clear();
    let reads = h.depth_reads;
    let socket = h.open();
    let sent: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    let subscribe = sent.iter().find(|t| t.contains("SUBSCRIBE")).unwrap();
    for topic in [
        "ticker.SOL_USDC_PERP",
        "trade.SOL_USDC_PERP",
        "bookTicker.SOL_USDC_PERP",
        "depth.SOL_USDC_PERP",
    ] {
        assert!(subscribe.contains(topic), "{topic}: {subscribe}");
    }
    assert!(sent.contains(&PING));
    assert_eq!(h.depth_reads, reads + 1, "fresh snapshot after reopen");
    h.message(old, TICKER);
    assert_eq!(h.engine.store().tickers()[0].last, 141.0);
    h.message(socket, TICKER);
    assert_eq!(h.engine.store().tickers()[0].last, 105.0);
}

fn pings_every_five_seconds_and_silence_retires_at_twenty(mut h: Harness) {
    let socket = h.open();
    let opened = h.now;
    h.advance(opened + 15_000.0);
    assert_eq!(h.pings(socket), 4, "open ping plus three cadence pings");
    h.advance(opened + 19_999.0);
    assert!(!h.closed.contains(&socket));
    h.advance(opened + 20_000.0);
    assert!(h.closed.contains(&socket));
    let sockets = h.sockets.len();
    let until = h.now + 1_000.0;
    h.advance(until);
    assert_eq!(h.sockets.len(), sockets + 1, "idle timeout retries");
}

#[test]
fn public_socket_pings_every_five_seconds_and_silence_retires_it_at_twenty() {
    pings_every_five_seconds_and_silence_retires_at_twenty(Harness::public());
}

#[test]
fn private_socket_pings_every_five_seconds_and_silence_retires_it_at_twenty() {
    pings_every_five_seconds_and_silence_retires_at_twenty(Harness::private());
}

fn pongs_keep_alive_and_unopened_socket_expires_at_ten(mut h: Harness) {
    let socket = h.open();
    let start = h.now;
    for step in 1..=8 {
        h.advance(start + f64::from(step) * 5_000.0);
        h.message(socket, PONG);
    }
    assert!(!h.closed.contains(&socket));

    let delay = h.close(socket);
    h.reconnect(delay);
    let pending = h.socket();
    let connecting = h.now;
    h.advance(connecting + 9_999.0);
    assert!(!h.closed.contains(&pending));
    h.advance(connecting + 10_000.0);
    assert!(h.closed.contains(&pending));
}

#[test]
fn public_pongs_keep_socket_alive_and_unopened_socket_expires_at_ten_seconds() {
    pongs_keep_alive_and_unopened_socket_expires_at_ten(Harness::public());
}

#[test]
fn private_pongs_keep_socket_alive_and_unopened_socket_expires_at_ten_seconds() {
    pongs_keep_alive_and_unopened_socket_expires_at_ten(Harness::private());
}

fn stable_traffic_resets_backoff(mut h: Harness) {
    for _ in 0..3 {
        let socket = h.open();
        let delay = h.close(socket);
        h.reconnect(delay);
    }
    let socket = h.open();
    let opened = h.now;
    for step in 1..=6 {
        h.advance(opened + f64::from(step) * 5_000.0);
        h.message(socket, PONG);
    }
    assert!((750.0..=1_000.0).contains(&h.close(socket)));
}

#[test]
fn stable_public_traffic_resets_backoff() {
    stable_traffic_resets_backoff(Harness::public());
}

#[test]
fn stable_private_traffic_resets_backoff() {
    stable_traffic_resets_backoff(Harness::private());
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::public_with(PublicFeedPolicy::Shared);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "SOL_USDC_PERP".into(),
    }));
    let until = h.now + 600_000.0;
    h.advance(until);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 0, "no supervision timers");
}
