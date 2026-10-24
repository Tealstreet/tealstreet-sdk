#[path = "support/account.rs"]
mod account;
#[path = "support/mod.rs"]
mod public;

use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
    Venue,
};
use safe_cex_venue_txflow::{Options, TxFlow};
use std::collections::HashMap;

const API_TICKER: &str = r#"{"channel":"ticker","data":{"instrumentId":1,"lastPrice":"999","coinVlm":"3","dayNtlVlm":"3000"}}"#;
const BOOK: &str = r#"{"channel":"l2Book","data":{"coin":"1","time":3000,"levels":[[{"px":"80","sz":"1"}],[{"px":"81","sz":"2"}]]}}"#;
const NODE_CONTEXT: &str =
    r#"{"method":"subscribe","type":"allAssetCtxs","subscription":{"type":"allAssetCtxs"}}"#;
const NODE_BOOK: &str =
    r#"{"method":"subscribe","type":"l2Book","subscription":{"type":"l2Book","coin":"1"}}"#;
const API_SUB: &str =
    r#"{"method":"subscribe","subscription":{"type":"ticker","coin":"BTC","instrumentId":1}}"#;
const PING: &str = r#"{"method":"ping"}"#;

struct Rig<V: Venue = TxFlow> {
    engine: Engine<V>,
    now: f64,
    timers: HashMap<TimerId, f64>,
    node: SocketId,
    api: Option<SocketId>,
    connected_at: f64,
}

fn connected(actions: &[Action], api: bool) -> Option<SocketId> {
    actions.iter().find_map(|action| match action {
        Action::WsConnect { socket, url, .. }
            if url.ends_with(if api { "/api-ws" } else { "/ws" }) =>
        {
            Some(*socket)
        }
        _ => None,
    })
}

fn sent(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { socket: s, text } if *s == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn private_frame(kind: &str) -> String {
    format!(
        r#"{{"method":"subscribe","type":"{kind}","subscription":{{"type":"{kind}","user":"{}"}}}}"#,
        account::WALLET
    )
}

fn financial_frame() -> String {
    let mut data: serde_json::Value = serde_json::from_str(account::STATE).unwrap();
    data["data"]["time"] = (account::NOW + 26.0).into();
    data["data"]["assetPositions"][0]["position"]["szi"] = "3".into();
    data["data"]["assetPositions"][0]["position"]["updateTime"] = (account::NOW + 26.0).into();
    serde_json::json!({"channel":"clearinghouseState", "data": data["data"]}).to_string()
}

impl Rig {
    fn new(private: bool) -> Self {
        Self::with_policy(private, PublicFeedPolicy::Direct).0
    }

    fn with_policy(private: bool, policy: PublicFeedPolicy) -> (Self, Vec<Action>) {
        let (mut rig, mut trace) = Self::before_account_start(private, policy);
        let seed = rig.advance(account::NOW + 25.0);
        trace.extend(rig.answer_reads(seed));
        assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 65000.0);
        if private {
            assert_eq!(rig.engine.store().positions()[0].contracts, 2.0);
        }
        (rig, trace)
    }

    fn before_account_start(private: bool, policy: PublicFeedPolicy) -> (Self, Vec<Action>) {
        let engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
            testnet: true,
            public_data_only: !private,
            application_id: private.then(|| account::WALLET.into()),
            secret: private.then(|| format!("0x{:064x}", 1)),
            ..Default::default()
        }));
        Self::connecting(engine, policy)
    }
}

impl<V: Venue> Rig<V> {
    fn connecting(mut engine: Engine<V>, policy: PublicFeedPolicy) -> (Self, Vec<Action>) {
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(account::NOW);
        let mut rig = Self {
            engine,
            now: account::NOW,
            timers: HashMap::new(),
            node: SocketId(0),
            api: None,
            connected_at: account::NOW,
        };
        let initial = rig.drain();
        let trace = rig.answer_reads(initial);
        rig.node = connected(&trace, false).expect("native node connection");
        rig.api = connected(&trace, true);
        (rig, trace)
    }

