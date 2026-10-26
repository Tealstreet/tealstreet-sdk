mod support;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, Query};
use serde::Deserialize;
use support::*;

const FILL: &str = r#"{"id":9007199254740993,"order_id":9007199254740995,"symbol":"PERP_BTC_USDC","side":"BUY","executed_price":"51000","executed_quantity":"0.25","executed_timestamp":2000,"fee":"0.1","fee_asset":"USDC","is_maker":1,"realized_pnl":"2.5"}"#;
const FUNDING: &str = r#"{"symbol":"PERP_BTC_USDC","funding_rate":0.0001,"mark_price":51000,"funding_fee":1.5,"payment_type":"Pay","status":"Settled","margin_mode":"CROSS","created_time":2000}"#;
const HIST_ORDER: &str = r#"{"order_id":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"FILLED","quantity":"1","price":"50000","average_executed_price":"50001","total_executed_quantity":"1","reduce_only":false,"created_time":2000,"updated_time":2500}"#;
const TRADE: &str = r#"{"symbol":"PERP_BTC_USDC","side":"SELL","executed_price":51000,"executed_quantity":0.25,"executed_timestamp":2000}"#;

fn ask(e: &mut Driver, query: Query) {
    ask_with_id(e, 77, query);
}
fn ask_with_id(e: &mut Driver, command: u32, query: Query) {
    e.handle(2000.0, Event::Command(Command::Query { command, query }));
}
fn result(e: &mut Driver) -> Result<String, String> {
    e.drain_actions()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 77,
                result,
            }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("query completed")
}
fn request(e: &mut Driver) -> HttpRequest {
    requests(e).pop().expect("history request")
}
/// The provider signs at release, so a delayed page is prepared at its clock.
fn request_at(e: &mut Driver, now: f64) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .next_back()
        .expect("history request")
}
/// The venue's spacing timer: command roots also arm their own deadlines.
fn earliest_timer(out: Vec<Action>) -> (safe_cex_engine::TimerId, f64) {
    out.into_iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap()
}
fn page(rows: &str, total: usize, number: u32) -> String {
    format!(
        r#"{{"success":true,"data":{{"rows":[{rows}],"meta":{{"total":{total},"current_page":{number},"records_per_page":500}}}}}}"#
    )
}
fn fills(symbol: Option<&str>, order: Option<&str>, limit: u32) -> Query {
    Query::Fills {
        symbol: symbol.map(String::from),
        order_id: order.map(String::from),
        from_ms: Some(2000.0),
        to_ms: Some(3000.0),
        limit: Some(limit),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillResult {
    id: String,
    order_id: String,
    timestamp: f64,
    qty: f64,
    commission: f64,
    maker: bool,
    realized_pnl: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeResult {
    income: f64,
    timestamp: f64,
    category: String,
    tran_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderResult {
    id: String,
    status: String,
    reduce_only: bool,
    created_time: f64,
    updated_time: Option<f64>,
    #[serde(rename = "type")]
    kind: String,
    average_price: Option<f64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeResult {
    id: String,
    timestamp: f64,
    quote_qty: f64,
}

#[test]
fn fills_preserve_large_ids_filters_seconds_and_fee_details() {
    let (mut e, _) = ready();
    actions(&mut e);
    let original = e.store().orders().to_vec();
    ask(&mut e, fills(Some("BTCUSDC"), None, 100));
    let r = request(&mut e);
    assert!(
        r.url
            .ends_with("/v1/trades?symbol=PERP_BTC_USDC&start_t=2000&end_t=3000&page=1&size=500")
    );
    let signed = r
        .headers
        .iter()
        .find(|(k, _)| *k == "orderly-textsign")
        .unwrap();
    assert!(
        signed
            .1
            .starts_with("1000GET/v1/trades?symbol=PERP_BTC_USDC&")
    );
    let outside = FILL
        .replace("9007199254740993", "9007199254740994")
        .replace("\"executed_timestamp\":2000", "\"executed_timestamp\":1999");
    reply(&mut e, &r, 200, &page(&format!("{FILL},{outside}"), 2, 1));
    let rows: Vec<FillResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "9007199254740993");
    assert_eq!(rows[0].order_id, "9007199254740995");
    assert_eq!(rows[0].timestamp, 2.0);
    assert_eq!(rows[0].qty, 0.25);
    assert_eq!(rows[0].commission, 0.1);
    assert!(rows[0].maker);
    assert_eq!(rows[0].realized_pnl, 2.5);
    assert_eq!(e.store().orders(), original);
}

#[test]
fn order_specific_fills_use_documented_endpoint_and_exact_client_filters() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        fills(Some("BTCUSDC"), Some("9007199254740995"), 100),
    );
    let r = request(&mut e);
    assert!(r.url.ends_with("/v1/order/9007199254740995/trades"));
    let wrong_order = FILL
        .replace("9007199254740993", "21")
        .replace("9007199254740995", "22");
    let wrong_symbol = FILL
        .replace("9007199254740993", "23")
        .replace("PERP_BTC_USDC", "PERP_ETH_USDC");
    reply(
        &mut e,
        &r,
        200,
        &format!(r#"{{"success":true,"data":{{"rows":[{FILL},{wrong_order},{wrong_symbol}]}}}}"#),
    );
    let rows: Vec<FillResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn paged_fills_deduplicate_overlaps_and_warn_on_failed_continuations() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(&mut e, fills(None, None, 100));
    let first = request(&mut e);
    reply(&mut e, &first, 200, &page(FILL, 2, 1));
    let second = request(&mut e);
    assert!(second.url.contains("page=2"));
    let other = FILL.replace("9007199254740993", "9007199254740994");
    reply(
        &mut e,
        &second,
        200,
        &page(&format!("{FILL},{other}"), 2, 2),
    );
    let rows: Vec<FillResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    ask(&mut e, fills(None, None, 100));
    let first = request(&mut e);
    reply(&mut e, &first, 200, &page(FILL, 2, 1));
    let second = request(&mut e);
    reply(
        &mut e,
        &second,
        503,
        r#"{"success":false,"message":"busy"}"#,
    );
    // Supersedes the old whole-read refusal: only the validated first page is
    // recoverable, and the shared owner marks that result incomplete.
    let rows: Vec<FillResult> =
        serde_json::from_str(&owned_terminal(&mut e, 77, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn repeated_or_malformed_pages_fail_the_whole_read() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(&mut e, fills(None, None, 100));
    let first = request(&mut e);
    reply(&mut e, &first, 200, &page(FILL, 3, 1));
    let second = request(&mut e);
    reply(&mut e, &second, 200, &page(FILL, 3, 2));
    assert!(result(&mut e).unwrap_err().contains("repeated"));
    ask(&mut e, fills(None, None, 100));
    let r = request(&mut e);
    let bad = FILL.replace(
        "\"executed_quantity\":\"0.25\"",
        "\"executed_quantity\":\"NaN\"",
    );
    reply(&mut e, &r, 200, &page(&format!("{FILL},{bad}"), 2, 1));
    assert!(result(&mut e).is_err());
}

#[test]
fn funding_is_settled_only_with_payment_direction_and_mode_safe_identity() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::Income {
            symbol: None,
            kind: Some("FUNDING_FEE".into()),
            from_ms: Some(2000.0),
            to_ms: Some(3000.0),
            limit: Some(100),
        },
    );
    let r = request(&mut e);
    assert!(
        r.url
            .contains("/v1/funding_fee/history?start_t=2000&end_t=3000")
    );
    let receive = FUNDING
        .replace("\"Pay\"", "\"Receive\"")
        .replace("\"CROSS\"", "\"ISOLATED\"");
    let accrued = FUNDING.replace("\"Settled\"", "\"Accrued\"");
    reply(
        &mut e,
        &r,
        200,
        &page(&format!("{FUNDING},{receive},{accrued}"), 3, 1),
    );
    let rows: Vec<IncomeResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.income == -1.5));
    assert!(rows.iter().any(|r| r.income == 1.5));
    assert_ne!(rows[0].tran_id, rows[1].tran_id);
    assert!(
        rows.iter()
            .all(|r| r.timestamp == 2.0 && r.category == "funding")
    );
}

#[test]
fn history_merges_ordinary_and_algo_orders_with_millisecond_order_times() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDC".into()),
            from_ms: Some(1000.0),
            to_ms: Some(3000.0),
            limit: Some(100),
        },
    );
    let r = request(&mut e);
    assert!(
        r.url
            .contains("/v1/orders?symbol=PERP_BTC_USDC&start_t=1000")
    );
    reply(&mut e, &r, 200, &page(HIST_ORDER, 1, 1));
    let r = request(&mut e);
    assert!(
        r.url
            .contains("/v1/algo/orders?symbol=PERP_BTC_USDC&start_t=1000")
    );
    let algo = r#"{"algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"CANCELLED","root_algo_order_status":"CANCELLED","created_time":2100,"updated_time":2600,"reduce_only":true,"child_orders":[{"algo_order_id":101,"algo_type":"STOP_LOSS","algo_status":"CANCELLED","side":"SELL","quantity":0,"trigger_price":49000}]}"#;
    reply(&mut e, &r, 200, &page(algo, 1, 1));
    let rows: Vec<OrderResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "101");
    assert_eq!(rows[0].kind, "stop_market");
    assert_eq!(rows[0].created_time, 2100.0);
    assert_eq!(rows[0].updated_time, Some(2600.0));
    assert_eq!(rows[1].average_price, Some(50001.0));
}

