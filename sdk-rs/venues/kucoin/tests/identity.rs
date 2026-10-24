//! Actual native identity requests: exact routes/signers, failure ownership, no Store commits.
mod support;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_kucoin::Kucoin;
use support::credentials::{BrokerOptions, Options};
const NOW: f64 = 1000.;
type TestEngine = Engine<Kucoin>;
fn seeded(options: Options) -> TestEngine {
    decorate(support::credentials::engine(options, Kucoin::new))
}
fn decorate(mut engine: TestEngine) -> TestEngine {
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
fn options(read: bool) -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    }
}
#[test]
fn identity_uses_futures_route_read_signer_passphrase_and_broker_provenance() {
    for read in [false, true] {
        let mut opts = options(read);
        opts.broker = Some(BrokerOptions {
            partner: "partner".into(),
            broker_key: "broker-secret".into(),
            broker_name: "Broker".into(),
            verify: None,
        });
        let mut engine = seeded(opts);
        let before = engine.account_snapshot_frame(NOW).to_vec();
        let request = identity_request(&mut engine);
        assert_eq!(
            request.url,
            "https://api-futures.kucoin.com/api/v1/user-info"
        );
        let (key, secret, pass) = if read {
            ("read-key", "read-secret", "read-pass")
        } else {
            ("primary-key", "primary-secret", "primary-pass")
        };
        assert_eq!(header(&request, "KC-API-KEY"), key);
        assert_eq!(header(&request, "KC-API-KEY-VERSION"), "2");
        assert_eq!(
            header(&request, "KC-API-SIGN"),
            hmac_sha256(
                secret.as_bytes(),
                b"1000GET/api/v1/user-info",
                Encoding::Base64
            )
        );
        assert_eq!(
            header(&request, "KC-API-PASSPHRASE"),
            hmac_sha256(secret.as_bytes(), pass.as_bytes(), Encoding::Base64)
        );
        assert_eq!(
            header(&request, "KC-API-PARTNER-SIGN"),
            hmac_sha256(
                b"broker-secret",
                format!("1000partner{key}").as_bytes(),
                Encoding::Base64
            )
        );
        assert_eq!(header(&request, "KC-API-PARTNER-VERIFY"), "true");
        let body = r#"{"code":"200000","data":{"userId":9007199254740993,"subUserId":9007199254740995,"referrerUserId":9007199254740997}}"#;
        assert_eq!(
            result(respond(&mut engine, &request, 200, body), false).unwrap(),
            r#"{"userId":"9007199254740993","subId":"9007199254740995","affiliateId":"9007199254740997"}"#
        );
        assert_eq!(engine.account_snapshot_frame(NOW), before);
        assert!(respond(&mut engine, &request, 200, body).is_empty());
    }
}
#[test]
fn optional_zero_null_empty_and_text_ids_keep_distinct_meaning() {
    for (extra, expected) in [
        ("", r#"{"userId":"own","subId":null,"affiliateId":null}"#),
        (
            r#","subUserId":0,"referrerUserId":0"#,
            r#"{"userId":"own","subId":null,"affiliateId":null}"#,
        ),
        (
            r#","subUserId":null,"referrerUserId":"""#,
            r#"{"userId":"own","subId":null,"affiliateId":null}"#,
        ),
        (
            r#","subUserId":"0","referrerUserId":"0""#,
            r#"{"userId":"own","subId":"0","affiliateId":"0"}"#,
        ),
    ] {
        let mut engine = seeded(options(false));
        let request = identity_request(&mut engine);
        let body = format!(r#"{{"code":"200000","data":{{"userId":"own"{extra}}}}}"#);
        assert_eq!(
            result(respond(&mut engine, &request, 200, &body), false).unwrap(),
            expected
        );
    }
}
#[test]
fn secondary_passphrase_falls_back_without_changing_selected_key() {
    let mut opts = options(true);
    opts.read_credentials.as_mut().unwrap().passphrase = None;
    let mut engine = seeded(opts);
    let request = identity_request(&mut engine);
    assert_eq!(header(&request, "KC-API-KEY"), "read-key");
    assert_eq!(
        header(&request, "KC-API-PASSPHRASE"),
        hmac_sha256(b"read-secret", b"primary-pass", Encoding::Base64)
    );
}
#[test]
fn failed_or_missing_identity_rejects_once_instead_of_empty_success() {
    for body in [
        r#"{"code":"200000","data":{}}"#,
        r#"{"code":"200000","data":{"userId":false}}"#,
        r#"{"code":"200000","data":{"userId":""}}"#,
        r#"{"code":"400001","msg":"denied","data":null}"#,
        "malformed",
    ] {
        let mut engine = seeded(options(false));
        let before = engine.account_snapshot_frame(NOW).to_vec();
        let request = identity_request(&mut engine);
        result(respond(&mut engine, &request, 200, body), true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
    for network in [false, true] {
        let mut engine = seeded(options(false));
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
fn private_identity_requires_credentials_before_any_network() {
    for variant in 0..5 {
        // Deliberately malformed opaque options exercise the venue boundary;
        // trusted setup's malformed raw-input refusal is covered separately.
        let engine = support::credentials::engine(options(true), |mut options| {
            match variant {
                0 => {
                    options.credentials = None;
                    options.read_credentials = None;
                }
                1 => options.public_data_only = true,
                2 => options.credentials.as_mut().unwrap().passphrase = None,
                3 => options.read_credentials.as_mut().unwrap().passphrase = None,
                _ => options.read_credentials.as_mut().unwrap().signing.0.clear(),
            }
            Kucoin::new(options)
        });
        let mut engine = decorate(engine);
        result(ask(&mut engine), true).unwrap_err();
    }
}
