mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query, Response};
use safe_cex_venue_woo::{Options, Woo};
use serde::Deserialize;
use support::*;

const NOW: f64 = 1_786_603_865_910.0;
fn ready() -> TestEngine {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: false,
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = prepared_seed_requests(&mut e, 10.).remove(0);
    response(&mut e, &r, 20.0, TICKERS);
    e.drain_actions().for_each(drop);
    e
}
fn query(e: &mut TestEngine, q: Query) -> safe_cex_engine::HttpRequest {
    e.handle(
        NOW,
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
fn fill(id: u64, time: f64, side: &str) -> String {
    format!(
        r#"{{"id":{id},"orderId":9007199254740993,"symbol":"PERP_BTC_USDT","side":"{side}","executedPrice":60000,"executedQuantity":0.001,"executedTimestamp":{time},"fee":0.01,"feeAsset":"USDT","realizedPnl":null,"isMaker":0}}"#
    )
}
const ORDER: &str = r#"{"order_id":9007199254740993,"symbol":"PERP_BTC_USDT","side":"BUY","type":"IOC","status":"FILLED","price":60000,"quantity":0.01,"executed":0.01,"average_executed_price":59999,"reduce_only":true,"position_side":"SHORT","created_time":"1786603865.910","updated_time":"1786603865.920","realized_pnl":0,"total_fee":0.00181194,"fee_asset":"USDT","total_rebate":0,"rebate_asset":"USDT"}"#;

#[test]
fn execution_queries_accept_chart_aliases_and_map_native_suffix_spellings() {
    for symbol in [
        "woo:BTCUSDT.P",
        "woo:btcusdt.p",
        "PERP_BTC_USDT",
        "BTC_USDT_PERP",
        "BTC-USDT-PERP",
        "BTC/USDT",
        "BTCUSDT",
    ] {
        let mut e = ready();
        let r = query(
            &mut e,
            Query::Fills {
                symbol: Some(symbol.into()),
                order_id: None,
                from_ms: Some(NOW - 1000.0),
                to_ms: Some(NOW + 1000.0),
                limit: Some(2),
            },
        );
        assert!(r.url.contains("symbol=PERP_BTC_USDT"), "{symbol}");
        assert!(r.url.contains("startTime=1786603864910"));
        assert!(r.url.contains("endTime=1786603866910"));
        let first = fill(1, NOW, "BUY").replace("PERP_BTC_USDT", "BTC_USDT_PERP");
        let second = fill(2, NOW + 1.0, "SELL").replace("PERP_BTC_USDT", "BTC-USDT-PERP");
        response(
            &mut e,
            &r,
            NOW + 2.0,
            &format!(r#"{{"success":true,"data":{{"rows":[{first},{second}]}}}}"#),
        );
        let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2, "{symbol}");
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .all(|r| r["symbol"] == "BTCUSDT")
        );
        assert!(e.store().orders().is_empty());
        assert!(e.store().positions().is_empty());
    }
}

#[test]
fn account_execution_history_retains_ts_product_and_symbol_normalization() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(8),
        },
    );
    let rows = [
        "PERP_BTC_USDT",
        "BTC_USDT_PERP",
        "BTC-USDT-PERP",
        "BTCUSDT",
        "btc/usdt",
        "SPOT_BTC_USDT",
        "woo:PERP_BTC_USDT",
        "BTCPERP",
    ]
    .iter()
    .enumerate()
    .map(|(i, symbol)| fill(i as u64 + 1, NOW, "BUY").replace("PERP_BTC_USDT", symbol))
    .collect::<Vec<_>>()
    .join(",");
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(r#"{{"success":true,"data":{{"rows":[{rows}]}}}}"#),
    );
    let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 8);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|r| r["symbol"] == "BTCUSDT")
    );
    assert_eq!(e.store().markets().len(), 1);
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}

