use super::*;
use safe_cex_engine::request_context::{HttpAction, Prepared, RoutingIntent};

fn cancel() -> Command {
    Command::CancelOrders {
        command: 1,
        ids: vec!["one".into()],
    }
}
fn raw_cancel(h: &mut Harness) -> Vec<Action> {
    h.engine.handle(h.now, Event::Command(cancel()));
    h.engine.drain_actions().collect()
}
fn prepared(actions: &[Action]) -> safe_cex_engine::request_context::Preparation {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "no raw trading transport before Prepare: {actions:?}"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Primary);
                    assert!(request.url.ends_with("/api/v1/orders/one"));
                    assert_eq!(request.method, Method::Delete);
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("a genuine Primary trading Prepare")
}

#[test]
fn cancel_requires_real_primary_preparation() {
    let mut h = Harness::new();
    let actions = raw_cancel(&mut h);
    prepared(&actions);
}

#[test]
fn preparation_at_the_original_twenty_second_bound_cannot_send() {
    let mut h = Harness::new();
    let actions = raw_cancel(&mut h);
    let preparation = prepared(&actions);
    h.engine.http_prepared(
        h.now + 20_000.,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now + 20_000.);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 1, error })
                if error.kind == Some(safe_cex_engine::ErrorKind::Timeout)
                    && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
        )),
        "original call bound owns one unreleased terminal: {actions:?}"
    );
}

#[test]
fn unclaimed_cancel_data_cannot_complete_or_advance_the_command() {
    let mut h = Harness::new();
    let actions = raw_cancel(&mut h);
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { request, .. } => Some(request.id),
                _ => None,
            },
            _ => None,
        })
        .expect("raw native compatibility or unclaimed managed ingress");
    h.event(Event::HttpResponse {
        request,
        response: Response {
            status: 200,
            headers: "",
            body: r#"{"code":"200000","data":{"cancelledOrderIds":["one"]}}"#,
        },
    });
    if h.actions
        .iter()
        .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.))
    {
        h.pace();
    }
    assert!(
        !h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: 1, .. } | Notice::CommandFailed { command: 1, .. }
            )
        )),
        "unclaimed DATA owns no terminal: {:?}",
        h.actions
    );
}

#[test]
fn a_nonfinite_entry_refuses_before_native_queue_or_signing() {
    let mut h = Harness::new();
    h.engine.handle(f64::NAN, Event::Command(cancel()));
    let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, f64::NAN);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    assert!(
        actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 1, error })
                if error.kind == Some(safe_cex_engine::ErrorKind::InvalidInput)
                    && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
        )),
        "shared entry validation precedes queue work: {actions:?}"
    );
}

#[test]
fn native_400005_resends_once_at_every_ruled_status_with_fresh_signing() {
    for status in [200, 401, 408, 500, 503, 599] {
        let mut h = Harness::new();
        h.command(cancel());
        let first = h.take("/api/v1/orders/one");
        h.now += 1.;
        h.response(&first, status, r#"{"code":"400005","msg":"clock refused"}"#);
        let second = h.take("/api/v1/orders/one");
        assert_ne!(first.id, second.id);
        assert_eq!(first.method, second.method);
        assert_eq!(first.url, second.url);
        assert_eq!(first.body, second.body);
        for name in ["KC-API-TIMESTAMP", "KC-API-SIGN"] {
            assert_ne!(
                first.headers.iter().find(|(key, _)| *key == name),
                second.headers.iter().find(|(key, _)| *key == name)
            );
        }
        h.response(
            &second,
            status,
            r#"{"code":"400005","msg":"clock refused again"}"#,
        );
        assert!(!h.has("/api/v1/orders/one"), "no third send at {status}");
        h.pace();
        assert!(
            !h.has("/api/v1/orders/one"),
            "no third send after the terminal timer at {status}"
        );
        assert!(h.done(1).is_err());
    }
}

#[test]
fn native_uncertain_cancel_still_continues_its_independent_sibling() {
    let mut h = Harness::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["one".into(), "two".into()],
    });
    let first = h.take("/api/v1/orders/one");
    h.response(&first, 503, "not JSON");
    h.pace();
    let second = h.take("/api/v1/orders/two");
    h.response(
        &second,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["two"]}}"#,
    );
    h.pace();
    assert!(h.done(1).is_err());
    assert_eq!(
        h.failures.last().unwrap().outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
}
