//! Synthetic native HTTP responses only; no network or account credentials.
mod support;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, Response};
use support::{Options, TestEngine};

fn reply(e: &mut TestEngine, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        100.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn messages(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) => Some(event.message.as_str()),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.message.as_str()),
            _ => None,
        })
        .collect()
}
fn assert_diagnostic(actions: &[Action], class: &str, code: Option<&str>) {
    let logs = messages(actions);
    let message = logs
        .iter()
        .find(|message| message.contains("MEXC HTTP 425"))
        .unwrap_or_else(|| panic!("missing HTTP status: {logs:?}"));
    assert!(
        message.contains(&format!("requestClass={class}")),
        "{message}"
    );
    if let Some(code) = code {
        assert!(message.contains(&format!("venueCode={code}")), "{message}");
    } else {
        assert!(
            !message.contains("venueCode="),
            "unknown code must stay absent: {message}"
        );
    }
    for message in logs {
        for private in [
            "PRIVATE-CANARY",
            "api.mexc.com",
            "scripted-key",
            "scripted-secret",
            "?",
            "<html>",
        ] {
            assert!(
                !message.contains(private),
                "raw response/request material in diagnostic: {message}"
            );
        }
    }
}

#[test]
fn account_snapshot_http_425_identifies_each_closed_family_without_retrying() {
    for (path, class) in [
        ("account/assets", "accountAssets"),
        ("position/open_positions", "accountPositions"),
        ("order/list/open_orders", "accountOrders"),
        ("planorder/list/orders", "accountPlans"),
        ("stoporder/list/orders", "accountStops"),
    ] {
        let (mut e, _, reads, _) = support::private_started();
        let request = reads.iter().find(|r| r.url.contains(path)).unwrap();
        let actions = reply(&mut e, request, 425, "<html>PRIVATE-CANARY</html>");
        assert_diagnostic(&actions, class, None);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "no immediate retry"
        );
        assert!(
            !e.store().loaded().orders,
            "failed full snapshots cannot publish completion"
        );
    }
}

#[test]
fn account_http_425_retains_only_present_valid_numeric_envelope_codes() {
    for (body, code) in [
        (
            r#"{"success":true,"code":600,"message":"PRIVATE-CANARY","data":null}"#,
            Some("600"),
        ),
        (
            r#"{"success":true,"code":"600","message":"PRIVATE-CANARY"}"#,
            Some("600"),
        ),
        (
            r#"{"success":true,"code":0,"data":"PRIVATE-CANARY"}"#,
            Some("0"),
        ),
        (r#"{"success":true,"message":"PRIVATE-CANARY"}"#, None),
        (
            r#"{"success":true,"code":null,"message":"PRIVATE-CANARY"}"#,
            None,
        ),
        (r#"{"code":"PRIVATE-CANARY"}"#, None),
        (r#"{"code":0.5}"#, None),
        (r#"{"code":-1}"#, None),
        (r#"{"code":9007199254740992}"#, None),
        ("not JSON PRIVATE-CANARY", None),
    ] {
        let (mut e, _, reads, _) = support::private_started();
        let request = reads
            .iter()
            .find(|r| r.url.contains("account/assets"))
            .unwrap();
        assert_diagnostic(&reply(&mut e, request, 425, body), "accountAssets", code);
    }
}

#[test]
fn identity_http_425_stays_scoped_to_its_command_and_retains_the_family() {
    let mut e = support::configured(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    });
    e.handle(100., Event::Command(Command::FetchAccount { command: 42 }));
    let request = support::prepared(&mut e, 100.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let actions = reply(&mut e, &request, 425, "PRIVATE-CANARY");
    assert_diagnostic(&actions, "identity", None);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn public_http_425_retains_the_markets_family_and_existing_backoff() {
    let mut e = support::configured(Options {
        public_data_only: true,
        ..Default::default()
    });
    e.start(0.);
    let request = support::public_requests(&mut e, 0.).remove(0);
    let actions = reply(&mut e, &request, 425, "PRIVATE-CANARY");
    assert_diagnostic(&actions, "markets", None);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(
        |a| matches!(a, Action::SetTimer { at_ms, .. } if support::jittered(*at_ms - 100., 1000.))
    ));
}

#[test]
fn ordinary_http_failures_keep_their_existing_text_and_success_still_commits() {
    let (mut e, _, reads, _) = support::private_started();
    let assets = reads
        .iter()
        .find(|r| r.url.contains("account/assets"))
        .unwrap();
    let actions = reply(&mut e, assets, 503, "PRIVATE-CANARY");
    assert!(messages(&actions).contains(&"MEXC HTTP 503"));
    let (mut healthy, _, reads, _) = support::private_started();
    for request in reads {
        reply(
            &mut healthy,
            &request,
            200,
            r#"{"success":true,"code":0,"data":[]}"#,
        );
    }
    assert!(healthy.store().loaded().orders);
    assert!(healthy.store().loaded().positions);
}
