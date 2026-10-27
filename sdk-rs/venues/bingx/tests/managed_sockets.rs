//! Exercises the real BingX binding, provider and captured socket identities.
mod support;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, PublicFeedPolicy, Response, SocketId};
use safe_cex_venue_bingx::Bingx;
use support::Options;

const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;

struct Harness {
    engine: Engine<Bingx>,
    now: f64,
    actions: Vec<Action>,
    public: Option<SocketId>,
    private: Option<SocketId>,
}
impl Harness {
    fn new(private: bool, suppressed: bool) -> Self {
        let mut engine = support::engine(Options {
            public_data_only: !private,
            key: private.then(|| "fixture-key".into()),
            secret: private.then(|| "fixture-secret".into()),
            ..Default::default()
        });
        if suppressed {
            engine
                .set_public_feed_policy(PublicFeedPolicy::Disabled)
                .unwrap();
        }
        engine.start(0.);
        let mut h = Self {
            engine,
            now: 0.,
            actions: vec![],
            public: None,
            private: None,
        };
        h.collect();
        let markets = h.request("/contracts");
        h.reply(&markets, MARKETS);
        if private {
            let key = h.request("/user/auth/userDataStream");
            h.reply(&key, r#"{"listenKey":"key-one"}"#);
            let orders = h.request("/trade/openOrders");
            h.reply(&orders, r#"{"code":0,"data":[]}"#);
        }
        h.actions.retain(|a| matches!(a, Action::Http(_)));
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            if let Action::WsConnect { socket, url, .. } = &a {
                if url.contains("listenKey=") {
                    self.private = Some(*socket);
                } else {
                    self.public = Some(*socket);
                }
            }
            self.actions.push(a);
        }
    }
    fn event(&mut self, event: Event<'_>) -> Vec<Action> {
        self.now += 1.;
        self.engine.handle(self.now, event);
        self.collect();
        std::mem::take(&mut self.actions)
    }
    fn request(&mut self, path: &str) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r) if r.url.contains(path)))
            .expect("real released request");
        match self.actions.remove(i) {
            Action::Http(r) => r,
            _ => unreachable!(),
        }
    }
    fn reply(&mut self, r: &HttpRequest, body: &str) {
        self.now += 1.;
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
}
fn sends(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
#[test]
fn duplicate_public_open_does_not_replay_or_reset_supervision() {
    let mut h = Harness::new(false, false);
    let socket = h.public.unwrap();
    let opened = h.event(Event::WsOpened { socket });
    assert!(
        sends(&opened, socket)
            .iter()
            .any(|s| s.contains("market.contracts"))
    );
    assert!(sends(&opened, socket).contains(&"Ping"));
    let duplicate = h.event(Event::WsOpened { socket });
    assert!(
        duplicate.is_empty(),
        "duplicate open cannot replay native work: {duplicate:?}"
    );
}
#[test]
fn duplicate_private_open_does_not_repeat_ping_or_account_refresh() {
    let mut h = Harness::new(true, false);
    let socket = h.private.unwrap();
    let opened = h.event(Event::WsOpened { socket });
    assert_eq!(sends(&opened, socket), vec!["Ping"]);
    assert!(
        opened
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("openOrders")))
    );
    let duplicate = h.event(Event::WsOpened { socket });
    assert!(
        duplicate.is_empty(),
        "duplicate private open owns no new work: {duplicate:?}"
    );
}
#[test]
fn suppressed_public_still_allows_the_real_private_feed() {
    let mut h = Harness::new(true, true);
    assert!(h.public.is_none());
    let socket = h.private.unwrap();
    assert_eq!(
        sends(&h.event(Event::WsOpened { socket }), socket),
        vec!["Ping"]
    );
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket,
                text: "Ping"
            }),
            socket
        ),
        vec!["Pong"]
    );
}
#[test]
fn retiring_public_does_not_retire_private_and_old_open_is_inert() {
    let mut h = Harness::new(true, false);
    let public = h.public.unwrap();
    let private = h.private.unwrap();
    h.event(Event::WsOpened { socket: public });
    h.event(Event::WsOpened { socket: private });
    let closed = h.event(Event::WsClosed {
        socket: public,
        code: 1006,
    });
    assert!(closed.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(h.event(Event::WsOpened { socket: public }).is_empty());
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket: private,
                text: "Ping"
            }),
            private
        ),
        vec!["Pong"]
    );
}

