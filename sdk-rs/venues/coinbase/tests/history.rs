use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, TimerId,
};
use safe_cex_venue_coinbase::{Coinbase, Options};
use serde::Deserialize;
const NOW: f64 = 1704067200000.0;
fn market(symbol: &str, cfm: bool) -> Market {
    Market {
        id: symbol.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: if cfm { 1.0 } else { 0.001 },
            price: 0.01,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 10.0,
            },
            price: None,
        },
        contract_size: Some(if cfm { 0.01 } else { 1.0 }),
        is_expiring: Some(cfm),
        is_inverse: None,
        is_spot: Some(!cfm),
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
struct Driver {
    e: Engine<Coinbase>,
    timers: Vec<(f64, TimerId)>,
    now: f64,
    result: Option<Result<String, String>>,
    events: Vec<safe_cex_engine::log::LogEvent>,
    completed: Vec<(u32, bool)>,
    error: Option<safe_cex_engine::VenueError>,
}
impl Driver {
    fn new(private: bool, symbol: &str, cfm: bool) -> Self {
        let options = if private {
            Options {
                key: "organizations/test/apiKeys/test".into(),
                secret: STANDARD.encode([7; 32]),
                ..Default::default()
            }
        } else {
            Options::default()
        };
        let mut e = safe_cex_engine::test_support::seeded_engine(Coinbase::new(options).unwrap());
        e.store_mut().set_markets(vec![market(symbol, cfm)]);
        Self {
            e,
            timers: vec![],
            now: NOW,
            result: None,
            events: vec![],
            completed: vec![],
            error: None,
        }
    }
    fn query(&mut self, q: Query) {
        self.e.handle(
            self.now,
            Event::Command(Command::Query {
                command: 77,
                query: q,
            }),
        );
    }
    fn collect(&mut self) -> Vec<HttpRequest> {
        let mut requests = vec![];
        for a in self.e.drain_actions() {
            match a {
                Action::Http(r) => requests.push(r),
                Action::SetTimer { timer, at_ms } => self.timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => self.timers.retain(|(_, t)| *t != timer),
                Action::Notice(Notice::CommandDone {
                    command: 77,
                    result,
                }) => {
                    self.completed.push((77, true));
                    self.result = Some(Ok(result));
                }
                Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                    self.completed.push((77, false));
                    self.result = Some(Err(error.legacy_text()));
                    self.error = Some(error)
                }
                Action::Notice(Notice::Event(event)) => self.events.push(event),
                Action::Notice(Notice::CommandDone { command, .. }) => {
                    self.completed.push((command, true))
                }
                Action::Notice(Notice::CommandFailed { command, .. }) => {
                    self.completed.push((command, false))
                }
                _ => {}
            }
        }
        requests
    }
    fn request(&mut self) -> HttpRequest {
        for _ in 0..5 {
            if let Some(r) = self.collect().pop() {
                return r;
            }
            self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (at, t) = self.timers.remove(0);
            self.now = at;
            self.e.handle(at, Event::TimerFired { timer: t });
        }
        panic!("history request missing")
    }
    fn reply(&mut self, r: HttpRequest, status: u16, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn result(&mut self) -> Result<String, String> {
        assert!(self.collect().is_empty());
        self.result.take().expect("query completion missing")
    }
}
fn fills(limit: u32) -> Query {
    Query::Fills {
        symbol: Some("BTC-USD".into()),
        order_id: Some("native-order".into()),
        from_ms: Some(NOW),
        to_ms: Some(NOW + 1000.0),
        limit: Some(limit),
    }
}
fn fill(id: &str, price: &str, size: &str, in_quote: bool, time: &str) -> String {
    format!(
        r#"{{"entry_id":"entry-{id}","trade_id":"{id}","order_id":"native-order","product_id":"BTC-USD","trade_type":"FILL","trade_time":"{time}","price":"{price}","size":"{size}","size_in_quote":{in_quote},"commission":"0.1","side":"BUY","liquidity_indicator":"MAKER"}}"#
    )
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
    commission: f64,
    maker: Option<bool>,
}
#[test]
fn paged_fills_preserve_native_ids_units_bounds_and_overlap_deduplication() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(2));
    let r = d.request();
    assert!(
        r.url
            .contains("start_sequence_timestamp=2023-12-31T23%3A59%3A59.999Z")
    );
    assert!(
        r.url
            .contains("end_sequence_timestamp=2024-01-01T00%3A00%3A01.001Z")
    );
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "Authorization" && value.starts_with("Bearer "))
    );
    let a = fill(
        "9007199254740993",
        "100",
        "100",
        true,
        "2024-01-01T00:00:01Z",
    );
    d.reply(
        r,
        200,
        &format!(r#"{{"fills":[{a}],"cursor":"next/a+ b","proof_token_required":false}}"#),
    );
    let r = d.request();
    assert!(r.url.ends_with("cursor=next%2Fa%2B%20b"));
    let b = fill("b", "99", "2", false, "2024-01-01T00:00:00Z");
    d.reply(
        r,
        200,
        &format!(r#"{{"fills":[{a},{b}],"cursor":"","proof_token_required":false}}"#),
    );
    let rows: Vec<Execution> = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (
            rows[0].id.as_str(),
            rows[0].symbol.as_str(),
            rows[0].order_id.as_str()
        ),
        ("9007199254740993", "BTC-USD", "native-order")
    );
    assert_eq!(
        (
            rows[0].timestamp,
            rows[0].price,
            rows[0].qty,
            rows[0].commission,
            rows[0].maker
        ),
        (NOW / 1000.0 + 1.0, 100.0, 1.0, 0.1, Some(true))
    );
}
#[test]
fn proof_token_failure_is_not_an_empty_ledger() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(2));
    let r = d.request();
    d.reply(
        r,
        200,
        r#"{"fills":[],"cursor":"","proof_token_required":true}"#,
    );
    assert!(d.result().unwrap_err().contains("proof token"));
}
#[test]
fn failed_second_page_returns_accepted_rows_with_owned_gap() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(2));
    let r = d.request();
    let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
    d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"next"}}"#));
    let r = d.request();
    d.reply(r, 503, "unavailable");
    assert_partial(&mut d, &["a"]);
}
#[test]
fn repeated_cursor_and_conflicting_execution_identity_keep_prior_pages_only() {
    for conflict in [false, true] {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.query(fills(2));
        let r = d.request();
        let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
        d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"same"}}"#));
        let r = d.request();
        let b = if conflict {
            a.replace("\"price\":\"100\"", "\"price\":\"101\"")
        } else {
            a
        };
        d.reply(r, 200, &format!(r#"{{"fills":[{b}],"cursor":"same"}}"#));
        assert_partial(&mut d, &["a"]);
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Historic {
    id: String,
    amount: f64,
    filled: f64,
    status: String,
    created_time: f64,
    average_price: Option<f64>,
}
fn order(product: &str, filled: &str, average: &str) -> String {
    format!(
        r#"{{"order_id":"order-a","product_id":"{product}","status":"CANCELLED","side":"SELL","order_type":"LIMIT","filled_size":"{filled}","average_filled_price":"{average}","created_time":"2024-01-01T00:00:00Z","last_fill_time":"","order_configuration":{{"limit_limit_gtc":{{"base_size":"2","limit_price":"100"}}}}}}"#
    )
}
fn history(symbol: &str) -> Query {
    Query::OrderHistory {
        symbol: Some(symbol.into()),
        from_ms: Some(NOW),
        to_ms: Some(NOW),
        limit: Some(2),
    }
}
#[test]
fn unfilled_canceled_history_accepts_empty_cumulative_average() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(history("BTC-USD"));
    let r = d.request();
    assert!(r.url.contains("end_date=2024-01-01T00%3A00%3A00.001Z"));
    d.reply(
        r,
        200,
        &format!(
            r#"{{"orders":[{}],"has_next":false,"cursor":""}}"#,
            order("BTC-USD", "0", "")
        ),
    );
    let rows: Vec<Historic> = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].id.as_str(),
            rows[0].status.as_str(),
            rows[0].amount,
            rows[0].filled,
            rows[0].created_time,
            rows[0].average_price
        ),
        ("order-a", "canceled", 2.0, 0.0, NOW, None)
    );
}

