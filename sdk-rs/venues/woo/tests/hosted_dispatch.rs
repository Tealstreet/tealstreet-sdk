mod support;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, SignError, SignFuture, SignRequest, SignResult,
    Signer,
};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Outcome, Response,
};
use safe_cex_venue_woo::{Options, Woo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
const NOW: f64 = 1_700_000_000_000.;
const MATERIAL: &str = "fixture-signing-material";
struct Delegate {
    local: LocalSigner,
    requests: Mutex<Vec<SignRequest>>,
    fail: AtomicBool,
}
impl Signer for Delegate {
    fn sign(&self, work: SignRequest) -> SignFuture {
        let result = self.sign_ready(&work).unwrap();
        Box::pin(async move { result })
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.requests.lock().unwrap().push(work.clone());
        Some(if self.fail.load(Ordering::SeqCst) {
            Err(SignError::Unavailable)
        } else {
            self.local.sign_now(work)
        })
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
}
fn provider(fail: bool) -> (Arc<Delegate>, Identities) {
    let (local, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-public".into()),
            secret: Secret::new(MATERIAL.into()),
            ..Default::default()
        }),
        secondary: Some(LocalIdentity {
            key: Secret::new("fixture-read".into()),
            secret: Secret::new("fixture-read-material".into()),
            ..Default::default()
        }),
    })
    .unwrap();
    (
        Arc::new(Delegate {
            local,
            requests: Mutex::new(Vec::new()),
            fail: AtomicBool::new(fail),
        }),
        identities,
    )
}
fn engine(delegated: bool, fail: bool) -> (Engine<Woo>, Arc<Delegate>) {
    let (seed, _) = support::started();
    let (provider, identities) = provider(fail);
    let options = if delegated {
        Options {
            credentials: identities.primary.clone(),
            read_signing: identities.secondary.clone(),
            ..Default::default()
        }
    } else {
        Options {
            key: Some("fixture-public".into()),
            secret: Some(MATERIAL.into()),
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "fixture-read".into(),
                secret: "fixture-read-material".into(),
                passphrase: None,
            }),
            ..Default::default()
        }
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(Woo::new(options));
    engine
        .store_mut()
        .set_markets(seed.store().markets().to_vec());
    engine
        .store_mut()
        .set_tickers(seed.store().tickers().to_vec());
    if delegated {
        engine.set_signer(Some(provider.clone())).unwrap();
        engine.set_identities(identities).unwrap();
    }
    (engine, provider)
}
fn place(engine: &mut Engine<Woo>, protective: bool) -> Vec<Action> {
    let mut order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":59000}"#,
    )
    .unwrap();
    if protective {
        order.kind = OrderType::StopLoss;
        order.side = OrderSide::Sell;
    }
    engine.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order],
        }),
    );
    safe_cex_engine::test_support::prepared_actions(engine, NOW)
}
fn request(actions: &[Action]) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .expect("signed write")
}
fn error(actions: &[Action]) -> &safe_cex_engine::VenueError {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandFailed { command: 1, error }) = a {
                Some(error)
            } else {
                None
            }
        })
        .expect("typed command failure")
}
#[test]
fn ordinary_and_protective_dispatch_are_byte_identical_to_local_key_controls() {
    for protective in [false, true] {
        let (mut local, _) = engine(false, false);
        let (mut delegated, provider) = engine(true, false);
        let expected = request(&place(&mut local, protective));
        let actual = request(&place(&mut delegated, protective));
        assert_eq!(actual.method, expected.method);
        assert_eq!(actual.url, expected.url);
        assert_eq!(actual.body, expected.body);
        assert_eq!(actual.headers, expected.headers);
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
    }
}

