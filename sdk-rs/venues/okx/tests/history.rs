#![allow(dead_code)]
mod support;

use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query};
use safe_cex_venue_okx::Okx;
use serde::Deserialize;

// OKX v5 documented schemas: fills, orders-history, account bills and market trades.
// https://www.okx.com/docs-v5/en/
fn query(e: &mut Engine<Okx>, query: Query) -> Action {
    e.handle(
        1_710_000_000_000.0,
        Event::Command(Command::Query {
            command: 991,
            query,
        }),
    );
    prepared(e, 1_710_000_000_000.0)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap()
}
fn result(e: &mut Engine<Okx>) -> String {
    actions(e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 991,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("completed query")
}
fn owned_incomplete(out: &[Action]) {
    let (index, event) = out
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some((i, event))
            }
            _ => None,
        })
        .expect("owned semantic history warning");
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(
        event.fields,
        vec![("command", safe_cex_engine::Value::Num(991.0))]
    );
    let done = out
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 991, .. })))
        .unwrap();
    assert!(index < done);
}
fn url(request: &Action) -> &str {
    let Action::Http(request) = request else {
        panic!("expected HTTP")
    };
    &request.url
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: Some("123".into()),
        from_ms: Some(1_709_000_000_000.0),
        to_ms: Some(1_711_000_000_000.0),
        limit: Some(5),
    }
}
const FILL: &str = r#"{"instId":"BTC-USDT-SWAP","tradeId":"200","billId":"900","ordId":"123","side":"buy","fillPx":"40000","fillSz":"2","fillTime":"1710000000123","fee":"-0.4","feeCcy":"USDT","fillPnl":"3","execType":"M","posSide":"long"}"#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    commission: f64,
    commission_asset: String,
    realized_pnl: f64,
    maker: bool,
    position_side: String,
}
#[test]
fn fills_fallback_deduplicates_trade_identity_and_converts_contracts() {
    let mut e = trading_engine();
    let a = query(&mut e, fills());
    assert!(url(&a).contains("/trade/fills?instType=SWAP&instId=BTC-USDT-SWAP&ordId=123&begin=1709000000000&end=1711000000000&limit=5"));
    reply(&mut e, &a, &envelope(FILL), 10.0);
    let a = take_http(&mut prepared(&mut e, 10.0));
    assert!(url(&a).contains("/trade/fills-history?"));
    let other = FILL.replace("\"200\"", "\"201\"");
    // The fee and fill bills of trade 200 remain a single execution.
    reply(&mut e, &a, &envelope(&format!("{FILL},{other}")), 11.0);
    let rows: Vec<Execution> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "200");
    assert_eq!(rows[1].id, "201");
    assert_eq!(rows[0].timestamp, 1_710_000_000.0);
    assert_eq!(rows[0].qty, 0.02);
    assert_eq!(rows[0].commission, 0.4);
    assert_eq!(rows[0].realized_pnl, 3.0);
    assert_eq!(rows[0].commission_asset, "USDT");
    assert_eq!(rows[0].symbol, "BTCUSDT");
    assert_eq!(rows[0].order_id, "123");
    assert_eq!(rows[0].position_side, "long");
    assert!(rows[0].maker);
}

