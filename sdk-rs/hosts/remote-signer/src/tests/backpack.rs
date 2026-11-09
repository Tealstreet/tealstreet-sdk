use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, Identity, KeyRef, Operation, PayloadPart, Provider,
    SignError, SignReceipt, SignRequest, SignResult, Signer,
};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{RemoteAccount, SIGN_TIMEOUT, SignerLink, remote_account};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

// Fixed TS backpackHostedFixtures.signature output (Node/OpenSSL), not Rust output.
const GOLDENS: &[(&str, &str)] = &[
    (
        "instruction=accountQuery&timestamp=1700000000123&window=60000",
        "1gZYZYQm8j7JrfPgz/g9tLQG/x695NEo6JMGoe/DyYRf1RVFGq/GJXkVovU1LNto73KT0JeQsIv2QNodJEy+Cw==",
    ),
    (
        "instruction=accountQuery&timestamp=1700000001123&window=60000",
        "IoGD5pJNhE06zvm4Vowbs+JZsggZyC9nOVxx7TtCtGlnPPJUYPRCMGjacv+S+Xz7y+VAXMg91zxFBO4dpns6Bg==",
    ),
    (
        "instruction=orderExecute&quantity=5&side=Bid&symbol=SOL_USDC_PERP&instruction=orderExecute&quantity=0.01&side=Ask&symbol=BTC_USDC_PERP&timestamp=1700000000123&window=60000",
        "IZP9j7brr6nQcnrXzeXTpQXRMhmJM/zMhc1tAhQ5a5p//wFOYGq2EWl/8O1z0e2DdLumIcmDZl3qwYQZqH8gBA==",
    ),
    (
        "instruction=orderExecute&quantity=5&side=Bid&symbol=SOL_USDC_PERP&instruction=orderExecute&quantity=0.01&side=Ask&symbol=BTC_USDC_PERP&timestamp=1700000001123&window=60000",
        "zP3eNu/inBgUcpjKWG07snuztZ9F+x1equ9i8aMEHGVIe3u+QfBLJr42owpI0dOGGb4791Hx0OwwEGzUqZAaDw==",
    ),
    (
        "instruction=subscribe&timestamp=1700000000123&window=5000",
        "6xio6nJUi3Q8BokO0mlnl1TwJ4MbBQfj6HdEGcZ931lPaaQC/IOAwdzOv7LhNLU/6tCzNjs5P/PLa0iUsnXzCg==",
    ),
    (
        "instruction=subscribe&timestamp=1700000001123&window=5000",
        "0d1hovY2iXOAdbG3Gg0gIXgD5H1vITuv4QBX1DypmjD++GFHYBeRC23p/+OB3LwkECZ/4TgZE60oUkB/NxrbCQ==",
    ),
];

fn channel() -> (Arc<SignerLink>, mpsc::UnboundedReceiver<String>) {
    let (sender, frames) = mpsc::unbounded_channel();
    (
        SignerLink::new(1, Arc::new(move |frame| sender.send(frame).is_ok())),
        frames,
    )
}

fn setup(link: Arc<SignerLink>) -> Result<(RemoteAccount, Identity), TestError> {
    let account = remote_account(
        link,
        "backpack",
        Some("fixture-account".into()),
        r#"{"key":"fixture-key"}"#,
    )?;
    let options: Value = serde_json::from_str(&account.options_json)?;
    let identity: Identity = serde_json::from_value(options["signing"]["primary"].clone())?;
    for field in ["key", "secret", "passphrase"] {
        assert!(options.get(field).is_none());
    }
    Ok((account, identity))
}

fn local() -> Result<(LocalSigner, Identities), SignError> {
    let mut signer = LocalSigner::new();
    let credential = KeyRef("device-public".into());
    let signing = KeyRef("device-owned-ed25519".into());
    signer.insert(credential.clone(), b"fixture-key".to_vec());
    signer.insert_declared(signing.clone(), vec![7; 32], Algorithm::Ed25519)?;
    Ok((
        signer,
        Identities {
            primary: Some(Identity::new(credential, Some(signing))),
            secondary: None,
        },
    ))
}

