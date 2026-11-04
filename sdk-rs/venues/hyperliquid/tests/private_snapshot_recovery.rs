mod support;

use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Engine, Event, Notice, Response, SocketId, TimerId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const MIDS: &str = r#"{"channel":"allMids","data":{"mids":{"BTC":"123"}}}"#;
const STATE: &str = r#"{"channel":"clearinghouseState","data":{"dex":"","clearinghouseState":{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":1000}}}"#;
const PONG: &str = r#"{"channel":"pong"}"#;

struct Harness {
    engine: Engine<Hyperliquid>,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    connects: Vec<SocketId>,
    closed: Vec<SocketId>,
    closed_at: Vec<(SocketId, f64)>,
    sent: Vec<(SocketId, String)>,
    hip: bool,
    mode: &'static str,
    bootstrap_warnings: usize,
}

impl Harness {
    fn private() -> Self {
        Self::account(false, false, "disabled")
    }

    fn account(spot: bool, hip: bool, mode: &'static str) -> Self {
        let engine = support::private_engine(
            Options {
                key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
                application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
                ..Options::default()
            },
            support::TEST_SECRET.into(),
            spot,
        );
        let mut h = Self {
            engine,
            now: 1000.,
            timers: Vec::new(),
            connects: Vec::new(),
            closed: Vec::new(),
            closed_at: Vec::new(),
            sent: Vec::new(),
            hip,
            mode,
            bootstrap_warnings: 0,
        };
        h.engine.start(h.now);
        h.collect();
        assert_eq!(h.connects.len(), 1);
        h.event(Event::WsOpened { socket: h.socket() });
        assert!(h.sent.iter().any(|(_, text)| text.contains(if spot {
            "spotState"
        } else {
            "clearinghouseState"
        })));
        // REST startup is healthy. It is not proof that the socket's private
        // subscriptions are working, even though it has already loaded the store.
        assert!(h.engine.store().loaded().balance);
        h
    }

    fn socket(&self) -> SocketId {
        *self.connects.last().unwrap()
    }

    fn collect(&mut self) {
        for _ in 0..100 {
            let actions = prepared_now(&mut self.engine);
            if actions.is_empty() {
                return;
            }
            for action in actions {
                match action {
                    Action::Http(request) => {
                        let kind = support::kind(&request);
                        let has_dex = request
                            .body
                            .as_deref()
                            .is_some_and(|body| body.contains("\"dex\""));
                        let mode = format!("\"{}\"", self.mode);
                        let body = match kind.as_str() {
                            "metaAndAssetCtxs" if has_dex => support::HIP3_META,
                            "metaAndAssetCtxs" => support::META,
                            "spotMetaAndAssetCtxs" => {
                                r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},[{"coin":"PURR/USDC","midPx":"1","markPx":"1","prevDayPx":"1"}]]"#
                            }
                            "perpDexs" if self.hip => r#"[null,{"name":"xyz"}]"#,
                            "perpDexs" => "[null]",
                            "userAbstraction" => &mode,
                            "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                            "clearinghouseState" => support::STATE,
                            "spotClearinghouseState" => {
                                r#"{"balances":[{"coin":"USDC","total":"1000","hold":"0"}]}"#
                            }
                            "frontendOpenOrders" => "[]",
                            "maxBuilderFee" => "30",
                            kind => panic!("unexpected read {kind}"),
                        };
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
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                    Action::WsConnect { socket, .. } => self.connects.push(socket),
                    Action::WsClose { socket } => {
                        self.closed.push(socket);
                        self.closed_at.push((socket, self.now));
                    }
                    Action::WsSend { socket, text } => self.sent.push((socket, text)),
                    Action::Notice(Notice::Event(event))
                        if event.code == safe_cex_engine::log::code::STREAM_UNAVAILABLE =>
                    {
                        self.bootstrap_warnings += 1;
                    }
                    _ => {}
                }
            }
        }
        panic!("native startup did not settle");
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }

    fn next_connection(&mut self) -> SocketId {
        let before = self.connects.len();
        while self.connects.len() == before {
            let at = self
                .timers
                .iter()
                .map(|(_, at)| *at)
                .min_by(f64::total_cmp)
                .expect("native reconnect must have a timer");
            self.advance(at);
        }
        self.socket()
    }

    fn advance(&mut self, until: f64) {
        while let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(id, _)| *id != timer);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
            if !self.closed.contains(&self.socket()) {
                self.message(self.socket(), PONG);
                self.message(self.socket(), MIDS);
            }
        }
        self.now = until;
    }
}

