use super::*;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use safe_cex_engine::signing::{Identities, Provider};
use safe_cex_venues::{EngineOptions, hosted_signing, local_signer_from_capsule};

const KEY: &str = "organizations/fixture/apiKeys/cosigned-proof";
const NOW: f64 = 1_700_000_000_000.;

fn input(algorithm: &str, nonce: &str, uri: Option<&str>) -> String {
    let alg = if algorithm == "es256" {
        "ES256"
    } else {
        "EdDSA"
    };
    let header = format!(r#"{{"alg":"{alg}","typ":"JWT","kid":"{KEY}","nonce":"{nonce}"}}"#);
    let claims = format!(
        r#"{{"iss":"cdp","nbf":1700000000,"exp":1700000120,"sub":"{KEY}"{}}}"#,
        uri.map_or(String::new(), |uri| format!(",\"uri\":\"{uri}\""))
    );
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header),
        URL_SAFE_NO_PAD.encode(claims)
    )
}

fn local(venue: &str, algorithm: &str) -> (Provider, Identities) {
    // Synthetic scalar 7 repeated: minimal PKCS8/P-256, never account material.
    let secret = if algorithm == "es256" {
        let mut der = vec![
            0x30, 0x41, 0x02, 0x01, 0x00, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d,
            0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x04, 0x27,
            0x30, 0x25, 0x02, 0x01, 0x01, 0x04, 0x20,
        ];
        der.extend([7; 32]);
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
            STANDARD.encode(der)
        )
    } else {
        STANDARD.encode([7; 32])
    };
    local_signer_from_capsule(
        venue,
        json!({"primary":{"key":KEY,"secret":secret}}).to_string(),
        &mut serde_json::from_str::<EngineOptions>("{}").unwrap(),
    )
    .unwrap()
}

fn graph(venue: &str, ids: &Identities, input: &str) -> (SignRequest, hosted_signing::OutputKind) {
    hosted_signing::prepare(
        venue,
        ids,
        hosted_signing::Input::WsAuth {
            to_sign: input.into(),
            output_format: hosted_signing::OutputFormat::CoinbaseJws,
        },
    )
    .unwrap()
}

fn setup(venue: &str, algorithm: Option<&str>) -> (RemoteAccount, Value) {
    let link = SignerLink::new(11, Arc::new(|_| panic!("setup must not sign")));
    let mut options = json!({"key":KEY});
    if let Some(algorithm) = algorithm {
        options["coinbaseJwtAlgorithm"] = algorithm.into();
    }
    account(link, venue, options)
}

fn binding_and_capability(venue: &str, algorithm: &str, explicit: bool) {
    let (account, options) = setup(venue, explicit.then_some(algorithm));
    assert_eq!(
        options["coinbaseSigning"],
        json!({"keyName":KEY,"algorithm":algorithm}),
        "{venue}: opaque CDP names need explicit public JWT metadata"
    );
    let (_, signing, _) = refs(&options);
    let expected = if algorithm == "es256" {
        Algorithm::Es256
    } else {
        Algorithm::Ed25519
    };
    assert_eq!(
        account.provider.can_sign(&signing, expected),
        Capability::Supported
    );
    for wrong in [Algorithm::HmacSha256, Algorithm::Secp256k1Digest] {
        assert_eq!(
            account.provider.can_sign(&signing, wrong),
            Capability::Unsupported
        );
    }
    assert_eq!(
        account
            .provider
            .can_sign(&KeyRef("foreign".into()), expected),
        Capability::Unsupported
    );
    for field in ["key", "secret", "passphrase"] {
        assert!(options.get(field).is_none());
    }
}

macro_rules! venue_contract {
    ($name:ident, $venue:literal) => {
        mod $name {
            use super::*;

            #[test]
            fn default_es256_installs_public_jwt_binding() {
                binding_and_capability($venue, "es256", false);
            }

            #[test]
            fn explicit_eddsa_installs_public_jwt_binding() {
                binding_and_capability($venue, "edDsa", true);
            }

            #[test]
            fn never_advertises_hmac_for_a_cdp_key_name() {
                let (account, options) = setup($venue, Some("edDsa"));
                assert_eq!(
                    account.provider.can_sign(&refs(&options).1, Algorithm::HmacSha256),
                    Capability::Unsupported
                );
            }

            #[test]
            fn unknown_jwt_algorithm_refuses_setup() {
                let link = SignerLink::new(11, Arc::new(|_| panic!("invalid setup must not sign")));
                assert!(remote_account(
                    link, $venue, None,
                    &json!({"key":KEY,"coinbaseJwtAlgorithm":"hs256"}).to_string(),
                ).is_err());
            }

            #[tokio::test]
            async fn es256_round_trip_returns_only_the_signature() {
                round_trip($venue, "es256").await;
            }

            #[tokio::test]
            async fn eddsa_round_trip_returns_only_the_signature() {
                round_trip($venue, "edDsa").await;
            }
        }
    };
}