#[test]
fn local_and_hosted_writes_use_protocol_utc_instead_of_scheduling_time() {
    for protective in [false, true] {
        let (mut local, _) = engine(false, false);
        let (mut delegated, _) = engine(true, false);
        for engine in [&mut local, &mut delegated] {
            assert!(engine.sync_host_time(NOW, NOW + 60_000.));
        }
        let expected = request(&place(&mut local, protective));
        let actual = request(&place(&mut delegated, protective));
        assert_eq!(actual.headers, expected.headers);
        assert_eq!(actual.body, expected.body);
        assert!(
            actual
                .headers
                .contains(&("x-api-timestamp", "1700000060000".into()))
        );
    }
}
#[test]
fn signer_failure_is_not_sent_and_does_not_fall_back_to_local_material() {
    for protective in [false, true] {
        let (mut engine, provider) = engine(true, true);
        let actions = place(&mut engine, protective);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(error(&actions).outcome, Some(Outcome::NotSent));
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
        assert!(!format!("{actions:?}").contains(MATERIAL));
    }
}
#[test]
fn ambiguous_dispatched_writes_keep_unknown_and_are_never_replayed() {
    for protective in [false, true] {
        for response in [
            None,
            Some((503, "<html>")),
            Some((
                429,
                r#"{"success":false,"code":-1003,"message":"fixture rate limit"}"#,
            )),
            Some((
                200,
                r#"{"success":false,"code":-1003,"message":"fixture rate limit"}"#,
            )),
        ] {
            let (mut engine, provider) = engine(true, false);
            let r = request(&place(&mut engine, protective));
            engine.handle(
                NOW + 10.,
                if let Some((status, body)) = response {
                    Event::HttpResponse {
                        request: r.id,
                        response: Response {
                            status,
                            headers: "",
                            body,
                        },
                    }
                } else {
                    Event::HttpFailed {
                        request: r.id,
                        error: "fixture transport reset",
                    }
                },
            );
            let actions: Vec<_> = engine.drain_actions().collect();
            assert_eq!(error(&actions).outcome, Some(Outcome::Unknown));
            if response.is_some_and(|(_, body)| body.contains("-1003")) {
                assert_eq!(
                    error(&actions).kind,
                    Some(safe_cex_engine::ErrorKind::RateLimited)
                );
                assert_eq!(error(&actions).venue_code.as_deref(), Some("-1003"));
            }
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
            );
            assert_eq!(provider.requests.lock().unwrap().len(), 1);
            assert!(!format!("{actions:?}").contains(MATERIAL));
        }
    }
}

#[test]
fn cancel_all_rate_limit_survives_the_next_legs_signing_failure() {
    use safe_cex_engine::ErrorKind;
    let (mut engine, provider) = engine(true, false);
    engine.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: String::new(),
        }),
    );
    let first = request(&safe_cex_engine::test_support::prepared_actions(
        &mut engine,
        NOW,
    ));
    assert!(first.url.ends_with("/v3/orders/pending"));
    provider.fail.store(true, Ordering::SeqCst);
    engine.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"{"success":false,"code":-1003,"message":"fixture rate limit"}"#,
            },
        },
    );
    let mut actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW + 1.);
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    for (timer, at_ms) in timers {
        engine.handle(at_ms, Event::TimerFired { timer });
        actions.extend(safe_cex_engine::test_support::prepared_actions(
            &mut engine,
            at_ms,
        ));
    }
    let failure = error(&actions);
    assert_eq!(failure.outcome, Some(Outcome::Unknown));
    assert_eq!(failure.kind, Some(ErrorKind::RateLimited));
    assert_eq!(failure.venue_code.as_deref(), Some("-1003"));
    assert!(failure.message.contains("/v3/orders/pending"));
    assert!(failure.message.contains("fixture rate limit"));
    assert_eq!(provider.requests.lock().unwrap().len(), 2);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!format!("{actions:?}").contains(MATERIAL));
}

#[test]
fn opaque_read_identity_preserves_the_local_secondary_get_wire() {
    let (mut local, _) = engine(false, false);
    let (mut delegated, provider) = engine(true, false);
    for engine in [&mut local, &mut delegated] {
        engine.handle(NOW, Event::Command(Command::FetchAccount { command: 1 }));
    }
    let expected = request(&safe_cex_engine::test_support::prepared_actions(
        &mut local, NOW,
    ));
    let actual = request(&safe_cex_engine::test_support::prepared_actions(
        &mut delegated,
        NOW,
    ));
    assert_eq!(actual.url, expected.url);
    assert_eq!(actual.headers, expected.headers);
    assert_eq!(actual.route, Some(safe_cex_engine::RequestRoute::Secondary));
    assert!(
        actual
            .headers
            .contains(&("x-api-key", "fixture-read".into()))
    );
    let work = provider.requests.lock().unwrap();
    assert_eq!(work.len(), 1);
    assert!(
        matches!(&work[0].operations[0], safe_cex_engine::signing::Operation::Present { key }
        if key == &delegated.identities().secondary.as_ref().unwrap().credential)
    );
}

