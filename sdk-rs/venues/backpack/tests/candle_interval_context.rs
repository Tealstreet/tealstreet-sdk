//! A native unsupported interval must use the genuine shared Read entry.
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Response, VenueError};
use safe_cex_venue_backpack::{Backpack, Options};
type Driver = Engine<Backpack>;

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
fn invoke(e: &mut Driver, command: u32, interval: Interval, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "SOL_USDC_PERP".into(),
            interval,
            limit: Some(0),
            from_ms: Some(-1.),
            to_ms: None,
        }),
    );
    e.drain_actions().collect()
}
fn failure(actions: &[Action], id: u32) -> &VenueError {
    assert!(
        actions.iter().all(|a| matches!(a, Action::Notice(_))),
        "unsupported local Read must not create physical work, timers or signing"
    );
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command, .. }) if *command == id)
    ));
    errors[0]
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
fn native_unsupported_interval_messages_remain_keyless_without_io_or_store_effects() {
    let intervals = [
        Interval::S1,
        Interval::S5,
        Interval::S15,
        Interval::S30,
        Interval::H3,
        Interval::H5,
        Interval::H7,
        Interval::H9,
        Interval::H10,
        Interval::H11,
        Interval::H24,
        Interval::D2,
        Interval::Mo1,
    ];
    for public in [false, true] {
        for interval in intervals {
            let mut e = engine(public);
            let actions = invoke(&mut e, 7, interval, 10.);
            let error = failure(&actions, 7);
            assert_eq!(error.message, interval.unsupported("Backpack"));
            assert_eq!(error.kind, None);
            assert_eq!(error.outcome, None, "a local Read has no write outcome");
            unchanged(&e);
        }
    }
}
#[test]
fn unsupported_interval_still_obeys_the_shared_clock_refusal() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(false);
        let actions = invoke(&mut e, 7, Interval::S1, now);
        let error = failure(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        unchanged(&e);
    }
}
#[test]
fn unsupported_interval_obeys_capacity_and_retains_an_existing_owner() {
    let mut e = engine(false);
    let first = park(&mut e, 1);
    for id in 2..=256 {
        park(&mut e, id);
    }
    let actions = invoke(&mut e, 257, Interval::S1, 11.);
    let error = failure(&actions, 257);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    complete_parked(&mut e, first, 1);
    unchanged(&e);
}
#[test]
fn unsupported_interval_duplicate_cannot_finish_the_parked_read() {
    let mut e = engine(false);
    let first = park(&mut e, 7);
    let actions = invoke(&mut e, 7, Interval::S1, 11.);
    assert!(actions.iter().all(|a| matches!(a, Action::Notice(_))));
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "command.duplicate")
    ));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 7, .. })
            | Action::Notice(Notice::CommandDone { command: 7, .. })
    )));
    complete_parked(&mut e, first, 7);
    unchanged(&e);
}
#[test]
fn local_refusals_allow_immediate_command_id_reuse() {
    let mut e = engine(false);
    for interval in [Interval::S1, Interval::H3, Interval::Mo1] {
        let actions = invoke(&mut e, 7, interval, 10.);
        let error = failure(&actions, 7);
        assert_eq!(error.message, interval.unsupported("Backpack"));
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
        unchanged(&e);
    }
}
#[test]
fn unsupported_websocket_subscription_diagnostic_is_unchanged() {
    let mut e = engine(false);
    e.handle(
        10.,
        Event::Command(Command::SubscribeCandles {
            symbol: "SOL_USDC_PERP".into(),
            interval: Interval::S1,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == "command.unsupported" && event.message == Interval::S1.unsupported("Backpack"))
    );
    unchanged(&e);
}
