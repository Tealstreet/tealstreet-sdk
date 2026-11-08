use super::*;
use crate::provider::{Inner, Owned};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{ExtraKey, Identities, Identity, Signer};
use safe_cex_venues::hosted_signing::{self, Input};

const PAYLOAD: &str = "1700000000123POST/api/v1/orders{\"size\":1,\"memo\":\"é/+%\"}";

fn identities() -> Identities {
    Identities {
        primary: Some(
            Identity::new(KeyRef("public".into()), Some(KeyRef("signing".into())))
                .with_passphrase(Some(KeyRef("passphrase".into()))),
        ),
        secondary: None,
    }
}

fn broker_identities() -> Identities {
    let mut ids = identities();
    ids.primary
        .as_mut()
        .unwrap()
        .extra
        .insert(ExtraKey::KucoinBroker, KeyRef("broker".into()));
    ids
}

fn local(ids: &Identities) -> LocalSigner {
    let identity = ids.primary.as_ref().unwrap();
    let mut signer = LocalSigner::new();
    signer.insert(identity.credential.clone(), b"fixture-key".to_vec());
    signer.insert(
        identity.signing_ref().unwrap().clone(),
        b"fixture-secret".to_vec(),
    );
    signer.insert(
        identity.passphrase.as_ref().unwrap().clone(),
        b"fixture-passphrase".to_vec(),
    );
    if let Some(broker) = identity.extra.get(&ExtraKey::KucoinBroker) {
        signer.insert(broker.clone(), b"fixture-broker".to_vec());
    }
    signer
}

fn graph(venue: &str, ids: &Identities) -> SignRequest {
    graph_with_broker(venue, ids, false)
}

fn graph_with_broker(venue: &str, ids: &Identities, broker: bool) -> SignRequest {
    let mut descriptor = json!({ "payload": PAYLOAD });
    if broker {
        descriptor["brokerPrefix"] = "1700000000123partner".into();
    }
    hosted_signing::prepare(
        venue,
        ids,
        Input::Http {
            payload: descriptor.to_string(),
            stamp_ms: 1_700_000_000_123.,
            revision: "0".into(),
        },
    )
    .unwrap()
    .0
}

fn kucoin_device(venue: &'static str) -> (Arc<SignerLink>, Arc<Mutex<Vec<Value>>>) {
    let ids = broker_identities();
    let signer = local(&ids);
    device(Box::new(move |request| {
        let (work, kind) = hosted_signing::prepare(
            venue,
            &ids,
            Input::WsAuth {
                to_sign: request["wsAuth"]["toSign"].as_str().unwrap().into(),
                output_format: serde_json::from_value(request["wsAuth"]["outputFormat"].clone())
                    .unwrap(),
            },
        )
        .unwrap();
        let result = signer.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        json!({ "wsAuthSignature": hosted_signing::finish(kind, &result).unwrap().signature.unwrap() })
    }))
}

#[test]
fn kucoin_setup_installs_device_passphrase_identity_without_header() {
    for venue in ["kucoin", "kucoinspot"] {
        for options in [
            json!({ "key": "fixture-key" }),
            json!({ "key": "fixture-key", "passphrase": "fixture-passphrase" }),
        ] {
            let link = SignerLink::new(1, Arc::new(|_| true));
            let (account, options) = account(link, venue, options);
            let (_, _, passphrase) = refs(&options);
            assert!(
                passphrase.is_some(),
                "{venue}: missing device passphrase identity"
            );
            assert!(!account.options_json.contains("fixture-passphrase"));
            assert!(account.inner.primary.passphrase_value.is_none());
            let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
            assert_eq!(graph(venue, &ids).operations.len(), 3);
        }
    }
}

