//! Provider dispatch reproduces the native Backpack bytes without venue keys.
use safe_cex_venue_backpack::sign;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::signing::{Identity, KeyEncoding};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Response, SocketId, TimerId, Venue,
};
use std::sync::Arc;

const NOW: f64 = 1_700_000_000_123.0;

struct Issue {
    identity: Identity,
    request: sign::Request,
    tamper: bool,
    route: Option<String>,
}
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn capture_http_reconciliation(
        &self,
        _: &(),
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new(self.request.path.clone())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        let context = cx
            .admit_http_command(Policy {
                operation: if self.request.method == Method::Get {
                    Operation::Read
                } else {
                    Operation::Mutation
                },
                credential: Some(safe_cex_engine::request_context::Credential {
                    slot: safe_cex_engine::CredentialSlot::Primary,
                    reference: self.identity.credential.clone(),
                }),
                timeout_ms: 20000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        let body = if self.tamper {
            Some("tampered".into())
        } else {
            self.request.body.clone()
        };
        let query = if self.request.method == Method::Get {
            sign::query(&self.request.rows[0])
        } else {
            String::new()
        };
        let suffix = if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        };
        cx.request_signed_managed(
            context,
            self.request.method,
            self.route.clone().unwrap_or_else(|| {
                format!("https://api.backpack.exchange{}{suffix}", self.request.path)
            }),
            body,
            Box::new(sign::Http::new(self.identity.clone(), self.request.clone())),
            (),
        )
        .unwrap();
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn provider() -> (Arc<LocalSigner>, Identity) {
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("provider-public-key".into()),
                secret: Secret::new(STANDARD.encode([7u8; 32])),
                secret_encoding: KeyEncoding::Base64,
                ..Default::default()
            }),
            secondary: None,
        },
        safe_cex_engine::signing::CredentialSetupPolicy {
            primary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Ed25519),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    (Arc::new(provider), identities.primary.unwrap())
}

fn engine(request: sign::Request, tamper: bool) -> Engine<Issue> {
    engine_route(request, tamper, None)
}

fn engine_route(request: sign::Request, tamper: bool, route: Option<String>) -> Engine<Issue> {
    let (provider, identity) = provider();
    let command = if request.method == Method::Get {
        Command::FetchLeverage {
            command: 4,
            symbol: "SOL_USDC_PERP".into(),
        }
    } else {
        Command::CancelOrders {
            command: 4,
            ids: vec![],
        }
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(Issue {
        identity,
        request,
        tamper,
        route,
    });
    engine.set_signer(Some(provider)).unwrap();
    engine.start(NOW);
    engine.handle(NOW, Event::Command(command));
    engine
}

fn row(values: &[(&str, &str)]) -> sign::Params {
    values
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}

fn one(request: sign::Request) -> HttpRequest {
    let mut engine = engine(request, false);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
    let mut requests = actions.into_iter().filter_map(|action| match action {
        Action::Http(request) => Some(request),
        _ => None,
    });
    let request = requests.next().expect("provider released request");
    assert!(requests.next().is_none());
    request
}

#[test]
fn dispatch_matches_legacy_single_batch_cancel_and_broker_window_bytes() {
    let legacy =
        sign::Credentials::new("provider-public-key".into(), &STANDARD.encode([7u8; 32])).unwrap();
    let buy = row(&[
        ("symbol", "SOL_USDC_PERP"),
        ("side", "Bid"),
        ("quantity", "1"),
    ]);
    let sell = row(&[
        ("symbol", "SOL_USDC_PERP"),
        ("side", "Ask"),
        ("quantity", "2"),
    ]);
    for (method, path, rows, body) in [
        (
            Method::Post,
            "/api/v1/order",
            vec![buy.clone()],
            "{\"quantity\":1,\"side\":\"Bid\",\"symbol\":\"SOL_USDC_PERP\"}",
        ),
        (
            Method::Post,
            "/api/v1/orders",
            vec![buy, sell],
            "[{\"quantity\":1,\"side\":\"Bid\",\"symbol\":\"SOL_USDC_PERP\"},{\"quantity\":2,\"side\":\"Ask\",\"symbol\":\"SOL_USDC_PERP\"}]",
        ),
        (
            Method::Delete,
            "/api/v1/order",
            vec![row(&[("symbol", "SOL_USDC_PERP"), ("orderId", "123")])],
            "{\"orderId\":\"123\",\"symbol\":\"SOL_USDC_PERP\"}",
        ),
        (
            Method::Delete,
            "/api/v1/orders",
            vec![row(&[("symbol", "SOL_USDC_PERP")])],
            "{\"symbol\":\"SOL_USDC_PERP\"}",
        ),
        (
            Method::Patch,
            "/api/v1/account",
            vec![row(&[("leverageLimit", "5")])],
            "{\"leverageLimit\":\"5\"}",
        ),
    ] {
        for window in [5000, 60000] {
            let mut expected = legacy_for_test();
            expected.window = window;
            let expected_headers =
                expected.headers(sign::instruction(method, path).unwrap(), &rows, NOW);
            let request = one(sign::Request {
                method,
                path: path.into(),
                rows: rows.clone(),
                body: Some(body.into()),
                window,
            });
            assert_eq!(request.headers, expected_headers);
            assert_eq!(request.body.as_deref(), Some(body));
            assert_eq!(request.url, format!("https://api.backpack.exchange{path}"));
        }
    }
    assert_eq!(legacy.window, 60000);
}

fn legacy_for_test() -> sign::Credentials {
    sign::Credentials::new("provider-public-key".into(), &STANDARD.encode([7u8; 32])).unwrap()
}

#[test]
fn a_mismatched_transport_body_is_refused_before_http() {
    let mut engine = engine(
        sign::Request {
            method: Method::Delete,
            path: "/api/v1/order".into(),
            rows: vec![row(&[("orderId", "123")])],
            body: Some("{\"orderId\":\"123\"}".into()),
            window: 60000,
        },
        true,
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
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

fn remote_actions(engine: &mut Engine<Issue>) -> Vec<Action> {
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
                HttpAction::Cancel { .. } => {}
            },
            action => out.push(action),
        }
        queue.extend(engine.drain_actions().collect::<Vec<_>>());
    }
    out
}

