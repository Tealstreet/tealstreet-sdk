use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, Response};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};
use serde::Deserialize;

const FILL: &str = r#"{"transactTimeNs":1669407633999999999,"symbol":"BTCUSDT","currency":"USDT","side":"Sell","tradeType":"Trade","execQtyRq":"0.001","execPriceRp":"16600","execFeeRv":"0.00166","execID":"5c3d96e1-8874-53b6-b6e5-9dcc4d28b4ab","orderID":"fcdfeafa-ed68-45d4-b2bd-7bc27f2b2b0b"}"#;
const ORDER: &str = r#"{"actionTimeNs":1667562110213260743,"transactTimeNs":1667562110221077395,"orderId":"743fc923-cb01-4261-88d1-b35dba2cdac0","symbol":"BTCUSDT","side":"Buy","ordType":"Market","ordStatus":"Filled","priceRp":"21206.7","orderQtyRq":"0.001","cumQtyRq":"0.001","reduceOnly":false}"#;
fn engine(private: bool) -> Engine<PhemexPerpetuals> {
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        key: private.then(|| "key".into()),
        secret: private.then(|| "secret".into()),
        ..Options::default()
    }));
    e.store_mut().set_markets(vec![Market {
        id: "BTC/USDT:USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            price: 0.1,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}
fn actions(e: &mut Engine<PhemexPerpetuals>) -> Vec<Action> {
    e.drain_actions().collect()
}
fn query(e: &mut Engine<PhemexPerpetuals>, query: Query) -> Action {
    e.handle(
        1_710_000_000_000.0,
        Event::Command(Command::Query { command: 10, query }),
    );
    actions(e)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .expect("HTTP request")
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn result(e: &mut Engine<PhemexPerpetuals>) -> Result<String, String> {
    actions(e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 10,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("completion")
}
fn respond(e: &mut Engine<PhemexPerpetuals>, request: &Action, body: &str, streamed: bool) {
    let Action::Http(r) = request else {
        panic!("HTTP")
    };
    if streamed {
        e.handle(
            2.0,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        for data in body.as_bytes().chunks(7) {
            e.handle(
                2.0,
                Event::HttpChunk {
                    request: r.id,
                    data,
                },
            );
        }
        e.handle(2.0, Event::HttpEnd { request: r.id });
    } else {
        e.handle(
            2.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    order_id: String,
    timestamp: f64,
    price: f64,
    qty: f64,
    side: String,
    commission: f64,
    commission_asset: String,
}
#[test]
fn documented_textual_history_is_signed_streamed_and_deduplicated_by_execution() {
    for streamed in [false, true] {
        let mut e = engine(true);
        let a = query(&mut e, fills());
        let Action::Http(r) = &a else { unreachable!() };
        assert!(
            r.url
                .ends_with("/api-data/g-futures/trades?symbol=BTCUSDT&offset=0&limit=100")
        );
        assert!(
            r.headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case("x-phemex-request-signature"))
        );
        let other = FILL.replace("5c3d96e1-8874-53b6-b6e5-9dcc4d28b4ab", "another-execution");
        respond(&mut e, &a, &format!("[{FILL},{FILL},{other}]"), streamed);
        let rows: Vec<Execution> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "5c3d96e1-8874-53b6-b6e5-9dcc4d28b4ab");
        assert_eq!(rows[0].order_id, "fcdfeafa-ed68-45d4-b2bd-7bc27f2b2b0b");
        assert_eq!(rows[0].timestamp, 1_669_407_633.0);
        assert_eq!((rows[0].price, rows[0].qty), (16600.0, 0.001));
        assert_eq!(
            (rows[0].side.as_str(), rows[0].commission_asset.as_str()),
            ("sell", "USDT")
        );
        assert_eq!(rows[0].commission, 0.00166);
    }
}
#[test]
fn fill_filters_use_milliseconds_before_seconds_are_truncated() {
    let mut e = engine(true);
    let a = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("other".into()),
            from_ms: Some(1_669_407_633_999.0),
            to_ms: Some(1_669_407_634_000.0),
            limit: Some(2),
        },
    );
    respond(&mut e, &a, &format!("[{FILL}]"), true);
    assert_eq!(result(&mut e).unwrap(), "[]");
    let a = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(1_669_407_634_000.0),
            to_ms: None,
            limit: Some(2),
        },
    );
    respond(&mut e, &a, &format!("[{FILL}]"), false);
    assert_eq!(result(&mut e).unwrap(), "[]");
}
#[test]
fn malformed_unknown_enum_and_failed_reads_are_errors_not_empty_success() {
    for body in [
        format!("[{}]", FILL.replace("\"Sell\"", "1")),
        format!("[{}]", FILL.replace("0.001", "NaN")),
        format!("[{FILL},]"),
        "{\"code\":10001,\"msg\":\"denied\"}".into(),
    ] {
        let mut e = engine(true);
        let a = query(&mut e, fills());
        respond(&mut e, &a, &body, true);
        assert!(result(&mut e).is_err(), "{body}");
    }
    let mut e = engine(true);
    let a = query(&mut e, fills());
    let Action::Http(r) = a else { unreachable!() };
    e.handle(
        2.0,
        Event::HttpFailed {
            request: r.id,
            error: "network failure",
        },
    );
    assert_eq!(result(&mut e).unwrap_err(), "network failure");
}
#[test]
fn documented_order_history_uses_exact_ns_and_closed_order_shape() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Order {
        id: String,
        status: String,
        #[serde(rename = "type")]
        kind: String,
        amount: f64,
        filled: f64,
        created_time: f64,
        updated_time: f64,
    }
    let mut e = engine(true);
    let a = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(999),
        },
    );
    let Action::Http(r) = &a else { unreachable!() };
    assert!(
        r.url
            .ends_with("/api-data/g-futures/orders?symbol=BTCUSDT&offset=0&limit=200")
    );
    let open = ORDER.replace("Filled", "New");
    respond(&mut e, &a, &format!("[{ORDER},{open}]"), true);
    let rows: Vec<Order> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "743fc923-cb01-4261-88d1-b35dba2cdac0");
    assert_eq!(
        (rows[0].status.as_str(), rows[0].kind.as_str()),
        ("closed", "market")
    );
    assert_eq!((rows[0].amount, rows[0].filled), (0.001, 0.001));
    assert_eq!(
        (rows[0].created_time, rows[0].updated_time),
        (1_667_562_110_213.0, 1_667_562_110_221.0)
    );
}
#[test]
fn recent_public_tuples_keep_duplicate_trades_and_native_millisecond_time() {
    #[derive(Deserialize)]
    struct Trade {
        id: String,
        timestamp: f64,
        #[serde(rename = "quoteQty")]
        quote_qty: f64,
    }
    for streamed in [false, true] {
        let mut e = engine(false);
        let a = query(
            &mut e,
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(2),
            },
        );
        respond(
            &mut e,
            &a,
            r#"{"error":null,"id":0,"result":{"sequence":77766796,"symbol":"BTCUSDT","trades_p":[[1666860389799499999,"Buy","20702.1","0.949"],[1666860389799499999,"Buy","20702.1","0.949"]],"type":"snapshot"}}"#,
            streamed,
        );
        let rows: Vec<Trade> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].id, rows[1].id);
        assert_eq!(rows[0].timestamp, 1_666_860_389_799.0);
        assert_eq!(rows[0].quote_qty, 20702.1 * 0.949);
    }
}
#[test]
fn unsupported_or_invalid_queries_never_issue_io() {
    for (private, query) in [
        (false, fills()),
        (
            true,
            Query::Income {
                symbol: Some("BTCUSDT".into()),
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        ),
        (
            true,
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: None,
            },
        ),
        (
            true,
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        ),
    ] {
        let mut e = engine(private);
        e.handle(1.0, Event::Command(Command::Query { command: 10, query }));
        let output = actions(&mut e);
        assert!(output.iter().all(|a| !matches!(a, Action::Http(_))));
        assert!(output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}

#[test]
fn history_waits_for_the_shared_rate_limit_deadline() {
    let mut e = engine(true);
    let a = query(&mut e, fills());
    let Action::Http(r) = a else { unreachable!() };
    let now = 1_710_000_000_000.0;
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 429,
                headers: "x-ratelimit-retry-after-contract: 2",
                body: "rate limited",
            },
        },
    );
    assert!(result(&mut e).is_err());
    e.handle(
        now,
        Event::Command(Command::Query {
            command: 10,
            query: fills(),
        }),
    );
    let output = actions(&mut e);
    assert!(output.iter().all(|a| !matches!(a, Action::Http(_))));
    let timer = output
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => {
                assert_eq!(at_ms, now + 2_000.0);
                Some(timer)
            }
            _ => None,
        })
        .unwrap();
    e.handle(now + 2_000.0, Event::TimerFired { timer });
    assert!(actions(&mut e).iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn private_history_rejects_rows_beyond_the_documented_page_maximum() {
    let mut e = engine(true);
    let a = query(&mut e, fills());
    let body = format!("[{}]", vec![FILL; 201].join(","));
    respond(&mut e, &a, &body, true);
    assert!(result(&mut e).is_err());
}

#[test]
fn public_only_mode_refuses_private_history_even_with_credentials() {
    let market = engine(true).store().markets()[0].clone();
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        public_data_only: true,
        ..Options::default()
    }));
    e.store_mut().set_markets(vec![market]);
    e.handle(
        1.0,
        Event::Command(Command::Query {
            command: 10,
            query: fills(),
        }),
    );
    let output = actions(&mut e);
    assert!(output.iter().all(|a| !matches!(a, Action::Http(_))));
    assert!(output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}

