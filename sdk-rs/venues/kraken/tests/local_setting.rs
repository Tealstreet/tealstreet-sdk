//! Genuine-entry fixed-setting success without credentials or physical work.
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Outcome};
use safe_cex_venue_kraken::{Kraken, Options};

fn command() -> Command {
    Command::SetPositionMode { command: 7, hedged: false, symbol: None }
}

#[test]
fn fixed_setting_remains_keyless_and_preserves_store_without_io() {
    for public_data_only in [false, true] {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Kraken::new(Options {
            public_data_only,
            ..Default::default()
        }));
        engine.store_mut().set_hedged(true);
        engine.handle(1., Event::Command(command()));
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(matches!(actions.as_slice(), [Action::Notice(Notice::CommandDone { command: 7, result })] if result == "null"));
        assert!(engine.store().hedged(), "fixed setting is a native Store no-op");
    }
}

#[test]
fn invalid_clock_refuses_local_setting_without_io_or_store_change() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Kraken::new(Options::default()));
    engine.store_mut().set_hedged(true);
    engine.handle(f64::NAN, Event::Command(command()));
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(matches!(actions.as_slice(), [Action::Notice(Notice::CommandFailed { command: 7, error })]
        if error.kind == Some(ErrorKind::InvalidInput) && error.outcome == Some(Outcome::NotSent)));
    assert!(engine.store().hedged());
}
