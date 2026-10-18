use super::*;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::{RestAdapter, RestRequest, RestResponse, RestResponseBody, socket::SocketFrame};
use safe_cex_venue_binance::public_listeners::Book;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

struct HeldDepth {
    released: AtomicBool,
    requests: Mutex<Vec<String>>,
}
impl RestAdapter for HeldDepth {
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(request.url);
        Box::pin(std::future::poll_fn(move |_| {
            if !self.released.load(Ordering::SeqCst) { return Poll::Pending; }
            Poll::Ready(Ok(RestResponse {
                status: 200, reason: None, headers: Default::default(),
                body: RestResponseBody::Encoded(br#"{"lastUpdateId":100,"bids":[["99.25","2.5"]],"asks":[["101.5","3.75"]]}"#.to_vec()),
            }))
        }))
    }
}
struct PublicSocket {
    events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
    sent: RefCell<Vec<String>>,
}
impl WsSocket for PublicSocket {
    fn ready_state(&self) -> u8 { 1 }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| events.borrow_mut().pop_front().map_or(Poll::Pending, |event| Poll::Ready(Ok(event)))))
    }
    fn send(&self, text: &str) -> Result<(), ExchangeError> { self.sent.borrow_mut().push(text.into()); Ok(()) }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool { false }
}

#[test]
fn production_native_public_listener_aliases_wait_buffer_reuse_and_unsubscribe() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let runtime = Rc::new(Runtime::default());
        let socket = Rc::new(PublicSocket {
            events: Rc::new(RefCell::new(std::collections::VecDeque::from([WsEvent::Open]))),
            sent: RefCell::default(),
        });
        *runtime.dial_socket.borrow_mut() = Some(socket.clone());
        let http = Arc::new(HeldDepth { released: AtomicBool::new(false), requests: Mutex::default() });
        let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(), http.clone(), Rc::new(|_, _| panic!("unexpected external data")), None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let name = match venue { Venue::Futures => "binance", Venue::PM => "binancepm", Venue::Spot => "binancespot" };
        let owner = ExchangeOwner::new(venue, serde_json::from_value(json!(name)).unwrap(),
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()), dependencies).unwrap();
        let observed: Rc<RefCell<Vec<Book>>> = Default::default();
        let copy = observed.clone();
        let cleanup = owner.public.listen_order_book("BTCUSDT".into(), Rc::new(move |book| {
            copy.borrow_mut().push(book); Ok(())
        })).unwrap();
        assert!(socket.sent.borrow().is_empty(), "wait Action must delay subscription until connected");
        assert!(http.requests.lock().unwrap().is_empty());
        owner.public_socket().connect_and_subscribe().unwrap();
        for _ in 0..8 { runtime.pump(); }
        runtime.sleep_clock.set(100.0);
        for _ in 0..8 { runtime.pump(); }
        assert!(socket.sent.borrow().iter().any(|wire| wire.contains("SUBSCRIBE") && wire.contains("btcusdt@depth")), "connected timer Action installs real Parser {venue:?}");
        let push = |id: u64, amount: &str| {
            socket.events.borrow_mut().push_back(WsEvent::Message(SocketFrame::Text(
                json!({"e":"depthUpdate","s":"BTCUSDT","U":id,"u":id,"pu":id-1,"b":[["99.25",amount]],"a":[]}).to_string()
            )));
            for _ in 0..12 { runtime.pump(); }
        };
        push(101, "4.25");
        push(102, "5.5");
        assert!(observed.borrow().is_empty(), "Snapshot/Job must stay pending while native HTTP is held {venue:?}");
        assert_eq!(http.requests.lock().unwrap().len(), 1, "one in-flight Snapshot despite buffered frames");
        let url = http.requests.lock().unwrap()[0].clone();
        assert!(url.contains("/depth") && url.contains("symbol=BTCUSDT") && url.contains("limit=1000"), "actual public snapshot request {url}");
        http.released.store(true, Ordering::SeqCst);
        for _ in 0..16 { runtime.pump(); }
        assert_eq!(observed.borrow().len(), 1, "fulfilled Job applies both buffered updates once {venue:?}");
        let first = observed.borrow()[0].clone();
        {
            let book = first.borrow();
            assert_eq!(book.bids.len(), 1); assert_eq!(book.asks.len(), 1);
            for (actual, expected) in [(book.bids[0].price,99.25_f64),(book.bids[0].amount,5.5),(book.bids[0].total,5.5),
                (book.asks[0].price,101.5),(book.asks[0].amount,3.75),(book.asks[0].total,3.75)] {
                assert_eq!(actual.to_bits(), expected.to_bits(), "native Book field bits {venue:?}");
            }
        }
        push(103,"6.75");
        assert_eq!(observed.borrow().len(),2,"SyncState keeps the next delta contiguous {venue:?}");
        assert!(Rc::ptr_eq(&first, &observed.borrow()[1]), "same native Book Rc survives callback generations {venue:?}");
        assert_eq!(first.borrow().bids[0].amount.to_bits(),6.75_f64.to_bits());
        assert_eq!(http.requests.lock().unwrap().len(),1,"live SyncState avoids a spurious refetch");
        cleanup().unwrap();
        assert!(socket.sent.borrow().iter().any(|wire| wire.contains("UNSUBSCRIBE") && wire.contains("btcusdt@depth")), "Unsubscribe forwards through real socket {venue:?}");
        assert!(first.borrow().bids.is_empty() && first.borrow().asks.is_empty(), "cleanup resets the retained Book");
        push(104,"9");
        assert_eq!(observed.borrow().len(),2,"removed Parser cannot receive later frames");
        assert!(runtime.reported.borrow().is_empty());
        owner.dispose().unwrap();
    }
}
