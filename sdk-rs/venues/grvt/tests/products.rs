use alloy_dyn_abi::TypedData;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, SocketChannel, SocketId,
    Venue,
};
use safe_cex_venue_grvt::{Grvt, Options};
use serde::Deserialize;

const NOW: f64 = 1_700_000_000_000.;
const MARKETS: [(&str, &str); 7] = [
    ("BTC_USDT_Perp", "BTC"),
    ("ETH_USDT_Perp", "ETH"),
    ("SOL_USDT_Perp", "SOL"),
    ("XRP_USDT_Perp", "XRP"),
    ("DOGE_USDT_Perp", "DOGE"),
    ("ADA_USDT_Perp", "ADA"),
    ("AVAX_USDT_Perp", "AVAX"),
];
fn catalog() -> String {
    format!(r#"{{"r":[{}]}}"#, MARKETS.iter().enumerate().map(|(n,(id,base))| format!(r#"{{"i":"{id}","k":"PERPETUAL","ih":"0x{:06x}","b":"{base}","q":"USDT","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}}"#, if n == 1 { 0x30508 } else { 0x30501+n })).collect::<Vec<_>>().join(","))
}
fn positions() -> String {
    format!(r#"{{"r":[{}]}}"#, MARKETS.iter().map(|(id,_)| format!(r#"{{"i":"{id}","s":"-0.25","ep":"100","mp":"100.5","n":"-25.125","up":"-0.125","l":"10","el":"200","mt":"CROSS"}}"#)).collect::<Vec<_>>().join(","))
}
fn order_row(id: &str, native: &str, client: &str) -> String {
    format!(
        r#"{{"sa":"777","oi":"{native}","im":false,"ro":false,"po":false,"ti":"GOOD_TILL_TIME","l":[{{"i":"{id}","s":"2","lp":"100","ib":true}}],"m":{{"co":"{client}","ct":"1700000000000000000"}},"s1":{{"s":"OPEN","ts":["0.25"],"ut":"1700000000001000000"}}}}"#
    )
}
fn orders() -> String {
    format!(
        r#"{{"r":[{}]}}"#,
        MARKETS
            .iter()
            .enumerate()
            .map(|(n, (id, _))| order_row(id, &format!("0x{:04x}", n + 1), &(n + 101).to_string()))
            .collect::<Vec<_>>()
            .join(",")
    )
}
struct Harness {
    e: Engine<Grvt>,
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    now: f64,
}
impl Harness {
    fn new(private: bool) -> Self {
        let mut h = Self {
            e: safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
                public_data_only: !private,
                key: Some("fixture-key".into()),
                secret: Some(format!("0x{}", "01".repeat(32))),
                trading_account_id: Some("777".into()),
                ..Options::default()
            })),
            requests: vec![],
            actions: vec![],
            now: NOW,
        };
        h.e.start(NOW);
        h.collect();
        let request = h.take("all_instruments");
        h.respond(request, &catalog(), "");
        while let Some(r) = h.requests.pop() {
            let (body, headers) = if r.url.ends_with("/login") {
                (
                    r#"{"sub_account_id":"777"}"#.into(),
                    "X-Ts-Grvt-Session: fixture-session\nX-Grvt-Account-Id: fixture-account",
                )
            } else if r.url.ends_with("aggregated_account_summary") {
                (
                    r#"{"r":{"te":"1000","ts2":"800","tu":"-0.875","sas":[{"si":"777"}]}}"#.into(),
                    "",
                )
            } else if r.url.ends_with("get_authorized_builders") {
                (r#"{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}"#.into(), "")
            } else if r.url.ends_with("positions") {
                assert_eq!(r.body.as_deref(), Some(r#"{"sa":"777"}"#));
                (positions(), "")
            } else if r.url.ends_with("open_orders") {
                assert_eq!(r.body.as_deref(), Some(r#"{"sa":"777"}"#));
                (orders(), "")
            } else {
                (r#"{"r":[]}"#.into(), "")
            };
            h.respond(r, &body, headers);
        }
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            if let Action::Http(r) = a {
                self.requests.push(r);
            } else {
                self.actions.push(a);
            }
        }
    }
    fn take(&mut self, suffix: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix}: {:?}", self.requests));
        self.requests.remove(index)
    }
    fn respond(&mut self, r: HttpRequest, body: &str, headers: &str) {
        self.respond_status(r, 200, body, headers);
    }
    fn respond_status(&mut self, r: HttpRequest, status: u16, body: &str, headers: &str) {
        self.now += 1.;
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers,
                    body,
                },
            },
        );
        self.collect();
    }
    fn command(&mut self, command: Command) {
        self.now += 1.;
        self.e.handle(self.now, Event::Command(command));
        self.collect();
    }
    fn socket(&self, role: SocketChannel) -> SocketId {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(c),
                    ..
                } if *c == role => Some(*socket),
                _ => None,
            })
            .unwrap()
    }
    fn ws(&mut self, socket: SocketId, text: &str) {
        self.now += 1.;
        self.e.handle(self.now, Event::WsMessage { socket, text });
        self.collect();
    }
    fn result(&mut self, command: u32) -> Result<String, String> {
        let index = self
            .actions
            .iter()
            .position(|a| {
                matches!(a, Action::Notice(Notice::CommandDone { command: c, .. }
                    | Notice::CommandFailed { command: c, .. }) if *c == command)
            })
            .expect("command completion");
        match self.actions.remove(index) {
            Action::Notice(Notice::CommandDone { result, .. }) => Ok(result),
            Action::Notice(Notice::CommandFailed { error, .. }) => Err(error.legacy_text()),
            _ => unreachable!(),
        }
    }
}

