//! R5: `AuthRecipe` through the real engine dispatcher and `LocalSigner`
//! places the legacy `auth_token` byte for byte in each of Lighter's three
//! placements, and refuses foreign targets and colliding auth before signing.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::entropy::Entropy;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, Identity, IdentitySetupPolicy, KeyEncoding,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, HttpRequest, Method, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_lighter::sign::{Account, AuthPlacement, AuthRecipe, Signer};
use std::cell::RefCell;

const ORIGIN: &str = "https://mainnet.zklighter.elliot.ai";
const STREAM: &str = "wss://mainnet.zklighter.elliot.ai/stream";
const KEY: &str =
    "0x0b2d3a1c9e8f7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a3928170605040302010000000000";
const NOW: f64 = 1_700_000_000_000.;
const ENTROPY: u8 = 0x5a;
const ORDERS: &str = "/api/v1/accountActiveOrders?account_index=42&market_id=255";

thread_local! {
    static FAILURES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn account() -> Account {
    Account::new(304, 42, 3).unwrap()
}

fn identity() -> (std::sync::Arc<LocalSigner>, Identity) {
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            signing_algorithm: Some(Algorithm::SchnorrDigest),
            ..Default::default()
        }),
        secondary: None,
    };
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("lighter-api-key".into()),
                secret: Secret::new(KEY.into()),
                secret_encoding: KeyEncoding::Hex,
                ..Default::default()
            }),
            secondary: None,
        },
        policy,
    )
    .unwrap();
    (std::sync::Arc::new(signer), ids.primary.unwrap())
}

/// Constant signing entropy, so the hedged nonce matches the legacy fill.
fn entropy() -> Entropy {
    Entropy::new(
        Box::new(|bytes| {
            bytes.fill(1);
            Ok(())
        }),
        Box::new(|bytes| {
            bytes.fill(ENTROPY);
            Ok(())
        }),
    )
}

/// The legacy token for the deadline the recipe derives from its stamp.
fn legacy_token() -> String {
    Signer::new(KEY, 304, 42, 3)
        .unwrap()
        .auth_token((NOW / 1000.).floor() as u64 + 60, |bytes| {
            bytes.fill(ENTROPY);
            Ok(())
        })
        .unwrap()
}

enum Job {
    Http {
        method: Method,
        path: &'static str,
        headers: Vec<(&'static str, String)>,
        origin: &'static str,
        placement: fn() -> AuthPlacement,
    },
    /// Binds the recipe to the first socket, then sends on `send_on`.
    Ws { send_on: usize },
}

struct Issue {
    job: Option<Job>,
    identity: Identity,
}
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        match self.job.take().unwrap() {
            Job::Http {
                method,
                path,
                headers,
                origin,
                placement,
            } => {
                let id = cx.request(method, format!("{ORIGIN}{path}"), headers, None, ());
                let recipe =
                    AuthRecipe::new(self.identity.clone(), account(), origin.into(), placement());
                cx.sign_http(id, Box::new(recipe));
            }
            Job::Ws { send_on } => {
                let sockets = [cx.connect(STREAM), cx.connect(STREAM)];
                let recipe = AuthRecipe::new(
                    self.identity.clone(),
                    account(),
                    ORIGIN.into(),
                    AuthPlacement::WsSubscribe {
                        socket: sockets[0],
                        url: STREAM.into(),
                        channel: "account_all_orders/42".into(),
                    },
                );
                cx.send_signed(sockets[send_on], String::new(), Box::new(recipe));
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        FAILURES.with(|failures| failures.borrow_mut().push(error.into()));
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn run(job: Job) -> Vec<Action> {
    FAILURES.with(|failures| failures.borrow_mut().clear());
    let (signer, identity) = identity();
    let mut engine = Engine::new(
        Issue {
            job: Some(job),
            identity,
        },
        Some(entropy()),
    )
    .unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    prepared_actions(&mut engine, NOW)
}

fn http(actions: &[Action]) -> Option<&HttpRequest> {
    actions.iter().find_map(|action| match action {
        Action::Http(http) => Some(http),
        _ => None,
    })
}

fn get(path: &'static str, placement: fn() -> AuthPlacement) -> Job {
    Job::Http {
        method: Method::Get,
        path,
        headers: Vec::new(),
        origin: ORIGIN,
        placement,
    }
}

#[test]
fn query_placement_appends_the_legacy_token_byte_for_byte() {
    let actions = run(get(ORDERS, || AuthPlacement::Query));
    let request = http(&actions).expect("signed request released");
    // Legacy appends a raw `&auth=` with its colons unescaped.
    assert_eq!(
        request.url,
        format!("{ORIGIN}{ORDERS}&auth={}", legacy_token())
    );
    assert!(FAILURES.with(|failures| failures.borrow().is_empty()));
}

#[test]
fn header_placement_carries_the_legacy_token() {
    let actions = run(get("/api/v1/accountLimits?account_index=42", || {
        AuthPlacement::Header
    }));
    let request = http(&actions).expect("signed request released");
    assert_eq!(
        request.headers,
        vec![("authorization", legacy_token())],
        "exactly one authorization header"
    );
}

#[test]
fn ws_placement_writes_the_legacy_subscribe_frame() {
    let actions = run(Job::Ws { send_on: 0 });
    let frame = actions
        .iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("signed subscribe released");
    assert_eq!(
        frame,
        format!(
            r#"{{"type":"subscribe","channel":"account_all_orders/42","auth":"{}"}}"#,
            legacy_token()
        )
    );
}

#[test]
fn a_send_on_another_socket_is_refused_before_signing() {
    let actions = run(Job::Ws { send_on: 1 });
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::WsSend { .. } | Action::Sign { .. })),
        "{actions:?}"
    );
}

