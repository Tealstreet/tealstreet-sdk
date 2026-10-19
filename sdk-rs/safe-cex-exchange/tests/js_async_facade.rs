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
    let store = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
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
                    await_js(&(_context.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>), checked).await.unwrap();
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
            await_js(
                &(_context.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
                result.unwrap(),
            )
            .await
            .unwrap();
            rows.push(serde_json::json!({"kind":kind,"coerced":coerced.get()}));
        }
        assert_eq!(serde_json::json!(rows), expected["guarded"]);
    });
}

#[test]
fn native_base_input_secondary_keeps_capabilities_and_original_failures() {
    use safe_cex_exchange::js_runtime::value::arg;
    use std::cell::RefCell;
    let expected = oracle();
    let mut observations = vec![];
    for mode in ["single", "split", "throw"] {
        let (this, _shared, _owner) = facade(false);
        let cause = Js::object(vec![("units", Js::utf16(&[0xd800]))]);
        let original = cause.clone();
        let payload = Js::object(vec![("opaque", Js::Undefined)]);
        let alias = payload.clone();
        let calls = Rc::new(Cell::new(0));
        let invoked = calls.clone();
        let seen = Rc::new(RefCell::new(vec![]));
        let options_seen = seen.clone();
        let create = Js::function("createApi", move |_, args| {
            invoked.set(invoked.get() + 1);
            if mode == "throw" {
                return Err(Thrown::new(original.clone()));
            }
            let options = arg(&args, 0);
            options_seen.borrow_mut().push(options.clone());
            let tag = options.get("key")?;
            Ok(Js::object(vec![
                ("tag", tag),
                (
                    "get",
                    Js::function("get", |_, args| Ok(arg(&args, 0))),
                ),
                (
                    "post",
                    Js::function("post", |this, args| {
                        Ok(Js::array(vec![this.get("tag")?, arg(&args, 0)]))
                    }),
                ),
            ]))
        });
        let signer = Js::function("host capability", |_, _| Ok(Js::Undefined));
        let options = Js::object(vec![
            ("key", Js::str("primary")),
            ("secret", Js::str("secret")),
            (
                "secondary_key",
                Js::str(if mode == "single" { "" } else { "secondary" }),
            ),
            ("remoteSigner", signer.clone()),
            ("payloadSigner", signer.clone()),
            ("unread", Js::utf16(&[0xd800])),
        ]);
        let result = this.call_method(
            "createXhrWithSecondaryKey",
            vec![options.clone(), create, Js::Undefined],
        );
        let observation = match result {
            Err(error) => serde_json::json!({"mode":mode,"calls":calls.get(),"causeIdentity":error.value.same_reference(&cause)}),
            Ok(xhr) => {
                assert!(seen.borrow()[0].same_reference(&options));
                for opts in seen.borrow().iter() {
                    assert!(opts.get("remoteSigner").unwrap().same_reference(&signer));
                    assert!(opts.get("payloadSigner").unwrap().same_reference(&signer));
                    assert_eq!(opts.get("unread").unwrap().to_js_string().unwrap().as_ref(), &[0xd800]);
                }
                let get = xhr.call_method("get", vec![payload]).unwrap();
                let post = xhr.call_method("post", vec![alias.clone()]).unwrap();
                serde_json::json!({"mode":mode,"calls":calls.get(),
                    "shared":xhr.same_reference(&this.get("_primaryXhr").unwrap()) && xhr.same_reference(&this.get("_secondaryXhr").unwrap()),
                    "getIdentity":get.same_reference(&alias),"postIdentity":post.get("1").unwrap().same_reference(&alias),
                    "postTag":post.get("0").unwrap().to_display(),"units":options.get("unread").unwrap().to_js_string().unwrap().as_ref()})
            }
        };
        observations.push(observation);
    }
    assert_eq!(serde_json::json!(observations), expected["secondaryOwners"]);
}

