//! Hostile delivery and failure controls through real managed HTTP handoffs.
mod support;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Permit, Preparation, Prepared};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Query, Response, VenueError,
};
use support::{Driver, SYMBOL, ready};

fn query(limit: u32) -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(limit),
    }
}
fn preparation(actions: Vec<Action>) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "managed preparation never releases physical HTTP"
    );
    let mut found = actions.into_iter().filter_map(|a| match a {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => Some((preparation, request)),
            _ => None,
        },
        _ => None,
    });
    let result = found.next().expect("one genuine managed preparation");
    assert!(found.next().is_none());
    result
}
fn enter(e: &mut Driver, command: u32, wait: u32, limit: u32) -> (Preparation, HttpRequest) {
    e.handle_command(
        2000.,
        Command::Query {
            command,
            query: query(limit),
        },
        Some(wait),
    )
    .unwrap();
    e.prepare_dispatch(2000.);
    preparation(e.drain_actions().collect())
}
fn bind(e: &mut Driver, token: Preparation, now: f64) -> (Permit, HttpRequest) {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    e.prepare_dispatch(now);
    let mut binds = e.drain_actions().filter_map(|a| match a {
        Action::ManagedHttp(a) => match *a {
            HttpAction::Bind {
                permit, request, ..
            } => Some((permit, request)),
            _ => None,
        },
        _ => None,
    });
    let result = binds.next().expect("the exact prepared route binds");
    assert!(binds.next().is_none());
    result
}
fn release(e: &mut Driver, token: Preparation, now: f64) -> HttpRequest {
    let (permit, request) = bind(e, token, now);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    e.drain_actions().for_each(drop);
    request
}
fn response(e: &mut Driver, request: &HttpRequest, now: f64, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn terminals(actions: &[Action], command: u32) -> (Vec<&str>, Vec<&VenueError>) {
    let mut success = vec![];
    let mut failure = vec![];
    for action in actions {
        match action {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => success.push(result.as_str()),
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                failure.push(error)
            }
            _ => {}
        }
    }
    (success, failure)
}
fn no_send(actions: &[Action]) {
    assert!(!actions.iter().any(|a| match a {
        Action::Http(_) => true,
        Action::ManagedHttp(a) =>
            matches!(**a, HttpAction::Prepare { .. } | HttpAction::Bind { .. }),
        _ => false,
    }));
}
#[test]
fn delivered_body_before_claim_cannot_stage_rows_or_diagnostics() {
    let (mut e, _) = ready();
    let (token, preview) = enter(&mut e, 71, 1000, 2);
    e.handle(
        2001.,
        Event::HttpStart {
            request: preview.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        2001.,
        Event::HttpChunk {
            request: preview.id,
            data: br#"[{"side":"Unknown"},"#,
        },
    );
    e.handle(
        2001.,
        Event::HttpEnd {
            request: preview.id,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    assert!(
        actions.iter().all(|a| !matches!(a, Action::Notice(_))),
        "unreleased DATA has no rows, diagnostics or terminals"
    );
    let request = release(&mut e, token, 2002.);
    response(&mut e, &request, 2003., 200, "[]");
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&actions, 71).0, ["[]"]);
    assert!(terminals(&actions, 71).1.is_empty());
}
#[test]
fn preparation_after_original_expiry_cannot_bind_or_publish() {
    let (mut e, _) = ready();
    let (token, preview) = enter(&mut e, 71, 37, 2);
    e.http_prepared(
        2038.,
        token,
        Prepared {
            id: preview.id.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    assert!(terminals(&actions, 71).0.is_empty());
    assert!(!e.http_preparation_pending(token));
    response(&mut e, &preview, 2039., 200, "[]");
    let late: Vec<_> = e.drain_actions().collect();
    no_send(&late);
    assert!(terminals(&late, 71).0.is_empty());
}
#[test]
fn bound_executor_cannot_claim_after_original_expiry() {
    let (mut e, _) = ready();
    let (token, _) = enter(&mut e, 71, 37, 2);
    let (permit, request) = bind(&mut e, token, 2001.);
    assert!(e.claim_http(2038., permit).is_err());
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    assert!(terminals(&actions, 71).0.is_empty());
    response(&mut e, &request, 2039., 200, "[]");
    assert!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 71)
            .0
            .is_empty()
    );
}
#[test]
fn first_preparation_refusal_preserves_typed_not_sent_cause() {
    let (mut e, _) = ready();
    let (token, _) = enter(&mut e, 71, 1000, 2);
    e.http_preparation_failed(
        2001.,
        token,
        Error::Invalid,
        VenueError::new("route rejected"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    let (success, failed) = terminals(&actions, 71);
    assert!(success.is_empty());
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].message, "route rejected");
    assert_eq!(failed[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(failed[0].outcome, Some(Outcome::NotSent));
}
fn fill(id: u64, symbol: &str) -> String {
    format!(
        r#"{{"orderId":"123","symbol":"{symbol}","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":{id},"timestamp":"2025-12-15T13:28:00Z"}}"#
    )
}
fn accepted_prefix(e: &mut Driver) -> (Preparation, HttpRequest) {
    let (first, _) = enter(e, 71, 50_000, 2);
    let first = release(e, first, 2001.);
    response(
        e,
        &first,
        2010.,
        200,
        &format!("[{},{}]", fill(1, SYMBOL), fill(2, "UNLISTED_PERP")),
    );
    e.prepare_dispatch(2010.);
    preparation(e.drain_actions().collect())
}
fn partial(actions: &[Action]) {
    no_send(actions);
    let (success, errors) = terminals(actions, 71);
    assert!(errors.is_empty());
    assert_eq!(success.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(success[0]).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "1");
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "history.incomplete")).count(), 1);
    assert!(!success[0].contains("999"));
}
#[test]
fn page_two_preparation_refusal_keeps_only_accepted_prefix() {
    let (mut e, _) = ready();
    let (token, _) = accepted_prefix(&mut e);
    e.http_preparation_failed(
        2011.,
        token,
        Error::Invalid,
        VenueError::new("page2 preparation refusal"),
    );
    partial(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn page_two_released_network_status_and_codec_failures_discard_unaccepted_rows() {
    for mode in 0..3 {
        let (mut e, _) = ready();
        let (token, _) = accepted_prefix(&mut e);
        let request = release(&mut e, token, 2011.);
        match mode {
            0 => {
                e.handle(
                    2012.,
                    Event::HttpStart {
                        request: request.id,
                        status: 200,
                        headers: "",
                    },
                );
                let prefix = format!("[{},", fill(999, SYMBOL));
                e.handle(
                    2012.,
                    Event::HttpChunk {
                        request: request.id,
                        data: prefix.as_bytes(),
                    },
                );
                e.handle(
                    2013.,
                    Event::HttpFailed {
                        request: request.id,
                        error: "page2 lost",
                    },
                );
            }
            1 => response(
                &mut e,
                &request,
                2012.,
                503,
                &format!("[{}]", fill(999, SYMBOL)),
            ),
            _ => response(
                &mut e,
                &request,
                2012.,
                200,
                &format!("[{},", fill(999, SYMBOL)),
            ),
        }
        partial(&e.drain_actions().collect::<Vec<_>>());
    }
}
#[test]
fn page_two_released_physical_timeout_preserves_prefix_while_root_is_live() {
    let (mut e, _) = ready();
    let (token, _) = accepted_prefix(&mut e);
    let (permit, request) = bind(&mut e, token, 2011.);
    let released = match e.claim_http(2011., permit).unwrap() {
        Claim::Released(release) => release,
        _ => panic!("release"),
    };
    e.drain_actions().for_each(drop);
    assert!(released.deadline < 52_000., "the caller root is still live");
    // Admission of this real HTTP event expires its released physical leg.
    response(
        &mut e,
        &request,
        released.deadline,
        200,
        &format!("[{}]", fill(999, SYMBOL)),
    );
    partial(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn duplicate_cannot_replace_live_history_and_original_still_delivers() {
    let (mut e, _) = ready();
    let (token, _) = enter(&mut e, 71, 1000, 2);
    e.handle_command(
        2001.,
        Command::Query {
            command: 71,
            query: query(2),
        },
        Some(1000),
    )
    .unwrap();
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    let (success, failed) = terminals(&actions, 71);
    assert!(success.is_empty() && failed.is_empty());
    let request = release(&mut e, token, 2002.);
    response(&mut e, &request, 2003., 200, "[]");
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 71).0,
        ["[]"]
    );
}

fn park_depth(e: &mut Driver, command: u32) -> Preparation {
    e.handle_command(
        2000.,
        Command::FetchOrderBook {
            command,
            symbol: SYMBOL.into(),
            limit: None,
        },
        Some(50_000),
    )
    .unwrap();
    e.prepare_dispatch(2000.);
    preparation(e.drain_actions().collect()).0
}
fn finish_depth(e: &mut Driver, token: Preparation, command: u32) {
    let request = release(e, token, 2015.);
    response(e, &request, 2016., 200, r#"{"bids":[],"asks":[]}"#);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&actions, command).0, [r#"{"bids":[],"asks":[]}"#]);
    assert!(terminals(&actions, command).1.is_empty());
}
#[test]
fn history_capacity_refusal_preserves_a_real_prior_owner() {
    let (mut e, _) = ready();
    let first = park_depth(&mut e, 1000);
    for command in 1001..1256 {
        park_depth(&mut e, command);
    }
    e.handle_command(
        2001.,
        Command::Query {
            command: 71,
            query: query(2),
        },
        Some(50_000),
    )
    .unwrap();
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    let (success, failures) = terminals(&actions, 71);
    assert!(success.is_empty());
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::Internal));
    assert_eq!(failures[0].outcome, None);
    finish_depth(&mut e, first, 1000);
}
#[test]
fn full_context_pressure_retires_page_one_before_minting_page_two() {
    let (mut e, _) = ready();
    let (first, _) = enter(&mut e, 71, 50_000, 2);
    let parked = park_depth(&mut e, 1000);
    for command in 1001..1254 {
        park_depth(&mut e, command);
    }
    let first = release(&mut e, first, 2001.);
    response(
        &mut e,
        &first,
        2010.,
        200,
        &format!("[{},{}]", fill(1, SYMBOL), fill(2, "UNLISTED_PERP")),
    );
    e.prepare_dispatch(2010.);
    let (second, _) = preparation(e.drain_actions().collect());
    let second = release(&mut e, second, 2011.);
    response(&mut e, &second, 2012., 200, "[]");
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    let (success, failures) = terminals(&actions, 71);
    assert!(failures.is_empty());
    assert_eq!(success.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(success[0]).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "1");
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == "history.incomplete")));
    finish_depth(&mut e, parked, 1000);
}

