//! Original identity entry and typed-failure assertions on unchanged BingX.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation, Policy, Prepared, RetryPolicy,
};
use safe_cex_engine::signing::{Identity, KeyRef, SignError, SignResult};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bingx::{Bingx, Options, Pending};
const NOW: f64 = 1000.;
const PROBE: &str = "__retained_read_probe__";
const RETIRE: &str = "__retire_read_probe__";
const MISSING: &str = "BingX account identity requires credentials";
const INVALID: &str = "Secondary read credentials require a nonempty key and secret";
const URL: &str = "https://open-api.bingx.com/openApi/swap/v3/user/balance";
// Fixture commands retain genuine engine-issued roots; FetchAccount and every
// delivery callback delegate to the unmodified native venue.
struct Probe {
    venue: Bingx,
    roots: Vec<Context>,
}
impl Venue for Probe {
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
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: safe_cex_engine::request_context::Error,
        error: VenueError,
    ) {
        self.venue
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == PROBE => {
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
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == [RETIRE] => {
                let root = self.roots.remove(0);
                cx.fail_context(&root, "retired fixture peer").unwrap();
            }
            _ => self.venue.on_command(cx, command),
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Branch {
    Missing,
    Public,
    SecondaryOnly,
    Invalid,
    Remote,
}
const BRANCHES: [Branch; 5] = [
    Branch::Missing,
    Branch::Public,
    Branch::SecondaryOnly,
    Branch::Invalid,
    Branch::Remote,
];
fn identity(slot: &str) -> Identity {
    Identity::new(
        KeyRef(format!("opaque:{slot}:credential")),
        Some(KeyRef(format!("opaque:{slot}:signing"))),
    )
}
fn options(branch: Branch) -> Options {
    let mut read = identity("secondary");
    if matches!(branch, Branch::Invalid) {
        read.set_signing(Some(KeyRef(String::new())));
    }
    Options {
        credentials: (!matches!(branch, Branch::Missing | Branch::SecondaryOnly))
            .then(|| identity("primary")),
        read_credentials: (!matches!(branch, Branch::Missing)).then_some(read),
        public_data_only: matches!(branch, Branch::Public),
    }
}
fn seeded(venue: Bingx) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        venue,
        roots: vec![],
    });
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 100.,
        used: 23.,
        upnl: 7.,
    });
    e
}
fn instance(branch: Branch) -> Engine<Probe> {
    seeded(Bingx::new(options(branch)))
}
fn ask(e: &mut Engine<Probe>, now: f64, command: u32) -> Vec<Action> {
    e.handle(now, Event::Command(Command::FetchAccount { command }));
    e.drain_actions().collect()
}
fn terminals(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
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
fn diagnostics(actions: &[Action], owner: u32) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandError { command, message }) if *command == owner => {
                Some(message.as_str())
            }
            _ => None,
        })
        .collect()
}
fn failure(actions: &[Action], owner: u32) -> VenueError {
    let results = terminals(actions, owner);
    assert_eq!(results.len(), 1, "exactly one terminal: {actions:?}");
    let diagnostic_at = actions.iter().position(
        |a| matches!(a, Action::Notice(Notice::CommandError {command,..}) if *command == owner),
    );
    let terminal_at = actions.iter().position(
        |a| matches!(a, Action::Notice(Notice::CommandFailed {command,..}) if *command == owner),
    );
    if let Some(at) = diagnostic_at {
        assert!(at < terminal_at.unwrap());
    }
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Sign { .. }
        ) || matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare {..} | HttpAction::Bind {..}))),
        "no retry or reconciliation: {actions:?}"
    );
    results[0].clone().unwrap_err()
}
fn native(actions: &[Action], owner: u32, message: &str) {
    assert_eq!(diagnostics(actions, owner), [message]);
    let error = failure(actions, owner);
    assert_eq!(error.message, message);
    assert_eq!((error.kind, error.outcome), (None, None));
}
fn denied(actions: &[Action], owner: u32, kind: ErrorKind, message: &str) {
    assert_eq!(
        actions.len(),
        1,
        "shared denial only, no native diagnostic: {actions:?}"
    );
    let error = failure(actions, owner);
    assert_eq!(error.message, message);
    assert_eq!((error.kind, error.outcome), (Some(kind), None));
}
fn park(e: &mut Engine<Probe>, owner: u32) {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: PROBE.into(),
            limit: None,
        }),
    );
    assert!(e.drain_actions().next().is_none());
}
fn retire(e: &mut Engine<Probe>, owner: u32) {
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(failure(&actions, owner).message, "retired fixture peer");
}
fn respond(
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
fn cleanup(
    actions: &[Action],
    expected: safe_cex_engine::request_context::Preparation,
    signed: Option<Option<safe_cex_engine::RequestId>>,
) {
    let cancels: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Cancel {
                    preparation,
                    signing,
                } => Some((*preparation, *signing)),
                _ => panic!("unexpected new managed work"),
            },
            _ => None,
        })
        .collect();
    assert_eq!(cancels.len(), 1);
    assert_eq!(
        cancels[0].0, expected,
        "only this exact preparation may retire"
    );
    if let Some(signed) = signed {
        assert_eq!(cancels[0].1, signed);
    }
}
fn unsigned_headers() -> Vec<(&'static str, String)> {
    vec![
        ("X-SOURCE-KEY", "Tealstreet".into()),
        ("Content-Type", "application/json, charset=utf-8".into()),
    ]
}
fn release(
    e: &mut Engine<Probe>,
    actions: Vec<Action>,
    secondary: bool,
) -> (
    HttpRequest,
    TimerId,
    safe_cex_engine::request_context::Preparation,
) {
    let prepares: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(prepares.len(), 1, "one managed native GET: {actions:?}");
    let (preparation, request) = prepares[0];
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.url, URL);
    assert_eq!(request.headers, unsigned_headers());
    assert_eq!(request.body, None);
    assert!(request.signer_headers.is_empty());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::Http(_)))
    );
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("original twenty-second timer");
    e.http_prepared(
        NOW,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let signs: Vec<_> = e.drain_actions().collect();
    let (attempt, work) = signs
        .into_iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    let slot = if secondary { "secondary" } else { "primary" };
    assert!(
        matches!(&work.operations[0],safe_cex_engine::signing::Operation::Present{key} if key.0==format!("opaque:{slot}:credential"))
    );
    assert!(
        matches!(&work.operations[1],safe_cex_engine::signing::Operation::Sign{key,..} if key.0==format!("opaque:{slot}:signing"))
    );
    assert_eq!(work.receipt.stamp_ms, Some(NOW));
    // Trusted fixture provider tests release custody; real LocalSigner vectors below pin MAC bytes.
    e.handle(
        NOW,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(
                vec![b"fixture-secondary".to_vec(), vec![7; 32]],
                work.receipt,
            )),
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(e.claim_http(NOW, permit), Ok(Claim::Released(_))));
    assert_eq!(request.route, None);
    assert!(e.drain_actions().next().is_none());
    (request, timer, preparation)
}
fn capacity(branch: Branch) {
    let mut e = instance(branch);
    let before = e.account_snapshot_frame(NOW).to_vec();
    for owner in 1..=256 {
        park(&mut e, owner);
    }
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: 900,
            symbol: PROBE.into(),
            limit: None,
        }),
    );
    let control: Vec<_> = e.drain_actions().collect();
    denied(
        &control,
        900,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    denied(
        &ask(&mut e, NOW, 800),
        800,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    // Recovery/refill is reached only after the original admission assertion turns green.
    retire(&mut e, 1);
    let actions = ask(&mut e, NOW, 800);
    if matches!(branch, Branch::Remote) {
        let (request, _, _) = release(&mut e, actions, true);
        assert_eq!(
            terminals(
                &respond(
                    &mut e,
                    NOW,
                    &request,
                    200,
                    r#"{"code":0,"data":[{"userId":"owner-800"}]}"#
                ),
                800
            ),
            [Ok(r#"{"userId":"owner-800"}"#.into())]
        );
    } else {
        native(
            &actions,
            800,
            if matches!(branch, Branch::Invalid) {
                INVALID
            } else {
                MISSING
            },
        );
    }
    park(&mut e, 900);
    denied(
        &ask(&mut e, NOW, 801),
        801,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    for owner in 2..=256 {
        retire(&mut e, owner);
    }
    retire(&mut e, 900);
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn missing_primary_requires_read_capacity() {
    capacity(Branch::Missing);
}
#[test]
fn public_only_requires_read_capacity() {
    capacity(Branch::Public);
}
#[test]
fn secondary_only_requires_read_capacity() {
    capacity(Branch::SecondaryOnly);
}
#[test]
fn invalid_secondary_requires_read_capacity() {
    capacity(Branch::Invalid);
}
#[test]
fn remote_entry_preserves_shared_capacity_denial() {
    capacity(Branch::Remote);
}
#[test]
fn invalid_clock_denies_before_native_local_or_remote_work() {
    for branch in BRANCHES {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut e = instance(branch);
            denied(
                &ask(&mut e, now, 810),
                810,
                ErrorKind::InvalidInput,
                "Invalid request context",
            );
        }
    }
}
#[test]
fn native_local_precedence_diagnostics_and_store_are_preserved() {
    for branch in BRANCHES.into_iter().take(4) {
        let mut e = instance(branch);
        let before = e.account_snapshot_frame(NOW).to_vec();
        native(
            &ask(&mut e, NOW, 42),
            42,
            if matches!(branch, Branch::Invalid) {
                INVALID
            } else {
                MISSING
            },
        );
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
    for (primary, public) in [(true, false), (true, true), (false, false), (false, true)] {
        let mut opts = options(Branch::Invalid);
        opts.credentials = primary.then(|| identity("primary"));
        opts.public_data_only = public;
        let mut e = seeded(Bingx::new(opts));
        native(&ask(&mut e, NOW, 42), 42, INVALID);
    }
}
#[test]
fn duplicate_and_fixed_clock_local_reuse_preserve_genuine_peer_owners() {
    for branch in BRANCHES.into_iter().take(4) {
        let mut e = instance(branch);
        park(&mut e, 820);
        let actions = ask(&mut e, NOW, 820);
        assert!(terminals(&actions, 820).is_empty());
        assert!(
            matches!(actions.as_slice(),[Action::Notice(Notice::Event(event))] if event.code==safe_cex_engine::log::code::COMMAND_DUPLICATE)
        );
        retire(&mut e, 820);
        for _ in 0..300 {
            native(
                &ask(&mut e, NOW, 820),
                820,
                if matches!(branch, Branch::Invalid) {
                    INVALID
                } else {
                    MISSING
                },
            );
        }
        for owner in 1..=256 {
            park(&mut e, owner);
        }
        for owner in 1..=256 {
            retire(&mut e, owner);
        }
    }
}
#[test]
fn real_local_provider_preserves_primary_secondary_wire_and_exact_codec() {
    for secondary in [false, true] {
        let (venue, provider) = support::Options {
            key: Some("primary-key".into()),
            secret: Some("primary-secret".into()),
            read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: None,
            }),
            public_data_only: false,
        }
        .build();
        let mut e = seeded(venue);
        e.set_signer(Some(provider)).unwrap();
        let before = e.account_snapshot_frame(NOW).to_vec();
        e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let requests: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        let (key, mac) = if secondary {
            (
                "read-key",
                "8ca0109999113cd1c9f3932e63775f22a359b20aed3c481b041a82a897a37ecd",
            )
        } else {
            (
                "primary-key",
                "050ac9d8221e49e933dc0e26ff89ce4357a93868cdcb0eea940c7e5d6324c4ec",
            )
        };
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, format!("{URL}?signature={mac}&timestamp=1000"));
        assert_eq!(
            request.headers,
            vec![
                ("X-SOURCE-KEY", "Tealstreet".into()),
                ("X-BX-APIKEY", key.into()),
                ("Content-Type", "application/json, charset=utf-8".into())
            ]
        );
        assert_eq!((request.body.as_ref(), request.route), (None, None));
        let (body, result) = if secondary {
            (
                r#"{"code":0,"data":[{"asset":"USDC","userId":"masked-***"}]}"#,
                r#"{"userId":"masked-***"}"#,
            )
        } else {
            (
                r#"{"code":0,"data":[{"asset":"USDC","userId":"wrong"},{"asset":"USDT","userId":9007199254740993}]}"#,
                r#"{"userId":"9007199254740993"}"#,
            )
        };
        let out = respond(&mut e, NOW, request, 200, body);
        assert_eq!(terminals(&out, 42), [Ok(result.into())]);
        assert!(diagnostics(&out, 42).is_empty());
        assert_eq!(e.account_snapshot_frame(NOW), before);
        assert!(respond(&mut e, NOW, request, 200, body).is_empty());
    }
}
#[test]
fn opaque_refs_and_duplicate_pending_keep_the_original_remote_owner() {
    for secondary in [false, true] {
        let mut opts = options(Branch::Remote);
        if !secondary {
            opts.read_credentials = None;
        }
        let mut e = seeded(Bingx::new(opts));
        let actions = ask(&mut e, NOW, 42);
        assert!(terminals(&ask(&mut e, NOW, 42), 42).is_empty());
        let (request, _, _) = release(&mut e, actions, secondary);
        let out = respond(
            &mut e,
            NOW,
            &request,
            200,
            r#"{"code":0,"data":[{"userId":"opaque-owner"}]}"#,
        );
        assert_eq!(
            terminals(&out, 42),
            [Ok(r#"{"userId":"opaque-owner"}"#.into())]
        );
    }
}
#[test]
fn native_malformed_status_and_signature_code_reject_once_without_retry() {
    for (status, body, message) in [
        (200, "{", "BingX account identity response is invalid"),
        (503, "unavailable", "BingX account identity HTTP 503"),
        (
            200,
            r#"{"code":100001,"msg":"signature verification failed","data":[]}"#,
            "signature verification failed",
        ),
        (
            200,
            r#"{"code":0,"data":[]}"#,
            "BingX balance response did not include a user ID",
        ),
    ] {
        let mut e = instance(Branch::Remote);
        let before = e.account_snapshot_frame(NOW).to_vec();
        let actions = ask(&mut e, NOW, 42);
        let (request, _, preparation) = release(&mut e, actions, true);
        let out = respond(&mut e, NOW, &request, status, body);
        cleanup(&out, preparation, Some(None));
        native(&out, 42, message);
        assert!(respond(&mut e, NOW, &request, status, body).is_empty());
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn released_network_failure_keeps_its_typed_kind_and_single_terminal() {
    let mut e = instance(Branch::Remote);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let actions = ask(&mut e, NOW, 42);
    let (request, _, preparation) = release(&mut e, actions, true);
    e.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    cleanup(&out, preparation, Some(None));
    assert_eq!(diagnostics(&out, 42), ["connection lost"]);
    let error = failure(&out, 42);
    assert!(
        respond(
            &mut e,
            NOW,
            &request,
            200,
            r#"{"code":0,"data":[{"userId":"late"}]}"#
        )
        .is_empty()
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Network), None)
    );
}
#[test]
fn released_root_timer_keeps_timeout_kind_and_denies_late_success() {
    let mut e = instance(Branch::Remote);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let actions = ask(&mut e, NOW, 42);
    let (request, timer, preparation) = release(&mut e, actions, true);
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    cleanup(&out, preparation, Some(None));
    let error = failure(&out, 42);
    assert_eq!(diagnostics(&out, 42), [error.message.as_str()]);
    assert!(
        respond(
            &mut e,
            NOW + 20_001.,
            &request,
            200,
            r#"{"code":0,"data":[{"userId":"late"}]}"#
        )
        .is_empty()
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Timeout), None)
    );
}
#[test]
fn preparation_and_provider_refusal_remain_typed_and_emit_no_transport() {
    for provider in [false, true] {
        let mut e = instance(Branch::Remote);
        let actions = ask(&mut e, NOW, 42);
        let preparation = actions
            .into_iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let mut signing = None;
        let expected = if provider {
            e.http_prepared(
                NOW,
                preparation,
                Prepared {
                    id: 1,
                    expires_at: None,
                },
            );
            let attempt = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::Sign { attempt, .. } => Some(attempt),
                    _ => None,
                })
                .unwrap();
            signing = Some(Some(attempt));
            e.handle(
                NOW,
                Event::Signed {
                    attempt,
                    result: Err(SignError::Refused),
                },
            );
            // SignError::venue_error is the common engine Refused projection.
            SignError::Refused.venue_error()
        } else {
            let error = VenueError::new("preparation refused")
                .kind(ErrorKind::Unsupported)
                .outcome(Outcome::NotSent);
            e.http_preparation_failed(
                NOW,
                preparation,
                safe_cex_engine::request_context::Error::Invalid,
                error.clone(),
            );
            error
        };
        let out: Vec<_> = e.drain_actions().collect();
        cleanup(&out, preparation, signing);
        assert_eq!(failure(&out, 42), expected);
        assert_eq!(diagnostics(&out, 42), [expected.message.as_str()]);
    }
}