#[test]
fn ordinary_and_protective_cancel_amend_keep_wire_and_failure_boundaries() {
    use safe_cex_engine::model::{Order, OrderStatus};
    for protective in [false, true] {
        for amend in [false, true] {
            let command = || {
                if amend {
                    Command::AmendOrder {
                        command: 1,
                        id: "42".into(),
                        price: Some(59500.),
                        amount: None,
                    }
                } else {
                    Command::CancelOrders {
                        command: 1,
                        ids: vec!["42".into()],
                    }
                }
            };
            let seed = |engine: &mut Engine<Woo>| {
                engine.store_mut().set_orders(vec![Order {
                    id: "42".into(),
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: "BTCUSDT".into(),
                    kind: if protective {
                        OrderType::StopLoss
                    } else {
                        OrderType::Limit
                    },
                    side: OrderSide::Sell,
                    price: 61000.,
                    amount: 0.01,
                    filled: 0.,
                    remaining: 0.01,
                    reduce_only: protective,
                    close: None,
                    client_order_id: None,
                    time_in_force: None,
                    last_update: 0.,
                    trigger_type: None,
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                }])
            };
            let (mut local, _) = engine(false, false);
            let (mut delegated, _) = engine(true, false);
            for engine in [&mut local, &mut delegated] {
                seed(engine);
                engine.handle(NOW, Event::Command(command()));
            }
            let expected = request(&safe_cex_engine::test_support::prepared_actions(
                &mut local, NOW,
            ));
            let actual = request(&safe_cex_engine::test_support::prepared_actions(
                &mut delegated,
                NOW,
            ));
            assert_eq!(
                (actual.method, &actual.url, &actual.body, &actual.headers),
                (
                    expected.method,
                    &expected.url,
                    &expected.body,
                    &expected.headers
                )
            );
            delegated.handle(
                NOW + 1.,
                Event::HttpFailed {
                    request: actual.id,
                    error: "fixture reset",
                },
            );
            let actions: Vec<_> = delegated.drain_actions().collect();
            assert_eq!(error(&actions).outcome, Some(Outcome::Unknown));
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
            );
            let (mut refused, _) = engine(true, true);
            seed(&mut refused);
            refused.handle(NOW, Event::Command(command()));
            let actions = safe_cex_engine::test_support::prepared_actions(&mut refused, NOW);
            assert_eq!(error(&actions).outcome, Some(Outcome::NotSent));
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
}

fn unknown_events(actions: &[Action]) -> Vec<&safe_cex_engine::LogEvent> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn assert_nx_unknown_fields(
    event: &safe_cex_engine::LogEvent,
    command_id: u32,
    request_id: safe_cex_engine::RequestId,
    status: Option<u16>,
) {
    use safe_cex_engine::log::Value;
    let mut names = event.fields.iter().map(|(key, _)| *key).collect::<Vec<_>>();
    names.sort_unstable();
    let mut expected = vec![
        "command",
        "outcome",
        "placedOrderIds",
        "request",
        "unknownIndices",
    ];
    if status.is_some() {
        expected.push("status");
    }
    expected.sort_unstable();
    assert_eq!(names, expected, "NX #9262 accepts exactly these fields");
    let field = |key| {
        &event
            .fields
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap()
            .1
    };
    let request = field("request")
        .as_str()
        .expect("decimal request ID string");
    // REQUIRED PIN: ^[0-9]{1,20}$, never the signed request URL.
    assert!((1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(request, request_id.0.to_string());
    assert_eq!(field("outcome").as_str(), Some("unknown"));
    let command = field("command").as_str().unwrap();
    assert!((1..=64).contains(&command.len()) && command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(command, format!("woo.{command_id}"));
    if let Some(status) = status {
        assert_eq!(*field("status"), Value::Num(status.into()));
    }
    for (key, value) in &event.fields {
        assert!(!["url", "endpoint", "query", "body", "signature", "payload"].contains(key));
        if let Some(text) = value.as_str() {
            let lower = text.to_ascii_lowercase();
            for poison in [
                "://",
                "/",
                "?",
                "=",
                "&",
                "signature",
                "payload",
                "body",
                "secret",
                "api_key",
            ] {
                assert!(!lower.contains(poison), "unsafe content in {key}: {poison}");
            }
        }
    }
}

#[test]
fn unknown_hook_legacy_write_and_refusal_are_exactly_once() {
    assert_unknown_hook_cases(&[
        (Some(503), "response-body-canary", 1),
        (Some(408), "response-body-canary", 1),
        (Some(429), "response-body-canary", 1),
        (None, "transport-signature-canary", 1),
        (
            Some(400),
            r#"{"success":false,"code":-1005,"message":"definite refusal"}"#,
            0,
        ),
    ]);
}

#[test]
fn unknown_hook_native_decoded_has_one_event_without_duplicate() {
    assert_unknown_hook_cases(&[(
        Some(200),
        r#"{"success":false,"code":-1012,"message":"native unknown"}"#,
        1,
    )]);
}

fn assert_unknown_hook_cases(cases: &[(Option<u16>, &str, usize)]) {
    for frame_fed in [false, true] {
        for &(status, body, event_count) in cases {
            let (mut e, _) = engine(true, false);
            if frame_fed {
                e.set_frame_fed().unwrap();
            }
            let sent = request(&place(&mut e, false));
            let event = if let Some(status) = status {
                Event::HttpResponse {
                    request: sent.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                }
            } else {
                Event::HttpFailed {
                    request: sent.id,
                    error: body,
                }
            };
            e.handle(NOW + 1., event);
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.);
            let events = unknown_events(&actions);
            assert_eq!(
                events.len(),
                event_count,
                "frame={frame_fed}, status={status:?}"
            );
            if let Some(event) = events.first() {
                assert_nx_unknown_fields(event, 1, sent.id, status);
            }
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
            );
            e.handle(
                NOW + 2.,
                Event::HttpResponse {
                    request: sent.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"success":true,"order_id":123}"#,
                    },
                },
            );
            let late = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 2.);
            assert!(
                unknown_events(&late).is_empty(),
                "late reply cannot duplicate telemetry"
            );
        }
    }
}

