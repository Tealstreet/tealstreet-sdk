//! The exported dispatch recipe reproduces the independent ethers / HTTP
//! serializer vectors through the real engine dispatcher and local provider.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{ASTERDEX_V3, NonceClaim};
use safe_cex_engine::request_context::{Credential, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::signing::{Identities, Identity, KeyEncoding};
use safe_cex_engine::test_support::{prepared_actions, seeded_entropy};
use safe_cex_engine::{Action, Command, Cx, Engine, HttpRequest, Method, Response, SignStep};
use safe_cex_engine::{SocketId, TimerId, Venue};
use safe_cex_venue_asterdex::sign::{Auth, Query, Recipe};
use std::collections::BTreeMap;
use std::sync::Arc;

const ORIGIN: &str = "https://fapi.asterdex.com";

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    private_key: String,
    user: String,
    signer: String,
    params: BTreeMap<String, String>,
    now_ms: f64,
    nonce: String,
    signature: String,
    code_query: String,
    code_signature: String,
    testnet: bool,
    v1_query: String,
}
#[derive(serde::Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

fn ordered_query(case: &Case) -> Query {
    let keys: &[&str] = if case.name.starts_with("documented") {
        &["symbol", "type", "side", "timeInForce", "quantity", "price"]
    } else if case.name.starts_with("decimal") {
        &["symbol", "quantity", "price", "reduceOnly", "note"]
    } else if case.name.starts_with("query") {
        &["symbol", "note"]
    } else {
        &[]
    };
    keys.iter()
        .fold(Query::new(), |q, key| q.str(key, &case.params[*key]))
}

fn provider(key: &str, secret: &str, encoding: KeyEncoding) -> (Arc<LocalSigner>, Identity) {
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(key.into()),
                secret: Secret::new(secret.into()),
                secret_encoding: encoding,
                ..Default::default()
            }),
            secondary: None,
        },
        safe_cex_engine::signing::CredentialSetupPolicy {
            primary: (encoding == KeyEncoding::Hex).then(|| {
                safe_cex_engine::signing::IdentitySetupPolicy {
                    signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Secp256k1Digest),
                    ..Default::default()
                }
            }),
            secondary: None,
        },
    )
    .unwrap();
    (Arc::new(signer), ids.primary.unwrap())
}

