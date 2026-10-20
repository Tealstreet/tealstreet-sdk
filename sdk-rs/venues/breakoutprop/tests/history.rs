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
