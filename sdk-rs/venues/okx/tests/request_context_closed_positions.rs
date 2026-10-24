//! Original actual-venue controls for caller-root closed-position adoption.
#![allow(dead_code)]
mod support;

use safe_cex_engine::request_context::{HttpAction, Preparation};
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, ReadCredentials,
    Response, TimerId, VenueAction,
};
use safe_cex_venue_okx::Okx;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const NOW: f64 = 1_710_000_000_000.0;
const OWNER: u32 = 911;

fn ready() -> Engine<Okx> {
    let mut engine = support::trading_engine();
    engine.drain_actions().for_each(drop);
    engine.take_frame(NOW);
    engine
}

fn command(symbol: Option<&str>, from_ms: Option<f64>, to_ms: Option<f64>) -> Command {
    Command::Venue {
        command: OWNER,
        action: VenueAction::FetchClosedPositions {
            symbol: symbol.map(Into::into),
            from_ms,
            to_ms,
        },
    }
}

fn fetch(engine: &mut Engine<Okx>, at: f64, symbol: Option<&str>, wait: Option<u32>) {
    engine
        .handle_command(at, command(symbol, None, None), wait)
        .expect("valid host receipt");
}

fn prepared(engine: &mut Engine<Okx>, at: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, at)
}

fn request(output: &[Action]) -> HttpRequest {
    let requests: Vec<_> = output
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.contains("/account/positions-history?") => {
                Some(request.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "one actual closed-position request");
    requests.into_iter().next().unwrap()
}

fn reply(engine: &mut Engine<Okx>, request: &HttpRequest, at: f64, body: &str) {
    engine.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn row(index: usize, time: u64) -> String {
    format!(
        r#"{{"instId":"BTC-USDT-SWAP","posId":"{index}","uTime":"{time}","closeTotalPos":"1.000","fee":"-0.1000","unknown":{{"keep":true}}}}"#
    )
}

fn rows(number: usize) -> Vec<String> {
    (0..100)
        .map(|index| {
            row(
                number * 100 + index,
                1_709_500_000_000 - (number * 100 + index) as u64,
            )
        })
        .collect()
}

fn page(rows: &[String]) -> String {
    format!(r#"{{"code":"0","msg":"","data":[{}]}}"#, rows.join(","))
}

fn done(output: &[Action]) -> Option<&str> {
    output.iter().find_map(|action| match action {
        Action::Notice(Notice::CommandDone { command, result }) if *command == OWNER => {
            Some(result.as_str())
        }
        _ => None,
    })
}

fn failures(output: &[Action]) -> Vec<&safe_cex_engine::VenueError> {
    output
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == OWNER => {
                Some(error)
            }
            _ => None,
        })
        .collect()
}

fn incomplete(output: &[Action]) -> bool {
    output.iter().any(|action| {
        matches!(action,
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        )
    })
}

fn no_dispatch(output: &[Action]) {
    assert!(!output.iter().any(|action| match action {
        Action::Http(_) | Action::Sign { .. } => true,
        Action::ManagedHttp(action) => {
            matches!(
                action.as_ref(),
                HttpAction::Prepare { .. } | HttpAction::Bind { .. }
            )
        }
        _ => false,
    }));
}

fn root_expired(output: &[Action]) {
    let errors = failures(output);
    assert_eq!(errors.len(), 1, "one strict root terminal");
    assert_eq!(errors[0].kind, Some(ErrorKind::Timeout));
    assert!(errors[0].outcome.is_none());
    assert!(done(output).is_none(), "expired root publishes no rows");
    assert!(!incomplete(output), "expired root publishes no marker");
    no_dispatch(output);
}

fn pacing(output: &[Action], at: f64) -> TimerId {
    output
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("actual native100ms continuation")
}

fn fill_capacity(engine: &mut Engine<Okx>, at: f64) -> Vec<Preparation> {
    let mut preparations = Vec::new();
    let mut full = false;
    for command in 1000..1600 {
        engine.handle(
            at,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let output: Vec<_> = engine.drain_actions().collect();
        assert!(failures(&output).is_empty(), "peers cannot finish Closed");
        if output.iter().any(|a| {
            matches!(a,
                Action::Notice(Notice::CommandFailed { command: id, error })
                    if *id == command && error.kind == Some(ErrorKind::Internal)
            )
        }) {
            full = true;
            break;
        }
        let preparation = output.iter().find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        });
        preparations.push(preparation.expect("peer holds genuine preparation custody"));
    }
    assert!(full && !preparations.is_empty());
    assert!(
        preparations
            .iter()
            .all(|p| engine.http_preparation_pending(*p))
    );
    preparations
}

