//! The assembled private class, not the obsolete oracle-only throttle copy.
use super::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use std::sync::{Arc, Mutex};
struct Orders(Arc<Mutex<Vec<String>>>);
impl RestAdapter for Orders {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.lock().unwrap().push(request.url);
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(b"[]".to_vec()),
            })
        })
    }
}
#[test]
fn actual_private_order_throttle_waits_then_publishes_loaded_snapshot() {
    for venue in [Venue::Futures, Venue::PM] {
        let runtime = Rc::new(Runtime::default());
        let requests = Arc::new(Mutex::new(vec![]));
        let deps = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(),
            Arc::new(Orders(requests.clone())),
            Rc::new(|_, _| panic!("unexpected external provider")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let store: Rc<dyn Store> = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
        let mut loaded = store.loaded().unwrap().clone();
        loaded.tickers = true;
        store
            .update(
                StoreUpdate {
                    loaded: Some(loaded),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        let name = if venue == Venue::PM {
            safe_cex_types::ExchangeName::Binancepm
        } else {
            safe_cex_types::ExchangeName::Binance
        };
        let owner = ExchangeOwner::new(
            venue,
            name,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            store,
            deps,
        )
        .unwrap();
        let before = owner.base.store.update_ids().unwrap().orders;
        owner.private.throttled_orders().unwrap();
        runtime.pump();
        assert_eq!(
            &*runtime.sleeps.borrow(),
            &[10_000],
            "source debounce duration"
        );
        assert!(requests.lock().unwrap().is_empty());
        assert_eq!(owner.base.store.update_ids().unwrap().orders, before);
        assert!(!owner.base.store.loaded().unwrap().orders);
        runtime.now.set(10_000.0);
        runtime.sleep_clock.set(10_000.0);
        for _ in 0..32 {
            runtime.pump();
        }
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "regular and conditional order requests");
        for url in requests.iter() {
            assert!(
                url.contains("timestamp=") && url.contains("signature="),
                "configured signed HTTP: {url}"
            );
        }
        assert_eq!(owner.base.store.update_ids().unwrap().orders, before + 1.0);
        assert!(
            owner.base.store.loaded().unwrap().orders,
            "real class publishes loaded.orders"
        );
        assert!(
            owner.base.store.loaded().unwrap().tickers,
            "loaded sibling survives the real Store update"
        );
        assert!(matches!(
            *owner.private.orders_timer.borrow(),
            EventValue::Undefined
        ));
        owner.dispose().unwrap();
    }
}