#[test]
fn terminal_history_includes_rejections_and_filters_open_orders_locally() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = request(&mut e);
    assert!(!r.url.contains("status="));
    let rejected = HIST_ORDER.replace("\"FILLED\"", "\"REJECTED\"").replace(
        "\"total_executed_quantity\":\"1\"",
        "\"total_executed_quantity\":\"0\"",
    );
    let open = HIST_ORDER
        .replace("9007199254740993", "9007199254740994")
        .replace("\"FILLED\"", "\"NEW\"");
    reply(&mut e, &r, 200, &page(&format!("{rejected},{open}"), 2, 1));
    let r = request(&mut e);
    assert!(!r.url.contains("status="));
    let rejected_algo = r#"{"algo_order_id":101,"symbol":"PERP_BTC_USDC","algo_type":"STOP_LOSS","algo_status":"REJECTED","side":"SELL","quantity":1,"trigger_price":49000,"created_time":2000}"#;
    reply(&mut e, &r, 200, &page(rejected_algo, 1, 1));
    let rows: Vec<OrderResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.status == "canceled"));
    assert!(!rows.iter().any(|r| r.id == "9007199254740994"));
}

#[test]
fn nested_bracket_history_keeps_entry_and_explicit_exit_statuses() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = request(&mut e);
    reply(&mut e, &r, 200, &page("", 0, 1));
    let r = request(&mut e);
    let bracket = r#"{"algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"BRACKET","type":"LIMIT","algo_status":"FILLED","side":"BUY","quantity":1,"price":50000,"created_time":2000,"reduce_only":false,"child_orders":[{"algo_order_id":101,"algo_type":"POSITIONAL_TP_SL","root_algo_order_status":"FILLED","created_time":2100,"reduce_only":true,"child_orders":[{"algo_order_id":102,"algo_type":"STOP_LOSS","algo_status":"FILLED","side":"SELL","quantity":1,"total_executed_quantity":1,"trigger_price":49000},{"algo_order_id":103,"algo_type":"TAKE_PROFIT","algo_status":"CANCELLED","side":"SELL","quantity":1,"trigger_price":52000}]}]}"#;
    reply(&mut e, &r, 200, &page(bracket, 1, 1));
    let rows: Vec<OrderResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 3);
    let entry = rows.iter().find(|r| r.id == "100").unwrap();
    assert_eq!(entry.kind, "limit");
    assert_eq!(entry.created_time, 2000.0);
    let stop = rows.iter().find(|r| r.id == "102").unwrap();
    assert_eq!(stop.status, "closed");
    assert_eq!(stop.created_time, 2100.0);
    assert_eq!(
        rows.iter().find(|r| r.id == "103").unwrap().status,
        "canceled"
    );
}

