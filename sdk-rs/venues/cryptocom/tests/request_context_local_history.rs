//! Synchronous history results still require genuine command-entry custody.
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Query, Response, VenueError,
};
use safe_cex_venue_cryptocom::{Cryptocom, Options};

type TestEngine = Engine<Cryptocom>;
const NOW: f64 = 1_710_000_000_000.;
const CREDS: &str = "Crypto.com credentials are required";
const UNSUPPORTED: &str = "Crypto.com client order lookup is not supported by the Rust engine";
fn engine(private: bool) -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(Cryptocom::new(Options {
        key: private.then(|| "fixture-identity".into()),
        secret: private.then(|| "fixture-material".into()),
        ..Options::default()
    }))
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "UNKNOWN".into(),
        client_order_id: "caller".into(),
    }
}
fn fills(from: f64, to: f64, limit: u32) -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: Some(from),
        to_ms: Some(to),
        limit: Some(limit),
    }
}
fn trades(limit: u32) -> Query {
    Query::PublicTrades {
        symbol: "UNKNOWN".into(),
        limit: Some(limit),
    }
}
fn query(e: &mut TestEngine, now: f64, command: u32, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command, query }));
    e.drain_actions().collect()
}
fn terminal(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn native(actions: &[Action], owner: u32, expected: Result<&str, &str>) {
    assert_eq!(
        actions.len(),
        1,
        "local terminal has no I/O, timer or publication"
    );
    let results = terminal(actions, owner);
    assert_eq!(results.len(), 1);
    match expected {
        Ok(data) => assert_eq!(results[0].as_ref().unwrap(), data),
        Err(message) => assert_eq!(*results[0].as_ref().unwrap_err(), VenueError::new(message)),
    }
}
fn rejection(actions: &[Action], owner: u32, kind: ErrorKind) {
    assert_eq!(
        actions.len(),
        1,
        "shared initial rejection is the only action"
    );
    let results = terminal(actions, owner);
    assert_eq!(results.len(), 1);
    let error = results[0]
        .as_ref()
        .expect_err("local results require bounded admission");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
}
fn book(e: &mut TestEngine, owner: u32) -> Preparation {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: "BTCUSD-PERP".into(),
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
        .expect("real book peer occupies one managed owner")
}
#[test]
fn original_local_history_capacity_refusal_and_terminal_peer_recovery() {
    for (read, result) in [
        (lookup(), Err(UNSUPPORTED)),
        (fills(1., 2., 0), Err(CREDS)),
        (trades(0), Ok("[]")),
    ] {
        let mut e = engine(false);
        let peers: Vec<_> = (1..=256).map(|owner| book(&mut e, owner)).collect();
        rejection(
            &query(&mut e, NOW, 600, read.clone()),
            600,
            ErrorKind::Internal,
        );
        assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
        e.http_preparation_failed(
            NOW + 1.,
            peers[0],
            Error::State,
            VenueError::new("settled peer"),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(terminal(&actions, 1).len(), 1);
        assert!(terminal(&actions, 600).is_empty());
        native(&query(&mut e, NOW + 2., 600, read), 600, result);
        assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
        assert_eq!(e.health().auth_failures, 0);
        assert!(e.take_frame(NOW + 2.).is_none());
    }
}
#[test]
fn original_local_history_invalid_clock_and_finite_id_reuse() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (read, result) in [
            (lookup(), Err(UNSUPPORTED)),
            (fills(1., 2., 0), Err(CREDS)),
            (trades(0), Ok("[]")),
        ] {
            let mut e = engine(false);
            rejection(
                &query(&mut e, now, 601, read.clone()),
                601,
                ErrorKind::InvalidInput,
            );
            native(&query(&mut e, NOW, 601, read), 601, result);
            assert_eq!(e.health().auth_failures, 0);
        }
    }
}
#[test]
fn finite_history_preserves_all_five_local_branches_and_validation_precedence() {
    for (private, read, result) in [
        (false, lookup(), Err(UNSUPPORTED)),
        (false, fills(f64::NAN, 1., 0), Err(CREDS)),
        (
            true,
            fills(f64::NAN, 1., 0),
            Err("Invalid history time range"),
        ),
        (
            true,
            fills(1., 1e14, 0),
            Err("History time range exceeds nanosecond bounds"),
        ),
        (true, fills(1., 2., 0), Ok("[]")),
        (true, fills(1., 1., 1), Ok("[]")),
        (true, fills(1.0000001, 1.0000002, 1), Ok("[]")),
        (false, trades(0), Ok("[]")),
    ] {
        let mut e = engine(private);
        native(&query(&mut e, NOW, 602, read), 602, result);
        assert_eq!(e.health().auth_failures, 0);
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn explicitly_bounded_income_limit_zero_and_successful_histories_keep_native_dispatch() {
    for read in [
        fills(1., 2., 1),
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1.),
            to_ms: Some(2.),
            limit: Some(0),
        },
        trades(1),
    ] {
        let private = !matches!(read, Query::PublicTrades { .. });
        let mut e = engine(private);
        e.handle(
            NOW,
            Event::Command(Command::Query {
                command: 603,
                query: read,
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(terminal(&actions, 603).is_empty());
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
        let request = actions
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        if private {
            let body: serde_json::Value =
                serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["params"]["limit"], 100);
            assert_eq!(body["params"]["start_time"], 1_000_000);
            assert_eq!(body["params"]["end_time"], 2_000_000);
        } else {
            assert!(
                request
                    .url
                    .ends_with("/public/get-trades?instrument_name=UNKNOWN&count=1")
            );
        }
        e.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"code":0,"result":{"data":[]}}"#,
                },
            },
        );
        let results = terminal(&e.drain_actions().collect::<Vec<_>>(), 603);
        assert_eq!(results, [Ok("[]".into())]);
    }
}
#[test]
fn repeated_local_results_release_owners_and_cannot_replace_a_live_managed_read() {
    let mut e = engine(false);
    for _ in 0..300 {
        native(&query(&mut e, NOW, 604, trades(0)), 604, Ok("[]"));
    }
    let peer = book(&mut e, 604);
    let actions = query(&mut e, NOW + 1., 604, lookup());
    assert!(terminal(&actions, 604).is_empty());
    assert!(
        matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    assert!(e.http_preparation_pending(peer));
    e.http_preparation_failed(
        NOW + 2.,
        peer,
        Error::State,
        VenueError::new("original owner"),
    );
    let result = terminal(&e.drain_actions().collect::<Vec<_>>(), 604);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].as_ref().unwrap_err().message, "original owner");
}

