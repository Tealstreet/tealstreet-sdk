#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    native::native_replay_runtime,
    runtime::{ExchangeRuntime, ExchangeTask},
};
use std::{cell::RefCell, rc::Rc};

#[test]
fn controlled_clock_drives_actual_native_timers_and_virtual_clock_in_order() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        assert!(
            !safe_cex_exchange::websocket::WsRuntime::frame_receipts_enabled(
                safe_cex_exchange::native::native_runtime().as_ref()
            )
        );
        assert!(safe_cex_exchange::websocket::WsRuntime::frame_receipts_enabled(runtime.as_ref()));
        let clock = runtime.replay_clock.as_ref().unwrap();
        safe_cex_exchange::replay_clock::activate(clock).unwrap();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut tasks: Vec<Box<dyn ExchangeTask>> = Vec::new();
        for (id, delay) in [(1, 250), (2, 100), (3, 250), (4, 200)] {
            let sleep = runtime.sleep(delay);
            let events = events.clone();
            let runtime_at = runtime.clone();
            tasks.push(runtime.spawn_local(Box::pin(async move {
                sleep.await.unwrap();
                events
                    .borrow_mut()
                    .push((id, runtime_at.now_ms(), runtime_at.monotonic_now_ms()));
            })));
        }
        // Cancellation before the timer's first poll must not stall advance.
        tasks[3].cancel();
        tokio::task::yield_now().await;
        safe_cex_exchange::replay_clock::advance(clock, 99.5)
            .await
            .unwrap();
        assert!(events.borrow().is_empty());
        safe_cex_exchange::replay_clock::advance(clock, 250.75)
            .await
            .unwrap();
        assert_eq!(
            *events.borrow(),
            vec![
                (2, 1_800_000_000_100.0, 100.0),
                (1, 1_800_000_000_250.0, 250.0),
                (3, 1_800_000_000_250.0, 250.0)
            ]
        );
        assert_eq!(
            safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
            1_800_000_000_250.0
        );
        assert!(
            safe_cex_exchange::replay_clock::advance(clock, 200.0)
                .await
                .is_err()
        );
        assert!(
            safe_cex_exchange::replay_clock::advance(clock, f64::NAN)
                .await
                .is_err()
        );
        safe_cex_store::virtual_clock::set_thread_host_time(None);
    });
}
