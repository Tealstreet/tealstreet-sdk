//! Concrete receiver witnesses use the Scope of the production constructor.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{
    classes::{ClassHost, Kind, Return},
    private_accounts::AccountHost,
    spot_cancel::SpotCancelHost,
};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        promise::await_js,
        value::{Js, Thrown, error_object},
    },
};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn post_cancel_uses_the_signed_spot_client_and_original_id() {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "POST",
            "/0/private/CancelOrder",
            serde_json::json!({"error":[],"result":{"count":1}}),
        );
    });
    for id in ["first/交易", "second+identity"] {
        let pending = SpotCancelHost::post_cancel(
            &h.exchange.rt,
            "/0/private/CancelOrder",
            "txid",
            Rc::new(Js::str(id)),
        )
        .unwrap();
        h.drive(Box::pin(async move {
            pending.await.map_err(|e| e.to_exchange_error())
        }))
        .unwrap();
    }
    let calls = h.http.calls("/0/private/CancelOrder");
    assert_eq!(calls.len(), 2);
    for (call, id) in calls.iter().zip(["first/交易", "second+identity"]) {
        assert_eq!(call.method, "POST");
        assert_eq!(call.headers["API-Key"], "fixture-key");
        assert!(!call.headers["API-Sign"].as_str().unwrap().is_empty());
        let data = call.data.as_str().unwrap();
        let encoded = if id == "first/交易" {
            "first%2F%E4%BA%A4%E6%98%93"
        } else {
            "second%2Bidentity"
        };
        assert!(
            data.split('&')
                .any(|pair| pair == format!("txid={encoded}"))
        );
        assert!(data.split('&').any(|pair| pair.starts_with("nonce=")));
    }
}

#[test]
fn spot_error_message_keeps_truthy_text_and_falsy_fallback() {
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    for message in [
        Js::utf16(&[0xd800]),
        Js::Number(f64::INFINITY),
        Js::object(vec![("own", Js::Undefined)]),
        Js::str(""),
        Js::Number(-0.0),
        Js::Number(f64::NAN),
        Js::Bool(false),
        Js::Null,
        Js::Undefined,
    ] {
        let error = Js::object(vec![("message", message.clone())]);
        let selected =
            SpotCancelHost::error_message(&h.exchange.rt, &Thrown::new(error), "fallback").unwrap();
        if message.truthy() {
            assert!(selected.strict_equals(&message));
        } else {
            assert_eq!(selected.to_display(), "fallback");
        }
    }
    let cause = error_object("RangeError", "message getter");
    let original = cause.clone();
    let error = Js::object(vec![]);
    error
        .define_accessor(
            "message",
            Some(Rc::new(move |_| Err(Thrown::new(original.clone())))),
            None,
        )
        .unwrap();
    assert!(
        SpotCancelHost::error_message(&h.exchange.rt, &Thrown::new(error), "fallback")
            .unwrap_err()
            .value
            .strict_equals(&cause)
    );
}

#[test]
fn spot_emit_error_reaches_the_real_base_reporter() {
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    h.exchange.base().on_args(
        "error",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    SpotCancelHost::emit_error(
        &h.exchange.rt,
        &Js::str("native cancel"),
        "CancelOrders Error",
    )
    .unwrap();
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(
        events.borrow()[0][0].to_display(),
        "CancelOrders Error: native cancel"
    );
    assert!(
        h.runtime
            .native_console
            .borrow()
            .iter()
            .any(|(level, args)| level == "error"
                && args.len() == 2
                && args[1].to_display() == "native cancel")
    );
}

#[test]
fn spot_cancel_preserves_the_caught_utf16_message_before_rethrow() {
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    let error = error_object("TypeError", "");
    error
        .set("message", Js::utf16(&[0xd800, b'X' as u16, 0xdc00]))
        .unwrap();
    let original = error.clone();
    let order = Js::object(vec![]);
    order
        .define_accessor(
            "id",
            Some(Rc::new(move |_| Err(Thrown::new(original.clone())))),
            None,
        )
        .unwrap();
    let promise = h
        .exchange
        .rt
        .this
        .call_method("cancelOrders", vec![Js::array(vec![order])])
        .unwrap();
    let runtime = h.exchange.rt.exchange_runtime();
    let failure = h
        .drive(Box::pin(async move {
            await_js(&runtime, promise)
                .await
                .map_err(|e| e.to_exchange_error())
        }))
        .unwrap_err();
    let retained = Thrown::from(failure).value;
    assert_eq!(
        retained.get("message").unwrap().as_str().unwrap().as_ref(),
        &[0xd800, b'X' as u16, 0xdc00],
        "Spot cancel rethrow must preserve native UTF16 message"
    );
    assert!(
        !retained.strict_equals(&error),
        "source creates a fresh Error"
    );
    assert_eq!(retained.get("name").unwrap().to_display(), "Error");
    let calls = h.runtime.native_console.borrow();
    let args = &calls
        .iter()
        .find(|(level, args)| {
            level == "error" && args.len() == 2 && args[0].to_display() == "exchange error"
        })
        .unwrap()
        .1;
    assert_eq!(
        args[1].as_str().unwrap().as_ref(),
        &[0xd800, b'X' as u16, 0xdc00],
        "report before rethrow retains exact message"
    );
    assert!(h.http.requests.borrow().is_empty());
}

#[test]
fn position_id_keeps_native_utf16_and_coercion_errors() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    assert_eq!(
        rt.position_id(&Js::utf16(&[b'X' as u16, 0xd800]), "short")
            .unwrap()
            .as_str()
            .unwrap()
            .as_ref(),
        &[
            b'X' as u16,
            0xd800,
            b':' as u16,
            b's' as u16,
            b'h' as u16,
            b'o' as u16,
            b'r' as u16,
            b't' as u16
        ]
    );
    assert_eq!(
        rt.position_id(&Js::Number(-0.0), "long")
            .unwrap()
            .to_display(),
        "0:long"
    );
    let object = Js::object(vec![]);
    let error = error_object("RangeError", "symbol coercion");
    let cause = error.clone();
    object
        .define(
            "toString",
            Js::function("toString", move |_, _| Err(Thrown::new(cause.clone()))),
        )
        .unwrap();
    assert!(
        rt.position_id(&object, "long")
            .unwrap_err()
            .value
            .strict_equals(&error)
    );
}

