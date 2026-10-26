mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use support::*;
const OBSERVATION: f64 = 1_700_000_000_000.;
fn requests(e: &mut TestEngine) -> Vec<safe_cex_engine::HttpRequest> {
    public_requests(e, OBSERVATION)
}
fn response(e: &mut TestEngine, request: &safe_cex_engine::HttpRequest, body: &str) {
    e.handle(
        OBSERVATION,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn ready() -> TestEngine {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        support::response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
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
                Some(Ok(result))
            } else if let Action::Notice(Notice::CommandFailed { command: 99, error }) = a {
                Some(Err(error.legacy_text()))
            } else {
                None
            }
        })
        .unwrap()
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
fn failed_second_page_returns_only_accepted_rows_with_owned_warning() {
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
    // Explicitly supersedes the former whole-read refusal.
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
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
            Action::Notice(Notice::CommandFailed { error, .. })
            if error.legacy_text() == "MEXC income supports only kind FUNDING_FEE"
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
    // 91 days split newest first: [to - 90d + 1, to], then the remaining day.
    assert!(r.url.contains("start_time=86400001"), "{}", r.url);
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("end_time=86400000"), "{}", r.url);
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
}

/// Pins the exact request sequence of a windowed, page-numbered read. Paging
/// refactors keep it; a deliberate change updates it with its regression.
#[test]
fn windowed_order_history_request_sequence_is_pinned() {
    let mut e = ready();
    let day = 86_400_000.0;
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(1.0),
            to_ms: Some(100.0 * day),
            limit: Some(1000),
        },
    );
    let mut sent = vec![r.url.split_once("/api/").unwrap().1.to_owned()];
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    let r = requests(&mut e).remove(0);
    sent.push(r.url.split_once("/api/").unwrap().1.to_owned());
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
    assert_eq!(
        sent,
        [
            "v1/private/order/list/history_orders?end_time=8640000000&page_num=1&page_size=100&start_time=864000001&states=3%2C4%2C5",
            "v1/private/order/list/history_orders?end_time=864000000&page_num=1&page_size=100&start_time=1&states=3%2C4%2C5",
        ]
    );
}

#[test]
fn a_limited_wide_read_returns_the_newest_rows_not_the_oldest_window() {
    let mut e = ready();
    let day = 86_400_000.0;
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(1.0),
            to_ms: Some(100.0 * day),
            limit: Some(1),
        },
    );
    // The newest window comes first, so one row is enough to stop.
    assert!(r.url.contains("start_time=864000001"), "{}", r.url);
    let order = r#"{"orderId":"newest","symbol":"BTC_USDT","positionId":1,"price":60000,"vol":1,"leverage":10,"side":1,"category":1,"orderType":1,"dealAvgPrice":60000,"dealVol":1,"orderMargin":6,"takerFee":0,"makerFee":0,"profit":0,"feeCurrency":"USDT","openType":1,"state":3,"externalOid":"","errorCode":0,"usedMargin":6,"createTime":8600000000,"updateTime":8600000000}"#;
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{order}]}}"#),
    );
    // Completing after the first page means no older window was requested.
    assert!(result(&mut e).unwrap().contains("newest"));
}

fn owned_terminal(
    e: &mut TestEngine,
    command: u32,
    partial: bool,
) -> Result<String, safe_cex_engine::VenueError> {
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let markers: Vec<_> = actions
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
        "one owner warning for accepted-only recovery"
    );
    for marker in markers {
        assert_eq!(marker.level, safe_cex_engine::Level::Warn);
        assert!(marker.fields.iter().any(|(k, v)| *k == "command"
            && matches!(v,safe_cex_engine::Value::Num(n) if *n == f64::from(command))));
    }
    let mut terminals = actions.into_iter().filter_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: id,
            result,
        }) if id == command => Some(Ok(result)),
        Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
            Some(Err(error))
        }
        _ => None,
    });
    let result = terminals.next().expect("one command terminal");
    assert!(terminals.next().is_none());
    result
}
fn full_page() -> String {
    format!(
        r#"{{"success":true,"code":0,"data":[{}]}}"#,
        vec![EXEC; 100].join(",")
    )
}
fn fail_read(e: &mut TestEngine, r: &safe_cex_engine::HttpRequest, failure: u8) {
    match failure {
        0 => response(
            e,
            r,
            r#"{"success":false,"code":510,"message":"busy","data":[]}"#,
        ),
        1 => e.handle(
            OBSERVATION,
            Event::HttpFailed {
                request: r.id,
                error: "offline",
            },
        ),
        2 => response(e, r, r#"{"success":true,"code":0,"data":{}}"#),
        _ => {
            let fresh = EXEC.replace("739113577038255616", "739113577038255618");
            response(
                e,
                r,
                &format!(r#"{{"success":true,"code":0,"data":[{fresh},{{"bad":true}}]}}"#),
            );
        }
    }
}
#[test]
fn owned_first_and_filtered_only_failures_are_typed_without_a_partial_marker() {
    for filtered in [false, true] {
        for failure in 0..4 {
            let mut e = ready();
            let q = if filtered {
                Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: None,
                    to_ms: Some(1700000000000.0),
                    limit: Some(101),
                }
            } else {
                fills(Some(101))
            };
            let mut r = query(&mut e, q);
            if filtered {
                response(&mut e, &r, &full_page());
                r = requests(&mut e).remove(0);
            }
            fail_read(&mut e, &r, failure);
            let err = owned_terminal(&mut e, 99, false).unwrap_err();
            assert_eq!(err.kind, Some(safe_cex_engine::ErrorKind::Internal));
        }
    }
}
#[test]
fn owned_later_failure_discards_current_prefix_and_keeps_only_accepted_rows() {
    for failure in 0..4 {
        let mut e = ready();
        let r = query(&mut e, fills(Some(101)));
        response(&mut e, &r, &full_page());
        let r = requests(&mut e).remove(0);
        fail_read(&mut e, &r, failure);
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&owned_terminal(&mut e, 99, true).unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "739113577038255616");
    }
}
#[test]
fn owned_funding_page_failure_retains_only_validated_prior_ledger() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        },
    );
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"currentPage":1,"totalPage":2,"resultList":[{"id":1,"symbol":"BTC_USDT","funding":0.1,"settleTime":1700000000123}]}}"#,
    );
    let r = requests(&mut e).remove(0);
    // Valid row prefix, invalid native page metadata: none of page two is admitted.
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"currentPage":9,"totalPage":2,"resultList":[{"id":2,"symbol":"BTC_USDT","funding":0.2,"settleTime":1700000000124}]}}"#,
    );
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["tranId"], "1");
}
#[test]
fn owned_window_transition_retains_accepted_rows_on_older_window_failure() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(1700000001000.0),
            limit: Some(1000),
        },
    );
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{EXEC}]}}"#),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("page_num=1"));
    fail_read(&mut e, &r, 0);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn owned_terminal_page_thousand_succeeds_and_continuation_cap_stays_strict() {
    for needs_more in [false, true] {
        let mut e = ready();
        let mut r = query(&mut e, fills(Some(101)));
        let full = full_page();
        for number in 1..1000 {
            response(&mut e, &r, &full);
            r = requests(&mut e).remove(0);
            assert!(r.url.contains(&format!("page_num={}&", number + 1)));
        }
        response(
            &mut e,
            &r,
            if needs_more {
                &full
            } else {
                r#"{"success":true,"code":0,"data":[]}"#
            },
        );
        let output = owned_terminal(&mut e, 99, false);
        if needs_more {
            assert!(output.unwrap_err().message.contains("pagination limit"));
        } else {
            let rows: Vec<serde_json::Value> = serde_json::from_str(&output.unwrap()).unwrap();
            assert_eq!(rows.len(), 1);
        }
    }
}

