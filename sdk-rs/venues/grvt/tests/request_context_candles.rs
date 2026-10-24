//! Original GRVT public candle controls over real command and dispatch APIs.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_grvt::{Grvt, Options, Pending};
const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;
const PEER: &str = "__grvt_candle_peer";
const RETIRE: &str = "__grvt_candle_retire";
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
const ROWS: &str = r#"{"r":[{"ot":"1700000002000000000","o":"2","h":"3","l":"1","c":"2","vb":"4"},{"ot":"1700000001000000000","o":"1","h":"2","l":"1","c":"2","vb":"3"}]}"#;
struct Probe {
    venue: Grvt,
    roots: Vec<Context>,
    validated: usize,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn guard_pending_placements(&self) -> bool {
        self.venue.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.venue.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.venue.fence_uncertain_cancel_all()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.venue
            .on_http_preparation_failed(cx, pending, reason, error)
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.venue.on_stream_error(cx, pending, response)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer)
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == PEER => {
                if let Ok(root) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.roots.push(root);
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["validate_peers"] => {
                for root in &self.roots {
                    cx.validate_http_context(root).unwrap();
                }
                self.validated = self.roots.len();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == [RETIRE] => {
                let root = self.roots.remove(0);
                cx.fail_context(&root, "retired fixture peer").unwrap();
            }
            _ => self.venue.on_command(cx, command),
        }
    }
}

