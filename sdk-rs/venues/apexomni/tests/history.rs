mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use serde::Deserialize;
use support::*;
fn seeded_socket() -> (TestEngine, safe_cex_engine::SocketId) {
    let mut e = Engine::new(ApexOmni::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        omni_key_seed: Some("11".repeat(65)),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let socket = e
        .drain_actions()
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
fn result(e: &mut TestEngine) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { command: 7, result }) = a {
                Some(result)
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
    let r = requests(&mut e).remove(0);
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
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
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
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        r#"{"code":0,"body":{"result":{"list":[{"s":"PERP_BTC_USDT","S":"S","p":"2","v":"3","T":"2023-11-15T00:13:20+02:00","i":123}]}}}"#,
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!((rows[0].timestamp, rows[0].qty), (1700000000.0, 3.0));
    assert_eq!(rows[0].side, "sell");
    assert_eq!(rows[0].order_id.as_deref(), Some("123"));
}
#[test]
fn empty_native_symbol_tries_display_symbol_then_history_orders() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 30.0, r#"{"code":0,"data":{"fills":[]}}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("symbol=BTCUSDT"));
    response(&mut e, &r, 40.0, r#"{"code":0,"data":[]}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("/v3/history-orders?"));
    assert!(r.url.contains("orderType=HISTORY"));
    assert!(r.url.contains("symbol=BTC-USDT"));
    response(
        &mut e,
        &r,
        50.0,
        r#"{"code":0,"data":{"orders":[{"id":"order","symbol":"BTCUSDT","side":"LONG","cumSuccessFillSize":"2","cumSuccessFillValue":"210","size":"10","createdAt":1700000000000}]}}"#,
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!((rows[0].price, rows[0].qty), (105.0, 2.0));
}
#[test]
fn primary_nonempty_rows_are_authoritative_and_filter_wrong_symbol_side_and_order() {
    let mut e = seeded();
    query(&mut e, Some("order1"), None, None, None);
    let r = requests(&mut e).remove(0);
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
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn native_execution_identity_keeps_same_second_same_quantity_distinct_fills() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e).remove(0);
    let second = FILL.replace("trade1", "trade2");
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{FILL},{FILL},{second}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
}
#[test]
fn rows_without_execution_identity_deduplicate_by_order_time_and_economics() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e).remove(0);
    let fill = FILL.replace("\"tradeId\":\"trade1\",", "");
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{fill},{fill}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn malformed_explicit_price_never_uses_aggregate_derived_fallback() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        r#"{"code":0,"data":[{"side":"BUY","fillPrice":"bad","cumSuccessFillSize":"2","cumSuccessFillValue":"200","createdAt":1700000000000}]}"#,
    );
    assert_eq!(result(&mut e).unwrap(), "[]");
}
#[test]
fn malformed_fee_is_absent_and_zero_fee_remains_zero() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        30.0,
        &format!(r#"{{"code":0,"data":[{}]}}"#, FILL.replace("-0.2", "bad")),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
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
    let r = requests(&mut e).remove(0);
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
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
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
        assert_eq!(result(&mut e).unwrap(), "[]");
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
    assert!(result(&mut e).unwrap_err().contains("definitive"));
}

#[test]
fn exhausted_rest_failures_use_filtered_ws_cache_without_merging_authoritative_rest() {
    let (mut e, socket) = seeded_socket();
    ws(
        &mut e,
        socket,
        15.0,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"fills":[{"id":"cached","orderId":"cached-order","symbol":"BTC-USDT","side":"BUY","size":"1","price":"102","fee":"-0.02","createdAt":1700000000123}]}}"#,
    );
    e.drain_actions().for_each(drop);
    query(&mut e, None, None, None, None);
    for i in 0..4 {
        let r = requests(&mut e).remove(0);
        response(
            &mut e,
            &r,
            30.0 + i as f64,
            r#"{"code":40001,"msg":"temporarily unavailable"}"#,
        );
    }
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id.as_deref(), Some("cached-order"));
    query(&mut e, None, None, None, None);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        40.0,
        &format!(r#"{{"code":0,"data":[{FILL}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id.as_deref(), Some("order1"));
}

#[test]
fn anonymous_distinct_subsecond_executions_remain_distinct_without_inventing_ids() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let request = requests(&mut e).remove(0);
    let first = FILL.replace("\"tradeId\":\"trade1\",", "");
    let second = first.replace("1700000000123", "1700000000456");
    response(
        &mut e,
        &request,
        30.0,
        &format!(r#"{{"code":0,"data":[{first},{second},{first}]}}"#),
    );
    let result = result(&mut e).unwrap();
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
    e.drain_actions().for_each(drop);
    query(&mut e, None, None, None, None);
    for offset in 0..4 {
        let request = requests(&mut e).remove(0);
        response(
            &mut e,
            &request,
            30.0 + offset as f64,
            r#"{"code":0,"data":[]}"#,
        );
    }
    let result = result(&mut e).unwrap();
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
    e.drain_actions().for_each(drop);
    query(&mut e, None, None, None, Some(1));
    for offset in 0..4 {
        let request = requests(&mut e).remove(0);
        response(
            &mut e,
            &request,
            30.0 + offset as f64,
            r#"{"code":0,"data":[]}"#,
        );
    }
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id.as_deref(), Some("second-order"));
    assert_eq!(rows[0].qty, 2.0);
}

#[test]
fn duplicate_rest_execution_preserves_first_seen_order_for_equal_second_timestamps() {
    let mut e = seeded();
    query(&mut e, None, None, None, None);
    let request = requests(&mut e).remove(0);
    let second = FILL
        .replace("trade1", "trade2")
        .replace("\"fillSize\":\"2\"", "\"fillSize\":\"3\"");
    response(
        &mut e,
        &request,
        30.0,
        &format!(r#"{{"code":0,"data":[{FILL},{second},{FILL}]}}"#),
    );
    let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
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
        let request = requests(&mut e).remove(0);
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
        let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
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
        e.drain_actions().for_each(drop);
        query(
            &mut e,
            None,
            Some(1700000000100.0),
            Some(1700000000456.0),
            Some(1),
        );
        for offset in 0..4 {
            let request = requests(&mut e).remove(0);
            response(
                &mut e,
                &request,
                30.0 + offset as f64,
                r#"{"code":0,"data":[]}"#,
            );
        }
        let rows: Vec<Row> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].order_id.as_deref(), Some("order-456"));
        assert_eq!(rows[0].timestamp, 1700000000.0);
    }
}
