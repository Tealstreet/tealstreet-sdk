//! Actual native identity requests: exact routes/signers, failure ownership, no Store commits.
use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_bitget::{BitgetFutures, Product};
const NOW: f64 = 1000.;
type TestEngine = Engine<BitgetFutures>;
fn seeded(options: Options) -> TestEngine {
    let mut engine = crate::support::engine(options);
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
    engine.drain_actions().collect()
}
fn identity_request(engine: &mut TestEngine) -> HttpRequest {
    let actions = ask(engine);
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
fn options(read: bool, product: Product) -> Options {
    Options {
        product,
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
fn every_product_uses_spot_identity_route_and_selected_read_credentials() {
    for product in [Product::Usdt, Product::Usdc, Product::Inverse] {
        for read in [false, true] {
            for testnet in [false, true] {
                let mut opts = options(read, product);
                opts.testnet = testnet;
                let mut engine = seeded(opts);
                let before = engine.account_snapshot_frame(NOW).to_vec();
                let request = identity_request(&mut engine);
                assert_eq!(
                    request.url,
                    "https://api.bitget.com/api/v2/spot/account/info"
                );
                let (key, secret, pass) = if read {
                    ("read-key", "read-secret", "read-pass")
                } else {
                    ("primary-key", "primary-secret", "primary-pass")
                };
                assert_eq!(header(&request, "ACCESS-KEY"), key);
                assert_eq!(header(&request, "ACCESS-PASSPHRASE"), pass);
                assert_eq!(
                    header(&request, "ACCESS-SIGN"),
                    hmac_sha256(
                        secret.as_bytes(),
                        b"1000GET/api/v2/spot/account/info",
                        Encoding::Base64
                    )
                );
                let body = r#"{"code":"00000","data":{"userId":9007199254740993,"parentId":9007199254740995,"inviterId":9007199254740997}}"#;
                assert_eq!(
                    result(respond(&mut engine, &request, 200, body), false).unwrap(),
                    r#"{"userId":"9007199254740995","subId":"9007199254740993","affiliateId":"9007199254740997"}"#
                );
                assert_eq!(engine.account_snapshot_frame(NOW), before);
                assert!(respond(&mut engine, &request, 200, body).is_empty());
            }
        }
    }
}
#[test]
fn parent_sub_affiliate_zero_null_and_text_semantics_are_preserved() {
    for (fields, expected) in [
        (
            r#""userId":"own","parentId":0,"inviterId":0"#,
            r#"{"userId":"own","subId":null,"affiliateId":null}"#,
        ),
        (
            r#""userId":"own","parentId":null,"inviterId":null"#,
            r#"{"userId":"own","subId":null,"affiliateId":null}"#,
        ),
        (
            r#""userId":"own","parentId":"","inviterId":"""#,
            r#"{"userId":"own","subId":null,"affiliateId":null}"#,
        ),
        (
            r#""userId":"own","parentId":"0","inviterId":"0""#,
            r#"{"userId":"0","subId":"own","affiliateId":"0"}"#,
        ),
        (
            r#""userId":"masked-***""#,
            r#"{"userId":"masked-***","subId":null,"affiliateId":null}"#,
        ),
    ] {
        let mut engine = seeded(options(false, Product::Usdt));
        let request = identity_request(&mut engine);
        let body = format!(r#"{{"code":"00000","data":{{{fields}}}}}"#);
        assert_eq!(
            result(respond(&mut engine, &request, 200, &body), false).unwrap(),
            expected
        );
    }
}
#[test]
fn secondary_passphrase_falls_back_to_primary_without_changing_identity_account() {
    let mut opts = options(true, Product::Usdt);
    opts.read_credentials.as_mut().unwrap().passphrase = None;
    let mut engine = seeded(opts);
    let request = identity_request(&mut engine);
    assert_eq!(header(&request, "ACCESS-KEY"), "read-key");
    assert_eq!(header(&request, "ACCESS-PASSPHRASE"), "primary-pass");
}
#[test]
fn failures_reject_once_preserve_state_and_do_not_start_account_detection() {
    for body in [
        r#"{"code":"00000","data":{}}"#,
        r#"{"code":"00000","data":{"userId":false}}"#,
        r#"{"code":"00000","data":{"userId":"","parentId":"parent"}}"#,
        r#"{"code":"40001","msg":"denied","data":null}"#,
        "malformed",
    ] {
        let mut engine = seeded(options(false, Product::Usdt));
        let before = engine.account_snapshot_frame(NOW).to_vec();
        let request = identity_request(&mut engine);
        result(respond(&mut engine, &request, 200, body), true).unwrap_err();
        assert_eq!(engine.account_snapshot_frame(NOW), before);
    }
    for network in [false, true] {
        let mut engine = seeded(options(false, Product::Usdt));
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
fn private_only_requirements_fail_before_network() {
    for (index, opts) in [
        Options::default(),
        Options {
            public_data_only: true,
            ..options(true, Product::Usdc)
        },
        Options {
            passphrase: Some(String::new()),
            ..options(false, Product::Usdt)
        },
        Options {
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read".into(),
                secret: "read-secret".into(),
                passphrase: Some(String::new()),
            }),
            ..options(false, Product::Usdt)
        },
        Options {
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read".into(),
                secret: String::new(),
                passphrase: None,
            }),
            ..options(false, Product::Usdt)
        },
    ]
    .into_iter()
    .enumerate()
    {
        if matches!(index, 2 | 3) {
            assert!(matches!(
                crate::support::try_engine(opts),
                Err(safe_cex_engine::signing::SignError::InvalidRequest)
            ));
            continue;
        }
        let mut engine = seeded(opts);
        result(ask(&mut engine), true).unwrap_err();
    }
}
