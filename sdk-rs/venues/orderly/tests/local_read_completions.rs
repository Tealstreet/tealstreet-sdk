mod support;

use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Response, VenueError};
use safe_cex_venue_orderly::{Options, Orderly};

const COMMAND: u32 = 971;
type Driver = Engine<Orderly>;

fn engine(options: Options, load: bool) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(options));
    if !load {
        return e;
    }
    e.start(0.0);
    for (suffix, body) in [
        ("/v1/public/info", support::MARKETS),
        ("/v1/public/futures", support::TICKERS),
    ] {
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
        let request = out
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.ends_with(suffix) => Some(request),
                _ => None,
            })
            .expect("genuine managed native startup request");
        e.handle(
            0.0,
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
    let _ = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    assert!(e.store().loaded().markets && e.store().loaded().tickers);
    e
}

fn public_engine() -> Driver {
    engine(
        Options {
            public_data_only: true,
            ..Options::default()
        },
        true,
    )
}

fn snapshot(e: &Driver) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}",
        e.store().loaded(),
        e.store().markets(),
        e.store().tickers(),
        e.store().balance(),
        e.store().orders(),
        e.store().positions()
    )
}

fn local(e: &mut Driver, now: f64, command: Command) -> Vec<Action> {
    let before = snapshot(e);
    e.handle(now, Event::Command(command));
    let out: Vec<_> = e.drain_actions().collect();
    assert_eq!(snapshot(e), before, "local completion cannot mutate Store");
    let mut timers = Vec::new();
    let mut terminals = 0;
    for action in &out {
        match action {
            Action::SetTimer { timer, at_ms } => {
                assert!(at_ms.is_finite(), "no nonfinite timer");
                timers.push(*timer);
            }
            Action::ClearTimer { timer } => {
                let index = timers
                    .iter()
                    .position(|value| value == timer)
                    .expect("clear only an admitted local timer");
                timers.remove(index);
            }
            Action::Notice(Notice::CommandDone { command, .. })
            | Action::Notice(Notice::CommandFailed { command, .. }) => {
                assert_eq!(*command, COMMAND);
                terminals += 1;
            }
            _ => panic!("local completion must not perform IO or unrelated work: {out:?}"),
        }
    }
    assert!(timers.is_empty(), "local Context must release its timer");
    assert_eq!(terminals, 1, "one correlated terminal: {out:?}");
    out
}

fn failure(out: &[Action]) -> &VenueError {
    out.iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("one failure")
}

fn success(out: &[Action]) -> &str {
    out.iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result.as_str()),
            _ => None,
        })
        .expect("one success")
}

fn candles(
    symbol: &str,
    interval: Interval,
    limit: u32,
    from: Option<f64>,
    to: Option<f64>,
) -> Command {
    Command::FetchCandles {
        command: COMMAND,
        symbol: symbol.into(),
        interval,
        limit: Some(limit),
        from_ms: from,
        to_ms: to,
    }
}

fn unstamped_candle(command: Command) {
    let out = local(&mut public_engine(), f64::NAN, command);
    let error = failure(&out);
    assert_eq!(error.outcome, None, "Read has no write outcome");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
}

#[test]
fn unstamped_unknown_symbol_uses_shared_entry_rejection() {
    unstamped_candle(candles("UNKNOWN", Interval::M1, 1, None, None));
}

#[test]
fn unstamped_unsupported_interval_uses_shared_entry_rejection() {
    unstamped_candle(candles("BTCUSDC", Interval::S1, 1, None, None));
}

#[test]
fn unstamped_invalid_parameters_use_shared_entry_rejection() {
    unstamped_candle(candles("BTCUSDC", Interval::M1, 0, None, None));
}

#[test]
fn unstamped_identity_success_cannot_bypass_shared_rejection() {
    let mut e = engine(
        Options {
            application_id: Some("fixture-broker".into()),
            ..Options::default()
        },
        false,
    );
    let out = local(&mut e, f64::NAN, Command::FetchAccount { command: COMMAND });
    assert_eq!(failure(&out).kind, Some(ErrorKind::InvalidInput));
    assert_eq!(failure(&out).outcome, None);
}