#[test]
fn excessively_nested_algo_history_fails_explicitly() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = request(&mut e);
    reply(&mut e, &r, 200, &page("", 0, 1));
    let r = request(&mut e);
    let mut node = r#"{"algo_order_id":300,"algo_type":"STOP_LOSS","side":"SELL","quantity":1,"trigger_price":49000}"#.to_owned();
    for id in 100..110 {
        node = format!(
            r#"{{"algo_order_id":{id},"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"FILLED","created_time":2000,"child_orders":[{node}]}}"#
        );
    }
    reply(&mut e, &r, 200, &page(&node, 1, 1));
    assert!(result(&mut e).unwrap_err().contains("budget"));
}

#[test]
fn algo_history_preserves_opening_entries_and_classifies_reduce_only_stops() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = request(&mut e);
    reply(&mut e, &r, 200, &page("", 0, 1));
    let r = request(&mut e);
    let bracket = r#"{"algo_order_id":100,"symbol":"PERP_BTC_USDC","algo_type":"BRACKET","type":"LIMIT","algo_status":"FILLED","side":"BUY","quantity":1,"price":50000,"created_time":2000}"#;
    let close = r#"{"algo_order_id":101,"symbol":"PERP_BTC_USDC","algo_type":"STOP","type":"MARKET","algo_status":"FILLED","side":"SELL","quantity":1,"trigger_price":49000,"created_time":2000,"reduce_only":true}"#;
    let opening = close
        .replace("\"algo_order_id\":101", "\"algo_order_id\":102")
        .replace("\"reduce_only\":true", "\"reduce_only\":false");
    reply(
        &mut e,
        &r,
        200,
        &page(&format!("{bracket},{close},{opening}"), 3, 1),
    );
    let rows: Vec<OrderResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert!(!rows.iter().find(|r| r.id == "100").unwrap().reduce_only);
    let close = rows.iter().find(|r| r.id == "101").unwrap();
    assert!(close.reduce_only);
    assert_eq!(close.kind, "stop_market");
    let opening = rows.iter().find(|r| r.id == "102").unwrap();
    assert!(!opening.reduce_only);
    assert_eq!(opening.kind, "conditional");
}

