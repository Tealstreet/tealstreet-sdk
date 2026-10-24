//! Payload oracles from BingX-API/api-ai-skills swap-{market,trade,account}.
//! fillHistory envelope/time aliases additionally follow the documented TS contract.
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, TimerId,
};
use safe_cex_venue_bingx::Bingx;
mod support;
use serde::Deserialize;
use support::Options;

struct Harness {
    engine: Engine<Bingx>,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    results: Vec<Result<String, String>>,
    now: f64,
}

impl Harness {
    fn new(private: bool) -> Self {
        Self {
            engine: support::engine(Options {
                read_credentials: None,
                public_data_only: !private,
                key: private.then(|| "key".into()),
                secret: private.then(|| "secret".into()),
            }),
            requests: Vec::new(),
            timers: Vec::new(),
            results: Vec::new(),
            now: 1_702_713_615_000.0,
        }
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::SetTimer { timer, at_ms } => {
                    self.timers.retain(|(existing, _)| *existing != timer);
                    self.timers.push((timer, at_ms));
                }
                Action::ClearTimer { timer } => {
                    self.timers.retain(|(existing, _)| *existing != timer)
                }
                Action::Notice(Notice::CommandDone { result, .. }) => self.results.push(Ok(result)),
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    self.results.push(Err(error.legacy_text()))
                }
                _ => {}
            }
        }
    }

    fn query(&mut self, query: Query) {
        self.event(Event::Command(Command::Query { command: 42, query }));
    }

    fn reply(&mut self, body: &str) {
        let request = self.requests.remove(0);
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }

    fn tick(&mut self) {
        self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (timer, at_ms) = self.timers.remove(0);
        self.now = at_ms;
        self.event(Event::TimerFired { timer });
    }

    fn result<T: for<'de> Deserialize<'de>>(&self) -> T {
        serde_json::from_str(self.results.last().unwrap().as_ref().unwrap()).unwrap()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: String,
    commission: f64,
    commission_asset: String,
    realized_pnl: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    trade_id: Option<String>,
    category: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Order {
    id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    price: Option<f64>,
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    position_side: Option<String>,
    created_time: f64,
    updated_time: Option<f64>,
}

fn fills(from: f64, to: f64) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: Some("9223372036854775817".into()),
        from_ms: Some(from),
        to_ms: Some(to),
        limit: Some(2),
    }
}

#[test]
fn public_trades_are_unsigned_and_buyer_maker_means_aggressive_sell() {
    // https://github.com/BingX-API/api-ai-skills/blob/main/swap-market/SKILL.md
    let mut h = Harness::new(false);
    h.query(Query::PublicTrades {
        symbol: "BTCUSDT".into(),
        limit: Some(10_000),
    });
    let request = &h.requests[0];
    assert!(request.url.contains("/openApi/swap/v2/quote/trades?"));
    assert!(request.url.contains("symbol=BTC-USDT"));
    assert!(request.url.contains("limit=1000"));
    assert!(!request.url.contains("signature="));
    h.reply(r#"{"code":0,"msg":"","data":[{"id":9223372036854775817,"price":"42000.1","qty":"0.002","quoteQty":"84.0002","time":1702713615000,"buyerMaker":true}]}"#);
    let rows: Vec<Trade> = h.result();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, "9223372036854775817");
    assert_eq!(row.symbol, "BTCUSDT");
    assert_eq!(
        (row.price, row.qty, row.quote_qty),
        (42000.1, 0.002, 84.0002)
    );
    assert_eq!(row.timestamp, 1_702_713_615_000.0);
    assert_eq!(row.side, "sell");
}

#[test]
fn private_reads_in_public_mode_finish_without_a_request() {
    let mut h = Harness::new(false);
    h.query(fills(1000.0, 2000.0));
    assert!(h.requests.is_empty());
    assert_eq!(h.results, [Ok("[]".into())]);
}