#[test]
fn an_exact_auth_parameter_collides_but_a_lookalike_does_not() {
    let colliding = run(get(
        "/api/v1/accountActiveOrders?account_index=42&auth=stale",
        || AuthPlacement::Query,
    ));
    assert!(http(&colliding).is_none());
    assert_eq!(FAILURES.with(|failures| failures.borrow().len()), 1);
    for path in [
        "/api/v1/accountActiveOrders?account_index=42&author=1",
        "/api/v1/accountActiveOrders?account_index=42&note=auth",
    ] {
        let path: &'static str = path;
        let actions = run(Job::Http {
            method: Method::Get,
            path,
            headers: Vec::new(),
            origin: ORIGIN,
            placement: || AuthPlacement::Query,
        });
        let request = http(&actions).expect("a lookalike is not a collision");
        assert!(request.url.ends_with(&format!("&auth={}", legacy_token())));
    }
}

#[test]
fn a_foreign_authorization_header_is_refused() {
    let actions = run(Job::Http {
        method: Method::Get,
        path: "/api/v1/accountLimits?account_index=42",
        headers: vec![("Authorization", "foreign".into())],
        origin: ORIGIN,
        placement: || AuthPlacement::Header,
    });
    assert!(http(&actions).is_none());
    assert_eq!(FAILURES.with(|failures| failures.borrow().len()), 1);
}

#[test]
fn a_foreign_origin_or_non_api_path_is_refused() {
    for (origin, path) in [
        ("https://other.example", ORDERS),
        (ORIGIN, "/info?account_index=42"),
    ] {
        let actions = run(Job::Http {
            method: Method::Get,
            path,
            headers: Vec::new(),
            origin,
            placement: || AuthPlacement::Query,
        });
        assert!(http(&actions).is_none(), "{origin}{path}");
    }
}

struct Parked(std::sync::Arc<LocalSigner>);
impl safe_cex_engine::signing::Signer for Parked {
    fn immediate(&self) -> bool {
        false
    }
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        safe_cex_engine::signing::Signer::sign(&*self.0, work)
    }
    fn sign_ready(
        &self,
        _: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        None
    }
}

