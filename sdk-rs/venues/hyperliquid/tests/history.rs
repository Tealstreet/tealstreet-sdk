//! Documented userFillsByTime, userFunding, historicalOrders and recentTrades shapes.
mod support;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::Value;
use support::*;
fn query(e: &mut Engine<Hyperliquid>, q: Query) {
    e.handle_command(
        5000.,
        Command::Query {
            command: 1,
            query: q,
        },
        Some(safe_cex_engine::MAX_COMMAND_WAIT_MS),
    )
    .unwrap();
}
fn read(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    read_at(e, &mut 5000.)
}
fn read_at(e: &mut Engine<Hyperliquid>, now: &mut f64) -> HttpRequest {
    loop {
        let output = safe_cex_engine::test_support::prepared_actions(e, *now);
        if let Some(request) = output.iter().find_map(|a| match a {
            Action::Http(request) => Some(request.clone()),
            _ => None,
        }) {
            return request;
        }
        let (timer, at) = output
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms > *now && *at_ms <= *now + 60_000. => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("native read-budget wake");
        *now = at;
        e.handle(at, Event::TimerFired { timer });
    }
}
fn reply(e: &mut Engine<Hyperliquid>, r: &HttpRequest, status: u16, body: &str) {
    reply_at(e, 5000., r, status, body)
}
fn reply_at(e: &mut Engine<Hyperliquid>, now: f64, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn result(e: &mut Engine<Hyperliquid>) -> Value {
    let result = prepared_now(e)
        .into_iter()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap();
    serde_json::from_str(&result).unwrap()
}
fn fill(tid: u64) -> String {
    format!(
        r#"{{"coin":"BTC","side":"B","px":"100","sz":"0.5","time":1000,"tid":{tid},"oid":99,"fee":"-0.01","closedPnl":"2","crossed":false,"feeToken":"USDC"}}"#
    )
}
#[test]
fn inclusive_fill_paging_preserves_distinct_identical_executions_and_rebate_sign() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Fills {
            symbol: Some("BTC-PERP".into()),
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, &format!("[{}]", fill(1)));
    let r = read(&mut e);
    let b: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["startTime"], 1000.0);
    reply(&mut e, &r, 200, &format!("[{},{}]", fill(1), fill(2)));
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["commission"], -0.01);
    assert_eq!(rows[0]["realizedPnl"], 2., "historical PnL remains gross");
    assert!(rows[0].get("positionSide").is_none());
}
#[test]
fn fills_use_vault_identity_and_keep_requested_time_range() {
    let vault = "0x0000000000000000000000000000000000000001";
    let (mut e, _) = ready_identity("disabled", false, None, "30", Some(vault.into()));
    query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(10_000_000_000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    let b: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["user"], vault);
    assert_eq!(b["startTime"], 1.0);
}
#[test]
fn order_transition_collapse_terminal_wins_ties_and_filters_creation_time() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    let wire = r#"{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"2","sz":"0","orderType":"Limit","timestamp":2000}"#;
    reply(
        &mut e,
        &r,
        200,
        &format!(
            r#"[{{"order":{wire},"status":"filled","statusTimestamp":2000}},{{"order":{wire},"status":"open","statusTimestamp":2000}}]"#
        ),
    );
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["status"], "closed");
    assert_eq!(rows[0]["amount"], 2.0);
    assert_eq!(rows[0]["filled"], 2.0);
    assert!(rows[0].get("averagePrice").is_none());
}

