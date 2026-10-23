//! Local order lookup results require genuine shared entry custody.
mod support;
use safe_cex_engine::model::{Interval, Order, OrderSide, OrderStatus, OrderType, TimeInForce};
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Query, VenueError};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const NOW: f64 = 1000.;
const VALID: &str = "0x00000000000000000000000000000001";
const IDENTITY: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
#[derive(Clone, Copy)]
enum Local {
    Cached,
    Invalid,
    Anonymous,
}
fn order() -> Order {
    Order {
        id: "native-17".into(),
        parent_id: Some("parent-3".into()),
        status: OrderStatus::Open,
        symbol: "BTC-PERP".into(),
        kind: OrderType::Limit,
        side: OrderSide::Sell,
        price: 91.25,
        amount: 2.,
        filled: 0.75,
        remaining: 1.25,
        reduce_only: true,
        close: Some(true),
        client_order_id: Some("cached-id".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 900.,
        trigger_type: None,
        take_profit: Some(95.),
        stop_loss: Some(80.),
        extra: None,
    }
}
fn engine(spot: bool, testnet: bool, public: bool, identity: bool) -> Engine<Hyperliquid> {
    let options = Options {
        public_data_only: public,
        testnet,
        application_id: identity.then(|| IDENTITY.into()),
        ..Options::default()
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(if spot {
        Hyperliquid::new_spot(options)
    } else {
        Hyperliquid::new(options)
    });
    engine.store_mut().set_orders(vec![order()]);
    engine.take_frame(NOW);
    engine
}
fn lookup(
    engine: &mut Engine<Hyperliquid>,
    now: f64,
    owner: u32,
    symbol: &str,
    client: &str,
) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::Query {
            command: owner,
            query: Query::OrderByClientId {
                symbol: symbol.into(),
                client_order_id: client.into(),
            },
        }),
    );
    engine.drain_actions().collect()
}
fn local(engine: &mut Engine<Hyperliquid>, now: f64, owner: u32, read: Local) -> Vec<Action> {
    lookup(
        engine,
        now,
        owner,
        "BTC-PERP",
        match read {
            Local::Cached => "cached-id",
            Local::Invalid => "bad-id",
            Local::Anonymous => VALID,
        },
    )
}
fn terminal(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|action| match action {
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
fn native(actions: &[Action], owner: u32, read: Local) {
    assert_eq!(
        actions.len(),
        1,
        "only one own terminal; no I/O, timer or marker"
    );
    let result = terminal(actions, owner);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].as_ref().unwrap(),
        &match read {
            Local::Cached => format!(
                r#"{{"found":true,"order":{}}}"#,
                serde_json::to_string(&order()).unwrap()
            ),
            _ => r#"{"found":false,"definitive":false}"#.into(),
        }
    );
}
fn refusal(actions: &[Action], owner: u32, kind: ErrorKind, message: &str) {
    assert_eq!(actions.len(), 1, "admission refusal has no side effects");
    let result = terminal(actions, owner);
    assert_eq!(result.len(), 1);
    let error = result[0]
        .as_ref()
        .expect_err("local lookup requires genuine entry admission");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.message, message);
    assert_eq!(error.outcome, None);
}
#[test]
fn exact_native_payloads_and_cache_precedence_in_all_modes() {
    for spot in [false, true] {
        for testnet in [false, true] {
            for public in [false, true] {
                for identity in [false, true] {
                    let mut engine = engine(spot, testnet, public, identity);
                    for read in [Local::Cached, Local::Invalid] {
                        native(&local(&mut engine, NOW, 500, read), 500, read);
                    }
                    if !identity {
                        native(
                            &local(&mut engine, NOW, 500, Local::Anonymous),
                            500,
                            Local::Anonymous,
                        );
                    }
                    native(
                        &lookup(&mut engine, NOW, 500, "ETH-PERP", "cached-id"),
                        500,
                        Local::Invalid,
                    );
                    assert_eq!(engine.store().orders(), &[order()]);
                    assert!(engine.take_frame(NOW).is_none());
                }
            }
        }
    }
}
fn invalid_clocks(read: Local) {
    for spot in [false, true] {
        for testnet in [false, true] {
            for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut engine = engine(spot, testnet, true, false);
                refusal(
                    &local(&mut engine, now, 501, read),
                    501,
                    ErrorKind::InvalidInput,
                    "Invalid request context",
                );
                native(&local(&mut engine, NOW, 501, read), 501, read);
                assert_eq!(engine.store().orders(), &[order()]);
                assert!(engine.take_frame(NOW).is_none());
            }
        }
    }
}
#[test]
fn cached_match_rejects_invalid_clock_before_result() {
    invalid_clocks(Local::Cached);
}
#[test]
fn invalid_client_rejects_invalid_clock_before_result() {
    invalid_clocks(Local::Invalid);
}
#[test]
fn missing_identity_rejects_invalid_clock_before_result() {
    invalid_clocks(Local::Anonymous);
}
fn started(testnet: bool, identity: bool) -> Engine<Hyperliquid> {
    let mut engine = engine(false, testnet, true, identity);
    engine.start(NOW);
    let meta = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) if support::kind(&request) == "metaAndAssetCtxs" => Some(request),
            _ => None,
        })
        .unwrap();
    support::reply(&mut engine, &meta, 200, support::META);
    engine.drain_actions().for_each(drop);
    engine.take_frame(NOW);
    engine
}
fn park(engine: &mut Engine<Hyperliquid>, owner: u32) -> Preparation {
    engine.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "BTC-PERP".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: Some(0.),
            to_ms: Some(60_000.),
        }),
    );
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real perps candle parks genuine managed owner")
}
fn capacity(read: Local) {
    for testnet in [false, true] {
        let mut engine = started(testnet, false);
        let peers: Vec<_> = (1..=256).map(|owner| park(&mut engine, owner)).collect();
        refusal(
            &local(&mut engine, NOW, 600, read),
            600,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
        engine.http_preparation_failed(
            NOW + 1.,
            peers[0],
            Error::State,
            VenueError::new("free peer"),
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(terminal(&actions, 1).len(), 1);
        assert!(terminal(&actions, 600).is_empty());
        native(&local(&mut engine, NOW + 2., 600, read), 600, read);
        engine.http_preparation_failed(
            NOW + 3.,
            peers[1],
            Error::State,
            VenueError::new("other peer"),
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(
            terminal(&actions, 2)[0].as_ref().unwrap_err().message,
            "other peer"
        );
        assert_eq!(engine.store().orders(), &[order()]);
        assert!(engine.take_frame(NOW + 3.).is_none());
    }
}
#[test]
fn cached_match_rejects_full_context_capacity() {
    capacity(Local::Cached);
}
#[test]
fn invalid_client_rejects_full_context_capacity() {
    capacity(Local::Invalid);
}
#[test]
fn missing_identity_rejects_full_context_capacity() {
    capacity(Local::Anonymous);
}
#[test]
fn public_only_uncached_refusal_is_unchanged_even_when_contexts_are_full() {
    let mut engine = started(false, true);
    for full in [false, true] {
        if full {
            for owner in 1..=256 {
                park(&mut engine, owner);
            }
        }
        let actions = local(&mut engine, NOW, 601, Local::Anonymous);
        assert_eq!(actions.len(), 1);
        let result = terminal(&actions, 601);
        let error = result[0].as_ref().unwrap_err();
        assert_eq!(error.message, "Hyperliquid account identity is missing");
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
    }
}
#[test]
fn duplicate_entry_cannot_replace_parked_owner() {
    let mut engine = started(false, false);
    let peer = park(&mut engine, 602);
    for read in [Local::Cached, Local::Invalid, Local::Anonymous] {
        let actions = local(&mut engine, NOW + 1., 602, read);
        assert!(terminal(&actions, 602).is_empty());
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
        );
    }
    engine.http_preparation_failed(
        NOW + 2.,
        peer,
        Error::State,
        VenueError::new("original owner"),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        terminal(&actions, 602)[0].as_ref().unwrap_err().message,
        "original owner"
    );
}
#[test]
fn repeated_synchronous_reads_release_context_capacity() {
    let mut engine = started(false, false);
    for _ in 0..300 {
        for read in [Local::Cached, Local::Invalid, Local::Anonymous] {
            native(&local(&mut engine, NOW, 603, read), 603, read);
        }
    }
    for owner in 1..=256 {
        park(&mut engine, owner);
    }
}
#[test]
fn uncached_valid_identity_retains_private_perps_rest_and_spot_local_miss() {
    let (mut perps, _) = support::ready("disabled");
    let actions = local(&mut perps, 5000., 604, Local::Anonymous);
    let request = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .expect("native legacy private lookup remains HTTP");
    assert_eq!(
        request.body.as_deref(),
        Some(&format!(r#"{{"type":"orderStatus","user":"{IDENTITY}","oid":"{VALID}"}}"#)[..])
    );
    assert!(terminal(&actions, 604).is_empty());
    let mut engine = engine(true, false, false, true);
    native(
        &local(&mut engine, NOW, 604, Local::Anonymous),
        604,
        Local::Anonymous,
    );
}
