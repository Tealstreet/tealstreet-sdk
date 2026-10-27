mod support;

use safe_cex_engine::{Action, Command, Event, SocketChannel, SocketId, Venue};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::{Material, Prepared, TestEngine};

const TICK: &str = r#"{"topic":"instrumentInfo.all","data":[{"s":"BTC-USDT","p":"60000","xp":"60001","pr":"0.0123","o":"20","fr":"-0.0002","v":"4","to":"240000"}]}"#;

fn connecting(private: bool) -> (TestEngine, SocketId, Option<SocketId>) {
    let mut engine = if private {
        support::private_engine(Options::default(), &Material::default())
    } else {
        safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
            public_data_only: true,
            ..Default::default()
        }))
    };
    engine.start(0.0);
    let catalogue = support::requests(&mut engine, 0.0).remove(0);
    support::response(&mut engine, &catalogue, 10.0, support::MARKETS);
    let actions: Vec<_> = engine.prepared(10.0).collect();
    let socket = |role| {
        actions.iter().find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(channel),
                ..
            } if *channel == role => Some(*socket),
            _ => None,
        })
    };
    (
        engine,
        socket(SocketChannel::Public).expect("genuine public connect"),
        socket(SocketChannel::Private),
    )
}

#[test]
fn actual_apex_binding_declares_managed_socket_custody() {
    assert!(<ApexOmni as Venue>::managed_sockets());
}

#[test]
fn a_duplicate_public_open_cannot_replay_topics_or_reset_supervision() {
    let (mut engine, public, _) = connecting(false);
    engine.handle(20.0, Event::WsOpened { socket: public });
    let first: Vec<_> = engine.prepared(20.0).collect();
    assert!(first.iter().any(|a| matches!(a,
        Action::WsSend { socket, text } if *socket == public && text.contains("instrumentInfo.all"))));
    engine.handle(21.0, Event::WsOpened { socket: public });
    assert!(
        engine.prepared(21.0).next().is_none(),
        "duplicate public Opened must not replay frames or replace timers"
    );
}

#[test]
fn a_duplicate_private_open_cannot_repeat_login_subscriptions_or_reads() {
    let (mut engine, _, private) = connecting(true);
    let private = private.expect("installed Primary opens private feed");
    engine.handle(20.0, Event::WsOpened { socket: private });
    let first: Vec<_> = engine.prepared(20.0).collect();
    let sends: Vec<_> = first
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket, text } if *socket == private => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(sends[0].contains(r#""op":"login""#));
    assert_eq!(
        sends[1],
        r#"{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}"#
    );
    assert_eq!(sends[2], r#"{"op":"ping"}"#);
    engine.handle(21.0, Event::WsOpened { socket: private });
    assert!(
        engine.prepared(21.0).next().is_none(),
        "duplicate private Opened must not grant another login or refresh"
    );
}

#[test]
fn a_connecting_public_epoch_cannot_publish_before_its_first_open() {
    let (mut engine, public, _) = connecting(false);
    let before = engine.store().tickers()[0].clone();
    engine.handle(
        11.0,
        Event::WsMessage {
            socket: public,
            text: TICK,
        },
    );
    assert_eq!(engine.store().tickers()[0].last, before.last);
    assert!(engine.prepared(11.0).next().is_none());
    engine.handle(20.0, Event::WsOpened { socket: public });
    engine.prepared(20.0).for_each(drop);
    engine.handle(
        21.0,
        Event::WsMessage {
            socket: public,
            text: TICK,
        },
    );
    assert_eq!(engine.store().tickers()[0].last, 60000.0);
}

#[test]
fn connecting_book_and_candle_demand_is_sent_on_the_first_open() {
    let (mut engine, public, _) = connecting(false);
    for command in [
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        },
    ] {
        engine.handle(11.0, Event::Command(command));
        assert!(
            engine
                .prepared(11.0)
                .all(|a| !matches!(a, Action::WsSend { .. }))
        );
    }
    engine.handle(20.0, Event::WsOpened { socket: public });
    let sends: Vec<_> = engine
        .prepared(20.0)
        .filter_map(|a| match a {
            Action::WsSend { socket, text } if socket == public => Some(text),
            _ => None,
        })
        .collect();
    assert!(sends.iter().any(|t| t.contains("orderBook200.H.BTCUSDT")));
    assert!(sends.iter().any(|t| t.contains("candle.1.BTCUSDT")));
    assert!(!sends.iter().any(|t| t.contains("recentlyTrade")));
}

const PROOF: &str = r#"{"topic":"ws_zk_accounts_v3","type":"snapshot","contents":{"orders":[{"id":"proof-order","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"OPEN","size":"0.001","price":"50000","cumSuccessFillSize":"0"}]}}"#;

#[test]
fn first_valid_account_data_authenticates_and_publishes_exactly_once() {
    let (mut engine, _, private) = connecting(true);
    let socket = private.unwrap();
    engine.handle(20.0, Event::WsOpened { socket });
    let watch = engine
        .prepared(20.0)
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 5020.0,
            } => Some(timer),
            _ => None,
        })
        .expect("actual authentication watchdog");
    engine.handle(
        21.0,
        Event::WsMessage {
            socket,
            text: PROOF,
        },
    );
    let actions: Vec<_> = engine.prepared(21.0).collect();
    assert_eq!(engine.store().orders().len(), 1);
    assert_eq!(engine.store().orders()[0].id, "proof-order");
    assert_eq!(engine.store().orders()[0].last_update, 21.0);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 1021.0))
            .count(),
        1,
        "the native order reducer must run once, not once before and once after authentication"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == watch))
    );
    engine.handle(10020.0, Event::TimerFired { timer: watch });
    assert!(engine.prepared(10020.0).next().is_none());
}

