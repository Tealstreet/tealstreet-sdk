//! Compiling original controls for native local settings and schedule refusal custody.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome,
    Response, SocketId, TimerId, Venue, VenueAction, VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options, Pending};
const NOW: f64 = 1000.;
const OWNER: u32 = 700;
const SYMBOL: &str = "UBTC/USDC";
const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100"}]]"#;
const SPOT: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"UBTC","index":1,"szDecimals":5}],"universe":[{"name":"@1","tokens":[1,0],"index":1}]},[{"coin":"@1","midPx":"60000","markPx":"60010","prevDayPx":"59000","dayNtlVlm":"1000"}]]"#;
struct Probe {
    inner: Hyperliquid,
    peers: Vec<Context>,
    validated: usize,
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn guard_pending_placements(&self) -> bool {
        self.inner.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.inner.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.inner.fence_uncertain_cancel_all()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId) {
        self.inner.on_open(cx, s)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, c: u16) {
        self.inner.on_close(cx, s, c)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, t: &str) {
        self.inner.on_message(cx, s, t)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, t: TimerId) {
        self.inner.on_timer(cx, t)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.inner.on_response(cx, p, r)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, s: &str) {
        self.inner.on_item(cx, p, s)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, s: &str) {
        self.inner.on_request_failed(cx, p, s)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.inner.on_http_failed(cx, p, e)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.inner.on_sign_error(cx, p, e)
    }
    fn book(&self, s: &str) -> Option<&Book> {
        self.inner.book(s)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_venue_action(&mut self, cx: &mut Cx<'_, Pending>, command: u32, action: VenueAction) {
        self.inner.on_venue_action(cx, command, action)
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__peer" => {
                if let Ok(c) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.peers.push(c)
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["validate_peers"] => {
                for peer in &self.peers {
                    cx.validate_http_context(peer).unwrap();
                }
                self.validated = self.peers.len();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["seed_hedged"] => {
                cx.store.set_hedged(true);
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire"] => {
                cx.fail_context(&self.peers.remove(0), "original peer retired")
                    .unwrap();
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
fn reply(e: &mut Engine<Probe>, now: f64, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    )
}
fn kind(r: &HttpRequest) -> String {
    serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap()["type"]
        .as_str()
        .unwrap()
        .into()
}
fn boot(mode: Mode) -> Engine<Probe> {
    let merged = matches!(mode, Mode::Merged);
    let options = Options {
        testnet: false,
        public_data_only: true,
        enable_spot_markets: merged,
        ..Default::default()
    };
    let inner = if !matches!(mode, Mode::Spot) {
        Hyperliquid::new(options)
    } else {
        Hyperliquid::new_spot(options)
    };
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        peers: vec![],
        validated: 0,
    });
    e.start(NOW);
    let initial: Vec<_> = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    for r in &initial {
        match kind(r).as_str() {
            "metaAndAssetCtxs" => reply(&mut e, NOW, r, 200, META),
            "spotMetaAndAssetCtxs" => reply(&mut e, NOW, r, 200, SPOT),
            other => panic!("unexpected initial {other}"),
        }
    }
    let follow: Vec<_> = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    for r in &follow {
        assert_eq!(kind(r), "perpDexs");
        reply(&mut e, NOW, r, 200, "[null]")
    }
    e.drain_actions().for_each(drop);
    if merged {
        assert_eq!(e.store().market(SYMBOL).unwrap().id, "@1");
    }
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["seed_hedged".into()],
        }),
    );
    assert!(e.drain_actions().next().is_none());
    assert!(e.store().hedged());
    e.take_frame(NOW);
    e
}