#[tokio::test]
async fn kucoin_provider_device_batch_matches_local_signer_with_independent_passphrase_ref() {
    for venue in ["kucoin", "kucoinspot"] {
        let (link, seen) = kucoin_device(venue);
        let ids = identities();
        let id = ids.primary.as_ref().unwrap();
        let provider = RemoteProvider {
            inner: Arc::new(Inner::new(
                venue.into(),
                policy::policy(venue).unwrap(),
                Some("main".into()),
                link,
                Owned {
                    credential: id.credential.clone(),
                    signing: id.signing_ref().unwrap().clone(),
                    public: b"fixture-key".to_vec(),
                    passphrase: id.passphrase.clone(),
                    passphrase_value: None,
                    broker: None,
                },
            )),
        };
        let work = graph(venue, &ids);
        work.validate().unwrap();
        let control = local(&ids).sign_now(&work).unwrap();
        work.validate_result(&control).unwrap();
        let result = provider.sign(work.clone()).await.unwrap();
        work.validate_result(&result).unwrap();
        assert_eq!(result.values, control.values);
        assert_eq!(result.receipt, control.receipt);
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["exchangeName"], venue);
        assert_eq!(requests[0]["accountName"], "main");
        assert_eq!(requests[0]["wsAuth"]["outputFormat"], "kucoin-batch");
        let descriptor: Value =
            serde_json::from_str(requests[0]["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
        assert_eq!(descriptor, json!({ "slot": "primary", "payload": PAYLOAD }));
        let wire = requests[0].to_string();
        for material in [
            "fixture-secret",
            "fixture-passphrase",
            "delegated/",
            "passphrase",
        ] {
            assert!(!wire.contains(material));
        }
    }
}

#[tokio::test]
async fn kucoin_account_device_round_trip_matches_local_with_optional_broker() {
    for venue in ["kucoin", "kucoinspot"] {
        for broker in [false, true] {
            let (link, seen) = kucoin_device(venue);
            let mut input = json!({ "key": "fixture-key" });
            if broker {
                input["extra"] = json!({ "kucoinBroker": {
                    "partner": "partner", "brokerName": "broker-name", "brokerKey": "fixture-broker"
                } });
            }
            let (account, options) = account(link, venue, input);
            assert!(!account.options_json.contains("fixture-broker"));
            assert!(options.pointer("/extra/kucoinBroker/brokerKey").is_none());
            let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
            let work = graph_with_broker(venue, &ids, broker);
            work.validate().unwrap();
            let control = local(&ids).sign_now(&work).unwrap();
            work.validate_result(&control).unwrap();
            let result = account.provider.sign(work.clone()).await.unwrap();
            work.validate_result(&result).unwrap();
            assert_eq!(result.values, control.values);
            assert_eq!(result.receipt, control.receipt);
            assert!(result.headers.is_empty());
            let requests = seen.lock().unwrap();
            assert_eq!(requests.len(), 1);
            let descriptor: Value =
                serde_json::from_str(requests[0]["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
            let mut expected = json!({ "slot": "primary", "payload": PAYLOAD });
            if broker {
                expected["brokerPrefix"] = "1700000000123partner".into();
            }
            assert_eq!(descriptor, expected);
            let wire = requests[0].to_string();
            for material in [
                "fixture-secret",
                "fixture-passphrase",
                "fixture-broker",
                "delegated/",
            ] {
                assert!(!wire.contains(material));
            }
            if let Some(broker) = ids
                .primary
                .as_ref()
                .unwrap()
                .extra
                .get(&ExtraKey::KucoinBroker)
            {
                assert_eq!(
                    account.provider.can_sign(broker, Algorithm::HmacSha256),
                    Capability::Supported
                );
                assert_eq!(
                    account
                        .provider
                        .can_sign(broker, Algorithm::Secp256k1Digest),
                    Capability::Unsupported
                );
            }
        }
    }
}

#[tokio::test]
async fn kucoin_refuses_foreign_partial_or_raw_passphrase_graphs_before_device_io() {
    for venue in ["kucoin", "kucoinspot"] {
        let (link, seen) = device(Box::new(
            |_| json!({ "error": "unexpected device request" }),
        ));
        let (account, options) = account(link, venue, json!({ "key": "fixture-key" }));
        let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
        let valid = graph(venue, &ids);
        let mut bad = Vec::new();
        let mut partial = valid.clone();
        partial.operations.pop();
        bad.push(partial);
        for replacement in [
            vec![bytes("fixture-passphrase")],
            vec![PayloadPart::Credential {
                key: KeyRef("foreign-passphrase".into()),
            }],
            vec![PayloadPart::Credential {
                key: ids.primary.as_ref().unwrap().credential.clone(),
            }],
        ] {
            let mut work = valid.clone();
            if let Operation::Sign { payload, .. } = &mut work.operations[2] {
                *payload = replacement;
            }
            bad.push(work);
        }
        for index in [0, 1, 2] {
            let mut work = valid.clone();
            match &mut work.operations[index] {
                Operation::Present { key } | Operation::Sign { key, .. } => {
                    *key = KeyRef("foreign-key".into())
                }
                _ => unreachable!(),
            }
            bad.push(work);
        }
        let mut raw = valid.clone();
        raw.operations[2] = Operation::Present {
            key: ids
                .primary
                .as_ref()
                .unwrap()
                .passphrase
                .as_ref()
                .unwrap()
                .clone(),
        };
        bad.push(raw);
        let mut extra = valid.clone();
        extra.operations.push(valid.operations[1].clone());
        bad.push(extra);
        for payload in [
            Vec::new(),
            vec![bytes("")],
            vec![bytes("a"), bytes("b")],
            vec![PayloadPart::Bytes { bytes: vec![0xff] }],
            vec![bytes(&"x".repeat(1_048_577))],
            vec![bytes(&"\n".repeat(524_288))],
        ] {
            let mut work = valid.clone();
            if let Operation::Sign {
                payload: target, ..
            } = &mut work.operations[1]
            {
                *target = payload;
            }
            bad.push(work);
        }
        for work in bad {
            assert_eq!(
                account.provider.sign(work).await.map(|_| ()),
                Err(SignError::Refused)
            );
        }
        assert!(seen.lock().unwrap().is_empty());
        let credential = ids.primary.as_ref().unwrap().credential.clone();
        let result = account
            .provider
            .sign(request(
                vec![Operation::Present { key: credential }],
                Default::default(),
            ))
            .await
            .unwrap();
        assert_eq!(result.values, vec![b"fixture-key".to_vec()]);
        assert!(seen.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn kucoin_rejects_malformed_or_wrong_account_device_batches_atomically() {
    let good = base64::engine::general_purpose::STANDARD.encode(MAC);
    let short = base64::engine::general_purpose::STANDARD.encode([7; 31]);
    let mut noncanonical = good.clone();
    noncanonical.replace_range(42..43, "d");
    let answers = [
        json!({ "wsAuthSignature": "not-json" }),
        json!({ "wsAuthSignature": json!({ "credential": "foreign", "macs": [good, good] }).to_string() }),
        json!({ "wsAuthSignature": json!({ "credential": "fixture-key", "macs": [good] }).to_string() }),
        json!({ "wsAuthSignature": json!({ "credential": "fixture-key", "macs": [good, good, good] }).to_string() }),
        json!({ "wsAuthSignature": json!({ "credential": "fixture-key", "macs": [good, short] }).to_string() }),
        json!({ "wsAuthSignature": json!({ "credential": "fixture-key", "macs": [good, noncanonical] }).to_string() }),
        json!({ "wsAuthSignature": json!({ "credential": "fixture-key", "macs": [good, good], "extra": true }).to_string() }),
        json!({ "error": "fixture-secret fixture-passphrase fixture-broker" }),
    ];
    for venue in ["kucoin", "kucoinspot"] {
        for answer in &answers {
            let answer = answer.clone();
            let (link, seen) = device(Box::new(move |_| answer.clone()));
            let (account, options) = account(link, venue, json!({ "key": "fixture-key" }));
            let ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
            let error = account.provider.sign(graph(venue, &ids)).await.unwrap_err();
            assert_eq!(error, SignError::Refused);
            assert!(!format!("{error:?}").contains("fixture-"));
            assert_eq!(seen.lock().unwrap().len(), 1);
        }
    }
}