#[test]
fn malformed_first_account_proof_cannot_authenticate_or_publish_partial_rows() {
    let (mut engine, _, private) = connecting(true);
    let socket = private.unwrap();
    engine.handle(20.0, Event::WsOpened { socket });
    engine.prepared(20.0).for_each(drop);
    let before = engine.snapshot_frame(100.0).to_vec();
    let bad = PROOF.replace("}]}}", "},{}]}}");
    engine.handle(21.0, Event::WsMessage { socket, text: &bad });
    assert_eq!(engine.snapshot_frame(100.0), before);
    assert!(engine.prepared(21.0).next().is_none());
    engine.handle(
        22.0,
        Event::WsMessage {
            socket,
            text: r#"{"op":"login","success":true}"#,
        },
    );
    engine.prepared(22.0).for_each(drop);
    assert!(
        engine.store().orders().is_empty(),
        "an ordinary ACK must not replay a rejected proof"
    );
    engine.handle(
        23.0,
        Event::WsMessage {
            socket,
            text: PROOF,
        },
    );
    assert_eq!(engine.store().orders().len(), 1);
}

#[test]
fn nonfinite_current_socket_deliveries_are_full_state_and_action_inert() {
    for private in [false, true] {
        let (mut engine, public, account) = connecting(private);
        let socket = account.unwrap_or(public);
        engine.handle(20.0, Event::WsOpened { socket });
        engine.prepared(20.0).for_each(drop);
        let before = engine.snapshot_frame(100.0).to_vec();
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            engine.handle(
                now,
                Event::WsMessage {
                    socket,
                    text: if private { PROOF } else { TICK },
                },
            );
            engine.handle(now, Event::WsOpened { socket });
            engine.handle(now, Event::WsClosed { socket, code: 1006 });
            assert_eq!(engine.snapshot_frame(100.0), before);
            assert!(engine.drain_actions().next().is_none());
        }
        engine.handle(
            21.0,
            Event::WsMessage {
                socket,
                text: if private { PROOF } else { TICK },
            },
        );
        if private {
            assert_eq!(engine.store().orders().len(), 1);
        } else {
            assert_eq!(engine.store().tickers()[0].last, 60000.0);
        }
    }
}

