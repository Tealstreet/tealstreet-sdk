//! Exercises the real BingX binding, provider and captured socket identities.
mod support;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, PublicFeedPolicy, Response, SocketId};
use safe_cex_venue_bingx::Bingx;
use support::Options;

const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;

struct Harness {
    engine: Engine<Bingx>,
    now: f64,
    actions: Vec<Action>,
    public: Option<SocketId>,
    private: Option<SocketId>,
}
impl Harness {
    fn new(private: bool, suppressed: bool) -> Self {
        let mut engine = support::engine(Options {
            public_data_only: !private,
            key: private.then(|| "fixture-key".into()),
            secret: private.then(|| "fixture-secret".into()),
            ..Default::default()
        });
        if suppressed {
            engine
                .set_public_feed_policy(PublicFeedPolicy::Disabled)
                .unwrap();
        }
        engine.start(0.);
        let mut h = Self {
            engine,
            now: 0.,
            actions: vec![],
            public: None,
            private: None,
        };
        h.collect();
        let markets = h.request("/contracts");
        h.reply(&markets, MARKETS);
        if private {
            let key = h.request("/user/auth/userDataStream");
            h.reply(&key, r#"{"listenKey":"key-one"}"#);
            let orders = h.request("/trade/openOrders");
            h.reply(&orders, r#"{"code":0,"data":[]}"#);
        }
        h.actions.clear();
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            if let Action::WsConnect { socket, url, .. } = &a {
                if url.contains("listenKey=") {
                    self.private = Some(*socket);
                } else {
                    self.public = Some(*socket);
                }
            }
            self.actions.push(a);
        }
    }
    fn event(&mut self, event: Event<'_>) -> Vec<Action> {
        self.now += 1.;
        self.engine.handle(self.now, event);
        self.collect();
        std::mem::take(&mut self.actions)
    }
    fn request(&mut self, path: &str) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r) if r.url.contains(path)))
            .expect("real released request");
        match self.actions.remove(i) {
            Action::Http(r) => r,
            _ => unreachable!(),
        }
    }
    fn reply(&mut self, r: &HttpRequest, body: &str) {
        self.now += 1.;
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
        self.collect();
    }
}
fn sends(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
#[test]
fn duplicate_public_open_does_not_replay_or_reset_supervision() {
    let mut h = Harness::new(false, false);
    let socket = h.public.unwrap();
    let opened = h.event(Event::WsOpened { socket });
    assert!(
        sends(&opened, socket)
            .iter()
            .any(|s| s.contains("market.contracts"))
    );
    assert!(sends(&opened, socket).contains(&"Ping"));
    let duplicate = h.event(Event::WsOpened { socket });
    assert!(
        duplicate.is_empty(),
        "duplicate open cannot replay native work: {duplicate:?}"
    );
}
#[test]
fn duplicate_private_open_does_not_repeat_ping_or_account_refresh() {
    let mut h = Harness::new(true, false);
    let socket = h.private.unwrap();
    let opened = h.event(Event::WsOpened { socket });
    assert_eq!(sends(&opened, socket), vec!["Ping"]);
    assert!(
        opened
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("openOrders")))
    );
    let duplicate = h.event(Event::WsOpened { socket });
    assert!(
        duplicate.is_empty(),
        "duplicate private open owns no new work: {duplicate:?}"
    );
}
#[test]
fn suppressed_public_still_allows_the_real_private_feed() {
    let mut h = Harness::new(true, true);
    assert!(h.public.is_none());
    let socket = h.private.unwrap();
    assert_eq!(
        sends(&h.event(Event::WsOpened { socket }), socket),
        vec!["Ping"]
    );
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket,
                text: "Ping"
            }),
            socket
        ),
        vec!["Pong"]
    );
}
#[test]
fn retiring_public_does_not_retire_private_and_old_open_is_inert() {
    let mut h = Harness::new(true, false);
    let public = h.public.unwrap();
    let private = h.private.unwrap();
    h.event(Event::WsOpened { socket: public });
    h.event(Event::WsOpened { socket: private });
    let closed = h.event(Event::WsClosed {
        socket: public,
        code: 1006,
    });
    assert!(closed.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(h.event(Event::WsOpened { socket: public }).is_empty());
    assert_eq!(
        sends(
            &h.event(Event::WsMessage {
                socket: private,
                text: "Ping"
            }),
            private
        ),
        vec!["Pong"]
    );
}
