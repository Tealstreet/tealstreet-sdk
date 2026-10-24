mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Owner, Policy, Preparation, Prepared,
    RetryPolicy, RoutingIntent,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome,
    Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex, Pending};
use serde_json::{Value, json};

const NOW: f64 = 1_700_000_000_000.;
const ID: u32 = 71;
const SYMBOL: &str = "BTC-USD-PERP";
const KLINES: &str =
    r#"{"results":[[120000,"1","3","0.5","2","4",99],[60000,"2","4","1","3","5"]]}"#;
const TV: &str = r#"{"s":"ok","t":[120,60],"o":["1","2"],"h":["3","4"],"l":["0.5","1"],"c":["2","3"],"v":["4","5"]}"#;

struct Probe {
    inner: Paradex,
    peers: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text)
    }
    fn on_binary(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, data: &[u8]) {
        self.inner.on_binary(cx, socket, data)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item)
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.inner.on_stream_error(cx, pending, response)
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.inner
            .on_stream_malformed(cx, pending, status, headers, error)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_sign_error(cx, pending, error)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error)
    }
    fn read_root_retired(&mut self, owner: Owner, pending: Vec<Pending>) {
        self.inner.read_root_retired(owner, pending)
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.on_public_feed(cx)
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.inner.cancel_needs_stored_order()
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__hold" => {
                self.peers.push(
                    cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 120_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .expect("genuine peer command Read"),
                );
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["__retire"] => {
                let original = self.peers.remove(0);
                cx.fail_context(&original, "fixture peer retired").unwrap();
                for peer in &self.peers {
                    cx.validate_read_root_context(peer).unwrap();
                }
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
type TestEngine = Engine<Probe>;
fn reply(
    e: &mut TestEngine,
    now: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn boot(testnet: bool) -> TestEngine {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Paradex::new(Options {
            public_data_only: true,
            testnet,
            ..Default::default()
        }),
        peers: vec![],
    });
    e.start(NOW);
    for _ in 0..8 {
        let actions = prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            if let Action::Http(request) = action {
                let body = if request.url.ends_with("/markets") {
                    support::MARKETS
                } else if request.url.contains("/markets/summary") {
                    support::TICKERS
                } else {
                    panic!("unexpected startup request {}", request.url)
                };
                e.handle(
                    NOW,
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
        }
    }
    assert!(e.store().market(SYMBOL).is_some());
    assert!(e.store().loaded().tickers);
    e.take_frame(NOW);
    e
}
fn command(interval: Interval) -> Command {
    Command::FetchCandles {
        command: ID,
        symbol: SYMBOL.into(),
        interval,
        limit: Some(1),
        from_ms: Some(120000.75),
        to_ms: Some(240000.5),
    }
}
fn ask(e: &mut TestEngine, now: f64, interval: Interval, wait: u32) -> Vec<Action> {
    e.handle_command(now, command(interval), Some(wait))
        .unwrap();
    e.drain_actions().collect()
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "command must Prepare before HTTP"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("one managed unsigned command preparation")
}
fn bind(
    e: &mut TestEngine,
    now: f64,
    token: Preparation,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared command binds")
}
fn release(e: &mut TestEngine, now: f64, token: Preparation) -> HttpRequest {
    let (permit, request) = bind(e, now, token);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn native_packet(e: &mut TestEngine, now: f64, actions: Vec<Action>) -> HttpRequest {
    let raw: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect();
    if !raw.is_empty() {
        assert_eq!(raw.len(), 1);
        raw[0].clone()
    } else {
        let (token, _) = preparation(&actions);
        release(e, now, token)
    }
}
fn terminal(actions: &[Action]) -> Result<Value, VenueError> {
    let result: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: ID,
                result,
            }) => Some(Ok(serde_json::from_str(result).unwrap())),
            Action::Notice(Notice::CommandFailed { command: ID, error }) => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(result.len(), 1, "exactly one command terminal: {actions:?}");
    result.into_iter().next().unwrap()
}
fn no_next(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))
            || matches!(a, Action::ManagedHttp(a) if matches!(&**a, HttpAction::Prepare { .. }))),
        "no next call: {actions:?}"
    );
}
fn no_terminal(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: ID, .. } | Notice::CommandFailed { command: ID, .. }
            )
        )),
        "no command terminal: {actions:?}"
    );
}
fn timer(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("genuine engine deadline timer")
}
fn expected() -> Value {
    json!([{"timestamp":120000.0,"open":1.0,"high":3.0,"low":0.5,"close":2.0,"volume":4.0},{"timestamp":60000.0,"open":2.0,"high":4.0,"low":1.0,"close":3.0,"volume":5.0}])
}

