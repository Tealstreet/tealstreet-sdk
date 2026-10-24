//! Native fixed-setting refusals must acquire the genuine command owner.
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Outcome, Response, VenueError,
};
use safe_cex_venue_backpack::{Backpack, Options};

type Driver = Engine<Backpack>;

#[derive(Clone, Copy)]
enum Setting {
    Margin,
    Position,
}
impl Setting {
    fn command(self, command: u32, value: bool) -> Command {
        match self {
            Self::Margin => Command::SetMarginType {
                command,
                symbol: "SOL_USDC_PERP".into(),
                isolated: value,
            },
            Self::Position => Command::SetPositionMode {
                command,
                symbol: None,
                hedged: value,
            },
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::Margin => "Backpack only supports cross margin",
            Self::Position => "Backpack position mode is fixed",
        }
    }
}
fn engine(public_data_only: bool) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only,
        ..Default::default()
    }));
    e.store_mut().set_hedged(true);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 90.,
        used: 33.,
        upnl: 7.,
    });
    e
}
fn unchanged(e: &Driver) {
    assert!(e.store().hedged());
    let b = e.store().balance();
    assert_eq!((b.total, b.free, b.used, b.upnl), (123., 90., 33., 7.));
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}
fn invoke(e: &mut Driver, setting: Setting, id: u32, value: bool, now: f64) -> Vec<Action> {
    e.handle(now, Event::Command(setting.command(id, value)));
    e.drain_actions().collect()
}
fn failure(actions: &[Action], id: u32) -> &VenueError {
    assert!(
        actions.iter().all(|a| matches!(a, Action::Notice(_))),
        "fixed refusal must not create physical work, timers or signing"
    );
    let failed: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1, "one terminal for this genuine entry");
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command, .. }) if *command == id)
    ));
    failed[0]
}
fn park(e: &mut Driver, command: u32) -> Preparation {
    e.handle(
        10.,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: "SOL_USDC_PERP".into(),
            limit: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("working depth read parks under an engine owner")
}
fn complete_parked(e: &mut Driver, token: Preparation, command: u32) {
    e.http_prepared(
        12.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prior owner still binds after the refused setting");
    assert!(matches!(e.claim_http(12., permit), Ok(Claim::Released(_))));
    e.handle(
        13.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"bids":[],"asks":[]}"#,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: id, result }) if *id == command && result == r#"{"bids":[],"asks":[]}"#)).count(), 1);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command)
    ));
}

#[test]
fn native_refusals_remain_keyless_for_both_values_and_preserve_store() {
    for public in [false, true] {
        for setting in [Setting::Margin, Setting::Position] {
            for value in [false, true] {
                let mut e = engine(public);
                let actions = invoke(&mut e, setting, 7, value, 10.);
                let error = failure(&actions, 7);
                assert_eq!(error.message, setting.message());
                assert_eq!(error.kind, Some(ErrorKind::Unsupported));
                assert_eq!(error.outcome, Some(Outcome::NotSent));
                unchanged(&e);
            }
        }
    }
}
fn invalid_clock(setting: Setting) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(false);
        let actions = invoke(&mut e, setting, 7, true, now);
        let error = failure(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        unchanged(&e);
    }
}
#[test]
fn margin_refusal_obeys_the_engine_clock() {
    invalid_clock(Setting::Margin);
}
#[test]
fn position_refusal_obeys_the_engine_clock() {
    invalid_clock(Setting::Position);
}
fn capacity(setting: Setting) {
    let mut e = engine(false);
    let first = park(&mut e, 1);
    for id in 2..=256 {
        park(&mut e, id);
    }
    let actions = invoke(&mut e, setting, 257, true, 11.);
    let error = failure(&actions, 257);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    unchanged(&e);
    complete_parked(&mut e, first, 1);
}
#[test]
fn margin_refusal_obeys_capacity_without_retiring_the_prior_owner() {
    capacity(Setting::Margin);
}
#[test]
fn position_refusal_obeys_capacity_without_retiring_the_prior_owner() {
    capacity(Setting::Position);
}
#[test]
fn duplicate_setting_cannot_terminate_a_parked_read_with_the_same_id() {
    for setting in [Setting::Margin, Setting::Position] {
        let mut e = engine(false);
        let first = park(&mut e, 7);
        let actions = invoke(&mut e, setting, 7, true, 11.);
        assert!(actions.iter().all(|a| matches!(a, Action::Notice(_))));
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "command.duplicate")));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 7, .. })
                | Action::Notice(Notice::CommandDone { command: 7, .. })
        )));
        complete_parked(&mut e, first, 7);
        unchanged(&e);
    }
}
#[test]
fn a_completed_refusal_does_not_block_immediate_command_reuse() {
    let mut e = engine(false);
    for setting in [Setting::Margin, Setting::Position, Setting::Margin] {
        let actions = invoke(&mut e, setting, 7, true, 10.);
        assert_eq!(failure(&actions, 7).message, setting.message());
        unchanged(&e);
    }
}
