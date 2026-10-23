//! Independent native contracts from Aster V1/V3 userTrades, allOrders, income,
//! and trades documentation. Cumulative-PnL cases also exercise TS's stated rule.
//! https://github.com/asterdex/api-docs/tree/master/V3(Recommended)/EN
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, TimerId,
};
mod support;
use safe_cex_venue_asterdex::{AsterDex, sign::Version};
use serde_json::{Value, json};
use support::Options;

struct Harness {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, String>)>,
}

impl Harness {
    fn new() -> Self {
        Self::options(Options {
            key: Some("synthetic-key".into()),
            secret: Some("synthetic-secret".into()),
            ..Options::default()
        })
    }
    fn v3() -> Self {
        Self::options(Options {
            key: Some("0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0".into()),
            secret: Some(
                "0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1".into(),
            ),
            application_id: Some("0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e".into()),
            ..Options::default()
        })
    }
    fn options(options: Options) -> Self {
        Self {
            engine: support::engine(options),
            now: 1700000000000.0,
            requests: Vec::new(),
            timers: Vec::new(),
            done: Vec::new(),
        }
    }
    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now).into_iter() {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())))
                }
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn query(&mut self, command: u32, query: Query) {
        self.event(Event::Command(Command::Query { command, query }));
        if let Some(index) = self
            .requests
            .iter()
            .position(|request| request.url.ends_with("/time"))
        {
            let clock = self.requests.remove(index);
            self.reply(clock, 200, r#"{"serverTime":1700000000000}"#);
        }
    }
    fn take(&mut self, endpoint: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|request| request.url.split('?').next().unwrap().ends_with(endpoint))
            .unwrap_or_else(|| panic!("missing {endpoint}: {:?}", self.requests));
        self.requests.remove(index)
    }
    fn reply(&mut self, request: HttpRequest, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn result(&mut self, command: u32) -> Result<Value, String> {
        let index = self
            .done
            .iter()
            .position(|(id, _)| *id == command)
            .expect("command completed");
        self.done
            .remove(index)
            .1
            .map(|body| serde_json::from_str(&body).unwrap())
    }
    fn fire(&mut self, delay: f64) {
        let index = self
            .timers
            .iter()
            .position(|(_, at)| (*at - self.now - delay).abs() < 0.01)
            .expect("expected timer");
        let (timer, at) = self.timers.remove(index);
        self.now = at;
        self.event(Event::TimerFired { timer });
    }
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
fn income(from: Option<f64>, to: Option<f64>, limit: Option<u32>) -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: from,
        to_ms: to,
        limit,
    }
}
fn fill(id: u64, time: u64, pnl: &str, qty: &str, symbol: &str, rail: &str) -> Value {
    json!({"id":id,"orderId":25851813,"symbol":symbol,"side":"SELL","positionSide":rail,"price":"7819.01","qty":qty,"realizedPnl":pnl,"time":time,"commission":"-0.07819010","commissionAsset":"USDT","maker":false})
}
fn ledger(id: &str, time: u64) -> Value {
    json!({"symbol":"BTCUSDT","incomeType":"COMMISSION","income":"-0.01","asset":"USDT","time":time,"tranId":id,"tradeId":"2059192"})
}
fn field<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request.url.split('?').nth(1)?.split('&').find_map(|part| {
        part.split_once('=')
            .filter(|(key, _)| *key == name)
            .map(|(_, value)| value)
    })
}