#[test]
fn fills_preserve_execution_ids_hedge_sides_and_absent_pnl_without_store_mutation() {
    let mut e = ready();
    let before = e.store().balance().clone();
    let r = query(&mut e, fills(Some(5)));
    assert!(r.url.contains("/v3/trade/transactionHistory"));
    assert!(!r.headers.is_empty());
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{},{}]}}}}"#,
            fill(9007199254740995, NOW, "OPEN LONG"),
            fill(9007199254740996, NOW + 1.0, "CLOSE SHORT")
        ),
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains(r#""id":"9007199254740995""#));
    assert!(json.contains(r#""orderId":"9007199254740993""#));
    assert!(json.contains(r#""positionSide":"long""#));
    assert!(json.contains(r#""positionSide":"short""#));
    assert!(!json.contains("realizedPnl"));
    assert_eq!(json.matches("timestamp").count(), 2);
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().balance(), &before);
}
#[test]
fn ascending_v3_history_advances_time_windows_instead_of_ignored_page() {
    let mut e = ready();
    let from = NOW - 4000.0;
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(from),
            to_ms: Some(NOW),
            limit: Some(3),
        },
    );
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{},{}]}}}}"#,
            fill(1, from + 1000.0, "BUY"),
            fill(2, from + 2000.0, "BUY")
        ),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains(&format!("startTime={}", from + 2000.0)));
    assert!(!r.url.contains("page="));
    response(
        &mut e,
        &r,
        NOW + 2.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{},{}]}}}}"#,
            fill(2, from + 2000.0, "BUY"),
            fill(3, from + 3000.0, "SELL")
        ),
    );
    assert_eq!(result(&mut e).unwrap().matches("timestamp").count(), 3);
}
#[test]
fn empty_v3_falls_back_to_v1_with_distinct_parameter_vocabulary() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(2)));
    let mut r = r;
    for i in 0..5 {
        response(
            &mut e,
            &r,
            NOW + i as f64 + 1.0,
            r#"{"success":true,"data":{"rows":[]}}"#,
        );
        r = requests(&mut e).remove(0);
    }
    assert!(r.url.contains("/v1/client/trades"));
    assert!(r.url.contains("size=2"));
    assert!(!r.url.contains("page="));
    assert!(!r.url.contains("limit="));
    assert!(!r.url.contains("hist_trades"));
    let row = r#"{"id":1,"order_id":2,"symbol":"PERP_BTC_USDT","side":"SELL","executed_price":60000,"executed_quantity":0.001,"executed_timestamp":"1786603865.910","fee":0.01,"fee_asset":"USDT","realized_pnl":null,"is_maker":true}"#;
    response(
        &mut e,
        &r,
        NOW + 2.0,
        &format!(
            r#"{{"success":true,"rows":[{row}],"meta":{{"total":1,"records_per_page":2,"current_page":1}}}}"#
        ),
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains(r#""timestamp":1786603865.0"#));
    assert!(json.contains(r#""maker":true"#));
    assert!(!json.contains("realizedPnl"));
}
#[test]
fn order_history_requests_pnl_and_keeps_fractional_time_zero_pnl_and_positive_fees() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW + 1000.0),
            limit: Some(5),
        },
    );
    assert!(r.url.contains("realized_pnl=true"));
    assert!(r.url.contains("start_t=1786603864910"));
    assert!(!r.url.contains("INCOMPLETE"));
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"rows":[{ORDER}],"meta":{{"total":1,"records_per_page":100,"current_page":1}}}}"#
        ),
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains(r#""createdTime":1786603865910.0"#));
    assert!(json.contains(r#""updatedTime":1786603865920.0"#));
    assert!(json.contains(r#""realizedPnl":0.0"#));
    assert!(json.contains(r#""fees":0.00181194"#));
    assert!(json.contains(r#""positionSide":"short""#));
    assert!(json.contains(r#""type":"limit""#));
}
#[test]
fn short_order_page_with_venue_census_continues_to_page_two() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: Some(5),
        },
    );
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"rows":[{ORDER}],"meta":{{"total":2,"records_per_page":1,"current_page":1}}}}"#
        ),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("page=2"));
    response(
        &mut e,
        &r,
        NOW + 2.0,
        &format!(
            r#"{{"success":true,"rows":[{}],"meta":{{"total":2,"records_per_page":1,"current_page":2}}}}"#,
            ORDER.replace("9007199254740993", "9007199254740994")
        ),
    );
    assert_eq!(result(&mut e).unwrap().matches("createdTime").count(), 2);
}
#[test]
fn ledger_retains_account_aggregates_signed_money_and_same_id_different_types() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW + 1000.0),
            limit: Some(5),
        },
    );
    assert!(r.url.contains("/v1/client/transaction_history"));
    assert!(r.url.contains("start_t=1786603864910"));
    response(
        &mut e,
        &r,
        NOW + 1.0,
        r#"{"success":true,"data":{"rows":[{"id":1786603865910,"type":"REALIZED_PNL","token":"USDT","amount":3.5,"timestamp":1786603865910},{"id":1786603865910,"type":"TRANSACTION_FEE","token":"USDT","symbol":"PERP_BTC_USDT","amount":-0.1,"timestamp":1786603865910},{"id":1786603865911,"type":"INTEREST","token":"USDT","amount":-0.02,"timestamp":1786603865911}],"meta":{"total":3,"records_per_page":100,"current_page":1}}}"#,
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains(r#""symbol":"","incomeType":"REALIZED_PNL""#));
    assert!(json.contains(r#""income":-0.1"#));
    assert!(json.contains(r#""category":"commission""#));
    assert!(json.contains(r#""category":"other""#));
    assert!(!json.contains(r#""category":"funding""#));
    assert_eq!(json.matches("tranId").count(), 3);
}
#[test]
fn ledger_pages_obey_ten_requests_per_minute_and_later_failure_keeps_coverage() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(5),
        },
    );
    response(
        &mut e,
        &r,
        NOW + 1.0,
        r#"{"success":true,"data":{"rows":[{"id":1,"type":"FUNDING_FEE","token":"USDT","amount":0.1,"timestamp":1786603865910}],"meta":{"total":2,"records_per_page":1,"current_page":1}}}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = actions
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                assert_eq!(at_ms, NOW + 6000.0);
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(NOW + 6000.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("page=2"));
    e.handle(
        NOW + 6001.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"{"success":false,"message":"rate limit"}"#,
            },
        },
    );
    // The first validated ledger row survives; page-two failure carries coverage.
    owned_history::partial(&e.drain_actions().collect::<Vec<_>>(), 1);
}
#[test]
fn malformed_history_row_is_skipped_with_a_warning_and_valid_rows_return() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(5)));
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{},{{"id":2}}]}}}}"#,
            fill(1, NOW, "BUY")
        ),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. }))
            if message.contains("WOO fills history: skipped 1")
    )));
    let json = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 99,
                result,
            }) => Some(result),
            _ => None,
        })
        .unwrap();
    assert!(json.contains(r#""id":"1""#), "{json}");
    assert!(!json.contains(r#""id":"2""#));
}
#[test]
fn malformed_order_history_and_ledger_rows_are_skipped_not_fatal() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW + 1000.0),
            limit: Some(5),
        },
    );
    let bad = ORDER
        .replace("9007199254740993", "7")
        .replace(r#""type":"IOC""#, r#""type":"UNKNOWN""#);
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"rows":[{bad},{ORDER}],"meta":{{"total":2,"records_per_page":100,"current_page":1}}}}"#
        ),
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains("9007199254740993"));
    assert!(!json.contains(r#""id":"7""#));
}
#[test]
fn wholly_malformed_history_page_still_fails_visibly() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(5)));
    response(
        &mut e,
        &r,
        NOW + 1.0,
        r#"{"success":true,"data":{"rows":[{"id":2},{"id":3}]}}"#,
    );
    assert!(result(&mut e).is_err());
}
#[test]
fn public_trades_use_unsigned_legacy_route_and_preserve_identical_prints() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(5),
        },
    );
    assert!(r.headers.is_empty());
    assert!(r.url.contains("/v1/public/market_trades"));
    let row = r#"{"symbol":"PERP_BTC_USDT","side":"BUY","executed_price":60000,"executed_quantity":0.01,"executed_timestamp":"1786603865.910"}"#;
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(r#"{{"success":true,"rows":[{row},{row}]}}"#),
    );
    #[derive(Deserialize)]
    struct Print {
        id: String,
        timestamp: f64,
        #[serde(rename = "quoteQty")]
        quote_qty: f64,
    }
    let rows: Vec<Print> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].id, rows[1].id);
    assert_eq!(rows[0].timestamp, NOW);
    assert_eq!(rows[0].quote_qty, 600.0);
}
#[test]
fn invalid_history_inputs_finish_without_http() {
    for q in [
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(NOW),
            to_ms: Some(NOW - 1.0),
            limit: None,
        },
        Query::OrderHistory {
            symbol: Some("UNKNOWN".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        fills(Some(0)),
    ] {
        let mut e = ready();
        e.handle(
            NOW,
            Event::Command(Command::Query {
                command: 99,
                query: q,
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}

#[test]
fn zero_order_update_time_is_absent_so_journal_uses_created_time() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"rows":[{}],"meta":{{"total":1,"records_per_page":100,"current_page":1}}}}"#,
            ORDER.replace("\"updated_time\":\"1786603865.920\"", "\"updated_time\":0")
        ),
    );
    let json = result(&mut e).unwrap();
    assert!(!json.contains("updatedTime"));
    assert!(json.contains("1786603865910"));
}