#[test]
fn perpetual_catalog_and_private_snapshots_preserve_every_native_identity() {
    let h = Harness::new(true);
    assert_eq!(h.e.store().markets().len(), MARKETS.len());
    assert_eq!(h.e.store().positions().len(), MARKETS.len());
    assert_eq!(h.e.store().orders().len(), MARKETS.len());
    for (n, (id, base)) in MARKETS.iter().enumerate() {
        let m = h.e.store().market_by_id(id).unwrap();
        assert_eq!(m.symbol, id.to_ascii_uppercase());
        assert_eq!(m.is_perpetual, Some(true));
        assert_eq!(m.base, *base);
        let p =
            h.e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == m.symbol)
                .unwrap();
        assert_eq!(
            (p.contracts, p.entry_price, p.notional, p.unrealized_pnl),
            (0.25, 100., 25.125, -0.125)
        );
        let o = h.e.store().order(&(n + 101).to_string()).unwrap();
        assert_eq!(
            (&*o.symbol, o.amount, o.filled, o.remaining),
            (&*m.symbol, 2., 0.25, 1.75)
        );
    }
}

#[test]
fn every_perpetual_has_independent_tickers_books_and_candles() {
    let mut h = Harness::new(false);
    let socket = h.socket(SocketChannel::Public);
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    for (n, (id, _)) in MARKETS.iter().enumerate() {
        let price = 100. + n as f64;
        h.ws(socket,&format!(r#"{{"s":"v1.ticker.d","sn":"0","f":{{"i":"{id}","lp":"{price}","op":"100","bv":"2","sv":"3"}}}}"#));
        assert_eq!(
            h.e.store().ticker(&id.to_ascii_uppercase()).unwrap().last,
            price
        );
        h.command(Command::SubscribeOrderBook {
            symbol: id.to_ascii_uppercase(),
        });
        h.ws(socket,&format!(r#"{{"s":"v1.book.s","sn":"0","f":{{"i":"{id}","b":[{{"p":"99","s":"2"}}],"a":[{{"p":"101","s":"3"}}]}}}}"#));
        assert_eq!(
            h.e.venue().book(&id.to_ascii_uppercase()).unwrap().bids(),
            [(99., 2.)]
        );
        h.command(Command::FetchCandles {
            command: 50 + n as u32,
            symbol: id.to_ascii_uppercase(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        });
        let r = h.take("kline");
        assert!(
            r.body
                .as_deref()
                .unwrap()
                .contains(&format!(r#""i":"{id}""#))
        );
        h.respond(r,r#"{"r":[{"ot":"1700000000000000000","o":"100","h":"102","l":"99","c":"101","vb":"4"}]}"#," ");
        assert!(h.result(50 + n as u32).unwrap().contains("101.0"));
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    symbol: String,
    qty: f64,
    realized_pnl: Option<f64>,
}

#[test]
fn broad_and_exact_fill_reads_isolate_supported_markets() {
    let mut h = Harness::new(true);
    for (n, (id, _)) in MARKETS.iter().enumerate() {
        let command = 100 + n as u32;
        h.command(Command::Query {
            command,
            query: Query::Fills {
                symbol: Some(id.to_ascii_uppercase()),
                order_id: Some((n + 101).to_string()),
                from_ms: Some(NOW - 1000.),
                to_ms: Some(NOW + 100.),
                limit: Some(1000),
            },
        });
        let r = h.take("fill_history");
        assert!(!r.body.as_deref().unwrap().contains(r#""k":"#));
        let rows=MARKETS.iter().enumerate().map(|(j,(i,_))|format!(r#"{{"et":"1700000000000000000","sa":"777","i":"{i}","ti":"fill-{j}","co":"{}","oi":"0x{:04x}","ib":true,"s":"0.2","p":"100","f":"0.01","rp":"-0.125","fc":"USDT"}}"#,j+101,j+1)).collect::<Vec<_>>().join(",");
        h.respond(r, &format!(r#"{{"r":[{rows}],"n":""}}"#), "");
        let fills: Vec<Execution> = serde_json::from_str(&h.result(command).unwrap()).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].id, format!("fill-{n}"));
        assert_eq!(fills[0].symbol, id.to_ascii_uppercase());
        assert_eq!(fills[0].qty, 0.2);
        assert_eq!(fills[0].realized_pnl, Some(-0.125));
    }
    h.command(Command::Query {
        command: 200,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(NOW - 1000.),
            to_ms: Some(NOW + 100.),
            limit: Some(1000),
        },
    });
    let r = h.take("fill_history");
    assert!(!r.body.as_deref().unwrap().contains(r#""k":"#));
    h.respond(r, r#"{"r":[],"n":""}"#, "");
    assert_eq!(h.result(200).unwrap(), "[]");
}

#[test]
fn malformed_catalog_refresh_keeps_the_previous_row_and_commits_valid_neighbours() {
    let mut h = Harness::new(false);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > NOW + 299000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now = NOW + 300010.;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    let r = h.take("all_instruments");
    let malformed = catalog().replace(r#""ts":"0.1""#, r#""ts":"0.2""#).replace(
        r#""b":"AVAX","q":"USDT","ts":"0.2""#,
        r#""b":"AVAX","q":"USDT","ts":"NaN""#,
    );
    assert!(malformed.contains(r#""ts":"NaN""#));
    h.respond(r, &malformed, "");
    // The malformed AVAX row keeps its previous market so held rows stay mapped.
    assert_eq!(h.e.store().markets().len(), MARKETS.len());
    for market in h.e.store().markets() {
        let expected = if market.base == "AVAX" { 0.1 } else { 0.2 };
        assert_eq!(market.precision.price, expected, "{}", market.symbol);
    }
}

#[test]
fn metadata_only_refresh_updates_perpetual_rails_without_losing_private_rows() {
    let mut h = Harness::new(true);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > NOW + 299000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now = NOW + 300010.;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    let r = h.take("all_instruments");
    h.respond(
        r,
        &catalog()
            .replace(r#""ts":"0.1""#, r#""ts":"0.2""#)
            .replace("0x030505", "0x040505"),
        "",
    );
    for (id, _) in MARKETS {
        let market = h.e.store().market_by_id(id).unwrap();
        assert_eq!(market.precision.price, 0.2);
        assert_eq!(market.is_perpetual, Some(true));
    }
    assert_eq!(h.e.store().positions().len(), MARKETS.len());
    assert_eq!(h.e.store().orders().len(), MARKETS.len());
}

#[test]
fn perpetual_push_beats_stale_rest_without_erasing_other_markets() {
    let mut h = Harness::new(true);
    let socket = h.socket(SocketChannel::Private);
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    let stale = h.take("positions");
    let (id, _) = MARKETS[4];
    h.ws(socket, &format!(r#"{{"s":"v1.position","f":{{"i":"{id}","s":"0.5","ep":"100","mp":"97.5","n":"48.75","up":"-1.25","l":"10","el":"200","mt":"CROSS"}}}}"#));
    h.respond(stale, &positions(), "");
    let row =
        h.e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == id.to_ascii_uppercase())
            .unwrap();
    assert_eq!((row.contracts, row.unrealized_pnl), (0.5, -1.25));
    assert_eq!(h.e.store().positions().len(), MARKETS.len());
    for (other, _) in MARKETS.iter().filter(|(other, _)| *other != id) {
        let row =
            h.e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == other.to_ascii_uppercase())
                .unwrap();
        assert_eq!((row.contracts, row.unrealized_pnl), (0.25, -0.125));
    }
}

fn no_cancel(h: &Harness) {
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("cancel_order") || r.url.ends_with("cancel_all_orders"))
    );
}
fn cancel_symbol(h: &mut Harness, n: usize, command: u32) -> HttpRequest {
    h.command(Command::CancelSymbolOrders {
        command,
        symbol: MARKETS[n].0.to_ascii_uppercase(),
    });
    let read = h.take("open_orders");
    assert_eq!(read.body.as_deref(), Some(r#"{"sa":"777"}"#));
    no_cancel(h);
    read
}

#[test]
fn exact_symbol_cancellation_reads_remote_snapshot_and_preserves_other_markets() {
    for (n, (id, _)) in MARKETS.iter().enumerate() {
        let mut h = Harness::new(true);
        let read = cancel_symbol(&mut h, n, 300);
        h.respond(read, &orders(), "");
        let write = h.take("cancel_order");
        assert_eq!(
            write.body.unwrap(),
            format!(r#"{{"sa":"777","oi":"0x{:04x}"}}"#, n + 1)
        );
        no_cancel(&h);
        for (j, _) in MARKETS.iter().enumerate() {
            assert_eq!(
                h.e.store().order(&(j + 101).to_string()).unwrap().status,
                if j == n {
                    safe_cex_engine::model::OrderStatus::Canceling
                } else {
                    safe_cex_engine::model::OrderStatus::Open
                }
            );
        }
        // The acknowledgement can be the official returned Order shape too.
        let proof = order_row(id, &format!("0x{:04x}", n + 1), &(n + 101).to_string());
        h.e.handle(
            h.now + 1.,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &format!(r#"{{"result":{proof},"request_id":"fixture"}}"#),
                },
            },
        );
        h.collect();
        assert_eq!(h.result(300).unwrap(), "[]");
        assert_eq!(h.e.store().orders().len(), MARKETS.len());
        no_cancel(&h);
    }
    let mut h = Harness::new(true);
    h.e.store_mut().set_orders(vec![]);
    let read = cancel_symbol(&mut h, 4, 301);
    h.respond(read, &orders(), "");
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x0005"}"#));
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    assert_eq!(h.result(301).unwrap(), "[]");
    assert!(
        h.e.store().orders().is_empty(),
        "remote cancellation must not fabricate a local order"
    );
}

#[test]
fn empty_snapshot_is_complete_but_truncated_malformed_or_ambiguous_snapshots_never_write() {
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 302);
    h.respond(read, r#"{"r":[]}"#, "");
    assert_eq!(h.result(302).unwrap(), "[]");
    no_cancel(&h);
    let mut bad = vec![
        "{}".to_owned(),
        r#"{"r":null}"#.into(),
        format!(r#"{},"n":"next"}}"#, orders().trim_end_matches('}')),
        format!(r#"{},"next":""}}"#, orders().trim_end_matches('}')),
        format!(r#"{},"truncated":false}}"#, orders().trim_end_matches('}')),
        format!(r#"{},"has_more":true}}"#, orders().trim_end_matches('}')),
        format!(r#"{},"result":[]}}"#, orders().trim_end_matches('}')),
        orders().replace(r#""sa":"777""#, r#""sa":"888""#),
        orders().replace(r#""sa":"777","oi":"0x0007""#, r#""oi":"0x0007""#),
        orders().replace("0x0007", "not-an-id"),
        orders().replace("0x0007", "0x100000000000000000000000000000000"),
        orders().replace("0x0007", "0x1"), // aliases the first row's native ID
        orders().replace(r#""co":"107""#, r#""co":"101""#),
        orders().replace(r#""s":"2","lp":"100""#, r#""s":"NaN","lp":"100""#),
        orders().replace(r#""i":"AVAX_USDT_Perp""#, r#""i":"unknown-instrument""#),
        format!(
            r#"{{"r":[{},{{"sa":"777","l":[{{"i":"BTC_USDT_Perp"}},{{"i":"SOL_USDT_Perp"}}]}}]}}"#,
            order_row(MARKETS[4].0, "0x1234", "400")
        ),
        orders().replace(
            r#""sa":"777","oi":"0x0007""#,
            r#""sa":"777","e":{"c":1},"oi":"0x0007""#,
        ),
    ];
    for key in ["e", "error", "c", "code"] {
        for value in ["null", "false", "0"] {
            bad.push(orders().replace(
                r#""sa":"777","oi":"0x0007""#,
                &format!(r#""sa":"777","{key}":{value},"oi":"0x0007""#),
            ));
        }
    }
    for body in bad {
        let mut h = Harness::new(true);
        let read = cancel_symbol(&mut h, 4, 303);
        h.respond(read, &body, "");
        assert!(h.result(303).is_err(), "must reject {body}");
        no_cancel(&h);
        assert!(
            h.e.store()
                .orders()
                .iter()
                .all(|o| o.status == safe_cex_engine::model::OrderStatus::Open)
        );
    }
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 304);
    h.respond_status(read, 206, &orders(), "");
    assert!(h.result(304).is_err());
    no_cancel(&h);
}

#[test]
fn full_uint64_client_fallback_is_exact_and_zero_is_only_an_absent_sentinel() {
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 305);
    h.respond(
        read,
        &format!(
            r#"{{"r":[{}]}}"#,
            order_row(MARKETS[4].0, "0x000000", "18446744073709551615")
        ),
        "",
    );
    let write = h.take("cancel_order");
    assert_eq!(
        write.body.as_deref(),
        Some(r#"{"sa":"777","co":"18446744073709551615"}"#)
    );
    h.respond(write, r#"{"r":{"ack":true}}"#, "");
    assert_eq!(h.result(305).unwrap(), "[]");
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 306);
    let row1 = order_row(MARKETS[4].0, "0x100", "0");
    let row2 = order_row(MARKETS[5].0, "0x101", "0");
    h.respond(read, &format!(r#"{{"r":[{row1},{row2}]}}"#), "");
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x100"}"#));
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    assert!(h.result(306).is_ok());
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 307);
    h.respond(
        read,
        &format!(r#"{{"r":[{}]}}"#, order_row(MARKETS[4].0, "0x0", "0")),
        "",
    );
    assert!(h.result(307).is_err());
    no_cancel(&h);
}

#[test]
fn cancellation_snapshot_requires_current_session_and_auth_retry_keeps_exact_read() {
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 308);
    h.respond(
        read,
        &orders(),
        "X-Ts-Grvt-Session: rotated-session\nX-Grvt-Account-Id: fixture-account",
    );
    assert!(h.result(308).unwrap_err().contains("session"));
    no_cancel(&h);
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 309);
    let body = read.body.clone();
    h.respond(read, r#"{"c":1000,"m":"session expired"}"#, "");
    let login = h.take("login");
    h.respond(
        login,
        r#"{"sub_account_id":"777"}"#,
        "X-Ts-Grvt-Session: fresh-session\nX-Grvt-Account-Id: fixture-account",
    );
    let read = h.take("open_orders");
    assert_eq!(read.body, body);
    assert!(
        read.headers
            .iter()
            .any(|(k, v)| *k == "X-Ts-Grvt-Session" && v == "fresh-session")
    );
    h.respond(read, &orders(), "");
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x0005"}"#));
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    assert!(h.result(309).is_ok());
}

#[test]
fn partial_cancel_failure_preserves_acknowledged_pending_state_and_never_retries_uncertainty() {
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 310);
    let first = order_row(MARKETS[4].0, "0x0005", "105");
    let second = order_row(MARKETS[4].0, "0x1234", "800");
    h.respond(
        read,
        &format!(
            r#"{{"r":[{first},{second},{}]}}"#,
            order_row(MARKETS[5].0, "0x0006", "106")
        ),
        "",
    );
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x0005"}"#));
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x1234"}"#));
    h.e.handle(
        h.now + 1.,
        Event::HttpFailed {
            request: write.id,
            error: "timeout",
        },
    );
    h.collect();
    let error = h.result(310).unwrap_err();
    assert!(error.contains("105") && error.contains("800"));
    no_cancel(&h);
    assert_eq!(
        h.e.store().order("105").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceling
    );
    assert_eq!(
        h.e.store().order("106").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn malformed_rejected_or_wrong_identity_cancel_ack_stops_the_remaining_writes() {
    let proof = order_row(MARKETS[4].0, "0x999", "105");
    let good_proof = order_row(MARKETS[4].0, "0x0005", "105");
    let mut bodies = vec![
        r#"{"r":{"a":false}}"#.to_owned(),
        r#"{"r":{}}"#.into(),
        r#"{"r":{"a":true},"e":{"c":999}}"#.into(),
        r#"{"r":{"a":true,"error":"bad"}}"#.into(),
        format!(r#"{{"r":{proof}}}"#),
        format!(
            r#"{{"r":{},"a":false}}}}"#,
            good_proof.strip_suffix('}').unwrap()
        ),
        format!(
            r#"{{"r":{},"ack":false}}}}"#,
            good_proof.strip_suffix('}').unwrap()
        ),
        format!(
            r#"{{"r":{},"a":true}}}}"#,
            good_proof.strip_suffix('}').unwrap()
        ),
        format!(
            r#"{{"r":{},"ack":true}}}}"#,
            good_proof.strip_suffix('}').unwrap()
        ),
        format!(
            r#"{{"r":{},"a":null}}}}"#,
            good_proof.strip_suffix('}').unwrap()
        ),
    ];
    for key in ["e", "error", "c", "code"] {
        for value in ["null", "false", "0"] {
            bodies.push(format!(
                r#"{{"r":{},"{key}":{value}}}}}"#,
                good_proof.strip_suffix('}').unwrap()
            ));
        }
    }
    for body in bodies {
        let _: Box<serde_json::value::RawValue> =
            serde_json::from_str(&body).expect("adversarial fixture is valid JSON");
        let mut h = Harness::new(true);
        let read = cancel_symbol(&mut h, 4, 311);
        h.respond(
            read,
            &format!(
                r#"{{"r":[{},{}]}}"#,
                order_row(MARKETS[4].0, "0x0005", "105"),
                order_row(MARKETS[4].0, "0x1234", "800")
            ),
            "",
        );
        let write = h.take("cancel_order");
        h.respond(write, &body, "");
        assert!(h.result(311).is_err());
        no_cancel(&h);
    }
}

fn private_socket(h: &mut Harness) -> SocketId {
    let socket = h.socket(SocketChannel::Private);
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    while let Some(r) = h.requests.pop() {
        let body = if r.url.ends_with("positions") {
            positions()
        } else if r.url.ends_with("open_orders") {
            orders()
        } else {
            r#"{"r":[]}"#.into()
        };
        h.respond(r, &body, "");
    }
    socket
}
#[allow(clippy::too_many_arguments)]
fn order_push(
    h: &mut Harness,
    socket: SocketId,
    id: &str,
    native: &str,
    client: &str,
    status: &str,
    filled: &str,
    new_creation: bool,
) {
    let mut row = order_row(id, native, client)
        .replace(r#""s":"OPEN""#, &format!(r#""s":"{status}""#))
        .replace(r#""ts":["0.25"]"#, &format!(r#""ts":["{filled}"]"#))
        .replace("1700000000001000000", "1700000000999000000");
    if new_creation {
        row = row.replace("1700000000000000000", "1700000000005000000");
    }
    h.ws(socket, &format!(r#"{{"s":"v1.order","f":{row}}}"#));
}

#[test]
fn terminal_and_reused_client_events_during_snapshot_fail_before_any_cancel() {
    for native in [true, false] {
        let mut h = Harness::new(true);
        let socket = private_socket(&mut h);
        let read = cancel_symbol(&mut h, 4, 320);
        order_push(
            &mut h,
            socket,
            MARKETS[4].0,
            "0x0005",
            "105",
            "FILLED",
            "2",
            false,
        );
        let snapshot = if native {
            orders()
        } else {
            orders().replace("0x0005", "0x0")
        };
        h.respond(read, &snapshot, "");
        assert!(h.result(320).is_err());
        no_cancel(&h);
        assert!(
            h.e.store()
                .order("105")
                .is_none_or(|o| o.status == safe_cex_engine::model::OrderStatus::Closed)
        );
        assert_eq!(
            h.e.store().order("106").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        let mut h = Harness::new(true);
        let socket = private_socket(&mut h);
        let read = cancel_symbol(&mut h, 4, 321);
        order_push(
            &mut h,
            socket,
            MARKETS[4].0,
            "0xbeef",
            "105",
            "OPEN",
            "0",
            true,
        );
        let newer = h.e.store().order("105").unwrap().clone();
        h.respond(read, &snapshot, "");
        assert!(h.result(321).is_err());
        no_cancel(&h);
        assert_eq!(h.e.store().order("105").unwrap(), &newer);
    }
}

#[test]
fn later_target_terminal_or_reused_lifecycle_aborts_after_only_the_first_cancel() {
    for terminal in [true, false] {
        let mut h = Harness::new(true);
        let socket = private_socket(&mut h);
        let read = cancel_symbol(&mut h, 4, 322);
        let first = order_row(MARKETS[4].0, "0x0005", "105");
        let second = order_row(MARKETS[4].0, "0x1234", "800");
        h.respond(read, &format!(r#"{{"r":[{first},{second}]}}"#), "");
        let write = h.take("cancel_order");
        order_push(
            &mut h,
            socket,
            MARKETS[4].0,
            if terminal { "0x1234" } else { "0xbeef" },
            "800",
            if terminal { "FILLED" } else { "OPEN" },
            if terminal { "2" } else { "0" },
            !terminal,
        );
        h.respond(write, r#"{"r":{"a":true}}"#, "");
        assert!(h.result(322).is_err());
        no_cancel(&h);
        assert_eq!(
            h.e.store().order("105").unwrap().status,
            safe_cex_engine::model::OrderStatus::Canceling
        );
        assert_eq!(
            h.e.store().order("106").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
    }
}

#[test]
fn late_partial_fills_remain_authoritative_while_cancelling_only_the_native_remainder() {
    let mut h = Harness::new(true);
    let socket = private_socket(&mut h);
    let read = cancel_symbol(&mut h, 4, 323);
    order_push(
        &mut h,
        socket,
        MARKETS[4].0,
        "0x0005",
        "105",
        "OPEN",
        "1",
        false,
    );
    h.respond(read, &orders(), "");
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x0005"}"#));
    assert_eq!(
        (
            h.e.store().order("105").unwrap().filled,
            h.e.store().order("105").unwrap().remaining
        ),
        (1., 1.)
    );
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    assert!(h.result(323).is_ok());
    assert_eq!(
        (
            h.e.store().order("105").unwrap().filled,
            h.e.store().order("105").unwrap().remaining
        ),
        (1., 1.)
    );
    assert_eq!(h.e.store().order("106").unwrap().filled, 0.25);
}

#[test]
fn explicit_auth_retry_preserves_cancel_identity_and_rechecks_terminal_client_evidence() {
    for terminal in [false, true] {
        let mut h = Harness::new(true);
        let socket = private_socket(&mut h);
        let read = cancel_symbol(&mut h, 4, 324);
        h.respond(read, &orders().replace("0x0005", "0x0"), "");
        let write = h.take("cancel_order");
        let original = write.body.clone();
        assert_eq!(original.as_deref(), Some(r#"{"sa":"777","co":"105"}"#));
        if terminal {
            order_push(
                &mut h,
                socket,
                MARKETS[4].0,
                "0x0005",
                "105",
                "FILLED",
                "2",
                false,
            );
        }
        h.respond(write, r#"{"c":1000,"m":"session expired"}"#, "");
        let login = h.take("login");
        h.respond(
            login,
            r#"{"sub_account_id":"777"}"#,
            "X-Ts-Grvt-Session: renewed-session\nX-Grvt-Account-Id: fixture-account",
        );
        if terminal {
            assert!(h.result(324).is_err());
            no_cancel(&h);
            assert!(
                h.e.store()
                    .order("105")
                    .is_none_or(|o| o.status == safe_cex_engine::model::OrderStatus::Closed)
            );
        } else {
            let retry = h.take("cancel_order");
            assert_eq!(retry.body, original);
            assert!(
                retry
                    .headers
                    .iter()
                    .any(|(k, v)| *k == "X-Ts-Grvt-Session" && v == "renewed-session")
            );
            h.respond(retry, r#"{"r":{"a":true}}"#, "");
            assert!(h.result(324).is_ok());
            no_cancel(&h);
        }
    }
}

#[derive(Deserialize)]
struct SignedCreate {
    o: SignedOrder,
}
#[derive(Deserialize)]
struct SignedOrder {
    sa: String,
    l: Vec<SignedLeg>,
    s: SignedSignature,
    m: SignedMetadata,
    b: String,
    bf: String,
}
#[derive(Deserialize)]
struct SignedLeg {
    i: String,
    s: String,
    lp: String,
}
#[derive(Deserialize)]
struct SignedSignature {
    r: String,
    s1: String,
    v: u8,
    e: String,
    n: u32,
}
#[derive(Deserialize)]
struct SignedMetadata {
    co: String,
}

#[test]
fn all_perpetual_orders_keep_refreshed_native_asset_hash_and_exact_signing_units() {
    let mut h = Harness::new(true);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > NOW + 299000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now = NOW + 300010.;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    let r = h.take("all_instruments");
    h.respond(
        r,
        &catalog()
            .replace("0x030505", "0x040505")
            .replace(r#""bd":9"#, r#""bd":3"#),
        "",
    );
    let signer: PrivateKeySigner = format!("0x{}", "01".repeat(32)).parse().unwrap();
    for (n, (id, _)) in MARKETS.iter().enumerate() {
        let client = (900 + n).to_string();
        let options=serde_json::from_str(&format!(r#"{{"symbol":"{}","type":"limit","side":"buy","amount":0.125,"price":101.2,"clientOrderId":"{client}"}}"#,id.to_ascii_uppercase())).unwrap();
        h.command(Command::PlaceOrders {
            command: 400 + n as u32,
            orders: vec![options],
        });
        let write = h.take("create_order");
        let native: SignedCreate = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        let o = native.o;
        assert_eq!(o.sa, "777");
        assert_eq!(o.l.len(), 1);
        assert_eq!(o.l[0].i, *id);
        assert_eq!(o.l[0].s, "0.125");
        assert_eq!(o.l[0].lp, "101.2");
        assert_eq!(o.m.co, client);
        assert_eq!(o.b, "0xee8f3559f298fffa79b8253437969d15b5dccff3");
        assert_eq!(o.bf, "0.03");
        let hash = if n == 1 {
            0x30508
        } else if n == 4 {
            0x40505
        } else {
            0x30501 + n
        };
        // Independent ABI/EIP712 oracle: economics and asset hashes are explicit
        // fixture inputs; only the intentionally random nonce comes from the wire.
        let typed:TypedData=serde_json::from_str(&format!(r#"{{"domain":{{"name":"GRVT Exchange","version":"0","chainId":325}},"primaryType":"OrderWithBuilderFee","types":{{"EIP712Domain":[{{"name":"name","type":"string"}},{{"name":"version","type":"string"}},{{"name":"chainId","type":"uint256"}}],"OrderWithBuilderFee":[{{"name":"subAccountID","type":"uint64"}},{{"name":"isMarket","type":"bool"}},{{"name":"timeInForce","type":"uint8"}},{{"name":"postOnly","type":"bool"}},{{"name":"reduceOnly","type":"bool"}},{{"name":"legs","type":"OrderLeg[]"}},{{"name":"builder","type":"address"}},{{"name":"builderFee","type":"uint32"}},{{"name":"nonce","type":"uint32"}},{{"name":"expiration","type":"int64"}}],"OrderLeg":[{{"name":"assetID","type":"uint256"}},{{"name":"contractSize","type":"uint64"}},{{"name":"limitPrice","type":"uint64"}},{{"name":"isBuyingContract","type":"bool"}}]}},"message":{{"subAccountID":"777","isMarket":false,"timeInForce":1,"postOnly":false,"reduceOnly":false,"legs":[{{"assetID":"0x{hash:06x}","contractSize":"125","limitPrice":"101200000000","isBuyingContract":true}}],"builder":"0xee8f3559f298fffa79b8253437969d15b5dccff3","builderFee":300,"nonce":{},"expiration":"{}"}}}}"#,o.s.n,o.s.e)).unwrap();
        let expected = signer
            .sign_dynamic_typed_data_sync(&typed)
            .unwrap()
            .to_string();
        assert_eq!(
            format!("0x{}{}{:02x}", &o.s.r[2..], &o.s.s1[2..], o.s.v),
            expected,
            "wrong signature for {id}"
        );
        h.respond(
            write,
            &format!(
                r#"{{"r":{{"oi":"0x{:04x}","m":{{"co":"{client}"}}}}}}"#,
                n + 500
            ),
            "",
        );
        assert_eq!(
            h.result(400 + n as u32).unwrap(),
            format!(r#"["{client}"]"#)
        );
    }
}

#[test]
fn streamed_cancel_snapshot_stages_until_complete_and_resets_after_partial_failure() {
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 330);
    let row = order_row(MARKETS[4].0, "0x0005", "105");
    h.e.handle(
        h.now,
        Event::HttpStart {
            request: read.id,
            status: 200,
            headers: "",
        },
    );
    h.e.handle(
        h.now,
        Event::HttpChunk {
            request: read.id,
            data: format!(r#"{{"r":[{row},"#).as_bytes(),
        },
    );
    h.collect();
    no_cancel(&h);
    assert_eq!(
        h.e.store().order("105").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: read.id,
            error: "partial network failure",
        },
    );
    h.collect();
    let retry = h.take("open_orders");
    assert_eq!(retry.body, read.body);
    h.e.handle(
        h.now,
        Event::HttpStart {
            request: retry.id,
            status: 200,
            headers: "",
        },
    );
    h.e.handle(
        h.now,
        Event::HttpChunk {
            request: retry.id,
            data: orders().as_bytes(),
        },
    );
    h.collect();
    no_cancel(&h);
    h.e.handle(h.now, Event::HttpEnd { request: retry.id });
    h.collect();
    let write = h.take("cancel_order");
    assert_eq!(write.body.as_deref(), Some(r#"{"sa":"777","oi":"0x0005"}"#));
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    assert!(h.result(330).is_ok());
    no_cancel(&h);
}

#[test]
fn truncated_or_auth_rejected_stream_never_promotes_staged_cancel_rows() {
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 331);
    let row = order_row(MARKETS[4].0, "0x0005", "105");
    h.e.handle(
        h.now,
        Event::HttpStart {
            request: read.id,
            status: 200,
            headers: "",
        },
    );
    h.e.handle(
        h.now,
        Event::HttpChunk {
            request: read.id,
            data: format!(r#"{{"r":[{row},"#).as_bytes(),
        },
    );
    h.e.handle(h.now, Event::HttpEnd { request: read.id });
    h.collect();
    no_cancel(&h);
    let retry = h.take("open_orders");
    h.respond(retry, r#"{"r":null}"#, "");
    assert!(h.result(331).is_err());
    no_cancel(&h);
    let mut h = Harness::new(true);
    let read = cancel_symbol(&mut h, 4, 332);
    let bad = order_row(MARKETS[5].0, "0x0006", "106").replace(r#""sa":"777""#, r#""sa":"888""#);
    h.respond(
        read,
        &format!(r#"{{"r":[{row},{bad}],"c":1000,"m":"session expired"}}"#),
        "",
    );
    no_cancel(&h);
    let login = h.take("login");
    h.respond(
        login,
        r#"{"sub_account_id":"777"}"#,
        "X-Ts-Grvt-Session: fresh-session\nX-Grvt-Account-Id: fixture-account",
    );
    let retry = h.take("open_orders");
    h.respond(retry, &orders(), "");
    let write = h.take("cancel_order");
    h.respond(write, r#"{"r":{"a":true}}"#, "");
    assert!(h.result(332).is_ok());
    no_cancel(&h);
}

#[test]
fn a_mismatched_configured_signer_warns_without_blocking_startup() {
    let secret = format!("0x{}", "01".repeat(32));
    let signer = secret
        .parse::<PrivateKeySigner>()
        .unwrap()
        .address()
        .to_string();
    for (configured, warns) in [
        ("0x0000000000000000000000000000000000000001", true),
        (signer.as_str(), false),
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_grvt::Grvt::new(
            safe_cex_venue_grvt::Options {
                key: Some("fixture-key".into()),
                secret: Some(secret.clone()),
                signer_address: Some(configured.into()),
                ..Default::default()
            },
        ));
        e.start(0.0);
        let warned = e.drain_actions().any(|a| {
            matches!(a,
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.level == safe_cex_engine::Level::Warn
                    && event.message.contains("does not match wallet address"))
        });
        assert_eq!(warned, warns, "{configured}");
    }
}
