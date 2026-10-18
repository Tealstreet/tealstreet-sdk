//! Production private socket event pump, actual Store and debounced REST binding.
#[path = "support/venue_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::{exchange::Exchange, socket_lifecycle::SocketSlot, websocket::WsEvent};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{OrderStatus, PositionSide};
use safe_cex_venue_bitunix::adapter::BitunixVenue;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, sync::Arc};

const ACCOUNT: &str = "/api/v1/futures/account";
const POSITIONS: &str = "/api/v1/futures/position/get_pending_positions";
struct Scene {
    venue: Rc<BitunixVenue>,
    runtime: Rc<support::Runtime>,
    store: Rc<DefaultStore>,
    wire: Arc<support::Wire>,
    socket: Rc<support::FakeSocket>,
    fills: Rc<Cell<usize>>,
}
impl Scene {
    fn new() -> Self {
        let (venue, runtime, store, wire) = support::fixture_with(false);
        runtime.now.set(1_700_000_000_000.0);
        runtime.dial.set(true);
        venue
            .base()
            .get_socket(SocketSlot::Private)
            .unwrap()
            .connect_and_subscribe(false)
            .unwrap();
        for _ in 0..32 {
            runtime.pump();
        }
        runtime.now.set(runtime.now.get() + 25.0);
        for _ in 0..32 {
            runtime.pump();
        }
        let socket = runtime
            .dialed
            .borrow()
            .iter()
            .find(|(url, _)| url.ends_with("/private/"))
            .unwrap()
            .1
            .clone();
        assert!(
            socket
                .sent
                .borrow()
                .iter()
                .any(|frame| frame.contains("\"op\":\"login\"")),
            "production private socket sent login"
        );
        let fills = Rc::new(Cell::new(0));
        let observed = fills.clone();
        venue.base().on(
            "fill",
            Rc::new(move |_, _| {
                observed.set(observed.get() + 1);
                Ok(())
            }),
        );
        let scene = Self {
            venue,
            runtime,
            store,
            wire,
            socket,
            fills,
        };
        scene.frame("login", json!({"result":true}));
        scene
    }
    fn frame(&self, channel: &str, data: Value) {
        self.socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"ch":channel,"data":data}).to_string(),
            )));
        for _ in 0..32 {
            self.runtime.pump();
        }
        assert!(
            self.runtime.errors.borrow().is_empty(),
            "{:?}",
            self.runtime.errors.borrow()
        );
    }
    fn response(&self, path: &str, data: Value) {
        self.wire
            .bodies
            .lock()
            .unwrap()
            .insert(path.into(), json!({"code":0,"msg":"Success","data":data}));
    }
    fn advance(&self, millis: f64) {
        self.runtime.now.set(self.runtime.now.get() + millis);
        for _ in 0..32 {
            self.runtime.pump();
        }
        assert!(
            self.runtime.errors.borrow().is_empty(),
            "{:?}",
            self.runtime.errors.borrow()
        );
    }
    fn requests(&self, path: &str) -> usize {
        self.wire
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.url == path)
            .count()
    }
}
impl Drop for Scene {
    fn drop(&mut self) {
        self.venue.dispose().unwrap();
    }
}
fn order(event: &str, status: &str, filled: &str) -> Value {
    json!({"event":event,"symbol":"BTCUSDT","orderId":"native-frame-order","positionType":"CROSS",
        "positionMode":"HEDGE","side":"Buy","effect":"GTC","type":"Limit","qty":"2","price":"123.5",
        "ctime":"2025-04-05T16:05:39.369Z","mtime":"2025-04-05T16:05:40.369Z","leverage":"20",
        "orderStatus":status,"fee":"0","averagePrice":"124.5","dealAmount":filled,"clientId":"native-frame-client"})
}
#[test]
fn private_order_frames_create_update_and_cancel_the_real_store_order() {
    let s = Scene::new();
    s.frame("order", order("CREATE", "NEW", "0"));
    assert_eq!(s.store.orders().len(), 1);
    assert_eq!(s.store.orders()[0].id, "native-frame-order");
    assert_eq!(s.store.orders()[0].price, 123.5);
    assert_eq!(s.store.orders()[0].remaining, 2.0);
    s.frame("order", order("UPDATE", "PARTIALLY_FILLED", "0.5"));
    assert_eq!(s.store.orders()[0].filled, 0.5);
    assert_eq!(s.store.orders()[0].remaining, 1.5);
    s.frame("order", order("CLOSE", "CANCELED", "0.5"));
    assert_eq!(s.store.orders()[0].status, OrderStatus::Canceled);
    assert_eq!(s.store.orders()[0].remaining, 0.0);
    assert_eq!(
        s.fills.get(),
        0,
        "TS private order updates do not emit fills"
    );
}
#[test]
fn private_filled_order_frame_closes_store_order_without_inventing_source_fill() {
    let s = Scene::new();
    s.frame("order", order("CREATE", "NEW", "0"));
    s.frame("order", order("CLOSE", "FILLED", "2"));
    let orders = s.store.orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "native-frame-order");
    assert_eq!(orders[0].status, OrderStatus::Closed);
    assert_eq!(orders[0].filled, 2.0);
    assert_eq!(orders[0].remaining, 0.0);
    assert_eq!(s.fills.get(), 0, "TS Bitunix has no private fill emitter");
}
#[test]
fn private_position_frame_updates_position_id_then_fetches_authoritative_position() {
    let s = Scene::new();
    s.response(
        POSITIONS,
        json!([{"positionId":"frame-position","symbol":"BTCUSDT","qty":"0.5",
        "entryValue":"50","side":"BUY","positionMode":"HEDGE","marginMode":"CROSS","leverage":"20",
        "fee":"0","funding":"0","realizedPNL":"0","margin":"2.5","unrealizedPNL":"1",
        "liqPrice":"50","avgOpenPrice":"100","ctime":"1700000000000","mtime":"1700000000000"}]),
    );
    s.frame(
        "position",
        json!({"event":"OPEN","positionId":"frame-position","symbol":"BTCUSDT",
        "side":"LONG","qty":"999","leverage":"99","entryValue":"999999"}),
    );
    let initial = s
        .store
        .positions()
        .iter()
        .find(|p| p.side == PositionSide::Long)
        .unwrap()
        .clone();
    assert_eq!(
        initial.contracts, 0.0,
        "position frame itself only patches the ID"
    );
    assert_eq!(
        initial.info.unwrap().get("positionId").unwrap(),
        "frame-position"
    );
    assert_eq!(s.requests(POSITIONS), 0);
    s.advance(999.0);
    assert_eq!(s.requests(POSITIONS), 0, "source one-second debounce");
    s.advance(1.0);
    assert_eq!(s.requests(POSITIONS), 1);
    let actual = s
        .store
        .positions()
        .iter()
        .find(|p| p.side == PositionSide::Long)
        .unwrap()
        .clone();
    assert_eq!(actual.contracts, 0.5);
    assert_eq!(actual.entry_price, 100.0);
    assert_eq!(actual.leverage, 20.0);
    assert_eq!(actual.unrealized_pnl, 1.0);
    assert!(s.store.loaded().positions);
    assert_eq!(s.fills.get(), 0);
}
#[test]
fn private_balance_frame_debounces_rest_balance_and_retains_positions() {
    let s = Scene::new();
    s.response(
        ACCOUNT,
        json!({"marginCoin":"USDT","available":"7","frozen":"1","margin":"2",
        "positionMode":"HEDGE","crossUnrealizedPNL":"0.5","isolationUnrealizedPNL":"0.25"}),
    );
    let before = s.store.balance().clone();
    let positions = s.store.positions().to_vec();
    s.frame(
        "balance",
        json!({"coin":"USDT","available":"999","frozen":"999","margin":"999"}),
    );
    assert_eq!(
        *s.store.balance(),
        before,
        "balance frame values are not authoritative"
    );
    s.advance(999.0);
    assert_eq!(s.requests(ACCOUNT), 0);
    s.advance(1.0);
    assert_eq!(s.requests(ACCOUNT), 1);
    let balance = s.store.balance();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (10.0, 7.0, 2.0, 0.75)
    );
    assert_eq!(s.store.positions().to_vec(), positions);
    assert!(s.store.loaded().balance && s.store.loaded().positions);
    assert_eq!(s.fills.get(), 0);
}
