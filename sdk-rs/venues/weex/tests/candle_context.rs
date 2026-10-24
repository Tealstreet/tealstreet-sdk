//! Original candle-query assertions against the unmodified native venue.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Response, TimerId,
    Venue,
};
use safe_cex_venue_weex::WeexFutures;
use std::collections::VecDeque;
use support::setup::Options;

const ROWS: &str = r#"[[1700000060000,"3572.1","3574","3571.8","3573.4","18.2"],[1700000000000,"3571","3573","3570","3572","10",1700000059999,"35720",9,"5","17860"]]"#;
const SORTED: &str = r#"[{"timestamp":1700000000.0,"open":3571.0,"high":3573.0,"low":3570.0,"close":3572.0,"volume":10.0},{"timestamp":1700000060.0,"open":3572.1,"high":3574.0,"low":3571.8,"close":3573.4,"volume":18.2}]"#;
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
fn ask(e: &mut Engine<WeexFutures>, now: f64, id: u32, wait: u32, historical: bool) -> Output {
    e.handle_command(
        now,
        Command::FetchCandles {
            command: id,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: Some(if historical { 200 } else { 0 }),
            from_ms: historical.then_some(1_700_000_000_000.),
            to_ms: historical.then_some(1_700_000_120_000.),
        },
        Some(wait),
    )
    .unwrap();
    drain(e, now)
}
fn reply(
    e: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    now: f64,
    status: u16,
    headers: &str,
    body: &str,
) -> Output {
    e.handle(
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
    drain(e, now)
}
fn terminal(a: &Action, id: u32) -> bool {
    matches!(a, Action::Notice(Notice::CommandDone {command, ..} | Notice::CommandFailed {command, ..}) if *command == id)
}
fn success(actions: &[Action], id: u32, expected: &str) {
    assert_eq!(actions.iter().filter(|a| terminal(a, id)).count(), 1);
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandDone {command, result}) if *command == id && result == expected
    )));
}
fn timeout(actions: &[Action], id: u32) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some((error.kind, error.outcome))
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors, vec![(Some(ErrorKind::Timeout), None)]);
    assert_eq!(actions.iter().filter(|a| terminal(a, id)).count(), 1);
}
fn failure_message(actions: &[Action], id: u32, message: &str) {
    assert_eq!(actions.iter().filter(|a| terminal(a, id)).count(), 1);
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed {command, error}) if *command == id && error.message == message
    )));
    warning(actions, message);
}
fn warning(actions: &[Action], message: &str) {
    let warnings = actions.iter().filter(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.level == safe_cex_engine::Level::Warn && event.code == safe_cex_engine::log::code::REQUEST_FAILED && event.message == format!("WEEX: {message}")
    )).count();
    assert_eq!(warnings, 1, "one native candle request warning");
}
fn internal(actions: &[Action], id: u32) {
    let causes: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some((error.kind, error.outcome))
            }
            _ => None,
        })
        .collect();
    assert_eq!(causes, vec![(Some(ErrorKind::Internal), None)]);
}
fn wire(out: &Output, historical: bool) {
    assert_eq!(out.requests.len(), 1);
    let request = &out.requests[0];
    let expected = if historical {
        "https://api-contract.weex.com/capi/v3/market/historyKlines?interval=1m&limit=100&symbol=BTCUSDT&startTime=1700000000000&endTime=1700000120000&priceType=LAST"
    } else {
        "https://api-contract.weex.com/capi/v3/market/klines?interval=1m&limit=1&symbol=BTCUSDT"
    };
    assert_eq!(request.url, expected);
    assert_eq!(request.method, Method::Get);
    assert!(request.headers.is_empty());
    assert!(request.body.is_none());
    assert!(!out.actions.iter().any(|a| matches!(
        a,
        Action::Sign { .. } | Action::Notice(Notice::Deactivate | Notice::AccountAuth { .. })
    )));
}
#[test]
fn live_candle_wire_positives_precede_managed_release_assertion() {
    let mut e = engine();
    let out = ask(&mut e, 1000., 7, 120_000, false);
    wire(&out, false);
    assert_eq!((out.legacy, out.prepared, out.claimed), (0, 1, 1));
}
#[test]
fn historical_candle_wire_positives_precede_managed_release_assertion() {
    let mut e = engine();
    let out = ask(&mut e, 1000., 7, 120_000, true);
    wire(&out, true);
    assert_eq!((out.legacy, out.prepared, out.claimed), (0, 1, 1));
}
#[test]
fn caller_receipt_caps_the_physical_candle_timeout() {
    let mut e = engine();
    let out = ask(&mut e, 1000., 7, 1234, false);
    assert_eq!(out.requests.len(), 1);
    assert_eq!(out.requests[0].timeout_ms, Some(1234));
}
#[test]
fn candle_data_at_receipt_equality_cannot_publish() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 100, true).requests.remove(0);
    let out = reply(&mut e, &request, 1100., 200, "", ROWS);
    timeout(&out.actions, 7);
    assert!(out.requests.is_empty());
}
fn pause(e: &mut Engine<WeexFutures>) {
    let request = ask(e, 1000., 7, 120_000, false).requests.remove(0);
    let out = reply(
        e,
        &request,
        1001.,
        429,
        "Retry-After: 3\r\nX-REMAINING-WEIGHT-1M: 0",
        "busy",
    );
    assert_eq!(out.actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(out.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed {command: 7, error}) if error.message == "HTTP 429: busy"
    )));
    assert!(out.requests.is_empty());
    warning(&out.actions, "HTTP 429: busy");
}
fn pause_timer(out: &Output) -> TimerId {
    out.actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 4001. => Some(*timer),
            _ => None,
        })
        .expect("native pause deadline")
}
#[test]
fn paused_candle_query_expires_without_a_physical_child() {
    let mut e = engine();
    pause(&mut e);
    let parked = ask(&mut e, 1002., 8, 1000, false);
    assert!(parked.requests.is_empty());
    assert_eq!((parked.legacy, parked.prepared, parked.claimed), (0, 0, 0));
    let timer = pause_timer(&parked);
    e.handle(2002., Event::PollScaleChanged { scale: 1. });
    let expired = drain(&mut e, 2002.);
    timeout(&expired.actions, 8);
    e.handle(4001., Event::TimerFired { timer });
    let late = drain(&mut e, 4001.);
    assert!(late.requests.is_empty());
    assert_eq!(late.prepared, 0);
    assert!(!late.actions.iter().any(|a| terminal(a, 8)));
}
#[test]
fn expired_candle_429_cannot_pause_an_unrelated_live_query() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 100, false).requests.remove(0);
    let late = reply(
        &mut e,
        &request,
        1100.,
        429,
        "Retry-After: 3\r\nX-REMAINING-WEIGHT-1M: 0",
        "busy",
    );
    let next = ask(&mut e, 1101., 8, 5000, false);
    assert_eq!(
        next.requests.len(),
        1,
        "late expired 429 cannot park the peer"
    );
    timeout(&late.actions, 7);
}
#[test]
fn native_candle_rows_keep_variable_lengths_sorting_and_exact_output() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 120_000, true).requests.remove(0);
    let out = reply(&mut e, &request, 1001., 200, "", ROWS);
    success(&out.actions, 7, SORTED);
    assert!(e.store().markets().is_empty());
    assert!(e.venue().book("BTCUSDT").is_none());
}
#[test]
fn native_empty_candles_complete_once() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 120_000, false).requests.remove(0);
    let out = reply(&mut e, &request, 1001., 200, "", "[]");
    success(&out.actions, 7, "[]");
}
#[test]
fn fractional_candle_receipt_remains_live_before_equality() {
    let mut e = engine();
    let request = ask(&mut e, 1000.25, 7, 100, true).requests.remove(0);
    let out = reply(&mut e, &request, 1099.75, 200, "", ROWS);
    success(&out.actions, 7, SORTED);
}
#[test]
fn raised_caller_wait_preserves_effective_twenty_second_call_bound() {
    let mut e = engine();
    let out = ask(&mut e, 1000., 7, 240_000, false);
    assert_eq!(out.requests.len(), 1);
    assert_eq!(out.requests[0].timeout_ms.unwrap_or(20_000), 20_000);
}
#[test]
fn unsupported_rest_interval_keeps_native_local_message() {
    let mut e = engine();
    e.handle_command(
        1000.,
        Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: Interval::H2,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
        Some(120_000),
    )
    .unwrap();
    let out = drain(&mut e, 1000.);
    assert!(out.requests.is_empty());
    assert_eq!((out.legacy, out.prepared, out.claimed), (0, 0, 0));
    assert_eq!(out.actions.iter().filter(|a| terminal(a, 7)).count(), 1);
    assert!(out.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed {command: 7, error}) if error.message == Interval::H2.unsupported("WEEX")
    )));
}
#[test]
fn live_candle_query_resumes_after_the_native_pause() {
    let mut e = engine();
    pause(&mut e);
    let parked = ask(&mut e, 1002., 8, 5000, false);
    assert!(parked.requests.is_empty());
    let timer = pause_timer(&parked);
    e.handle(4001., Event::TimerFired { timer });
    let mut out = drain(&mut e, 4001.);
    assert_eq!(out.requests.len(), 1);
    let out = reply(&mut e, &out.requests.remove(0), 4002., 200, "", "[]");
    success(&out.actions, 8, "[]");
}