#[test]
fn close_on_trigger_history_retains_its_implied_reduce_only_contract() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Historical {
        #[serde(rename = "type")]
        kind: String,
        reduce_only: bool,
        price: f64,
    }
    let mut e = engine(true);
    let a = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let row = ORDER.replace("\"ordType\":\"Market\"", "\"ordType\":\"Stop\"").replace("\"reduceOnly\":false", "\"reduceOnly\":false,\"closeOnTrigger\":true,\"stopDirection\":\"Rising\",\"stopPxRp\":\"22000\"");
    respond(&mut e, &a, &format!("[{row}]"), true);
    let rows: Vec<Historical> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows[0].kind, "stop_market");
    assert!(rows[0].reduce_only);
    assert_eq!(rows[0].price, 22000.0);
}

#[test]
fn historical_full_position_stop_preserves_zero_native_quantity_and_actual_fill() {
    #[derive(Deserialize)]
    struct Historical {
        amount: f64,
        filled: f64,
    }
    let mut e = engine(true);
    let a = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let row = ORDER.replace("\"ordType\":\"Market\"", "\"ordType\":\"Stop\"").replace("\"orderQtyRq\":\"0.001\"", "\"orderQtyRq\":\"0\"").replace("\"reduceOnly\":false", "\"reduceOnly\":false,\"closeOnTrigger\":true,\"stopDirection\":\"Rising\",\"stopPxRp\":\"22000\"");
    respond(&mut e, &a, &format!("[{row}]"), true);
    let rows: Vec<Historical> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!((rows[0].amount, rows[0].filled), (0.0, 0.001));
}

