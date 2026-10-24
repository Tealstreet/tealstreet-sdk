//! Original synchronous Aster controls; peer roots use genuine engine entries.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Command, CommandId, Cx, Engine, ErrorKind, Event, Notice, Query, Response, SocketId,
    TimerId, Venue,
};
use safe_cex_venue_asterdex::{AsterDex, Pending, sign::Version};
use support::Options;

const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;
const PEER: &str = "__fixture_peer_root__";
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
struct Peer {
    context: Context,
    timer: TimerId,
}
struct Probe {
    inner: AsterDex,
    peers: Vec<Peer>,
    live_before: usize,
    live_after: usize,
}
impl Probe {
    fn live(&self, cx: &Cx<'_, Pending>) -> usize {
        self.peers
            .iter()
            .filter(|peer| cx.validate_read_root_context(&peer.context).is_ok())
            .count()
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(index) = self.peers.iter().position(|peer| peer.timer == timer) {
            let peer = self.peers.remove(index);
            cx.done_context(&peer.context, "[]".into())
                .expect("real peer completion frees its slot");
        } else {
            self.inner.on_timer(cx, timer);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.live_before = self.live(cx);
        if matches!(&command, Command::FetchOrderBook { symbol, .. } if symbol == PEER) {
            if let Ok(context) = cx.admit_http_command(policy()) {
                let timer = cx.set_timer(1000.);
                self.peers.push(Peer { context, timer });
            }
        } else {
            self.inner.on_command(cx, command);
        }
        self.live_after = self.live(cx);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}
#[derive(Clone, Copy, Debug)]
enum Local {
    Keyless,
    InvalidRange,
    OrderWindow,
    Lookup,
    MissingFills,
    Precision,
    V1Orders,
    Leverage,
    MissingLeverage,
    Interval,
}
impl Local {
    fn command(self, command: CommandId) -> Command {
        let query = match self {
            Self::Keyless => Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(f64::NAN),
                to_ms: Some(-1.),
                limit: None,
            },
            Self::InvalidRange => Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(1.),
                to_ms: Some(1.),
                limit: None,
            },
            Self::OrderWindow => Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(0.),
                to_ms: Some(7. * 86_400_000. + 1.),
                limit: None,
            },
            Self::Lookup => Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "caller".into(),
            },
            Self::MissingFills => Query::Fills {
                symbol: Some(String::new()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
            Self::Precision => Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(1e20),
                to_ms: Some(1e20),
                limit: None,
            },
            Self::V1Orders => Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
            Self::Leverage | Self::MissingLeverage => {
                return Command::FetchLeverage {
                    command,
                    symbol: "BTCUSDT".into(),
                };
            }
            Self::Interval => {
                return Command::FetchCandles {
                    command,
                    symbol: "BTCUSDT".into(),
                    interval: Interval::S1,
                    limit: None,
                    from_ms: None,
                    to_ms: None,
                };
            }
        };
        Command::Query { command, query }
    }
    fn expected(self) -> Result<&'static str, &'static str> {
        match self {
            Self::Keyless | Self::MissingFills | Self::V1Orders => Ok("[]"),
            Self::Leverage => Ok("{\"buyLeverage\":25,\"sellLeverage\":25}"),
            Self::InvalidRange | Self::Precision => Err("Invalid Aster history time range"),
            Self::OrderWindow => {
                Err("Aster order history requires a time window of at most seven days")
            }
            Self::Lookup => Err("Aster does not support caller client order IDs"),
            Self::MissingLeverage => Err("Aster leverage is not loaded for BTCUSDT"),
            Self::Interval => Err("AsterDEX does not support 1s candles"),
        }
    }
}
fn engine(local: Local, testnet: bool) -> Engine<Probe> {
    let keyed = !matches!(local, Local::Keyless);
    let (inner, signer) = Options {
        key: keyed.then(|| "local-fixture-key".into()),
        secret: keyed.then(|| "local-fixture-secret".into()),
        testnet,
        api_version: Some(Version::V1),
        ..Options::default()
    }
    .build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        peers: vec![],
        live_before: 0,
        live_after: 0,
    });
    engine.set_signer(Some(signer)).unwrap();
    if matches!(local, Local::Leverage) {
        engine
            .store_mut()
            .update_setting("BTCUSDT", |setting| setting.leverage = 25.);
    }
    engine
}
fn act(engine: &mut Engine<Probe>, at: f64, local: Local, command: CommandId) -> Vec<Action> {
    engine.handle(at, Event::Command(local.command(command)));
    engine.drain_actions().collect()
}
fn native(output: &[Action], local: Local, owner: CommandId) {
    assert_eq!(
        output.len(),
        1,
        "native local terminal has no other effects: {local:?}"
    );
    match (local.expected(), &output[0]) {
        (Ok(expected), Action::Notice(Notice::CommandDone { command, result })) => {
            assert_eq!(*command, owner);
            assert_eq!(result, expected);
        }
        (Err(expected), Action::Notice(Notice::CommandFailed { command, error })) => {
            assert_eq!(*command, owner);
            assert_eq!(error.message, expected);
            assert!(error.kind.is_none());
            assert!(error.outcome.is_none());
        }
        _ => panic!("native terminal mismatch for {local:?}: {output:?}"),
    }
}
fn refused(output: &[Action], owner: CommandId, kind: ErrorKind, message: &str) {
    assert_eq!(
        output.len(),
        1,
        "admission refusal must have zero other effects"
    );
    let Action::Notice(Notice::CommandFailed { command, error }) = &output[0] else {
        panic!("typed failure expected: {output:?}");
    };
    assert_eq!(*command, owner);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.message, message);
    assert!(error.outcome.is_none());
}
fn invalid(local: Local) {
    for testnet in [false, true] {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
            let mut engine = engine(local, testnet);
            let output = act(&mut engine, now, local, OWNER);
            refused(
                &output,
                OWNER,
                ErrorKind::InvalidInput,
                "Invalid request context",
            );
        }
    }
}
fn capacity(local: Local) {
    for testnet in [false, true] {
        let mut engine = engine(local, testnet);
        for command in 1..=256 {
            engine.handle(
                NOW,
                Event::Command(Command::FetchOrderBook {
                    command,
                    symbol: PEER.into(),
                    limit: None,
                }),
            );
            assert!(
                engine
                    .drain_actions()
                    .all(|a| matches!(a, Action::SetTimer { .. }))
            );
        }
        assert_eq!(engine.venue().peers.len(), 256);
        let before = engine.snapshot_frame(NOW).to_vec();
        let output = act(&mut engine, NOW, local, OWNER);
        refused(
            &output,
            OWNER,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
        assert_eq!(engine.venue().live_before, 256);
        assert_eq!(engine.venue().live_after, 256);
        assert_eq!(before, engine.snapshot_frame(NOW));
        let timer = engine.venue().peers[0].timer;
        engine.handle(NOW + 1000., Event::TimerFired { timer });
        let released: Vec<_> = engine.drain_actions().collect();
        assert!(released.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, result }) if result == "[]")));
        assert!(!released.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: OWNER, .. }
                    | Notice::CommandFailed { command: OWNER, .. }
            )
        )));
        native(&act(&mut engine, NOW + 1001., local, OWNER), local, OWNER);
        assert_eq!(engine.venue().live_before, 255);
        assert_eq!(engine.venue().live_after, 255);
    }
}
macro_rules! pair {
    ($invalid:ident, $capacity:ident, $case:ident) => {
        #[test]
        fn $invalid() {
            invalid(Local::$case);
        }
        #[test]
        fn $capacity() {
            capacity(Local::$case);
        }
    };
}
pair!(invalid_keyless_history, capacity_keyless_history, Keyless);
pair!(invalid_history_range, capacity_history_range, InvalidRange);
pair!(invalid_order_window, capacity_order_window, OrderWindow);
pair!(invalid_lookup, capacity_lookup, Lookup);
pair!(invalid_missing_fills, capacity_missing_fills, MissingFills);
pair!(
    invalid_inclusive_precision,
    capacity_inclusive_precision,
    Precision
);
pair!(invalid_v1_orders, capacity_v1_orders, V1Orders);
pair!(invalid_cached_leverage, capacity_cached_leverage, Leverage);
pair!(
    invalid_missing_leverage,
    capacity_missing_leverage,
    MissingLeverage
);
pair!(invalid_candle_interval, capacity_candle_interval, Interval);

