#![cfg(feature = "spot")]
//! KuCoin Spot documented HF/public payloads and current TS adapter contracts.
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Query, Response, SocketChannel,
    SocketId, Venue,
};
use safe_cex_venue_kucoin::{Options, spot::KucoinSpot};
use serde_json::{Value, json};
const MARKETS: &str = r#"{"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"100"},{"symbol":"ETH-USDT","baseCurrency":"ETH","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.01","priceIncrement":"0.01","baseMinSize":"0.01","baseMaxSize":"1000"}]}"#;
const TICKERS: &str = r#"{"code":"200000","data":{"time":1700000000000,"ticker":[{"symbol":"BTC-USDT","last":"100","buy":"99","sell":"101","changeRate":"0.02","vol":"10","volValue":"1000"},{"symbol":"ETH-USDT","last":"10","buy":"9","sell":"11"}]}}"#;
const ORDER: &str = r#"{"id":"order-1","symbol":"BTC-USDT","clientOid":"caller-1","type":"limit","side":"buy","price":"90","size":"2","dealSize":"0.5","remainSize":"1.5","status":"open","createdAt":1700000000000}"#;
struct H {
    e: Engine<KucoinSpot>,
    actions: Vec<Action>,
    requests: Vec<HttpRequest>,
    now: f64,
    public: SocketId,
    private: SocketId,
}
impl H {
    fn new(private: bool) -> Self {
        Self::with_read_credentials(private, None)
    }
    fn with_read_credentials(
        private: bool,
        read_credentials: Option<safe_cex_engine::ReadCredentials>,
    ) -> Self {
        let mut options = if private {
            Options {
                key: Some("replay-key".into()),
                secret: Some("replay-secret".into()),
                passphrase: Some("replay-passphrase".into()),
                ..Default::default()
            }
        } else {
            Options {
                public_data_only: true,
                ..Default::default()
            }
        };
        options.read_credentials = read_credentials;
        let mut h = Self {
            e: Engine::new(KucoinSpot::new(options)),
            actions: vec![],
            requests: vec![],
            now: 1700000000000.0,
            public: SocketId(0),
            private: SocketId(0),
        };
        h.e.start(h.now);
        h.collect();
        h.reply("/api/v2/symbols", MARKETS);
        h.reply("/allTickers", TICKERS);
        h.reply("/bullet-public",r#"{"code":"200000","data":{"token":"public-token","instanceServers":[{"endpoint":"wss://spot.example.test/public","pingInterval":18000,"pingTimeout":10000}]}}"#);
        h.public = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.event(Event::WsOpened { socket: h.public });
        h.event(Event::WsMessage {
            socket: h.public,
            text: r#"{"type":"welcome"}"#,
        });
        if private {
            h.reply("/bullet-private",r#"{"code":"200000","data":{"token":"private-token","instanceServers":[{"endpoint":"wss://spot.example.test/private","pingInterval":18000,"pingTimeout":10000}]}}"#);
            h.private = h
                .actions
                .iter()
                .find_map(|a| match a {
                    Action::WsConnect {
                        socket,
                        channel: Some(SocketChannel::Private),
                        ..
                    } => Some(*socket),
                    _ => None,
                })
                .unwrap();
            h.event(Event::WsOpened { socket: h.private });
            h.event(Event::WsMessage {
                socket: h.private,
                text: r#"{"type":"welcome"}"#,
            });
            h.reply("/accounts?type=trade",r#"{"code":"200000","data":[{"currency":"USDT","available":"900","holds":"100","balance":"1000"},{"currency":"BTC","available":"2","holds":"1","balance":"3"}]}"#);
            h.reply(
                "/active/symbols",
                r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
            );
            h.reply(
                "/active/page",
                &format!(r#"{{"code":"200000","data":{{"totalPage":1,"items":[{ORDER}]}}}}"#),
            );
        }
        h
    }
    fn collect(&mut self) {
        for action in self.e.drain_actions() {
            if let Action::Http(request) = &action {
                self.requests.push(request.clone());
            }
            if let Action::ClearTimer { timer } = &action {
                self.actions
                    .retain(|a| !matches!(a,Action::SetTimer{timer:old,..}if timer==old));
            }
            self.actions.push(action);
        }
    }
    fn fill_rows(&mut self) -> Vec<(f64, f64)> {
        let Some(frame) = self.e.take_frame(self.now) else {
            return vec![];
        };
        let Some((_, bytes)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
        else {
            return vec![];
        };
        let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let offset = (4 + 12 * count).div_ceil(8) * 8;
        (0..count)
            .map(|i| {
                let at = offset + 16 * i;
                (
                    f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()),
                    f64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap()),
                )
            })
            .collect()
    }
    fn event(&mut self, event: Event<'_>) {
        self.e.handle(self.now, event);
        self.collect();
    }
    fn http(&mut self, path: &str) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r)if r.url.contains(path)))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.actions));
        match self.actions.remove(i) {
            Action::Http(r) => r,
            _ => unreachable!(),
        }
    }
    fn respond(&mut self, r: &HttpRequest, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn reply(&mut self, path: &str, body: &str) {
        let r = self.http(path);
        self.respond(&r, 200, body);
    }
    fn ws(&mut self, private: bool, topic: &str, data: Value) {
        let text = json!({"type":"message","topic":topic,"data":data}).to_string();
        self.event(Event::WsMessage {
            socket: if private { self.private } else { self.public },
            text: &text,
        });
    }
    fn command(&mut self, c: Command) {
        self.event(Event::Command(c));
    }
    fn done(&mut self, id: u32) -> Result<Value, String> {
        let i = self
            .actions
            .iter()
            .position(
                |a| matches!(a,Action::Notice(Notice::CommandDone{command,..})if *command==id),
            )
            .unwrap();
        match self.actions.remove(i) {
            Action::Notice(Notice::CommandDone { result, .. }) => {
                result.map(|r| serde_json::from_str(&r).unwrap())
            }
            _ => unreachable!(),
        }
    }
    fn retry(&mut self, low: f64, high: f64) {
        let (index, timer, at) = self
            .actions
            .iter()
            .enumerate()
            .find_map(|(index, a)| match a {
                Action::SetTimer { timer, at_ms }
                    if (low..=high).contains(&(*at_ms - self.now)) =>
                {
                    Some((index, *timer, *at_ms))
                }
                _ => None,
            })
            .expect("bounded retry timer");
        self.actions.remove(index);
        self.now = at;
        self.event(Event::TimerFired { timer });
    }
    fn timer(&mut self, delay: f64) {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms==self.now+delay))
            .unwrap();
        match self.actions.remove(i) {
            Action::SetTimer { timer, at_ms } => {
                self.now = at_ms;
                self.event(Event::TimerFired { timer });
            }
            _ => unreachable!(),
        }
    }
}
fn place(id: Option<&str>) -> safe_cex_engine::model::PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTC-USDT","type":"limit","side":"buy","amount":1.2345,"price":95.26, "clientOrderId":id.unwrap_or("caller-123")})).unwrap()
}
#[test]
fn streamed_markets_keep_spot_identity_and_fixed_settings() {
    let h = H::new(false);
    let m = h.e.store().market("BTC-USDT").unwrap();
    assert_eq!(m.is_spot, Some(true));
    assert_eq!(m.pretty_symbol.as_deref(), Some("BTCUSDT"));
    assert_eq!(m.contract_size, Some(1.0));
    assert_eq!(h.e.store().setting("BTC-USDT").unwrap().leverage, 1.0);
    assert!(h.e.store().positions().is_empty());
    assert!(h.e.store().loaded().positions);
    assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().percentage, 2.0);
}
#[test]
fn public_ticker_all_uses_subject_and_preserves_absent_fields() {
    let mut h = H::new(false);
    let text = r#"{"type":"message","topic":"/market/ticker:all","subject":"BTC-USDT","data":{"bestBid":"102","bestAsk":"104","price":"103"}}"#;
    h.event(Event::WsMessage {
        socket: h.public,
        text,
    });
    let t = h.e.store().ticker("BTC-USDT").unwrap();
    assert_eq!(
        (t.bid, t.ask, t.last, t.mark, t.index),
        (102.0, 104.0, 103.0, 103.0, 103.0)
    );
    assert_eq!(t.percentage, 2.0);
    assert_eq!(t.volume, 10.0);
}
#[test]
fn ticker_envelopes_preserve_field_order_aliases_and_symbol_precedence() {
    let mut h = H::new(false);
    h.event(Event::WsMessage {
        socket: h.public,
        text: r#"{"data":{"symbol":"ETH-USDT","bestBidPrice":"12","bestAskPrice":"14","lastPrice":"13"},"subject":"BTC-USDT","topic":"/market/ticker:all","type":"message"}"#,
    });
    assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().last, 100.0);
    let eth = h.e.store().ticker("ETH-USDT").unwrap();
    assert_eq!((eth.bid, eth.ask, eth.last), (12.0, 14.0, 13.0));
    h.event(Event::WsMessage {
        socket: h.public,
        text: r#"{"data":{"price":"103"},"topic":"/market/ticker:BTC-USDT","type":"message"}"#,
    });
    assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().last, 103.0);
    h.event(Event::WsMessage {
        socket: h.public,
        text: r#"{"data":{"price":"104","bestBid":"invalid"},"subject":"BTC-USDT","topic":"/market/ticker:all","type":"message"}"#,
    });
    assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().last, 103.0);
}
#[test]
fn ticker_envelopes_cannot_bypass_control_kind_or_public_socket_readiness() {
    let mut h = H::new(false);
    for kind in ["ack", "pong"] {
        let text = json!({"data":{"price":"103"},"subject":"BTC-USDT","topic":"/market/ticker:all","type":kind}).to_string();
        h.event(Event::WsMessage {
            socket: h.public,
            text: &text,
        });
        assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().last, 100.0);
    }
    h.event(Event::WsClosed {
        socket: h.public,
        code: 1006,
    });
    h.retry(750.0, 1000.0);
    h.reply("/bullet-public",r#"{"code":"200000","data":{"token":"fresh-public","instanceServers":[{"endpoint":"wss://spot.example.test/public","pingInterval":18000,"pingTimeout":10000}]}}"#);
    let socket = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let text = r#"{"data":{"price":"103"},"subject":"BTC-USDT","topic":"/market/ticker:all","type":"message"}"#;
    h.event(Event::WsMessage { socket, text });
    assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().last, 100.0);
    h.event(Event::WsOpened { socket });
    h.event(Event::WsMessage {
        socket,
        text: r#"{"type":"welcome"}"#,
    });
    h.event(Event::WsMessage { socket, text });
    assert_eq!(h.e.store().ticker("BTC-USDT").unwrap().last, 103.0);
}
#[test]
fn book_snapshots_replace_sort_and_refcount_subscriptions() {
    let mut h = H::new(false);
    h.command(Command::SubscribeOrderBook {
        symbol: "BTC-USDT".into(),
    });
    h.command(Command::SubscribeOrderBook {
        symbol: "BTC-USDT".into(),
    });
    let n=h.actions.iter().filter(|a|matches!(a,Action::WsSend{text,..}if text.contains("level2Depth50")&&text.contains("subscribe"))).count();
    assert_eq!(n, 1);
    h.ws(
        false,
        "/spotMarket/level2Depth50:BTC-USDT",
        json!({"timestamp":1,"bids":[[99,2],[100,1]],"asks":[[102,4],[101,3]]}),
    );
    assert_eq!(
        h.e.venue().book("BTC-USDT").unwrap().bids(),
        &[(100.0, 1.0), (99.0, 2.0)]
    );
    h.ws(
        false,
        "/spotMarket/level2Depth50:BTC-USDT",
        json!({"timestamp":2,"bids":[[98,1]],"asks":[]}),
    );
    assert_eq!(h.e.venue().book("BTC-USDT").unwrap().bids(), &[(98.0, 1.0)]);
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTC-USDT".into(),
    });
    assert!(h.e.venue().book("BTC-USDT").is_some());
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTC-USDT".into(),
    });
    assert!(h.e.venue().book("BTC-USDT").is_none());
}
#[test]
fn candles_use_spot_order_and_second_timestamps() {
    let mut h = H::new(false);
    h.command(Command::FetchCandles {
        command: 1,
        symbol: "BTC-USDT".into(),
        interval: "1m".into(),
        limit: Some(1),
        from_ms: None,
        to_ms: Some(1700000060000.0),
    });
    let r = h.http("/market/candles?");
    assert!(
        r.url
            .ends_with("symbol=BTC-USDT&type=1min&startAt=1700000000&endAt=1700000060")
    );
    h.respond(
        &r,
        200,
        r#"{"code":"200000","data":[["1700000000","90","95","100","80","10"]]}"#,
    );
    assert_eq!(
        h.done(1).unwrap(),
        json!([{"timestamp":1700000000.0,"open":90.0,"close":95.0,"high":100.0,"low":80.0,"volume":10.0}])
    );
}
#[test]
fn balances_value_each_currency_without_futures_contract_conversion() {
    let h = H::new(true);
    let b = h.e.store().balance();
    assert_eq!((b.free, b.used, b.total), (1100.0, 200.0, 1300.0));
    assert_eq!(h.e.store().coin_balances().unwrap()[1].amount, 3.0);
    assert_eq!(
        h.e.store().currency_balances().unwrap()[1].balance.total,
        3.0
    );
    assert_eq!(h.e.store().orders()[0].filled, 0.5);
}
#[test]
fn hf_native_place_and_amend_keep_caller_identity_on_original() {
    let mut h = H::new(true);
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![place(None)],
    });
    let r = h.http("/hf/orders");
    assert_eq!(r.method, Method::Post);
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_deref().unwrap()).unwrap(),
        json!({"clientOid":"caller-123","symbol":"BTC-USDT","type":"limit","side":"buy","price":"95.3","size":"1.235","timeInForce":"GTC"})
    );
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "KC-API-PASSPHRASE" && !v.is_empty())
    );
    h.respond(&r, 200, r#"{"code":"200000","data":{"orderId":"new"}}"#);
    assert_eq!(h.done(1).unwrap(), json!(["new"]));
    h.command(Command::AmendOrder {
        command: 2,
        id: "order-1".into(),
        price: Some(91.21),
        amount: Some(2.002),
    });
    let r = h.http("/orders/alter");
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_deref().unwrap()).unwrap(),
        json!({"symbol":"BTC-USDT","orderId":"order-1","newPrice":"91.2","newSize":"2.002"})
    );
    h.respond(
        &r,
        200,
        r#"{"code":"200000","data":{"newOrderId":"amended"}}"#,
    );
    assert_eq!(h.done(2).unwrap(), json!(["amended"]));
    assert_eq!(
        h.e.store().orders()[0].client_order_id.as_deref(),
        Some("caller-1")
    );
}
#[test]
fn whole_batch_preflight_rejects_duplicate_ids_and_unsupported_shapes() {
    assert!(!KucoinSpot::new(Options::default()).supports_options());
    for invalid in ["duplicate", "bracket", "type", "id", "iv"] {
        let mut h = H::new(true);
        h.actions.clear();
        let mut a = place(None);
        let mut b = a.clone();
        if invalid != "duplicate" {
            a.client_order_id = None;
        }
        match invalid {
            "bracket" => b.stop_loss = Some(80.0),
            "type" => b.kind = safe_cex_engine::model::OrderType::Conditional,
            "id" => b.client_order_id = Some("x".repeat(41)),
            "iv" => b.implied_volatility = Some(0.5),
            _ => {}
        }
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![a, b],
        });
        let result = h.done(1);
        assert!(result.is_err());
        if invalid == "iv" {
            assert!(result.unwrap_err().contains("Options are not enabled"));
        }
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}
#[test]
fn generated_client_ids_are_unique_across_identically_started_engines() {
    let mut left = H::new(true);
    let mut right = H::new(true);
    assert_eq!(left.now, right.now);
    let mut order = place(None);
    order.client_order_id = None;
    let mut ids = Vec::new();
    for h in [&mut left, &mut right] {
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order.clone()],
        });
        let request = h.http("/hf/orders");
        let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let id = payload["clientOid"].as_str().unwrap().to_owned();
        assert!(!id.is_empty() && id.len() <= 40);
        ids.push(id);
    }
    assert_ne!(ids[0], ids[1]);
    assert!(
        ids.iter()
            .all(|id| id.bytes().all(|b| b.is_ascii_alphanumeric()))
    );
}
fn assert_prewelcome_pong_preserves_recovery(private: bool) {
    let mut h = H::new(private);
    let socket = if private { h.private } else { h.public };
    h.event(Event::WsClosed { socket, code: 1006 });
    h.retry(750.0, 1000.0);
    h.reply(if private { "/bullet-private" } else { "/bullet-public" }, r#"{"code":"200000","data":{"token":"replacement","instanceServers":[{"endpoint":"wss://spot.example.test/replacement","pingInterval":18000,"pingTimeout":10000}]}}"#);
    let socket = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.actions.clear();
    h.event(Event::WsOpened { socket });
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 5000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.event(Event::WsMessage {
        socket,
        text: r#"{"type":"pong"}"#,
    });
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer: t } if *t == timer))
    );
    assert!(!h.actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    h.retry(1500.0, 2000.0);
    assert_eq!(
        h.http(if private {
            "/bullet-private"
        } else {
            "/bullet-public"
        })
        .method,
        Method::Post
    );
}
#[test]
fn public_prewelcome_pong_cannot_cancel_welcome_watchdog() {
    assert_prewelcome_pong_preserves_recovery(false);
}
#[test]
fn private_prewelcome_pong_cannot_cancel_welcome_watchdog() {
    assert_prewelcome_pong_preserves_recovery(true);
}
#[test]
fn valid_pong_schedules_one_ping_and_unsolicited_pong_cannot_postpone_it() {
    for private in [false, true] {
        let mut h = H::new(private);
        let socket = if private { h.private } else { h.public };
        h.actions.clear();
        h.event(Event::WsMessage {
            socket,
            text: r#"{"type":"pong"}"#,
        });
        let timer = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now + 5000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        h.actions.clear();
        h.now += 1000.0;
        h.event(Event::WsMessage {
            socket,
            text: r#"{"type":"pong"}"#,
        });
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::ClearTimer { timer: t } if *t == timer))
        );
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
        h.now += 17000.0;
        h.event(Event::TimerFired { timer });
        assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text.contains("\"type\":\"ping\""))));
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 10000.0))
        );
    }
}
#[test]
fn ambiguous_writes_are_never_retried() {
    let mut h = H::new(true);
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![place(None)],
    });
    let r = h.http("/hf/orders");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "reset after accept",
    });
    assert!(h.done(1).is_err());
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
}
#[test]
fn private_fill_emits_once_and_canceled_done_does_not_reopen() {
    let mut h = H::new(true);
    let match_row = json!({"orderId":"order-1","symbol":"BTC-USDT","type":"match","orderType":"limit","status":"match","side":"buy","price":"90","size":"2","filledSize":"0.7","remainSize":"1.3","matchSize":"0.2","matchPrice":"89.5","tradeId":"trade-1","ts":1700000000001000000u64});
    h.ws(true, "/spotMarket/tradeOrdersV2", match_row.clone());
    h.ws(true, "/spotMarket/tradeOrdersV2", match_row);
    assert_eq!(h.e.store().orders()[0].filled, 0.7);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"canceled","status":"done","ts":1700000000002000000u64}));
    assert!(h.e.store().orders().is_empty());
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"open","status":"open","ts":1700000000003000000u64}));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn numbered_open_pages_use_fifty_and_failed_page_keeps_good_store() {
    let mut h = H::new(true);
    h.timer(5000.0);
    h.reply(
        "/active/symbols",
        r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
    );
    let r = h.http("/active/page");
    assert!(r.url.ends_with("symbol=BTC-USDT&pageNum=1&pageSize=50"));
    h.respond(
        &r,
        200,
        &format!(r#"{{"code":"200000","data":{{"totalPage":2,"items":[{ORDER}]}}}}"#),
    );
    let r = h.http("/active/page");
    assert!(r.url.contains("pageNum=2&pageSize=50"));
    h.respond(&r, 500, r#"{"code":"500000","msg":"failed"}"#);
    assert_eq!(h.e.store().orders().len(), 1);
}
#[test]
fn terminal_event_fences_inflight_open_order_snapshot() {
    let mut h = H::new(true);
    h.timer(5000.0);
    h.reply(
        "/active/symbols",
        r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
    );
    let r = h.http("/active/page");
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"filled","status":"done","ts":1700000006000000000u64}));
    h.respond(
        &r,
        200,
        &format!(r#"{{"code":"200000","data":{{"totalPage":1,"items":[{ORDER}]}}}}"#),
    );
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn active_symbol_failure_preserves_last_good_orders() {
    let mut h = H::new(true);
    h.timer(5000.0);
    h.reply("/active/symbols", r#"{"code":"400100","msg":"denied"}"#);
    assert_eq!(h.e.store().orders().len(), 1);
}
#[test]
fn lookup_is_open_only_nondefinitive_and_does_not_mutate_store() {
    let mut h = H::new(true);
    h.command(Command::Query {
        command: 1,
        query: Query::OrderByClientId {
            symbol: "ETH-USDT".into(),
            client_order_id: "caller-1".into(),
        },
    });
    h.reply(
        "/active/page",
        r#"{"code":"200000","data":{"totalPage":1,"items":[]}}"#,
    );
    assert_eq!(
        h.done(1).unwrap(),
        json!({"found":false,"definitive":false})
    );
    assert_eq!(h.e.store().orders().len(), 1);
    h.command(Command::Query {
        command: 2,
        query: Query::OrderByClientId {
            symbol: "BTC-USDT".into(),
            client_order_id: "caller-1".into(),
        },
    });
    assert_eq!(h.done(2).unwrap()["order"]["id"], "order-1");
}
#[test]
fn historical_fills_match_hf_base_units_and_ms_timestamp() {
    let mut h = H::new(true);
    h.command(Command::Query {
        command: 1,
        query: Query::Fills {
            symbol: Some("BTC-USDT".into()),
            order_id: Some("order-1".into()),
            from_ms: Some(1700000000000.0),
            to_ms: Some(1700000060000.0),
            limit: Some(500),
        },
    });
    let r = h.http("/hf/fills");
    assert!(r.url.ends_with(
        "symbol=BTC-USDT&orderId=order-1&limit=100&startAt=1700000000000&endAt=1700000060000"
    ));
    h.respond(&r,200,r#"{"code":"200000","data":{"items":[{"symbol":"BTC-USDT","orderId":"order-1","side":"sell","price":"100","size":"0.2","fee":"0.01","feeCurrency":"USDT","createdAt":1700000001000}]}}"#);
    assert_eq!(
        h.done(1).unwrap(),
        json!([{"timestamp":1700000001000.0,"symbol":"BTC-USDT","orderId":"order-1","side":"sell","price":100.0,"qty":0.2,"commission":0.01,"commissionAsset":"USDT"}])
    );
}
#[test]
fn fixed_settings_changes_never_send_derivative_writes() {
    let mut h = H::new(true);
    h.actions.clear();
    for c in [
        Command::SetLeverage {
            command: 1,
            symbol: "BTC-USDT".into(),
            leverage: 99.0,
        },
        Command::SetMarginType {
            command: 2,
            symbol: "BTC-USDT".into(),
            isolated: false,
        },
        Command::SetPositionMode {
            command: 3,
            hedged: true,
        },
    ] {
        h.command(c);
    }
    for id in 1..=3 {
        assert!(h.done(id).is_ok());
    }
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(h.e.store().setting("BTC-USDT").unwrap().leverage, 1.0);
    assert!(!h.e.store().hedged());
}
#[test]
fn watchdog_closes_silent_socket_and_gets_fresh_bullet_token() {
    let mut h = H::new(false);
    h.timer(10000.0);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==h.public))
    );
    h.retry(750.0, 1000.0);
    let r = h.http("/bullet-public");
    assert_eq!(r.method, Method::Post);
}
#[test]
fn late_distinct_executions_survive_newer_order_and_terminal_fences_once_each() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"update","status":"open","price":"91","ts":1700000000003000000u64}));
    let matched = json!({"orderId":"order-1","symbol":"BTC-USDT","side":"buy","type":"match","status":"match","orderType":"limit","size":"2","filledSize":"0.7","remainSize":"1.3","matchSize":"0.2","matchPrice":"89.5","tradeId":"late-1","ts":1700000000001000000u64});
    h.ws(true, "/spotMarket/tradeOrdersV2", matched.clone());
    h.ws(true, "/spotMarket/tradeOrdersV2", matched.clone());
    assert_eq!(h.fill_rows(), vec![(89.5, 0.2)]);
    assert_eq!(h.e.store().orders()[0].price, 91.0);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"canceled","status":"done","ts":1700000000004000000u64}));
    let mut last = matched;
    last["tradeId"] = json!("late-2");
    h.ws(true, "/spotMarket/tradeOrdersV2", last.clone());
    h.ws(true, "/spotMarket/tradeOrdersV2", last);
    assert_eq!(h.fill_rows(), vec![(89.5, 0.2)]);
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn account_summary_preserves_main_sub_affiliate_and_uid_fallback() {
    for (data, expected) in [
        (
            json!({"userId":"main","subUserId":"sub","referrerUserId":"ref"}),
            json!({"userId":"main","subId":"sub","affiliateId":"ref"}),
        ),
        (
            json!({"uid":"fallback"}),
            json!({"userId":"fallback","subId":null,"affiliateId":null}),
        ),
    ] {
        let mut h = H::new(true);
        h.command(Command::FetchAccount { command: 1 });
        let r = h.http("/api/v2/user-info");
        h.respond(&r, 200, &json!({"code":"200000","data":data}).to_string());
        assert_eq!(h.done(1).unwrap(), expected);
    }
}
#[test]
fn caller_symbol_cancels_an_order_before_any_snapshot_has_seen_it() {
    let mut h = H::new(true);
    h.command(Command::CancelOrdersWithSymbols {
        command: 1,
        orders: vec![safe_cex_engine::model::CancelOrderRef {
            id: "uncached-order".into(),
            symbol: "ETH-USDT".into(),
        }],
    });
    let r = h.http("/hf/orders/uncached-order");
    assert_eq!(
        r.url,
        "https://api.kucoin.com/api/v1/hf/orders/uncached-order?symbol=ETH-USDT"
    );
    assert_eq!(r.method, Method::Delete);
    h.respond(
        &r,
        200,
        r#"{"code":"200000","data":{"orderId":"uncached-order"}}"#,
    );
    assert_eq!(h.done(1).unwrap(), json!([]));
}
#[test]
fn caller_symbol_native_amend_works_before_the_order_is_cached() {
    for (data, expected) in [
        (json!({}), "uncached-order"),
        (json!({"newOrderId":null}), "uncached-order"),
        (json!({"newOrderId":""}), "uncached-order"),
        (json!({"newOrderId":"amended"}), "amended"),
    ] {
        let mut h = H::new(true);
        h.command(Command::AmendOrderWithSymbol {
            command: 1,
            id: "uncached-order".into(),
            symbol: "ETH-USDT".into(),
            price: Some(12.345),
            amount: Some(2.345),
        });
        let r = h.http("/hf/orders/alter");
        assert_eq!(
            serde_json::from_str::<Value>(r.body.as_deref().unwrap()).unwrap(),
            json!({"symbol":"ETH-USDT","orderId":"uncached-order","newPrice":"12.35","newSize":"2.35"})
        );
        h.respond(&r, 200, &json!({"code":"200000","data":data}).to_string());
        assert_eq!(h.done(1).unwrap(), json!([expected]));
    }
}
#[test]
fn malformed_active_order_rows_never_commit_partial_or_empty_reconciliation() {
    for items in [
        json!([{}]),
        json!([serde_json::from_str::<Value>(ORDER).unwrap(),{"id":"bad","symbol":"BTC-USDT"}]),
        json!([{"id":"bad","symbol":"BTC-USDT","type":"limit","side":"buy"}]),
    ] {
        let mut h = H::new(true);
        h.timer(5000.0);
        h.reply(
            "/active/symbols",
            r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
        );
        h.reply(
            "/active/page",
            &json!({"code":"200000","data":{"totalPage":1,"items":items}}).to_string(),
        );
        assert_eq!(h.e.store().orders().len(), 1);
        assert_eq!(h.e.store().orders()[0].id, "order-1");
    }
}
#[test]
fn lookup_native_order_alias_matches_ts_and_failed_read_stays_nondefinitive() {
    let mut h = H::new(true);
    h.command(Command::Query {
        command: 1,
        query: Query::OrderByClientId {
            symbol: "BTC-USDT".into(),
            client_order_id: "order-1".into(),
        },
    });
    assert_eq!(h.done(1).unwrap()["found"], true);
    h.command(Command::Query {
        command: 2,
        query: Query::OrderByClientId {
            symbol: "BTC-USDT".into(),
            client_order_id: "missing".into(),
        },
    });
    let r = h.http("/active/page");
    h.respond(&r, 500, r#"{"code":"500000","msg":"failed"}"#);
    assert_eq!(
        h.done(2).unwrap(),
        json!({"found":false,"definitive":false})
    );
    assert_eq!(h.e.store().orders().len(), 1);
}
#[test]
fn invalid_active_order_financial_geometry_preserves_last_good_snapshot() {
    for field in [
        json!({"size":"-1"}),
        json!({"dealSize":"3"}),
        json!({"remainSize":"-1"}),
        json!({"remainSize":"3"}),
        json!({"price":"-1"}),
        json!({"price":null}),
    ] {
        let mut bad: Value = serde_json::from_str(ORDER).unwrap();
        for (k, v) in field.as_object().unwrap() {
            bad[k] = v.clone();
        }
        let mut h = H::new(true);
        h.timer(5000.0);
        h.reply(
            "/active/symbols",
            r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
        );
        h.reply(
            "/active/page",
            &json!({"code":"200000","data":{"totalPage":1,"items":[bad]}}).to_string(),
        );
        assert_eq!(h.e.store().orders()[0].amount, 2.0);
        assert_eq!(h.e.store().orders()[0].price, 90.0);
    }
}
#[test]
fn a_recovery_read_started_after_execution_keeps_newer_fill_and_price_floors() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","side":"buy","type":"match","status":"match","orderType":"limit","price":"91","size":"2","filledSize":"0.7","remainSize":"1.3","matchSize":"0.2","matchPrice":"89.5","tradeId":"new-fill","ts":1700000000001000000u64}));
    h.timer(250.0);
    h.reply(
        "/active/symbols",
        r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
    );
    h.reply(
        "/active/page",
        &format!(r#"{{"code":"200000","data":{{"totalPage":1,"items":[{ORDER}]}}}}"#),
    );
    let o = &h.e.store().orders()[0];
    assert_eq!((o.price, o.filled, o.remaining), (91.0, 0.7, 1.3));
}
#[test]
fn recovery_cannot_merge_a_native_size_smaller_than_the_known_fill_floor() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","side":"buy","type":"match","status":"match","orderType":"limit","price":"91","size":"2","filledSize":"0.7","remainSize":"1.3","matchSize":"0.2","matchPrice":"89.5","tradeId":"new-fill","ts":1700000000001000000u64}));
    h.timer(250.0);
    h.reply(
        "/active/symbols",
        r#"{"code":"200000","data":{"symbols":["BTC-USDT"]}}"#,
    );
    h.reply("/active/page",r#"{"code":"200000","data":{"totalPage":1,"items":[{"id":"order-1","symbol":"BTC-USDT","type":"limit","side":"buy","status":"open","price":"92","size":"0.5","dealSize":"0.2","remainSize":"0.3","lastUpdatedAt":1700000000002}]}}"#);
    let o = &h.e.store().orders()[0];
    assert_eq!(
        (o.amount, o.price, o.filled, o.remaining),
        (2.0, 91.0, 0.7, 1.3)
    );
}
#[test]
fn malformed_private_financial_rows_never_emit_invalid_fills_or_replace_good_orders() {
    for field in [
        json!({"matchPrice":"-10"}),
        json!({"matchPrice":"0"}),
        json!({"size":"-1"}),
        json!({"filledSize":"3"}),
        json!({"remainSize":"1.5"}),
        json!({"matchSize":"-1"}),
        json!({"side":"invalid"}),
        json!({"symbol":"ETH-USDT"}),
    ] {
        let mut event = json!({"orderId":"order-1","symbol":"BTC-USDT","side":"buy","type":"match","status":"match","orderType":"limit","price":"90","size":"2","filledSize":"0.7","remainSize":"1.3","matchSize":"0.2","matchPrice":"89.5","tradeId":"execution","ts":1700000000001000000u64});
        for (k, v) in field.as_object().unwrap() {
            event[k] = v.clone();
        }
        let mut h = H::new(true);
        h.ws(true, "/spotMarket/tradeOrdersV2", event);
        assert!(h.fill_rows().is_empty());
        let o = &h.e.store().orders()[0];
        assert_eq!(
            (o.amount, o.price, o.filled, o.remaining),
            (2.0, 90.0, 0.5, 1.5)
        );
    }
}
#[test]
fn private_partial_fields_preserve_known_amount_and_zero_size_terminal_is_valid() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"update","status":"open","remainSize":"1.4","ts":1700000000001000000u64}));
    let o = &h.e.store().orders()[0];
    assert_eq!((o.amount, o.filled, o.remaining), (2.0, 0.5, 1.4));
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"canceled","status":"done","size":"0","filledSize":"0","remainSize":"0","ts":1700000000002000000u64}));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn missing_null_empty_or_negative_trade_account_numbers_preserve_good_balance() {
    for row in [
        json!({"currency":"USDT","holds":"100","balance":"1000"}),
        json!({"currency":"USDT","available":null,"holds":"100","balance":"1000"}),
        json!({"currency":"USDT","available":"","holds":"100","balance":"1000"}),
        json!({"currency":"USDT","available":"-1","holds":"100","balance":"1000"}),
    ] {
        let mut h = H::new(true);
        h.timer(5000.0);
        h.reply(
            "/accounts?type=trade",
            &json!({"code":"200000","data":[row]}).to_string(),
        );
        let b = h.e.store().balance();
        assert_eq!((b.free, b.used, b.total), (1100.0, 200.0, 1300.0));
        assert_eq!(h.e.store().coin_balances().unwrap().len(), 2);
    }
}
#[test]
fn newer_private_rows_cannot_shrink_below_the_existing_execution_floor() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","side":"buy","type":"match","status":"match","orderType":"limit","price":"91","size":"2","filledSize":"0.7","remainSize":"1.3","matchSize":"0.2","matchPrice":"89.5","tradeId":"new-fill","ts":1700000000001000000u64}));
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"update","status":"open","price":"92","size":"0.5","filledSize":"0.2","remainSize":"0.3","ts":1700000000002000000u64}));
    let o = &h.e.store().orders()[0];
    assert_eq!(
        (o.amount, o.price, o.filled, o.remaining),
        (2.0, 91.0, 0.7, 1.3)
    );
}
#[test]
fn private_disconnect_fences_account_reads_started_before_the_socket_gap() {
    let mut h = H::new(true);
    h.timer(5000.0);
    let orders = h.http("/active/symbols");
    let balance = h.http("/accounts?type=trade");
    h.event(Event::WsClosed {
        socket: h.private,
        code: 1006,
    });
    h.respond(&orders, 200, r#"{"code":"200000","data":{"symbols":[]}}"#);
    h.respond(&balance, 200, r#"{"code":"200000","data":[]}"#);
    assert_eq!(h.e.store().orders().len(), 1);
    assert_eq!(h.e.store().balance().total, 1300.0);
}
#[test]
fn contradictory_new_matches_do_not_emit_or_consume_the_execution_identity() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"update","status":"open","size":"2","filledSize":"0.7","remainSize":"1.3","ts":1700000000001000000u64}));
    let invalid = json!({"orderId":"order-1","symbol":"BTC-USDT","type":"match","orderType":"limit","status":"match","side":"buy","size":"0.5","filledSize":"0.2","remainSize":"0.3","price":"90","matchSize":"0.2","matchPrice":"89.5","tradeId":"retry-after-invalid","ts":1700000000002000000u64});
    h.ws(true, "/spotMarket/tradeOrdersV2", invalid.clone());
    assert!(h.fill_rows().is_empty());
    let o = &h.e.store().orders()[0];
    assert_eq!((o.amount, o.filled, o.remaining), (2.0, 0.7, 1.3));
    let mut valid = invalid;
    valid["size"] = json!("2");
    valid["filledSize"] = json!("0.9");
    valid["remainSize"] = json!("1.1");
    valid["ts"] = json!(1700000000003000000u64);
    h.ws(true, "/spotMarket/tradeOrdersV2", valid.clone());
    h.ws(true, "/spotMarket/tradeOrdersV2", valid);
    assert_eq!(h.fill_rows(), vec![(89.5, 0.2)]);
}
#[test]
fn explicit_native_execution_bounds_reject_without_consuming_identity() {
    for (total, cumulative) in [
        (Some("size"), Some("filledSize")),
        (Some("originSize"), Some("dealSize")),
        (Some("orderSize"), Some("filledSize")),
        (Some("size"), None),
        (None, Some("dealSize")),
    ] {
        let mut h = H::new(true);
        let mut event = json!({"orderId":"order-1","symbol":"BTC-USDT","type":"match","orderType":"limit","status":"match","side":"buy","price":"90","matchSize":"3","matchPrice":"89.5","tradeId":"retry-bounds","ts":1700000000001000000u64});
        if let Some(field) = total {
            event[field] = json!("2");
        }
        if let Some(field) = cumulative {
            event[field] = json!("0.7");
        }
        h.ws(true, "/spotMarket/tradeOrdersV2", event.clone());
        assert!(h.fill_rows().is_empty());
        event["matchSize"] = json!("0.2");
        h.ws(true, "/spotMarket/tradeOrdersV2", event.clone());
        h.ws(true, "/spotMarket/tradeOrdersV2", event);
        assert_eq!(h.fill_rows(), vec![(89.5, 0.2)]);
    }
}
#[test]
fn optional_native_totals_do_not_borrow_cached_fill_bounds_for_unique_executions() {
    for scenario in [
        "stale-total",
        "stale-partial",
        "current-partial",
        "terminal-partial",
    ] {
        let mut h = H::new(true);
        h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"update","status":"open","size":"2","filledSize":"0.7","remainSize":"1.3","ts":1700000000003000000u64}));
        if scenario == "terminal-partial" {
            h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"canceled","status":"done","ts":1700000000004000000u64}));
        }
        let mut event = json!({"orderId":"order-1","symbol":"BTC-USDT","type":"match","orderType":"limit","status":"match","side":"buy","price":"90","matchSize":"0.8","matchPrice":"89.5","tradeId":"unique-partial","ts":1700000000001000000u64});
        let amount = if scenario == "stale-total" {
            event["size"] = json!("0.5");
            event["matchSize"] = json!("0.2");
            0.2
        } else {
            0.8
        };
        if scenario == "current-partial" {
            event["ts"] = json!(1700000000005000000u64);
        }
        h.ws(true, "/spotMarket/tradeOrdersV2", event.clone());
        h.ws(true, "/spotMarket/tradeOrdersV2", event);
        assert_eq!(h.fill_rows(), vec![(89.5, amount)]);
        if scenario == "terminal-partial" {
            assert!(h.e.store().orders().is_empty());
        } else {
            let o = &h.e.store().orders()[0];
            assert_eq!((o.amount, o.filled, o.remaining), (2.0, 0.7, 1.3));
        }
    }
}
#[test]
fn explicit_execution_bounds_tolerate_decimal_roundoff() {
    let mut h = H::new(true);
    let amount = 0.1 + 0.2;
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"decimal-order","symbol":"BTC-USDT","type":"match","orderType":"limit","status":"match","side":"buy","size":"0.3","filledSize":"0.3","remainSize":"0","price":"90","matchSize":amount,"matchPrice":"89.5","tradeId":"decimal","ts":1700000000001000000u64}));
    assert_eq!(h.fill_rows(), vec![(89.5, amount)]);
}
#[test]
fn zero_size_terminal_without_cumulative_removes_a_partially_filled_order() {
    let mut h = H::new(true);
    h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"canceled","status":"done","size":"0","remainSize":"0","ts":1700000000001000000u64}));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn current_partial_matches_reject_cached_geometry_before_consuming_identity() {
    for field in ["filledSize", "remainSize"] {
        let mut h = H::new(true);
        h.ws(true,"/spotMarket/tradeOrdersV2",json!({"orderId":"order-1","symbol":"BTC-USDT","type":"update","status":"open","size":"2","filledSize":"0.7","remainSize":"1.3","ts":1700000000001000000u64}));
        let mut event = json!({"orderId":"order-1","symbol":"BTC-USDT","type":"match","orderType":"limit","status":"match","side":"buy","price":"90","matchSize":"0.2","matchPrice":"89.5","tradeId":"retry-cached-geometry","ts":1700000000002000000u64});
        event[field] = json!("3");
        h.ws(true, "/spotMarket/tradeOrdersV2", event.clone());
        assert!(h.fill_rows().is_empty());
        let o = &h.e.store().orders()[0];
        assert_eq!((o.amount, o.filled, o.remaining), (2.0, 0.7, 1.3));
        event[field] = json!(if field == "filledSize" { "0.9" } else { "1.1" });
        h.ws(true, "/spotMarket/tradeOrdersV2", event.clone());
        h.ws(true, "/spotMarket/tradeOrdersV2", event);
        assert_eq!(h.fill_rows(), vec![(89.5, 0.2)]);
    }
}

