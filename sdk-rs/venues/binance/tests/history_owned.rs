mod support;
use safe_cex_engine::{Command, ErrorKind, Event, Level, Outcome, Query, Value};
use support::{Harness, Options};

const WEEK: f64 = 604_800_000.0;
const FILL: &str = r#"{"id":1,"symbol":"BTCUSDT","orderId":9,"price":"100","qty":"2","side":"BUY","time":1700000000123}"#;
const INCOME: &str = r#"{"tranId":1,"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"2","asset":"USDT","time":20}"#;
const ORDER: &str = r#"{"orderId":1,"symbol":"BTCUSDT","status":"FILLED","type":"LIMIT","side":"BUY","price":"100","origQty":"2","executedQty":"2","time":100}"#;
const TRADE: &str =
    r#"{"id":1,"price":"100","qty":"2","quoteQty":"200","time":100,"isBuyerMaker":false}"#;

fn harness() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.engine.set_log_level(Level::Error);
    h.requests.clear();
    h
}
fn start(h: &mut Harness, command: u32, query: Query) {
    h.event(Event::Command(Command::Query { command, query }));
}
fn fills(range: bool) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: range.then_some(0.0),
        to_ms: range.then_some(WEEK + 10.0),
        limit: Some(1),
    }
}
fn income(range: bool) -> Query {
    Query::Income {
        symbol: Some("BTCUSDT".into()),
        kind: None,
        from_ms: range.then_some(0.0),
        to_ms: range.then_some(100.0),
        limit: Some(1),
    }
}
fn markers(h: &Harness) -> Vec<&safe_cex_engine::log::LogEvent> {
    h.events
        .iter()
        .filter(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
        .collect()
}
fn typed_failure(h: &Harness, command: u32, kind: ErrorKind) {
    assert!(h.result(command).unwrap().is_err(), "{:?}", h.done);
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].0, command);
    assert_eq!(h.failures[0].1.kind, Some(kind));
    assert!(h.failures[0].1.venue_code.is_none());
    assert!(markers(h).is_empty());
    assert_eq!(h.command_notices, [(command, "failed")]);
    assert!(h.command_errors.is_empty());
}
fn partial(h: &Harness, command: u32, expected_count: usize) -> Vec<serde_json::Value> {
    assert!(h.failures.is_empty(), "{:?}", h.failures);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(command).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), expected_count);
    let events = markers(h);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::Warn);
    assert!(
        events[0]
            .fields
            .contains(&("command", Value::Num(f64::from(command))))
    );
    assert_eq!(
        h.command_notices,
        [(command, "incomplete"), (command, "done")]
    );
    assert!(h.command_errors.is_empty());
    rows
}

