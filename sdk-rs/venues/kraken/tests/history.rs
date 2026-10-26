//! Native schemas: docs.kraken.com/api-reference/account-history/get-order-events
//! and /market-data/get-trade-history; measured fill/ledger semantics in the TS
//! kraken.fills.ts and kraken.income.ts runtime contracts.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query, Response};
use safe_cex_venue_kraken::Kraken;
use serde_json::{Value, json};
use support::*;

fn query(e: &mut Engine<Kraken>, query: Query) -> HttpRequest {
    e.handle(NOW, Event::Command(Command::Query { command: 91, query }));
    requests(e).pop().expect("history request")
}
fn result(e: &mut Engine<Kraken>) -> Result<Value, String> {
    e.drain_actions()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(Ok(serde_json::from_str(&result).unwrap())),
            Action::Notice(Notice::CommandFailed { command: 91, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("query completion")
}
fn fills() -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: Some(NOW - 60_000.0),
        to_ms: Some(NOW),
        limit: None,
    }
}
fn income() -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(NOW - 60_000.0),
        to_ms: Some(NOW),
        limit: None,
    }
}
fn orders() -> Query {
    Query::OrderHistory {
        symbol: None,
        from_ms: Some(NOW - 60_000.0),
        to_ms: Some(NOW),
        limit: None,
    }
}
fn fill(id: &str, time: &str) -> Value {
    json!({"fill_id":id,"symbol":"pf_xbtusd","side":"buy","order_id":"18446744073709551610","size":"0.1","price":"100","fillTime":time,"fillType":"maker","realized_pnl":"0"})
}
fn ledger(id: &str, time: &str) -> Value {
    json!({"booking_uid":id,"date":time,"asset":"usd","contract":"pf_xbtusd","execution":"18446744073709551615","info":"futures trade","realized_pnl":"2.5","realized_funding":"-0.125","fee":"0.2","liquidation_fee":"0.1","old_balance":"100","new_balance":"102.075"})
}
fn event(id: &str, order_id: &str, event: &str, time: f64) -> Value {
    json!({"uid":id,"timestamp":time,"event":{event:{"order":{"uid":order_id,"tradeable":"pf_xbtusd","direction":"Sell","quantity":"2","filled":"0.5","timestamp":NOW-10_000.0,"limitPrice":"110","orderType":"Limit","reduceOnly":true,"lastUpdateTimestamp":time}}}})
}
fn answer(e: &mut Engine<Kraken>, request: &HttpRequest, field: &str, rows: Vec<Value>) {
    reply(
        e,
        request,
        200,
        &json!({"result":"success",field:rows}).to_string(),
    );
}
#[test]
fn fills_keep_exact_ids_missing_fees_zero_pnl_and_millisecond_boundaries() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("PF_XBTUSD".into()),
            order_id: Some("18446744073709551610".into()),
            from_ms: Some(NOW - 999.0),
            to_ms: Some(NOW - 1.0),
            limit: Some(1),
        },
    );
    assert!(
        r.url.ends_with("/derivatives/api/v3/fills"),
        "newest page must be unwindowed to retain PnL"
    );
    assert!(r.headers.iter().any(|(name, _)| *name == "Authent"));
    answer(
        &mut e,
        &r,
        "fills",
        vec![
            fill("old", "2023-11-14T22:13:19.000Z"),
            fill("18446744073709551615", "2023-11-14T22:13:19.999Z"),
            fill("new", "2023-11-14T22:13:20.000Z"),
        ],
    );
    let result = result(&mut e).unwrap();
    let row = &result[0];
    assert_eq!(result.as_array().unwrap().len(), 1);
    assert_eq!(row["id"], "18446744073709551615");
    assert_eq!(row["orderId"], "18446744073709551610");
    assert_eq!(row["timestamp"], 1_699_999_999.0);
    assert_eq!(row["qty"], 0.1);
    assert_eq!(row["realizedPnl"], 0.0);
    assert_eq!(row["maker"], true);
    assert!(row.get("commission").is_none());
    assert!(row.get("positionSide").is_none());
}
#[test]
fn iso_cursor_keeps_native_milliseconds_and_overlap_fills_are_deduplicated() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills());
    let rows = (0..100)
        .map(|i| {
            fill(
                &i.to_string(),
                if i == 99 {
                    "2023-11-14T22:13:19.234Z"
                } else {
                    "2023-11-14T22:13:19.999Z"
                },
            )
        })
        .collect();
    answer(&mut e, &r, "fills", rows);
    let next = requests(&mut e).pop().expect("next page");
    assert!(
        next.url
            .contains("lastFillTime=2023-11-14T22%3A13%3A19.235Z"),
        "{}",
        next.url
    );
    let mut old = fill("100", "2023-11-14T22:13:18.100Z");
    old.as_object_mut().unwrap().remove("realized_pnl");
    answer(
        &mut e,
        &next,
        "fills",
        vec![fill("99", "2023-11-14T22:13:19.234Z"), old],
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 101);
    assert!(rows[100].get("realizedPnl").is_none());
}
#[test]
fn full_fill_boundary_reports_incomplete_and_never_drops_same_time_executions() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills());
    let rows: Vec<_> = (0..100)
        .map(|i| fill(&i.to_string(), "2023-11-14T22:13:19.999Z"))
        .collect();
    answer(&mut e, &r, "fills", rows.clone());
    let next = requests(&mut e).pop().unwrap();
    answer(&mut e, &next, "fills", rows);
    assert!(result(&mut e).unwrap_err().contains("INCOMPLETE"));
}
#[test]
fn full_fill_page_beyond_from_proves_coverage_before_output_limit() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(NOW - 100.0),
            to_ms: Some(NOW),
            limit: Some(2),
        },
    );
    answer(
        &mut e,
        &r,
        "fills",
        (0..100)
            .map(|i| {
                fill(
                    &i.to_string(),
                    if i == 99 {
                        "2023-11-14T22:13:19.000Z"
                    } else {
                        "2023-11-14T22:13:19.999Z"
                    },
                )
            })
            .collect(),
    );
    assert_eq!(result(&mut e).unwrap().as_array().unwrap().len(), 2);
}
#[test]
fn ledger_uses_correct_signing_path_no_trap_params_and_only_cash_leg() {
    let (mut e, _) = ready();
    let r = query(&mut e, income());
    assert!(
        r.url
            .contains("/api/history/v3/account-log?count=500&since=1699999940000")
    );
    assert!(!r.url.contains("derivatives"));
    assert!(!r.url.contains("before="));
    assert!(!r.url.contains("sort="));
    let row = ledger("booking", "2023-11-14T22:13:19.999Z");
    let mut twin = row.clone();
    twin["asset"] = json!("pf_xbtusd");
    answer(&mut e, &r, "logs", vec![row.clone(), twin, row]);
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 4);
    let category = |kind: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|row| row["category"] == kind)
            .unwrap()
    };
    assert_eq!(category("commission")["income"], -0.2);
    assert_eq!(category("funding")["income"], -0.125);
    assert_eq!(category("realized_pnl")["income"], 2.5);
    assert_eq!(category("liquidation")["income"], -0.1);
    assert_eq!(category("commission")["tradeId"], "18446744073709551615");
    assert_eq!(category("commission")["tranId"], "booking:fee");
    assert_eq!(category("commission")["timestamp"], NOW - 1.0);
}
#[test]
fn ledger_transfers_preserve_account_rows_and_filter_native_income_type() {
    let (mut e, _) = ready();
    let r = query(&mut e, income());
    answer(
        &mut e,
        &r,
        "logs",
        vec![
            json!({"id":18446744073709551615u64,"date":"2023-11-14T22:13:20Z","info":"admin transfer","asset":"usd","old_balance":"5","new_balance":"10"}),
        ],
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["symbol"], "");
    assert_eq!(rows[0]["category"], "transfer");
    assert_eq!(rows[0]["income"], 5.0);
    assert_eq!(rows[0]["tranId"], "18446744073709551615:transfer");
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("PF_XBTUSD".into()),
            kind: Some("futures trade_fee".into()),
            from_ms: Some(NOW - 60_000.0),
            to_ms: Some(NOW),
            limit: None,
        },
    );
    answer(
        &mut e,
        &r,
        "logs",
        vec![ledger("1", "2023-11-14T22:13:20Z")],
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["category"], "commission");
}
#[test]
fn saturated_descending_ledger_returns_the_newest_page_marked_truncated() {
    let (mut e, _) = ready();
    let r = query(&mut e, income());
    let rows = (0..500)
        .map(|i| {
            let mut row = ledger(
                &i.to_string(),
                if i == 499 {
                    "2023-11-14T22:13:19Z"
                } else {
                    "2023-11-14T22:13:20Z"
                },
            );
            if i % 2 == 1 {
                row["asset"] = json!("pf_xbtusd");
            }
            row
        })
        .collect();
    answer(&mut e, &r, "logs", rows);
    let actions: Vec<_> = e.drain_actions().collect();
    // Like TS: the newest page comes back, explicitly flagged as a lower bound.
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event))
            if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                && event.message.contains("TRUNCATED")
                && event.fields.iter().any(|(key, value)| *key == "command"
                    && matches!(value, safe_cex_engine::Value::Num(n) if *n == 91.0)))));
    let rows = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("truncated, not failed");
    let rows: Vec<Value> = serde_json::from_str(&rows).unwrap();
    assert!(!rows.is_empty(), "the cash legs read are returned");
    let read = rows.len();
    // A caller whose limit the page already satisfies gets a complete answer and no warning.
    let (mut e, _) = ready();
    let mut capped = income();
    if let Query::Income { limit, .. } = &mut capped {
        *limit = Some(1);
    }
    let r = query(&mut e, capped);
    let page: Vec<_> = (0..500)
        .map(|i| ledger(&i.to_string(), "2023-11-14T22:13:20Z"))
        .collect();
    answer(&mut e, &r, "logs", page);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Event(_))))
    );
    assert!(read > 1);
}
#[test]
fn saturated_ascending_ledger_overlaps_newest_boundary_and_detects_stall() {
    let (mut e, _) = ready();
    let r = query(&mut e, income());
    let rows: Vec<_> = (0..500)
        .map(|i| {
            ledger(
                &i.to_string(),
                if i == 0 {
                    "2023-11-14T22:13:19Z"
                } else {
                    "2023-11-14T22:13:20Z"
                },
            )
        })
        .collect();
    answer(&mut e, &r, "logs", rows.clone());
    let next = requests(&mut e).pop().unwrap();
    assert!(next.url.ends_with("since=1700000000000"));
    answer(&mut e, &next, "logs", rows);
    assert!(result(&mut e).unwrap_err().contains("INCOMPLETE"));
}
#[test]
fn order_history_walks_opaque_header_cursor_and_keeps_latest_terminal_state() {
    let (mut e, _) = ready();
    let r = query(&mut e, orders());
    assert!(r.url.contains("/api/history/v3/orders?"));
    assert!(r.url.contains("closed=true"));
    assert!(r.url.contains("opened=false"));
    assert!(r.url.contains("sort=desc"));
    let body=json!({"accountUid":"a","len":1,"elements":[event("e1","native","OrderCancelled",NOW-1.0)]}).to_string();
    e.handle(
        NOW,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "next-continuation-token: a+/=",
                body: &body,
            },
        },
    );
    let next = requests(&mut e).pop().unwrap();
    assert!(next.url.contains("continuation_token=a%2B%2F%3D"));
    reply(&mut e,&next,200,&json!({"accountUid":"a","len":1,"elements":[event("e2","native","OrderCancelled",NOW-2.0)]}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "native");
    assert_eq!(rows[0]["status"], "canceled");
    assert_eq!(rows[0]["filled"], 0.5);
    assert_eq!(rows[0]["updatedTime"], NOW - 1.0);
    assert!(rows[0].get("averagePrice").is_none());
}
#[test]
fn order_history_full_fill_and_rejected_edit_never_create_phantom_open_orders() {
    let (mut e, _) = ready();
    let r = query(&mut e, orders());
    let mut filled = event("e1", "filled", "OrderPlaced", NOW - 1.0);
    filled["event"]["OrderPlaced"]["order"]["filled"] = json!("2");
    let mut edited = event("e2", "filled", "OrderEditRejected", NOW);
    let order = edited["event"]["OrderEditRejected"]["order"].take();
    edited["event"]["OrderEditRejected"] =
        json!({"oldOrder":order,"attemptedOrder":{},"orderError":"rejected"});
    reply(&mut e,&r,200,&json!({"accountUid":"a","len":3,"elements":[filled,edited,event("e3","open","OrderPlaced",NOW-1.0)]}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["status"], "closed");
}
#[test]
fn repeated_order_cursor_error_is_not_partial_success() {
    let (mut e, _) = ready();
    let r = query(&mut e, orders());
    let body=json!({"accountUid":"a","len":1,"continuationToken":"same","elements":[event("e1","native","OrderCancelled",NOW-1.0)]}).to_string();
    reply(&mut e, &r, 200, &body);
    let next = requests(&mut e).pop().unwrap();
    reply(&mut e, &next, 200, &body);
    assert!(result(&mut e).unwrap_err().contains("INCOMPLETE"));
}
#[test]
fn public_trades_use_official_unsigned_route_taker_side_and_exact_identity() {
    let (mut e, _) = ready_with(safe_cex_venue_kraken::Options {
        public_data_only: true,
        ..Default::default()
    });
    let r = query(
        &mut e,
        Query::PublicTrades {
            symbol: "PF_XBTUSD".into(),
            limit: Some(1),
        },
    );
    assert!(
        r.url
            .ends_with("/derivatives/api/v3/history?symbol=pf_xbtusd")
    );
    assert!(r.headers.is_empty());
    answer(
        &mut e,
        &r,
        "history",
        vec![
            json!({"trade_id":18446744073709551615u64,"time":"2023-11-14T22:13:19.999Z","price":100,"size":"0.5","side":"sell","type":"fill"}),
        ],
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["id"], "18446744073709551615");
    assert_eq!(rows[0]["timestamp"], NOW - 1.0);
    assert_eq!(rows[0]["quoteQty"], 50.0);
    assert_eq!(rows[0]["side"], "sell");
}
#[test]
fn malformed_or_refused_history_never_completes_as_an_empty_account() {
    for query_kind in [
        fills(),
        income(),
        orders(),
        Query::PublicTrades {
            symbol: "PF_XBTUSD".into(),
            limit: None,
        },
    ] {
        for body in [
            r#"{"result":"error","error":"authenticationError"}"#,
            r#"{"status":"unauthorized","reason":"Not authorized"}"#,
            r#"{"result":"success"}"#,
            r#"{"result":"success","fills":[{"fill_id":"broken"}]}"#,
        ] {
            let (mut e, _) = ready();
            let r = query(&mut e, query_kind.clone());
            reply(&mut e, &r, 200, body);
            assert!(result(&mut e).is_err(), "{body}");
        }
    }
}
#[test]
fn query_failure_and_client_id_lookup_fail_without_partial_rows_or_writes() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills());
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "connection reset",
        },
    );
    assert!(result(&mut e).is_err());
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 91,
            query: Query::OrderByClientId {
                symbol: "PF_XBTUSD".into(),
                client_order_id: "caller".into(),
            },
        }),
    );
    assert!(result(&mut e).unwrap_err().contains("does not support"));
}
#[test]
fn concurrent_history_queries_keep_their_own_range_and_completion() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills());
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 92,
            query: income(),
        }),
    );
    let ledger_request = requests(&mut e).pop().unwrap();
    answer(
        &mut e,
        &ledger_request,
        "logs",
        vec![ledger("1", "2023-11-14T22:13:20Z")],
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 92,
            result: _
        })
    )));
    answer(&mut e, &r, "fills", vec![fill("1", "2023-11-14T22:13:20Z")]);
    assert_eq!(result(&mut e).unwrap().as_array().unwrap().len(), 1);
}