use safe_cex_engine::socket_lifecycle::{
    FeedId, Interest, LifecycleEvent, RetireReason, SocketError,
};
use safe_cex_engine::{Cx, Engine, Response, TimerId, VenueError};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Default, PartialEq)]
enum Scenario {
    #[default]
    Plain,
    RetireProof,
    Burst,
    Pressure,
    Credits,
}
#[derive(Default)]
struct Trace {
    scenario: Scenario,
    public: Option<SocketId>,
    public_feed: Option<FeedId>,
    authenticated: usize,
    retired: bool,
    pressure_used: bool,
}
struct Probe(ApexOmni, Rc<RefCell<Trace>>);
fn add_book() -> Command {
    Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }
}
fn add_candle() -> Command {
    Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }
}
fn hostile_symbol(index: usize) -> String {
    format!("{index}{}", "\u{0001}".repeat(800))
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        <ApexOmni as Venue>::managed_sockets()
    }
    fn socket_capabilities() -> safe_cex_engine::socket_lifecycle::SocketCapabilities {
        <ApexOmni as Venue>::socket_capabilities()
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        let public_ready = matches!(&event, LifecycleEvent::Ready { epoch, .. } if self.1.borrow().public == Some(epoch.socket()));
        if let LifecycleEvent::Authenticated { epoch, .. } = &event {
            self.1.borrow_mut().authenticated += 1;
            if self.1.borrow().scenario == Scenario::RetireProof && !self.1.borrow().retired {
                self.1.borrow_mut().retired = true;
                cx.retire_socket(epoch.feed(), RetireReason::Closed);
            }
        }
        if let LifecycleEvent::Ready { epoch, .. } = &event
            && public_ready
        {
            self.1.borrow_mut().public_feed = Some(epoch.feed());
        }
        self.0.on_socket_event(cx, event)?;
        if public_ready {
            let scenario = self.1.borrow().scenario;
            match scenario {
                Scenario::Burst if !self.1.borrow().pressure_used => {
                    self.1.borrow_mut().pressure_used = true;
                    for command in [add_book(), add_candle(), add_book(), add_candle()] {
                        self.0.on_command(cx, command);
                    }
                }
                Scenario::Pressure => {
                    if !self.1.borrow().pressure_used {
                        self.1.borrow_mut().pressure_used = true;
                        for index in 0..20 {
                            self.0.on_command(
                                cx,
                                Command::SubscribeOrderBook {
                                    symbol: hostile_symbol(index),
                                },
                            );
                        }
                    }
                }
                Scenario::Credits if !self.1.borrow().pressure_used => {
                    self.1.borrow_mut().pressure_used = true;
                    let feed = cx
                        .socket_epoch(self.1.borrow().public_feed.expect("captured real feed"))
                        .unwrap()
                        .feed();
                    for _ in 0..511 {
                        cx.set_socket_interests(
                            feed,
                            vec![Interest::from_wire_args("instrumentInfo.all")],
                        )
                        .unwrap();
                    }
                    self.0.on_command(
                        cx,
                        Command::SubscribeOrderBook {
                            symbol: "REFUSED".into(),
                        },
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn socket_control(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.0.socket_control(store, feed, authenticated, text)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        values: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.0
            .encode_socket_interests(store, feed, subscribe, values, emit)
    }

    type Pending = <ApexOmni as Venue>::Pending;
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
        self.0.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId) {
        self.0.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str) {
        self.0.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, code: u16) {
        self.0.on_close(cx, socket, code);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Self::Pending>, pending: &mut Self::Pending, item: &str) {
        self.0.on_item(cx, pending, item);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_response(cx, pending, response);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.0.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.0.on_sign_error(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.0
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_stream_error(cx, pending, response);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId) {
        self.0.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        self.0.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.0.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.0.books()
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.0.capabilities()
    }
}

fn probe(scenario: Scenario) -> (Engine<Probe>, Rc<RefCell<Trace>>, SocketId, SocketId) {
    let (signer, identities) = support::fixture_provider(&Material::default());
    let trace = Rc::new(RefCell::new(Trace {
        scenario,
        ..Default::default()
    }));
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe(
        ApexOmni::new(Options {
            identity: identities.primary.clone(),
            ..Default::default()
        }),
        trace.clone(),
    ));
    engine.set_identities(identities).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(0.0);
    let catalogue = engine
        .prepared(0.0)
        .find_map(|a| {
            if let Action::Http(request) = a {
                Some(request)
            } else {
                None
            }
        })
        .unwrap();
    engine.handle(
        10.0,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let actions: Vec<_> = engine.prepared(10.0).collect();
    let find = |role| {
        actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(channel),
                    ..
                } if *channel == role => Some(*socket),
                _ => None,
            })
            .unwrap()
    };
    let public = find(SocketChannel::Public);
    let private = find(SocketChannel::Private);
    trace.borrow_mut().public = Some(public);
    (engine, trace, public, private)
}
fn sends(actions: Vec<Action>, socket: SocketId) -> Vec<String> {
    actions
        .into_iter()
        .filter_map(|a| match a {
            Action::WsSend {
                socket: actual,
                text,
            } if actual == socket => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn revoked_auth_callback_cannot_publish_or_leak_a_proof_into_the_next_epoch() {
    let (mut engine, trace, public, private) = probe(Scenario::RetireProof);
    engine.handle(20.0, Event::WsOpened { socket: private });
    engine.prepared(20.0).for_each(drop);
    engine.handle(
        21.0,
        Event::WsMessage {
            socket: private,
            text: PROOF,
        },
    );
    let retired: Vec<_> = engine.prepared(21.0).collect();
    assert_eq!(
        trace.borrow().authenticated,
        1,
        "genuine shared authentication callback"
    );
    assert!(trace.borrow().retired);
    assert!(
        engine.store().orders().is_empty(),
        "retirement before the callback must invalidate the retained proof"
    );
    assert_eq!(
        retired
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == private))
            .count(),
        1
    );
    let retries: Vec<_> = retired
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > 21.0 && *at_ms <= 1021.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retries.len(), 1);
    let (retry, at) = retries[0];
    engine.handle(at, Event::TimerFired { timer: retry });
    let replacement = engine
        .prepared(at)
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement, private);
    engine.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    engine.prepared(at).for_each(drop);
    engine.handle(
        at + 1.0,
        Event::WsMessage {
            socket: private,
            text: PROOF,
        },
    );
    assert!(engine.prepared(at + 1.0).next().is_none());
    engine.handle(
        at + 2.0,
        Event::WsMessage {
            socket: replacement,
            text: r#"{"op":"login","success":true}"#,
        },
    );
    engine.prepared(at + 2.0).for_each(drop);
    assert!(
        engine.store().orders().is_empty(),
        "login success cannot resurrect an old epoch's proof"
    );
    let fresh = PROOF.replace("proof-order", "new-epoch-order");
    engine.handle(
        at + 3.0,
        Event::WsMessage {
            socket: replacement,
            text: &fresh,
        },
    );
    assert_eq!(engine.store().orders().len(), 1);
    assert_eq!(engine.store().orders()[0].id, "new-epoch-order");
    engine.prepared(at + 3.0).for_each(drop);
    engine.handle(at + 4.0, Event::WsOpened { socket: public });
    assert!(
        sends(engine.prepared(at + 4.0).collect(), public)
            .iter()
            .any(|s| s.contains("instrumentInfo.all")),
        "independent public peer survives"
    );
}