#[test]
fn missing_initial_private_snapshot_reconnects_despite_public_data_and_pongs() {
    let mut h = Harness::private();
    let old = h.socket();
    h.message(old, PONG);
    h.message(old, MIDS);
    h.advance(30_999.);
    assert!(
        h.closed.is_empty(),
        "allow the complete private snapshot deadline"
    );
    h.advance(31_000.);
    assert_eq!(
        h.closed,
        [old],
        "public frames and pongs cannot prove private readiness"
    );
    // The browser may remain in CLOSING forever: no WsClosed callback is sent.
    h.advance(32_001.);
    let replacement = h.socket();
    assert_ne!(
        replacement, old,
        "native retry must not wait for a close callback"
    );
    h.event(Event::WsOpened {
        socket: replacement,
    });
    assert!(
        h.sent
            .iter()
            .any(|(socket, text)| *socket == replacement && text.contains("clearinghouseState"))
    );
    h.message(replacement, STATE);
    h.advance(92_001.);
    assert_eq!(
        h.closed,
        [old],
        "replacement initial snapshot completes recovery"
    );
}

#[test]
fn a_completed_initial_private_snapshot_does_not_watch_idle_account_events() {
    let mut h = Harness::private();
    let socket = h.socket();
    h.message(socket, STATE);
    h.advance(601_000.);
    assert!(
        h.closed.is_empty(),
        "quiet fills and orderUpdates are legitimate"
    );
    assert_eq!(h.connects, [socket]);
}

#[test]
fn hidden_tab_wake_expires_before_a_late_snapshot_can_publish() {
    let mut h = Harness::private();
    let old = h.socket();
    h.advance(1001.);
    let before = h.engine.store().positions().to_vec();
    h.now = 937_152.;
    // The browser can deliver a message before its delayed timer callback.
    h.message(old, STATE);
    assert_eq!(h.closed, [old], "close times: {:?}", h.closed_at);
    assert_eq!(h.engine.store().positions(), before);
    h.advance(938_153.);
    let replacement = h.socket();
    assert_ne!(replacement, old);
    h.event(Event::WsOpened {
        socket: replacement,
    });
    h.message(old, STATE);
    h.advance(h.now + 30_000.);
    assert_eq!(
        h.closed,
        [old, replacement],
        "retired frames cannot satisfy a replacement's watch"
    );
}

#[test]
fn duplicate_open_and_invalid_or_foreign_snapshots_never_renew_the_deadline() {
    let mut h = Harness::private();
    let socket = h.socket();
    h.advance(15_000.);
    h.event(Event::WsOpened { socket });
    for text in [
        r#"{"channel":"clearinghouseState","data":{}}"#,
        r#"{"channel":"clearinghouseState","data":{"dex":"foreign","clearinghouseState":{"assetPositions":[]}}}"#,
        r#"{"channel":"orderUpdates","data":[]}"#,
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"clearinghouseState"}}}"#,
    ] {
        h.message(socket, text);
    }
    h.advance(31_000.);
    assert_eq!(h.closed, [socket]);
}