#[test]
fn saturated_descending_ledger_proves_narrow_window_coverage_when_it_crosses_start() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(NOW - 100.0),
            to_ms: Some(NOW),
            limit: None,
        },
    );
    let rows = (0..500)
        .map(|i| {
            ledger(
                &i.to_string(),
                if i < 2 {
                    "2023-11-14T22:13:20Z"
                } else {
                    "2023-11-14T22:13:19Z"
                },
            )
        })
        .collect();
    answer(&mut e, &r, "logs", rows);
    assert_eq!(result(&mut e).unwrap().as_array().unwrap().len(), 8);
}

#[test]
fn fill_page_budget_exhaustion_is_an_explicit_incomplete_read() {
    let (mut e, _) = ready();
    let mut request = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(NOW - 7.0 * 86_400_000.0),
            to_ms: Some(NOW),
            limit: Some(1),
        },
    );
    for page in 0..50 {
        let time = format!("2023-11-14T22:12:{:02}.123Z", 59 - page);
        answer(
            &mut e,
            &request,
            "fills",
            (0..100)
                .map(|i| fill(&format!("{page}:{i}"), &time))
                .collect(),
        );
        if page < 49 {
            request = requests(&mut e).pop().expect("continue before page budget");
        }
    }
    assert!(result(&mut e).unwrap_err().contains("INCOMPLETE"));
}

