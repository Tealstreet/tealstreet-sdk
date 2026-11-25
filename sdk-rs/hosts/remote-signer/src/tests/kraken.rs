use super::*;

fn work(options: &Value, input: &[u8]) -> SignRequest {
    let (credential, signing, _) = refs(options);
    request(
        vec![
            Operation::Present { key: credential },
            Operation::Sha256 {
                payload: vec![PayloadPart::Bytes {
                    bytes: input.into(),
                }],
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key: signing,
                payload: vec![PayloadPart::Result {
                    index: 1,
                    encoding: Encoding::Raw,
                }],
            },
        ],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_123.),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn kraken_codec_preserves_primary_sha512_graph_and_receipt() {
    let mac = [7u8; 64];
    let (link, seen) = device(Box::new(move |_| {
        json!({"wsAuthSignature":json!({
        "credential":"pub", "signature":base64::engine::general_purpose::STANDARD.encode(mac)
    }).to_string()})
    }));
    let (account, options) = account(link, "kraken", json!({"key":"pub"}));
    for input in ["/api/v3/accounts", "synthetic-challenge"] {
        let work = work(&options, input.as_bytes());
        let receipt = work.receipt;
        let result = account.provider.sign(work).await.unwrap();
        assert_eq!(result.receipt, receipt);
        assert_eq!(
            result.values,
            vec![
                b"pub".to_vec(),
                Sha256::digest(input.as_bytes()).to_vec(),
                mac.to_vec()
            ]
        );
        let frame = seen.lock().unwrap().last().unwrap().clone();
        assert_eq!(frame["exchangeName"], "kraken");
        assert_eq!(frame["accountName"], "main");
        assert_eq!(frame["wsAuth"]["outputFormat"], "kraken-sha512");
        let descriptor: Value =
            serde_json::from_str(frame["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
        assert_eq!(descriptor, json!({"slot":"primary","payload":input}));
        assert!(frame["wsAuth"].get("extraData").is_none());
    }
}

#[tokio::test]
async fn kraken_codec_advertises_only_the_owned_sha512_key() {
    let (link, seen) = device(Box::new(|_| panic!("capability query must not ask device")));
    let (account, options) = account(link, "kraken", json!({"key":"pub"}));
    let (_, signing, _) = refs(&options);
    assert_eq!(
        account.provider.can_sign(&signing, Algorithm::HmacSha512),
        Capability::Supported
    );
    assert_eq!(
        account.provider.can_sign(&signing, Algorithm::HmacSha256),
        Capability::Unsupported
    );
    assert_eq!(
        account
            .provider
            .can_sign(&KeyRef("foreign".into()), Algorithm::HmacSha512),
        Capability::Unsupported
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn kraken_codec_rejects_malformed_or_foreign_batches() {
    let valid = base64::engine::general_purpose::STANDARD.encode([7u8; 64]);
    for response in [
        json!({"credential":"other","signature":valid}),
        json!({"credential":"pub","signature":valid,"extra":true}),
        json!({"credential":"pub","signature":"invalid"}),
        json!({"credential":"pub","signature":base64::engine::general_purpose::STANDARD.encode([7u8;32])}),
    ] {
        let (link, _) = device(Box::new(
            move |_| json!({"wsAuthSignature":response.to_string()}),
        ));
        let (account, options) = account(link, "kraken", json!({"key":"pub"}));
        assert!(
            account
                .provider
                .sign(work(&options, b"/api/v3/accounts"))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn kraken_codec_rejects_unowned_or_unbound_graphs_before_io() {
    let (link, seen) = device(Box::new(|_| panic!("invalid graph cannot ask device")));
    let (account, options) = account(link, "kraken", json!({"key":"pub"}));
    let correct = work(&options, b"/api/v3/accounts");
    for tamper in 0..7 {
        let mut work = correct.clone();
        match tamper {
            0 => {
                if let Operation::Present { key } = &mut work.operations[0] {
                    *key = KeyRef("foreign".into());
                }
            }
            1 => {
                if let Operation::Sign { key, .. } = &mut work.operations[2] {
                    *key = KeyRef("foreign".into());
                }
            }
            2 => {
                if let Operation::Sign { payload, .. } = &mut work.operations[2] {
                    *payload = vec![PayloadPart::Result {
                        index: 0,
                        encoding: Encoding::Raw,
                    }];
                }
            }
            3 => {
                if let Operation::Sign { algorithm, .. } = &mut work.operations[2] {
                    *algorithm = Algorithm::HmacSha256;
                }
            }
            4 => {
                work.operations.pop();
            }
            5 => {
                if let Operation::Sha256 { payload } = &mut work.operations[1] {
                    *payload = vec![bytes("")];
                }
            }
            _ => work.receipt.stamp_ms = Some(f64::NAN),
        }
        assert!(
            account.provider.sign(work).await.is_err(),
            "tamper {tamper}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}
