use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, RequestId, SocketId};
use safe_cex_venue_binance::{BinanceFutures, Options};

const EXCHANGE_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"120"}]}]}"#;
const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]"#;
const BOOK_TICKERS: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]"#;
const PREMIUM: &str = r#"[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]"#;

struct Harness {
    engine: Engine<BinanceFutures>,
    now: f64,
    requests: Vec<HttpRequest>,
    public_socket: Option<SocketId>,
}

impl Harness {
    fn new() -> Self {
        let mut harness = Self {
            engine: Engine::new(BinanceFutures::new(Options {
                testnet: false,
                public_data_only: true,
            })),
            now: 1_000.0,
            requests: Vec::new(),
            public_socket: None,
        };
        harness.engine.start(harness.now);
        harness.collect();
        harness.respond("/fapi/v1/exchangeInfo", 200, EXCHANGE_INFO);
        harness.respond("/fapi/v1/ticker/24hr", 200, DAILY);
        harness.respond("/fapi/v1/ticker/bookTicker", 200, BOOK_TICKERS);
        harness.respond("/fapi/v1/premiumIndex", 200, PREMIUM);
        let socket = harness
            .public_socket
            .expect("public socket connects after tickers");
        harness.event(Event::WsOpened { socket });
        harness.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }));
        harness
    }

    fn collect(&mut self) {
        for action in self.engine.drain_actions().collect::<Vec<_>>() {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect { socket, url } if url.ends_with("/public/ws") => {
                    self.public_socket = Some(socket)
                }
                _ => {}
            }
        }
    }

    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    fn take_request(&mut self, path: &str) -> RequestId {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("no request for {path}"));
        self.requests.remove(index).id
    }

    fn respond(&mut self, path: &str, status: u16, body: &str) {
        let request = self.take_request(path);
        self.event(Event::HttpResponse {
            request,
            status,
            body,
        });
    }

    fn diff(&mut self, first: u64, last: u64, previous: u64, bid: &str) {
        let text = format!(
            r#"{{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":{first},"u":{last},"pu":{previous},"b":[["{bid}","1"]],"a":[]}}"#
        );
        let socket = self.public_socket.unwrap();
        self.event(Event::WsMessage {
            socket,
            text: &text,
        });
    }

    fn snapshot_requests(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| r.url.contains("/fapi/v1/depth"))
            .count()
    }

    fn best_bid(&self) -> Option<f64> {
        self.engine.book("BTCUSDT").and_then(|book| book.best_bid())
    }
}

const SNAPSHOT: &str = r#"{"lastUpdateId":100,"bids":[["99.0","5"]],"asks":[["101.0","5"]]}"#;

#[test]
fn first_diff_after_snapshot_is_checked_by_id_range_not_pu() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    assert_eq!(h.snapshot_requests(), 1);
    h.respond("/fapi/v1/depth", 200, SNAPSHOT);
    assert_eq!(h.best_bid(), Some(99.0));

    // Its `pu` predates the snapshot; only U <= 101 <= u matters.
    h.diff(98, 105, 97, "99.5");
    h.diff(106, 110, 105, "99.7");
    assert_eq!(h.best_bid(), Some(99.7));
    assert_eq!(h.snapshot_requests(), 0, "no resync while continuous");
}

#[test]
fn a_gap_resyncs_once_from_the_next_diff() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    h.respond("/fapi/v1/depth", 200, SNAPSHOT);
    h.diff(98, 105, 97, "99.5");
    h.diff(120, 125, 119, "99.6");
    assert_eq!(h.best_bid(), None, "book dropped on a gap");
    h.diff(126, 130, 125, "99.6");
    assert_eq!(h.snapshot_requests(), 1);
}

#[test]
fn rate_limits_pause_snapshots_instead_of_retrying() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    h.respond("/fapi/v1/depth", 418, r#"{"code":-1003}"#);
    for update in 0..50 {
        h.diff(200 + update, 200 + update, 199 + update, "98.0");
    }
    assert_eq!(h.snapshot_requests(), 0, "no requests during the ban");
}
