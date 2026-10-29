use std::sync::{Arc, Mutex};

use base64::Engine as _;
use safe_cex_engine::signing::{
    Algorithm, Capability, Encoding, KeyRef, Operation, PayloadPart, SignError, SignReceipt,
    SignRequest,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use super::*;

type Answer = Box<dyn Fn(&Value) -> Value + Send + Sync>;

/// A device on the other end of the link: answers every sign-req with `answer`.
fn device(answer: Answer) -> (Arc<SignerLink>, Arc<Mutex<Vec<Value>>>) {
    let (sender, mut frames) = mpsc::unbounded_channel::<String>();
    let link = SignerLink::new(1, Arc::new(move |frame| sender.send(frame).is_ok()));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (weak, record) = (Arc::downgrade(&link), seen.clone());
    tokio::spawn(async move {
        while let Some(frame) = frames.recv().await {
            assert!(frame.starts_with(r#"{"t":"sign-req","d":"#), "{frame}");
            let request = serde_json::from_str::<Value>(&frame).unwrap()["d"].clone();
            record.lock().unwrap().push(request.clone());
            let mut response = answer(&request);
            response["id"] = request["id"].clone();
            if let Some(link) = weak.upgrade() {
                link.resolve(1, response);
            }
        }
    });
    (link, seen)
}

fn account(link: Arc<SignerLink>, venue: &str, options: Value) -> (RemoteAccount, Value) {
    let account = remote_account(link, venue, Some("main".into()), &options.to_string()).unwrap();
    let options: Value = serde_json::from_str(&account.options_json).unwrap();
    (account, options)
}

fn refs(options: &Value) -> (KeyRef, KeyRef, Option<KeyRef>) {
    let primary = &options["signing"]["primary"];
    let get = |field: &str| primary[field].as_str().map(|text| KeyRef(text.into()));
    (
        get("credential").unwrap(),
        get("signing").unwrap(),
        get("passphrase"),
    )
}

fn bytes(text: &str) -> PayloadPart {
    PayloadPart::Bytes {
        bytes: text.as_bytes().to_vec(),
    }
}

fn request(operations: Vec<Operation>, receipt: SignReceipt) -> SignRequest {
    SignRequest {
        receipt,
        operations,
        context: Default::default(),
    }
}

const MAC: [u8; 32] = [7; 32];

#[tokio::test]
async fn okx_login_sends_the_seconds_stamp_and_presents_the_passphrase() {
    let (link, seen) = device(Box::new(
        |_| json!({ "wsAuthSignature": base64::engine::general_purpose::STANDARD.encode(MAC) }),
    ));
    let (account, options) = account(
        link,
        "okx",
        json!({ "key": "pub", "secret": "", "passphrase": "pp", "testnet": false }),
    );
    assert!(
        options.get("key").is_none()
            && options.get("passphrase").is_none()
            && options.get("secret").is_none()
    );
    assert_eq!(options["remoteSigning"], true);
    assert_eq!(
        options["publicMode"],
        json!({ "requested": null, "remoteSigner": true, "payloadSigner": false, "apiKeyIndexPresent": false })
    );
    let (credential, signing, passphrase) = refs(&options);
    let passphrase = passphrase.unwrap();
    let work = request(
        vec![
            Operation::Present { key: credential },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload: vec![
                    bytes("1700000000"),
                    bytes("GET"),
                    bytes("/users/self/verify"),
                ],
            },
            Operation::Present { key: passphrase },
        ],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_000.),
            ..Default::default()
        },
    );
    let result = account.provider.sign(work).await.unwrap();
    assert_eq!(
        result.values,
        vec![b"pub".to_vec(), MAC.to_vec(), b"pp".to_vec()]
    );
    let sent = seen.lock().unwrap()[0].clone();
    assert_eq!(sent["type"], "ws-auth");
    assert_eq!(sent["exchangeName"], "okx");
    assert_eq!(sent["accountName"], "main");
    assert_eq!(
        sent["wsAuth"],
        json!({ "toSign": "1700000000GET/users/self/verify", "outputFormat": "base64", "extraData": { "timestamp": 1_700_000_000u64 } })
    );
    assert_eq!(
        account.provider.can_sign(&signing, Algorithm::HmacSha256),
        Capability::Supported
    );
    assert_eq!(
        account
            .provider
            .can_sign(&signing, Algorithm::Secp256k1Digest),
        Capability::Unsupported
    );
}