fn engine(testnet: bool) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: Grvt::new(Options {
            testnet,
            public_data_only: true,
            ..Default::default()
        }),
        roots: vec![],
        validated: 0,
    });
    e.start(NOW);
    let requests = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("/lite/v1/all_instruments"));
    e.handle(
        NOW,
        Event::HttpResponse {
            request: requests[0].id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOG,
            },
        },
    );
    prepared_actions(&mut e, NOW);
    assert_eq!(
        e.store().market("BTC_USDT_PERP").unwrap().id,
        "BTC_USDT_Perp"
    );
    e.take_frame(NOW);
    e
}
#[derive(Clone, Copy)]
enum Local {
    Interval,
    Market,
    Bounds,
}
fn command(local: Option<Local>) -> Command {
    Command::FetchCandles {
        command: OWNER,
        symbol: if matches!(local, Some(Local::Interval | Local::Market)) {
            "MISSING"
        } else {
            "BTC_USDT_PERP"
        }
        .into(),
        interval: if matches!(local, Some(Local::Interval)) {
            Interval::S1
        } else {
            Interval::M1
        },
        limit: Some(2),
        from_ms: Some(if matches!(local, Some(Local::Bounds)) {
            f64::NAN
        } else {
            NOW
        }),
        to_ms: None,
    }
}
fn ask(e: &mut Engine<Probe>, now: f64, wait: u32, local: Option<Local>) -> Vec<Action> {
    e.handle_command(now, command(local), Some(wait)).unwrap();
    e.drain_actions().collect()
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
            | Action::Sign { .. }
            | Action::SignedOutbound { .. }
            | Action::SignPayload { .. }
            | Action::WsConnect { .. }
            | Action::WsSend { .. }
    )));
    assert!(!actions.iter().any(|action| matches!(action,
        Action::ManagedHttp(managed) if matches!(&**managed, HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
}
fn prep(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "candle must prepare before HTTP"
    );
    let mut found = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(action) => match &**action {
            HttpAction::Prepare {
                preparation,
                request,
                intent,
            } => {
                assert_eq!(*intent, RoutingIntent::Public);
                Some((*preparation, request.clone()))
            }
            _ => None,
        },
        _ => None,
    });
    let p = found.next().expect("one genuine candle Prepare");
    assert!(found.next().is_none());
    p
}
fn bind(
    e: &mut Engine<Probe>,
    now: f64,
    p: Preparation,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation, p);
                    Some((permit, request))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("real candle Bind")
}
fn release(e: &mut Engine<Probe>, now: f64, actions: &[Action]) -> HttpRequest {
    let raw = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if let Some(r) = raw.first() {
        assert_eq!(raw.len(), 1);
        return r.clone();
    }
    let (p, _) = prep(actions);
    let (permit, r) = bind(e, now, p);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    r
}
fn reply(e: &mut Engine<Probe>, now: f64, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
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
    );
    e.drain_actions().collect()
}
fn failed(e: &mut Engine<Probe>, now: f64, r: &HttpRequest) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpFailed {
            request: r.id,
            error: "network unavailable",
        },
    );
    e.drain_actions().collect()
}
fn wire(r: &HttpRequest, testnet: bool) {
    assert_eq!(r.method, Method::Post);
    assert_eq!(
        r.url,
        format!(
            "https://market-data{}.grvt.io/lite/v1/kline",
            if testnet { ".testnet" } else { "" }
        )
    );
    assert_eq!(r.headers, vec![("Content-Type", "application/json".into())]);
    assert_eq!(r.route, None);
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"i":"BTC_USDT_Perp","t":"TRADE","i1":"CI_1_M","st":"1700000000000000000","l":2}"#)
    );
}
fn done(actions: &[Action]) -> serde_json::Value {
    let values = outcomes(actions, OWNER);
    assert_eq!(values.len(), 1);
    serde_json::from_str(values[0].as_ref().unwrap()).unwrap()
}
fn strict_error(actions: &[Action], message: &str) {
    no_io(actions);
    let values = outcomes(actions, OWNER);
    assert_eq!(values.len(), 1);
    let error = values[0].as_ref().unwrap_err();
    assert_eq!(error.message, message);
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
}
fn peer(e: &mut Engine<Probe>, id: u32) {
    e.handle_command(
        NOW,
        Command::FetchOrderBook {
            command: id,
            symbol: PEER.into(),
            limit: None,
        },
        Some(60_000),
    )
    .unwrap();
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    assert!(outcomes(&actions, id).is_empty());
}
#[test]
fn mainnet_requires_managed_prepare() {
    let mut e = engine(false);
    prep(&ask(&mut e, NOW, 60_000, None));
}
#[test]
fn testnet_requires_managed_prepare() {
    let mut e = engine(true);
    prep(&ask(&mut e, NOW, 60_000, None));
}
#[test]
fn native_wire_rows_and_empty_results_on_both_hosts() {
    for testnet in [false, true] {
        for body in [ROWS, r#"{"result":[]}"#] {
            let mut e = engine(testnet);
            let actions = ask(&mut e, NOW, 60_000, None);
            let r = release(&mut e, NOW, &actions);
            wire(&r, testnet);
            let before = e.snapshot_frame(NOW).to_vec();
            let actions = reply(&mut e, NOW + 1., &r, 200, body);
            let rows = done(&actions);
            if body == ROWS {
                assert_eq!(rows.as_array().unwrap().len(), 2);
                assert_eq!(rows[0]["timestamp"], 1700000001.);
                assert_eq!(rows[1]["timestamp"], 1700000002.);
            } else {
                assert_eq!(rows, serde_json::json!([]));
            }
            assert_eq!(e.snapshot_frame(NOW), before);
            assert!(reply(&mut e, NOW + 2., &r, 200, ROWS).is_empty());
        }
    }
}
#[test]
fn native_validation_order_and_messages() {
    for (local, message) in [
        (Local::Interval, "GRVT does not support 1s candles"),
        (Local::Market, "Unknown GRVT market"),
        (Local::Bounds, "Invalid GRVT candle bounds"),
    ] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 60_000, Some(local));
        strict_error(&actions, message);
    }
}
#[test]
fn native_invalid_rows_status_and_code_are_strict() {
    for (status, body, message) in [
        (
            200,
            r#"{"r":[{"ot":"1700000000000000000","o":"1","h":"2","l":"1","vb":"2"}]}"#,
            "Invalid GRVT candles",
        ),
        (503, r#"{}"#, "GRVT candle read failed"),
        (
            200,
            r#"{"c":"1000","m":"auth expired","r":[]}"#,
            "GRVT candle read failed",
        ),
    ] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 60_000, None);
        let r = release(&mut e, NOW, &actions);
        let actions = reply(&mut e, NOW + 1., &r, status, body);
        strict_error(&actions, message);
    }
}
#[test]
fn native_transport_budget_is_initial_plus_five_immediate_attempts() {
    let mut e = engine(false);
    let mut actions = ask(&mut e, NOW, 120_000, None);
    for attempt in 0..6 {
        let r = release(&mut e, NOW + f64::from(attempt), &actions);
        wire(&r, false);
        actions = failed(&mut e, NOW + f64::from(attempt) + 0.5, &r);
        if attempt < 5 {
            assert!(outcomes(&actions, OWNER).is_empty());
        } else {
            strict_error(&actions, "GRVT candle read failed");
            assert!(
                failed(&mut e, NOW + 7., &r).is_empty(),
                "no seventh physical attempt or duplicate terminal"
            );
        }
    }
}
fn capacity(local: Option<Local>) {
    let mut e = engine(false);
    for id in 1000..1256 {
        peer(&mut e, id)
    }
    let actions = ask(&mut e, NOW, 60_000, local);
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    let error = values[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(outcomes(&actions, 1000).len(), 1);
    let actions = ask(&mut e, NOW, 60_000, local);
    if let Some(local) = local {
        let message = match local {
            Local::Interval => "GRVT does not support 1s candles",
            Local::Market => "Unknown GRVT market",
            Local::Bounds => "Invalid GRVT candle bounds",
        };
        strict_error(&actions, message);
    } else {
        prep(&actions);
    }
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        }),
    );
    assert_eq!(e.venue().validated, 255);
}
#[test]
fn capacity_precedes_interval_refusal() {
    capacity(Some(Local::Interval))
}
#[test]
fn capacity_precedes_market_refusal() {
    capacity(Some(Local::Market))
}
#[test]
fn capacity_precedes_bounds_refusal() {
    capacity(Some(Local::Bounds))
}
#[test]
fn capacity_precedes_remote_work_and_recovers() {
    capacity(None)
}
fn clock(local: Option<Local>) {
    let mut e = engine(false);
    e.handle(f64::NAN, Event::Command(command(local)));
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    let error = values[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, None);
}
#[test]
fn invalid_clock_precedes_local_refusal() {
    clock(Some(Local::Interval))
}
#[test]
fn invalid_clock_precedes_remote_issue() {
    clock(None)
}
#[test]
fn unclaimed_data_cannot_publish() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let (p, r) = prep(&actions);
    let actions = reply(&mut e, NOW + 1., &r, 200, ROWS);
    no_io(&actions);
    assert!(outcomes(&actions, OWNER).is_empty());
    let (permit, r) = bind(&mut e, NOW + 2., p);
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
    assert!(done(&reply(&mut e, NOW + 3., &r, 200, ROWS)).is_array());
}
#[test]
fn caller_expiry_refuses_late_claimed_data() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 1000, None);
    let r = release(&mut e, NOW, &actions);
    let actions = reply(&mut e, NOW + 1000., &r, 200, ROWS);
    let results = outcomes(&actions, OWNER);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    assert!(reply(&mut e, NOW + 1001., &r, 200, ROWS).is_empty());
}
#[test]
fn caller_timer_expires_without_an_http_callback() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 1000, None);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1000. => Some(*timer),
            _ => None,
        })
        .expect("original caller Plan timer");
    e.handle(NOW + 1000., Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        outcomes(&actions, OWNER)[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
#[test]
fn preparation_failure_is_typed_and_never_transport_retry() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let (p, _) = prep(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::Invalid,
        VenueError::new("route refused")
            .kind(ErrorKind::InvalidInput)
            .retry_after_ms(17.),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let results = outcomes(&actions, OWNER);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.message, "route refused");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.retry_after_ms, Some(17.));
    assert!(actions.iter().all(|a|!matches!(a,Action::ManagedHttp(action)if !matches!(&**action,HttpAction::Cancel{preparation,..}if *preparation==p))));
    assert_eq!(actions.iter().filter(|action| matches!(action,
        Action::ManagedHttp(managed) if matches!(&**managed, HttpAction::Cancel { preparation, signing: None } if *preparation == p))).count(), 1);
}
#[test]
fn physical_timeout_keeps_a_fresh_twenty_second_retry_window() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let (p, _) = prep(&actions);
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("first shared physical20s timer");
    let (permit, _) = bind(&mut e, NOW, p);
    assert!(matches!(e.claim_http(NOW, permit), Ok(Claim::Released(_))));
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(outcomes(&actions, OWNER).is_empty());
    let r = release(&mut e, NOW + 20_000., &actions);
    assert!(done(&reply(&mut e, NOW + 39_000., &r, 200, ROWS)).is_array());
}
#[test]
fn short_remaining_caller_cap_wins_over_the_next_full_window() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 1000, None);
    let (p, _) = prep(&actions);
    let (permit, r) = bind(&mut e, NOW, p);
    assert!(matches!(e.claim_http(NOW, permit), Ok(Claim::Released(_))));
    let actions = failed(&mut e, NOW + 900., &r);
    let (p, _) = prep(&actions);
    let (permit, r) = bind(&mut e, NOW + 900., p);
    assert!(r.timeout_ms.is_some_and(|ms| ms <= 100));
    assert!(matches!(
        e.claim_http(NOW + 900., permit),
        Ok(Claim::Released(_))
    ));
    let actions = reply(&mut e, NOW + 1000., &r, 200, ROWS);
    assert_eq!(
        outcomes(&actions, OWNER)[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
#[test]
fn stale_reply_cannot_consume_a_retried_attempt() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let (p, _) = prep(&actions);
    let (permit, old) = bind(&mut e, NOW, p);
    assert!(matches!(e.claim_http(NOW, permit), Ok(Claim::Released(_))));
    let actions = failed(&mut e, NOW + 1., &old);
    let (p, _) = prep(&actions);
    let actions = reply(&mut e, NOW + 2., &old, 200, ROWS);
    assert!(outcomes(&actions, OWNER).is_empty());
    let (permit, r) = bind(&mut e, NOW + 3., p);
    assert!(matches!(
        e.claim_http(NOW + 3., permit),
        Ok(Claim::Released(_))
    ));
    assert!(done(&reply(&mut e, NOW + 4., &r, 200, ROWS)).is_array());
}
#[test]
fn duplicate_command_preserves_original_peer() {
    let mut e = engine(false);
    peer(&mut e, OWNER);
    let actions = ask(&mut e, NOW, 60_000, None);
    assert!(
        matches!(actions.as_slice(),[Action::Notice(Notice::Event(event))]if event.code==safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        outcomes(&actions, OWNER)[0].as_ref().unwrap_err().message,
        "retired fixture peer"
    );
}

#[test]
fn native_limit_bounds_omission_and_reversed_range_are_preserved() {
    for (limit, expected) in [(None, 500), (Some(0), 1), (Some(u32::MAX), 1000)] {
        let mut e = engine(false);
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command: OWNER,
                symbol: "BTC_USDT_PERP".into(),
                interval: Interval::M1,
                limit,
                from_ms: Some(1.000001),
                to_ms: Some(0.5),
            },
            Some(60_000),
        )
        .unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        let r = release(&mut e, NOW, &actions);
        let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["l"], expected);
        assert_eq!(body["st"], "1000001");
        assert_eq!(body["et"], "500000");
    }
    let mut e = engine(false);
    e.handle_command(
        NOW,
        Command::FetchCandles {
            command: OWNER,
            symbol: "BTC_USDT_PERP".into(),
            interval: Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
        Some(60_000),
    )
    .unwrap();
    let actions = e.drain_actions().collect::<Vec<_>>();
    let r = release(&mut e, NOW, &actions);
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"i":"BTC_USDT_Perp","t":"TRADE","i1":"CI_1_M","l":500}"#)
    );
}
#[test]
fn native_partial_rows_are_discarded_before_transport_retry() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let r = release(&mut e, NOW, &actions);
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: r.id,
            data:
                br#"{"r":[{"ot":"1700000000000000000","o":"1","h":"2","l":"1","c":"1","vb":"2"},"#,
        },
    );
    assert!(outcomes(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty());
    let actions = failed(&mut e, NOW + 3., &r);
    let next = release(&mut e, NOW + 3., &actions);
    let rows = done(&reply(&mut e, NOW + 4., &next, 200, ROWS));
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["timestamp"], 1700000001.);
}
#[test]
fn invalid_caller_wait_refuses_before_native_issue() {
    let mut e = engine(false);
    assert_eq!(
        e.handle_command(NOW, command(None), Some(0)),
        Err(Error::Invalid)
    );
    assert!(e.drain_actions().next().is_none());
}

