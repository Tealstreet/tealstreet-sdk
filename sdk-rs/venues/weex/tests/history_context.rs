//! Original-source assertions: native wire positives precede custody REDs.
mod support;

use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Query, ReadCredentials,
    Response, Value, VenueError,
};
use safe_cex_venue_weex::WeexFutures;
use std::collections::VecDeque;
use support::setup::Options;

const NOW: f64 = 1_764_505_700_000.;
const NORMAL: &str = r#"[{"orderId":702345678901234567,"symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","status":"FILLED","type":"LIMIT","time":1764400000000,"price":"65000","origQty":"0.01","executedQty":"0.01","avgPrice":"64999.5","updateTime":1764400001234}]"#;

#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    prepared: usize,
    claimed: usize,
}

// Drive the real host protocol, while counting original legacy releases so
// the same assertion bodies expose the old path before the source cutover.
fn drain(e: &mut Engine<WeexFutures>, now: f64) -> Output {
    let mut output = Output::default();
    let mut queue: VecDeque<_> = e.drain_actions().collect();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::Http(request) => {
                output.legacy += 1;
                output.requests.push(request);
            }
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(e.http_preparation_pending(preparation));
                    output.prepared += 1;
                    e.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation.attempt().request, request.id);
                    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
                    output.claimed += 1;
                    output.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
            },
            action => output.actions.push(action),
        }
        queue.extend(e.drain_actions());
    }
    output
}

fn engine() -> Engine<WeexFutures> {
    support::setup::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    })
}

fn query(e: &mut Engine<WeexFutures>, read: Query, wait: u32) -> Output {
    e.handle_command(
        NOW,
        Command::Query {
            command: 7,
            query: read,
        },
        Some(wait),
    )
    .unwrap();
    drain(e, NOW)
}

fn orders() -> Query {
    Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}

fn income(ranged: bool) -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: ranged.then_some(NOW),
        to_ms: ranged.then_some(NOW + 1000.),
        limit: Some(1),
    }
}

fn public() -> Query {
    Query::PublicTrades {
        symbol: "BTCUSDT".into(),
        limit: Some(5),
    }
}

fn reply(e: &mut Engine<WeexFutures>, now: f64, request: &HttpRequest, body: &str) -> Output {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    drain(e, now)
}

fn terminals(output: &Output) -> Vec<Result<String, VenueError>> {
    output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(Err(error.clone())),
            _ => None,
        })
        .collect()
}

fn markers(output: &Output) -> usize {
    output.actions.iter().filter(|action| matches!(action,
        Action::Notice(Notice::Event(event))
        if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        && event.fields.iter().any(|(key, value)| *key == "command" && *value == Value::Num(7.))
    )).count()
}

fn strict_timeout(output: &Output) {
    let results = terminals(output);
    assert_eq!(results.len(), 1, "one root terminal: {results:?}");
    let error = results[0]
        .as_ref()
        .expect_err("root expiry cannot return accepted rows");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    assert_eq!(markers(output), 0);
    assert!(output.requests.is_empty());
}

#[test]
fn signed_list_releases_only_after_prepare_bind_claim() {
    let mut e = engine();
    let output = query(&mut e, orders(), 1000);
    assert_eq!(output.requests.len(), 1);
    let request = &output.requests[0];
    assert!(
        request
            .url
            .ends_with("/capi/v3/order/history?limit=100&page=0")
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "ACCESS-KEY" && value == "read-key")
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "ACCESS-PASSPHRASE" && value == "primary-pass")
    );
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
}

#[test]
fn signed_income_keeps_primary_and_releases_through_prepare() {
    let mut e = engine();
    let output = query(&mut e, income(false), 1000);
    assert_eq!(output.requests.len(), 1);
    let request = &output.requests[0];
    assert_eq!(request.method, safe_cex_engine::Method::Post);
    assert!(request.url.ends_with("/capi/v3/account/income"));
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "ACCESS-KEY" && value == "primary-key")
    );
    assert_eq!(
        request.body.as_deref(),
        Some(
            r#"{"asset":"","symbol":"","incomeType":"","startTime":null,"endTime":null,"limit":1}"#
        )
    );
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
}

#[test]
fn public_history_releases_through_prepare_without_signed_headers() {
    let mut e = engine();
    let output = query(&mut e, public(), 1000);
    assert_eq!(output.requests.len(), 1);
    assert!(
        !output.requests[0]
            .headers
            .iter()
            .any(|(key, _)| key.starts_with("ACCESS-"))
    );
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
}

