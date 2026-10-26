mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::socket_lifecycle::{
    Control, EncodedBatch, Epoch, FeedId, Interest, LifecycleEvent, SocketError,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Response, SocketChannel, SocketId, Store,
    TimerId, Venue,
};
use safe_cex_venue_woo::{Options, Pending, Woo};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default)]
struct Trace {
    refusals: Vec<(FeedId, SocketError)>,
    public_feed: Option<FeedId>,
}

struct Observed {
    native: Woo,
    trace: Rc<RefCell<Trace>>,
    encoder_failure: Rc<Cell<Option<SocketError>>>,
}

impl Venue for Observed {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn guard_order_mutations(&self) -> bool {
        self.native.guard_order_mutations()
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.native.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.native.start(cx)
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.native.on_command(cx, command)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.native.on_open(cx, socket)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.native.on_close(cx, socket, code)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.native.on_message(cx, socket, text)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.native.on_item(cx, pending, item)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.native.on_response(cx, pending, response)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.native.on_request_failed(cx, pending, error)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.native.on_timer(cx, timer)
    }
    fn socket_control(&self, store: &Store, feed: FeedId, private: bool, text: &str) -> Control {
        self.native.socket_control(store, feed, private, text)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match &event {
            LifecycleEvent::Opened(epoch) if self.trace.borrow().public_feed.is_none() => {
                self.trace.borrow_mut().public_feed = Some(epoch.feed());
            }
            LifecycleEvent::InterestsRefused { feed, error, .. } => {
                self.trace.borrow_mut().refusals.push((*feed, *error))
            }
            _ => (),
        }
        self.native.on_socket_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        epoch: Epoch,
        pending: Pending,
    ) -> Result<(), SocketError> {
        self.native.on_socket_interests_released(cx, epoch, pending)
    }
    fn encode_socket_interests(
        &self,
        store: &Store,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if let Some(error) = self.encoder_failure.get() {
            return Err(error);
        }
        self.native
            .encode_socket_interests(store, feed, subscribe, items, emit)
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.native.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.native.books()
    }
}

type TestEngine = Engine<Observed>;
struct Harness {
    engine: TestEngine,
    public: SocketId,
    private: SocketId,
    trace: Rc<RefCell<Trace>>,
    encoder_failure: Rc<Cell<Option<SocketError>>>,
}

fn response(engine: &mut TestEngine, request: &HttpRequest, now: f64, body: &str) {
    engine.handle(
        now,
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

impl Harness {
    fn connecting() -> Self {
        Self::connecting_with_topics(0)
    }
    fn connecting_with_topics(count: usize) -> Self {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let encoder_failure = Rc::new(Cell::new(None));
        let mut engine = safe_cex_engine::test_support::seeded_engine(Observed {
            native: Woo::new(Options {
                key: Some("fixture-key".into()),
                secret: Some("fixture-secret".into()),
                application_id: Some("fixture-app".into()),
                ..Default::default()
            }),
            trace: trace.clone(),
            encoder_failure: encoder_failure.clone(),
        });
        engine.start(0.);
        let request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        response(&mut engine, &request, 10., support::MARKETS);
        let actions: Vec<_> = engine.drain_actions().collect();
        let private = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .unwrap();
        let request = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains("/public/futures") => Some(r.clone()),
                _ => None,
            })
            .unwrap();
        if count > 0 {
            let mut rows = engine.store().markets().to_vec();
            let template = rows[0].clone();
            for n in 0..count {
                let mut row = template.clone();
                row.symbol = format!("P{n:04}USDT");
                row.base = format!("P{n:04}{}", "X".repeat(950));
                assert!(format!("PERP_{}_USDT@kline_1m", row.base).len() <= 1024);
                rows.push(row);
            }
            engine.store_mut().set_markets(rows);
            for n in 0..count {
                engine.handle(
                    11.,
                    Event::Command(Command::SubscribeCandles {
                        symbol: format!("P{n:04}USDT"),
                        interval: Interval::M1,
                    }),
                );
            }
        }
        response(&mut engine, &request, 20., support::TICKERS);
        let public = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => Some(socket),
                _ => None,
            })
            .unwrap();
        // Public opens first, so the observer records its genuine FeedId.
        Self {
            engine,
            public,
            private,
            trace,
            encoder_failure,
        }
    }
    fn ready() -> Self {
        Self::ready_with_topics(0)
    }
    fn ready_with_topics(count: usize) -> Self {
        let mut h = Self::connecting_with_topics(count);
        h.engine.handle(30., Event::WsOpened { socket: h.public });
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(31., Event::WsOpened { socket: h.private });
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            32.,
            Event::WsMessage {
                socket: h.private,
                text: r#"{"event":"auth","success":true}"#,
            },
        );
        h.engine.drain_actions().for_each(drop);
        h
    }
    fn command(&mut self, command: Command) {
        self.engine.handle(40., Event::Command(command));
    }
    fn peer_alive(&mut self, now: f64) {
        self.engine.handle(now, Event::WsMessage { socket: self.private, text: r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":123,"status":"NEW","side":"BUY","type":"LIMIT","quantity":0.02,"price":59000}}"# });
        assert!(
            self.engine.store().order("123").is_some(),
            "private sibling still publishes native execution"
        );
        assert!(
            !self
                .engine
                .drain_actions()
                .any(|a| matches!(a, Action::WsClose { socket } if socket == self.private))
        );
    }
    fn market(&mut self, symbol: &str, base: String) {
        let mut rows = self.engine.store().markets().to_vec();
        let mut market = rows[0].clone();
        market.symbol = symbol.into();
        market.base = base;
        rows.push(market);
        self.engine.store_mut().set_markets(rows);
    }
}

fn retry(actions: &[Action]) -> (TimerId, f64) {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (790. ..=1040.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("public Capacity must retain the native Closed/backoff recovery")
}
fn wake(actions: &[Action], now: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
            _ => None,
        })
        .expect("genuine handoff wake")
}
fn no_private_close(actions: &[Action], private: SocketId) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == private))
    );
}