fn fire(h: &mut Harness, timer: safe_cex_engine::TimerId, at: f64) -> Vec<Action> {
    h.now = at;
    h.engine.handle(at, Event::TimerFired { timer });
    h.collect();
    std::mem::take(&mut h.actions)
}
fn retry(actions: &[Action], now: f64) -> (safe_cex_engine::TimerId, f64) {
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750. ..=1000.).contains(&(*at_ms - now)) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "one genuine engine backoff");
    timers[0]
}
#[test]
fn private_reconnect_gets_a_fresh_key_and_old_socket_cannot_revive() {
    let mut h = Harness::new(true, false);
    let public = h.public.unwrap();
    let old = h.private.unwrap();
    h.event(Event::WsOpened { socket: public });
    h.event(Event::WsOpened { socket: old });
    let retired = h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let (timer, at) = retry(&retired, h.now);
    assert!(
        retired
            .iter()
            .all(|a| !matches!(a,Action::Http(r) if r.url.contains("userDataStream")))
    );
    assert!(h.event(Event::WsOpened { socket: old }).is_empty());
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket: public,
                text: "Ping"
            }),
            public
        ),
        vec!["Pong"]
    );
    let reconnect = fire(&mut h, timer, at);
    assert!(
        reconnect
            .iter()
            .all(|a| !matches!(a, Action::WsConnect { .. }))
    );
    h.actions = reconnect;
    let key = h.request("userDataStream");
    assert_eq!(key.method, safe_cex_engine::Method::Post);
    h.reply(&key, r#"{"listenKey":"key-two"}"#);
    let current = h.private.unwrap();
    assert_ne!(old, current);
    h.actions.clear();
    assert_eq!(
        sends(&h.event(Event::WsOpened { socket: current }), current),
        vec!["Ping"]
    );
    assert!(
        h.event(Event::WsClosed {
            socket: old,
            code: 1006
        })
        .is_empty()
    );
    assert!(
        h.event(Event::WsMessage {
            socket: old,
            text: "listenKeyExpired"
        })
        .is_empty()
    );
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket: current,
                text: "Ping"
            }),
            current
        ),
        vec!["Pong"]
    );
    assert!(fire(&mut h, timer, at + 100.).is_empty());
}
#[test]
fn current_interests_and_refcounts_survive_reconnect_without_replaying_removed_books() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new(false, false);
    let old = h.public.unwrap();
    for _ in 0..2 {
        h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }));
    }
    h.event(Event::Command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
    }));
    let opened = h.event(Event::WsOpened { socket: old });
    let depth = |s: &str| s.contains("market.depth.");
    assert_eq!(sends(&opened, old).iter().filter(|s| depth(s)).count(), 1);
    assert!(
        h.event(Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into()
        }))
        .is_empty()
    );
    let retired = h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let (timer, at) = retry(&retired, h.now);
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let next = fire(&mut h, timer, at);
    let current = h.public.unwrap();
    assert_ne!(current, old);
    assert_eq!(
        next.iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        1
    );
    let opened = h.event(Event::WsOpened { socket: current });
    assert_eq!(
        sends(&opened, current).iter().filter(|s| depth(s)).count(),
        0
    );
    assert!(
        sends(&opened, current)
            .iter()
            .any(|s| s.contains("market.kline."))
    );
    let subscribed = h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    assert_eq!(
        sends(&subscribed, current)
            .iter()
            .filter(|s| depth(s))
            .count(),
        1
    );
    assert!(h.event(Event::WsOpened { socket: old }).is_empty());
}
#[test]
fn nonfinite_socket_deliveries_preserve_full_store_and_current_epoch() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::new(false, false);
        let socket = h.public.unwrap();
        let daily = h.request("/quote/ticker");
        h.reply(&daily,r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#);
        let premium = h.request("/quote/premiumIndex");
        h.reply(&premium,r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#);
        h.actions.clear();
        h.event(Event::WsOpened { socket });
        let before = h.engine.snapshot_frame(100.).to_vec();
        for event in [
            Event::WsOpened { socket },
            Event::WsMessage {
                socket,
                text: r#"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"BTC-USDT","tradePrice":"99999"}]}}"#,
            },
            Event::WsClosed { socket, code: 1006 },
        ] {
            h.engine.handle(now, event);
            let actions: Vec<_> = h.engine.drain_actions().collect();
            assert!(
                actions.is_empty(),
                "nonfinite ingress produces no work: {actions:?}"
            );
            assert_eq!(h.engine.snapshot_frame(100.), before);
        }
        assert_eq!(
            sends(
                &h.event(Event::WsMessage {
                    socket,
                    text: "Ping"
                }),
                socket
            ),
            vec!["Pong"]
        );
    }
}
#[test]
fn real_valid_interest_frame_pressure_retires_only_public_and_recovers_desired_demand() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new(true, false);
    let public = h.public.unwrap();
    let private = h.private.unwrap();
    let template = h.engine.store().markets()[0].clone();
    let mut markets = vec![template.clone()];
    for i in 0..2100 {
        let mut m = template.clone();
        m.symbol = format!("COIN{i}USDT");
        m.id = format!("COIN{i}-USDT");
        markets.push(m);
    }
    h.engine.store_mut().set_markets(markets);
    h.event(Event::WsOpened { socket: public });
    h.event(Event::WsOpened { socket: private });
    for i in 0..2100 {
        let sent = h.event(Event::Command(Command::SubscribeOrderBook {
            symbol: format!("COIN{i}USDT"),
        }));
        assert_eq!(sends(&sent, public).len(), 1);
    }
    h.event(Event::WsMessage {socket:public,text:r#"{"dataType":"market.depth.COIN0-USDT.step0.level100","data":{"bids":[{"price":"10","volume":"2"}],"asks":[{"price":"11","volume":"3"}]}}"#});
    assert_eq!(h.engine.book("COIN0USDT").unwrap().bids(), [(10., 2.)]);
    let mut changed = h.engine.store().markets().to_vec();
    for m in &mut changed[1..] {
        m.id = format!("remapped-{}", m.id);
    }
    h.engine.store_mut().set_markets(changed);
    let pressure = h.event(Event::Command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
    }));
    assert!(
        pressure
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket} if *socket==public))
    );
    assert!(
        pressure
            .iter()
            .all(|a| !matches!(a,Action::WsClose{socket} if *socket==private))
    );
    assert!(pressure.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(e)) if e.message.contains("Capacity"))));
    assert!(h.engine.book("COIN0USDT").is_none());
    let (timer, at) = retry(&pressure, h.now);
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket: private,
                text: "Ping"
            }),
            private
        ),
        vec!["Pong"]
    );
    fire(&mut h, timer, at);
    let current = h.public.unwrap();
    assert_ne!(current, public);
    let opened = h.event(Event::WsOpened { socket: current });
    let frames = sends(&opened, current);
    assert_eq!(
        frames
            .iter()
            .filter(|s| s.contains("market.depth.remapped-"))
            .count(),
        2100
    );
    assert_eq!(
        frames
            .iter()
            .filter(|s| s.contains("market.kline."))
            .count(),
        1
    );
    assert_eq!(frames.iter().filter(|s| **s == "Ping").count(), 1);
    assert!(h.event(Event::WsOpened { socket: public }).is_empty());
}