struct Issue(Vec<(Method, String, Box<dyn SignStep>)>);
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for (method, path, step) in self.0.drain(..) {
            let id = cx.request(method, format!("{ORIGIN}{path}"), vec![], None, ());
            cx.sign_http(id, step);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

type Build = Box<dyn FnOnce(Identity, NonceClaim) -> Recipe>;

/// A V3 issuer as AsterDEX runs it: the address domain is declared, the claim
/// is minted from the sealed identity, and the request is managed.
struct IssueV3(Vec<(Method, String, Build)>);
impl Venue for IssueV3 {
    type Pending = ();
    fn address_nonce_domain(&self) -> Option<&'static safe_cex_engine::nonce::NonceDomain> {
        Some(&ASTERDEX_V3)
    }
    fn public_address_slots(&self) -> safe_cex_engine::AddressSetupSlots {
        safe_cex_engine::AddressSetupSlots::Primary
    }
    fn dispatch_nonce() -> bool {
        true
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let now = cx.now;
        cx.observe_protocol_time(now);
        let ids = cx.address_nonce_identities();
        let slots = Slots::new(ids.primary.clone(), None, SlotPolicy::default());
        for (method, path, build) in self.0.drain(..) {
            let (selection, identity, claim) = slots
                .select_with_nonce(CredentialRole::Read { scope: None }, &ASTERDEX_V3)
                .unwrap();
            // A non-finite clock admits no context, so nothing is released.
            let Ok(context) = cx.http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: Some(Credential {
                        slot: selection.slot(),
                        reference: identity.credential.clone(),
                    }),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            ) else {
                continue;
            };
            let recipe = build(identity.clone(), claim);
            let queued = cx.request_signed_managed(
                context,
                None,
                method,
                format!("{ORIGIN}{path}"),
                None,
                Box::new(recipe),
                (),
            );
            assert!(queued.is_ok(), "the managed V3 request queues");
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// Signs one V3 request with a wallet through address setup and an engine
/// nonce claim; `None` when nothing is released.
fn send_v3(
    wallet: &str,
    key: &str,
    now: f64,
    method: Method,
    path: &str,
    build: Build,
) -> Option<HttpRequest> {
    let (signer, identity) = provider(wallet, key, KeyEncoding::Hex);
    let mut engine = Engine::new(
        IssueV3(vec![(method, path.into(), build)]),
        Some(seeded_entropy(5)),
    )
    .unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine
        .set_identities(Identities {
            primary: Some(identity),
            secondary: None,
        })
        .unwrap();
    engine.initialize_address_nonce(0.0).unwrap();
    engine.start(now);
    prepared_actions(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
}

fn nonce_of(url: &str) -> u64 {
    url.split("nonce=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn send(signer: Arc<LocalSigner>, now: f64, step: (Method, &str, Recipe)) -> HttpRequest {
    let (method, path, recipe) = step;
    let mut engine = Engine::new(
        Issue(vec![(method, path.into(), Box::new(recipe))]),
        Some(seeded_entropy(5)),
    )
    .unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(now);
    prepared_actions(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .expect("signed request released")
}

#[test]
fn v1_v3_and_code_recipes_match_independent_vectors() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/signing.json")).unwrap();
    for case in corpus.cases {
        let key = case.private_key.trim_start_matches("0x");

        // V1: HMAC over the raw query; identical wire bytes.
        let (signer, identity) = provider(&case.signer, "documentation-secret", KeyEncoding::Utf8);
        let path = "/fapi/v1/order";
        let recipe = Recipe::new(
            identity,
            Auth::V1,
            Method::Post,
            ORIGIN,
            path,
            ordered_query(&case),
            case.testnet,
        )
        .unwrap();
        let http = send(signer, case.now_ms, (Method::Post, path, recipe));
        assert_eq!(
            http.url,
            format!("{ORIGIN}{path}?{}", case.v1_query),
            "V1 {}",
            case.name
        );
        assert_eq!(http.headers, vec![("X-MBX-APIKEY", case.signer.clone())]);

        // Code EIP-712 and standard EIP-191 through address setup and the
        // engine nonce: each wire equals the recipe's own preimage and wire
        // code at the reserved nonce, which the unit vectors pin at the fixture
        // nonce. The engine nonce sits on the protocol-ms floor.
        // The oracle reproduces the independent vectors at the fixture nonce.
        let fixture: u64 = case.nonce.parse().unwrap();
        let oracle = |method| {
            safe_cex_venue_asterdex::sign::v3_wire_for_nonce(
                method,
                ORIGIN,
                "/fapi/v3/order",
                ordered_query(&case),
                &case.user,
                &case.signer,
                fixture,
                case.now_ms,
                case.testnet,
                &case.private_key,
            )
            .unwrap()
        };
        assert_eq!(
            oracle(Method::Post),
            format!(
                "{ORIGIN}/fapi/v3/order?{}&signature={}",
                case.code_query, case.code_signature
            ),
            "Code {}",
            case.name
        );
        let transport = case.v1_query.split("&signature=").next().unwrap();
        assert_eq!(
            oracle(Method::Delete),
            format!(
                "{ORIGIN}/fapi/v3/order?{transport}&user={}&signer={}&nonce={fixture}&signature={}",
                case.user, case.signer, case.signature
            ),
            "V3 {}",
            case.name
        );
        for method in [Method::Post, Method::Delete] {
            let path = "/fapi/v3/order";
            let user = case.user.clone();
            let testnet = case.testnet;
            let query = ordered_query(&case);
            let http = send_v3(
                &case.signer,
                key,
                case.now_ms,
                method,
                path,
                Box::new(move |identity, claim| {
                    Recipe::new(
                        identity,
                        Auth::V3 { user, claim },
                        method,
                        ORIGIN,
                        path,
                        query,
                        testnet,
                    )
                    .unwrap()
                }),
            )
            .expect("signed request released");
            let nonce = nonce_of(&http.url);
            assert!(nonce >= case.now_ms as u64 * 1000, "{} {nonce}", case.name);
            let expected = safe_cex_venue_asterdex::sign::v3_wire_for_nonce(
                method,
                ORIGIN,
                path,
                ordered_query(&case),
                &case.user,
                &case.signer,
                nonce,
                case.now_ms,
                case.testnet,
                &case.private_key,
            )
            .unwrap();
            assert_eq!(http.url, expected, "{method:?} {}", case.name);
            assert_eq!(http.headers, vec![("X-MBX-APIKEY", case.signer.clone())]);
        }
    }
}

#[test]
fn engine_nonce_reserves_monotonically_at_each_preparation_and_methods_must_agree() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/signing.json")).unwrap();
    let case = &corpus.cases[0];
    let key = case.private_key.trim_start_matches("0x");
    let mut seen = vec![];
    for _ in 0..2 {
        let user = case.user.clone();
        let path = "/fapi/v3/openOrders";
        let http = send_v3(
            &case.signer,
            key,
            case.now_ms,
            Method::Get,
            path,
            Box::new(move |identity, claim| {
                Recipe::new(
                    identity,
                    Auth::V3 { user, claim },
                    Method::Get,
                    ORIGIN,
                    path,
                    Query::new(),
                    false,
                )
                .unwrap()
            }),
        )
        .unwrap();
        seen.push(nonce_of(&http.url));
    }
    assert!(
        seen[0] >= case.now_ms as u64 * 1000 && seen[1] > seen[0],
        "{seen:?}"
    );

    let (signer, identity) = provider(&case.signer, "documentation-secret", KeyEncoding::Utf8);
    let path = "/fapi/v1/order";
    let recipe = Recipe::new(
        identity,
        Auth::V1,
        Method::Post,
        ORIGIN,
        path,
        Query::new(),
        false,
    )
    .unwrap();
    let mut engine = Engine::new(
        Issue(vec![(Method::Delete, path.into(), Box::new(recipe))]),
        Some(seeded_entropy(5)),
    )
    .unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(case.now_ms);
    assert!(
        prepared_actions(&mut engine, case.now_ms)
            .iter()
            .all(|action| !matches!(action, Action::Http(_))),
        "a POST recipe never signs a DELETE"
    );
}

// Ported from the deleted in-source eager oracle: each runs through the
// provider and `Recipe` at dispatch.

/// Independent Python hashlib/hmac vectors over the exact native V1 payload,
/// for two keys, each installed as the primary as the old oracle did. Actual
/// primary/secondary slot selection is pinned in tests/security.rs.
#[test]
fn v1_recipe_matches_independent_hmac_vectors_for_two_keys() {
    for (key, material, expected) in [
        (
            "primary-key",
            "primary-raw-secret",
            "29306ee689ad7468724aa8f72aed3a3bb507291d3c577faa91fda70fcf7cb3c0",
        ),
        (
            "secondary-key",
            "secondary-raw-secret",
            "cd0441e53172012534659d4a39a53bd924f19b2c4f35e4f01ca7be577ea7372d",
        ),
    ] {
        let (signer, identity) = provider(key, material, KeyEncoding::Utf8);
        let path = "/fapi/v1/account";
        let recipe = Recipe::new(
            identity,
            Auth::V1,
            Method::Get,
            ORIGIN,
            path,
            Query::new().str("symbol", "BTCUSDT"),
            false,
        )
        .unwrap();
        let http = send(signer, 1_700_000_000_000.0, (Method::Get, path, recipe));
        assert_eq!(
            http.url,
            format!(
                "{ORIGIN}{path}?symbol=BTCUSDT&recvWindow=20000&timestamp=1700000000000&signature={expected}"
            ),
            "{key}"
        );
    }
}

/// Caller-supplied signing fields never reach the wire: the recipe owns them.
#[test]
fn recipe_replaces_stale_unsigned_fields() {
    // A dedicated agent keeps this nonce sequence apart from other tests.
    let wallet = "91".repeat(32);
    let agent = safe_cex_crypto::ethereum::private_key_address(&wallet).unwrap();
    let path = "/fapi/v3/order";
    let query = Query::new()
        .str("symbol", "BTCUSDT")
        .str("signature", "stale")
        .str("timestamp", "1")
        .str("recvWindow", "1")
        .str("user", "stale")
        .str("signer", "stale")
        .str("nonce", "1");
    let http = send_v3(
        &agent,
        &wallet,
        200.0,
        Method::Post,
        path,
        Box::new(move |identity, claim| {
            let user = "0x0000000000000000000000000000000000000333".into();
            Recipe::new(
                identity,
                Auth::V3 { user, claim },
                Method::Post,
                ORIGIN,
                path,
                query,
                false,
            )
            .unwrap()
        }),
    )
    .unwrap();
    assert!(!http.url.contains("stale"), "{}", http.url);
    assert!(!http.url.contains("timestamp"), "{}", http.url);
    assert!(!http.url.contains("recvWindow"), "{}", http.url);
    assert_eq!(http.url.matches("&signature=").count(), 1);
    assert_eq!(http.url.matches("nonce=").count(), 1);
}

/// The nonce scope is the agent, not the owner: two owners sharing one agent
/// draw one strictly increasing sequence.
#[test]
fn owners_sharing_an_agent_share_its_nonce_sequence() {
    let wallet = "92".repeat(32);
    let agent = safe_cex_crypto::ethereum::private_key_address(&wallet).unwrap();
    let path = "/fapi/v3/account";
    let mut nonces = vec![];
    for (owner, now) in [
        ("0x0000000000000000000000000000000000000111", 1234.0),
        ("0x0000000000000000000000000000000000000222", 1234.0),
        ("0x0000000000000000000000000000000000000111", 1233.0),
    ] {
        let http = send_v3(
            &agent,
            &wallet,
            now,
            Method::Get,
            path,
            Box::new(move |identity, claim| {
                let user = owner.into();
                Recipe::new(
                    identity,
                    Auth::V3 { user, claim },
                    Method::Get,
                    ORIGIN,
                    path,
                    Query::new(),
                    false,
                )
                .unwrap()
            }),
        )
        .unwrap();
        nonces.push(nonce_of(&http.url));
    }
    assert_eq!(nonces, [1_234_000, 1_234_001, 1_234_002]);
}

/// Replaces the deleted oracle's non-finite-clock negative
/// (`validation_nonce_freshness_and_unsigned_fields`): a non-finite dispatch
/// clock releases nothing through the real dispatcher and provider.
#[test]
fn non_finite_dispatch_clock_releases_no_signed_request() {
    let wallet = "94".repeat(32);
    let agent = safe_cex_crypto::ethereum::private_key_address(&wallet).unwrap();
    let path = "/fapi/v3/account";
    for now in [f64::NAN, f64::INFINITY] {
        let (signer, identity) = provider(&agent, "fixture-only-v1-material", KeyEncoding::Utf8);
        let recipe = Recipe::new(
            identity,
            Auth::V1,
            Method::Get,
            ORIGIN,
            path,
            Query::new(),
            false,
        )
        .unwrap();
        let mut engine = Engine::new(
            Issue(vec![(Method::Get, path.into(), Box::new(recipe))]),
            Some(seeded_entropy(5)),
        )
        .unwrap();
        engine.set_signer(Some(signer)).unwrap();
        engine.start(now);
        let actions = prepared_actions(&mut engine, now);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "V1 {now}"
        );
        let released = send_v3(
            &agent,
            &wallet,
            now,
            Method::Get,
            path,
            Box::new(move |identity, claim| {
                let user = "0x0000000000000000000000000000000000000444".into();
                Recipe::new(
                    identity,
                    Auth::V3 { user, claim },
                    Method::Get,
                    ORIGIN,
                    path,
                    Query::new(),
                    false,
                )
                .unwrap()
            }),
        );
        assert!(released.is_none(), "V3 {now}");
    }
}