#[test]
fn keyless_empty_result_requires_admission_at_an_invalid_clock() {
    let mut engine = support::engine(support::FixtureOptions::default());
    engine.handle(f64::NAN, Event::Command(command(None, None, None)));
    let output: Vec<_> = engine.drain_actions().collect();
    let errors = failures(&output);
    assert_eq!(errors.len(), 1, "keyless[] cannot bypass entry refusal");
    assert_eq!(errors[0].kind, Some(ErrorKind::InvalidInput));
    assert!(errors[0].outcome.is_none());
    assert!(done(&output).is_none());
    no_dispatch(&output);
}

#[test]
fn finite_missing_receipt_cannot_become_a_default_root_or_empty_result() {
    let mut engine = support::engine(support::FixtureOptions::default());
    engine.handle(f64::MAX, Event::Command(command(None, None, None)));
    let output: Vec<_> = engine.drain_actions().collect();
    let errors = failures(&output);
    assert_eq!(errors.len(), 1, "missing receipt fails closed");
    assert_eq!(errors[0].kind, Some(ErrorKind::InvalidInput));
    assert!(errors[0].outcome.is_none());
    assert!(done(&output).is_none());
    no_dispatch(&output);
}

#[test]
fn unknown_symbol_empty_result_cannot_bypass_full_genuine_capacity() {
    let mut engine = ready();
    let peers = fill_capacity(&mut engine, NOW);
    fetch(&mut engine, NOW + 1.0, Some("UNKNOWN"), None);
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(peers.iter().all(|p| engine.http_preparation_pending(*p)));
    let errors = failures(&output);
    assert_eq!(
        errors.len(),
        1,
        "local[] must obtain genuine Read authority"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
    assert_eq!(errors[0].message, "Engine request capacity reached");
    assert!(errors[0].outcome.is_none());
    assert!(done(&output).is_none());
    no_dispatch(&output);
}

#[test]
fn first_closed_read_waits_for_managed_preparation_before_signing() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, None);
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    assert_eq!(output.iter().filter(|a| matches!(a,
        Action::ManagedHttp(action) if matches!(action.as_ref(),
            HttpAction::Prepare { request, .. } if request.url.contains("/account/positions-history?")
        )
    )).count(), 1, "one genuine managed page preparation");
}

#[test]
fn response_at_the_caller_deadline_cannot_publish_raw_rows() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, Some(50));
    let first = request(&prepared(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 50.0, &page(&[row(0, 1000)]));
    root_expired(&prepared(&mut engine, NOW + 50.0));
    assert!(engine.take_frame(NOW + 50.0).is_none());
}

#[test]
fn caller_expiry_discards_accepted_output_waiting_for_native_dequeue() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, Some(250));
    let first = request(&prepared(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 200.0, &page(&rows(0)));
    let timer = pacing(&prepared(&mut engine, NOW + 200.0), NOW + 300.0);
    engine.handle(NOW + 300.0, Event::TimerFired { timer });
    root_expired(&prepared(&mut engine, NOW + 300.0));
    assert!(engine.take_frame(NOW + 300.0).is_none());
}

#[test]
fn default_caller_wait_is_not_renewed_by_successful_physical_pages() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, None);
    let mut current = request(&prepared(&mut engine, NOW));
    for number in 0..8 {
        let at = NOW + (number as f64 + 1.0) * 15_000.0;
        reply(&mut engine, &current, at, &page(&rows(number)));
        let output = prepared(&mut engine, at);
        if number == 7 {
            root_expired(&output);
            return;
        }
        let timer = pacing(&output, at + 100.0);
        engine.handle(at + 100.0, Event::TimerFired { timer });
        current = request(&prepared(&mut engine, at + 100.0));
    }
    unreachable!();
}

