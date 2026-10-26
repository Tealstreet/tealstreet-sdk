//! Engine-managed socket lifecycle for both AsterDEX feeds: frames are admitted
//! only on the open, current, unexpired epoch, and each open runs its resync once.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
mod support;
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}]}"#;
const DEPTH: &str =
    r#"{"stream":"btcusdt@depth20@100ms","data":{"u":10,"b":[["100","1"]],"a":[["101","1"]]}}"#;
const LATER_DEPTH: &str =
    r#"{"stream":"btcusdt@depth20@100ms","data":{"u":11,"b":[["90","1"]],"a":[["91","1"]]}}"#;
const LIST_PUBLIC: &str = r#"{"id":-1,"method":"LIST_SUBSCRIPTIONS"}"#;
const LIST_PRIVATE: &str = r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#;

struct H {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<(SocketId, String)>,
    sent: Vec<(SocketId, String)>,
    timers: Vec<(TimerId, f64)>,
    closed: Vec<SocketId>,
}

impl H {
    fn start(options: Options) -> Self {
        let mut h = Self {
            engine: support::engine(options),
            now: 1_700_000_000_000.0,
            requests: vec![],
            sockets: vec![],
            sent: vec![],
            timers: vec![],
            closed: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        if h.requests.iter().any(|r| r.url.ends_with("/time")) {
            h.reply("/time", r#"{"serverTime":1700000000000}"#);
        }
        h.reply("/exchangeInfo", MARKETS);
        h.reply("/leverageBracket", "[]");
        h.reply(
            "/ticker/24hr",
            r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#,
        );
        h.reply("/openInterest", "[]");
        h
    }

    /// Public data only, with the public socket connecting but not yet open.
    fn public() -> Self {
        Self::start(Options {
            public_data_only: true,
            ..Options::default()
        })
    }

    /// An account whose private socket is connecting but not yet open.
    fn account() -> Self {
        let mut h = Self::start(Options {
            key: Some("documented-key".into()),
            secret: Some("synthetic-secret".into()),
            ..Default::default()
        });
        h.reply("/account", r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"0","totalOpenOrderInitialMargin":"0","totalUnrealizedProfit":"0"}"#);
        h.reply("/positionRisk", "[]");
        h.reply("/openOrders", "[]");
        h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
        h.reply("/listenKey", r#"{"listenKey":"private-contract"}"#);
        h
    }

    fn public_socket(&self) -> SocketId {
        self.sockets
            .iter()
            .rev()
            .find(|(_, url)| url.ends_with("/stream"))
            .unwrap()
            .0
    }
    fn private_socket(&self) -> SocketId {
        self.sockets
            .iter()
            .rev()
            .find(|(_, url)| url.contains("/ws/"))
            .unwrap()
            .0
    }
    fn sent_on(&self, socket: SocketId, text: &str) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t.contains(text))
            .count()
    }
    fn count(&self, suffix: &str) -> usize {
        self.requests
            .iter()
            .filter(|r| r.url.split('?').next().unwrap().ends_with(suffix))
            .count()
    }

    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn reply(&mut self, suffix: &str, body: &str) {
        self.reply_status(suffix, 200, body);
    }
    fn reply_status(&mut self, suffix: &str, status: u16, body: &str) {
        let position = self
            .requests
            .iter()
            .position(|r| r.url.split('?').next().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix}"));
        let request = self.requests.remove(position);
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }
    fn subscribe_book(&mut self) {
        self.event(Event::Command(
            safe_cex_engine::Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            },
        ));
    }
    /// Fires every timer due by `now + ms`, in order.
    fn advance(&mut self, ms: f64) {
        let until = self.now + ms;
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first() else {
                break;
            };
            if at > until {
                break;
            }
            self.timers.remove(0);
            self.now = at;
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }
    fn order_frame(&self, id: u64) -> String {
        serde_json::json!({"e":"ORDER_TRADE_UPDATE","E":self.now,"o":{"i":id,"s":"BTCUSDT","S":"BUY","ot":"LIMIT","X":"NEW","p":"100","q":"2","z":"0","l":"0","L":"0","ap":"0","t":0,"f":"GTC","c":"native-only"}}).to_string()
    }
}