#[test]
fn public_history_expires_at_the_original_caller_deadline() {
    let mut e = engine();
    let output = query(&mut e, public(), 100);
    assert_eq!(output.requests.len(), 1);
    e.handle(NOW + 100., Event::PollScaleChanged { scale: 1. });
    strict_timeout(&drain(&mut e, NOW + 100.));
}

#[test]
fn order_page_failure_at_root_equality_never_recovers_accepted_rows() {
    let mut e = engine();
    let first = query(&mut e, orders(), 100);
    let next = reply(&mut e, NOW + 10., &first.requests[0], NORMAL);
    assert_eq!(next.requests.len(), 1);
    assert!(next.requests[0].url.contains("allAlgoOrders"));
    e.handle(
        NOW + 100.,
        Event::HttpFailed {
            request: next.requests[0].id,
            error: "page reset",
        },
    );
    strict_timeout(&drain(&mut e, NOW + 100.));
}

#[test]
fn parked_income_cannot_send_another_page_after_caller_expiry() {
    let mut e = engine();
    let first = query(&mut e, income(true), 50);
    let rows: Vec<_> = (1..=100).map(|id| format!(
        r#"{{"billId":"{id}","symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"1","asset":"USDT","time":{}}}"#, NOW + f64::from(id)
    )).collect();
    let next = reply(
        &mut e,
        NOW + 10.,
        &first.requests[0],
        &format!(r#"{{"items":[{}]}}"#, rows.join(",")),
    );
    assert!(terminals(&next).is_empty());
    let timer = next
        .actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 110. => Some(*timer),
            _ => None,
        })
        .expect("native 100ms page delay");
    e.handle(NOW + 110., Event::TimerFired { timer });
    strict_timeout(&drain(&mut e, NOW + 110.));
}

#[test]
fn a_live_root_later_page_failure_keeps_only_the_accepted_normal_rows() {
    let mut e = engine();
    let first = query(&mut e, orders(), 1000);
    let next = reply(&mut e, NOW + 10., &first.requests[0], NORMAL);
    assert_eq!(next.requests.len(), 1);
    e.handle(
        NOW + 20.,
        Event::HttpFailed {
            request: next.requests[0].id,
            error: "page reset",
        },
    );
    let output = drain(&mut e, NOW + 20.);
    let results = terminals(&output);
    assert_eq!(results.len(), 1);
    let body = results[0].as_ref().expect("accepted-only native recovery");
    let rows: Vec<serde_json::Value> = serde_json::from_str(body).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "702345678901234567");
    assert_eq!(markers(&output), 1);
    assert!(output.requests.is_empty());
}

#[test]
fn native_public_empty_response_completes_once_inside_the_bound() {
    let mut e = engine();
    let first = query(&mut e, public(), 1000);
    let output = reply(&mut e, NOW + 10., &first.requests[0], "[]");
    assert_eq!(terminals(&output), [Ok("[]".into())]);
    assert_eq!(markers(&output), 0);
}

#[test]
fn empty_fills_windows_do_not_authorize_later_partial_success() {
    let mut e = engine();
    let mut output = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(NOW - 30. * 86_400_000.),
            to_ms: Some(NOW),
            limit: Some(1),
        },
        1000,
    );
    for page in 1..=3 {
        assert_eq!(output.requests.len(), 1);
        assert!(output.requests[0].url.contains("userTrades"));
        output = reply(&mut e, NOW + f64::from(page), &output.requests[0], "[]");
        assert!(terminals(&output).is_empty());
    }
    assert_eq!(output.requests.len(), 1, "fourth native fills window");
    e.handle(
        NOW + 4.,
        Event::HttpFailed {
            request: output.requests[0].id,
            error: "empty-prefix page reset",
        },
    );
    let failed = drain(&mut e, NOW + 4.);
    let results = terminals(&failed);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_err(), "empty accepted prefix cannot recover");
    assert_eq!(markers(&failed), 0);
    assert!(failed.requests.is_empty());
    assert_eq!(
        failed
            .actions
            .iter()
            .filter(|action| matches!(action,
                Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::REQUEST_FAILED
            ))
            .count(),
        1
    );
}