#[test]
fn rolling_order_retention_marker_owns_its_result_before_done_at_error_only_level() {
    let (mut e, _) = ready("disabled");
    e.set_log_level(safe_cex_engine::Level::Error);
    query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let request = read(&mut e);
    assert_eq!(kind(&request), "historicalOrders");
    let rows: Vec<_> = (1..=2000).map(|id| format!(r#"{{"order":{{"coin":"BTC","oid":{id},"side":"B","limitPx":"90","origSz":"2","sz":"0","orderType":"Limit","timestamp":2000}},"status":"filled","statusTimestamp":2000}}"#)).collect();
    reply(&mut e, &request, 200, &format!("[{}]", rows.join(",")));
    let actions: Vec<_> = prepared_now(&mut e).into_iter().collect();
    let (index, event) = actions
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
        .expect("semantic retention marker survives Error-only logging");
    assert_eq!(event.level, safe_cex_engine::Level::Warn);
    assert_eq!(
        event.fields,
        vec![("command", safe_cex_engine::Value::Num(1.0))]
    );
    assert!(event.message.contains("rolling retention limit"));
    let (done_index, result) = actions
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Notice(Notice::CommandDone { command: 1, result }) => Some((i, result)),
            _ => None,
        })
        .expect("retained history completes");
    assert!(index < done_index);
    let rows: Vec<Value> = serde_json::from_str(result).unwrap();
    assert_eq!(rows.len(), 2000);
    assert!(
        rows.iter()
            .all(|row| row["status"] == "closed" && row["amount"] == 2.0 && row["filled"] == 2.0)
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
}
#[test]
fn income_splits_fill_pnl_and_rebate_and_reads_nested_funding() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, &format!("[{}]", fill(1)));
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let r = read(&mut e);
    assert_eq!(kind(&r), "userFunding");
    reply(
        &mut e,
        &r,
        200,
        r#"[{"time":2000,"hash":"fund","delta":{"coin":"BTC","usdc":" -0.2"}}]"#,
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "rebate" && r["income"] == 0.01)
    );
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "funding" && r["income"] == -0.2)
    );
}
#[test]
fn failed_or_malformed_history_never_resolves_as_empty_success() {
    for (status, body) in [
        (503, "unavailable"),
        (200, r#"{"error":"rate limited"}"#),
        (200, r#"[{"coin":"BTC","tid":1}]"#),
    ] {
        let (mut e, _) = ready("disabled");
        query(
            &mut e,
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(1.0),
                to_ms: Some(5000.0),
                limit: None,
            },
        );
        let r = read(&mut e);
        reply(&mut e, &r, status, body);
        assert!(
            prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}
#[test]
fn public_trades_use_market_coin_and_return_engine_json_shape() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::PublicTrades {
            symbol: "BTC-PERP".into(),
            limit: Some(1),
        },
    );
    let r = read(&mut e);
    assert_eq!(kind(&r), "recentTrades");
    assert!(r.body.as_ref().unwrap().contains("\"coin\":\"BTC\""));
    reply(
        &mut e,
        &r,
        200,
        r#"[{"coin":"BTC","side":"B","px":"100","sz":"2","time":3000,"tid":7}]"#,
    );
    let rows = result(&mut e);
    assert_eq!(rows[0]["quoteQty"], 200.0);
    assert_eq!(rows[0]["timestamp"], 3000.0);
}
#[test]
fn client_id_lookup_is_pure_and_misses_are_never_definitive() {
    let (mut e, _) = ready("disabled");
    let before = e.store().orders().to_vec();
    query(
        &mut e,
        Query::OrderByClientId {
            symbol: "BTC-PERP".into(),
            client_order_id: "0x00000000000000000000000000000001".into(),
        },
    );
    let r = read(&mut e);
    assert_eq!(kind(&r), "orderStatus");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"order","order":{"order":{"coin":"BTC","oid":9,"cloid":"0x00000000000000000000000000000001","side":"B","limitPx":"90","origSz":"1","sz":"0","orderType":"Limit"},"status":"filled"}}"#,
    );
    let found = result(&mut e);
    assert_eq!(found["found"], true);
    assert_eq!(found["order"]["status"], "closed");
    assert_eq!(e.store().orders(), before);
    query(
        &mut e,
        Query::OrderByClientId {
            symbol: "BTC-PERP".into(),
            client_order_id: "0x00000000000000000000000000000002".into(),
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, r#"{"status":"unknownOid"}"#);
    let miss = result(&mut e);
    assert_eq!(miss["definitive"], false);
}
#[test]
fn execution_limit_sorts_by_original_milliseconds_before_serialized_seconds() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: Some(1),
        },
    );
    let r = read(&mut e);
    reply(
        &mut e,
        &r,
        200,
        &format!(
            "[{},{}]",
            fill(9).replace("\"time\":1000", "\"time\":1100"),
            fill(10).replace("\"time\":1000", "\"time\":1900")
        ),
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows[0]["id"], "10");
}
#[test]
fn full_overlap_page_is_incomplete_instead_of_an_empty_success() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let r = read(&mut e);
    reply(&mut e, &r, 200, &format!("[{}]", fill(1)));
    let r = read(&mut e);
    let repeated = format!("[{}]", vec![fill(1); 2000].join(","));
    reply(&mut e, &r, 200, &repeated);
    // Superseded strict-failure contract: admitted rows now survive with owned
    // incompleteness; this must still never become an empty complete result.
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let rows = owned_partial(&actions, 1);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "1");
}
#[test]
fn concurrent_heavy_reads_reserve_response_weight_before_dispatch() {
    let (mut e, _) = ready("disabled");
    for command in 1..=20 {
        e.handle(
            5000.0,
            Event::Command(Command::Query {
                command,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: Some(1.0),
                    to_ms: Some(5000.0),
                    limit: None,
                },
            }),
        );
    }
    let requests = safe_cex_engine::test_support::prepared_actions(&mut e, 5000.)
        .iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .count();
    assert!(requests > 0, "budget control must release physical reads");
    assert!(
        requests <= 8,
        "must reserve the 120-weight maximum per heavy response"
    );
}

