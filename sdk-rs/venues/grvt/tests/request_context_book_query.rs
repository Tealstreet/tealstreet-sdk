//! Original GRVT BookQuery controls before its production cutover.
use safe_cex_engine::book::Book;
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
const PEER: &str = "__grvt_book_peer";
const RETIRE: &str = "__grvt_book_retire";
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
const BOOK: &str = r#"{"r":{"bids":[{"price":"1.20","size":"2.5"},{"price":"1.2","size":"0"},{"price":"1.9","size":"4"}],"asks":[{"price":"2.00","size":"3"},{"price":"1.99","size":"2"}]}}"#;
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
fn command(local: bool) -> Command {
    Command::FetchOrderBook {
        command: OWNER,
        symbol: if local { "MISSING" } else { "BTC_USDT_PERP" }.into(),
        limit: Some(1),
    }
}
fn ask(e: &mut Engine<Probe>, now: f64, wait: u32, local: bool) -> Vec<Action> {
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
        "book must prepare before HTTP"
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
    let p = found.next().expect("one genuine book Prepare");
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
        .expect("real book Bind")
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

fn wire(request: &HttpRequest, testnet: bool) {
    assert_eq!(request.method, Method::Post);
    assert_eq!(
        request.url,
        format!(
            "https://market-data{}.grvt.io/lite/v1/book",
            if testnet { ".testnet" } else { "" }
        )
    );
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    assert_eq!(
        request.body.as_deref(),
        Some(r#"{"i":"BTC_USDT_Perp","d":50}"#)
    );
    assert_eq!(request.route, None);
}
fn done(actions: &[Action]) -> serde_json::Value {
    let values = outcomes(actions, OWNER);
    assert_eq!(values.len(), 1);
    serde_json::from_str(values[0].as_ref().unwrap()).unwrap()
}
fn native_error(actions: &[Action], kind: ErrorKind) -> VenueError {
    no_io(actions);
    let values = outcomes(actions, OWNER);
    assert_eq!(values.len(), 1);
    let error = values[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    error.clone()
}
#[test]
fn mainnet_requires_managed_prepare() {
    let mut e = engine(false);
    prep(&ask(&mut e, NOW, 60_000, false));
}
#[test]
fn testnet_requires_managed_prepare() {
    let mut e = engine(true);
    prep(&ask(&mut e, NOW, 60_000, false));
}
#[test]
fn native_wire_rows_and_unknown_market_empty_on_both_hosts() {
    for testnet in [false, true] {
        let mut e = engine(testnet);
        let actions = ask(&mut e, NOW, 60_000, false);
        let request = release(&mut e, NOW, &actions);
        wire(&request, testnet);
        let result = done(&reply(&mut e, NOW + 1., &request, 200, BOOK));
        assert_eq!(
            result,
            serde_json::json!({"bids":[{"price":1.2,"amount":2.5,"total":2.5},{"price":1.2,"amount":0.0,"total":2.5},{"price":1.9,"amount":4.0,"total":6.5}],"asks":[{"price":2.0,"amount":3.0,"total":3.0},{"price":1.99,"amount":2.0,"total":5.0}]})
        );
        assert!(e.venue().book("BTC_USDT_PERP").is_none());
        assert!(reply(&mut e, NOW + 2., &request, 200, BOOK).is_empty());
        let actions = ask(&mut e, NOW + 3., 60_000, true);
        no_io(&actions);
        assert_eq!(done(&actions), serde_json::json!({"bids":[],"asks":[]}));
    }
}
#[test]
fn native_ignores_limit_and_preserves_empty_and_full_alias() {
    for limit in [None, Some(0), Some(1), Some(u32::MAX)] {
        let mut e = engine(false);
        let Command::FetchOrderBook {
            command, symbol, ..
        } = command(false)
        else {
            unreachable!()
        };
        e.handle_command(
            NOW,
            Command::FetchOrderBook {
                command,
                symbol,
                limit,
            },
            Some(60_000),
        )
        .unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        wire(&release(&mut e, NOW, &actions), false);
    }
    for body in [
        r#"{}"#,
        r#"{"r":null}"#,
        r#"{"r":{}}"#,
        r#"{"result":{"bids":null,"asks":null}}"#,
    ] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 60_000, false);
        let request = release(&mut e, NOW, &actions);
        assert_eq!(
            done(&reply(&mut e, NOW + 1., &request, 200, body)),
            serde_json::json!({"bids":[],"asks":[]})
        );
    }
}
fn capacity(local: bool) {
    let mut e = engine(false);
    for id in 1000..1256 {
        peer(&mut e, id);
    }
    let actions = ask(&mut e, NOW, 60_000, local);
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    assert!(
        values[0].is_err(),
        "shared capacity must precede native completion"
    );
    let error = values[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        }),
    );
    assert_eq!(
        outcomes(&e.drain_actions().collect::<Vec<_>>(), 1000).len(),
        1
    );
    let actions = ask(&mut e, NOW, 60_000, local);
    if local {
        assert_eq!(done(&actions), serde_json::json!({"bids":[],"asks":[]}));
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
fn capacity_precedes_unknown_market_empty_and_recovers() {
    capacity(true);
}
#[test]
fn capacity_precedes_remote_work_and_recovers() {
    capacity(false);
}
fn clock(local: bool) {
    let mut e = engine(false);
    e.handle(f64::NAN, Event::Command(command(local)));
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    assert!(
        values[0].is_err(),
        "invalid clock must refuse before empty success"
    );
    assert_eq!(
        values[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::InvalidInput)
    );
}
#[test]
fn invalid_clock_precedes_unknown_market_empty() {
    clock(true);
}
#[test]
fn invalid_clock_precedes_remote_issue() {
    clock(false);
}
#[test]
fn unclaimed_data_cannot_publish() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, false);
    let (_, request) = prep(&actions);
    let late = reply(&mut e, NOW + 1., &request, 200, BOOK);
    no_io(&late);
    assert!(outcomes(&late, OWNER).is_empty());
    let released = release(&mut e, NOW + 2., &actions);
    assert_eq!(
        done(&reply(&mut e, NOW + 3., &released, 200, BOOK))["bids"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn caller_expiry_refuses_late_claimed_data() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 100, false);
    let request = release(&mut e, NOW, &actions);
    let actions = reply(&mut e, NOW + 100., &request, 200, BOOK);
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    assert!(values[0].is_err(), "caller cap must beat late DATA");
    assert_eq!(
        values[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
#[test]
fn caller_timer_expires_without_a_venue_callback() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 100, false);
    let timers = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!timers.is_empty(), "caller root must have an engine timer");
    let _ = release(&mut e, NOW, &actions);
    for timer in timers {
        e.handle(NOW + 100., Event::TimerFired { timer });
    }
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    assert_eq!(
        values[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
fn preparation_refusal(reason: Error, kind: ErrorKind) {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, false);
    let (preparation, _) = prep(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        preparation,
        reason,
        VenueError::new("preparation refused")
            .kind(kind)
            .retry_after_ms(17.),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let error = native_error(&actions, kind);
    assert_eq!(error.message, "preparation refused");
    assert_eq!(error.retry_after_ms, Some(17.));
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::ManagedHttp(a) if matches!(&**a,HttpAction::Cancel { preparation:p,signing:None } if *p==preparation))).count(), 1);
    e.http_preparation_failed(NOW + 2., preparation, reason, VenueError::new("duplicate"));
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn preparation_refusal_is_strict_and_typed() {
    preparation_refusal(Error::Invalid, ErrorKind::InvalidInput);
}
#[test]
fn preparation_timeout_is_strict_while_the_caller_lives() {
    preparation_refusal(Error::TimedOut, ErrorKind::Timeout);
}
#[test]
fn native_six_immediate_transport_attempts_have_no_seventh() {
    let mut e = engine(false);
    let mut actions = ask(&mut e, NOW, 60_000, false);
    for attempt in 0..6 {
        let request = release(&mut e, NOW + f64::from(attempt), &actions);
        wire(&request, false);
        actions = failed(&mut e, NOW + f64::from(attempt) + 0.5, &request);
        if attempt < 5 {
            assert!(outcomes(&actions, OWNER).is_empty());
        } else {
            let error = native_error(&actions, ErrorKind::Network);
            assert_eq!(error.message, "network unavailable");
            assert!(failed(&mut e, NOW + 7., &request).is_empty());
        }
    }
}
#[test]
fn physical_timeout_keeps_a_fresh_twenty_second_window() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, false);
    let (preparation, _) = prep(&actions);
    let first = release(&mut e, NOW, &actions);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("shared physical 20s timer");
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(outcomes(&actions, OWNER).is_empty());
    let (next, request) = prep(&actions);
    assert_ne!(next, preparation);
    assert_ne!(request.id, first.id);
    assert_eq!(request.timeout_ms, Some(20_000));
    let second = release(&mut e, NOW + 20_000., &actions);
    assert_eq!(
        done(&reply(&mut e, NOW + 39_000., &second, 200, BOOK))["asks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn short_remaining_caller_cap_shortens_a_retry() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 1000, false);
    let first = release(&mut e, NOW, &actions);
    let actions = failed(&mut e, NOW + 900., &first);
    let (_, request) = prep(&actions);
    assert!(request.timeout_ms.unwrap() <= 100);
    let second = release(&mut e, NOW + 900., &actions);
    let actions = reply(&mut e, NOW + 1000., &second, 200, BOOK);
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    assert_eq!(
        values[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
#[test]
fn stale_response_cannot_consume_the_replacement() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, false);
    let first = release(&mut e, NOW, &actions);
    let actions = failed(&mut e, NOW + 1., &first);
    prep(&actions);
    assert!(reply(&mut e, NOW + 2., &first, 200, BOOK).is_empty());
    let next = release(&mut e, NOW + 3., &actions);
    assert_eq!(
        done(&reply(&mut e, NOW + 4., &next, 200, BOOK))["bids"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn stale_failure_cannot_fail_the_replacement() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, false);
    let first = release(&mut e, NOW, &actions);
    let actions = failed(&mut e, NOW + 1., &first);
    prep(&actions);
    let late = failed(&mut e, NOW + 2., &first);
    no_io(&late);
    assert!(outcomes(&late, OWNER).is_empty());
    let next = release(&mut e, NOW + 3., &actions);
    assert!(outcomes(&reply(&mut e, NOW + 4., &next, 200, BOOK), OWNER)[0].is_ok());
}
#[test]
fn response_timeout_status_is_strict_not_a_physical_retry() {
    for status in [408, 504] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 60_000, false);
        let request = release(&mut e, NOW, &actions);
        let error = native_error(
            &reply(&mut e, NOW + 1., &request, status, "not JSON"),
            ErrorKind::Timeout,
        );
        assert_eq!(
            error.message,
            format!("GRVT order-book query HTTP {status}")
        );
    }
}
#[test]
fn rate_limit_is_one_native_terminal_with_clamped_hint_and_no_deferral() {
    for status in [200, 429] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 60_000, false);
        let request = release(&mut e, NOW, &actions);
        e.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "Retry-After: 9999999\r\n",
                    body: r#"{"error":{"code":"1006","message":"native rate limit"},"r":null}"#,
                },
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = native_error(&actions, ErrorKind::RateLimited);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::SetTimer { .. })),
            "rate refusal cannot auto-defer"
        );
        assert_eq!(
            error.retry_after_ms,
            Some(safe_cex_engine::startup::MAX_RATE_LIMIT_PAUSE_MS)
        );
        assert_eq!(error.venue_code.as_deref(), Some("1006"));
        assert_eq!(error.venue_message.as_deref(), Some("native rate limit"));
        assert!(failed(&mut e, NOW + 2., &request).is_empty());
    }
}
#[test]
fn native_malformed_rows_and_aggregate_overflow_are_strict() {
    for body in [
        "[]",
        r#"{"r":[]}"#,
        r#"{"r":{"bids":[{"price":"0","size":"1"}]}}"#,
        r#"{"r":{"bids":[{"price":"1","size":"-1"}]}}"#,
        r#"{"r":{"bids":[{"price":"1","size":"1e308"},{"price":"1","size":"1e308"}]}}"#,
    ] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 60_000, false);
        let request = release(&mut e, NOW, &actions);
        let error = native_error(
            &reply(&mut e, NOW + 1., &request, 200, body),
            ErrorKind::Internal,
        );
        assert_eq!(error.message, "Invalid GRVT order-book snapshot");
    }
}
#[test]
fn non_utf8_actual_ingress_is_strict_without_transport_retry() {
    let mut e = engine(false);
    let actions = ask(&mut e, NOW, 60_000, false);
    let request = release(&mut e, NOW, &actions);
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
            data: b"\xff",
        },
    );
    let mut actions = e.drain_actions().collect::<Vec<_>>();
    e.handle(
        NOW + 3.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    actions.extend(e.drain_actions());
    no_io(&actions);
    let values = outcomes(&actions, OWNER);
    assert_eq!(values.len(), 1);
    assert_eq!(
        values[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Internal)
    );
}
#[test]
fn duplicate_command_retains_the_original_peer() {
    let mut e = engine(false);
    peer(&mut e, OWNER);
    e.handle(NOW, Event::Command(command(false)));
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    assert!(outcomes(&actions, OWNER).is_empty());
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::COMMAND_DUPLICATE)));
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        }),
    );
    assert_eq!(e.venue().validated, 1);
}
#[test]
fn zero_wait_refuses_before_native_issue() {
    let mut e = engine(false);
    assert_eq!(
        e.handle_command(NOW, command(false), Some(0)),
        Err(Error::Invalid)
    );
    assert!(e.drain_actions().next().is_none());
}