    fn drain(&mut self) -> Vec<Action> {
        let actions: Vec<_> = self.engine.drain_actions().collect();
        for action in &actions {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(timer);
                }
                _ => {}
            }
        }
        actions
    }

    // Replies are explicit; the returned trace retains every original HTTP action.
    fn answer_reads(&mut self, initial: Vec<Action>) -> Vec<Action> {
        let mut trace = Vec::new();
        let mut batch = initial;
        for _ in 0..32 {
            if batch.is_empty() {
                return trace;
            }
            for action in batch {
                if let Action::Http(request) = &action {
                    let body = match account::kind(request).as_str() {
                        "perpMeta" => account::META.to_owned(),
                        "activeAssetCtx" => public::TICKER.to_owned(),
                        "clearinghouseState" => account::STATE.to_owned(),
                        "frontendOpenOrders" => {
                            format!(r#"{{"success":true,"code":0,"data":[{}]}}"#, account::ORDER)
                        }
                        "activeAssetData" => format!(
                            r#"{{"success":true,"code":0,"data":{{"coin":"1","user":"{}","leverage":{{"mode":"oneWay","type":"Cross","value":10}}}}}}"#,
                            account::WALLET
                        ),
                        "userFills" => {
                            r#"{"success":true,"code":0,"data":{"data":[],"hasMore":false}}"#.into()
                        }
                        kind => panic!("unexpected native read {kind}"),
                    };
                    self.engine.handle(
                        self.now,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: &body,
                            },
                        },
                    );
                }
                trace.push(action);
            }
            batch = self.drain();
        }
        panic!("native reads did not settle");
    }

    fn handle(&mut self, event: Event<'_>) -> Vec<Action> {
        self.engine.handle(self.now, event);
        self.drain()
    }

    fn open(&mut self, api: bool) -> Vec<Action> {
        self.handle(Event::WsOpened {
            socket: self.socket(api),
        })
    }

    fn socket(&self, api: bool) -> SocketId {
        if api {
            self.api.expect("API connection")
        } else {
            self.node
        }
    }

    fn message(&mut self, api: bool, text: &str) -> Vec<Action> {
        self.handle(Event::WsMessage {
            socket: self.socket(api),
            text,
        })
    }

    fn book(&mut self) {
        self.handle(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }));
    }

    fn advance(&mut self, until: f64) -> Vec<Action> {
        let mut trace = Vec::new();
        for _ in 0..1024 {
            let next = self
                .timers
                .iter()
                .filter(|(_, at)| **at <= until)
                .min_by(|a, b| a.1.total_cmp(b.1).then(a.0.0.cmp(&b.0.0)))
                .map(|(id, at)| (*id, *at));
            let Some((timer, at)) = next else {
                self.now = self.now.max(until);
                return trace;
            };
            self.timers.remove(&timer);
            self.now = self.now.max(at);
            trace.extend(self.handle(Event::TimerFired { timer }));
        }
        panic!("native timers did not settle");
    }

    fn reconnect(&mut self, api: bool) -> Vec<Action> {
        let old = self.socket(api);
        let closed = self.handle(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        let at = closed
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { at_ms, .. } => Some(*at_ms),
                _ => None,
            })
            .min_by(f64::total_cmp)
            .expect("native retry timer");
        let mut actions = self.advance(at);
        let socket = connected(&actions, api).expect("native replacement connection");
        assert_ne!(socket, old);
        if api {
            self.api = Some(socket);
        } else {
            self.node = socket;
        }
        actions.extend(self.open(api));
        actions
    }
}

#[test]
fn duplicate_api_open_does_not_replay_tickers() {
    let mut rig = Rig::new(false);
    assert_eq!(sent(&rig.open(true), rig.socket(true)), [API_SUB]);
    let duplicate = rig.open(true);
    assert!(
        sent(&duplicate, rig.socket(true)).is_empty(),
        "duplicate replay: {duplicate:?}"
    );
}

#[test]
fn duplicate_node_open_does_not_replay_interests() {
    let mut rig = Rig::new(true);
    assert_eq!(sent(&rig.open(false), rig.node), [NODE_CONTEXT]);
    let duplicate = rig.open(false);
    assert!(
        sent(&duplicate, rig.node).is_empty(),
        "duplicate replay: {duplicate:?}"
    );
}