#[test]
fn dequeue_capacity_refusal_rejects_prefix_without_finishing_live_peers() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, Some(60_000));
    let first = request(&prepared(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 10.0, &page(&rows(0)));
    let timer = pacing(&prepared(&mut engine, NOW + 10.0), NOW + 110.0);
    let peers = fill_capacity(&mut engine, NOW + 20.0);
    engine.handle(NOW + 110.0, Event::TimerFired { timer });
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(peers.iter().all(|p| engine.http_preparation_pending(*p)));
    let errors = failures(&output);
    assert_eq!(
        errors.len(),
        1,
        "child refusal settles its original root once"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
    assert!(errors[0].outcome.is_none());
    assert!(done(&output).is_none());
    assert!(!incomplete(&output));
    no_dispatch(&output);
}

#[test]
fn retired_native_queue_timer_cannot_send_for_a_reused_command_id() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, Some(50));
    let first = request(&prepared(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 10.0, &page(&rows(0)));
    let old_timer = pacing(&prepared(&mut engine, NOW + 10.0), NOW + 110.0);
    engine.take_frame(NOW + 50.0);
    engine.drain_actions().for_each(drop);
    fetch(&mut engine, NOW + 51.0, None, Some(1000));
    let replacement_output = prepared(&mut engine, NOW + 51.0);
    assert!(
        !replacement_output.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE
        )),
        "root expiry must release the old command ID before replacement"
    );
    let replacement = request(&replacement_output);
    engine.handle(NOW + 110.0, Event::TimerFired { timer: old_timer });
    let stale = prepared(&mut engine, NOW + 110.0);
    no_dispatch(&stale);
    assert!(done(&stale).is_none());
    assert!(failures(&stale).is_empty());
    reply(
        &mut engine,
        &replacement,
        NOW + 120.0,
        &page(&[row(303, 900)]),
    );
    let output = prepared(&mut engine, NOW + 120.0);
    assert_eq!(done(&output), Some(format!("[{}]", row(303, 900)).as_str()));
    assert!(!incomplete(&output));
}

#[test]
fn expired_old_response_cannot_publish_into_a_reused_command_id() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, Some(50));
    let old = request(&prepared(&mut engine, NOW));
    engine.take_frame(NOW + 50.0);
    engine.drain_actions().for_each(drop);
    fetch(&mut engine, NOW + 51.0, None, Some(1000));
    let replacement_output = prepared(&mut engine, NOW + 51.0);
    assert!(
        !replacement_output.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE
        )),
        "root expiry must release the old command ID before replacement"
    );
    let replacement = request(&replacement_output);
    assert_ne!(old.id, replacement.id);
    reply(&mut engine, &old, NOW + 60.0, &page(&[row(999, 1000)]));
    let stale = prepared(&mut engine, NOW + 60.0);
    assert!(
        done(&stale).is_none(),
        "old DATA cannot complete the replacement"
    );
    assert!(failures(&stale).is_empty());
    assert!(!incomplete(&stale));
    no_dispatch(&stale);
    reply(
        &mut engine,
        &replacement,
        NOW + 70.0,
        &page(&[row(303, 900)]),
    );
    let output = prepared(&mut engine, NOW + 70.0);
    assert_eq!(done(&output), Some(format!("[{}]", row(303, 900)).as_str()));
}

#[test]
fn native_keyless_and_unknown_symbols_keep_empty_precedence() {
    let mut keyless = support::engine(support::FixtureOptions::default());
    keyless
        .handle_command(
            NOW,
            command(Some("UNKNOWN"), Some(f64::NAN), Some(f64::INFINITY)),
            None,
        )
        .unwrap();
    let output = prepared(&mut keyless, NOW);
    assert_eq!(done(&output), Some("[]"));
    no_dispatch(&output);
    let mut private = ready();
    fetch(&mut private, NOW, Some("UNKNOWN"), None);
    let output = prepared(&mut private, NOW);
    assert_eq!(done(&output), Some("[]"));
    no_dispatch(&output);
}

#[test]
fn native_secondary_wire_and_raw_row_bytes_remain_exact() {
    let mut engine = support::trading_engine_with(
        support::FixtureOptions {
            testnet: true,
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            read_credentials: Some(ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: Some("read-passphrase".into()),
            }),
            ..Default::default()
        },
        |_| {},
    );
    engine.drain_actions().for_each(drop);
    engine
        .handle_command(
            NOW,
            command(Some("BTCUSDT"), Some(1000.0), Some(2000.0)),
            None,
        )
        .unwrap();
    let first = request(&prepared(&mut engine, NOW));
    assert!(first.url.ends_with("/api/v5/account/positions-history?instType=SWAP&instId=BTC-USDT-SWAP&before=1000&after=2000&limit=100"));
    assert!(first.body.is_none());
    assert!(
        first
            .headers
            .iter()
            .any(|(k, v)| *k == "OK-ACCESS-KEY" && v == "read-key")
    );
    assert!(
        first
            .headers
            .iter()
            .any(|(k, v)| *k == "x-simulated-trading" && v == "1")
    );
    let raw = row(99, 1500);
    reply(
        &mut engine,
        &first,
        NOW + 1.0,
        &page(std::slice::from_ref(&raw)),
    );
    let output = prepared(&mut engine, NOW + 1.0);
    assert_eq!(done(&output), Some(format!("[{raw}]").as_str()));
    assert!(failures(&output).is_empty());
    assert!(!incomplete(&output));
}