#[test]
fn pending_full_desired_changes_preserve_duplicate_book_and_candle_refs() {
    let (mut engine, _, public, _) = probe(Scenario::Burst);
    engine.handle(20.0, Event::WsOpened { socket: public });
    let first = sends(engine.prepared(20.0).collect(), public);
    assert_eq!(
        first
            .iter()
            .filter(|s| s.contains("orderBook200.H.BTCUSDT"))
            .count(),
        1
    );
    assert_eq!(
        first
            .iter()
            .filter(|s| s.contains("candle.1.BTCUSDT"))
            .count(),
        1
    );
    assert_eq!(
        first
            .iter()
            .filter(|s| s.contains("recentlyTrade.H.BTCUSDT"))
            .count(),
        1
    );
    for command in [
        Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        },
    ] {
        engine.handle(21.0, Event::Command(command));
        assert!(sends(engine.prepared(21.0).collect(), public).is_empty());
    }
    engine.handle(
        22.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        sends(engine.prepared(22.0).collect(), public),
        [r#"{"op":"unsubscribe","args":["orderBook200.H.BTCUSDT"]}"#]
    );
    engine.handle(
        23.0,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let last = sends(engine.prepared(23.0).collect(), public);
    assert_eq!(last.len(), 1);
    assert!(
        last[0].contains("unsubscribe")
            && last[0].contains("candle.1.BTCUSDT")
            && last[0].contains("recentlyTrade.H.BTCUSDT")
    );
}

#[test]
fn actual_proposal_credit_refusal_rolls_back_only_the_unadmitted_book() {
    let (mut engine, _, public, private) = probe(Scenario::Credits);
    engine.handle(20.0, Event::WsOpened { socket: private });
    engine.prepared(20.0).for_each(drop);
    engine.handle(21.0, Event::WsOpened { socket: public });
    let actions: Vec<_> = engine.prepared(21.0).collect();
    assert!(engine.book("REFUSED").is_none());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("REFUSED")))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(e)) if e.message.contains("Capacity"))));
    engine.handle(
        22.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "REFUSED".into(),
        }),
    );
    assert!(
        engine.book("REFUSED").is_some(),
        "the first real admitted subscriber is not a phantom duplicate"
    );
    assert_eq!(
        sends(engine.prepared(22.0).collect(), public)
            .iter()
            .filter(|s| s.contains("orderBook200.H.REFUSED"))
            .count(),
        1
    );
    engine.handle(
        23.0,
        Event::WsMessage {
            socket: private,
            text: r#"{"op":"ping","args":["peer"]}"#,
        },
    );
    assert_eq!(
        sends(engine.prepared(23.0).collect(), private),
        [r#"{"op":"pong","args":["peer"]}"#]
    );
}

