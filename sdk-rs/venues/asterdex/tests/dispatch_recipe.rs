//! The exported dispatch recipe reproduces the independent ethers / HTTP
//! serializer vectors through the real engine dispatcher and local provider.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{Identity, KeyEncoding};
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
        let v3 = |nonce: u64| Auth::V3 {
            user: case.user.clone(),
            signer: case.signer.clone(),
            nonce: Box::new(move |_, _| Ok(nonce)),
        };
        let nonce: u64 = case.nonce.parse().unwrap();
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

        // Code EIP-712: raw (unencoded) query and exact signature.
        let (signer, identity) = provider(&case.signer, key, KeyEncoding::Hex);
        let path = "/fapi/v3/order";
        let recipe = Recipe::new(
            identity.clone(),
            v3(nonce),
            Method::Post,
            ORIGIN,
            path,
            ordered_query(&case),
            case.testnet,
        )
        .unwrap();
        let http = send(signer.clone(), case.now_ms, (Method::Post, path, recipe));
        assert_eq!(
            http.url,
            format!(
                "{ORIGIN}{path}?{}&signature={}",
                case.code_query, case.code_signature
            ),
            "Code {}",
            case.name
        );

        // Standard EIP-191: transport fields in caller order, ethers signature.
        let recipe = Recipe::new(
            identity,
            v3(nonce),
            Method::Delete,
            ORIGIN,
            path,
            ordered_query(&case),
            case.testnet,
        )
        .unwrap();
        let http = send(signer, case.now_ms, (Method::Delete, path, recipe));
        // V1's wire carries the same encoded business params, window and stamp.
        let transport = case.v1_query.split("&signature=").next().unwrap();
        assert_eq!(
            http.url,
            format!(
                "{ORIGIN}{path}?{transport}&user={}&signer={}&nonce={nonce}&signature={}",
                case.user, case.signer, case.signature
            ),
            "V3 {}",
            case.name
        );
    }
}

#[test]
fn interim_nonce_reserves_monotonically_at_each_preparation_and_methods_must_agree() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/signing.json")).unwrap();
    let case = &corpus.cases[0];
    let key = case.private_key.trim_start_matches("0x");
    let nonce_of = |url: &str| -> u64 {
        url.split("&nonce=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let mut seen = vec![];
    for _ in 0..2 {
        let (signer, identity) = provider(&case.signer, key, KeyEncoding::Hex);
        let auth = Auth::V3 {
            user: case.user.clone(),
            signer: case.signer.clone(),
            nonce: safe_cex_venue_asterdex::sign::interim_nonce(),
        };
        let path = "/fapi/v3/openOrders";
        let recipe = Recipe::new(
            identity,
            auth,
            Method::Get,
            ORIGIN,
            path,
            Query::new(),
            false,
        )
        .unwrap();
        seen.push(nonce_of(
            &send(signer, case.now_ms, (Method::Get, path, recipe)).url,
        ));
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
    let (signer, identity) = provider(&agent, &wallet, KeyEncoding::Hex);
    let path = "/fapi/v3/order";
    let query = Query::new()
        .str("symbol", "BTCUSDT")
        .str("signature", "stale")
        .str("timestamp", "1")
        .str("recvWindow", "1")
        .str("user", "stale")
        .str("signer", "stale")
        .str("nonce", "1");
    let recipe = Recipe::new(
        identity,
        Auth::V3 {
            user: "0x0000000000000000000000000000000000000333".into(),
            signer: agent,
            nonce: safe_cex_venue_asterdex::sign::interim_nonce(),
        },
        Method::Post,
        ORIGIN,
        path,
        query,
        false,
    )
    .unwrap();
    let http = send(signer, 200.0, (Method::Post, path, recipe));
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
    let nonce_of = |url: &str| -> u64 {
        url.split("nonce=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let path = "/fapi/v3/account";
    let mut nonces = vec![];
    for (owner, now) in [
        ("0x0000000000000000000000000000000000000111", 1234.0),
        ("0x0000000000000000000000000000000000000222", 1234.0),
        ("0x0000000000000000000000000000000000000111", 1233.0),
    ] {
        let (signer, identity) = provider(&agent, &wallet, KeyEncoding::Hex);
        let recipe = Recipe::new(
            identity,
            Auth::V3 {
                user: owner.into(),
                signer: agent.clone(),
                nonce: safe_cex_venue_asterdex::sign::interim_nonce(),
            },
            Method::Get,
            ORIGIN,
            path,
            Query::new(),
            false,
        )
        .unwrap();
        nonces.push(nonce_of(
            &send(signer, now, (Method::Get, path, recipe)).url,
        ));
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
    for (auth, encoding, material) in [
        (
            Auth::V1,
            KeyEncoding::Utf8,
            "fixture-only-v1-material".to_owned(),
        ),
        (
            Auth::V3 {
                user: "0x0000000000000000000000000000000000000444".into(),
                signer: agent.clone(),
                nonce: safe_cex_venue_asterdex::sign::interim_nonce(),
            },
            KeyEncoding::Hex,
            wallet.clone(),
        ),
    ] {
        for now in [f64::NAN, f64::INFINITY] {
            let (signer, identity) = provider(&agent, &material, encoding);
            let path = "/fapi/v3/account";
            let recipe = Recipe::new(
                identity,
                match &auth {
                    Auth::V1 => Auth::V1,
                    Auth::V3 { user, signer, .. } => Auth::V3 {
                        user: user.clone(),
                        signer: signer.clone(),
                        nonce: safe_cex_venue_asterdex::sign::interim_nonce(),
                    },
                },
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
                "{now}"
            );
        }
    }
}
