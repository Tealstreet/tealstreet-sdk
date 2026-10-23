use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent, Scope,
};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, RequestId, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bitunix::{BitunixFutures, Options, Pending};
use serde_json::{Value, json};

const PAIRS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}]}"#;
const TICKERS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","last":"96000","open":"95000","markPrice":"96001","baseVol":"1000","quoteVol":"96000000"}]}"#;
const CANDLES: &str = r#"{"code":0,"data":[{"time":2000,"open":"3","high":"4","low":"1","close":"2","quoteVol":"8"},{"time":1000,"open":"1","high":"3","low":"1","close":"2","quoteVol":"99"}]}"#;

fn venue(testnet: bool) -> BitunixFutures {
    BitunixFutures::new(Options {
        public_data_only: true,
        testnet,
        ..Options::default()
    })
}

fn response<V: Venue>(
    e: &mut Engine<V>,
    now: f64,
    request: RequestId,
    status: u16,
    headers: &str,
    body: &str,
) {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
}

fn released<V: Venue>(e: &mut Engine<V>, now: f64, path: &str) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r),
            _ => None,
        })
        .expect("native public request")
}

fn seeded<V: Venue>(v: V) -> Engine<V> {
    let mut e = safe_cex_engine::test_support::seeded_engine(v);
    e.start(0.);
    for (path, body) in [("/trading_pairs", PAIRS), ("/tickers", TICKERS)] {
        let request = released(&mut e, 0., path);
        response(&mut e, 0., request.id, 200, "", body);
    }
    e.drain_actions().for_each(drop);
    e
}

fn candles<V: Venue>(
    e: &mut Engine<V>,
    now: f64,
    command: u32,
    interval: Interval,
    limit: Option<u32>,
    from_ms: Option<f64>,
    to_ms: Option<f64>,
) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "UNLISTED".into(),
            interval,
            limit,
            from_ms,
            to_ms,
        }),
    );
}

fn preparation(actions: &[Action], path: &str) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "public HTTP must wait for real Prepare/Bind/Claim"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } if request.url.contains(path) => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("managed public preparation")
}

fn claim<V: Venue>(e: &mut Engine<V>, now: f64, preparation: Preparation) {
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared binding");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    e.drain_actions().for_each(drop);
}

fn terminals(actions: &[Action], command: u32) -> Vec<Result<Value, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some(Ok(serde_json::from_str(result).unwrap())),
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

fn failed(actions: &[Action], command: u32) -> VenueError {
    let mut results = terminals(actions, command);
    assert_eq!(results.len(), 1, "one typed failure: {results:?}");
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandError { .. } | Notice::CommandProgress { .. })
    )));
    results.remove(0).expect_err("failed read")
}

#[test]
fn markets_require_preparation_and_exact_delivery_before_store_or_followup() {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
    e.start(0.);
    let (token, request) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/trading_pairs");
    response(&mut e, 0., request.id, 200, "", PAIRS);
    assert!(!e.store().loaded().markets);
    assert!(e.drain_actions().next().is_none());
    claim(&mut e, 0., token);
    response(&mut e, 0., request.id, 200, "", PAIRS);
    assert!(e.store().loaded().markets);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let _ = preparation(&actions, "/tickers");
}

#[test]
fn tickers_cannot_publish_or_open_public_shards_before_claim() {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
    e.start(0.);
    let request = released(&mut e, 0., "/trading_pairs");
    response(&mut e, 0., request.id, 200, "", PAIRS);
    let (token, ticker) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/tickers");
    response(&mut e, 0., ticker.id, 200, "", TICKERS);
    assert!(!e.store().loaded().tickers);
    assert!(e.drain_actions().next().is_none());
    claim(&mut e, 0., token);
    response(&mut e, 0., ticker.id, 200, "", TICKERS);
    assert!(e.store().loaded().tickers);
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
}

#[test]
fn candle_result_requires_the_released_original_read() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 7, Interval::M1, Some(1), None, None);
    let (token, request) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/kline?");
    response(&mut e, 100., request.id, 200, "", CANDLES);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 7).is_empty());
    claim(&mut e, 100., token);
    response(&mut e, 100., request.id, 200, "", CANDLES);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 7).len(),
        1
    );
    response(&mut e, 100., request.id, 200, "", CANDLES);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 7).is_empty());
}