#[test]
fn native_documented_wire_and_financial_rows_keep_order_and_ignore_limit() {
    for testnet in [false, true] {
        let mut e = boot(testnet);
        let actions = ask(&mut e, NOW, Interval::M1, 60000);
        let request = native_packet(&mut e, NOW, actions);
        let host = if testnet {
            "api.testnet.paradex.trade"
        } else {
            "api.prod.paradex.trade"
        };
        assert_eq!(
            request.url,
            format!(
                "https://{host}/v1/markets/klines?symbol=BTC-USD-PERP&resolution=1&start_at=120000.75&end_at=240000.5&price_kind=last"
            )
        );
        assert!(request.headers.is_empty() && request.body.is_none());
        assert_eq!(request.method, safe_cex_engine::Method::Get);
        assert_eq!(
            terminal(&reply(&mut e, NOW + 1., &request, 200, KLINES)).unwrap(),
            expected()
        );
    }
}
#[test]
fn native_wide_interval_floors_seconds_and_does_not_truncate_or_sort() {
    let mut e = boot(false);
    let actions = ask(&mut e, NOW, Interval::H4, 60000);
    let request = native_packet(&mut e, NOW, actions);
    assert_eq!(
        request.url,
        "https://api.prod.paradex.trade/v1/tradingview/history?symbol=BTC-USD-PERP&resolution=240&from=120&to=240&countback=2000&price_kind=last"
    );
    assert_eq!(
        terminal(&reply(&mut e, NOW + 1., &request, 200, TV)).unwrap(),
        expected()
    );
}
#[test]
fn native_first_route_errors_fallback_once_and_discard_first_rows() {
    for (status, body) in [
        (503, r#"{"error":"unavailable"}"#),
        (200, r#"{"message":"no list"}"#),
        (200, r#"{"results":[[1,2,3,4,5,6],[1]]}"#),
        (
            200,
            r#"{"results":[[1,2,3,4,5,6]],"error":{},"message":"bad"}"#,
        ),
    ] {
        let mut e = boot(false);
        let actions = ask(&mut e, NOW, Interval::M5, 60000);
        let first = native_packet(&mut e, NOW, actions);
        let actions = reply(&mut e, NOW + 1., &first, status, body);
        no_terminal(&actions);
        let second = native_packet(&mut e, NOW + 1., actions);
        assert!(second.url.contains("tradingview/history"));
        let out = reply(&mut e, NOW + 2., &second, 200, TV);
        assert_eq!(terminal(&out).unwrap(), expected());
        no_next(&out);
    }
}
#[test]
fn native_transport_failure_uses_one_fallback_and_no_third_call() {
    let mut e = boot(false);
    let actions = ask(&mut e, NOW, Interval::M1, 60000);
    let first = native_packet(&mut e, NOW, actions);
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: first.id,
            error: "fixture network",
        },
    );
    let actions = e.drain_actions().collect();
    let second = native_packet(&mut e, NOW + 1., actions);
    let out = reply(&mut e, NOW + 2., &second, 503, r#"{"error":"down"}"#);
    assert_eq!(terminal(&out).unwrap_err().message, "Paradex HTTP 503");
    no_next(&out);
}
#[test]
fn native_legacy_empty_and_malformed_parallel_arrays() {
    for (body, success) in [
        (r#"{"s":"no_data"}"#, true),
        (
            r#"{"s":"ok","t":[1],"o":[],"h":[],"l":[],"c":[],"v":[]}"#,
            false,
        ),
    ] {
        let mut e = boot(false);
        let a = ask(&mut e, NOW, Interval::H4, 60000);
        let request = native_packet(&mut e, NOW, a);
        let out = reply(&mut e, NOW + 1., &request, 200, body);
        if success {
            assert_eq!(terminal(&out).unwrap(), json!([]));
        } else {
            assert_eq!(
                terminal(&out).unwrap_err().message,
                "Malformed Paradex candles"
            );
        }
        no_next(&out);
    }
}
#[test]
fn native_local_validation_precedence_stays_before_remote_work() {
    let mut e = boot(false);
    e.handle_command(
        NOW,
        Command::FetchCandles {
            command: ID,
            symbol: "missing".into(),
            interval: Interval::S1,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
        Some(60000),
    )
    .unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        terminal(&out).unwrap_err().message,
        Interval::S1.unsupported("Paradex")
    );
    no_next(&out);
}
#[test]
fn command_klines_requires_managed_preparation() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let (t, _) = preparation(&a);
    let r = release(&mut e, NOW, t);
    assert_eq!(
        terminal(&reply(&mut e, NOW + 1., &r, 200, KLINES)).unwrap(),
        expected()
    );
}
#[test]
fn command_tradingview_requires_managed_preparation() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::H4, 60000);
    let (t, _) = preparation(&a);
    let r = release(&mut e, NOW, t);
    assert_eq!(
        terminal(&reply(&mut e, NOW + 1., &r, 200, TV)).unwrap(),
        expected()
    );
}
#[test]
fn fallback_requires_a_fresh_preparation_after_first_retirement() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let first = native_packet(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &first, 503, "{}");
    let (token, unsigned) = preparation(&a);
    assert_ne!(unsigned.id, first.id);
    let second = release(&mut e, NOW + 1., token);
    assert_eq!(
        terminal(&reply(&mut e, NOW + 2., &second, 200, TV)).unwrap(),
        expected()
    );
}
#[test]
fn short_fractional_original_receipt_caps_physical_request() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW + 0.25, Interval::M1, 1000);
    let r = native_packet(&mut e, NOW + 0.25, a.clone());
    assert_eq!(r.timeout_ms, Some(1000));
    let t = timer(&a, NOW + 1000.25);
    e.handle(NOW + 1000.25, Event::TimerFired { timer: t });
    let out = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(terminal(&out).unwrap_err().kind, Some(ErrorKind::Timeout));
    no_next(&out);
}
#[test]
fn live_long_root_does_not_extend_stuck_prepare_bound() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let (token, _) = preparation(&a);
    let t = timer(&a, NOW + 20000.);
    e.handle(NOW + 20000., Event::TimerFired { timer: t });
    let out: Vec<_> = e.drain_actions().collect();
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_next(&out);
    e.http_prepared(
        NOW + 20001.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let late = e.drain_actions().collect::<Vec<_>>();
    no_next(&late);
    no_terminal(&late);
}
#[test]
fn live_long_root_does_not_extend_unclaimed_bind_bound() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let (token, _) = preparation(&a);
    let (permit, _) = bind(&mut e, NOW + 1., token);
    let t = timer(&a, NOW + 20000.);
    e.handle(NOW + 20000., Event::TimerFired { timer: t });
    let out: Vec<_> = e.drain_actions().collect();
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_next(&out);
    assert!(e.claim_http(NOW + 20001., permit).is_err());
}
#[test]
fn unclaimed_data_cannot_complete_or_fallback() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let r = a
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare { request, .. } => Some(request.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let out = reply(&mut e, NOW + 1., &r, 200, KLINES);
    no_terminal(&out);
    no_next(&out);
}
#[test]
fn root_expiry_before_first_data_forbids_fallback() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 1000);
    let r = native_packet(&mut e, NOW, a);
    let out = reply(&mut e, NOW + 1000., &r, 503, "{}");
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    no_next(&out);
}
#[test]
fn root_expiry_during_fallback_does_not_publish_legacy_rows() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 1000);
    let first = native_packet(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &first, 503, "{}");
    let second = native_packet(&mut e, NOW + 1., a);
    let out = reply(&mut e, NOW + 1000., &second, 200, TV);
    assert_eq!(terminal(&out).unwrap_err().kind, Some(ErrorKind::Timeout));
    no_next(&out);
}
#[test]
fn released_first_child_timeout_uses_live_original_root_for_fallback() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let (token, _) = preparation(&a);
    let _first = release(&mut e, NOW, token);
    let t = timer(&a, NOW + 20000.);
    e.handle(NOW + 20000., Event::TimerFired { timer: t });
    let a = e.drain_actions().collect::<Vec<_>>();
    no_terminal(&a);
    let (token, r) = preparation(&a);
    assert!(r.url.contains("tradingview"));
    let second = release(&mut e, NOW + 20000., token);
    assert_eq!(
        terminal(&reply(&mut e, NOW + 20001., &second, 200, TV)).unwrap(),
        expected()
    );
}
#[test]
fn released_second_child_timeout_is_typed_and_has_no_third_call() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let first = native_packet(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &first, 503, "{}");
    let (token, _) = preparation(&a);
    let _second = release(&mut e, NOW + 1., token);
    let t = timer(&a, NOW + 20001.);
    e.handle(NOW + 20001., Event::TimerFired { timer: t });
    let out = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    no_next(&out);
}
#[test]
fn typed_preparation_refusal_is_strict_and_preserves_all_fields() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let (token, _) = preparation(&a);
    e.http_preparation_failed(
        NOW + 1.,
        token,
        Error::Invalid,
        VenueError::new("fixture route denied")
            .kind(ErrorKind::Unsupported)
            .venue_code("route.fixture"),
    );
    let out = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.message, "fixture route denied");
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.venue_code.as_deref(), Some("route.fixture"));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_next(&out);
}
#[test]
fn duplicate_command_keeps_the_first_owner_and_one_result() {
    let mut e = boot(false);
    let a = ask(&mut e, NOW, Interval::M1, 60000);
    let r = native_packet(&mut e, NOW, a);
    let a = ask(&mut e, NOW + 1., Interval::M1, 60000);
    no_next(&a);
    no_terminal(&a);
    let out = reply(&mut e, NOW + 2., &r, 200, KLINES);
    assert_eq!(terminal(&out).unwrap(), expected());
    let late = reply(&mut e, NOW + 3., &r, 200, KLINES);
    no_terminal(&late);
    no_next(&late);
}
#[test]
fn nan_event_entry_refuses_without_private_or_public_http() {
    let mut e = boot(false);
    e.handle(f64::NAN, Event::Command(command(Interval::M1)));
    let out = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    no_next(&out);
}
#[test]
fn capacity_two_slot_recovery_retires_no_foreign_owner() {
    let mut e = boot(false);
    for id in 1000..1256 {
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command: id,
                symbol: "__hold".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(120000),
        )
        .unwrap();
        assert!(e.drain_actions().next().is_none());
    }
    assert_eq!(e.venue().peers.len(), 256);
    let out = ask(&mut e, NOW, Interval::M1, 60000);
    let error = terminal(&out).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    no_next(&out);
    for peer_count in [255, 254] {
        e.handle(
            NOW,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["__retire".into()],
            }),
        );
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue().peers.len(), peer_count);
        let out = ask(&mut e, NOW, Interval::M1, 60000);
        if peer_count == 255 {
            let error = terminal(&out).unwrap_err();
            assert_eq!(error.kind, Some(ErrorKind::Internal));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            no_next(&out);
        } else {
            let r = native_packet(&mut e, NOW, out);
            assert_eq!(
                terminal(&reply(&mut e, NOW + 1., &r, 200, KLINES)).unwrap(),
                expected()
            );
        }
    }
}

