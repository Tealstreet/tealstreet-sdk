// Actual native venue methods and terminal host only.
#![cfg(feature = "native")]
#[path = "support/inverse_rest_runtime.rs"]
mod host;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_types::ExchangeName;
use safe_cex_venue_binance::exchange_classes::{
    BinanceExchange, BinancePMExchange, BinanceSpotExchange,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
fn drive<T>(runtime: &host::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
    }
    panic!("bounded inherited request did not settle")
}
async fn invoke<E: Exchange>(
    subject: &E,
    method: &str,
    variant: bool,
) -> Result<(), ExchangeError> {
    let symbol = if variant { "ETHUSDT" } else { "BTCUSDT" }.to_owned();
    match method {
        "fetchSymbolMaxLeverage" => subject.fetch_symbol_max_leverage(symbol).await.map(|_| ()),
        "fetchPublicTrades" => subject
            .fetch_public_trades(symbol, Some(if variant { 0. } else { 17. }))
            .await
            .map(|_| ()),
        "setLeverage" => {
            subject
                .set_leverage(symbol, if variant { 23. } else { 17. }, None)
                .await
        }
        "changePositionMode" => subject.change_position_mode(variant, Some(symbol)).await,
        "changeMarginType" => subject.change_margin_type(variant, Some(symbol)).await,
        _ => panic!("unreviewed inherited method"),
    }
}
fn observe(stem: &str, method: &str, variant: bool) -> Value {
    let runtime = Rc::new(host::Runtime::default());
    let http = Arc::new(host::Http::default());
    let deps = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let store = Rc::new(DefaultStore::new());
    let options = host::options();
    fn call<E: Exchange>(
        subject: E,
        runtime: &host::Runtime,
        http: &host::Http,
        stem: &str,
        method: &str,
        variant: bool,
    ) -> Value {
        let events = Rc::new(RefCell::new(0usize));
        let seen = events.clone();
        subject.base().on(
            "error",
            Rc::new(move |_, _| {
                *seen.borrow_mut() += 1;
                Ok(())
            }),
        );
        let error = drive(runtime, invoke(&subject, method, variant))
            .expect_err("actual inherited rejection");
        assert_eq!(error.name, "Error", "{stem}.{method}");
        assert_eq!(error.message, "Not implemented", "{stem}.{method}");
        let requests = http.sent.lock().unwrap().len();
        assert_eq!(requests, 0);
        let event_count = *events.borrow();
        assert_eq!(event_count, 0);
        let reports = runtime.reports.borrow().len();
        assert_eq!(reports, 0);
        subject.dispose().unwrap();
        let symbol = if variant { "ETHUSDT" } else { "BTCUSDT" };
        let args = match method {
            "changePositionMode" | "changeMarginType" => json!([variant, symbol]),
            "setLeverage" => json!([symbol, if variant { 23 } else { 17 }]),
            "fetchPublicTrades" => json!([symbol, if variant { 0 } else { 17 }]),
            _ => json!([symbol]),
        };
        json!({"stem":stem,"method":method,"args":args,"error":error.message,"error_name":error.name,"http":requests,"events":event_count,"reports":reports})
    }
    match stem {
        "binance" => call(
            BinanceExchange::new(ExchangeName::Binance, options, store, deps).unwrap(),
            &runtime,
            &http,
            stem,
            method,
            variant,
        ),
        "binancepm" => call(
            BinancePMExchange::new(ExchangeName::Binancepm, options, store, deps).unwrap(),
            &runtime,
            &http,
            stem,
            method,
            variant,
        ),
        "binancespot" => call(
            BinanceSpotExchange::new(ExchangeName::Binancespot, options, store, deps).unwrap(),
            &runtime,
            &http,
            stem,
            method,
            variant,
        ),
        _ => panic!("unreviewed family"),
    }
}
fn observations() -> Vec<Value> {
    let mut out = vec![];
    for (stem, methods) in [
        ("binance", vec!["fetchSymbolMaxLeverage"]),
        (
            "binancepm",
            vec!["fetchSymbolMaxLeverage", "fetchPublicTrades"],
        ),
        (
            "binancespot",
            vec![
                "fetchSymbolMaxLeverage",
                "fetchPublicTrades",
                "setLeverage",
                "changePositionMode",
                "changeMarginType",
            ],
        ),
    ] {
        for method in methods {
            for variant in [false, true] {
                out.push(observe(stem, method, variant));
            }
        }
    }
    out
}
#[test]
fn futures_max_leverage_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binance", "fetchSymbolMaxLeverage", variant);
    }
}
#[test]
fn pm_max_leverage_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancepm", "fetchSymbolMaxLeverage", variant);
    }
}
#[test]
fn pm_public_trades_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancepm", "fetchPublicTrades", variant);
    }
}
#[test]
fn spot_max_leverage_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancespot", "fetchSymbolMaxLeverage", variant);
    }
}
#[test]
fn spot_public_trades_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancespot", "fetchPublicTrades", variant);
    }
}
#[test]
fn spot_leverage_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancespot", "setLeverage", variant);
    }
}
#[test]
fn spot_position_mode_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancespot", "changePositionMode", variant);
    }
}
#[test]
fn spot_margin_type_preserves_inherited_refusal() {
    for variant in [false, true] {
        observe("binancespot", "changeMarginType", variant);
    }
}
#[test]
#[ignore = "executes maintained TypeScript constructors with isolated dependencies"]
fn maintained_inherited_refusals_match_native() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/inherited-refusal-source.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "maintained source failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 16);
    assert_eq!(observations(), source);
}