#[test]
fn duplicate_node_open_does_not_clear_published_book() {
    let mut rig = Rig::new(false);
    rig.book();
    rig.open(false);
    rig.message(false, BOOK);
    assert_eq!(rig.engine.book("BTC-USDC").unwrap().best_bid(), Some(80.0));
    rig.open(false);
    assert_eq!(
        rig.engine.book("BTC-USDC").and_then(|book| book.best_bid()),
        Some(80.0)
    );
}

fn late_open(api: bool) {
    let mut rig = Rig::new(false);
    rig.now = rig.connected_at + 10_000.0;
    let actions = rig.open(api);
    assert!(
        sent(&actions, rig.socket(api)).is_empty(),
        "late open: {actions:?}"
    );
}

#[test]
fn api_open_at_connect_deadline_is_inert_without_watchdog() {
    late_open(true);
}
#[test]
fn node_open_at_connect_deadline_is_inert_without_watchdog() {
    late_open(false);
}

#[test]
fn api_ticker_at_idle_deadline_cannot_publish_without_watchdog() {
    let mut rig = Rig::new(false);
    rig.open(true);
    rig.now += 30_000.0;
    rig.message(true, API_TICKER);
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 65000.0);
}

#[test]
fn node_book_at_idle_deadline_cannot_publish_without_watchdog() {
    let mut rig = Rig::new(false);
    rig.book();
    rig.open(false);
    rig.now += 30_000.0;
    rig.message(false, BOOK);
    assert!(
        rig.engine.book("BTC-USDC").is_none(),
        "expired book published"
    );
}

#[test]
fn private_state_at_idle_deadline_cannot_publish_without_watchdog() {
    let mut rig = Rig::new(true);
    rig.open(false);
    rig.now += 30_000.0;
    rig.message(false, &financial_frame());
    assert_eq!(rig.engine.store().positions()[0].contracts, 2.0);
}

fn opening_trace() -> (Rig, Vec<Action>) {
    let mut rig = Rig::new(true);
    let mut trace = rig.open(false);
    trace.extend(rig.advance(rig.now + 600.0));
    (rig, trace)
}

fn private_subscription_positions(trace: &[Action], socket: SocketId) -> Vec<usize> {
    ["orderUpdates", "clearinghouseState", "userAssetData"].iter().map(|kind| {
        let frame = private_frame(kind);
        trace.iter().position(|action| matches!(action, Action::WsSend { socket: s, text } if *s == socket && text == &frame))
            .unwrap_or_else(|| panic!("missing required {kind}: {trace:?}"))
    }).collect()
}

#[test]
fn opening_refresh_waits_for_all_paced_private_subscriptions() {
    let (rig, trace) = opening_trace();
    let last = *private_subscription_positions(&trace, rig.node)
        .last()
        .unwrap();
    let refresh = trace.iter().position(|action| matches!(action, Action::Http(request) if account::kind(request) == "clearinghouseState"))
        .expect("opening native refresh");
    assert!(
        last < refresh,
        "private batch ends at {last}, refresh at {refresh}: {trace:?}"
    );
}

#[test]
fn released_private_batch_precedes_opening_refresh() {
    let (rig, trace) = opening_trace();
    let positions = private_subscription_positions(&trace, rig.node);
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    let refresh = trace.iter().position(|action| matches!(action, Action::Http(request) if account::kind(request) == "clearinghouseState"))
        .expect("opening native refresh");
    assert!(positions.iter().all(|position| *position < refresh));
}

