//! Exercise the source private class through the production realm/socket bridge.
use safe_cex_exchange::js_runtime::{value::Js};
#[allow(dead_code)]
#[path = "../../tests/support/binding_runtime.rs"]
mod host;

use crate::{
    binding::{HyperliquidDependencies, RuntimeClock},
    exchange::HyperliquidExchange,
    rate_limit::RateLimiter,
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{runtime::ExchangeSigners, socket_lifecycle::SocketSlot};
use serde_json::json;
use std::{rc::Rc, sync::Arc};

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn private_no_super_dispose_closes_wire_without_force_closing_base() {
    let runtime = Rc::new(host::Host::default());
    let options = serde_json::from_value(json!({
        "key": "0x14791697260e4c9a71f18484c9f997b308e59325",
        "secret": "",
        "applicationId": "0x14791697260e4c9a71f18484c9f997b308e59325"
    }))
    .unwrap();
    let exchange = HyperliquidExchange::new(
        options,
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: runtime.clone(),
            http: host::Routes::new(vec![]),
            limiter: Rc::new(RateLimiter::new(
                Arc::new(RuntimeClock::new(runtime.clone())),
                1200.0,
                60000.0,
            )),
            signers: ExchangeSigners::default(),
            external_public_data: None,
        },
    )
    .unwrap();
    let (mut realm, instance) = {
        let binding = exchange.binding.borrow();
        let binding = binding.as_ref().unwrap();
        (binding.realm.clone(), binding.instance.clone())
    };
    let private = realm.get_str(&instance, "privateWebsocket").unwrap();
    realm
        .call_method(&private, "connectAndSubscribe", &[Js::Bool(false)])
        .unwrap();
    runtime.settle();
    let base = exchange.base.get_socket(SocketSlot::Private).unwrap();
    let wire = runtime.sockets.borrow()[0].clone();
    assert_eq!(wire.closes.get(), 0);
    assert!(!base.is_force_closed());
    realm.call_method(&private, "dispose", &[]).unwrap();
    assert_eq!(
        wire.closes.get(),
        1,
        "source ws.close closes the current native wire"
    );
    assert!(
        !base.is_force_closed(),
        "source private dispose has no super call and must not set forceClosed"
    );
    assert!(
        !exchange.base.is_disposed(),
        "private no-super dispose leaves its parent active"
    );

    // The actual source class is loaded independently; changing its ws.close
    // to forceClose must fail that program's behavioral assertions.
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/private-close-source.cjs"
    );
    let source = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(source["forceClosed"], false);
    assert_eq!(source["isDisposed"], false);
    assert_eq!(source["closes"], wire.closes.get());
    let control = std::process::Command::new("node")
        .arg(script)
        .env("SAFE_CEX_HL_CLOSE_CONTROL", "1")
        .output()
        .unwrap();
    assert!(
        !control.status.success(),
        "controlled source must reject forceClose"
    );
    assert!(
        String::from_utf8_lossy(&control.stderr)
            .contains("actual private dispose does not call forceClose")
    );
}
