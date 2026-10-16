//! Registry construction must preserve the source's fail-closed Phase-0 scaffold.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use safe_cex_venue_txflow::types::TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

#[test]
fn txflow_native_registry_preserves_phase_zero_refusal_without_io() {
    support::run_local(async {
        for testnet in [false, true] {
            let h = support::venue_harness(
                ExchangeName::Txflow,
                Rc::new(|_| panic!("TxFlow Phase-0 must not send HTTP")),
                json!({"key":"", "secret":"", "testnet":testnet}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert_eq!(
                h.exchange.name(),
                &ExchangeName::Txflow,
                "REGISTRY TxFlow identity"
            );
            assert_eq!(h.exchange.base().options().testnet, Some(testnet));
            let errors = Rc::new(RefCell::new(vec![]));
            let observed = errors.clone();
            h.exchange.base().on(
                "error",
                Rc::new(move |event, value| {
                    observed
                        .borrow_mut()
                        .push((event.to_owned(), value.clone()));
                    Ok(())
                }),
            );
            let pending = h.exchange.start();
            assert_eq!(
                errors.borrow().len(),
                1,
                "Phase-0 rejection emits before polling"
            );
            assert_eq!(
                pending.await.unwrap_err().message,
                TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE,
                "REGISTRY TxFlow retains exact source refusal"
            );
            assert_eq!(errors.borrow().len(), 1, "Phase-0 emits once");
            tokio::task::yield_now().await;
            assert!(h.seen.borrow().is_empty(), "Phase-0 must not send HTTP");
            assert!(
                h.peers.borrow().is_empty(),
                "Phase-0 must not dial a socket"
            );
            assert!(h.store.markets().is_empty());
            h.exchange.dispose().expect("dispose scaffold");
        }
    });
}