#[test]
fn physical_expiry_is_timeout_and_cannot_publish_candle_rows() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let request = released(&mut e, 100., "/kline?");
    response(&mut e, 20_100., request.id, 200, "", CANDLES);
    let error = failed(&e.drain_actions().collect::<Vec<_>>(), 7);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
}

#[test]
fn parked_provider_refusal_preserves_every_supplied_typed_field() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let (token, _) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/kline?");
    let error = VenueError::new("provider refusal without auth inference")
        .kind(ErrorKind::Unsupported)
        .outcome(Outcome::NotSent)
        .venue_code("fixture-code")
        .venue_message("fixture-native")
        .retry_after_ms(1234.);
    e.http_preparation_failed(100., token, Error::Invalid, error.clone());
    assert_eq!(failed(&e.drain_actions().collect::<Vec<_>>(), 7), error);
    assert_eq!(e.health().auth_failures, 0);
}

#[test]
fn malformed_candle_body_and_transport_failure_keep_distinct_typed_kinds() {
    for transport in [false, true] {
        let mut e = seeded(venue(false));
        candles(&mut e, 100., 7, Interval::M1, None, None, None);
        let request = released(&mut e, 100., "/kline?");
        if transport {
            e.handle(
                100.,
                Event::HttpFailed {
                    request: request.id,
                    error: "fixture transport failure",
                },
            );
        } else {
            response(
                &mut e,
                100.,
                request.id,
                200,
                "",
                r#"{"code":0,"data":[false]}"#,
            );
        }
        let error = failed(&e.drain_actions().collect::<Vec<_>>(), 7);
        assert_eq!(
            error.kind,
            Some(if transport {
                ErrorKind::Network
            } else {
                ErrorKind::Internal
            })
        );
        assert_eq!(error.outcome, None);
    }
}

#[test]
fn duplicate_candle_command_does_not_allocate_a_second_request() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let first = released(&mut e, 100., "/kline?");
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    assert!(terminals(&actions, 7).is_empty());
    response(&mut e, 100., first.id, 200, "", CANDLES);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 7).len(),
        1
    );
}

#[test]
fn unsigned_public_auth_failure_neither_retries_nor_spends_private_auth_budget() {
    for (status, body) in [
        (200, r#"{"code":10007,"msg":"bad signature","data":[]}"#),
        (401, "{}"),
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
        e.start(0.);
        let request = released(&mut e, 0., "/trading_pairs");
        response(&mut e, 0., request.id, status, "", body);
        assert!(!e.store().loaded().markets);
        assert_eq!(e.health().auth_failures, 0);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. } | Action::Http(_)))
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
        assert!(actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.message.contains("bad signature") || event.message.contains("401"))));
        assert!(terminals(&actions, 7).is_empty());
    }
}

