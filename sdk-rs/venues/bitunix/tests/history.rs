//! History payloads and paging contracts: exchange Bitunix CLAUDE.md and official history APIs.
mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use serde::Deserialize;
use support::*;
fn done(e: &mut TestEngine) -> Result<String, String> {
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
            _ => None,
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
        assert!(done(&mut e).is_err(), "unvalidated history must reject");
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
    reply(&mut e, r.id, r#"{"code":0,"data":{"tradeList":[]}}"#);
    let rows: Vec<Income> = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 200);
    assert_eq!(rows[0].income_type, "REALIZED_PNL");
    assert_eq!(rows[0].income, -3.5);
    assert_eq!(rows[1].income_type, "COMMISSION");
    assert_eq!(rows[1].income, -2.0);
    assert!(rows.iter().all(|r| !r.tran_id.is_empty()));
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { command: 8, error })
            if error.legacy_text() == "Bitunix client order lookup is not supported by the Rust engine"
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Notice(Notice::CommandDone {result:s,..}) if s=="[]"))
    );
}

fn history_query(kind: usize) -> Query {
    match kind {
        0 => Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        1 => Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        _ => Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }
}
fn start_history(e: &mut TestEngine, kind: usize) -> safe_cex_engine::HttpRequest {
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 41,
            query: history_query(kind),
        }),
    );
    requests(e).remove(0)
}
fn history_page(kind: usize, page: usize) -> String {
    let rows = (0..100).map(|i| {
        let id = page * 100 + i;
        let time = 1_700_000_000_000.0 - id as f64;
        if kind == 1 { history_order(id, time) } else { format!(r#"{{"tradeId":"{id}","symbol":"BTCUSDT","ctime":{time},"realizedPNL":"-3.5","fee":"2"}}"#) }
    }).collect::<Vec<_>>().join(",");
    let key = if kind == 1 { "orderList" } else { "tradeList" };
    format!(r#"{{"code":0,"data":{{"{key}":[{rows}]}}}}"#)
}
fn fail_history(
    e: &mut TestEngine,
    request: safe_cex_engine::RequestId,
    failure: usize,
    kind: usize,
) {
    let key = if kind == 1 { "orderList" } else { "tradeList" };
    match failure {
        0 => e.handle(
            2.0,
            Event::HttpFailed {
                request,
                error: "connection reset",
            },
        ),
        1 => e.handle(
            2.0,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status: 503,
                    headers: "",
                    body: "upstream unavailable",
                },
            },
        ),
        2 => reply(
            e,
            request,
            r#"{"code":10001,"msg":"read failed","data":{}}"#,
        ),
        3 => reply(
            e,
            request,
            &format!(r#"{{"code":0,"data":{{"{key}":[null]}}}}"#),
        ),
        4 => reply(
            e,
            request,
            &format!(r#"{{"code":0,"data":{{"{key}":[],"broken":"#),
        ),
        _ => reply(e, request, r#"{"code":0,"data":{}}"#),
    }
}
fn owned_partial(actions: &[Action], expected_rows: usize) {
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)).count(), 1);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 41, .. })
            ))
            .count(),
        1
    );
    let (index, marker) = actions
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
        .expect("incomplete history must identify its caller");
    assert_eq!(marker.level, safe_cex_engine::Level::Warn);
    assert_eq!(
        marker.fields,
        vec![("command", safe_cex_engine::Value::Num(41.0))]
    );
    let (done_index, result) = actions
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Notice(Notice::CommandDone {
                command: 41,
                result,
            }) => Some((i, result)),
            _ => None,
        })
        .expect("validated retained history");
    assert!(index < done_index);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(result)
            .unwrap()
            .len(),
        expected_rows
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
}
#[test]
fn first_page_failures_never_complete_as_empty_history() {
    for kind in 0..3 {
        for failure in 0..5 {
            let (mut e, _) = startup();
            let request = start_history(&mut e, kind);
            fail_history(&mut e, request.id, failure, kind);
            assert!(done(&mut e).is_err(), "kind={kind}, failure={failure}");
        }
    }
}
#[test]
fn later_page_failures_resolve_only_accepted_rows_with_owned_incomplete_evidence() {
    for kind in 1..3 {
        for failure in 0..6 {
            let (mut e, _) = startup();
            let request = start_history(&mut e, kind);
            reply(&mut e, request.id, &history_page(kind, 0));
            let request = requests(&mut e).remove(0);
            fail_history(&mut e, request.id, failure, kind);
            owned_partial(
                &safe_cex_engine::test_support::prepared_now(&mut e)
                    .into_iter()
                    .collect::<Vec<_>>(),
                if kind == 1 { 100 } else { 200 },
            );
        }
    }
}
#[test]
fn duplicate_full_pages_retain_rows_only_with_owned_incomplete_evidence() {
    for kind in 1..3 {
        let (mut e, _) = startup();
        e.set_log_level(safe_cex_engine::Level::Error);
        let request = start_history(&mut e, kind);
        let page = history_page(kind, 0);
        reply(&mut e, request.id, &page);
        let request = requests(&mut e).remove(0);
        reply(&mut e, request.id, &page);
        owned_partial(
            &safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .collect::<Vec<_>>(),
            if kind == 1 { 100 } else { 200 },
        );
    }
}
#[test]
fn page_caps_retain_rows_with_owned_warning_before_done() {
    for kind in 1..3 {
        let (mut e, _) = startup();
        e.set_log_level(safe_cex_engine::Level::Error);
        let mut request = start_history(&mut e, kind);
        let pages = if kind == 1 { 50 } else { 100 };
        for page in 0..pages {
            reply(&mut e, request.id, &history_page(kind, page));
            if page + 1 < pages {
                request = requests(&mut e).remove(0);
            }
        }
        let actions = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>();
        owned_partial(&actions, if kind == 1 { 5000 } else { 20000 });
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message.contains("page cap"))));
    }
}
#[test]
fn a_valid_empty_end_page_remains_complete_after_retained_rows() {
    for kind in 1..3 {
        let (mut e, _) = startup();
        let request = start_history(&mut e, kind);
        reply(&mut e, request.id, &history_page(kind, 0));
        let request = requests(&mut e).remove(0);
        reply(
            &mut e,
            request.id,
            if kind == 1 {
                r#"{"code":0,"data":{"orderList":[]}}"#
            } else {
                r#"{"code":0,"data":{"tradeList":[]}}"#
            },
        );
        let actions = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>();
        assert!(!actions.iter().any(|a|matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
        let result = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: 41,
                    result,
                }) => Some(result),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Vec<serde_json::Value>>(result)
                .unwrap()
                .len(),
            if kind == 1 { 100 } else { 200 }
        );
    }
}