fn secondary_only_private(read: Query) {
    let mut e = support::setup::engine(Options {
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    });
    let output = query(&mut e, read, 1000);
    assert_eq!(terminals(&output), [Ok("[]".into())]);
    assert_eq!(markers(&output), 0);
    assert!(output.requests.is_empty());
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 0, 0));
    assert!(
        output
            .actions
            .iter()
            .all(|action| matches!(action, Action::Notice(_)))
    );
}

#[test]
fn secondary_without_primary_keeps_private_fills_local_empty() {
    secondary_only_private(Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
}

#[test]
fn secondary_without_primary_keeps_private_orders_local_empty() {
    secondary_only_private(orders());
}

#[test]
fn secondary_without_primary_keeps_private_income_local_empty() {
    secondary_only_private(income(true));
}

#[test]
fn a_history_429_keeps_the_native_pause_and_request_failed_notice() {
    let mut e = engine();
    let first = query(&mut e, public(), 2000);
    e.handle(
        NOW + 10.,
        Event::HttpResponse {
            request: first.requests[0].id,
            response: Response {
                status: 429,
                headers: "Retry-After: 1",
                body: "rate limited",
            },
        },
    );
    let failed = drain(&mut e, NOW + 10.);
    assert_eq!(terminals(&failed).len(), 1);
    assert!(terminals(&failed)[0].is_err());
    assert_eq!(
        failed
            .actions
            .iter()
            .filter(|action| matches!(action,
                Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::REQUEST_FAILED
            ))
            .count(),
        1
    );
    e.handle_command(
        NOW + 11.,
        Command::Query {
            command: 7,
            query: public(),
        },
        Some(2000),
    )
    .unwrap();
    let parked = drain(&mut e, NOW + 11.);
    assert!(parked.requests.is_empty());
    assert_eq!((parked.prepared, parked.claimed), (0, 0));
    assert!(terminals(&parked).is_empty());
    let timer = parked
        .actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1010. => Some(*timer),
            _ => None,
        })
        .expect("Retry-After keeps the public history parked");
    e.handle(NOW + 1010., Event::TimerFired { timer });
    let released = drain(&mut e, NOW + 1010.);
    assert_eq!(released.requests.len(), 1);
    assert!(released.requests[0].url.contains("market/trades"));
    assert!(released.requests[0].headers.is_empty());
    let done = reply(&mut e, NOW + 1011., &released.requests[0], "[]");
    assert_eq!(terminals(&done), [Ok("[]".into())]);
}

fn issue_at(
    e: &mut Engine<WeexFutures>,
    now: f64,
    command: u32,
    read: Query,
    wait: Option<u32>,
) -> Output {
    e.handle_command(
        now,
        Command::Query {
            command,
            query: read,
        },
        wait,
    )
    .unwrap();
    drain(e, now)
}

fn long_orders() -> Query {
    Query::OrderHistory {
        symbol: None,
        from_ms: Some(NOW - 300. * 86_400_000.),
        to_ms: Some(NOW),
        limit: None,
    }
}

fn empty_page(request: &HttpRequest) -> &'static str {
    if request.url.contains("allAlgoOrders") {
        r#"{"orders":[],"hasMore":false}"#
    } else {
        "[]"
    }
}

fn active_timers(output: &Output, deadline: f64) -> Vec<safe_cex_engine::TimerId> {
    let cleared: Vec<_> = output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::ClearTimer { timer } => Some(*timer),
            _ => None,
        })
        .collect();
    output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == deadline && !cleared.contains(timer) => {
                Some(*timer)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_default_receipt_deadline_survives_signed_endpoint_restarts() {
    let mut e = engine();
    let mut output = issue_at(&mut e, NOW, 7, long_orders(), None);
    for page in 1..=6 {
        assert_eq!(output.requests.len(), 1);
        let request = &output.requests[0];
        assert_eq!(request.timeout_ms, Some(20_000));
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == "ACCESS-KEY" && value == "read-key")
        );
        output = reply(
            &mut e,
            NOW + f64::from(page) * 19_000.,
            request,
            empty_page(request),
        );
        assert!(terminals(&output).is_empty());
    }
    assert_eq!(output.requests.len(), 1);
    assert_eq!(output.requests[0].timeout_ms, Some(6_000));
    e.handle(NOW + 120_000., Event::PollScaleChanged { scale: 1. });
    strict_timeout(&drain(&mut e, NOW + 120_000.));
}

