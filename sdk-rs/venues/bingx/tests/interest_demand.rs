//! Real BingX binding, actual proposal callbacks and installed signing provider.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::socket_lifecycle::{FeedId, Interest, LifecycleEvent, SocketError};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bingx::Bingx;
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const BOOK: &str = "market.depth.BTC-USDT.step0.level100";
const CANDLE: &str = "market.kline.BTC-USDT.1min.trade.utc+8";
#[derive(Clone, Copy, Default, PartialEq)]
enum Scenario {
    #[default]
    Plain,
    Burst,
    AddRemove,
    Renew,
    PrivateRefusal,
    PublicOpeningRefusal,
}
#[derive(Default)]
struct Trace {
    scenario: Scenario,
    public: Option<SocketId>,
    public_feed: Option<FeedId>,
    private_refusals: usize,
    public_refusals: usize,
}
struct Probe(Bingx, Rc<RefCell<Trace>>);
fn add_book() -> Command {
    Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }
}
fn remove_book() -> Command {
    Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }
}
fn add_candle() -> Command {
    Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
    }
}
fn remove_candle() -> Command {
    Command::UnsubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
    }
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        <Bingx as Venue>::managed_sockets()
    }
    fn socket_capabilities() -> safe_cex_engine::socket_lifecycle::SocketCapabilities {
        <Bingx as Venue>::socket_capabilities()
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        let point = match &event {
            LifecycleEvent::Ready { epoch, .. } => {
                Some((self.1.borrow().public == Some(epoch.socket()), epoch.feed()))
            }
            _ => None,
        };
        if let Some((true, feed)) = point {
            self.1.borrow_mut().public_feed = Some(feed);
        }
        if let LifecycleEvent::InterestsRefused { feed, .. } = &event {
            if self.1.borrow().public_feed == Some(*feed) {
                self.1.borrow_mut().public_refusals += 1;
            } else {
                self.1.borrow_mut().private_refusals += 1;
            }
        }
        self.0.on_socket_event(cx, event)?;
        let scenario = self.1.borrow().scenario;
        match (point, scenario) {
            (Some((true, _)), Scenario::Burst) => {
                self.0.on_command(cx, add_book());
                self.0.on_command(cx, add_candle());
                self.0.on_command(cx, add_book());
                self.0.on_command(cx, add_candle());
            }
            (Some((true, _)), Scenario::AddRemove) => {
                self.0.on_command(cx, add_book());
                self.0.on_command(cx, remove_book());
            }
            (Some((true, _)), Scenario::PublicOpeningRefusal) => self.0.on_command(cx, add_book()),
            (Some((false, feed)), Scenario::PrivateRefusal) => {
                cx.set_socket_interests(
                    feed,
                    vec![Interest::from_wire_args("private.refusal.control")],
                )
                .expect("real private proposal");
                self.0.on_command(cx, add_book());
            }
            _ => (),
        }
        Ok(())
    }
    fn socket_control(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.0.socket_control(store, feed, authenticated, text)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        values: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        if self.1.borrow().scenario == Scenario::PublicOpeningRefusal
            && self.1.borrow().public_feed == Some(feed)
            && !values.is_empty()
            && subscribe
            && values
                .iter()
                .all(|i| i.wire_args.as_ref() == "market.contracts")
        {
            return Err(SocketError::InvalidInterest);
        }
        self.0
            .encode_socket_interests(store, feed, subscribe, values, emit)
    }

    type Pending = <Bingx as Venue>::Pending;
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
        self.0.start(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str) {
        self.0.on_message(cx, socket, text);
    }
    fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Self::Pending>, pending: &mut Self::Pending, item: &str) {
        self.0.on_item(cx, pending, item);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_response(cx, pending, response);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.0.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.0.on_sign_error(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.0
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_stream_error(cx, pending, response);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId) {
        self.0.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        let renew = self.1.borrow().scenario == Scenario::Renew
            && matches!(&command, Command::UnsubscribeOrderBook { .. });
        self.0.on_command(cx, command);
        if renew {
            self.0.on_command(cx, add_book());
        }
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.0.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.0.books()
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.0.capabilities()
    }
}

