//! The exported Coinbase dispatch recipes build the legacy JWT at release
//! through the real engine dispatcher: the provider holds the key, presents
//! the key name and signs the exact JWS input; the venue holds refs only.
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use p256::pkcs8::EncodePrivateKey;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{
    CredentialSetupPolicy, Identities, Identity, IdentitySetupPolicy, KeyEncoding, KeyRef,
    SignFuture, SignRequest, Signer,
};
use safe_cex_engine::test_support::{prepared_actions, seeded_entropy};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Response, SignStep, SocketId, TimerId,
    Venue,
};
use safe_cex_venue_coinbase::sign::{self, JwtAlgorithm};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static FAILURES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

const NOW: f64 = 1_700_000_123_456.0;
const ORIGIN: &str = "https://api.coinbase.com";
const KEY: &str = "organizations/org-id/apiKeys/key-id";
const READ_KEY: &str = "organizations/org-id/apiKeys/read-id";
// RFC 8032 test 1: a published seed/public-key pair, not account material.
const ED_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const ED_PUBLIC: [u8; 32] = [
    0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
    0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
];

fn ec_key() -> p256::SecretKey {
    p256::SecretKey::from_slice(&[7; 32]).unwrap()
}
fn ec_pem() -> String {
    let der = ec_key().to_pkcs8_der().unwrap();
    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(der.as_bytes())
    )
}

/// Trading slot: an ES256 PEM key; read slot: an Ed25519 keypair (seed || public).
fn provider() -> (Arc<LocalSigner>, Identities) {
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Pem),
            ..Default::default()
        }),
        secondary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Base64),
            ..Default::default()
        }),
    };
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(KEY.into()),
                secret: Secret::new(ec_pem()),
                ..Default::default()
            }),
            secondary: Some(LocalIdentity {
                key: Secret::new(READ_KEY.into()),
                secret: Secret::new(STANDARD.encode([ED_SEED, ED_PUBLIC].concat())),
                ..Default::default()
            }),
        },
        policy,
    )
    .unwrap();
    (Arc::new(signer), ids)
}