fn owned_fills_query(symbol: Option<&str>) -> Query {
    Query::Fills {
        symbol: symbol.map(str::to_owned),
        order_id: None,
        from_ms: Some(1.0),
        to_ms: Some(5000.0),
        limit: Some(500),
    }
}
fn owned_partial(actions: &[Action], command: u32) -> Value {
    assert!(
        !actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: c, .. }) if *c == command
        )),
        "accepted pages must survive a later failure"
    );
    let markers: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                    && event.fields.contains(&(
                        "command",
                        safe_cex_engine::Value::Num(f64::from(command)),
                    )) =>
            {
                Some((i, event))
            }
            _ => None,
        })
        .collect();
    assert_eq!(markers.len(), 1, "one marker belongs to this query");
    assert_eq!(markers[0].1.level, safe_cex_engine::Level::Warn);
    let done: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                Some((i, result))
            }
            _ => None,
        })
        .collect();
    assert_eq!(done.len(), 1);
    assert!(markers[0].0 < done[0].0);
    serde_json::from_str(done[0].1).unwrap()
}
fn owned_first_failure(actions: &[Action], command: u32) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command: c, .. }) if *c == command)
    ));
    assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}
#[test]
fn owned_history_first_and_zero_eligible_failures_reject_typed() {
    for filtered in [false, true] {
        let (mut e, _) = ready("disabled");
        query(&mut e, owned_fills_query(filtered.then_some("ETH-PERP")));
        let mut request = read(&mut e);
        if filtered {
            reply(&mut e, &request, 200, &format!("[{}]", fill(1)));
            request = read(&mut e);
        }
        reply(&mut e, &request, 503, "unavailable");
        owned_first_failure(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), 1);
    }
}
#[test]
fn owned_history_later_failures_keep_only_validated_pages() {
    for terminal in ["http", "malformed", "stream"] {
        let (mut e, _) = ready("disabled");
        e.set_log_level(safe_cex_engine::Level::Error);
        let before_orders = e.store().orders().to_vec();
        let before_positions = e.store().positions().to_vec();
        query(&mut e, owned_fills_query(None));
        let request = read(&mut e);
        reply(&mut e, &request, 200, &format!("[{}]", fill(1)));
        let request = read(&mut e);
        match terminal {
            "http" => reply(&mut e, &request, 503, "unavailable"),
            "malformed" => reply(&mut e, &request, 200, &format!("[{},{{}}]", fill(2))),
            _ => {
                e.handle(
                    5000.0,
                    Event::HttpStart {
                        request: request.id,
                        status: 200,
                        headers: "",
                    },
                );
                let prefix = format!("[{},", fill(2));
                e.handle(
                    5000.0,
                    Event::HttpChunk {
                        request: request.id,
                        data: prefix.as_bytes(),
                    },
                );
                e.handle(
                    5000.0,
                    Event::HttpFailed {
                        request: request.id,
                        error: "lost partial page",
                    },
                );
            }
        }
        let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
        let rows = owned_partial(&actions, 1);
        assert_eq!(rows.as_array().unwrap().len(), 1, "{terminal}");
        assert_eq!(rows[0]["id"], "1", "failed-page prefix must not leak");
        assert_eq!(e.store().orders(), before_orders);
        assert_eq!(e.store().positions(), before_positions);
    }
}
#[test]
fn owned_history_income_retains_fills_when_funding_fails() {
    let (mut e, _) = ready("disabled");
    e.set_log_level(safe_cex_engine::Level::Error);
    query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let request = read(&mut e);
    reply(&mut e, &request, 200, &format!("[{}]", fill(1)));
    let request = read(&mut e);
    reply(&mut e, &request, 200, "[]");
    let funding = read(&mut e);
    assert_eq!(kind(&funding), "userFunding");
    reply(&mut e, &funding, 503, "unavailable");
    let rows = owned_partial(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), 1);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert!(rows.as_array().unwrap().iter().all(|r| r["tradeId"] == "1"));
}
#[test]
fn owned_history_concurrent_markers_never_cross_queries() {
    let (mut e, _) = ready("disabled");
    e.set_log_level(safe_cex_engine::Level::Error);
    let mut pending = Vec::new();
    for command in [11, 12] {
        e.handle(
            5000.0,
            Event::Command(Command::Query {
                command,
                query: owned_fills_query(None),
            }),
        );
        let request = read(&mut e);
        reply(
            &mut e,
            &request,
            200,
            &format!("[{}]", fill(u64::from(command))),
        );
        pending.push((command, read(&mut e)));
    }
    let mut actions = Vec::new();
    for (command, request) in pending.into_iter().rev() {
        reply(&mut e, &request, 503, "unavailable");
        actions.extend(prepared_now(&mut e).into_iter());
        assert_eq!(
            owned_partial(&actions, command)[0]["id"],
            command.to_string()
        );
    }
}
#[test]
fn owned_history_full_inclusive_stall_preserves_accepted_rows() {
    let (mut e, _) = ready("disabled");
    e.set_log_level(safe_cex_engine::Level::Error);
    query(&mut e, owned_fills_query(None));
    let request = read(&mut e);
    reply(&mut e, &request, 200, &format!("[{}]", fill(1)));
    let request = read(&mut e);
    reply(
        &mut e,
        &request,
        200,
        &format!("[{}]", vec![fill(1); 2000].join(",")),
    );
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let rows = owned_partial(&actions, 1);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn owned_history_page_cap_keeps_page_200_but_no_request_201() {
    for positive_pages in [199_u32, 200] {
        let (mut e, _) = ready("disabled");
        e.set_log_level(safe_cex_engine::Level::Error);
        query(&mut e, owned_fills_query(None));
        let mut now = 5000.;
        let mut request = read_at(&mut e, &mut now);
        for page in 1..=positive_pages {
            let body = format!("[{}]", fill(u64::from(page)));
            now += 1.;
            e.handle(
                now,
                Event::HttpResponse {
                    request: request.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body: &body,
                    },
                },
            );
            if page == 200 {
                break;
            }
            request = read_at(&mut e, &mut now);
            let sent: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            assert_eq!(
                sent["startTime"], 1000,
                "same-time native identities must advance without changing inclusive bound"
            );
        }
        if positive_pages == 199 {
            reply_at(&mut e, now, &request, 200, "[]");
            let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
            assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
            let data = actions
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandDone { result, .. }) => Some(result),
                    _ => None,
                })
                .unwrap();
            assert_eq!(serde_json::from_str::<Vec<Value>>(data).unwrap().len(), 199);
        } else {
            let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
            assert_eq!(owned_partial(&actions, 1).as_array().unwrap().len(), 200);
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
}

