//! Local write refusals borrow the actual engine entry, including entropy preflight.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Outcome, VenueError};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use support::Options;
use support::TestEngine;
const NOW: f64 = 1000.;
const OWNER: u32 = 700;

#[derive(Clone, Copy, Debug)]
enum Case {
    Validation,
    NoKeyMode,
    EntropyValidation,
    EntropyNoKey,
}
impl Case {
    fn command(self, command: u32) -> Command {
        match self {
            Self::Validation => Command::SetLeverage {
                command,
                symbol: "BTCUSDT".into(),
                leverage: f64::NAN,
                side: None,
            },
            Self::NoKeyMode => Command::SetPositionMode {
                command,
                symbol: None,
                hedged: true,
            },
            Self::EntropyValidation | Self::EntropyNoKey => {
                let good: PlaceOrderOpts = serde_json::from_str(
                    r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":59000}"#,
                )
                .unwrap();
                let mut orders = vec![good.clone()];
                if matches!(self, Self::EntropyValidation) {
                    orders.push(PlaceOrderOpts {
                        amount: -1.,
                        ..good
                    });
                }
                Command::PlaceOrders { command, orders }
            }
        }
    }
    fn error(self) -> VenueError {
        match self {
            Self::Validation => VenueError::new("Invalid leverage").outcome(Outcome::NotSent),
            Self::EntropyValidation => {
                VenueError::new("MEXC order requires a positive native amount")
                    .outcome(Outcome::NotSent)
            }
            Self::NoKeyMode | Self::EntropyNoKey => {
                VenueError::new("MEXC trading needs an API key")
                    .kind(ErrorKind::AuthInvalid)
                    .outcome(Outcome::NotSent)
            }
        }
    }
    fn diagnostic(self) -> bool {
        matches!(self, Self::Validation | Self::EntropyValidation)
    }
    fn entropy(self) -> bool {
        matches!(self, Self::EntropyValidation | Self::EntropyNoKey)
    }
}
fn engine(private: bool) -> (TestEngine, Arc<AtomicUsize>) {
    let markets = support::started().0.store().markets().to_vec();
    let mut e = support::configured(Options {
        key: private.then(|| "fixture-key".into()),
        secret: private.then(|| "fixture-secret".into()),
        ..Options::default()
    });
    e.store_mut().set_markets(markets);
    e.take_frame(NOW);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    e.set_entropy(Box::new(move |bytes| {
        let seed = count.fetch_add(1, Ordering::SeqCst);
        bytes.fill((seed as u8).wrapping_add(1));
        Ok(())
    }))
    .unwrap();
    (e, calls)
}
fn act(e: &mut TestEngine, now: f64, case: Case, id: u32) -> Vec<Action> {
    e.handle(now, Event::Command(case.command(id)));
    e.drain_actions().collect()
}
fn native(actions: &[Action], case: Case, id: u32) {
    assert_eq!(
        actions.len(),
        if case.diagnostic() { 2 } else { 1 },
        "exact native effects for {case:?}: {actions:?}"
    );
    if case.diagnostic() {
        assert!(
            matches!(&actions[0], Action::Notice(Notice::CommandError { command, message })
            if *command == id && *message == case.error().message)
        );
    }
    assert!(
        matches!(actions.last().unwrap(), Action::Notice(Notice::CommandFailed { command, error }) if *command == id && *error == case.error())
    );
}
fn refused(actions: &[Action], kind: ErrorKind) {
    assert_eq!(
        actions.len(),
        1,
        "no diagnostic, I/O, timer or peer effects"
    );
    let Action::Notice(Notice::CommandFailed { command, error }) = &actions[0] else {
        panic!("typed refusal required: {actions:?}");
    };
    assert_eq!(*command, OWNER);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}