#[derive(Clone, Copy)]
enum Mode {
    Spot,
    Perps,
    Merged,
}
#[derive(Clone, Copy)]
enum Case {
    SpotLeverage,
    SpotLeverageRefused,
    SpotCross,
    SpotIsolated,
    MergedLeverage,
    MergedMargin,
    Hedged,
    OneWay,
    SpotSchedule,
    InvalidTime,
}
impl Case {
    fn mode(self) -> Mode {
        match self {
            Self::SpotLeverage
            | Self::SpotLeverageRefused
            | Self::SpotCross
            | Self::SpotIsolated
            | Self::SpotSchedule => Mode::Spot,
            Self::MergedLeverage | Self::MergedMargin => Mode::Merged,
            _ => Mode::Perps,
        }
    }
    fn command(self, command: u32) -> Command {
        match self {
            Self::SpotLeverage | Self::SpotLeverageRefused | Self::MergedLeverage => {
                Command::SetLeverage {
                    command,
                    symbol: if matches!(self, Self::MergedLeverage) {
                        SYMBOL
                    } else {
                        "UNKNOWN"
                    }
                    .into(),
                    leverage: if matches!(self, Self::SpotLeverage) {
                        1.
                    } else {
                        9.
                    },
                    side: None,
                }
            }
            Self::SpotCross | Self::SpotIsolated | Self::MergedMargin => Command::SetMarginType {
                command,
                symbol: if matches!(self, Self::MergedMargin) {
                    SYMBOL
                } else {
                    "UNKNOWN"
                }
                .into(),
                isolated: !matches!(self, Self::SpotCross),
            },
            Self::Hedged | Self::OneWay => Command::SetPositionMode {
                command,
                symbol: None,
                hedged: matches!(self, Self::Hedged),
            },
            Self::SpotSchedule | Self::InvalidTime => Command::Venue {
                command,
                action: VenueAction::ScheduleCancel {
                    time_ms: Some(f64::NAN),
                },
            },
        }
    }
    fn refusal(self) -> Option<(&'static str, ErrorKind)> {
        match self {
            Self::SpotLeverageRefused => Some((
                "Hyperliquid spot supports 1x leverage only",
                ErrorKind::Unsupported,
            )),
            Self::SpotIsolated => Some((
                "Hyperliquid spot supports cross margin only",
                ErrorKind::Unsupported,
            )),
            Self::Hedged => Some((
                "Hyperliquid supports one-way positions only",
                ErrorKind::Unsupported,
            )),
            Self::SpotSchedule => Some((
                "scheduleCancel is not supported by Hyperliquid spot",
                ErrorKind::Unsupported,
            )),
            Self::InvalidTime => Some((
                "Hyperliquid scheduleCancel time must be a safe integer UTC timestamp in milliseconds",
                ErrorKind::InvalidInput,
            )),
            _ => None,
        }
    }
}
fn outcomes(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn no_io(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::SignedOutbound { .. }
            | Action::Sign { .. }
            | Action::ManagedHttp(_)
            | Action::WsSend { .. }
            | Action::WsConnect { .. }
    )));
}
fn native(e: &Engine<Probe>, actions: &[Action], case: Case) {
    no_io(actions);
    let results = outcomes(actions, OWNER);
    assert_eq!(results.len(), 1, "one native terminal");
    if let Some((message, kind)) = case.refusal() {
        let error = results[0].as_ref().unwrap_err();
        assert_eq!(error.message, message);
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    } else {
        assert_eq!(results[0].as_ref().unwrap(), "[]");
    }
    assert_eq!(e.store().hedged(), !matches!(case, Case::OneWay));
}
fn peer(e: &mut Engine<Probe>, command: u32) {
    e.handle_command(
        NOW,
        Command::FetchCandles {
            command,
            symbol: "__peer".into(),
            interval: Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
        Some(60_000),
    )
    .unwrap();
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        outcomes(&actions, command).is_empty(),
        "real peer remains retained"
    );
    no_io(&actions);
}
fn capacity(case: Case) {
    let mut e = boot(case.mode());
    for id in 1000..1256 {
        peer(&mut e, id);
    }
    e.handle_command(NOW, case.command(OWNER), Some(60_000))
        .unwrap();
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let results = outcomes(&actions, OWNER);
    assert_eq!(results.len(), 1, "one shared capacity terminal");
    let error = results[0]
        .as_ref()
        .expect_err("full genuine command pool must refuse the native local path");
    assert_eq!(
        error.kind,
        Some(ErrorKind::Internal),
        "existing hard Capacity projection"
    );
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(
        !actions.iter().any(
            |a| matches!(a,Action::Notice(Notice::CommandError{command,..}) if *command==OWNER)
        )
    );
    assert!(
        e.store().hedged(),
        "refused one-way setting must not project"
    );
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["retire".into()],
        }),
    );
    let retired = e.drain_actions().collect::<Vec<_>>();
    let peer_result = outcomes(&retired, 1000);
    assert_eq!(peer_result.len(), 1, "exact first peer still owned");
    assert_eq!(
        peer_result[0].as_ref().unwrap_err().message,
        "original peer retired"
    );
    e.handle_command(NOW, case.command(OWNER), Some(60_000))
        .unwrap();
    let actions = e.drain_actions().collect::<Vec<_>>();
    native(&e, &actions, case);
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        }),
    );
    assert!(e.drain_actions().next().is_none());
    assert_eq!(
        e.venue().validated,
        255,
        "every retained peer remains valid"
    );
}
macro_rules! controls {
    ($capacity:ident,$native:ident,$case:ident) => {
        #[test]
        fn $capacity() {
            capacity(Case::$case)
        }
        #[test]
        fn $native() {
            let mut e = boot(Case::$case.mode());
            e.handle_command(NOW, Case::$case.command(OWNER), Some(60_000))
                .unwrap();
            let actions = e.drain_actions().collect::<Vec<_>>();
            native(&e, &actions, Case::$case);
        }
    };
}
controls!(capacity_spot_leverage, native_spot_leverage, SpotLeverage);
controls!(
    capacity_spot_leverage_refusal,
    native_spot_leverage_refusal,
    SpotLeverageRefused
);
controls!(capacity_spot_cross, native_spot_cross, SpotCross);
controls!(capacity_spot_isolated, native_spot_isolated, SpotIsolated);
controls!(
    capacity_merged_leverage,
    native_merged_leverage,
    MergedLeverage
);
controls!(capacity_merged_margin, native_merged_margin, MergedMargin);
controls!(capacity_hedged_refusal, native_hedged_refusal, Hedged);
controls!(capacity_one_way, native_one_way, OneWay);
controls!(capacity_spot_schedule, native_spot_schedule, SpotSchedule);
controls!(capacity_invalid_time, native_invalid_time, InvalidTime);
fn clock(case: Case) {
    let mut e = boot(case.mode());
    e.handle(f64::NAN, Event::Command(case.command(OWNER)));
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let result = outcomes(&actions, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0]
        .as_ref()
        .expect_err("invalid external event clock must refuse");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(e.store().hedged());
}
#[test]
fn invalid_clock_precedes_one_way_projection() {
    clock(Case::OneWay)
}
#[test]
fn invalid_clock_precedes_spot_schedule_refusal() {
    clock(Case::SpotSchedule)
}
#[test]
fn duplicate_preserves_original_owner_before_local_setting() {
    let mut e = boot(Mode::Perps);
    peer(&mut e, OWNER);
    e.handle_command(NOW, Case::OneWay.command(OWNER), Some(60_000))
        .unwrap();
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    assert!(outcomes(&actions, OWNER).is_empty());
    assert!(e.store().hedged());
    assert!(
        matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["retire".into()],
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        outcomes(&actions, OWNER)[0].as_ref().unwrap_err().message,
        "original peer retired"
    );
}
#[test]
fn zero_wait_is_rejected_before_venue_entry() {
    let mut e = boot(Mode::Perps);
    assert_eq!(
        e.handle_command(NOW, Case::OneWay.command(OWNER), Some(0)),
        Err(Error::Invalid)
    );
    assert!(e.drain_actions().next().is_none());
    assert!(e.store().hedged());
}
#[test]
fn native_schedule_deadline_below_and_at_five_seconds() {
    for (time, kind, message) in [
        (
            Some(NOW + 4999.),
            ErrorKind::InvalidInput,
            "Hyperliquid scheduleCancel time must be at least 5 seconds in the future",
        ),
        (
            Some(NOW + 5000.),
            ErrorKind::AuthInvalid,
            "Hyperliquid signing key is missing",
        ),
        (
            None,
            ErrorKind::AuthInvalid,
            "Hyperliquid signing key is missing",
        ),
    ] {
        let mut e = boot(Mode::Perps);
        e.handle_command(
            NOW,
            Command::Venue {
                command: OWNER,
                action: VenueAction::ScheduleCancel { time_ms: time },
            },
            Some(60_000),
        )
        .unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        no_io(&actions);
        let results = outcomes(&actions, OWNER);
        assert_eq!(results.len(), 1);
        let error = results[0].as_ref().unwrap_err();
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.message, message);
        assert!(e.store().hedged());
    }
}
