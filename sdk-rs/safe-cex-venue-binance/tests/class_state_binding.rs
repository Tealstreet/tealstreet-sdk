#![cfg(feature = "native")]
use safe_cex_venue_binance::{exchange_classes, native_host, private_session};
#[path = "support/inverse_rest_runtime.rs"]
mod host;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::Store;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{rc::Rc, sync::Arc};
struct Parts {
    store: Rc<safe_cex_exchange::native_store::DefaultStore>,
    http: Arc<host::Http>,
    options: safe_cex_types::ExchangeOptions,
    deps: Option<exchange_classes::ExchangeDependencies>,
}
fn parts(testnet: bool) -> Parts {
    let runtime = Rc::new(host::Runtime::default());
    let http = Arc::new(host::Http::default());
    let store = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
    let mut options = host::options();
    options.testnet = Some(testnet);
    let deps = native_host::native_dependencies(
        runtime,
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    Parts {
        store,
        http,
        options,
        deps: Some(deps),
    }
}
#[test]
fn concrete_class_constructors_retain_native_family_and_children() {
    macro_rules! check {
        ($class:ident,$name:ident,$family:literal,$label:literal) => {{
            for testnet in [false, true] {
                let mut p = parts(testnet);
                let subject = exchange_classes::$class::new(
                    ExchangeName::$name,
                    p.options.clone(),
                    p.store.clone(),
                    p.deps.take().unwrap(),
                )
                .unwrap();
                assert_eq!(format!("{:?}", subject.venue), $label);
                assert_eq!(subject.base().options().testnet, Some(testnet));
                assert_eq!(subject.base().name(), &ExchangeName::$name);
                assert!(Rc::ptr_eq(&subject.public, &subject.state.public_socket()));
                assert!(Rc::ptr_eq(
                    &subject.private,
                    &subject.state.private_socket()
                ));
                assert!(p.http.sent.lock().unwrap().is_empty());
                assert!(std::ptr::addr_eq(
                    subject.base().store.as_ref(),
                    p.store.as_ref() as &dyn Store
                ));
                assert!(Rc::ptr_eq(&subject.0, std::ops::Deref::deref(&subject)));
                subject.dispose().unwrap();
            }
        }};
    }
    check!(BinanceExchange, Binance, "futures", "Futures");
    check!(BinancePMExchange, Binancepm, "pm", "PM");
    check!(BinanceSpotExchange, Binancespot, "spot", "Spot");
}
#[test]
fn actual_state_worker_confirmation_changes_only_confirmed_symbol() {
    for hedged in [false, true] {
        let mut p = parts(false);
        let owner = exchange_classes::ExchangeOwner::new(
            private_session::Venue::Futures,
            ExchangeName::Binance,
            p.options.clone(),
            p.store.clone(),
            p.deps.take().unwrap(),
        )
        .unwrap();
        owner
            .state
            .after_method_called_in_worker("ignored", &[json!(true), json!("UNRELATED")])
            .unwrap();
        assert!(p.store.fetched_leverage().unwrap().is_empty());
        owner
            .state
            .after_method_called_in_worker("changePositionMode", &[json!(hedged), json!("BTCUSDT")])
            .unwrap();
        assert_eq!(p.store.options().unwrap().is_hedged, hedged);
        let rows = p.store.fetched_leverage().unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows.contains_key("BTCUSDT"));
        assert_eq!(
            rows["BTCUSDT"].position_mode,
            Some(if hedged {
                safe_cex_types::PositionMode::Hedged
            } else {
                safe_cex_types::PositionMode::OneWay
            })
        );
        owner.base.dispose().unwrap();
    }
}
#[test]
#[ignore = "executes maintained TypeScript classes and real Axios/Store with isolated dependencies"]
fn maintained_class_state_profiles_remain_current() {
    let output = std::process::Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/class-state-source.cjs"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 36, "maintained full-constructor case floor");
    let contracts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../safe-cex-parity/accepted-set/contracts/safe-cex-venue-binance.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for row in rows {
        let class = match row["stem"].as_str().unwrap() {
            "binance" => "BinanceExchange",
            "binancepm" => "BinancePMExchange",
            "binancespot" => "BinanceSpotExchange",
            _ => panic!("unreviewed source class"),
        };
        let method = match row["method"].as_str().unwrap() {
            "fetchPositions" => "fetch_positions",
            "fetchOrders" => "fetch_orders",
            "cancelSymbolOrders" => "cancel_symbol_orders",
            "setLeverage" => "set_leverage",
            "changePositionMode" => "change_position_mode",
            "changeMarginType" => "change_margin_type",
            _ => panic!("unreviewed source method"),
        };
        let entries =
            contracts[format!("safe_cex_venue_binance::exchange_classes::{class}")]["entrypoints"]
                .as_array()
                .unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry["api"]["name"] == method)
            .unwrap();
        let case = &entry["cases"][usize::from(row["variant"].as_bool().unwrap())];
        let expected = case["output"].get("wire").cloned().unwrap_or(json!([]));
        assert_eq!(
            row["requests"], expected,
            "source/native authored wire expectation {class}.{method}"
        );
    }
}
