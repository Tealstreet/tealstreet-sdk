//! Original native identity boundaries: command admission and terminal custody.
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Response, VenueError};
use safe_cex_venue_backpack::{Backpack, Options};
type Driver = Engine<Backpack>;
#[derive(Clone, Copy)]
enum Refusal {
    PublicOnly,
    MissingCredentials,
    InvalidSecondary,
}
const CASES: [Refusal; 3] = [
    Refusal::PublicOnly,
    Refusal::MissingCredentials,
    Refusal::InvalidSecondary,
];
impl Refusal {
    fn options(self) -> Options {
        match self {
            Self::PublicOnly => Options {
                public_data_only: true,
                ..Default::default()
            },
            Self::MissingCredentials => Options::default(),
            Self::InvalidSecondary => Options {
                public_data_only: true,
                read_credentials: Some(safe_cex_engine::ReadCredentials {
                    key: "secondary-key".into(),
                    secret: String::new(),
                    passphrase: None,
                }),
                ..Default::default()
            },
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::PublicOnly => "Backpack account identity requires credentials",
            Self::MissingCredentials => "Backpack credentials unavailable",
            Self::InvalidSecondary => {
                "Secondary read credentials require a nonempty key and secret"
            }
        }
    }
}
fn engine(options: Options) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(options));
    e.store_mut().set_hedged(true);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 90.,
        used: 33.,
        upnl: 7.,
    });
    e
}
fn unchanged(e: &Driver) {
    assert!(e.store().hedged());
    let b = e.store().balance();
    assert_eq!((b.total, b.free, b.used, b.upnl), (123., 90., 33., 7.));
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}
fn ask(e: &mut Driver, command: u32, now: f64) -> Vec<Action> {
    e.handle(now, Event::Command(Command::FetchAccount { command }));
    e.drain_actions().collect()
}
fn failure(actions: &[Action], command: u32) -> &VenueError {
    assert!(
        actions.iter().all(|a| matches!(a, Action::Notice(_))),
        "local refusal creates no physical work"
    );
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)
    ));
    errors[0]
}
fn native(case: Refusal) {
    let mut e = engine(case.options());
    let actions = ask(&mut e, 7, 10.);
    let error = failure(&actions, 7);
    assert_eq!(error.message, case.message());
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
    let diagnostics: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::CommandError {
                command: 7,
                message,
            }) => Some((i, message)),
            _ => None,
        })
        .collect();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].1, case.message());
    let terminal = actions
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
        .unwrap();
    assert!(diagnostics[0].0 < terminal);
    unchanged(&e);
}
#[test]
fn native_public_only_refusal() {
    native(Refusal::PublicOnly);
}
#[test]
fn native_missing_credentials_refusal() {
    native(Refusal::MissingCredentials);
}
#[test]
fn native_secondary_validation_precedes_public_only_refusal() {
    native(Refusal::InvalidSecondary);
}
fn clock(case: Refusal) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(case.options());
        let actions = ask(&mut e, 7, now);
        let error = failure(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        unchanged(&e);
    }
}
#[test]
fn public_only_obeys_shared_clock() {
    clock(Refusal::PublicOnly);
}
#[test]
fn missing_credentials_obeys_shared_clock() {
    clock(Refusal::MissingCredentials);
}
#[test]
fn invalid_secondary_obeys_shared_clock() {
    clock(Refusal::InvalidSecondary);
}
fn park(e: &mut Driver, command: u32) -> Preparation {
    e.handle(
        10.,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: "SOL_USDC_PERP".into(),
            limit: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("working managed book owner")
}
fn complete(e: &mut Driver, token: Preparation, command: u32) {
    e.http_prepared(
        12.,
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
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prior owner still binds");
    assert!(matches!(e.claim_http(12., permit), Ok(Claim::Released(_))));
    e.handle(
        13.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"bids":[],"asks":[]}"#,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: id, result }) if *id == command && result == r#"{"bids":[],"asks":[]}"#)).count(), 1);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command)
    ));
}
fn capacity(case: Refusal) {
    let mut e = engine(case.options());
    let first = park(&mut e, 1);
    for id in 2..=256 {
        park(&mut e, id);
    }
    let actions = ask(&mut e, 257, 11.);
    let error = failure(&actions, 257);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    complete(&mut e, first, 1);
    unchanged(&e);
}
#[test]
fn public_only_obeys_shared_capacity() {
    capacity(Refusal::PublicOnly);
}
#[test]
fn missing_credentials_obeys_shared_capacity() {
    capacity(Refusal::MissingCredentials);
}
#[test]
fn invalid_secondary_obeys_shared_capacity() {
    capacity(Refusal::InvalidSecondary);
}
fn duplicate(case: Refusal) {
    let mut e = engine(case.options());
    let first = park(&mut e, 7);
    let actions = ask(&mut e, 7, 11.);
    assert!(actions.iter().all(|a| matches!(a, Action::Notice(_))));
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "command.duplicate")
    ));
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandError { command: 7, .. })
                | Action::Notice(Notice::CommandFailed { command: 7, .. })
                | Action::Notice(Notice::CommandDone { command: 7, .. })
        )),
        "duplicate identity entry cannot publish into the prior owner"
    );
    complete(&mut e, first, 7);
    unchanged(&e);
}
#[test]
fn duplicate_public_only_preserves_prior_owner() {
    duplicate(Refusal::PublicOnly);
}
#[test]
fn duplicate_missing_credentials_preserves_prior_owner() {
    duplicate(Refusal::MissingCredentials);
}
#[test]
fn duplicate_invalid_secondary_preserves_prior_owner() {
    duplicate(Refusal::InvalidSecondary);
}
#[test]
fn existing_book_owner_working_delivery_control() {
    let mut e = engine(Options::default());
    let first = park(&mut e, 7);
    complete(&mut e, first, 7);
    unchanged(&e);
}
#[test]
fn local_identity_refusal_releases_id_for_reuse() {
    for case in CASES {
        let mut e = engine(case.options());
        for now in [10., 11.] {
            let actions = ask(&mut e, 7, now);
            assert_eq!(failure(&actions, 7).message, case.message());
        }
        unchanged(&e);
    }
}
fn live() -> Driver {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    engine(Options {
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: STANDARD.encode([8u8; 32]),
            passphrase: None,
        }),
        ..Default::default()
    })
}
fn identity_request(e: &mut Driver) -> safe_cex_engine::HttpRequest {
    let mut requests = ask(e, 7, 1000.).into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let request = requests
        .next()
        .expect("original native signed identity request");
    assert!(requests.next().is_none());
    assert_eq!(request.url, "https://api.backpack.exchange/api/v1/account");
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert_eq!(request.body, None);
    assert_eq!(request.route, None);
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| *k == "X-API-Key" && v == "read-key")
    );
    request
}
fn identity_response(
    e: &mut Driver,
    request: safe_cex_engine::RequestId,
    now: f64,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
#[test]
fn native_signed_identity_success_and_failure_preserve_store() {
    for (body, expected) in [
        (
            r#"{"accountId":9007199254740993,"userId":"other"}"#,
            Some(r#"{"userId":"9007199254740993","affiliateId":null}"#),
        ),
        ("{}", None),
    ] {
        let mut e = live();
        let r = identity_request(&mut e);
        let actions = identity_response(&mut e, r.id, 1001., body);
        if let Some(expected) = expected {
            assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == expected)).count(), 1);
        } else {
            assert_eq!(
                failure(&actions, 7).message,
                "Backpack account response did not include an account ID or user ID"
            );
        }
        unchanged(&e);
    }
}
#[test]
fn identity_delivery_after_original_root_maximum_cannot_publish_native_id() {
    let mut e = live();
    let r = identity_request(&mut e);
    let actions = identity_response(&mut e, r.id, 21_001., r#"{"accountId":"late"}"#);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))),
        "late identity data cannot finish beyond the original20s maximum"
    );
    unchanged(&e);
}

