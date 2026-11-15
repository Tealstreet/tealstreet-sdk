#![cfg(feature = "backpack")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

#[test]
fn backpack_private_capsule_refuses_bad_seed_width_before_startup() {
    for seed in ["BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBw==", "not base64"] {
        let input =
            serde_json::json!({"primary": {"key": "fixture-key", "secret": seed}}).to_string();
        assert!(safe_cex_venues::local_signer_from_json_for("backpack", input).is_err());
    }
}

#[test]
fn backpack_device_resolves_ed25519_graph_atomically() {
    let mut signer = LocalSigner::new();
    signer.insert(KeyRef("public".into()), b"fixture-key".to_vec());
    signer
        .insert_declared(
            KeyRef("seed".into()),
            vec![7; 32],
            safe_cex_engine::signing::Algorithm::Ed25519,
        )
        .unwrap();
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("public".into()),
            Some(KeyRef("seed".into())),
        )),
        secondary: None,
    };
    let format: OutputFormat = serde_json::from_str("\"backpack-ed25519\"").unwrap();
    let (work, kind) = hosted_signing::prepare("backpack", &ids, Input::WsAuth {
        to_sign: r#"{"slot":"primary","payload":"instruction=subscribe&timestamp=1700000000123&window=5000"}"#.into(),
        output_format: format,
    }).unwrap();
    let result = signer.sign_now(&work).unwrap();
    let output = hosted_signing::finish(kind, &result).unwrap();
    let output: serde_json::Value =
        serde_json::from_str(output.signature.as_deref().unwrap()).unwrap();
    assert_eq!(output["credential"], "fixture-key");
    assert_eq!(
        output["signature"],
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Base64)
    );
    assert_eq!(result.values[1].len(), 64);
}

struct Startup;
impl safe_cex_venues::Visit for Startup {
    type Output = Vec<safe_cex_engine::Action>;
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        mut engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        use safe_cex_engine::{Action, Event, Response};
        engine.start(1_700_000_000_123.);
        let mut seen = vec![];
        for _ in 0..8 {
            let actions =
                safe_cex_engine::test_support::prepared_actions(&mut engine, 1_700_000_000_123.);
            for action in actions {
                if let Action::Http(request) = &action {
                    let body = if request.url.ends_with("/markets") {
                        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#
                    } else if request.url.ends_with("/account") {
                        r#"{"leverageLimit":"10"}"#
                    } else if request.url.ends_with("/collateral") {
                        r#"{"netEquity":"1005","netEquityAvailable":"905","netEquityLocked":"100"}"#
                    } else {
                        "[]"
                    };
                    engine.handle(
                        1_700_000_000_123.,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                if let Action::WsConnect { socket, .. } = &action {
                    engine.handle(1_700_000_000_123., Event::WsOpened { socket: *socket });
                }
                seen.push(action);
            }
        }
        seen
    }
}

#[test]
fn injected_backpack_starts_private_reads_and_ws_without_raw_credentials() {
    use safe_cex_engine::{Action, signing::Algorithm};
    let mut signer = LocalSigner::new();
    signer.insert(KeyRef("public".into()), b"fixture-key".to_vec());
    signer
        .insert_declared(KeyRef("seed".into()), vec![7; 32], Algorithm::Ed25519)
        .unwrap();
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("public".into()),
            Some(KeyRef("seed".into())),
        )),
        secondary: None,
    };
    let options =
        serde_json::from_value(serde_json::json!({"signing": ids, "remoteSigning": true})).unwrap();
    let actions = safe_cex_venues::build_with_signer(
        "backpack",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Some(std::sync::Arc::new(signer)),
        Startup,
    )
    .unwrap();
    assert!(actions.iter().any(|action| matches!(action, Action::Http(request)
        if request.url.ends_with("/api/v1/account") && request.headers.iter().any(|(name, _)| *name == "X-Signature"))));
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::WsSend { text, .. }
        if text.contains("account.orderUpdate") && text.contains("signature")))
    );
}

#[test]
fn backpack_capture_retires_clock_skew_and_bad_completions_before_fresh_signing() {
    use safe_cex_engine::{signing::Algorithm, signing_capture::Capture};
    let mut signer = LocalSigner::new();
    signer.insert(KeyRef("public".into()), b"fixture-key".to_vec());
    signer
        .insert_declared(KeyRef("seed".into()), vec![7; 32], Algorithm::Ed25519)
        .unwrap();
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("public".into()),
            Some(KeyRef("seed".into())),
        )),
        secondary: None,
    };
    let stamp = 1_700_000_000_123.;
    let input = |stamp: f64| Input::Http {
        payload: format!(
            r#"{{"slot":"primary","payload":"instruction=accountQuery&timestamp={}&window=60000"}}"#,
            stamp as u64
        ),
        stamp_ms: stamp,
        revision: "0".into(),
    };
    let mut capture = Capture::default();
    let (work, kind) = hosted_signing::prepare("backpack", &ids, input(stamp)).unwrap();
    let first = capture.prepare(work, kind, 0., stamp, 0, 5000).unwrap();
    let old = signer.sign_now(&first.work).unwrap();
    let old_values = old.values.clone();
    assert!(
        capture
            .finish(first.id, 1., stamp + 1000., 1, Ok(old))
            .is_err()
    );
    assert!(capture.is_empty());
    let (work, kind) = hosted_signing::prepare("backpack", &ids, input(stamp + 1000.)).unwrap();
    let fresh = capture
        .prepare(work, kind, 1., stamp + 1000., 1, 5000)
        .unwrap();
    let signed = signer.sign_now(&fresh.work).unwrap();
    assert_ne!(
        old_values[1], signed.values[1],
        "clock correction must re-sign"
    );
    let (kind, signed) = capture
        .finish(fresh.id, 2., stamp + 1000., 1, Ok(signed))
        .unwrap();
    hosted_signing::finish(kind, &signed).unwrap();
    assert!(capture.is_empty());
    for malformed in [false, true] {
        let (work, kind) = hosted_signing::prepare("backpack", &ids, input(stamp)).unwrap();
        let task = capture.prepare(work, kind, 0., stamp, 0, 5000).unwrap();
        let mut signed = signer.sign_now(&task.work).unwrap();
        if malformed {
            signed.values[1].pop();
        }
        assert!(
            capture
                .finish(
                    task.id,
                    if malformed { 1. } else { 5000. },
                    stamp,
                    0,
                    Ok(signed)
                )
                .is_err()
        );
        assert!(capture.is_empty());
    }
    for descriptor in [
        r#"{"payload":"p","unknown":true}"#,
        r#"{"payload":"p","payload":"q"}"#,
        r#"{"payload":""}"#,
        r#"{"payload":"p","slot":"secondary"}"#,
        r#"{"payload":"p","slot":"foreign"}"#,
    ] {
        assert!(
            hosted_signing::prepare(
                "backpack",
                &ids,
                Input::Http {
                    payload: descriptor.into(),
                    stamp_ms: stamp,
                    revision: "0".into()
                }
            )
            .is_err()
        );
    }
    assert!(
        hosted_signing::prepare(
            "backpack",
            &ids,
            Input::WsAuth {
                to_sign: "raw".into(),
                output_format: OutputFormat::Base64
            }
        )
        .is_err()
    );
    assert!(hosted_signing::prepare("unknown", &ids, input(stamp)).is_err());
}
