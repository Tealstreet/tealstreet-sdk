//! Spot public-feed ownership retains native private balance cadence and auth.
#[path = "spot_support/mod.rs"]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_kraken::KrakenSpot;
use support::*;

struct Rig {
    engine: Engine<KrakenSpot>,
    public: Option<SocketId>,
    private: Vec<SocketId>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    reads: Vec<String>,
}
impl Rig {
    fn new(policy: PublicFeedPolicy) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(options()));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(NOW);
        let mut rig = Self {
            engine,
            public: None,
            private: vec![],
            timers: vec![],
            sent: vec![],
            reads: vec![],
        };
        rig.drain(NOW);
        assert_eq!(rig.engine.store().balance().total, 1355.);
        rig
    }
    fn drain(&mut self, now: f64) {
        for _ in 0..50 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut self.engine, now);
            if actions.is_empty() {
                return;
            }
            for action in actions {
                match action {
                    Action::Http(request) => {
                        self.reads.push(request.url.clone());
                        let body = if request.url.ends_with("AssetPairs") {
                            META
                        } else if request.url.ends_with("Ticker") {
                            TICKERS
                        } else {
                            for name in ["API-Key", "API-Sign"] {
                                assert!(
                                    request
                                        .headers
                                        .iter()
                                        .any(|(key, value)| key.eq_ignore_ascii_case(name)
                                            && !value.is_empty())
                                );
                            }
                            if request.url.ends_with("Balance") {
                                BALANCE
                            } else if request.url.ends_with("OpenOrders") {
                                OPEN
                            } else if request.url.ends_with("GetWebSocketsToken") {
                                r#"{"error":[],"result":{"token":"fresh-token","expires":900}}"#
                            } else {
                                panic!("unexpected request {}", request.url)
                            }
                        };
                        reply_at(&mut self.engine, now, &request, body);
                    }
                    Action::WsConnect {
                        socket,
                        channel,
                        url,
                        ..
                    } => {
                        match channel {
                            Some(SocketChannel::Public) => {
                                assert_eq!(url, "wss://ws.kraken.com/v2");
                                self.public = Some(socket);
                            }
                            Some(SocketChannel::Private) => {
                                assert_eq!(url, "wss://ws-auth.kraken.com/v2");
                                self.private.push(socket);
                            }
                            _ => panic!("unclassified spot socket"),
                        }
                        self.engine.handle(now, Event::WsOpened { socket });
                    }
                    Action::WsSend { socket, text } => self.sent.push((socket, text)),
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                    _ => {}
                }
            }
        }
        panic!("native spot fixture did not settle");
    }
    fn fire(&mut self, at: f64) {
        let mut fired = false;
        while let Some((timer, time)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, time)| *time <= at)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            fired = true;
            self.timers.retain(|(id, _)| *id != timer);
            self.engine.handle(time, Event::TimerFired { timer });
            self.drain(time);
        }
        assert!(fired, "missing native timer by {at}");
    }
    fn quote(&mut self, policy: PublicFeedPolicy, now: f64, price: f64) {
        if policy == PublicFeedPolicy::Direct {
            let text = serde_json::json!({"channel":"ticker","type":"update","data":[{"symbol":"BTC/USD","last":price}]}).to_string();
            self.engine.handle(
                now,
                Event::WsMessage {
                    socket: self.public.unwrap(),
                    text: &text,
                },
            );
        } else {
            self.engine.apply_public_feed(now, serde_json::from_value(serde_json::json!({"tickers":[{"symbol":"XBT/USD","last":price,"mark":price,"index":price}]})).unwrap()).unwrap();
        }
        assert_eq!(self.engine.store().ticker("XBT/USD").unwrap().last, price);
    }
}

#[test]
fn shared_and_direct_spot_quotes_preserve_private_snapshot_valuation_cadence() {
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        let raw = rig.engine.store().currency_balances().unwrap().to_vec();
        let metadata = rig.engine.store().markets().to_vec();
        let balance = rig.engine.store().balance().clone();
        rig.quote(policy, NOW + 100., 120.);
        assert_eq!(rig.engine.store().balance(), &balance);
        assert_eq!(rig.engine.store().currency_balances().unwrap(), raw);
        rig.fire(NOW + 10_000.);
        assert_eq!(rig.engine.store().balance().total, 1395.);
        rig.quote(policy, NOW + 11_000., 130.);
        if policy == PublicFeedPolicy::Shared {
            // Model a late native REST quote seed at the native Store boundary.
            rig.engine.store_mut().update_ticker("XBT/USD", |t| {
                t.last = 999.;
                t.mark = 999.;
            });
            rig.engine.handle(
                NOW + 12_000.,
                Event::WsMessage {
                    socket: rig.private[0],
                    text: r#"{"channel":"heartbeat"}"#,
                },
            );
            assert_eq!(rig.engine.store().ticker("XBT/USD").unwrap().last, 130.);
        }
        assert_eq!(rig.engine.store().balance().total, 1395.);
        rig.fire(NOW + 20_000.);
        assert_eq!(rig.engine.store().balance().total, 1415.);
        assert_eq!(rig.engine.store().currency_balances().unwrap(), raw);
        assert_eq!(rig.engine.store().markets(), metadata);
        assert_eq!(rig.engine.store().order("O-100").unwrap().remaining, 1.5);
        assert!(rig.engine.store().positions().is_empty());
    }
}

#[test]
fn non_direct_spot_suppresses_public_socket_but_preserves_private_token_refresh_and_ping() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        assert!(rig.public.is_none());
        assert_eq!(rig.private.len(), 1);
        assert!(rig.reads.iter().any(|url| url.ends_with("AssetPairs")));
        assert!(rig.reads.iter().any(|url| url.ends_with("Ticker")));
        let sent = rig.sent.len();
        for command in [
            Command::SubscribeOrderBook {
                symbol: "XBT/USD".into(),
            },
            Command::SubscribeCandles {
                symbol: "XBT/USD".into(),
                interval: safe_cex_engine::model::Interval::M1,
            },
        ] {
            rig.engine.handle(NOW + 100., Event::Command(command));
        }
        rig.drain(NOW + 100.);
        assert_eq!(rig.sent.len(), sent);
        rig.fire(NOW + 15_000.);
        assert!(
            rig.sent
                .iter()
                .any(|(id, text)| *id == rig.private[0] && text == r#"{"method":"ping"}"#)
        );
        let tokens = rig
            .reads
            .iter()
            .filter(|url| url.ends_with("GetWebSocketsToken"))
            .count();
        rig.engine.handle(
            NOW + 16_000.,
            Event::WsClosed {
                socket: rig.private[0],
                code: 1006,
            },
        );
        rig.drain(NOW + 16_000.);
        rig.fire(NOW + 17_000.);
        assert!(rig.public.is_none());
        assert_eq!(rig.private.len(), 2);
        assert_eq!(
            rig.reads
                .iter()
                .filter(|url| url.ends_with("GetWebSocketsToken"))
                .count(),
            tokens + 1
        );
        assert!(
            rig.sent
                .iter()
                .any(|(id, text)| *id == rig.private[1] && text.contains("fresh-token"))
        );
    }
}