#[test]
fn owned_history_zero_income_rows_cannot_authorize_partial_recovery() {
    for kind in [None, Some("REALIZED_PNL")] {
        let (mut e, _) = ready("disabled");
        query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: kind.map(str::to_owned),
                from_ms: Some(1.0),
                to_ms: Some(5000.0),
                limit: None,
            },
        );
        let request = read(&mut e);
        let zero = fill(1)
            .replace("\"closedPnl\":\"2\"", "\"closedPnl\":\"0\"")
            .replace("\"fee\":\"-0.01\"", "\"fee\":\"0\"");
        reply(&mut e, &request, 200, &format!("[{zero}]"));
        let request = read(&mut e);
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["startTime"], 1000);
        if kind.is_none() {
            reply(&mut e, &request, 200, "[]");
            let funding = read(&mut e);
            assert_eq!(support::kind(&funding), "userFunding");
            reply(&mut e, &funding, 503, "unavailable");
        } else {
            reply(&mut e, &request, 503, "unavailable");
        }
        owned_first_failure(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), 1);
    }
}
#[test]
fn owned_history_income_keeps_the_first_gap_through_funding_failure() {
    let (mut e, _) = ready("disabled");
    e.set_log_level(safe_cex_engine::Level::Error);
    query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let mut now = 5000.;
    let mut request = read_at(&mut e, &mut now);
    for page in 1..=200_u32 {
        let body = format!("[{}]", fill(u64::from(page)));
        now += 1.;
        e.handle(
            now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        request = read_at(&mut e, &mut now);
        assert_eq!(
            kind(&request),
            if page == 200 {
                "userFunding"
            } else {
                "userFillsByTime"
            }
        );
    }
    reply_at(&mut e, now, &request, 503, "later funding failure");
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    assert_eq!(owned_partial(&actions, 1).as_array().unwrap().len(), 400);
    let message = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(event.message.as_str())
            }
            _ => None,
        })
        .unwrap();
    assert!(
        message.contains("page cap"),
        "the original gap must survive a later error: {message}"
    );
}
#[test]
fn owned_history_order_transition_fold_precedes_filters_and_keeps_first_equal_rank() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTC-PERP".into()),
            from_ms: Some(1500.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let request = read(&mut e);
    let row = |coin: &str, created: u64| {
        format!(
            r#"{{"order":{{"coin":"{coin}","oid":9,"side":"B","limitPx":"90","origSz":"2","sz":"0","orderType":"Limit","timestamp":{created}}},"status":"filled","statusTimestamp":2500}}"#
        )
    };
    reply(
        &mut e,
        &request,
        200,
        &format!("[{},{}]", row("ETH", 1000), row("BTC", 2000)),
    );
    let rows = result(&mut e);
    assert!(
        rows.as_array().unwrap().is_empty(),
        "equal time and rank keep the first transition before symbol/time filters"
    );
}

