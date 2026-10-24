//! Review regressions for native cleanup after shared Context metadata retirement.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_mexc::{MexcFutures, Options, Pending};

struct WithPeers {
    venue: MexcFutures,
    peers: Vec<Context>,
}
impl Venue for WithPeers {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for _ in 0..255 {
            self.peers.push(
                cx.http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 60_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap(),
            );
        }
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.venue.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
}
fn collect<V: Venue>(e: &mut Engine<V>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn http(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .expect("public read")
}
fn reply<V: Venue>(e: &mut Engine<V>, now: f64, r: &HttpRequest, body: &str) {
    e.handle(
        now,
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
fn options() -> Options {
    Options {
        public_data_only: true,
        ..Default::default()
    }
}
#[test]
fn completed_catalogue_releases_its_slot_before_ticker_admission() {
    let mut e = safe_cex_engine::test_support::seeded_engine(WithPeers {
        venue: MexcFutures::new(options()),
        peers: vec![],
    });
    e.start(0.);
    assert_eq!(e.venue().peers.len(), 255);
    let catalogue = http(&collect(&mut e, 0.), "contract/detail");
    reply(&mut e, 100., &catalogue, support::MARKETS);
    assert!(e.store().loaded().markets);
    let a = collect(&mut e, 100.);
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("contract/ticker"))),
        "completed catalogue must not occupy the ticker's only free slot"
    );
    let ticker = http(&a, "contract/ticker");
    reply(&mut e, 101., &ticker, support::TICKERS);
    assert!(e.store().loaded().tickers);
}
#[test]
fn sibling_issuance_pruning_expired_handle_does_not_strand_depth_recovery() {
    let mut e = safe_cex_engine::test_support::seeded_engine(MexcFutures::new(options()));
    e.start(0.);
    let catalogue = http(&collect(&mut e, 0.), "contract/detail");
    let mut rows: serde_json::Value = serde_json::from_str(support::MARKETS).unwrap();
    let mut eth = rows["data"][0].clone();
    eth["symbol"] = "ETH_USDT".into();
    eth["baseCoin"] = "ETH".into();
    rows["data"].as_array_mut().unwrap().push(eth);
    reply(&mut e, 100., &catalogue, &rows.to_string());
    let ticker = http(&collect(&mut e, 100.), "contract/ticker");
    reply(&mut e, 100., &ticker, support::TICKERS);
    let socket = collect(&mut e, 100.)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(100., Event::WsOpened { socket });
    collect(&mut e, 100.);
    e.handle(
        200.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let a = collect(&mut e, 200.);
    let old = http(&a, "depth/BTC_USDT");
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_200. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        20_200.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETHUSDT".into(),
        }),
    );
    let peer = http(&collect(&mut e, 20_200.), "depth/ETH_USDT");
    e.handle(20_200., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    let retry = a.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms == 20_700. => Some(*timer),
        _ => None,
    });
    assert!(
        retry.is_some(),
        "engine-admitted timeout must recover even if issuance pruned cleanup metadata"
    );
    const DEPTH: &str = r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,10]],"asks":[[60001,40]]}}"#;
    reply(&mut e, 20_201., &peer, DEPTH);
    assert_eq!(e.book("ETHUSDT").unwrap().best_bid(), Some(60000.));
    e.handle(
        20_700.,
        Event::TimerFired {
            timer: retry.unwrap(),
        },
    );
    let fresh = http(&collect(&mut e, 20_700.), "depth/BTC_USDT");
    reply(&mut e, 20_701., &old, DEPTH);
    assert!(e.book("BTCUSDT").is_none());
    reply(&mut e, 20_702., &fresh, DEPTH);
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(60000.));
    assert_eq!(e.book("ETHUSDT").unwrap().best_bid(), Some(60000.));
}