#[tokio::test]
async fn an_http_capture_has_no_metadata_and_a_mismatched_stamp_refuses() {
    let (link, seen) = device(Box::new(
        |_| json!({ "wsAuthSignature": base64::engine::general_purpose::STANDARD.encode(MAC) }),
    ));
    let (account, options) = account(link, "bitget", json!({ "key": "pub", "passphrase": "pp" }));
    let (_, signing, _) = refs(&options);
    let sign = |payload| Operation::Sign {
        algorithm: Algorithm::HmacSha256,
        key: signing.clone(),
        payload,
    };
    let http = request(
        vec![sign(vec![bytes("1700000000123POST/api/v2/order{}")])],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_123.),
            ..Default::default()
        },
    );
    account.provider.sign(http).await.unwrap();
    assert_eq!(
        seen.lock().unwrap()[0]["wsAuth"],
        json!({ "toSign": "1700000000123POST/api/v2/order{}", "outputFormat": "base64" })
    );
    let wrong = request(
        vec![sign(vec![
            bytes("1700000001"),
            bytes("GET"),
            bytes("/user/verify"),
        ])],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_000.),
            ..Default::default()
        },
    );
    assert_eq!(
        account.provider.sign(wrong).await.map(|_| ()),
        Err(SignError::Refused)
    );
}

#[tokio::test]
async fn blofin_answers_in_hex_base64_and_a_passphrase_alone_refuses() {
    let hex: String = MAC.iter().map(|byte| format!("{byte:02x}")).collect();
    let (link, _) = device(Box::new(
        move |_| json!({ "wsAuthSignature": base64::engine::general_purpose::STANDARD.encode(&hex) }),
    ));
    let (account, options) = account(link, "blofin", json!({ "key": "pub", "passphrase": "pp" }));
    let (_, signing, passphrase) = refs(&options);
    let sign = Operation::Sign {
        algorithm: Algorithm::HmacSha256,
        key: signing,
        payload: vec![bytes("x")],
    };
    assert_eq!(
        account
            .provider
            .sign(request(vec![sign], Default::default()))
            .await
            .unwrap()
            .values,
        vec![MAC.to_vec()]
    );
    let alone = request(
        vec![Operation::Present {
            key: passphrase.unwrap(),
        }],
        Default::default(),
    );
    assert_eq!(
        account.provider.sign(alone).await.map(|_| ()),
        Err(SignError::Refused)
    );
}

#[tokio::test]
async fn binance_spot_user_data_uses_the_declared_id_and_metadata() {
    let (link, seen) = device(Box::new(|_| json!({ "wsAuthSignature": "07".repeat(32) })));
    let (account, options) = account(link, "binancespot", json!({ "key": "AK" }));
    let (credential, signing, _) = refs(&options);
    let work = request(
        vec![Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: signing,
            payload: vec![
                bytes("apiKey="),
                PayloadPart::Credential { key: credential },
                bytes("&recvWindow=5000&timestamp=1700000000000"),
            ],
        }],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_000.),
            ..Default::default()
        },
    );
    account.provider.sign(work).await.unwrap();
    let sent = seen.lock().unwrap()[0].clone();
    assert_eq!(sent["id"], "tealstreet-user-data");
    assert_eq!(sent["exchangeName"], "binancespot");
    assert_eq!(
        sent["wsAuth"]["extraData"],
        json!({ "apiKey": "AK", "recvWindow": 5000, "timestamp": 1_700_000_000_000u64 })
    );
}

#[tokio::test]
async fn bybit_ws_auth_carries_the_expiry_and_answers_are_bounded() {
    let (link, seen) = device(Box::new(|request| {
        // An uppercase or short answer is refused, never coerced.
        if request["wsAuth"]["toSign"] == "GET/realtime1" {
            json!({ "wsAuthSignature": "07" })
        } else {
            json!({ "wsAuthSignature": "07".repeat(32) })
        }
    }));
    let (account, options) = account(link, "bybit", json!({ "key": "AK" }));
    let (_, signing, _) = refs(&options);
    let sign = |text: &str| {
        request(
            vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload: vec![bytes(text)],
            }],
            SignReceipt {
                expiry_ms: Some(1_700_000_000_000.),
                ..Default::default()
            },
        )
    };
    account
        .provider
        .sign(sign("GET/realtime1700000000000"))
        .await
        .unwrap();
    assert_eq!(
        seen.lock().unwrap()[0]["wsAuth"]["extraData"],
        json!({ "expires": 1_700_000_000_000u64 })
    );
    assert_eq!(seen.lock().unwrap()[0]["exchangeName"], "bybit");
    assert_eq!(
        account
            .provider
            .sign(sign("GET/realtime1"))
            .await
            .map(|_| ()),
        Err(SignError::Refused)
    );
}

