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
fn income_and_orders_follow_empty_cursor_pages_before_real_rows() {
    for income in [false, true] {
        let mut h = private();
        let query = if income {
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            }
        } else {
            Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            }
        };
        read(&mut h, 19, query);
        h.reply(
            if income {
                "/fills?"
            } else {
                "/orders-history?"
            },
            r#"{"results":[],"next":"empty-page"}"#,
        );
        h.reply("cursor=empty-page", r#"{"results":[],"next":"real-page"}"#);
        h.reply(
            "cursor=real-page",
            &format!(r#"{{"results":[{}]}}"#, if income { FILL } else { ORDER }),
        );
        let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(19).unwrap()).unwrap();
        assert!(!rows.is_empty());
        assert_eq!(rows[0]["symbol"], "BTC-USD-PERP");
        assert!(h.e.store().orders().is_empty());
        assert!(h.e.store().positions().is_empty());
    }
}

#[test]
fn repeated_empty_history_cursor_fails_instead_of_returning_a_complete_empty_result() {
    let mut h = private();
    read(
        &mut h,
        20,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    h.reply("/orders-history?", r#"{"results":[],"next":"again"}"#);
    h.reply("cursor=again", r#"{"results":[],"next":"again"}"#);
    assert!(h.result(20).is_err());
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("/orders-history"))
    );
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

/// Pins the exact request sequence of each paged read. Paging refactors keep
/// it; a deliberate request change updates it with its bug-fix regression.
#[test]
fn paged_request_sequences_are_pinned() {
    let path = |url: &str| url.split_once("/v1").map_or(url, |(_, p)| p).to_owned();
    let mut sent = Vec::new();
    let mut h = private();
    let page = |id: &str, next: Option<&str>, row: &str| {
        let row = row
            .replace("fill-a", id)
            .replace(r#""id":"123""#, &format!(r#""id":"{id}""#));
        match next {
            Some(n) => format!(r#"{{"next":"{n}","results":[{row}]}}"#),
            None => format!(r#"{{"results":[{row}]}}"#),
        }
    };
    read(
        &mut h,
        1,
        Query::Fills {
            symbol: Some("BTC-USD-PERP".into()),
            order_id: None,
            from_ms: Some(1_681_375_000_000.),
            to_ms: Some(1_681_375_200_000.),
            limit: Some(5),
        },
    );
    for (id, next) in [("f1", Some("c1")), ("f2", None)] {
        let r = h.take("/fills?");
        sent.push(path(&r.url));
        h.respond(r, 200, &page(id, next, FILL));
    }
    assert!(h.result(1).is_ok());
    read(
        &mut h,
        2,
        Query::OrderHistory {
            symbol: Some("BTC-USD-PERP".into()),
            from_ms: Some(1_681_375_000_000.),
            to_ms: Some(1_681_375_200_000.),
            limit: None,
        },
    );
    for (id, next) in [("o1", Some("c2")), ("o2", None)] {
        let r = h.take("/orders-history?");
        sent.push(path(&r.url));
        h.respond(r, 200, &page(id, next, ORDER));
    }
    assert!(h.result(2).is_ok());
    read(
        &mut h,
        3,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1_681_375_000_000.),
            to_ms: Some(1_681_375_200_000.),
            limit: None,
        },
    );
    for (id, next) in [("i1", Some("c3")), ("i2", None)] {
        let r = h.take("/fills?");
        sent.push(path(&r.url));
        h.respond(r, 200, &page(id, next, FILL));
    }
    assert!(h.result(3).is_ok());
    assert_eq!(sent, PINNED);
}

const PINNED: [&str; 6] = [
    "/fills?market=BTC-USD-PERP&page_size=5&start_at=1681375000000&end_at=1681375200000",
    "/fills?market=BTC-USD-PERP&page_size=5&start_at=1681375000000&end_at=1681375200000&cursor=c1",
    "/orders-history?market=BTC-USD-PERP&page_size=100&start_at=1681375000000&end_at=1681375200000",
    "/orders-history?market=BTC-USD-PERP&page_size=100&start_at=1681375000000&end_at=1681375200000&cursor=c2",
    "/fills?page_size=500&start_at=1681375000000&end_at=1681375200000",
    "/fills?page_size=500&start_at=1681375000000&end_at=1681375200000&cursor=c3",
];

#[test]
fn fills_by_order_id_walk_past_pages_of_other_orders() {
    // The limit counts fills the read keeps, not raw rows before filtering.
    let mut h = private();
    read(
        &mut h,
        1,
        Query::Fills {
            symbol: Some("BTC-USD-PERP".into()),
            order_id: Some("target".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(2),
        },
    );
    let other = |id: &str| {
        FILL.replace("fill-a", id)
            .replace(r#""order_id":"123""#, r#""order_id":"other""#)
    };
    h.reply(
        "/fills?",
        &format!(
            r#"{{"next":"c1","results":[{},{}]}}"#,
            other("x1"),
            other("x2")
        ),
    );
    let target = FILL
        .replace("fill-a", "t1")
        .replace(r#""order_id":"123""#, r#""order_id":"target""#);
    h.reply("cursor=c1", &format!(r#"{{"results":[{target}]}}"#));
    let rows: Vec<serde_json::Value> = serde_json::from_str(h.result(1).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "t1");
}

#[test]
fn fills_send_their_start_bound() {
    let mut h = private();
    read(
        &mut h,
        1,
        Query::Fills {
            symbol: Some("BTC-USD-PERP".into()),
            order_id: None,
            from_ms: Some(1_681_375_000_000.),
            to_ms: None,
            limit: Some(5),
        },
    );
    let r = h.take("/fills?");
    assert!(r.url.contains("start_at=1681375000000"), "{}", r.url);
}

#[test]
fn a_page_cap_with_a_pending_cursor_is_incomplete_not_partial_success() {
    let mut h = private();
    read(
        &mut h,
        1,
        Query::OrderHistory {
            symbol: Some("BTC-USD-PERP".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    for page in 0..50 {
        let row = ORDER.replace(r#""id":"123""#, &format!(r#""id":"o{page}""#));
        h.reply(
            "/orders-history?",
            &format!(r#"{{"next":"c{page}","results":[{row}]}}"#),
        );
    }
    assert!(h.result(1).unwrap_err().contains("INCOMPLETE"));
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders-history?")));
}

#[test]
fn a_limited_order_history_stops_at_its_limit_instead_of_paging_to_the_cap() {
    let mut h = private();
    read(
        &mut h,
        1,
        Query::OrderHistory {
            symbol: Some("BTC-USD-PERP".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(2),
        },
    );
    let rows: Vec<String> = (0..3)
        .map(|i| ORDER.replace(r#""id":"123""#, &format!(r#""id":"o{i}""#)))
        .collect();
    h.reply(
        "/orders-history?",
        &format!(r#"{{"next":"c1","results":[{}]}}"#, rows.join(",")),
    );
    let result: Vec<serde_json::Value> = serde_json::from_str(h.result(1).unwrap()).unwrap();
    assert_eq!(result.len(), 2);
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders-history?")));
}

#[test]
fn a_lookup_found_in_history_finishes_without_walking_more_pages() {
    let mut h = private();
    read(
        &mut h,
        10,
        Query::OrderByClientId {
            symbol: "BTC-USD-PERP".into(),
            client_order_id: "caller".into(),
        },
    );
    h.reply("/orders?", r#"{"results":[]}"#);
    h.reply(
        "/orders-history?",
        &format!(r#"{{"next":"h2","results":[{ORDER}]}}"#),
    );
    let result: serde_json::Value = serde_json::from_str(h.result(10).unwrap()).unwrap();
    assert_eq!(result["found"], true);
    assert!(!h.pending.iter().any(|r| r.url.contains("cursor=h2")));
}