#[test]
fn public_duplicate_open_does_not_resubscribe() {
    let mut h = H::public();
    let socket = h.public_socket();
    h.event(Event::WsOpened { socket });
    h.subscribe_book();
    let (lists, subscribes) = (
        h.sent_on(socket, LIST_PUBLIC),
        h.sent_on(socket, "\"SUBSCRIBE\""),
    );
    assert_eq!(lists, 1);
    h.event(Event::WsOpened { socket });
    assert_eq!(h.sent_on(socket, LIST_PUBLIC), lists);
    assert_eq!(h.sent_on(socket, "\"SUBSCRIBE\""), subscribes);
}

#[test]
fn public_frame_before_open_is_not_published() {
    let mut h = H::public();
    h.subscribe_book();
    let socket = h.public_socket();
    h.message(socket, DEPTH);
    assert!(h.engine.book("BTCUSDT").is_none());
    h.event(Event::WsOpened { socket });
    h.message(socket, DEPTH);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(100.0));
}

#[test]
fn public_frame_at_idle_equality_is_not_published() {
    let mut h = H::public();
    let socket = h.public_socket();
    h.event(Event::WsOpened { socket });
    h.subscribe_book();
    h.message(socket, DEPTH);
    // The idle deadline is due but its timer has not been delivered yet.
    h.now += 20_000.0;
    h.message(socket, LATER_DEPTH);
    assert_ne!(
        h.engine.book("BTCUSDT").and_then(|book| book.best_bid()),
        Some(90.0)
    );
}

#[test]
fn private_duplicate_open_does_not_refresh_twice() {
    let mut h = H::account();
    let socket = h.private_socket();
    h.event(Event::WsOpened { socket });
    assert_eq!(h.sent_on(socket, LIST_PRIVATE), 1);
    // Answered, so an in-flight guard cannot mask a second refresh.
    h.reply("/openOrders", "[]");
    h.event(Event::WsOpened { socket });
    assert_eq!(h.sent_on(socket, LIST_PRIVATE), 1);
    assert_eq!(h.count("/openOrders"), 0);
}

#[test]
fn private_frame_before_open_is_not_reduced() {
    let mut h = H::account();
    let socket = h.private_socket();
    let frame = h.order_frame(777);
    h.message(socket, &frame);
    assert!(h.engine.store().order("777").is_none());
    h.event(Event::WsOpened { socket });
    h.reply("/openOrders", "[]");
    let frame = h.order_frame(778);
    h.message(socket, &frame);
    assert!(
        h.engine.store().order("778").is_some(),
        "an open epoch reduces"
    );
}

#[test]
fn private_frame_at_idle_equality_is_not_reduced() {
    let mut h = H::account();
    let socket = h.private_socket();
    h.event(Event::WsOpened { socket });
    h.reply("/openOrders", "[]");
    let frame = h.order_frame(776);
    h.message(socket, &frame);
    assert!(
        h.engine.store().order("776").is_some(),
        "a live epoch reduces"
    );
    h.now += 20_000.0;
    let frame = h.order_frame(777);
    h.message(socket, &frame);
    assert!(h.engine.store().order("777").is_none());
}

