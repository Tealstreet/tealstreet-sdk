//! The canonical drop path completes shared cleanup after a venue cleanup error.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    native::native_runtime,
    native_final_drop,
    runtime::ExchangeError,
};
use safe_cex_store::{StoreUpdate, store::DefaultStore};
use safe_cex_types::ExchangeName;
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[test]
fn native_final_drop_completes_after_both_venue_and_store_errors() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        let store = Rc::new(DefaultStore::new());
        let base = BaseExchange::new(
            ExchangeName::Backpack,
            serde_json::from_str(r#"{"key":"","secret":""}"#).unwrap(),
            store.clone(),
            native_runtime(),
            ExchangeSettings::default(),
        );
        safe_cex_store::Store::update(
            store.as_ref(),
            StoreUpdate {
                loaded: Some(safe_cex_types::StoreDataLoaded {
                    orders: true,
                    balance: false,
                    markets: false,
                    tickers: false,
                    positions: false,
                }),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let called = Cell::new(0);
        let errors = Arc::new(AtomicUsize::new(0));
        let seen = errors.clone();
        let unsubscribe = store.subscribe(Arc::new(move |_, _| {
            seen.fetch_add(1, Ordering::SeqCst);
            Err(safe_cex_types::StoreSubscriberError::new(
                "reset observer failed",
            ))
        }));
        let operation = base.start_owned_operation(std::future::pending());
        native_final_drop(&base, || {
            assert!(
                base.is_disposed(),
                "shared flag precedes source-independent venue cleanup"
            );
            assert!(
                operation.is_active(),
                "venue cleanup precedes shared operation closure"
            );
            called.set(called.get() + 1);
            Err(ExchangeError::new("venue cleanup failed"))
        });
        assert_eq!(called.get(), 1);
        assert!(base.is_disposed());
        assert!(
            !operation.is_active(),
            "venue error cannot suppress shared cancellation"
        );
        assert!(
            !store.loaded().orders,
            "Store reset committed despite its failing observer"
        );
        assert_eq!(errors.load(Ordering::SeqCst), 1);
        unsubscribe();
    });
}
