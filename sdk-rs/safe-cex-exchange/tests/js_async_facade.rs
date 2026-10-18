#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        client, globals,
        promise::await_js,
        scope::ScopeShared,
        value::{Js, Thrown, from_json},
    },
    native::native_runtime,
};
use std::{cell::Cell, process::Command, rc::Rc, sync::Arc};

struct Owner(Rc<BaseExchange>, ExchangeCapabilities);
impl Exchange for Owner {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.1
    }
}
fn oracle() -> serde_json::Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_async_facade_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn facade(public: bool) -> (Js, Rc<ScopeShared<()>>, Rc<dyn Exchange>) {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        store.clone(),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    let context = shared.sockets.clone();
    let options = serde_json::json!({"key": if public {""} else {"key"}, "secret":"secret"});
    let (this, base) = base_exchange::create(
        &context,
        BaseExchangeParts {
            name: safe_cex_types::ExchangeName::Bybit,
            options: serde_json::from_value(options.clone()).unwrap(),
            store,
            settings: ExchangeSettings::default(),
            external_public_data: None,
        },
        Js::str("bybit"),
        from_json(&options),
        Js::object(vec![]),
        |_| Ok(()),
    )
    .unwrap();
    assert!(shared.base.set(base.clone()).is_ok());
    *shared.exchange.borrow_mut() = this.clone();
    let owner: Rc<dyn Exchange> = Rc::new(Owner(base, ExchangeCapabilities::default()));
    *context.venue.borrow_mut() = Some(Rc::downgrade(&owner));
    (this, shared, owner)
}

#[test]
fn native_async_facade_returns_rejections_for_pre_await_failures() {
    let expected = oracle();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let (this, _context, _owner) = facade(false);
        let mut rows = vec![];
        for method in ["fetchAndSetOrders", "fetchAndSetPrivateSnapshot", "getOrFetchPositionsForSymbol"] {
            let cause = Js::object(vec![("witness", Js::str(method))]);
            let argument = if method == "fetchAndSetOrders" {
                Js::Number(1.0)
            } else {
                let thrown = cause.clone();
                Js::object(vec![(
                    if method == "fetchAndSetPrivateSnapshot" { "valueOf" } else { "toString" },
                    Js::function("throwing conversion", move |_, _| Err(Thrown::new(thrown.clone()))),
                )])
            };
            match this.call_method(method, vec![argument]) {
                Err(_) => rows.push(serde_json::json!({"method":method,"syncThrow":true,"promise":false,"caught":false})),
                Ok(value) => {
                    let promise = matches!(value, Js::Promise(_));
                    let caught = Rc::new(Cell::new(false));
                    let recorded = caught.clone();
                    let original = cause.clone();
                    let checked = value.call_method("catch", vec![Js::function("catch", move |_, args| {
                        if method != "fetchAndSetOrders" {
                            assert!(args[0].same_reference(&original), "original conversion error");
                        }
                        recorded.set(true);
                        Ok(Js::Undefined)
                    })]).unwrap();
                    await_js(&_context.runtime, checked).await.unwrap();
                    rows.push(serde_json::json!({"method":method,"syncThrow":false,"promise":promise,"caught":caught.get()}));
                }
            }
        }
        assert_eq!(serde_json::json!(rows), expected["rows"]);
    });
}

#[test]
fn native_snapshot_guards_precede_tries_coercion() {
    let expected = oracle();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let mut rows = vec![];
        for kind in ["public", "disposed"] {
            let (this, _context, owner) = facade(kind == "public");
            if kind == "disposed" {
                owner.dispose().unwrap();
            }
            let coerced = Rc::new(Cell::new(false));
            let observed = coerced.clone();
            let argument = Js::object(vec![(
                "valueOf",
                Js::function("unreached", move |_, _| {
                    observed.set(true);
                    Err(Thrown::error("Error", "unreached"))
                }),
            )]);
            let result = this.call_method("fetchAndSetPrivateSnapshot", vec![argument]);
            assert!(
                result.is_ok(),
                "snapshot guard must return its async result"
            );
            await_js(&_context.runtime, result.unwrap()).await.unwrap();
            rows.push(serde_json::json!({"kind":kind,"coerced":coerced.get()}));
        }
        assert_eq!(serde_json::json!(rows), expected["guarded"]);
    });
}