#[test]
fn rejected_zero_size_attempt_is_a_canceled_historical_order_without_executions() {
    let (mut e, _) = ready();
    let r = query(&mut e, orders());
    let mut rejected = event("reject", "native-reject", "OrderRejected", NOW);
    let wire = &mut rejected["event"]["OrderRejected"]["order"];
    wire["quantity"] = json!("0");
    wire["filled"] = json!("0");
    reply(
        &mut e,
        &r,
        200,
        &json!({"len":1,"elements":[rejected]}).to_string(),
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["status"], "canceled");
    assert_eq!(rows[0]["amount"], 0.0);
    assert_eq!(rows[0]["filled"], 0.0);
}

fn owned_actions(e: &mut Engine<Kraken>, command: u32) -> Vec<Action> {
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "terminal history must not dispatch another request"
    );
    actions
        .into_iter()
        .filter(|a| match a {
            Action::Notice(Notice::CommandDone { command: id, .. })
            | Action::Notice(Notice::CommandFailed { command: id, .. }) => *id == command,
            Action::Notice(Notice::Event(event)) => {
                event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            }
            _ => false,
        })
        .collect()
}
fn owned_partial(actions: &[Action], command: u32) -> Value {
    let markers: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                assert_eq!(event.level, safe_cex_engine::Level::Warn);
                assert!(event.fields.iter().any(|(k, v)| *k == "command"
                    && matches!(v,safe_cex_engine::Value::Num(n) if *n == f64::from(command))));
                Some(i)
            }
            _ => None,
        })
        .collect();
    let done: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some((i, result)),
            Action::Notice(Notice::CommandFailed { .. }) => {
                panic!("accepted history must recover with owned incompleteness")
            }
            _ => None,
        })
        .collect();
    assert_eq!(markers.len(), 1);
    assert_eq!(done.len(), 1);
    assert!(markers[0] < done[0].0);
    serde_json::from_str(done[0].1).unwrap()
}
fn owned_continuation(e: &mut Engine<Kraken>, mode: usize) -> HttpRequest {
    let q = match mode {
        0 => fills(),
        1 => income(),
        2 => orders(),
        _ => Query::PublicTrades {
            symbol: "PF_XBTUSD".into(),
            limit: Some(200),
        },
    };
    let r = query(e, q);
    match mode {
        0=>answer(e,&r,"fills",(0..100).map(|i| fill(&format!("accepted-{i}"),"2023-11-14T22:13:19.999Z")).collect()),
        1=>answer(e,&r,"logs",(0..500).map(|i| ledger(&format!("accepted-{i}"),if i==0 {"2023-11-14T22:13:19.000Z"}else{"2023-11-14T22:13:19.999Z"})).collect()),
        2=>reply(e,&r,200,&json!({"len":1,"continuationToken":"next","elements":[event("accepted-event","native","OrderCancelled",NOW-10.0)]}).to_string()),
        _=>answer(e,&r,"history",(0..100).map(|i|json!({"trade_id":format!("accepted-{i}"),"time":"2023-11-14T22:13:19.999Z","price":100,"size":"0.5","side":"sell"})).collect()),
    }
    requests(e)
        .pop()
        .expect("admitted first page must continue")
}
#[test]
fn owned_first_actual_failures_are_typed_and_never_empty_success() {
    for q in [
        fills(),
        income(),
        orders(),
        Query::PublicTrades {
            symbol: "PF_XBTUSD".into(),
            limit: Some(200),
        },
    ] {
        for failure in 0..3 {
            let (mut e, _) = ready();
            let r = query(&mut e, q.clone());
            match failure {
                0 => e.handle(
                    NOW,
                    Event::HttpFailed {
                        request: r.id,
                        error: "lost before acceptance",
                    },
                ),
                1 => reply(&mut e, &r, 503, r#"{"result":"error","error":"temporary"}"#),
                _ => reply(&mut e, &r, 200, "{"),
            }
            let a = owned_actions(&mut e, 91);
            assert_eq!(a.len(), 1);
            if matches!(q, Query::PublicTrades { .. }) && failure == 0 {
                assert!(
                    matches!(&a[0], Action::Notice(Notice::CommandFailed { command: 91, error })
                    if error.kind == Some(safe_cex_engine::ErrorKind::Network)
                    && error.message == "Kraken history failed: lost before acceptance")
                );
            } else {
                assert!(
                    matches!(&a[0],Action::Notice(Notice::CommandFailed{command:91,error}) if error.kind==Some(safe_cex_engine::ErrorKind::Internal))
                );
            }
        }
    }
}
#[test]
fn owned_later_native_transport_and_malformed_failures_retain_only_accepted_pages() {
    for mode in 0..4 {
        for failure in 0..3 {
            let (mut e, _) = ready();
            e.set_log_level(safe_cex_engine::Level::Error);
            let next = owned_continuation(&mut e, mode);
            match failure {
                0 => reply(
                    &mut e,
                    &next,
                    503,
                    r#"{"result":"error","error":"temporarily unavailable"}"#,
                ),
                1 => e.handle(
                    NOW,
                    Event::HttpFailed {
                        request: next.id,
                        error: "later connection reset",
                    },
                ),
                _ => {
                    let (field, prefix) = match mode {
                        0 => ("fills", fill("failed-prefix", "2023-11-14T22:13:19.000Z")),
                        1 => ("logs", ledger("failed-prefix", "2023-11-14T22:13:19.000Z")),
                        2 => (
                            "elements",
                            event("failed-event", "native", "OrderCancelled", NOW - 1.0),
                        ),
                        _ => (
                            "history",
                            json!({"trade_id":"failed-prefix","time":"2023-11-14T22:13:19.000Z","price":101,"size":"0.5","side":"buy"}),
                        ),
                    };
                    reply(
                        &mut e,
                        &next,
                        200,
                        &json!({"result":"success",field:[prefix,{}]}).to_string(),
                    );
                }
            }
            let rows = owned_partial(&owned_actions(&mut e, 91), 91);
            assert_eq!(rows.as_array().unwrap().len(), [100, 2000, 1, 100][mode]);
            assert!(!rows.to_string().contains("failed-prefix"));
            if mode == 2 {
                assert_eq!(rows[0]["updatedTime"], NOW - 10.0);
            }
            assert!(requests(&mut e).is_empty());
            assert!(e.store().orders().is_empty());
        }
    }
}
#[test]
fn owned_filtered_pages_cannot_authorize_failure_recovery() {
    let (mut e, _) = ready();
    let mut q = fills();
    if let Query::Fills { symbol, .. } = &mut q {
        *symbol = Some("PF_ETHUSD".into());
    }
    let r = query(&mut e, q);
    answer(
        &mut e,
        &r,
        "fills",
        (0..100)
            .map(|i| fill(&i.to_string(), "2023-11-14T22:13:19.999Z"))
            .collect(),
    );
    let next = requests(&mut e)
        .pop()
        .expect("raw facts, not output filters, drive progress");
    e.handle(
        NOW,
        Event::HttpFailed {
            request: next.id,
            error: "no eligible accepted output",
        },
    );
    let a = owned_actions(&mut e, 91);
    assert_eq!(a.len(), 1);
    assert!(
        matches!(&a[0],Action::Notice(Notice::CommandFailed{error,..}) if error.kind==Some(safe_cex_engine::ErrorKind::Internal))
    );
}
#[test]
fn owned_order_length_mismatch_cannot_admit_a_newer_terminal_prefix() {
    let (mut e, _) = ready();
    let next = owned_continuation(&mut e, 2);
    reply(
        &mut e,
        &next,
        200,
        &json!({"len":2,"elements":[event("failed-newer","native","OrderCancelled",NOW-1.0)]})
            .to_string(),
    );
    let rows = owned_partial(&owned_actions(&mut e, 91), 91);
    assert_eq!(rows[0]["updatedTime"], NOW - 10.0);
}
#[test]
fn owned_order_continuation_proof_refusals_remain_strict() {
    for conflict in [false, true] {
        let (mut e, _) = ready();
        let next = owned_continuation(&mut e, 2);
        let body=if conflict {json!({"len":1,"continuationToken":"body","elements":[event("new","native","OrderCancelled",NOW-1.0)]})}else{json!({"len":0,"continuationToken":"next-again","elements":[]})}.to_string();
        e.handle(
            NOW,
            Event::HttpResponse {
                request: next.id,
                response: Response {
                    status: 200,
                    headers: if conflict {
                        "Next-Continuation-Token: other"
                    } else {
                        ""
                    },
                    body: &body,
                },
            },
        );
        let a = owned_actions(&mut e, 91);
        assert_eq!(a.len(), 1);
        assert!(matches!(a[0], Action::Notice(Notice::CommandFailed { .. })));
        assert!(requests(&mut e).is_empty());
    }
}

#[test]
fn owned_fiftieth_terminal_page_is_complete_without_an_extra_request() {
    let (mut e, _) = ready();
    let mut q = fills();
    if let Query::Fills { limit, from_ms, .. } = &mut q {
        *limit = Some(1);
        *from_ms = Some(NOW - 7.0 * 86_400_000.0);
    }
    let mut r = query(&mut e, q);
    for page in 0..49 {
        let time = format!("2023-11-14T22:12:{:02}.123Z", 59 - page);
        answer(
            &mut e,
            &r,
            "fills",
            (0..100)
                .map(|i| fill(&format!("{page}:{i}"), &time))
                .collect(),
        );
        r = requests(&mut e)
            .pop()
            .expect("fills must not stop at the output limit");
    }
    answer(&mut e, &r, "fills", Vec::new());
    let a = owned_actions(&mut e, 91);
    assert_eq!(a.len(), 1);
    assert!(
        matches!(&a[0],Action::Notice(Notice::CommandDone{result,..}) if serde_json::from_str::<Value>(result).unwrap().as_array().unwrap().len()==1)
    );
    assert!(requests(&mut e).is_empty());
}
#[test]
fn owned_saturated_orders_without_native_continuation_proof_reject_after_acceptance() {
    let (mut e, _) = ready();
    let next = owned_continuation(&mut e, 2);
    reply(&mut e,&next,200,&json!({"len":1000,"elements":(0..1000).map(|i|event(&format!("missing-{i}"),&format!("id-{i}"),"OrderCancelled",NOW-1.0)).collect::<Vec<_>>()} ).to_string());
    let a = owned_actions(&mut e, 91);
    assert_eq!(a.len(), 1);
    assert!(
        matches!(&a[0],Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("no continuation token"))
    );
    assert!(requests(&mut e).is_empty());
}
#[test]
fn owned_ledger_above_end_keeps_descending_gap_and_filtered_gap_can_be_empty() {
    for filtered in [false, true] {
        let (mut e, _) = ready();
        e.set_log_level(safe_cex_engine::Level::Error);
        let mut q = income();
        if let Query::Income { to_ms, kind, .. } = &mut q {
            *to_ms = Some(NOW - 1000.0);
            if filtered {
                *kind = Some("absent-category".into());
            }
        }
        let r = query(&mut e, q);
        answer(
            &mut e,
            &r,
            "logs",
            (0..500)
                .map(|i| {
                    ledger(
                        &format!("gap-{i}"),
                        if i == 0 {
                            "2023-11-14T22:13:20Z"
                        } else {
                            "2023-11-14T22:13:18Z"
                        },
                    )
                })
                .collect(),
        );
        let rows = owned_partial(&owned_actions(&mut e, 91), 91);
        assert_eq!(
            rows.as_array().unwrap().len(),
            if filtered { 0 } else { 1996 }
        );
        assert!(requests(&mut e).is_empty());
    }
}
#[test]
fn owned_concurrent_partial_and_complete_queries_keep_distinct_delivery_owners() {
    let (mut e, _) = ready();
    let lost = owned_continuation(&mut e, 0);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 92,
            query: income(),
        }),
    );
    let other = requests(&mut e).pop().unwrap();
    e.handle(
        NOW,
        Event::HttpFailed {
            request: lost.id,
            error: "only owner91 lost a page",
        },
    );
    let rows = owned_partial(&owned_actions(&mut e, 91), 91);
    assert_eq!(rows.as_array().unwrap().len(), 100);
    answer(
        &mut e,
        &other,
        "logs",
        vec![ledger("other", "2023-11-14T22:13:20Z")],
    );
    let a = owned_actions(&mut e, 92);
    assert_eq!(a.len(), 1);
    assert!(matches!(
        &a[0],
        Action::Notice(Notice::CommandDone { command: 92, .. })
    ));
}
#[test]
fn owned_order_equal_timestamp_keeps_first_terminal_state_and_setup_stays_strict() {
    let (mut e, _) = ready();
    let next = owned_continuation(&mut e, 2);
    let mut same = event("different-event", "native", "OrderCancelled", NOW - 10.0);
    same["event"]["OrderCancelled"]["order"]["limitPrice"] = json!(999);
    reply(
        &mut e,
        &next,
        200,
        &json!({"len":1,"elements":[same]}).to_string(),
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["price"], 110.0);
    let (mut e, _) = ready_with(safe_cex_venue_kraken::Options {
        public_data_only: true,
        ..Default::default()
    });
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 91,
            query: fills(),
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    let a: Vec<_> = a
        .into_iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 91, .. })
                    | Action::Notice(Notice::CommandDone { command: 91, .. })
                    | Action::Notice(Notice::Event(_))
            )
        })
        .collect();
    assert_eq!(a.len(), 1);
    assert!(matches!(
        &a[0],
        Action::Notice(Notice::CommandFailed { .. })
    ));
}