#[test]
fn late_pong_cannot_revive_expired_private_epoch() {
    let mut h = H::account();
    let socket = h.private_socket();
    h.event(Event::WsOpened { socket });
    h.reply("/openOrders", "[]");
    h.now += 20_000.0;
    h.message(socket, r#"{"id":42,"result":[]}"#);
    h.advance(0.0);
    assert!(h.closed.contains(&socket));
    let posts = h.count("/listenKey");
    h.advance(1_000.0);
    assert!(h.count("/listenKey") > posts, "a reconnect follows");
}

// Controls: green before and after adoption.

#[test]
fn opening_order_is_list_then_one_subscribe_and_ids_increase_across_reconnects() {
    let mut h = H::public();
    h.subscribe_book();
    let socket = h.public_socket();
    h.event(Event::WsOpened { socket });
    let opening: Vec<_> = h.sent.iter().filter(|(s, _)| *s == socket).collect();
    assert_eq!(opening[0].1, LIST_PUBLIC);
    let subscribes: Vec<_> = opening
        .iter()
        .filter(|(_, t)| t.contains("\"SUBSCRIBE\""))
        .collect();
    assert_eq!(subscribes.len(), 1);
    for topic in ["!ticker@arr", "!markPrice@arr@1s", "btcusdt@depth20@100ms"] {
        assert!(subscribes[0].1.contains(topic), "{topic}");
    }
    let id = |text: &str| {
        serde_json::from_str::<serde_json::Value>(text).unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    let first = id(&subscribes[0].1);
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(500.0);
    let replacement = h.public_socket();
    assert_ne!(replacement, socket);
    h.event(Event::WsOpened {
        socket: replacement,
    });
    let again = h
        .sent
        .iter()
        .rev()
        .find(|(s, t)| *s == replacement && t.contains("\"SUBSCRIBE\""))
        .unwrap();
    assert!(again.1.contains("btcusdt@depth20@100ms"));
    assert!(id(&again.1) > first);
}

#[test]
fn private_open_lists_then_refreshes_orders_once() {
    let mut h = H::account();
    let socket = h.private_socket();
    h.event(Event::WsOpened { socket });
    assert_eq!(
        h.sent
            .iter()
            .filter(|(s, _)| *s == socket)
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>(),
        [LIST_PRIVATE]
    );
    assert_eq!(h.count("/openOrders"), 1);
}

#[test]
fn failed_listen_key_create_is_retried_until_the_private_stream_opens() {
    let mut h = H::start(Options {
        key: Some("documented-key".into()),
        secret: Some("synthetic-secret".into()),
        ..Default::default()
    });
    h.reply("/account", r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"0","totalOpenOrderInitialMargin":"0","totalUnrealizedProfit":"0"}"#);
    h.reply("/positionRisk", "[]");
    h.reply("/openOrders", "[]");
    h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
    h.reply_status("/listenKey", 500, r#"{"code":-1001,"msg":"internal"}"#);
    assert!(!h.sockets.iter().any(|(_, url)| url.contains("/ws/")));
    for _ in 0..4 {
        if h.count("/listenKey") > 0 {
            break;
        }
        h.advance(5_000.0);
    }
    h.reply("/listenKey", r#"{"listenKey":"second"}"#);
    let socket = h.private_socket();
    assert!(
        h.sockets
            .iter()
            .any(|(s, url)| *s == socket && url.ends_with("/ws/second"))
    );
    h.event(Event::WsOpened { socket });
    assert_eq!(h.sent_on(socket, LIST_PRIVATE), 1);
}

#[test]
fn listen_key_expiry_reconnects_on_a_new_key() {
    let mut h = H::account();
    let socket = h.private_socket();
    h.event(Event::WsOpened { socket });
    h.reply("/openOrders", "[]");
    h.message(socket, r#"{"e":"listenKeyExpired","E":1}"#);
    assert!(h.closed.contains(&socket));
    h.advance(1_000.0);
    h.reply("/listenKey", r#"{"listenKey":"renewed"}"#);
    let replacement = h.private_socket();
    assert_ne!(replacement, socket);
    assert!(
        h.sockets
            .iter()
            .any(|(s, url)| *s == replacement && url.ends_with("/ws/renewed"))
    );
}

#[test]
fn unanswered_listen_key_create_times_out_and_a_late_answer_is_fenced() {
    let mut h = H::start(Options {
        key: Some("documented-key".into()),
        secret: Some("synthetic-secret".into()),
        ..Default::default()
    });
    h.reply("/account", r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"0","totalOpenOrderInitialMargin":"0","totalUnrealizedProfit":"0"}"#);
    h.reply("/positionRisk", "[]");
    h.reply("/openOrders", "[]");
    h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
    assert_eq!(h.count("/listenKey"), 1);
    h.advance(10_000.0);
    h.advance(1_000.0);
    assert_eq!(h.count("/listenKey"), 2, "the deadline retried the create");
    h.reply("/listenKey", r#"{"listenKey":"late"}"#);
    assert!(!h.sockets.iter().any(|(_, url)| url.contains("/ws/")));
    h.reply("/listenKey", r#"{"listenKey":"current"}"#);
    assert!(
        h.sockets
            .iter()
            .any(|(_, url)| url.ends_with("/ws/current"))
    );
}

#[test]
fn listen_key_backoff_continues_across_a_socket_retirement() {
    let mut h = H::account();
    let socket = h.private_socket();
    h.event(Event::WsOpened { socket });
    h.reply("/openOrders", "[]");
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(1_000.0);
    let armed: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
    h.reply_status("/listenKey", 500, r#"{"code":-1001,"msg":"internal"}"#);
    let (_, at) = *h
        .timers
        .iter()
        .filter(|(id, _)| !armed.contains(id))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    assert!(
        (1_500.0..=2_000.0).contains(&(at - h.now)),
        "{}",
        at - h.now
    );
}