#[test]
fn native_wire_roles_and_first_frame_then_150ms_pacing() {
    let (mut rig, startup) = Rig::with_policy(true, PublicFeedPolicy::Direct);
    assert!(startup.iter().any(|action| matches!(action, Action::WsConnect { socket, channel: Some(SocketChannel::Private), .. } if *socket == rig.node)));
    assert!(startup.iter().any(|action| matches!(action, Action::WsConnect { socket, channel: Some(SocketChannel::Public), .. } if Some(*socket) == rig.api)));
    assert_eq!(sent(&rig.open(true), rig.socket(true)), [API_SUB]);
    let at = rig.now;
    assert_eq!(sent(&rig.open(false), rig.node), [NODE_CONTEXT]);
    assert!(sent(&rig.advance(at + 149.0), rig.node).is_empty());
    assert_eq!(
        sent(&rig.advance(at + 150.0), rig.node),
        [private_frame("orderUpdates")]
    );
    assert_eq!(
        sent(&rig.advance(at + 300.0), rig.node),
        [private_frame("clearinghouseState")]
    );
    assert_eq!(
        sent(&rig.advance(at + 450.0), rig.node),
        [private_frame("userAssetData")]
    );
}

#[test]
fn identical_live_public_and_private_frames_publish() {
    let mut rig = Rig::new(true);
    rig.book();
    rig.open(false);
    rig.open(true);
    rig.now += 1.0;
    rig.message(false, BOOK);
    rig.message(true, API_TICKER);
    rig.message(false, &financial_frame());
    assert_eq!(rig.engine.book("BTC-USDC").unwrap().best_bid(), Some(80.0));
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 999.0);
    assert_eq!(rig.engine.store().positions()[0].contracts, 3.0);
}

#[test]
fn shared_and_disabled_preserve_private_topics_and_financial_data() {
    for policy in [PublicFeedPolicy::Shared, PublicFeedPolicy::Disabled] {
        let (mut rig, startup) = Rig::with_policy(true, policy);
        assert!(rig.api.is_none());
        assert!(startup.iter().any(|action| matches!(action, Action::WsConnect { socket, channel: Some(SocketChannel::Private), .. } if *socket == rig.node)));
        let mut trace = rig.open(false);
        trace.extend(rig.advance(rig.now + 450.0));
        assert_eq!(
            sent(&trace, rig.node),
            [
                private_frame("orderUpdates"),
                private_frame("clearinghouseState"),
                private_frame("userAssetData")
            ]
        );
        rig.message(false, &financial_frame());
        assert_eq!(rig.engine.store().positions()[0].contracts, 3.0);
    }
}

#[test]
fn api_close_preserves_book_and_retired_traffic_cannot_publish() {
    let mut rig = Rig::new(false);
    rig.book();
    rig.open(false);
    rig.open(true);
    rig.message(false, BOOK);
    let old = rig.socket(true);
    rig.reconnect(true);
    assert_eq!(rig.engine.book("BTC-USDC").unwrap().best_bid(), Some(80.0));
    rig.handle(Event::WsMessage {
        socket: old,
        text: API_TICKER,
    });
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 65000.0);
    rig.message(true, API_TICKER);
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 999.0);
}

#[test]
fn node_close_clears_book_and_retired_traffic_cannot_publish() {
    let mut rig = Rig::new(false);
    rig.book();
    rig.open(false);
    rig.message(false, BOOK);
    let old = rig.node;
    rig.reconnect(false);
    assert!(rig.engine.book("BTC-USDC").is_none());
    rig.handle(Event::WsMessage {
        socket: old,
        text: BOOK,
    });
    assert!(rig.engine.book("BTC-USDC").is_none());
    rig.message(false, BOOK);
    assert_eq!(rig.engine.book("BTC-USDC").unwrap().best_bid(), Some(80.0));
}

#[test]
fn malformed_current_text_refreshes_activity_and_ping_wire_is_exact() {
    let mut rig = Rig::new(false);
    rig.open(true);
    let opened = rig.now;
    rig.now = opened + 29_000.0;
    rig.message(true, "not-json");
    rig.now = opened + 30_000.0;
    rig.message(true, API_TICKER);
    assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 999.0);
    let actions = rig.advance(rig.now + 10_000.0);
    assert!(sent(&actions, rig.socket(true)).contains(&PING));
    assert!(
        !actions.iter().any(
            |action| matches!(action, Action::WsClose { socket } if *socket == rig.socket(true))
        )
    );
}

