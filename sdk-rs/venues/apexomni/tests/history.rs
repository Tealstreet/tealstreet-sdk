mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use safe_cex_venue_apexomni::Options;
use serde::Deserialize;
use support::*;
fn seeded_socket() -> (TestEngine, safe_cex_engine::SocketId) {
    let mut e = private_engine(
        Options {
            ..Default::default()
        },
        &Material {
            key: "key",
            signing: "secret",
            passphrase: Some("pass"),
            seed: Some(("11".repeat(65)).to_string()),
        },
    );
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let socket = e.prepared(10.0)
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    (e, socket)
}
fn seeded() -> TestEngine {
    seeded_socket().0
}
fn query(
    e: &mut TestEngine,
    order_id: Option<&str>,
    from: Option<f64>,
    to: Option<f64>,
    limit: Option<u32>,
) {
    e.handle(
        20.0,
        Event::Command(Command::Query {
            command: 7,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: order_id.map(str::to_owned),
                from_ms: from,
                to_ms: to,
                limit,
            },
        }),
    );
}
fn result(e: &mut TestEngine, now: f64) -> Result<String, String> {
    e.prepared(now)
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { command: 7, result }) = a {
                Some(Ok(result))
            } else if let Action::Notice(Notice::CommandFailed { command: 7, error }) = a {
                Some(Err(error.legacy_text()))
            } else {
                None
            }
        })
        .expect("query completed")
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    timestamp: f64,
    symbol: String,
    order_id: Option<String>,
    price: f64,
    qty: f64,
    side: String,
    commission: Option<f64>,
    commission_asset: String,
}
const FILL: &str = r#"{"tradeId":"trade1","symbol":"BTC-USDT","side":"BUY","fillPrice":"100","fillSize":"2","createdAt":1700000000123,"orderId":"order1","fee":"-0.2","token":"USDT"}"#;
#[test]
fn native_fills_are_signed_without_token_or_page_zero_and_exact_bounds() {
    let mut e = seeded();
    query(
        &mut e,
        None,
        Some(1699999999000.0),
        Some(1700000001000.0),
        Some(1000),
    );
    let r = requests(&mut e, 20.0).remove(0);
    assert!(r.url.contains("/v3/fills?"));
    assert!(r.url.contains("symbol=BTC-USDT"));
    assert!(r.url.contains("beginTimeInclusive=1699999999000"));
    assert!(r.url.contains("endTimeExclusive=1700000001001"));
    assert!(r.url.contains("limit=100"));
    assert!(!r.url.contains("token="));
    assert!(!r.url.contains("page="));
    assert!(r.headers.iter().any(|(k, _)| *k == "APEX-SIGNATURE"));
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":{{"fills":[{FILL}]}}}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    let row = &rows[0];
    assert_eq!(
        (row.timestamp, row.price, row.qty),
        (1700000000.0, 100.0, 2.0)
    );
    assert_eq!(row.symbol, "BTCUSDT");
    assert_eq!(row.order_id.as_deref(), Some("order1"));
    assert_eq!(row.side, "buy");
    assert_eq!(row.commission, Some(0.2));
    assert_eq!(row.commission_asset, "USDT");
}
#[test]
fn compact_fills_and_iso_timestamps_parse_in_proxy_body() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        r#"{"code":0,"body":{"result":{"list":[{"s":"PERP_BTC_USDT","S":"S","p":"2","v":"3","T":"2023-11-15T00:13:20+02:00","i":123}]}}}"#,
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!((rows[0].timestamp, rows[0].qty), (1700000000.0, 3.0));
    assert_eq!(rows[0].side, "sell");
    assert_eq!(rows[0].order_id.as_deref(), Some("123"));
}
#[test]
fn empty_native_symbol_tries_display_symbol_then_history_orders() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    response(&mut e, &r, 30.0, r#"{"code":0,"data":{"fills":[]}}"#);
    let r = requests(&mut e, 30.0).remove(0);
    assert!(r.url.contains("symbol=BTCUSDT"));
    response(&mut e, &r, 40.0, r#"{"code":0,"data":[]}"#);
    let r = requests(&mut e, 40.0).remove(0);
    assert!(r.url.contains("/v3/history-orders?"));
    assert!(r.url.contains("orderType=HISTORY"));
    assert!(r.url.contains("symbol=BTC-USDT"));
    response(
        &mut e,
        &r,
        50.0,
        r#"{"code":0,"data":{"orders":[{"id":"order","symbol":"BTCUSDT","side":"LONG","cumSuccessFillSize":"2","cumSuccessFillValue":"210","size":"10","createdAt":1700000000000}]}}"#,
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 50.0).unwrap()).unwrap();
    assert_eq!((rows[0].price, rows[0].qty), (105.0, 2.0));
}
#[test]
fn primary_nonempty_rows_are_authoritative_and_filter_wrong_symbol_side_and_order() {
    let mut e = seeded();
    query(&mut e, Some("order1"), None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        &format!(
            r#"{{"code":0,"data":[{}, {}, {}, {FILL}]}}"#,
            FILL.replace("BTC-USDT", "ETH-USDT"),
            FILL.replace("BUY", "unknown"),
            FILL.replace("order1", "other")
        ),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn native_execution_identity_keeps_same_second_same_quantity_distinct_fills() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    let second = FILL.replace("trade1", "trade2");
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{FILL},{FILL},{second}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
}
#[test]
fn rows_without_execution_identity_deduplicate_by_order_time_and_economics() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    let fill = FILL.replace("\"tradeId\":\"trade1\",", "");
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{fill},{fill}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn malformed_explicit_price_never_uses_aggregate_derived_fallback() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        r#"{"code":0,"data":[{"side":"BUY","fillPrice":"bad","cumSuccessFillSize":"2","cumSuccessFillValue":"200","createdAt":1700000000000}]}"#,
    );
    assert_eq!(result(&mut e, 30.0).unwrap(), "[]");
}
#[test]
fn malformed_fee_is_absent_and_zero_fee_remains_zero() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 20.0).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{}]}}"#, FILL.replace("-0.2", "bad")),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!(rows[0].commission, None);
}
#[test]
fn bounds_filters_and_limit_sorts_latest_first() {
    let mut e = seeded();
    query(
        &mut e,
        None,
        Some(1699999999000.0),
        Some(1700000003000.0),
        Some(1),
    );
    let r = requests(&mut e, 20.0).remove(0);
    let newer = FILL
        .replace("trade1", "trade2")
        .replace("1700000000123", "1700000002000");
    let outside = FILL
        .replace("trade1", "trade3")
        .replace("1700000000123", "1700000004000");
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{FILL},{newer},{outside}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].timestamp, 1700000002.0);
}
#[test]
fn unsupported_income_and_orders_match_base_empty_and_client_lookup_is_not_definitive() {
    let mut e = seeded();
    for query in [
        Query::Income {
            symbol: None,
            kind: None,
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
    ] {
        e.handle(20.0, Event::Command(Command::Query { command: 7, query }));
        assert_eq!(result(&mut e, 20.0).unwrap(), "[]");
    }
    e.handle(
        30.0,
        Event::Command(Command::Query {
            command: 7,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "uncertain".into(),
            },
        }),
    );
    assert!(result(&mut e, 30.0).unwrap_err().contains("definitive"));
}