#[test]
fn public_trade_keys_are_stable_and_keep_identical_same_time_executions() {
    let (mut e, _) = ready();
    actions(&mut e);
    let mut previous = vec![];
    for _ in 0..2 {
        ask(
            &mut e,
            Query::PublicTrades {
                symbol: "BTCUSDC".into(),
                limit: Some(1000),
            },
        );
        let r = request(&mut e);
        assert!(
            r.url
                .ends_with("/v1/public/market_trades?symbol=PERP_BTC_USDC&limit=500")
        );
        assert!(r.headers.is_empty());
        reply(
            &mut e,
            &r,
            200,
            &format!(r#"{{"success":true,"data":{{"rows":[{TRADE},{TRADE}]}}}}"#),
        );
        let rows: Vec<TradeResult> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].id, rows[1].id);
        assert!(
            rows.iter()
                .all(|r| r.timestamp == 2.0 && r.quote_qty == 12750.0)
        );
        let keys: Vec<_> = rows.into_iter().map(|r| r.id).collect();
        if previous.is_empty() {
            previous = keys;
        } else {
            assert_eq!(previous, keys);
        }
    }
}

#[test]
fn query_preflight_rejects_unsupported_ledgers_ranges_and_noncanonical_ids() {
    let (mut e, _) = ready();
    actions(&mut e);
    let mut invalid = vec![
        fills(None, Some("+1"), 100),
        fills(None, Some("01"), 100),
        fills(None, Some("18446744073709551616"), 100),
        fills(None, None, 0),
        fills(Some("MISSING"), None, 100),
        Query::Income {
            symbol: None,
            kind: Some("REALIZED_PNL".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    ];
    invalid.push(Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: Some(3000.0),
        to_ms: Some(2000.0),
        limit: None,
    });
    invalid.push(Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: Some(f64::NAN),
        to_ms: None,
        limit: None,
    });
    for query in invalid {
        ask(&mut e, query);
        let out = actions(&mut e);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}

#[test]
fn pagination_has_a_hard_fifty_page_budget_and_checks_metadata() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(&mut e, fills(Some("BTCUSDC"), None, 100));
    for number in 1..=50 {
        let r = request(&mut e);
        let outside = FILL
            .replace("9007199254740993", &format!("{}", number + 10))
            .replace("PERP_BTC_USDC", "PERP_ETH_USDC");
        reply(&mut e, &r, 200, &page(&outside, 51, number));
    }
    assert!(result(&mut e).unwrap_err().contains("pagination"));
    ask(&mut e, fills(None, None, 100));
    let r = request(&mut e);
    reply(&mut e, &r, 200, &page(FILL, 1, 2));
    assert!(result(&mut e).unwrap_err().contains("pagination"));
}

