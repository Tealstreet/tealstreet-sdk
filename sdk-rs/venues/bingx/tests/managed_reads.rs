//! Production read builders must prepare transport before asking the provider.
use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_bingx::{Bingx, Options};

fn identity(slot: &str) -> Identity {
    Identity {
        extra: Default::default(),
        credential: KeyRef(format!("ref:{slot}-credential")),
        signing: KeyRef(format!("ref:{slot}-signing")),
        passphrase: None,
        stable_id: None,
    }
}
fn engine() -> Engine<Bingx> {
    safe_cex_engine::test_support::seeded_engine(Bingx::new(Options {
        credentials: Some(identity("primary")),
        read_credentials: Some(identity("secondary")),
        public_data_only: false,
    }))
}
fn assert_prepared_read(mut engine: Engine<Bingx>, identity_read: bool) {
    if identity_read {
        engine.handle(
            1_000.,
            Event::Command(Command::FetchAccount { command: 42 }),
        );
    } else {
        engine.start(1_000.);
    }
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let prepares = actions
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prepares.len(),
        1,
        "the actual native builder must park one unsigned read"
    );
    let (preparation, request) = prepares[0];
    assert_eq!(
        request.headers,
        vec![
            ("X-SOURCE-KEY", "Tealstreet".into()),
            ("Content-Type", "application/json, charset=utf-8".into()),
        ]
    );
    assert!(request.signer_headers.is_empty());
    assert!(!request.url.contains("timestamp="));
    assert!(!request.url.contains("signature="));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
    engine.http_prepared(
        1_001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let signs = actions
        .iter()
        .filter_map(|action| match action {
            Action::Sign { work, .. } => Some(work),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(signs.len(), 1);
    assert!(
        matches!(&signs[0].operations[0], safe_cex_engine::signing::Operation::Present { key } if key.0 == "ref:secondary-credential")
    );
    assert_eq!(signs[0].receipt.stamp_ms, Some(1_001.));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}
#[test]
fn credentialed_market_read_keeps_native_secondary_signing_after_route_preparation() {
    assert_prepared_read(engine(), false);
}
#[test]
fn identity_read_uses_managed_transport_and_the_selected_secondary() {
    assert_prepared_read(engine(), true);
}

fn identity_preparation() -> (Engine<Bingx>, safe_cex_engine::request_context::Preparation) {
    let mut engine = engine();
    engine.handle(
        1_000.,
        Event::Command(Command::FetchAccount { command: 42 }),
    );
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    (engine, preparation)
}
fn identity_release() -> (Engine<Bingx>, safe_cex_engine::RequestId) {
    let (engine, preparation) = identity_preparation();
    release_preparation(engine, preparation)
}
fn release_preparation(
    engine: Engine<Bingx>,
    preparation: safe_cex_engine::request_context::Preparation,
) -> (Engine<Bingx>, safe_cex_engine::RequestId) {
    release_preparation_at(engine, preparation, 1_001., 1)
}
fn release_preparation_at(
    mut engine: Engine<Bingx>,
    preparation: safe_cex_engine::request_context::Preparation,
    now: f64,
    prepared_id: u64,
) -> (Engine<Bingx>, safe_cex_engine::RequestId) {
    engine.http_prepared(
        now,
        preparation,
        Prepared {
            id: prepared_id,
            expires_at: None,
        },
    );
    let (attempt, work) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    // The injected provider is trusted. This control tests lifecycle custody,
    // not MAC correctness (independent native vectors cover that separately).
    engine.handle(
        now + 1.,
        Event::Signed {
            attempt,
            result: Ok(safe_cex_engine::signing::SignResult {
                values: vec![b"fixture-secondary".to_vec(), vec![7; 32]],
                receipt: work.receipt,
                headers: Vec::new(),
            }),
        },
    );
    let (permit, request) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(now + 1., permit),
        Ok(safe_cex_engine::request_context::Claim::Released(_))
    ));
    assert_eq!(
        request.headers[1],
        ("X-BX-APIKEY", "fixture-secondary".into())
    );
    (engine, request.id)
}
fn completions(engine: &mut Engine<Bingx>) -> Vec<Result<String, safe_cex_engine::VenueError>> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 42,
                result,
            }) => Some(Ok(result)),
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 42, error }) => {
                Some(Err(error))
            }
            _ => None,
        })
        .collect()
}
#[test]
fn managed_identity_success_and_native_rejection_complete_once_with_context_authority() {
    for (body, success) in [
        (
            r#"{"code":0,"data":[{"asset":"USDT","userId":"venue-account"}]}"#,
            true,
        ),
        (r#"{"code":100004,"msg":"native refusal","data":[]}"#, false),
        (r#"{"code":0,"data":[]}"#, false),
        (
            r#"{"code":100001,"msg":"signature verification failed","data":[]}"#,
            false,
        ),
    ] {
        let (mut engine, request) = identity_release();
        engine.handle(
            1_003.,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        let done = completions(&mut engine);
        assert_eq!(
            done.len(),
            1,
            "managed identity must not use numeric completion"
        );
        assert_eq!(done[0].is_ok(), success);
        if success {
            assert_eq!(done[0].as_ref().unwrap(), r#"{"userId":"venue-account"}"#);
        }
        engine.handle(
            1_004.,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        assert!(
            completions(&mut engine).is_empty(),
            "retired physical response cannot complete again"
        );
    }
}
#[test]
fn managed_identity_provider_refusal_keeps_the_typed_error_and_completes_once() {
    let (mut engine, preparation) = identity_preparation();
    engine.http_prepared(
        1_001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let attempt = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Sign { attempt, .. } => Some(attempt),
            _ => None,
        })
        .unwrap();
    engine.handle(
        1_002.,
        Event::Signed {
            attempt,
            result: Err(safe_cex_engine::signing::SignError::Refused),
        },
    );
    assert_eq!(
        completions(&mut engine),
        vec![Err(
            safe_cex_engine::signing::SignError::Refused.venue_error()
        )]
    );
    engine.handle(
        1_003.,
        Event::Signed {
            attempt,
            result: Err(safe_cex_engine::signing::SignError::Refused),
        },
    );
    assert!(completions(&mut engine).is_empty());
}

#[test]
fn managed_identity_route_failure_is_typed_and_never_invokes_the_provider() {
    let (mut engine, preparation) = identity_preparation();
    let error = safe_cex_engine::VenueError::new("fixture route denied")
        .kind(safe_cex_engine::ErrorKind::Unsupported)
        .outcome(safe_cex_engine::Outcome::NotSent);
    engine.http_preparation_failed(
        1_001.,
        preparation,
        safe_cex_engine::request_context::Error::Invalid,
        error.clone(),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Sign { .. } | Action::Http(_)) || matches!(action, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Bind { .. })))
    );
    let failures = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 42, error }) => {
                Some(error)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures, vec![error]);
}
#[test]
fn managed_identity_original_deadline_retires_late_results() {
    let (mut engine, request) = identity_release();
    let body = r#"{"code":0,"data":[{"asset":"USDT","userId":"late"}]}"#;
    engine.handle(
        21_000.,
        Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    let done = completions(&mut engine);
    assert_eq!(done.len(), 1);
    assert!(
        done[0].is_err(),
        "a released read cannot renew its original 20s deadline"
    );
    engine.handle(
        21_001.,
        Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    assert!(completions(&mut engine).is_empty());
}

fn candle_market() -> safe_cex_engine::model::Market {
    safe_cex_engine::model::Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: safe_cex_engine::model::Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: safe_cex_engine::model::Limits {
            amount: safe_cex_engine::model::MinMax {
                min: 0.001,
                max: 10.0,
            },
            leverage: safe_cex_engine::model::MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn candle_preparation() -> (Engine<Bingx>, safe_cex_engine::request_context::Preparation) {
    let mut engine = engine();
    engine.store_mut().set_markets(vec![candle_market()]);
    engine.handle(
        1_000.,
        Event::Command(Command::FetchCandles {
            command: 42,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: Some(120_000.),
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let prepare = actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("actual candle builder must prepare managed transport before provider");
    assert_eq!(
        prepare.1.url,
        "https://open-api.bingx.com/openApi/swap/v3/quote/klines?endTime=120000&interval=1m&startTime=0&symbol=BTC-USDT"
    );
    assert!(prepare.1.signer_headers.is_empty());
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Sign { .. } | Action::Http(_)))
    );
    (engine, prepare.0)
}
#[test]
fn credentialed_candle_builder_prepares_managed_transport_before_signing() {
    let _ = candle_preparation();
}

fn candle_done(engine: &mut Engine<Bingx>) -> Vec<Result<String, safe_cex_engine::VenueError>> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::AccountAuth { .. }) => {
                panic!("candle data is not private account auth evidence")
            }
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 42,
                result,
            }) => Some(Ok(result)),
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 42, error }) => {
                Some(Err(error))
            }
            _ => None,
        })
        .collect()
}
#[test]
fn managed_candles_preserve_native_rows_and_complete_once_for_all_response_shapes() {
    let rows = r#"{"code":0,"data":[{"time":120000,"open":"2","high":"3","low":"1","close":"2.5","volume":"4"},{"time":60000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}]}"#;
    for (status, body, ok) in [
        (200, rows, true),
        (200, r#"{"code":0,"data":[]}"#, true),
        (
            200,
            r#"{"code":100004,"msg":"native refusal","data":[]}"#,
            false,
        ),
        (500, r#"{"code":0,"data":[]}"#, false),
        (200, r#"{"code":0,"data":[{}]}"#, false),
        (200, r#"{"code":0}"#, false),
        (200, r#"{"code":0,"data":null}"#, false),
        (200, r#"{"code":0,"data":["#, false),
    ] {
        let (engine, preparation) = candle_preparation();
        let (mut engine, request) = release_preparation(engine, preparation);
        engine.handle(
            1_003.,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let done = candle_done(&mut engine);
        assert_eq!(done.len(), 1, "status {status} body {body}");
        assert_eq!(done[0].is_ok(), ok, "{body}");
        if body == rows {
            assert!(
                done[0]
                    .as_ref()
                    .unwrap()
                    .starts_with(r#"[{"timestamp":60.0,"#)
            );
        }
        if ok && body != rows {
            assert_eq!(done[0].as_ref().unwrap(), "[]");
        }
        assert!(engine.store().orders().is_empty());
        assert!(engine.store().positions().is_empty());
        engine.handle(
            1_004.,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        assert!(candle_done(&mut engine).is_empty());
    }
}
#[test]
fn managed_candle_provider_refusal_preserves_exact_typed_error() {
    let (mut engine, preparation) = candle_preparation();
    engine.http_prepared(
        1_001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let attempt = engine
        .drain_actions()
        .find_map(|a| {
            if let Action::Sign { attempt, .. } = a {
                Some(attempt)
            } else {
                None
            }
        })
        .unwrap();
    let error = safe_cex_engine::signing::SignError::Refused;
    engine.handle(
        1_002.,
        Event::Signed {
            attempt,
            result: Err(error),
        },
    );
    assert_eq!(candle_done(&mut engine), vec![Err(error.venue_error())]);
    engine.handle(
        1_003.,
        Event::Signed {
            attempt,
            result: Err(error),
        },
    );
    assert!(candle_done(&mut engine).is_empty());
}
#[test]
fn managed_candle_route_refusal_is_typed_and_never_signs_or_binds() {
    let (mut engine, preparation) = candle_preparation();
    let error = safe_cex_engine::VenueError::new("candle route denied")
        .kind(safe_cex_engine::ErrorKind::Unsupported)
        .outcome(safe_cex_engine::Outcome::NotSent);
    engine.http_preparation_failed(
        1_001.,
        preparation,
        safe_cex_engine::request_context::Error::Invalid,
        error.clone(),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::Http(_))
                || matches!(a,Action::ManagedHttp(a)if matches!(a.as_ref(),HttpAction::Bind{..})))
    );
    let errors = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 42, error }) = a
            {
                Some(error)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(errors, vec![error]);
}
#[test]
fn managed_candle_deadline_rejects_late_success_once() {
    let (engine, preparation) = candle_preparation();
    let (mut engine, request) = release_preparation(engine, preparation);
    let body = r#"{"code":0,"data":[]}"#;
    for now in [21_000., 21_001.] {
        engine.handle(
            now,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        let done = candle_done(&mut engine);
        if now == 21_000. {
            assert_eq!(done.len(), 1);
            assert!(done[0].is_err());
        } else {
            assert!(done.is_empty());
        }
    }
}

#[test]
fn managed_candle_signature_retry_keeps_the_original_twenty_second_bound() {
    let (engine, preparation) = candle_preparation();
    let (mut engine, first_request) = release_preparation(engine, preparation);
    engine.handle(
        2_000.,
        Event::HttpResponse {
            request: first_request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":100001,"msg":"signature verification failed","data":[]}"#,
            },
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(
            safe_cex_engine::Notice::CommandDone { .. }
                | safe_cex_engine::Notice::CommandFailed { .. }
        )
    )));
    let preparation = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("one native read retry");
    engine.http_prepared(
        20_990.,
        preparation,
        Prepared {
            id: 2,
            expires_at: None,
        },
    );
    let (attempt, work) = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .unwrap();
    assert_eq!(work.receipt.stamp_ms, Some(20_990.));
    engine.handle(
        20_991.,
        Event::Signed {
            attempt,
            result: Ok(safe_cex_engine::signing::SignResult::new(
                vec![b"fixture-secondary".to_vec(), vec![7; 32]],
                work.receipt,
            )),
        },
    );
    let (permit, request) = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request.id)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_ne!(request, first_request);
    assert!(matches!(
        engine.claim_http(20_991., permit),
        Ok(safe_cex_engine::request_context::Claim::Released(_))
    ));
    engine.handle(
        21_000.,
        Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":[]}"#,
            },
        },
    );
    let done = candle_done(&mut engine);
    assert_eq!(done.len(), 1);
    assert!(
        done[0].is_err(),
        "repreparation must not renew the original command bound"
    );
}

#[test]
fn managed_candle_retry_drops_rows_and_decode_failure_from_the_refused_response() {
    for first_data in [
        r#"[{"time":120000,"open":"9","high":"9","low":"9","close":"9","volume":"9"}]"#,
        r#"[{}]"#,
    ] {
        let (engine, preparation) = candle_preparation();
        let (mut engine, request) = release_preparation(engine, preparation);
        let first_body = format!(
            r#"{{"code":100001,"msg":"signature verification failed","data":{first_data}}}"#
        );
        engine.handle(
            2_000.,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: &first_body,
                },
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        let preparation = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let (mut engine, request) = release_preparation_at(engine, preparation, 2_001., 2);
        engine.handle(
            2_003.,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: r#"{"code":0,"data":[]}"#,
                },
            },
        );
        assert_eq!(
            candle_done(&mut engine),
            vec![Ok("[]".into())],
            "refused data must neither escape nor poison retry"
        );
    }
}

// Paired metadata production wiring waits for the shared root/child read-delivery
// validator. These original-RED builders pin the transport seam it must adopt.
fn metadata_preparation(secondary: bool) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Bingx::new(Options {
        credentials: Some(identity("primary")),
        read_credentials: secondary.then(|| identity("secondary")),
        public_data_only: false,
    }));
    engine.store_mut().set_markets(vec![candle_market()]);
    engine.handle(
        1_000.,
        Event::Command(Command::FetchLeverage {
            command: 42,
            symbol: "BTCUSDT".into(),
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let prepares = actions
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prepares.len(),
        1,
        "actual metadata builder must prepare managed transport before signing"
    );
    let (preparation, request) = prepares[0];
    assert_eq!(
        request.url,
        "https://open-api.bingx.com/openApi/swap/v2/trade/leverage?symbol=BTC-USDT"
    );
    assert_eq!(
        request.headers,
        vec![
            ("X-SOURCE-KEY", "Tealstreet".into()),
            ("Content-Type", "application/json, charset=utf-8".into()),
        ]
    );
    assert!(request.signer_headers.is_empty());
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
    engine.http_prepared(
        1_001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let work = actions
        .iter()
        .find_map(|action| match action {
            Action::Sign { work, .. } => Some(work),
            _ => None,
        })
        .expect("metadata signs only after route preparation");
    let expected = if secondary {
        "ref:secondary-credential"
    } else {
        "ref:primary-credential"
    };
    assert!(
        matches!(&work.operations[0], safe_cex_engine::signing::Operation::Present { key } if key.0 == expected)
    );
    assert_eq!(work.receipt.stamp_ms, Some(1_001.));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}
#[test]
#[ignore = "original RED: paired metadata waits for the shared Plan/CallWindow seam"]
fn paired_metadata_primary_builder_requires_managed_route_preparation() {
    metadata_preparation(false);
}
#[test]
#[ignore = "original RED: paired metadata waits for the shared Plan/CallWindow seam"]
fn paired_metadata_secondary_builder_requires_managed_route_preparation() {
    metadata_preparation(true);
}
