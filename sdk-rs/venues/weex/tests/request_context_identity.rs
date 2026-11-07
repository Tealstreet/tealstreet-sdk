//! FetchAccount refusals and remote validation retain genuine initial Read custody.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, ReadCredentials, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_weex::{Pending, WeexFutures};
use support::setup::{self, Options};
const NOW: f64 = 1_700_000_000_000.;
const PROBE: &str = "__retained_read_probe__";
const RETIRE: &str = "__retire_read_probe__";
const CONFIG: &str = r#"{"canTrade":true,"dualSidePosition":true}"#;
// Only fixture commands create these genuine engine-issued roots. Native
// WEEX Query dispatch and its credential selection are delegated unchanged.
struct Probe {
    venue: WeexFutures,
    roots: Vec<Context>,
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
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
fn private() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        stable_id: Some("weex-account-7".into()),
        ..Default::default()
    }
}
fn read() -> ReadCredentials {
    ReadCredentials {
        key: "read-key".into(),
        secret: "read-secret".into(),
        passphrase: Some("read-pass".into()),
    }
}
#[derive(Clone, Copy, Debug)]
enum Branch {
    InvalidRead,
    Primary,
    Stable,
    Paused,
    Remote,
}
const BRANCHES: [Branch; 5] = [
    Branch::InvalidRead,
    Branch::Primary,
    Branch::Stable,
    Branch::Paused,
    Branch::Remote,
];
fn engine(options: Options) -> Engine<Probe> {
    let mut e = setup::with_venue(options, |venue| Probe {
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
fn ask(e: &mut Engine<Probe>, now: f64, command: u32) -> Vec<Action> {
    e.handle(now, Event::Command(Command::FetchAccount { command }));
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn respond(e: &mut Engine<Probe>, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        NOW,
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
fn released(actions: Vec<Action>) -> HttpRequest {
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 1);
    requests.into_iter().next().unwrap()
}
fn instance(branch: Branch) -> Engine<Probe> {
    let mut e = engine(match branch {
        Branch::InvalidRead => Options {
            read_credentials: Some(ReadCredentials {
                secret: String::new(),
                ..read()
            }),
            ..private()
        },
        Branch::Primary => Options::default(),
        Branch::Stable => Options {
            stable_id: None,
            ..private()
        },
        Branch::Paused | Branch::Remote => private(),
    });
    if matches!(branch, Branch::Paused) {
        let actions = ask(&mut e, NOW, 700);
        let request = released(actions);
        let actions = respond(&mut e, &request, 429, "rate limited");
        assert_eq!(terminals(&actions, 700).len(), 1);
        assert!(terminals(&actions, 700)[0].is_err());
    }
    e
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
fn native(actions: &[Action], owner: u32, branch: Branch) {
    let message = match branch {
        Branch::InvalidRead => "WEEX read credentials are incomplete in trusted signer setup",
        Branch::Primary => "WEEX account identity requires a non-public primary key",
        Branch::Stable => "WEEX account identity requires a trusted primary stableId",
        Branch::Paused => "WEEX account reads are rate limited",
        Branch::Remote => panic!("remote must produce HTTP"),
    };
    assert!(
        matches!(actions, [Action::Notice(Notice::CommandError { command, message: diagnostic }), Action::Notice(Notice::CommandFailed { command: failed, error })] if *command == owner && *failed == owner && diagnostic == message && error.message == message),
        "exact native diagnostic-before-terminal: {actions:?}"
    );
    let error = terminals(actions, owner).pop().unwrap().unwrap_err();
    assert_eq!(
        (error.kind, error.outcome),
        if matches!(branch, Branch::Stable) {
            (Some(ErrorKind::Unsupported), Some(Outcome::NotSent))
        } else {
            (None, None)
        }
    );
}
fn refused(actions: &[Action], owner: u32, kind: ErrorKind, message: &str) {
    assert_eq!(
        actions.len(),
        1,
        "no stale diagnostic or extra effect: {actions:?}"
    );
    let error = terminals(actions, owner).pop().unwrap().unwrap_err();
    assert_eq!(error.message, message);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
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
    assert_eq!(actions.len(), 1);
    assert_eq!(
        terminals(&actions, owner)
            .pop()
            .unwrap()
            .unwrap_err()
            .message,
        "retired fixture peer"
    );
}
fn success(e: &mut Engine<Probe>, command: u32, request: &HttpRequest) {
    let actions = respond(e, request, 200, CONFIG);
    assert_eq!(
        terminals(&actions, command),
        [Ok(r#"{"userId":"weex-account-7"}"#.into())]
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandError { .. })))
    );
    assert!(respond(e, request, 200, CONFIG).is_empty());
}
#[test]
fn native_local_order_metadata_and_store_are_preserved() {
    for branch in BRANCHES.into_iter().take(4) {
        let mut e = instance(branch);
        let before = e.account_snapshot_frame(NOW).to_vec();
        native(&ask(&mut e, NOW, 800), 800, branch);
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
    for opts in [
        Options {
            public_data_only: true,
            read_credentials: Some(read()),
            ..private()
        },
        Options {
            read_credentials: Some(read()),
            ..Default::default()
        },
    ] {
        let mut e = engine(opts);
        native(&ask(&mut e, NOW, 801), 801, Branch::Primary);
    }
    let mut e = engine(Options {
        stable_id: None,
        read_credentials: Some(ReadCredentials {
            secret: String::new(),
            ..read()
        }),
        ..private()
    });
    native(&ask(&mut e, NOW, 802), 802, Branch::InvalidRead);
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
    let actions: Vec<_> = e.drain_actions().collect();
    refused(
        &actions,
        900,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    refused(
        &ask(&mut e, NOW, 800),
        800,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    retire(&mut e, 1);
    let actions = ask(&mut e, NOW, 800);
    if matches!(branch, Branch::Remote) {
        let request = released(actions);
        success(&mut e, 800, &request);
    } else {
        native(&actions, 800, branch);
    }
    park(&mut e, 900);
    refused(
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
fn invalid_read_auth_refusal_requires_read_capacity() {
    capacity(Branch::InvalidRead);
}
#[test]
fn absent_primary_refusal_requires_read_capacity() {
    capacity(Branch::Primary);
}
#[test]
fn stable_identity_refusal_requires_read_capacity() {
    capacity(Branch::Stable);
}
#[test]
fn paused_identity_refusal_requires_read_capacity() {
    capacity(Branch::Paused);
}
#[test]
fn remote_entry_refuses_capacity_without_a_numeric_fallback() {
    capacity(Branch::Remote);
}
#[test]
fn invalid_clock_uses_engine_entry_without_native_diagnostic() {
    for branch in BRANCHES {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut e = instance(branch);
            refused(
                &ask(&mut e, now, 810),
                810,
                ErrorKind::InvalidInput,
                "Invalid request context",
            );
            let actions = ask(&mut e, NOW, 810);
            if matches!(branch, Branch::Remote) {
                let request = released(actions);
                success(&mut e, 810, &request);
            } else {
                native(&actions, 810, branch);
            }
        }
    }
}
#[test]
fn duplicate_and_fixed_clock_reuse_preserve_owners_and_retire_local_roots() {
    for branch in BRANCHES.into_iter().take(4) {
        let mut e = instance(branch);
        park(&mut e, 820);
        let actions = ask(&mut e, NOW, 820);
        assert!(terminals(&actions, 820).is_empty());
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
        );
        retire(&mut e, 820);
        for _ in 0..300 {
            native(&ask(&mut e, NOW, 820), 820, branch);
        }
        for owner in 1..=256 {
            park(&mut e, owner);
        }
    }
}
#[test]
fn native_remote_wire_primary_namespace_and_selected_signer_are_preserved() {
    for secondary in [false, true] {
        let mut e = engine(Options {
            read_credentials: secondary.then(read),
            ..private()
        });
        let before = e.account_snapshot_frame(NOW).to_vec();
        let actions = ask(&mut e, NOW, 830);
        let request = released(actions);
        assert_eq!(request.method, Method::Get);
        assert_eq!(
            request.url,
            "https://api-contract.weex.com/capi/v3/account/accountConfig"
        );
        assert_eq!(request.body, None);
        assert_eq!(request.route, None);
        let (key, secret, passphrase) = if secondary {
            ("read-key", "read-secret", "read-pass")
        } else {
            ("primary-key", "primary-secret", "primary-pass")
        };
        let expected = setup::oracle(
            key,
            secret,
            passphrase,
            Method::Get,
            "/capi/v3/account/accountConfig",
            "",
            "1700000000000",
        );
        assert_eq!(request.headers, expected);
        success(&mut e, 830, &request);
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