#[test]
fn core_snapshot_is_required_and_optional_hip3_and_spot_scopes_are_best_effort() {
    let mut h = Harness::account(false, true, "unifiedAccount");
    let socket = h.socket();
    h.advance(1001.);
    h.message(socket, r#"{"channel":"clearinghouseState","data":{"dex":"xyz","clearinghouseState":{"assetPositions":[]}}}"#);
    h.message(socket, r#"{"channel":"spotState","data":{"balances":[]}}"#);
    h.advance(31_000.);
    assert_eq!(
        h.closed,
        [socket],
        "HIP-3 and spot cannot mask a missing core snapshot"
    );

    let mut h = Harness::account(false, true, "unifiedAccount");
    let socket = h.socket();
    h.advance(1001.);
    h.message(socket, STATE);
    h.advance(121_000.);
    assert!(
        h.closed.is_empty(),
        "absent optional snapshots must not cause churn"
    );
}

#[test]
fn consecutive_bootstrap_misses_back_off_despite_healthy_public_traffic_and_warn_once() {
    let mut h = Harness::private();
    for (miss, deadline_ms) in [30_000., 60_000., 120_000., 240_000., 300_000., 300_000.]
        .into_iter()
        .enumerate()
    {
        let socket = h.socket();
        let opened = h.now;
        h.advance(opened + deadline_ms - 1.);
        assert_eq!(
            h.closed.len(),
            miss,
            "bootstrap miss {miss} keeps its growing deadline"
        );
        h.advance(opened + deadline_ms);
        assert_eq!(h.closed.len(), miss + 1);
        assert_eq!(h.closed.last(), Some(&socket));
        assert_eq!(
            h.bootstrap_warnings, 1,
            "one warning per incomplete-bootstrap episode"
        );
        let replacement = h.next_connection();
        assert_ne!(replacement, socket);
        h.event(Event::WsOpened {
            socket: replacement,
        });
    }

    let gaps: Vec<_> = h
        .closed_at
        .windows(2)
        .map(|pair| pair[1].1 - pair[0].1)
        .collect();
    assert!(
        gaps[0] < gaps[1] && gaps[1] < gaps[2],
        "bootstrap-miss reconnect gaps grow: {gaps:?}"
    );

    h.message(h.socket(), STATE);
    h.event(Event::WsClosed {
        socket: h.socket(),
        code: 1006,
    });
    let closed_before = h.closed.len();
    let replacement = h.next_connection();
    h.event(Event::WsOpened {
        socket: replacement,
    });
    let opened = h.now;
    h.advance(opened + 29_999.);
    assert_eq!(
        h.closed.len(),
        closed_before,
        "close times: {:?}",
        h.closed_at
    );
    h.advance(opened + 30_000.);
    assert_eq!(h.closed.len(), closed_before + 1);
    assert_eq!(
        h.closed.last(),
        Some(&replacement),
        "a completed watch resets the miss streak"
    );
    assert_eq!(
        h.bootstrap_warnings, 2,
        "a new failure episode reports once again"
    );
}

#[test]
fn standalone_spot_requires_its_own_initial_state() {
    for receive_state in [false, true] {
        let mut h = Harness::account(true, false, "disabled");
        let socket = h.socket();
        h.advance(1001.);
        h.message(socket, STATE);
        if receive_state {
            h.message(
                socket,
                r#"{"channel":"spotState","data":{"spotState":{"balances":[]}}}"#,
            );
        }
        h.advance(31_000.);
        assert_eq!(h.closed.is_empty(), receive_state);
    }
}

#[test]
fn early_or_stale_watch_callbacks_do_not_shorten_or_extend_the_original_deadline() {
    let mut h = Harness::private();
    let socket = h.socket();
    h.advance(1001.);
    let timer = h.timers.iter().find(|(_, at)| *at == 31_000.).unwrap().0;
    h.now = 15_000.;
    h.timers.retain(|(id, _)| *id != timer);
    h.event(Event::TimerFired { timer });
    assert!(h.closed.is_empty());
    let armed: Vec<_> = h.timers.iter().filter(|(_, at)| *at == 31_000.).collect();
    assert_eq!(armed.len(), 1);
    assert_ne!(armed[0].0, timer);
    h.event(Event::TimerFired { timer });
    assert_eq!(h.timers.iter().filter(|(_, at)| *at == 31_000.).count(), 1);
    h.advance(31_000.);
    assert_eq!(h.closed, [socket]);
    h.event(Event::TimerFired { timer });
    assert_eq!(h.closed, [socket]);
}

#[test]
fn invalid_watch_clock_preserves_the_deadline_for_the_next_finite_turn() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::private();
        let socket = h.socket();
        h.advance(1001.);
        let timer = h.timers.iter().find(|(_, at)| *at == 31_000.).unwrap().0;
        h.timers.retain(|(id, _)| *id != timer);
        h.now = invalid;
        h.event(Event::TimerFired { timer });
        assert!(h.closed.is_empty());
        assert!(h.timers.iter().all(|(_, at)| at.is_finite()));
        h.now = 2000.;
        h.message(socket, PONG);
        assert_eq!(h.timers.iter().filter(|(_, at)| *at == 31_000.).count(), 1);
        h.advance(31_000.);
        assert_eq!(h.closed, [socket]);
    }
}
