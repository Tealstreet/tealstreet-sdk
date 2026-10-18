use super::{CoinbaseDependencies, CoinbaseExchange};
use crate::{
    cdp_signing::CdpSigningCache, starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::runtime::ExchangeRuntime;
use safe_cex_exchange::{
    js_runtime::{
        client,
        value::{Js, from_typed, to_retained_input, to_typed},
    },
    native::native_runtime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, ExchangeOptions, Order};
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc};
#[path = "retained_info.rs"]
mod observation;

#[test]
fn real_merge_binding_keeps_undefined_and_nonfinite_info() {
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-create.cjs"
        ))
        .arg("--retained-merge")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = native_runtime();
        let now = runtime.now_ms();
        let http = client::rest_http_host(runtime.clone(), Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
        let store = Rc::new(DefaultStore::new());
        let options: ExchangeOptions = serde_json::from_value(serde_json::json!({"key":"synthetic","secret":"synthetic"})).unwrap();
        let venue = CoinbaseExchange::new(ExchangeName::Coinbasespot, options, store.clone(), CoinbaseDependencies {
            runtime, http, external_public_data: None, signing_cache: Rc::new(CdpSigningCache::default()), local_wall_time_to_utc: Rc::new(|v| v),
        }).unwrap();
        let order: Order = serde_json::from_value(serde_json::json!({"id":"fresh","symbol":"BTCUSD","status":"open","type":"limit","side":"buy","price":12,"amount":3,"filled":0,"remaining":3,"reduceOnly":false,"lastUpdate":now})).unwrap();
        let order = from_typed(&order).unwrap();
        order.define("info", Js::object(vec![
            ("missing", Js::Undefined),
            ("array", Js::array(vec![Js::Number(f64::NAN), Js::Number(f64::INFINITY), Js::Undefined])),
        ])).unwrap();
        let order: Order = to_typed(&order).unwrap();
        store.add_or_update_order(order).unwrap();
        let merged = venue.host.merge_rest(Frame::Array(Rc::new(RefCell::new(vec![]))))
            .expect("real merge binding must retain undefined array info");
        let merged = super::super::value::to_js(&merged);
        let info = merged.get("0").unwrap().get("info").unwrap();
        observation::assert_info(Some(&to_retained_input(&info).unwrap()), &expected, "merged raw info");
    });
}
