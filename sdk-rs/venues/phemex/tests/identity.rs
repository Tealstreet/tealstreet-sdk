//! Native rare identity reads: route/signer, exact IDs, failure ownership and no state commits.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;
const NOW: f64 = 1000.0;
fn engine(read: bool, public: bool) -> Engine<PhemexPerpetuals> {
    let mut e = support::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        public_data_only: public,
        ..Default::default()
    });

    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.0,
        free: 100.0,
        used: 23.0,
        upnl: 7.0,
    });
    e.drain_actions().for_each(drop);
    e
}
fn ask(e: &mut Engine<PhemexPerpetuals>) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    e.drain_actions().collect()
}
fn request(e: &mut Engine<PhemexPerpetuals>) -> HttpRequest {
    let actions = ask(e);
    let mut reads = actions.into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let r = reads.next().expect("identity HTTP request");
    assert!(reads.next().is_none(), "identity requires one rare read");
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(r.route, None, "use the selected ordinary read egress");
    assert_eq!(
        r.url,
        "https://api.phemex.com/g-accounts/accountPositions?currency=USDT"
    );
    r
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
                assert!(done.is_empty(), "diagnostic before Done");
                errors.push(message);
            }
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error,
                ..
            })) => panic!("identity error must be correlated"),
            Action::Http(_) => panic!("identity must not trigger account reconciliation"),
            _ => {}
        }
    }
    assert_eq!(done.len(), 1);
    assert_eq!(errors.len(), usize::from(rejected));
    let result = done.remove(0);
    assert_eq!(result.is_err(), rejected);
    result
}
fn respond(
    e: &mut Engine<PhemexPerpetuals>,
    r: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
#[test]
fn identity_returns_exact_response_ids_without_mutating_live_account() {
    let mut e = engine(false, false);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == "primary-key")
    );
    let out = respond(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":{"account":{"userID":9007199254740993,"currency":"USDT","accountBalanceRv":"1000"},"positions":[]}}"#,
    );
    assert_eq!(
        result(out, false).unwrap(),
        r#"{"userId":"9007199254740993"}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(respond(&mut e,&r,200,r#"{"code":0,"data":{"account":{"userID":9007199254740993,"currency":"USDT","accountBalanceRv":"1000"},"positions":[]}}"#).is_empty(),"duplicate response is consumed once");
}
#[test]
fn identity_uses_secondary_read_signer_and_contract_fallback() {
    let mut e = engine(true, false);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == "read-key")
    );
    let out = respond(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":{"account":{"userID":"9007199254740994"}}}"#,
    );
    assert_eq!(
        result(out, false).unwrap(),
        r#"{"userId":"9007199254740994"}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn identity_missing_false_empty_or_failed_read_rejects_once_and_preserves_state() {
    for body in [
        r#"{"code":0,"data":{}}"#,
        r#"{"code":0,"data":{"account":{"userID":false}}}"#,
        r#"{"code":0,"data":{"account":{"userID":""}}}"#,
        r#"{"code":10001,"data":{"account":{"userID":"fake"}}}"#,
    ] {
        let mut e = engine(false, false);
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        result(respond(&mut e, &r, 200, body), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
    for network in [false, true] {
        let mut e = engine(false, false);
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        let out = if network {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: r.id,
                    error: "connection lost",
                },
            );
            e.drain_actions().collect()
        } else {
            respond(&mut e, &r, 503, "unavailable")
        };
        result(out, true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_public_only_fails_before_network_even_with_saved_credentials() {
    let mut e = engine(true, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    result(ask(&mut e), true).unwrap_err();
    assert_eq!(e.account_snapshot_frame(NOW), before);
}

#[test]
fn identity_without_credentials_fails_before_network() {
    let mut e = support::engine(Options::default());
    result(ask(&mut e), true).unwrap_err();
}

#[test]
fn identity_rejects_incomplete_saved_read_credentials_without_primary_fallback() {
    let mut e = support::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: String::new(),
            passphrase: None,
        }),
        ..Default::default()
    });
    let error = result(ask(&mut e), true).unwrap_err();
    assert!(error.contains("Secondary read credentials"));
}