#[test]
fn owned_history_zero_income_page_still_advances_to_later_eligible_rows() {
    let (mut e, _) = ready("disabled");
    query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: Some("REALIZED_PNL".into()),
            from_ms: Some(1.0),
            to_ms: Some(5000.0),
            limit: None,
        },
    );
    let request = read(&mut e);
    let zero = fill(1)
        .replace("\"closedPnl\":\"2\"", "\"closedPnl\":\"0\"")
        .replace("\"fee\":\"-0.01\"", "\"fee\":\"0\"");
    reply(&mut e, &request, 200, &format!("[{zero}]"));
    let request = read(&mut e);
    reply(&mut e, &request, 200, &format!("[{}]", fill(2)));
    let request = read(&mut e);
    reply(&mut e, &request, 200, "[]");
    let rows = result(&mut e);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["tradeId"], "2");
    assert_eq!(rows[0]["income"], 2.0);
}
#[test]
fn owned_history_full_inclusive_gap_survives_both_funding_outcomes() {
    for funding_fails in [false, true] {
        let (mut e, _) = ready("disabled");
        e.set_log_level(safe_cex_engine::Level::Error);
        query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(1.0),
                to_ms: Some(5000.0),
                limit: None,
            },
        );
        let request = read(&mut e);
        reply(&mut e, &request, 200, &format!("[{}]", fill(1)));
        let request = read(&mut e);
        reply(
            &mut e,
            &request,
            200,
            &format!("[{}]", vec![fill(1); 2000].join(",")),
        );
        let funding = read(&mut e);
        assert_eq!(kind(&funding), "userFunding");
        if funding_fails {
            reply(&mut e, &funding, 503, "later funding failure");
        } else {
            reply(
                &mut e,
                &funding,
                200,
                r#"[{"time":2000,"hash":"fund","delta":{"coin":"BTC","usdc":"-0.2"}}]"#,
            );
            let final_probe = read(&mut e);
            reply(&mut e, &final_probe, 200, "[]");
        }
        let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
        let rows = owned_partial(&actions, 1);
        assert_eq!(
            rows.as_array().unwrap().len(),
            if funding_fails { 2 } else { 3 }
        );
        let event = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
                {
                    Some(event)
                }
                _ => None,
            })
            .unwrap();
        assert!(event.message.contains("full inclusive page"));
    }
}