#[test]
fn retired_page_data_cannot_publish_diagnostics_or_complete_its_successor() {
    let (mut e, _) = ready();
    let (first, _) = enter(&mut e, 71, 50_000, 2);
    let first = release(&mut e, first, 2001.);
    response(
        &mut e,
        &first,
        2010.,
        200,
        &format!("[{},{}]", fill(1, SYMBOL), fill(2, "UNLISTED_PERP")),
    );
    e.prepare_dispatch(2010.);
    let (second, _) = preparation(e.drain_actions().collect());
    e.handle(
        2011.,
        Event::HttpStart {
            request: first.id,
            status: 503,
            headers: "",
        },
    );
    e.handle(
        2011.,
        Event::HttpChunk {
            request: first.id,
            data: br#"[{"side":"Unknown"},"#,
        },
    );
    e.handle(2011., Event::HttpEnd { request: first.id });
    let stale: Vec<_> = e.drain_actions().collect();
    no_send(&stale);
    assert!(
        !stale
            .iter()
            .any(|action| matches!(action, Action::Notice(_)))
    );
    let second = release(&mut e, second, 2012.);
    response(&mut e, &second, 2013., 200, "[]");
    let actions: Vec<_> = e.drain_actions().collect();
    let (success, errors) = terminals(&actions, 71);
    assert!(errors.is_empty());
    assert_eq!(success.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(success[0]).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "1");
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == "history.incomplete")));
}
