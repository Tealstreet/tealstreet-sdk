//! Original public-candle custody controls; the native implementation is unchanged.
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
use safe_cex_venue_kraken::{Kraken, Options, Pending};
use serde_json::{Value, json};

const NOW: f64 = 1_700_000_000_000.;
const COMMAND: u32 = 71;
const SYMBOL: &str = "PF_XBTUSD";
const META: &str = r#"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]}]}"#;
const TICKERS: &str = r#"{"result":"success","tickers":[{"symbol":"PF_XBTUSD","last":100,"markPrice":101,"indexPrice":102,"bid":99,"ask":101,"vol24h":2}]}"#;
struct Probe {
    inner: Kraken,
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
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item)
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
                    .expect("genuine peer command admission"),
                );
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["__retire"] => {
                let original = self.peers.remove(0);
                cx.fail_context(&original, "original peer retired").unwrap();
                for peer in &self.peers {
                    cx.validate_read_root_context(peer).unwrap();
                }
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
type TestEngine = Engine<Probe>;
fn response(e: &mut TestEngine, now: f64, request: &HttpRequest, status: u16, body: &str) {
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
}
fn boot() -> (TestEngine, TimerId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Kraken::new(Options {
            public_data_only: true,
            ..Default::default()
        }),
        peers: vec![],
    });
    e.start(NOW);
    let mut poll = None;
    for _ in 0..8 {
        let actions = prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::SetTimer { timer, at_ms } if at_ms == NOW + 5000. => {
                    poll.get_or_insert(timer);
                }
                Action::Http(request) => {
                    let body = if request.url.ends_with("/instruments") {
                        META
                    } else if request.url.ends_with("/tickers") {
                        TICKERS
                    } else {
                        panic!("unexpected fixture request {}", request.url)
                    };
                    response(&mut e, NOW, &request, 200, body);
                }
                _ => {}
            }
        }
    }
    assert!(e.store().market(SYMBOL).is_some());
    assert!(e.store().ticker(SYMBOL).is_some());
    e.take_frame(NOW);
    (e, poll.expect("native five-second polling timer"))
}
fn command(id: u32) -> Command {
    Command::FetchCandles {
        command: id,
        symbol: SYMBOL.into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: Some(NOW - 120_000.),
        to_ms: Some(NOW),
    }
}
fn fetch(e: &mut TestEngine, now: f64, wait: u32) -> Vec<Action> {
    e.handle_command(now, command(COMMAND), Some(wait)).unwrap();
    e.drain_actions().collect()
}
fn subscribe(e: &mut TestEngine, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: Interval::M1,
        }),
    );
    e.drain_actions().collect()
}
fn packet(actions: &[Action]) -> HttpRequest {
    let packets: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 1, "exactly one physical page: {actions:?}");
    packets[0].clone()
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "physical HTTP must wait for Prepare/Bind/Claim"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match &**action {
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
        .expect("one genuine unsigned page preparation")
}
fn release(e: &mut TestEngine, now: f64, token: Preparation) -> HttpRequest {
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
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared page binds");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn native_packet(e: &mut TestEngine, actions: Vec<Action>, now: f64) -> HttpRequest {
    if actions.iter().any(|a| matches!(a, Action::Http(_))) {
        packet(&actions)
    } else {
        let (token, _) = preparation(&actions);
        release(e, now, token)
    }
}
fn terminal(actions: &[Action]) -> Result<Value, VenueError> {
    let results: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => Some(Ok(serde_json::from_str(result).unwrap())),
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => Some(Err(error.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        results.len(),
        1,
        "one original command terminal: {actions:?}"
    );
    results.into_iter().next().unwrap()
}
fn empty() -> &'static str {
    r#"{"candles":[]}"#
}
fn page(time: f64, more: bool) -> String {
    json!({"candles":[{"time":time,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}],"more_candles":more}).to_string()
}
fn outcome(e: &mut TestEngine, now: f64, request: &HttpRequest, body: &str) -> Vec<Action> {
    response(e, now, request, 200, body);
    e.drain_actions().collect()
}
fn no_http(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Sign { .. } | Action::SignedOutbound { .. }
    )));
}
fn saturate(e: &mut TestEngine) {
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
            Some(120_000),
        )
        .unwrap();
        assert!(e.drain_actions().next().is_none());
    }
    assert_eq!(e.venue().peers.len(), 256);
}
fn retire(e: &mut TestEngine) {
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["__retire".into()],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(e.venue().peers.len(), 255);
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1000, error }) if error.message == "original peer retired")).count(), 1);
}