fn peer(e: &mut TestEngine, id: u32) -> Preparation {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: id,
            symbol: "BTCUSDT".into(),
            limit: None,
        }),
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed book peer")
}
fn invalid(case: Case) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        let (mut e, calls) = engine(false);
        let before = e.snapshot_frame(NOW).to_vec();
        let output = act(&mut e, now, case, OWNER);
        if case.entropy() {
            assert!(
                calls.load(Ordering::SeqCst) > 0,
                "native entropy preflight must run before local refusal"
            );
        }
        refused(&output, ErrorKind::InvalidInput);
        assert_eq!(before, e.snapshot_frame(NOW));
        native(&act(&mut e, NOW, case, OWNER), case, OWNER);
    }
}
fn capacity(case: Case) {
    let (mut e, calls) = engine(false);
    let peers: Vec<_> = (1..=256).map(|id| peer(&mut e, id)).collect();
    let before = e.snapshot_frame(NOW).to_vec();
    let output = act(&mut e, NOW, case, OWNER);
    if case.entropy() {
        assert!(calls.load(Ordering::SeqCst) > 0);
    }
    refused(&output, ErrorKind::Internal);
    assert_eq!(before, e.snapshot_frame(NOW));
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("settle peer"),
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
            .count(),
        1
    );
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: OWNER, .. }
                | Notice::CommandFailed { command: OWNER, .. }
        )
    )));
    native(&act(&mut e, NOW + 2., case, OWNER), case, OWNER);
    assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
}
macro_rules! pair {
    ($invalid:ident,$capacity:ident,$case:ident) => {
        #[test]
        fn $invalid() {
            invalid(Case::$case);
        }
        #[test]
        fn $capacity() {
            capacity(Case::$case);
        }
    };
}
pair!(invalid_validation, capacity_validation, Validation);
pair!(invalid_missing_key, capacity_missing_key, NoKeyMode);
pair!(
    invalid_entropy_prefix_validation,
    capacity_entropy_prefix_validation,
    EntropyValidation
);
pair!(
    invalid_entropy_prefix_missing_key,
    capacity_entropy_prefix_missing_key,
    EntropyNoKey
);
#[test]
fn missing_key_has_no_credential_auth_health_evidence() {
    for case in [Case::NoKeyMode, Case::EntropyNoKey] {
        let (mut e, _) = engine(false);
        native(&act(&mut e, NOW, case, OWNER), case, OWNER);
        assert_eq!(e.health().auth_failures, 0);
    }
}
#[test]
fn native_errors_diagnostic_order_and_key_precedence_are_exact() {
    for case in [
        Case::Validation,
        Case::NoKeyMode,
        Case::EntropyValidation,
        Case::EntropyNoKey,
    ] {
        let (mut e, calls) = engine(false);
        native(&act(&mut e, NOW, case, OWNER), case, OWNER);
        if case.entropy() {
            assert!(calls.load(Ordering::SeqCst) > 0);
        }
    }
    let (mut e, _) = engine(true);
    native(
        &act(&mut e, NOW, Case::Validation, OWNER),
        Case::Validation,
        OWNER,
    );
}
#[test]
fn synchronous_refusals_release_capacity_and_order_locks_for_id_reuse() {
    let (mut e, _) = engine(false);
    for index in 0..300 {
        native(
            &act(&mut e, NOW + f64::from(index), Case::Validation, OWNER),
            Case::Validation,
            OWNER,
        );
    }
}
#[test]
fn duplicate_id_does_not_complete_the_existing_read_or_steal_its_slot() {
    let (mut e, _) = engine(false);
    let token = peer(&mut e, OWNER);
    let output = act(&mut e, NOW, Case::Validation, OWNER);
    assert!(
        matches!(output.as_slice(),[Action::Notice(Notice::Event(event))] if event.code==safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    assert!(e.http_preparation_pending(token));
    e.http_preparation_failed(
        NOW + 1.,
        token,
        Error::State,
        VenueError::new("existing owner"),
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: OWNER, .. })
            ))
            .count(),
        1
    );
    native(
        &act(&mut e, NOW + 2., Case::Validation, OWNER),
        Case::Validation,
        OWNER,
    );
}
#[test]
fn accepted_private_mode_keeps_its_native_legacy_post_and_body() {
    let (mut e, _) = engine(true);
    let output = act(&mut e, NOW, Case::NoKeyMode, OWNER);
    let requests: Vec<_> = output
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .url
            .ends_with("/api/v1/private/position/change_position_mode")
    );
    assert_eq!(requests[0].method, safe_cex_engine::Method::Post);
    assert_eq!(requests[0].body.as_deref(), Some("{\"positionMode\":1}"));
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_)
            | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
