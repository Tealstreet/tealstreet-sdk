mod support;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, SignError, SignFuture, SignRequest, SignResult,
    Signer,
};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Response};
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
