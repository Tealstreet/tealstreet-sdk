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