#[test]
fn bounded_income_range_without_limit_walks_past_five_hundred_records() {
    let mut e = ready();
    let mut r = query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW + 1000.0),
            limit: None,
        },
    );
    for page in 1..=6 {
        let first = (page - 1) * 100;
        let count = 100.min(501 - first);
        let rows=(first..first+count).map(|id|format!(r#"{{"id":{id},"type":"REALIZED_PNL","token":"USDT","amount":0.1,"timestamp":{NOW}}}"#)).collect::<Vec<_>>().join(",");
        response(
            &mut e,
            &r,
            NOW + (page - 1) as f64 * 6000.0 + 1.0,
            &format!(
                r#"{{"success":true,"data":{{"rows":[{rows}],"meta":{{"total":501,"records_per_page":100,"current_page":{page}}}}}}}"#
            ),
        );
        if page < 6 {
            let timer = e
                .drain_actions()
                .find_map(|a| {
                    if let Action::SetTimer { timer, .. } = a {
                        Some(timer)
                    } else {
                        None
                    }
                })
                .unwrap();
            e.handle(NOW + page as f64 * 6000.0, Event::TimerFired { timer });
            r = requests(&mut e).remove(0);
        }
    }
    assert_eq!(result(&mut e).unwrap().matches("tranId").count(), 501);
}

#[test]
fn maker_flags_preserve_boolean_integer_string_and_null_semantics() {
    #[derive(Deserialize)]
    struct Execution {
        maker: Option<bool>,
    }
    for (raw, expected) in [
        ("false", Some(false)),
        ("true", Some(true)),
        ("0", Some(false)),
        ("1", Some(true)),
        (r#""0""#, Some(false)),
        (r#""1""#, Some(true)),
        ("null", None),
    ] {
        let mut e = ready();
        let r = query(&mut e, fills(Some(1)));
        let row = fill(1, NOW, "BUY").replace(r#""isMaker":0"#, &format!(r#""isMaker":{raw}"#));
        response(
            &mut e,
            &r,
            NOW + 1.0,
            &format!(r#"{{"success":true,"data":{{"rows":[{row}]}}}}"#),
        );
        let rows: Vec<Execution> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows[0].maker, expected, "{raw}");
    }
    for raw in [
        "-1",
        "-0",
        "2",
        "255",
        "256",
        "1.0",
        "1e0",
        r#""true""#,
        r#""""#,
        r#""01""#,
        r#"" 1""#,
        "[]",
        "{}",
    ] {
        let mut e = ready();
        let r = query(&mut e, fills(Some(1)));
        let row = fill(1, NOW, "BUY").replace(r#""isMaker":0"#, &format!(r#""isMaker":{raw}"#));
        response(
            &mut e,
            &r,
            NOW + 1.0,
            &format!(r#"{{"success":true,"data":{{"rows":[{row}]}}}}"#),
        );
        assert!(result(&mut e).is_err(), "{raw}");
    }
}

#[test]
fn optional_execution_ids_keep_exact_strings_and_omit_null_or_empty() {
    #[derive(Deserialize)]
    struct Execution {
        id: String,
        #[serde(rename = "orderId")]
        order_id: Option<String>,
    }
    for (raw, expected) in [
        ("null", Some("18446744073709551615")),
        (r#""""#, None),
        ("0", Some("0")),
        (r#""0001""#, Some("0001")),
        ("18446744073709551615", Some("18446744073709551615")),
    ] {
        let mut e = ready();
        let r = query(&mut e, fills(Some(1)));
        let row = fill(1, NOW, "BUY")
            .replace("9007199254740993", raw)
            .replace(r#""id":1"#, r#""id":18446744073709551615"#);
        response(
            &mut e,
            &r,
            NOW + 1.0,
            &format!(r#"{{"success":true,"data":{{"rows":[{row}]}}}}"#),
        );
        let rows: Vec<Execution> = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows[0].id, "18446744073709551615");
        assert_eq!(rows[0].order_id.as_deref(), expected, "{raw}");
    }
    for raw in ["-1", "-0", "1.0", "true", "[]"] {
        let mut e = ready();
        let r = query(&mut e, fills(Some(1)));
        let row = fill(1, NOW, "BUY").replace(r#""id":1"#, &format!(r#""id":{raw}"#));
        response(
            &mut e,
            &r,
            NOW + 1.0,
            &format!(r#"{{"success":true,"data":{{"rows":[{row}]}}}}"#),
        );
        assert!(result(&mut e).is_err(), "{raw}");
    }
}

#[test]
fn empty_canonical_fill_query_tries_alternate_v3_symbol_before_legacy() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(2)));
    assert!(r.url.contains("symbol=PERP_BTC_USDT"));
    response(&mut e, &r, NOW + 1.0, r#"{"success":true,"rows":[]}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("/v3/trade/transactionHistory"));
    assert!(r.url.contains("symbol=BTC_USDT_PERP"));
    let row = r#"{"symbol":"BTC_USDT_PERP","orderId":99,"executedPrice":"60000","executedQuantity":"0.005","side":"BUY","executedTimestamp":"1752055400.100"}"#;
    response(
        &mut e,
        &r,
        NOW + 2.0,
        &format!(r#"{{"success":true,"rows":[{row}]}}"#),
    );
    let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(
        rows[0],
        serde_json::json!({
            "symbol":"BTCUSDT", "orderId":"99", "timestamp":1752055400.0,
            "price":60000.0, "qty":0.005, "side":"buy", "positionSide":"both",
        })
    );
    assert!(requests(&mut e).is_empty());
}

#[test]
fn legacy_aggregate_fill_accepts_optional_id_and_legacy_price_quantity_time_aliases() {
    let mut e = ready();
    let mut r = query(&mut e, fills(Some(2)));
    for (i, symbol) in [
        "PERP_BTC_USDT",
        "BTC_USDT_PERP",
        "BTC-USDT-PERP",
        "BTCUSDT",
        "",
    ]
    .iter()
    .enumerate()
    {
        assert!(r.url.contains("/v3/trade/transactionHistory"));
        if symbol.is_empty() {
            assert!(!r.url.contains("symbol="));
        } else {
            assert!(r.url.contains(&format!("symbol={symbol}")));
        }
        response(
            &mut e,
            &r,
            NOW + i as f64 + 1.0,
            r#"{"success":true,"rows":[]}"#,
        );
        r = requests(&mut e).remove(0);
    }
    assert!(r.url.contains("/v1/client/trades"));
    let row = r#"{"symbol":"PERP_BTC_USDT","order_id":123,"average_executed_price":"62000","executed":"0.002","side":"BUY","created_time":"1752055600.100","total_fee":"0.01","fee_asset":"USDT"}"#;
    response(
        &mut e,
        &r,
        NOW + 6.0,
        &format!(
            r#"{{"success":true,"rows":[{row}],"meta":{{"total":1,"records_per_page":2,"current_page":1}}}}"#
        ),
    );
    let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(
        rows[0],
        serde_json::json!({
            "symbol":"BTCUSDT", "orderId":"123", "timestamp":1752055600.0,
            "price":62000.0, "qty":0.002, "side":"buy", "positionSide":"both",
            "commission":0.01, "commissionAsset":"USDT",
        })
    );
}

#[test]
fn empty_bounded_fill_window_walks_each_v3_and_legacy_spelling_then_finishes() {
    let mut e = ready();
    let mut r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW),
            limit: Some(2),
        },
    );
    for i in 0..10 {
        let v3 = i < 5;
        assert!(r.url.contains(if v3 {
            "/v3/trade/transactionHistory"
        } else {
            "/v1/client/trades"
        }));
        assert!(r.url.contains(if v3 {
            "startTime=1786603864910"
        } else {
            "start_t=1786603864910"
        }));
        let symbol = [
            "PERP_BTC_USDT",
            "BTC_USDT_PERP",
            "BTC-USDT-PERP",
            "BTCUSDT",
            "",
        ][i % 5];
        if symbol.is_empty() {
            assert!(!r.url.contains("symbol="));
        } else {
            assert!(r.url.contains(&format!("symbol={symbol}")));
        }
        response(
            &mut e,
            &r,
            NOW + i as f64 + 1.0,
            r#"{"success":true,"rows":[]}"#,
        );
        if i < 9 {
            r = requests(&mut e).remove(0);
        }
    }
    assert_eq!(result(&mut e).unwrap(), "[]");
    assert!(requests(&mut e).is_empty());
}

#[test]
fn idless_fills_preserve_distinct_millisecond_times_and_optional_id_shapes() {
    for id in [None, Some("null"), Some(r#""""#)] {
        let mut e = ready();
        let r = query(&mut e, fills(Some(2)));
        let rows = [NOW, NOW + 1.0]
            .iter()
            .map(|time| {
                let row = fill(1, *time, "BUY");
                match id {
                    Some(id) => row.replace(r#""id":1"#, &format!(r#""id":{id}"#)),
                    None => row.replace(r#""id":1,"#, ""),
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        response(
            &mut e,
            &r,
            NOW + 2.0,
            &format!(r#"{{"success":true,"rows":[{rows}]}}"#),
        );
        let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2);
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .all(|row| row.get("id").is_none())
        );
    }
}

#[test]
fn fill_fields_choose_active_ts_precedence_when_legacy_and_canonical_values_coexist() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(1)));
    let row = fill(1, NOW, "BUY").replace(
        r#""executedPrice":60000"#,
        r#""executedPrice":60000,"price":59000,"average_executed_price":58000,"last_price":57000"#,
    ).replace(
        r#""executedQuantity":0.001"#,
        r#""executedQuantity":0.001,"quantity":0.01,"executed":0.02,"last_qty":0.03"#,
    ).replace(
        r#""executedTimestamp":1786603865910"#,
        r#""executedTimestamp":1786603865910,"created_time":1752055600.1,"trade_time":1752055600.2"#,
    );
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(r#"{{"success":true,"data":{{"rows":[{row}]}}}}"#),
    );
    let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows[0]["price"], 60000.0);
    assert_eq!(rows[0]["qty"], 0.001);
    assert_eq!(rows[0]["timestamp"], 1786603865.0);
}

#[test]
fn failed_symbol_candidates_remain_visible_when_all_fallbacks_are_empty() {
    let mut e = ready();
    let mut r = query(&mut e, fills(Some(2)));
    for i in 0..10 {
        if i == 0 {
            e.handle(
                NOW + i as f64 + 1.0,
                Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status: 500,
                        headers: "",
                        body: r#"{"success":false,"message":"busy"}"#,
                    },
                },
            );
        } else {
            response(
                &mut e,
                &r,
                NOW + i as f64 + 1.0,
                r#"{"success":true,"rows":[]}"#,
            );
        }
        if i < 9 {
            r = requests(&mut e).remove(0);
        }
    }
    assert!(
        result(&mut e)
            .unwrap_err()
            .contains("history request rejected")
    );
}

#[test]
fn id_only_fill_inherits_order_id_for_output_and_order_filter() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("1".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let row = fill(1, NOW, "BUY").replace(r#""orderId":9007199254740993,"#, "");
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(r#"{{"success":true,"data":{{"rows":[{row}]}}}}"#),
    );
    let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
    assert_eq!(rows[0]["id"], "1");
    assert_eq!(rows[0]["orderId"], "1");
}

#[test]
fn new_v3_window_resets_page_after_a_paginated_legacy_fallback() {
    let mut e = ready();
    let from = NOW - 30.0 * 86_400_000.0 - 1000.0;
    let mut r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(from),
            to_ms: Some(NOW),
            limit: Some(10),
        },
    );
    for i in 0..5 {
        response(
            &mut e,
            &r,
            NOW + i as f64 + 1.0,
            r#"{"success":true,"rows":[]}"#,
        );
        r = requests(&mut e).remove(0);
    }
    assert!(r.url.contains("/v1/client/trades"));
    assert!(!r.url.contains("page="));
    response(
        &mut e,
        &r,
        NOW + 6.0,
        &format!(
            r#"{{"success":true,"rows":[{}],"meta":{{"total":2,"records_per_page":1,"current_page":1}}}}"#,
            fill(1, from + 1.0, "BUY"),
        ),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("page=2"));
    response(
        &mut e,
        &r,
        NOW + 7.0,
        &format!(
            r#"{{"success":true,"rows":[{}],"meta":{{"total":2,"records_per_page":1,"current_page":2}}}}"#,
            fill(2, from + 2.0, "BUY"),
        ),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("/v3/trade/transactionHistory"));
    assert!(r.url.contains("symbol=PERP_BTC_USDT"));
    assert!(r.url.contains(&format!("startTime={}", NOW - 1000.0)));
    response(&mut e, &r, NOW + 8.0, r#"{"success":true,"rows":[]}"#);
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("/v3/trade/transactionHistory"));
    assert!(r.url.contains("symbol=BTC_USDT_PERP"));
}

#[test]
fn trade_id_only_rows_emit_fill_ids_without_inventing_order_ids_or_matching_order_filters() {
    for key in ["tradeId", "trade_id"] {
        for order_filter in [None, Some("102")] {
            let mut e = ready();
            let r = query(
                &mut e,
                Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: order_filter.map(String::from),
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
            );
            let row = fill(1, NOW, "BUY")
                .replace(r#""id":1"#, &format!(r#""{key}":102"#))
                .replace(r#""orderId":9007199254740993,"#, "");
            response(
                &mut e,
                &r,
                NOW + 1.0,
                &format!(r#"{{"success":true,"rows":[{row}]}}"#),
            );
            let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
            if order_filter.is_some() {
                assert!(rows.as_array().unwrap().is_empty());
            } else {
                assert_eq!(rows[0]["id"], "102");
                assert!(rows[0].get("orderId").is_none());
            }
        }
    }
}

#[test]
fn fill_identifier_precedence_preserves_nullish_and_explicit_empty_boundaries() {
    for (raw_id, trade_id, snake_trade_id, expected_fill, expected_order) in [
        ("123", "456", "789", Some("123"), Some("123")),
        (r#""""#, "789", "456", None, None),
        ("null", "789", "456", Some("789"), None),
        ("null", r#""""#, "456", None, None),
        ("null", "null", "456", Some("456"), None),
    ] {
        let mut e = ready();
        let r = query(&mut e, fills(Some(1)));
        let row = fill(1, NOW, "BUY")
            .replace(
                r#""id":1"#,
                &format!(r#""id":{raw_id},"tradeId":{trade_id},"trade_id":{snake_trade_id}"#),
            )
            .replace(r#""orderId":9007199254740993,"#, "");
        response(
            &mut e,
            &r,
            NOW + 1.0,
            &format!(r#"{{"success":true,"rows":[{row}]}}"#),
        );
        let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows[0].get("id").and_then(|v| v.as_str()), expected_fill);
        assert_eq!(
            rows[0].get("orderId").and_then(|v| v.as_str()),
            expected_order
        );
    }
}
#[test]
fn full_walk_page_with_malformed_rows_is_not_mistaken_for_the_last_page() {
    let mut e = ready();
    let from = NOW - 4000.0;
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(from),
            to_ms: Some(NOW),
            limit: Some(3),
        },
    );
    response(
        &mut e,
        &r,
        NOW + 1.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{},{}]}}}}"#,
            fill(1, from + 1000.0, "BUY"),
            fill(2, from + 2000.0, "BUY")
        ),
    );
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        NOW + 2.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{},{{"id":8}},{{"id":9}}]}}}}"#,
            fill(2, from + 2000.0, "BUY")
        ),
    );
    let r = requests(&mut e)
        .into_iter()
        .next()
        .expect("a full page must keep walking");
    assert!(r.url.contains(&format!("startTime={}", from + 2001.0)));
    response(
        &mut e,
        &r,
        NOW + 3.0,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{}]}}}}"#,
            fill(3, from + 3000.0, "SELL")
        ),
    );
    assert_eq!(result(&mut e).unwrap().matches("timestamp").count(), 3);
}