#[test]
fn native_absent_bounds_and_empty_results_keep_defaults() {
    for interval in [Interval::M1, Interval::H4] {
        let mut e = boot(false);
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command: ID,
                symbol: SYMBOL.into(),
                interval,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(60000),
        )
        .unwrap();
        let actions = e.drain_actions().collect();
        let request = native_packet(&mut e, NOW, actions);
        if interval == Interval::M1 {
            assert!(
                request
                    .url
                    .ends_with("&start_at=1699880000000&end_at=1700000000000&price_kind=last")
            );
        } else {
            assert!(
                request
                    .url
                    .ends_with("resolution=240&countback=2000&price_kind=last")
            );
        }
        let body = if interval == Interval::M1 {
            r#"{"results":[]}"#
        } else {
            r#"{"s":"no_data"}"#
        };
        assert_eq!(
            terminal(&reply(&mut e, NOW + 1., &request, 200, body)).unwrap(),
            json!([])
        );
    }
}

#[test]
fn native_finished_first_call_late_failure_cannot_replace_fallback() {
    let mut e = boot(false);
    let actions = ask(&mut e, NOW, Interval::M1, 60000);
    let first = native_packet(&mut e, NOW, actions);
    let actions = reply(&mut e, NOW + 1., &first, 503, "{}");
    let second = native_packet(&mut e, NOW + 1., actions);
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: first.id,
            error: "retired fixture failure",
        },
    );
    let late = e.drain_actions().collect::<Vec<_>>();
    no_terminal(&late);
    no_next(&late);
    assert_eq!(
        terminal(&reply(&mut e, NOW + 3., &second, 200, TV)).unwrap(),
        expected()
    );
}

#[test]
fn streamed_prefix_at_original_deadline_cannot_publish() {
    let mut e = boot(false);
    let actions = ask(&mut e, NOW, Interval::M1, 1000);
    let request = native_packet(&mut e, NOW, actions);
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: request.id,
            data: br#"{"results":[[120000,1,3,0.5,2,4]]}"#,
        },
    );
    let interim = e.drain_actions().collect::<Vec<_>>();
    no_terminal(&interim);
    e.handle(
        NOW + 1000.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    let out = e.drain_actions().collect::<Vec<_>>();
    let result = terminal(&out);
    assert!(
        result.is_err(),
        "staged candles crossed original deadline: {result:?}"
    );
    assert_eq!(result.unwrap_err().kind, Some(ErrorKind::Timeout));
    no_next(&out);
}
