mod support;
use safe_cex_engine::log::code;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Level, Notice, Query, Value,
};
use support::*;
fn done(h: &Harness, command: u32) -> Result<&str, &str> {
    h.actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                Some(Ok(result.as_str()))
            }
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(Err(error.message.as_str()))
            }
            _ => None,
        })
        .unwrap()
}
fn fill(trade: u64, nanos: u64) -> String {
    format!(
        r#"{{"trade_id":{trade},"order_id":9007199254740993,"instrument_name":"BTCUSD-PERP","side":"BUY","traded_price":"100","traded_quantity":"0.1","fees":"-0.02","fee_instrument_name":"USD","taker_side":"MAKER","create_time":1610000000000,"create_time_ns":{nanos}}}"#
    )
}
fn query(h: &mut Harness, query: Query) {
    h.event(Event::Command(Command::Query { command: 1, query }));
}
fn fills(limit: u32) -> Query {
    Query::Fills {
        symbol: Some("BTCUSD-PERP".into()),
        order_id: None,
        from_ms: Some(1609999999000.0),
        to_ms: Some(1610000001000.0),
        limit: Some(limit),
    }
}
fn page_timer(h: &mut Harness) {
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 1000.0))
        .unwrap();
    let Action::SetTimer { timer, at_ms } = h.actions.remove(i) else {
        unreachable!()
    };
    h.now = at_ms;
    h.event(Event::TimerFired { timer });
}
#[test]
fn fills_preserve_large_ids_and_fee_sign_and_seconds() {
    let mut h = Harness::new();
    h.load();
    query(&mut h, fills(1));
    h.reply(
        "private/get-trades",
        &format!(
            r#"{{"code":0,"result":{{"data":[{}]}}}}"#,
            fill(9007199254740997, 1610000000000000001)
        ),
    );
    let result = done(&h, 1).unwrap();
    assert!(result.contains(r#""id":"9007199254740997""#));
    assert!(result.contains(r#""commission":0.02"#));
    assert!(result.contains(r#""timestamp":1610000000.0"#));
}
#[test]
fn private_history_pages_overlap_exact_nanosecond_boundary_and_deduplicate() {
    let mut h = Harness::new();
    h.load();
    query(&mut h, fills(101));
    let rows: Vec<_> = (1..=100)
        .map(|n| fill(n, 1610000000000000000 + n))
        .collect();
    h.reply(
        "private/get-trades",
        &format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, rows.join(",")),
    );
    page_timer(&mut h);
    let request = h.http("private/get-trades");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains(r#""end_time":1610000000000000002"#)
    );
    h.respond(
        &request,
        &format!(
            r#"{{"code":0,"result":{{"data":[{},{}]}}}}"#,
            fill(1, 1610000000000000001),
            fill(101, 1610000000000000000)
        ),
    );
    assert_eq!(
        done(&h, 1).unwrap().matches(r#""commission":"#).count(),
        101
    );
}
#[test]
fn failed_streamed_page_is_discarded_before_read_retry() {
    let mut h = Harness::new();
    h.load();
    query(&mut h, fills(1));
    let request = h.http("private/get-trades");
    h.event(Event::HttpStart {
        request: request.id,
        status: 200,
        headers: "",
    });
    let partial = format!(
        r#"{{"code":0,"result":{{"data":[{},"#,
        fill(9, 1610000000000000001)
    );
    h.event(Event::HttpChunk {
        request: request.id,
        data: partial.as_bytes(),
    });
    h.event(Event::HttpFailed {
        request: request.id,
        error: "reset during page",
    });
    page_timer(&mut h);
    h.reply(
        "private/get-trades",
        &format!(
            r#"{{"code":0,"result":{{"data":[{}]}}}}"#,
            fill(9, 1610000000000000001)
        ),
    );
    assert_eq!(done(&h, 1).unwrap().matches(r#""commission":"#).count(), 1);
}
#[test]
fn order_history_and_income_use_existing_json_shapes() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(0.0),
            to_ms: Some(100.0),
            limit: Some(1),
        },
    );
    h.reply(
        "private/get-order-history",
        &format!(
            r#"{{"code":0,"result":{{"data":[{}]}}}}"#,
            ORDER.replace("ACTIVE", "CANCELED").replace(
                "\"update_time\":1",
                "\"create_time\":1,\"update_time\":2,\"avg_price\":\"90\""
            )
        ),
    );
    let result = done(&h, 1).unwrap();
    assert!(result.contains(r#""createdTime":1.0"#));
    assert!(result.contains(r#""status":"canceled""#));
    h.actions.clear();
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.0),
            to_ms: Some(100.0),
            limit: None,
        },
    );
    h.reply("private/get-transactions", r#"{"code":0,"result":{"data":[{"journal_id":9007199254740995,"instrument_name":"BTCUSD-PERP","journal_type":"FUNDING","transaction_qty":"-1.25","event_timestamp_ms":1,"event_timestamp_ns":1000000}]}}"#);
    page_timer(&mut h);
    h.reply("private/get-transactions", EMPTY);
    let result = done(&h, 1).unwrap();
    assert!(result.contains(r#""category":"funding""#));
    assert!(result.contains(r#""asset":"USD""#));
    assert!(result.contains(r#""income":-1.25"#));
}
#[test]
fn public_trade_read_uses_get_and_honors_limit() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::PublicTrades {
            symbol: "BTCUSD-PERP".into(),
            limit: Some(1),
        },
    );
    let request = h.http("public/get-trades");
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert!(request.url.ends_with("count=1"));
    h.respond(&request, r#"{"code":0,"result":{"data":[{"d":"123","i":"BTCUSD-PERP","p":"100","q":"2","s":"SELL","t":1610000000000}]}}"#);
    assert!(done(&h, 1).unwrap().contains(r#""quoteQty":200.0"#));
}

#[test]
fn full_pages_without_exact_nanosecond_cursors_fail_closed() {
    for suffix in ["", r#","create_time_ns":"bad""#] {
        let mut h = Harness::new();
        h.load();
        query(&mut h, fills(101));
        let rows: Vec<_> = (1..=100).map(|n| format!(r#"{{"trade_id":{n},"order_id":"1","instrument_name":"BTCUSD-PERP","side":"BUY","traded_price":"100","traded_quantity":"1","create_time":1610000000000{suffix}}}"#)).collect();
        h.reply(
            "private/get-trades",
            &format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, rows.join(",")),
        );
        assert!(done(&h, 1).unwrap_err().contains("nanosecond"));
    }
}

#[test]
fn ranged_income_walks_short_pages_and_preserves_session_settlement_pnl() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.0),
            to_ms: Some(1000.0),
            limit: None,
        },
    );
    let row = |id, ns| {
        format!(
            r#"{{"journal_id":"{id}","instrument_name":"BTCUSD-PERP","journal_type":"SESSION_SETTLE","transaction_qty":"0","realized_pnl":"-0.007800","event_timestamp_ms":1,"event_timestamp_ns":{ns}}}"#
        )
    };
    let rows: Vec<_> = (1..=100).map(|n| row(n, 1000000 + n)).collect();
    h.reply(
        "private/get-transactions",
        &format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, rows.join(",")),
    );
    page_timer(&mut h);
    h.reply(
        "private/get-transactions",
        &format!(
            r#"{{"code":0,"result":{{"data":[{},{}]}}}}"#,
            row(1, 1000001),
            row(101, 1000000)
        ),
    );
    page_timer(&mut h);
    h.reply("private/get-transactions", EMPTY);
    let result = done(&h, 1).unwrap();
    assert_eq!(result.matches(r#""income":-0.0078"#).count(), 101);
    assert!(result.contains(r#""category":"realized_pnl""#));
}
#[test]
fn stop_limit_history_separates_requested_price_from_trigger_and_omits_missing_average() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(0.0),
            to_ms: Some(100.0),
            limit: Some(1),
        },
    );
    let row = ORDER.replace("LIMIT", "STOP_LIMIT").replace(
        r#""limit_price":"90""#,
        r#""limit_price":"95","ref_price":"100","create_time":1"#,
    );
    h.reply(
        "private/get-order-history",
        &format!(r#"{{"code":0,"result":{{"data":[{row}]}}}}"#),
    );
    let result = done(&h, 1).unwrap();
    assert!(result.contains(r#""price":95.0"#));
    assert!(!result.contains("averagePrice"));
}

#[test]
fn fractional_millisecond_bounds_remain_exact_and_overflow_rejects() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1610000000000.5),
            to_ms: Some(1610000000001.5),
            limit: Some(1),
        },
    );
    let request = h.http("private/get-trades");
    let body = request.body.as_deref().unwrap();
    assert!(body.contains(r#""start_time":1610000000000500000"#));
    assert!(body.contains(r#""end_time":1610000000001500000"#));
    h.respond(
        &request,
        &format!(
            r#"{{"code":0,"result":{{"data":[{},{},{}]}}}}"#,
            fill(1, 1610000000000499999),
            fill(2, 1610000000000500000),
            fill(3, 1610000000001500000)
        ),
    );
    let result = done(&h, 1).unwrap();
    assert!(result.contains(r#""id":"2""#));
    assert!(!result.contains(r#""id":"1""#));
    h.event(Event::Command(Command::Query {
        command: 2,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1e20),
            to_ms: Some(1e20),
            limit: None,
        },
    }));
    assert!(done(&h, 2).is_err());
}

#[test]
fn concurrent_history_reads_share_method_limits_but_not_other_methods() {
    let mut h = Harness::new();
    h.load();
    query(&mut h, fills(1));
    h.event(Event::Command(Command::Query {
        command: 2,
        query: fills(1),
    }));
    h.event(Event::Command(Command::Query {
        command: 3,
        query: Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    }));
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("private/get-trades")))
            .count(),
        1
    );
    assert_eq!(
        h.actions
            .iter()
            .filter(
                |a| matches!(a, Action::Http(r) if r.url.ends_with("private/get-order-history"))
            )
            .count(),
        1
    );
    page_timer(&mut h);
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("private/get-trades")))
            .count(),
        2
    );
}

#[test]
fn documented_deposit_withdrawal_and_rollback_variants_are_transfers() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.0),
            to_ms: Some(1000.0),
            limit: Some(4),
        },
    );
    let rows = ["DEPOSIT", "WITHDRAW", "ROLLBACK_DEPOSIT", "ROLLBACK_WITHDRAW"].iter().enumerate()
        .map(|(i, kind)| format!(r#"{{"journal_id":"{i}","instrument_name":"USD","journal_type":"{kind}","transaction_qty":"-100","event_timestamp_ms":1,"event_timestamp_ns":1000000}}"#))
        .collect::<Vec<_>>().join(",");
    h.reply(
        "private/get-transactions",
        &format!(r#"{{"code":0,"result":{{"data":[{rows}]}}}}"#),
    );
    page_timer(&mut h);
    h.reply("private/get-transactions", EMPTY);
    let result = done(&h, 1).unwrap();
    assert_eq!(result.matches(r#""category":"transfer""#).count(), 4);
    assert_eq!(result.matches(r#""income":-100.0"#).count(), 4);
}
#[test]
fn null_optional_fill_fields_do_not_fail_history() {
    let mut h = Harness::new();
    h.load();
    query(&mut h, fills(1));
    let row = fill(1, 1610000000000000001)
        .replace(r#""-0.02""#, "null")
        .replace(r#""USD""#, "null")
        .replace(r#""MAKER""#, "null");
    h.reply(
        "private/get-trades",
        &format!(r#"{{"code":0,"result":{{"data":[{row}]}}}}"#),
    );
    assert!(done(&h, 1).unwrap().contains(r#""id":"1""#));
}

fn owned_notices(h: &Harness, command: u32) -> Vec<&Notice> {
    h.actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(notice) => match notice {
                Notice::Event(event)
                    if event.code == code::HISTORY_INCOMPLETE
                        && event.fields == vec![("command", Value::Num(f64::from(command)))] =>
                {
                    Some(notice)
                }
                Notice::CommandDone { command: id, .. }
                | Notice::CommandFailed { command: id, .. }
                    if *id == command =>
                {
                    Some(notice)
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn accept_one_page(h: &mut Harness, command: u32, query: Query) -> HttpRequest {
    h.event(Event::Command(Command::Query { command, query }));
    h.reply(
        "private/get-trades",
        &format!(
            r#"{{"code":0,"result":{{"data":[{}]}}}}"#,
            fill(1, 1610000000000000001)
        ),
    );
    page_timer(h);
    h.http("private/get-trades")
}

#[test]
fn first_native_history_failure_rejects_typed_without_incomplete_success() {
    let mut h = Harness::new();
    h.load();
    h.engine.set_log_level(Level::Error);
    query(&mut h, fills(2));
    h.reply(
        "private/get-trades",
        r#"{"code":999,"message":"fixture unavailable","result":{"data":[]}}"#,
    );
    let notices = owned_notices(&h, 1);
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(notices[0], Notice::CommandFailed { error, .. } if error.kind == Some(ErrorKind::Internal))
    );
}

#[test]
fn later_native_or_malformed_page_returns_only_previously_accepted_rows() {
    for malformed in [false, true] {
        let mut h = Harness::new();
        h.load();
        h.engine.set_log_level(Level::Error);
        let orders = h.engine.store().orders().to_vec();
        let request = accept_one_page(&mut h, 1, fills(3));
        if malformed {
            h.respond(
                &request,
                &format!(
                    r#"{{"code":0,"result":{{"data":[{},{{"invalid":true}}]}}}}"#,
                    fill(2, 1610000000000000000)
                ),
            );
        } else {
            h.respond(
                &request,
                r#"{"code":999,"message":"fixture unavailable","result":{"data":[]}}"#,
            );
        }
        let notices = owned_notices(&h, 1);
        assert_eq!(notices.len(), 2);
        assert!(matches!(notices[0], Notice::Event(event) if event.level == Level::Warn));
        let Notice::CommandDone { result, .. } = notices[1] else {
            panic!("accepted rows resolve")
        };
        let rows: Vec<serde_json::Value> = serde_json::from_str(result).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "1");
        assert_eq!(h.engine.store().orders(), orders);
    }
}

#[test]
fn filtered_native_rows_cannot_enable_empty_partial_success() {
    let mut h = Harness::new();
    h.load();
    h.engine.set_log_level(Level::Error);
    let mut q = fills(3);
    if let Query::Fills { order_id, .. } = &mut q {
        *order_id = Some("other-order".into());
    }
    let request = accept_one_page(&mut h, 1, q);
    h.respond(
        &request,
        r#"{"code":999,"message":"fixture unavailable","result":{"data":[]}}"#,
    );
    let notices = owned_notices(&h, 1);
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(notices[0], Notice::CommandFailed { error, .. } if error.kind == Some(ErrorKind::Internal))
    );
}

#[test]
fn exhausted_transport_retries_drop_each_failed_prefix_but_keep_accepted_output() {
    let mut h = Harness::new();
    h.load();
    h.engine.set_log_level(Level::Error);
    let mut request = accept_one_page(&mut h, 1, fills(3));
    let params = serde_json::from_str::<serde_json::Value>(request.body.as_deref().unwrap())
        .unwrap()["params"]
        .clone();
    for attempt in 0..4 {
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(request.body.as_deref().unwrap()).unwrap()["params"],
            params
        );
        h.event(Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        });
        let prefix = format!(
            r#"{{"code":0,"result":{{"data":[{},"#,
            fill(2, 1610000000000000000)
        );
        h.event(Event::HttpChunk {
            request: request.id,
            data: prefix.as_bytes(),
        });
        h.event(Event::HttpFailed {
            request: request.id,
            error: "fixture network failure",
        });
        if attempt < 3 {
            assert!(owned_notices(&h, 1).is_empty());
            page_timer(&mut h);
            request = h.http("private/get-trades");
        }
    }
    let notices = owned_notices(&h, 1);
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Notice::Event(_)));
    let Notice::CommandDone { result, .. } = notices[1] else {
        panic!("earlier accepted output survives")
    };
    let rows: Vec<serde_json::Value> = serde_json::from_str(result).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "1");
}

#[test]
fn concurrent_query_failure_markers_remain_owned_and_precede_their_results() {
    let mut h = Harness::new();
    h.load();
    h.engine.set_log_level(Level::Error);
    let first = accept_one_page(&mut h, 1, fills(3));
    h.event(Event::Command(Command::Query {
        command: 2,
        query: fills(3),
    }));
    page_timer(&mut h);
    h.reply(
        "private/get-trades",
        &format!(
            r#"{{"code":0,"result":{{"data":[{}]}}}}"#,
            fill(11, 1610000000000000001)
        ),
    );
    page_timer(&mut h);
    let second = h.http("private/get-trades");
    for (command, request) in [(2, second), (1, first)] {
        h.respond(
            &request,
            r#"{"code":999,"message":"fixture unavailable","result":{"data":[]}}"#,
        );
        let notices = owned_notices(&h, command);
        assert_eq!(notices.len(), 2);
        assert!(matches!(notices[0], Notice::Event(_)));
        assert!(matches!(notices[1], Notice::CommandDone { .. }));
    }
}

#[test]
fn later_exact_cursor_proof_failures_still_reject_every_prior_row() {
    for missing in [false, true] {
        let mut h = Harness::new();
        h.load();
        let request = accept_one_page(&mut h, 1, fills(3));
        let mut row =
            serde_json::from_str::<serde_json::Value>(&fill(2, 1610000000000000001)).unwrap();
        if missing {
            row.as_object_mut().unwrap().remove("create_time_ns");
        }
        h.respond(
            &request,
            &format!(r#"{{"code":0,"result":{{"data":[{row}]}}}}"#),
        );
        let notices = owned_notices(&h, 1);
        assert_eq!(notices.len(), 1);
        assert!(matches!(notices[0], Notice::CommandFailed { .. }));
    }
}

#[test]
fn short_duplicate_page_proves_exhaustion_before_exact_cursor_is_required() {
    let mut h = Harness::new();
    h.load();
    let request = accept_one_page(&mut h, 1, fills(3));
    let mut duplicate =
        serde_json::from_str::<serde_json::Value>(&fill(1, 1610000000000000001)).unwrap();
    duplicate.as_object_mut().unwrap().remove("create_time_ns");
    h.respond(
        &request,
        &format!(r#"{{"code":0,"result":{{"data":[{duplicate}]}}}}"#),
    );
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(done(&h, 1).unwrap())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(owned_notices(&h, 1).len(), 1);
}

#[test]
fn a_full_duplicate_page_can_advance_an_older_exact_native_bound() {
    let mut h = Harness::new();
    h.load();
    query(&mut h, fills(101));
    let rows: Vec<_> = (1..=100)
        .map(|id| fill(id, 1610000000000000000 + id))
        .collect();
    h.reply(
        "private/get-trades",
        &format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, rows.join(",")),
    );
    page_timer(&mut h);
    let rows: Vec<_> = (1..=100).map(|id| fill(id, 1610000000000000000)).collect();
    h.reply(
        "private/get-trades",
        &format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, rows.join(",")),
    );
    assert!(owned_notices(&h, 1).is_empty());
    page_timer(&mut h);
    let request = h.http("private/get-trades");
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["params"]["end_time"], 1610000000000000001_u64);
    h.respond(&request, EMPTY);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(done(&h, 1).unwrap())
            .unwrap()
            .len(),
        100
    );
    assert_eq!(owned_notices(&h, 1).len(), 1);
}

#[test]
fn transport_and_40101_share_one_retry_counter() {
    let mut h = Harness::new();
    h.load();
    let request = accept_one_page(&mut h, 1, fills(3));
    h.event(Event::HttpFailed {
        request: request.id,
        error: "fixture transport",
    });
    page_timer(&mut h);
    h.reply(
        "private/get-trades",
        r#"{"code":40101,"message":"fixture refusal","result":{"data":[]}}"#,
    );
    assert_eq!(owned_notices(&h, 1).len(), 2);
    assert!(done(&h, 1).is_ok());
    assert!(
        !h.actions
            .iter()
            .any(|action| matches!(action, Action::SetTimer {at_ms,..} if *at_ms==h.now+1000.))
    );
}

#[test]
fn retry_drops_only_new_page_ids_and_keeps_prior_accepted_overlap_deduplicated() {
    let mut h = Harness::new();
    h.load();
    let request = accept_one_page(&mut h, 1, fills(2));
    let old = fill(1, 1610000000000000001);
    let new = fill(2, 1610000000000000000);
    h.event(Event::HttpStart {
        request: request.id,
        status: 200,
        headers: "",
    });
    let prefix = format!(r#"{{"code":0,"result":{{"data":[{old},{new},"#);
    h.event(Event::HttpChunk {
        request: request.id,
        data: prefix.as_bytes(),
    });
    h.event(Event::HttpFailed {
        request: request.id,
        error: "fixture interrupted overlap",
    });
    page_timer(&mut h);
    h.reply(
        "private/get-trades",
        &format!(r#"{{"code":0,"result":{{"data":[{old},{new}]}}}}"#),
    );
    let rows: Vec<serde_json::Value> = serde_json::from_str(done(&h, 1).unwrap()).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
    assert_eq!(owned_notices(&h, 1).len(), 1);
}

#[test]
fn one_40101_retry_does_not_grant_a_fresh_transport_retry_budget() {
    let mut h = Harness::new();
    h.load();
    let request = accept_one_page(&mut h, 1, fills(3));
    h.respond(
        &request,
        r#"{"code":40101,"message":"fixture retry","result":{"data":[]}}"#,
    );
    for attempt in 0..3 {
        page_timer(&mut h);
        let request = h.http("private/get-trades");
        let before = h.actions.len();
        h.event(Event::HttpFailed {
            request: request.id,
            error: "fixture subsequent transport",
        });
        if attempt < 2 {
            assert!(owned_notices(&h, 1).is_empty());
        } else {
            assert!(
                !h.actions[before..]
                    .iter()
                    .any(|a| matches!(a, Action::SetTimer { .. }))
            );
        }
    }
    assert_eq!(owned_notices(&h, 1).len(), 2);
    assert!(done(&h, 1).is_ok());
}

#[test]
fn public_trade_duplicates_remain_in_native_order_without_another_page() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::PublicTrades {
            symbol: "BTCUSD-PERP".into(),
            limit: Some(2),
        },
    );
    let request = h.http("public/get-trades");
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert!(request.headers.is_empty());
    let row = r#"{"d":1,"i":"BTCUSD-PERP","p":"100","q":"1","s":"SELL","t":1610000000000}"#;
    h.respond(
        &request,
        &format!(r#"{{"code":0,"result":{{"data":[{row},{row}]}}}}"#),
    );
    let rows: Vec<serde_json::Value> = serde_json::from_str(done(&h, 1).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], rows[1]);
    assert_eq!(owned_notices(&h, 1).len(), 1);
}

#[test]
fn public_trade_native_refusal_keeps_the_existing_one_retry_and_unsigned_route() {
    let mut h = Harness::new();
    h.load();
    query(
        &mut h,
        Query::PublicTrades {
            symbol: "BTCUSD-PERP".into(),
            limit: Some(1),
        },
    );
    h.reply(
        "public/get-trades",
        r#"{"code":40101,"message":"fixture retry","result":{"data":[]}}"#,
    );
    assert!(owned_notices(&h, 1).is_empty());
    page_timer(&mut h);
    let request = h.http("public/get-trades");
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert!(request.headers.is_empty());
    assert!(request.body.is_none());
    h.respond(&request, r#"{"code":0,"result":{"data":[{"d":1,"i":"BTCUSD-PERP","p":"100","q":"1","s":"SELL","t":1610000000000}]}}"#);
    assert!(done(&h, 1).is_ok());
    assert_eq!(owned_notices(&h, 1).len(), 1);
}

#[test]
fn ranged_income_later_failure_preserves_native_session_pnl() {
    let mut h = Harness::new();
    h.load();
    h.engine.set_log_level(Level::Error);
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.),
            to_ms: Some(1000.),
            limit: Some(1),
        },
    );
    let row = r#"{"journal_id":"ledger-1","instrument_name":"BTCUSD-PERP","journal_type":"SESSION_SETTLE","transaction_qty":"0","realized_pnl":"-0.007800","event_timestamp_ms":1,"event_timestamp_ns":1000001}"#;
    let second = row
        .replace("ledger-1", "ledger-2")
        .replace("1000001", "1000002");
    h.reply(
        "private/get-transactions",
        &format!(r#"{{"code":0,"result":{{"data":[{row},{second}]}}}}"#),
    );
    page_timer(&mut h);
    h.reply(
        "private/get-transactions",
        r#"{"code":999,"message":"fixture unavailable","result":{"data":[]}}"#,
    );
    let notices = owned_notices(&h, 1);
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Notice::Event(_)));
    let rows: Vec<serde_json::Value> = serde_json::from_str(done(&h, 1).unwrap()).unwrap();
    assert_eq!(
        rows.len(),
        2,
        "explicitly ranged income preserves its uncapped native contract"
    );
    assert!(
        rows.iter()
            .all(|row| row["income"] == -0.0078 && row["category"] == "realized_pnl")
    );
}