fn secondary_credentials(passphrase: Option<&str>) -> safe_cex_engine::ReadCredentials {
    safe_cex_engine::ReadCredentials {
        key: "read-only-key".into(),
        secret: "read-only-secret".into(),
        passphrase: passphrase.map(str::to_owned),
    }
}
fn assert_identity(request: &HttpRequest, secondary: bool, passphrase: &str, now: f64) {
    let credentials = safe_cex_venue_kucoin::sign::Credentials {
        key: if secondary {
            "read-only-key"
        } else {
            "replay-key"
        }
        .into(),
        secret: if secondary {
            "read-only-secret"
        } else {
            "replay-secret"
        }
        .into(),
        passphrase: passphrase.into(),
        broker: None,
    };
    assert_eq!(
        request.headers,
        safe_cex_venue_kucoin::sign::headers(
            &credentials,
            request.method,
            request.url.strip_prefix("https://api.kucoin.com").unwrap(),
            request.body.as_deref().unwrap_or(""),
            now,
        ),
    );
}
#[test]
fn secondary_reads_preserve_primary_mutations_and_unsigned_public_gets() {
    for read in [
        None,
        Some(secondary_credentials(Some("read-only-pass"))),
        Some(secondary_credentials(None)),
    ] {
        let secondary = read.is_some();
        let passphrase = read
            .as_ref()
            .and_then(|r| r.passphrase.as_deref())
            .unwrap_or("replay-passphrase");
        let mut h = H::with_read_credentials(true, read.clone());
        for request in &h.requests {
            if request.method == Method::Get
                && !request.url.contains("/api/v2/symbols")
                && !request.url.contains("/market/allTickers")
            {
                assert_identity(request, secondary, passphrase, h.now);
            } else if request.url.contains("/bullet-private") {
                assert_identity(request, false, "replay-passphrase", h.now);
            } else {
                assert!(
                    !request
                        .headers
                        .iter()
                        .any(|(name, _)| name.starts_with("KC-API-"))
                );
            }
            // Host GET routing selects the read client even for unsigned public endpoints.
            assert_eq!(request.route, None);
        }
        h.command(Command::FetchAccount { command: 1 });
        assert_identity(&h.http("/api/v2/user-info"), secondary, passphrase, h.now);
        h.command(Command::Query {
            command: 2,
            query: Query::Fills {
                symbol: Some("BTC-USDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(10),
            },
        });
        assert_identity(&h.http("/hf/fills"), secondary, passphrase, h.now);
        h.command(Command::FetchCandles {
            command: 3,
            symbol: "BTC-USDT".into(),
            interval: "1m".into(),
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        });
        let candles = h.http("/market/candles");
        assert_eq!(candles.method, Method::Get);
        assert_eq!(candles.route, None);
        assert!(
            !candles
                .headers
                .iter()
                .any(|(name, _)| name.starts_with("KC-API-"))
        );
        h.command(Command::PlaceOrders {
            command: 4,
            orders: vec![place(Some("primary-write"))],
        });
        assert_identity(&h.http("/hf/orders"), false, "replay-passphrase", h.now);
        h.command(Command::AmendOrderWithSymbol {
            command: 5,
            id: "uncached".into(),
            symbol: "BTC-USDT".into(),
            price: Some(99.0),
            amount: None,
        });
        assert_identity(&h.http("/orders/alter"), false, "replay-passphrase", h.now);
        h.command(Command::CancelOrdersWithSymbols {
            command: 6,
            orders: vec![safe_cex_engine::model::CancelOrderRef {
                id: "uncached".into(),
                symbol: "BTC-USDT".into(),
            }],
        });
        let cancel = h.http("/hf/orders/uncached");
        assert_eq!(cancel.method, Method::Delete);
        assert_identity(&cancel, false, "replay-passphrase", h.now);
    }
}
#[test]
fn invalid_secondary_identity_rejects_before_any_startup_io() {
    for invalid in ["key", "secret", "passphrase", "missing-passphrase"] {
        let mut read = secondary_credentials(Some("read-only-pass"));
        match invalid {
            "key" => read.key.clear(),
            "secret" => read.secret.clear(),
            "passphrase" => read.passphrase = Some(String::new()),
            "missing-passphrase" => read.passphrase = None,
            _ => unreachable!(),
        }
        let mut e = Engine::new(KucoinSpot::new(Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            passphrase: (invalid != "missing-passphrase").then(|| "replay-passphrase".into()),
            read_credentials: Some(read),
            ..Options::default()
        }));
        e.start(1700000000000.0);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("Secondary read credentials"))));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::WsConnect { .. } | Action::WsConnectWithHeaders { .. }
        )));
    }
}
#[test]
fn secondary_lookup_retains_cache_identity_and_never_mutates_store() {
    let mut h = H::with_read_credentials(true, Some(secondary_credentials(Some("read-only-pass"))));
    let before = h.e.store().orders().to_vec();
    let requests = h.requests.len();
    h.command(Command::Query {
        command: 1,
        query: Query::OrderByClientId {
            symbol: "BTC-USDT".into(),
            client_order_id: "caller-1".into(),
        },
    });
    assert_eq!(h.done(1).unwrap()["order"]["id"], "order-1");
    assert_eq!(h.requests.len(), requests);
    h.command(Command::Query {
        command: 2,
        query: Query::OrderByClientId {
            symbol: "ETH-USDT".into(),
            client_order_id: "caller-1".into(),
        },
    });
    let request = h.http("/active/page");
    assert_identity(&request, true, "read-only-pass", h.now);
    h.respond(
        &request,
        200,
        r#"{"code":"200000","data":{"totalPage":1,"items":[]}}"#,
    );
    assert_eq!(
        h.done(2).unwrap(),
        json!({"found":false,"definitive":false})
    );
    assert_eq!(h.e.store().orders(), before);
}
#[test]
fn secondary_config_never_authenticates_public_only_account_commands() {
    let mut h =
        H::with_read_credentials(false, Some(secondary_credentials(Some("read-only-pass"))));
    let before = h.requests.len();
    h.command(Command::FetchAccount { command: 1 });
    assert!(h.done(1).unwrap_err().contains("requires credentials"));
    assert_eq!(h.requests.len(), before);
    assert!(h.requests.iter().all(|r| {
        !r.headers
            .iter()
            .any(|(name, _)| name.starts_with("KC-API-"))
    }));
}

