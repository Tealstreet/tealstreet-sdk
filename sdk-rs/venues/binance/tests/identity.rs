//! Actual native identity requests: exact routes/signers, failure ownership, no Store commits.
use support::Options;
mod support;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_binance::{BinanceFutures, Variant};
const NOW: f64 = 1000.;
type TestEngine = Engine<BinanceFutures>;
fn seeded(options: Options) -> TestEngine {
    let mut engine = support::engine(options);
    engine
        .store_mut()
        .set_balance(safe_cex_engine::model::Balance {
            total: 123.,
            free: 100.,
            used: 23.,
            upnl: 7.,
        });
    engine.store_mut().set_orders(vec![]);
    engine.store_mut().set_positions(vec![]);
    engine.store_mut().set_hedged(true);
    engine
}
fn ask(engine: &mut TestEngine) -> Vec<Action> {
    engine.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    safe_cex_engine::test_support::prepared_actions(engine, NOW)
}
fn identity_request(engine: &mut TestEngine) -> HttpRequest {
    let actions: Vec<_> = ask(engine)
        .into_iter()
        .filter(|action| !matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. }))
        .collect();
    assert_eq!(
        actions.len(),
        1,
        "one rare read, no reconciliation/startup work"
    );
    let Action::Http(request) = actions.into_iter().next().unwrap() else {
        panic!("expected identity request")
    };
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.body, None);
    assert_eq!(request.route, None, "ordinary selected-read egress");
    request
}
fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
        .as_str()
}
/// Capability-unsupported products are refused by the engine before the venue runs.
fn assert_engine_refusal(actions: Vec<Action>) {
    let [Action::Notice(Notice::CommandFailed { command, error })] = actions.as_slice() else {
        panic!("expected only the engine refusal: {actions:?}")
    };
    assert_eq!(*command, 42);
    assert_eq!(error.message, "This action is not supported on this venue");
}
fn result(actions: Vec<Action>, rejected: bool) -> Result<String, String> {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    for action in actions {
        match action {
            Action::Notice(Notice::CommandDone { command, result }) => {
                let result: Result<String, String> = Ok(result);
                assert_eq!(command, 42);
                done.push(result);
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                let result: Result<String, String> = Err(error.legacy_text());
                assert_eq!(command, 42);
                done.push(result);
            }
            Action::Notice(Notice::CommandError { command, message }) => {
                assert_eq!(command, 42);
                assert!(done.is_empty(), "diagnostic before completion");
                errors.push(message);
            }
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error,
                ..
            })) => panic!("identity errors must be correlated"),
            Action::Http(_) => panic!("identity must not trigger reconciliation"),
            _ => {}
        }
    }
    assert_eq!(done.len(), 1);
    assert_eq!(errors.len(), usize::from(rejected));
    let result = done.remove(0);
    assert_eq!(result.is_err(), rejected);
    if let Err(error) = &result {
        assert_eq!(&errors[0], error);
    }
    result
}
fn respond(engine: &mut TestEngine, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    engine.drain_actions().collect()
}
fn options(read: bool, variant: Variant) -> Options {
    Options {
        variant,
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    }
}
#[test]
fn identity_uses_balance_list_for_both_products_and_selected_exact_signer() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        for read in [false, true] {
            for testnet in [false, true] {
                let mut opts = options(read, variant);
                opts.testnet = testnet;
                let mut engine = seeded(opts);
                let before = engine.account_snapshot_frame(NOW).to_vec();
                let request = identity_request(&mut engine);
                let (origin, path) = match (variant, testnet) {
                    (Variant::UsdM, false) => ("https://fapi.binance.com", "/fapi/v3/balance"),
                    (Variant::PortfolioMargin, false) => {
                        ("https://papi.binance.com", "/papi/v1/balance")
                    }
                    (Variant::UsdM, true) => ("https://demo-fapi.binance.com", "/fapi/v3/balance"),
                    _ => ("https://demo-fapi.binance.com", "/papi/v1/balance"),
                };
                let (key, secret) = if read {
                    ("read-key", "read-secret")
                } else {
                    ("primary-key", "primary-secret")
                };
                let query = "timestamp=1000&recvWindow=20000";
                let signature = hmac_sha256(secret.as_bytes(), query.as_bytes(), Encoding::Hex);
                assert_eq!(
                    request.url,
                    format!("{origin}{path}?{query}&signature={signature}")
                );
                assert_eq!(header(&request, "X-MBX-APIKEY"), key);
                let body =
                    r#"[{"accountAlias":9007199254740993},{"accountAlias":"wrong-second-row"}]"#;
                assert_eq!(
                    result(respond(&mut engine, &request, 200, body), false).unwrap(),
                    r#"{"userId":"9007199254740993"}"#
                );
                assert_eq!(engine.account_snapshot_frame(NOW), before);
                assert!(respond(&mut engine, &request, 200, body).is_empty());
            }
        }
    }
}
#[test]
fn aliases_keep_masked_text_and_required_first_row_never_falls_back() {
    let mut engine = seeded(options(false, Variant::UsdM));
    let request = identity_request(&mut engine);
    assert_eq!(
        result(
            respond(
                &mut engine,
                &request,
                200,
                r#"[{"accountAlias":"masked-***"}]"#
            ),
            false
        )
        .unwrap(),
        r#"{"userId":"masked-***"}"#
    );
    for body in [
        "[]",
        r#"[{"accountAlias":null}]"#,
        r#"[{"accountAlias":false}]"#,
        r#"[{"accountAlias":""},{"accountAlias":"must-not-fallback"}]"#,
        r#"{"code":-2015,"msg":"rejected"}"#,
        "malformed",
    ] {
        let mut engine = seeded(options(false, Variant::UsdM));
        let before = engine.account_snapshot_frame(NOW).to_vec();
        let request = identity_request(&mut engine);
        result(respond(&mut engine, &request, 200, body), true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn failures_preserve_state_and_finish_once() {
    for network in [false, true] {
        let mut engine = seeded(options(false, Variant::PortfolioMargin));
        let before = engine.account_snapshot_frame(NOW).to_vec();
        let request = identity_request(&mut engine);
        let actions = if network {
            engine.handle(
                NOW,
                Event::HttpFailed {
                    request: request.id,
                    error: "connection lost",
                },
            );
            engine.drain_actions().collect()
        } else {
            respond(&mut engine, &request, 503, "unavailable")
        };
        result(actions, true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn unsupported_and_private_disabled_accounts_fail_before_io() {
    for opts in [
        Options::default(),
        Options {
            public_data_only: true,
            ..options(true, Variant::UsdM)
        },
        Options {
            secret: Some(String::new()),
            ..options(false, Variant::UsdM)
        },
        Options {
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read".into(),
                secret: String::new(),
                passphrase: None,
            }),
            ..options(false, Variant::UsdM)
        },
    ] {
        let mut engine = seeded(opts);
        let before = engine.account_snapshot_frame(NOW).to_vec();
        result(ask(&mut engine), true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}

#[test]
fn spot_and_coin_m_identity_is_refused_by_capability_before_io() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut engine = seeded(options(false, variant));
        let before = engine.account_snapshot_frame(NOW).to_vec();
        assert_engine_refusal(ask(&mut engine));
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}

#[test]
fn repeated_local_identity_refusals_preserve_messages_and_release_read_custody() {
    let mut engine = seeded(Options::default());
    let before = engine.account_snapshot_frame(NOW).to_vec();
    for _ in 0..300 {
        assert_eq!(
            result(ask(&mut engine), true).unwrap_err(),
            "Binance account identity requires credentials"
        );
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}