#[test]
fn exhausted_rest_failures_reject_without_laundering_cache_then_rest_stays_authoritative() {
    let (mut e, socket) = seeded_socket();
    ws(
        &mut e,
        socket,
        15.0,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"fills":[{"id":"cached","orderId":"cached-order","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","fee":"-0.02","createdAt":1700000000123}]}}"#,
    );
    e.prepared(15.0).for_each(drop);
    query(&mut e, None, None, None, None);
    for i in 0..4 {
        let r = owned_request(&mut e, 20.0);
        response(
            &mut e,
            &r,
            30.0 + i as f64,
            r#"{"code":40001,"msg":"temporarily unavailable"}"#,
        );
    }
    assert!(
        owned_error(&mut e, 30.0 + i as f64)
            .message
            .contains("temporarily unavailable")
    );
    query(&mut e, None, None, None, None);
    let r = requests(&mut e, 30.0 + i as f64).remove(0);
    response(
        &mut e,
        &r,
        40.0,
        &format!(r#"{{"code":0,"data":[{FILL}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 40.0).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id.as_deref(), Some("order1"));
}

#[test]
fn anonymous_distinct_subsecond_executions_remain_distinct_without_inventing_ids() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let request = requests(&mut e, 20.0).remove(0);
    let first = FILL.replace("\"tradeId\":\"trade1\",", "");
    let second = first.replace("1700000000123", "1700000000456");
    response(
        &mut e,
        &request,
        30.0,
        &format!(r#"{{"code":0,"data":[{first},{second},{first}]}}"#),
    );
    let result = result(&mut e, 30.0).unwrap();
    let rows: Vec<Row> = serde_json::from_str(&result).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.timestamp == 1700000000.0));
    assert!(!result.contains("\"id\""));
}