#[test]
fn native_public_book_and_candle_wire() {
    let mut rig = Rig::new(false);
    rig.book();
    rig.handle(Event::Command(Command::SubscribeCandles {
        symbol: "BTC-USDC".into(),
        interval: safe_cex_engine::model::Interval::M1,
    }));
    let at = rig.now;
    assert_eq!(sent(&rig.open(false), rig.node), [NODE_CONTEXT]);
    assert_eq!(sent(&rig.open(true), rig.socket(true)), [API_SUB]);
    let paced = rig.advance(at + 150.0);
    assert_eq!(sent(&paced, rig.node), [NODE_BOOK]);
    assert_eq!(
        sent(&paced, rig.socket(true)),
        [r#"{"method":"subscribe","subscription":{"type":"candle","coin":"1","interval":"1m"}}"#]
    );
}

#[test]
fn duplicate_node_open_does_not_restart_account_refresh() {
    let mut rig = Rig::new(true);
    let initial = rig.open(false);
    rig.answer_reads(initial);
    let duplicate = rig.open(false);
    assert!(
        !duplicate
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "duplicate open restarted native account reads"
    );
}

#[test]
fn reconnect_refresh_waits_for_all_paced_private_subscriptions() {
    let mut rig = Rig::new(true);
    let initial = rig.open(false);
    rig.answer_reads(initial);
    let mut trace = rig.reconnect(false);
    trace.extend(rig.advance(rig.now + 600.0));
    let last = *private_subscription_positions(&trace, rig.node)
        .last()
        .unwrap();
    let refresh = trace.iter().position(|action| matches!(action, Action::Http(request) if account::kind(request) == "clearinghouseState"))
        .expect("reconnect native refresh");
    assert!(
        last < refresh,
        "private reconnect batch ends at {last}, refresh at {refresh}"
    );
}

#[test]
fn private_retired_socket_cannot_publish_but_live_replacement_can() {
    let mut rig = Rig::new(true);
    rig.open(false);
    let old = rig.node;
    rig.reconnect(false);
    rig.handle(Event::WsMessage {
        socket: old,
        text: &financial_frame(),
    });
    assert_eq!(rig.engine.store().positions()[0].contracts, 2.0);
    rig.message(false, &financial_frame());
    assert_eq!(rig.engine.store().positions()[0].contracts, 3.0);
}

#[test]
fn missing_private_address_rejects_setup_without_artificial_socket_or_account_work() {
    let options = Options {
        testnet: true,
        public_data_only: false,
        application_id: None,
        vault: None,
        secret: Some(format!("0x{:064x}", 1)),
        ..Default::default()
    };
    assert!(options.validate().is_err());
    let mut engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(options));
    engine.start(account::NOW);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.code == safe_cex_engine::log::code::CONFIG_INVALID)));
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::Notice(_))),
        "invalid setup must not manufacture HTTP, socket, signing or timer work"
    );
    assert!(engine.store().positions().is_empty());
}