#[test]
fn trailing_history_requires_a_known_trigger_and_keeps_its_type() {
    #[derive(Deserialize)]
    struct Historical {
        #[serde(rename = "type")]
        kind: String,
    }
    let row = ORDER.replace("\"ordType\":\"Market\"", "\"ordType\":\"Stop\"").replace("\"reduceOnly\":false", "\"reduceOnly\":true,\"pegPriceType\":\"TrailingStopPeg\",\"stopDirection\":\"Rising\",\"stopPxRp\":\"22000\"");
    let mut e = engine(true);
    let a = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    respond(&mut e, &a, &format!("[{row}]"), true);
    let rows: Vec<Historical> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows[0].kind, "trailing_stop_market");
    let a = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    respond(
        &mut e,
        &a,
        &format!("[{}]", row.replace("22000", "0")),
        true,
    );
    assert!(result(&mut e).is_err());
}

fn lookup(id: &str) -> Query {
    Query::OrderByClientId {
        symbol: "BTCUSDT".into(),
        client_order_id: id.into(),
    }
}
#[test]
fn direct_client_id_lookup_covers_closed_orders_without_mutating_store() {
    #[derive(Deserialize)]
    struct Lookup {
        found: bool,
        order: FoundOrder,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FoundOrder {
        client_order_id: Option<String>,
        status: String,
    }
    for streamed in [false, true] {
        for (native, status) in [
            ("Filled", "closed"),
            ("Canceled", "canceled"),
            ("Triggered", "closed"),
            ("New", "open"),
        ] {
            let mut e = engine(true);
            let id = "caller /?🌊";
            let a = query(&mut e, lookup(id));
            let Action::Http(r) = &a else { unreachable!() };
            assert!(r.url.ends_with("/api-data/g-futures/orders/by-order-id?symbol=BTCUSDT&clOrdID=caller%20%2F%3F%F0%9F%8C%8A"));
            let row = ORDER
                .replace("Filled", native)
                .replace("\"symbol\"", "\"clOrdId\":\"caller /?🌊\",\"symbol\"");
            respond(&mut e, &a, &format!("[{row}]"), streamed);
            let result: Lookup = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
            assert!(result.found);
            assert_eq!(result.order.client_order_id.as_deref(), Some(id));
            assert_eq!(result.order.status, status);
            assert!(e.store().orders().is_empty());
        }
    }
}
#[test]
fn only_successful_exact_client_id_search_can_report_definitive_absence() {
    for streamed in [false, true] {
        let mut e = engine(true);
        let a = query(&mut e, lookup("caller"));
        respond(&mut e, &a, "[]", streamed);
        assert_eq!(
            result(&mut e).unwrap(),
            r#"{"found":false,"definitive":true}"#
        );
        for body in [
            format!("[{ORDER}]"),
            format!(
                "[{}]",
                ORDER.replace("\"symbol\"", "\"clOrdId\":\"wrong\",\"symbol\"")
            ),
            format!(
                "[{}]",
                ORDER
                    .replace("BTCUSDT", "ETHUSDT")
                    .replace("\"symbol\"", "\"clOrdId\":\"caller\",\"symbol\"")
            ),
            "null".into(),
            r#"{"code":10001,"msg":"denied"}"#.into(),
        ] {
            let a = query(&mut e, lookup("caller"));
            respond(&mut e, &a, &body, streamed);
            assert!(result(&mut e).is_err(), "{body}");
        }
    }
}
fn income(from: f64, to: f64, kind: Option<&str>) -> Query {
    Query::Income {
        symbol: None,
        kind: kind.map(str::to_owned),
        from_ms: Some(from),
        to_ms: Some(to),
        limit: None,
    }
}
fn ledger(time: u64, amount: &str, balance: &str, kind: &str, currency: &str) -> String {
    format!(
        r#"{{"typeDesc":"{kind}","amountRv":"{amount}","balanceRv":"{balance}","createTime":{time},"currency":"{currency}"}}"#
    )
}
fn next_http(e: &mut Engine<PhemexPerpetuals>) -> Action {
    actions(e)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .expect("next page")
}
fn finish_empty_probes(e: &mut Engine<PhemexPerpetuals>) -> Result<String, String> {
    for _ in 0..100 {
        let output = actions(e);
        for action in &output {
            if let Action::Notice(Notice::CommandDone { result, .. }) = action {
                return result.clone();
            }
        }
        let a = output
            .into_iter()
            .find(|a| matches!(a, Action::Http(_)))
            .expect("remaining span probe");
        respond(e, &a, "[]", true);
    }
    panic!("bounded span probes must terminate");
}
#[test]
fn income_uses_shared_span_walk_across_short_pages_and_both_settlements() {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Income {
        income: f64,
        asset: String,
        income_type: String,
        category: String,
        tran_id: String,
    }
    for streamed in [false, true] {
        let mut e = engine(true);
        let first = ledger(1100, "-0.01", "99.99", "REALIZED_PNL", "USDT");
        let second = ledger(1200, "2", "101.99", "TRANSFER", "USDT");
        let third = ledger(1300, "0.5", "102.49", "NEW_BIZ_TYPE", "USDT");
        let fourth = ledger(1150, "-1", "99", "REALIZED_PNL", "USDC");
        let a = query(&mut e, income(1000., 2000., None));
        let Action::Http(r) = &a else { unreachable!() };
        assert!(r.url.ends_with("/api-data/futures/v2/tradeAccountDetail?currency=USDT&start=1000&end=2000&offset=0&limit=200"));
        respond(&mut e, &a, &format!("[{first},{second}]"), streamed);
        let a = next_http(&mut e);
        let Action::Http(r) = &a else { unreachable!() };
        assert!(r.url.contains("start=1200&end=2000"));
        respond(&mut e, &a, &format!("[{second},{third}]"), streamed);
        let a = next_http(&mut e);
        respond(&mut e, &a, &format!("[{third}]"), streamed);
        let a = next_http(&mut e);
        let Action::Http(r) = &a else { unreachable!() };
        assert!(r.url.contains("currency=USDC&start=1000&end=2000"));
        respond(&mut e, &a, &format!("[{fourth}]"), streamed);
        let a = next_http(&mut e);
        respond(&mut e, &a, &format!("[{fourth}]"), streamed);
        let rows: Vec<Income> =
            serde_json::from_str(&finish_empty_probes(&mut e).unwrap()).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].income, 0.5);
        assert_eq!(rows[0].income_type, "NEW_BIZ_TYPE");
        assert_eq!(rows[0].category, "other");
        assert_eq!(rows[1].category, "transfer");
        assert_eq!(rows[2].asset, "USDC");
        assert_eq!(rows[3].income, -0.01);
        assert!(rows.iter().all(|row| !row.tran_id.is_empty()));
        assert_eq!(e.store().balance().total, 0.);
    }
}
#[test]
fn income_keeps_distinct_equal_time_postings_and_filters_after_coverage() {
    let mut e = engine(true);
    let a = query(&mut e, income(1000., 2000., Some("REALIZED_PNL")));
    let x = ledger(1100, "1", "100", "REALIZED_PNL", "USDT");
    let y = ledger(1100, "1", "101", "REALIZED_PNL", "USDT");
    let z = ledger(1100, "0", "101", "OTHER", "USDT");
    let body = format!("[{x},{y},{z},{z}]");
    respond(&mut e, &a, &body, true);
    let a = next_http(&mut e);
    respond(&mut e, &a, &body, true);
    let a = next_http(&mut e);
    respond(&mut e, &a, "[]", true);
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Income {
        tran_id: String,
    }
    let rows: Vec<Income> = serde_json::from_str(&finish_empty_probes(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].tran_id, rows[1].tran_id);
}
#[test]
fn income_failed_currency_or_saturated_timestamp_never_returns_partial_success() {
    let mut e = engine(true);
    let a = query(&mut e, income(1000., 2000., None));
    let x = ledger(1100, "1", "100", "REALIZED_PNL", "USDT");
    respond(&mut e, &a, &format!("[{x}]"), false);
    let a = next_http(&mut e);
    respond(&mut e, &a, &format!("[{x}]"), false);
    let a = next_http(&mut e);
    respond(&mut e, &a, r#"{"code":10001,"msg":"denied"}"#, false);
    assert!(result(&mut e).is_err());

    let mut e = engine(true);
    let mut a = query(&mut e, income(1000., 2000., None));
    let body = format!("[{}]", vec![x; 200].join(","));
    respond(&mut e, &a, &body, true);
    a = next_http(&mut e);
    respond(&mut e, &a, &body, true);
    // A full duplicate-only timestamp is ambiguous. The shared pager marks
    // incomplete and probes the remaining span, then completion must fail.
    for _ in 0..60 {
        let output = actions(&mut e);
        if let Some(result) = output.iter().find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result),
            _ => None,
        }) {
            assert!(result.is_err());
            return;
        }
        let a = output
            .into_iter()
            .find(|a| matches!(a, Action::Http(_)))
            .expect("coverage probe");
        respond(&mut e, &a, "[]", true);
    }
    panic!("must terminate visibly incomplete");
}
#[test]
fn income_single_millisecond_window_and_unsupported_filters_fail_closed() {
    let mut e = engine(true);
    let a = query(&mut e, income(1100., 1100., None));
    let x = ledger(1100, "-0.01", "99.99", "REALIZED_PNL", "USDT");
    respond(&mut e, &a, &format!("[{x}]"), false);
    let a = next_http(&mut e);
    respond(&mut e, &a, &format!("[{x}]"), false);
    let a = next_http(&mut e);
    respond(&mut e, &a, "[]", false);
    assert!(finish_empty_probes(&mut e).unwrap().contains("-0.01"));
    for bad in [
        income(f64::NAN, 2000., None),
        income(2001., 2000., None),
        lookup(""),
    ] {
        e.handle(
            1.,
            Event::Command(Command::Query {
                command: 10,
                query: bad,
            }),
        );
        let output = actions(&mut e);
        assert!(output.iter().all(|a| !matches!(a, Action::Http(_))));
        assert!(output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
    }
}

#[test]
fn secondary_reads_never_change_definitive_lookup_identity_or_routing() {
    let market = engine(true).store().markets()[0].clone();
    let mut e = Engine::new(PhemexPerpetuals::new(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    }));
    e.store_mut().set_markets(vec![market]);
    let a = query(&mut e, fills());
    let Action::Http(r) = &a else { unreachable!() };
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("x-phemex-access-token") && v == "read-key")
    );
    assert!(r.route.is_none());
    respond(&mut e, &a, "[]", false);
    assert_eq!(result(&mut e).unwrap(), "[]");
    let a = query(&mut e, lookup("caller"));
    let Action::Http(r) = &a else { unreachable!() };
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("x-phemex-access-token") && v == "primary-key")
    );
    assert_eq!(r.route, Some(safe_cex_engine::RequestRoute::Primary));
    respond(&mut e, &a, "[]", false);
    assert_eq!(
        result(&mut e).unwrap(),
        r#"{"found":false,"definitive":true}"#
    );
}
#[test]
fn income_rejects_bad_rows_and_unknown_currency_instead_of_empty_ledger() {
    let x = ledger(1100, "1", "100", "REALIZED_PNL", "USDT");
    for bad in [
        x.replace("USDT", "BTC"),
        x.replace("\"amountRv\":\"1\"", "\"amountRv\":\"NaN\""),
        x.replace("\"balanceRv\":\"100\"", "\"balanceRv\":\"Infinity\""),
        x.replace("1100", "9007199254740992"),
        x.replace("REALIZED_PNL", ""),
    ] {
        for streamed in [true, false] {
            let mut e = engine(true);
            let a = query(&mut e, income(1000., 2000., None));
            respond(&mut e, &a, &format!("[{bad}]"), streamed);
            assert!(result(&mut e).is_err(), "{bad}");
        }
    }
}