fn private_error_admission(read: Query, message: &str, full: bool) {
    let mut e = engine(true);
    let peers: Vec<_> = if full {
        (1..=256).map(|owner| book(&mut e, owner)).collect()
    } else {
        vec![]
    };
    let output = query(&mut e, if full { NOW } else { f64::NAN }, 605, read.clone());
    rejection(
        &output,
        605,
        if full {
            ErrorKind::Internal
        } else {
            ErrorKind::InvalidInput
        },
    );
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    if let Some(peer) = peers.first() {
        e.http_preparation_failed(NOW + 1., *peer, Error::State, VenueError::new("free peer"));
        let output: Vec<_> = e.drain_actions().collect();
        assert_eq!(terminal(&output, 1).len(), 1);
        assert!(terminal(&output, 605).is_empty());
    }
    native(&query(&mut e, NOW + 2., 605, read), 605, Err(message));
    assert!(peers.iter().skip(1).all(|p| e.http_preparation_pending(*p)));
    assert_eq!(e.health().auth_failures, 0);
    assert!(e.take_frame(NOW + 2.).is_none());
}
#[test]
fn original_private_range_error_requires_valid_clock() {
    private_error_admission(fills(f64::NAN, 1., 0), "Invalid history time range", false);
}
#[test]
fn original_private_range_error_requires_capacity() {
    private_error_admission(fills(f64::NAN, 1., 0), "Invalid history time range", true);
}
#[test]
fn original_private_nanosecond_error_requires_valid_clock() {
    private_error_admission(
        fills(1., 1e14, 0),
        "History time range exceeds nanosecond bounds",
        false,
    );
}
#[test]
fn original_private_nanosecond_error_requires_capacity() {
    private_error_admission(
        fills(1., 1e14, 0),
        "History time range exceeds nanosecond bounds",
        true,
    );
}
