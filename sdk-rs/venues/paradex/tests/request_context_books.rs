//! Explicit host readiness and claim controls for independent snapshot reads.
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Preparation, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex};

const NOW: f64 = 1000.;
const BOOK: &str = r#"{"bids":[["2","3"]],"asks":[]}"#;

fn engine(testnet: bool) -> Engine<Paradex> {
    safe_cex_engine::test_support::seeded_engine(Paradex::new(Options {
        testnet,
        ..Options::default()
    }))
}
fn ask(e: &mut Engine<Paradex>, now: f64, command: u32) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: "BTC /&-USD-PERP".into(),
            limit: Some(7),
        }),
    );
    e.drain_actions().collect()
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "no legacy HTTP or physical send before claim"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request.clone())),
                _ => None,
            },
            _ => None,
        })
        .expect("snapshot read must await managed preparation")
}
fn release(e: &mut Engine<Paradex>, token: Preparation, now: f64) -> HttpRequest {
    e.http_prepared(
        now,
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
        .unwrap();
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn reply(e: &mut Engine<Paradex>, now: f64, r: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn results(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
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

#[test]
fn public_snapshot_parks_with_exact_wire_and_ignores_all_preclaim_callbacks() {
    for (testnet, root) in [
        (false, "https://api.prod.paradex.trade/v1"),
        (true, "https://api.testnet.paradex.trade/v1"),
    ] {
        let mut e = engine(testnet);
        e.take_frame(NOW);
        let actions = ask(&mut e, NOW, 500);
        let (token, unsigned) = preparation(&actions);
        assert_eq!(unsigned.method, Method::Get);
        assert_eq!(
            unsigned.url,
            format!("{root}/orderbook/BTC%20%2F%26-USD-PERP?depth=7")
        );
        assert!(unsigned.headers.is_empty());
        assert!(unsigned.body.is_none());
        assert!(results(&actions, 500).is_empty());
        assert!(results(&reply(&mut e, NOW + 1., &unsigned, BOOK), 500).is_empty());
        e.handle(
            NOW + 1.,
            Event::HttpStart {
                request: unsigned.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            NOW + 1.,
            Event::HttpChunk {
                request: unsigned.id,
                data: BOOK.as_bytes(),
            },
        );
        e.handle(
            NOW + 1.,
            Event::HttpEnd {
                request: unsigned.id,
            },
        );
        assert!(results(&e.drain_actions().collect::<Vec<_>>(), 500).is_empty());
        let request = release(&mut e, token, NOW + 2.);
        let done = results(&reply(&mut e, NOW + 3., &request, BOOK), 500);
        assert_eq!(done.len(), 1);
        assert_eq!(
            done[0].as_ref().unwrap(),
            r#"{"bids":[{"price":2.0,"amount":3.0,"total":6.0}],"asks":[]}"#
        );
        assert!(e.store().markets().is_empty());
        assert!(
            e.take_frame(NOW + 3.).is_none(),
            "REST snapshot does not publish a live book"
        );
        assert!(results(&reply(&mut e, NOW + 4., &request, BOOK), 500).is_empty());
    }
}

#[test]
fn typed_preparation_denial_is_one_failure_without_network_or_auth_side_effects() {
    let mut e = engine(false);
    let (token, _) = preparation(&ask(&mut e, NOW, 501));
    e.http_preparation_failed(
        NOW + 1.,
        token,
        Error::State,
        VenueError::new("route denied").kind(ErrorKind::AuthPermission),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let done = results(&actions, 501);
    assert_eq!(done.len(), 1);
    let error = done[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::AuthPermission));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.message, "route denied");
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::AccountAuth { .. })
    )));
    e.http_preparation_failed(NOW + 2., token, Error::State, VenueError::new("duplicate"));
    assert!(results(&e.drain_actions().collect::<Vec<_>>(), 501).is_empty());
}

#[test]
fn original_deadline_fences_parked_or_released_result_and_reused_command_id() {
    for claimed in [false, true] {
        let mut e = engine(false);
        let actions = ask(&mut e, NOW, 502);
        let (token, unsigned) = preparation(&actions);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        if claimed {
            release(&mut e, token, NOW + 10_000.);
        }
        e.handle(NOW + 20_000., Event::TimerFired { timer });
        let failures = results(&e.drain_actions().collect::<Vec<_>>(), 502);
        assert!(matches!(failures.as_slice(), [Err(_)]));
        if !claimed {
            assert_eq!(
                failures[0].as_ref().unwrap_err().kind,
                Some(ErrorKind::Timeout)
            );
        }
        let (next, _) = preparation(&ask(&mut e, NOW + 20_001., 502));
        assert!(results(&reply(&mut e, NOW + 20_002., &unsigned, BOOK), 502).is_empty());
        let request = release(&mut e, next, NOW + 20_003.);
        assert!(matches!(
            results(&reply(&mut e, NOW + 20_004., &request, BOOK), 502).as_slice(),
            [Ok(_)]
        ));
    }
}

#[test]
fn concurrent_read_failure_cannot_retire_its_sibling() {
    let mut e = engine(false);
    let (first, _) = preparation(&ask(&mut e, NOW, 503));
    let (second, _) = preparation(&ask(&mut e, NOW, 504));
    e.http_preparation_failed(
        NOW + 1.,
        first,
        Error::State,
        VenueError::new("first denied"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(matches!(results(&actions, 503).as_slice(), [Err(_)]));
    assert!(results(&actions, 504).is_empty());
    assert!(e.http_preparation_pending(second));
    let request = release(&mut e, second, NOW + 2.);
    assert!(matches!(
        results(&reply(&mut e, NOW + 3., &request, BOOK), 504).as_slice(),
        [Ok(_)]
    ));
}

#[test]
fn initial_authority_capacity_refusal_is_visible_and_does_not_leak_the_freed_slot() {
    let mut e = engine(false);
    let mut first = None;
    for command in 1..=256 {
        let (token, _) = preparation(&ask(&mut e, NOW, command));
        first.get_or_insert(token);
    }
    let actions = ask(&mut e, NOW, 257);
    assert!(matches!(results(&actions, 257).as_slice(), [Err(_)]));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    e.http_preparation_failed(
        NOW + 1.,
        first.unwrap(),
        Error::State,
        VenueError::new("free slot"),
    );
    e.drain_actions().for_each(drop);
    preparation(&ask(&mut e, NOW + 2., 257));
}