#[test]
fn first_pages_reject_instead_of_empty_or_failed_body_prefix_success() {
    for kind in 0..5 {
        for failure in 0..4 {
            let mut h = harness();
            let (query, path, row) = match kind {
                0 => (fills(false), "/userTrades", FILL),
                1 => (income(false), "/income", INCOME),
                2 => (income(true), "/income", INCOME),
                3 => (
                    Query::OrderHistory {
                        symbol: Some("BTCUSDT".into()),
                        from_ms: None,
                        to_ms: None,
                        limit: None,
                    },
                    "/allOrders",
                    ORDER,
                ),
                _ => (
                    Query::PublicTrades {
                        symbol: "BTCUSDT".into(),
                        limit: None,
                    },
                    "/trades",
                    TRADE,
                ),
            };
            start(&mut h, 91, query);
            let request = h.take(path);
            match failure {
                0 => h.reply(request.id, 200, "", r#"{"code":-1,"msg":"native failed"}"#),
                1 => h.reply(request.id, 503, "", "unavailable"),
                2 => h.event(Event::HttpFailed {
                    request: request.id,
                    error: "lost",
                }),
                _ => h.reply(request.id, 200, "", &format!("[{row},{{\"broken\":true}}]")),
            }
            typed_failure(
                &h,
                91,
                if failure == 2 {
                    ErrorKind::Network
                } else {
                    ErrorKind::Internal
                },
            );
            assert!(!h.requests.iter().any(|r| r.url.contains(path)));
            // Retired HTTP callbacks cannot publish a second completion.
            h.reply(request.id, 200, "", "[]");
            assert_eq!(h.done.len(), 1);
        }
    }
}

#[test]
fn later_fill_window_failure_retains_only_prior_rows_with_one_owned_marker() {
    for failure in 0..4 {
        let mut h = harness();
        start(&mut h, 92, fills(true));
        let first = h.take("/userTrades");
        assert!(first.url.contains("startTime=0&endTime=604800000"));
        assert!(first.url.contains("limit=1"));
        h.reply(first.id, 200, "", &format!("[{FILL}]"));
        let second = h.take("/userTrades");
        assert!(second.url.contains("startTime=604800001&endTime=604800010"));
        match failure {
            0 => h.reply(second.id, 400, "", r#"{"code":-1,"msg":"second failed"}"#),
            1 => h.reply(second.id, 503, "", "unavailable"),
            2 => h.event(Event::HttpFailed {
                request: second.id,
                error: "lost",
            }),
            _ => h.reply(
                second.id,
                200,
                "",
                &format!("[{},{{}}]", FILL.replace("\"id\":1", "\"id\":2")),
            ),
        }
        let rows = partial(&h, 92, 1);
        assert_eq!(rows[0]["id"], "1");
        assert_eq!(rows[0]["timestamp"], 1_700_000_000.0);
        assert!(!h.requests.iter().any(|r| r.url.contains("/userTrades")));
    }
}

#[test]
fn fills_keep_native_order_duplicate_ids_and_per_window_limit_without_global_cap() {
    let mut h = harness();
    start(&mut h, 93, fills(true));
    let r = h.take("/userTrades");
    h.reply(
        r.id,
        200,
        "",
        &format!("[{},{}]", FILL.replace("\"id\":1", "\"id\":2"), FILL),
    );
    let r = h.take("/userTrades");
    assert!(r.url.contains("limit=1"));
    h.reply(r.id, 200, "", &format!("[{FILL}]"));
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(93).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2", "1", "1"]
    );
    // Both limit-1 windows filled their page, so the rows are kept and the
    // result is marked incomplete once (history_saturation.rs; #2128 finding).
    assert_eq!(markers(&h).len(), 1);
    assert_eq!(h.command_notices, [(93, "incomplete"), (93, "done")]);
}

#[test]
fn ranged_income_later_failure_returns_selected_rows_and_excludes_bad_prefix() {
    for failure in 0..4 {
        let mut h = harness();
        start(&mut h, 94, income(true));
        let r = h.take("/income");
        h.reply(r.id, 200, "", &format!("[{INCOME}]"));
        let r = h.take("/income");
        assert!(r.url.contains("startTime=20&endTime=100"));
        match failure {
            0 => h.reply(r.id, 200, "", r#"{"code":-1,"msg":"later native failed"}"#),
            1 => h.reply(r.id, 503, "", "unavailable"),
            2 => h.event(Event::HttpFailed {
                request: r.id,
                error: "lost",
            }),
            _ => h.reply(
                r.id,
                200,
                "",
                &format!("[{},{{}}]", INCOME.replace("\"tranId\":1", "\"tranId\":2")),
            ),
        }
        let rows = partial(&h, 94, 1);
        assert_eq!(rows[0]["tranId"], "1");
    }
}

#[test]
fn successful_empty_or_excluded_range_pages_do_not_enable_failure_recovery() {
    for body in [
        "[]".to_owned(),
        format!("[{}]", INCOME.replace("\"time\":20", "\"time\":-1")),
    ] {
        let mut h = harness();
        start(
            &mut h,
            95,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(0.0),
                to_ms: Some(WEEK + 100.0),
                limit: None,
            },
        );
        let r = h.take("/income");
        h.reply(r.id, 200, "", &body);
        assert!(h.result(95).is_none());
        let (timer, at) = *h
            .timers
            .iter()
            .find(|(_, at)| *at == h.now + 2000.0)
            .expect("native chunk pacing");
        h.now = at;
        h.event(Event::TimerFired { timer });
        let r = h.take("/income");
        h.reply(r.id, 503, "", "unavailable");
        typed_failure(&h, 95, ErrorKind::Internal);
    }
}

#[test]
fn ranged_income_preserves_compound_identity_descending_output_and_chunk_pacing() {
    let mut h = harness();
    start(
        &mut h,
        96,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(0.0),
            to_ms: Some(WEEK + 100.0),
            limit: Some(1),
        },
    );
    let r = h.take("/income");
    assert!(r.url.contains("limit=1"));
    let commission = INCOME.replace("FUNDING_FEE", "COMMISSION");
    h.reply(r.id, 200, "", &format!("[{INCOME},{commission}]"));
    let r = h.take("/income");
    assert!(r.url.contains("startTime=20&endTime=604800000"));
    h.reply(r.id, 200, "", "[]");
    let r = h.take("/income");
    assert!(r.url.contains("startTime=0&endTime=20"));
    h.reply(r.id, 200, "", "[]");
    assert!(!h.requests.iter().any(|r| r.url.contains("/income")));
    let (timer, at) = *h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 2000.0)
        .unwrap();
    h.now = at;
    h.event(Event::TimerFired { timer });
    let r = h.take("/income");
    assert!(r.url.contains("startTime=604800000&endTime=604800100"));
    let last = INCOME
        .replace("\"time\":20", "\"time\":604800010")
        .replace("\"tranId\":1", "\"tranId\":3");
    h.reply(r.id, 200, "", &format!("[{last}]"));
    let r = h.take("/income");
    h.reply(r.id, 200, "", "[]");
    let r = h.take("/income");
    h.reply(r.id, 200, "", "[]");
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(96).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(
        rows.len(),
        3,
        "same tranId with different income type remains distinct"
    );
    assert_eq!(rows[0]["tranId"], "3");
    assert_eq!(rows[1]["incomeType"], "FUNDING_FEE");
    assert_eq!(rows[2]["incomeType"], "COMMISSION");
    assert!(markers(&h).is_empty());
}