#[test]
fn valid_frame_queue_capacity_recovers_public_and_preserves_private_peer() {
    let mut h = Harness::ready_with_topics(600);
    let public_feed = h.trace.borrow().public_feed.unwrap();
    let original = h.engine.store().markets().to_vec();
    let mut changed = original.clone();
    for row in &mut changed {
        if row.symbol.starts_with('P') {
            row.base.replace_range(..1, "Q");
        }
    }
    h.engine.store_mut().set_markets(changed);
    h.market("NEWUSDT", "NEW".into());
    h.command(Command::SubscribeCandles {
        symbol: "NEWUSDT".into(),
        interval: Interval::M1,
    });
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert_eq!(
        h.trace.borrow().refusals,
        [(public_feed, SocketError::Capacity)],
        "single genuine valid-key replacement exceeds queued frame bytes"
    );
    no_private_close(&actions, h.private);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }))
    );
    let (timer, at) = retry(&actions);
    h.peer_alive(40.);
    // Restore the original valid metadata and remove only the new native ref.
    h.engine.store_mut().set_markets(original);
    h.command(Command::UnsubscribeCandles {
        symbol: "NEWUSDT".into(),
        interval: Interval::M1,
    });
    assert!(
        !h.engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
    h.engine.handle(at, Event::TimerFired { timer });
    let socket = h
        .engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(socket),
            _ => None,
        })
        .expect("shared native replacement");
    assert_ne!(socket, h.public);
    h.engine.handle(at, Event::WsOpened { socket });
    let actions: Vec<_> = h.engine.drain_actions().collect();
    no_private_close(&actions, h.private);
    let frames: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        frames
            .iter()
            .filter(|text| text.contains("@kline_1m"))
            .count(),
        600,
        "all healthy native candle refs survive"
    );
    assert_eq!(
        frames
            .iter()
            .filter(|text| text.contains("@trades"))
            .count(),
        600
    );
    h.engine.handle(at + 1., Event::WsMessage { socket, text: r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":"61000","volume":5,"amount":305000}]}"# });
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 61000.);
    h.peer_alive(at + 1.);
}