#[test]
fn order_history_uses_documented_default_sort_across_pages() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(Query::OrderHistory {
        symbol: Some("BTC-USD".into()),
        from_ms: Some(NOW),
        to_ms: Some(NOW + 1000.0),
        limit: Some(2),
    });
    let r = d.request();
    assert!(!r.url.contains("sort_by="));
    let newer = order("BTC-USD", "0", "")
        .replace("order-a", "newer")
        .replace("00:00:00Z", "00:00:01Z");
    d.reply(
        r,
        200,
        &format!(r#"{{"orders":[{newer}],"has_next":true,"cursor":"next"}}"#),
    );
    let r = d.request();
    assert!(!r.url.contains("sort_by="));
    assert!(r.url.ends_with("cursor=next"));
    let older = order("BTC-USD", "0", "");
    d.reply(
        r,
        200,
        &format!(r#"{{"orders":[{older}],"has_next":false}}"#),
    );
    let rows: Vec<Historic> = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["newer", "order-a"]
    );
}
#[test]
fn cfm_history_returns_native_contract_amounts() {
    let mut d = Driver::new(true, "BTC-CDE", true);
    d.query(history("BTC-CDE"));
    let r = d.request();
    assert!(
        r.url
            .contains("product_type=FUTURE&contract_expiry_type=EXPIRING")
    );
    d.reply(
        r,
        200,
        &format!(
            r#"{{"orders":[{}],"has_next":false}}"#,
            order("BTC-CDE", "1", "110")
        ),
    );
    let rows: Vec<Historic> = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!(
        (rows[0].amount, rows[0].filled, rows[0].average_price),
        (2.0, 1.0, Some(110.0))
    );
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicTrade {
    id: String,
    symbol: String,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: String,
}
#[test]
fn public_trades_use_unsigned_alias_path_and_keep_selected_symbol() {
    let mut d = Driver::new(false, "BTC-USDC", false);
    d.query(Query::PublicTrades {
        symbol: "BTC-USDC".into(),
        limit: Some(2),
    });
    let r = d.request();
    assert!(r.url.ends_with("/market/products/BTC-USD/ticker?limit=2"));
    assert!(r.headers.is_empty());
    d.reply(r,200,r#"{"trades":[{"trade_id":"t","product_id":"BTC-USD","price":"100","size":"2","time":"2024-01-01T00:00:00.123Z","side":"SELL"}],"best_bid":"99","best_ask":"101"}"#);
    let rows: Vec<PublicTrade> = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!(
        (
            rows[0].id.as_str(),
            rows[0].symbol.as_str(),
            rows[0].qty,
            rows[0].quote_qty,
            rows[0].timestamp,
            rows[0].side.as_str()
        ),
        ("t", "BTC-USDC", 2.0, 200.0, NOW + 123.0, "buy")
    );
}
#[test]
fn cfm_public_trade_quote_quantity_includes_contract_multiplier() {
    let mut d = Driver::new(false, "BTC-CDE", true);
    d.query(Query::PublicTrades {
        symbol: "BTC-CDE".into(),
        limit: Some(2),
    });
    let r = d.request();
    d.reply(r,200,r#"{"trades":[{"trade_id":"t","product_id":"BTC-CDE","price":"110","size":"2","time":"2024-01-01T00:00:00Z","side":"BUY"}]}"#);
    let rows: Vec<PublicTrade> = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!((rows[0].qty, rows[0].quote_qty), (2.0, 2.2));
    assert_eq!(rows[0].side, "sell");
}
#[test]
fn unsupported_or_invalid_queries_fail_before_http() {
    let queries = [
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderByClientId {
            symbol: "BTC-USD".into(),
            client_order_id: "caller".into(),
        },
        Query::Fills {
            symbol: Some("BTC-USD".into()),
            order_id: None,
            from_ms: Some(f64::NAN),
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: None,
            from_ms: Some(2.0),
            to_ms: Some(1.0),
            limit: None,
        },
        Query::PublicTrades {
            symbol: "BTC-USD".into(),
            limit: Some(0),
        },
        Query::PublicTrades {
            symbol: "BTC-USD".into(),
            limit: Some(1001),
        },
        Query::PublicTrades {
            symbol: "missing".into(),
            limit: Some(1),
        },
    ];
    for q in queries {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.query(q);
        assert!(d.result().is_err());
    }
}
#[test]
fn malformed_success_envelope_cannot_become_empty_history() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(2));
    let r = d.request();
    d.reply(r, 200, r#"{"error":"permission rejected"}"#);
    assert!(d.result().is_err());
}

/// Pins the exact request sequence of paged fills and order history.
/// Paging refactors keep it.
#[test]
fn paged_request_sequences_are_pinned() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(5));
    let mut urls = Vec::new();
    let r = d.request();
    urls.push(r.url.clone());
    let a = fill("a", "100", "1", false, "2024-01-01T00:00:01Z");
    d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"c1"}}"#));
    let r = d.request();
    urls.push(r.url.clone());
    let b = fill("b", "100", "1", false, "2024-01-01T00:00:00Z");
    d.reply(r, 200, &format!(r#"{{"fills":[{b}],"cursor":""}}"#));
    assert!(d.result().is_ok());
    d.query(Query::OrderHistory {
        symbol: Some("BTC-USD".into()),
        from_ms: Some(NOW),
        to_ms: Some(NOW + 1000.0),
        limit: Some(5),
    });
    let r = d.request();
    urls.push(r.url.clone());
    let newer = order("BTC-USD", "0", "").replace("order-a", "newer");
    d.reply(
        r,
        200,
        &format!(r#"{{"orders":[{newer}],"has_next":true,"cursor":"o1"}}"#),
    );
    let r = d.request();
    urls.push(r.url.clone());
    let older = order("BTC-USD", "0", "");
    d.reply(
        r,
        200,
        &format!(r#"{{"orders":[{older}],"has_next":false}}"#),
    );
    assert!(d.result().is_ok());
    let paths: Vec<_> = urls
        .iter()
        .map(|u| {
            u.split_once(".com")
                .map_or(u.as_str(), |(_, p)| p)
                .to_owned()
        })
        .collect();
    assert_eq!(paths, PINNED);
}

const PINNED: [&str; 4] = [
    "/api/v3/brokerage/orders/historical/fills?limit=5&sort_by=TRADE_TIME&product_ids=BTC-USD&order_ids=native-order&product_types=SPOT&start_sequence_timestamp=2023-12-31T23%3A59%3A59.999Z&end_sequence_timestamp=2024-01-01T00%3A00%3A01.001Z",
    "/api/v3/brokerage/orders/historical/fills?limit=5&sort_by=TRADE_TIME&product_ids=BTC-USD&order_ids=native-order&product_types=SPOT&start_sequence_timestamp=2023-12-31T23%3A59%3A59.999Z&end_sequence_timestamp=2024-01-01T00%3A00%3A01.001Z&cursor=c1",
    "/api/v3/brokerage/orders/historical/batch?limit=5&order_status=FILLED&order_status=CANCELLED&order_status=EXPIRED&order_status=FAILED&product_ids=BTC-USD&product_type=SPOT&start_date=2024-01-01T00%3A00%3A00.000Z&end_date=2024-01-01T00%3A00%3A01.001Z",
    "/api/v3/brokerage/orders/historical/batch?limit=5&order_status=FILLED&order_status=CANCELLED&order_status=EXPIRED&order_status=FAILED&product_ids=BTC-USD&product_type=SPOT&start_date=2024-01-01T00%3A00%3A00.000Z&end_date=2024-01-01T00%3A00%3A01.001Z&cursor=o1",
];

fn assert_partial(d: &mut Driver, expected: &[&str]) {
    let rows: serde_json::Value = serde_json::from_str(&d.result().unwrap()).unwrap();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    let gaps: Vec<_> = d
        .events
        .iter()
        .filter(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
        .collect();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].level, safe_cex_engine::Level::Warn);
    assert_eq!(
        gaps[0].fields,
        vec![("command", safe_cex_engine::Value::Num(77.0))]
    );
}

#[test]
fn first_history_failures_are_typed_and_do_not_emit_success_or_gap() {
    for (status, body) in [
        (503, "unavailable"),
        (200, r#"{"fills":[{}]}"#),
        (200, r#"{"error":"bad envelope"}"#),
    ] {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.query(fills(3));
        let r = d.request();
        d.reply(r, status, body);
        assert!(d.result().is_err());
        assert_eq!(
            d.error.unwrap().kind,
            Some(safe_cex_engine::ErrorKind::Internal)
        );
        assert!(
            !d.events
                .iter()
                .any(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
        );
    }
}

#[test]
fn later_failed_pages_never_publish_their_valid_prefix() {
    for mode in 0..4 {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.e.set_log_level(safe_cex_engine::Level::Error);
        d.query(fills(4));
        let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
        let r = d.request();
        d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"next"}}"#));
        let b = fill("bad-prefix", "100", "1", false, "2024-01-01T00:00:01Z");
        let r = d.request();
        match mode {
            0 => d.reply(r, 200, &format!(r#"{{"fills":[{b},{{}}],"cursor":""}}"#)),
            1 => d.reply(r, 200, &format!(r#"{{"fills":[{b}],"cursor":"next" "#)),
            2 => d.reply(r, 503, &format!(r#"{{"fills":[{b}],"cursor":""}}"#)),
            _ => d.e.handle(
                d.now,
                Event::HttpFailed {
                    request: r.id,
                    error: "fixture transport failed",
                },
            ),
        }
        assert_partial(&mut d, &["a"]);
        // The slot was removed, so the same external command can start again.
        d.query(fills(1));
        let r = d.request();
        d.reply(r, 200, r#"{"fills":[],"cursor":""}"#);
        assert_eq!(d.result().unwrap(), "[]");
    }
}

#[test]
fn filtered_or_empty_prior_pages_do_not_authorize_partial_success() {
    for row in [
        String::new(),
        fill("old", "100", "1", false, "2023-12-31T23:59:00Z"),
    ] {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.query(history("BTC-USD"));
        // Orders use has_next so even an empty, valid page may continue.
        let r = d.request();
        let body = if row.is_empty() {
            r#"{"orders":[],"has_next":true,"cursor":"next"}"#.into()
        } else {
            format!(
                r#"{{"orders":[{}],"has_next":true,"cursor":"next"}}"#,
                order("BTC-USD", "0", "").replace("2024-01-01T00:00:00Z", "2023-12-31T23:59:00Z")
            )
        };
        d.reply(r, 200, &body);
        let r = d.request();
        d.reply(r, 503, "bad page");
        assert!(d.result().is_err());
        assert_eq!(
            d.error.unwrap().kind,
            Some(safe_cex_engine::ErrorKind::Internal)
        );
        assert!(
            !d.events
                .iter()
                .any(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
        );
    }
}

fn strict_later_preparation(signer_failure: bool) {
    {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.query(fills(3));
        let r = d.request();
        let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
        d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"next"}}"#));
        if signer_failure {
            d.e.set_signing_entropy(Box::new(|_| Err("fixture signer refusal".into())))
                .unwrap();
            assert!(d.collect().is_empty());
            d.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (at, t) = d.timers.remove(0);
            d.e.handle(at, Event::TimerFired { timer: t });
        } else {
            let r = d.request();
            d.reply(r, 200, r#"{"fills":[],"proof_token_required":true}"#);
        }
        assert!(d.result().is_err());
        assert_eq!(
            d.error.as_ref().unwrap().kind,
            Some(safe_cex_engine::ErrorKind::Internal)
        );
        if signer_failure {
            assert_eq!(
                d.error.unwrap().outcome,
                Some(safe_cex_engine::Outcome::NotSent)
            );
        }
        assert!(
            !d.events
                .iter()
                .any(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
        );
    }
}

#[test]
fn later_proof_token_requirement_stays_strict() {
    strict_later_preparation(false);
}
#[test]
fn later_queued_signer_refusal_stays_strict_and_not_sent() {
    strict_later_preparation(true);
}

#[test]
fn cursor_budget_is_owned_incomplete_not_discarded_history() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(1000));
    for i in 0..50 {
        let r = d.request();
        let row = fill(&i.to_string(), "100", "1", false, "2024-01-01T00:00:00Z");
        d.reply(
            r,
            200,
            &format!(r#"{{"fills":[{row}],"cursor":"next-{i}"}}"#),
        );
    }
    let result = d.result().unwrap();
    let rows: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 50);
    assert_eq!(
        d.events
            .iter()
            .filter(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
            .count(),
        1
    );
    assert!(d.collect().is_empty());
}

#[test]
fn concurrent_queries_bind_only_the_incomplete_owner_at_error_level() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.e.set_log_level(safe_cex_engine::Level::Error);
    d.query(fills(3));
    let first = d.request();
    d.e.handle(
        d.now,
        Event::Command(Command::Query {
            command: 88,
            query: fills(1),
        }),
    );
    let second = d.request();
    let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
    d.reply(first, 200, &format!(r#"{{"fills":[{a}],"cursor":"next"}}"#));
    d.reply(second, 200, r#"{"fills":[],"cursor":""}"#);
    let third = d.request();
    d.reply(third, 503, "later failure");
    assert_partial(&mut d, &["a"]);
    assert!(d.completed.contains(&(88, true)));
    assert_eq!(d.completed.iter().filter(|(id, _)| *id == 77).count(), 1);
}

#[test]
fn malformed_continuation_metadata_rejects_even_after_accepted_rows() {
    for tail in [
        r#""cursor":null"#.to_string(),
        format!(r#""cursor":"{}""#, "x".repeat(2049)),
    ] {
        let mut d = Driver::new(true, "BTC-USD", false);
        d.query(fills(3));
        let r = d.request();
        let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
        d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"next"}}"#));
        let r = d.request();
        let prefix = fill("bad-prefix", "100", "1", false, "2024-01-01T00:00:01Z");
        d.reply(r, 200, &format!(r#"{{"fills":[{prefix}],{tail}}}"#));
        assert!(d.result().is_err());
        assert_eq!(
            d.error.unwrap().kind,
            Some(safe_cex_engine::ErrorKind::Internal)
        );
        assert!(
            !d.events
                .iter()
                .any(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
        );
    }
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(history("BTC-USD"));
    let r = d.request();
    d.reply(
        r,
        200,
        &format!(
            r#"{{"orders":[{}],"has_next":true,"cursor":"next"}}"#,
            order("BTC-USD", "0", "")
        ),
    );
    let r = d.request();
    d.reply(r, 200, r#"{"orders":[],"has_next":true}"#);
    assert!(d.result().is_err());
    assert_eq!(
        d.error.unwrap().kind,
        Some(safe_cex_engine::ErrorKind::Internal)
    );
    assert!(
        !d.events
            .iter()
            .any(|e| e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
    );
}

#[test]
fn filtered_fingerprint_conflict_cannot_hide_behind_shared_deduplication() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(4));
    let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
    let filtered = fill("filtered", "100", "1", false, "2023-12-31T23:59:00Z");
    let r = d.request();
    d.reply(
        r,
        200,
        &format!(r#"{{"fills":[{filtered},{a}],"cursor":"next"}}"#),
    );
    let r = d.request();
    let conflict = fill("filtered", "101", "1", false, "2024-01-01T00:00:00Z");
    d.reply(r, 200, &format!(r#"{{"fills":[{conflict}],"cursor":""}}"#));
    assert_partial(&mut d, &["a"]);
}

#[test]
fn duplicate_only_changing_cursors_continue_to_native_family_budget() {
    let mut d = Driver::new(true, "BTC-USD", false);
    d.query(fills(3));
    let a = fill("a", "100", "1", false, "2024-01-01T00:00:00Z");
    for i in 0..50 {
        let r = d.request();
        d.reply(r, 200, &format!(r#"{{"fills":[{a}],"cursor":"next-{i}"}}"#));
    }
    assert_partial(&mut d, &["a"]);
}
