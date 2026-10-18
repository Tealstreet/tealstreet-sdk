//! Exercise the source private class through the production realm/socket bridge.
use safe_cex_exchange::js_runtime::value::Js;
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
fn both_constructed_families_keep_original_js_arguments_and_start_async_calls_eagerly() {
    use safe_cex_exchange::{exchange::Exchange, js_runtime::promise::Promise};
    use std::cell::{Cell, RefCell};
    for spot in [false, true] {
        let runtime = Rc::new(host::Host::default());
        let dependencies = HyperliquidDependencies {
            runtime: runtime.clone(),
            http: host::Routes::new(vec![]),
            limiter: Rc::new(RateLimiter::new(
                Arc::new(RuntimeClock::new(runtime.clone())),
                1200.0,
                60_000.0,
            )),
            signers: ExchangeSigners::default(),
            external_public_data: None,
        };
        let options = serde_json::from_value(json!({"key":"", "secret":""})).unwrap();
        let store = Rc::new(DefaultStore::new());
        let (exchange, binding): (Rc<dyn Exchange>, _) = if spot {
            let exchange =
                crate::spot_binding::HyperliquidSpotExchange::new(options, store, dependencies)
                    .unwrap();
            let binding = exchange.binding.borrow().as_ref().unwrap().clone();
            (exchange, binding)
        } else {
            let exchange = HyperliquidExchange::new(options, store, dependencies).unwrap();
            let binding = exchange.binding.borrow().as_ref().unwrap().clone();
            (exchange, binding)
        };
        let mut context = binding.realm.clone();
        let captured = Rc::new(RefCell::new(Vec::new()));
        let observed = captured.clone();
        let _cleanup = exchange.base().on_args(
            "identity",
            Rc::new(move |_, args| {
                *observed.borrow_mut() = args.to_vec();
                Ok(())
            }),
        );
        let array = Js::array(vec![Js::Number(7.0)]);
        let nested = Js::object(vec![("array", array.clone())]);
        let expected_receiver = nested.clone();
        let callback = Js::function("callback", move |receiver, _| {
            assert!(receiver.same_reference(&expected_receiver));
            Ok(receiver)
        });
        let cause = Js::object(vec![("cause", Js::utf16(&[0xd800]))]);
        let iterable = Js::array(vec![nested.clone()]);
        let iterator = iterable.iterator().unwrap();
        let args = vec![
            nested.clone(),
            array.clone(),
            callback.clone(),
            cause.clone(),
            iterator.clone(),
        ];
        let emitter = binding.instance.get("emitter").unwrap();
        emitter
            .call_method(
                "emit",
                std::iter::once(Js::str("identity"))
                    .chain(args.clone())
                    .collect(),
            )
            .unwrap();
        assert_eq!(captured.borrow().len(), args.len());
        for (actual, original) in captured.borrow().iter().zip(&args) {
            assert!(actual.same_reference(original));
        }
        array.set("0", Js::Number(8.0)).unwrap();
        assert_eq!(
            captured.borrow()[0]
                .get("array")
                .unwrap()
                .get("0")
                .unwrap()
                .as_f64(),
            Some(8.0)
        );
        assert!(
            captured.borrow()[2]
                .call(nested.clone(), vec![])
                .unwrap()
                .same_reference(&nested)
        );
        assert!(
            captured.borrow()[4]
                .call_method("next", vec![])
                .unwrap()
                .get("value")
                .unwrap()
                .same_reference(&nested)
        );
        let returned_cause = cause.clone();
        let throwing = context.native("throwing", 0, move |_, _, _| {
            Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                returned_cause.clone(),
            ))
        });
        assert!(
            context
                .call(&throwing, &nested, &[])
                .unwrap_err()
                .value
                .same_reference(&cause)
        );
        let prefix = Rc::new(Cell::new(0));
        let suffix = Rc::new(Cell::new(0));
        let before = prefix.clone();
        let after = suffix.clone();
        let receiver = binding.instance.clone();
        binding
            .instance
            .define(
                "setLeverage",
                context.native("setLeverage", 2, move |context, this, args| {
                    assert!(this.same_reference(&receiver));
                    assert_eq!(args[0].to_display(), "BTC");
                    before.set(before.get() + 1);
                    let after = after.clone();
                    let sleep = context.runtime().sleep(5);
                    Ok(Js::Promise(Promise::spawn(
                        &context.runtime(),
                        Box::pin(async move {
                            sleep
                                .await
                                .map_err(safe_cex_exchange::js_runtime::value::Thrown::from)?;
                            after.set(after.get() + 1);
                            Ok(Js::Undefined)
                        }),
                    )))
                }),
            )
            .unwrap();
        let waiter = exchange.set_leverage("BTC".into(), 3.0, None);
        assert_eq!(
            prefix.get(),
            1,
            "public async prefix must run before the waiter is polled: spot={spot}"
        );
        assert_eq!(suffix.get(), 0);
        drop(waiter);
        runtime.advance(5.0);
        assert_eq!(
            suffix.get(),
            1,
            "dropping the waiter does not cancel source async work: spot={spot}"
        );
        exchange.dispose().unwrap();
    }
}

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