#[test]
fn valid_continuation_pressure_uses_one_shared_retry_and_keeps_private_alive() {
    let mut h = Harness::ready();
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    let mut actions: Vec<_> = h.engine.drain_actions().collect();
    let old = wake(&actions, 40.);
    for n in 0..512 {
        h.command(if n % 2 == 0 {
            Command::SubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
            }
        } else {
            Command::UnsubscribeCandles {
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
            }
        });
        actions = h.engine.drain_actions().collect();
        if actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
        {
            break;
        }
    }
    assert!(
        h.trace.borrow().refusals.is_empty(),
        "this is synchronous genuine continuation credit refusal"
    );
    no_private_close(&actions, h.private);
    let (timer, at) = retry(&actions);
    assert_eq!(
        actions
            .iter()
            .filter(
                |a| matches!(a, Action::SetTimer { at_ms, .. } if (790. ..=1040.).contains(at_ms))
            )
            .count(),
        1
    );
    h.engine.handle(40., Event::TimerFired { timer: old });
    assert!(
        h.engine.drain_actions().next().is_none(),
        "superseded wake cannot issue work"
    );
    h.peer_alive(40.);
    h.engine.handle(at, Event::TimerFired { timer });
    let socket = h
        .engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(socket),
            _ => None,
        })
        .unwrap();
    h.engine.handle(at, Event::WsOpened { socket });
    let actions: Vec<_> = h.engine.drain_actions().collect();
    let timer = wake(&actions, at);
    h.engine.handle(at, Event::TimerFired { timer });
    let requests: Vec<_> = h
        .engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("/orderbook/PERP_BTC_USDT"));
    h.peer_alive(at);
}

#[test]
fn permanent_invalid_interest_stops_only_public_and_private_remains_live() {
    let mut h = Harness::ready();
    h.market("BADUSDT", "X".repeat(1024));
    h.command(Command::SubscribeCandles {
        symbol: "BADUSDT".into(),
        interval: Interval::M1,
    });
    assert_eq!(
        h.trace.borrow().refusals,
        [(
            h.trace.borrow().public_feed.unwrap(),
            SocketError::InvalidInterest
        )]
    );
    let actions: Vec<_> = h.engine.drain_actions().collect();
    no_private_close(&actions, h.private);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::SetTimer { .. } | Action::Http(_) | Action::WsSend { .. }
    )));
    h.peer_alive(41.);
}

#[test]
fn inherited_initial_ready_encoder_capacity_is_fail_closed_not_venue_recovery() {
    let mut h = Harness::connecting();
    h.encoder_failure.set(Some(SocketError::Capacity));
    h.engine.handle(30., Event::WsOpened { socket: h.public });
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        h.trace.borrow().refusals.is_empty(),
        "Ready replay fails before InterestsRefused callback"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
            .count(),
        1
    );
    no_private_close(&actions, h.private);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }))
    );
    for action in &actions {
        if let Action::SetTimer { timer, .. } = action {
            assert!(
                actions.iter().any(
                    |a| matches!(a, Action::ClearTimer { timer: cleared } if cleared == timer)
                ),
                "opening watchdog is exactly canceled; no live recovery timer"
            );
        }
    }
    h.encoder_failure.set(None);
    h.engine.handle(31., Event::WsOpened { socket: h.private });
    h.engine.drain_actions().for_each(drop);
    h.engine.handle(
        32.,
        Event::WsMessage {
            socket: h.private,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    h.engine.drain_actions().for_each(drop);
    h.peer_alive(33.);
}

#[test]
fn inherited_private_ready_encoder_capacity_stops_only_private_and_public_remains_live() {
    let mut h = Harness::connecting();
    h.engine.handle(30., Event::WsOpened { socket: h.public });
    h.engine.drain_actions().for_each(drop);
    h.engine.handle(31., Event::WsOpened { socket: h.private });
    h.engine.drain_actions().for_each(drop);
    h.encoder_failure.set(Some(SocketError::Capacity));
    h.engine.handle(
        32.,
        Event::WsMessage {
            socket: h.private,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        h.trace.borrow().refusals.is_empty(),
        "private Ready replay also precedes InterestsRefused"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == h.private))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }))
    );
    for action in &actions {
        if let Action::SetTimer { timer, .. } = action {
            assert!(
                actions.iter().any(
                    |a| matches!(a, Action::ClearTimer { timer: cleared } if cleared == timer)
                ),
                "no live private retry timer after inherited fail-closed"
            );
        }
    }
    h.encoder_failure.set(None);
    h.engine.handle(33., Event::WsMessage { socket: h.public, text: r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDT","close":"61000","volume":5,"amount":305000}]}"# });
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 61000.);
    assert!(
        !h.engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}
