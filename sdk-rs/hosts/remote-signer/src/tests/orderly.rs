use super::*;
use crate::provider::{Inner, Owned};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Context, Identities, Identity, Signer};
use safe_cex_venues::hosted_signing::{self, Input};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const ALIASES: [&str; 4] = ["kodiakfi", "mode", "what", "woofi"];
const STAMP: f64 = 1_700_000_000_123.;
fn golden() -> TestResult<Value> {
    Ok(serde_json::from_str(include_str!(
        "../../../../../packages/safe-cex/src/__tests__/fixtures/orderly-device-signing.json"
    ))?)
}
fn text(value: &Value) -> TestResult<&str> {
    value.as_str().ok_or("missing fixture field".into())
}
fn ids() -> Identities {
    Identities {
        primary: Some(Identity::new(
            KeyRef("owned-public".into()),
            Some(KeyRef("owned-signing".into())),
        )),
        secondary: None,
    }
}
fn local() -> TestResult<LocalSigner> {
    let mut signer = LocalSigner::new();
    signer.insert(
        KeyRef("owned-public".into()),
        text(&golden()?["publicKey"])?.as_bytes().to_vec(),
    );
    signer.insert_declared(
        KeyRef("owned-signing".into()),
        vec![7; 32],
        Algorithm::Ed25519,
    )?;
    Ok(signer)
}
fn provider(venue: &str, link: Arc<SignerLink>) -> TestResult<RemoteProvider> {
    Ok(RemoteProvider {
        inner: Arc::new(Inner::new(
            venue.into(),
            policy::policy("binance").ok_or("missing control policy")?,
            Some("main".into()),
            link,
            Owned {
                credential: KeyRef("owned-public".into()),
                signing: KeyRef("owned-signing".into()),
                public: text(&golden()?["publicKey"])?.as_bytes().to_vec(),
                passphrase: None,
                passphrase_value: None,
                broker: None,
            },
        )),
    })
}
fn graph(payload: &str, stamp_ms: f64, ws: bool) -> SignRequest {
    SignRequest {
        receipt: SignReceipt {
            stamp_ms: Some(stamp_ms),
            ..Default::default()
        },
        context: Context {
            venue: "orderly".into(),
            method: if ws { "WS" } else { "HOSTED" }.into(),
            path: if ws { "auth" } else { "" }.into(),
            ..Default::default()
        },
        operations: vec![
            Operation::Present {
                key: KeyRef("owned-public".into()),
            },
            Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: KeyRef("owned-signing".into()),
                payload: vec![bytes(payload)],
            },
        ],
    }
}
fn codec_answer(venue: &str, request: &Value) -> TestResult<Value> {
    if request["exchangeName"] != venue || request["accountName"] != "main" {
        return Err("wrong device route".into());
    }
    let input: Input = serde_json::from_value(
        json!({ "kind":"wsAuth", "toSign":request["wsAuth"]["toSign"],
        "outputFormat":request["wsAuth"]["outputFormat"] }),
    )?;
    let (work, kind) = hosted_signing::prepare(venue, &ids(), input)?;
    let signed = local()?.sign_now(&work)?;
    work.validate_result(&signed)?;
    Ok(
        json!({ "wsAuthSignature": hosted_signing::finish(kind, &signed)?.signature.ok_or("missing signature")? }),
    )
}

#[tokio::test]
async fn orderly_remote_graph_matches_local_through_sign_req_sign_res() -> TestResult {
    for venue in ALIASES {
        let (link, seen) = device(Box::new(move |request| {
            match codec_answer(venue, request) {
                Ok(answer) => answer,
                Err(_) => json!({"error":"synthetic device refusal"}),
            }
        }));
        let remote = provider(venue, link)?;
        assert_eq!(
            remote.can_sign(&KeyRef("owned-signing".into()), Algorithm::Ed25519),
            Capability::Supported
        );
        let fixture = golden()?;
        let vectors = fixture["vectors"].as_array().ok_or("missing vectors")?;
        for row in vectors {
            let work = graph(
                text(&row["payload"])?,
                row["stampMs"].as_f64().ok_or("stamp")?,
                row["kind"] == "wsAuth",
            );
            let control = local()?.sign_now(&work)?;
            let result = remote.sign(work.clone()).await?;
            work.validate_result(&result)?;
            assert!(result.values == control.values && result.receipt == work.receipt);
            assert!(
                base64::engine::general_purpose::URL_SAFE.encode(&result.values[1])
                    == text(&row["signature"])?
            );
        }
        let sent = seen.lock().map_err(|_| "poisoned record")?;
        assert_eq!(sent.len(), vectors.len());
        assert!(
            sent.iter()
                .all(|row| row["exchangeName"] == venue && row["accountName"] == "main")
        );
        assert!(
            sent.iter()
                .all(|row| !row.to_string().contains("owned-signing"))
        );
    }
    Ok(())
}

