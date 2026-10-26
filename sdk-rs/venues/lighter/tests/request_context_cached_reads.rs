//! Cached reads still require genuine command admission, without transport work.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, VenueAction, VenueError};
use safe_cex_venue_lighter::Lighter;
use support::Options;
use support::{META, NOW, reply_at};

#[derive(Clone, Copy)]
enum Cached {
    Leverage,
    MissingLeverage,
    Tier,
}
fn engine(robinhood: bool, testnet: bool) -> Engine<Lighter> {
    let mut engine = support::engine(Options {
        public_data_only: true,
        robinhood,
        testnet,
        ..Options::default()
    });
    engine
        .store_mut()
        .update_setting("ETH", |setting| setting.leverage = 7.5);
    engine.take_frame(NOW);
    engine
}
fn cached(engine: &mut Engine<Lighter>, now: f64, command: u32, read: Cached) -> Vec<Action> {
    engine.handle(
        now,
        match read {
            Cached::Tier => Event::Command(Command::Venue {
                command,
                action: VenueAction::FetchAccountTier,
            }),
            Cached::Leverage | Cached::MissingLeverage => Event::Command(Command::FetchLeverage {
                command,
                symbol: if matches!(read, Cached::Leverage) {
                    "ETH"
                } else {
                    "UNKNOWN"
                }
                .into(),
            }),
        },
    );
    engine.drain_actions().collect()
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
fn native_result(actions: &[Action], owner: u32, read: Cached) {
    assert_eq!(
        actions.len(),
        1,
        "cached reads emit only their own terminal, no I/O or timer"
    );
    let result = terminal(actions, owner);
    assert_eq!(result.len(), 1);
    match read {
        Cached::Leverage => assert_eq!(
            result[0].as_ref().unwrap(),
            r#"{"buyLeverage":7.5,"sellLeverage":7.5}"#
        ),
        Cached::Tier => assert_eq!(result[0].as_ref().unwrap(), "null"),
        Cached::MissingLeverage => {
            let error = result[0].as_ref().unwrap_err();
            assert_eq!(error.message, "Lighter leverage is not loaded");
            assert_eq!(error.kind, None);
            assert_eq!(error.outcome, None);
        }
    }
}
fn candle(engine: &mut Engine<Lighter>, owner: u32) -> Preparation {
    engine.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "ETH".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
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
        .expect("actual managed candle peer parks one owner")
}
fn started(robinhood: bool, testnet: bool) -> Engine<Lighter> {
    let mut engine = engine(robinhood, testnet);
    engine.start(NOW);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    reply_at(&mut engine, NOW, &request, 200, META);
    engine.drain_actions().for_each(drop);
    engine
        .store_mut()
        .update_setting("ETH", |setting| setting.leverage = 7.5);
    engine.take_frame(NOW);
    engine
}
#[test]
fn finite_cached_native_results_have_no_transport_or_publication_in_all_deployments() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut engine = engine(robinhood, testnet);
            for read in [Cached::Leverage, Cached::MissingLeverage, Cached::Tier] {
                native_result(&cached(&mut engine, NOW, 500, read), 500, read);
                assert!(engine.take_frame(NOW).is_none());
            }
        }
    }
}
fn capacity_refusal(read: Cached) {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut engine = started(robinhood, testnet);
            let mut peers = Vec::new();
            for command in 1..=256 {
                peers.push(candle(&mut engine, command));
            }
            let owner = 600;
            let actions = cached(&mut engine, NOW, owner, read);
            assert_eq!(
                actions.len(),
                1,
                "refusal allocates no I/O, timer or cache publication"
            );
            let result = terminal(&actions, owner);
            assert_eq!(result.len(), 1);
            let error = result[0]
                .as_ref()
                .expect_err("cached reads require bounded admission");
            assert_eq!(error.kind, Some(ErrorKind::Internal));
            assert_eq!(error.outcome, None);
            assert_eq!(error.message, "Engine request capacity reached");
            engine.http_preparation_failed(
                NOW + 1.,
                peers[0],
                Error::State,
                VenueError::new("free peer slot"),
            );
            let actions: Vec<_> = engine.drain_actions().collect();
            assert_eq!(terminal(&actions, 1).len(), 1);
            assert!(terminal(&actions, 600).is_empty());
            native_result(&cached(&mut engine, NOW + 2., 600, read), 600, read);
            engine.http_preparation_failed(
                NOW + 3.,
                peers[1],
                Error::State,
                VenueError::new("independent peer"),
            );
            let result = terminal(&engine.drain_actions().collect::<Vec<_>>(), 2);
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].as_ref().unwrap_err().message, "independent peer");
        }
    }
}
#[test]
fn cached_leverage_refuses_full_owner_capacity() {
    capacity_refusal(Cached::Leverage);
}
#[test]
fn missing_leverage_capacity_refusal_precedes_native_cache_error() {
    capacity_refusal(Cached::MissingLeverage);
}
#[test]
fn cached_tier_refuses_full_owner_capacity() {
    capacity_refusal(Cached::Tier);
}
#[test]
fn cached_leverage_invalid_clocks_refuse_then_allow_finite_id_reuse() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut engine = engine(false, false);
        let actions = cached(&mut engine, now, 700, Cached::Leverage);
        assert_eq!(actions.len(), 1);
        let result = terminal(&actions, 700);
        assert_eq!(result.len(), 1);
        let error = result[0]
            .as_ref()
            .expect_err("cached leverage must admit the genuine entry clock");
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        assert_eq!(error.message, "Invalid request context");
        native_result(
            &cached(&mut engine, NOW, 700, Cached::Leverage),
            700,
            Cached::Leverage,
        );
    }
}
#[test]
fn cached_tier_invalid_clocks_refuse_then_allow_finite_id_reuse() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut engine = engine(false, false);
        let actions = cached(&mut engine, now, 701, Cached::Tier);
        assert_eq!(actions.len(), 1);
        let result = terminal(&actions, 701);
        assert_eq!(result.len(), 1);
        let error = result[0]
            .as_ref()
            .expect_err("cached tier must admit the genuine entry clock");
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        assert_eq!(error.message, "Invalid request context");
        native_result(
            &cached(&mut engine, NOW, 701, Cached::Tier),
            701,
            Cached::Tier,
        );
    }
}
#[test]
fn cached_synchronous_reuse_releases_every_owner_without_accumulating_resources() {
    let mut engine = engine(false, false);
    for index in 0..300 {
        let read = if index % 2 == 0 {
            Cached::Leverage
        } else {
            Cached::Tier
        };
        native_result(&cached(&mut engine, NOW, 702, read), 702, read);
    }
    assert!(engine.take_frame(NOW).is_none());
}
#[test]
fn cached_entry_cannot_replace_an_existing_candle_owner() {
    let mut engine = started(false, false);
    let peer = candle(&mut engine, 703);
    for read in [Cached::Leverage, Cached::Tier] {
        let actions = cached(&mut engine, NOW + 1., 703, read);
        assert!(terminal(&actions, 703).is_empty());
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
    let result = terminal(&engine.drain_actions().collect::<Vec<_>>(), 703);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].as_ref().unwrap_err().message, "original owner");
}