#[test]
fn anonymous_ws_cache_preserves_raw_execution_identity_across_second_precision_output() {
    let (mut e, socket) = seeded_socket();
    ws(
        &mut e,
        socket,
        15.0,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"fills":[{"orderId":"parent","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","createdAt":1700000000123},{"orderId":"parent","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","createdAt":1700000000456}]}}"#,
    );
    e.prepared(15.0).for_each(drop);
    query(&mut e, None, None, None, None);
    for offset in 0..4 {
        let request = requests(&mut e, 15.0).remove(0);
        response(
            &mut e,
            &request,
            30.0 + offset as f64,
            r#"{"code":0,"data":[]}"#,
        );
    }
    let result = result(&mut e, 30.0 + offset as f64).unwrap();
    let rows: Vec<Row> = serde_json::from_str(&result).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.timestamp == 1700000000.0));
    assert!(!result.contains("\"id\""));
}

#[test]
fn same_second_ws_cache_uses_latest_execution_before_limit() {
    let (mut e, socket) = seeded_socket();
    ws(
        &mut e,
        socket,
        15.0,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"fills":[{"id":"first","orderId":"first-order","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","createdAt":1700000000123},{"id":"second","orderId":"second-order","symbol":"BTC-USDT","side":"BUY","size":"2","price":"102","createdAt":1700000000456}]}}"#,
    );
    e.prepared(15.0).for_each(drop);
    query(&mut e, None, None, None, Some(1));
    for offset in 0..4 {
        let request = requests(&mut e, 15.0).remove(0);
        response(
            &mut e,
            &request,
            30.0 + offset as f64,
            r#"{"code":0,"data":[]}"#,
        );
    }
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0 + offset as f64).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id.as_deref(), Some("second-order"));
    assert_eq!(rows[0].qty, 2.0);
}