#[test]
fn acquisition_retry_uses_its_captured_not_before_and_rejects_late_key_results() {
    let mut h = Harness::new(true, false);
    let old = h.private.unwrap();
    h.event(Event::WsOpened { socket: old });
    let closed = h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let (reconnect, at) = retry(&closed, h.now);
    h.actions = fire(&mut h, reconnect, at);
    let first = h.request("userDataStream");
    let failed = h.event(Event::HttpFailed {
        request: first.id,
        error: "key service unavailable",
    });
    let owed: Vec<_> = failed
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 30_000. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(owed.len(), 1);
    h.actions.clear();
    h.reply(&first, r#"{"listenKey":"late-retired-key"}"#);
    assert!(
        h.actions.is_empty(),
        "retired acquisition cannot connect or rearm"
    );
    let (early, bound) = owed[0];
    let actions = fire(&mut h, early, bound - 1.);
    assert!(actions.iter().all(|a| !matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::WsConnect { .. }
    )));
    let rearmed: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == bound => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(rearmed.len(), 1);
    assert_ne!(rearmed[0], early);
    assert!(fire(&mut h, early, bound - 0.5).is_empty());
    h.actions = fire(&mut h, rearmed[0], bound);
    let fresh = h.request("userDataStream");
    assert_ne!(fresh.id, first.id);
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a,Action::Http(r) if r.url.contains("userDataStream")))
            .count(),
        0
    );
    h.reply(&fresh, r#"{"listenKey":"key-after-bound"}"#);
    let current = h.private.unwrap();
    assert_ne!(current, old);
    h.actions.clear();
    assert!(fire(&mut h, rearmed[0], bound + 0.5).is_empty());
    assert_eq!(
        sends(&h.event(Event::WsOpened { socket: current }), current),
        vec!["Ping"]
    );
}