#[test]
fn oversized_page_rejects_and_failed_algo_history_recovers_validated_regular_orders() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::PublicTrades {
            symbol: "BTCUSDC".into(),
            limit: Some(500),
        },
    );
    let r = request(&mut e);
    let rows = std::iter::repeat_n(TRADE, 501)
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        &r,
        200,
        &format!(r#"{{"success":true,"data":{{"rows":[{rows}]}}}}"#),
    );
    assert!(result(&mut e).unwrap_err().contains("oversized"));
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = request(&mut e);
    reply(&mut e, &r, 200, &page(HIST_ORDER, 1, 1));
    let r = request(&mut e);
    reply(
        &mut e,
        &r,
        401,
        r#"{"success":false,"message":"unauthorized"}"#,
    );
    let rows: Vec<OrderResult> =
        serde_json::from_str(&owned_terminal(&mut e, 77, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn missing_data_or_rows_and_nonarray_rows_are_errors() {
    let (mut e, _) = ready();
    actions(&mut e);
    for body in [
        r#"{"success":true}"#,
        r#"{"success":true,"data":{"meta":{"total":0}}}"#,
        r#"{"success":true,"data":{"rows":null}}"#,
        r#"{"success":true,"data":{"rows":{}}}"#,
    ] {
        ask(&mut e, fills(None, None, 100));
        let r = request(&mut e);
        reply(&mut e, &r, 200, body);
        assert!(result(&mut e).is_err());
    }
}

#[test]
fn funding_commands_and_page_continuations_share_account_rate_slots() {
    let (mut e, _) = ready();
    actions(&mut e);
    let income = || Query::Income {
        symbol: None,
        kind: None,
        from_ms: None,
        to_ms: None,
        limit: Some(100),
    };
    ask(&mut e, income());
    let first = request(&mut e);
    ask_with_id(&mut e, 78, income());
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let (second_timer, second_at) = earliest_timer(out);
    assert_eq!(second_at, 5001.0);
    reply(&mut e, &first, 200, &page(FUNDING, 2, 1));
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let (page_timer, page_at) = earliest_timer(out);
    assert_eq!(page_at, 8002.0);
    e.handle(
        second_at,
        Event::TimerFired {
            timer: second_timer,
        },
    );
    let second = request_at(&mut e, second_at);
    assert!(second.url.contains("page=1"));
    assert_eq!(
        second
            .headers
            .iter()
            .find(|(k, _)| *k == "orderly-timestamp")
            .unwrap()
            .1,
        "5001"
    );
    e.handle(page_at, Event::TimerFired { timer: page_timer });
    let continuation = request_at(&mut e, page_at);
    assert!(continuation.url.contains("page=2"));
    assert_eq!(
        continuation
            .headers
            .iter()
            .find(|(k, _)| *k == "orderly-timestamp")
            .unwrap()
            .1,
        "8002"
    );
}

#[test]
fn funding_timers_delayed_by_host_throttling_cannot_dispatch_together() {
    let (mut e, _) = ready();
    actions(&mut e);
    let income = || Query::Income {
        symbol: None,
        kind: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    };
    ask(&mut e, income());
    request(&mut e);
    let mut timers = vec![];
    for command in 78..80 {
        ask_with_id(&mut e, command, income());
        timers.push(earliest_timer(actions(&mut e)).0);
    }
    e.handle(10000.0, Event::TimerFired { timer: timers[0] });
    let dispatched = request_at(&mut e, 10000.0);
    assert_eq!(
        dispatched
            .headers
            .iter()
            .find(|(k, _)| *k == "orderly-timestamp")
            .unwrap()
            .1,
        "10000"
    );
    e.handle(10000.0, Event::TimerFired { timer: timers[1] });
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let (timer, at) = earliest_timer(out);
    assert!(at >= 13001.0);
    e.handle(at, Event::TimerFired { timer });
    let dispatched = request_at(&mut e, at);
    assert_eq!(
        dispatched
            .headers
            .iter()
            .find(|(k, _)| *k == "orderly-timestamp")
            .unwrap()
            .1,
        format!("{at:.0}")
    );
}

// Owned-history controls: validated output, never native census or failed-page
// prefixes, is the authority for recovery after an actual read failure.
fn owned_terminal(
    e: &mut Driver,
    command: u32,
    partial: bool,
) -> Result<String, safe_cex_engine::VenueError> {
    let out = actions(e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let markers: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        markers.len(),
        usize::from(partial),
        "exactly one owned marker for partial recovery"
    );
    for marker in markers {
        assert_eq!(marker.level, safe_cex_engine::Level::Warn);
        assert!(marker.fields.iter().any(|(k, v)| *k == "command"
            && matches!(v,safe_cex_engine::Value::Num(n) if *n == f64::from(command))));
    }
    let mut terminals = out.into_iter().filter_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: id,
            result,
        }) if id == command => Some(Ok(result)),
        Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
            Some(Err(error))
        }
        _ => None,
    });
    let result = terminals.next().expect("one terminal");
    assert!(terminals.next().is_none());
    result
}

