use safe_cex_engine::log::code;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query, Response};
use safe_cex_engine::{ErrorKind, Level, Value};
use safe_cex_venue_grvt::{Grvt, Options};
use serde::Deserialize;
const NOW: f64 = 1_700_000_000_000.;
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","bd":9,"qd":6}]}"#;
const FILL: &str = r#"{"et":"1700000000000000000","sa":"777","i":"BTC_USDT_Perp","ti":"9007199254740993-1","co":"9223372036854775807","oi":"0xaabb","ib":true,"it":false,"s":"0.2","p":"100","f":"0.01","rp":"3","fc":"USDT"}"#;
struct Harness {
    e: Engine<Grvt>,
    requests: Vec<HttpRequest>,
    results: Vec<(u32, Result<String, String>)>,
    logs: Vec<String>,
    notices: Vec<Notice>,
    now: f64,
}
impl Harness {
    fn new(private: bool) -> Self {
        Self::with_catalog(private, CATALOG)
    }
    fn with_catalog(private: bool, catalog: &str) -> Self {
        let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
            public_data_only: !private,
            key: Some("fixture-key".into()),
            ..Options::default()
        }));
        e.start(NOW);
        let mut h = Self {
            e,
            requests: vec![],
            results: vec![],
            logs: vec![],
            notices: vec![],
            now: NOW,
        };
        h.collect();
        h.reply("all_instruments", catalog, "");
        if private {
            h.reply(
                "login",
                r#"{"status":"success","sub_account_id":"777"}"#,
                "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
            );
        }
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            if let Action::Notice(notice) = &a {
                self.notices.push(notice.clone());
            }
            match a {
                Action::Http(r) => self.requests.push(r),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.results.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.results.push((command, Err(error.legacy_text())))
                }
                Action::Notice(Notice::Event(event)) => self.logs.push(event.message),
                _ => {}
            }
        }
    }
    fn logs(&self) -> &[String] {
        &self.logs
    }
    fn query(&mut self, q: Query) {
        self.query_with_id(8, q);
    }
    fn query_with_id(&mut self, command: u32, q: Query) {
        self.now += 1.;
        self.e.handle(
            self.now,
            Event::Command(Command::Query { command, query: q }),
        );
        self.collect();
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.ends_with(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.requests));
        self.requests.remove(i)
    }
    fn reply(&mut self, path: &str, body: &str, headers: &str) {
        let r = self.take(path);
        self.respond(r, body, headers);
    }
    fn respond(&mut self, r: HttpRequest, body: &str, headers: &str) {
        self.now += 1.;
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers,
                    body,
                },
            },
        );
        self.collect();
    }
    fn result(&mut self) -> Result<String, String> {
        let i = self
            .results
            .iter()
            .position(|(id, _)| *id == 8)
            .expect("query completion");
        self.results.remove(i).1
    }
}
fn fills(order_id: Option<&str>, limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: Some("BTC_USDT_PERP".into()),
        order_id: order_id.map(str::to_owned),
        from_ms: Some(NOW - 1000.),
        to_ms: Some(NOW),
        limit,
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    order_id: String,
    qty: f64,
    timestamp: f64,
    commission: f64,
    realized_pnl: Option<f64>,
    maker: Option<bool>,
}