#[test]
fn optional_preference_uses_parent_maps_and_original_keys() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let socket = Js::object(vec![("parent", rt.this.clone())]);
    let scope = rt.with_this(socket);
    for (kind, property) in [
        ("margin", "marginTypePreferences"),
        ("leverage", "leverageSettings"),
    ] {
        let map = rt.this.get(property).unwrap();
        let key = Js::utf16(&[0xd800]);
        let value = Js::object(vec![
            ("amount", Js::Number(f64::INFINITY)),
            ("missing", Js::Undefined),
        ]);
        map.call_method("set", vec![key.clone(), value.clone()])
            .unwrap();
        assert!(
            scope
                .optional_preference(kind, &key)
                .unwrap()
                .strict_equals(&value)
        );
        assert!(
            scope
                .optional_preference(kind, &Js::str("missing"))
                .unwrap()
                .is_undefined()
        );
        rt.this.set(property, Js::Null).unwrap();
        assert!(
            scope
                .optional_preference(kind, &key)
                .unwrap()
                .is_undefined()
        );
        rt.this.set(property, Js::Number(3.0)).unwrap();
        assert!(scope.optional_preference(kind, &key).is_err());
        rt.this.set(property, map).unwrap();
    }
}

#[test]
fn imported_values_match_the_constructed_spot_endpoint_tables() {
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    let rt = &h.exchange.rt;
    for (import, property, expected) in [
        (
            "GRAFANA_ENDPOINTS_EVERY",
            "grafanaEndpointsEvery",
            crate::spot_types::GRAFANA_ENDPOINTS_EVERY,
        ),
        (
            "GRAFANA_ENDPOINTS_PERIODIC",
            "grafanaEndpointsPeriodic",
            crate::spot_types::GRAFANA_ENDPOINTS_PERIODIC,
        ),
    ] {
        for value in [
            rt.imported(import).unwrap(),
            Rc::new(rt.this.get(property).unwrap()),
        ] {
            let rows = value
                .array_values()
                .expect("fixture array property reads")
                .unwrap();
            assert_eq!(rows.len(), expected.len());
            for (actual, expected) in rows.iter().zip(expected) {
                assert_eq!(actual.to_display(), *expected);
            }
        }
    }
    assert_eq!(
        rt.imported("UNDECLARED")
            .unwrap_err()
            .value
            .get("name")
            .unwrap()
            .to_display(),
        "ReferenceError"
    );
}

#[test]
fn member_dispatch_uses_the_supplied_receiver_and_argument_cells() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let observed = Rc::new(RefCell::new(vec![]));
    let seen = observed.clone();
    let cell = Js::object(vec![("raw", Js::Number(f64::NAN))]);
    let result = cell.clone();
    let socket = Js::object(vec![]);
    socket
        .define(
            "listenOrderBook",
            Js::function("listenOrderBook", move |this, args| {
                seen.borrow_mut().push((this, args));
                Ok(result.clone())
            }),
        )
        .unwrap();
    let receiver = Js::object(vec![("publicWebsocket", socket.clone())]);
    let a = Js::utf16(&[0xd800]);
    let b = Js::object(vec![("own", Js::Undefined)]);
    let member = rt
        .member(
            Kind::FuturesExchange,
            "listenOrderBook",
            "safe-cex-venue-kraken/src/stream_adapters.rs::futures_listen_order_book",
        )
        .unwrap();
    let Return::Value(value) = member(
        Rc::new(receiver),
        vec![Rc::new(a.clone()), Rc::new(b.clone())],
    )
    .unwrap() else {
        panic!("sync delegate");
    };
    assert!(value.strict_equals(&cell));
    let calls = observed.borrow();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].0.strict_equals(&socket));
    assert_eq!(calls[0].1.len(), 2);
    assert!(calls[0].1[0].strict_equals(&a));
    assert!(calls[0].1[1].strict_equals(&b));
}