#[test]
fn fills_bounds_and_order_filter_apply_to_history_response() {
    let mut e = trading_engine();
    let a = query(&mut e, fills());
    reply(&mut e, &a, &envelope(""), 10.0);
    let a = take_http(&mut prepared(&mut e, 10.0));
    let wrong_order = FILL.replace("\"123\"", "\"999\"");
    let old = FILL.replace("1710000000123", "1708000000000");
    reply(&mut e, &a, &envelope(&format!("{wrong_order},{old}")), 11.0);
    assert_eq!(result(&mut e), "[]");
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    position_side: String,
    created_time: f64,
    price: Option<f64>,
    average_price: Option<f64>,
}
fn orders(limit: Option<u32>) -> Query {
    Query::OrderHistory {
        symbol: Some("BTCUSDT".into()),
        from_ms: None,
        to_ms: None,
        limit,
    }
}
fn order(id: &str) -> String {
    format!(
        r#"{{"instId":"BTC-USDT-SWAP","ordId":"{id}","side":"sell","ordType":"post_only","state":"mmp_canceled","sz":"3","accFillSz":"1.5","px":"41000","avgPx":"","reduceOnly":"true","posSide":"net","cTime":"1710000000123","uTime":"1710000000200"}}"#
    )
}
#[test]
fn history_preserves_canceled_orders_and_native_contract_conversion() {
    let mut e = trading_engine();
    let a = query(&mut e, orders(None));
    assert!(url(&a).contains("/trade/orders-history?instType=SWAP&instId=BTC-USDT-SWAP&limit=100"));
    reply(&mut e, &a, &envelope(&order("999")), 10.0);
    let rows: Vec<HistoricalOrder> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "canceled");
    assert_eq!(rows[0].kind, "limit");
    assert_eq!(rows[0].amount, 0.03);
    assert_eq!(rows[0].filled, 0.015);
    assert_eq!(rows[0].created_time, 1_710_000_000_123.0);
    assert_eq!(rows[0].position_side, "both");
    assert_eq!(rows[0].price, Some(41000.0));
    assert_eq!(rows[0].average_price, None);
    assert!(rows[0].reduce_only);
}

fn next_page(e: &mut Engine<Okx>) -> Action {
    let out = actions(e);
    let (timer, time) = out
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("paced continuation");
    e.handle(time, Event::TimerFired { timer });
    prepared(e, time)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap()
}
#[test]
fn history_cursor_uses_smallest_exact_id_even_when_response_order_changes() {
    let mut e = trading_engine();
    let a = query(&mut e, orders(None));
    let body = (0..100u64)
        .map(|i| order(&(9_007_199_254_740_992u64 + i).to_string()))
        .collect::<Vec<_>>()
        .join(",");
    reply(&mut e, &a, &envelope(&body), 10.0);
    let a = next_page(&mut e);
    assert!(url(&a).ends_with("&after=9007199254740992"));
    reply(&mut e, &a, &envelope(&order("9007199254740991")), 120.0);
    let rows: Vec<HistoricalOrder> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows.len(), 101);
}

fn income(from: Option<f64>, limit: Option<u32>) -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: from,
        to_ms: None,
        limit,
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    timestamp: f64,
    tran_id: String,
    category: String,
}
fn bill(id: &str, fee: &str, pnl: &str, change: &str) -> String {
    format!(
        r#"{{"instId":"BTC-USDT-SWAP","billId":"{id}","tradeId":"200","type":"2","balChg":"{change}","fee":"{fee}","pnl":"{pnl}","ccy":"USDT","ts":"1710000000123"}}"#
    )
}
#[test]
fn ledger_decomposes_fee_pnl_residual_without_losing_balance_identity() {
    let mut e = trading_engine();
    let a = query(&mut e, income(Some(1_700_000_000_000.0), None));
    assert!(url(&a).contains("/account/bills-archive?instType=SWAP&begin=1700000000000&limit=100"));
    reply(
        &mut e,
        &a,
        &envelope(&bill("500", "-0.3", "10", "11.7")),
        10.0,
    );
    let rows: Vec<Income> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows.len(), 3);
    assert!((rows.iter().map(|r| r.income).sum::<f64>() - 11.7).abs() < 1e-10);
    assert_eq!(rows[0].tran_id, "500:pnl");
    assert_eq!(rows[1].tran_id, "500:fee");
    assert_eq!(rows[1].category, "commission");
    assert_eq!(rows[1].income_type, "COMMISSION");
    assert_eq!(rows[2].tran_id, "500:adj");
    assert_eq!(rows[0].timestamp, 1_710_000_000_123.0);
}
#[test]
fn ledger_relative_epsilon_suppresses_phantom_adjustment_and_preserves_unknown_market_value() {
    let mut e = trading_engine();
    let a = query(&mut e, income(None, None));
    assert!(url(&a).contains("/account/bills?"));
    let row = bill("500", "-0.1", "250000", "249999.90000000002")
        .replace("BTC-USDT-SWAP", "OLD-USDT-SWAP");
    reply(&mut e, &a, &envelope(&row), 10.0);
    let rows: Vec<Income> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].symbol, "OLD-USDT-SWAP");
}
#[test]
fn ledger_limit_counts_bills_rather_than_decomposed_records() {
    let mut e = trading_engine();
    let a = query(&mut e, income(None, Some(101)));
    let body = (100..200)
        .map(|id| bill(&id.to_string(), "-1", "2", "1"))
        .collect::<Vec<_>>()
        .join(",");
    reply(&mut e, &a, &envelope(&body), 10.0);
    let a = next_page(&mut e);
    assert!(url(&a).ends_with("&after=100"));
    reply(&mut e, &a, &envelope(&bill("99", "-1", "2", "1")), 120.0);
    let rows: Vec<Income> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows.len(), 202);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: String,
}
#[test]
fn public_trades_are_keyless_and_convert_contracts_to_base_and_quote() {
    let (mut e, _, _) = public(Options::default());
    let a = query(
        &mut e,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(1000),
        },
    );
    assert!(url(&a).ends_with("/market/trades?instId=BTC-USDT-SWAP&limit=500"));
    let Action::Http(request) = &a else { panic!() };
    assert!(
        !request
            .headers
            .iter()
            .any(|(k, _)| k.starts_with("OK-ACCESS"))
    );
    reply(
        &mut e,
        &a,
        &envelope(
            r#"{"instId":"BTC-USDT-SWAP","tradeId":"44","px":"40000","sz":"2","side":"sell","ts":"1710000000123"}"#,
        ),
        10.0,
    );
    let rows: Vec<Trade> = serde_json::from_str(&result(&mut e)).unwrap();
    assert_eq!(rows[0].qty, 0.02);
    assert_eq!(rows[0].quote_qty, 800.0);
    assert_eq!(rows[0].timestamp, 1_710_000_000_123.0);
    assert_eq!(rows[0].side, "sell");
}