#[test]
fn documented_execution_fields_fees_identity_and_seconds_are_preserved() {
    let mut h = Harness::new();
    h.query(
        1,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("25851813".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(5000),
        },
    );
    let request = h.take("/userTrades");
    assert_eq!(field(&request, "symbol"), Some("BTCUSDT"));
    assert_eq!(field(&request, "orderId"), Some("25851813"));
    assert_eq!(field(&request, "startTime"), None);
    assert_eq!(field(&request, "endTime"), None);
    assert_eq!(field(&request, "limit"), Some("1000"));
    assert!(request.body.is_none());
    h.reply(
        request,
        200,
        &json!([fill(
            698759,
            1569514978020,
            "-0.91539999",
            "0.002",
            "BTCUSDT",
            "SHORT"
        )])
        .to_string(),
    );
    assert_eq!(
        h.result(1).unwrap(),
        json!([{"id":"698759","timestamp":1569514978.0,"symbol":"BTCUSDT","orderId":"25851813","price":7819.01,"qty":0.002,"side":"sell","realizedPnl":0.0,"positionSide":"short","maker":false,"commission":-0.07819010,"commissionAsset":"USDT"}])
    );
}

#[test]
fn cumulative_pnl_uses_time_id_and_rail_baselines_without_reordering_results() {
    let mut h = Harness::new();
    h.query(1, fills());
    let request = h.take("/userTrades");
    let rows = json!([
        fill(30, 3000, "130", "1", "BTCUSDT", "LONG"),
        fill(20, 1000, "120", "1", "BTCUSDT", "Long"),
        fill(10, 1000, "100", "1", "BTCUSDT", "long"),
        fill(40, 2000, "-10", "1", "BTCUSDT", "SHORT"),
        fill(50, 4000, "-7", "1", "BTCUSDT", "SHORT"),
        fill(60, 2000, "999", "1", "ETHUSDT", "LONG"),
        fill(70, 3000, "3", "1", "BTCUSDT", "BOTH")
    ]);
    h.reply(request, 200, &rows.to_string());
    let rows = h.result(1).unwrap();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["30", "20", "10", "40", "50", "60", "70"]
    );
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| row["realizedPnl"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        [10.0, 20.0, 0.0, 0.0, 3.0, 0.0, 0.0]
    );
    assert_eq!(rows[1]["positionSide"], "long");
}

#[test]
fn invalid_execution_sizes_are_filtered_after_their_cumulative_pnl_baseline() {
    let mut h = Harness::new();
    h.query(1, fills());
    let request = h.take("/userTrades");
    h.reply(
        request,
        200,
        &json!([
            fill(1, 1000, "100", "1", "BTCUSDT", "BOTH"),
            fill(2, 2000, "120", "0", "BTCUSDT", "BOTH"),
            fill(3, 3000, "125", "1", "BTCUSDT", "BOTH"),
            fill(4, 4000, "130", "NaN", "BTCUSDT", "BOTH"),
            fill(5, 5000, "135", "1", "BTCUSDT", "BOTH")
        ])
        .to_string(),
    );
    let rows = h.result(1).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert_eq!(rows[1]["id"], "3");
    assert_eq!(rows[1]["realizedPnl"], 5.0);
    assert_eq!(rows[2]["realizedPnl"], 5.0);
}

#[test]
fn omitted_or_invalid_pnl_never_changes_the_next_valid_baseline() {
    let mut h = Harness::new();
    h.query(1, fills());
    let request = h.take("/userTrades");
    let mut missing = fill(2, 2000, "0", "1", "BTCUSDT", "BOTH");
    missing.as_object_mut().unwrap().remove("realizedPnl");
    h.reply(
        request,
        200,
        &json!([
            fill(1, 1000, "100", "1", "BTCUSDT", "BOTH"),
            missing,
            fill(3, 3000, "Infinity", "1", "BTCUSDT", "BOTH"),
            fill(4, 4000, "103", "1", "BTCUSDT", "BOTH")
        ])
        .to_string(),
    );
    let rows = h.result(1).unwrap();
    assert!(rows[1].get("realizedPnl").is_none());
    assert!(rows[2].get("realizedPnl").is_none());
    assert_eq!(rows[3]["realizedPnl"], 3.0);
}