#[test]
fn native_base_input_overlay_runs_the_installed_callable() {
    let expected = oracle();
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let mut rows = vec![];
        for mode in ["null", "undefined", "identity"] {
            let (this, shared, owner) = facade(false);
            let calls = Rc::new(Cell::new(0));
            let invoked = calls.clone();
            let overlay = match mode {
                "null" => Js::Null,
                "undefined" => Js::Undefined,
                _ => Js::function("overlay", move |_, args| {
                    invoked.set(invoked.get() + 1);
                    Ok(args[0].clone())
                }),
            };
            let result = this
                .call_method(
                    "loadAccountMarketOverlay",
                    vec![Js::function("load", move |_, _| Ok(overlay.clone()))],
                )
                .unwrap();
            let result = await_js(
                &(shared.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
                result,
            )
            .await
            .unwrap();
            let markets = owner.base().adapt_external_markets(vec![]);
            rows.push(serde_json::json!({"mode":mode,"undefinedResult":result.is_undefined(),"calls":calls.get(),"length":markets.len()}));
        }
        assert_eq!(serde_json::json!(rows), expected["overlayOwners"]);
    });
}

#[test]
fn native_base_input_worker_rpc_keeps_the_reached_wire_fields() {
    use safe_cex_exchange::js_runtime::value::to_json;
    use std::cell::RefCell;
    let expected = oracle();
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let mut rows = vec![];
        for success in [true, false] {
            let (this, shared, owner) = facade(false);
            let request = Rc::new(RefCell::new(Js::Undefined));
            let captured = request.clone();
            owner.base().on("callMethod", Rc::new(move |_, value| {
                *captured.borrow_mut() = value.clone();
                Ok(())
            }));
            let result = this.call_method("callMethodInWorker", vec![Js::str("removeOrders"), Js::array(vec![Js::object(vec![("id", Js::str("wire"))])])]).unwrap();
            let captured = request.borrow().clone();
            assert_eq!(to_json(&captured.get("args").unwrap()).unwrap(), serde_json::json!([[{"id":"wire"}]]));
            assert_eq!(captured.get("method").unwrap().to_display(), "removeOrders");
            let response = if success { Js::object(vec![("removed", Js::str("wire"))]) } else { Js::object(vec![("name", Js::str("WorkerFailure")),("message", Js::str("rejected")),("code", Js::Number(409.)),("stack",Js::str("worker stack"))]) };
            owner.base().emit("callMethod:done",Js::object(vec![("requestId",captured.get("requestId").unwrap()),("success",Js::Bool(success)),("result",response)])).unwrap();
            let result=await_js(&(shared.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),result).await;
            rows.push(match result {
                Ok(value)=>serde_json::json!({"success":true,"removed":value.get("removed").unwrap().to_display()}),
                Err(error)=>serde_json::json!({"success":false,"name":error.value.get("name").unwrap().to_display(),"message":error.message(),"code":error.value.get("code").unwrap().to_number().unwrap(),"stack":error.value.get("stack").unwrap().to_display()})
            });
            // The installed converter must retain the exact original coercion failure.
            let cause = Js::object(vec![("opaque",Js::utf16(&[0xd800]))]);
            let thrown=cause.clone();
            let argument=Js::object(vec![("toString",Js::function("throw",move |_,_|Err(Thrown::new(thrown.clone()))))]);
            let rejected=this.call_method("getOrFetchPositionsForSymbol",vec![argument]).unwrap();
            let error=await_js(&(shared.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),rejected).await.unwrap_err();
            assert!(error.value.same_reference(&cause));
        }
        assert_eq!(serde_json::json!(rows),expected["workerOwners"]);
    });
}

#[test]
fn native_base_overlay_propagates_original_throw_instead_of_markets() {
    let expected = oracle();
    assert_eq!(expected["overlayThrow"]["propagates"], true);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let (this, shared, owner) = facade(false);
        let cause = Thrown::error("RangeError", "overlay source failure").value;
        cause.set("message", Js::utf16(&[0x62, 0xd800])).unwrap();
        let original = cause.clone();
        let overlay = Js::function("throwing overlay", move |_, _| {
            Err(Thrown::new(original.clone()))
        });
        let loading = this.call_method("loadAccountMarketOverlay", vec![Js::function("load", move |_, _| Ok(overlay.clone()))]).unwrap();
        await_js(&(shared.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>), loading).await.unwrap();
        let outcome = owner.base().adapt_external_markets(vec![]);
        assert!(format!("{outcome:?}").starts_with("Err("), "OVERLAY_ORIGINAL_THROW: source rejects instead of returning unadapted markets");
    });
}
