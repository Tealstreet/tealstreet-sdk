//! Real Kraken codecs under valid shared proposal, frame and continuation pressure.
#[path = "spot_support/mod.rs"]
mod spot_support;
#[path = "support/mod.rs"]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context as HttpContext, Operation as HttpOperation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::socket_lifecycle::*;
use safe_cex_engine::*;
use safe_cex_venue_kraken::{Kraken, KrakenSpot};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Pressure {
    Proposals,
    Continuations,
    WarmContinuation,
    Frames,
}
#[derive(Default)]
struct Trace {
    public: Option<SocketId>,
    private: Option<SocketId>,
    feed: Option<FeedId>,
    desired: BTreeMap<String, Interest>,
    pressure: Option<Pressure>,
    refused: Vec<SocketError>,
    retired: usize,
    callbacks: usize,
    encoder_pressure: bool,
}
struct Observed<V: Venue> {
    native: V,
    trace: Rc<RefCell<Trace>>,
    pending: fn(HttpContext) -> V::Pending,
}
impl<V: Venue> Venue for Observed<V> {
    type Pending = V::Pending;
    fn capabilities(&self) -> Capabilities {
        self.native.capabilities()
    }
    fn guard_order_mutations(&self) -> bool {
        self.native.guard_order_mutations()
    }
    fn dispatch_nonce() -> bool {
        V::dispatch_nonce()
    }
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
        self.native.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, s: SocketId) {
        self.native.on_open(cx, s)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, s: SocketId, c: u16) {
        self.native.on_close(cx, s, c)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, s: SocketId, t: &str) {
        self.native.on_message(cx, s, t)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Self::Pending>, p: Self::Pending, r: Response<'_>) {
        self.native.on_response(cx, p, r)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Self::Pending>, p: &mut Self::Pending, t: &str) {
        self.native.on_item(cx, p, t)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Self::Pending>, p: Self::Pending, e: &str) {
        self.native.on_request_failed(cx, p, e)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, t: TimerId) {
        self.native.on_timer(cx, t)
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, c: Command) {
        let mode = self.trace.borrow_mut().pressure.take();
        if let Some(mode) = mode {
            self.trace.borrow_mut().encoder_pressure = matches!(mode, Pressure::Frames);
            let feed = self.trace.borrow().feed.unwrap();
            let desired: Vec<_> = self.trace.borrow().desired.values().cloned().collect();
            assert!(desired.len() >= 3, "real ticker and two book interests");
            let context = if matches!(mode, Pressure::Continuations | Pressure::WarmContinuation) {
                Some(
                    cx.http_context(
                        Scope::Background,
                        Policy {
                            operation: HttpOperation::Read,
                            credential: None,
                            timeout_ms: 20_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap(),
                )
            } else {
                None
            };
            if matches!(mode, Pressure::Frames) {
                cx.repair_socket_interests(
                    feed,
                    desired.iter().map(|item| item.key.to_string()).collect(),
                )
                .unwrap();
                return;
            }
            if !matches!(mode, Pressure::Frames) {
                let count = if matches!(mode, Pressure::WarmContinuation) {
                    1
                } else if matches!(mode, Pressure::Continuations) {
                    MAX_FEEDS - 1
                } else {
                    MAX_FEEDS
                };
                for _ in 0..count {
                    if let Some(context) = &context {
                        let key = desired[0].key.as_ref();
                        cx.set_socket_interests_then(
                            feed,
                            desired.clone(),
                            &[key],
                            (self.pending)(context.clone()),
                        )
                        .unwrap();
                    } else {
                        cx.set_socket_interests(feed, desired.clone()).unwrap();
                    }
                }
            }
        }
        if matches!(mode, Some(Pressure::WarmContinuation)) {
            return;
        }
        if matches!(mode, Some(Pressure::Proposals)) {
            self.native
                .on_command(cx, Command::SetTickerSymbols { symbols: vec![] });
        }
        self.native.on_command(cx, c)
    }
    fn socket_control(&self, s: &Store, f: FeedId, a: bool, t: &str) -> Control {
        self.native.socket_control(s, f, a, t)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        e: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match &e {
            LifecycleEvent::Opened(epoch) if self.trace.borrow().public == Some(epoch.socket()) => {
                self.trace.borrow_mut().feed = Some(epoch.feed())
            }
            LifecycleEvent::InterestsRefused { error, .. } => {
                self.trace.borrow_mut().refused.push(*error)
            }
            LifecycleEvent::Retired { epoch, .. }
                if self.trace.borrow().public == Some(epoch.socket()) =>
            {
                self.trace.borrow_mut().retired += 1
            }
            _ => (),
        }
        self.native.on_socket_event(cx, e)
    }
    fn on_socket_interests_released(
        &mut self,
        _: &mut Cx<'_, Self::Pending>,
        _: Epoch,
        _: Self::Pending,
    ) -> Result<(), SocketError> {
        self.trace.borrow_mut().callbacks += 1;
        panic!("retired pressure continuations cannot reach native callbacks")
    }
    fn encode_socket_interests(
        &self,
        s: &Store,
        f: FeedId,
        sub: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        self.native
            .encode_socket_interests(s, f, sub, items, &mut |mut batch| {
                // A valid-key native batch enlarged beyond the actual shared frame-byte cap.
                if self.trace.borrow().encoder_pressure {
                    let native: serde_json::Value = serde_json::from_str(&batch.text).unwrap();
                    batch.text = " ".repeat(60_000) + &batch.text;
                    assert!(batch.text.len() < MAX_FRAME_BYTES);
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&batch.text).unwrap(),
                        native
                    );
                }
                emit(batch)?;
                if self.trace.borrow().feed == Some(f) {
                    let mut trace = self.trace.borrow_mut();
                    for item in items {
                        if sub {
                            trace.desired.insert(item.key.to_string(), item.clone());
                        } else {
                            trace.desired.remove(item.key.as_ref());
                        }
                    }
                }
                Ok(())
            })
    }
    fn book(&self, s: &str) -> Option<&Book> {
        self.native.book(s)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.native.books()
    }
}