struct Harness {
    e: Engine<Probe>,
    trace: Rc<RefCell<Trace>>,
    actions: Vec<Action>,
    now: f64,
    public: SocketId,
    private: Option<SocketId>,
}
impl Harness {
    fn new(private: bool, scenario: Scenario) -> Self {
        let (venue, signer) = support::Options {
            public_data_only: !private,
            key: private.then(|| "fixture-key".into()),
            secret: private.then(|| "fixture-secret".into()),
            ..Default::default()
        }
        .build();
        let trace = Rc::new(RefCell::new(Trace {
            scenario,
            ..Default::default()
        }));
        let mut e = safe_cex_engine::test_support::seeded_engine(Probe(venue, trace.clone()));
        e.set_signer(Some(signer)).unwrap();
        e.start(0.);
        let mut h = Self {
            e,
            trace,
            actions: vec![],
            now: 0.,
            public: SocketId(0),
            private: None,
        };
        h.collect();
        h.reply("/contracts", MARKETS);
        if private {
            h.reply("/user/auth/userDataStream", r#"{"listenKey":"key-one"}"#);
            h.reply("/trade/openOrders", r#"{"code":0,"data":[]}"#);
        }
        h.public = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if !url.contains("listenKey=") => {
                    Some(*socket)
                }
                _ => None,
            })
            .expect("actual public connect");
        h.private = h.actions.iter().find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.contains("listenKey=") => Some(*socket),
            _ => None,
        });
        h.trace.borrow_mut().public = Some(h.public);
        h.actions.clear();
        h
    }
    fn collect(&mut self) {
        self.actions
            .extend(safe_cex_engine::test_support::prepared_actions(
                &mut self.e,
                self.now,
            ));
    }
    fn reply(&mut self, path: &str, body: &str) {
        let index = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r) if r.url.contains(path)))
            .expect("actual released startup read");
        let Action::Http(HttpRequest { id, .. }) = self.actions.remove(index) else {
            unreachable!()
        };
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    fn event(&mut self, event: Event<'_>) -> Vec<Action> {
        self.now += 1.;
        self.e.handle(self.now, event);
        self.collect();
        std::mem::take(&mut self.actions)
    }
}
fn topics(out: &[Action], socket: SocketId) -> Vec<(String, String)> {
    out.iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: own, text } if *own == socket => {
                serde_json::from_str::<serde_json::Value>(text).ok()
            }
            _ => None,
        })
        .filter_map(|v| {
            Some((
                v["reqType"].as_str()?.into(),
                v["dataType"].as_str()?.into(),
            ))
        })
        .collect()
}
fn live(groups: impl IntoIterator<Item = (String, String)>) -> BTreeSet<String> {
    let mut live = BTreeSet::new();
    for (op, topic) in groups {
        if op == "sub" {
            live.insert(topic);
        } else if op == "unsub" {
            live.remove(&topic);
        }
    }
    live
}
#[test]
fn connecting_book_and_candle_demands_replay_on_the_first_open() {
    for (command, topic) in [(add_book(), BOOK), (add_candle(), CANDLE)] {
        let mut h = Harness::new(false, Scenario::Plain);
        let socket = h.public;
        assert!(topics(&h.event(Event::Command(command)), socket).is_empty());
        let opened = h.event(Event::WsOpened { socket });
        assert!(live(topics(&opened, socket)).contains(topic));
    }
}
#[test]
fn pending_changes_keep_distinct_topics_and_duplicate_book_and_candle_refs() {
    let mut h = Harness::new(false, Scenario::Burst);
    let socket = h.public;
    let out = h.event(Event::WsOpened { socket });
    let demand = live(topics(&out, socket));
    assert!(demand.contains(BOOK));
    assert!(
        demand.contains(CANDLE),
        "second pending candle demand lost: {demand:?}"
    );
    assert!(
        topics(&h.event(Event::Command(remove_book())), socket).is_empty(),
        "one of two book refs remains"
    );
    let out = h.event(Event::Command(remove_book()));
    assert_eq!(topics(&out, socket), vec![("unsub".into(), BOOK.into())]);
    assert!(
        topics(&h.event(Event::Command(remove_candle())), socket).is_empty(),
        "one of two candle refs remains"
    );
    assert_eq!(
        topics(&h.event(Event::Command(remove_candle())), socket),
        vec![("unsub".into(), CANDLE.into())]
    );
}
#[test]
fn pending_add_then_remove_has_no_surviving_book_demand() {
    let mut h = Harness::new(false, Scenario::AddRemove);
    let socket = h.public;
    let out = h.event(Event::WsOpened { socket });
    assert!(
        !live(topics(&out, socket)).contains(BOOK),
        "pending removal was lost"
    );
    assert_eq!(
        topics(&h.event(Event::Command(add_book())), socket),
        vec![("sub".into(), BOOK.into())]
    );
}
#[test]
fn pending_remove_then_renewal_preserves_the_book() {
    let mut h = Harness::new(false, Scenario::Renew);
    let socket = h.public;
    h.event(Event::WsOpened { socket });
    let first = h.event(Event::Command(add_book()));
    let last = h.event(Event::Command(remove_book()));
    assert!(
        live(
            topics(&first, socket)
                .into_iter()
                .chain(topics(&last, socket))
        )
        .contains(BOOK),
        "pending removal discarded renewed book demand"
    );
}
#[test]
fn private_refusal_cannot_discard_public_demand_or_close_the_public_peer() {
    let mut h = Harness::new(true, Scenario::PrivateRefusal);
    let public = h.public;
    let private = h.private.unwrap();
    h.event(Event::WsOpened { socket: public });
    let out = h.event(Event::WsOpened { socket: private });
    assert!(
        h.trace.borrow().private_refusals > 0,
        "real private refusal reached production callback"
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::WsClose{socket} if *socket==public))
    );
    assert!(live(topics(&out, public)).contains(BOOK));
    assert_eq!(
        topics(&h.event(Event::Command(remove_book())), public),
        vec![("unsub".into(), BOOK.into())]
    );
}
#[test]
fn refused_public_opening_cannot_erase_a_later_admitted_book_change() {
    let mut h = Harness::new(true, Scenario::PublicOpeningRefusal);
    let public = h.public;
    let private = h.private.unwrap();
    h.event(Event::WsOpened { socket: private });
    let out = h.event(Event::WsOpened { socket: public });
    assert!(
        h.trace.borrow().public_refusals > 0,
        "real opening proposal refusal"
    );
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a,Action::WsClose{socket}if *socket==public))
            .count(),
        1
    );
    assert!(
        h.e.venue().0.books().any(|(symbol, _)| symbol == "BTCUSDT"),
        "refused opening must retain later admitted book demand"
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==private))
    );
    assert!(
        h.e.venue().0.book("BTCUSDT").is_none(),
        "retired book data stays invalidated"
    );
    let peer = h.event(Event::WsMessage {
        socket: private,
        text: "Ping",
    });
    assert!(peer.iter().any(
        |a| matches!(a, Action::WsSend { socket, text } if *socket == private && text == "Pong")
    ));
}
