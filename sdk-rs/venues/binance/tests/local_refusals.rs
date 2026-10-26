//! Request slice (`docs/plans/RUST_BINANCE_REQUEST_REFUSALS.md`): every
//! pre-admission refusal is the command's own typed engine terminal, with
//! native's text and kind, NotSent, and no I/O.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Outcome, VenueError};
use safe_cex_venue_binance::{BinanceFutures, Variant};
use support::Options;

const NOW: f64 = 1000.;

fn keyed(variant: Variant) -> Engine<BinanceFutures> {
    support::engine(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("fixture-seed".into()),
        ..Options::default()
    })
}

/// The one terminal of `command`, after asserting no HTTP was sent.
fn refused(engine: &mut Engine<BinanceFutures>, id: u32, command: Command) -> VenueError {
    engine.handle(NOW, Event::Command(command));
    let out = safe_cex_engine::test_support::prepared_actions(engine, NOW);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
    let mut terminals = out.into_iter().filter_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command, error }) if command == id => Some(error),
        Action::Notice(Notice::CommandDone { command, .. }) if command == id => {
            panic!("refused command {id} completed")
        }
        _ => None,
    });
    let error = terminals.next().expect("one terminal");
    assert!(terminals.next().is_none(), "exactly one terminal");
    error
}

fn assert_refusal(error: &VenueError, message: &str, kind: ErrorKind) {
    assert_eq!(error.message, message);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn unsupported_commands_are_typed_not_sent_refusals() {
    let mut engine = keyed(Variant::UsdM);
    for (id, command) in [
        (11, Command::FetchStpMode { command: 11 }),
        (
            12,
            Command::SetStpMode {
                command: 12,
                mode: safe_cex_engine::model::StpMode::ExpireTaker,
            },
        ),
    ] {
        let error = refused(&mut engine, id, command);
        assert_refusal(
            &error,
            "This Binance command is not supported",
            ErrorKind::Unsupported,
        );
    }
}

#[test]
fn spot_refuses_derivative_settings() {
    let mut engine = keyed(Variant::Spot);
    let error = refused(
        &mut engine,
        21,
        Command::SetLeverage {
            command: 21,
            symbol: "BTCUSDT".into(),
            leverage: 5.,
            side: None,
        },
    );
    assert_refusal(
        &error,
        "Binance Spot does not support derivative settings",
        ErrorKind::Unsupported,
    );
}

#[test]
fn keyless_trading_refuses_before_any_work() {
    let mut engine = support::engine(Options::default());
    let error = refused(
        &mut engine,
        31,
        Command::SetLeverage {
            command: 31,
            symbol: "BTCUSDT".into(),
            leverage: 5.,
            side: None,
        },
    );
    assert_refusal(
        &error,
        "Binance trading needs an API key",
        ErrorKind::AuthInvalid,
    );
}

#[test]
fn an_unsupported_candle_interval_is_a_typed_read_refusal() {
    let mut engine = keyed(Variant::UsdM);
    let error = refused(
        &mut engine,
        41,
        Command::FetchCandles {
            command: 41,
            symbol: "BTCUSDT".into(),
            interval: Interval::S1,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
    );
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}