#[test]
fn unknown_lookup_market_is_nondefinitive_without_io_even_without_credentials() {
    for private in [false, true] {
        let mut e = engine(private);
        e.handle(
            1.,
            Event::Command(Command::Query {
                command: 10,
                query: Query::OrderByClientId {
                    symbol: "UNKNOWN".into(),
                    client_order_id: "caller".into(),
                },
            }),
        );
        let output = actions(&mut e);
        assert!(output.iter().all(|a| !matches!(a, Action::Http(_))));
        assert!(output.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Ok(body), .. }) if body == r#"{"found":false,"definitive":false}"#)));
    }
}

#[test]
fn ledger_identity_preserves_money_digits_beyond_binary_float_precision() {
    let mut e = engine(true);
    let a = query(&mut e, income(1000., 2000., None));
    let x = ledger(1100, "0.000000001", "10000000000000000.000000001", "REALIZED_PNL", "USDT");
    let y = ledger(1100, "0.000000001", "10000000000000000.000000002", "REALIZED_PNL", "USDT");
    let body = format!("[{x},{y}]");
    respond(&mut e, &a, &body, true);
    let a = next_http(&mut e);
    respond(&mut e, &a, &body, true);
    let result = finish_empty_probes(&mut e).unwrap();
    #[derive(Deserialize)]
    #[serde(rename_all="camelCase")]
    struct Income { tran_id: String }
    let rows: Vec<Income> = serde_json::from_str(&result).unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].tran_id, rows[1].tran_id);
}