mod owned_history {
    use super::*;
    use safe_cex_engine::{ErrorKind, HttpRequest, Level, Value, log::code};

    fn orders() -> Query {
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(2000),
        }
    }
    fn income(kind: Option<&str>) -> Query {
        Query::Income {
            symbol: None,
            kind: kind.map(str::to_owned),
            from_ms: None,
            to_ms: None,
            limit: Some(5),
        }
    }
    fn ledger(id: u32) -> String {
        format!(
            r#"{{"id":{id},"type":"FUNDING_FEE","token":"USDT","amount":0.1,"timestamp":{NOW}}}"#
        )
    }
    fn order(id: u32) -> String {
        ORDER.replace("9007199254740993", &id.to_string())
    }
    fn rows(axis: u8, rows: &str, page: u32, total: u32, size: u32) -> String {
        let data = format!(
            r#""rows":[{rows}],"meta":{{"total":{total},"records_per_page":{size},"current_page":{page}}}"#
        );
        if axis == 0 || axis == 2 {
            format!(r#"{{"success":true,"data":{{{data}}}}}"#)
        } else {
            format!(r#"{{"success":true,{data}}}"#)
        }
    }
    fn send(e: &mut TestEngine, r: &HttpRequest, text: &str, streamed: bool) {
        if streamed {
            e.handle(
                NOW + 1.,
                Event::HttpStart {
                    request: r.id,
                    status: 200,
                    headers: "",
                },
            );
            for chunk in text.as_bytes().chunks(19) {
                e.handle(
                    NOW + 1.,
                    Event::HttpChunk {
                        request: r.id,
                        data: chunk,
                    },
                );
            }
            e.handle(NOW + 1., Event::HttpEnd { request: r.id });
        } else {
            response(e, r, NOW + 1., text);
        }
    }
    fn next(e: &mut TestEngine) -> HttpRequest {
        let actions: Vec<_> = e.drain_actions().collect();
        if let Some(r) = actions.iter().find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        }) {
            return r;
        }
        let (timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("paced continuation");
        e.handle(at, Event::TimerFired { timer });
        requests(e).remove(0)
    }
    fn markers(output: &[Action]) -> Vec<&safe_cex_engine::LogEvent> {
        output
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) if event.code == code::HISTORY_INCOMPLETE => {
                    Some(event)
                }
                _ => None,
            })
            .collect()
    }
    fn typed_reject(output: &[Action]) {
        assert!(
            markers(output).is_empty(),
            "strict rejection has no result coverage: {output:?}"
        );
        assert!(
            !output
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
        );
        let errors: Vec<_> = output
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 99, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1, "one failure: {output:?}");
        assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
        assert_eq!(errors[0].outcome, None);
    }
    pub(super) fn partial(output: &[Action], count: usize) {
        let gaps = markers(output);
        assert_eq!(gaps.len(), 1, "one shared coverage marker: {output:?}");
        assert_eq!(gaps[0].level, Level::Warn);
        assert!(
            matches!(gaps[0].fields.as_slice(), [("command", Value::Num(owner))] if *owner == 99.)
        );
        let (terminal, result) = output
            .iter()
            .enumerate()
            .find_map(|(i, a)| match a {
                Action::Notice(Notice::CommandDone {
                    command: 99,
                    result,
                }) => Some((i, result)),
                _ => None,
            })
            .expect("accepted-only result");
        assert!(output.iter().position(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code==code::HISTORY_INCOMPLETE)).unwrap() < terminal);
        let result: Vec<serde_json::Value> = serde_json::from_str(result).unwrap();
        assert_eq!(result.len(), count);
        assert!(
            !result
                .iter()
                .any(|r| r["id"] == "999999" || r["tranId"] == "999999"),
            "failed-page prefix cannot escape"
        );
        assert!(
            !output
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
    fn failed(e: &mut TestEngine, r: &HttpRequest, failure: u8, axis: u8, streamed: bool) {
        match failure {
            0 => e.handle(
                NOW + 10000.,
                Event::HttpFailed {
                    request: r.id,
                    error: "connection lost",
                },
            ),
            1 => e.handle(
                NOW + 10000.,
                Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status: 503,
                        headers: "",
                        body: r#"{"success":false}"#,
                    },
                },
            ),
            2 => send(e, r, &rows(axis, "{}", 2, 2, 1), streamed),
            3 => {
                let prefix = if axis == 1 {
                    order(999999)
                } else {
                    ledger(999999)
                };
                let text = if axis == 2 {
                    format!(r#"{{"success":true,"data":{{"rows":[{prefix},"#)
                } else {
                    format!(r#"{{"success":true,"rows":[{prefix},"#)
                };
                send(e, r, &text, streamed);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn first_page_failures_reject_typed_without_a_marker() {
        for axis in 0..4 {
            for failure in 0..4 {
                for streamed in [false, true] {
                    let mut e = ready();
                    let q = match axis {
                        0 => fills(Some(5)),
                        1 => orders(),
                        2 => income(None),
                        _ => Query::PublicTrades {
                            symbol: "BTCUSDT".into(),
                            limit: Some(5),
                        },
                    };
                    let mut r = query(&mut e, q);
                    // A wholly unmappable page is a terminal failure. Other
                    // first-fill failures keep the existing native alternatives.
                    let attempts = if axis == 0 && failure != 2 { 10 } else { 1 };
                    for attempt in 0..attempts {
                        failed(&mut e, &r, failure, axis, streamed);
                        if attempt + 1 < attempts {
                            r = next(&mut e);
                        }
                    }
                    typed_reject(&e.drain_actions().collect::<Vec<_>>());
                    assert!(e.store().orders().is_empty());
                }
            }
        }
    }

    #[test]
    fn later_order_and_income_failure_keep_only_validated_prior_rows() {
        for axis in [1, 2] {
            for failure in 0..4 {
                for streamed in [false, true] {
                    let mut e = ready();
                    e.set_log_level(Level::Error);
                    let first = query(&mut e, if axis == 1 { orders() } else { income(None) });
                    send(
                        &mut e,
                        &first,
                        &rows(axis, &if axis == 1 { order(1) } else { ledger(1) }, 1, 2, 1),
                        streamed,
                    );
                    let second = next(&mut e);
                    failed(&mut e, &second, failure, axis, streamed);
                    partial(&e.drain_actions().collect::<Vec<_>>(), 1);
                    assert!(e.store().orders().is_empty());
                }
            }
        }
    }

    #[test]
    fn ranged_fills_keep_admitted_rows_when_later_alternatives_all_fail() {
        for streamed in [false, true] {
            let mut e = ready();
            let first = query(
                &mut e,
                Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: Some(NOW - 4000.),
                    to_ms: Some(NOW),
                    limit: Some(5),
                },
            );
            send(
                &mut e,
                &first,
                &rows(0, &fill(1, NOW - 1000., "BUY"), 1, 1, 1),
                streamed,
            );
            let mut second = next(&mut e);
            for attempt in 0..10 {
                failed(&mut e, &second, 0, 0, streamed);
                if attempt < 9 {
                    second = next(&mut e);
                }
            }
            partial(&e.drain_actions().collect::<Vec<_>>(), 1);
        }
    }

    #[test]
    fn legacy_fill_second_page_failure_retains_the_first_legacy_page() {
        for streamed in [false, true] {
            let mut e = ready();
            let mut r = query(&mut e, fills(Some(5)));
            for _ in 0..5 {
                send(&mut e, &r, &rows(0, "", 1, 0, 1), streamed);
                r = next(&mut e);
            }
            assert!(r.url.contains("/v1/client/trades"));
            send(
                &mut e,
                &r,
                &rows(1, &fill(1, NOW, "BUY"), 1, 2, 1),
                streamed,
            );
            let second = next(&mut e);
            assert!(second.url.contains("page=2"));
            failed(&mut e, &second, 0, 0, streamed);
            partial(&e.drain_actions().collect::<Vec<_>>(), 1);
        }
    }

    #[test]
    fn mixed_pages_keep_row_warning_and_gain_owned_missing_coverage() {
        for axis in 0..4 {
            for streamed in [false, true] {
                let mut e = ready();
                let q = match axis {
                    0 => fills(Some(5)),
                    1 => orders(),
                    2 => income(None),
                    _ => Query::PublicTrades {
                        symbol: "BTCUSDT".into(),
                        limit: Some(5),
                    },
                };
                let first = query(&mut e, q);
                let valid = match axis {
                    0 => fill(1, NOW, "BUY"),
                    1 => order(1),
                    2 => ledger(1),
                    _ => format!(
                        r#"{{"symbol":"PERP_BTC_USDT","side":"BUY","executed_price":60000,"executed_quantity":0.001,"executed_timestamp":{}}}"#,
                        NOW / 1000.
                    ),
                };
                send(
                    &mut e,
                    &first,
                    &rows(axis, &format!("{valid},{{}}"), 1, 2, 100),
                    streamed,
                );
                let output = e.drain_actions().collect::<Vec<_>>();
                assert!(output.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.code==code::ROW_SKIPPED)));
                partial(&output, 1);
            }
        }
    }

    #[test]
    fn filtered_only_pages_never_authorize_empty_recovery() {
        for axis in [1, 2] {
            let mut e = ready();
            let r = query(
                &mut e,
                if axis == 1 {
                    orders()
                } else {
                    income(Some("REALIZED_PNL"))
                },
            );
            let excluded = if axis == 1 {
                order(1).replace("PERP_BTC_USDT", "PERP_ETH_USDT")
            } else {
                ledger(1)
            };
            send(&mut e, &r, &rows(axis, &excluded, 1, 2, 1), true);
            let second = next(&mut e);
            failed(&mut e, &second, 0, axis, true);
            typed_reject(&e.drain_actions().collect::<Vec<_>>());
        }
    }

    #[test]
    fn native_metadata_and_no_progress_refusals_remain_strict_after_acceptance() {
        for metadata in [false, true] {
            let mut e = ready();
            let r = query(&mut e, orders());
            send(&mut e, &r, &rows(1, &order(1), 1, 3, 1), true);
            let second = next(&mut e);
            send(
                &mut e,
                &second,
                &rows(
                    1,
                    &order(if metadata { 2 } else { 1 }),
                    if metadata { 999 } else { 2 },
                    3,
                    1,
                ),
                true,
            );
            let output = e.drain_actions().collect::<Vec<_>>();
            typed_reject(&output);
        }
    }

    #[test]
    fn native_range_truncation_is_strict_even_with_accepted_rows() {
        let mut e = ready();
        let mut r = query(
            &mut e,
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(NOW - 4000.),
                to_ms: Some(NOW),
                limit: Some(100000),
            },
        );
        for page in 1..=50 {
            send(
                &mut e,
                &r,
                &rows(0, &fill(page, NOW - 1000., "BUY"), 1, 1, 1),
                false,
            );
            if page < 50 {
                r = next(&mut e);
            }
        }
        let output = e.drain_actions().collect::<Vec<_>>();
        typed_reject(&output);
        assert!(
            !output.iter().any(|a| matches!(a, Action::Http(_))),
            "no extra native range request"
        );
    }

    #[test]
    fn terminal_attempt_1000_succeeds_and_a_needed_1001_is_never_sent() {
        for terminal in [false, true] {
            let mut e = ready();
            let mut r = query(&mut e, orders());
            for page in 1..=1000 {
                send(
                    &mut e,
                    &r,
                    &rows(1, &order(page), page, if terminal { 1000 } else { 1001 }, 1),
                    false,
                );
                if page < 1000 {
                    r = next(&mut e);
                }
            }
            let output = e.drain_actions().collect::<Vec<_>>();
            assert!(
                !output.iter().any(|a| matches!(a, Action::Http(_))),
                "never request1001"
            );
            if terminal {
                assert!(markers(&output).is_empty());
                let json = output
                    .iter()
                    .find_map(|a| match a {
                        Action::Notice(Notice::CommandDone {
                            command: 99,
                            result,
                        }) => Some(result),
                        _ => None,
                    })
                    .expect("terminal page1000 completes");
                assert_eq!(
                    serde_json::from_str::<Vec<serde_json::Value>>(json)
                        .unwrap()
                        .len(),
                    1000
                );
            } else {
                assert!(markers(&output).is_empty());
                assert!(output.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:99,error}) if error.message=="WOO history pagination limit exceeded")));
            }
        }
    }

    #[test]
    fn simultaneous_owners_keep_failure_and_accepted_coverage_separate() {
        let mut e = ready();
        e.set_log_level(Level::Error);
        let before = e.store().balance().clone();
        let first = query(&mut e, orders());
        e.handle(
            NOW,
            Event::Command(Command::Query {
                command: 42,
                query: orders(),
            }),
        );
        let peer = requests(&mut e).remove(0);
        send(&mut e, &first, &rows(1, &order(1), 1, 2, 1), true);
        let second = next(&mut e);
        failed(&mut e, &peer, 0, 1, true);
        let peer_output: Vec<_> = e.drain_actions().collect();
        assert!(markers(&peer_output).is_empty());
        assert!(peer_output.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 42, error }) if error.kind == Some(ErrorKind::Internal)
        )));
        failed(&mut e, &second, 0, 1, true);
        partial(&e.drain_actions().collect::<Vec<_>>(), 1);
        assert!(e.store().orders().is_empty());
        assert!(e.store().positions().is_empty());
        assert_eq!(e.store().balance(), &before);
    }

    #[test]
    fn mixed_pages_keep_the_first_gap_and_emit_one_terminal_marker() {
        let mut e = ready();
        let first = query(&mut e, orders());
        send(
            &mut e,
            &first,
            &rows(1, &format!("{},{{}}", order(1)), 1, 5, 3),
            true,
        );
        let mut output: Vec<_> = e.drain_actions().collect();
        let second = output
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.clone()),
                _ => None,
            })
            .unwrap();
        assert!(
            markers(&output).is_empty(),
            "only immediate row warning before terminal delivery"
        );
        send(
            &mut e,
            &second,
            &rows(1, &format!("{},{{}},{{}}", order(2)), 2, 5, 3),
            true,
        );
        output.extend(e.drain_actions());
        partial(&output, 2);
        let gap = markers(&output)[0];
        assert!(gap.message.contains("skipped 1"));
        assert_eq!(output.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::ROW_SKIPPED)).count(), 2);
    }

    #[test]
    fn empty_public_snapshot_completes_once_without_a_private_request() {
        for streamed in [false, true] {
            let mut e = ready();
            let r = query(
                &mut e,
                Query::PublicTrades {
                    symbol: "BTCUSDT".into(),
                    limit: Some(5),
                },
            );
            assert!(r.headers.is_empty());
            send(&mut e, &r, &rows(3, "", 1, 0, 100), streamed);
            let output: Vec<_> = e.drain_actions().collect();
            assert!(markers(&output).is_empty());
            assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
            assert_eq!(output.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 99, result }) if result == "[]")).count(), 1);
        }
    }

    #[test]
    fn consuming_signed_setup_refusal_keeps_legacy_strict_error() {
        let (mut e, _) = started();
        e.drain_actions().for_each(drop);
        e.handle(
            NOW,
            Event::Command(Command::Query {
                command: 99,
                query: fills(Some(5)),
            }),
        );
        let output: Vec<_> = e.drain_actions().collect();
        assert!(markers(&output).is_empty());
        assert!(!output.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::CommandDone { .. })
        )));
        assert!(output.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 99, error })
                if error.kind.is_none() && error.outcome.is_none() && error.message == "WOO private requests require credentials"
        )));
    }

    #[test]
    fn empty_v3_alias_attempts_count_against_the_shared_legacy_page_budget() {
        let mut e = ready();
        let mut r = query(&mut e, fills(Some(2000)));
        for _ in 0..5 {
            send(&mut e, &r, &rows(0, "", 1, 0, 1), false);
            r = next(&mut e);
        }
        for page in 1..=995 {
            send(
                &mut e,
                &r,
                &rows(1, &fill(page as u64, NOW, "BUY"), page, 996, 1),
                false,
            );
            if page < 995 {
                r = next(&mut e);
            }
        }
        let output = e.drain_actions().collect::<Vec<_>>();
        assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(output.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:99,error}) if error.message=="WOO history pagination limit exceeded")));
    }
}

#[test]
fn failed_streamed_fill_prefix_cannot_poison_the_same_identity_on_an_alternative() {
    let mut e = ready();
    let first = query(&mut e, fills(Some(5)));
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: first.id,
            status: 200,
            headers: "",
        },
    );
    let prefix = format!(
        r#"{{"success":true,"data":{{"rows":[{},"#,
        fill(1, NOW, "BUY")
    );
    e.handle(
        NOW + 1.,
        Event::HttpChunk {
            request: first.id,
            data: prefix.as_bytes(),
        },
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: first.id,
            error: "prefix lost",
        },
    );
    let second = requests(&mut e).remove(0);
    assert!(second.url.contains("BTC_USDT_PERP"));
    response(
        &mut e,
        &second,
        NOW + 3.,
        &format!(
            r#"{{"success":true,"data":{{"rows":[{}]}}}}"#,
            fill(1, NOW, "BUY")
        ),
    );
    assert_eq!(result(&mut e).unwrap().matches("timestamp").count(), 1);
}