#[test]
fn native_local_results_keep_ordering_and_have_no_effects() {
    for local in [
        Local::Keyless,
        Local::InvalidRange,
        Local::OrderWindow,
        Local::Lookup,
        Local::MissingFills,
        Local::Precision,
        Local::V1Orders,
        Local::Leverage,
        Local::MissingLeverage,
        Local::Interval,
    ] {
        for testnet in [false, true] {
            let mut engine = engine(local, testnet);
            native(&act(&mut engine, NOW, local, OWNER), local, OWNER);
        }
    }
}
#[test]
fn repeated_synchronous_reads_release_capacity_and_allow_id_reuse() {
    let mut engine = engine(Local::Leverage, false);
    for index in 0..300 {
        native(
            &act(&mut engine, NOW + f64::from(index), Local::Leverage, OWNER),
            Local::Leverage,
            OWNER,
        );
    }
}
#[test]
fn duplicate_id_keeps_the_retained_owner_and_its_diagnostic() {
    let mut engine = engine(Local::Keyless, false);
    engine.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: OWNER,
            symbol: PEER.into(),
            limit: None,
        }),
    );
    engine.drain_actions().for_each(drop);
    let output = act(&mut engine, NOW, Local::Keyless, OWNER);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: OWNER, .. }
                | Notice::CommandFailed { command: OWNER, .. }
        )
    )));
    assert_eq!(output.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)).count(), 1);
    let timer = engine.venue().peers[0].timer;
    engine.handle(NOW + 1000., Event::TimerFired { timer });
    let output: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone { command: OWNER, .. })
            ))
            .count(),
        1
    );
    native(
        &act(&mut engine, NOW + 1001., Local::Keyless, OWNER),
        Local::Keyless,
        OWNER,
    );
}
#[test]
fn cached_nonpositive_and_nonfinite_leverage_keep_native_error() {
    for value in [0., -1., f64::NAN, f64::INFINITY] {
        let mut engine = engine(Local::MissingLeverage, false);
        engine
            .store_mut()
            .update_setting("BTCUSDT", |setting| setting.leverage = value);
        native(
            &act(&mut engine, NOW, Local::MissingLeverage, OWNER),
            Local::MissingLeverage,
            OWNER,
        );
    }
}
#[test]
fn keyless_public_trades_remain_on_the_native_remote_path() {
    let mut engine = engine(Local::Keyless, false);
    engine.handle(
        NOW,
        Event::Command(Command::Query {
            command: OWNER,
            query: Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(2),
            },
        }),
    );
    let output: Vec<_> = engine.drain_actions().collect();
    assert_eq!(output.iter().filter(|a| matches!(a, Action::Http(request) if request.url.contains("/trades?") && request.url.contains("limit=2"))).count(), 1);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_)
            | Action::Notice(
                Notice::CommandDone { command: OWNER, .. }
                    | Notice::CommandFailed { command: OWNER, .. }
            )
    )));
}