#[test]
fn overflowing_cumulative_pnl_delta_uses_ts_zero_fallback() {
    let mut h = Harness::new();
    h.query(1, fills());
    let request = h.take("/userTrades");
    h.reply(
        request,
        200,
        &json!([
            fill(1, 1000, "-1e308", "1", "BTCUSDT", "BOTH"),
            fill(2, 2000, "1e308", "1", "BTCUSDT", "BOTH")
        ])
        .to_string(),
    );
    let rows = h.result(1).unwrap();
    assert_eq!(rows[0]["realizedPnl"], 0.0);
    assert_eq!(rows[1]["realizedPnl"], 0.0);
}

#[test]
fn documented_income_string_ids_negative_amounts_and_optional_trade_ids_decode() {
    let mut h = Harness::new();
    h.query(1, income(None, None, None));
    let request = h.take("/income");
    assert_eq!(field(&request, "limit"), Some("100"));
    h.reply(request, 200, r#"[{"symbol":"","incomeType":"TRANSFER","income":"-0.37500000","asset":"USDT","info":"TRANSFER","time":1570608000000,"tranId":"9689322392","tradeId":""},{"symbol":"BTCUSDT","incomeType":"COMMISSION","income":"-0.01000000","asset":"USDT","info":"COMMISSION","time":1570636800000,"tranId":"9689322392","tradeId":"2059192"}]"#);
    assert_eq!(
        h.result(1).unwrap(),
        json!([
            {"symbol":"","incomeType":"TRANSFER","income":-0.375,"asset":"USDT","timestamp":1570608000000.0,"tranId":"9689322392","category":"transfer"},
            {"symbol":"BTCUSDT","incomeType":"COMMISSION","income":-0.01,"asset":"USDT","timestamp":1570636800000.0,"tranId":"9689322392","tradeId":"2059192","category":"commission"}
        ])
    );
}

#[test]
fn income_numeric_ids_and_documented_categories_remain_typed() {
    let mut h = Harness::new();
    h.query(
        1,
        Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(0),
        },
    );
    let request = h.take("/income");
    assert_eq!(field(&request, "limit"), Some("1"));
    assert_eq!(field(&request, "symbol"), Some("BTCUSDT"));
    assert_eq!(field(&request, "incomeType"), Some("FUNDING_FEE"));
    h.reply(request, 200, r#"[{"incomeType":"FUNDING_FEE","income":"0.1","asset":"USDT","time":1000,"tranId":123},{"incomeType":"MARKET_MAKER_REBATE","income":"0.2","asset":"USDT","time":2000,"tranId":124},{"incomeType":"CROSS_COLLATERAL_TRANSFER","income":"0.3","asset":"USDT","time":3000,"tranId":125}]"#);
    let rows = h.result(1).unwrap();
    assert_eq!(rows[0]["tranId"], "123");
    assert_eq!(rows[0]["category"], "funding");
    assert_eq!(rows[1]["category"], "rebate");
    assert_eq!(rows[2]["category"], "transfer");
}

#[test]
fn public_trades_accept_documented_optional_quote_quantity_without_auth() {
    let mut h = Harness::options(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.query(
        1,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(5000),
        },
    );
    let request = h.take("/trades");
    assert!(request.headers.is_empty());
    assert_eq!(field(&request, "limit"), Some("1000"));
    h.reply(request, 200, r#"[{"id":28457,"price":"4.00000100","qty":"12.00000000","time":1499865549590,"isBuyerMaker":true},{"id":28458,"price":"4.00000100","qty":"12.00000000","quoteQty":"49.5","time":1499865549591,"isBuyerMaker":false}]"#);
    let rows = h.result(1).unwrap();
    assert_eq!(rows[0]["side"], "sell");
    assert_eq!(rows[0]["quoteQty"], 4.000001 * 12.0);
    assert_eq!(rows[0]["timestamp"], 1499865549590.0);
    assert_eq!(rows[1]["quoteQty"], 49.5);
    assert_eq!(rows[1]["side"], "buy");
}

#[test]
fn documented_order_history_handles_trailing_and_close_position_semantics() {
    let mut h = Harness::new();
    h.query(
        1,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let request = h.take("/allOrders");
    h.reply(request, 200, r#"[{"avgPrice":"0.00000","clientOrderId":"abc","cumQuote":"0","executedQty":"0","orderId":1917641,"origQty":"0.40","origType":"TRAILING_STOP_MARKET","price":"0","reduceOnly":false,"side":"BUY","positionSide":"SHORT","status":"NEW","stopPrice":"9300","closePosition":false,"symbol":"BTCUSDT","time":1579276756075,"timeInForce":"GTC","type":"TRAILING_STOP_MARKET","activatePrice":"9020","priceRate":"0.3","updateTime":1579276756075,"workingType":"CONTRACT_PRICE","priceProtect":false},{"orderId":1917642,"symbol":"BTCUSDT","status":"FILLED","price":"0","avgPrice":"9300","origQty":"0","executedQty":"1","type":"STOP_MARKET","side":"SELL","closePosition":true,"reduceOnly":false,"positionSide":"LONG","time":1579276756076}]"#);
    let rows = h.result(1).unwrap();
    assert_eq!(rows[0]["id"], "1917642");
    assert_eq!(rows[0]["reduceOnly"], true);
    assert_eq!(rows[0]["status"], "closed");
    assert_eq!(rows[0]["averagePrice"], 9300.0);
    assert_eq!(rows[1]["type"], "trailing_stop_market");
    assert!(rows[1].get("price").is_none());
    assert!(rows[1].get("averagePrice").is_none());
}

#[test]
fn required_symbol_checks_and_v3_account_wide_order_history_follow_native_versions() {
    let mut h = Harness::new();
    h.query(
        1,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert_eq!(h.result(1).unwrap(), json!([]));
    h.query(
        2,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert_eq!(h.result(2).unwrap(), json!([]));
    assert!(h.requests.is_empty());
    let mut h = Harness::v3();
    h.query(
        3,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let request = h.take("/allOrders");
    assert!(request.url.contains("/fapi/v3/allOrders?"));
    assert_eq!(field(&request, "symbol"), None);
    h.reply(request, 200, "[]");
    assert_eq!(h.result(3).unwrap(), json!([]));
}

#[test]
fn invalid_ranges_fail_before_io_instead_of_silently_shortening() {
    for query in [
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(0.0),
            to_ms: Some(604800001.0),
            limit: None,
        },
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(100.0),
            to_ms: Some(99.0),
            limit: None,
        },
        income(Some(1.0), Some(1.0), None),
        income(Some(f64::NAN), None, None),
    ] {
        let mut h = Harness::new();
        h.query(1, query);
        assert!(h.result(1).is_err());
        assert!(h.requests.is_empty());
    }
}

#[test]
fn authentication_and_malformed_stream_failures_never_complete_as_empty_success() {
    for (status, body) in [
        (401, r#"{"code":-2015,"msg":"Invalid API-key"}"#),
        (200, r#"{"code":"-2015","msg":"Invalid API-key"}"#),
        (200, r#"[{"symbol":"BTCUSDT"}]"#),
        (200, r#"{"unexpected":[]}"#),
        (200, "["),
    ] {
        let mut h = Harness::new();
        h.query(1, fills());
        let request = h.take("/userTrades");
        h.reply(request, status, body);
        // A truncated transport body follows the bounded safe-read retry path.
        for attempt in 1..=3 {
            if !h.done.is_empty() {
                break;
            }
            h.fire(f64::from(attempt) * 1000.0);
            let request = h.take("/userTrades");
            h.reply(request, status, body);
        }
        assert!(h.result(1).is_err(), "{body}");
    }
    // An explicit V3 wallet key that is not hex never reaches the venue.
    let setup = Options {
        key: Some("agent".into()),
        secret: Some("bad-key".into()),
        api_version: Some(Version::V3),
        ..Options::default()
    };
    assert!(setup.try_build().is_err());
}

#[test]
fn failed_stream_retry_discards_staged_rows_before_successful_replacement() {
    let mut h = Harness::new();
    h.query(1, fills());
    let request = h.take("/userTrades");
    let first = fill(1, 1000, "100", "1", "BTCUSDT", "BOTH");
    h.event(Event::HttpStart {
        request: request.id,
        status: 200,
        headers: "",
    });
    h.event(Event::HttpChunk {
        request: request.id,
        data: format!("[{first},").as_bytes(),
    });
    h.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert!(h.done.is_empty());
    h.fire(1000.0);
    let request = h.take("/userTrades");
    h.reply(
        request,
        200,
        &json!([first, fill(2, 2000, "103", "1", "BTCUSDT", "BOTH")]).to_string(),
    );
    let rows = h.result(1).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[1]["realizedPnl"], 3.0);
}

#[test]
fn income_range_walks_pages_and_deduplicates_inclusive_boundaries() {
    let mut h = Harness::new();
    h.query(1, income(Some(0.0), Some(10_000.0), Some(2)));
    let request = h.take("/income");
    assert_eq!(field(&request, "limit"), Some("2"));
    h.reply(
        request,
        200,
        &json!([ledger("1", 1000), ledger("2", 2000)]).to_string(),
    );
    let request = h.take("/income");
    assert_eq!(field(&request, "startTime"), Some("2000"));
    h.reply(
        request,
        200,
        &json!([ledger("2", 2000), ledger("3", 3000)]).to_string(),
    );
    let request = h.take("/income");
    h.reply(request, 200, &json!([ledger("3", 3000)]).to_string());
    let rows = h.result(1).unwrap();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| row["tranId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
}

#[test]
fn income_range_paces_seven_day_chunks_and_never_returns_failed_partial_results() {
    let mut h = Harness::new();
    h.query(1, income(Some(0.0), Some(604801000.0), None));
    let request = h.take("/income");
    assert_eq!(field(&request, "endTime"), Some("604800000"));
    h.reply(request, 200, "[]");
    assert!(h.requests.is_empty());
    assert!(h.done.is_empty());
    h.fire(2000.0);
    let request = h.take("/income");
    assert_eq!(field(&request, "startTime"), Some("604800000"));
    h.reply(request, 401, r#"{"code":-2015,"msg":"Invalid API-key"}"#);
    assert!(h.result(1).is_err());
}

#[test]
fn income_range_failure_after_collected_rows_returns_error_and_dense_ties_are_explicit() {
    let mut h = Harness::new();
    h.query(1, income(Some(0.0), Some(10000.0), Some(2)));
    let request = h.take("/income");
    h.reply(
        request,
        200,
        &json!([ledger("1", 1000), ledger("2", 2000)]).to_string(),
    );
    let request = h.take("/income");
    h.reply(
        request,
        200,
        r#"[{"incomeType":"COMMISSION","income":"broken"}]"#,
    );
    assert!(h.result(1).is_err());

    let mut h = Harness::new();
    h.query(1, income(Some(0.0), Some(10000.0), Some(2)));
    for _ in 0..50 {
        if !h.done.is_empty() {
            break;
        }
        let request = h.take("/income");
        h.reply(
            request,
            200,
            &json!([ledger("1", 1000), ledger("2", 1000)]).to_string(),
        );
    }
    assert!(h.result(1).unwrap_err().contains("could not be exhausted"));
}

fn bounded_fills(from: f64, to: f64, limit: u32) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: Some(from),
        to_ms: Some(to),
        limit: Some(limit),
    }
}

#[test]
fn bounded_fills_walk_seven_day_chunks_before_normalizing_cumulative_pnl() {
    for v3 in [false, true] {
        let mut h = if v3 { Harness::v3() } else { Harness::new() };
        h.query(1, bounded_fills(0.0, 604801000.0, 5));
        let first = h.take("/userTrades");
        assert!(first.url.contains(if v3 { "/v3/" } else { "/v1/" }));
        assert_eq!(field(&first, "endTime"), Some("604800000"));
        let boundary = fill(2, 604800000, "10", "1", "BTCUSDT", "LONG");
        h.reply(
            first,
            200,
            &json!([fill(1, 1, "5", "1", "BTCUSDT", "LONG"), boundary.clone()]).to_string(),
        );
        let edge = h.take("/userTrades");
        h.reply(edge, 200, &json!([boundary.clone()]).to_string());
        h.fire(2000.0);
        let second = h.take("/userTrades");
        assert_eq!(field(&second, "startTime"), Some("604800000"));
        h.reply(
            second,
            200,
            &json!([boundary, fill(3, 604801000, "15", "1", "BTCUSDT", "LONG")]).to_string(),
        );
        let edge = h.take("/userTrades");
        h.reply(
            edge,
            200,
            &json!([fill(3, 604801000, "15", "1", "BTCUSDT", "LONG")]).to_string(),
        );
        let result = h.result(1).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 3);
        assert_eq!(result[0]["realizedPnl"], 0.0);
        assert_eq!(result[1]["realizedPnl"], 5.0);
        assert_eq!(result[2]["realizedPnl"], 5.0);
    }
}

#[test]
fn bounded_order_filter_uses_other_orders_and_invalid_sizes_as_pnl_baselines() {
    let mut h = Harness::v3();
    h.query(
        1,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("42".into()),
            from_ms: Some(1000.0),
            to_ms: Some(2000.0),
            limit: Some(10),
        },
    );
    let request = h.take("/userTrades");
    assert_eq!(field(&request, "orderId"), None);
    assert_eq!(field(&request, "fromId"), None);
    let mut target = fill(11, 1500, "15", "1", "BTCUSDT", "LONG");
    target["orderId"] = json!(42);
    // Same native millisecond and reverse wire ordering: numeric id decides PnL.
    h.reply(
        request,
        200,
        &json!([target, fill(10, 1500, "10", "0", "BTCUSDT", "LONG")]).to_string(),
    );
    let request = h.take("/userTrades");
    h.reply(request, 200, "[]");
    let request = h.take("/userTrades");
    h.reply(request, 200, "[]");
    let rows = h.result(1).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "11");
    assert_eq!(rows[0]["realizedPnl"], 5.0);
}

#[test]
fn bounded_fills_failure_after_a_page_and_saturated_milliseconds_never_return_partial() {
    let mut h = Harness::new();
    h.query(1, bounded_fills(0.0, 10000.0, 2));
    let first = h.take("/userTrades");
    h.reply(
        first,
        200,
        &json!([fill(1, 1000, "5", "1", "BTCUSDT", "LONG")]).to_string(),
    );
    let next = h.take("/userTrades");
    h.reply(next, 200, r#"[{"id":"malformed"}]"#);
    assert!(h.result(1).unwrap_err().contains("Malformed"));

    for limit in [1, 2] {
        let mut h = Harness::new();
        h.query(1, bounded_fills(1000.0, 1000.0, limit));
        let request = h.take("/userTrades");
        assert_eq!(field(&request, "startTime"), Some("1000"));
        assert_eq!(field(&request, "endTime"), Some("1000"));
        h.reply(
            request,
            200,
            &json!([fill(1, 1000, "5", "1", "BTCUSDT", "LONG")]).to_string(),
        );
        if limit == 1 {
            assert!(h.result(1).unwrap_err().contains("saturated"));
        } else {
            assert_eq!(h.result(1).unwrap().as_array().unwrap().len(), 1);
        }
        assert!(h.requests.is_empty());
    }
}

#[test]
fn bounded_fills_refuse_dense_ties_wrong_symbols_and_out_of_window_rows() {
    let mut h = Harness::new();
    h.query(1, bounded_fills(0.0, 10000.0, 2));
    for _ in 0..50 {
        if !h.done.is_empty() {
            break;
        }
        let request = h.take("/userTrades");
        let includes_tie = field(&request, "startTime")
            .unwrap()
            .parse::<f64>()
            .unwrap()
            <= 1000.0
            && field(&request, "endTime").unwrap().parse::<f64>().unwrap() >= 1000.0;
        let rows = if includes_tie {
            json!([
                fill(1, 1000, "5", "1", "BTCUSDT", "LONG"),
                fill(2, 1000, "10", "1", "BTCUSDT", "LONG")
            ])
        } else {
            json!([])
        };
        h.reply(request, 200, &rows.to_string());
    }
    assert!(h.result(1).unwrap_err().contains("could not be exhausted"));
    for row in [
        fill(1, 1000, "5", "1", "ETHUSDT", "LONG"),
        fill(1, 3000, "5", "1", "BTCUSDT", "LONG"),
    ] {
        let mut h = Harness::new();
        h.query(1, bounded_fills(1000.0, 2000.0, 2));
        let request = h.take("/userTrades");
        h.reply(request, 200, &json!([row]).to_string());
        assert!(h.result(1).unwrap_err().contains("Malformed"));
    }
}

#[test]
fn bounded_fills_retry_discards_partial_page_but_retains_prior_pnl_baseline() {
    let mut h = Harness::new();
    h.query(1, bounded_fills(0.0, 10000.0, 5));
    let request = h.take("/userTrades");
    h.reply(
        request,
        200,
        &json!([fill(1, 1000, "5", "1", "BTCUSDT", "LONG")]).to_string(),
    );
    let request = h.take("/userTrades");
    h.event(Event::HttpStart {
        request: request.id,
        status: 200,
        headers: "",
    });
    let partial = format!("[{},", fill(99, 1500, "500", "1", "BTCUSDT", "LONG"));
    h.event(Event::HttpChunk {
        request: request.id,
        data: partial.as_bytes(),
    });
    h.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    let index = h
        .timers
        .iter()
        .position(|(_, at)| *at - h.now >= 750.0 && *at - h.now <= 1000.0)
        .unwrap();
    let (timer, at) = h.timers.remove(index);
    h.now = at;
    h.event(Event::TimerFired { timer });
    let request = h.take("/userTrades");
    assert_eq!(field(&request, "startTime"), Some("1000"));
    h.reply(
        request,
        200,
        &json!([
            fill(1, 1000, "5", "1", "BTCUSDT", "LONG"),
            fill(2, 2000, "10", "1", "BTCUSDT", "LONG")
        ])
        .to_string(),
    );
    let request = h.take("/userTrades");
    h.reply(
        request,
        200,
        &json!([fill(2, 2000, "10", "1", "BTCUSDT", "LONG")]).to_string(),
    );
    let request = h.take("/userTrades");
    assert_eq!(field(&request, "endTime"), Some("1000"));
    h.reply(
        request,
        200,
        &json!([fill(1, 1000, "5", "1", "BTCUSDT", "LONG")]).to_string(),
    );
    let result = h.result(1).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 2);
    assert_eq!(result[1]["realizedPnl"], 5.0);
}

#[test]
fn bounded_fills_empty_optional_order_id_still_returns_all_orders() {
    let mut h = Harness::new();
    h.query(
        1,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some(String::new()),
            from_ms: Some(1000.0),
            to_ms: Some(1000.0),
            limit: Some(10),
        },
    );
    let request = h.take("/userTrades");
    assert_eq!(field(&request, "orderId"), None);
    h.reply(
        request,
        200,
        &json!([fill(1, 1000, "5", "1", "BTCUSDT", "LONG")]).to_string(),
    );
    assert_eq!(h.result(1).unwrap().as_array().unwrap().len(), 1);
}