#[test]
fn delegated_ed25519_uses_only_the_remote_answer_after_signed_event() {
    use safe_cex_engine::signing::{Algorithm, Operation, PayloadPart, SignResult};
    let (_, identity) = provider();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Issue {
        identity: identity.clone(),
        request: sign::Request {
            method: Method::Delete,
            path: "/api/v1/order".into(),
            rows: vec![row(&[("orderId", "123"), ("symbol", "SOL_USDC_PERP")])],
            body: Some("{\"orderId\":\"123\",\"symbol\":\"SOL_USDC_PERP\"}".into()),
            window: 60000,
        },
        tamper: false,
        route: None,
    });
    engine.set_signer(Some(Arc::new(Remote))).unwrap();
    engine.start(NOW);
    engine.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec![],
        }),
    );
    let actions = remote_actions(&mut engine);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    let (attempt, work) = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .expect("async provider work");
    assert_eq!(work.receipt.stamp_ms, Some(NOW));
    assert_eq!(work.context.venue, "backpack");
    assert_eq!(work.operations.len(), 2);
    assert!(
        matches!(&work.operations[0], Operation::Present { key } if key == &identity.credential)
    );
    let Operation::Sign {
        algorithm: Algorithm::Ed25519,
        key,
        payload,
    } = &work.operations[1]
    else {
        panic!("exact Ed25519 operation required");
    };
    assert_eq!(key, &identity.signing);
    assert_eq!(payload, &[PayloadPart::Bytes {
        bytes: b"instruction=orderCancel&orderId=123&symbol=SOL_USDC_PERP&timestamp=1700000000123&window=60000".to_vec(),
    }]);
    // Node crypto.sign(null, the exact orderCancel preimage above, PKCS8(seed=[9;32])).
    let signature =
        "TmWNCZn0JG0hE4DL7IM6ffnz2PMij9L4ogbidW7PImn39Wy8x8IfH7dMy8sC2IwahHKaAykqeeuSCay9rCtKBw==";
    let PayloadPart::Bytes { bytes } = &payload[0] else {
        unreachable!()
    };
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
    engine.handle(
        NOW,
        Event::Signed {
            attempt,
            result: Ok(result.clone()),
        },
    );
    let actions = remote_actions(&mut engine);
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].headers,
        vec![
            ("X-API-Key", "remote-only-public-key".into()),
            ("X-Broker-Id", "1200".into()),
            ("X-Signature", signature.into()),
            ("X-Timestamp", "1700000000123".into()),
            ("X-Window", "60000".into()),
            ("Content-Type", "application/json".into()),
        ]
    );
    assert_eq!(
        requests[0].body.as_deref(),
        Some("{\"orderId\":\"123\",\"symbol\":\"SOL_USDC_PERP\"}")
    );
    engine.handle(
        NOW,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    assert!(
        !remote_actions(&mut engine)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn get_query_and_transport_route_are_bound_exactly() {
    let params = row(&[("symbol", "SOL_USDC_PERP"), ("limit", "100")]);
    let request = sign::Request {
        method: Method::Get,
        path: "/api/v1/orders".into(),
        rows: vec![params.clone()],
        body: None,
        window: 60000,
    };
    let released = one(request.clone());
    assert_eq!(
        released.url,
        "https://api.backpack.exchange/api/v1/orders?limit=100&symbol=SOL_USDC_PERP"
    );
    assert_eq!(
        released.headers,
        legacy_for_test().headers("orderQueryAll", &[params], NOW)
    );
    assert_eq!(released.body, None);
    for url in [
        "https://other.example/api/v1/orders?limit=100&symbol=SOL_USDC_PERP",
        "https://api.backpack.exchange/extra/api/v1/orders?limit=100&symbol=SOL_USDC_PERP",
    ] {
        let mut engine = engine_route(request.clone(), false, Some(url.into()));
        assert!(
            !safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
    let mut mismatch = request;
    mismatch.method = Method::Delete;
    mismatch.path = "/api/v1/order".into();
    mismatch.body = Some("{\"orderId\":\"other\"}".into());
    let mut engine = engine(mismatch, false);
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn duplicate_body_fields_cannot_disagree_with_the_canonical_instruction() {
    let mut e = engine(
        sign::Request {
            method: Method::Delete,
            path: "/api/v1/order".into(),
            rows: vec![row(&[("orderId", "123"), ("symbol", "SOL_USDC_PERP")])],
            body: Some(r#"{"orderId":"123","symbol":"OTHER","symbol":"SOL_USDC_PERP"}"#.into()),
            window: 60000,
        },
        false,
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
}