#[test]
fn documented_fill_payloads_support_all_envelopes_and_preserve_execution_ids() {
    // swap-trade: allFillOrders fill fields; fillHistory: fill_orders envelope.
    let data = r#"[{"tradeId":9223372036854775818,"symbol":"BTC-USDT","orderId":9223372036854775817,"side":"BUY","price":"42000.1","qty":"0.002","realizedPnl":"2.5","fee":"-0.042","time":1702713615000}]"#;
    for envelope in [
        data.to_owned(),
        format!(r#"{{"fill_orders":{data}}}"#),
        format!(r#"{{"fill_history_orders":{data}}}"#),
    ] {
        let mut h = Harness::new(true);
        h.query(fills(1_702_713_615_000.0, 1_702_713_615_000.0));
        let url = &h.requests[0].url;
        assert!(url.contains("/openApi/swap/v2/trade/fillHistory?"));
        for parameter in [
            "startTs=1702713615000",
            "endTs=1702713615000",
            "pageSize=2",
            "orderId=9223372036854775817",
            "symbol=BTC-USDT",
            "signature=",
        ] {
            assert!(url.contains(parameter), "{url} lacks {parameter}");
        }
        assert!(!url.contains("startTime="));
        assert!(!url.contains("limit="));
        h.reply(&format!(r#"{{"code":0,"msg":"","data":{envelope}}}"#));
        let rows: Vec<Execution> = h.result();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.id, "9223372036854775818");
        assert_eq!(row.order_id, "9223372036854775817");
        assert_eq!(
            (row.timestamp, row.price, row.qty),
            (1_702_713_615.0, 42000.1, 0.002)
        );
        assert_eq!(row.symbol, "BTCUSDT");
        assert_eq!(row.side, "buy");
        assert_eq!(row.commission, 0.042);
        assert_eq!(row.commission_asset, "USDT");
        assert_eq!(row.realized_pnl, Some(2.5));
    }
}

#[test]
fn fill_iso_offsets_and_legacy_field_names_are_timezone_independent() {
    let mut h = Harness::new(true);
    h.query(fills(1_768_295_597_000.0, 1_768_295_597_000.0));
    h.reply(r#"{"code":0,"data":{"fill_history_orders":[{"fillId":"a","order_id":"b","symbol":"BTC-USDC","side":"SELL","filledPrice":"100","volume":"3","filledTime":"2026-01-13T17:13:17.000+08:00","commission":"-0.1","currency":"USDC"}]}}"#);
    let rows: Vec<Execution> = h.result();
    assert_eq!(rows[0].timestamp, 1_768_295_597.0);
    assert_eq!(rows[0].commission_asset, "USDC");
    assert_eq!(rows[0].symbol, "BTCUSDC");
    assert_eq!(rows[0].side, "sell");
}

#[test]
fn official_income_example_keeps_milliseconds_and_categorizes_funding() {
    // https://github.com/BingX-API/api-ai-skills/blob/main/swap-account/SKILL.md
    let mut h = Harness::new(true);
    h.query(Query::Income {
        symbol: Some("LDOUSDT".into()),
        kind: Some("FUNDING_FEE".into()),
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    assert!(h.requests[0].url.contains("incomeType=FUNDING_FEE"));
    h.reply(r#"{"code":0,"msg":"","data":[{"symbol":"LDO-USDT","incomeType":"FUNDING_FEE","income":"-0.0292","asset":"USDT","info":"Funding Fee","time":1702713615000,"tranId":"170***6*2_3*9_20***97","tradeId":"170***6*2_3*9_20***97"}]}"#);
    let rows: Vec<Income> = h.result();
    let row = &rows[0];
    assert_eq!(row.symbol, "LDOUSDT");
    assert_eq!(row.income_type, "FUNDING_FEE");
    assert_eq!(row.income, -0.0292);
    assert_eq!(row.asset, "USDT");
    assert_eq!(row.timestamp, 1_702_713_615_000.0);
    assert_eq!(row.tran_id, "170***6*2_3*9_20***97");
    assert_eq!(row.trade_id.as_deref(), Some("170***6*2_3*9_20***97"));
    assert_eq!(row.category, "funding");
}

#[test]
fn order_history_uses_binance_shape_and_lossless_ids() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    assert!(h.requests[0].url.contains("/trade/allOrders?"));
    h.reply(r#"{"code":0,"data":[{"orderId":9223372036854775817,"symbol":"BTC-USDT","side":"BUY","positionSide":"LONG","type":"LIMIT","origQty":"0.01","executedQty":"0.005","price":"42000","avgPrice":"41999","status":"CANCELED","time":1702713615000,"updateTime":1702713616000}]}"#);
    let rows: Vec<Order> = h.result();
    let row = &rows[0];
    assert_eq!(row.id, "9223372036854775817");
    assert_eq!(row.symbol, "BTCUSDT");
    assert_eq!(row.side, "buy");
    assert_eq!(row.kind, "limit");
    assert_eq!(row.status, "canceled");
    assert_eq!(row.price, Some(42000.0));
    assert_eq!(row.average_price, Some(41999.0));
    assert_eq!((row.amount, row.filled), (0.01, 0.005));
    assert!(!row.reduce_only);
    assert_eq!(row.position_side.as_deref(), Some("long"));
    assert_eq!(row.created_time, 1_702_713_615_000.0);
    assert_eq!(row.updated_time, Some(1_702_713_616_000.0));
}

#[test]
fn fill_paging_uses_native_pages_and_preserves_distinct_executions() {
    let mut h = Harness::new(true);
    h.query(fills(1000.0, 3000.0));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","time":2500},{"tradeId":"b","orderId":"o","side":"BUY","price":"1","qty":"1","time":1500}]}}"#);
    h.tick();
    assert!(h.requests[0].url.contains("pageIndex=2"));
    assert!(h.requests[0].url.contains("endTs=3000"));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"b","orderId":"o","side":"BUY","price":"1","qty":"1","time":1500},{"tradeId":"c","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}]}}"#);
    h.tick();
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"c","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}]}}"#);
    let rows: Vec<Execution> = h.result();
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
}

#[test]
fn failed_page_reports_error_without_a_partial_success() {
    let mut h = Harness::new(true);
    h.query(fills(1000.0, 3000.0));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","time":2500},{"tradeId":"b","orderId":"o","side":"BUY","price":"1","qty":"1","time":2500}]}}"#);
    h.tick();
    h.reply(r#"{"code":109400,"msg":"invalid query","data":null}"#);
    assert_eq!(h.results.len(), 1);
    assert!(h.results[0].as_ref().unwrap_err().contains("109400"));
}

#[test]
fn streamed_error_envelope_and_malformed_row_fail_the_command() {
    for body in [
        r#"{"code":109400,"msg":"invalid query","data":[]}"#,
        r#"{"code":0,"data":[{"id":"a","price":"1","qty":"2","quoteQty":"2","time":1000,"buyerMaker":true},{"id":"b","price":"bad","qty":"2","quoteQty":"2","time":1000,"buyerMaker":true}]}"#,
    ] {
        let mut h = Harness::new(false);
        h.query(Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: None,
        });
        h.reply(body);
        assert!(h.results[0].is_err());
    }
}

#[test]
fn order_history_splits_ranges_at_the_seven_day_api_limit() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: Some("BTCUSDT".into()),
        from_ms: Some(0.0),
        to_ms: Some(1_209_600_000.0),
        limit: None,
    });
    assert!(h.requests[0].url.contains("startTime=0"));
    assert!(h.requests[0].url.contains("endTime=604800000"));
    h.reply(r#"{"code":0,"data":[]}"#);
    h.tick();
    assert!(h.requests[0].url.contains("startTime=604800000"));
    assert!(h.requests[0].url.contains("endTime=1209600000"));
    h.reply(r#"{"code":0,"data":[]}"#);
    h.tick();
    assert_eq!(h.results, [Ok("[]".into())]);
}

#[test]
fn public_trade_decoder_accepts_chunked_http_without_buffering_the_list() {
    let mut h = Harness::new(false);
    h.query(Query::PublicTrades {
        symbol: "BTCUSDT".into(),
        limit: None,
    });
    let request = h.requests.remove(0).id;
    h.event(Event::HttpStart {
        request,
        status: 200,
        headers: "",
    });
    let body = br#"{"code":0,"data":[{"id":1,"price":"1","qty":"2","quoteQty":"2","time":1000,"buyerMaker":false}]}"#;
    for chunk in body.chunks(7) {
        h.event(Event::HttpChunk {
            request,
            data: chunk,
        });
    }
    h.event(Event::HttpEnd { request });
    let rows: Vec<Trade> = h.result();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].side, "buy");
}

#[test]
fn full_fill_pages_at_one_millisecond_walk_native_pages_until_empty() {
    let mut h = Harness::new(true);
    h.query(fills(1000.0, 1000.0));
    assert!(h.requests[0].url.contains("pageIndex=1"));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000},{"tradeId":"b","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}]}}"#);
    assert!(h.results.is_empty());
    h.tick();
    assert!(h.requests[0].url.contains("pageIndex=2"));
    assert!(h.requests[0].url.contains("startTs=1000"));
    assert!(h.requests[0].url.contains("endTs=1000"));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"c","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000},{"tradeId":"d","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}]}}"#);
    assert!(h.results.is_empty());
    h.tick();
    assert!(h.requests[0].url.contains("pageIndex=3"));
    h.reply(r#"{"code":0,"data":{"fill_orders":[]}}"#);
    let rows: Vec<Execution> = h.result();
    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
}

#[test]
fn ignored_native_page_cursor_fails_instead_of_claiming_completeness() {
    let mut h = Harness::new(true);
    h.query(fills(1000.0, 1000.0));
    let page = r#"{"code":0,"data":{"fill_orders":[{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000},{"tradeId":"b","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}]}}"#;
    h.reply(page);
    h.tick();
    h.reply(page);
    assert!(h.results[0].as_ref().unwrap_err().contains("pagination"));
    assert!(h.requests.is_empty());
}

#[test]
fn native_fill_pages_reset_across_disjoint_seven_day_windows() {
    let mut h = Harness::new(true);
    h.query(fills(0.0, 1_209_600_000.0));
    assert!(h.requests[0].url.contains("endTs=604800000"));
    h.reply(r#"{"code":0,"data":{"fill_orders":[]}}"#);
    h.tick();
    assert!(h.requests[0].url.contains("startTs=604800001"));
    assert!(h.requests[0].url.contains("endTs=1209600000"));
    assert!(h.requests[0].url.contains("pageIndex=1"));
    h.reply(r#"{"code":0,"data":{"fill_orders":[]}}"#);
    assert_eq!(h.results, [Ok("[]".into())]);
}

#[test]
fn saturated_equal_bound_order_and_income_reads_report_incompleteness() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: None,
        from_ms: Some(1000.0),
        to_ms: Some(1000.0),
        limit: Some(1),
    });
    h.reply(r#"{"code":0,"data":[{"orderId":"a","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"FILLED","origQty":"1","executedQty":"1","price":"1","time":1000}]}"#);
    assert!(h.results[0].as_ref().unwrap_err().contains("incomplete"));
    let mut h = Harness::new(true);
    h.query(Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(1000.0),
        to_ms: Some(1000.0),
        limit: Some(1),
    });
    h.reply(r#"{"code":0,"data":[{"symbol":"BTC-USDT","incomeType":"FUNDING_FEE","income":"1","asset":"USDT","time":1000,"tranId":"a"}]}"#);
    assert!(h.results[0].as_ref().unwrap_err().contains("incomplete"));
}

#[test]
fn blank_or_missing_mandatory_native_identifiers_fail_without_deduplication() {
    for id in ["", " "] {
        let mut h = Harness::new(true);
        h.query(Query::OrderHistory {
            symbol: None,
            from_ms: Some(1000.0),
            to_ms: Some(2000.0),
            limit: Some(2),
        });
        h.reply(&format!(r#"{{"code":0,"data":[{{"orderId":"{id}","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"FILLED","origQty":"1","executedQty":"1","price":"1","time":1000}}]}}"#));
        assert!(h.results[0].is_err());
        let mut h = Harness::new(true);
        h.query(fills(1000.0, 1000.0));
        h.reply(&format!(r#"{{"code":0,"data":{{"fill_orders":[{{"tradeId":"{id}","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}}]}}}}"#));
        assert!(h.results[0].is_err());
    }
    let mut h = Harness::new(true);
    h.query(fills(1000.0, 1000.0));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}]}}"#);
    assert!(h.results[0].is_err());
}

#[test]
fn oversized_iso_year_is_an_error_and_cannot_panic_the_engine() {
    let mut h = Harness::new(true);
    h.query(fills(1000.0, 1000.0));
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","filledTime":"10000000-01-01T00:00:00Z"}]}}"#);
    assert!(h.results[0].as_ref().unwrap_err().contains("timestamp"));
}

#[test]
fn historical_orders_accept_documented_working_status_and_string_boolean() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    h.reply(r#"{"code":0,"data":[{"orderId":"a","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"WORKING","origQty":"1","executedQty":"0","price":"1","time":1000,"reduceOnly":"true"}]}"#);
    let rows: Vec<Order> = h.result();
    assert_eq!(rows[0].status, "open");
    assert!(rows[0].reduce_only);
}

#[test]
fn fills_stream_chunked_direct_and_both_nested_aliases_without_losing_rows() {
    let rows = r#"[{"tradeId":9223372036854775818,"orderId":"o","side":"BUY","filledPrice":"10","volume":"0.1","filledTm":"1000","commission":"-0.01","currency":"USDT"}]"#;
    for data in [
        rows.to_owned(),
        format!(r#"{{"fill_orders":{rows}}}"#),
        format!(r#"{{"fill_history_orders":{rows}}}"#),
    ] {
        for width in [1, 7, 1000] {
            let mut h = Harness::new(true);
            h.query(fills(1000.0, 1000.0));
            let request = h.requests.remove(0).id;
            h.event(Event::HttpStart {
                request,
                status: 200,
                headers: "",
            });
            // Code after the array must still be checked after streamed rows.
            let body = format!(r#"{{"data":{data},"code":0,"msg":""}}"#);
            for chunk in body.as_bytes().chunks(width) {
                h.event(Event::HttpChunk {
                    request,
                    data: chunk,
                });
            }
            h.event(Event::HttpEnd { request });
            let rows: Vec<Execution> = h.result();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].id, "9223372036854775818");
            assert_eq!(rows[0].timestamp, 1.0);
            assert_eq!(
                (rows[0].price, rows[0].qty, rows[0].commission),
                (10.0, 0.1, 0.01)
            );
        }
    }
}

#[test]
fn fills_stream_error_metadata_and_truncation_discard_already_decoded_rows() {
    let row = r#"{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","time":1000}"#;
    for body in [
        format!(r#"{{"data":{{"fill_orders":[{row}]}},"code":109400,"msg":"denied"}}"#),
        format!(r#"{{"code":0,"data":{{"fill_history_orders":[{row}"#),
    ] {
        let mut h = Harness::new(true);
        h.query(fills(1000.0, 1000.0));
        h.reply(&body);
        assert_eq!(h.results.len(), 1);
        assert!(h.results[0].is_err());
    }
}