struct Harness<V: Venue> {
    engine: Engine<Observed<V>>,
    trace: Rc<RefCell<Trace>>,
    now: f64,
    respond: fn(&str) -> &'static str,
}
impl<V: Venue> Harness<V> {
    fn collect(&mut self) -> Vec<Action> {
        let mut output = vec![];
        loop {
            let actions =
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now);
            if actions.is_empty() {
                break;
            }
            for a in actions {
                match a {
                    Action::Http(r) => {
                        let body = (self.respond)(&r.url);
                        self.engine.handle(
                            self.now,
                            Event::HttpResponse {
                                request: r.id,
                                response: Response {
                                    status: 200,
                                    headers: "",
                                    body,
                                },
                            },
                        );
                    }
                    Action::WsConnect {
                        socket,
                        ref channel,
                        ..
                    } => {
                        match channel {
                            Some(SocketChannel::Public) => {
                                self.trace.borrow_mut().public = Some(socket)
                            }
                            Some(SocketChannel::Private) => {
                                self.trace.borrow_mut().private = Some(socket)
                            }
                            _ => panic!("native channel required"),
                        }
                        self.engine.handle(self.now, Event::WsOpened { socket });
                        output.push(a);
                    }
                    a => output.push(a),
                }
            }
        }
        output
    }
    fn start(
        native: V,
        pending: fn(HttpContext) -> V::Pending,
        respond: fn(&str) -> &'static str,
    ) -> Self {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut engine = safe_cex_engine::test_support::seeded_engine(Observed {
            native,
            trace: trace.clone(),
            pending,
        });
        engine.start(support::NOW);
        let mut h = Self {
            engine,
            trace,
            now: support::NOW,
            respond,
        };
        h.collect();
        h
    }
    fn message(&mut self, socket: SocketId, text: &str) -> Vec<Action> {
        self.engine
            .handle(self.now, Event::WsMessage { socket, text });
        self.collect()
    }
    fn command(&mut self, c: Command) -> Vec<Action> {
        self.engine.handle(self.now, Event::Command(c));
        self.collect()
    }
}
fn futures_response(url: &str) -> &'static str {
    if url.contains("instruments") {
        support::META
    } else if url.contains("tickers") {
        support::TICKERS
    } else if url.contains("openorders") {
        support::OPEN
    } else if url.contains("accounts") {
        support::ACCOUNT
    } else if url.contains("openpositions") {
        r#"{"result":"success","openPositions":[]}"#
    } else if url.contains("leveragepreferences") {
        r#"{"result":"success","leveragePreferences":[]}"#
    } else {
        panic!("unexpected {url}")
    }
}
fn spot_response(url: &str) -> &'static str {
    if url.ends_with("AssetPairs") {
        spot_support::META
    } else if url.ends_with("Ticker") {
        spot_support::TICKERS
    } else if url.ends_with("Balance") {
        spot_support::BALANCE
    } else if url.ends_with("OpenOrders") {
        spot_support::OPEN
    } else if url.ends_with("GetWebSocketsToken") {
        r#"{"error":[],"result":{"token":"pressure-token"}}"#
    } else {
        panic!("unexpected {url}")
    }
}
fn futures() -> Harness<Kraken> {
    let mut h = Harness::start(
        Kraken::new(support::options()),
        |context| safe_cex_venue_kraken::Pending::Markets {
            context,
            rows: vec![],
            failed: false,
        },
        futures_response,
    );
    let private = h.trace.borrow().private.unwrap();
    h.message(private, r#"{"event":"challenge","message":"pressure"}"#);
    for feed in ["fills", "open_orders", "open_positions"] {
        h.message(
            private,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    h
}
fn spot() -> Harness<KrakenSpot> {
    Harness::start(
        KrakenSpot::new(spot_support::options()),
        |context| {
            let _ = context;
            panic!("Spot source has no native continuation consumer")
        },
        spot_response,
    )
}
fn exercise<V: Venue>(mut h: Harness<V>, mode: Pressure, spot: bool) {
    let symbols: Vec<_> = h
        .engine
        .store()
        .markets()
        .iter()
        .take(2)
        .map(|m| m.symbol.clone())
        .collect();
    assert_eq!(symbols.len(), 2);
    for symbol in &symbols {
        h.command(Command::SubscribeOrderBook {
            symbol: symbol.clone(),
        });
    }
    if spot {
        for symbol in &symbols {
            h.command(Command::SubscribeCandles {
                symbol: symbol.clone(),
                interval: model::Interval::M1,
            });
        }
    }
    let public = h.trace.borrow().public.unwrap();
    let private = h.trace.borrow().private.unwrap();
    let seed = if spot {
        format!(
            r#"{{"channel":"book","type":"snapshot","data":[{{"symbol":"{}","bids":[{{"price":100,"qty":1}}],"asks":[{{"price":101,"qty":1}}]}}]}}"#,
            symbols[0]
        )
    } else {
        format!(
            r#"{{"feed":"book_snapshot","product_id":"{}","seq":1,"bids":[{{"price":100,"qty":1}}],"asks":[{{"price":101,"qty":1}}]}}"#,
            symbols[0]
        )
    };
    h.message(public, &seed);
    assert_eq!(
        h.engine.venue().book(&symbols[0]).unwrap().best_bid(),
        Some(100.)
    );
    if matches!(mode, Pressure::Frames) {
        let mut markets = h.engine.store().markets().to_vec();
        let template = markets[0].clone();
        let mut extra = vec![];
        for index in 0..80 {
            let mut market = template.clone();
            market.symbol = if spot {
                format!("P{index:04}/USD")
            } else {
                format!("PF_P{index:04}USD")
            };
            market.id = market.symbol.clone();
            extra.push(market.symbol.clone());
            markets.push(market);
        }
        h.engine.store_mut().set_markets(markets);
        for symbol in extra {
            h.command(Command::SubscribeOrderBook { symbol });
        }
    }
    let old_wakes = if matches!(mode, Pressure::Continuations) {
        h.trace.borrow_mut().pressure = Some(Pressure::WarmContinuation);
        let warm = h.command(Command::SetTickerSymbols { symbols: vec![] });
        let wakes: Vec<_> = warm
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(wakes.len(), 1, "one real published continuation wake");
        wakes
    } else {
        vec![]
    };
    h.trace.borrow_mut().pressure = Some(mode);
    let actions = h.command(Command::SetTickerSymbols {
        symbols: if matches!(mode, Pressure::Frames) {
            vec![symbols[0].clone()]
        } else {
            vec![]
        },
    });
    h.trace.borrow_mut().encoder_pressure = false;
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,Action::WsClose{socket} if *socket==public))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket} if *socket==private))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }))
    );
    assert_eq!(h.trace.borrow().retired, 1);
    assert!(h.engine.venue().book(&symbols[0]).is_none());
    let retries: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if (h.now + 750. ..=h.now + 1000.).contains(at_ms) =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        retries.len(),
        1,
        "public Capacity must keep one native shared backoff: {actions:?}"
    );
    if matches!(mode, Pressure::Frames) {
        assert_eq!(h.trace.borrow().refused, [SocketError::Capacity]);
    }
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==h.now)),
        "unpublished retired wakes cannot escape"
    );
    h.now += 1.;
    for timer in &old_wakes {
        h.engine.handle(h.now, Event::TimerFired { timer: *timer });
        assert!(h.collect().is_empty());
    }
    // A native private publication proves sibling ingress survived public retirement.
    if spot {
        h.engine.store_mut().set_balance(model::Balance::default());
        h.message(
            private,
            r#"{"channel":"balances","type":"update","data":[]}"#,
        );
        assert_eq!(h.engine.store().balance().total, 1355.);
    } else {
        assert!(h.engine.store().order("pressure-peer").is_none());
        h.message(private,r#"{"feed":"open_orders_snapshot","orders":[{"instrument":"PF_XBTUSD","order_id":"pressure-peer","qty":2,"filled":0,"type":"limit","direction":0,"limit_price":99,"last_update_time":1700000000000}]}"#);
        assert!(h.engine.store().order("pressure-peer").is_some());
    }
    h.command(Command::SetTickerSymbols { symbols: vec![] });
    let (timer, at) = retries[0];
    h.now = at;
    h.engine.handle(at, Event::TimerFired { timer });
    let actions = h.collect();
    let replacement = h.trace.borrow().public.unwrap();
    assert_ne!(replacement, public);
    let wire_symbols: Vec<_> = symbols
        .iter()
        .map(|symbol| {
            let market = h.engine.store().market(symbol).unwrap();
            if spot {
                format!("{}/{}", market.base, market.quote)
            } else {
                symbol.clone()
            }
        })
        .collect();
    for symbol in &wire_symbols {
        assert!(actions.iter().any(|a|matches!(a,Action::WsSend{socket,text} if *socket==replacement && text.contains("book") && text.contains(symbol))),"retained book {symbol} did not replay: {actions:?}");
    }
    if spot {
        for symbol in &wire_symbols {
            assert!(actions.iter().any(|a| matches!(a,Action::WsSend{socket,text} if *socket==replacement && text.contains("ohlc") && text.contains(symbol) && text.contains("\"interval\":1"))));
        }
    }
    assert!(actions.iter().any(|a| matches!(a,Action::WsSend{socket,text} if *socket==replacement && text.contains("ticker"))));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket} if *socket==private))
    );
    h.now = at + 1.;
    for timer in &old_wakes {
        h.engine.handle(h.now, Event::TimerFired { timer: *timer });
        assert!(h.collect().is_empty());
    }
    assert_eq!(h.trace.borrow().callbacks, 0);
}
#[test]
fn futures_public_proposal_pressure_recovers_and_keeps_private_live() {
    exercise(futures(), Pressure::Proposals, false)
}
#[test]
fn spot_public_proposal_pressure_recovers_and_keeps_private_live() {
    exercise(spot(), Pressure::Proposals, true)
}
#[test]
fn futures_public_continuation_pressure_cancels_old_wakes_and_recovers() {
    exercise(futures(), Pressure::Continuations, false)
}
#[test]
fn futures_valid_key_frame_pressure_recovers_without_partial_frames() {
    exercise(futures(), Pressure::Frames, false)
}
#[test]
fn spot_valid_key_frame_pressure_recovers_without_partial_frames() {
    exercise(spot(), Pressure::Frames, true)
}
