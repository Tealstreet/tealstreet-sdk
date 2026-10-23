mod support;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Level, Notice, Query, Response, Value};
use support::*;
fn query(h: &mut Harness, command: u32, query: Query) {
    h.ws("pong");
    h.e.handle(1.0, Event::Command(Command::Query { command, query }));
    h.collect_at(1.0);
}
fn income() -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn harness() -> Harness {
    let mut h = Harness::new();
    h.seed();
    h.e.set_log_level(Level::Error);
    h.actions.clear();
    h
}
fn row(id: u32, fee: bool) -> String {
    format!(
        r#"{{"instId":"BTC-USDT","tradeId":"{id}","orderId":"12","side":"buy","fillPrice":"60000","fillSize":"1","fillPnl":"1","fee":"{}","ts":"1700000000000"}}"#,
        if fee { "0.2" } else { "0" }
    )
}
fn page(rows: &[String]) -> String {
    format!(r#"{{"code":"0","data":[{}]}}"#, rows.join(","))
}
fn full() -> String {
    page(
        &(901..=1000)
            .rev()
            .map(|id| row(id, false))
            .collect::<Vec<_>>(),
    )
}
fn outcome(h: &Harness, command: u32) -> Option<Result<&str, &safe_cex_engine::VenueError>> {
    h.actions.iter().rev().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
            Some(Ok(result.as_str()))
        }
        Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
            Some(Err(error))
        }
        _ => None,
    })
}
fn markers(h: &Harness, command: u32) -> Vec<&safe_cex_engine::log::LogEvent> {
    h.actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(e))
                if e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                    && e.fields
                        .contains(&("command", Value::Num(f64::from(command)))) =>
            {
                Some(e)
            }
            _ => None,
        })
        .collect()
}
fn timeline(h: &Harness, command: u32) -> Vec<&'static str> {
    h.actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: c, .. }) if *c == command => Some("done"),
            Action::Notice(Notice::CommandFailed { command: c, .. }) if *c == command => {
                Some("failed")
            }
            Action::Notice(Notice::Event(e))
                if e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                    && e.fields
                        .contains(&("command", Value::Num(f64::from(command)))) =>
            {
                Some("incomplete")
            }
            _ => None,
        })
        .collect()
}
fn typed_failure(h: &Harness, command: u32) {
    let error = outcome(h, command)
        .expect("command settled")
        .expect_err("must reject");
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert!(markers(h, command).is_empty());
    assert_eq!(timeline(h, command), ["failed"]);
}
fn partial(h: &Harness, command: u32, count: usize) -> Vec<serde_json::Value> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(
        outcome(h, command)
            .expect("settled")
            .expect("partial success"),
    )
    .unwrap();
    assert_eq!(rows.len(), count);
    let e = markers(h, command);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].level, Level::Warn);
    assert_eq!(timeline(h, command), ["incomplete", "done"]);
    rows
}
fn fail(h: &mut Harness, request: safe_cex_engine::HttpRequest, kind: u8) {
    match kind {
        0 => h.respond(request, r#"{"code":"152401","msg":"denied","data":[]}"#),
        1 => {
            h.e.handle(
                1.0,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: r#"{"code":"0","data":[]}"#,
                    },
                },
            );
            h.collect();
        }
        2 => {
            h.e.handle(
                1.0,
                Event::HttpFailed {
                    request: request.id,
                    error: "network lost",
                },
            );
            h.collect();
        }
        3 => h.respond(
            request,
            &format!(r#"{{"code":"0","data":[{},{{}}]}}"#, row(1, false)),
        ),
        4 => h.respond(
            request,
            &format!(
                r#"{{"data":[{}],"code":"152401","msg":"native after prefix"}}"#,
                row(1, false)
            ),
        ),
        5 => h.respond(
            request,
            &format!(r#"{{"code":"0","data":[{}]}} trailing"#, row(1, false)),
        ),
        6 => h.respond(request, r#"{"code":"0"}"#),
        7 => h.respond(request, r#"{"code":"0","data":null}"#),
        _ => h.respond(request, r#"{"code":"0","data":{}}"#),
    }
}
#[test]
fn general_history_first_failures_are_typed_and_never_accept_streamed_prefixes() {
    for operation in 0..4 {
        for kind in 0..9 {
            let mut h = harness();
            let (q, path) = match operation {
                0 => (income(), "fills-history"),
                1 => (
                    Query::Fills {
                        symbol: Some("BTCUSDT".into()),
                        order_id: None,
                        from_ms: None,
                        to_ms: None,
                        limit: None,
                    },
                    "fills-history",
                ),
                2 => (
                    Query::OrderHistory {
                        symbol: Some("BTCUSDT".into()),
                        from_ms: None,
                        to_ms: None,
                        limit: None,
                    },
                    "orders-history",
                ),
                _ => (
                    Query::PublicTrades {
                        symbol: "BTCUSDT".into(),
                        limit: None,
                    },
                    "market/trades",
                ),
            };
            query(&mut h, 7, q);
            let r = h.take(path);
            fail(&mut h, r, kind);
            if operation == 3 && kind == 2 {
                let error = outcome(&h, 7)
                    .expect("command settled")
                    .expect_err("must reject");
                assert_eq!(error.kind, Some(ErrorKind::Network));
                assert!(markers(&h, 7).is_empty());
                assert_eq!(timeline(&h, 7), ["failed"]);
            } else {
                typed_failure(&h, 7);
            }
        }
    }
}
#[test]
fn later_income_failures_keep_only_accepted_rows_with_owned_error_only_marker() {
    for kind in 0..9 {
        let mut h = harness();
        query(&mut h, 7, income());
        h.reply("fills-history", &full());
        let r = h.take("fills-history");
        assert!(r.url.ends_with("after=901"));
        fail(&mut h, r, kind);
        let rows = partial(&h, 7, 100);
        assert!(!rows.iter().any(|r| r["tradeId"] == "1"));
        assert!(!h.pending.iter().any(|r| r.url.contains("fills-history")));
    }
}
#[test]
fn filtered_income_pages_do_not_enable_recovery() {
    let mut h = harness();
    query(
        &mut h,
        7,
        Query::Income {
            kind: Some("COMMISSION".into()),
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    h.reply("fills-history", &full());
    let r = h.take("fills-history");
    fail(&mut h, r, 0);
    typed_failure(&h, 7);
}
#[test]
fn native_trade_id_dedupe_keeps_both_pnl_and_fee_across_recovered_page() {
    let mut h = harness();
    query(&mut h, 7, income());
    h.reply(
        "fills-history",
        &page(
            &(901..=1000)
                .rev()
                .map(|id| row(id, true))
                .collect::<Vec<_>>(),
        ),
    );
    let r = h.take("fills-history");
    fail(&mut h, r, 3);
    let rows = partial(&h, 7, 200);
    assert_eq!(rows[0]["tranId"], "1000:pnl");
    assert_eq!(rows[1]["tranId"], "1000:fee");
    assert_eq!(rows[1]["income"], -0.2);
}
#[test]
fn repeated_or_missing_cursor_remains_strict_after_accepted_rows() {
    for missing in [false, true] {
        let mut h = harness();
        query(&mut h, 7, income());
        h.reply("fills-history", &full());
        let body = if missing {
            page(
                &(0..100)
                    .map(|_| row(0, false).replace("\"tradeId\":\"0\"", "\"tradeId\":\"\""))
                    .collect::<Vec<_>>(),
            )
        } else {
            full()
        };
        h.reply("fills-history", &body);
        typed_failure(&h, 7);
    }
}
#[test]
fn header_preparation_refusal_is_strict_after_accepted_rows() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let refuse = Arc::new(AtomicBool::new(false));
    let gate = refuse.clone();
    let mut h = Harness::with_signing_entropy(Box::new(move |bytes| {
        if gate.load(Ordering::Relaxed) {
            Err("test signing entropy unavailable".into())
        } else {
            bytes.fill(1);
            Ok(())
        }
    }));
    h.seed();
    h.actions.clear();
    query(&mut h, 7, income());
    refuse.store(true, Ordering::Relaxed);
    h.reply("fills-history", &full());
    typed_failure(&h, 7);
    assert!(!h.pending.iter().any(|r| r.url.contains("fills-history")));
}
#[test]
fn simultaneous_history_owners_do_not_leak_completeness() {
    let mut h = harness();
    query(&mut h, 7, income());
    query(&mut h, 8, income());
    let first = h.take("fills-history");
    let other = h.take("fills-history");
    h.respond(first, &full());
    fail(&mut h, other, 0);
    let next = h.take("fills-history");
    fail(&mut h, next, 1);
    typed_failure(&h, 8);
    partial(&h, 7, 100);
}

#[test]
fn actual_page_budget_remains_strict_after_all_admitted_pages() {
    let mut h = harness();
    query(&mut h, 7, income());
    for index in 0..100 {
        let rows = (1..=100)
            .rev()
            .map(|id| row(10000 - index * 100 + id, false))
            .collect::<Vec<_>>();
        h.reply("fills-history", &page(&rows));
        if index < 99 {
            assert!(outcome(&h, 7).is_none());
        }
    }
    typed_failure(&h, 7);
    assert!(!h.pending.iter().any(|r| r.url.contains("fills-history")));
}

#[test]
fn native_admission_facts_have_explicit_user_error_kinds_before_io() {
    for (query_value, expected) in [
        (
            Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "client".into(),
            },
            ErrorKind::Unsupported,
        ),
        (
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: None,
            },
            ErrorKind::InvalidInput,
        ),
        (
            Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(2.0),
                to_ms: Some(1.0),
                limit: None,
            },
            ErrorKind::InvalidInput,
        ),
        (
            Query::PublicTrades {
                symbol: "UNKNOWNUSDT".into(),
                limit: None,
            },
            ErrorKind::InvalidInput,
        ),
    ] {
        let mut h = harness();
        let before = h.pending.len();
        query(&mut h, 7, query_value);
        let error = outcome(&h, 7)
            .expect("admission settles")
            .expect_err("admission rejects");
        assert_eq!(error.kind, Some(expected));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert_eq!(h.pending.len(), before);
        assert!(markers(&h, 7).is_empty());
        assert_eq!(timeline(&h, 7), ["failed"]);
    }
}