#[test]
fn owned_first_failures_are_typed_and_filtered_rows_cannot_authorize_partial() {
    for filtered in [false, true] {
        for failure in 0..3 {
            let (mut e, _) = ready();
            actions(&mut e);
            ask(&mut e, fills(Some("BTCUSDC"), None, 100));
            let mut r = request(&mut e);
            if filtered {
                let outside = FILL.replace("PERP_BTC_USDC", "PERP_ETH_USDC");
                reply(&mut e, &r, 200, &page(&outside, 2, 1));
                r = request(&mut e);
            }
            match failure {
                0 => reply(&mut e, &r, 503, r#"{"success":false,"message":"busy"}"#),
                1 => reply(&mut e, &r, 200, r#"{"success":true,"data":{"rows":{}}}"#),
                _ => e.handle(
                    2500.0,
                    Event::HttpFailed {
                        request: r.id,
                        error: "offline",
                    },
                ),
            }
            let error = owned_terminal(&mut e, 77, false).unwrap_err();
            // A lost request is typed Network; a delivered failure stays Internal.
            let kind = if failure == 2 {
                safe_cex_engine::ErrorKind::Network
            } else {
                safe_cex_engine::ErrorKind::Internal
            };
            assert_eq!(error.kind, Some(kind));
        }
    }
}

#[test]
fn owned_later_failures_return_only_accepted_pages_with_one_warning() {
    for failure in 0..4 {
        let (mut e, _) = ready();
        actions(&mut e);
        ask(&mut e, fills(None, None, 100));
        let r = request(&mut e);
        reply(&mut e, &r, 200, &page(FILL, 3, 1));
        let r = request(&mut e);
        match failure {
            0 => reply(&mut e, &r, 503, r#"{"success":false,"message":"busy"}"#),
            1 => reply(&mut e, &r, 200, r#"{"success":true,"data":{"rows":{}}}"#),
            2 => e.handle(
                2500.0,
                Event::HttpFailed {
                    request: r.id,
                    error: "offline",
                },
            ),
            _ => {
                let fresh = FILL.replace("9007199254740993", "9007199254740994");
                let invalid = FILL.replace(
                    "\"executed_quantity\":\"0.25\"",
                    "\"executed_quantity\":\"NaN\"",
                );
                reply(&mut e, &r, 200, &page(&format!("{fresh},{invalid}"), 3, 2));
            }
        }
        let rows: Vec<FillResult> =
            serde_json::from_str(&owned_terminal(&mut e, 77, true).unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "9007199254740993");
    }
}

#[test]
fn owned_regular_orders_survive_algo_read_failure() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = request(&mut e);
    reply(&mut e, &r, 200, &page(HIST_ORDER, 1, 1));
    let r = request(&mut e);
    assert!(r.url.contains("/algo/orders"));
    reply(&mut e, &r, 503, r#"{"success":false,"message":"busy"}"#);
    let rows: Vec<OrderResult> =
        serde_json::from_str(&owned_terminal(&mut e, 77, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "9007199254740993");
}

#[test]
fn owned_native_proof_refusals_remain_strict_after_accepted_output() {
    for inconsistent in [false, true] {
        let (mut e, _) = ready();
        actions(&mut e);
        ask(&mut e, fills(None, None, 100));
        let r = request(&mut e);
        reply(&mut e, &r, 200, &page(FILL, 3, 1));
        let r = request(&mut e);
        reply(
            &mut e,
            &r,
            200,
            &page(FILL, 3, if inconsistent { 9 } else { 2 }),
        );
        assert!(owned_terminal(&mut e, 77, false).is_err());
    }
}

#[test]
fn owned_fifty_page_budget_spans_regular_and_algo_but_terminal_fiftieth_succeeds() {
    for needs_more in [false, true] {
        let (mut e, _) = ready();
        actions(&mut e);
        ask(
            &mut e,
            Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: Some(1000),
            },
        );
        for number in 1..=49 {
            let r = request(&mut e);
            assert!(!r.url.contains("/algo/"));
            let row = HIST_ORDER.replace("9007199254740993", &number.to_string());
            reply(&mut e, &r, 200, &page(&row, 49, number));
        }
        let r = request(&mut e);
        assert!(r.url.contains("/algo/orders") && r.url.contains("page=1"));
        let row = HIST_ORDER
            .replace("order_id", "algo_order_id")
            .replace("\"type\":\"LIMIT\"", "\"algo_type\":\"LIMIT\"")
            .replace("9007199254740993", "999");
        reply(
            &mut e,
            &r,
            200,
            &page(&row, if needs_more { 2 } else { 1 }, 1),
        );
        let terminal = owned_terminal(&mut e, 77, false);
        if needs_more {
            assert!(terminal.unwrap_err().message.contains("pagination"));
        } else {
            let rows: Vec<OrderResult> = serde_json::from_str(&terminal.unwrap()).unwrap();
            assert_eq!(rows.len(), 50);
        }
    }
}

#[test]
fn owned_stage_identity_and_limits_keep_algo_rows_in_final_sort() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let r = request(&mut e);
    // A native total beyond the result limit must not add regular-page I/O.
    reply(&mut e, &r, 200, &page(HIST_ORDER, 2, 1));
    let r = request(&mut e);
    assert!(r.url.contains("/algo/orders") && r.url.contains("page=1"));
    let algo = HIST_ORDER
        .replace("order_id", "algo_order_id")
        .replace("\"type\":\"LIMIT\"", "\"algo_type\":\"LIMIT\"")
        .replace("\"created_time\":2000", "\"created_time\":2100");
    // Same native ID in another endpoint is a separate identity.
    reply(&mut e, &r, 200, &page(&algo, 1, 1));
    let rows: Vec<OrderResult> =
        serde_json::from_str(&owned_terminal(&mut e, 77, false).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].created_time, 2100.0);
}

#[test]
fn owned_concurrent_reads_do_not_share_rows_or_markers() {
    let (mut e, _) = ready();
    actions(&mut e);
    ask(&mut e, fills(None, None, 100));
    let first = request(&mut e);
    ask_with_id(&mut e, 78, fills(None, None, 100));
    let other = request(&mut e);
    reply(&mut e, &first, 200, &page(FILL, 2, 1));
    let next = request(&mut e);
    reply(&mut e, &other, 200, &page("", 0, 1));
    assert_eq!(owned_terminal(&mut e, 78, false).unwrap(), "[]");
    reply(&mut e, &next, 503, r#"{"success":false,"message":"busy"}"#);
    let rows: Vec<FillResult> =
        serde_json::from_str(&owned_terminal(&mut e, 77, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn owned_signing_refusal_after_accepted_page_is_strict() {
    let (mut e, _, refuse) = ready_refusable();
    actions(&mut e);
    ask(&mut e, fills(None, None, 100));
    let r = request(&mut e);
    // The provider refuses to sign the continuation. That must never turn
    // accepted output into a partial signing success.
    refuse.store(true, std::sync::atomic::Ordering::SeqCst);
    reply(&mut e, &r, 200, &page(FILL, 2, 1));
    let error = owned_terminal(&mut e, 77, false).unwrap_err();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
}
