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

async fn native_actions<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
) -> TestResult<Vec<safe_cex_engine::Action>> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    use safe_cex_engine::{Action, Event};
    let mut out = Vec::new();
    for _ in 0..128 {
        engine.prepare_dispatch(STAMP);
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return Ok(out);
        }
        for action in actions {
            match action {
                Action::Sign { attempt, work, .. } => {
                    assert!(engine.signing_pending(attempt));
                    let result = engine
                        .signer()
                        .ok_or("missing native provider")?
                        .sign(work)
                        .await;
                    engine.handle(STAMP, Event::Signed { attempt, result });
                }
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => engine.http_prepared(
                        STAMP,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    ),
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(matches!(
                            engine.claim_http(STAMP, permit),
                            Ok(Claim::Released(_))
                        ));
                        out.push(Action::Http(request));
                    }
                    HttpAction::Cancel { .. } => {}
                    HttpAction::BindGroup { .. } => {
                        return Err("unexpected grouped Orderly read".into());
                    }
                },
                action => out.push(action),
            }
        }
    }
    Err("native fixture did not quiesce".into())
}
struct Native {
    refuse: Arc<std::sync::atomic::AtomicBool>,
    fail: bool,
}
impl safe_cex_venues::Visit for Native {
    type Output = std::pin::Pin<Box<dyn std::future::Future<Output = TestResult>>>;
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        mut engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        Box::pin(async move {
            use safe_cex_engine::{Action, Command, Event, Method, Notice, Outcome, Response};
            engine.start(STAMP);
            let mut private = None;
            let mut reads = 0;
            for _ in 0..12 {
                let actions = native_actions(&mut engine).await?;
                if actions.is_empty() {
                    break;
                }
                for action in actions {
                    match action {
                        Action::Http(http) => {
                            if http
                                .headers
                                .iter()
                                .any(|(key, _)| *key == "orderly-signature")
                            {
                                reads += 1;
                            }
                            let body = if http.url.ends_with("/v1/client/info") {
                                r#"{"success":true,"data":{"max_leverage":20}}"#
                            } else if http.url.ends_with("/v1/public/info") {
                                r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01}]}}"#
                            } else if http.url.ends_with("/v1/public/futures") {
                                r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","24h_open":50000,"24h_close":51000,"mark_price":51000}]}}"#
                            } else if http.url.ends_with("/v1/positions") {
                                r#"{"success":true,"data":{"rows":[],"total_collateral_value":10000,"free_collateral":7000}}"#
                            } else {
                                r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#
                            };
                            engine.handle(
                                STAMP,
                                Event::HttpResponse {
                                    request: http.id,
                                    response: Response {
                                        status: 200,
                                        headers: "",
                                        body,
                                    },
                                },
                            );
                        }
                        Action::WsConnect { socket, url, .. } if url.contains("private") => {
                            private = Some(socket)
                        }
                        _ => {}
                    }
                }
            }
            assert!(reads >= 4);
            self.refuse
                .store(self.fail, std::sync::atomic::Ordering::SeqCst);
            let socket = private.ok_or("missing private socket")?;
            engine.handle(STAMP, Event::WsOpened { socket });
            let auth = native_actions(&mut engine).await?;
            assert_eq!(auth.iter().any(|action| matches!(action, Action::WsSend { text, .. } if text.contains("orderly_key"))), !self.fail);
            engine.handle(
                STAMP,
                Event::WsMessage {
                    socket,
                    text: r#"{"event":"auth","success":true}"#,
                },
            );
            native_actions(&mut engine).await?;
            engine.handle(STAMP, Event::Command(Command::PlaceOrders { command:71, orders:vec![serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":0.01,"price":50000}))?] }));
            let actions = native_actions(&mut engine).await?;
            let writes: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Http(http) if http.method != Method::Get => Some(http),
                    _ => None,
                })
                .collect();
            if self.fail {
                assert!(writes.is_empty());
                assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command:71, error }) if error.outcome == Some(Outcome::NotSent))));
            } else {
                assert_eq!(writes.len(), 1);
                assert!(
                    writes[0]
                        .headers
                        .iter()
                        .any(|(name, _)| *name == "orderly-signature")
                );
                engine.handle(
                    STAMP,
                    Event::HttpFailed {
                        request: writes[0].id,
                        error: "synthetic lost response",
                    },
                );
                let after = native_actions(&mut engine).await?;
                assert!(after.iter().all(
                    |action| !matches!(action, Action::Http(http) if http.method != Method::Get)
                ));
                assert!(after.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command:71, error }) if error.outcome == Some(Outcome::Unknown))));
            }
            Ok(())
        })
    }
}
#[tokio::test]
async fn orderly_native_registry_provider_device_release_and_unknown_write_never_replay()
-> TestResult {
    for venue in ALIASES {
        for fail in [false, true] {
            let refuse = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let control = refuse.clone();
            let (link, seen) = device(Box::new(move |request| {
                if control.load(std::sync::atomic::Ordering::SeqCst) {
                    return json!({"error":"synthetic refusal"});
                }
                match codec_answer(venue, request) {
                    Ok(answer) => answer,
                    Err(_) => json!({"error":"synthetic routing refusal"}),
                }
            }));
            let remote = Arc::new(provider(venue, link)?);
            let mut options: safe_cex_venues::EngineOptions = serde_json::from_value(
                json!({"key":golden()?["publicKey"],"applicationId":"account-1"}),
            )?;
            options.signing = Some(ids());
            safe_cex_venues::build_with_signer(
                venue,
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Some(remote),
                Native { refuse, fail },
            )?
            .await?;
            let sent = seen.lock().map_err(|_| "poisoned record")?;
            assert!(
                sent.iter()
                    .all(|row| row["exchangeName"] == venue && row["accountName"] == "main")
            );
            let descriptors: Vec<Value> = sent
                .iter()
                .map(|row| {
                    serde_json::from_str(text(&row["wsAuth"]["toSign"])?).map_err(Into::into)
                })
                .collect::<TestResult<_>>()?;
            assert_eq!(
                descriptors
                    .iter()
                    .filter(|row| row["kind"] == "wsAuth")
                    .count(),
                1
            );
            assert_eq!(
                descriptors
                    .iter()
                    .filter(|row| row["payload"]
                        .as_str()
                        .is_some_and(|payload| payload.contains("POST/v1/order")))
                    .count(),
                1
            );
        }
    }
    Ok(())
}
