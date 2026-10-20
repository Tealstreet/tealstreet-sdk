use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query, Response};
use safe_cex_venue_grvt::{Grvt, Options};
use serde::Deserialize;
const NOW: f64 = 1_700_000_000_000.;
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","bd":9,"qd":6}]}"#;
const FILL: &str = r#"{"et":"1700000000000000000","sa":"777","i":"BTC_USDT_Perp","ti":"9007199254740993-1","co":"9223372036854775807","oi":"0xaabb","ib":true,"it":false,"s":"0.2","p":"100","f":"0.01","rp":"3","fc":"USDT"}"#;
struct Harness {
    e: Engine<Grvt>,
    requests: Vec<HttpRequest>,
    results: Vec<(u32, Result<String, String>)>,
    now: f64,
}
impl Harness {
    fn new(private: bool) -> Self {
        Self::with_catalog(private, CATALOG)
    }
    fn with_catalog(private: bool, catalog: &str) -> Self {
        let mut e = Engine::new(Grvt::new(Options {
            public_data_only: !private,
            key: Some("fixture-key".into()),
            ..Options::default()
        }));
        e.start(NOW);
        let mut h = Self {
            e,
            requests: vec![],
            results: vec![],
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
        for a in self.e.drain_actions() {
            match a {
                Action::Http(r) => self.requests.push(r),
                Action::Notice(Notice::CommandDone { command, result }) => self
                    .results
                    .push((command, result.map_err(|error| error.message))),
                _ => {}
            }
        }
    }
    fn query(&mut self, q: Query) {
        self.now += 1.;
        self.e.handle(
            self.now,
            Event::Command(Command::Query {
                command: 8,
                query: q,
            }),
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
fn missing_repeated_and_empty_nonterminal_cursors_fail_closed() {
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
    assert!(h.result().unwrap_err().contains("INCOMPLETE"));
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
        let unsupported = ORDER.replace("BTC_USDT_Perp", instrument);
        h.reply(
            "order_history",
            &format!(r#"{{"r":[{ORDER},{unsupported}],"n":""}}"#),
            "",
        );
        assert!(h.result().is_err());

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