#[test]
fn v3_local_precedence_and_unscoped_remote_order_path_are_distinct() {
    for local in [
        Local::Keyless,
        Local::InvalidRange,
        Local::Lookup,
        Local::MissingFills,
        Local::Leverage,
        Local::MissingLeverage,
        Local::Interval,
    ] {
        let keyed = !matches!(local, Local::Keyless);
        let (inner, signer) = Options {
            key: keyed.then(|| format!("0x{}", "11".repeat(20))),
            secret: keyed.then(|| format!("0x{}", "22".repeat(32))),
            application_id: keyed.then(|| format!("0x{}", "33".repeat(20))),
            api_version: Some(Version::V3),
            ..Options::default()
        }
        .build();
        let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
            inner,
            peers: vec![],
            live_before: 0,
            live_after: 0,
        });
        engine.set_signer(Some(signer)).unwrap();
        if matches!(local, Local::Leverage) {
            engine
                .store_mut()
                .update_setting("BTCUSDT", |setting| setting.leverage = 25.);
        }
        native(&act(&mut engine, NOW, local, OWNER), local, OWNER);
        if matches!(local, Local::InvalidRange) {
            engine.handle(NOW + 1., Event::Command(Local::V1Orders.command(OWNER + 1)));
            let output: Vec<_> = engine.drain_actions().collect();
            assert!(
                output
                    .iter()
                    .any(|a| matches!(a, Action::Http(request) if request.url.ends_with("/time")))
            );
            assert!(!output.iter().any(|a| match a {
                Action::ManagedHttp(_) => true,
                Action::Notice(
                    Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. },
                ) => *command == OWNER + 1,
                _ => false,
            }));
        }
    }
}