#[test]
fn native_later_failed_page_keeps_only_accepted_raw_output() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, None);
    let first = request(&prepared(&mut engine, NOW));
    let accepted = rows(0);
    reply(&mut engine, &first, NOW + 10.0, &page(&accepted));
    let timer = pacing(&prepared(&mut engine, NOW + 10.0), NOW + 110.0);
    engine.handle(NOW + 110.0, Event::TimerFired { timer });
    let second = request(&prepared(&mut engine, NOW + 110.0));
    let body = format!(
        r#"{{"code":"50011","msg":"rate limited","data":[{}]}}"#,
        row(999, 1)
    );
    reply(&mut engine, &second, NOW + 120.0, &body);
    let output = prepared(&mut engine, NOW + 120.0);
    assert_eq!(
        done(&output),
        Some(format!("[{}]", accepted.join(",")).as_str())
    );
    assert!(failures(&output).is_empty());
    let warning = output.iter().position(|a| matches!(a,
        Action::Notice(Notice::Event(event))
            if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                && event.fields == vec![("command", safe_cex_engine::Value::Num(f64::from(OWNER)))]
    )).unwrap();
    let terminal = output
        .iter()
        .position(|a| {
            matches!(a,
                Action::Notice(Notice::CommandDone { command, .. }) if *command == OWNER
            )
        })
        .unwrap();
    assert!(warning < terminal);
}

#[test]
fn native_429_evidence_is_counted_before_closed_payload_rejection() {
    let mut engine = ready();
    fetch(&mut engine, NOW, None, None);
    let first = request(&prepared(&mut engine, NOW));
    let previous = engine.health().rate_limited;
    engine.handle(
        NOW + 1.0,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 429,
                headers: "Retry-After: 1",
                body: r#"{"code":"50011","msg":"rate limited","data":[]}"#,
            },
        },
    );
    let output = prepared(&mut engine, NOW + 1.0);
    assert_eq!(engine.health().rate_limited, previous + 1);
    assert_eq!(failures(&output).len(), 1);
    assert!(done(&output).is_none());
    assert!(!incomplete(&output));
}

struct RefusalGate {
    provider: Provider,
    refuse: Arc<AtomicBool>,
}

impl Signer for RefusalGate {
    fn sign(&self, request: SignRequest) -> SignFuture {
        if self.refuse.load(Ordering::SeqCst) {
            Box::pin(async { Err(SignError::Refused) })
        } else {
            self.provider.sign(request)
        }
    }

    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.refuse.load(Ordering::SeqCst) {
            Some(Err(SignError::Refused))
        } else {
            self.provider.sign_ready(request)
        }
    }
}

#[test]
fn native_provider_refusal_recovers_only_nonempty_accepted_output() {
    for accepted_prefix in [false, true] {
        let refuse = Arc::new(AtomicBool::new(false));
        let mut engine = support::trading_engine_configure(|engine| {
            let provider = engine.signer().unwrap();
            engine
                .set_signer(Some(Arc::new(RefusalGate {
                    provider,
                    refuse: refuse.clone(),
                })))
                .unwrap();
        });
        engine.drain_actions().for_each(drop);
        let accepted = rows(0);
        let output = if accepted_prefix {
            fetch(&mut engine, NOW, None, None);
            let first = request(&prepared(&mut engine, NOW));
            reply(&mut engine, &first, NOW + 10.0, &page(&accepted));
            let timer = pacing(&prepared(&mut engine, NOW + 10.0), NOW + 110.0);
            refuse.store(true, Ordering::SeqCst);
            engine.handle(NOW + 110.0, Event::TimerFired { timer });
            prepared(&mut engine, NOW + 110.0)
        } else {
            refuse.store(true, Ordering::SeqCst);
            fetch(&mut engine, NOW, None, None);
            prepared(&mut engine, NOW)
        };
        no_dispatch(&output);
        if accepted_prefix {
            assert_eq!(
                done(&output),
                Some(format!("[{}]", accepted.join(",")).as_str())
            );
            assert!(incomplete(&output));
            assert!(failures(&output).is_empty());
        } else {
            assert!(done(&output).is_none());
            assert!(!incomplete(&output));
            let errors = failures(&output);
            assert_eq!(errors.len(), 1);
            assert!(errors[0].kind.is_some());
            assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
        }
    }
}

#[test]
fn host_invalid_clock_is_refused_without_dispatch_or_notice() {
    let mut engine = support::engine(support::FixtureOptions::default());
    let error = engine
        .handle_command(f64::NAN, command(None, None, None), None)
        .expect_err("host boundary rejects an invalid receipt");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(engine.drain_actions().next().is_none());
}
