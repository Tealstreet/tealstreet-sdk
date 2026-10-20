//! History payloads and paging contracts: exchange Bitunix CLAUDE.md and official history APIs.
mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use serde::Deserialize;
use support::*;
fn done(e: &mut TestEngine) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    qty: f64,
    commission: f64,
    commission_asset: String,
}
#[test]
fn fills_keep_distinct_execution_ids_at_the_same_price_and_timestamp() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 1,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: Some("one".into()),
                from_ms: Some(1700000000.0),
                to_ms: Some(1700000010.0),
                limit: Some(100),
            },
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("startTime=1700000000000"));
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"tradeList":[{"tradeId":"a","orderId":"one","symbol":"BTCUSDT","ctime":1700000001000,"side":"SELL","price":"100","qty":"0.1","fee":"0.002","marginCoin":"USDT"},{"tradeId":"b","orderId":"one","symbol":"BTCUSDT","ctime":1700000001000,"side":"SELL","price":"100","qty":"0.1","fee":"0.002","marginCoin":"USDT"}]}}"#,
    );
    let rows: Vec<Execution> = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "a");
    assert_eq!(rows[1].id, "b");
    assert_eq!(rows[0].timestamp, 1700000001.0);
    assert_eq!(rows[0].qty, 0.1);
    assert_eq!(rows[0].commission, 0.002);
    assert_eq!(rows[0].commission_asset, "USDT");
}
#[test]
fn failed_or_truncated_streams_cannot_return_unvalidated_history_rows() {
    for body in [
        r#"{"code":10001,"data":{"tradeList":[{"tradeId":"a","orderId":"one","symbol":"BTCUSDT","ctime":1700000001000,"side":"SELL","price":"100","qty":"1"}]}}"#,
        r#"{"code":0,"data":{"tradeList":[{"tradeId":"a","orderId":"one","symbol":"BTCUSDT","ctime":1700000001000,"side":"SELL","price":"100","qty":"1"}],"broken": "#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::Query {
                command: 2,
                query: Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let r = requests(&mut e).remove(0);
        reply(&mut e, r.id, body);
        assert_eq!(done(&mut e).unwrap(), "[]");
    }
}
fn history_order(id: usize, t: f64) -> String {
    format!(
        r#"{{"orderId":"{id}","symbol":"BTCUSDT","ctime":{t},"side":"BUY","orderType":"MARKET","price":"MARKET","avgPrice":"100","qty":"1","tradeQty":"1","status":"FILLED"}}"#
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    price: Option<f64>,
    average_price: Option<f64>,
}
#[test]
fn orders_walk_overlapping_time_boundaries_instead_of_ignored_skip() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 3,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: Some(1700000010000.0),
                limit: None,
            },
        }),
    );
    let r = requests(&mut e).remove(0);
    let rows = (0..100)
        .map(|id| history_order(id, 1700000000000.0))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        r.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("endTime=1700000000000"));
    assert!(!r.url.contains("skip"));
    reply(
        &mut e,
        r.id,
        &format!(
            r#"{{"code":0,"data":{{"orderList":[{},{}]}}}}"#,
            history_order(99, 1700000000000.0),
            history_order(100, 1699999999999.0)
        ),
    );
    let rows: Vec<HistoricalOrder> = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 101);
    assert_eq!(rows.last().unwrap().id, "100");
    assert_eq!(rows[0].price, None);
    assert_eq!(rows[0].average_price, Some(100.0));
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    income_type: String,
    income: f64,
    tran_id: String,
}
#[test]
fn income_uses_skip_pages_preserves_net_realized_and_keeps_fee_signs() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 4,
            query: Query::Income {
                symbol: Some("BTCUSDT".into()),
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let r = requests(&mut e).remove(0);
    let rows=(0..100).map(|id|format!(r#"{{"tradeId":"{id}","symbol":"BTCUSDT","ctime":1700000000000,"realizedPNL":"-3.5","fee":"2"}}"#)).collect::<Vec<_>>().join(",");
    reply(
        &mut e,
        r.id,
        &format!(r#"{{"code":0,"data":{{"tradeList":[{rows}]}}}}"#),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("skip=100"));
    reply(
        &mut e,
        r.id,
        r#"{"code":10001,"msg":"read failed","data":{"tradeList":[{"tradeId":"fake","symbol":"BTCUSDT","ctime":1700000000000,"realizedPNL":"999"}]}}"#,
    );
    let rows: Vec<Income> = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 200);
    assert_eq!(rows[0].income_type, "REALIZED_PNL");
    assert_eq!(rows[0].income, -3.5);
    assert_eq!(rows[1].income_type, "COMMISSION");
    assert_eq!(rows[1].income, -2.0);
    assert!(!rows.iter().any(|r| r.tran_id.starts_with("fake")));
}
#[test]
fn public_trades_preserves_the_ts_unsupported_contract() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 5,
            query: Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: None,
            },
        }),
    );
    assert_eq!(done(&mut e).unwrap_err(), "Not implemented");
}

#[test]
fn client_order_lookup_rejects_without_a_request_or_panic() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 8,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "caller-id".into(),
            },
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 8, result: Err(error) })
            if error == "Bitunix client order lookup is not supported by the Rust engine"
    )));
}

#[test]
fn fills_accept_timestamp_and_fee_aliases_and_emit_canonical_symbols() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 6,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"userTrades":[{"tradeId":"alias","instId":"btc-usdt","ts":1700000001000000,"side":"Buy","fillPrice":"100","fillSize":"0.2","execFee":"-0.004","feeCoin":"USDT"}]}}"#,
    );
    let rows: Vec<Execution> = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].timestamp, 1700000001.0);
    assert_eq!(rows[0].commission, 0.004);
}
#[test]
fn an_id_free_order_page_never_counts_as_paging_progress() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 7,
            query: Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let r = requests(&mut e).remove(0);
    let rows = (0..100)
        .map(|_| r#"{"symbol":"BTCUSDT","ctime":1700000000000,"side":"BUY"}"#)
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        r.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Notice(Notice::CommandDone {result:Ok(s),..}) if s=="[]"))
    );
}
