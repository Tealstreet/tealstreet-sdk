//! Bootstrap must recover without a caller read, a private socket, or a manual refresh.
mod support;

use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::{META, STATE, kind};

const SPOT: &str = r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"#;

struct Run {
    engine: Engine<Hyperliquid>,
    held_kind: &'static str,
    mode: &'static str,
    held: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    sent: usize,
    socket: Option<SocketId>,
    notices: Vec<safe_cex_engine::log::LogEvent>,
    now: f64,
}
impl Run {
    fn new(held_kind: &'static str, mode: &'static str) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
            application_id: (held_kind != "userRole")
                .then(|| "0x14791697260e4c9a71f18484c9f997b308e59325".into()),
            key: (held_kind == "userRole")
                .then(|| "0x1111111111111111111111111111111111111111".into()),
            ..Options::default()
        }));
        engine
            .set_entropy(Box::new(|bytes| {
                bytes.fill(0);
                Ok(())
            }))
            .unwrap();
        engine.set_account_startup(true);
        engine.start(1000.0);
        let mut run = Self {
            engine,
            held_kind,
            mode,
            held: vec![],
            timers: vec![],
            sent: 0,
            socket: None,
            notices: vec![],
            now: 1000.0,
        };
        run.drain();
        assert_eq!(
            run.held.len(),
            1,
            "real bootstrap issues exactly one target read"
        );
        run
    }
    fn drain(&mut self) {
        loop {
            let actions: Vec<_> = prepared_now(&mut self.engine).into_iter().collect();
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    Action::Http(request) if kind(&request) == self.held_kind => {
                        self.sent += 1;
                        self.held.push(request);
                    }
                    Action::Http(request) => {
                        let body = match kind(&request).as_str() {
                            "metaAndAssetCtxs" => META,
                            "perpDexs" => "[null]",
                            "userAbstraction" => self.mode,
                            "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                            "maxBuilderFee" => "30",
                            "clearinghouseState" => STATE,
                            "frontendOpenOrders" => "[]",
                            "spotClearinghouseState" => SPOT,
                            other => panic!("unexpected read {other}"),
                        };
                        self.answer(&request, 200, "", body);
                    }
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                    // Never open a socket: no private stream or reconnect refresh can rescue bootstrap.
                    Action::WsConnect { socket, .. } => self.socket = Some(socket),
                    Action::Notice(safe_cex_engine::Notice::Event(notice)) => {
                        self.notices.push(notice)
                    }
                    _ => {}
                }
            }
        }
    }
    fn next_retry(&mut self) -> f64 {
        let before = self.now;
        while self.now - before <= 60_000.0 {
            let Some(at) = self.timers.iter().map(|(_, at)| *at).min_by(f64::total_cmp) else {
                break;
            };
            self.advance((at - self.now).max(0.0));
            if !self.held.is_empty() {
                return self.now - before;
            }
        }
        panic!("no bootstrap retry within its cap");
    }
    fn open(&mut self) {
        self.engine.handle(
            self.now,
            Event::WsOpened {
                socket: self.socket.expect("real source socket"),
            },
        );
        self.drain();
        self.advance(0.0); // Complete the actual subscription release continuation before testing retry cancellation.
    }
    fn answer(&mut self, request: &HttpRequest, status: u16, headers: &str, body: &str) {
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers,
                    body,
                },
            },
        );
    }
    fn fail(&mut self, request: &HttpRequest) {
        self.engine.handle(
            self.now,
            Event::HttpFailed {
                request: request.id,
                error: "test transport failure",
            },
        );
        self.drain();
    }
    fn advance(&mut self, elapsed: f64) {
        let end = self.now + elapsed;
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first() else {
                break;
            };
            if at > end {
                break;
            }
            self.timers.remove(0);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
            self.drain();
        }
        self.now = end;
    }
}

#[test]
fn failed_initial_core_state_retries_without_a_caller_or_stream() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let first = run.held.remove(0);
    assert!(!run.engine.store().loaded().balance);
    run.fail(&first);
    run.advance(10_000.0);
    assert_eq!(
        run.held.len(),
        1,
        "failed initial core state must retry independently"
    );
    let retry = run.held.remove(0);
    assert_ne!(first.id, retry.id);
    run.answer(&retry, 200, "", STATE);
    run.drain();
    assert!(run.engine.store().loaded().balance);
    assert_eq!(run.engine.store().positions()[0].contracts, 2.0);
}