#[test]
fn owned_interrupted_stream_discards_valid_unfinished_prefix_after_accepted_pages() {
    for mode in 0..4 {
        let (mut e, _) = ready();
        let next = owned_continuation(&mut e, mode);
        let (field, row) = match mode {
            0 => (
                "fills",
                fill("interrupted-prefix", "2023-11-14T22:13:19.000Z"),
            ),
            1 => (
                "logs",
                ledger("interrupted-prefix", "2023-11-14T22:13:19.000Z"),
            ),
            2 => (
                "elements",
                event("interrupted-event", "native", "OrderCancelled", NOW - 1.0),
            ),
            _ => (
                "history",
                json!({"trade_id":"interrupted-prefix","time":"2023-11-14T22:13:19.000Z","price":101,"size":"0.5","side":"buy"}),
            ),
        };
        e.handle(
            NOW,
            Event::HttpStart {
                request: next.id,
                status: 200,
                headers: "",
            },
        );
        let chunk = format!("{{\"{field}\":[{row},");
        e.handle(
            NOW,
            Event::HttpChunk {
                request: next.id,
                data: chunk.as_bytes(),
            },
        );
        assert!(e.drain_actions().all(|action| match action {
            Action::Notice(Notice::CommandDone { .. })
            | Action::Notice(Notice::CommandFailed { .. })
            | Action::Http(_) => false,
            Action::Notice(Notice::Event(event)) =>
                event.code != safe_cex_engine::log::code::HISTORY_INCOMPLETE,
            _ => true,
        }));
        e.handle(
            NOW,
            Event::HttpFailed {
                request: next.id,
                error: "body interrupted after valid prefix",
            },
        );
        let rows = owned_partial(&owned_actions(&mut e, 91), 91);
        assert_eq!(rows.as_array().unwrap().len(), [100, 2000, 1, 100][mode]);
        assert!(!rows.to_string().contains("interrupted-prefix"));
        if mode == 2 {
            assert_eq!(rows[0]["updatedTime"], NOW - 10.0);
        }
    }
}