fn work(identity: &Identity, payload: &str) -> Result<SignRequest, SignError> {
    Ok(SignRequest {
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sign {
                key: identity.signing_ref()?.clone(),
                algorithm: Algorithm::Ed25519,
                payload: vec![PayloadPart::Bytes {
                    bytes: payload.as_bytes().to_vec(),
                }],
            },
        ],
        receipt: SignReceipt {
            stamp_ms: Some(1_700_000_000_123.),
            ..Default::default()
        },
        context: Default::default(),
    })
}

fn reply(request: &Value, value: Value) -> Value {
    json!({ "id": request["id"], "wsAuthSignature": value })
}

async fn exchange(
    provider: &Provider,
    link: &SignerLink,
    frames: &mut mpsc::UnboundedReceiver<String>,
    request: SignRequest,
    answer: impl FnOnce(&Value) -> Result<Value, TestError>,
) -> Result<Result<SignResult, SignError>, TestError> {
    let mut pending = provider.sign(request);
    let frame = tokio::select! {
        _ = &mut pending => return Err("signing completed before device round-trip".into()),
        frame = frames.recv() => frame.ok_or("device channel closed")?,
    };
    let envelope: Value = serde_json::from_str(&frame)?;
    assert!(link.resolve(1, answer(&envelope["d"])?));
    Ok(pending.await)
}

#[tokio::test]
async fn backpack_native_advertises_only_owned_ed25519() -> TestResult {
    let (link, _) = channel();
    let (account, identity) = setup(link)?;
    let signing = identity.signing_ref()?;
    let (local, ids) = local()?;
    let local_identity = ids.primary.as_ref().ok_or("missing local identity")?;
    assert_eq!(
        STANDARD.encode(&local.sign_now(&work(local_identity, GOLDENS[0].0)?)?.values[1]),
        GOLDENS[0].1
    );
    assert_eq!(
        account.provider.can_sign(signing, Algorithm::Ed25519),
        Capability::Supported
    );
    for algorithm in [Algorithm::HmacSha256, Algorithm::Secp256k1Digest] {
        assert_eq!(
            account.provider.can_sign(signing, algorithm),
            Capability::Unsupported
        );
    }
    for key in [
        identity.credential,
        KeyRef("foreign".into()),
        KeyRef("secondary".into()),
    ] {
        assert_eq!(
            account.provider.can_sign(&key, Algorithm::Ed25519),
            Capability::Unsupported
        );
    }
    Ok(())
}