#[test]
fn bad_secondary_seed_keeps_native_identity_refusal_and_no_send() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    for (secret, message) in [
        ("not base64!".to_string(), "Invalid Backpack base64 seed"),
        (STANDARD.encode([8; 31]), "Ed25519 seed must be 32 bytes"),
    ] {
        for public_data_only in [false, true] {
            let mut e = engine(Options {
                public_data_only,
                read_credentials: Some(safe_cex_engine::ReadCredentials {
                    key: "read-key".into(),
                    secret: secret.clone(),
                    passphrase: None,
                }),
                ..Default::default()
            });
            let actions = ask(&mut e, 7, 10.);
            let error = failure(&actions, 7);
            assert_eq!(
                error.message,
                if public_data_only {
                    "Backpack account identity requires credentials"
                } else {
                    message
                }
            );
            assert_eq!(error.kind, None);
            assert_eq!(error.outcome, None);
            unchanged(&e);
        }
    }
}

#[test]
fn unregistered_direct_constructor_keeps_native_missing_credentials_refusal() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut e = engine(Options {
        key: Some("primary-key".into()),
        secret: Some(STANDARD.encode([7; 32])),
        ..Default::default()
    });
    let actions = ask(&mut e, 7, 10.);
    let error = failure(&actions, 7);
    assert_eq!(error.message, "Backpack credentials unavailable");
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
    unchanged(&e);
}