#[test]
fn a_missing_streamed_array_keeps_its_message_and_delivered_internal_cause() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 120_000, false).requests.remove(0);
    let out = reply(&mut e, &request, 1001., 200, "", "{}");
    failure_message(
        &out.actions,
        7,
        "response did not contain the expected list: {}",
    );
    internal(&out.actions, 7);
}

#[test]
fn a_broken_stream_keeps_its_message_and_delivered_internal_cause() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 120_000, false).requests.remove(0);
    e.handle(
        1001.,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        1002.,
        Event::HttpChunk {
            request: request.id,
            data: b"{",
        },
    );
    e.handle(
        1003.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    let out = drain(&mut e, 1003.);
    failure_message(
        &out.actions,
        7,
        "response did not contain the expected list: {",
    );
    internal(&out.actions, 7);
}

#[test]
fn a_malformed_candle_row_preserves_its_native_error_and_warning() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 120_000, false).requests.remove(0);
    let out = reply(&mut e, &request, 1001., 200, "", "[[1]]");
    failure_message(&out.actions, 7, "malformed REST row");
}

#[test]
fn a_non_rate_limited_http_error_preserves_its_native_error_and_warning() {
    let mut e = engine();
    let request = ask(&mut e, 1000., 7, 120_000, false).requests.remove(0);
    let out = reply(&mut e, &request, 1001., 500, "", "bad");
    failure_message(&out.actions, 7, "HTTP 500: bad");
}
