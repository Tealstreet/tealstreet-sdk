//! Official /fills and /orders-history examples, synthetic IDs and account.
mod support;
use safe_cex_engine::{Command, Query};
use safe_cex_venue_paradex::Options;
use support::*;

fn private() -> Harness {
    let mut h = Harness::empty(Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h.reply("/auth/", r#"{"jwt_token":"test-jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h
}
fn fills(limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: Some("BTC-USD-PERP".into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit,
    }
}
fn read(h: &mut Harness, id: u32, query: Query) {
    h.command(Command::Query { command: id, query });
}
const FILL: &str = r#"{"id":"fill-a","order_id":"123","market":"BTC-USD-PERP","side":"BUY","price":"30000.12","size":"0.5","remaining_size":"0.5","fee":"-0.02","fee_currency":"USDC","realized_pnl":"7.56","realized_funding":"-0.1","created_at":1681375176910,"liquidity":"MAKER"}"#;
const ORDER: &str = r#"{"id":"123","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","status":"CLOSED","size":"2","remaining_size":"0.5","price":"30000","avg_fill_price":"30001","cancel_reason":"USER_CANCELED","created_at":1681375176910,"last_updated_at":1681375177000,"client_id":"caller","flags":[],"instruction":"GTC"}"#;

#[test]
fn fills_use_native_execution_size_and_identity_and_stop_after_limit() {
    let mut h = private();
    read(&mut h, 1, fills(Some(2)));
    h.reply(
        "/fills?",
        &format!(
            r#"{{"next":"unused-next","results":[{FILL},{}]}}"#,
            FILL.replace("fill-a", "fill-b")
        ),
    );
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(1).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["qty"], 0.5);
    assert_eq!(rows[0]["commission"], 0.02);
    assert_ne!(rows[0]["id"], rows[1]["id"]);
    assert!(!h.pending.iter().any(|r| r.url.contains("cursor=")));
}
#[test]
fn empty_fills_use_the_bounded_order_history_fallback() {
    let mut h = private();
    read(&mut h, 2, fills(Some(100)));
    h.reply("/fills?", r#"{"results":[]}"#);
    let r = h.take("/orders-history");
    assert!(r.url.contains("limit=100"));
    h.respond(r, 200, &format!(r#"{{"results":[{ORDER}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(2).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["qty"], 1.5);
    assert_eq!(rows[0]["orderId"], "123");
}
#[test]
fn cursor_loops_and_malformed_financial_reads_fail_without_touching_store() {
    let mut h = private();
    let balance = h.e.store().balance().clone();
    read(&mut h, 3, fills(Some(100)));
    h.reply(
        "/fills?",
        &format!(r#"{{"results":[{FILL}],"next":"again"}}"#),
    );
    h.reply(
        "cursor=again",
        &format!(r#"{{"results":[{FILL}],"next":"again"}}"#),
    );
    assert!(h.result(3).unwrap_err().contains("cursor"));
    read(&mut h, 4, fills(None));
    h.reply(
        "/fills?",
        r#"{"error":{"code":"UNAVAILABLE"},"results":[]}"#,
    );
    assert!(h.result(4).is_err());
    assert_eq!(h.e.store().balance(), &balance);
}
#[test]
fn income_preserves_rebates_funding_and_realized_pnl_from_one_fill() {
    let mut h = private();
    read(
        &mut h,
        5,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1681300000000.),
            to_ms: Some(1681400000000.),
            limit: None,
        },
    );
    h.reply("/fills?", &format!(r#"{{"results":[{FILL}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(5).unwrap()).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["income"], 7.56);
    assert_eq!(rows[1]["income"], 0.02);
    assert_eq!(rows[1]["category"], "rebate");
    assert_eq!(rows[2]["income"], -0.1);
}
#[test]
fn terminal_order_history_uses_cancel_reason_and_total_minus_remaining() {
    let mut h = private();
    read(
        &mut h,
        6,
        Query::OrderHistory {
            symbol: Some("BTC-USD-PERP".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    h.reply("/orders-history", &format!(r#"{{"results":[{ORDER}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(6).unwrap()).unwrap();
    assert_eq!(rows[0]["status"], "canceled");
    assert_eq!(rows[0]["filled"], 1.5);
    assert_eq!(rows[0]["averagePrice"], 30001.);
}
#[test]
fn lookup_checks_open_and_closed_orders_and_only_complete_misses_are_definitive() {
    let mut h = private();
    read(
        &mut h,
        7,
        Query::OrderByClientId {
            symbol: "BTC-USD-PERP".into(),
            client_order_id: "caller".into(),
        },
    );
    h.reply("/orders?", r#"{"results":[]}"#);
    h.reply("/orders-history", &format!(r#"{{"results":[{ORDER}]}}"#));
    let result: serde_json::Value = serde_json::from_str(h.result(7).unwrap()).unwrap();
    assert_eq!(result["found"], true);
    assert_eq!(result["order"]["id"], "123");
    assert_eq!(result["order"]["remaining"], 0.5);
    read(
        &mut h,
        8,
        Query::OrderByClientId {
            symbol: "BTC-USD-PERP".into(),
            client_order_id: "absent".into(),
        },
    );
    h.reply("/orders?", r#"{"results":[]}"#);
    h.reply("/orders-history", r#"{"results":[]}"#);
    assert_eq!(h.result(8).unwrap(), r#"{"found":false,"definitive":true}"#);
}
#[test]
fn public_trades_keep_the_shared_qty_and_quote_qty_shape_without_auth() {
    let mut h = Harness::new();
    read(
        &mut h,
        9,
        Query::PublicTrades {
            symbol: "BTC-USD-PERP".into(),
            limit: Some(1),
        },
    );
    let r = h.take("/trades?");
    assert!(r.headers.iter().all(|(k, _)| *k != "Authorization"));
    h.respond(r, 200, &format!(r#"{{"results":[{FILL}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(9).unwrap()).unwrap();
    assert_eq!(rows[0]["qty"], 0.5);
    assert_eq!(rows[0]["quoteQty"], 15000.06);
    assert_eq!(rows[0]["timestamp"], 1681375176910.);
}

#[test]
fn lookup_exhausts_open_order_pages_before_a_definitive_miss() {
    let mut h = private();
    read(
        &mut h,
        10,
        Query::OrderByClientId {
            symbol: "BTC-USD-PERP".into(),
            client_order_id: "caller".into(),
        },
    );
    h.reply("/orders?", r#"{"results":[],"next":"open-page-2"}"#);
    let r = h.take("cursor=open-page-2");
    assert!(r.url.contains("/orders?"));
    h.respond(
        r,
        200,
        &format!(r#"{{"results":[{}]}}"#, ORDER.replace("CLOSED", "OPEN")),
    );
    let result: serde_json::Value = serde_json::from_str(h.result(10).unwrap()).unwrap();
    assert_eq!(result["found"], true);
    assert_eq!(result["order"]["id"], "123");
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders-history")));
}
#[test]
fn empty_fill_page_with_cursor_is_not_an_order_aggregate_fallback() {
    let mut h = private();
    read(&mut h, 11, fills(Some(100)));
    h.reply("/fills?", r#"{"results":[],"next":"page-2"}"#);
    let r = h.take("cursor=page-2");
    assert!(r.url.contains("/fills?"));
    h.respond(r, 200, &format!(r#"{{"results":[{FILL}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(11).unwrap()).unwrap();
    assert_eq!(rows[0]["qty"], 0.5);
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders-history")));
}
#[test]
fn malformed_financial_scalars_fail_instead_of_becoming_zero_or_missing_rows() {
    for field in [
        "price",
        "size",
        "created_at",
        "fee",
        "realized_pnl",
        "realized_funding",
    ] {
        let mut h = private();
        read(&mut h, 12, fills(None));
        let mut row: serde_json::Value = serde_json::from_str(FILL).unwrap();
        row[field] = "bad-number".into();
        h.reply("/fills?", &format!(r#"{{"results":[{row}]}}"#));
        assert!(h.result(12).is_err(), "{field}");
    }
}
#[test]
fn invalid_fill_time_ranges_reject_before_reads_and_unknown_lookup_keeps_its_shape() {
    let mut h = private();
    let pending = h.pending.len();
    for (id, from, to) in [(13, f64::NAN, 1.), (14, 2., 1.)] {
        read(
            &mut h,
            id,
            Query::Fills {
                symbol: Some("BTC-USD-PERP".into()),
                order_id: None,
                from_ms: Some(from),
                to_ms: Some(to),
                limit: None,
            },
        );
        assert!(h.result(id).is_err());
        assert_eq!(h.pending.len(), pending);
    }
    read(
        &mut h,
        15,
        Query::OrderByClientId {
            symbol: "UNKNOWN".into(),
            client_order_id: "caller".into(),
        },
    );
    assert_eq!(
        h.result(15).unwrap(),
        r#"{"found":false,"definitive":false}"#
    );
}

#[test]
fn bounded_lookup_never_calls_an_incomplete_open_order_walk_a_definitive_miss() {
    let mut h = private();
    read(
        &mut h,
        16,
        Query::OrderByClientId {
            symbol: "BTC-USD-PERP".into(),
            client_order_id: "caller".into(),
        },
    );
    for page in 0..50 {
        h.reply(
            "/orders?",
            &format!(r#"{{"results":[],"next":"page-{page}"}}"#),
        );
    }
    h.reply("/orders-history", r#"{"results":[]}"#);
    assert_eq!(
        h.result(16).unwrap(),
        r#"{"found":false,"definitive":false}"#
    );
}
#[test]
fn zero_order_limit_means_unbounded_and_public_trade_limit_clamps_to_one() {
    let mut h = private();
    read(
        &mut h,
        17,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: Some(0),
        },
    );
    h.reply("/orders-history", &format!(r#"{{"results":[{ORDER}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(17).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    read(
        &mut h,
        18,
        Query::PublicTrades {
            symbol: "BTC-USD-PERP".into(),
            limit: Some(0),
        },
    );
    h.reply("/trades?", &format!(r#"{{"results":[{FILL}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(18).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
