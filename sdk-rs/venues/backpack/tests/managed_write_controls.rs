mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Outcome, VenueError};
use support::managed::*;

fn failure(out: &[Action], command: u32) -> &VenueError {
    let failures = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: own,
                error,
            }) if *own == command => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    failures[0]
}

#[test]
fn raw_credentials_cannot_substitute_for_a_managed_identity() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_backpack::Backpack::new(safe_cex_venue_backpack::Options {
            key: Some("legacy-only-key".into()),
            secret: Some("legacy-only-secret".into()),
            ..Default::default()
        }),
    );
    engine.start(1000.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 720,
            ids: vec![],
        }),
    );
    let out = engine.drain_actions().collect::<Vec<_>>();
    let error = failure(&out, 720);
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::InvalidInput), Some(Outcome::NotSent))
    );
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
    );
}

#[test]
fn required_amend_preparation_refusal_restores_only_unchanged_cancel_state() {
    use safe_cex_engine::request_context::{Error, HttpAction};
    for stream_fill in [false, true] {
        let (mut e, socket) = ready_orders(&format!("[{ORDER}]"));
        e.handle(
            2000.,
            Event::Command(Command::AmendOrder {
                command: 721,
                id: "123".into(),
                price: Some(142.),
                amount: None,
            }),
        );
        assert_eq!(
            e.store().order("123").unwrap().status,
            OrderStatus::Canceling
        );
        let token = e
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        if stream_fill {
            ws(
                &mut e,
                socket,
                "account.orderUpdate",
                r#"{"e":"orderFill","i":"123","E":3000000,"X":"Filled","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"5","l":"5","L":"140","t":999,"f":"GTC"}"#,
            );
        }
        e.http_preparation_failed(
            2000.,
            token,
            Error::Invalid,
            VenueError::new("provider refused"),
        );
        let out = actions(&mut e);
        assert_eq!(failure(&out, 721).outcome, Some(Outcome::NotSent));
        assert_eq!(
            e.store().order("123").unwrap().status,
            if stream_fill {
                OrderStatus::Closed
            } else {
                OrderStatus::Open
            }
        );
        assert!(!out.iter().any(|action| matches!(action, Action::Http(request) if request.method != safe_cex_engine::Method::Get)));
    }
}

#[test]
fn unproven_child_stops_the_plan_and_keeps_native_metadata_and_prior_ids() {
    for status in [408, 503] {
        let (mut e, _) = ready();
        e.handle(2000., Event::Command(Command::PlaceOrders {
            command: 722,
            orders: serde_json::from_str(r#"[{"symbol":"SOL_USDC_PERP","type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}]"#).unwrap(),
        }));
        let entry = requests(&mut e).pop().unwrap();
        reply(&mut e, &entry, 200, r#"{"id":"accepted-entry"}"#);
        let child = requests(&mut e).pop().unwrap();
        reply(
            &mut e,
            &child,
            status,
            r#"{"code":"NATIVE_UNAVAILABLE","message":"native detail"}"#,
        );
        let out = actions(&mut e);
        let error = failure(&out, 722);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.venue_code.as_deref(), Some("NATIVE_UNAVAILABLE"));
        assert_eq!(error.venue_message.as_deref(), Some("native detail"));
        assert_eq!(
            error.partial.as_ref().unwrap().order_ids,
            ["accepted-entry"]
        );
        assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
        reply(&mut e, &child, 200, r#"{"id":"late-child"}"#);
        assert!(!actions(&mut e).iter().any(|action| matches!(
            action,
            Action::Http(_)
                | Action::Notice(
                    Notice::CommandFailed { command: 722, .. }
                        | Notice::CommandDone { command: 722, .. }
                )
        )));
    }
}

struct Remote;
impl safe_cex_engine::signing::Signer for Remote {
    fn sign(
        &self,
        _: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        Box::pin(std::future::pending())
    }
}
const NOW: f64 = 1_700_000_000_123.;
fn remote_actions(engine: &mut Driver) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    engine.prepare_dispatch(NOW);
    let mut queue = std::collections::VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut out = vec![];
    while let Some(action) = queue.pop_front() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(request.headers.is_empty());
                    engine.http_prepared(
                        NOW,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(
                        engine.claim_http(NOW, permit),
                        Ok(Claim::Released(_))
                    ));
                    out.push(Action::Http(request));
                }
                HttpAction::BindGroup { .. } => {
                    panic!("Backpack fixture declares no Mutation barriers")
                }
                HttpAction::Cancel { .. } => {}
            },
            action => out.push(action),
        }
        queue.extend(engine.drain_actions().collect::<Vec<_>>());
    }
    out
}

