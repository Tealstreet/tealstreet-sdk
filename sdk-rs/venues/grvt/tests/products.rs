use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, SocketChannel, SocketId,
    Venue,
};
use safe_cex_venue_grvt::{Grvt, Options};
use serde::Deserialize;

const NOW: f64 = 1_700_000_000_000.;
const PRODUCTS: [(&str, &str); 7] = [
    ("BTC_USDT_Perp", "PERPETUAL"),
    ("BTC_USDT_StablePerp", "STABLE_PERP"),
    ("BTC_USDT_20261225", "FUTURE"),
    ("BTC_USDT_20270326", "FUTURE"),
    ("BTC_USDT_20261225_50000_C", "CALL"),
    ("BTC_USDT_20261225_60000_C", "CALL"),
    ("BTC_USDT_20261225_50000_P", "PUT"),
];
fn catalog() -> String {
    format!(r#"{{"r":[{}]}}"#, PRODUCTS.iter().enumerate().map(|(n,(id,kind))| format!(r#"{{"i":"{id}","k":"{kind}","ih":"0x{:06x}","b":"BTC","q":"USDT","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}}"#, if n == 1 { 0x30508 } else { 0x30501+n })).collect::<Vec<_>>().join(","))
}
fn positions() -> String {
    format!(r#"{{"r":[{}]}}"#, PRODUCTS.iter().map(|(id,_)| format!(r#"{{"i":"{id}","s":"-0.25","ep":"100","n":"25","up":"-0.125","l":"10","el":"200","mt":"CROSS"}}"#)).collect::<Vec<_>>().join(","))
}
fn orders() -> String {
    format!(r#"{{"r":[{}]}}"#, PRODUCTS.iter().enumerate().map(|(n,(id,_))| format!(r#"{{"oi":"0x{:04x}","im":false,"ro":false,"po":false,"ti":"GOOD_TILL_TIME","l":[{{"i":"{id}","s":"2","lp":"100","ib":true}}],"m":{{"co":"{}","ct":"1700000000000000000"}},"s1":{{"s":"OPEN","ts":["0.25"],"ut":"1700000000001000000"}}}}"#, n+1,n+101)).collect::<Vec<_>>().join(","))
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
            e: Engine::new(Grvt::new(Options {
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
        for a in self.e.drain_actions() {
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
                }
                | Action::WsConnectWithHeaders {
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
            .position(
                |a| matches!(a,Action::Notice(Notice::CommandDone{command:c,..}) if *c==command),
            )
            .expect("command completion");
        let Action::Notice(Notice::CommandDone { result, .. }) = self.actions.remove(index) else {
            unreachable!()
        };
        result
    }
}

#[test]
fn derivative_catalog_and_private_snapshots_preserve_every_native_identity() {
    let h = Harness::new(true);
    assert_eq!(h.e.store().markets().len(), PRODUCTS.len());
    assert_eq!(h.e.store().positions().len(), PRODUCTS.len());
    assert_eq!(h.e.store().orders().len(), PRODUCTS.len());
    for (n, (id, kind)) in PRODUCTS.iter().enumerate() {
        let m = h.e.store().market_by_id(id).unwrap();
        assert_eq!(m.symbol, id.to_ascii_uppercase());
        assert_eq!(m.is_perpetual, Some(*kind == "PERPETUAL"));
        let p =
            h.e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == m.symbol)
                .unwrap();
        assert_eq!(
            (p.contracts, p.entry_price, p.notional, p.unrealized_pnl),
            (0.25, 100., 25., -0.125)
        );
        let o = h.e.store().order(&(n + 101).to_string()).unwrap();
        assert_eq!(
            (&*o.symbol, o.amount, o.filled, o.remaining),
            (&*m.symbol, 2., 0.25, 1.75)
        );
    }
}

#[test]
fn every_derivative_has_independent_tickers_books_and_candles() {
    let mut h = Harness::new(false);
    let socket = h.socket(SocketChannel::Public);
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    for (n, (id, _)) in PRODUCTS.iter().enumerate() {
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
            interval: "1m".into(),
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
fn broad_and_exact_fill_reads_do_not_filter_derivative_families() {
    let mut h = Harness::new(true);
    for (n, (id, _)) in PRODUCTS.iter().enumerate() {
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
        let rows=PRODUCTS.iter().enumerate().map(|(j,(i,_))|format!(r#"{{"et":"1700000000000000000","sa":"777","i":"{i}","ti":"fill-{j}","co":"{}","oi":"0x{:04x}","ib":true,"s":"0.2","p":"100","f":"0.01","rp":"-0.125","fc":"USDT"}}"#,j+101,j+1)).collect::<Vec<_>>().join(",");
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
fn mixed_catalog_refresh_is_atomic_and_keeps_existing_product_data() {
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
    let malformed = catalog().replace(r#""k":"PUT""#, r#""k":"PUT","ts":"NaN""#);
    h.respond(r, &malformed, "");
    assert_eq!(h.e.store().markets().len(), PRODUCTS.len());
    assert_eq!(
        h.e.store()
            .market_by_id(PRODUCTS[6].0)
            .unwrap()
            .precision
            .price,
        0.1
    );
}

#[test]
fn metadata_only_refresh_updates_derivative_rails_without_losing_private_rows() {
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
    for (id, kind) in PRODUCTS {
        let market = h.e.store().market_by_id(id).unwrap();
        assert_eq!(market.precision.price, 0.2);
        assert_eq!(market.is_perpetual, Some(kind == "PERPETUAL"));
    }
    assert_eq!(h.e.store().positions().len(), PRODUCTS.len());
    assert_eq!(h.e.store().orders().len(), PRODUCTS.len());
}

#[test]
fn derivative_push_beats_stale_rest_without_erasing_expiry_or_strike_siblings() {
    let mut h = Harness::new(true);
    let socket = h.socket(SocketChannel::Private);
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    let stale = h.take("positions");
    let (id, _) = PRODUCTS[4];
    h.ws(socket, &format!(r#"{{"s":"v1.position","f":{{"i":"{id}","s":"0.5","ep":"100","n":"50","up":"-1.25","l":"10","el":"200","mt":"CROSS"}}}}"#));
    h.respond(stale, &positions(), "");
    let row =
        h.e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == id.to_ascii_uppercase())
            .unwrap();
    assert_eq!((row.contracts, row.unrealized_pnl), (0.5, -1.25));
    assert_eq!(h.e.store().positions().len(), PRODUCTS.len());
    for (other, _) in PRODUCTS.iter().filter(|(other, _)| *other != id) {
        let row =
            h.e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == other.to_ascii_uppercase())
                .unwrap();
        assert_eq!((row.contracts, row.unrealized_pnl), (0.25, -0.125));
    }
}