#[test]
fn unstamped_identity_failure_uses_shared_entry_rejection() {
    let mut e = engine(Options::default(), false);
    let out = local(&mut e, f64::NAN, Command::FetchAccount { command: COMMAND });
    assert_eq!(failure(&out).kind, Some(ErrorKind::InvalidInput));
    assert_eq!(failure(&out).outcome, None);
}

#[test]
fn native_symbol_interval_and_parameter_precedence_is_preserved() {
    for (command, expected) in [
        (
            candles("UNKNOWN", Interval::S1, 0, None, None),
            "Orderly unknown symbol".to_owned(),
        ),
        (
            candles("BTCUSDC", Interval::S1, 0, None, None),
            Interval::S1.unsupported("Orderly"),
        ),
        (
            candles("BTCUSDC", Interval::M1, 0, None, None),
            "Orderly invalid candle parameters".to_owned(),
        ),
    ] {
        let out = local(&mut public_engine(), 100.0, command);
        assert_eq!(failure(&out).legacy_text(), expected);
        assert_eq!(failure(&out).kind, None);
        assert_eq!(failure(&out).outcome, None);
    }
}

#[test]
fn native_invalid_bound_variants_are_preserved() {
    for (from, to) in [
        (Some(f64::NAN), None),
        (None, Some(f64::INFINITY)),
        (Some(-1.0), None),
        (None, Some(253_402_300_799_001.0)),
        (Some(2.0), Some(1.0)),
    ] {
        let out = local(
            &mut public_engine(),
            100.0,
            candles("BTCUSDC", Interval::M1, 1, from, to),
        );
        assert_eq!(
            failure(&out).legacy_text(),
            "Orderly invalid candle parameters"
        );
        assert_eq!(failure(&out).kind, None);
        assert_eq!(failure(&out).outcome, None);
    }
}

#[test]
fn native_future_lower_bound_with_no_upper_returns_exact_empty_result() {
    let out = local(
        &mut public_engine(),
        100.0,
        candles("BTCUSDC", Interval::M1, 1, Some(101.0), None),
    );
    assert_eq!(success(&out), "[]");
}

#[test]
fn native_identity_needs_no_key_and_preserves_exact_json_escaping() {
    for id in ["fixture-broker", "fixture-\"broker\\é"] {
        let mut e = engine(
            Options {
                application_id: Some(id.into()),
                ..Options::default()
            },
            false,
        );
        let out = local(&mut e, 100.0, Command::FetchAccount { command: COMMAND });
        let quoted = serde_json::to_string(id).unwrap();
        assert_eq!(
            success(&out),
            format!("{{\"userId\":{quoted},\"affiliateId\":{quoted}}}")
        );
    }
}

#[test]
fn native_missing_empty_and_public_identity_refusals_are_preserved() {
    for options in [
        Options::default(),
        Options {
            application_id: Some(String::new()),
            ..Options::default()
        },
        Options {
            application_id: Some("fixture-broker".into()),
            public_data_only: true,
            ..Options::default()
        },
    ] {
        let mut e = engine(options, false);
        let out = local(&mut e, 100.0, Command::FetchAccount { command: COMMAND });
        assert_eq!(
            failure(&out).legacy_text(),
            "Orderly private account ID is required"
        );
        assert_eq!(failure(&out).kind, None);
        assert_eq!(failure(&out).outcome, None);
    }
}

#[test]
fn accepted_current_and_history_candle_wire_remains_managed() {
    for (from, to, suffix) in [
        (None, None, "/v1/kline?symbol=PERP_BTC_USDC&type=1m&limit=2"),
        (
            Some(1001.0),
            Some(120_999.0),
            "/v1/tv/kline_history?symbol=PERP_BTC_USDC&resolution=1m&limit=2&from=1&to=120",
        ),
    ] {
        let mut e = public_engine();
        e.handle(
            100.0,
            Event::Command(candles("BTCUSDC", Interval::M1, 2, from, to)),
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, 100.0);
        let requests: Vec<_> = out
            .iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, safe_cex_engine::Method::Get);
        assert_eq!(
            requests[0].url,
            format!("https://api-evm.orderly.org{suffix}")
        );
        assert!(requests[0].headers.is_empty());
        assert!(requests[0].body.is_none());
        assert!(!out.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )));
    }
}
