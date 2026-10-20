use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query};
use safe_cex_venue_polymarket::{Options, Polymarket};
#[test]
fn unsupported_histories_fail_explicitly_without_network_or_empty_success() {
    let mut e = Engine::new(Polymarket::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    for query in [
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::PublicTrades {
            symbol: "anything".into(),
            limit: None,
        },
    ] {
        e.handle(0.0, Event::Command(Command::Query { command: 1, query }));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            matches!(&actions[..],[Action::Notice(Notice::CommandDone{command:1,result:Err(error)})] if error.contains("unsupported"))
        );
    }
}
#[test]
fn fixed_one_way_one_x_settings_are_read_only_and_client_lookup_is_not_definitive() {
    let mut e = Engine::new(Polymarket::new(Options::default()));
    for command in [
        Command::SetLeverage {
            command: 1,
            symbol: "x".into(),
            leverage: 100.0,
        },
        Command::SetMarginType {
            command: 1,
            symbol: "x".into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 1,
            hedged: true,
        },
    ] {
        e.handle(0.0, Event::Command(command));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            matches!(&actions[..],[Action::Notice(Notice::CommandDone{result:Ok(result),..})] if result=="[]")
        );
        assert!(!e.store().hedged());
    }
    e.handle(
        0.0,
        Event::Command(Command::FetchLeverage {
            command: 2,
            symbol: "x".into(),
        }),
    );
    assert!(
        matches!(e.drain_actions().next(),Some(Action::Notice(Notice::CommandDone{result:Ok(result),..})) if result==r#"{"buyLeverage":1,"sellLeverage":1}"#)
    );
    e.handle(
        0.0,
        Event::Command(Command::Query {
            command: 3,
            query: Query::OrderByClientId {
                symbol: "x".into(),
                client_order_id: "caller".into(),
            },
        }),
    );
    assert!(
        matches!(e.drain_actions().next(),Some(Action::Notice(Notice::CommandDone{result:Ok(result),..})) if result==r#"{"found":false,"definitive":false}"#)
    );
}