/// One outbound to issue: an HTTP request (method, path, body) or a WS frame.
enum Issue {
    Http(Method, String, Option<String>, Box<dyn SignStep>),
    Ws(Box<dyn SignStep>),
}
struct Issuer(Vec<Issue>);
impl Venue for Issuer {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let mut socket = None;
        for issue in self.0.drain(..) {
            match issue {
                Issue::Http(method, path, body, step) => {
                    // A full URL issues to another host; a path issues to Coinbase.
                    let url = if path.starts_with("https://") {
                        path
                    } else {
                        format!("{ORIGIN}{path}")
                    };
                    let id = cx.request(method, url, vec![], body, ());
                    cx.sign_http(id, step);
                }
                Issue::Ws(step) => {
                    let socket = *socket.get_or_insert_with(|| {
                        cx.connect("wss://advanced-trade-ws-user.coinbase.com")
                    });
                    cx.send_signed(socket, String::new(), step);
                }
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

fn failures() -> Vec<String> {
    FAILURES.with(|failures| std::mem::take(&mut *failures.borrow_mut()))
}

fn run(issues: Vec<Issue>) -> Vec<Action> {
    failures();
    let (signer, _) = provider();
    let mut engine = Engine::new(Issuer(issues), Some(seeded_entropy(3))).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    prepared_actions(&mut engine, NOW)
}

fn https(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect()
}

/// Splits a released JWT into its decoded header/claims text, signing input
/// and raw signature.
fn decode(jwt: &str) -> (String, String, String, Vec<u8>) {
    let parts: Vec<_> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3, "{jwt}");
    let text = |part: &str| String::from_utf8(URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap();
    (
        text(parts[0]),
        text(parts[1]),
        format!("{}.{}", parts[0], parts[1]),
        URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
    )
}

/// The nonce is the legacy nanoid form: up to 21 chars of its alphabet.
fn nonce_of(header: &str) -> String {
    let nonce = header
        .split("\"nonce\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_owned();
    assert!(!nonce.is_empty() && nonce.len() <= 21, "{nonce}");
    assert!(nonce.bytes().all(|b| b.is_ascii_alphanumeric()), "{nonce}");
    nonce
}

fn verify(algorithm: JwtAlgorithm, input: &str, signature: &[u8]) -> bool {
    match algorithm {
        JwtAlgorithm::Es256 => {
            use p256::ecdsa::signature::Verifier;
            let key = p256::ecdsa::VerifyingKey::from(&p256::ecdsa::SigningKey::from(ec_key()));
            p256::ecdsa::Signature::from_slice(signature)
                .is_ok_and(|signature| key.verify(input.as_bytes(), &signature).is_ok())
        }
        JwtAlgorithm::EdDsa => {
            safe_cex_crypto::ed25519::verify(input.as_bytes(), &ED_PUBLIC, signature).unwrap()
        }
    }
}

const NBF: u64 = 1_700_000_123;

#[test]
fn rest_recipes_release_the_legacy_jwt_for_get_with_query_and_post() {
    let (_, ids) = provider();
    let trading = ids.primary.clone().unwrap();
    let read = ids.secondary.clone().unwrap();
    let get = "/api/v3/brokerage/orders/historical/batch?limit=100&order_status=OPEN";
    let post = "/api/v3/brokerage/orders";
    let body = r#"{"client_order_id":"c-1","product_id":"BTC-USD"}"#;
    let actions = run(vec![
        Issue::Http(
            Method::Get,
            get.into(),
            None,
            Box::new(sign::Http::new(
                read,
                READ_KEY,
                JwtAlgorithm::EdDsa,
                Method::Get,
                get,
            )),
        ),
        Issue::Http(
            Method::Post,
            post.into(),
            Some(body.into()),
            Box::new(sign::Http::new(
                trading,
                KEY,
                JwtAlgorithm::Es256,
                Method::Post,
                post,
            )),
        ),
    ]);
    assert_eq!(failures(), Vec::<String>::new());
    let released = https(&actions);
    assert_eq!(released.len(), 2);
    for (http, kid, algorithm, header_alg, iss_aud, uri) in [
        (
            released[0],
            READ_KEY,
            JwtAlgorithm::EdDsa,
            "EdDSA",
            r#""iss":"cdp","aud":["cdp_service"]"#,
            "GET api.coinbase.com/api/v3/brokerage/orders/historical/batch",
        ),
        (
            released[1],
            KEY,
            JwtAlgorithm::Es256,
            "ES256",
            r#""iss":"coinbase-cloud""#,
            "POST api.coinbase.com/api/v3/brokerage/orders",
        ),
    ] {
        assert_eq!(http.headers.len(), 2);
        assert_eq!(http.headers[1], ("Content-Type", "application/json".into()));
        let (name, value) = &http.headers[0];
        assert_eq!(*name, "Authorization");
        let jwt = value.strip_prefix("Bearer ").unwrap();
        let (header, claims, input, signature) = decode(jwt);
        let nonce = nonce_of(&header);
        // Independent expectation of the legacy claim contract.
        assert_eq!(
            header,
            format!(r#"{{"alg":"{header_alg}","typ":"JWT","kid":"{kid}","nonce":"{nonce}"}}"#)
        );
        assert_eq!(
            claims,
            format!(
                r#"{{{iss_aud},"nbf":{NBF},"exp":{},"sub":"{kid}","uri":"{uri}"}}"#,
                NBF + 120
            )
        );
        assert!(verify(algorithm, &input, &signature), "{kid}");
    }
    assert_eq!(released[1].body.as_deref(), Some(body));
}

#[test]
fn rest_claims_match_the_legacy_signer_apart_from_the_nonce() {
    let (_, ids) = provider();
    let path = "/api/v3/brokerage/key_permissions";
    let actions = run(vec![Issue::Http(
        Method::Get,
        path.into(),
        None,
        Box::new(sign::Http::new(
            ids.primary.unwrap(),
            KEY,
            JwtAlgorithm::Es256,
            Method::Get,
            path,
        )),
    )]);
    let released = https(&actions);
    let jwt = released[0].headers[0].1.strip_prefix("Bearer ").unwrap();
    let (header, claims, _, _) = decode(jwt);
    let legacy = sign::Credentials::new(KEY, &ec_pem()).unwrap();
    let legacy_headers = legacy
        .headers(Method::Get, path, NOW, |bytes| {
            bytes.fill(1);
            Ok(())
        })
        .unwrap();
    let legacy_jwt = legacy_headers[0].1.strip_prefix("Bearer ").unwrap();
    let (legacy_header, legacy_claims, _, _) = decode(legacy_jwt);
    assert_eq!(claims, legacy_claims);
    assert_eq!(
        header.replace(&nonce_of(&header), "N"),
        legacy_header.replace(&nonce_of(&legacy_header), "N")
    );
    assert_eq!(released[0].headers[1], legacy_headers[1]);
}

#[test]
fn ws_subscribe_recipes_frame_both_channels_without_a_uri_claim() {
    let (_, ids) = provider();
    let trading = ids.primary.unwrap();
    let actions = run(["user", "heartbeats"]
        .into_iter()
        .map(|channel| {
            Issue::Ws(Box::new(sign::WsSubscribe::new(
                trading.clone(),
                KEY,
                JwtAlgorithm::Es256,
                channel,
            )) as Box<dyn SignStep>)
        })
        .collect());
    assert_eq!(failures(), Vec::<String>::new());
    let frames: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(frames.len(), 2);
    for (frame, channel) in frames.iter().zip(["user", "heartbeats"]) {
        let prefix = format!(r#"{{"type":"subscribe","channel":"{channel}","jwt":""#);
        let jwt = frame
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix("\"}"))
            .unwrap_or_else(|| panic!("{frame}"));
        let (_, claims, input, signature) = decode(jwt);
        assert_eq!(
            claims,
            format!(
                r#"{{"iss":"coinbase-cloud","nbf":{NBF},"exp":{},"sub":"{KEY}"}}"#,
                NBF + 120
            )
        );
        assert!(verify(JwtAlgorithm::Es256, &input, &signature));
    }
}

#[test]
fn wrong_request_key_slot_or_algorithm_never_releases() {
    let (_, ids) = provider();
    let trading = ids.primary.unwrap();
    let read = ids.secondary.unwrap();
    let path = "/api/v3/brokerage/orders";
    let cases: Vec<(String, Box<dyn SignStep>)> = vec![
        // Different route or host than the request that would go out.
        (
            "route".into(),
            Box::new(sign::Http::new(
                trading.clone(),
                KEY,
                JwtAlgorithm::Es256,
                Method::Post,
                "/api/v3/brokerage/portfolios/move_funds",
            )),
        ),
        // The recipe names one key; the selected slot presents another.
        (
            "slot".into(),
            Box::new(sign::Http::new(
                read.clone(),
                KEY,
                JwtAlgorithm::EdDsa,
                Method::Post,
                path,
            )),
        ),
        // The algorithm does not match the provider's key type.
        (
            "algorithm".into(),
            Box::new(sign::Http::new(
                trading.clone(),
                KEY,
                JwtAlgorithm::EdDsa,
                Method::Post,
                path,
            )),
        ),
        // A forged identity whose credential ref is another key's.
        (
            "credential".into(),
            Box::new(sign::Http::new(
                Identity {
                    credential: KeyRef(read.credential.0.clone()),
                    ..trading.clone()
                },
                KEY,
                JwtAlgorithm::Es256,
                Method::Post,
                path,
            )),
        ),
    ];
    for (label, step) in cases {
        let actions = run(vec![Issue::Http(
            Method::Post,
            path.into(),
            Some("{}".into()),
            step,
        )]);
        assert!(https(&actions).is_empty(), "{label}");
        assert_eq!(failures().len(), 1, "{label}");
    }
    // Same path, another host: the recipe binds the exact Coinbase origin.
    let actions = run(vec![Issue::Http(
        Method::Post,
        format!("https://evil.test{path}"),
        Some("{}".into()),
        Box::new(sign::Http::new(
            trading,
            KEY,
            JwtAlgorithm::Es256,
            Method::Post,
            path,
        )),
    )]);
    assert!(https(&actions).is_empty(), "host");
    assert_eq!(failures().len(), 1, "host");
}

/// Never answers; the test delivers the signed result itself.
struct Pending;
impl Signer for Pending {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}

#[test]
/// Proves the receipt declares exp as its expiry, and that a result delivered
/// after it is not released (here the 5 s freshness/deadline gate refuses it
/// first, since freshness sits well inside the 120 s token).
fn receipt_declares_jwt_expiry_and_a_late_result_is_never_released() {
    let (_, ids) = provider();
    let path = "/api/v3/brokerage/orders";
    let mut engine = Engine::new(
        Issuer(vec![Issue::Http(
            Method::Post,
            path.into(),
            Some("{}".into()),
            Box::new(sign::Http::new(
                ids.primary.unwrap(),
                KEY,
                JwtAlgorithm::Es256,
                Method::Post,
                path,
            )),
        )]),
        Some(seeded_entropy(3)),
    )
    .unwrap();
    engine.set_signer(Some(Arc::new(Pending))).unwrap();
    engine.start(NOW);
    let (attempt, work) = prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("the provider is asked to sign");
    let expiry = work.receipt.expiry_ms.unwrap();
    assert_eq!(expiry, ((NBF + 120) * 1000) as f64);
    // A well-formed result arriving after exp must not release the old JWT.
    let mut values = vec![KEY.as_bytes().to_vec(), vec![0; 64]];
    values.truncate(work.operations.len());
    engine.handle(
        expiry + 1.0,
        Event::Signed {
            attempt,
            result: Ok(safe_cex_engine::signing::SignResult::new(
                values,
                work.receipt,
            )),
        },
    );
    assert!(https(&prepared_actions(&mut engine, expiry + 1.0)).is_empty());
}