#[test]
fn duplicate_rest_execution_preserves_first_seen_order_for_equal_second_timestamps() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let request = requests(&mut e, 20.0).remove(0);
    let second = FILL
        .replace("trade1", "trade2")
        .replace("\"fillSize\":\"2\"", "\"fillSize\":\"3\"");
    response(
        &mut e,
        &request,
        30.0,
        &format!(r#"{{"code":0,"data":[{FILL},{second},{FILL}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
    assert_eq!(rows.iter().map(|r| r.qty).collect::<Vec<_>>(), [2.0, 3.0]);
}

#[test]
fn millisecond_bounds_are_inclusive_and_sort_raw_times_before_truncating_output() {
    for format in ["milliseconds", "seconds", "iso"] {
        let mut e = seeded();
        query(
            &mut e,
            None,
            Some(1700000000100.0),
            Some(1700000000456.0),
            None,
        );
        let request = requests(&mut e, 20.0).remove(0);
        assert!(request.url.contains("endTimeExclusive=1700000000457"));
        let rows = [99, 100, 123, 456, 457].map(|millis| {
            let time = if format == "iso" { format!(r#""2023-11-15T00:13:20.{millis:03}+02:00""#) }
                else if format == "seconds" { format!("1700000000.{millis:03}") }
                else { format!("{}", 1700000000000u64 + millis) };
            format!(r#"{{"tradeId":"trade-{millis}","orderId":"order-{millis}","symbol":"BTC-USDT","side":"BUY","price":"100","size":"1","createdAt":{time}}}"#)
        }).join(",");
        response(
            &mut e,
            &request,
            30.0,
            &format!(r#"{{"code":0,"data":[{rows}]}}"#),
        );
        let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0).unwrap()).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.order_id.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["order-456", "order-123", "order-100"]
        );
        assert!(rows.iter().all(|r| r.timestamp == 1700000000.0));
    }
}

#[test]
fn ws_and_synthetic_cache_filter_and_sort_using_original_milliseconds() {
    for synthetic in [false, true] {
        let (mut e, socket) = seeded_socket();
        let contents = if synthetic {
            let orders = [456, 457, 123].map(|millis| format!(r#"{{"id":"order-{millis}","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"OPEN","size":"1","price":"100","cumSuccessFillSize":"1","updatedAt":{}}}"#, 1700000000000u64 + millis)).join(",");
            format!(r#"{{"orders":[{orders}]}}"#)
        } else {
            let fills = [456, 457, 123].map(|millis| format!(r#"{{"orderId":"order-{millis}","symbol":"BTC-USDT","side":"BUY","size":"1","price":"100","createdAt":{}}}"#, 1700000000000u64 + millis)).join(",");
            format!(r#"{{"fills":[{fills}]}}"#)
        };
        ws(
            &mut e,
            socket,
            15.0,
            &format!(r#"{{"topic":"ws_zk_accounts_v3","type":"delta","contents":{contents}}}"#),
        );
        e.prepared(15.0).for_each(drop);
        query(
            &mut e,
            None,
            Some(1700000000100.0),
            Some(1700000000456.0),
            Some(1),
        );
        for offset in 0..4 {
            let request = requests(&mut e, 15.0).remove(0);
            response(
                &mut e,
                &request,
                30.0 + offset as f64,
                r#"{"code":0,"data":[]}"#,
            );
        }
        let rows: Vec<Row> = serde_json::from_str(&result(&mut e, 30.0 + offset as f64).unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].order_id.as_deref(), Some("order-456"));
        assert_eq!(rows[0].timestamp, 1700000000.0);
    }
}

fn owned_request(e: &mut TestEngine, now: f64) -> safe_cex_engine::HttpRequest {
    let mut http = Vec::new();
    for action in e.prepared(now) {
        match action {
            Action::Http(r) => http.push(r),
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. }) => {
                panic!("early completion")
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE)
            }
            _ => {}
        }
    }
    assert_eq!(http.len(), 1);
    http.pop().unwrap()
}
fn owned_error(e: &mut TestEngine, now: f64) -> safe_cex_engine::VenueError {
    let mut errors = Vec::new();
    for action in e.prepared(now) {
        match action {
            Action::Http(_) => panic!("exhausted aliases continued"),
            Action::Notice(Notice::CommandDone { .. }) => {
                panic!("failed REST became cache/empty success")
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                assert_eq!(command, 7);
                errors.push(error);
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE)
            }
            _ => {}
        }
    }
    assert_eq!(errors.len(), 1);
    let error = errors.pop().unwrap();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    error
}
fn owned_rows(e: &mut TestEngine, expected_command: u32, now: f64) -> Vec<Row> {
    let mut results = Vec::new();
    for action in e.prepared(now) {
        match action {
            Action::Http(_) => panic!("terminal selection continued"),
            Action::Notice(Notice::CommandFailed { .. }) => panic!("valid selection rejected"),
            Action::Notice(Notice::CommandDone { command, result }) => {
                assert_eq!(command, expected_command);
                results.push(serde_json::from_str(&result).unwrap());
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE)
            }
            _ => {}
        }
    }
    assert_eq!(results.len(), 1);
    results.pop().unwrap()
}
fn cached() -> TestEngine {
    let (mut e, socket) = seeded_socket();
    ws(
        &mut e,
        socket,
        15.0,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"fills":[{"id":"cached","orderId":"cached-order","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","fee":"-0.02","createdAt":1700000000123}]}}"#,
    );
    e.prepared(15.0).for_each(drop);
    e
}
fn candidate(r: &safe_cex_engine::HttpRequest, index: usize) {
    assert!(r.url.contains(if index < 2 {
        "/v3/fills?"
    } else {
        "/v3/history-orders?"
    }));
    assert!(r.url.contains(if index.is_multiple_of(2) {
        "symbol=BTC-USDT"
    } else {
        "symbol=BTCUSDT"
    }));
}
#[test]
fn all_native_alias_failures_reject_typed_instead_of_empty_success() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    for i in 0..4 {
        let r = owned_request(&mut e, 20.0);
        candidate(&r, i);
        response(
            &mut e,
            &r,
            30.0 + i as f64,
            r#"{"code":40001,"msg":"native unavailable"}"#,
        );
    }
    assert!(owned_error(&mut e, 30.0 + i as f64).message.contains("native unavailable"));
}
#[test]
fn malformed_and_missing_list_exhaustion_cannot_publish_valid_prefixes_or_cache() {
    for body in [
        r#"{"code":0,"data":null}"#.to_owned(),
        format!(r#"{{"code":0,"data":[{FILL}],"code":{{}}}}"#),
    ] {
        let mut e = cached();
        query(&mut e, None, None, None, None);
        for i in 0..4 {
            let r = owned_request(&mut e, 20.0);
            candidate(&r, i);
            response(&mut e, &r, 30.0 + i as f64, &body);
        }
        owned_error(&mut e, 30.0 + i as f64);
    }
}
#[test]
fn transport_exhaustion_resets_all_sixteen_attempt_prefixes_before_typed_rejection() {
    let mut e = cached();
    query(&mut e, None, None, None, None);
    for attempt in 0..16 {
        let r = owned_request(&mut e, 20.0);
        candidate(&r, attempt / 4);
        e.handle(
            30.0,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        let prefix = format!(r#"{{"code":0,"data":[{FILL},"#);
        e.handle(
            30.0,
            Event::HttpChunk {
                request: r.id,
                data: prefix.as_bytes(),
            },
        );
        for action in e.prepared(30.0) {
            match action {
                Action::Http(_)
                | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. }) => {
                    panic!("prefix escaped")
                }
                Action::Notice(Notice::Event(event)) => {
                    assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE)
                }
                _ => {}
            }
        }
        e.handle(
            30.0,
            Event::HttpFailed {
                request: r.id,
                error: "network unavailable",
            },
        );
    }
    assert!(owned_error(&mut e, 30.0).message.contains("network unavailable"));
}
#[test]
fn validated_empty_or_filtered_success_keeps_cache_selection_amid_failed_alternatives() {
    for success_at in [0, 3] {
        let mut e = cached();
        query(&mut e, None, None, None, None);
        for i in 0..4 {
            let r = owned_request(&mut e, 20.0);
            candidate(&r, i);
            let body = if i == success_at {
                r#"{"code":0,"data":[]}"#
            } else {
                r#"{"code":40001,"msg":"unavailable"}"#
            };
            response(&mut e, &r, 30.0 + i as f64, body);
        }
        let rows = owned_rows(&mut e, 7, 30.0 + i as f64);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].order_id.as_deref(), Some("cached-order"));
    }
    let mut e = cached();
    query(&mut e, None, None, None, None);
    let r = owned_request(&mut e, 20.0);
    response(
        &mut e,
        &r,
        30.0,
        &format!(
            r#"{{"code":0,"data":[{}]}}"#,
            FILL.replace("BTC-USDT", "ETH-USDT")
        ),
    );
    assert_eq!(
        owned_rows(&mut e, 7, 30.0)[0].order_id.as_deref(),
        Some("cached-order")
    );
}
#[test]
fn identical_native_and_display_symbol_has_only_two_alternatives() {
    let mut e = seeded();
    let mut markets = e.store().markets().to_vec();
    for market in &mut markets {
        market.id = market.symbol.clone();
    }
    e.store_mut().set_markets(markets);
    query(&mut e, None, None, None, None);
    for route in ["/v3/fills?", "/v3/history-orders?"] {
        let r = owned_request(&mut e, 20.0);
        assert!(r.url.contains(route));
        response(&mut e, &r, 30.0, r#"{"code":0,"data":[]}"#);
    }
    assert!(owned_rows(&mut e, 7, 30.0).is_empty());
}

#[test]
fn bounded_native_upsert_preserves_eviction_before_later_duplicate_replacement() {
    let mut e = seeded();
    query(&mut e, None, None, None, Some(1));
    let r = owned_request(&mut e, 20.0);
    let newest = FILL.replace("1700000000123", "1700000005123");
    let evicted = FILL
        .replace("trade1", "trade2")
        .replace("1700000000123", "1700000004123");
    let replacement = FILL
        .replace("1700000000123", "1700000003123")
        .replace("\"fillSize\":\"2\"", "\"fillSize\":\"7\"");
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{newest},{evicted},{replacement}]}}"#),
    );
    let rows = owned_rows(&mut e, 7, 30.0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].timestamp, 1700000003.0);
    assert_eq!(rows[0].qty, 7.0);

    // An identity evicted from the bounded native page may reappear later.
    query(&mut e, None, None, None, Some(1));
    let r = owned_request(&mut e, 20.0);
    let reappeared = FILL
        .replace("trade1", "trade2")
        .replace("1700000000123", "1700000006123")
        .replace("\"fillSize\":\"2\"", "\"fillSize\":\"9\"");
    response(
        &mut e,
        &r,
        40.0,
        &format!(r#"{{"code":0,"data":[{newest},{evicted},{replacement},{reappeared}]}}"#),
    );
    let rows = owned_rows(&mut e, 7, 40.0);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].timestamp, rows[0].qty), (1700000006.0, 9.0));
}
#[test]
fn signature_reset_discards_prefix_but_retains_original_history_owner() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let first = owned_request(&mut e, 20.0);
    response(
        &mut e,
        &first,
        30.0,
        &format!(r#"{{"code":20016,"msg":"signature rejected","data":[{FILL}]}}"#),
    );
    let retry = owned_request(&mut e, 30.0);
    assert_eq!(first.url, retry.url);
    let corrected = FILL
        .replace("\"fillPrice\":\"100\"", "\"fillPrice\":\"101\"")
        .replace("\"fillSize\":\"2\"", "\"fillSize\":\"3\"");
    response(
        &mut e,
        &retry,
        40.0,
        &format!(r#"{{"code":0,"data":[{corrected}]}}"#),
    );
    let rows = owned_rows(&mut e, 7, 40.0);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].price, rows[0].qty), (101.0, 3.0));
}
#[test]
fn history_rejection_does_not_erase_background_cache_or_cross_concurrent_owners() {
    let mut e = cached();
    query(&mut e, None, None, None, None);
    let first = owned_request(&mut e, 20.0);
    e.handle(
        21.0,
        Event::Command(Command::Query {
            command: 8,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let second = owned_request(&mut e, 21.0);
    response(
        &mut e,
        &second,
        30.0,
        &format!(r#"{{"code":0,"data":[{FILL}]}}"#),
    );
    assert_eq!(owned_rows(&mut e, 8, 30.0)[0].order_id.as_deref(), Some("order1"));
    response(
        &mut e,
        &first,
        31.0,
        r#"{"code":40001,"msg":"unavailable"}"#,
    );
    for _ in 1..4 {
        let r = owned_request(&mut e, 31.0);
        response(&mut e, &r, 32.0, r#"{"code":40001,"msg":"unavailable"}"#);
    }
    owned_error(&mut e, 32.0);
    query(&mut e, None, None, None, None);
    for _ in 0..4 {
        let r = owned_request(&mut e, 20.0);
        response(&mut e, &r, 40.0, r#"{"code":0,"data":[]}"#);
    }
    assert_eq!(
        owned_rows(&mut e, 7, 40.0)[0].order_id.as_deref(),
        Some("cached-order")
    );
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}