#[test]
fn a_full_fresh_order_page_without_time_progress_is_owned_partial_history() {
    let (mut e, _) = startup();
    let request = start_history(&mut e, 1);
    reply(&mut e, request.id, &history_page(1, 0));
    let request = requests(&mut e).remove(0);
    let rows = (100..200)
        .map(|id| history_order(id, 1_700_000_000_000.0 - 99.0))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        request.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    owned_partial(
        &safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>(),
        200,
    );
}
#[test]
fn a_full_order_page_at_the_inclusive_start_cannot_prove_complete_history() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 41,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(1_700_000_000_000.0),
                to_ms: None,
                limit: None,
            },
        }),
    );
    let request = requests(&mut e).remove(0);
    let rows = (0..100)
        .map(|id| history_order(id, 1_700_000_000_000.0))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        request.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    owned_partial(
        &safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>(),
        100,
    );
}
#[test]
fn order_rows_before_the_requested_start_prove_the_walk_is_complete() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 41,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(1_700_000_000_000.0),
                to_ms: None,
                limit: None,
            },
        }),
    );
    let request = requests(&mut e).remove(0);
    let rows = (0..100)
        .map(|id| history_order(id, 1_700_000_000_000.0 - 1.0))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        request.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    let actions = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone {command:41,result}) if result=="[]")
    ));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Notice(Notice::Event(_))))
    );
}

fn typed_rejection(actions: &[Action], command: u32, not_sent: bool) {
    let failures = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: owner,
                error,
            }) if *owner == command => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1, "one typed rejection");
    assert_eq!(failures[0].kind, Some(safe_cex_engine::ErrorKind::Internal));
    if not_sent {
        assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    }
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { .. }) | Action::Http(_)
    )));
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
    )));
}

#[test]
fn first_page_failure_has_a_typed_owner_and_never_an_empty_success() {
    for kind in 0..3 {
        for failure in 0..6 {
            let (mut e, _) = startup();
            let request = start_history(&mut e, kind);
            fail_history(&mut e, request.id, failure, kind);
            typed_rejection(
                &safe_cex_engine::test_support::prepared_now(&mut e)
                    .into_iter()
                    .collect::<Vec<_>>(),
                41,
                false,
            );
        }
    }
}

#[test]
fn failed_later_page_cannot_publish_its_valid_prefix() {
    for kind in 1..3 {
        let (mut e, _) = startup();
        e.set_log_level(safe_cex_engine::Level::Error);
        let request = start_history(&mut e, kind);
        reply(&mut e, request.id, &history_page(kind, 0));
        let request = requests(&mut e).remove(0);
        let page = history_page(kind, 1);
        let mut value: serde_json::Value = serde_json::from_str(&page).unwrap();
        let key = if kind == 1 { "orderList" } else { "tradeList" };
        value["data"][key].as_array_mut().unwrap().truncate(1);
        value["data"][key]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::Null);
        reply(&mut e, request.id, &value.to_string());
        owned_partial(
            &safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .collect::<Vec<_>>(),
            if kind == 1 { 100 } else { 200 },
        );
    }
}