#[test]
fn nonfinite_acquisition_retry_recovers_once_on_a_finite_poll_at_its_original_bound() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::new(true, true);
        for (path, body) in [
            (
                "/positionSide/dual",
                r#"{"code":0,"data":{"dualSidePosition":false}}"#,
            ),
            ("/user/positions", r#"{"code":0,"data":[]}"#),
            (
                "/user/balance",
                r#"{"code":0,"data":[{"asset":"USDT","balance":"1000","availableMargin":"1000","usedMargin":"0","unrealizedProfit":"0"}]}"#,
            ),
            (
                "/quote/ticker",
                r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#,
            ),
            (
                "/quote/premiumIndex",
                r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#,
            ),
        ] {
            let request = h.request(path);
            h.reply(&request, body);
        }
        let old = h.private.unwrap();
        h.event(Event::WsOpened { socket: old });
        let closed = h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        let (reconnect, at) = retry(&closed, h.now);
        h.actions = fire(&mut h, reconnect, at);
        let key = h.request("userDataStream");
        let failed = h.event(Event::HttpFailed {
            request: key.id,
            error: "key unavailable",
        });
        let bound = h.now + 30_000.;
        let owed: Vec<_> = failed
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == bound => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(owed.len(), 1);
        let changed = h.event(Event::PollScaleChanged { scale: 1.5 });
        let poll_at = h.now + 3_000.;
        let polls: Vec<_> = changed
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == poll_at => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(polls.len(), 1);
        let before = h.engine.snapshot_frame(100.).to_vec();
        assert!(fire(&mut h, owed[0], invalid).is_empty());
        assert_eq!(h.engine.snapshot_frame(100.), before);
        assert!(fire(&mut h, owed[0], poll_at - 1.).is_empty());
        let recovered = fire(&mut h, polls[0], poll_at);
        assert!(recovered.iter().all(|a| !matches!(a,
            Action::Http(r) if r.url.contains("userDataStream"))));
        let wakes: Vec<_> = recovered
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == bound => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(
            wakes.len(),
            1,
            "finite poll must rearm the consumed retry: {recovered:?}"
        );
        assert_ne!(wakes[0], owed[0]);
        let next_poll: Vec<_> = recovered
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == poll_at + 3_000. => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(next_poll.len(), 1);
        let again = fire(&mut h, next_poll[0], poll_at + 3_000.);
        assert!(again.iter().all(|a| !matches!(a,
            Action::SetTimer { at_ms, .. } if *at_ms == bound)));
        assert!(again.iter().all(|a| !matches!(a,
            Action::Http(r) if r.url.contains("userDataStream"))));
        let early = fire(&mut h, wakes[0], bound - 1.);
        let current: Vec<_> = early
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == bound => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(current.len(), 1);
        assert!(early.iter().all(|a| !matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::WsConnect { .. }
        )));
        assert!(fire(&mut h, wakes[0], bound - 0.5).is_empty());
        h.actions = fire(&mut h, current[0], bound);
        let fresh = h.request("userDataStream");
        assert_ne!(fresh.id, key.id);
        assert!(h.actions.iter().all(|a| !matches!(a,
            Action::Http(r) if r.url.contains("userDataStream"))));
        h.actions.clear();
        assert!(fire(&mut h, current[0], bound + 0.5).is_empty());
        h.reply(&fresh, r#"{"listenKey":"finite-recovery"}"#);
        let socket = h.private.unwrap();
        assert_ne!(socket, old);
        h.actions.clear();
        assert_eq!(
            sends(&h.event(Event::WsOpened { socket }), socket),
            vec!["Ping"]
        );
    }
}
