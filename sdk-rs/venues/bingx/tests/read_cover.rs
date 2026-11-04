//! Read covers on a real BingX source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! §5.2, §5.3): a page's bounded refresh ask reads open orders, and the read
//! publishes its account cover at its send time.
mod support;
use safe_cex_engine::wire::section;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, VenueAction};
use safe_cex_venue_bingx::Bingx;
use support::Options;

const KEY: &str = "/openApi/user/auth/userDataStream";
const MODE: &str = "/openApi/swap/v1/positionSide/dual";
const BALANCE: &str = "/openApi/swap/v3/user/balance";
const POSITIONS: &str = "/openApi/swap/v2/user/positions";
const ORDERS: &str = "/openApi/swap/v2/trade/openOrders";
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const DAILY: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;
const BALANCES: &str = r#"{"code":0,"data":[{"asset":"USDT","balance":"1000","availableMargin":"950","usedMargin":"50","unrealizedProfit":"10"}]}"#;
const FLAT: &str = r#"{"code":0,"data":[]}"#;
const OPEN: &str = r#"{"code":0,"data":{"orders":[]}}"#;

struct H {
    e: Engine<Bingx>,
    now: f64,
    http: Vec<HttpRequest>,
}

impl H {
    fn pump(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            if let Action::Http(request) = action {
                self.http.push(request);
            }
        }
    }
    fn take(&mut self, part: &str) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.contains(part))
            .unwrap_or_else(|| panic!("a request for {part}"));
        self.http.remove(index)
    }
    fn reply(&mut self, request: &HttpRequest, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.pump();
    }
    fn answer(&mut self, part: &str, body: &str) {
        let request = self.take(part);
        self.reply(&request, body);
    }
    fn refresh(&mut self, command: u32, symbols: Option<Vec<String>>) {
        self.e.handle(
            self.now,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::RefreshOrders { symbols },
            }),
        );
        self.pump();
    }
    fn orders_reads(&self) -> Vec<HttpRequest> {
        self.http
            .iter()
            .filter(|r| r.url.contains(ORDERS))
            .cloned()
            .collect()
    }
    /// Answer `reads` with no open orders.
    fn answer_reads(&mut self, reads: &[HttpRequest]) {
        for read in reads {
            self.http.retain(|r| r.id != read.id);
            self.reply(read, OPEN);
        }
    }
    /// The snapshot's account cover (ORDERS_READ), if any.
    fn account_cover(&mut self) -> Option<f64> {
        let frame = self.e.snapshot_frame(self.now).to_vec();
        safe_cex_engine::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::ORDERS_READ)
            .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
            .filter(|cover| !cover.is_nan())
    }
}

/// A started, loaded source with its private stream open and nothing out.
fn started_with_orders_out() -> H {
    let mut h = H {
        e: support::engine(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }),
        now: 1000.0,
        http: Vec::new(),
    };
    h.e.start(h.now);
    h.pump();
    h.answer("/quote/contracts", MARKETS);
    h.answer("/quote/ticker", DAILY);
    h.answer("/quote/premiumIndex", PREMIUM);
    h.answer(KEY, r#"{"listenKey":"key-one"}"#);
    h.answer(MODE, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.answer(POSITIONS, FLAT);
    h.answer(BALANCE, BALANCES);
    h
}

fn loaded() -> H {
    let mut h = started_with_orders_out();
    h.answer(ORDERS, OPEN);
    while h.http.iter().any(|r| r.url.contains(ORDERS)) {
        h.answer(ORDERS, OPEN);
    }
    h.http.clear();
    h
}

#[test]
fn bingx_advertises_read_covers() {
    let mut h = loaded();
    let frame = h.e.snapshot_frame(h.now).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    assert_ne!(
        flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT,
        0
    );
}

/// The ask reads open orders; the answer publishes an account cover at the
/// ask's send time, not the landing time.
#[test]
fn a_refresh_ask_reads_orders_and_publishes_its_send_time() {
    for symbols in [None, Some(vec!["BTC-USDT".to_owned()])] {
        let mut h = loaded();
        h.now += 5_000.0;
        let asked = h.now;
        h.refresh(60, symbols.clone());
        let reads = h.orders_reads();
        assert_eq!(reads.len(), 1, "{symbols:?}");
        h.now += 1_000.0;
        h.answer_reads(&reads);
        assert_eq!(h.account_cover(), Some(asked), "{symbols:?}");
    }
}

/// H7/H3: asks while the read is out start nothing; exactly one follow-up
/// read starts after it lands, and nothing after that one.
#[test]
fn asks_while_a_read_is_out_get_one_follow_up() {
    let mut h = loaded();
    h.now += 5_000.0;
    h.refresh(60, Some(vec!["BTC-USDT".into()]));
    let first = h.orders_reads();
    assert_eq!(first.len(), 1);
    for command in 61..161 {
        h.refresh(command, None);
    }
    assert_eq!(
        h.orders_reads().len(),
        1,
        "H7: nothing more while it is out"
    );
    h.now += 100.0;
    let sent = h.now;
    h.answer_reads(&first);
    let follow = h.orders_reads();
    assert_eq!(follow.len(), 1, "one follow-up read");
    h.now += 100.0;
    h.answer_reads(&follow);
    assert_eq!(h.account_cover(), Some(sent));
    h.now += 1_000.0;
    h.pump();
    assert!(h.orders_reads().is_empty());
}

#[test]
fn a_source_ask_joins_an_ordinary_census_and_owes_one_successor() {
    let mut h = started_with_orders_out();
    let first = h.orders_reads();
    assert_eq!(first.len(), 1);
    h.now += 100.;
    for command in 100..200 {
        h.refresh(command, None);
    }
    assert_eq!(h.orders_reads().iter().map(|r| r.id).collect::<Vec<_>>(),
        first.iter().map(|r| r.id).collect::<Vec<_>>(), "retain the ordinary read, no competing batch");
    h.now += 100.;
    let fresh_sent = h.now;
    h.answer_reads(&first);
    let successor = h.orders_reads();
    assert_eq!(successor.len(), 1);
    assert_ne!(successor[0].id, first[0].id);
    assert!(h.account_cover().is_none_or(|at| at < fresh_sent));
    h.now += 100.;
    h.answer_reads(&successor);
    assert_eq!(h.account_cover(), Some(fresh_sent));
    assert!(h.orders_reads().is_empty());
}
