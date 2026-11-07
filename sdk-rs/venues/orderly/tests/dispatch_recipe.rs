//! The exported Orderly dispatch recipe reproduces the legacy signed headers
//! byte for byte through the real engine dispatcher, with the seed installed
//! by trusted setup as declared base58 and signed only by the provider.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, Identity, IdentitySetupPolicy, KeyEncoding,
};
use safe_cex_engine::test_support::{prepared_actions, seeded_entropy};
use safe_cex_engine::{
    Action, Command, Cx, Engine, HttpRequest, Method, Response, SignStep, SocketId, TimerId, Venue,
};
use safe_cex_venue_orderly::sign;
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static FAILURES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

const NOW: f64 = 1_700_000_000_123.0;
const REST: &str = "https://api.orderly.org";
const ACCOUNT: &str = "0xaccount";
// RFC 8032 test 1: a published seed/public-key pair, not account material.
const SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const PUBLIC: [u8; 32] = [
    0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
    0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
];

fn public_b58() -> String {
    bs58::encode(PUBLIC).into_string()
}

/// The provider holds the seed; the identity carries only refs.
fn provider() -> (Arc<LocalSigner>, Identity) {
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Base58),
            signing_algorithm: Some(Algorithm::Ed25519),
            ..Default::default()
        }),
        secondary: None,
    };
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(format!("ed25519:{}", public_b58())),
                secret: Secret::new(bs58::encode(SEED).into_string()),
                ..Default::default()
            }),
            secondary: None,
        },
        policy,
    )
    .unwrap();
    (Arc::new(signer), ids.primary.unwrap())
}

/// One request to issue: method, path with query, body and its recipe.
type Signed = (Method, String, Option<String>, Box<dyn SignStep>);

struct Issue(Vec<Signed>);
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for (method, path, body, step) in self.0.drain(..) {
            let id = cx.request(method, format!("{REST}{path}"), vec![], body, ());
            cx.sign_http(id, step);
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

fn run(requests: Vec<Signed>) -> Vec<HttpRequest> {
    FAILURES.with(|failures| failures.borrow_mut().clear());
    let (signer, _) = provider();
    let mut engine = Engine::new(Issue(requests), Some(seeded_entropy(3))).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    prepared_actions(&mut engine, NOW)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect()
}

#[test]
fn dispatch_recipe_headers_equal_the_legacy_signer_for_every_method() {
    let (_, identity) = provider();
    let legacy =
        sign::Credentials::new(&public_b58(), &bs58::encode(SEED).into_string(), ACCOUNT).unwrap();
    let cases = [
        (Method::Get, "/v1/positions", None),
        (
            Method::Get,
            "/v1/orders?status=INCOMPLETE&page=1&size=500",
            None,
        ),
        (
            Method::Post,
            "/v1/order",
            Some(
                r#"{"symbol":"PERP_BTC_USDC","order_type":"LIMIT","client_order_id":"caller-42"}"#,
            ),
        ),
        (
            Method::Put,
            "/v1/order",
            Some(r#"{"order_id":7,"order_price":"64000"}"#),
        ),
        (
            Method::Delete,
            "/v1/order?order_id=7&symbol=PERP_BTC_USDC",
            None,
        ),
    ];
    let requests = cases
        .iter()
        .map(|(method, path, body)| {
            (
                *method,
                (*path).to_owned(),
                body.map(str::to_owned),
                Box::new(sign::Http::new(
                    identity.clone(),
                    ACCOUNT,
                    *method,
                    REST,
                    path,
                )) as Box<dyn SignStep>,
            )
        })
        .collect();
    let released = run(requests);
    assert_eq!(
        FAILURES.with(|failures| failures.borrow().clone()),
        Vec::<String>::new()
    );
    assert_eq!(released.len(), cases.len());
    for ((method, path, body), http) in cases.iter().zip(&released) {
        let expected = legacy.headers(*method, path, *body, NOW).unwrap();
        assert_eq!(http.headers, expected, "{method:?} {path}");
        assert_eq!(http.body.as_deref(), *body);
        // Independent check: the released signature verifies under the public key.
        let header = |name: &str| &http.headers.iter().find(|(k, _)| *k == name).unwrap().1;
        use base64::Engine as _;
        let signature = base64::engine::general_purpose::URL_SAFE
            .decode(header("orderly-signature"))
            .unwrap();
        assert!(
            safe_cex_crypto::ed25519::verify(
                header("orderly-textsign").as_bytes(),
                &PUBLIC,
                &signature
            )
            .unwrap()
        );
        assert_eq!(header("orderly-key"), &format!("ed25519:{}", public_b58()));
    }
}

#[test]
fn a_recipe_for_a_different_request_is_refused_before_any_http() {
    let (_, identity) = provider();
    // Different route, another host, and a recipe naming only a suffix of the
    // real path: none may be signed for the request that would go out.
    for (origin, path) in [
        (REST, "/v1/withdraw"),
        ("https://evil.example", "/v1/order"),
        (REST, "/order"),
        ("https://api.orderly.org/v1", "/order"),
        ("https://api.orderly.org@evil.example", "/v1/order"),
    ] {
        let released = run(vec![(
            Method::Post,
            "/v1/order".into(),
            Some("{}".into()),
            Box::new(sign::Http::new(
                identity.clone(),
                ACCOUNT,
                Method::Post,
                origin,
                path,
            )),
        )]);
        assert!(released.is_empty(), "{origin}{path}");
        assert_eq!(
            FAILURES.with(|failures| failures.borrow().len()),
            1,
            "{origin}{path}"
        );
    }
}
