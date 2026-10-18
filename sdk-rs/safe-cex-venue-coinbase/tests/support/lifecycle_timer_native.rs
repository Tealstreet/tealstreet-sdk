use super::{CoinbaseDependencies, CoinbaseExchange};
use crate::{cdp_signing::CdpSigningCache, exchange_lifecycle::{LifecycleHost, LifecycleTimer}};
use safe_cex_exchange::{js_runtime::client, native::native_runtime};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

#[test]
fn lifecycle_timer_alias_is_created_and_cancelled_by_the_real_runtime() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let runtime = native_runtime();
            let http = client::rest_http_host(
                runtime.clone(),
                Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            );
            let exchange = CoinbaseExchange::new(
                name,
                serde_json::from_value(json!({"key":"synthetic","secret":"synthetic"})).unwrap(),
                Rc::new(DefaultStore::new()),
                CoinbaseDependencies {
                    runtime,
                    http,
                    external_public_data: None,
                    signing_cache: Rc::new(CdpSigningCache::default()),
                    local_wall_time_to_utc: Rc::new(|v| v),
                },
            )
            .unwrap();
            let fired = Rc::new(Cell::new(0));
            let callback_count = fired.clone();
            let timer: LifecycleTimer = exchange
                .host
                .set_timeout(
                    Rc::new(move || {
                        callback_count.set(callback_count.get() + 1);
                        Ok(())
                    }),
                    5.0,
                )
                .unwrap();
            let captured = timer.clone();
            assert!(Rc::ptr_eq(&timer, &captured));
            exchange.host.clear_timeout(captured).unwrap();
            tokio::time::sleep(Duration::from_millis(15)).await;
            assert_eq!(fired.get(), 0);
            let callback_count = fired.clone();
            let _timer: LifecycleTimer = exchange
                .host
                .set_timeout(
                    Rc::new(move || {
                        callback_count.set(callback_count.get() + 1);
                        Ok(())
                    }),
                    0.0,
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                while fired.get() == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(fired.get(), 1);
        }
    });
}
