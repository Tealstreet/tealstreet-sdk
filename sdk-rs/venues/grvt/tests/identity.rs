//! Identity uses the authenticated main-account header, never the trading subaccount.
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_grvt::{Grvt, Options};
const NOW: f64 = 1_700_000_000_000.;
const SUMMARY: &str = r#"{"r":{"sa":"777","te":"100","ab":"90"}}"#;
struct Harness {
    e: Engine<Grvt>,
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
}
impl Harness {
    fn new(options: Options) -> Self {
        let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(options));
        e.store_mut().set_balance(safe_cex_engine::model::Balance {
            total: 123.,
            free: 100.,
            used: 23.,
            upnl: 7.,
        });
        Self {
            e,
            requests: vec![],
            actions: vec![],
        }
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, NOW) {
            if let Action::Http(r) = a {
                self.requests.push(r);
            } else {
                self.actions.push(a);
            }
        }
    }
    fn ask(&mut self, command: u32) {
        self.e
            .handle(NOW, Event::Command(Command::FetchAccount { command }));
        self.collect();
    }
    fn take(&mut self, suffix: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix}: {:?}", self.requests));
        self.requests.remove(i)
    }
    fn respond(&mut self, r: &HttpRequest, status: u16, body: &str, headers: &str) {
        self.e.handle(
            NOW,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    body,
                    headers,
                },
            },
        );
        self.collect();
    }
    fn login(&mut self, account_id: &str) {
        let r = self.take("/auth/api_key/login");
        assert_eq!(r.method, Method::Post);
        assert_eq!(r.url, "https://edge.grvt.io/auth/api_key/login");
        assert_eq!(r.body.as_deref(), Some(r#"{"api_key":"primary-key"}"#));
        self.respond(
            &r,
            200,
            r#"{"status":"success","sub_account_id":"777"}"#,
            &format!("x-ts-grvt-session: fixture-session\nx-grvt-account-id: {account_id}"),
        );
    }
    fn output(&mut self, command: u32, rejected: bool) -> Result<String, String> {
        result(std::mem::take(&mut self.actions), command, rejected)
    }
}
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        trading_account_id: Some("777".into()),
        ..Default::default()
    }
}
fn loaded(main: &str) -> Harness {
    let mut h = Harness::new(options());
    h.ask(42);
    h.login(main);
    h
}

fn result(actions: Vec<Action>, command_id: u32, rejected: bool) -> Result<String, String> {
    let mut done = Vec::new();
    let mut errors = 0;
    for action in actions {
        match action {
            Action::Notice(Notice::CommandDone { command, result }) if command == command_id => {
                let result: Result<String, String> = Ok(result);
                done.push(result)
            }
            Action::Notice(Notice::CommandFailed { command, error }) if command == command_id => {
                let result: Result<String, String> = Err(error.legacy_text());
                done.push(result)
            }
            Action::Notice(Notice::CommandError { command, .. }) if command == command_id => {
                assert!(
                    done.is_empty(),
                    "command diagnostic must precede completion"
                );
                errors += 1;
            }
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error,
                ..
            })) => panic!("identity failures must be command-scoped"),
            _ => {}
        }
    }
    assert_eq!(done.len(), 1, "settles exactly once");
    assert_eq!(errors, usize::from(rejected));
    let out = done.remove(0);
    assert_eq!(out.is_err(), rejected);
    out
}

