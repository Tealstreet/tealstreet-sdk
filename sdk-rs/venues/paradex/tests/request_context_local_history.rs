//! Synchronous history results retain genuine Read completion custody.
mod support;

use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Query, VenueError};
use safe_cex_venue_paradex::{Options, Paradex};
use support::{Harness, MARKETS, TICKERS};

const NOW: f64 = 1_700_000_000_000.;
const SYMBOL: &str = "BTC-USD-PERP";

#[derive(Clone, Copy, Debug)]
enum Local {
    Public,
    Client,
    Bounds,
    NoSymbol,
}
const LOCALS: [Local; 4] = [Local::Public, Local::Client, Local::Bounds, Local::NoSymbol];

fn engine(testnet: bool, private: bool) -> Engine<Paradex> {
    let mut h = Harness::empty(Options {
        testnet,
        public_data_only: !private,
        key: private.then(|| "opaque-public-subkey".into()),
        paradex_address: private.then(|| "0x123".into()),
        subkey_address: private.then(|| "0x456".into()),
        payload_signer: private,
    });
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    if private {
        // Settle the startup root while retaining the fixture's uninitialized auth.
        h.reply("/system/config", "{}");
    }
    h.e.take_frame(NOW);
    h.e
}

fn fills(symbol: Option<&str>, from_ms: Option<f64>, to_ms: Option<f64>) -> Query {
    Query::Fills {
        symbol: symbol.map(str::to_owned),
        order_id: None,
        from_ms,
        to_ms,
        limit: Some(100),
    }
}
fn lookup(id: &str) -> Query {
    Query::OrderByClientId {
        symbol: "UNKNOWN".into(),
        client_order_id: id.into(),
    }
}
fn query(e: &mut Engine<Paradex>, now: f64, command: u32, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command, query }));
    e.drain_actions().collect()
}
fn local(e: &mut Engine<Paradex>, now: f64, command: u32, branch: Local) -> Vec<Action> {
    query(
        e,
        now,
        command,
        match branch {
            Local::Public | Local::Client => lookup(""),
            Local::Bounds => fills(None, Some(2.), Some(1.)),
            Local::NoSymbol => fills(None, Some(0.), Some(0.)),
        },
    )
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
fn only_terminal(actions: &[Action], owner: u32) {
    assert_eq!(
        actions.len(),
        1,
        "local history must have no companion effects"
    );
    assert_eq!(terminal(actions, owner).len(), 1);
}
fn native(actions: &[Action], owner: u32, branch: Local) {
    only_terminal(actions, owner);
    let result = terminal(actions, owner).pop().unwrap();
    match branch {
        Local::Public => assert_eq!(result.unwrap(), r#"{"found":false,"definitive":false}"#),
        Local::NoSymbol => assert_eq!(result.unwrap(), "[]"),
        Local::Client | Local::Bounds => {
            let error = result.unwrap_err();
            assert_eq!(
                error.message,
                if matches!(branch, Local::Client) {
                    "invalid Paradex client order ID"
                } else {
                    "invalid Paradex history time range"
                }
            );
            assert_eq!((error.kind, error.outcome), (None, None));
        }
    }
}
fn refused(actions: &[Action], owner: u32, kind: ErrorKind, message: &str) {
    only_terminal(actions, owner);
    let result = terminal(actions, owner).pop().unwrap();
    assert!(
        result.is_err(),
        "Read admission must deny this local result: {result:?}"
    );
    let error = result.unwrap_err();
    assert_eq!(error.message, message);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
}
fn no_store_effects(e: &mut Engine<Paradex>) {
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
    assert!(e.take_frame(NOW).is_none());
}
fn park(e: &mut Engine<Paradex>, command: u32) -> Preparation {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: SYMBOL.into(),
            limit: Some(1),
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(terminal(&actions, command).is_empty());
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let prepared: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(request.url.contains("/orderbook/"));
                    assert!(request.headers.is_empty());
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(prepared.len(), 1, "peer must own a genuine parked Read");
    prepared[0]
}
fn retire(e: &mut Engine<Paradex>, peer: Preparation, owner: u32, message: &str) {
    e.http_preparation_failed(NOW, peer, Error::State, VenueError::new(message));
    let actions: Vec<_> = e.drain_actions().collect();
    let errors = terminal(&actions, owner);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].as_ref().unwrap_err().message, message);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn native_local_precedence_and_read_metadata_are_preserved() {
    for testnet in [false, true] {
        for branch in LOCALS {
            let mut e = engine(testnet, !matches!(branch, Local::Public));
            native(&local(&mut e, NOW, 800, branch), 800, branch);
            no_store_effects(&mut e);
        }
        let mut public = engine(testnet, false);
        for q in [
            fills(Some("UNKNOWN"), Some(f64::NAN), None),
            Query::OrderHistory {
                symbol: None,
                from_ms: Some(-1.),
                to_ms: None,
                limit: None,
            },
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(2.),
                to_ms: Some(1.),
                limit: None,
            },
        ] {
            let actions = query(&mut public, NOW, 801, q);
            only_terminal(&actions, 801);
            assert_eq!(terminal(&actions, 801).as_slice(), [Ok("[]".into())]);
        }
        no_store_effects(&mut public);
        let mut private = engine(testnet, true);
        for id in ["".into(), "界".repeat(65), "a\rb".into(), "a\nb".into()] {
            native(
                &query(&mut private, NOW, 802, lookup(&id)),
                802,
                Local::Client,
            );
        }
        for id in ["a".into(), "界".repeat(64)] {
            let actions = query(&mut private, NOW, 803, lookup(&id));
            assert_eq!(
                terminal(&actions, 803)[0].as_ref().unwrap_err().message,
                "Paradex private auth is not initialized"
            );
        }
        for (from, to) in [
            (Some(f64::NAN), None),
            (None, Some(f64::INFINITY)),
            (Some(f64::NEG_INFINITY), None),
            (Some(-1.), None),
            (None, Some(-1.)),
            (Some(2.), Some(1.)),
        ] {
            for kind in 0..3 {
                let q = match kind {
                    0 => fills(None, from, to),
                    1 => Query::OrderHistory {
                        symbol: None,
                        from_ms: from,
                        to_ms: to,
                        limit: None,
                    },
                    _ => Query::Income {
                        symbol: None,
                        kind: None,
                        from_ms: from,
                        to_ms: to,
                        limit: None,
                    },
                };
                native(&query(&mut private, NOW, 804, q), 804, Local::Bounds);
            }
        }
        for (from, to) in [(None, None), (Some(0.), Some(0.)), (Some(1.), Some(2.))] {
            native(
                &query(&mut private, NOW, 805, fills(None, from, to)),
                805,
                Local::NoSymbol,
            );
        }
        no_store_effects(&mut private);
    }
}

