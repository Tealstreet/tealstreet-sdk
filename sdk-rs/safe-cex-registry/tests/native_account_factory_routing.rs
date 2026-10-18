//! Production registry/reqwest witnesses for source safeCexAxiosCreate callers.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, time::Duration};

async fn check_factory(name: ExchangeName, mut options: Value) {
    options["overrideProxyUrl"] = json!("https://ffi-proxy.invalid");
    options["tealstreetTunnel"] = json!(false);
    let denied = Rc::new(Cell::new(0));
    let failures = denied.clone();
    let source = name.clone();
    let h = support::venue_harness(
        name.clone(),
        Rc::new(move |request| {
            if request.path.ends_with("/time") {
                return Some(if source == ExchangeName::Polymarket {
                    json!(1700000000)
                } else {
                    json!({"serverTime":1700000000000u64})
                });
            }
            failures.set(failures.get() + 1);
            Some(json!({"__status":401,"body":{"message":"Invalid JWT"}}))
        }),
        options,
        None,
        safe_cex_exchange::native::native_runtime(),
    )
    .await;
    let result = tokio::time::timeout(Duration::from_secs(5), h.exchange.validate_account())
        .await
        .unwrap_or_else(|_| panic!("{name:?} account read timed out: {:?}", h.seen.borrow()));
    assert!(
        denied.get() > 0,
        "{name:?} validation reached real loopback HTTP: {result:?}"
    );
    let requests = h.seen.borrow();
    for request in requests
        .iter()
        .filter(|request| !request.path.ends_with("/time"))
    {
        assert_eq!(
            request.path.matches("https://").count(),
            1,
            "{name:?} applies the source factory route exactly once: {request:?}"
        );
    }
    assert_eq!(
        h.store.request_stats().unwrap().auth,
        denied.get() as f64,
        "{name:?} records each actual HTTP auth failure on its owning Store"
    );
    h.exchange.dispose().unwrap();
}

macro_rules! factory_case {
    ($test:ident, $name:ident, $options:expr) => {
        #[test]
        fn $test() {
            support::run_local(check_factory(ExchangeName::$name, $options));
        }
    };
}
factory_case!(
    okx_source_factory_routes_and_counts,
    Okx,
    json!({"key":"native-key","secret":support::SECRET,"passphrase":"native-passphrase"})
);
factory_case!(
    polymarket_source_factory_routes_and_counts,
    Polymarket,
    json!({"key":format!("0x{}", "01".repeat(32)),"secret":"unused"})
);
factory_case!(
    asterdex_source_factory_routes_and_counts,
    Asterdex,
    json!({"key":"native-key","secret":support::SECRET})
);
factory_case!(
    phemex_source_factory_routes_and_counts,
    Phemex,
    json!({"key":"native-key","secret":support::SECRET})
);
factory_case!(
    woo_source_factory_routes_and_counts,
    Woo,
    json!({"key":"native-key","secret":support::SECRET})
);
factory_case!(
    orderly_source_factory_routes_and_counts,
    Kodiakfi,
    json!({"key":"native-key","secret":"11111111111111111111111111111111","applicationId":"account-native"})
);
factory_case!(
    breakoutprop_source_factory_routes_and_counts,
    Breakoutprop,
    json!({"key":"k","secret":"p","applicationId":"default"})
);