#[test]
fn genuine_private_open_before_native_account_start_keeps_refresh_noop() {
    let (mut rig, initial) = Rig::before_account_start(true, PublicFeedPolicy::Direct);
    assert!(initial.iter().any(|action| matches!(action,
        Action::WsConnect { socket, channel: Some(SocketChannel::Private), .. } if *socket == rig.node)));
    assert!(!initial.iter().any(|action| matches!(action,
        Action::Http(request) if account::kind(request) == "clearinghouseState")));
    assert!(rig.engine.store().positions().is_empty());
    let opening = rig.open(false);
    assert_eq!(sent(&opening, rig.node), [NODE_CONTEXT]);
    assert!(
        !opening
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    rig.message(false, &financial_frame());
    assert!(
        rig.engine.store().positions().is_empty(),
        "opening must not fabricate account.started"
    );
    let startup = rig.advance(account::NOW + 25.0);
    assert_eq!(
        startup
            .iter()
            .filter(|action| matches!(action,
        Action::Http(request) if account::kind(request) == "clearinghouseState"))
            .count(),
        1
    );
    rig.answer_reads(startup);
    assert_eq!(rig.engine.store().positions()[0].contracts, 2.0);
    let after_batch = rig.advance(account::NOW + 1_000.0);
    assert!(
        !after_batch.iter().any(|action| matches!(action,
        Action::Http(request) if matches!(account::kind(request).as_str(),
            "clearinghouseState" | "frontendOpenOrders" | "userFills"))),
        "subscription completion must not duplicate native startup reads"
    );
}

#[test]
fn public_only_without_address_has_no_private_refresh_obligation() {
    let mut rig = Rig::new(false);
    let mut trace = rig.open(false);
    trace.extend(rig.advance(rig.now + 450.0));
    assert_eq!(sent(&trace, rig.node), [NODE_CONTEXT]);
    assert!(!trace.iter().any(|action| matches!(action, Action::Http(_))));
    rig.message(false, &financial_frame());
    assert!(rig.engine.store().positions().is_empty());
}

fn hold_refresh_wake<V: Venue>(rig: &mut Rig<V>) -> (TimerId, Vec<Action>) {
    let at = rig.now;
    let mut trace = rig.open(false);
    trace.extend(rig.advance(at + 449.0));
    assert!(
        !trace.iter().any(|action| matches!(action, Action::Http(_))),
        "partial paced private batch must not release HTTP"
    );
    let timer = *rig
        .timers
        .iter()
        .find(|(_, when)| **when == at + 450.0)
        .unwrap()
        .0;
    rig.timers.remove(&timer);
    rig.now = at + 450.0;
    let final_frame = rig.handle(Event::TimerFired { timer });
    assert_eq!(
        sent(&final_frame, rig.node),
        [private_frame("userAssetData")]
    );
    assert!(
        !final_frame
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "final frame handoff is not the later callback ingress"
    );
    let wake = final_frame
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == rig.now => Some(*timer),
            _ => None,
        })
        .expect("real shared released-interest wake");
    trace.extend(final_frame);
    (wake, trace)
}

