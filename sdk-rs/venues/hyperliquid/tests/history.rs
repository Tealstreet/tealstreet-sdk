//! Documented userFillsByTime, userFunding, historicalOrders and recentTrades shapes.
mod support;
use support::TestEngine as Engine;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, Query};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::Value;
use support::*;
fn query(e: &mut Engine<Hyperliquid>, q: Query) {
    e.handle(
        5000.0,
        Event::Command(Command::Query {
            command: 1,
            query: q,
        }),
    );
}
fn read(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    e.wait_http(|_| true)
}
fn result(e: &mut Engine<Hyperliquid>) -> Value {
    let result = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap()
        .unwrap();
    serde_json::from_str(&result).unwrap()
}
fn fill(tid: u64) -> String {
    format!(
        r#"{{"coin":"BTC","side":"B","px":"100","sz":"0.5","time":1000,"tid":{tid},"oid":99,"fee":"-0.01","closedPnl":"2","crossed":false,"feeToken":"USDC"}}"#
    )
}
#[test]
fn inclusive_fill_paging_preserves_distinct_identical_executions_and_rebate_sign() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Fills {
            symbol: Some("BTC-PERP".into()),
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, &format!("[{}]", fill(1)));
    let r = read(&mut e);
    let b: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["startTime"], 1000.0);
    reply(&mut e, &r, 200, &format!("[{},{}]", fill(1), fill(2)));
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["commission"], -0.01);
    assert!(rows[0].get("positionSide").is_none());
}
#[test]
fn fills_use_vault_identity_and_keep_requested_time_range() {
    let vault = "0x0000000000000000000000000000000000000001";
    let (mut e, _) = ready_identity("disabled", false, None, "30", Some(vault.into()));
    query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(10_000_000_000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    let b: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["user"], vault);
    assert_eq!(b["startTime"], 1.0);
}
#[test]
fn order_transition_collapse_terminal_wins_ties_and_filters_creation_time() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    let wire = r#"{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"2","sz":"0","orderType":"Limit","timestamp":2000}"#;
    reply(
        &mut e,
        &r,
        200,
        &format!(
            r#"[{{"order":{wire},"status":"filled","statusTimestamp":2000}},{{"order":{wire},"status":"open","statusTimestamp":2000}}]"#
        ),
    );
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["status"], "closed");
    assert_eq!(rows[0]["amount"], 2.0);
    assert_eq!(rows[0]["filled"], 2.0);
    assert!(rows[0].get("averagePrice").is_none());
}
#[test]
fn income_splits_fill_pnl_and_rebate_and_reads_nested_funding() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, &format!("[{}]", fill(1)));
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let r = read(&mut e);
    assert_eq!(kind(&r), "userFunding");
    reply(
        &mut e,
        &r,
        200,
        r#"[{"time":2000,"hash":"fund","delta":{"coin":"BTC","usdc":" -0.2"}}]"#,
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "rebate" && r["income"] == 0.01)
    );
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "funding" && r["income"] == -0.2)
    );
}
#[test]
fn failed_or_malformed_history_never_resolves_as_empty_success() {
    for (status, body) in [
        (503, "unavailable"),
        (200, r#"{"error":"rate limited"}"#),
        (200, r#"[{"coin":"BTC","tid":1}]"#),
    ] {
        let (mut e, _) = ready("disabled");
        query(
            &mut e,
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(1.0),
                to_ms: Some(5000.0),
                limit: None,
            },
        );
        let r = read(&mut e);
        reply(&mut e, &r, status, body);
        assert!(e.drain_actions().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}
#[test]
fn public_trades_use_market_coin_and_return_engine_json_shape() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::PublicTrades {
            symbol: "BTC-PERP".into(),
            limit: Some(1),
        },
    );
    let r = read(&mut e);
    assert_eq!(kind(&r), "recentTrades");
    assert!(r.body.as_ref().unwrap().contains("\"coin\":\"BTC\""));
    reply(
        &mut e,
        &r,
        200,
        r#"[{"coin":"BTC","side":"B","px":"100","sz":"2","time":3000,"tid":7}]"#,
    );
    let rows = result(&mut e);
    assert_eq!(rows[0]["quoteQty"], 200.0);
    assert_eq!(rows[0]["timestamp"], 3000.0);
}
#[test]
fn client_id_lookup_is_pure_and_misses_are_never_definitive() {
    let (mut e, _) = ready("disabled");
    let before = e.store().orders().to_vec();
    query(
        &mut e,
        Query::OrderByClientId {
            symbol: "BTC-PERP".into(),
            client_order_id: "0x00000000000000000000000000000001".into(),
        },
    );
    let r = read(&mut e);
    assert_eq!(kind(&r), "orderStatus");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"order","order":{"order":{"coin":"BTC","oid":9,"cloid":"0x00000000000000000000000000000001","side":"B","limitPx":"90","origSz":"1","sz":"0","orderType":"Limit"},"status":"filled"}}"#,
    );
    let found = result(&mut e);
    assert_eq!(found["found"], true);
    assert_eq!(found["order"]["status"], "closed");
    assert_eq!(e.store().orders(), before);
    query(
        &mut e,
        Query::OrderByClientId {
            symbol: "BTC-PERP".into(),
            client_order_id: "0x00000000000000000000000000000002".into(),
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, r#"{"status":"unknownOid"}"#);
    let miss = result(&mut e);
    assert_eq!(miss["definitive"], false);
}
#[test]
fn execution_limit_sorts_by_original_milliseconds_before_serialized_seconds() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: Some(1),
        },
    );
    let r = read(&mut e);
    reply(
        &mut e,
        &r,
        200,
        &format!(
            "[{},{}]",
            fill(9).replace("\"time\":1000", "\"time\":1100"),
            fill(10).replace("\"time\":1000", "\"time\":1900")
        ),
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows[0]["id"], "10");
}
#[test]
fn full_overlap_page_is_incomplete_instead_of_an_empty_success() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, &format!("[{}]", fill(1)));
    let r = read(&mut e);
    let repeated = format!("[{}]", vec![fill(1); 2000].join(","));
    reply(&mut e, &r, 200, &repeated);
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}
#[test]
fn concurrent_heavy_reads_reserve_response_weight_before_dispatch() {
    let (mut e, _) = ready("disabled");
    for command in 1..=20 {
        e.handle(
            5000.0,
            Event::Command(Command::Query {
                command,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: Some(1.0),
                    to_ms: Some(5000.0),
                    limit: None,
                },
            }),
        );
    }
    let requests = e
        .drain_actions()
        .filter(|a| matches!(a, Action::Http(_)))
        .count();
    assert!(
        requests <= 8,
        "must reserve the 120-weight maximum per heavy response"
    );
}