fn spot_bullet_body(token: &str, interval: f64, timeout: f64) -> String {
    json!({"code":"200000","data":{"token":token,"instanceServers":[{"endpoint":"wss://spot.example.test/recovered","pingInterval":interval,"pingTimeout":timeout}]}}).to_string()
}
fn spot_private_path(private: bool) -> &'static str {
    if private {
        "/bullet-private"
    } else {
        "/bullet-public"
    }
}
fn spot_latest_socket(h: &H, private: bool) -> SocketId {
    h.actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(role),
                ..
            } if *role
                == if private {
                    SocketChannel::Private
                } else {
                    SocketChannel::Public
                } =>
            {
                Some(*socket)
            }
            _ => None,
        })
        .unwrap()
}
fn spot_latest_watch(h: &H) -> safe_cex_engine::TimerId {
    h.actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 5000.0 => Some(*timer),
            _ => None,
        })
        .expect("connection or acquisition watchdog")
}
fn spot_retire_and_acquire(h: &mut H, private: bool) -> HttpRequest {
    let socket = if private { h.private } else { h.public };
    h.actions.clear();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.retry(750.0, 1000.0);
    h.http(spot_private_path(private))
}

#[test]
fn spot_supervisors_repost_role_tokens_with_increasing_backoff_and_restore_interests() {
    for private in [false, true] {
        let mut h = H::new(private);
        if !private {
            h.command(Command::SubscribeOrderBook {
                symbol: "BTC-USDT".into(),
            });
            h.command(Command::SubscribeCandles {
                symbol: "BTC-USDT".into(),
                interval: "1m".into(),
            });
        }
        let old = if private { h.private } else { h.public };
        let request = spot_retire_and_acquire(&mut h, private);
        assert_eq!(request.method, Method::Post);
        assert_eq!(
            request.headers.iter().any(|(key, _)| *key == "KC-API-KEY"),
            private
        );
        h.actions.clear();
        h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        h.event(Event::WsMessage {
            socket: old,
            text: r#"{"type":"welcome"}"#,
        });
        assert!(h.actions.is_empty());
        h.respond(&request, 503, r#"{"code":"503000","msg":"unavailable"}"#);
        assert!(
            !h.actions.iter().any(|a| matches!(a, Action::Http(_))),
            "token failure must back off"
        );
        h.retry(1500.0, 2000.0);
        let request = h.http(spot_private_path(private));
        h.respond(
            &request,
            200,
            &spot_bullet_body("fresh-token", 18000.0, 10000.0),
        );
        let socket = spot_latest_socket(&h, private);
        assert_ne!(old, socket);
        assert!(h.actions.iter().any(
            |a| matches!(a, Action::WsConnect { url, .. } if url.contains("token=fresh-token"))
        ));
        h.actions.clear();
        h.event(Event::WsOpened { socket });
        h.event(Event::WsMessage {
            socket,
            text: r#"{"type":"welcome"}"#,
        });
        for topic in if private {
            vec!["/spotMarket/tradeOrdersV2", "/account/balance"]
        } else {
            vec![
                "/market/ticker:all",
                "/spotMarket/level2Depth50:BTC-USDT",
                "/market/candles:BTC-USDT_1min",
            ]
        } {
            assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text.contains(topic))), "restore {topic}");
        }
        h.now += 1.0;
        h.event(Event::WsClosed { socket, code: 1006 });
        h.retry(3000.0, 4000.0);
        assert_eq!(h.http(spot_private_path(private)).method, Method::Post);
    }
}

