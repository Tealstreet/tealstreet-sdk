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
                symbol: None,
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