#[tokio::test]
async fn bitunix_double_sha256_sends_the_inner_preimage_and_presents_the_key() {
    let (link, seen) = device(Box::new(|_| json!({ "wsAuthSignature": "07".repeat(32) })));
    let (account, options) = account(link, "bitunix", json!({ "key": "  AK  " }));
    let (credential, signing, _) = refs(&options);
    let work = request(
        vec![
            Operation::Sha256 {
                payload: vec![
                    bytes("nonce1"),
                    PayloadPart::Credential {
                        key: credential.clone(),
                    },
                    bytes("body"),
                ],
            },
            Operation::Sign {
                algorithm: Algorithm::Sha256SecretSuffix,
                key: signing,
                payload: vec![PayloadPart::Result {
                    index: 0,
                    encoding: Encoding::Hex,
                }],
            },
            Operation::Present { key: credential },
        ],
        Default::default(),
    );
    let result = account.provider.sign(work).await.unwrap();
    assert_eq!(
        result.values,
        vec![
            Sha256::digest(b"nonce1AKbody").to_vec(),
            MAC.to_vec(),
            b"AK".to_vec()
        ]
    );
    assert_eq!(
        seen.lock().unwrap()[0]["wsAuth"],
        json!({ "toSign": "nonce1AKbody", "outputFormat": "bitunix-double-sha256" })
    );
}

fn poll_setup(
    setup: &mut Box<dyn safe_cex_engine::signing::OwnedSetupExecution>,
) -> impl std::future::Future<Output = Result<safe_cex_engine::signing::SignResult, SignError>> + '_
{
    std::future::poll_fn(move |cx| setup.poll_result(cx))
}

