mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use support::*;
fn ready() -> TestEngine {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e
}
fn query(e: &mut TestEngine, q: Query) -> safe_cex_engine::HttpRequest {
    e.handle(
        1_700_000_000_000.0,
        Event::Command(Command::Query {
            command: 99,
            query: q,
        }),
    );
    requests(e).remove(0)
}
fn result(e: &mut TestEngine) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone {
                command: 99,
                result,
            }) = a
            {
                Some(result)
            } else {
                None
            }
        })
        .unwrap()
        .map_err(|error| error.message)
}
fn fills(limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit,
    }
}
const EXEC: &str = r#"{"id":739113577038255616,"orderId":"739113577038255617","symbol":"BTC_USDT","side":4,"vol":60,"price":60000,"fee":0.072,"feeCurrency":"USDT","timestamp":1700000000123,"profit":3,"isTaker":true}"#;
#[test]
fn executions_keep_exact_ids_units_and_seconds_without_store_writes() {
    let mut e = ready();
    let before = e.store().orders().len();
    let r = query(&mut e, fills(None));
    assert!(r.url.contains("page_size=100"));
    assert!(r.headers.iter().any(|(k, _)| *k == "Signature"));
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{EXEC}]}}"#),
    );
    let output = result(&mut e).unwrap();
    assert!(output.contains(r#""id":"739113577038255616""#));
    assert!(output.contains(r#""qty":0.006"#));
    assert!(output.contains(r#""timestamp":1700000000.0"#));
    assert!(output.contains(r#""positionSide":"long""#));
    assert!(output.contains(r#""maker":false"#));
    assert_eq!(e.store().orders().len(), before);
}
#[test]
fn paginates_full_pages_and_deduplicates_exact_execution_ids() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(102)));
    let body = format!(
        r#"{{"success":true,"code":0,"data":[{}]}}"#,
        (0..100)
            .map(|n| EXEC.replace(
                "739113577038255616",
                &(739113577038255616u64 + n).to_string()
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    response(&mut e, &r, &body);
    let next = requests(&mut e).remove(0);
    assert!(next.url.contains("page_num=2"));
    response(
        &mut e,
        &next,
        &format!(r#"{{"success":true,"code":0,"data":[{EXEC}]}}"#),
    );
    let out = result(&mut e).unwrap();
    assert_eq!(out.matches("commissionAsset").count(), 100);
}
#[test]
fn failed_second_page_rejects_instead_of_returning_partial_history() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(101)));
    let rows = vec![EXEC; 100].join(",");
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{rows}]}}"#),
    );
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        r#"{"success":false,"code":510,"message":"rate limited","data":[]}"#,
    );
    assert!(result(&mut e).is_err());
}
#[test]
fn funding_nested_pages_filter_time_and_keep_ledger_units() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: Some(1700000000000.0),
            to_ms: Some(1700000001000.0),
            limit: Some(10),
        },
    );
    assert!(!r.url.contains("start_time"));
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"pageSize":100,"currentPage":1,"totalPage":2,"resultList":[{"id":328033,"symbol":"BTC_USDT","funding":0.0837798,"settleTime":1700000000123}]}}"#,
    );
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"currentPage":2,"totalPage":2,"resultList":[{"id":328032,"symbol":"BTC_USDT","funding":-0.1,"settleTime":1699999999000}]}}"#,
    );
    let out = result(&mut e).unwrap();
    assert!(out.contains(r#""income":0.0837798"#));
    assert!(out.contains(r#""timestamp":1700000000123.0"#));
    assert!(!out.contains("328032"));
}
#[test]
fn history_and_public_trades_use_binance_result_shapes() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(r.url.contains("states=3%2C4%2C5"));
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":[{"orderId":"739113577038255616","symbol":"BTC_USDT","side":4,"orderType":1,"state":4,"vol":60,"dealVol":10,"price":60000,"dealAvgPrice":60001,"createTime":1700000000000,"updateTime":1700000001000}]}"#,
    );
    let out = result(&mut e).unwrap();
    assert!(out.contains(r#""filled":0.001"#));
    assert!(out.contains(r#""createdTime":1700000000000.0"#));
    assert!(out.contains(r#""reduceOnly":true"#));
    let r = query(
        &mut e,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(2),
        },
    );
    assert!(r.headers.is_empty());
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":[{"p":60000,"v":60,"T":1,"t":1700000000123},{"p":60000,"v":60,"T":1,"t":1700000000123}]}"#,
    );
    let out = result(&mut e).unwrap();
    assert_eq!(out.matches("quoteQty").count(), 2);
    assert!(out.contains(r#""quoteQty":360.0"#));
}
#[test]
fn unsupported_income_missing_symbol_and_invalid_ranges_finish_with_errors() {
    for q in [
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(2.0),
            to_ms: Some(1.0),
            limit: None,
        },
    ] {
        let mut e = ready();
        e.handle(
            1000.0,
            Event::Command(Command::Query {
                command: 99,
                query: q,
            }),
        );
        assert!(result(&mut e).is_err());
    }
}

#[test]
fn general_income_cannot_silently_return_a_funding_only_ledger() {
    for kind in [
        None,
        Some("REALIZED_PNL"),
        Some("COMMISSION"),
        Some("TRANSFER"),
    ] {
        let mut e = ready();
        e.handle(
            1000.0,
            Event::Command(Command::Query {
                command: 99,
                query: Query::Income {
                    symbol: Some("BTCUSDT".into()),
                    kind: kind.map(str::to_owned),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandDone { result: Err(error), .. })
            if error.message == "MEXC income supports only kind FUNDING_FEE"
        )));
    }
}
#[test]
fn wide_ranges_are_split_into_disjoint_ninety_day_windows() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(1.0),
            to_ms: Some(91.0 * 86400000.0),
            limit: Some(1000),
        },
    );
    assert!(r.url.contains("end_time=7776000000"));
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("start_time=7776000001"));
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
}
