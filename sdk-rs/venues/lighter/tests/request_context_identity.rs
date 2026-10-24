//! Original identity controls over genuine Engine entry and dispatch APIs.
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Permit, Policy, Preparation, Prepared,
    RetryPolicy, RoutingIntent,
};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_lighter::{Lighter, Options, Pending};

const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;
const ADDRESS_A: &str = "0x1111111111111111111111111111111111111111";
const ADDRESS_B: &str = "0x2222222222222222222222222222222222222222";
const ROW: &str = r#"{"code":200,"accounts":[{"account_index":0,"referrer":"own"},{"index":2,"referrer":"other"}]}"#;

struct Probe {
    inner: Lighter,
    peers: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
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
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__identity_peer" => {
                if let Ok(root) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 120_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.peers.push(root);
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire_peer"] => {
                cx.fail_context(&self.peers.remove(0), "retired genuine identity peer")
                    .unwrap();
            }
            _ => self.inner.on_command(cx, command),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_sign_error(cx, pending, error);
    }
}
fn engine(options: Options) -> Engine<Probe> {
    safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Lighter::new(options),
        peers: vec![],
    })
}
fn options() -> Options {
    Options {
        application_id: Some("0".into()),
        public_data_only: true,
        ..Default::default()
    }
}
fn ask(e: &mut Engine<Probe>, now: f64, wait: u32) -> Vec<Action> {
    e.handle_command(now, Command::FetchAccount { command: OWNER }, Some(wait))
        .expect("genuine stamped caller entry");
    e.drain_actions().collect()
}
fn outcomes(actions: &[Action]) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone { command, result }) if *command == OWNER => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == OWNER => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn diagnostics(actions: &[Action]) -> usize {
    actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandError { command, .. }) if *command == OWNER)).count()
}
fn no_io(actions: &[Action]) {
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::ManagedHttp(_)
            | Action::WsSend { .. }
            | Action::WsConnect { .. }
            | Action::SignPayload { .. }
    )));
}
fn prep(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "identity must prepare before any outbound"
    );
    let mut found = actions.iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match action.as_ref() {
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
    let selected = found.next().expect("one identity preparation");
    assert!(found.next().is_none());
    selected
}
fn bind(e: &mut Engine<Probe>, now: f64, p: Preparation) -> (Permit, HttpRequest) {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit,
                    request,
                    preparation,
                } => {
                    assert_eq!(preparation, p);
                    Some((permit, request))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("real prepared identity bind")
}
fn released(e: &mut Engine<Probe>, now: f64, actions: &[Action]) -> HttpRequest {
    let legacy: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request.clone()),
            _ => None,
        })
        .collect();
    if let Some(request) = legacy.first() {
        assert_eq!(legacy.len(), 1);
        return request.clone();
    }
    let (p, _) = prep(actions);
    let (permit, request) = bind(e, now, p);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn reply(
    e: &mut Engine<Probe>,
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
fn wire(request: &HttpRequest, host: &str, path: &str) {
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.url, format!("{host}/api/v1/{path}"));
    assert_eq!(request.body, None);
    assert!(request.headers.is_empty());
    assert_eq!(request.route, None);
}
fn peer(e: &mut Engine<Probe>, command: u32) {
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "__identity_peer".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().next().is_none());
}
fn prepare_deployment(robinhood: bool, testnet: bool) {
    let mut e = engine(Options {
        robinhood,
        testnet,
        ..options()
    });
    let actions = ask(&mut e, NOW, 60_000);
    let (_, request) = prep(&actions);
    assert_eq!(request.timeout_ms, Some(20_000));
}
#[test]
fn lighter_mainnet_identity_prepares() {
    prepare_deployment(false, false);
}
#[test]
fn lighter_testnet_identity_prepares() {
    prepare_deployment(false, true);
}
#[test]
fn robinhood_mainnet_identity_prepares() {
    prepare_deployment(true, false);
}
#[test]
fn robinhood_testnet_identity_prepares() {
    prepare_deployment(true, true);
}