#[test]
fn command_page_requires_public_preparation_before_http() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let (token, unsigned) = preparation(&actions);
    assert!(unsigned.headers.is_empty() && unsigned.body.is_none());
    let request = release(&mut e, NOW + 1., token);
    assert_eq!(request.url, unsigned.url);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 2., &request, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn background_page_requires_public_preparation_before_http() {
    let (mut e, _) = boot();
    let actions = subscribe(&mut e, NOW);
    let (token, unsigned) = preparation(&actions);
    assert!(unsigned.headers.is_empty());
    let request = release(&mut e, NOW + 1., token);
    assert!(
        outcome(&mut e, NOW + 2., &request, empty())
            .iter()
            .all(|a| !matches!(
                a,
                Action::Notice(Notice::CommandDone { .. })
                    | Action::Notice(Notice::CommandFailed { .. })
            ))
    );
}
#[test]
fn command_root_keeps_fractional_raw_caller_cap() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW + 0.25, 1000);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1000.25 => Some(*timer),
            _ => None,
        })
        .expect("raw caller root deadline, without rounding");
    e.handle(NOW + 1000.25, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let error = terminal(&actions).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    no_http(&actions);
}
#[test]
fn original_root_expiry_refuses_late_valid_page() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 1000);
    let request = native_packet(&mut e, actions, NOW);
    let actions = outcome(&mut e, NOW + 1000., &request, &page(NOW, false));
    let error =
        terminal(&actions).expect_err("a valid late body cannot outlive the original caller root");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    assert!(e.take_frame(NOW + 1000.).is_none());
}
#[test]
fn second_page_keeps_original_root_and_discards_staged_prefix_on_expiry() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 1000);
    let first = native_packet(&mut e, actions, NOW);
    let actions = outcome(&mut e, NOW + 1., &first, &page(NOW - 120_000., true));
    let second = native_packet(&mut e, actions, NOW + 1.);
    assert!(second.url.contains("from=1699999940&to=1700000000"));
    let actions = outcome(&mut e, NOW + 1000., &second, &page(NOW - 60_000., false));
    let error = terminal(&actions).expect_err("staged pages cannot become late success");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
}
#[test]
fn second_page_gets_its_own_managed_preparation() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let first = native_packet(&mut e, actions, NOW);
    let actions = outcome(&mut e, NOW + 1., &first, &page(NOW - 120_000., true));
    let (next, _) = preparation(&actions);
    let second = release(&mut e, NOW + 2., next);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 3., &second, empty()))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn preparation_refusal_preserves_shared_fields_and_late_success_is_inert() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let (token, _) = preparation(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        token,
        Error::Invalid,
        VenueError::new("fixture route unavailable")
            .kind(ErrorKind::Unsupported)
            .venue_code("route.fixture"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let error = terminal(&actions).unwrap_err();
    assert_eq!(error.message, "fixture route unavailable");
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.venue_code.as_deref(), Some("route.fixture"));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_http(&actions);
    e.http_prepared(
        NOW + 2.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let late: Vec<_> = e.drain_actions().collect();
    no_http(&late);
    assert!(!late.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. }) | Action::Notice(Notice::CommandFailed { .. })
    )));
}
#[test]
fn original_root_expiry_revokes_a_held_bind_permit() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 1000);
    let (token, _) = preparation(&actions);
    e.http_prepared(
        NOW + 1.,
        token,
        Prepared {
            id: token.attempt().request.0,
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
        .unwrap();
    assert!(e.claim_http(NOW + 1000., permit).is_err());
    let actions: Vec<_> = e.drain_actions().collect();
    let error = terminal(&actions).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    no_http(&actions);
}
#[test]
fn genuine_capacity_refuses_command_before_http_and_recovers_after_peer_retirement() {
    let (mut e, _) = boot();
    saturate(&mut e);
    let actions = fetch(&mut e, NOW, 60_000);
    no_http(&actions);
    let error = terminal(&actions).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    retire(&mut e);
    let actions = fetch(&mut e, NOW, 60_000);
    let request = native_packet(&mut e, actions, NOW);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 1., &request, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn genuine_capacity_refuses_poll_without_io_and_recovers_next_eligible_poll() {
    let (mut e, poll) = boot();
    saturate(&mut e);
    let actions = subscribe(&mut e, NOW);
    no_http(&actions);
    assert!(e.take_frame(NOW).is_none());
    retire(&mut e);
    e.handle(NOW + 5000., Event::TimerFired { timer: poll });
    let request = packet(&prepared_actions(&mut e, NOW + 5000.));
    assert!(request.url.contains("/api/charts/"));
}
#[test]
fn live_root_child_timeout_preserves_typed_read_failure() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("physical page deadline");
    let _request = native_packet(&mut e, actions, NOW);
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let error = terminal(&actions).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
}
#[test]
fn expired_background_page_cannot_publish_a_candle() {
    let (mut e, _) = boot();
    let actions = subscribe(&mut e, NOW);
    let request = native_packet(&mut e, actions, NOW);
    outcome(&mut e, NOW + 20_000., &request, &page(NOW, false));
    assert!(
        e.take_frame(NOW + 20_000.).is_none(),
        "expired physical page must not publish"
    );
}
#[test]
fn invalid_external_clock_refuses_valid_command_before_http() {
    let (mut e, _) = boot();
    e.handle(f64::NAN, Event::Command(command(COMMAND)));
    let actions: Vec<_> = e.drain_actions().collect();
    no_http(&actions);
    assert_eq!(
        terminal(&actions).unwrap_err().kind,
        Some(ErrorKind::InvalidInput)
    );
}
#[test]
fn native_single_page_wire_and_numbers_stay_exact() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let request = native_packet(&mut e, actions, NOW);
    assert_eq!(
        request.url,
        "https://futures.kraken.com/api/charts/v1/trade/PF_XBTUSD/1m?from=1699999880&to=1700000000"
    );
    assert!(request.headers.is_empty() && request.body.is_none());
    let result = terminal(&outcome(&mut e, NOW + 1., &request, &page(NOW, false))).unwrap();
    assert_eq!(
        result,
        json!([{"timestamp":1_700_000_000.,"open":1.,"high":2.,"low":0.5,"close":1.5,"volume":3.}])
    );
}
#[test]
fn native_defaults_clamp_only_the_default_window() {
    for (limit, seconds) in [(None, 30_000.), (Some(0), 60.), (Some(6000), 300_000.)] {
        let (mut e, _) = boot();
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command: COMMAND,
                symbol: SYMBOL.into(),
                interval: Interval::M1,
                limit,
                from_ms: None,
                to_ms: Some(NOW),
            },
            Some(60_000),
        )
        .unwrap();
        let actions: Vec<_> = e.drain_actions().collect();
        let request = native_packet(&mut e, actions, NOW);
        assert!(
            request
                .url
                .ends_with(&format!("from={:.0}&to=1700000000", NOW / 1000. - seconds))
        );
    }
}
#[test]
fn native_sort_dedup_and_explicit_range_ignore_local_limit() {
    let (mut e, _) = boot();
    e.handle_command(
        NOW,
        Command::FetchCandles {
            command: COMMAND,
            symbol: SYMBOL.into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: Some(NOW - 120_000.),
            to_ms: Some(NOW),
        },
        Some(60_000),
    )
    .unwrap();
    let actions: Vec<_> = e.drain_actions().collect();
    let request = native_packet(&mut e, actions, NOW);
    let body = json!({"candles":[
        {"time":NOW,"open":3,"high":4,"low":2,"close":3.5,"volume":2},
        {"time":NOW-60_000.,"open":1,"high":2,"low":0.5,"close":1.5,"volume":3},
        {"time":NOW,"open":9,"high":9,"low":9,"close":9,"volume":9}
    ]})
    .to_string();
    let rows = terminal(&outcome(&mut e, NOW + 1., &request, &body)).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["timestamp"], json!((NOW - 60_000.) / 1000.));
    assert_eq!(rows[1]["close"], json!(3.5));
}
#[test]
fn native_pages_continue_and_finish_once() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let first = native_packet(&mut e, actions, NOW);
    let actions = outcome(&mut e, NOW + 1., &first, &page(NOW - 120_000., true));
    let next = native_packet(&mut e, actions, NOW + 1.);
    assert!(next.url.contains("from=1699999940&to=1700000000"));
    let rows = terminal(&outcome(
        &mut e,
        NOW + 2.,
        &next,
        &page(NOW - 60_000., false),
    ))
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert!(outcome(&mut e, NOW + 3., &next, empty()).is_empty());
}
#[test]
fn native_no_progress_and_later_page_failure_never_return_prefix() {
    for body in [
        r#"{"candles":[],"more_candles":true}"#,
        r#"{"candles":[],"error":"fixture"}"#,
    ] {
        let (mut e, _) = boot();
        let actions = fetch(&mut e, NOW, 60_000);
        let first = native_packet(&mut e, actions, NOW);
        let actions = outcome(&mut e, NOW + 1., &first, &page(NOW - 120_000., true));
        let next = native_packet(&mut e, actions, NOW + 1.);
        assert!(terminal(&outcome(&mut e, NOW + 2., &next, body)).is_err());
    }
}
#[test]
fn native_ten_page_budget_fails_instead_of_returning_truncated_rows() {
    let (mut e, _) = boot();
    e.handle_command(
        NOW,
        Command::FetchCandles {
            command: COMMAND,
            symbol: SYMBOL.into(),
            interval: Interval::M1,
            limit: None,
            from_ms: Some(NOW - 20. * 60_000.),
            to_ms: Some(NOW),
        },
        Some(60_000),
    )
    .unwrap();
    let actions: Vec<_> = e.drain_actions().collect();
    let mut request = native_packet(&mut e, actions, NOW);
    for index in 0..10 {
        let actions = outcome(
            &mut e,
            NOW + 1.,
            &request,
            &page(NOW - (20 - index) as f64 * 60_000., true),
        );
        if index == 9 {
            assert_eq!(
                terminal(&actions).unwrap_err().message,
                "Kraken candle range exceeds the paging bound"
            );
            no_http(&actions);
        } else {
            request = native_packet(&mut e, actions, NOW + 1.);
        }
    }
}
#[test]
fn native_empty_page_is_one_success() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let request = native_packet(&mut e, actions, NOW);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 1., &request, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn native_poll_publishes_then_waits_five_seconds() {
    let (mut e, poll) = boot();
    let actions = subscribe(&mut e, NOW);
    let request = native_packet(&mut e, actions, NOW);
    let actions = outcome(&mut e, NOW, &request, &page(NOW, false));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. }) | Action::Notice(Notice::CommandFailed { .. })
    )));
    assert!(e.take_frame(NOW).is_some());
    e.handle(NOW + 5000., Event::TimerFired { timer: poll });
    assert!(
        packet(&prepared_actions(&mut e, NOW + 5000.))
            .url
            .contains("/api/charts/")
    );
}
#[test]
fn native_stale_subscription_success_and_failure_preserve_new_generation() {
    for fail in [false, true] {
        let (mut e, _) = boot();
        let actions = subscribe(&mut e, NOW);
        let old = native_packet(&mut e, actions, NOW);
        e.handle(
            NOW,
            Event::Command(Command::UnsubscribeCandles {
                symbol: SYMBOL.into(),
                interval: Interval::M1,
            }),
        );
        assert!(e.drain_actions().next().is_none());
        let actions = subscribe(&mut e, NOW);
        let current = native_packet(&mut e, actions, NOW);
        if fail {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: old.id,
                    error: "fixture old failure",
                },
            );
            e.drain_actions().for_each(drop);
        } else {
            outcome(&mut e, NOW, &old, &page(NOW, false));
        }
        assert!(e.take_frame(NOW).is_none());
        outcome(&mut e, NOW, &current, &page(NOW, false));
        assert!(e.take_frame(NOW).is_some());
    }
}
#[test]
fn native_local_validation_precedence_and_strings_are_preserved() {
    for (symbol, interval, from, to, message) in [
        (
            "UNKNOWN",
            Interval::M3,
            NOW + 1.,
            NOW,
            "Unknown Kraken market",
        ),
        (
            SYMBOL,
            Interval::M3,
            NOW + 1.,
            NOW,
            "Kraken does not support 3m candles",
        ),
        (
            SYMBOL,
            Interval::M1,
            NOW + 1.,
            NOW,
            "Invalid Kraken candle range",
        ),
    ] {
        let (mut e, _) = boot();
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command: COMMAND,
                symbol: symbol.into(),
                interval,
                limit: Some(1),
                from_ms: Some(from),
                to_ms: Some(to),
            },
            Some(60_000),
        )
        .unwrap();
        let actions: Vec<_> = e.drain_actions().collect();
        no_http(&actions);
        assert_eq!(terminal(&actions).unwrap_err().message, message);
    }
}
#[test]
fn duplicate_command_keeps_original_pending_page() {
    let (mut e, _) = boot();
    let actions = fetch(&mut e, NOW, 60_000);
    let request = native_packet(&mut e, actions, NOW);
    let duplicate = fetch(&mut e, NOW, 60_000);
    no_http(&duplicate);
    assert!(duplicate.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)));
    assert_eq!(
        terminal(&outcome(&mut e, NOW, &request, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn zero_wait_is_rejected_before_native_entry() {
    let (mut e, _) = boot();
    assert_eq!(
        e.handle_command(NOW, command(COMMAND), Some(0)),
        Err(Error::Invalid)
    );
    assert!(e.drain_actions().next().is_none());
}
