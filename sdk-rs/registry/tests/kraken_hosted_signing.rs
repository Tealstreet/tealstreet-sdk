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

struct PublicStartup;
impl safe_cex_venues::Visit for PublicStartup {
    type Output = ();
    fn visit<V: safe_cex_engine::Venue + 'static>(self, mut engine: safe_cex_engine::Engine<V>) {
        use safe_cex_engine::{Action, Event, Response};
        engine.start(support::NOW);
        let mut instruments = false;
        let mut tickers = false;
        for _ in 0..5 {
            for action in safe_cex_engine::test_support::prepared_actions(&mut engine, support::NOW)
            {
                if let Action::Http(request) = action {
                    assert!(!request.headers.iter().any(|(name, _)| {
                        name.eq_ignore_ascii_case("APIKey") || name.eq_ignore_ascii_case("Authent")
                    }));
                    let body = if request.url.ends_with("/instruments") {
                        instruments = true;
                        support::META
                    } else {
                        assert!(request.url.ends_with("/tickers"), "{}", request.url);
                        tickers = true;
                        support::TICKERS
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
            }
        }
        assert!(instruments, "public startup must fetch instruments");
        assert!(tickers, "public startup must fetch tickers");
    }
}

fn public_startup_with_saved_key(key: &str) {
    let options = serde_json::from_value(serde_json::json!({
        "publicDataOnly": true, "accountStartup": true, "key": key, "secret": ""
    }))
    .unwrap();
    safe_cex_venues::build(
        "kraken",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        PublicStartup,
    )
    .expect("unused saved credentials must not block public startup");
}

#[test]
fn public_startup_with_saved_blank_credentials_reaches_http() {
    public_startup_with_saved_key("");
}

#[test]
fn public_startup_with_saved_key_only_credentials_reaches_http() {
    public_startup_with_saved_key("fixture-key");
}

fn injected_primary_options(public_data_only: bool) -> safe_cex_venues::EngineOptions {
    serde_json::from_value(serde_json::json!({
        "publicDataOnly": public_data_only,
        "signing": Identities {
            primary: Some(Identity::new(
                KeyRef("fixture-public-ref".into()),
                Some(KeyRef("fixture-signing-ref".into())),
            )),
            secondary: None,
        },
    }))
    .unwrap()
}

#[test]
fn public_startup_ignores_unused_primary_refs_without_provider() {
    safe_cex_venues::build_with_signer(
        "kraken",
        injected_primary_options(true),
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        None,
        PublicStartup,
    )
    .expect("public startup must not require an unused primary provider");
}

#[test]
fn private_startup_still_requires_primary_provider() {
    let error = safe_cex_venues::build_with_signer(
        "kraken",
        injected_primary_options(false),
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        None,
        PublicStartup,
    )
    .expect_err("private startup must require its primary provider");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert_eq!(error.message, "Engine signing provider is required");
}

#[test]
fn private_startup_still_refuses_incomplete_saved_credentials() {
    for key in ["", "fixture-key"] {
        let options = serde_json::from_value(serde_json::json!({
            "publicDataOnly": false, "key": key, "secret": ""
        }))
        .unwrap();
        let error = safe_cex_venues::build(
            "kraken",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(5)),
            PublicStartup,
        )
        .expect_err("private startup must validate incomplete saved credentials");
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    }
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
fn concurrent_engines_on_one_key_need_no_stable_id_or_shared_nonce_state() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|seed| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let (signer, mut ids) = custody();
                ids.primary.as_mut().unwrap().stable_id = None;
                let options = serde_json::from_value(serde_json::json!({
                    "signing": ids, "remoteSigning": true, "accountStartup": true
                }))
                .unwrap();
                barrier.wait();
                let actions = safe_cex_venues::build_with_signer(
                    "kraken",
                    options,
                    Some(safe_cex_engine::test_support::seeded_entropy(seed + 5)),
                    Some(std::sync::Arc::new(signer)),
                    Startup,
                )
                .unwrap();
                let mut signed = 0;
                for action in &actions {
                    if let safe_cex_engine::Action::Http(request) = action
                        && request.headers.iter().any(|(name, _)| *name == "Authent")
                    {
                        signed += 1;
                        assert!(!request.headers.iter().any(|(name, _)| *name == "Nonce"));
                        assert!(!request.body.as_deref().unwrap_or("").contains("nonce="));
                    }
                }
                assert!(signed >= 5);
                for expected in [77, 78] {
                    assert!(actions.iter().any(|action| matches!(action,
                        safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandDone {command, ..})
                            if *command == expected)));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
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
                            request
                                .headers
                                .iter()
                                .find(|(name, _)| *name == "Nonce")
                                .map_or("", |(_, value)| value.as_str()),
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
    let private_socket = actions
        .iter()
        .find_map(|action| match action {
            safe_cex_engine::Action::WsSend { socket, text }
                if text.contains("signed_challenge") =>
            {
                Some(*socket)
            }
            _ => None,
        })
        .unwrap();
    let private_messages: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            safe_cex_engine::Action::WsSend { socket, text } if *socket == private_socket => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    let heartbeat = r#"{"event":"subscribe","feed":"heartbeat"}"#;
    assert_eq!(private_messages.first().copied(), Some(heartbeat));
    assert_eq!(
        private_messages
            .iter()
            .filter(|text| **text == heartbeat)
            .count(),
        1
    );
    assert!(actions.iter().any(|action| matches!(
        action,
        safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandDone { command: 77, .. })
    )));
    assert!(actions.iter().any(|action| matches!(
        action,
        safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandDone { command: 78, .. })
    )));
}