#[test]
fn unknown_hook_projection_matches_nx_bounds_caps_and_secret_exclusion() {
    use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
    use safe_cex_engine::{EngineUnknownOutcome, RequestId, Venue, VenueError};
    let hook =
        <Woo as Venue>::unknown_outcome_telemetry().expect("WOO opts into engine-sealed telemetry");
    let poison = "https://signed.invalid/order?api_key=fixture&signature=fixture-body-payload";
    let ids = [poison.into(), "9".repeat(33), "١".into(), "".into()]
        .into_iter()
        .chain((0..20).map(|i| format!("{i:020}")))
        .collect::<Vec<_>>();
    let mut error = VenueError::new(poison)
        .venue_message(poison)
        .partial(&ids, 32, usize::MAX);
    error.elements = Some(Box::new(ElementOutcomes(
        (0..20)
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some(poison.into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(poison).outcome(Outcome::Unknown)),
                },
            })
            .collect(),
    )));
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = hook(&EngineUnknownOutcome {
            now: NOW,
            command: u32::MAX,
            request: RequestId(u64::MAX),
            status,
            error: &error,
        });
        assert_eq!(event.code, "order.outcome_unknown");
        assert_eq!(event.level, safe_cex_engine::Level::Warn);
        assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
        assert_nx_unknown_fields(
            &event,
            u32::MAX,
            RequestId(u64::MAX),
            status.filter(|s| (100..=599).contains(s)),
        );
        let value = |key| {
            event
                .fields
                .iter()
                .find(|(name, _)| *name == key)
                .unwrap()
                .1
                .as_str()
                .unwrap()
        };
        let placed = value("placedOrderIds").split(',').collect::<Vec<_>>();
        assert_eq!(placed.len(), 16, "NX #9262 list cap");
        assert_eq!(
            placed,
            (0..16).map(|i| format!("{i:020}")).collect::<Vec<_>>()
        );
        assert!(
            placed
                .iter()
                .all(|id| (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
        );
        let indices = value("unknownIndices")
            .split(',')
            .map(|i| i.parse::<u16>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(indices, (0..16).collect::<Vec<_>>());
    }
    error.elements = Some(Box::new(ElementOutcomes(
        [65536, usize::MAX, 65535, 0]
            .into_iter()
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: None,
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(poison)),
                },
            })
            .collect(),
    )));
    let project = |error: &VenueError| {
        hook(&EngineUnknownOutcome {
            now: NOW,
            command: 1,
            request: RequestId(0),
            status: None,
            error,
        })
    };
    let event = project(&error);
    assert_nx_unknown_fields(&event, 1, RequestId(0), None);
    assert_eq!(
        event
            .fields
            .iter()
            .find(|(k, _)| *k == "unknownIndices")
            .unwrap()
            .1
            .as_str(),
        Some("65535,0")
    );
    error.elements = None;
    let event = project(&error);
    assert_eq!(
        event
            .fields
            .iter()
            .find(|(k, _)| *k == "unknownIndices")
            .unwrap()
            .1
            .as_str(),
        Some("")
    );
    let long_ids = vec!["9".repeat(32); 20];
    error = error.partial(&long_ids, 20, 0);
    let event = project(&error);
    let ids = event
        .fields
        .iter()
        .find(|(k, _)| *k == "placedOrderIds")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert!(
        ids.len() <= safe_cex_engine::log::MAX_LOG_BYTES,
        "engine must not truncate a valid list into an invalid NX value"
    );
    assert_eq!(ids.split(',').count(), 15);
}

