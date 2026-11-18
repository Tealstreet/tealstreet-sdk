use super::*;

type Check = Result<(), Box<dyn std::error::Error>>;

fn vector(testnet: bool) -> Result<Value, Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/grvt-browser-goldens.json"))?;
    fixture["vectors"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["testnet"] == testnet))
        .cloned()
        .ok_or_else(|| "missing browser golden".into())
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, Box<dyn std::error::Error>> {
    value[field]
        .as_str()
        .ok_or_else(|| format!("missing golden field {field}").into())
}

fn golden_bytes(value: &Value, field: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    for pair in text(value, field)?.as_bytes().as_chunks::<2>().0 {
        bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    Ok(bytes)
}

fn order(signing: KeyRef, golden: &Value) -> Result<SignRequest, Box<dyn std::error::Error>> {
    Ok(request(
        vec![
            Operation::Sha256 {
                payload: vec![bytes(text(golden, "statement")?)],
            },
            Operation::PublicAddress {
                key: signing.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: signing,
                payload: vec![PayloadPart::Bytes {
                    bytes: golden_bytes(golden, "digest")?,
                }],
            },
        ],
        SignReceipt {
            expiry_ms: Some(1_700_003_600_000.),
            ..SignReceipt::default()
        },
    ))
}

fn configured(link: Arc<SignerLink>) -> (RemoteAccount, KeyRef, KeyRef) {
    let (account, options) = account(link, "grvt", json!({ "key": "grvt-fixture-api" }));
    let (credential, signing, _) = refs(&options);
    (account, credential, signing)
}

#[tokio::test]
async fn native_grvt_advertises_secp256k1_for_owned_primary_only() {
    let (link, seen) = device(Box::new(|_| json!({ "error": "must not sign to probe" })));
    let (account, credential, signing) = configured(link);
    assert_eq!(
        account
            .provider
            .can_sign(&signing, Algorithm::Secp256k1Digest),
        Capability::Supported
    );
    for key in [credential, KeyRef("foreign".into())] {
        assert_eq!(
            account.provider.can_sign(&key, Algorithm::Secp256k1Digest),
            Capability::Unsupported
        );
    }
    assert_eq!(
        account.provider.can_sign(&signing, Algorithm::HmacSha256),
        Capability::Unsupported
    );
    assert!(seen.lock().unwrap_or_else(|p| p.into_inner()).is_empty());
}

#[tokio::test]
async fn native_grvt_login_presents_only_its_owned_credential() -> Check {
    let (link, seen) = device(Box::new(
        |_| json!({ "error": "login needs no device work" }),
    ));
    let (account, credential, signing) = configured(link);
    let receipt = SignReceipt::default();
    let result = account
        .provider
        .sign(request(
            vec![Operation::Present { key: credential }],
            receipt,
        ))
        .await?;
    assert_eq!(result.values, vec![b"grvt-fixture-api".to_vec()]);
    assert_eq!(result.receipt, receipt);
    assert!(result.headers.is_empty());
    for key in [signing, KeyRef("foreign".into())] {
        assert!(matches!(
            account
                .provider
                .sign(request(vec![Operation::Present { key }], receipt))
                .await,
            Err(SignError::Refused)
        ));
    }
    assert!(seen.lock().unwrap_or_else(|p| p.into_inner()).is_empty());
    Ok(())
}

#[tokio::test]
async fn native_grvt_forwards_one_closed_statement_and_decodes_address_signature() -> Check {
    for testnet in [false, true] {
        let golden = vector(testnet)?;
        let wire = golden["wire"].clone();
        let (link, seen) = device(Box::new(move |_| json!({ "wsAuthSignature": wire })));
        let (account, _, signing) = configured(link);
        let work = order(signing, &golden)?;
        let receipt = work.receipt;
        let result = account.provider.sign(work).await?;
        assert_eq!(
            result.values,
            vec![
                golden_bytes(&golden, "commitment")?,
                golden_bytes(&golden, "address")?,
                golden_bytes(&golden, "signature")?,
            ]
        );
        assert_eq!(result.receipt, receipt);
        assert!(result.headers.is_empty());
        let seen = seen.lock().unwrap_or_else(|p| p.into_inner());
        assert_eq!(seen.len(), 1);
        let mut envelope = seen[0].clone();
        assert!(envelope["id"].as_str().is_some_and(|id| !id.is_empty()));
        envelope
            .as_object_mut()
            .ok_or("request is not an object")?
            .remove("id");
        assert_eq!(
            envelope,
            json!({ "type": "ws-auth", "exchangeName": "grvt", "accountName": "main",
                "wsAuth": { "toSign": golden["statement"], "outputFormat": "grvt-order" } })
        );
        assert!(!seen[0].to_string().contains("delegated/"));
        assert!(!seen[0].to_string().contains("grvt-fixture-api"));
    }
    Ok(())
}

#[tokio::test]
async fn grvt_foreign_refs_algorithms_and_reordered_graphs_never_contact_device() -> Check {
    let (link, seen) = device(Box::new(
        |_| json!({ "error": "invalid graph reached device" }),
    ));
    let (account, credential, signing) = configured(link);
    let golden = vector(false)?;
    let valid = order(signing, &golden)?;
    let mut invalid = vec![vec![], valid.operations[1..].to_vec()];
    let mut operations = valid.operations.clone();
    operations.swap(0, 1);
    invalid.push(operations);
    let mut operations = valid.operations.clone();
    operations.push(operations[1].clone());
    invalid.push(operations);
    for index in [1, 2] {
        for foreign in [credential.clone(), KeyRef("foreign".into())] {
            let mut operations = valid.operations.clone();
            match &mut operations[index] {
                Operation::PublicAddress { key } | Operation::Sign { key, .. } => *key = foreign,
                _ => return Err("unexpected golden operation".into()),
            }
            invalid.push(operations);
        }
    }
    let mut operations = valid.operations.clone();
    if let Operation::Sign { algorithm, .. } = &mut operations[2] {
        *algorithm = Algorithm::HmacSha256;
    }
    invalid.push(operations);
    for length in [0, 31, 33] {
        let mut operations = valid.operations.clone();
        if let Operation::Sign { payload, .. } = &mut operations[2] {
            *payload = vec![PayloadPart::Bytes {
                bytes: vec![0; length],
            }];
        }
        invalid.push(operations);
    }
    for payload in [
        vec![],
        vec![bytes("{"), bytes("}")],
        vec![PayloadPart::Bytes { bytes: vec![255] }],
        vec![PayloadPart::Bytes {
            bytes: vec![0; 1_048_577],
        }],
    ] {
        let mut operations = valid.operations.clone();
        operations[0] = Operation::Sha256 { payload };
        invalid.push(operations);
    }
    for operations in invalid {
        assert!(matches!(
            account
                .provider
                .sign(request(operations, valid.receipt))
                .await,
            Err(SignError::Refused)
        ));
    }
    assert!(seen.lock().unwrap_or_else(|p| p.into_inner()).is_empty());
    Ok(())
}

#[tokio::test]
async fn grvt_malformed_hex_wrong_width_or_error_reply_never_yields_a_sign_result() -> Check {
    let golden = vector(false)?;
    let wire = text(&golden, "wire")?;
    let malformed = vec![
        json!({}),
        json!({ "wsAuthSignature": false }),
        json!({ "wsAuthSignature": "" }),
        json!({ "wsAuthSignature": &wire[2..] }),
        json!({ "wsAuthSignature": wire.replace("0x", "0X") }),
        json!({ "wsAuthSignature": &wire[..wire.len() - 2] }),
        json!({ "wsAuthSignature": format!("{wire}00") }),
        json!({ "wsAuthSignature": format!("0x{}z", &wire[2..wire.len() - 1]) }),
        json!({ "wsAuthSignature": wire, "error": "device refused" }),
    ];
    for response in malformed {
        let (link, seen) = device(Box::new(move |_| response.clone()));
        let (account, _, signing) = configured(link);
        let result = account.provider.sign(order(signing, &golden)?).await;
        assert!(matches!(result, Err(SignError::Refused)));
        assert_eq!(seen.lock().unwrap_or_else(|p| p.into_inner()).len(), 1);
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn grvt_link_cancel_timeout_disconnect_and_late_reply_preserve_outcomes() -> Check {
    let golden = vector(false)?;
    for mode in ["cancel", "timeout", "disconnect"] {
        let (sender, mut frames) = mpsc::unbounded_channel();
        let link = SignerLink::new(7, Arc::new(move |frame| sender.send(frame).is_ok()));
        let (account, _, signing) = configured(link.clone());
        let work = order(signing, &golden)?;
        let pending = tokio::spawn(async move { account.provider.sign(work).await });
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), frames.recv())
            .await?
            .ok_or("missing sign request")?;
        let frame: Value = serde_json::from_str(&frame)?;
        let id = frame["d"]["id"].clone();
        assert!(!link.resolve(8, json!({ "id": id, "wsAuthSignature": golden["wire"] })));
        match mode {
            "cancel" => {
                pending.abort();
                assert!(pending.await.is_err_and(|error| error.is_cancelled()));
            }
            "timeout" => {
                link.heard(7);
                tokio::time::advance(SIGN_TIMEOUT + std::time::Duration::from_millis(1)).await;
                assert!(matches!(pending.await?, Err(SignError::TimedOut)));
            }
            _ => {
                link.mark_unavailable();
                assert!(matches!(pending.await?, Err(SignError::Unavailable)));
            }
        }
        assert!(!link.resolve(7, json!({ "id": id, "wsAuthSignature": golden["wire"] })));
        assert!(frames.try_recv().is_err());
    }
    Ok(())
}