fn integrity_failure(payload: &[u8]) {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let r = release(&mut e, NOW, &actions);
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: r.id,
            data: payload,
        },
    );
    let mut actions = e.drain_actions().collect::<Vec<_>>();
    e.handle(NOW + 3., Event::HttpEnd { request: r.id });
    actions.extend(e.drain_actions());
    let prepared = actions
        .iter()
        .filter(|a| matches!(a,Action::ManagedHttp(a) if matches!(&**a,HttpAction::Prepare { .. })))
        .count();
    assert_eq!(
        prepared,
        0,
        "integrity failure prepared another attempt; terminal count={}",
        outcomes(&actions, OWNER).len()
    );
    assert_eq!(
        outcomes(&actions, OWNER).len(),
        1,
        "malformed ingress must seal once"
    );
    no_io(&actions);
    let failures = outcomes(&actions, OWNER);
    let error = failures[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    assert!(reply(&mut e, NOW + 4., &r, 200, ROWS).is_empty());
}
#[test]
fn non_utf8_ingress_is_strict() {
    integrity_failure(b"\xff");
}
#[test]
fn unfinished_stream_is_strict() {
    integrity_failure(
        br#"{"r":[{"ot":"1700000000000000000","o":"1","h":"2","l":"1","c":"1","vb":"2"},"#,
    );
}

#[test]
fn late_failed_attempt_cannot_fail_the_live_replacement() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let first = release(&mut e, NOW, &actions);
    let replacement = failed(&mut e, NOW + 1., &first);
    prep(&replacement);
    let late = failed(&mut e, NOW + 2., &first);
    no_io(&late);
    assert!(outcomes(&late, OWNER).is_empty());
    let second = release(&mut e, NOW + 3., &replacement);
    assert_eq!(
        done(&reply(&mut e, NOW + 4., &second, 200, ROWS))
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn preparation_timeout_is_strict_while_the_caller_is_live() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, None);
    let (preparation, _) = prep(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        preparation,
        Error::TimedOut,
        VenueError::new("preparation timed out").kind(ErrorKind::Timeout),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    let error = values[0].as_ref().unwrap_err();
    assert_eq!(error.message, "preparation timed out");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}