#[test]
fn native_public_rate_pause_keeps_header_seconds_and_business_floor() {
    for (status, headers, code, pause) in [
        (429, "Retry-After: 15", 10005, 15_000.),
        (200, "", 10006, 60_000.),
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
        e.start(0.);
        let request = released(&mut e, 0., "/trading_pairs");
        response(
            &mut e,
            0.,
            request.id,
            status,
            headers,
            &format!(r#"{{"code":{code},"msg":"rate limited","data":[]}}"#),
        );
        let timers = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(timers, [pause]);
    }
}

#[test]
fn usable_peers_publish_but_a_broken_envelope_never_publishes_its_prefix() {
    for broken in [false, true] {
        let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
        e.start(0.);
        let request = released(&mut e, 0., "/trading_pairs");
        let peer = PAIRS.replace("]}", ",null]}");
        let body = if broken {
            &PAIRS[..PAIRS.len() - 2]
        } else {
            &peer
        };
        response(&mut e, 0., request.id, 200, "", body);
        assert_eq!(e.store().loaded().markets, !broken);
        assert_eq!(e.store().markets().len(), usize::from(!broken));
    }
}

#[test]
fn native_candle_query_bytes_header_and_result_order_remain_compatible() {
    for testnet in [false, true] {
        for (limit, start, suffix) in [
            (None, 123., ""),
            (Some(0), 99_940_000., "&limit=1"),
            (Some(2), 99_880_000., "&limit=2"),
            (Some(999), 88_000_000., "&limit=200"),
        ] {
            let mut e = seeded(venue(testnet));
            candles(
                &mut e,
                100.,
                7,
                Interval::M1,
                limit,
                Some(123.),
                Some(100_000_000.),
            );
            let request = released(&mut e, 100., "/kline?");
            assert_eq!(request.method, Method::Get);
            assert_eq!(
                request.url,
                format!(
                    "https://fapi.bitunix.com/api/v1/futures/market/kline?symbol=UNLISTED&interval=1m&startTime={start}&endTime=100000000{suffix}"
                )
            );
            assert_eq!(
                request.headers,
                if testnet {
                    vec![("x-simulated-trading", "1".into())]
                } else {
                    vec![]
                }
            );
            assert!(request.signer_headers.is_empty());
            assert_eq!(request.body, None);
            response(&mut e, 100., request.id, 200, "", CANDLES);
            assert_eq!(
                terminals(&e.drain_actions().collect::<Vec<_>>(), 7),
                [Ok(json!([
                    {"timestamp":2.0,"open":3.0,"high":4.0,"low":1.0,"close":2.0,"volume":8.0},
                    {"timestamp":1.0,"open":1.0,"high":3.0,"low":1.0,"close":2.0,"volume":99.0}
                ]))]
            );
        }
    }
}

#[test]
fn candle_success_preserves_legacy_permissive_code_coercions() {
    for code in ["null", "0.0", "\"bad\""] {
        let mut e = seeded(venue(false));
        candles(&mut e, 100., 7, Interval::M1, None, None, None);
        let request = released(&mut e, 100., "/kline?");
        let body = CANDLES.replacen("\"code\":0", &format!("\"code\":{code}"), 1);
        response(&mut e, 100., request.id, 200, "", &body);
        assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 7)[0].is_ok());
    }
}

struct Probe {
    venue: BitunixFutures,
    held: Vec<Context>,
}

impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn guard_order_mutations(&self) -> bool {
        self.venue.guard_order_mutations()
    }
    fn guard_pending_placements(&self) -> bool {
        self.venue.guard_pending_placements()
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
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
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
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.venue.on_stream_error(cx, pending, response);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("restart") => self.venue.start(cx),
                Some("hold") => {
                    for _ in 0..256 {
                        self.held.push(
                            cx.http_context(
                                Scope::Background,
                                Policy {
                                    operation: Operation::Read,
                                    credential: None,
                                    timeout_ms: 20_000,
                                    pacing: None,
                                    retry: RetryPolicy::Never,
                                },
                            )
                            .unwrap(),
                        );
                    }
                    assert_eq!(self.held.len(), 256);
                }
                Some("release") => {
                    for root in self.held.drain(..) {
                        cx.finish_http_context(&root).unwrap();
                    }
                }
                _ => {}
            }
        } else {
            self.venue.on_command(cx, command);
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
}

fn probe_command(e: &mut Engine<Probe>, name: &str) {
    e.handle(
        100.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![name.into()],
        }),
    );
}

#[test]
fn genuine_entry_capacity_refuses_local_and_remote_candles_then_recovers() {
    let mut e = seeded(Probe {
        venue: venue(false),
        held: vec![],
    });
    probe_command(&mut e, "hold");
    e.drain_actions().for_each(drop);
    for interval in [Interval::M1, Interval::S1].into_iter().cycle().take(270) {
        candles(&mut e, 100., 7, interval, None, None, None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(error.message, Error::Capacity.to_string());
        assert_eq!(error.outcome, None);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
    probe_command(&mut e, "release");
    e.drain_actions().for_each(drop);
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let request = released(&mut e, 100., "/kline?");
    response(&mut e, 100., request.id, 200, "", CANDLES);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 7).len(),
        1
    );
}

