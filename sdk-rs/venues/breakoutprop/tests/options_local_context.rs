//! Options queries are refused by the engine before command entry, because
//! BreakoutProp declares no options capability
//! (RUST_BREAKOUTPROP_OPTIONS_LOCAL_CONTEXT.md). The venue is never called.
use safe_cex_engine::model::OptionChainQuery;
use safe_cex_engine::{Action, Command, Engine, Event, Notice};
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

/// Control: at a finite or invalid clock the engine's pre-entry refusal
/// answers with the native text, no kind and no outcome. The venue's own
/// unsupported path would say "Unsupported BreakoutProp command" instead.
#[test]
fn the_engine_refuses_options_before_the_venue_is_called() {
    for now in [1000., f64::NAN] {
        for command in commands() {
            let mut engine = fresh();
            engine.handle(now, Event::Command(command.clone()));
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
            let error = failure(&actions);
            assert_eq!(error.message, MESSAGE, "{command:?} at {now}");
            assert_eq!(error.kind, None, "{command:?} at {now}");
            assert_eq!(error.outcome, None, "{command:?} at {now}");
        }
    }
}