#[test]
fn private_queries_are_empty_without_credentials() {
    let (mut e, _, _) = public(Options::default());
    e.handle(
        5.0,
        Event::Command(Command::Query {
            command: 991,
            query: fills(),
        }),
    );
    assert_eq!(result(&mut e), "[]");
}
#[test]
fn failed_or_malformed_later_page_does_not_publish_partial_history() {
    for body in [
        r#"{"code":"51000","msg":"bad request","data":[]}"#.to_owned(),
        envelope("42"),
    ] {
        let mut e = trading_engine();
        let a = query(&mut e, orders(None));
        let page = (100..200)
            .map(|id| order(&id.to_string()))
            .collect::<Vec<_>>()
            .join(",");
        reply(&mut e, &a, &envelope(&page), 10.0);
        let a = next_page(&mut e);
        reply(&mut e, &a, &body, 120.0);
        assert_eq!(result(&mut e), "[]");
    }
}

#[test]
fn full_recent_fills_page_does_not_trigger_archive_and_limit_is_clamped() {
    let mut e = trading_engine();
    let a = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(0),
        },
    );
    assert!(url(&a).ends_with("limit=1"));
    reply(&mut e, &a, &envelope(FILL), 10.0);
    let out = prepared(&mut e, 10.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: s, .. }) if s.contains("\"qty\":0.02"))));
}

#[test]
fn unknown_market_does_not_issue_a_history_request() {
    let mut e = trading_engine();
    e.handle(
        10.0,
        Event::Command(Command::Query {
            command: 991,
            query: Query::OrderHistory {
                symbol: Some("UNKNOWNUSDT".into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let out = prepared(&mut e, 10.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        out.iter().any(
            |a| matches!(a, Action::Notice(Notice::CommandDone { result: s, .. }) if s == "[]")
        )
    );
}

#[test]
fn repeating_cursor_stops_with_warning_without_refetching_forever() {
    let mut e = trading_engine();
    e.set_log_level(safe_cex_engine::Level::Error);
    let a = query(&mut e, orders(None));
    let page = (100..200)
        .map(|id| order(&id.to_string()))
        .collect::<Vec<_>>()
        .join(",");
    reply(&mut e, &a, &envelope(&page), 10.0);
    let a = next_page(&mut e);
    reply(&mut e, &a, &envelope(&page), 120.0);
    let out = prepared(&mut e, 120.0);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: s, .. }) if serde_json::from_str::<Vec<HistoricalOrder>>(s).unwrap().len() == 100)));
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message.contains("cursor did not advance"))));
    owned_incomplete(&out);
}