#[test]
fn spot_hung_bullet_acquisition_retires_late_success_and_failure_without_touching_new_request() {
    for private in [false, true] {
        for failed in [false, true] {
            let mut h = H::new(private);
            let old = spot_retire_and_acquire(&mut h, private);
            let watch = spot_latest_watch(&h);
            h.now += 10_000.0;
            h.event(Event::TimerFired { timer: watch });
            h.retry(1500.0, 2000.0);
            let current = h.http(spot_private_path(private));
            h.actions.clear();
            if failed {
                h.event(Event::HttpFailed {
                    request: old.id,
                    error: "late reset",
                });
            } else {
                h.respond(
                    &old,
                    200,
                    &spot_bullet_body("retired-token", 18000.0, 10000.0),
                );
            }
            assert!(
                h.actions.is_empty(),
                "retired token outcome cannot affect current acquisition"
            );
            h.respond(
                &current,
                200,
                &spot_bullet_body("current-token", 18000.0, 10000.0),
            );
            assert_eq!(
                h.actions
                    .iter()
                    .filter(|a| matches!(a, Action::WsConnect { .. }))
                    .count(),
                1
            );
            assert!(h.actions.iter().any(|a| matches!(a, Action::WsConnect { url, .. } if url.contains("token=current-token"))));
        }
    }
}