// WOO still uses legacy HTTP. Exercise its registered static projection at an
// explicit managed engine boundary without changing WOO's native dispatch.
struct ManagedWooHook;
impl safe_cex_engine::Venue for ManagedWooHook {
    type Pending = safe_cex_engine::request_context::Context;
    fn unknown_outcome_telemetry() -> Option<safe_cex_engine::UnknownOutcomeTelemetry> {
        <Woo as safe_cex_engine::Venue>::unknown_outcome_telemetry()
    }
    fn start(&mut self, _: &mut safe_cex_engine::Cx<'_, Self::Pending>) {}
    fn on_open(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: safe_cex_engine::SocketId,
    ) {
    }
    fn on_message(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: safe_cex_engine::SocketId,
        _: &str,
    ) {
    }
    fn on_close(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: safe_cex_engine::SocketId,
        _: u16,
    ) {
    }
    fn on_timer(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: safe_cex_engine::TimerId,
    ) {
    }
    fn on_request_failed(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: Self::Pending,
        _: &str,
    ) {
        panic!("managed failure uses typed callback");
    }
    fn on_response(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: Self::Pending,
        _: Response<'_>,
    ) {
        panic!("engine-sealed Unknown bypasses decoder");
    }
    fn on_http_failed(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        context: Self::Pending,
        error: safe_cex_engine::VenueError,
    ) {
        cx.fail_context(&context, error).unwrap();
    }
    fn capture_http_reconciliation(
        &self,
        _: &Self::Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("woo-hook-fixture".into())
    }
    fn on_command(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, _: Command) {
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
        let context = cx
            .admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 1000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        cx.request_managed(
            context.clone(),
            None,
            Method::Post,
            "https://signed.invalid/order?api_key=fixture&signature=fixture".into(),
            Some("body-payload-canary".into()),
            context,
        )
        .unwrap();
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn unknown_hook_engine_sealed_boundary_emits_once_with_woo_projection() {
    for frame_fed in [false, true] {
        for status in [Some(408), Some(429), Some(503), None] {
            let mut e = safe_cex_engine::test_support::seeded_engine(ManagedWooHook);
            let (seed, _) = support::started();
            e.store_mut().set_markets(seed.store().markets().to_vec());
            if frame_fed {
                e.set_frame_fed().unwrap();
            }
            e.handle(
                NOW,
                Event::Command(Command::PlaceOrders {
                    command: 1,
                    orders: vec![],
                }),
            );
            let sent = request(&safe_cex_engine::test_support::prepared_actions(
                &mut e, NOW,
            ));
            let event = match status {
                Some(status) => Event::HttpResponse {
                    request: sent.id,
                    response: Response {
                        status,
                        headers: "",
                        body: "response-signature-canary",
                    },
                },
                None => Event::HttpFailed {
                    request: sent.id,
                    error: "body-payload-canary",
                },
            };
            e.handle(NOW + 1., event);
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.);
            let events = unknown_events(&actions);
            assert_eq!(events.len(), 1);
            assert_nx_unknown_fields(events[0], 1, sent.id, status);
            assert_eq!(error(&actions).outcome, Some(Outcome::Unknown));
            e.handle(
                NOW + 2.,
                Event::HttpFailed {
                    request: sent.id,
                    error: "late",
                },
            );
            assert!(
                unknown_events(&safe_cex_engine::test_support::prepared_actions(
                    &mut e,
                    NOW + 2.
                ))
                .is_empty()
            );
        }
    }
}
