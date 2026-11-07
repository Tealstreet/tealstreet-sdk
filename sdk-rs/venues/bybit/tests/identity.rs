//! Actual native identity requests: exact routes/signers, failure ownership, no Store commits.
mod support;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_bybit::{BybitLinear, Variant};
use support::Options;
const NOW: f64 = 1000.;
type TestEngine = Engine<BybitLinear>;
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
    // Identity is a managed read: the fixture driver prepares and signs it.
    safe_cex_engine::test_support::prepared_actions(engine, NOW)
}
fn identity_request(engine: &mut TestEngine) -> HttpRequest {
    let mut requests: Vec<_> = ask(engine)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "one rare read, no reconciliation/startup work"
    );
    let request = requests.remove(0);
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
    safe_cex_engine::test_support::prepared_actions(engine, NOW)
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
fn identity_queries_all_v5_products_with_selected_read_signer() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Spot,
        Variant::Demo,
    ] {
        for read in [false, true] {
            for testnet in [false, true] {
                let mut opts = options(read, variant);
                opts.testnet = testnet;
                let mut engine = seeded(opts);
                let before = engine.account_snapshot_frame(NOW).to_vec();
                let request = identity_request(&mut engine);
                let origin = if testnet {
                    "https://api-testnet.bybit.com"
                } else if variant == Variant::Demo {
                    "https://api-demo.bybit.com"
                } else {
                    "https://api.bybit.com"
                };
                assert_eq!(request.url, format!("{origin}/v5/user/query-api"));
                let (key, secret) = if read {
                    ("read-key", "read-secret")
                } else {
                    ("primary-key", "primary-secret")
                };
                assert_eq!(header(&request, "X-BAPI-API-KEY"), key);
                assert_eq!(header(&request, "X-BAPI-TIMESTAMP"), "1000");
                assert_eq!(
                    header(&request, "X-BAPI-SIGN"),
                    hmac_sha256(
                        secret.as_bytes(),
                        format!("1000{key}20000").as_bytes(),
                        Encoding::Hex
                    )
                );
                let body = r#"{"retCode":0,"result":{"userID":9007199254740993,"inviterID":9007199254740995}}"#;
                assert_eq!(
                    result(respond(&mut engine, &request, 200, body), false).unwrap(),
                    r#"{"userId":"9007199254740993","affiliateId":"9007199254740995"}"#
                );
                assert_eq!(engine.account_snapshot_frame(NOW), before);
                assert!(respond(&mut engine, &request, 200, body).is_empty());
            }
        }
    }
}
#[test]
fn optional_affiliate_zero_null_and_exact_text_are_distinct() {
    for (affiliate, expected) in [
        ("0", "null"),
        ("null", "null"),
        (r#""""#, "null"),
        (r#""0""#, r#""0""#),
    ] {
        let mut engine = seeded(options(false, Variant::Linear));
        let request = identity_request(&mut engine);
        let body = format!(
            r#"{{"retCode":0,"result":{{"userID":"masked-***","inviterID":{affiliate}}}}}"#
        );
        assert_eq!(
            result(respond(&mut engine, &request, 200, &body), false).unwrap(),
            format!(r#"{{"userId":"masked-***","affiliateId":{expected}}}"#)
        );
    }
}
#[test]
fn signature_rejection_resends_identity_once_without_completing_or_mutating_account() {
    let mut engine = seeded(options(true, Variant::Linear));
    let before = engine.account_snapshot_frame(NOW).to_vec();
    let first = identity_request(&mut engine);
    let actions = respond(
        &mut engine,
        &first,
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[1000],server_timestamp[8000],recv_window[20000]","result":{},"time":8000}"#,
    );
    let mut requests = Vec::new();
    for action in actions {
        match action {
            Action::Http(request) => requests.push(request),
            Action::Notice(
                Notice::CommandDone { .. }
                | Notice::CommandFailed { .. }
                | Notice::CommandError { .. },
            ) => {
                panic!("retry must retain original command")
            }
            _ => {}
        }
    }
    assert_eq!(requests.len(), 1);
    let second = requests.pop().unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.url, second.url);
    assert_eq!(header(&second, "X-BAPI-API-KEY"), "read-key");
    assert_eq!(header(&second, "X-BAPI-TIMESTAMP"), "8000");
    assert_eq!(
        header(&second, "X-BAPI-SIGN"),
        hmac_sha256(b"read-secret", b"8000read-key20000", Encoding::Hex)
    );
    result(
        respond(
            &mut engine,
            &second,
            200,
            r#"{"retCode":10004,"retMsg":"error sign!","result":{}}"#,
        ),
        true,
    )
    .unwrap_err();
    assert_eq!(engine.account_snapshot_frame(NOW), before);
}
#[test]
fn malformed_missing_or_failed_identity_finishes_once_without_state_changes() {
    for body in [
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0,"result":{"userID":false}}"#,
        r#"{"retCode":0,"result":{"userID":""}}"#,
        r#"{"retCode":10005,"retMsg":"denied","result":{}}"#,
        "malformed",
    ] {
        let mut engine = seeded(options(false, Variant::Linear));
        let before = engine.account_snapshot_frame(NOW).to_vec();
        let request = identity_request(&mut engine);
        result(respond(&mut engine, &request, 200, body), true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
    for network in [false, true] {
        let mut engine = seeded(options(false, Variant::Linear));
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
            safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        } else {
            respond(&mut engine, &request, 503, "unavailable")
        };
        result(actions, true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn public_only_and_missing_credentials_never_query_bybit_identity() {
    for opts in [
        Options::default(),
        Options {
            public_data_only: true,
            ..options(true, Variant::Spot)
        },
        Options {
            secret: Some(String::new()),
            ..options(false, Variant::Linear)
        },
        Options {
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read".into(),
                secret: String::new(),
                passphrase: None,
            }),
            ..options(false, Variant::Linear)
        },
    ] {
        let mut engine = seeded(opts);
        let before = engine.account_snapshot_frame(NOW).to_vec();
        result(ask(&mut engine), true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
}

#[test]
fn hyro_identity_is_refused_by_capability_before_io() {
    let mut engine = seeded(options(false, Variant::Hyrotrader));
    let before = engine.account_snapshot_frame(NOW).to_vec();
    assert_engine_refusal(ask(&mut engine));
    assert_eq!(engine.account_snapshot_frame(NOW), before);
}