venue_contract!(coinbase, "coinbase");
venue_contract!(coinbasespot, "coinbasespot");

#[test]
fn coinbase_device_codec_already_signs_exact_server_claims_for_both_algorithms() {
    for venue in ["coinbase", "coinbasespot"] {
        for algorithm in ["es256", "edDsa"] {
            let (provider, ids) = local(venue, algorithm);
            let payload = input(
                algorithm,
                "server-nonce",
                Some("GET api.coinbase.com/api/v3/brokerage/accounts"),
            );
            let (work, kind) = graph(venue, &ids, &payload);
            let result = provider.sign_ready(&work).unwrap().unwrap();
            work.validate_result(&result).unwrap();
            let output = hosted_signing::finish(kind, &result).unwrap();
            assert_eq!(
                URL_SAFE_NO_PAD.decode(output.signature.unwrap()).unwrap(),
                result.values[1]
            );
            assert_eq!(result.values[1].len(), 64);
            let changed = input(
                algorithm,
                "different-server-nonce",
                Some("GET api.coinbase.com/api/v3/brokerage/accounts"),
            );
            let changed = provider
                .sign_ready(&graph(venue, &ids, &changed).0)
                .unwrap()
                .unwrap();
            assert_ne!(
                result.values[1], changed.values[1],
                "nonce must bind the signature"
            );
        }
    }
}

async fn round_trip(venue: &str, algorithm: &str) {
    let (sender, mut frames) = mpsc::unbounded_channel();
    let link = SignerLink::new(11, Arc::new(move |frame| sender.send(frame).is_ok()));
    let (account, options) = account(
        link.clone(),
        venue,
        json!({"key":KEY,"coinbaseJwtAlgorithm":algorithm}),
    );
    let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
    let (device, device_ids) = local(venue, algorithm);
    for uri in [
        Some("GET api.coinbase.com/api/v3/brokerage/accounts"),
        Some("POST drb.coinbase.com/api/v2/public/auth"),
        None,
    ] {
        let payload = input(algorithm, "server-nonce", uri);
        let work = graph(venue, &ids, &payload).0;
        assert_eq!(work.receipt.stamp_ms, Some(NOW));
        assert_eq!(work.receipt.expiry_ms, Some(NOW + 120_000.));
        let mut pending = account.provider.sign(work.clone());
        let frame = tokio::select! {
            result = &mut pending => panic!("{venue}/{algorithm}: no CDP device round trip: {result:?}"),
            frame = frames.recv() => frame.unwrap(),
        };
        let frame: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(frame["t"], "sign-req");
        let sent = &frame["d"];
        assert_eq!(sent["type"], "ws-auth");
        assert_eq!(sent["exchangeName"], venue);
        assert_eq!(sent["accountName"], "main");
        assert_eq!(
            sent["wsAuth"],
            json!({"toSign":payload,"outputFormat":"coinbase-jws"})
        );
        let (device_work, kind) = graph(venue, &device_ids, &payload);
        let signed = device.sign_ready(&device_work).unwrap().unwrap();
        let output = hosted_signing::finish(kind, &signed).unwrap();
        let reply = json!({"id":sent["id"],"wsAuthSignature":output.signature.unwrap()});
        assert!(
            !link.resolve(12, reply.clone()),
            "foreign socket must not consume the wait"
        );
        assert!(link.resolve(11, reply.clone()));
        assert!(!link.resolve(11, reply), "response is single-use");
        let result = pending.await.unwrap();
        assert_eq!(result.values, signed.values);
        assert_eq!(result.receipt, work.receipt);
        assert!(result.headers.is_empty());
        assert!(
            frames.try_recv().is_err(),
            "one request, no duplicate dispatch"
        );
    }
}