#[test]
fn prototype_installs_native_this_args_and_future_on_the_matching_socket_kind() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let first = rt
        .super_socket(Kind::FuturesPublic, Rc::new(rt.this.clone()))
        .unwrap();
    let second = rt
        .super_socket(Kind::FuturesPublic, Rc::new(rt.this.clone()))
        .unwrap();
    let other = rt
        .super_socket(Kind::SpotPublic, Rc::new(rt.this.clone()))
        .unwrap();
    let seen = Rc::new(Cell::new(0));
    let count = seen.clone();
    let expected = (*first).clone();
    let cell = Js::object(vec![("raw", Js::Number(-0.0))]);
    let returned = cell.clone();
    rt.prototype(
        Kind::FuturesPublic,
        "nativeWitness",
        Rc::new(move |this, args| {
            assert!(this.strict_equals(&expected));
            assert_eq!(args.len(), 1);
            count.set(count.get() + 1);
            Ok(Return::Value(args[0].clone()))
        }),
    )
    .unwrap();
    assert!(!first.has_own("nativeWitness"));
    assert!(
        first
            .get("nativeWitness")
            .unwrap()
            .strict_equals(&second.get("nativeWitness").unwrap())
    );
    assert!(other.get("nativeWitness").unwrap().is_undefined());
    assert!(
        first
            .call_method("nativeWitness", vec![cell.clone()])
            .unwrap()
            .strict_equals(&cell)
    );
    assert_eq!(seen.get(), 1);
    rt.prototype(
        Kind::FuturesPublic,
        "nativeAsync",
        Rc::new(move |_, _| {
            let result = returned.clone();
            Ok(Return::Future(Box::pin(async move { Ok(Rc::new(result)) })))
        }),
    )
    .unwrap();
    let pending = first.call_method("nativeAsync", vec![]).unwrap();
    let runtime = rt.exchange_runtime();
    let value = h
        .drive(Box::pin(async move {
            await_js(&runtime, pending)
                .await
                .map_err(|e| e.to_exchange_error())
        }))
        .unwrap();
    assert!(value.strict_equals(&cell));
    let error = error_object("RangeError", "prototype throw");
    let cause = error.clone();
    rt.prototype(
        Kind::FuturesPublic,
        "nativeThrow",
        Rc::new(move |_, _| Err(Thrown::new(cause.clone()))),
    )
    .unwrap();
    assert!(
        first
            .call_method("nativeThrow", vec![])
            .unwrap_err()
            .value
            .strict_equals(&error)
    );
}

#[test]
fn spot_cancel_selects_native_messages_before_reporting_and_fresh_error_coercion() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/additional-receiver-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["profiles"], 12);
    let messages = [
        Js::utf16(&[0xd800, b'X' as u16, 0xdc00]),
        Js::utf16(&[0xdc00]),
        Js::Number(f64::INFINITY),
        Js::Number(7.0),
        Js::Bool(true),
        Js::Undefined,
        Js::Null,
        Js::Bool(false),
        Js::Number(0.0),
        Js::Number(-0.0),
        Js::Number(f64::NAN),
        Js::str(""),
    ];
    for message in messages {
        let h = harness(ExchangeName::Krakenspot, options(), |_| {});
        let original = Js::object(vec![("message", message.clone())]);
        let cause = original.clone();
        let gets = Rc::new(Cell::new(0));
        let counted = gets.clone();
        let order = Js::object(vec![]);
        order
            .define_accessor(
                "id",
                Some(Rc::new(move |_| {
                    counted.set(counted.get() + 1);
                    Err(Thrown::new(cause.clone()))
                })),
                None,
            )
            .unwrap();
        let calls = Rc::new(RefCell::new(vec![]));
        let seen = calls.clone();
        h.exchange
            .rt
            .this
            .set(
                "emitAndLogError",
                Js::function("emitAndLogError", move |_, args| {
                    seen.borrow_mut().push(args);
                    Ok(Js::Undefined)
                }),
            )
            .unwrap();
        let pending = h
            .exchange
            .rt
            .this
            .call_method("cancelOrders", vec![Js::array(vec![order])])
            .unwrap();
        let runtime = h.exchange.rt.exchange_runtime();
        let rejected = h
            .drive(Box::pin(
                async move { Ok(await_js(&runtime, pending).await) },
            ))
            .unwrap()
            .unwrap_err();
        let selected = if message.truthy() {
            message
        } else {
            Js::str("Failed to cancel orders")
        };
        assert_eq!(gets.get(), 1);
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(calls.borrow()[0].len(), 2);
        assert!(calls.borrow()[0][0].strict_equals(&selected));
        assert_eq!(calls.borrow()[0][1].to_display(), "CancelOrders Error");
        assert!(!rejected.value.strict_equals(&original));
        assert_eq!(rejected.value.get("name").unwrap().to_display(), "Error");
        assert_eq!(
            rejected
                .value
                .get("message")
                .unwrap()
                .as_str()
                .unwrap()
                .as_ref(),
            selected.to_js_string().unwrap().as_ref()
        );
        assert!(h.http.requests.borrow().is_empty());
    }
}
