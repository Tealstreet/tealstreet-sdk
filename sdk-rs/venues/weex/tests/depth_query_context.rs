//! Compiling original assertions for public command snapshots; no source adapter.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Response, TimerId,
};
use safe_cex_venue_weex::WeexFutures;
use std::collections::VecDeque;
use support::setup::Options;

const BOOK: &str = r#"{"lastUpdateId":"77","bids":[["1","2"]],"asks":[]}"#;
#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    prepared: usize,
    claimed: usize,
}
fn drain(engine: &mut Engine<WeexFutures>, now: f64) -> Output {
    let mut output = Output::default();
    let mut queue: VecDeque<_> = engine.drain_actions().collect();
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
                    assert!(engine.http_preparation_pending(preparation));
                    output.prepared += 1;
                    engine.http_prepared(
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
                    assert!(matches!(
                        engine.claim_http(now, permit),
                        Ok(Claim::Released(_))
                    ));
                    output.claimed += 1;
                    output.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
            },
            action => output.actions.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}
fn engine() -> Engine<WeexFutures> {
    support::setup::engine(Options::default())
}
fn ask(engine: &mut Engine<WeexFutures>, now: f64, id: u32, wait: u32) -> Output {
    engine
        .handle_command(
            now,
            Command::FetchOrderBook {
                command: id,
                symbol: "BTC /&USDT".into(),
                limit: Some(1),
            },
            Some(wait),
        )
        .unwrap();
    drain(engine, now)
}
fn reply(
    engine: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    now: f64,
    status: u16,
    headers: &str,
    body: &str,
) -> Output {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
    drain(engine, now)
}
fn terminal(action: &Action, id: u32) -> bool {
    matches!(action, Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. }) if *command == id)
}
fn strict_timeout(actions: &[Action], id: u32) {
    assert_eq!(
        actions.iter().filter(|a| terminal(a, id)).count(),
        1,
        "one root terminal at the original receipt deadline"
    );
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some((error.kind, error.outcome))
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors, vec![(Some(ErrorKind::Timeout), None)]);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command, .. }) if *command == id)
    ));
}
fn assert_success(actions: &[Action], id: u32) {
    assert_eq!(actions.iter().filter(|a| terminal(a, id)).count(), 1);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command, result }) if *command == id && result == r#"{"bids":[{"price":1.0,"amount":2.0,"total":2.0}],"asks":[],"lastUpdateId":"77"}"#)));
}
#[test]
fn public_snapshot_native_wire_precedes_managed_custody_assertion() {
    let mut e = engine();
    let mut out = ask(&mut e, 1000., 7, 120_000);
    assert_eq!(out.requests.len(), 1);
    let request = out.requests.remove(0);
    assert_eq!(request.method, Method::Get);
    assert_eq!(
        request.url,
        "https://api-contract.weex.com/capi/v3/market/depth?symbol=BTC%20%2F%26USDT&limit=1"
    );
    assert!(request.headers.is_empty());
    assert!(request.body.is_none());
    assert!(e.store().markets().is_empty());
    assert_eq!((out.legacy, out.prepared, out.claimed), (0, 1, 1));
}
#[test]
fn configured_receipt_caps_the_physical_call_before_release() {
    let mut e = engine();
    let out = ask(&mut e, 1000., 7, 1234);
    assert_eq!(out.requests.len(), 1);
    assert_eq!(out.requests[0].timeout_ms, Some(1234));
}
#[test]
fn raised_receipt_keeps_the_native_twenty_second_physical_bound() {
    let mut e = engine();
    let out = ask(&mut e, 1000., 7, 240_000);
    assert_eq!(out.requests.len(), 1);
    assert_eq!(out.requests[0].timeout_ms.unwrap_or(20_000), 20_000);
}
#[test]
fn data_at_the_exact_caller_deadline_cannot_complete_successfully() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 100).requests.remove(0);
    let out = reply(&mut e, &request, 1100., 200, "", BOOK);
    strict_timeout(&out.actions, 7);
    assert!(out.requests.is_empty());
}
#[test]
fn fractional_receipt_is_live_half_a_millisecond_before_equality() {
    let mut e = engine();
    let request = ask(&mut e, 1000.25, 7, 100).requests.remove(0);
    let out = reply(&mut e, &request, 1099.75, 200, "", BOOK);
    assert_success(&out.actions, 7);
}
fn pause(e: &mut Engine<WeexFutures>) {
    let request = ask(e, 1000., 7, 120_000).requests.remove(0);
    let out = reply(
        e,
        &request,
        1001.,
        429,
        "Retry-After: 3\r\nX-REMAINING-WEIGHT-1M: 0",
        "malformed",
    );
    assert_eq!(out.actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(out.actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.kind == Some(ErrorKind::RateLimited) && error.retry_after_ms == Some(3000.))));
    assert!(out.requests.is_empty());
}
fn pause_timer(out: &Output) -> TimerId {
    out.actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 4001. => Some(*timer),
            _ => None,
        })
        .expect("native shared pause at 4001")
}
#[test]
fn paused_query_expires_before_dequeue_without_minting_a_child() {
    let mut e = engine();
    pause(&mut e);
    let out = ask(&mut e, 1002., 8, 1000);
    assert!(out.requests.is_empty());
    assert_eq!((out.legacy, out.prepared, out.claimed), (0, 0, 0));
    let timer = pause_timer(&out);
    e.handle(2002., Event::PollScaleChanged { scale: 1. });
    let expired = drain(&mut e, 2002.);
    strict_timeout(&expired.actions, 8);
    e.handle(4001., Event::TimerFired { timer });
    let late = drain(&mut e, 4001.);
    assert!(late.requests.is_empty());
    assert_eq!(late.prepared, 0);
    assert!(!late.actions.iter().any(|a| terminal(a, 8)));
}
#[test]
fn live_query_after_the_native_pause_preserves_public_snapshot_output() {
    let mut e = engine();
    pause(&mut e);
    let parked = ask(&mut e, 1002., 8, 5000);
    assert!(parked.requests.is_empty());
    let timer = pause_timer(&parked);
    e.handle(4001., Event::TimerFired { timer });
    let mut out = drain(&mut e, 4001.);
    assert_eq!(out.requests.len(), 1);
    let reply = reply(&mut e, &out.requests.remove(0), 4002., 200, "", BOOK);
    assert_success(&reply.actions, 8);
    assert!(e.store().markets().is_empty());
}

#[test]
fn expired_429_cannot_park_an_unrelated_live_query() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 100).requests.remove(0);
    let late = reply(
        &mut e,
        &request,
        1100.,
        429,
        "Retry-After: 3\r\nX-REMAINING-WEIGHT-1M: 0",
        "malformed",
    );
    let next = ask(&mut e, 1101., 8, 5000);
    assert_eq!(
        next.requests.len(),
        1,
        "expired 429 has no shared-pause authority"
    );
    strict_timeout(&late.actions, 7);
}
