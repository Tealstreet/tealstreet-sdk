#![cfg(feature = "kraken")]

use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

#[path = "../../venues/kraken/tests/support/mod.rs"]
mod support;

fn custody() -> (safe_cex_crypto::signer::LocalSigner, Identities) {
    let mut signer = safe_cex_crypto::signer::LocalSigner::new();
    signer.insert(KeyRef("public".into()), b"fixture-key".to_vec());
    signer.insert(KeyRef("signing".into()), b"secret".to_vec());
    let mut identity = Identity::new(KeyRef("public".into()), Some(KeyRef("signing".into())));
    identity.stable_id = Some("kraken-hosted-fixture".into());
    (
        signer,
        Identities {
            primary: Some(identity),
            secondary: None,
        },
    )
}

#[test]
fn device_resolves_sha256_then_sha512_with_its_owned_material() {
    let (signer, ids) = custody();
    let format: OutputFormat = serde_json::from_str("\"kraken-sha512\"").unwrap();
    let (work, kind) = hosted_signing::prepare(
        "kraken",
        &ids,
        Input::WsAuth {
            to_sign: r#"{"slot":"primary","payload":"challenge-æ-123"}"#.into(),
            output_format: format,
        },
    )
    .unwrap();
    let result = signer.sign_now(&work).unwrap();
    assert_eq!(result.values[1].len(), 32);
    assert_eq!(result.values[2].len(), 64);
    let output = hosted_signing::finish(kind, &result).unwrap();
    let batch: serde_json::Value =
        serde_json::from_str(output.signature.as_deref().unwrap()).unwrap();
    assert_eq!(batch["credential"], "fixture-key");
    assert_eq!(
        batch["signature"],
        "RrIo22Xn634Pt3sq0W+1h/7OUdrAgbSG4qt+qaycGFEledqV4+k70KrF0smVdYMou8BVj/wi056PrIXGJfjRtQ=="
    );
}

#[test]
fn device_refuses_unknown_fields_missing_slots_wrong_formats_and_bad_completions() {
    let (signer, ids) = custody();
    for payload in [
        r#"{"payload":"challenge","unknown":true}"#,
        r#"{"payload":""}"#,
        r#"{"slot":"secondary","payload":"challenge"}"#,
    ] {
        assert!(
            hosted_signing::prepare(
                "kraken",
                &ids,
                Input::WsAuth {
                    to_sign: payload.into(),
                    output_format: serde_json::from_str("\"kraken-sha512\"").unwrap()
                }
            )
            .is_err()
        );
    }
    assert!(
        hosted_signing::prepare(
            "kraken",
            &ids,
            Input::WsAuth {
                to_sign: "challenge".into(),
                output_format: OutputFormat::Base64
            }
        )
        .is_err()
    );
    let (work, kind) = hosted_signing::prepare(
        "kraken",
        &ids,
        Input::WsAuth {
            to_sign: "challenge-æ-123".into(),
            output_format: serde_json::from_str("\"kraken-challenge\"").unwrap(),
        },
    )
    .unwrap();
    let mut result = signer.sign_now(&work).unwrap();
    assert_eq!(
        hosted_signing::finish(kind, &result)
            .unwrap()
            .signature
            .unwrap(),
        "RrIo22Xn634Pt3sq0W+1h/7OUdrAgbSG4qt+qaycGFEledqV4+k70KrF0smVdYMou8BVj/wi056PrIXGJfjRtQ=="
    );
    result.values[2].pop();
    assert!(hosted_signing::finish(kind, &result).is_err());
}

#[test]
fn missing_nonce_identity_refuses_before_startup() {
    let (signer, mut ids) = custody();
    ids.primary.as_mut().unwrap().stable_id = None;
    let options = serde_json::from_value(
        serde_json::json!({"signing": ids, "remoteSigning": true, "accountStartup": true}),
    )
    .unwrap();
    let error = safe_cex_venues::build_with_signer(
        "kraken",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Some(std::sync::Arc::new(signer)),
        Startup,
    )
    .err()
    .unwrap();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert!(error.message.contains("stable signing identities"));
}