#[test]
fn failed_initial_perp_collateral_retries_without_a_caller_or_stream() {
    let mut run = Run::new("spotClearinghouseState", "\"portfolioMargin\"");
    let first = run.held.remove(0);
    assert!(!run.engine.store().loaded().balance);
    run.fail(&first);
    run.advance(10_000.0);
    assert_eq!(
        run.held.len(),
        1,
        "failed initial perp collateral must retry independently"
    );
    let retry = run.held.remove(0);
    run.answer(&retry, 200, "", SPOT);
    run.drain();
    assert!(run.engine.store().loaded().balance);
    assert_eq!(run.engine.store().balance().free, 1050.0);
}

#[test]
fn successful_initial_core_state_needs_no_retry_or_private_socket() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let first = run.held.remove(0);
    run.answer(&first, 200, "", STATE);
    run.drain();
    run.advance(10_000.0);
    assert!(run.engine.store().loaded().balance);
    assert_eq!(run.sent, 1);
    assert!(run.held.is_empty());
}

#[test]
fn no_retry_overlaps_a_pending_read_and_a_real_native_timeout_recovers() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let first = run.held.remove(0);
    run.advance(10_000.0);
    assert_eq!(
        run.sent, 1,
        "pending work must not get a separate bootstrap deadline"
    );
    run.advance(11_000.0);
    assert_eq!(
        run.held.len(),
        1,
        "the real managed read deadline schedules recovery"
    );
    assert_ne!(run.held[0].id, first.id);
    run.advance(10_000.0);
    assert_eq!(
        run.sent, 2,
        "the retry remains single-flight while its own read is pending"
    );
}

#[test]
fn repeated_failures_use_exponential_jittered_backoff_with_a_cap() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let mut delays = Vec::new();
    for _ in 0..8 {
        let request = run.held.remove(0);
        run.fail(&request);
        delays.push(run.next_retry());
        assert_eq!(run.held.len(), 1);
    }
    // Zero injected entropy pins the real [0.75,1] jitter, not an alternate retry implementation.
    assert_eq!(
        delays,
        [
            750.0, 1500.0, 3000.0, 6000.0, 12000.0, 22500.0, 22500.0, 22500.0
        ]
    );
}

#[test]
fn retry_after_is_a_floor_for_initial_state_recovery() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let request = run.held.remove(0);
    run.answer(&request, 429, "Retry-After: 12", "busy");
    run.drain();
    run.advance(11_999.0);
    assert!(run.held.is_empty());
    run.advance(1.0);
    assert_eq!(run.held.len(), 1);
}

#[test]
fn stream_success_cancels_a_queued_retry_and_late_failure_cannot_rearm_it() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    run.open();
    let first = run.held.remove(0);
    run.fail(&first);
    let text = format!(
        r#"{{"channel":"clearinghouseState","data":{{"user":"x","dex":"","clearinghouseState":{STATE}}}}}"#
    );
    run.engine.handle(
        run.now,
        Event::WsMessage {
            socket: run.socket.unwrap(),
            text: &text,
        },
    );
    run.drain();
    assert!(run.engine.store().loaded().balance);
    run.fail(&first); // Retired managed request has no continuation authority.
    run.advance(10_000.0);
    assert_eq!(run.sent, 1);
    assert!(run.held.is_empty());
}

#[test]
fn only_bootstrap_retries_and_existing_stream_revision_guards_still_win() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let first = run.held.remove(0);
    run.fail(&first);
    run.next_retry();
    let retry = run.held.remove(0);
    run.open();
    let newer = STATE
        .replace("\"szi\":\"2\"", "\"szi\":\"3\"")
        .replace("\"time\":1000", "\"time\":3000");
    let text = format!(
        r#"{{"channel":"clearinghouseState","data":{{"user":"x","dex":"","clearinghouseState":{newer}}}}}"#
    );
    run.engine.handle(
        run.now,
        Event::WsMessage {
            socket: run.socket.unwrap(),
            text: &text,
        },
    );
    run.drain();
    run.answer(&retry, 200, "", STATE);
    run.drain();
    assert_eq!(run.engine.store().positions()[0].contracts, 3.0);
    run.advance(10_000.0);
    assert!(run.held.is_empty());
}

#[test]
fn real_failures_publish_only_closed_private_read_kind_facts_before_the_watchdog() {
    use safe_cex_engine::log::Value;
    for (request, expected) in [
        ("clearinghouseState", "state"),
        ("spotClearinghouseState", "spot"),
        ("frontendOpenOrders", "orders"),
        ("userRole", "role"),
        ("userAbstraction", "mode"),
        ("perpDexs", "discovery"),
    ] {
        let mut run = Run::new(request, "\"portfolioMargin\"");
        let first = run.held.remove(0);
        run.fail(&first);
        let facts: Vec<_> = run
            .notices
            .iter()
            .filter(|notice| notice.code == "request.failed")
            .flat_map(|notice| notice.fields.iter())
            .filter(|(name, _)| *name == "lastFailedPrivateReadKind" || *name == "readKind")
            .collect();
        assert_eq!(
            facts,
            vec![
                &("lastFailedPrivateReadKind", Value::Str(expected.into())),
                &("readKind", Value::Str(expected.into()))
            ]
        );
        assert!(run.now < 60_000.0);
    }
}

