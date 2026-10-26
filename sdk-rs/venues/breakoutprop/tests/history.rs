mod support;
use safe_cex_engine::{Command, Query};
use support::Rig;
#[test]
fn inherited_unimplemented_history_axes_return_empty_without_io() {
    let mut rig = Rig::defaults();
    rig.boot();
    let queries = [
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
    ];
    for (index, query) in queries.into_iter().enumerate() {
        let id = index as u32 + 1;
        rig.command(Command::Query { command: id, query });
        assert_eq!(rig.result(id).as_deref(), Ok("[]"));
    }
    assert!(rig.requests.is_empty());
}
#[test]
fn unsupported_client_ids_have_nondefinitive_lookup_and_leverage_is_unknown() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::Query {
        command: 1,
        query: Query::OrderByClientId {
            symbol: "ETHUSD".into(),
            client_order_id: "caller".into(),
        },
    });
    assert_eq!(
        rig.result(1).as_deref(),
        Ok(r#"{"found":false,"definitive":false}"#)
    );
    rig.command(Command::FetchLeverage {
        command: 2,
        symbol: "ETHUSD".into(),
    });
    assert_eq!(rig.result(2).as_deref(), Ok("null"));
    assert!(rig.requests.is_empty());
}
#[test]
fn public_trades_are_explicitly_unimplemented() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::Query {
        command: 1,
        query: Query::PublicTrades {
            symbol: "ETHUSD".into(),
            limit: Some(100),
        },
    });
    assert!(rig.result(1).is_err());
    assert!(rig.requests.is_empty());
}

mod owned_nonpageable {
    use super::*;
    use safe_cex_engine::{Action, ErrorKind, Event, Level, Notice, Outcome, Value, log::code};

    fn queries() -> [Query; 3] {
        [
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
        ]
    }
    fn incomplete(output: &[Action], command: u32) {
        let [
            Action::Notice(Notice::Event(marker)),
            Action::Notice(Notice::CommandDone {
                command: owner,
                result,
            }),
        ] = output
        else {
            panic!("one coverage marker before one result, and no I/O: {output:?}");
        };
        assert_eq!(*owner, command);
        assert_eq!(result, "[]");
        assert_eq!(marker.code, code::HISTORY_INCOMPLETE);
        assert_eq!(marker.level, Level::Warn);
        assert!(marker.message.contains("INCOMPLETE"));
        assert!(marker.message.contains("BreakoutProp"));
        assert!(
            matches!(marker.fields.as_slice(), [("command", Value::Num(owner))] if *owner == f64::from(command))
        );
    }

    #[test]
    fn history_has_owned_incomplete_coverage_even_with_cached_orders_and_error_log_level() {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            safe_cex_venue_breakoutprop::BreakoutProp::new(Default::default()),
        );
        e.store_mut()
            .set_orders(vec![safe_cex_engine::model::Order {
                id: "live-only".into(),
                parent_id: None,
                status: safe_cex_engine::model::OrderStatus::Open,
                symbol: "ETHUSD".into(),
                kind: safe_cex_engine::model::OrderType::Limit,
                side: safe_cex_engine::model::OrderSide::Buy,
                price: 1.,
                amount: 1.,
                filled: 0.,
                remaining: 1.,
                reduce_only: false,
                close: None,
                client_order_id: None,
                time_in_force: None,
                last_update: 1.,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            }]);
        e.drain_actions().for_each(drop);
        let before = e.store().orders().to_vec();
        assert!(
            !before.is_empty(),
            "live orders do not establish history completeness"
        );
        let balance = e.store().balance().clone();
        e.set_log_level(Level::Error);
        for (index, query) in queries().into_iter().enumerate() {
            let command = 100 + index as u32;
            e.handle(5000., Event::Command(Command::Query { command, query }));
            incomplete(&e.drain_actions().collect::<Vec<_>>(), command);
            assert_eq!(e.store().orders(), before);
            assert_eq!(e.store().balance(), &balance);
        }
    }

    #[test]
    fn public_trade_refusal_is_typed_not_sent_with_original_text_and_no_coverage() {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            safe_cex_venue_breakoutprop::BreakoutProp::new(Default::default()),
        );
        e.drain_actions().for_each(drop);
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 200,
                query: Query::PublicTrades {
                    symbol: "ETHUSD".into(),
                    limit: Some(10),
                },
            }),
        );
        let output: Vec<_> = e.drain_actions().collect();
        let [
            Action::Notice(Notice::CommandFailed {
                command: 200,
                error,
            }),
        ] = output.as_slice()
        else {
            panic!("one refusal, no I/O or unrelated coverage: {output:?}");
        };
        assert_eq!(
            error.message,
            "BreakoutProp public trades are not implemented"
        );
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }

    #[test]
    fn lookup_miss_remains_nondefinitive_without_a_history_marker_or_io() {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            safe_cex_venue_breakoutprop::BreakoutProp::new(Default::default()),
        );
        e.drain_actions().for_each(drop);
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 300,
                query: Query::OrderByClientId {
                    symbol: "ETHUSD".into(),
                    client_order_id: "0x11111111111111111111111111111111".into(),
                },
            }),
        );
        let output: Vec<_> = e.drain_actions().collect();
        let [
            Action::Notice(Notice::CommandDone {
                command: 300,
                result,
            }),
        ] = output.as_slice()
        else {
            panic!("one explicit nondefinitive lookup, no history marker or I/O: {output:?}");
        };
        assert_eq!(result, r#"{"found":false,"definitive":false}"#);
    }
}