#[test]
fn unsupported_interval_retirement_and_typed_preflight_survive_fixed_clock_reuse() {
    let mut e = seeded(venue(false));
    for _ in 0..270 {
        candles(&mut e, 100., 7, Interval::S1, None, None, None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.message, "Unsupported Bitunix candle interval");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
}

#[test]
fn background_seed_owners_retire_at_one_clock_across_success_and_failure() {
    for failing in [false, true] {
        let mut e = seeded(Probe {
            venue: venue(false),
            held: vec![],
        });
        for _ in 0..270 {
            probe_command(&mut e, "restart");
            let request = released(&mut e, 100., "/trading_pairs");
            if failing {
                e.handle(
                    100.,
                    Event::HttpFailed {
                        request: request.id,
                        error: "background transport",
                    },
                );
                e.drain_actions().for_each(drop);
            } else {
                response(&mut e, 100., request.id, 200, "", PAIRS);
                let ticker = released(&mut e, 100., "/tickers");
                response(&mut e, 100., ticker.id, 200, "", TICKERS);
                e.drain_actions().for_each(drop);
            }
        }
        probe_command(&mut e, "restart");
        let request = released(&mut e, 100., "/trading_pairs");
        response(&mut e, 100., request.id, 200, "", PAIRS);
        let ticker = released(&mut e, 100., "/tickers");
        response(&mut e, 100., ticker.id, 200, "", TICKERS);
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
        e.drain_actions().for_each(drop);
        probe_command(&mut e, "hold");
        probe_command(&mut e, "release");
    }
}

#[test]
fn invalid_clock_entry_refuses_candles_before_transport() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = seeded(venue(false));
        candles(&mut e, now, 7, Interval::M1, None, None, None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(failed(&actions, 7).kind, Some(ErrorKind::InvalidInput));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
}

#[test]
fn parked_expiry_and_expired_prepared_binding_never_release_or_publish() {
    for prepared_expiry in [false, true] {
        let mut e = seeded(venue(false));
        candles(&mut e, 100., 7, Interval::M1, None, None, None);
        let (token, request) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/kline?");
        e.http_prepared(
            if prepared_expiry { 100. } else { 20_100. },
            token,
            Prepared {
                id: request.id.0,
                expires_at: prepared_expiry.then_some(100.),
            },
        );
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))
            || matches!(a,
            Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))));
        response(&mut e, 20_100., request.id, 200, "", CANDLES);
        actions.extend(e.drain_actions());
        let error = failed(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::Timeout));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }
}

#[test]
fn retired_attempt_cannot_settle_a_reused_candle_command() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let old = released(&mut e, 100., "/kline?");
    response(&mut e, 100., old.id, 200, "", CANDLES);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 7).len(),
        1
    );
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let (token, request) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/kline?");
    response(&mut e, 100., old.id, 200, "", CANDLES);
    response(&mut e, 100., RequestId(u64::MAX), 200, "", CANDLES);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 7).is_empty());
    claim(&mut e, 100., token);
    response(&mut e, 100., request.id, 200, "", CANDLES);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 7)[0].is_ok());
}

#[test]
fn both_background_seed_failures_retire_prepared_and_released_owners() {
    for tickers in [false, true] {
        for preparation_failure in [false, true] {
            let mut e = seeded(Probe {
                venue: venue(false),
                held: vec![],
            });
            for _ in 0..270 {
                probe_command(&mut e, "restart");
                if tickers {
                    let r = released(&mut e, 100., "/trading_pairs");
                    response(&mut e, 100., r.id, 200, "", PAIRS);
                }
                let path = if tickers {
                    "/tickers"
                } else {
                    "/trading_pairs"
                };
                if preparation_failure {
                    let (token, _) = preparation(&e.drain_actions().collect::<Vec<_>>(), path);
                    e.http_preparation_failed(
                        100.,
                        token,
                        Error::Invalid,
                        VenueError::new("typed seed preparation refusal")
                            .kind(ErrorKind::Unsupported),
                    );
                } else {
                    let request = released(&mut e, 100., path);
                    e.handle(
                        100.,
                        Event::HttpFailed {
                            request: request.id,
                            error: "seed transport",
                        },
                    );
                }
                e.drain_actions().for_each(drop);
            }
            probe_command(&mut e, "restart");
            let request = released(&mut e, 100., "/trading_pairs");
            response(&mut e, 100., request.id, 200, "", PAIRS);
            let request = released(&mut e, 100., "/tickers");
            response(&mut e, 100., request.id, 200, "", TICKERS);
            assert!(e.store().loaded().markets && e.store().loaded().tickers);
            // Filling the whole context registry proves all prior attempt roots retired.
            probe_command(&mut e, "hold");
            probe_command(&mut e, "release");
        }
    }
}

