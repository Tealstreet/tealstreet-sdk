use super::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct HeldOrders {
    malformed: bool,
    released: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<String>>>,
}
impl RestAdapter for HeldOrders {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(request.url);
        let released = self.released.clone();
        let malformed = self.malformed;
        Box::pin(std::future::poll_fn(move |_| {
            if !released.load(Ordering::SeqCst) {
                return Poll::Pending;
            }
            Poll::Ready(Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(if malformed {
                    b"[null]".to_vec()
                } else {
                    b"[]".to_vec()
                }),
            }))
        }))
    }
}

#[test]
fn production_native_retry_flags_read_live_owner_after_held_http() {
    for venue in [Venue::Futures, Venue::PM] {
        for malformed in [false, true] {
            let runtime = Rc::new(Runtime::default());
            runtime.now.set(20_000.0);
            let released = Arc::new(AtomicBool::new(false));
            let requests = Arc::new(Mutex::new(vec![]));
            let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
                runtime.clone(),
                Arc::new(HeldOrders {
                    malformed,
                    released: released.clone(),
                    requests: requests.clone(),
                }),
                Rc::new(|_, _| panic!("no external provider")),
                None,
                safe_cex_exchange::native::NativeHttpEvents::default(),
            );
            let store = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
            Store::update(store.as_ref(), StoreUpdate {
                orders: Some(vec![serde_json::from_value(json!({
                    "id":"existing-order", "symbol":"BTCUSDT", "status":"open", "type":"limit", "side":"buy",
                    "price":10, "amount":2, "filled":0, "remaining":2, "reduceOnly":false, "lastUpdate":17
                })).unwrap()]),
                ..Default::default()
            }, false).unwrap();
            let owner = ExchangeOwner::new(
                venue,
                serde_json::from_value(json!(if venue == Venue::PM {
                    "binancepm"
                } else {
                    "binance"
                }))
                .unwrap(),
                serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"}))
                    .unwrap(),
                store.clone(),
                dependencies,
            )
            .unwrap();
            assert!(!owner.retry.snapshot_stale.get());
            assert!(!owner.retry.is_disposed.get());
            let mut pending = Box::pin(owner.retry_orders());
            assert!(
                pending
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            let count = if venue == Venue::Futures { 2 } else { 1 };
            assert_eq!(
                requests.lock().unwrap().len(),
                count,
                "actual native order requests started"
            );
            assert!(
                requests
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|url| url.contains("signature=") && url.contains("timestamp=")),
                "requests traverse actual native signing"
            );
            released.store(true, Ordering::SeqCst);
            run(&runtime, pending).unwrap();
            assert_eq!(
                owner.retry.snapshot_stale.get(),
                malformed,
                "retry flag reads current completed snapshot state"
            );
            assert_eq!(
                store.loaded().unwrap().orders,
                !malformed,
                "live stale flag controls the real snapshot publication"
            );
            let ids: Vec<_> = store
                .orders()
                .unwrap()
                .iter()
                .map(|order| order.id.clone())
                .collect();
            assert_eq!(
                ids,
                if malformed {
                    vec!["existing-order".to_owned()]
                } else {
                    vec![]
                },
                "real retry preserves stale Store rows and publishes a successful empty snapshot"
            );
            owner.dispose().unwrap();
            assert!(
                owner.retry.is_disposed.get(),
                "retry flag observes actual Base disposal"
            );
        }
    }
}

#[test]
fn production_flag_cell_conversion_retains_each_owned_boolean() {
    use safe_cex_venue_binance::class_flags::LiveFlag;
    for initial in [false, true] {
        let flag = LiveFlag::from(Cell::new(initial));
        assert_eq!(
            flag.get(),
            initial,
            "actual Cell conversion preserves the initial boolean"
        );
        flag.set(!initial);
        assert_eq!(
            flag.get(),
            !initial,
            "actual Cell flag retains its subsequent write"
        );
    }
}