fn sign_work(
    engine: &mut Engine<Issue>,
) -> (
    safe_cex_engine::RequestId,
    safe_cex_engine::signing::SignRequest,
) {
    let mut works: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            Action::Http(_) => panic!("a parked provider must not release"),
            _ => None,
        })
        .collect();
    assert_eq!(works.len(), 1);
    works.remove(0)
}

fn signed_bytes(work: &safe_cex_engine::signing::SignRequest) -> Vec<u8> {
    match &work.operations[..] {
        [safe_cex_engine::signing::Operation::Sign { payload, .. }] => match &payload[..] {
            [safe_cex_engine::signing::PayloadPart::Bytes { bytes }] => bytes.clone(),
            _ => panic!("one byte payload"),
        },
        _ => panic!("one Sign operation"),
    }
}

/// A stale result releases nothing; re-preparation draws fresh entropy and a
/// fresh 60-second deadline from its own stamp, and only that token is sent.
#[test]
fn re_preparation_replaces_entropy_and_deadline() {
    let mut next = 0u8;
    let counting = Entropy::new(
        Box::new(|bytes| {
            bytes.fill(1);
            Ok(())
        }),
        Box::new(move |bytes| {
            next = next.wrapping_add(1);
            bytes.fill(next);
            Ok(())
        }),
    );
    let (local, identity) = identity();
    let mut engine = Engine::new(
        Issue {
            job: Some(get(ORDERS, || AuthPlacement::Query)),
            identity,
        },
        Some(counting),
    )
    .unwrap();
    engine
        .set_signer(Some(std::sync::Arc::new(Parked(local.clone()))))
        .unwrap();
    engine.start(NOW);
    engine.prepare_dispatch(NOW);
    let (old, old_work) = sign_work(&mut engine);
    let old_result = safe_cex_engine::signing::Signer::sign_ready(&*local, &old_work)
        .unwrap()
        .unwrap();
    let later = NOW + 5_001.;
    engine.handle(
        later,
        safe_cex_engine::Event::Signed {
            attempt: old,
            result: Ok(old_result),
        },
    );
    let (fresh, fresh_work) = sign_work(&mut engine);
    let (old_bytes, fresh_bytes) = (signed_bytes(&old_work), signed_bytes(&fresh_work));
    assert_ne!(old_bytes[..32], fresh_bytes[..32], "fresh entropy");
    let deadline = |bytes: &[u8]| -> String {
        String::from_utf8(bytes[32..].to_vec())
            .unwrap()
            .trim_end_matches('\0')
            .split(':')
            .next()
            .unwrap()
            .to_owned()
    };
    let base = (NOW / 1000.).floor() as u64;
    assert_eq!(deadline(&old_bytes), (base + 60).to_string());
    assert_eq!(deadline(&fresh_bytes), (base + 5 + 60).to_string());
    let fresh_result = safe_cex_engine::signing::Signer::sign_ready(&*local, &fresh_work)
        .unwrap()
        .unwrap();
    engine.handle(
        later,
        safe_cex_engine::Event::Signed {
            attempt: fresh,
            result: Ok(fresh_result),
        },
    );
    let released: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http.url),
            _ => None,
        })
        .collect();
    assert_eq!(released.len(), 1);
    assert!(released[0].contains(&format!("&auth={}:42:3:", base + 5 + 60)));
}

/// The WS signing context is the socket's own URL, supplied by the venue; a
/// send carries no URL, so nothing is derived from the chain id.
#[test]
fn ws_signing_context_is_the_supplied_stream_url() {
    let (local, identity) = identity();
    let mut engine = Engine::new(
        Issue {
            job: Some(Job::Ws { send_on: 0 }),
            identity,
        },
        Some(entropy()),
    )
    .unwrap();
    engine
        .set_signer(Some(std::sync::Arc::new(Parked(local))))
        .unwrap();
    engine.start(NOW);
    engine.prepare_dispatch(NOW);
    let (_, work) = sign_work(&mut engine);
    assert_eq!(work.context.venue, "lighter");
    assert_eq!(work.context.method, "WS");
    assert_eq!(work.context.host, "mainnet.zklighter.elliot.ai");
    assert_eq!(work.context.path, "/stream");
}
