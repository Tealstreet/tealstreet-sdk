//! Genuine local command entries; the original production remains unchanged.
use safe_cex_engine::model::{Interval, StpMode};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Outcome};
use safe_cex_venue_breakoutprop::{BreakoutProp, Options};

#[derive(Clone, Copy, Debug)]
enum Case {
    CandleInterval,
    CandleRange,
    Leverage,
    MaxLeverage,
    Book,
    ReadStp,
    WriteStp,
    FixedLeverage,
    FixedMargin,
    FixedPosition,
    BuildFailure,
}
const CASES: [Case; 11] = [
    Case::CandleInterval,
    Case::CandleRange,
    Case::Leverage,
    Case::MaxLeverage,
    Case::Book,
    Case::ReadStp,
    Case::WriteStp,
    Case::FixedLeverage,
    Case::FixedMargin,
    Case::FixedPosition,
    Case::BuildFailure,
];
fn fresh() -> Engine<BreakoutProp> {
    safe_cex_engine::test_support::seeded_engine(BreakoutProp::new(Options {
        public_data_only: true,
        ..Options::default()
    }))
}
fn command(case: Case) -> Command {
    match case {
        Case::CandleInterval | Case::CandleRange => Command::FetchCandles {
            command: 91,
            symbol: "ETHUSD".into(),
            interval: if matches!(case, Case::CandleInterval) {
                Interval::S1
            } else {
                Interval::M1
            },
            limit: Some(1),
            from_ms: matches!(case, Case::CandleRange).then_some(-1.),
            to_ms: None,
        },
        Case::Leverage => Command::FetchLeverage {
            command: 91,
            symbol: "ETHUSD".into(),
        },
        Case::MaxLeverage => Command::FetchMaxLeverage {
            command: 91,
            symbol: "ETHUSD".into(),
        },
        Case::Book => Command::FetchOrderBook {
            command: 91,
            symbol: "ETHUSD".into(),
            limit: None,
        },
        Case::ReadStp => Command::FetchStpMode { command: 91 },
        Case::WriteStp => Command::SetStpMode {
            command: 91,
            mode: StpMode::ExpireMaker,
        },
        Case::FixedLeverage => Command::SetLeverage {
            command: 91,
            symbol: "ETHUSD".into(),
            leverage: 2.,
            side: None,
        },
        Case::FixedMargin => Command::SetMarginType {
            command: 91,
            symbol: "ETHUSD".into(),
            isolated: true,
        },
        Case::FixedPosition => Command::SetPositionMode {
            command: 91,
            hedged: true,
            symbol: None,
        },
        Case::BuildFailure => Command::PlaceOrders {
            command: 91,
            orders: vec![
                serde_json::from_value(serde_json::json!({
                    "symbol":"ETHUSD", "type":"limit", "side":"buy", "amount":1.,
                    "price":100., "clientOrderId":"native-refuses-caller-id"
                }))
                .unwrap(),
            ],
        },
    }
}
fn mutation(case: Case) -> bool {
    matches!(
        case,
        Case::WriteStp
            | Case::FixedLeverage
            | Case::FixedMargin
            | Case::FixedPosition
            | Case::BuildFailure
    )
}
fn terminal(actions: &[Action]) -> &Notice {
    assert_eq!(
        actions.len(),
        1,
        "one terminal, no I/O or timer: {actions:?}"
    );
    let Action::Notice(notice @ (Notice::CommandDone { .. } | Notice::CommandFailed { .. })) =
        &actions[0]
    else {
        panic!("not a command terminal: {actions:?}");
    };
    notice
}
fn invalid(case: Case) {
    let mut engine = fresh();
    engine.handle(f64::NAN, Event::Command(command(case)));
    let actions: Vec<_> = engine.drain_actions().collect();
    let Notice::CommandFailed { command, error } = terminal(&actions) else {
        panic!("invalid entry succeeded: {actions:?}");
    };
    assert_eq!(*command, 91);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, mutation(case).then_some(Outcome::NotSent));
    assert!(engine.store().orders().is_empty());
    assert!(engine.store().positions().is_empty());
    assert!(engine.store().setting("ETHUSD").is_none());
    assert!(engine.store().markets().is_empty());
}
macro_rules! entry_control {
    ($name:ident, $case:ident) => {
        #[test]
        fn $name() {
            invalid(Case::$case);
        }
    };
}
entry_control!(candle_interval_requires_valid_entry, CandleInterval);
entry_control!(candle_range_requires_valid_entry, CandleRange);
entry_control!(local_leverage_requires_valid_entry, Leverage);
entry_control!(maximum_leverage_requires_valid_entry, MaxLeverage);
entry_control!(unsupported_book_requires_valid_entry, Book);
entry_control!(unsupported_stp_read_requires_valid_entry, ReadStp);
entry_control!(unsupported_stp_write_requires_valid_entry, WriteStp);
entry_control!(fixed_leverage_requires_valid_entry, FixedLeverage);
entry_control!(fixed_margin_requires_valid_entry, FixedMargin);
entry_control!(fixed_position_requires_valid_entry, FixedPosition);
entry_control!(trade_validation_requires_valid_entry, BuildFailure);