#[test]
fn a_raised_wait_completes_the_same_signed_walk_beyond_the_default_wait() {
    let mut e = engine();
    let mut output = issue_at(&mut e, NOW, 7, long_orders(), Some(200_000));
    for page in 1..=8 {
        assert_eq!(output.requests.len(), 1, "native page {page}");
        let request = &output.requests[0];
        assert_eq!(request.timeout_ms, Some(20_000));
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == "ACCESS-KEY" && value == "read-key")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == "ACCESS-PASSPHRASE" && value == "primary-pass")
        );
        output = reply(
            &mut e,
            NOW + f64::from(page) * 19_000.,
            request,
            empty_page(request),
        );
        if page < 8 {
            assert!(terminals(&output).is_empty());
        }
    }
    assert_eq!(terminals(&output), [Ok("[]".into())]);
    assert_eq!(markers(&output), 0);
    assert!(output.requests.is_empty());
}

fn fractional_orders() -> (Engine<WeexFutures>, Output) {
    let mut e = engine();
    let first = issue_at(&mut e, NOW + 0.25, 7, orders(), Some(100));
    let next = reply(&mut e, NOW + 10.25, &first.requests[0], NORMAL);
    assert_eq!(next.requests.len(), 1);
    (e, next)
}

#[test]
fn half_a_millisecond_before_root_expiry_still_allows_native_prefix_recovery() {
    let (mut e, next) = fractional_orders();
    e.handle(
        NOW + 99.75,
        Event::HttpFailed {
            request: next.requests[0].id,
            error: "live fractional failure",
        },
    );
    let output = drain(&mut e, NOW + 99.75);
    let result = terminals(&output);
    assert_eq!(result.len(), 1);
    let rows: Vec<serde_json::Value> = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(markers(&output), 1);
    assert!(output.requests.is_empty());
}

#[test]
fn fractional_root_equality_rejects_without_native_prefix_recovery() {
    let (mut e, next) = fractional_orders();
    e.handle(
        NOW + 100.25,
        Event::HttpFailed {
            request: next.requests[0].id,
            error: "expired fractional failure",
        },
    );
    strict_timeout(&drain(&mut e, NOW + 100.25));
}

#[test]
fn an_actual_child_timer_with_a_live_longer_root_recovers_only_accepted_rows() {
    let mut e = engine();
    let first = query(&mut e, orders(), 100_000);
    let next = reply(&mut e, NOW + 10., &first.requests[0], NORMAL);
    assert_eq!(next.requests[0].timeout_ms, Some(20_000));
    let timers = active_timers(&next, NOW + 20_010.);
    assert_eq!(timers.len(), 1, "exact released second-page timer");
    e.handle(NOW + 20_010., Event::TimerFired { timer: timers[0] });
    let output = drain(&mut e, NOW + 20_010.);
    let result = terminals(&output);
    assert_eq!(result.len(), 1);
    let rows: Vec<serde_json::Value> = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(markers(&output), 1);
    assert!(output.requests.is_empty());
}

fn equal_deadline_timer_order(root_first: bool) {
    let mut e = engine();
    let first = query(&mut e, orders(), 100);
    // Root admission emits its timer before the first physical preparation.
    let root = active_timers(&first, NOW + 100.)[0];
    let next = reply(&mut e, NOW + 10., &first.requests[0], NORMAL);
    let physical = active_timers(&next, NOW + 100.);
    assert_eq!(physical.len(), 1, "exact released second-page timer");
    assert_ne!(root, physical[0]);
    let (first, stale) = if root_first {
        (root, physical[0])
    } else {
        (physical[0], root)
    };
    e.handle(NOW + 100., Event::TimerFired { timer: first });
    strict_timeout(&drain(&mut e, NOW + 100.));
    e.handle(NOW + 100.5, Event::TimerFired { timer: stale });
    let late_timer = drain(&mut e, NOW + 100.5);
    assert!(terminals(&late_timer).is_empty());
    assert_eq!(markers(&late_timer), 0);
    let late = reply(
        &mut e,
        NOW + 101.,
        &next.requests[0],
        r#"{"orders":[{"invalid":true}]}"#,
    );
    assert!(terminals(&late).is_empty());
    assert_eq!(markers(&late), 0);
    assert!(late.requests.is_empty());
}