#[tokio::test]
async fn backpack_native_device_roundtrip_matches_local_bytes() -> TestResult {
    let (link, mut frames) = channel();
    let (account, identity) = setup(link.clone())?;
    let (local, ids) = local()?;
    for &(payload, signature) in GOLDENS {
        let local_identity = ids.primary.as_ref().ok_or("missing local identity")?;
        let expected = local.sign_now(&work(local_identity, payload)?)?;
        assert_eq!(STANDARD.encode(&expected.values[1]), signature);
        let mut request = work(&identity, payload)?;
        if payload.contains("1700000001123") {
            request.receipt.stamp_ms = Some(1_700_000_001_123.);
        }
        let receipt = request.receipt;
        let result = exchange(&account.provider, &link, &mut frames, request, |request| {
            assert_eq!(request["type"], "ws-auth");
            assert_eq!(request["exchangeName"], "backpack");
            assert_eq!(request["accountName"], "fixture-account");
            assert_eq!(request["wsAuth"]["outputFormat"], "backpack-ed25519");
            let to_sign = request["wsAuth"]["toSign"]
                .as_str()
                .ok_or("missing descriptor")?;
            assert_eq!(
                serde_json::from_str::<Value>(to_sign)?,
                json!({"slot":"primary", "payload":payload})
            );
            assert!(!to_sign.contains(&identity.credential.0));
            assert!(!to_sign.contains(&identity.signing_ref()?.0));
            let (graph, output) = hosted_signing::prepare(
                "backpack",
                &ids,
                Input::WsAuth {
                    to_sign: to_sign.into(),
                    output_format: OutputFormat::BackpackEd25519,
                },
            )?;
            let signed = local.sign_now(&graph)?;
            let output = hosted_signing::finish(output, &signed)?;
            Ok(reply(
                request,
                json!(output.signature.ok_or("missing signature")?),
            ))
        })
        .await??;
        assert_eq!(result.values, expected.values);
        assert_eq!(result.receipt, receipt);
        assert!(result.headers.is_empty());
    }
    let presented = account
        .provider
        .sign(SignRequest {
            operations: vec![Operation::Present {
                key: identity.credential,
            }],
            receipt: Default::default(),
            context: Default::default(),
        })
        .await?;
    assert_eq!(presented.values, vec![b"fixture-key".to_vec()]);
    assert!(frames.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn backpack_native_refuses_foreign_mixed_and_malformed_graphs_before_io() -> TestResult {
    let (link, mut frames) = channel();
    let (account, identity) = setup(link.clone())?;
    let valid = work(&identity, GOLDENS[0].0)?;
    let mut invalid = vec![
        vec![],
        vec![valid.operations[1].clone()],
        vec![valid.operations[1].clone(), valid.operations[0].clone()],
        vec![
            valid.operations[0].clone(),
            valid.operations[1].clone(),
            valid.operations[1].clone(),
        ],
        vec![Operation::Present {
            key: KeyRef("foreign".into()),
        }],
    ];
    for key in [
        identity.credential.clone(),
        KeyRef("foreign".into()),
        KeyRef("secondary".into()),
    ] {
        invalid.push(vec![
            valid.operations[0].clone(),
            Operation::Sign {
                key,
                algorithm: Algorithm::Ed25519,
                payload: vec![PayloadPart::Bytes {
                    bytes: b"payload".to_vec(),
                }],
            },
        ]);
    }
    invalid.push(vec![
        Operation::Present {
            key: identity.signing_ref()?.clone(),
        },
        valid.operations[1].clone(),
    ]);
    for algorithm in [Algorithm::HmacSha256, Algorithm::Secp256k1Digest] {
        invalid.push(vec![
            valid.operations[0].clone(),
            Operation::Sign {
                key: identity.signing_ref()?.clone(),
                algorithm,
                payload: vec![PayloadPart::Bytes {
                    bytes: b"payload".to_vec(),
                }],
            },
        ]);
    }
    for payload in [
        vec![],
        vec![PayloadPart::Bytes { bytes: vec![] }],
        vec![PayloadPart::Bytes { bytes: vec![255] }],
        vec![PayloadPart::Bytes {
            bytes: vec![b'x'; 1_048_577],
        }],
        vec![
            PayloadPart::Bytes { bytes: vec![b'x'] },
            PayloadPart::Bytes { bytes: vec![b'y'] },
        ],
        vec![PayloadPart::Credential {
            key: identity.credential.clone(),
        }],
    ] {
        invalid.push(vec![
            valid.operations[0].clone(),
            Operation::Sign {
                key: identity.signing_ref()?.clone(),
                algorithm: Algorithm::Ed25519,
                payload,
            },
        ]);
    }
    for operations in invalid {
        let mut pending = account.provider.sign(SignRequest {
            operations,
            receipt: Default::default(),
            context: Default::default(),
        });
        let result = tokio::select! {
            result = &mut pending => result,
            _ = frames.recv() => return Err("invalid graph reached device".into()),
        };
        assert_eq!(result.err(), Some(SignError::Refused));
    }
    for options in [
        json!({"key":"fixture-key", "secondary_key":"secondary"}),
        json!({"key":"fixture-key", "secret":"SYNTHETIC-LEAK-MARKER"}),
    ] {
        let result = remote_account(link.clone(), "backpack", None, &options.to_string());
        assert!(result.is_err());
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn backpack_native_refuses_unbound_or_noncanonical_device_results_atomically() -> TestResult {
    let (link, mut frames) = channel();
    let (account, identity) = setup(link.clone())?;
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = failures.clone();
    account.on_fatal(move || {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let good = GOLDENS[0].1;
    let mut noncanonical = good.to_owned();
    noncanonical.replace_range(85..86, "B");
    let invalid = vec![
        json!({"credential":"foreign", "signature":good}).to_string(),
        json!({"credential":"fixture-key"}).to_string(),
        json!({"credential":"fixture-key", "signature":good, "extra":true}).to_string(),
        format!(r#"{{"credential":"foreign","credential":"fixture-key","signature":"{good}"}}"#),
        json!({"credential":"fixture-key", "signature":STANDARD.encode([0; 32])}).to_string(),
        json!({"credential":"fixture-key", "signature":good.trim_end_matches('=')}).to_string(),
        json!({"credential":"fixture-key", "signature":noncanonical}).to_string(),
        json!({"credential":"fixture-key", "signature":"not base64"}).to_string(),
        "malformed".into(),
    ];
    for value in invalid {
        let result = exchange(
            &account.provider,
            &link,
            &mut frames,
            work(&identity, GOLDENS[0].0)?,
            |request| Ok(reply(request, json!(value))),
        )
        .await?;
        assert_eq!(result.err(), Some(SignError::Refused));
        tokio::time::advance(std::time::Duration::from_secs(4)).await;
    }
    let result = exchange(
        &account.provider,
        &link,
        &mut frames,
        work(&identity, GOLDENS[0].0)?,
        |request| Ok(json!({"id":request["id"], "error":"SYNTHETIC-LEAK-MARKER"})),
    )
    .await?;
    let error = result.err().ok_or("callback error signed")?;
    assert_eq!(error, SignError::Refused);
    assert_eq!(failures.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!error.to_string().contains("SYNTHETIC-LEAK-MARKER"));
    assert_eq!(
        error.venue_error().outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn backpack_native_cancellation_timeout_and_late_answer_do_not_publish() -> TestResult {
    let (link, mut frames) = channel();
    let (account, identity) = setup(link.clone())?;
    let mut pending = account.provider.sign(work(&identity, GOLDENS[0].0)?);
    let frame = tokio::select! {
        _ = &mut pending => return Err("sign completed without device".into()),
        frame = frames.recv() => frame.ok_or("channel closed")?,
    };
    let request: Value = serde_json::from_str(&frame)?;
    let good = json!({"credential":"fixture-key", "signature":GOLDENS[0].1}).to_string();
    assert!(!link.resolve(1, json!({"id":"wrong", "wsAuthSignature":good})));
    assert!(!link.resolve(2, reply(&request["d"], json!(good))));
    drop(pending);
    assert!(!link.resolve(1, reply(&request["d"], json!(good))));
    assert!(frames.try_recv().is_err());

    let mut pending = account.provider.sign(work(&identity, GOLDENS[0].0)?);
    let frame = tokio::select! {
        _ = &mut pending => return Err("sign completed without device".into()),
        frame = frames.recv() => frame.ok_or("channel closed")?,
    };
    let request: Value = serde_json::from_str(&frame)?;
    link.heard(1);
    tokio::time::advance(SIGN_TIMEOUT).await;
    let error = pending.await.err().ok_or("timeout signed")?;
    assert_eq!(error, SignError::TimedOut);
    assert!(!link.resolve(1, reply(&request["d"], json!(good))));
    assert!(frames.try_recv().is_err());

    let mut pending = account.provider.sign(work(&identity, GOLDENS[0].0)?);
    let frame = tokio::select! {
        _ = &mut pending => return Err("sign completed without device".into()),
        frame = frames.recv() => frame.ok_or("channel closed")?,
    };
    let request: Value = serde_json::from_str(&frame)?;
    let (sender, mut reattached) = mpsc::unbounded_channel();
    link.suspend();
    link.attach(2, Arc::new(move |frame| sender.send(frame).is_ok()));
    assert_eq!(reattached.recv().await, Some(frame));
    assert!(!link.resolve(1, reply(&request["d"], json!(good))));
    assert!(link.resolve(2, reply(&request["d"], json!(good))));
    let signed = pending.await?;
    assert_eq!(STANDARD.encode(&signed.values[1]), GOLDENS[0].1);
    assert!(reattached.try_recv().is_err());

    let mut pending = account.provider.sign(work(&identity, GOLDENS[0].0)?);
    let frame = tokio::select! {
        _ = &mut pending => return Err("sign completed without device".into()),
        frame = reattached.recv() => frame.ok_or("channel closed")?,
    };
    let request: Value = serde_json::from_str(&frame)?;
    link.mark_unavailable();
    assert_eq!(pending.await.err(), Some(SignError::Unavailable));
    assert!(!link.resolve(2, reply(&request["d"], json!(good))));
    assert_eq!(
        account
            .provider
            .sign(work(&identity, GOLDENS[0].0)?)
            .await
            .err(),
        Some(SignError::Unavailable)
    );
    assert!(frames.try_recv().is_err());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn backpack_native_login_switch_during_device_await_keeps_the_original_owner() -> TestResult {
    fn device_answer(
        signer: &LocalSigner,
        identities: &Identities,
        request: &Value,
    ) -> Result<Value, TestError> {
        assert_eq!(request["type"], "ws-auth");
        assert_eq!(request["exchangeName"], "backpack");
        assert_eq!(request["accountName"], "fixture-account");
        assert_eq!(request["wsAuth"]["outputFormat"], "backpack-ed25519");
        let (graph, output) = hosted_signing::prepare(
            "backpack",
            identities,
            Input::WsAuth {
                to_sign: request["wsAuth"]["toSign"]
                    .as_str()
                    .ok_or("missing descriptor")?
                    .into(),
                output_format: OutputFormat::BackpackEd25519,
            },
        )?;
        let output = hosted_signing::finish(output, &signer.sign_now(&graph)?)?;
        Ok(reply(
            request,
            json!(output.signature.ok_or("missing signature")?),
        ))
    }

    // Changing the login must not change the owner captured by an existing call.
    // Retiring A additionally drops its pending completion, as logout requires.
    for retire_a in [false, true] {
        let (link_a, mut frames_a) = channel();
        let (account_a, identity_a) = setup(link_a.clone())?;
        let (sender_b, mut frames_b) = mpsc::unbounded_channel();
        let link_b = SignerLink::new(2, Arc::new(move |frame| sender_b.send(frame).is_ok()));
        // Deliberately reuse the account name across two users, but not their
        // credentials, opaque refs, device keys, links, sockets, or receipts.
        let account_b = remote_account(
            link_b.clone(),
            "backpack",
            Some("fixture-account".into()),
            r#"{"key":"fixture-key-B"}"#,
        )?;
        let options_b: Value = serde_json::from_str(&account_b.options_json)?;
        let identity_b: Identity = serde_json::from_value(options_b["signing"]["primary"].clone())?;
        assert_ne!(identity_a, identity_b);
        let (device_a, ids_a) = local()?;
        let mut device_b = LocalSigner::new();
        let credential_b = KeyRef("device-B-public".into());
        let signing_b = KeyRef("device-B-owned-ed25519".into());
        device_b.insert(credential_b.clone(), b"fixture-key-B".to_vec());
        device_b.insert_declared(signing_b.clone(), vec![8; 32], Algorithm::Ed25519)?;
        let ids_b = Identities {
            primary: Some(Identity::new(credential_b, Some(signing_b))),
            secondary: None,
        };
        let expected_a = device_a.sign_now(&work(
            ids_a.primary.as_ref().ok_or("missing A identity")?,
            GOLDENS[0].0,
        )?)?;
        let expected_b = device_b.sign_now(&work(
            ids_b.primary.as_ref().ok_or("missing B identity")?,
            GOLDENS[1].0,
        )?)?;
        assert_ne!(expected_a.values, expected_b.values);

        // Model the host's active-login selection using the actual native
        // provider. Its returned future must retain A even when selection moves.
        let mut login = (
            "user-A",
            crate::RemoteProvider {
                inner: account_a.inner.clone().ok_or("missing A owner")?,
            },
        );
        let request_a = work(&identity_a, GOLDENS[0].0)?;
        let receipt_a = request_a.receipt;
        let mut pending_a = login.1.sign(request_a);
        let frame_a = tokio::select! {
            _ = &mut pending_a => return Err("A completed before its device answered".into()),
            frame = frames_a.recv() => frame.ok_or("A channel closed")?,
        };
        let envelope_a: Value = serde_json::from_str(&frame_a)?;
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(
                pending_a.as_mut().poll(cx).is_pending()
            ))
            .await,
            "A is awaiting its device when login changes"
        );
        assert!(frames_b.try_recv().is_err());
        assert_eq!(login.0, "user-A");
        login.0 = "user-B";
        login.1.inner = account_b.inner.clone().ok_or("missing B owner")?;
        if retire_a {
            link_a.mark_unavailable();
        }

        // B must really sign while A is still held, not merely refuse all work.
        let mut request_b = work(&identity_b, GOLDENS[1].0)?;
        request_b.receipt.stamp_ms = Some(1_700_000_001_123.);
        let receipt_b = request_b.receipt;
        let mut pending_b = login.1.sign(request_b);
        let frame_b = tokio::select! {
            _ = &mut pending_b => return Err("B completed before its device answered".into()),
            frame = frames_b.recv() => frame.ok_or("B channel closed")?,
        };
        let envelope_b: Value = serde_json::from_str(&frame_b)?;
        assert_ne!(envelope_a["d"]["id"], envelope_b["d"]["id"]);
        let answer_a = device_answer(&device_a, &ids_a, &envelope_a["d"])?;
        let answer_b = device_answer(&device_b, &ids_b, &envelope_b["d"])?;
        assert!(
            !link_b.resolve(2, answer_a.clone()),
            "A never resolves B's work"
        );
        assert!(
            !link_a.resolve(2, answer_a.clone()),
            "B's socket cannot answer A"
        );
        assert!(link_b.resolve(2, answer_b));
        let signed_b = pending_b.await?;
        assert_eq!(login.0, "user-B");
        assert_eq!(signed_b.values, expected_b.values);
        assert_eq!(signed_b.receipt, receipt_b);
        assert!(signed_b.headers.is_empty());

        if retire_a {
            assert!(
                !link_a.resolve(1, answer_a),
                "retired A drops its late answer"
            );
            assert_eq!(pending_a.await.err(), Some(SignError::Unavailable));
        } else {
            assert!(link_a.resolve(1, answer_a));
            let signed_a = pending_a.await;
            assert!(
                signed_a.is_ok(),
                "A retains its captured owner after B signs"
            );
            let signed_a = signed_a?;
            assert_eq!(
                signed_a.values, expected_a.values,
                "A never adopts B's identity"
            );
            assert_eq!(signed_a.receipt, receipt_a);
            assert!(signed_a.headers.is_empty());
        }
        assert_eq!(
            login.1.sign(work(&identity_a, GOLDENS[0].0)?).await.err(),
            Some(SignError::Refused)
        );
        assert!(frames_a.try_recv().is_err(), "A work is never replayed");
        assert!(
            frames_b.try_recv().is_err(),
            "B sees no reattributed A work"
        );
    }
    Ok(())
}