#[tokio::test]
async fn orderly_context_maps_only_to_bound_alias_and_account() -> TestResult {
    let (link, seen) = device(Box::new(|request| match codec_answer("mode", request) {
        Ok(answer) => answer,
        Err(_) => json!({"error":"wrong account route"}),
    }));
    let remote = provider("mode", link)?;
    remote
        .sign(graph("1700000000123GET/v1/positions", STAMP, false))
        .await?;
    seen.lock().map_err(|_| "poisoned record")?.clear();
    for foreign in ["binance", "woofi", "", "main"] {
        let mut bad = graph("1700000000123GET/v1/positions", STAMP, false);
        bad.context.venue = foreign.into();
        assert!(remote.sign(bad).await.is_err());
    }
    let mut bad = graph("1700000000123GET/v1/positions", STAMP, false);
    bad.operations[1] = Operation::Sign {
        algorithm: Algorithm::Ed25519,
        key: KeyRef("other-account-signing".into()),
        payload: vec![bytes("1700000000123GET/v1/positions")],
    };
    assert!(remote.sign(bad).await.is_err());
    assert!(seen.lock().map_err(|_| "poisoned record")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn orderly_remote_refuses_partial_foreign_oversized_and_noncanonical_replies_before_publish()
-> TestResult {
    let fixture = golden()?;
    let signature = text(&fixture["vectors"][0]["signature"])?;
    let credential = text(&fixture["publicKey"])?;
    let valid = json!({"credential":credential,"signature":signature});
    let cases = vec![
        json!({"credential":credential}).to_string(),
        json!({"signature":signature}).to_string(),
        json!({"credential":"ed25519:foreign","signature":signature}).to_string(),
        json!({"credential":credential,"signature":signature.trim_end_matches('=')}).to_string(),
        json!({"credential":credential,"signature":"A".repeat(88)}).to_string(),
        json!({"credential":credential,"signature":signature,"headers":[]}).to_string(),
        format!(
            "{{\"credential\":{0},\"signature\":{1},\"signature\":{1}}}",
            json!(credential),
            json!(signature)
        ),
        "x".repeat(MAX_RESPONSE_FIELD + 1),
        json!([credential, signature]).to_string(),
    ];
    for reply in cases {
        let (link, seen) = device(Box::new(move |_| json!({"wsAuthSignature":reply})));
        let remote = provider("mode", link)?;
        assert!(
            remote
                .sign(graph("1700000000123GET/v1/positions", STAMP, false))
                .await
                .is_err()
        );
        assert_eq!(seen.lock().map_err(|_| "poisoned record")?.len(), 1);
    }
    let (link, _) = device(Box::new(
        move |_| json!({"wsAuthSignature":valid.to_string()}),
    ));
    provider("mode", link)?
        .sign(graph("1700000000123GET/v1/positions", STAMP, false))
        .await?;
    Ok(())
}

#[tokio::test]
async fn orderly_invalid_graphs_refuse_before_device_io() -> TestResult {
    let (link, seen) = device(Box::new(|_| json!({"error":"unexpected device work"})));
    let remote = provider("mode", link.clone())?;
    for name in [None, Some(String::new()), Some("foreign\nroute".into())] {
        let mut remote = provider("mode", link.clone())?;
        Arc::get_mut(&mut remote.inner)
            .ok_or("shared test provider")?
            .account_name = name;
        assert!(
            remote
                .sign(graph("1700000000123GET/v1/positions", STAMP, false))
                .await
                .is_err()
        );
    }
    let valid = graph("1700000000123GET/v1/positions", STAMP, false);
    let mut cases = Vec::new();
    for count in [0, 1, 3] {
        let mut bad = valid.clone();
        bad.operations.resize(
            count,
            Operation::Present {
                key: KeyRef("owned-public".into()),
            },
        );
        cases.push(bad);
    }
    for receipt in [
        SignReceipt::default(),
        SignReceipt {
            stamp_ms: Some(STAMP + 1.),
            ..Default::default()
        },
        SignReceipt {
            stamp_ms: Some(STAMP),
            expiry_ms: Some(STAMP + 1.),
            nonce: None,
        },
    ] {
        let mut bad = valid.clone();
        bad.receipt = receipt;
        cases.push(bad);
    }
    for payload in [
        "x".repeat(1_048_577),
        "01700000000123GET/v1/positions".into(),
        "1700000000123get/v1/positions".into(),
    ] {
        let mut bad = valid.clone();
        bad.operations[1] = Operation::Sign {
            algorithm: Algorithm::Ed25519,
            key: KeyRef("owned-signing".into()),
            payload: vec![bytes(&payload)],
        };
        cases.push(bad);
    }
    let mut bad = valid.clone();
    bad.operations[0] = Operation::Present {
        key: KeyRef("other-account-public".into()),
    };
    cases.push(bad);
    let mut bad = valid.clone();
    bad.context.method = "DELETE".into();
    bad.context.path = "/v1/order".into();
    cases.push(bad);
    let mut bad = valid;
    bad.operations.swap(0, 1);
    cases.push(bad);
    for bad in cases {
        assert!(remote.sign(bad).await.is_err());
    }
    assert!(seen.lock().map_err(|_| "poisoned record")?.is_empty());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn orderly_timeout_cancellation_disposal_and_unavailable_withdraw_device_work() -> TestResult
{
    for failure in ["timeout", "cancel", "dispose", "unavailable"] {
        let frames = Arc::new(Mutex::new(Vec::<Value>::new()));
        let record = frames.clone();
        let link = SignerLink::new(
            1,
            Arc::new(move |frame| {
                let Ok(frame) = serde_json::from_str::<Value>(&frame) else {
                    return false;
                };
                let Ok(mut frames) = record.lock() else {
                    return false;
                };
                frames.push(frame["d"].clone());
                true
            }),
        );
        if failure == "unavailable" {
            link.mark_unavailable();
        }
        let remote = provider("mode", link.clone())?;
        let mut pending = remote.sign(graph("1700000000123POST/v1/order{}", STAMP, false));
        std::future::poll_fn(|cx| {
            if failure == "unavailable" {
                assert!(matches!(
                    pending.as_mut().poll(cx),
                    std::task::Poll::Ready(Err(SignError::Unavailable))
                ));
            } else {
                assert!(pending.as_mut().poll(cx).is_pending());
            }
            std::task::Poll::Ready(())
        })
        .await;
        let sent = frames.lock().map_err(|_| "poisoned record")?.clone();
        assert_eq!(sent.len(), usize::from(failure != "unavailable"));
        match failure {
            "timeout" => {
                link.heard(1);
                tokio::time::advance(SIGN_TIMEOUT + std::time::Duration::from_millis(1)).await;
                assert!(matches!(pending.await, Err(SignError::TimedOut)));
            }
            "dispose" => {
                link.mark_unavailable();
                assert!(matches!(pending.await, Err(SignError::Unavailable)));
            }
            _ => drop(pending),
        }
        if let Some(sent) = sent.first() {
            let answer = codec_answer("mode", sent)?;
            let mut late = answer;
            late["id"] = sent["id"].clone();
            assert!(!link.resolve(1, late));
        }
        assert_eq!(
            frames.lock().map_err(|_| "poisoned record")?.len(),
            sent.len()
        );
    }
    for venue in ["orderly", "kodiakfi", "mode", "what", "woofi"] {
        let link = SignerLink::new(1, Arc::new(|_| panic!("closed admission reached device")));
        assert!(remote_account(link, venue, Some("main".into()), "{}").is_err());
    }
    Ok(())
}

struct NeverConstruct;
impl safe_cex_venues::Visit for NeverConstruct {
    type Output = ();
    fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {
        panic!("closed private admission visited an engine");
    }
}

#[tokio::test]
async fn orderly_native_registry_refuses_private_injection_before_device_work() -> TestResult {
    for venue in ALIASES {
        let (link, seen) = device(Box::new(move |request| {
            match codec_answer(venue, request) {
                Ok(answer) => answer,
                Err(_) => json!({"error":"synthetic routing refusal"}),
            }
        }));
        let remote = Arc::new(provider(venue, link)?);
        let mut options: safe_cex_venues::EngineOptions = serde_json::from_value(
            json!({"key":golden()?["publicKey"],"applicationId":"account-1", "publicMode":{"requested":false}}),
        )?;
        options.signing = Some(ids());
        let error = safe_cex_venues::build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(remote),
            NeverConstruct,
        )
        .expect_err("private injection admission must stay closed");
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(seen.lock().map_err(|_| "poisoned record")?.is_empty());
    }
    Ok(())
}