struct Startup;
impl safe_cex_venues::Visit for Startup {
    type Output = Vec<safe_cex_engine::Action>;
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        mut engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        use safe_cex_engine::{Action, Event, Response};
        engine.start(support::NOW);
        let mut seen = vec![];
        for _ in 0..20 {
            let actions =
                safe_cex_engine::test_support::prepared_actions(&mut engine, support::NOW);
            for action in actions {
                match &action {
                    Action::Http(request) => {
                        let body = if request.url.contains("/instruments") {
                            support::META
                        } else if request.url.contains("/tickers") {
                            support::TICKERS
                        } else if request.url.contains("/accounts") {
                            support::ACCOUNT
                        } else if request.url.contains("/openorders") {
                            support::OPEN
                        } else if request.url.contains("/openpositions") {
                            r#"{"result":"success","openPositions":[]}"#
                        } else {
                            r#"{"result":"success","leveragePreferences":[]}"#
                        };
                        engine.handle(
                            support::NOW,
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
                    Action::WsConnect { socket, .. } => {
                        engine.handle(support::NOW, Event::WsOpened { socket: *socket })
                    }
                    Action::WsSend { socket, text } if text.contains("\"challenge\"") => {
                        engine.handle(
                            support::NOW,
                            Event::WsMessage {
                                socket: *socket,
                                text: r#"{"event":"challenge","message":"challenge-æ-123"}"#,
                            },
                        );
                    }
                    _ => {}
                }
                seen.push(action);
            }
        }
        let order = serde_json::from_str(
            r#"{"symbol":"PF_XBTUSD","type":"limit","side":"buy","amount":0.1,"price":100}"#,
        )
        .unwrap();
        engine.handle(
            support::NOW,
            Event::Command(safe_cex_engine::Command::PlaceOrders {
                command: 77,
                orders: vec![order],
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, support::NOW);
        assert!(actions.iter().any(|action| matches!(action, Action::Http(request) if request.url.ends_with("/sendorder"))), "placement diagnostics: {:?}", actions.iter().map(|action| match action { Action::Notice(notice) => format!("{notice:?}"), Action::Sign { .. } => "sign".into(), Action::SignedOutbound { .. } => "signed outbound".into(), Action::SetTimer { at_ms, .. } => format!("timer {at_ms}"), _ => "other".into() }).collect::<Vec<_>>());
        for action in actions {
            if let Action::Http(request) = &action {
                assert!(request.url.ends_with("/sendorder"));
                let header = |name| {
                    request
                        .headers
                        .iter()
                        .find(|(key, _)| *key == name)
                        .unwrap()
                        .1
                        .as_str()
                };
                let control = safe_cex_venue_kraken::sign::Signer::new("c2VjcmV0").unwrap();
                assert_eq!(
                    header("Authent"),
                    control
                        .signature(
                            request.body.as_deref().unwrap(),
                            header("Nonce"),
                            "/derivatives/api/v3/sendorder"
                        )
                        .unwrap()
                );
                engine.handle(support::NOW, Event::HttpResponse { request: request.id, response: Response { status: 200, headers: "",
                    body: r#"{"result":"success","sendStatus":{"order_id":"fixture-order","status":"placed"}}"# } });
            }
            seen.push(action);
        }
        seen.extend(safe_cex_engine::test_support::prepared_actions(
            &mut engine,
            support::NOW,
        ));
        engine.handle(
            support::NOW,
            Event::Command(safe_cex_engine::Command::Query {
                command: 78,
                query: safe_cex_engine::Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: Some(support::NOW - 60_000.),
                    to_ms: Some(support::NOW),
                    limit: None,
                },
            }),
        );
        let reads = safe_cex_engine::test_support::prepared_actions(&mut engine, support::NOW);
        assert!(reads.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("/fills"))
        ));
        for action in reads {
            if let Action::Http(request) = &action {
                engine.handle(
                    support::NOW,
                    Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: r#"{"result":"success","fills":[]}"#,
                        },
                    },
                );
            }
            seen.push(action);
        }
        seen.extend(safe_cex_engine::test_support::prepared_actions(
            &mut engine,
            support::NOW,
        ));
        seen
    }
}

#[test]
fn injected_custody_starts_private_rest_and_signed_ws_without_any_raw_key() {
    let (signer, ids) = custody();
    let options = serde_json::from_value(
        serde_json::json!({"signing": ids, "remoteSigning": true, "accountStartup": true}),
    )
    .unwrap();
    let actions = safe_cex_venues::build_with_signer(
        "kraken",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        Some(std::sync::Arc::new(signer)),
        Startup,
    )
    .unwrap();
    assert!(actions.iter().any(|action| matches!(action, safe_cex_engine::Action::Http(request)
        if request.url.contains("/accounts") && request.headers.iter().any(|(name, _)| *name == "Authent"))));
    assert!(actions.iter().any(|action| matches!(action, safe_cex_engine::Action::WsSend { text, .. }
        if text.contains("signed_challenge") && text.contains("RrIo22Xn634Pt3sq0W+1h/7OUdrAgbSG4qt+qaycGFEledqV4+k70KrF0smVdYMou8BVj/wi056PrIXGJfjRtQ=="))));
    assert!(actions.iter().any(|action| matches!(
        action,
        safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandDone { command: 77, .. })
    )));
    assert!(actions.iter().any(|action| matches!(
        action,
        safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandDone { command: 78, .. })
    )));
}