#[tokio::test]
async fn coinbase_closed_graphs_refuse_foreign_refs_claims_algorithms_and_receipts_before_io() {
    for venue in ["coinbase", "coinbasespot"] {
        for algorithm in ["es256", "edDsa"] {
            let (account, options) = setup(venue, Some(algorithm));
            let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
            let canonical = input(algorithm, "server-nonce", None);
            let base = graph(venue, &ids, &canonical).0;
            for mutation in [
                "credential",
                "signer",
                "hmac",
                "extra",
                "name",
                "claims-algorithm",
                "stamp",
                "expiry",
                "nonce",
                "missing-stamp",
                "transaction",
            ] {
                let mut work = base.clone();
                match mutation {
                    "credential" => {
                        work.operations[0] = Operation::Present {
                            key: KeyRef("foreign".into()),
                        }
                    }
                    "signer" => {
                        if let Operation::Sign { key, .. } = &mut work.operations[1] {
                            *key = KeyRef("foreign".into());
                        }
                    }
                    "hmac" => {
                        if let Operation::Sign { algorithm, .. } = &mut work.operations[1] {
                            *algorithm = Algorithm::HmacSha256;
                        }
                    }
                    "extra" => work.operations.push(work.operations[0].clone()),
                    "stamp" => work.receipt.stamp_ms = Some(NOW + 1000.),
                    "expiry" => work.receipt.expiry_ms = Some(NOW + 121_000.),
                    "nonce" => work.receipt.nonce = Some(1),
                    "missing-stamp" => work.receipt.stamp_ms = None,
                    _ => {
                        let changed = if mutation == "name" {
                            safe_cex_venue_coinbase::sign::signing_input_for(
                                if algorithm == "es256" {
                                    crate::CoinbaseJwtAlgorithm::Es256
                                } else {
                                    crate::CoinbaseJwtAlgorithm::EdDsa
                                },
                                "organizations/foreign/apiKeys/other",
                                "server-nonce",
                                1_700_000_000,
                                None,
                                true,
                            )
                            .unwrap()
                        } else if mutation == "claims-algorithm" {
                            input(
                                if algorithm == "es256" {
                                    "edDsa"
                                } else {
                                    "es256"
                                },
                                "server-nonce",
                                None,
                            )
                        } else {
                            "{\"transaction\":\"withdraw\"}".into()
                        };
                        if let Operation::Sign { payload, .. } = &mut work.operations[1] {
                            *payload = vec![PayloadPart::Bytes {
                                bytes: changed.into_bytes(),
                            }];
                        }
                    }
                }
                assert!(
                    account.provider.sign(work).await.is_err(),
                    "{venue}/{algorithm}/{mutation}"
                );
            }
        }
    }
}

#[tokio::test]
async fn coinbase_remote_refuses_whole_jwts_der_objects_and_noncanonical_signature_replies() {
    for venue in ["coinbase", "coinbasespot"] {
        for algorithm in ["es256", "edDsa"] {
            for bad in [
                Value::Null,
                json!({"signature":"client","headers":{}}),
                json!(format!(
                    "{}.client-signature",
                    input(algorithm, "server-nonce", None)
                )),
                json!(URL_SAFE_NO_PAD.encode([0u8; 70])),
                json!("A".repeat(85)),
                json!(format!("{}=", "A".repeat(86))),
                json!(format!("{}B", "A".repeat(85))),
            ] {
                let (sender, mut frames) = mpsc::unbounded_channel();
                let link = SignerLink::new(11, Arc::new(move |frame| sender.send(frame).is_ok()));
                let (account, options) = account(
                    link.clone(),
                    venue,
                    json!({"key":KEY,"coinbaseJwtAlgorithm":algorithm}),
                );
                let ids = serde_json::from_value(options["signing"].clone()).unwrap();
                let mut pending = account
                    .provider
                    .sign(graph(venue, &ids, &input(algorithm, "server-nonce", None)).0);
                let frame = tokio::select! {
                    result = &mut pending => panic!("valid graph must reach device: {result:?}"),
                    frame = frames.recv() => frame.unwrap(),
                };
                let frame: Value = serde_json::from_str(&frame).unwrap();
                assert!(link.resolve(11, json!({"id":frame["d"]["id"],"wsAuthSignature":bad})));
                assert!(pending.await.is_err(), "{venue}/{algorithm}");
                assert!(frames.try_recv().is_err());
            }
        }
    }
}
