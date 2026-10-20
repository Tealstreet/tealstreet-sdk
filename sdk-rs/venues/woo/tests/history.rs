mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, Response};
use safe_cex_venue_woo::{Options, Woo};
use serde::Deserialize;
use support::*;

const NOW: f64 = 1_786_603_865_910.0;
fn ready() -> TestEngine {
    let mut e = Engine::new(Woo::new(Options {
        public_data_only: false,
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = requests(&mut e).remove(0);
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
                Some(result)
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
        assert!(rows.as_array().unwrap().iter().all(|r| r["symbol"] == "BTCUSDT"));
        assert!(e.store().orders().is_empty());
        assert!(e.store().positions().is_empty());
    }
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
    response(
        &mut e,
        &r,
        NOW + 1.0,
        r#"{"success":true,"data":{"rows":[]}}"#,
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("/v1/client/trades"));
    assert!(r.url.contains("size=2"));
    assert!(r.url.contains("page=1"));
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
fn ledger_pages_obey_ten_requests_per_minute_and_failure_is_not_empty() {
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
    assert!(result(&mut e).is_err());
}
#[test]
fn malformed_history_page_rejects_valid_prefix() {
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
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
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
fn required_and_optional_history_ids_keep_distinct_empty_and_null_rules() {
    #[derive(Deserialize)]
    struct Execution {
        id: String,
        #[serde(rename = "orderId")]
        order_id: Option<String>,
    }
    for (raw, expected) in [
        ("null", None),
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
    for raw in ["null", r#""""#, "-1", "-0", "1.0", "true", "[]"] {
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