#[test]
fn keyless_missing_symbol_and_unsigned_trade_paths_stay_no_private_io() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.requests.clear();
    for query in [
        fills(false),
        income(false),
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    ] {
        start(&mut h, 97, query);
        assert_eq!(h.result(97), Some(&Ok("[]".into())));
        assert!(h.requests.is_empty());
    }
    start(
        &mut h,
        98,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(2000),
        },
    );
    let r = h.take("/trades");
    assert!(r.url.contains("limit=1000"));
    assert!(!r.url.contains("signature"));
    assert!(r.headers.is_empty());
    h.reply(r.id, 200, "", &format!("[{TRADE}]"));
    assert!(h.result(98).unwrap().is_ok());
    let mut h = harness();
    for query in [
        Query::Fills {
            symbol: None,
            order_id: None,
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
        start(&mut h, 99, query);
        assert_eq!(h.result(99), Some(&Ok("[]".into())));
        assert!(h.requests.is_empty());
    }
}

#[test]
fn signing_failure_after_accepted_income_stays_typed_strict_and_cancels_timers() {
    use safe_cex_engine::signing::{
        Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Controlled {
        local: Provider,
        refuse: Arc<AtomicBool>,
        failure: SignError,
    }
    impl Signer for Controlled {
        fn sign(&self, r: SignRequest) -> SignFuture {
            self.local.sign(r)
        }
        fn immediate(&self) -> bool {
            true
        }
        fn sign_ready(&self, r: &SignRequest) -> Option<Result<SignResult, SignError>> {
            if self.refuse.load(Ordering::Relaxed) {
                Some(Err(self.failure))
            } else {
                self.local.sign_ready(r)
            }
        }
    }
    for (failure, expected_kind) in [
        (SignError::Refused, ErrorKind::InvalidInput),
        (SignError::TimedOut, ErrorKind::Timeout),
    ] {
        let (venue, local) = Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }
        .build();
        let refuse = Arc::new(AtomicBool::new(false));
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine
            .set_signer(Some(Arc::new(Controlled {
                local,
                refuse: refuse.clone(),
                failure,
            })))
            .unwrap();
        let mut h = Harness::from_engine(engine);
        h.requests.clear();
        start(&mut h, 100, income(true));
        let r = h.take("/income");
        refuse.store(true, Ordering::Relaxed);
        h.reply(r.id, 200, "", &format!("[{INCOME}]"));
        typed_failure(&h, 100, expected_kind);
        assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
        assert_eq!(h.failures[0].1.message, failure.to_string());
        assert!(!h.requests.iter().any(|r| r.url.contains("/income")));
        assert!(!h.timers.iter().any(|(_, at)| *at == h.now + 2000.0));
    }
}