fn message(case: Case) -> &'static str {
    match case {
        Case::CandleInterval => "DXtrade does not support 1s candles",
        Case::CandleRange => "Invalid DXtrade candle request",
        Case::MaxLeverage => "Maximum leverage queries are unavailable for DXtrade",
        Case::Book | Case::ReadStp | Case::WriteStp => "Unsupported BreakoutProp command",
        Case::FixedLeverage | Case::FixedMargin | Case::FixedPosition => {
            "Breakout Prop uses fixed one-way cross accounts; this setting is unsupported"
        }
        Case::BuildFailure => "Breakout Prop does not support caller clientOrderId",
        Case::Leverage => unreachable!(),
    }
}
#[test]
fn finite_local_cases_keep_native_results_messages_kind_and_no_io() {
    for case in CASES {
        let mut engine = fresh();
        engine.handle(1000., Event::Command(command(case)));
        let actions: Vec<_> = engine.drain_actions().collect();
        match terminal(&actions) {
            Notice::CommandDone { command, result } => {
                assert!(matches!(case, Case::Leverage));
                assert_eq!(*command, 91);
                assert_eq!(result, "null");
            }
            Notice::CommandFailed { command, error } => {
                assert_eq!(*command, 91);
                assert_eq!(error.message, message(case), "{case:?}");
                let fixed = matches!(
                    case,
                    Case::FixedLeverage | Case::FixedMargin | Case::FixedPosition
                );
                assert_eq!(error.kind, fixed.then_some(ErrorKind::Unsupported));
                if !matches!(case, Case::WriteStp) {
                    assert_eq!(error.outcome, mutation(case).then_some(Outcome::NotSent));
                }
            }
            _ => unreachable!(),
        }
        assert!(engine.store().orders().is_empty());
        assert!(engine.store().positions().is_empty());
        assert!(engine.store().setting("ETHUSD").is_none());
        assert!(engine.store().markets().is_empty());
    }
}
#[test]
fn new_contract_finite_unsupported_stp_mutation_is_not_sent() {
    let mut engine = fresh();
    engine.handle(1000., Event::Command(command(Case::WriteStp)));
    let actions: Vec<_> = engine.drain_actions().collect();
    let Notice::CommandFailed { error, .. } = terminal(&actions) else {
        panic!("unsupported STP succeeded");
    };
    assert_eq!(error.message, message(Case::WriteStp));
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}
#[test]
fn settled_local_id_reuse_keeps_every_native_terminal() {
    let mut engine = fresh();
    for case in [
        Case::Leverage,
        Case::FixedMargin,
        Case::ReadStp,
        Case::BuildFailure,
    ] {
        engine.handle(1000., Event::Command(command(case)));
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(matches!(
            terminal(&actions),
            Notice::CommandDone { command: 91, .. } | Notice::CommandFailed { command: 91, .. }
        ));
    }
}
#[test]
fn unsupported_capabilities_still_refuse_before_native_entry() {
    let mut engine = fresh();
    for command in [
        Command::FetchAccount { command: 91 },
        Command::FetchAccountGreeks { command: 91 },
    ] {
        engine.handle(1000., Event::Command(command));
        let actions: Vec<_> = engine.drain_actions().collect();
        let Notice::CommandFailed { error, .. } = terminal(&actions) else {
            panic!("unsupported capability succeeded");
        };
        assert!(matches!(
            error.message.as_str(),
            "This action is not supported on this venue"
                | "Options are not enabled on this account"
        ));
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
    }
}