#[test]
fn own_fills_keep_native_ids_units_and_seconds() {
    let mut h = Harness::new(true);
    h.query(fills(None, None));
    let r = h.take("fill_history");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains(r#""sa":"777""#));
    assert!(body.contains("1699999999000000000"));
    assert!(
        !body.contains(r#""k":"#),
        "history spans every derivative kind"
    );
    h.respond(r, &format!(r#"{{"r":[{FILL}],"n":""}}"#), "");
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    let f = &rows[0];
    assert_eq!(f.id, "9007199254740993-1");
    assert_eq!(f.order_id, "9223372036854775807");
    assert_eq!(
        (f.qty, f.timestamp, f.commission),
        (0.2, 1_700_000_000., 0.01)
    );
    assert_eq!(f.realized_pnl, Some(3.));
    assert_eq!(f.maker, Some(true));
}
#[test]
fn order_filtered_fills_walk_to_terminal_and_deduplicate_pages() {
    let mut h = Harness::new(true);
    h.query(fills(Some("9223372036854775807"), Some(3)));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"page-2"}}"#),
        "",
    );
    assert!(h.results.is_empty());
    let r = h.take("fill_history");
    assert!(r.body.as_deref().unwrap().contains(r#""c":"page-2""#));
    let second = FILL
        .replace("9007199254740993-1", "9007199254740993-2")
        .replace("\"0.2\"", "\"0.3\"");
    h.respond(
        r,
        &format!(r#"{{"result":[{FILL},{second}],"next":""}}"#),
        "",
    );
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().map(|f| f.qty).sum::<f64>(), 0.5);
}
#[test]
fn filtered_fill_limit_cannot_return_partial_totals() {
    let mut h = Harness::new(true);
    h.query(fills(Some("9223372036854775807"), Some(1)));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"more"}}"#),
        "",
    );
    assert!(h.results.is_empty());
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{}],"n":""}}"#, FILL.replace("-1", "-2")),
        "",
    );
    assert!(h.result().unwrap_err().contains("INCOMPLETE"));
}
#[test]
fn missing_or_empty_first_cursors_reject_but_later_stalls_keep_owned_coverage() {
    for body in [
        r#"{"r":[]}"#,
        r#"{"r":[],"n":null}"#,
        r#"{"r":[],"n":"more"}"#,
    ] {
        let mut h = Harness::new(true);
        h.query(fills(None, None));
        h.reply("fill_history", body, "");
        assert!(h.result().unwrap_err().contains("INCOMPLETE"));
    }
    let mut h = Harness::new(true);
    h.query(fills(None, None));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"same"}}"#),
        "",
    );
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"same"}}"#),
        "",
    );
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Notice::Event(event) if event.message.contains("INCOMPLETE")));
    assert!(matches!(notices[1], Notice::CommandDone { .. }));
}
#[test]
fn invalid_amount_owner_or_conflicting_client_id_never_returns_partial() {
    for bad in [
        FILL.replace("\"0.2\"", "\"0.2junk\""),
        FILL.replace("\"777\"", "\"888\""),
        FILL.replace("\"oi\":", r#""m":{"co":"123"},"oi":"#),
    ] {
        let mut h = Harness::new(true);
        h.query(fills(None, None));
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{FILL},{bad}],"n":""}}"#),
            "",
        );
        assert!(h.result().is_err());
    }
}
#[test]
fn disagreeing_client_id_never_credits_native_order_filter() {
    let mut h = Harness::new(true);
    h.query(fills(Some("0xaabb"), None));
    h.reply("fill_history", &format!(r#"{{"r":[{FILL}],"n":""}}"#), "");
    assert_eq!(h.result().unwrap(), "[]");
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    income_type: String,
    income: f64,
    timestamp: f64,
    asset: String,
}
#[test]
fn income_combines_native_funding_commission_and_realized_rows() {
    let mut h = Harness::new(true);
    h.query(Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(NOW - 1000.),
        to_ms: Some(NOW),
        limit: None,
    });
    h.reply("funding_payment_history",r#"{"r":[{"et":"1700000000000000000","sa":"777","i":"BTC_USDT_Perp","ti":"0x000123","c":"USDT","a":"25"}],"n":""}"#,"");
    assert!(h.results.is_empty());
    h.reply("fill_history", &format!(r#"{{"r":[{FILL}],"n":""}}"#), "");
    let rows: Vec<Ledger> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .any(|r| r.income_type == "FUNDING_FEE" && r.income == -25.)
    );
    assert!(
        rows.iter()
            .any(|r| r.income_type == "COMMISSION" && r.income == -0.01)
    );
    assert!(
        rows.iter()
            .any(|r| r.income_type == "REALIZED_PNL" && r.income == 3.)
    );
    assert!(rows.iter().all(|r| r.timestamp == NOW && r.asset == "USDT"));
}
#[test]
fn missing_realized_pnl_does_not_fabricate_income() {
    let mut h = Harness::new(true);
    h.query(Query::Income {
        symbol: None,
        kind: Some("REALIZED_PNL".into()),
        from_ms: None,
        to_ms: Some(NOW),
        limit: None,
    });
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{}],"n":""}}"#, FILL.replace(r#","rp":"3""#, "")),
        "",
    );
    assert_eq!(h.result().unwrap(), "[]");
}
#[test]
fn public_trade_history_is_unsigned_and_uses_taker_side() {
    let mut h = Harness::new(false);
    h.query(Query::PublicTrades {
        symbol: "BTC_USDT_PERP".into(),
        limit: Some(1),
    });
    let r = h.take("trade_history");
    assert!(r.url.starts_with("https://market-data.grvt.io/"));
    assert_eq!(r.headers.len(), 1);
    h.respond(r,r#"{"r":[{"et":"1700000000000000000","i":"BTC_USDT_Perp","ti":"0x000123","it":false,"s":"2","p":"100"}],"n":""}"#,"");
    let result = h.result().unwrap();
    assert!(result.contains(r#""side":"sell""#));
    assert!(result.contains(r#""quoteQty":200.0"#));
    assert!(result.contains(r#""timestamp":1700000000000.0"#));
}
#[test]
fn unsupported_income_and_invalid_bounds_fail_before_http() {
    for query in [
        Query::Income {
            symbol: None,
            kind: Some("TRANSFER".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(f64::INFINITY),
            to_ms: None,
            limit: None,
        },
    ] {
        let mut h = Harness::new(true);
        let count = h.requests.len();
        h.query(query);
        assert!(h.result().is_err());
        assert_eq!(h.requests.len(), count);
    }
}

#[test]
fn retired_product_queries_never_fall_back_to_the_same_base_quote_perpetual() {
    for (kind, instrument) in [
        ("FUTURE", "BTC_USDT_30DEC26_Future"),
        ("CALL", "BTC_USDT_30DEC26_100000_Call"),
        ("PUT", "BTC_USDT_30DEC26_100000_Put"),
    ] {
        let catalog = format!(
            r#"{},{{"i":"{instrument}","ih":"0x030502","b":"BTC","q":"USDT","k":"{kind}","ts":"0.1","ms":"0.001","bd":9,"qd":6}}]}}"#,
            CATALOG.strip_suffix("]}").unwrap()
        );
        for symbol in [instrument.to_owned(), instrument.to_ascii_uppercase()] {
            for query in [
                Query::Fills {
                    symbol: Some(symbol.clone()),
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                Query::Income {
                    symbol: Some(symbol.clone()),
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                Query::OrderHistory {
                    symbol: Some(symbol.clone()),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                Query::OrderByClientId {
                    symbol: symbol.clone(),
                    client_order_id: "9223372036854775807".into(),
                },
                Query::PublicTrades {
                    symbol: symbol.clone(),
                    limit: None,
                },
            ] {
                let mut h = Harness::with_catalog(true, &catalog);
                let pending = h.requests.len();
                h.query(query);
                assert!(
                    h.result()
                        .unwrap_err()
                        .contains("Unknown GRVT query market")
                );
                assert_eq!(h.requests.len(), pending);
            }
        }
    }
}

#[test]
fn retired_product_history_rows_cannot_be_credited_to_perpetual_queries() {
    for instrument in [
        "BTC_USDT_30DEC26_Future",
        "BTC_USDT_30DEC26_100000_Call",
        "BTC_USDT_30DEC26_100000_Put",
    ] {
        let mut h = Harness::new(true);
        h.query(fills(None, None));
        let unsupported = FILL
            .replace("BTC_USDT_Perp", instrument)
            .replace("9007199254740993-1", "9007199254740993-2");
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{FILL},{unsupported}],"n":""}}"#),
            "",
        );
        let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "9007199254740993-1");

        let mut h = Harness::new(true);
        h.query(Query::Income {
            symbol: Some("BTC_USDT_PERP".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        });
        let funding = r#"{"et":"1700000000000000000","sa":"777","i":"BTC_USDT_Perp","ti":"0x000123","c":"USDT","a":"25"}"#;
        let unsupported = funding
            .replace("BTC_USDT_Perp", instrument)
            .replace("0x000123", "0x000124");
        h.reply(
            "funding_payment_history",
            &format!(r#"{{"r":[{funding},{unsupported}],"n":""}}"#),
            "",
        );
        let rows: Vec<Ledger> = serde_json::from_str(&h.result().unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].income, -25.);

        let mut h = Harness::new(true);
        h.query(Query::OrderHistory {
            symbol: Some("BTC_USDT_PERP".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        });
        let unsupported = ORDER
            .replace("BTC_USDT_Perp", instrument)
            .replace("0x000abcd", "0x000abce");
        h.reply(
            "order_history",
            &format!(r#"{{"r":[{ORDER},{unsupported}],"n":""}}"#),
            "",
        );
        // The retired row is skipped on its own; it neither fails nor joins the read.
        let rows: Vec<serde_json::Value> = serde_json::from_str(&h.result().unwrap()).unwrap();
        assert_eq!(rows.len(), 1, "{instrument}");
        assert!(
            h.logs()
                .iter()
                .any(|l| l.starts_with("Skipped malformed GRVT historical order"))
        );

        let mut h = Harness::new(true);
        h.query(Query::OrderByClientId {
            symbol: "BTC_USDT_PERP".into(),
            client_order_id: "9223372036854775807".into(),
        });
        h.reply(
            "/order",
            &format!(r#"{{"r":{}}}"#, ORDER.replace("BTC_USDT_Perp", instrument)),
            "",
        );
        assert!(h.result().is_err());

        let mut h = Harness::new(false);
        h.query(Query::PublicTrades {
            symbol: "BTC_USDT_PERP".into(),
            limit: None,
        });
        h.reply(
            "trade_history",
            &format!(
                r#"{{"r":[{{"et":"1700000000000000000","i":"{instrument}","ti":"0x000123","it":false,"s":"2","p":"100"}}],"n":""}}"#
            ),
            "",
        );
        assert!(h.result().is_err());
    }
}
const ORDER: &str = r#"{"oi":"0x000abcd","sa":"777","im":false,"ro":true,"po":false,"ti":"GOOD_TILL_TIME","l":[{"i":"BTC_USDT_Perp","s":"2","lp":"100","ib":true}],"m":{"co":"9223372036854775807","ct":"1699999999999000000"},"s1":{"s":"FILLED","ts":["2"],"af":["101"],"ut":"1700000000000000000"}}"#;
#[test]
fn closed_order_history_keeps_real_average_and_creation_time() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: Some("BTC_USDT_PERP".into()),
        from_ms: None,
        to_ms: Some(NOW),
        limit: None,
    });
    h.reply("order_history", &format!(r#"{{"r":[{ORDER}],"n":""}}"#), "");
    let result = h.result().unwrap();
    assert!(result.contains(r#""averagePrice":101.0"#));
    assert!(result.contains(r#""createdTime":1699999999999.0"#));
    assert!(result.contains(r#""status":"closed""#));
}
#[test]
fn client_lookup_validates_symbol_identity_and_returns_safe_found_shape() {
    let mut h = Harness::new(true);
    h.query(Query::OrderByClientId {
        symbol: "BTC_USDT_PERP".into(),
        client_order_id: "9223372036854775807".into(),
    });
    let r = h.take("/order");
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"sa":"777","co":"9223372036854775807"}"#)
    );
    h.respond(r, &format!(r#"{{"r":{ORDER}}}"#), "");
    let result = h.result().unwrap();
    assert!(result.contains(r#""found":true"#));
    assert!(result.contains("9223372036854775807"));
    let mut h = Harness::new(true);
    h.query(Query::OrderByClientId {
        symbol: "BTC_USDT_PERP".into(),
        client_order_id: "123".into(),
    });
    h.reply("/order", &format!(r#"{{"r":{ORDER}}}"#), "");
    assert!(h.result().is_err());
}

fn know_current_native(h: &mut Harness, native: &str) {
    h.reply(
        "aggregated_account_summary",
        r#"{"r":{"te":"100","ts2":"80"}}"#,
        "",
    );
    let order = ORDER
        .replace("0x000abcd", native)
        .replace("\"FILLED\"", "\"OPEN\"")
        .replace("\"ts\":[\"2\"]", "\"ts\":[\"0\"]");
    h.reply("open_orders", &format!(r#"{{"r":[{order}]}}"#), "");
}
#[test]
fn exact_millisecond_bounds_do_not_shift_nanosecond_edges() {
    let mut h = Harness::new(true);
    h.query(Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: Some(NOW + 3.),
        to_ms: Some(NOW + 3.),
        limit: None,
    });
    let request = h.take("fill_history");
    let body = request.body.as_deref().unwrap();
    assert!(body.contains(r#""st":"1700000000003000000""#));
    assert!(body.contains(r#""et":"1700000000003000000""#));
    let boundary = FILL.replace("1700000000000000000", "1700000000003000000");
    let outside = FILL
        .replace("1700000000000000000", "1700000000003000001")
        .replace("-1", "-2");
    h.respond(
        request,
        &format!(r#"{{"r":[{boundary},{outside}],"n":""}}"#),
        "",
    );
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}
#[test]
fn current_native_lifecycle_excludes_previous_reused_client_fills() {
    let mut h = Harness::new(true);
    know_current_native(&mut h, "0xnew");
    h.query(fills(Some("9223372036854775807"), None));
    let current = FILL.replace("0xaabb", "0xnew").replace("-1", "-2");
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL},{current}],"n":""}}"#),
        "",
    );
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "9007199254740993-2");
}
#[test]
fn zero_client_id_uses_known_native_identity_and_ambiguous_lifecycles_fail() {
    let mut h = Harness::new(true);
    know_current_native(&mut h, "0xaabb");
    h.query(fills(Some("9223372036854775807"), None));
    h.reply(
        "fill_history",
        &format!(
            r#"{{"r":[{}],"n":""}}"#,
            FILL.replace("\"co\":\"9223372036854775807\"", "\"co\":\"0\"")
        ),
        "",
    );
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].order_id, "9223372036854775807");
    let mut h = Harness::new(true);
    h.query(fills(Some("9223372036854775807"), None));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
        "",
    );
    let other = FILL.replace("0xaabb", "0xother").replace("-1", "-2");
    h.reply("fill_history", &format!(r#"{{"r":[{other}],"n":""}}"#), "");
    assert!(h.result().unwrap_err().contains("INCOMPLETE"));
}
#[test]
fn streamed_failure_retries_only_current_page_and_retains_committed_fills() {
    let mut h = Harness::new(true);
    h.query(fills(Some("9223372036854775807"), None));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
        "",
    );
    let request = h.take("fill_history");
    let id = request.id;
    let body = request.body.clone();
    h.now += 1.;
    h.e.handle(
        h.now,
        Event::HttpStart {
            request: id,
            status: 200,
            headers: "",
        },
    );
    let second = FILL.replace("-1", "-2").replace("\"0.2\"", "\"0.3\"");
    let chunk = format!(r#"{{"r":[{second},"#);
    h.e.handle(
        h.now,
        Event::HttpChunk {
            request: id,
            data: chunk.as_bytes(),
        },
    );
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: id,
            error: "fixture network failure",
        },
    );
    h.collect();
    let retry = h.take("fill_history");
    assert_eq!(retry.body, body);
    h.respond(retry, &format!(r#"{{"r":[{second}],"n":""}}"#), "");
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().map(|r| r.qty).sum::<f64>(), 0.5);
}

#[test]
fn explicit_session_expiry_resends_current_page_once_with_new_session() {
    let mut h = Harness::new(true);
    h.query(fills(Some("9223372036854775807"), None));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
        "",
    );
    let request = h.take("fill_history");
    let original_body = request.body.clone();
    let second = FILL.replace("-1", "-2");
    h.respond(
        request,
        &format!(r#"{{"r":[{second}],"n":"","c":1000,"m":"Session expired"}}"#),
        "",
    );
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: renewed-fixture-session\nx-grvt-account-id: 0xaccount",
    );
    let retry = h.take("fill_history");
    assert_eq!(retry.body, original_body);
    assert!(retry.headers.iter().any(
        |(name, value)| name.eq_ignore_ascii_case("X-Ts-Grvt-Session")
            && value == "renewed-fixture-session"
    ));
    h.respond(retry, &format!(r#"{{"r":[{second}],"n":""}}"#), "");
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);

    let mut h = Harness::new(true);
    h.query(fills(None, None));
    h.reply("fill_history", r#"{"c":1000,"m":"Session expired"}"#, "");
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: renewed-fixture-session\nx-grvt-account-id: 0xaccount",
    );
    h.reply(
        "fill_history",
        r#"{"c":1000,"m":"Session expired again"}"#,
        "",
    );
    assert!(h.result().is_err());
    assert!(
        h.requests.iter().all(
            |request| !request.url.ends_with("login") && !request.url.ends_with("fill_history")
        )
    );
}

#[test]
fn filtered_history_page_cap_fails_without_returning_partial_data() {
    let mut h = Harness::new(true);
    h.query(fills(Some("9223372036854775807"), None));
    for index in 0..50 {
        let unrelated = FILL
            .replace("9223372036854775807", "123")
            .replace("9007199254740993-1", &format!("9007199254740993-{index}"));
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{unrelated}],"n":"next-{index}"}}"#),
            "",
        );
        if index < 49 {
            assert!(h.results.is_empty());
        }
    }
    assert!(h.result().unwrap_err().contains("INCOMPLETE"));
    assert!(
        h.requests
            .iter()
            .all(|request| !request.url.ends_with("fill_history"))
    );
}

#[test]
fn filtered_current_order_walk_starts_near_creation_without_changing_explicit_bounds() {
    for (known, from, to, expected) in [
        (true, 0., NOW, "1699999699999000000"),
        (true, NOW - 500., NOW, "1699999999500000000"),
        (false, 0., NOW, "0"),
        (true, 0., NOW - 600_000., "1699999400000000000"),
    ] {
        let mut h = Harness::new(true);
        if known {
            know_current_native(&mut h, "0xaabb");
        }
        h.query(Query::Fills {
            symbol: Some("BTC_USDT_PERP".into()),
            order_id: Some("9223372036854775807".into()),
            from_ms: Some(from),
            to_ms: Some(to),
            limit: None,
        });
        let first = h.take("fill_history");
        assert!(
            first
                .body
                .as_deref()
                .unwrap()
                .contains(&format!(r#""st":"{expected}""#))
        );
        h.respond(first, &format!(r#"{{"r":[{FILL}],"n":"next"}}"#), "");
        let second = h.take("fill_history");
        assert!(
            second
                .body
                .as_deref()
                .unwrap()
                .contains(&format!(r#""st":"{expected}""#))
        );
        h.respond(second, r#"{"r":[],"n":""}"#, "");
        let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
        assert_eq!(rows.len(), usize::from(to == NOW));
    }
}

/// Pins the exact request sequence of a paged read: funding pages by cursor,
/// then the fills stage from a fresh cursor. Paging refactors keep it.
#[test]
fn paged_income_request_sequence_is_pinned() {
    let mut h = Harness::new(true);
    h.query(Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(NOW - 1000.),
        to_ms: Some(NOW),
        limit: None,
    });
    let mut sent = Vec::new();
    let mut step = |h: &mut Harness, path: &str, body: String| {
        let r = h.take(path);
        sent.push(format!(
            "{} {}",
            r.url.rsplit('/').next().unwrap(),
            r.body.clone().unwrap_or_default()
        ));
        h.respond(r, &body, "");
    };
    let funding = |ti: &str, next: &str| {
        format!(
            r#"{{"r":[{{"et":"1700000000000000000","sa":"777","i":"BTC_USDT_Perp","ti":"{ti}","c":"USDT","a":"25"}}],"n":"{next}"}}"#
        )
    };
    step(&mut h, "funding_payment_history", funding("0x01", "f1"));
    step(&mut h, "funding_payment_history", funding("0x02", ""));
    step(
        &mut h,
        "fill_history",
        format!(r#"{{"r":[{FILL}],"n":"t1"}}"#),
    );
    let second = FILL.replace("9007199254740993-1", "9007199254740993-2");
    step(
        &mut h,
        "fill_history",
        format!(r#"{{"r":[{second}],"n":""}}"#),
    );
    assert!(h.result().is_ok());
    assert_eq!(sent.len(), 4);
    let cursors: Vec<_> = sent
        .iter()
        .map(|s| {
            let body: serde_json::Value =
                serde_json::from_str(s.split_once(' ').unwrap().1).unwrap();
            (
                s.split(' ').next().unwrap().to_owned(),
                body["c"].as_str().unwrap_or("").to_owned(),
                body["l"].clone(),
            )
        })
        .collect();
    assert_eq!(
        cursors,
        [
            (
                "funding_payment_history".into(),
                "".into(),
                serde_json::json!(1000)
            ),
            (
                "funding_payment_history".into(),
                "f1".into(),
                serde_json::json!(1000)
            ),
            ("fill_history".into(), "".into(), serde_json::json!(1000)),
            ("fill_history".into(), "t1".into(), serde_json::json!(1000)),
        ]
    );
    // Every other body field is unchanged between pages of one stage.
    let strip = |s: &str| {
        let mut v: serde_json::Value = serde_json::from_str(s.split_once(' ').unwrap().1).unwrap();
        v.as_object_mut().unwrap().remove("c");
        v
    };
    assert_eq!(strip(&sent[0]), strip(&sent[1]));
    assert_eq!(strip(&sent[2]), strip(&sent[3]));
    insta_like(&sent);
}

/// The full request bodies, pinned verbatim.
fn insta_like(sent: &[String]) {
    let expected = [
        r#"funding_payment_history {"sa":"777","st":"1699999999000000000","et":"1700000000000000000","l":1000,"c":""}"#,
        r#"funding_payment_history {"sa":"777","st":"1699999999000000000","et":"1700000000000000000","l":1000,"c":"f1"}"#,
        r#"fill_history {"sa":"777","st":"1699999999000000000","et":"1700000000000000000","l":1000,"c":""}"#,
        r#"fill_history {"sa":"777","st":"1699999999000000000","et":"1700000000000000000","l":1000,"c":"t1"}"#,
    ];
    assert_eq!(sent, expected);
}

#[test]
fn a_history_page_of_only_malformed_orders_still_advances_the_cursor() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: Some(NOW),
        limit: None,
    });
    let bad = ORDER.replace(r#""s":"FILLED""#, r#""s":"BOGUS""#);
    // A cursor follows, so the skipped row must count as fresh or the walk would stall.
    h.reply(
        "order_history",
        &format!(r#"{{"r":[{bad}],"n":"next"}}"#),
        "",
    );
    assert!(
        h.logs()
            .iter()
            .any(|l| l.starts_with("Skipped malformed GRVT historical order"))
    );
    h.reply("order_history", &format!(r#"{{"r":[{ORDER}],"n":""}}"#), "");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn an_undecodable_order_from_another_subaccount_still_fails_the_read() {
    let mut h = Harness::new(true);
    h.query(Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: Some(NOW),
        limit: None,
    });
    let foreign = ORDER
        .replace(r#""sa":"777""#, r#""sa":"888""#)
        .replace(r#""s":"FILLED""#, r#""s":"BOGUS""#);
    h.reply(
        "order_history",
        &format!(r#"{{"r":[{foreign}],"n":""}}"#),
        "",
    );
    assert!(h.result().is_err());
}

fn incomplete_pairs(h: &Harness, command: u32) -> Vec<&Notice> {
    h.notices
        .iter()
        .filter(|notice| match notice {
            Notice::Event(event) => {
                event.code == code::HISTORY_INCOMPLETE
                    && event.fields == vec![("command", Value::Num(f64::from(command)))]
            }
            Notice::CommandDone { command: id, .. } | Notice::CommandFailed { command: id, .. } => {
                *id == command
            }
            _ => false,
        })
        .collect()
}

fn http_failure(h: &mut Harness, request: HttpRequest) {
    h.now += 1.;
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"c":999,"m":"fixture unavailable"}"#,
            },
        },
    );
    h.collect();
}

#[test]
fn first_history_failure_is_typed_without_empty_success_or_incomplete_marker() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    h.query(fills(None, None));
    let request = h.take("fill_history");
    http_failure(&mut h, request);
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 1);
    assert!(matches!(notices[0], Notice::CommandFailed { error, .. }
        if error.kind == Some(ErrorKind::Internal)));
}

#[test]
fn later_page_failure_retains_only_accepted_rows_with_one_owned_marker() {
    for malformed in [false, true] {
        let mut h = Harness::new(true);
        h.e.set_log_level(Level::Error);
        let orders = h.e.store().orders().to_vec();
        h.query(fills(None, None));
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
            "",
        );
        let request = h.take("fill_history");
        if malformed {
            let prefix = FILL.replace("-1", "-2");
            let bad = FILL.replace("-1", "-3").replace("\"0.2\"", "\"invalid\"");
            h.respond(request, &format!(r#"{{"r":[{prefix},{bad}],"n":""}}"#), "");
        } else {
            http_failure(&mut h, request);
        }
        let notices = incomplete_pairs(&h, 8);
        assert_eq!(notices.len(), 2);
        assert!(matches!(notices[0], Notice::Event(event) if event.level == Level::Warn));
        let Notice::CommandDone { result, .. } = notices[1] else {
            panic!("accepted rows must resolve")
        };
        let rows: Vec<Execution> = serde_json::from_str(result).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "9007199254740993-1");
        assert_eq!(h.e.store().orders(), orders);
    }
}

#[test]
fn income_second_stage_failure_keeps_funding_and_command_owner() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    h.query(Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(NOW - 1000.),
        to_ms: Some(NOW),
        limit: None,
    });
    h.reply("funding_payment_history", r#"{"r":[{"et":"1700000000000000000","sa":"777","i":"BTC_USDT_Perp","ti":"funding-1","c":"USDT","a":"25"}],"n":""}"#, "");
    let request = h.take("fill_history");
    http_failure(&mut h, request);
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Notice::Event(_)));
    let Notice::CommandDone { result, .. } = notices[1] else {
        panic!("funding remains accepted")
    };
    let rows: Vec<Ledger> = serde_json::from_str(result).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].income_type, "FUNDING_FEE");
    assert_eq!(rows[0].income, -25.);
}

#[test]
fn filtered_native_rows_cannot_authorize_empty_partial_recovery() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    let mut query = fills(None, None);
    if let Query::Fills { from_ms, to_ms, .. } = &mut query {
        *from_ms = Some(NOW + 10.);
        *to_ms = Some(NOW + 20.);
    }
    h.query(query);
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
        "",
    );
    let request = h.take("fill_history");
    http_failure(&mut h, request);
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(notices[0], Notice::CommandFailed { error, .. } if error.kind == Some(ErrorKind::Internal))
    );
}

#[test]
fn concurrent_history_failures_keep_their_own_markers_before_success() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    let mut second_pages = Vec::new();
    for command in [8, 9] {
        h.query_with_id(command, fills(None, None));
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
            "",
        );
        second_pages.push(h.take("fill_history"));
    }
    for (command, request) in [9, 8].into_iter().zip(second_pages.into_iter().rev()) {
        http_failure(&mut h, request);
        let notices = incomplete_pairs(&h, command);
        assert_eq!(notices.len(), 2);
        assert!(matches!(notices[0], Notice::Event(_)));
        assert!(matches!(notices[1], Notice::CommandDone { .. }));
    }
}

fn exhaust_streamed_page(h: &mut Harness, path: &str, prefix: &str) {
    let mut expected = None;
    for attempt in 0..6 {
        let request = h.take(path);
        if let Some(expected) = &expected {
            assert_eq!(&request.body, expected, "retry must not advance the page");
        } else {
            expected = Some(request.body.clone());
        }
        h.now += 1.;
        h.e.handle(
            h.now,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        let chunk = format!(r#"{{"r":[{prefix},"#);
        h.e.handle(
            h.now,
            Event::HttpChunk {
                request: request.id,
                data: chunk.as_bytes(),
            },
        );
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: request.id,
                error: "fixture exhausted read",
            },
        );
        h.collect();
        if attempt < 5 {
            assert!(h.results.is_empty(), "five retries stay pending");
        }
    }
}

#[test]
fn exhausted_streamed_retries_keep_prior_rows_and_exclude_every_failed_prefix() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    h.query(fills(None, None));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
        "",
    );
    exhaust_streamed_page(&mut h, "fill_history", &FILL.replace("-1", "-2"));
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "9007199254740993-1");
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Notice::Event(_)));
    assert!(matches!(notices[1], Notice::CommandDone { .. }));
}

#[test]
fn order_filtered_total_proofs_remain_strict_after_accepted_rows() {
    for transport in [false, true] {
        let mut h = Harness::new(true);
        h.e.set_log_level(Level::Error);
        h.query(fills(Some("9223372036854775807"), None));
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
            "",
        );
        if transport {
            exhaust_streamed_page(&mut h, "fill_history", &FILL.replace("-1", "-2"));
        } else {
            let request = h.take("fill_history");
            let bad = FILL.replace("-1", "-2").replace("\"0.2\"", "\"invalid\"");
            h.respond(request, &format!(r#"{{"r":[{bad}],"n":""}}"#), "");
        }
        let notices = incomplete_pairs(&h, 8);
        assert_eq!(notices.len(), 1);
        assert!(matches!(notices[0], Notice::CommandFailed { .. }));
        assert!(
            h.result().is_err(),
            "a partial total cannot masquerade as a proof"
        );
    }
}

#[test]
fn session_refresh_preparation_failure_stays_strict_after_accepted_rows() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    h.query(fills(None, None));
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"next"}}"#),
        "",
    );
    h.reply("fill_history", r#"{"c":1000,"m":"Session expired"}"#, "");
    for attempt in 0..6 {
        let request = h.take("login");
        h.now += 1.;
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: request.id,
                error: "fixture login unavailable",
            },
        );
        h.collect();
        if attempt < 5 {
            assert!(h.results.is_empty());
        }
    }
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(notices[0], Notice::CommandFailed { error, .. } if error.message.contains("session refresh failed"))
    );
}

