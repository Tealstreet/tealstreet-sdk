//! Dormant codec exercised through the real dispatcher, before native adoption.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Credential, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, Identity, KeyRef, SignError, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_engine::test_support::{prepared_actions, seeded_engine};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, Method, Response,
    SignStep, SocketId, TimerId, Venue,
};
use safe_cex_venue_kucoin::sign::{self, dispatch};
use std::sync::{Arc, Mutex};

const NOW: f64 = 1_700_000_000_123.;

struct Provider {
    inner: LocalSigner,
    corrupt: u8,
    held: bool,
    seen: Arc<Mutex<Vec<SignRequest>>>,
}
impl Signer for Provider {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.inner.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.seen.lock().unwrap().push(request.clone());
        if self.held {
            return None;
        }
        Some(self.inner.sign_now(request).map(|mut result| {
            match self.corrupt {
                1 => result.receipt.stamp_ms = Some(NOW + 1.),
                2 => result.headers.push(("injected".into(), "value".into())),
                3 => {
                    result.values.pop();
                }
                4 => result.values.push(vec![0; 32]),
                5 => {
                    result.values[1].pop();
                }
                6 => result.values[0] = b"bad\r\nkey".to_vec(),
                7 => result.values[0] = vec![0xff],
                8 => result.values[0].clear(),
                _ => {}
            }
            result
        }))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}

enum Delivery {
    Immediate(u8),
    Held,
}

struct ChangedTransport {
    inner: Box<dyn SignStep>,
    mode: u8,
    after_prepare: bool,
}
impl ChangedTransport {
    fn change(&self, outbound: &mut Action) {
        let Action::Http(http) = outbound else {
            panic!("HTTP recipe")
        };
        match self.mode {
            0 => http.url.push_str("/changed"),
            1 => http.method = Method::Delete,
            2 => http.body = Some("changed".into()),
            3 => http.headers.push(("extra", "changed".into())),
            _ => http
                .signer_headers
                .push(("injected".into(), "changed".into())),
        }
    }
}
impl SignStep for ChangedTransport {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        self.inner.unsigned_http_headers()
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if !self.after_prepare {
            self.change(outbound);
        }
        self.inner.prepare(outbound, context)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if self.after_prepare {
            self.change(outbound);
        }
        self.inner.apply(outbound, result)
    }
    fn freshness_ms(&self) -> f64 {
        self.inner.freshness_ms()
    }
}

struct Issue {
    step: Option<Box<dyn SignStep>>,
    credential: Credential,
    method: Method,
    url: String,
    body: Option<String>,
}
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: Some(self.credential.clone()),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.request_signed_managed(
            context,
            None,
            self.method,
            self.url.clone(),
            self.body.clone(),
            self.step.take().unwrap(),
            (),
        )
        .unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn setup(broker: bool) -> (LocalSigner, Identity, Identity) {
    let policy: CredentialSetupPolicy = serde_json::from_value(serde_json::json!({
        "primary": {"passphrase":"requiredForPrivate", "extra": {
            "key":"kucoinBroker", "encoding":"utf8", "trim":true}},
        "secondary": {"passphrase":"requiredWithPrimaryFallback", "extra": {
            "key":"kucoinBroker", "field":"aliasPrimary"}}
    }))
    .unwrap();
    let mut input = serde_json::json!({
        "primary": {"key":"replay-key", "secret":"replay-secret", "passphrase":"passphrase!@#"},
        "secondary": {"key":"read-key", "secret":"read-secret"}
    });
    if broker {
        input["primary"]["extra"] =
            serde_json::json!({"kucoinBroker":{"value":" broker-test-key "}});
    }
    let (provider, ids) = LocalSigner::from_json_with_policy(input.to_string(), policy).unwrap();
    (provider, ids.primary.unwrap(), ids.secondary.unwrap())
}

fn issue(
    origin: &'static str,
    method: Method,
    path: &str,
    body: Option<&str>,
    broker: bool,
    secondary: bool,
    delivery: Delivery,
) -> (Engine<Issue>, Arc<Provider>) {
    let (corrupt, held) = match delivery {
        Delivery::Immediate(corrupt) => (corrupt, false),
        Delivery::Held => (0, true),
    };
    let (inner, primary, read) = setup(broker);
    let credential = Credential {
        slot: if secondary {
            CredentialSlot::Secondary
        } else {
            CredentialSlot::Primary
        },
        reference: if secondary { &read } else { &primary }.credential.clone(),
    };
    let step = dispatch::Http::new(
        if secondary { read } else { primary },
        method,
        origin,
        path.into(),
        body.map(Into::into),
        broker.then(|| dispatch::Broker {
            partner: " partner-test ".into(),
            name: " Broker Name ".into(),
            verify: false,
        }),
    );
    let provider = Arc::new(Provider {
        inner,
        corrupt,
        held,
        seen: Arc::default(),
    });
    let mut engine = seeded_engine(Issue {
        step: Some(Box::new(step)),
        credential,
        method,
        url: format!(
            "{origin}{}",
            path.replace('!', "%21")
                .replace('@', "%40")
                .replace('#', "%23")
        ),
        body: body.map(Into::into),
    });
    engine.set_signer(Some(provider.clone())).unwrap();
    (engine, provider)
}