#[test]
fn valid_key_encoded_frame_pressure_retires_once_and_recovers_without_losing_the_private_peer() {
    let (mut engine, _, public, private) = probe(Scenario::Pressure);
    engine.handle(20.0, Event::WsOpened { socket: private });
    engine.prepared(20.0).for_each(drop);
    engine.handle(21.0, Event::WsOpened { socket: public });
    let actions: Vec<_> = engine.prepared(21.0).collect();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsClose { socket } if *socket == public))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == private))
    );
    let retries: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > 21.0 && *at_ms <= 1021.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retries.len(), 1);
    for index in 0..20 {
        engine.handle(
            22.0,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: hostile_symbol(index),
            }),
        );
        engine.prepared(22.0).for_each(drop);
    }
    engine.handle(23.0, Event::Command(add_book()));
    engine.prepared(23.0).for_each(drop);
    let (retry, at) = retries[0];
    engine.handle(at, Event::TimerFired { timer: retry });
    let replacement = engine
        .prepared(at)
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement, public);
    engine.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let frames = sends(engine.prepared(at).collect(), replacement);
    assert!(frames.iter().any(|t| t.contains("orderBook200.H.BTCUSDT")));
    assert!(frames.iter().all(|t| t.len() <= 65536));
    engine.handle(
        at + 1.0,
        Event::WsMessage {
            socket: private,
            text: r#"{"op":"ping","args":["peer"]}"#,
        },
    );
    assert_eq!(
        sends(engine.prepared(at + 1.0).collect(), private),
        [r#"{"op":"pong","args":["peer"]}"#]
    );
}