#[test]
fn native_wire_identity_row_and_duplicate_reply_on_all_four_hosts() {
    for (robinhood, testnet, host) in [
        (false, false, "https://mainnet.zklighter.elliot.ai"),
        (false, true, "https://testnet-api.lighter.xyz"),
        (true, false, "https://api.rh.lighter.xyz"),
        (true, true, "https://api.rh-testnet.lighter.xyz"),
    ] {
        let mut e = engine(Options {
            robinhood,
            testnet,
            ..options()
        });
        let before = e.account_snapshot_frame(NOW).to_vec();
        let actions = ask(&mut e, NOW, 60_000);
        let request = released(&mut e, NOW, &actions);
        wire(&request, host, "account?by=index&value=0");
        let actions = reply(&mut e, NOW + 1., &request, 200, ROW);
        assert_eq!(
            outcomes(&actions),
            [Ok(r#"{"userId":"0","affiliateId":"own"}"#.into())]
        );
        assert_eq!(diagnostics(&actions), 0);
        assert_eq!(e.account_snapshot_frame(NOW), before);
        assert!(outcomes(&reply(&mut e, NOW + 2., &request, 200, ROW)).is_empty());
    }
}
#[test]
fn two_distinct_addresses_keep_all_four_native_candidates_in_order() {
    let mut e = engine(Options {
        application_id: Some(ADDRESS_A.into()),
        key: Some(ADDRESS_B.into()),
        public_data_only: true,
        ..Default::default()
    });
    let mut actions = ask(&mut e, NOW, 120_000);
    for (index, path) in [
        format!("account?by=l1_address&value={ADDRESS_A}"),
        format!("account?by=l1_address&value={ADDRESS_B}"),
        format!("accountsByL1Address?l1_address={ADDRESS_A}"),
        format!("accountsByL1Address?l1_address={ADDRESS_B}"),
    ]
    .iter()
    .enumerate()
    {
        let request = released(&mut e, NOW + index as f64, &actions);
        wire(&request, "https://mainnet.zklighter.elliot.ai", path);
        actions = reply(
            &mut e,
            NOW + index as f64 + 1.,
            &request,
            200,
            if index == 3 {
                r#"{"sub_accounts":[{"index":"9","referrer":"chosen"}],"accounts":[{"index":"10","referrer":"wrong"}]}"#
            } else {
                r#"{"success":false}"#
            },
        );
        if index < 3 {
            assert!(outcomes(&actions).is_empty());
            assert_eq!(diagnostics(&actions), 0);
        }
    }
    assert_eq!(
        outcomes(&actions),
        [Ok(r#"{"userId":"9","affiliateId":"chosen"}"#.into())]
    );
}
#[test]
fn missing_selector_keeps_exact_native_companion_and_no_key_index_fallback() {
    for public_data_only in [false, true] {
        let mut e = engine(Options {
            public_data_only,
            api_key_index: Some("7".into()),
            ..Default::default()
        });
        let actions = ask(&mut e, NOW, 60_000);
        no_io(&actions);
        assert_eq!(diagnostics(&actions), 1);
        let results = outcomes(&actions);
        assert_eq!(results.len(), 1);
        let failure = results[0].as_ref().unwrap_err();
        assert_eq!(
            failure.message,
            "Lighter identity requires an account index or L1 address selector"
        );
        assert_eq!((failure.kind, failure.outcome), (None, None));
    }
}
#[test]
fn later_candidate_has_its_own_native_window_under_the_original_longer_root() {
    let mut e = engine(Options {
        application_id: Some(ADDRESS_A.into()),
        public_data_only: true,
        ..Default::default()
    });
    let actions = ask(&mut e, NOW, 60_000);
    let first = released(&mut e, NOW, &actions);
    let actions = reply(&mut e, NOW + 19_000., &first, 404, "missing");
    let second = released(&mut e, NOW + 19_000., &actions);
    assert!(second.url.contains("accountsByL1Address"));
    let actions = reply(
        &mut e,
        NOW + 38_000.,
        &second,
        200,
        r#"{"sub_accounts":[{"index":5}]}"#,
    );
    assert_eq!(outcomes(&actions), [Ok(r#"{"userId":"5"}"#.into())]);
}
#[test]
fn unclaimed_reply_cannot_publish_identity() {
    let mut e = engine(options());
    let actions = ask(&mut e, NOW, 60_000);
    let request = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.clone()),
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { request, .. } => Some(request.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let actions = reply(&mut e, NOW + 1., &request, 200, ROW);
    assert!(
        outcomes(&actions).is_empty(),
        "an unreleased response must not publish DATA"
    );
}
#[test]
fn original_caller_expiry_refuses_late_identity_and_duplicate_settlement() {
    let mut e = engine(options());
    let actions = ask(&mut e, NOW, 25);
    let request = released(&mut e, NOW, &actions);
    let actions = reply(&mut e, NOW + 26., &request, 200, ROW);
    let results = outcomes(&actions);
    assert_eq!(results.len(), 1);
    let failure = results[0]
        .as_ref()
        .expect_err("original caller expiry must deny late DATA");
    assert_eq!(
        (failure.kind, failure.outcome),
        (Some(ErrorKind::Timeout), None)
    );
    assert!(outcomes(&reply(&mut e, NOW + 27., &request, 200, ROW)).is_empty());
}

#[test]
fn original_root_timer_expires_without_an_http_or_venue_callback() {
    let mut e = engine(options());
    let actions = ask(&mut e, NOW, 25);
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 25. => Some(*timer),
            _ => None,
        })
        .expect("an original caller root must have its engine-owned expiry timer");
    e.handle(NOW + 25., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let results = outcomes(&actions);
    assert_eq!(results.len(), 1);
    let failure = results[0].as_ref().unwrap_err();
    assert_eq!(
        (failure.kind, failure.outcome),
        (Some(ErrorKind::Timeout), None)
    );
}
#[test]
fn fallback_cannot_renew_the_original_caller_cap() {
    let mut e = engine(Options {
        application_id: Some(ADDRESS_A.into()),
        public_data_only: true,
        ..Default::default()
    });
    let actions = ask(&mut e, NOW, 40);
    let first = released(&mut e, NOW, &actions);
    let actions = reply(&mut e, NOW + 30., &first, 404, "missing");
    let next = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { request, .. } => Some(request),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(
        next.timeout_ms.is_some_and(|timeout| timeout <= 10),
        "fallback must keep the remaining original caller bound"
    );
    let request = released(&mut e, NOW + 30., &actions);
    assert!(outcomes(&reply(&mut e, NOW + 41., &request, 200, ROW))[0].is_err());
}
#[test]
fn shared_capacity_precedes_missing_selector_and_emits_no_native_diagnostic() {
    let mut e = engine(Options::default());
    for id in 1..=256 {
        peer(&mut e, id);
    }
    let actions = ask(&mut e, NOW, 60_000);
    no_io(&actions);
    assert_eq!(
        diagnostics(&actions),
        0,
        "shared admission owns refusal before native selector effects"
    );
    let result = outcomes(&actions);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Internal)
    );
}
#[test]
fn shared_capacity_precedes_remote_issue_and_recovery_reuses_freed_slots() {
    let mut e = engine(options());
    for id in 1..=256 {
        peer(&mut e, id);
    }
    let actions = ask(&mut e, NOW, 60_000);
    no_io(&actions);
    assert_eq!(outcomes(&actions).len(), 1);
    for _ in 0..2 {
        e.handle(
            NOW + 1.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["retire_peer".into()],
            }),
        );
        e.drain_actions().for_each(drop);
    }
    let actions = ask(&mut e, NOW + 2., 60_000);
    let request = released(&mut e, NOW + 2., &actions);
    assert!(outcomes(&reply(&mut e, NOW + 3., &request, 200, ROW))[0].is_ok());
}
#[test]
fn duplicate_command_cannot_issue_or_replace_the_original_peer_root() {
    let mut e = engine(options());
    peer(&mut e, OWNER);
    let actions = ask(&mut e, NOW, 60_000);
    no_io(&actions);
    assert!(outcomes(&actions).is_empty());
}
#[test]
fn preparation_refusal_stays_typed_and_never_consumes_another_candidate() {
    let mut e = engine(Options {
        application_id: Some(ADDRESS_A.into()),
        public_data_only: true,
        ..Default::default()
    });
    let actions = ask(&mut e, NOW, 60_000);
    let (p, request) = prep(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::State,
        VenueError::new("identity fixture route refusal").kind(ErrorKind::InvalidInput),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let mut cancelled = 0;
    for action in &actions {
        match action {
            Action::Http(_) => panic!("preparation refusal issued HTTP"),
            Action::ManagedHttp(action) => {
                assert_eq!(
                    action.as_ref(),
                    &HttpAction::Cancel {
                        preparation: p,
                        signing: None
                    }
                );
                cancelled += 1;
            }
            _ => {}
        }
    }
    assert_eq!(cancelled, 1);
    let results = outcomes(&actions);
    assert_eq!(results.len(), 1);
    let failure = results[0].as_ref().unwrap_err();
    assert_eq!(failure.message, "identity fixture route refusal");
    assert_eq!(
        (failure.kind, failure.outcome),
        (Some(ErrorKind::InvalidInput), Some(Outcome::NotSent))
    );
    assert!(outcomes(&reply(&mut e, NOW + 2., &request, 200, ROW)).is_empty());
}

#[test]
fn native_first_row_and_field_presence_precede_index_validation() {
    for body in [
        r#"{"code":200,"accounts":[{"referrer":"invalid-first"},{"index":2}]}"#,
        r#"{"code":200,"accounts":[{"account_index":"invalid","index":2}]}"#,
    ] {
        let mut e = engine(options());
        let actions = ask(&mut e, NOW, 60_000);
        let request = released(&mut e, NOW, &actions);
        let actions = reply(&mut e, NOW + 1., &request, 200, body);
        assert_eq!(diagnostics(&actions), 1);
        let results = outcomes(&actions);
        assert_eq!(results.len(), 1);
        let failure = results[0].as_ref().unwrap_err();
        assert_eq!(
            failure.message,
            "Lighter identity lookup returned no valid account index"
        );
        assert_eq!((failure.kind, failure.outcome), (None, None));
    }
}