#[tokio::test]
async fn hyperliquid_binds_the_agent_then_signs_statements_only() {
    let address = format!("0x{}", "ab".repeat(20));
    let signature = format!("0x{}1b", "cd".repeat(64));
    let (answer_address, answer_signature) = (address.clone(), signature.clone());
    let (link, seen) = device(Box::new(
        move |request| match request["wsAuth"]["outputFormat"].as_str().unwrap() {
            "hyperliquid-address" => json!({ "wsAuthSignature": answer_address }),
            _ => json!({ "wsAuthSignature": answer_signature }),
        },
    ));
    let (account, options) = account(
        link,
        "hyperliquid",
        json!({ "key": "0xACCOUNT", "vault": "0xV" }),
    );
    assert_eq!(options["vault"], "0xV");
    let (_, signing, _) = refs(&options);
    assert_eq!(
        account.provider.owned_public_address_capability(),
        Capability::Supported
    );
    let statement = r#"{"kind":"l1","action":{"type":"cancel"},"nonce":1,"isMainnet":true}"#;
    let l1 = || {
        request(
            vec![
                Operation::Sha256 {
                    payload: vec![bytes(statement)],
                },
                Operation::PublicAddress {
                    key: signing.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Secp256k1Digest,
                    key: signing.clone(),
                    payload: vec![PayloadPart::Bytes { bytes: vec![1; 32] }],
                },
            ],
            Default::default(),
        )
    };
    // Nothing is signed before the agent address is bound.
    assert_eq!(
        account.provider.sign(l1()).await.map(|_| ()),
        Err(SignError::Refused)
    );
    let mut setup = account
        .provider
        .create_public_address_setup(request(
            vec![Operation::PublicAddress {
                key: signing.clone(),
            }],
            Default::default(),
        ))
        .unwrap();
    assert!(
        setup
            .poll_joined(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    );
    setup.start().unwrap();
    assert_eq!(
        poll_setup(&mut setup).await.unwrap().values,
        vec![vec![0xab; 20]]
    );
    assert!(
        setup
            .poll_joined(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            .is_ready()
    );
    let result = account.provider.sign(l1()).await.unwrap();
    let mut expected = vec![0xcd; 64];
    expected.push(0x1b);
    assert_eq!(
        result.values,
        vec![Sha256::digest(statement).to_vec(), vec![0xab; 20], expected]
    );
    let sent = seen.lock().unwrap().clone();
    assert_eq!(
        sent[0]["wsAuth"],
        json!({ "toSign": "0xACCOUNT", "outputFormat": "hyperliquid-address" })
    );
    assert_eq!(
        sent[1]["wsAuth"],
        json!({ "toSign": statement, "outputFormat": "hyperliquid-l1" })
    );
    // A bare digest is never sent.
    let bare = request(
        vec![Operation::Sign {
            algorithm: Algorithm::Secp256k1Digest,
            key: signing,
            payload: vec![PayloadPart::Bytes { bytes: vec![1; 32] }],
        }],
        Default::default(),
    );
    assert_eq!(
        account.provider.sign(bare).await.map(|_| ()),
        Err(SignError::Refused)
    );
}

#[tokio::test]
async fn lighter_splits_the_concatenated_signatures() {
    let (link, seen) = device(Box::new(
        |_| json!({ "wsAuthSignature": format!("0x{}{}", "11".repeat(80), "22".repeat(80)) }),
    ));
    let (account, options) = account(link, "lighter", json!({ "key": "0xL1", "apiKeyIndex": 3 }));
    assert_eq!(options["publicMode"]["apiKeyIndexPresent"], true);
    assert_eq!(options["apiKeyIndex"], 3);
    let (_, signing, _) = refs(&options);
    let sign = Operation::Sign {
        algorithm: Algorithm::SchnorrDigest,
        key: signing,
        payload: vec![PayloadPart::Bytes { bytes: vec![0; 40] }],
    };
    let work = request(
        vec![
            Operation::Sha256 {
                payload: vec![bytes("{\"tx\":1}")],
            },
            sign.clone(),
            sign,
        ],
        Default::default(),
    );
    let result = account.provider.sign(work).await.unwrap();
    assert_eq!(
        result.values,
        vec![
            Sha256::digest(b"{\"tx\":1}").to_vec(),
            vec![0x11; 80],
            vec![0x22; 80]
        ]
    );
    assert_eq!(
        seen.lock().unwrap()[0]["wsAuth"]["outputFormat"],
        "lighter-tx"
    );
}

#[tokio::test]
async fn paradex_binds_the_stark_key_then_signs_descriptors() {
    let (link, seen) = device(Box::new(|request| match request["wsAuth"]["outputFormat"]
        .as_str()
        .unwrap()
    {
        "paradex-public" => json!({ "wsAuthSignature": format!("0x{}", "0a".repeat(32)) }),
        _ => json!({ "wsAuthSignature": format!("0x{}", "0b".repeat(64)) }),
    }));
    let (account, options) = account(link, "paradex", json!({ "key": "0xPARADEX" }));
    let (_, signing, _) = refs(&options);
    let mut setup = account
        .provider
        .create_public_address_setup(request(
            vec![Operation::PublicKey {
                algorithm: Algorithm::StarkDigest,
                key: signing.clone(),
            }],
            Default::default(),
        ))
        .unwrap();
    setup.start().unwrap();
    assert_eq!(
        poll_setup(&mut setup).await.unwrap().values,
        vec![vec![0x0a; 32]]
    );
    let sign = Operation::Sign {
        algorithm: Algorithm::StarkDigest,
        key: signing,
        payload: vec![PayloadPart::Bytes { bytes: vec![0; 32] }],
    };
    let work = request(
        vec![
            Operation::Sha256 {
                payload: vec![bytes("paradex/1 d")],
            },
            sign,
        ],
        Default::default(),
    );
    assert_eq!(
        account.provider.sign(work).await.unwrap().values[1],
        vec![0x0b; 64]
    );
    let sent = seen.lock().unwrap().clone();
    assert_eq!(
        sent[0]["wsAuth"],
        json!({ "toSign": "0xPARADEX", "outputFormat": "paradex-public" })
    );
    assert_eq!(
        sent[1]["wsAuth"],
        json!({ "toSign": "paradex/1 d", "outputFormat": "paradex-stark" })
    );
}

#[tokio::test]
async fn setup_refuses_secrets_secondaries_missing_passphrases_and_undeclared_venues() {
    let link = SignerLink::new(1, Arc::new(|_| true));
    let build = |venue: &str, options: Value| {
        remote_account(link.clone(), venue, None, &options.to_string()).map(|_| ())
    };
    assert!(
        build(
            "okx",
            json!({ "key": "k", "secret": "s", "passphrase": "p" })
        )
        .is_err()
    );
    assert!(build("okx", json!({ "key": "k" })).is_err());
    assert!(build("okx", json!({ "key": "k", "publicDataOnly": true })).is_ok());
    assert!(build("bybit", json!({ "key": "k", "secondary_key": "r" })).is_err());
    assert!(build("bybit", json!({})).is_err());
    assert!(build("kraken", json!({ "key": "k" })).is_err());
}

#[tokio::test(start_paused = true)]
async fn timeouts_count_toward_the_fatal_streak_and_suspension_does_not() {
    let link = SignerLink::new(1, Arc::new(|_| true));
    let (account, options) = account(link.clone(), "bybit", json!({ "key": "k" }));
    let fired = Arc::new(Mutex::new(0));
    let count = fired.clone();
    account.on_fatal(move || *count.lock().unwrap() += 1);
    let (_, signing, _) = refs(&options);
    let work = || {
        request(
            vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload: vec![bytes("x")],
            }],
            Default::default(),
        )
    };
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert_eq!(
            account.provider.sign(work()).await.map(|_| ()),
            Err(SignError::TimedOut)
        );
    }
    // Ten 15s timeouts span 150s, past the 30s floor.
    assert_eq!(*fired.lock().unwrap(), 1);
    account.provider.sign(work()).await.ok();
    assert_eq!(*fired.lock().unwrap(), 1, "fires once");

    let pending = tokio::spawn(account.provider.sign(work()));
    tokio::task::yield_now().await;
    link.suspend();
    assert_eq!(
        pending.await.unwrap().map(|_| ()),
        Err(SignError::Unavailable)
    );
    assert!(!link.is_available());
    link.attach(1, Arc::new(|_| false));
    assert!(link.is_available());
    assert_eq!(
        account.provider.sign(work()).await.map(|_| ()),
        Err(SignError::Unavailable)
    );
    link.mark_unavailable();
    link.attach(1, Arc::new(|_| true));
    assert!(!link.is_available());
}