#[test]
fn identity_joins_login_then_validates_exact_route_and_returns_main_id_without_state_commit() {
    for main in ["0xmain-account", "900719925474099312345", "0"] {
        let mut h = Harness::new(options());
        h.ask(42);
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.ends_with("/account_summary"))
        );
        h.login(main);
        let r = h.take("/account_summary");
        assert_eq!(r.url, "https://trades.grvt.io/lite/v1/account_summary");
        assert_eq!(r.method, Method::Post);
        assert_eq!(r.route, None);
        assert_eq!(r.body.as_deref(), Some(r#"{"sub_account_id":"777"}"#));
        for (name, value) in [
            ("X-Grvt-Account-Id", main),
            ("Cookie", "rm=true; gravity=fixture-session"),
            ("X-Ts-Grvt-Session", "fixture-session"),
        ] {
            assert!(r.headers.iter().any(|(n, v)| *n == name && v == value));
        }
        let before = h.e.account_snapshot_frame(NOW).to_vec();
        h.respond(&r, 200, SUMMARY, "");
        assert_eq!(
            h.output(42, false).unwrap(),
            format!(r#"{{"userId":"{main}","affiliateId":null}}"#)
        );
        assert_eq!(h.e.account_snapshot_frame(NOW), before);
        h.respond(&r, 200, SUMMARY, "");
        assert!(h.actions.is_empty());
    }
}
#[test]
fn identity_accepts_current_response_header_rotation_but_rejects_older_session_response() {
    let mut h = loaded("old-main");
    let first = h.take("/account_summary");
    h.ask(43);
    let stale = h.take("/account_summary");
    let before = h.e.account_snapshot_frame(NOW).to_vec();
    h.respond(
        &first,
        200,
        SUMMARY,
        "x-ts-grvt-session: rotated-session\nx-grvt-account-id: new-main",
    );
    assert_eq!(
        h.output(42, false).unwrap(),
        r#"{"userId":"new-main","affiliateId":null}"#
    );
    h.respond(
        &stale,
        200,
        SUMMARY,
        "x-grvt-account-id: must-not-overwrite",
    );
    assert!(h.output(43, true).unwrap_err().contains("Stale"));
    assert_eq!(h.e.account_snapshot_frame(NOW), before);
    h.ask(44);
    let current = h.take("/account_summary");
    assert!(
        current
            .headers
            .iter()
            .any(|(n, v)| *n == "X-Grvt-Account-Id" && v == "new-main")
    );
}
#[test]
fn identity_expired_session_refreshes_login_once_and_uses_new_main_identity() {
    let mut h = loaded("old-main");
    let r = h.take("/account_summary");
    h.respond(&r, 200, r#"{"e":{"c":"1000"}}"#, "");
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
    h.login("new-main");
    let retry = h.take("/account_summary");
    assert_eq!(retry.body, r.body);
    h.respond(&retry, 200, SUMMARY, "");
    assert_eq!(
        h.output(42, false).unwrap(),
        r#"{"userId":"new-main","affiliateId":null}"#
    );
}
#[test]
fn identity_repeated_auth_rejection_is_bounded() {
    let mut h = loaded("main");
    let r = h.take("/account_summary");
    h.respond(&r, 200, r#"{"e":{"c":"1000"}}"#, "");
    h.login("main");
    let retry = h.take("/account_summary");
    h.respond(&retry, 200, r#"{"e":{"c":"1000"}}"#, "");
    h.output(42, true).unwrap_err();
}
#[test]
fn identity_rejects_missing_or_malformed_summary_and_http_errors_without_state_commit() {
    for (status, body) in [
        (200, "{}"),
        (200, r#"{"r":false}"#),
        (200, r#"{"r":[]}"#),
        (200, r#"{"r":{}}"#),
        (200, r#"{"r":{"te":"bad"}}"#),
        (200, r#"{"e":{"c":"123"}}"#),
        (503, SUMMARY),
    ] {
        let mut h = loaded("main");
        let r = h.take("/account_summary");
        let before = h.e.account_snapshot_frame(NOW).to_vec();
        h.respond(&r, status, body, "");
        h.output(42, true).unwrap_err();
        assert_eq!(h.e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_network_retries_are_bounded_and_keep_same_summary_request() {
    let mut h = loaded("main");
    let mut r = h.take("/account_summary");
    for attempt in 0..6 {
        h.e.handle(
            NOW,
            Event::HttpFailed {
                request: r.id,
                error: "network unavailable",
            },
        );
        h.collect();
        if attempt < 5 {
            let next = h.take("/account_summary");
            assert_eq!(next.body, r.body);
            r = next;
        } else {
            h.output(42, true).unwrap_err();
        }
    }
}
#[test]
fn identity_public_only_or_missing_key_rejects_before_login_io() {
    for opts in [
        Options {
            public_data_only: true,
            ..options()
        },
        Options::default(),
        Options {
            key: Some("".into()),
            ..options()
        },
    ] {
        let mut h = Harness::new(opts);
        h.ask(42);
        assert!(h.requests.is_empty());
        h.output(42, true).unwrap_err();
    }
}
#[test]
fn identity_failed_login_settles_all_query_waiters_once() {
    let mut h = Harness::new(options());
    h.ask(42);
    h.ask(43);
    assert_eq!(h.requests.len(), 1, "share one login");
    let login = h.take("/auth/api_key/login");
    h.respond(&login, 403, r#"{"e":{"c":"1001"}}"#, "");
    let actions = std::mem::take(&mut h.actions);
    let for_command = |id| {
        actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. } | Notice::CommandError { command, .. }) if *command == id)).cloned().collect()
    };
    result(for_command(42), 42, true).unwrap_err();
    result(for_command(43), 43, true).unwrap_err();
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("/account_summary"))
    );
}

#[test]
fn identity_default_subaccount_body_and_missing_authenticated_main_id_are_explicit() {
    let mut h = Harness::new(Options {
        trading_account_id: None,
        ..options()
    });
    h.ask(42);
    h.login("main");
    let r = h.take("/account_summary");
    assert_eq!(r.body.as_deref(), Some(r#"{"sub_account_id":""}"#));
    h.respond(&r, 200, SUMMARY, "");
    h.output(42, false).unwrap();

    let mut h = Harness::new(options());
    h.ask(42);
    let login = h.take("/auth/api_key/login");
    h.respond(
        &login,
        200,
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: cookie-without-main-id",
    );
    let scoped: Vec<_> = h
        .actions
        .into_iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(
                    Notice::CommandDone { command: 42, .. }
                        | Notice::CommandFailed { command: 42, .. }
                        | Notice::CommandError { command: 42, .. }
                )
            )
        })
        .collect();
    result(scoped, 42, true).unwrap_err();
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("/account_summary"))
    );
}
