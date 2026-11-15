use super::*;

fn read(options: &Value, payload: Vec<PayloadPart>) -> SignRequest {
    let (credential, signing, passphrase) = refs(options);
    request(
        vec![
            Operation::Present { key: credential },
            Operation::Present {
                key: passphrase.unwrap(),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing,
                payload,
            },
        ],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_123.),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn apexomni_batch_bridge_keeps_rest_and_socket_bytes_in_one_closed_descriptor() {
    let (link, seen) = device(Box::new(|request| {
        if request["wsAuth"]["outputFormat"] != "apexomni-batch" {
            return json!({"error":"synthetic unsupported device request"});
        }
        json!({"wsAuthSignature":json!({"credential":"pub","contracts":[],"signature":base64::engine::general_purpose::STANDARD.encode(MAC)}).to_string()})
    }));
    let (account, options) = account(
        link,
        "apexomni",
        json!({"key":"pub","passphrase":"synthetic-passphrase"}),
    );
    for payload in [
        "1700000000123GET/api/v3/account",
        "1700000000123GET/ws/accounts",
    ] {
        let work = read(&options, vec![bytes(payload)]);
        let receipt = work.receipt;
        let signed = account.provider.sign(work).await.unwrap();
        assert_eq!(signed.receipt, receipt);
        assert_eq!(
            signed.values,
            vec![
                b"pub".to_vec(),
                b"synthetic-passphrase".to_vec(),
                MAC.to_vec()
            ]
        );
        let sent = seen.lock().unwrap().last().unwrap().clone();
        assert_eq!(sent["exchangeName"], "apexomni");
        assert_eq!(sent["accountName"], "main");
        assert_eq!(sent["wsAuth"]["outputFormat"], "apexomni-batch");
        assert!(sent["wsAuth"].get("extraData").is_none());
        let descriptor: Value =
            serde_json::from_str(sent["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
        assert_eq!(
            descriptor,
            json!({"contracts":[],"payload":[{"bytes":payload.as_bytes()}]})
        );
        let text = sent.to_string();
        assert!(!text.contains("synthetic-passphrase"));
        assert!(
            !text.contains("delegated/"),
            "opaque engine refs never cross the wire"
        );
    }
}

#[tokio::test]
async fn apexomni_batch_bridge_rejects_foreign_credential_and_malformed_batch_replies() {
    for response in [
        json!({"credential":"other","contracts":[],"signature":base64::engine::general_purpose::STANDARD.encode(MAC)}),
        json!({"credential":"pub","contracts":["00".repeat(64)],"signature":base64::engine::general_purpose::STANDARD.encode(MAC)}),
        json!({"credential":"pub","contracts":[],"signature":"invalid"}),
        json!({"credential":"pub","contracts":[],"signature":base64::engine::general_purpose::STANDARD.encode(MAC),"extra":true}),
    ] {
        let (link, _) = device(Box::new(
            move |_| json!({"wsAuthSignature":response.to_string()}),
        ));
        let (account, options) = account(
            link,
            "apexomni",
            json!({"key":"pub","passphrase":"synthetic-passphrase"}),
        );
        assert!(
            account
                .provider
                .sign(read(
                    &options,
                    vec![bytes("1700000000123GET/api/v3/account")]
                ))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn apexomni_batch_bridge_refuses_unowned_refs_and_unbound_result_dependencies_without_io() {
    let (link, seen) = device(Box::new(|_| {
        panic!("invalid graph must refuse before device IO")
    }));
    let (account, options) = account(
        link,
        "apexomni",
        json!({"key":"pub","passphrase":"synthetic-passphrase"}),
    );
    let mut foreign = read(&options, vec![bytes("1700000000123GET/api/v3/account")]);
    if let Operation::Sign { key, .. } = &mut foreign.operations[2] {
        *key = KeyRef("foreign".into());
    }
    assert!(account.provider.sign(foreign).await.is_err());
    assert!(
        account
            .provider
            .sign(read(
                &options,
                vec![PayloadPart::Result {
                    index: 0,
                    encoding: Encoding::Hex
                }]
            ))
            .await
            .is_err()
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn apexomni_batch_bridge_preserves_owned_contract_dependencies_and_result_order() {
    let (link, seen) = device(Box::new(|_| {
        json!({"wsAuthSignature":json!({
        "credential":"pub", "contracts":["ab".repeat(64)],
        "signature":base64::engine::general_purpose::STANDARD.encode(MAC)
    }).to_string()})
    }));
    let (account, options) = account(
        link,
        "apexomni",
        json!({"key":"pub","passphrase":"synthetic-passphrase"}),
    );
    let seed = KeyRef(
        options["signing"]["primary"]["extra"]["zklinkSeed"]
            .as_str()
            .expect("ApeX installs only an opaque device-owned seed role")
            .into(),
    );
    assert_eq!(
        account.provider.can_sign(&seed, Algorithm::ZkLinkContract),
        Capability::Supported
    );
    assert_eq!(
        account.provider.can_sign(&seed, Algorithm::HmacSha256),
        Capability::Unsupported
    );
    let mut contract = vec![0; 36];
    contract[0] = 0xfe;
    contract[4] = 1;
    let mut work = read(
        &options,
        vec![
            bytes("prefix"),
            PayloadPart::Result {
                index: 0,
                encoding: Encoding::Hex,
            },
            bytes("suffix"),
        ],
    );
    work.operations.insert(
        0,
        Operation::Sign {
            algorithm: Algorithm::ZkLinkContract,
            key: seed,
            payload: vec![PayloadPart::Bytes {
                bytes: contract.clone(),
            }],
        },
    );
    let receipt = work.receipt;
    let signed = account.provider.sign(work).await.unwrap();
    assert_eq!(signed.receipt, receipt);
    assert_eq!(
        signed.values,
        vec![
            vec![0xab; 64],
            b"pub".to_vec(),
            b"synthetic-passphrase".to_vec(),
            MAC.to_vec()
        ]
    );
    let sent = seen.lock().unwrap().last().unwrap().clone();
    assert_eq!(sent["wsAuth"]["outputFormat"], "apexomni-batch");
    let descriptor: Value =
        serde_json::from_str(sent["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
    assert_eq!(
        descriptor,
        json!({"contracts":[contract],"payload":[{"bytes":b"prefix"},{"contract":0},{"bytes":b"suffix"}]})
    );
}

fn contract_request(options: &Value) -> SignRequest {
    let seed = KeyRef(
        options["signing"]["primary"]["extra"]["zklinkSeed"]
            .as_str()
            .unwrap()
            .into(),
    );
    let mut contract = vec![0; 36];
    contract[0] = 0xfe;
    contract[4] = 1;
    let mut work = read(
        options,
        vec![
            bytes("prefix"),
            PayloadPart::Result {
                index: 0,
                encoding: Encoding::Hex,
            },
        ],
    );
    work.operations.insert(
        0,
        Operation::Sign {
            algorithm: Algorithm::ZkLinkContract,
            key: seed,
            payload: vec![PayloadPart::Bytes { bytes: contract }],
        },
    );
    work
}

#[tokio::test]
async fn apexomni_batch_bridge_rejects_invalid_contract_graphs_before_device_io() {
    let (link, seen) = device(Box::new(|_| {
        panic!("invalid graph cannot reach the device")
    }));
    let (account, options) = account(
        link,
        "apexomni",
        json!({"key":"pub","passphrase":"synthetic-passphrase"}),
    );
    for case in [
        "foreign-seed",
        "missing-reference",
        "duplicate-reference",
        "wrong-encoding",
        "wrong-size",
        "wrong-transaction",
        "nonce",
    ] {
        let mut work = contract_request(&options);
        match case {
            "foreign-seed" => {
                if let Operation::Sign { key, .. } = &mut work.operations[0] {
                    *key = KeyRef("foreign".into());
                }
            }
            "missing-reference" => {
                if let Operation::Sign { payload, .. } = &mut work.operations[3] {
                    payload.pop();
                }
            }
            "duplicate-reference" => {
                if let Operation::Sign { payload, .. } = &mut work.operations[3] {
                    payload.push(payload[1].clone());
                }
            }
            "wrong-encoding" => {
                if let Operation::Sign { payload, .. } = &mut work.operations[3] {
                    payload[1] = PayloadPart::Result {
                        index: 0,
                        encoding: Encoding::Base64,
                    };
                }
            }
            "wrong-size" => {
                if let Operation::Sign { payload, .. } = &mut work.operations[0] {
                    payload[0] = PayloadPart::Bytes {
                        bytes: vec![0xfe; 35],
                    };
                }
            }
            "wrong-transaction" => {
                if let Operation::Sign { payload, .. } = &mut work.operations[0] {
                    payload[0] = PayloadPart::Bytes { bytes: vec![0; 36] };
                }
            }
            "nonce" => work.receipt.nonce = Some(1),
            _ => unreachable!(),
        }
        assert!(account.provider.sign(work).await.is_err(), "{case}");
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn apexomni_batch_bridge_rejects_noncanonical_contract_signatures_atomically() {
    for signature in ["AB".repeat(64), "ab".repeat(63), "zz".repeat(64)] {
        let (link, _) = device(Box::new(move |_| {
            json!({"wsAuthSignature":json!({
            "credential":"pub", "contracts":[signature],
            "signature":base64::engine::general_purpose::STANDARD.encode(MAC)
        }).to_string()})
        }));
        let (account, options) = account(
            link,
            "apexomni",
            json!({"key":"pub","passphrase":"synthetic-passphrase"}),
        );
        assert!(
            account
                .provider
                .sign(contract_request(&options))
                .await
                .is_err()
        );
    }
}