#[tokio::test]
async fn an_error_answer_refuses_and_a_late_answer_is_ignored() {
    let (link, _) = device(Box::new(|_| json!({ "error": "no key" })));
    let (account, options) = account(link.clone(), "phemex", json!({ "key": "k" }));
    let (_, signing, _) = refs(&options);
    let work = request(
        vec![Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: signing,
            payload: vec![bytes("x")],
        }],
        Default::default(),
    );
    assert_eq!(
        account.provider.sign(work).await.map(|_| ()),
        Err(SignError::Refused)
    );
    assert!(!link.resolve(1, json!({ "id": "unknown", "wsAuthSignature": "00" })));
}

#[tokio::test]
async fn echo_probes_alternate_sizes_and_match_by_id() {
    let frames = Arc::new(Mutex::new(Vec::<String>::new()));
    let record = frames.clone();
    let link = SignerLink::new(
        1,
        Arc::new(move |frame| {
            record.lock().unwrap().push(frame);
            true
        }),
    );
    assert!(link.send_echo());
    assert!(!link.send_echo(), "one outstanding echo");
    assert_eq!(
        frames.lock().unwrap()[0],
        r#"{"t":"echo","d":{"id":1,"pad":""}}"#
    );
    assert_eq!(link.echo_res(&json!({ "id": 9 })), None);
    assert_eq!(
        link.echo_res(&json!({ "id": 1 })).map(|(_, size)| size),
        Some(EchoSize::Small)
    );
    assert!(link.send_echo());
    assert_eq!(
        link.echo_res(&json!({ "id": 2 })).map(|(_, size)| size),
        Some(EchoSize::Large)
    );
}