#[test]
fn history_page_cap_keeps_rows_and_owns_its_warning_before_done() {
    let mut e = trading_engine();
    e.set_log_level(safe_cex_engine::Level::Error);
    let mut request = query(&mut e, orders(None));
    for page in 0..50 {
        let rows = (0..100)
            .map(|i| order(&(10_000 - page * 100 - i).to_string()))
            .collect::<Vec<_>>()
            .join(",");
        reply(
            &mut e,
            &request,
            &envelope(&rows),
            page as f64 * 100.0 + 10.0,
        );
        if page < 49 {
            request = next_page(&mut e);
        }
    }
    let out = actions(&mut e);
    owned_incomplete(&out);
    let result = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 991,
                result,
            }) => Some(result),
            _ => None,
        })
        .unwrap();
    let rows: Vec<HistoricalOrder> = serde_json::from_str(result).unwrap();
    assert_eq!(rows.len(), 5000);
    assert!(
        rows.iter()
            .all(|row| row.amount == 0.03 && row.filled == 0.015)
    );
    assert!(out.iter().any(
        |a| matches!(a, Action::Notice(Notice::Event(e)) if e.message.contains("50-page cap"))
    ));
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::SetTimer { .. } | Action::Notice(Notice::CommandFailed { .. })
    )));
}

#[test]
fn fills_preserve_ts_seconds_bounds_normalization_and_floor_milliseconds() {
    // RustEngineExchange forwards ExecutionsOptions.from/to unchanged. TS accepts
    // Unix seconds here even though the engine query fields are named *_ms.
    for (from, to) in [
        (1_709_000_000.000_9, 1_711_000_000.000_9),
        (1_709_000_000_000.9, 1_711_000_000_000.9),
    ] {
        let mut e = trading_engine();
        let a = query(
            &mut e,
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(from),
                to_ms: Some(to),
                limit: Some(1),
            },
        );
        assert!(url(&a).contains("begin=1709000000000&end=1711000000000&limit=1"));
        reply(&mut e, &a, &envelope(FILL), 10.0);
        let rows: Vec<Execution> = serde_json::from_str(&result(&mut e)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].timestamp, 1_710_000_000.0);
    }
}

#[test]
fn block_trade_fills_omit_missing_order_id_and_invalid_commission_like_ts() {
    // OKX documents empty ordId for block trading. TS omitUndefined removes
    // orderId, and a nonnumeric fee is omitted instead of becoming a zero fee.
    let mut e = trading_engine();
    let a = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let row = FILL
        .replace("\"ordId\":\"123\"", "\"ordId\":\"\"")
        .replace("\"fee\":\"-0.4\"", "\"fee\":\"invalid\"");
    reply(&mut e, &a, &envelope(&row), 10.0);
    let json = result(&mut e);
    assert!(json.contains("\"id\":\"200\""));
    assert!(!json.contains("\"orderId\":"));
    assert!(!json.contains("\"commission\":"));
}

#[test]
fn fills_apply_bounds_to_native_milliseconds_before_output_seconds_are_truncated() {
    for (from, to) in [
        (1_710_000_001_500.0, 1_710_000_002_500.0),
        (1_710_000_001.5, 1_710_000_002.5),
    ] {
        let mut e = trading_engine();
        let a = query(
            &mut e,
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(from),
                to_ms: Some(to),
                limit: Some(3),
            },
        );
        assert!(url(&a).contains("begin=1710000001500&end=1710000002500&limit=3"));
        let valid = FILL.replace("1710000000123", "1710000001600");
        let before = FILL
            .replace("1710000000123", "1710000001400")
            .replace("\"200\"", "\"201\"");
        let after = FILL
            .replace("1710000000123", "1710000002600")
            .replace("\"200\"", "\"202\"");
        reply(
            &mut e,
            &a,
            &envelope(&format!("{valid},{before},{after}")),
            10.0,
        );
        let rows: Vec<Execution> = serde_json::from_str(&result(&mut e)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "200");
        assert_eq!(rows[0].timestamp, 1_710_000_001.0);
    }
}