fn capacity(branch: Local) {
    for testnet in [false, true] {
        let mut e = engine(testnet, !matches!(branch, Local::Public));
        let peers: Vec<_> = (1..=256).map(|owner| park(&mut e, owner)).collect();
        refused(
            &local(&mut e, NOW, 810, branch),
            810,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
        retire(&mut e, peers[0], 1, "free one slot");
        native(&local(&mut e, NOW, 810, branch), 810, branch);
        park(&mut e, 811);
        refused(
            &local(&mut e, NOW, 812, branch),
            812,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
        for (index, peer) in peers.into_iter().enumerate().skip(1) {
            retire(
                &mut e,
                peer,
                index as u32 + 1,
                "peer survived local admission",
            );
        }
        no_store_effects(&mut e);
    }
}
#[test]
fn public_only_result_requires_available_read_authority() {
    capacity(Local::Public);
}
#[test]
fn invalid_client_refusal_requires_available_read_authority() {
    capacity(Local::Client);
}
#[test]
fn invalid_bounds_refusal_requires_available_read_authority() {
    capacity(Local::Bounds);
}
#[test]
fn no_symbol_fills_requires_available_read_authority() {
    capacity(Local::NoSymbol);
}

#[test]
fn invalid_clock_refusal_precedes_each_local_terminal_without_financial_outcome() {
    for testnet in [false, true] {
        for branch in LOCALS {
            for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut e = engine(testnet, !matches!(branch, Local::Public));
                refused(
                    &local(&mut e, now, 820, branch),
                    820,
                    ErrorKind::InvalidInput,
                    "Invalid request context",
                );
                native(&local(&mut e, NOW, 820, branch), 820, branch);
                no_store_effects(&mut e);
            }
        }
    }
}

#[test]
fn duplicate_entry_cannot_complete_another_read_and_reused_ids_retire_fully() {
    for testnet in [false, true] {
        for branch in LOCALS {
            let mut e = engine(testnet, !matches!(branch, Local::Public));
            let peer = park(&mut e, 830);
            let actions = local(&mut e, NOW, 830, branch);
            assert!(terminal(&actions, 830).is_empty());
            assert!(
                matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
                if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
            );
            retire(&mut e, peer, 830, "original owner survived");
            for _ in 0..300 {
                native(&local(&mut e, NOW, 830, branch), 830, branch);
            }
            for owner in 1..=256 {
                park(&mut e, owner);
            }
            no_store_effects(&mut e);
        }
    }
}

#[test]
fn public_trades_bypass_local_private_history_completion_on_both_modes() {
    for testnet in [false, true] {
        for private in [false, true] {
            let mut e = engine(testnet, private);
            let actions = query(
                &mut e,
                NOW,
                840,
                Query::PublicTrades {
                    symbol: SYMBOL.into(),
                    limit: Some(1),
                },
            );
            assert!(terminal(&actions, 840).is_empty());
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::ManagedHttp(action)
                if matches!(action.as_ref(), HttpAction::Prepare { request, .. }
                    if request.url.ends_with("/trades?market=BTC-USD-PERP&page_size=1"))))
            );
            no_store_effects(&mut e);
        }
    }
}