#[test]
fn spot_never_opened_connections_and_early_messages_cannot_escape_connect_timeout() {
    for private in [false, true] {
        let mut h = H::new(private);
        let request = spot_retire_and_acquire(&mut h, private);
        h.respond(&request, 200, &spot_bullet_body("fresh", 18000.0, 10000.0));
        let socket = spot_latest_socket(&h, private);
        let watch = spot_latest_watch(&h);
        h.actions.clear();
        for text in [r#"{"type":"welcome"}"#, r#"{"type":"pong"}"#] {
            h.event(Event::WsMessage { socket, text });
        }
        assert!(
            h.actions.is_empty(),
            "host must open before protocol messages establish readiness"
        );
        h.now += 10000.0;
        h.event(Event::TimerFired { timer: watch });
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
        );
        h.retry(1500.0, 2000.0);
        assert_eq!(h.http(spot_private_path(private)).method, Method::Post);
    }
}

#[test]
fn spot_negotiated_welcome_deadline_ignores_pongs_and_cannot_send_unauthenticated_ping() {
    for private in [false, true] {
        let mut h = H::new(private);
        let request = spot_retire_and_acquire(&mut h, private);
        h.respond(
            &request,
            200,
            &spot_bullet_body("short-welcome", 2000.0, 1500.0),
        );
        let socket = spot_latest_socket(&h, private);
        h.actions.clear();
        h.event(Event::WsOpened { socket });
        let watch = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1500.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        h.now += 1000.0;
        h.event(Event::WsMessage {
            socket,
            text: r#"{"type":"pong"}"#,
        });
        assert!(!h.actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
        h.now += 500.0;
        h.event(Event::TimerFired { timer: watch });
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
        );
    }
}