#[test]
fn owned_window_identity_retention_and_encounter_order_are_unchanged() {
    let mut e = ready();
    let day = 86_400_000.0;
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(100.0 * day),
            limit: Some(1000),
        },
    );
    let newest = EXEC.replace("1700000000123", "8639999999");
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{newest}]}}"#),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("end_time=864000000") && r.url.contains("page_num=1"));
    let duplicate = EXEC.replace("1700000000123", "100");
    let fresh = duplicate.replace("739113577038255616", "739113577038255618");
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{duplicate},{fresh}]}}"#),
    );
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, false).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "739113577038255616");
    assert_eq!(rows[0]["timestamp"], 8639999.0);
    assert_eq!(rows[1]["id"], "739113577038255618");
}
#[test]
fn owned_concurrent_query_markers_and_empty_success_are_isolated() {
    let mut e = ready();
    let first = query(&mut e, fills(Some(101)));
    e.handle(
        1700000000000.0,
        Event::Command(Command::Query {
            command: 100,
            query: fills(Some(101)),
        }),
    );
    let other = requests(&mut e).remove(0);
    response(&mut e, &first, &full_page());
    let continuation = requests(&mut e).remove(0);
    response(&mut e, &other, r#"{"success":true,"code":0,"data":[]}"#);
    assert_eq!(owned_terminal(&mut e, 100, false).unwrap(), "[]");
    fail_read(&mut e, &continuation, 0);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, true).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn owned_order_specific_fills_stay_single_page_and_global_limit_is_exact() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: Some("739113577038255617".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(101),
        },
    );
    assert!(r.url.contains("/deal_details/739113577038255617?"));
    response(&mut e, &r, &full_page());
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, false).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    let r = query(&mut e, fills(Some(1)));
    let fresh = EXEC.replace("739113577038255616", "739113577038255618");
    response(
        &mut e,
        &r,
        &format!(r#"{{"success":true,"code":0,"data":[{EXEC},{fresh}]}}"#),
    );
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, false).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "739113577038255616");
}

#[test]
fn owned_native_window_budget_resets_after_terminal_page_thousand() {
    let mut e = ready();
    let mut r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(100.0 * 86_400_000.0),
            limit: Some(101),
        },
    );
    let newest = EXEC.replace("1700000000123", "8639999999");
    let full = format!(
        r#"{{"success":true,"code":0,"data":[{}]}}"#,
        vec![newest.as_str(); 100].join(",")
    );
    for number in 1..1000 {
        response(&mut e, &r, &full);
        r = requests(&mut e).remove(0);
        assert!(r.url.contains(&format!("page_num={}&", number + 1)));
    }
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("end_time=864000000") && r.url.contains("page_num=1&"));
    let older = EXEC
        .replace("1700000000123", "100")
        .replace("739113577038255616", "739113577038255618");
    let full = format!(
        r#"{{"success":true,"code":0,"data":[{}]}}"#,
        vec![older.as_str(); 100].join(",")
    );
    response(&mut e, &r, &full);
    let r = requests(&mut e).remove(0);
    assert!(
        r.url.contains("page_num=2&"),
        "second window has its own budget"
    );
    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&owned_terminal(&mut e, 99, false).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "739113577038255616");
    assert_eq!(rows[1]["id"], "739113577038255618");
}
