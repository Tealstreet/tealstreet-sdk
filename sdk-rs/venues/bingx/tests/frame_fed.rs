//! A frame-fed BingX engine: public contracts only, no sockets or account
//! reads, and writes that read the store and VENUE_STATE its data source sends.
use safe_cex_engine::model::{
    Limits, Market, MinMax, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, Precision,
};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response, Store};
use safe_cex_venue_bingx::Bingx;
mod support;

const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const ORDER_ID: &str = "8389765519993909000";

struct Harness {
    engine: Engine<Bingx>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: usize,
    timers: Vec<safe_cex_engine::TimerId>,
    failed: Vec<(u32, String)>,
}

impl Harness {
    fn frame_fed() -> Self {
        let mut engine = support::engine(support::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Default::default()
        });
        engine.set_frame_fed().unwrap();
        let mut h = Self {
            engine,
            now: 1000.0,
            requests: Vec::new(),
            sockets: 0,
            timers: Vec::new(),
            failed: Vec::new(),
        };
        h.engine.start(h.now);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect { .. } => self.sockets += 1,
                Action::SetTimer { timer, .. } => self.timers.push(timer),
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.failed.push((command, error.legacy_text()))
                }
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn paths(&self) -> Vec<&str> {
        self.requests.iter().map(|r| r.url.as_str()).collect()
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}; have {:?}", self.paths()));
        self.requests.remove(index)
    }
    fn load(&mut self) {
        assert_eq!(self.requests.len(), 1, "only contracts: {:?}", self.paths());
        let contracts = self.take("/quote/contracts");
        self.event(Event::HttpResponse {
            request: contracts.id,
            response: Response {
                status: 200,
                headers: "",
                body: MARKETS,
            },
        });
        assert!(
            self.requests.is_empty(),
            "nothing after contracts: {:?}",
            self.paths()
        );
    }
    fn place(&mut self, command: u32) {
        let orders: Vec<PlaceOrderOpts> = serde_json::from_str(
            r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":60000}]"#,
        )
        .unwrap();
        self.now += 1.;
        self.event(Event::Command(Command::PlaceOrders { command, orders }));
    }
}

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

/// A data source's snapshot: one open order and BingX's VENUE_STATE (version 1:
/// mode known, that order observed as a NEW one-way LIMIT).
fn source_frame() -> Vec<u8> {
    let mut source = Store::default();
    source.set_markets(vec![Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.0001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.0001,
                max: f64::INFINITY,
            },
            leverage: MinMax { min: 1., max: 125. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    source.set_orders(vec![Order {
        id: ORDER_ID.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 59000.,
        amount: 0.2,
        filled: 0.,
        remaining: 0.2,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }]);
    let mut state = vec![1u8, 1];
    state.extend_from_slice(&1u32.to_le_bytes());
    for text in [ORDER_ID, "LIMIT", "BOTH"] {
        put_str(&mut state, text);
    }
    state.push(0);
    put_str(&mut state, "NEW");
    let pending = source.take_pending();
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.0);
    source.write(&pending, &mut writer);
    writer.venue_state(&state);
    writer.finish()
}

#[test]
fn a_frame_fed_start_reads_only_public_contracts_and_opens_no_socket() {
    let mut h = Harness::frame_fed();
    h.load();
    assert!(h.engine.store().loaded().markets);
    assert_eq!(h.sockets, 0);
    for timer in std::mem::take(&mut h.timers) {
        h.now += 120_000.;
        h.event(Event::TimerFired { timer });
    }
    assert!(h.requests.is_empty(), "no polls: {:?}", h.paths());
}

#[test]
fn a_place_refuses_until_the_mode_is_known_then_sends_only_itself() {
    let mut h = Harness::frame_fed();
    h.load();
    h.place(7);
    assert!(
        h.failed
            .iter()
            .any(|(command, text)| *command == 7 && text.contains("position mode")),
        "{:?}",
        h.failed
    );
    assert!(h.requests.is_empty());

    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    assert!(
        h.requests.is_empty(),
        "ingest reads nothing: {:?}",
        h.paths()
    );
    h.place(8);
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
    let write = h.take("/openApi/swap/v2/trade/order");
    // An unknown outcome: no resend and no reconcile read; frames settle it.
    h.event(Event::HttpFailed {
        request: write.id,
        error: "connection reset",
    });
    for timer in std::mem::take(&mut h.timers) {
        h.now += 300_000.;
        h.event(Event::TimerFired { timer });
    }
    assert!(
        h.requests.is_empty(),
        "nothing after the write: {:?}",
        h.paths()
    );
}

#[test]
fn an_ingested_order_cancels_in_exactly_one_request() {
    let mut h = Harness::frame_fed();
    h.load();
    h.engine.apply_frame(h.now, &source_frame(), true).unwrap();
    h.collect();
    h.now += 1.;
    h.event(Event::Command(Command::CancelOrders {
        command: 9,
        ids: vec![ORDER_ID.into()],
    }));
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
    assert!(h.requests[0].url.contains(ORDER_ID), "{:?}", h.paths());
}