#[tokio::test]
async fn only_the_sending_socket_answers_and_a_new_socket_aborts_what_waits() {
    let frames = Arc::new(Mutex::new(Vec::<String>::new()));
    let record = frames.clone();
    let outbox: Outbox = Arc::new(move |frame| {
        record.lock().unwrap().push(frame);
        true
    });
    let link = SignerLink::new(1, outbox.clone());
    let (account, options) = account(link.clone(), "binancespot", json!({ "key": "AK" }));
    let (_, signing, _) = refs(&options);
    let work = || {
        request(
            vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload: vec![bytes("x")],
            }],
            Default::default(),
        )
    };
    let answer = |id: &Value| json!({ "id": id, "wsAuthSignature": "07".repeat(32) });

    let pending = tokio::spawn(account.provider.sign(work()));
    while frames.lock().unwrap().is_empty() {
        tokio::task::yield_now().await;
    }
    let id = serde_json::from_str::<Value>(&frames.lock().unwrap()[0]).unwrap()["d"]["id"].clone();
    assert!(
        !link.resolve(2, answer(&id)),
        "another socket cannot answer"
    );
    assert!(link.resolve(1, answer(&id)));
    assert!(!link.resolve(1, answer(&id)), "one response per id");
    assert_eq!(pending.await.unwrap().unwrap().values, vec![MAC.to_vec()]);

    let pending = tokio::spawn(account.provider.sign(work()));
    while frames.lock().unwrap().len() < 2 {
        tokio::task::yield_now().await;
    }
    link.attach(2, outbox.clone());
    assert_eq!(
        pending.await.unwrap().map(|_| ()),
        Err(SignError::Unavailable)
    );

    let pending = tokio::spawn(account.provider.sign(work()));
    while frames.lock().unwrap().len() < 3 {
        tokio::task::yield_now().await;
    }
    let id = serde_json::from_str::<Value>(&frames.lock().unwrap()[2]).unwrap()["d"]["id"].clone();
    assert!(link.resolve(
        2,
        json!({ "id": id, "wsAuthSignature": "0".repeat(MAX_RESPONSE_FIELD + 1) })
    ));
    assert_eq!(pending.await.unwrap().map(|_| ()), Err(SignError::Refused));
}

#[tokio::test]
async fn options_carrying_signing_material_under_any_name_refuse() {
    let link = SignerLink::new(1, Arc::new(|_| true));
    for field in [
        "apiSecret",
        "privateKey",
        "agent_key",
        "starkKey",
        "omniSeed",
        "mnemonic",
        "l2Key",
        "secondaryPassphrase",
    ] {
        for options in [
            json!({ "key": "k", field: "x" }),
            json!({ "key": "k", "nested": { field: "x" } }),
        ] {
            let result = remote_account(link.clone(), "bybit", None, &options.to_string());
            assert!(result.is_err(), "{options}");
        }
    }
    // Ordinary public fields stay.
    let (_, options) = account(
        link,
        "bybit",
        json!({ "key": "k", "privateOnly": true, "privateWsHeaders": false, "auth": { "privateKey": "x", "userAddress": "0xU" } }),
    );
    assert_eq!(options["auth"], json!({ "userAddress": "0xU" }));
    assert_eq!(options["privateOnly"], true);
}

#[tokio::test(start_paused = true)]
async fn an_undecodable_answer_counts_toward_the_fatal_streak() {
    let (link, _) = device(Box::new(
        |_| json!({ "error": "", "wsAuthSignature": "nope" }),
    ));
    let (account, options) = account(link, "bybit", json!({ "key": "k" }));
    let fired = Arc::new(Mutex::new(0));
    let count = fired.clone();
    account.on_fatal(move || *count.lock().unwrap() += 1);
    let (_, signing, _) = refs(&options);
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        let work = request(
            vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload: vec![bytes("x")],
            }],
            Default::default(),
        );
        assert_eq!(
            account.provider.sign(work).await.map(|_| ()),
            Err(SignError::Refused)
        );
        tokio::time::advance(std::time::Duration::from_secs(4)).await;
    }
    assert_eq!(*fired.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_fixed_request_id_waits_for_the_previous_one() {
    let (link, seen) = device(Box::new(|_| json!({ "wsAuthSignature": "07".repeat(32) })));
    let (account, options) = account(link, "binancespot", json!({ "key": "AK" }));
    let (credential, signing, _) = refs(&options);
    let work = || {
        request(
            vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload: vec![
                    bytes("apiKey="),
                    PayloadPart::Credential {
                        key: credential.clone(),
                    },
                    bytes("&recvWindow=5000&timestamp=1700000000000"),
                ],
            }],
            SignReceipt {
                stamp_ms: Some(1_700_000_000_000.),
                ..Default::default()
            },
        )
    };
    let (a, b) = tokio::join!(account.provider.sign(work()), account.provider.sign(work()));
    assert!(a.is_ok() && b.is_ok());
    assert_eq!(seen.lock().unwrap().len(), 2);
}