#[test]
fn success_messages_cannot_hide_empty_seed_or_candle_decoder_failures() {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
    e.start(0.);
    let r = released(&mut e, 0., "/trading_pairs");
    response(
        &mut e,
        0.,
        r.id,
        200,
        "",
        r#"{"code":0,"msg":"Success","data":[]}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Event(event))
        if event.message.contains("no eligible rows")))
    );
    for (status, body) in [
        (200, r#"{"code":0,"msg":"Success","data":[false]}"#),
        (500, r#"{"code":0,"msg":"Success","data":[]}"#),
    ] {
        let mut e = seeded(venue(false));
        candles(&mut e, 100., 7, Interval::M1, None, None, None);
        let r = released(&mut e, 100., "/kline?");
        response(&mut e, 100., r.id, status, "", body);
        let error = failed(&e.drain_actions().collect::<Vec<_>>(), 7);
        assert_ne!(error.message, "Success");
        if status == 500 {
            assert_eq!(error.message, "Bitunix HTTP 500");
        } else {
            assert!(error.message.contains("invalid type"));
        }
    }
}

#[test]
fn malformed_discovery_stream_preserves_rate_headers_without_raw_preview() {
    for tickers in [false, true] {
        let mut e = safe_cex_engine::test_support::seeded_engine(venue(false));
        e.start(0.);
        let mut r = released(&mut e, 0., "/trading_pairs");
        if tickers {
            response(&mut e, 0., r.id, 200, "", PAIRS);
            r = released(&mut e, 0., "/tickers");
        }
        response(
            &mut e,
            0.,
            r.id,
            429,
            "Retry-After: 15",
            r#"{"data":[{"RAW_BODY_SENTINEL":"secret"},"#,
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!format!("{actions:?}").contains("RAW_BODY_SENTINEL"));
        assert_eq!(
            actions
                .iter()
                .filter_map(|a| match a {
                    Action::SetTimer { at_ms, .. } => Some(*at_ms),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![15_000.]
        );
    }
}

#[test]
fn a_fresh_native_initialization_recovers_after_each_public_auth_refusal() {
    let mut e = seeded(Probe {
        venue: venue(false),
        held: vec![],
    });
    for _ in 0..270 {
        probe_command(&mut e, "restart");
        let r = released(&mut e, 100., "/trading_pairs");
        response(
            &mut e,
            100.,
            r.id,
            200,
            "",
            r#"{"code":10007,"msg":"bad signature","data":[]}"#,
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.message.contains("bad signature"))));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. } | Action::Http(_)))
        );
        assert_eq!(e.health().auth_failures, 0);
    }
    probe_command(&mut e, "restart");
    let r = released(&mut e, 100., "/trading_pairs");
    response(&mut e, 100., r.id, 200, "", PAIRS);
    let r = released(&mut e, 100., "/tickers");
    response(&mut e, 100., r.id, 200, "", TICKERS);
    e.drain_actions().for_each(drop);
    probe_command(&mut e, "hold");
    probe_command(&mut e, "release");
}

#[test]
fn invalid_prepared_binding_refuses_before_release_without_losing_typed_failure() {
    let mut e = seeded(venue(false));
    candles(&mut e, 100., 7, Interval::M1, None, None, None);
    let (token, request) = preparation(&e.drain_actions().collect::<Vec<_>>(), "/kline?");
    e.http_prepared(
        100.,
        token,
        Prepared {
            id: request.id.0,
            expires_at: Some(f64::NAN),
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))
        || matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Bind{..}))));
    let error = failed(&actions, 7);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    response(&mut e, 100., request.id, 200, "", CANDLES);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 7).is_empty());
}
