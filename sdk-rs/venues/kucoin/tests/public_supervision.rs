//! Shared/Disabled public feeds never open, so KuCoin must not fetch bullets or supervise them.
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, PublicFeedPolicy, Response, SocketChannel, SocketId,
    TimerId, Venue,
};
mod support;
use safe_cex_venue_kucoin::Kucoin;
use support::credentials::Options;

const FUTURES_MARKETS: &str = r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"XBT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"maxOrderQty":1000000,"maxLeverage":125,"multiplier":0.001,"expireDate":null,"isInverse":false}]}"#;
const FUTURES_TICKERS: &str = r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100","bestBidPrice":"99","bestAskPrice":"101","size":12}]}"#;
const BULLET: &str = r#"{"code":"200000","data":{"token":"token","instanceServers":[{"endpoint":"wss://example.test/ws","pingInterval":18000,"pingTimeout":10000}]}}"#;

struct Sim<V: Venue> {
    e: Engine<V>,
    now: f64,
    http: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    public_bullets: usize,
    private_bullets: usize,
    connects: Vec<(SocketId, Option<SocketChannel>)>,
    closed: Vec<SocketId>,
    fired: usize,
}

impl<V: Venue> Sim<V> {
    fn start(mut e: Engine<V>, policy: PublicFeedPolicy) -> Self {
        e.set_public_feed_policy(policy).unwrap();
        let mut sim = Self {
            e,
            now: 1_700_000_000_000.0,
            http: vec![],
            timers: vec![],
            public_bullets: 0,
            private_bullets: 0,
            connects: vec![],
            closed: vec![],
            fired: 0,
        };
        sim.e.start(sim.now);
        sim.collect();
        sim
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            match action {
                Action::Http(r) => {
                    self.public_bullets += usize::from(r.url.ends_with("/bullet-public"));
                    self.private_bullets += usize::from(r.url.ends_with("/bullet-private"));
                    self.http.push(r);
                }
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect {
                    socket, channel, ..
                } => self.connects.push((socket, channel)),
                Action::WsClose { socket } => self.closed.push(socket),
                _ => {}
            }
        }
    }
    fn reply(&mut self, path: &str, body: &str) {
        let i = self
            .http
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}"));
        let request = self.http.remove(i).id;
        let response = Response {
            status: 200,
            headers: "",
            body,
        };
        self.e
            .handle(self.now, Event::HttpResponse { request, response });
        self.collect();
    }
    /// Answers every bullet like a reachable REST API and fires each timer due by `until`.
    fn run(&mut self, until: f64) {
        loop {
            while self.http.iter().any(|r| r.url.contains("/bullet-")) {
                self.reply("/bullet-", BULLET);
            }
            let Some(&(timer, at)) = self
                .timers
                .iter()
                .filter(|(_, at)| *at <= until)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            else {
                break;
            };
            self.timers.retain(|(t, _)| *t != timer);
            self.now = at;
            self.e.handle(at, Event::TimerFired { timer });
            self.collect();
            self.fired += 1;
        }
        self.now = until;
    }
    fn private_socket(&self) -> SocketId {
        self.connects
            .iter()
            .find(|(_, channel)| *channel == Some(SocketChannel::Private))
            .expect("private connect")
            .0
    }
}

fn futures(policy: PublicFeedPolicy, options: Options) -> Sim<Kucoin> {
    let mut sim = Sim::start(support::credentials::engine(options, Kucoin::new), policy);
    sim.reply("/contracts/active", FUTURES_MARKETS);
    sim.reply("/allTickers", FUTURES_TICKERS);
    sim
}

fn credentials() -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        ..Default::default()
    }
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut sim = futures(
        PublicFeedPolicy::Shared,
        Options {
            public_data_only: true,
            ..Default::default()
        },
    );
    assert_eq!(sim.e.store().markets().len(), 1, "public startup completed");
    let until = sim.now + 600_000.0;
    sim.run(until);
    assert_eq!(sim.public_bullets, 0, "no public bullet POSTs");
    assert!(sim.connects.is_empty(), "no connects: {:?}", sim.connects);
    assert!(sim.closed.is_empty(), "no closes: {:?}", sim.closed);
    assert_eq!(sim.fired, 0, "no supervision timers fire");
}

#[test]
fn private_socket_stays_supervised_under_shared_public_feed() {
    let mut sim = futures(PublicFeedPolicy::Shared, credentials());
    let until = sim.now + 1.0;
    sim.run(until);
    assert_eq!(sim.public_bullets, 0, "no public bullet POSTs");
    assert_eq!(sim.private_bullets, 1);
    let private = sim.private_socket();
    let until = sim.now + 12_000.0;
    sim.run(until);
    assert!(
        sim.closed.contains(&private),
        "unopened private socket retires"
    );
    assert_eq!(sim.public_bullets, 0, "no public bullet POSTs");
    assert!(
        sim.private_bullets >= 2,
        "private reconnects with a fresh bullet"
    );
}

#[cfg(feature = "spot")]
mod spot {
    use super::*;
    use safe_cex_venue_kucoin::spot::KucoinSpot;

    const SPOT_MARKETS: &str = r#"{"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"100"}]}"#;
    const SPOT_TICKERS: &str = r#"{"code":"200000","data":{"time":1700000000000,"ticker":[{"symbol":"BTC-USDT","last":"100","buy":"99","sell":"101"}]}}"#;

    fn spot(policy: PublicFeedPolicy, options: Options) -> Sim<KucoinSpot> {
        let mut sim = Sim::start(
            support::credentials::engine(options, KucoinSpot::new),
            policy,
        );
        sim.reply("/api/v2/symbols", SPOT_MARKETS);
        sim.reply("/allTickers", SPOT_TICKERS);
        sim
    }

    #[test]
    fn spot_suppressed_public_feed_is_not_supervised() {
        let mut sim = spot(
            PublicFeedPolicy::Shared,
            Options {
                public_data_only: true,
                ..Default::default()
            },
        );
        assert_eq!(sim.e.store().markets().len(), 1, "public startup completed");
        let until = sim.now + 600_000.0;
        sim.run(until);
        assert_eq!(sim.public_bullets, 0, "no public bullet POSTs");
        assert!(sim.connects.is_empty(), "no connects: {:?}", sim.connects);
        assert!(sim.closed.is_empty(), "no closes: {:?}", sim.closed);
        assert_eq!(sim.fired, 0, "no supervision timers fire");
    }

    #[test]
    fn spot_private_socket_stays_supervised_under_shared_public_feed() {
        let mut sim = spot(PublicFeedPolicy::Shared, credentials());
        let until = sim.now + 1.0;
        sim.run(until);
        assert_eq!(sim.public_bullets, 0, "no public bullet POSTs");
        assert_eq!(sim.private_bullets, 1);
        let private = sim.private_socket();
        let until = sim.now + 12_000.0;
        sim.run(until);
        assert!(
            sim.closed.contains(&private),
            "unopened private socket retires"
        );
        assert_eq!(sim.public_bullets, 0, "no public bullet POSTs");
        assert!(
            sim.private_bullets >= 2,
            "private reconnects with a fresh bullet"
        );
    }
}