#[test]
fn combined_income_keeps_stage_limit_separate_and_selects_newest_global_output() {
    let mut h = Harness::new(true);
    h.query(Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(NOW - 1000.),
        to_ms: Some(NOW),
        limit: Some(1),
    });
    h.reply("funding_payment_history", r#"{"r":[{"et":"1699999999900000000","sa":"777","i":"BTC_USDT_Perp","ti":"older-funding","c":"USDT","a":"25"}],"n":""}"#, "");
    assert!(h.results.is_empty());
    h.reply(
        "fill_history",
        &format!(r#"{{"r":[{FILL}],"n":"would-have-more"}}"#),
        "",
    );
    let rows: Vec<Ledger> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].income_type, "COMMISSION");
    assert_eq!(rows[0].income, -0.01);
    assert_eq!(rows[0].timestamp, NOW);
    assert!(h.requests.iter().all(|r| !r.url.ends_with("fill_history")));
    assert!(
        !h.notices
            .iter()
            .any(|n| matches!(n, Notice::Event(event) if event.code == code::HISTORY_INCOMPLETE))
    );
}

#[test]
fn general_page_cap_returns_bounded_rows_with_one_owned_gap() {
    let mut h = Harness::new(true);
    h.e.set_log_level(Level::Error);
    h.query(fills(None, None));
    for index in 0..50 {
        let row = FILL.replace("9007199254740993-1", &format!("native-fill-{index}"));
        h.reply(
            "fill_history",
            &format!(r#"{{"r":[{row}],"n":"next-{index}"}}"#),
            "",
        );
        if index < 49 {
            assert!(h.results.is_empty());
        }
    }
    let rows: Vec<Execution> = serde_json::from_str(&h.result().unwrap()).unwrap();
    assert_eq!(rows.len(), 50);
    let notices = incomplete_pairs(&h, 8);
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Notice::Event(event) if event.message.contains("page cap")));
    assert!(matches!(notices[1], Notice::CommandDone { .. }));
    assert!(h.requests.iter().all(|r| !r.url.ends_with("fill_history")));
}