#[test]
fn raw_progress_without_accepted_rows_still_rejects_a_later_failure() {
    for kind in 1..3 {
        let (mut e, _) = startup();
        let request = start_history(&mut e, kind);
        let page = if kind == 1 {
            history_page(kind, 0).replace("\"side\":\"BUY\"", "\"side\":\"UNRECOGNIZED\"")
        } else {
            history_page(kind, 0).replace("\"symbol\":\"BTCUSDT\"", "\"symbol\":\"\"")
        };
        reply(&mut e, request.id, &page);
        let request = requests(&mut e).remove(0);
        fail_history(&mut e, request.id, 0, kind);
        typed_rejection(
            &safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .collect::<Vec<_>>(),
            41,
            false,
        );
    }
}

#[test]
fn partial_query_cannot_mark_or_complete_its_pending_neighbour() {
    let (mut e, _) = startup();
    e.set_log_level(safe_cex_engine::Level::Error);
    let first = start_history(&mut e, 1);
    reply(&mut e, first.id, &history_page(1, 0));
    let next = requests(&mut e).remove(0);
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 42,
            query: history_query(2),
        }),
    );
    let neighbour = requests(&mut e).remove(0);
    fail_history(&mut e, next.id, 0, 1);
    let actions = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect::<Vec<_>>();
    owned_partial(&actions, 100);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { command: 42, .. } | Notice::CommandFailed { command: 42, .. }
        )
    )));
    reply(
        &mut e,
        neighbour.id,
        r#"{"code":0,"data":{"tradeList":[]}}"#,
    );
    let actions = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 42, result }) if result == "[]")));
    assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}

#[test]
fn signing_preparation_refusal_stays_strict_after_accepted_rows() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let refused = Arc::new(AtomicBool::new(false));
    let control = refused.clone();
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    });
    e.set_signing_entropy(Box::new(move |bytes| {
        if control.load(Ordering::SeqCst) {
            Err("signing source unavailable".into())
        } else {
            bytes.fill(7);
            Ok(())
        }
    }))
    .unwrap();
    let (mut e, _) = startup_with_engine(e);
    let request = start_history(&mut e, 1);
    refused.store(true, Ordering::SeqCst);
    reply(&mut e, request.id, &history_page(1, 0));
    // A managed page signs when it is prepared.
    typed_rejection(
        &safe_cex_engine::test_support::prepared_actions(&mut e, 0.0),
        41,
        true,
    );
}

#[test]
fn a_genuine_empty_first_page_is_complete_without_a_followup_request() {
    for kind in 0..3 {
        let (mut e, _) = startup();
        let request = start_history(&mut e, kind);
        reply(
            &mut e,
            request.id,
            if kind == 1 {
                r#"{"code":0,"data":{"orderList":[]}}"#
            } else {
                r#"{"code":0,"data":{"tradeList":[]}}"#
            },
        );
        let actions = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>();
        assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 41, result }) if result == "[]")));
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
        )));
        assert!(!actions.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
    }
}

#[test]
fn an_unbounded_first_page_with_invalid_time_progress_stalls_without_leaking_a_token() {
    let (mut e, _) = startup();
    let request = start_history(&mut e, 1);
    assert!(!request.url.contains("endTime="));
    assert!(!request.url.contains("unbounded"));
    let rows = (0..100)
        .map(|id| history_order(id, 0.0))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        request.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    owned_partial(
        &safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>(),
        0,
    );
}

#[test]
fn an_explicit_first_end_time_that_does_not_advance_is_incomplete() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 41,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: Some(1_700_000_000_000.0),
                limit: None,
            },
        }),
    );
    let request = requests(&mut e).remove(0);
    let rows = (0..100)
        .map(|id| history_order(id, 1_700_000_000_000.0))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut e,
        request.id,
        &format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#),
    );
    owned_partial(
        &safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect::<Vec<_>>(),
        100,
    );
}

#[test]
fn a_requested_order_limit_does_not_short_circuit_native_paging() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 41,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: None,
                limit: Some(1),
            },
        }),
    );
    let request = requests(&mut e).remove(0);
    reply(&mut e, request.id, &history_page(1, 0));
    let request = requests(&mut e).remove(0);
    assert!(request.url.contains("endTime=1699999999901"));
    reply(&mut e, request.id, r#"{"code":0,"data":{"orderList":[]}}"#);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&done(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "0");
}

/// History pages sign with the read key when one is configured; each page is a
/// leg of the command's own caller root.
#[test]
fn history_pages_sign_with_the_read_key() {
    let (mut e, _) = startup_with_engine(support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-material".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-material".into(),
            passphrase: None,
        }),
        ..Default::default()
    }));
    let request = start_history(&mut e, 1);
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "api-key" && value == "read-key"),
        "the read key signs history"
    );
}
