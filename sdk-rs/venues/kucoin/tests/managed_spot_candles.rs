#![cfg(feature = "spot")]
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Permit, Policy, Preparation, Prepared,
    RetryPolicy, RoutingIntent, Scope,
};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, PublicFeedPolicy, RequestId, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_kucoin::{
    Options,
    spot::{KucoinSpot, Pending},
};
use serde_json::{Value, json};

const MARKETS: &str = r#"{"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"100"}]}"#;
const TICKERS: &str = r#"{"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","last":"100","buy":"99","sell":"101"}]}}"#;
const EMPTY: &str = r#"{"code":"200000","data":[]}"#;
const ROW: &str = r#"{"code":"200000","data":[[100,1,2,3,0,4]]}"#;
fn venue(keyed: bool) -> KucoinSpot {
    KucoinSpot::new(if keyed {
        Options {
            key: Some("fixture-key".into()),
            secret: Some("fixture-secret".into()),
            passphrase: Some("fixture-passphrase".into()),
            ..Options::default()
        }
    } else {
        Options {
            public_data_only: true,
            ..Options::default()
        }
    })
}
fn seeded<V: Venue>(venue: V) -> Engine<V> {
    seeded_policy(venue, PublicFeedPolicy::Disabled)
}
fn seeded_policy<V: Venue>(venue: V, policy: PublicFeedPolicy) -> Engine<V> {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.set_public_feed_policy(policy).unwrap();
    e.start(0.);
    for (suffix, body) in [("/symbols", MARKETS), ("/allTickers", TICKERS)] {
        let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with(suffix) => Some(r),
                _ => None,
            })
            .expect("native discovery request");
        response(&mut e, 0., r.id, 200, body);
    }
    if policy == PublicFeedPolicy::Direct {
        let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.ends_with("/bullet-public") => Some(request),
                _ => None,
            })
            .expect("native public bullet is settled before saturation");
        response(
            &mut e,
            0.,
            request.id,
            200,
            r#"{"code":"200000","data":{"token":"fixture-token","instanceServers":[{"endpoint":"wss://fixture.invalid/public","pingInterval":18000,"pingTimeout":10000}]}}"#,
        );
    }
    e.drain_actions().for_each(drop);
    e
}
fn response<V: Venue>(e: &mut Engine<V>, now: f64, request: RequestId, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn query<V: Venue>(
    e: &mut Engine<V>,
    now: f64,
    command: u32,
    symbol: &str,
    interval: Interval,
    limit: Option<u32>,
    range: (Option<f64>, Option<f64>),
) {
    let (from_ms, to_ms) = range;
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: symbol.into(),
            interval,
            limit,
            from_ms,
            to_ms,
        }),
    );
}
fn candles<V: Venue>(e: &mut Engine<V>, now: f64, command: u32) {
    query(
        e,
        now,
        command,
        "BTC-USDT",
        Interval::M1,
        Some(2),
        (None, None),
    );
}
fn headers(r: &HttpRequest) {
    assert_eq!(r.method, Method::Get);
    assert_eq!(
        r.headers,
        [
            ("X-CHANNEL-API-CODE", "3zexp".into()),
            ("Content-Type", "application/json".into())
        ]
    );
    assert!(r.signer_headers.is_empty());
    assert!(r.body.is_none());
}
fn attempt(actions: &[Action]) -> (RequestId, Option<Preparation>) {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/market/candles?") => Some((r.id, None)),
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } if request.url.contains("/market/candles?") => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    headers(request);
                    Some((request.id, Some(*preparation)))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("native candle request identity")
}
fn prepare(actions: &[Action]) -> Preparation {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "HTTP must wait for preparation"
    );
    attempt(actions).1.expect("managed candle preparation")
}
fn bind<V: Venue>(e: &mut Engine<V>, now: f64, token: Preparation) -> (HttpRequest, Permit) {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation, token);
                    Some((permit, request))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared route bind");
    (request, permit)
}
fn released<V: Venue>(e: &mut Engine<V>, now: f64) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/market/candles?") => Some(r),
            _ => None,
        })
        .expect("released candle request")
}
fn timeline(actions: &[Action], command: u32) -> Vec<&'static str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandError { command: id, .. }) if *id == command => {
                Some("error")
            }
            Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command => {
                Some("failed")
            }
            Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command => {
                Some("done")
            }
            _ => None,
        })
        .collect()
}
fn failure(actions: &[Action], command: u32) -> &VenueError {
    let terminal = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(
                n @ (Notice::CommandFailed { command: id, .. }
                | Notice::CommandDone { command: id, .. }),
            ) if *id == command => Some(n),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(terminal.len(), 1, "one terminal for {command}");
    match terminal[0] {
        Notice::CommandFailed { error, .. } => error,
        _ => panic!("expected failure"),
    }
}
fn result(actions: &[Action], command: u32) -> Value {
    assert_eq!(timeline(actions, command), ["done"]);
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some(serde_json::from_str(result).unwrap()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn candle_http_waits_for_public_preparation() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 71);
    let a = e.drain_actions().collect::<Vec<_>>();
    prepare(&a);
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 20_100.))
    );
}
#[test]
fn unreleased_and_unclaimed_candle_replies_cannot_publish() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 71);
    let a = e.drain_actions().collect::<Vec<_>>();
    let (request, token) = attempt(&a);
    response(&mut e, 101., request, 200, ROW);
    assert!(timeline(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    let (r, permit) = bind(
        &mut e,
        102.,
        token.expect("identity alone is not preparation"),
    );
    response(&mut e, 103., r.id, 200, ROW);
    assert!(timeline(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    assert!(matches!(e.claim_http(104., permit), Ok(Claim::Released(_))));
    response(&mut e, 104., r.id, 200, EMPTY);
    assert_eq!(
        result(&e.drain_actions().collect::<Vec<_>>(), 71),
        json!([])
    );
}
fn candle_deadline(sent: bool, response_first: bool) {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 71);
    let (request, timer) = if sent {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 100.);
        let request = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.id),
                _ => None,
            })
            .unwrap();
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
                _ => None,
            })
            .expect("physical deadline");
        (request, timer)
    } else {
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepare(&actions);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
                _ => None,
            })
            .unwrap();
        (token.attempt().request, timer)
    };
    if response_first {
        response(&mut e, 20_100., request, 200, ROW);
    } else {
        e.handle(20_100., Event::TimerFired { timer });
    }
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&a, 71);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(
        error.outcome,
        if sent { None } else { Some(Outcome::NotSent) }
    );
    assert_eq!(timeline(&a, 71), ["error", "failed"]);
    e.handle(20_101., Event::TimerFired { timer });
    assert!(timeline(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    response(&mut e, 20_101., request, 200, ROW);
    assert!(timeline(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    candles(&mut e, 20_102., 71);
    let r = released(&mut e, 20_102.);
    response(&mut e, 20_102., r.id, 200, EMPTY);
    assert_eq!(
        result(&e.drain_actions().collect::<Vec<_>>(), 71),
        json!([])
    );
}

#[test]
fn parked_candle_deadline_settles_once() {
    candle_deadline(false, false);
}

#[test]
fn released_candle_deadline_settles_once() {
    candle_deadline(true, false);
}

#[test]
fn released_response_at_deadline_expires_before_row_admission() {
    candle_deadline(true, true);
}

fn typed_candle_failure(sent: bool) {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 71);
    if sent {
        let r = released(&mut e, 100.);
        e.handle(
            101.,
            Event::HttpFailed {
                request: r.id,
                error: "offline",
            },
        );
    } else {
        let token = prepare(&e.drain_actions().collect::<Vec<_>>());
        e.http_preparation_failed(
            101.,
            token,
            Error::Invalid,
            VenueError::new("unsupported prepared transport").kind(ErrorKind::Unsupported),
        );
    }
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&a, 71);
    assert_eq!(
        error.kind,
        Some(if sent {
            ErrorKind::Network
        } else {
            ErrorKind::Unsupported
        })
    );
    assert_eq!(
        error.outcome,
        if sent { None } else { Some(Outcome::NotSent) }
    );
    assert_eq!(timeline(&a, 71), ["error", "failed"]);
}

#[test]
fn typed_preparation_refusal_keeps_error_companion() {
    typed_candle_failure(false);
}

#[test]
fn released_network_failure_keeps_typed_error_companion() {
    typed_candle_failure(true);
}

#[test]
fn native_wire_limits_ranges_and_unsigned_headers_are_preserved() {
    for keyed in [false, true] {
        for (interval, limit, from, to, expected) in [
            (
                Interval::M1,
                None,
                Some(1_699_999_990_555.),
                1_700_000_001_999.,
                "type=1min&startAt=1699999990&endAt=1700000001",
            ),
            (
                Interval::M1,
                None,
                None,
                1_700_000_001_999.,
                "type=1min&startAt=1699970001&endAt=1700000001",
            ),
            (
                Interval::M1,
                Some(0),
                Some(5.),
                1_700_000_001_999.,
                "type=1min&startAt=1699970001&endAt=1700000001",
            ),
            (
                Interval::M1,
                Some(1),
                Some(5.),
                1_700_000_001_999.,
                "type=1min&startAt=1699999941&endAt=1700000001",
            ),
            (
                Interval::M1,
                Some(2000),
                None,
                1_700_000_001_999.,
                "type=1min&startAt=1699910001&endAt=1700000001",
            ),
            (
                Interval::H2,
                Some(1),
                None,
                1_700_000_001_999.,
                "type=2hour&startAt=1699992801&endAt=1700000001",
            ),
            (
                Interval::M1,
                None,
                Some(-1000.),
                999.,
                "type=1min&startAt=0&endAt=0",
            ),
        ] {
            let mut e = seeded(venue(keyed));
            query(
                &mut e,
                100.,
                71,
                "UNKNOWN-USDT",
                interval,
                limit,
                (from, Some(to)),
            );
            let r = released(&mut e, 100.);
            headers(&r);
            assert_eq!(
                r.url,
                format!(
                    "https://api.kucoin.com/api/v1/market/candles?symbol=UNKNOWN-USDT&{expected}"
                )
            );
            response(&mut e, 100., r.id, 200, EMPTY);
            assert_eq!(
                result(&e.drain_actions().collect::<Vec<_>>(), 71),
                json!([])
            );
        }
    }
}
#[test]
fn native_rows_keep_duplicate_times_oversized_results_and_extra_columns() {
    let mut e = seeded(venue(false));
    query(
        &mut e,
        100.,
        71,
        "MISSING",
        Interval::M1,
        Some(1),
        (None, None),
    );
    let r = released(&mut e, 100.);
    response(
        &mut e,
        100.,
        r.id,
        200,
        r#"{"code":"200000","data":[[200,2,3,4,1,5,999],[1000000000000,6,7,8,5,9],[200,20,30,40,10,50]]}"#,
    );
    assert_eq!(
        result(&e.drain_actions().collect::<Vec<_>>(), 71),
        json!([
            {"timestamp":200.,"open":2.,"close":3.,"high":4.,"low":1.,"volume":5.},
            {"timestamp":200.,"open":20.,"close":30.,"high":40.,"low":10.,"volume":50.},
            {"timestamp":1000000000.,"open":6.,"close":7.,"high":8.,"low":5.,"volume":9.}
        ])
    );
}
#[test]
fn row_and_envelope_errors_preserve_their_different_terminal_timelines() {
    for (body, companion) in [
        (r#"{"code":"200000","data":[[100,1,2,3,0,4],[1,2]]}"#, false),
        (
            r#"{"code":"400002","msg":"denied","data":[[100,1,2,3,0,4]]}"#,
            true,
        ),
        (r#"{"code":"200000"}"#, true),
        (r#"{"code":"200000","data":null}"#, true),
        (r#"{"code":"200000","data":{}}"#, true),
        ("not JSON", true),
    ] {
        let mut e = seeded(venue(false));
        candles(&mut e, 100., 71);
        let r = released(&mut e, 100.);
        response(&mut e, 100., r.id, 200, body);
        let a = e.drain_actions().collect::<Vec<_>>();
        let error = failure(&a, 71);
        if companion {
            assert_eq!(timeline(&a, 71), ["error", "failed"]);
        } else {
            assert_eq!(timeline(&a, 71), ["failed"]);
            assert_eq!(error.message, "invalid KuCoin Spot candles");
        }
    }
}
#[test]
fn duplicate_and_fixed_clock_terminal_reuse_preserve_generation() {
    let mut e = seeded(venue(false));
    for _ in 0..270 {
        query(
            &mut e,
            100.,
            71,
            "BTC-USDT",
            Interval::S1,
            None,
            (None, None),
        );
        let a = e.drain_actions().collect::<Vec<_>>();
        let error = failure(&a, 71);
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.message, "KuCoin Spot does not support 1s candles");
        assert_eq!(timeline(&a, 71), ["failed"]);
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
    let mut previous = None;
    for _ in 0..270 {
        candles(&mut e, 100., 71);
        let r = released(&mut e, 100.);
        candles(&mut e, 100., 71);
        let a = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        assert!(timeline(&a, 71).is_empty());
        if let Some(old) = previous {
            response(&mut e, 100., old, 200, ROW);
            assert!(timeline(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
        }
        response(&mut e, 100., r.id, 200, EMPTY);
        assert_eq!(
            result(&e.drain_actions().collect::<Vec<_>>(), 71),
            json!([])
        );
        previous = Some(r.id);
    }
}
#[test]
fn invalid_clock_entry_is_typed_without_io_or_companion() {
    let mut e = seeded(venue(false));
    candles(&mut e, f64::NAN, 71);
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = failure(&a, 71);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, None);
    assert_eq!(error.message, Error::Invalid.to_string());
    assert_eq!(timeline(&a, 71), ["failed"]);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    candles(&mut e, 100., 71);
    let r = released(&mut e, 100.);
    response(&mut e, 100., r.id, 200, EMPTY);
    assert_eq!(
        result(&e.drain_actions().collect::<Vec<_>>(), 71),
        json!([])
    );
}
struct Capacity {
    venue: KucoinSpot,
    held: Vec<Context>,
}
impl Venue for Capacity {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, r: Response<'_>) {
        self.venue.on_response(cx, pending, r);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.venue
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_stream_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, r: Response<'_>) {
        self.venue.on_stream_error(cx, pending, r);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            if symbols.is_empty() {
                for root in self.held.drain(..) {
                    cx.finish_http_context(&root).unwrap();
                }
            } else {
                assert!(self.held.is_empty());
                let policy = || Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                };
                if symbols[0] == "roots" {
                    for _ in 0..256 {
                        self.held
                            .push(cx.http_context(Scope::Background, policy()).unwrap());
                    }
                } else {
                    let root = cx.http_context(Scope::Background, policy()).unwrap();
                    for _ in 0..256 {
                        assert!(
                            cx.request_managed(
                                root.clone(),
                                None,
                                Method::Get,
                                "https://fixture.invalid/parked".into(),
                                None,
                                Pending::Account(0)
                            )
                            .is_ok()
                        );
                    }
                    self.held.push(root);
                }
            }
            return;
        }
        self.venue.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.venue.books()
    }
}
fn capacity_refusal(roots: bool) {
    let mut e = seeded_policy(
        Capacity {
            venue: venue(false),
            held: vec![],
        },
        PublicFeedPolicy::Direct,
    );
    e.handle(
        100.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![if roots { "roots" } else { "queue" }.into()],
        }),
    );
    let parked = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(e.venue().held.len(), if roots { 256 } else { 1 });
    assert!(!parked.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(
        parked
            .iter()
            .filter(|action| matches!(action,
                Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
            ))
            .count(),
        if roots { 0 } else { 256 }
    );
    for _ in 0..270 {
        candles(&mut e, 100., 71);
        let a = e.drain_actions().collect::<Vec<_>>();
        let error = failure(&a, 71);
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(error.message, Error::Capacity.to_string());
        assert_eq!(
            error.outcome,
            if roots { None } else { Some(Outcome::NotSent) }
        );
        assert_eq!(
            timeline(&a, 71),
            if roots {
                vec!["failed"]
            } else {
                vec!["error", "failed"]
            }
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
    e.handle(
        100.,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    e.drain_actions().for_each(drop);
    assert!(e.venue().held.is_empty());
    candles(&mut e, 100., 71);
    let r = released(&mut e, 100.);
    response(&mut e, 100., r.id, 200, EMPTY);
    assert_eq!(
        result(&e.drain_actions().collect::<Vec<_>>(), 71),
        json!([])
    );
}

#[test]
fn initial_registry_refusal_is_typed_without_companion() {
    capacity_refusal(true);
}

#[test]
fn immediate_managed_queue_refusal_is_typed_not_sent() {
    capacity_refusal(false);
}