#[test]
fn root_timer_before_equal_page_timer_is_strict_and_late_data_is_inert() {
    equal_deadline_timer_order(true);
}

#[test]
fn page_timer_before_equal_root_timer_is_strict_and_late_data_is_inert() {
    equal_deadline_timer_order(false);
}

#[test]
fn expired_generation_cannot_complete_its_reused_id_or_retire_a_live_peer() {
    let mut e = engine();
    let first = query(&mut e, orders(), 100);
    let root_timer = active_timers(&first, NOW + 100.)[0];
    let old = reply(&mut e, NOW + 10., &first.requests[0], NORMAL);
    let peer = issue_at(&mut e, NOW + 20., 8, public(), Some(1000));
    assert_eq!(peer.requests.len(), 1);
    e.handle(NOW + 100., Event::TimerFired { timer: root_timer });
    strict_timeout(&drain(&mut e, NOW + 100.));
    let current = issue_at(&mut e, NOW + 101., 7, orders(), Some(1000));
    assert_eq!(current.requests.len(), 1);
    assert_ne!(old.requests[0].id, current.requests[0].id);
    let stale = reply(
        &mut e,
        NOW + 102.,
        &old.requests[0],
        r#"{"orders":[{"invalid":true}]}"#,
    );
    assert!(terminals(&stale).is_empty());
    assert_eq!(markers(&stale), 0);
    assert!(stale.requests.is_empty());
    e.handle(NOW + 103., Event::TimerFired { timer: root_timer });
    let stale_timer = drain(&mut e, NOW + 103.);
    assert!(terminals(&stale_timer).is_empty());
    assert!(stale_timer.requests.is_empty());
    let peer_done = reply(&mut e, NOW + 104., &peer.requests[0], "[]");
    assert_eq!(
        peer_done
            .actions
            .iter()
            .filter(|action| matches!(action,
                Action::Notice(Notice::CommandDone { command: 8, result }) if result == "[]"
            ))
            .count(),
        1
    );
    assert!(terminals(&peer_done).is_empty());
    let next = reply(&mut e, NOW + 105., &current.requests[0], "[]");
    assert_eq!(next.requests.len(), 1);
    let done = reply(&mut e, NOW + 106., &next.requests[0], r#"{"orders":[]}"#);
    assert_eq!(terminals(&done), [Ok("[]".into())]);
    assert_eq!(markers(&done), 0);
}

#[test]
fn second_page_preparation_refusal_is_strict_after_accepted_rows() {
    let mut e = engine();
    let first = query(&mut e, orders(), 1000);
    e.handle(
        NOW + 10.,
        Event::HttpResponse {
            request: first.requests[0].id,
            response: Response {
                status: 200,
                headers: "",
                body: NORMAL,
            },
        },
    );
    let pending: Vec<_> = e.drain_actions().collect();
    let preparation = pending
        .into_iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("second-page route preparation");
    let cause = VenueError::new("route reservation refused")
        .kind(ErrorKind::Internal)
        .outcome(safe_cex_engine::Outcome::NotSent);
    e.http_preparation_failed(
        NOW + 11.,
        preparation,
        safe_cex_engine::request_context::Error::Capacity,
        cause.clone(),
    );
    let refused = drain(&mut e, NOW + 11.);
    assert_eq!(terminals(&refused), [Err(cause)]);
    assert_eq!(markers(&refused), 0);
    assert!(refused.requests.is_empty());
}

#[test]
fn provider_refusal_after_accepted_rows_keeps_typed_fields_without_auth_evidence() {
    let mut e = support::setup::engine_with_provider(
        Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..Default::default()
        },
        |inner| std::sync::Arc::new(support::setup::Gated::new(inner, &[true, false])),
    );
    let first = query(&mut e, orders(), 1000);
    assert_eq!((first.legacy, first.prepared, first.claimed), (0, 1, 1));
    let refused = reply(&mut e, NOW + 10., &first.requests[0], NORMAL);
    assert_eq!(
        terminals(&refused),
        [Err(
            safe_cex_engine::signing::SignError::Refused.venue_error()
        )]
    );
    assert_eq!(markers(&refused), 0);
    assert!(refused.requests.is_empty());
    assert!(!refused.actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::AccountAuth { .. } | Notice::Deactivate)
    )));
    assert_eq!(e.health().auth_failures, 0);
}
