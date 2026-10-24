//! Original native-builder custody assertions; no signer recipe or production cutover.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    Algorithm, Capability, Context as SignContext, Identities, Identity, KeyRef, Operation,
    PayloadPart, SignError, SignFuture, SignReceipt, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Command, Engine, HttpRequest, Method, ReadCredentials, Venue};
#[cfg(feature = "spot")]
use safe_cex_venue_kucoin::spot::KucoinSpot;
use safe_cex_venue_kucoin::{Kucoin, Options};
use std::sync::{Arc, Mutex};

const NOW: f64 = 1234.9;
struct RecordingSigner {
    inner: LocalSigner,
    requests: Arc<Mutex<Vec<SignRequest>>>,
}
impl Signer for RecordingSigner {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.sign(request)
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.sign_ready(request)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
struct Harness<V: Venue> {
    engine: Engine<V>,
    requests: Arc<Mutex<Vec<SignRequest>>>,
    identities: Identities,
    provider: Arc<RecordingSigner>,
}
fn options() -> Options {
    Options {
        key: Some("fixture-primary-key".into()),
        secret: Some("fixture-primary-secret".into()),
        passphrase: Some("fixture-primary-passphrase".into()),
        read_credentials: Some(ReadCredentials {
            key: "fixture-read-key".into(),
            secret: "fixture-read-secret".into(),
            passphrase: Some("fixture-read-passphrase".into()),
        }),
        ..Options::default()
    }
}
fn fixture<V: Venue>(venue: V) -> Harness<V> {
    let (inner, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-primary-key".into()),
            secret: Secret::new("fixture-primary-secret".into()),
            passphrase: Some(Secret::new("fixture-primary-passphrase".into())),
            ..Default::default()
        }),
        secondary: Some(LocalIdentity {
            key: Secret::new("fixture-read-key".into()),
            secret: Secret::new("fixture-read-secret".into()),
            passphrase: Some(Secret::new("fixture-read-passphrase".into())),
            ..Default::default()
        }),
    })
    .unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    let provider = Arc::new(RecordingSigner {
        inner,
        requests: requests.clone(),
    });
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.set_identities(identities.clone()).unwrap();
    Harness {
        engine,
        requests,
        identities,
        provider,
    }
}
fn http(actions: &[Action]) -> &HttpRequest {
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "one native request: {actions:?}");
    requests[0]
}
fn selected_batch<V: Venue>(h: &Harness<V>, identity: &Identity) {
    let requests = h.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "native signing must pass through the injected provider"
    );
    let request = &requests[0];
    assert_eq!(request.receipt.stamp_ms, Some(1234.));
    assert_eq!(
        request.operations.len(),
        3,
        "one key presentation and two atomic HMACs"
    );
    assert!(
        matches!(&request.operations[0], Operation::Present { key } if key == &identity.credential)
    );
    for operation in &request.operations[1..] {
        assert!(
            matches!(operation, Operation::Sign { algorithm: Algorithm::HmacSha256, key, .. }
            if key == &identity.signing)
        );
    }
}
#[test]
fn original_futures_authenticated_catalogue_uses_primary_atomic_provider() {
    let mut h = fixture(Kucoin::new(options()));
    h.engine.start(NOW);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    let request = http(&actions);
    assert_eq!(request.method, Method::Get);
    assert!(request.url.ends_with("/api/v1/contracts/active"));
    selected_batch(&h, h.identities.primary.as_ref().unwrap());
}
#[test]
fn original_futures_identity_uses_secondary_atomic_provider() {
    let mut h = fixture(Kucoin::new(options()));
    h.engine
        .handle_command(NOW, Command::FetchAccount { command: 71 }, Some(120_000))
        .unwrap();
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    assert!(http(&actions).url.ends_with("/api/v1/user-info"));
    selected_batch(&h, h.identities.secondary.as_ref().unwrap());
}
#[cfg(feature = "spot")]
#[test]
fn original_spot_identity_uses_secondary_atomic_provider() {
    let mut h = fixture(KucoinSpot::new(options()));
    h.engine
        .handle_command(NOW, Command::FetchAccount { command: 71 }, Some(120_000))
        .unwrap();
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    assert!(http(&actions).url.ends_with("/api/v2/user-info"));
    selected_batch(&h, h.identities.secondary.as_ref().unwrap());
}
#[test]
fn native_futures_ordered_wire_matches_independent_fixed_stamp_vectors() {
    let mut h = fixture(Kucoin::new(options()));
    h.engine.start(NOW);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    let request = http(&actions);
    // Independently computed with Python's hmac/hashlib and standard padded base64.
    assert_eq!(
        request.headers,
        vec![
            ("KC-API-KEY", "fixture-primary-key".into()),
            ("KC-API-TIMESTAMP", "1234".into()),
            ("KC-API-KEY-VERSION", "2".into()),
            (
                "KC-API-SIGN",
                "qPzTd/t+AOs5TVn+1qDaP7eWIbi/DZKLCA79jXFYmbM=".into()
            ),
            (
                "KC-API-PASSPHRASE",
                "XY2N6aArz59rfZ2rp5NyjVGaK5OMltv3hIFwjskG4L4=".into()
            ),
            ("Content-Type", "application/json".into()),
            ("X-CHANNEL-API-CODE", "3zexp".into()),
        ]
    );
}
#[cfg(feature = "spot")]
#[test]
fn native_spot_public_catalogue_stays_unsigned_with_configured_read_key() {
    let mut h = fixture(KucoinSpot::new(options()));
    h.engine.start(NOW);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    assert_eq!(
        http(&actions).headers,
        vec![
            ("X-CHANNEL-API-CODE", "3zexp".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
    assert!(h.requests.lock().unwrap().is_empty());
}
#[test]
fn native_keyless_public_futures_catalogue_stays_unsigned() {
    let mut h = fixture(Kucoin::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    h.engine.start(NOW);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    assert_eq!(
        http(&actions).headers,
        vec![
            ("X-CHANNEL-API-CODE", "3zexp".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
    assert!(h.requests.lock().unwrap().is_empty());
}

#[test]
fn recording_provider_working_control_matches_both_independent_mac_vectors() {
    let h = fixture(Kucoin::new(options()));
    let identity = h.identities.primary.as_ref().unwrap();
    let mut request = safe_cex_engine::signing::hmac_sha256_work(
        identity,
        vec![PayloadPart::Bytes {
            bytes: b"1234GET/api/v1/contracts/active".to_vec(),
        }],
        SignContext {
            venue: "kucoin".into(),
            method: "GET".into(),
            host: "api-futures.kucoin.com".into(),
            path: "/api/v1/contracts/active".into(),
        },
        SignReceipt {
            stamp_ms: Some(1234.),
            expiry_ms: None,
        },
    );
    request.operations.push(Operation::Sign {
        algorithm: Algorithm::HmacSha256,
        key: identity.signing.clone(),
        payload: vec![PayloadPart::Credential {
            key: identity.passphrase.clone().unwrap(),
        }],
    });
    let result = h.provider.sign_ready(&request).unwrap().unwrap();
    assert_eq!(result.values.len(), 3);
    assert_eq!(result.values[0], b"fixture-primary-key");
    assert_eq!(
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Base64),
        "qPzTd/t+AOs5TVn+1qDaP7eWIbi/DZKLCA79jXFYmbM="
    );
    assert_eq!(
        safe_cex_crypto::encode(&result.values[2], safe_cex_crypto::Encoding::Base64),
        "XY2N6aArz59rfZ2rp5NyjVGaK5OMltv3hIFwjskG4L4="
    );
    assert_eq!(h.requests.lock().unwrap().len(), 1);
}

fn broker_options(verify: Option<bool>) -> Options {
    let mut options = options();
    options.broker = Some(safe_cex_venue_kucoin::sign::BrokerOptions {
        partner: " fixture-partner \t".into(),
        broker_key: " fixture-broker-secret \t".into(),
        broker_name: " fixture-name \t".into(),
        verify,
    });
    options
}

#[test]
fn original_broker_catalogue_uses_one_atomic_provider_batch() {
    let mut h = fixture(Kucoin::new(broker_options(None)));
    h.engine.start(NOW);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    assert_eq!(http(&actions).headers.len(), 11);
    let requests = h.requests.lock().unwrap();
    assert_eq!(requests.len(), 1, "broker signing must enter the provider");
    assert_eq!(requests[0].receipt.stamp_ms, Some(1234.));
    assert_eq!(requests[0].operations.len(), 4);
}

#[test]
fn native_broker_headers_preserve_order_trim_and_boolean_verify() {
    for (verify, expected) in [(None, "true"), (Some(false), "false")] {
        let mut h = fixture(Kucoin::new(broker_options(verify)));
        h.engine.start(NOW);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
        let request = http(&actions);
        assert_eq!(request.headers[1], ("KC-API-TIMESTAMP", "1234".into()));
        assert_eq!(request.headers.len(), 11);
        assert_eq!(
            &request.headers[7..],
            &[
                ("KC-API-PARTNER", "fixture-partner".into()),
                (
                    "KC-API-PARTNER-SIGN",
                    "QZajsbPrEIWDNZvcc2gaC6WQ0dVWLovAICOP4Xu9iEE=".into()
                ),
                ("KC-BROKER-NAME", "fixture-name".into()),
                ("KC-API-PARTNER-VERIFY", expected.into()),
            ]
        );
    }
}

#[test]
fn native_partial_broker_triplets_omit_all_four_headers() {
    for missing in 0..3 {
        let mut options = broker_options(None);
        let broker = options.broker.as_mut().unwrap();
        match missing {
            0 => broker.partner = " \t ".into(),
            1 => broker.broker_key = " \t ".into(),
            _ => broker.broker_name = " \t ".into(),
        }
        let mut h = fixture(Kucoin::new(options));
        h.engine.start(NOW);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
        assert_eq!(http(&actions).headers.len(), 7);
    }
}

#[test]
fn native_secondary_identity_broker_mac_binds_the_read_api_key() {
    let mut h = fixture(Kucoin::new(broker_options(None)));
    h.engine
        .handle_command(NOW, Command::FetchAccount { command: 92 }, Some(120_000))
        .unwrap();
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, NOW);
    let request = http(&actions);
    assert_eq!(
        request.headers[0],
        ("KC-API-KEY", "fixture-read-key".into())
    );
    assert_eq!(request.headers[1], ("KC-API-TIMESTAMP", "1234".into()));
    assert_eq!(
        request.headers[8],
        (
            "KC-API-PARTNER-SIGN",
            "E9VO/x0oG8e8e6XpbCv03ouisGZKFD/WeWtGaH10zGI=".into()
        )
    );
}