#[test]
fn malformed_initial_responses_retry_without_publishing_empty_financial_state() {
    for (kind, mode, valid) in [
        ("clearinghouseState", "\"disabled\"", STATE),
        ("spotClearinghouseState", "\"portfolioMargin\"", SPOT),
    ] {
        let mut run = Run::new(kind, mode);
        let first = run.held.remove(0);
        run.answer(&first, 200, "", "not-json");
        run.drain();
        let expected = if kind == "clearinghouseState" {
            "state"
        } else {
            "spot"
        };
        assert!(run.notices.iter().any(|event| event.fields.contains(&(
            "lastFailedPrivateReadKind",
            safe_cex_engine::log::Value::Str(expected.into()),
        ))));
        assert!(!run.engine.store().loaded().balance);
        run.next_retry();
        let retry = run.held.remove(0);
        run.answer(&retry, 200, "", valid);
        run.drain();
        assert!(run.engine.store().loaded().balance);
        let sent = run.sent;
        run.advance(10_000.0);
        assert_eq!(
            run.sent, sent,
            "successful bootstrap is permanently disarmed"
        );
    }
}

#[test]
fn perp_collateral_success_cancels_retry_even_with_a_late_retired_failure() {
    let mut run = Run::new("spotClearinghouseState", "\"portfolioMargin\"");
    let first = run.held.remove(0);
    run.fail(&first);
    run.next_retry();
    let retry = run.held.remove(0);
    run.answer(&retry, 200, "", SPOT);
    run.drain();
    run.fail(&first);
    run.advance(10_000.0);
    assert!(run.held.is_empty());
    assert_eq!(run.sent, 2);
    assert_eq!(run.engine.store().balance().free, 1050.0);
}

#[test]
fn the_native_watchdog_preserves_the_last_failed_kind_as_a_closed_fact() {
    use safe_cex_engine::log::Value;
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let first = run.held.remove(0);
    run.fail(&first);
    run.advance(70_000.0);
    let event = run
        .notices
        .iter()
        .find(|event| event.code == "startup.incomplete")
        .expect("real native startup watchdog");
    assert!(
        event
            .fields
            .contains(&("lastFailedPrivateReadKind", Value::Str("state".into())))
    );
}

#[test]
fn later_live_failures_do_not_restart_the_completed_bootstrap_retry() {
    let mut run = Run::new("clearinghouseState", "\"disabled\"");
    let initial = run.held.remove(0);
    run.answer(&initial, 200, "", STATE);
    run.drain();
    run.open(); // Existing socket-open refresh, after successful initial publication.
    let refresh = run.held.remove(0);
    run.fail(&refresh);
    run.advance(10_000.0);
    assert!(run.engine.store().loaded().balance);
    assert!(
        run.held.is_empty(),
        "live polling is not changed by a bootstrap-only retry"
    );
    assert_eq!(run.sent, 2);
}

#[test]
fn native_startup_recheck_recovers_role_with_only_info_reads_and_keeps_pending_single_flight() {
    let mut run = Run::new("userRole", "\"disabled\"");
    assert!(run.engine.recheck_startup_reads(run.now));
    run.drain();
    assert_eq!(run.sent, 1, "an admitted role read remains single flight");
    let first = run.held.remove(0);
    run.answer(&first, 200, "", r#"{"role":"missing"}"#);
    run.drain();
    assert!(!run.engine.private_startup_complete());
    assert!(run.engine.recheck_startup_reads(run.now));
    run.drain();
    assert_eq!(run.sent, 2);
    let retry = run.held.remove(0);
    assert_ne!(first.id, retry.id);
    assert_eq!(retry.url, "https://api.hyperliquid.xyz/info");
    assert_eq!(kind(&retry), "userRole");
    run.answer(
        &retry,
        200,
        "",
        r#"{"role":"agent","data":{"user":"0x14791697260e4c9a71f18484c9f997b308e59325"}}"#,
    );
    run.drain();
    assert!(run.engine.private_startup_complete());
    assert!(!run.engine.recheck_startup_reads(run.now));
    run.drain();
    assert_eq!(run.sent, 2, "successful recovery retires rechecks");
}