#[test]
fn both_origins_and_slots_match_the_existing_ordered_native_headers() {
    for origin in ["https://api-futures.kucoin.com", "https://api.kucoin.com"] {
        for secondary in [false, true] {
            for broker in [false, true] {
                for (method, path, body) in [
                    (
                        Method::Get,
                        "/api/v1/orders?symbol=XBTUSDTM&clientOid=hello!@#",
                        None,
                    ),
                    (
                        Method::Post,
                        "/api/v1/orders",
                        Some(r#"{"symbol":"XBTUSDTM","size":2}"#),
                    ),
                    (Method::Delete, "/api/v1/orders/42", None),
                ] {
                    let (mut engine, provider) = issue(
                        origin,
                        method,
                        path,
                        body,
                        broker,
                        secondary,
                        Delivery::Immediate(0),
                    );
                    engine.start(NOW + 0.9);
                    let actions = prepared_actions(&mut engine, NOW + 0.9);
                    let https: Vec<_> = actions
                        .iter()
                        .filter_map(|a| match a {
                            Action::Http(http) => Some(http),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(https.len(), 1, "{actions:?}");
                    let credentials = sign::Credentials {
                        key: if secondary { "read-key" } else { "replay-key" }.into(),
                        secret: if secondary {
                            "read-secret"
                        } else {
                            "replay-secret"
                        }
                        .into(),
                        passphrase: "passphrase!@#".into(),
                        broker: broker.then(|| sign::BrokerOptions {
                            partner: " partner-test ".into(),
                            broker_key: " broker-test-key ".into(),
                            broker_name: " Broker Name ".into(),
                            verify: Some(false),
                        }),
                    };
                    assert_eq!(
                        https[0].headers,
                        sign::headers(&credentials, method, path, body.unwrap_or(""), NOW)
                    );
                    let seen = provider.seen.lock().unwrap();
                    assert_eq!(seen.len(), 1);
                    assert_eq!(seen[0].operations.len(), if broker { 4 } else { 3 });
                    assert_eq!(seen[0].receipt.stamp_ms, Some(NOW));
                    assert_eq!(seen[0].receipt.nonce, None);
                }
            }
        }
    }
}

#[test]
fn independent_request_and_passphrase_vectors_are_unchanged() {
    let (mut engine, _) = issue(
        "https://api-futures.kucoin.com",
        Method::Get,
        "/api/v1/orders?symbol=XBTUSDTM&clientOid=hello!@#",
        None,
        false,
        false,
        Delivery::Immediate(0),
    );
    engine.start(NOW);
    let actions = prepared_actions(&mut engine, NOW);
    let http = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .unwrap();
    for (name, expected) in [
        (
            "KC-API-SIGN",
            "pxv5Qv3DGyKlZAA4GNIcY8RVnp7FtaZmSmJx28Us0Ac=",
        ),
        (
            "KC-API-PASSPHRASE",
            "EjiYZdaPT/Nrmf+GmwgMlSoUxuCsxdZclZcIu/0i9Co=",
        ),
    ] {
        assert!(http.headers.contains(&(name, expected.into())));
    }
}

#[test]
fn malformed_atomic_results_never_release_partial_headers() {
    for corrupt in 1..=8 {
        let (mut engine, _) = issue(
            "https://api.kucoin.com",
            Method::Post,
            "/api/v1/bullet-private",
            None,
            true,
            false,
            Delivery::Immediate(corrupt),
        );
        engine.start(NOW);
        assert!(
            !prepared_actions(&mut engine, NOW)
                .iter()
                .any(|a| matches!(a, Action::Http(_)))
        );
    }
}

#[test]
fn changed_transport_is_refused_before_signing_or_binding() {
    for after_prepare in [false, true] {
        for mode in 0..5 {
            let (inner, primary, _) = setup(false);
            let provider = Arc::new(Provider {
                inner,
                corrupt: 0,
                held: false,
                seen: Arc::default(),
            });
            let step = dispatch::Http::new(
                primary.clone(),
                Method::Get,
                "https://api.kucoin.com",
                "/api/v1/orders".into(),
                None,
                None,
            );
            let mut engine = seeded_engine(Issue {
                credential: Credential {
                    slot: CredentialSlot::Primary,
                    reference: primary.credential.clone(),
                },
                step: Some(Box::new(ChangedTransport {
                    inner: Box::new(step),
                    mode,
                    after_prepare,
                })),
                method: Method::Get,
                url: "https://api.kucoin.com/api/v1/orders".into(),
                body: None,
            });
            engine.set_signer(Some(provider.clone())).unwrap();
            engine.start(NOW);
            assert!(
                !prepared_actions(&mut engine, NOW)
                    .iter()
                    .any(|a| matches!(a, Action::Http(_)))
            );
            assert_eq!(
                provider.seen.lock().unwrap().len(),
                usize::from(after_prepare)
            );
        }
    }
}

#[test]
fn deadline_retirement_cannot_be_revived_by_a_late_valid_result() {
    let (mut engine, provider) = issue(
        "https://api.kucoin.com",
        Method::Post,
        "/api/v1/bullet-private",
        None,
        true,
        false,
        Delivery::Held,
    );
    engine.start(NOW);
    let mut actions: Vec<_> = engine.drain_actions().collect();
    for action in &actions {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare { preparation, .. } = action.as_ref()
        {
            engine.http_prepared(
                NOW,
                *preparation,
                Prepared {
                    id: 1,
                    expires_at: None,
                },
            );
        }
    }
    engine.prepare_dispatch(NOW);
    actions.extend(engine.drain_actions());
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (attempt, work) = actions
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.clone())),
            _ => None,
        })
        .unwrap();
    let result = provider.inner.sign_now(&work).unwrap();
    let (timer, deadline) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    engine.handle(deadline, Event::TimerFired { timer });
    engine.handle(
        deadline + 1.,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    assert!(
        !prepared_actions(&mut engine, deadline + 1.)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
}