#[test]
fn spot_account_or_market_data_cannot_satisfy_mandatory_pong() {
    for private in [false, true] {
        let mut h = H::new(private);
        let socket = if private { h.private } else { h.public };
        let pong = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        h.now += 9999.0;
        if private {
            h.ws(
                true,
                "/account/balance",
                json!({"currency":"USDT","available":"900","hold":"100","total":"1000"}),
            );
        } else {
            h.ws(false, "/market/ticker:BTC-USDT", json!({"price":"101"}));
        }
        h.now += 1.0;
        h.event(Event::TimerFired { timer: pong });
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
        );
        h.retry(750.0, 1000.0);
        assert_eq!(h.http(spot_private_path(private)).method, Method::Post);
    }
}

#[test]
fn spot_invalid_negotiated_heartbeat_intervals_reject_before_connecting() {
    for private in [false, true] {
        for (interval, timeout) in [
            (0.0, 10000.0),
            (18000.0, 0.0),
            (-1.0, 10000.0),
            (18000.0, 999.0),
        ] {
            let mut h = H::new(private);
            let request = spot_retire_and_acquire(&mut h, private);
            h.actions.clear();
            h.respond(
                &request,
                200,
                &spot_bullet_body("invalid-timing", interval, timeout),
            );
            assert!(
                !h.actions
                    .iter()
                    .any(|a| matches!(a, Action::WsConnect { .. }))
            );
            h.retry(1500.0, 2000.0);
            assert_eq!(h.http(spot_private_path(private)).method, Method::Post);
        }
    }
}