#[test]
fn real_cancel_uses_the_out_of_band_remote_ed25519_answer() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_engine::signing::{Algorithm, Operation, PayloadPart, SignResult};
    use safe_cex_engine::signing::{Identity, KeyRef};
    let identity = Identity {
        credential: KeyRef("remote-credential".into()),
        signing: KeyRef("remote-signing".into()),
        passphrase: None,
        stable_id: None,
        extra: Default::default(),
    };
    let mut e = identity_only_orders(
        &format!("[{ORDER}]"),
        identity.clone(),
        std::sync::Arc::new(Remote),
        None,
    );
    e.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 723,
            ids: vec!["123".into()],
        }),
    );
    let out = remote_actions(&mut e);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    let (attempt, work) = out
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    assert_eq!(work.receipt.stamp_ms, Some(NOW));
    assert!(
        matches!(&work.operations[0], Operation::Present { key } if key == &identity.credential)
    );
    let Operation::Sign {
        algorithm: Algorithm::Ed25519,
        payload,
        ..
    } = &work.operations[1]
    else {
        panic!("Ed25519 required");
    };
    let bytes = b"instruction=orderCancel&orderId=123&symbol=SOL_USDC_PERP&timestamp=1700000000123&window=60000";
    assert_eq!(
        payload,
        &[PayloadPart::Bytes {
            bytes: bytes.to_vec()
        }]
    );
    // Node crypto.sign(null, the exact preimage above, PKCS8(seed=[9;32])).
    let signature =
        "TmWNCZn0JG0hE4DL7IM6ffnz2PMij9L4ogbidW7PImn39Wy8x8IfH7dMy8sC2IwahHKaAykqeeuSCay9rCtKBw==";
    assert!(
        safe_cex_crypto::ed25519::verify(
            bytes,
            &STANDARD
                .decode("/RckOFqgx1tk+3jNYC+h2ZH96/drE8WO1wLqyDXp9hg=")
                .unwrap(),
            &STANDARD.decode(signature).unwrap()
        )
        .unwrap()
    );
    let result = SignResult::new(
        vec![
            b"remote-only-public-key".to_vec(),
            STANDARD.decode(signature).unwrap(),
        ],
        work.receipt,
    );
    e.handle(
        NOW,
        Event::Signed {
            attempt,
            result: Ok(result.clone()),
        },
    );
    let out = remote_actions(&mut e);
    let requests = out
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request.body.as_deref(),
        Some("{\"orderId\":\"123\",\"symbol\":\"SOL_USDC_PERP\"}")
    );
    assert_eq!(
        request.headers,
        vec![
            ("X-API-Key", "remote-only-public-key".into()),
            ("X-Broker-Id", "1200".into()),
            ("X-Signature", signature.into()),
            ("X-Timestamp", "1700000000123".into()),
            ("X-Window", "60000".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
    e.handle(
        NOW,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    assert!(
        !remote_actions(&mut e)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    e.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    let out = remote_actions(&mut e);
    assert!(out.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 723, result }) if result=="[]")));
    assert_eq!(
        e.store().order("123").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn identity_only_local_write_preserves_the_declared_receive_window() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("provider-only-key".into()),
            secret: Secret::new(STANDARD.encode([7u8; 32])),
            secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    let mut e = identity_only_orders(
        &format!("[{ORDER}]"),
        identities.primary.unwrap(),
        std::sync::Arc::new(provider),
        Some(5000.),
    );
    e.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 724,
            ids: vec!["123".into()],
        }),
    );
    let http = requests(&mut e).pop().unwrap();
    assert!(
        http.headers
            .iter()
            .any(|(key, value)| *key == "X-Window" && value == "5000")
    );
    assert!(
        http.headers
            .iter()
            .any(|(key, value)| *key == "X-API-Key" && value == "provider-only-key")
    );
}

#[test]
fn public_data_only_mode_refuses_even_a_trusted_signing_identity() {
    use safe_cex_engine::signing::{Identity, KeyRef};
    let mut e = identity_only_orders_mode(
        &format!("[{ORDER}]"),
        Identity {
            credential: KeyRef("remote-credential".into()),
            signing: KeyRef("remote-signing".into()),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        },
        std::sync::Arc::new(Remote),
        None,
        true,
    );
    e.handle(
        2000.,
        Event::Command(Command::CancelOrders {
            command: 725,
            ids: vec!["123".into()],
        }),
    );
    let out = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(failure(&out, 725).outcome, Some(Outcome::NotSent));
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
    );
}