#[test]
fn partial_private_batch_and_final_frame_wait_for_real_callback_ingress() {
    let mut rig = Rig::new(true);
    let (wake, _) = hold_refresh_wake(&mut rig);
    rig.timers.remove(&wake);
    let released = rig.handle(Event::TimerFired { timer: wake });
    assert_eq!(
        released
            .iter()
            .filter(|action| matches!(action,
        Action::Http(request) if account::kind(request) == "clearinghouseState"))
            .count(),
        1
    );
    let duplicate = rig.handle(Event::TimerFired { timer: wake });
    assert!(
        !duplicate
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn current_native_poll_ownership_consumes_due_before_a_delayed_handoff() {
    let mut rig = Rig::new(true);
    let (wake, _) = hold_refresh_wake(&mut rig);
    rig.timers.remove(&wake);
    let poll_at = account::NOW + 5025.0;
    let reads = rig.advance(poll_at);
    assert_eq!(
        reads
            .iter()
            .filter(|action| matches!(action,
        Action::Http(request) if account::kind(request) == "clearinghouseState"))
            .count(),
        1
    );
    rig.answer_reads(reads);
    let late = rig.handle(Event::TimerFired { timer: wake });
    assert!(
        !late.iter().any(|action| matches!(action, Action::Http(_))),
        "completed current native ownership must not be duplicated by old intent"
    );
}

#[test]
fn retired_admitted_refresh_recovers_and_stale_wake_cannot_consume_new_work() {
    let mut rig = Rig::new(true);
    let (old_wake, _) = hold_refresh_wake(&mut rig);
    rig.timers.remove(&old_wake);
    let old = rig.node;
    let retired = rig.handle(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let retry_at = retired
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { at_ms, .. } if *at_ms > rig.now => Some(*at_ms),
            _ => None,
        })
        .min_by(f64::total_cmp)
        .expect("shared retry after retirement");
    let stale = rig.handle(Event::TimerFired { timer: old_wake });
    assert!(!stale.iter().any(|action| matches!(action, Action::Http(_))));
    let connecting = rig.advance(retry_at);
    rig.node = connected(&connecting, false).expect("real replacement");
    assert_ne!(rig.node, old);
    let (fresh_wake, _) = hold_refresh_wake(&mut rig);
    assert_ne!(fresh_wake, old_wake);
    let stale = rig.handle(Event::TimerFired { timer: old_wake });
    assert!(!stale.iter().any(|action| matches!(action, Action::Http(_))));
    rig.timers.remove(&fresh_wake);
    let fresh = rig.handle(Event::TimerFired { timer: fresh_wake });
    assert_eq!(
        fresh
            .iter()
            .filter(|action| matches!(action,
        Action::Http(request) if account::kind(request) == "clearinghouseState"))
            .count(),
        1
    );
    rig.answer_reads(fresh);
    for timer in [old_wake, fresh_wake] {
        let duplicate = rig.handle(Event::TimerFired { timer });
        assert!(
            !duplicate
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}

struct Pressure {
    inner: TxFlow,
    inject: bool,
    spare: Option<safe_cex_engine::socket_lifecycle::FeedId>,
}

impl Venue for Pressure {
    type Pending = safe_cex_venue_txflow::Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.inner.capabilities()
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.inner.cancel_needs_stored_order()
    }
    fn guard_order_mutations(&self) -> bool {
        self.inner.guard_order_mutations()
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.inner.books()
    }
    fn start(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_close(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        socket: SocketId,
        code: u16,
    ) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_message(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        socket: SocketId,
        text: &str,
    ) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_response(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_item(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: &mut Self::Pending,
        item: &str,
    ) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_command(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, command: Command) {
        self.inner.on_command(cx, command);
    }
    fn on_timer(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_socket_event(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        use safe_cex_engine::socket_lifecycle::*;
        if self.inject && matches!(event, LifecycleEvent::Ready { .. }) {
            self.inject = false;
            let spare = cx.register_socket_feed(FeedPolicy {
                role: SocketChannel::Public,
                supervision: safe_cex_engine::socket::Config::default(),
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::None,
                invalidation: Invalidation::None,
                subscription_interval_ms: 0.,
            })?;
            self.spare = Some(spare);
            for _ in 0..MAX_FEEDS {
                cx.set_socket_interests(spare, Vec::new())?;
            }
        }
        self.inner.on_socket_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        epoch: safe_cex_engine::socket_lifecycle::Epoch,
        pending: Self::Pending,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.inner.on_socket_interests_released(cx, epoch, pending)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        interests: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        if self.spare == Some(feed) {
            assert!(interests.is_empty());
            Ok(())
        } else {
            self.inner
                .encode_socket_interests(store, feed, subscribe, interests, emit)
        }
    }
}

#[test]
fn real_proposal_capacity_refusal_frees_then_reconnects_and_refreshes_once() {
    let engine = safe_cex_engine::test_support::seeded_engine(Pressure {
        inner: TxFlow::new(Options {
            testnet: true,
            application_id: Some(account::WALLET.into()),
            secret: Some(format!("0x{:064x}", 1)),
            ..Default::default()
        }),
        inject: true,
        spare: None,
    });
    let (mut rig, _) = Rig::connecting(engine, PublicFeedPolicy::Direct);
    let startup = rig.advance(account::NOW + 25.0);
    rig.answer_reads(startup);
    let old = rig.node;
    let refused = rig.open(false);
    assert!(refused.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("Capacity"))), "real retained proposal capacity must refuse");
    assert!(
        refused
            .iter()
            .any(|action| matches!(action, Action::WsClose { socket } if *socket == old))
    );
    assert!(
        !refused
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::WsSend { .. }))
    );
    let retry_at = refused
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { at_ms, .. } if *at_ms > rig.now => Some(*at_ms),
            _ => None,
        })
        .min_by(f64::total_cmp)
        .expect("shared refusal backoff");
    let connecting = rig.advance(retry_at);
    rig.node = connected(&connecting, false).expect("shared replacement after pressure drained");
    assert_ne!(rig.node, old);
    let (wake, _) = hold_refresh_wake(&mut rig);
    rig.timers.remove(&wake);
    let fresh = rig.handle(Event::TimerFired { timer: wake });
    assert_eq!(
        fresh
            .iter()
            .filter(|action| matches!(action,
        Action::Http(request) if account::kind(request) == "clearinghouseState"))
            .count(),
        1
    );
    rig.answer_reads(fresh);
    let duplicate = rig.handle(Event::TimerFired { timer: wake });
    assert!(
        !duplicate
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(rig.engine.store().positions()[0].contracts, 2.0);
}
