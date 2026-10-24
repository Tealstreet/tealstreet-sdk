//! The options local refusal consumes a genuine command entry
//! (RUST_BREAKOUTPROP_OPTIONS_LOCAL_CONTEXT.md); its native text is unchanged.
use safe_cex_engine::model::OptionChainQuery;
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice};
use safe_cex_venue_breakoutprop::{BreakoutProp, Options};

const MESSAGE: &str = "Options are not enabled on this account";

fn fresh() -> Engine<BreakoutProp> {
    safe_cex_engine::test_support::seeded_engine(BreakoutProp::new(Options {
        public_data_only: true,
        ..Options::default()
    }))
}

fn commands() -> [Command; 3] {
    [
        Command::FetchOptionChain {
            command: 91,
            query: OptionChainQuery {
                underlying: "ETH".into(),
                expiry: None,
                right: None,
                strike_min: None,
                strike_max: None,
            },
        },
        Command::FetchOptionExpiries {
            command: 91,
            underlying: "ETH".into(),
        },
        Command::FetchAccountGreeks { command: 91 },
    ]
}

/// The single command terminal, with no I/O, timer or other action.
fn failure(actions: &[Action]) -> &safe_cex_engine::VenueError {
    assert_eq!(
        actions.len(),
        1,
        "one terminal, no I/O or timer: {actions:?}"
    );
    let Action::Notice(Notice::CommandFailed { command, error }) = &actions[0] else {
        panic!("not a command failure: {actions:?}");
    };
    assert_eq!(*command, 91);
    error
}

/// RED: an invalid entry clock is refused by the engine's command entry as a
/// typed Read failure, not answered by the native refusal text.
#[test]
fn an_options_query_requires_a_valid_command_entry() {
    for command in commands() {
        let mut engine = fresh();
        engine.handle(f64::NAN, Event::Command(command.clone()));
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, f64::NAN);
        let error = failure(&actions);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput), "{command:?}");
        assert_eq!(error.outcome, None, "a Read carries no write outcome");
        assert_ne!(error.message, MESSAGE, "{command:?}");
    }
}

/// Control: a valid entry keeps the native text, no kind and no outcome.
#[test]
fn an_options_query_keeps_its_native_refusal() {
    for command in commands() {
        let mut engine = fresh();
        engine.handle(1000., Event::Command(command.clone()));
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.);
        let error = failure(&actions);
        assert_eq!(error.message, MESSAGE, "{command:?}");
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
    }
}
