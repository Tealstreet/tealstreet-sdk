#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    exchange::{AccountIdentityPatch, ExchangeSettings, IdentityFieldPatch},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown, WriteObserver, from_json, to_json},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::Arc,
};

fn scope() -> Rc<ScopeShared<()>> {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    shared
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_write_observers_source.cjs"
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

#[test]
fn every_successful_options_write_is_immediately_visible_to_native_account_readers() {
    let expected = source();
    let shared = scope();
    let options = from_json(&json!({"key":"key", "secret":"secret"}));
    let (facade, base) = base_exchange::create(
        &shared.sockets,
        BaseExchangeParts {
            name: safe_cex_types::ExchangeName::Hyperliquid,
            options: serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
            store: shared.store.clone(),
            settings: ExchangeSettings::default(),
            external_public_data: None,
        },
        Js::str("hyperliquid"),
        options.clone(),
        Js::object(vec![]),
        |_| Ok(()),
    )
    .unwrap();
    let identity = Rc::downgrade(&base);
    let calls = Rc::new(Cell::new(0));
    let recorded = calls.clone();
    options
        .set_write_observer(Some(Rc::new(move |receiver, key, value| {
            let key = String::from_utf16(key).unwrap();
            assert_eq!(
                to_json(&receiver.get(&key)?)?,
                to_json(value)?,
                "WRITE_OBSERVER committed before publication"
            );
            let field = match value {
                Js::Undefined => IdentityFieldPatch::Remove,
                Js::String(value) => IdentityFieldPatch::Set(String::from_utf16(value).unwrap()),
                _ => {
                    return Err(Thrown::type_error(
                        "Native account identity must be a string",
                    ));
                }
            };
            let mut patch = AccountIdentityPatch {
                application_id: IdentityFieldPatch::Unchanged,
                vault: IdentityFieldPatch::Unchanged,
            };
            match key.as_str() {
                "applicationId" => patch.application_id = field,
                "vault" => patch.vault = field,
                _ => unreachable!(),
            }
            identity
                .upgrade()
                .expect("live account")
                .patch_account_identity(patch);
            recorded.set(recorded.get() + 1);
            Ok(())
        })))
        .unwrap();
    let alias = facade.get("options").unwrap();
    assert_eq!(alias.same_reference(&options), expected["sameObject"]);
    let mut rows = vec![];
    for (key, value) in [
        ("applicationId", Js::str("master")),
        ("vault", Js::str("vault")),
        ("applicationId", Js::str("")),
        ("vault", Js::Undefined),
        ("applicationId", Js::str("账户😀")),
        ("applicationId", Js::Undefined),
    ] {
        alias.set(key, value).unwrap();
        let current = base.options();
        rows.push(json!([current.application_id, current.vault]));
    }
    assert_eq!(
        json!(rows),
        expected["identity"],
        "WRITE_OBSERVER native reader sees every successful write immediately"
    );
    shared
        .globals
        .get("Object")
        .unwrap()
        .call_method("freeze", vec![options.clone()])
        .unwrap();
    let failure = alias.set("applicationId", Js::str("refused")).unwrap_err();
    assert_eq!(
        failure.value.get("name").unwrap().to_display(),
        expected["frozenError"]
    );
    assert_eq!(
        calls.get(),
        6,
        "WRITE_OBSERVER no publication for refused write"
    );
    assert_eq!(
        json!([base.options().application_id, base.options().vault]),
        expected["final"]
    );
    drop(base);
    alias.set_write_observer(None).unwrap();
}

#[test]
fn inherited_setter_reentry_and_observer_failure_preserve_source_order_and_identity() {
    let expected = source();
    let shared = scope();
    let object_ctor = shared.globals.get("Object").unwrap();
    let prototype = Js::object(vec![]);
    object_ctor
        .call_method(
            "defineProperty",
            vec![
                prototype.clone(),
                Js::str("inherited"),
                Js::object(vec![(
                    "set",
                    Js::function("set inherited", |this, args| {
                        this.set("inner", args[0].clone())?;
                        Ok(Js::Undefined)
                    }),
                )]),
            ],
        )
        .unwrap();
    let object = Js::object(vec![]);
    object_ctor
        .call_method("setPrototypeOf", vec![object.clone(), prototype])
        .unwrap();
    let trace = Rc::new(RefCell::new(vec![]));
    let rows = trace.clone();
    let Js::Object(data) = &object else {
        unreachable!()
    };
    let original = Rc::downgrade(data);
    object
        .set_write_observer(Some(Rc::new(move |receiver, key, _| {
            let key = String::from_utf16(key).unwrap();
            let same = receiver.same_reference(&Js::Object(original.upgrade().unwrap()));
            let value = receiver.get(&key)?;
            rows.borrow_mut().push(Js::array(vec![
                Js::str(&key),
                Js::Bool(same),
                if value.is_nullish() { Js::Null } else { value },
            ]));
            Ok(())
        })))
        .unwrap();
    object.set("inherited", Js::Number(9.0)).unwrap();
    assert_eq!(
        String::from_utf16(
            &globals::stringify_json(&Js::array(trace.borrow().clone()))
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        expected["trace"].to_string(),
        "WRITE_OBSERVER nested setter before outer notification"
    );
    let cause = Js::object(vec![]);
    let thrown = cause.clone();
    object
        .set_write_observer(Some(Rc::new(move |receiver, _, _| {
            receiver.set_write_observer(None)?;
            Err(Thrown::new(thrown.clone()))
        })))
        .unwrap();
    let failure = object.set("value", Js::Number(4.0)).unwrap_err();
    assert!(
        failure.value.same_reference(&cause),
        "WRITE_OBSERVER original failure identity"
    );
    assert_eq!(
        object.get("value").unwrap().to_number().unwrap(),
        expected["failedWrite"].as_f64().unwrap(),
        "WRITE_OBSERVER failing publication never rolls back source write"
    );
    object.set("next", Js::Bool(true)).unwrap();
}

#[test]
fn refused_assignments_never_notify_and_throwing_setter_keeps_original_error() {
    let shared = scope();
    let object_ctor = shared.globals.get("Object").unwrap();
    let object = Js::object(vec![]);
    let cause = Js::object(vec![]);
    let thrown = cause.clone();
    for (key, descriptor) in [
        ("readonly", Js::object(vec![("value", Js::Number(1.0))])),
        (
            "getterOnly",
            Js::object(vec![(
                "get",
                Js::function("get only", |_, _| Ok(Js::Number(2.0))),
            )]),
        ),
        (
            "throwing",
            Js::object(vec![(
                "set",
                Js::function("throwing setter", move |_, _| {
                    Err(Thrown::new(thrown.clone()))
                }),
            )]),
        ),
    ] {
        object_ctor
            .call_method(
                "defineProperty",
                vec![object.clone(), Js::str(key), descriptor],
            )
            .unwrap();
    }
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    object
        .set_write_observer(Some(Rc::new(move |_, _, _| {
            count.set(count.get() + 1);
            Ok(())
        })))
        .unwrap();
    for key in ["readonly", "getterOnly"] {
        assert_eq!(
            object
                .set(key, Js::Number(3.0))
                .unwrap_err()
                .value
                .get("name")
                .unwrap()
                .to_display(),
            "TypeError"
        );
    }
    assert!(
        object
            .set("throwing", Js::Null)
            .unwrap_err()
            .value
            .same_reference(&cause)
    );
    assert_eq!(
        calls.get(),
        0,
        "WRITE_OBSERVER only successful assignments notify"
    );
}

#[test]
fn observer_preserves_utf16_value_identity_bits_and_releases_replaced_capture() {
    let object = Js::object(vec![]);
    let observed = Rc::new(RefCell::new(vec![]));
    let output = observed.clone();
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let observer: WriteObserver = Rc::new(move |receiver, key, value| {
        let _alive = &capture;
        let stored = receiver.get_key(key)?;
        if value.is_object() {
            assert!(
                stored.same_reference(value),
                "WRITE_OBSERVER original value reference"
            );
        }
        output.borrow_mut().push((key.to_vec(), value.clone()));
        Ok(())
    });
    object.set_write_observer(Some(observer)).unwrap();
    let payload = Js::object(vec![]);
    object.set_key(&[0xd800, 0], payload.clone()).unwrap();
    for bits in [0x7ff8_0000_0000_0042, (-0.0_f64).to_bits()] {
        object
            .set("number", Js::Number(f64::from_bits(bits)))
            .unwrap();
        assert_eq!(
            observed
                .borrow()
                .last()
                .unwrap()
                .1
                .to_number()
                .unwrap()
                .to_bits(),
            bits
        );
    }
    assert_eq!(observed.borrow()[0].0, vec![0xd800, 0]);
    assert!(observed.borrow()[0].1.same_reference(&payload));
    object.define("data", Js::Bool(true)).unwrap();
    let spread = object.spread_object().unwrap();
    spread.set("number", Js::Number(12.0)).unwrap();
    let child = Js::object(vec![]);
    let Js::Object(data) = &child else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(object.clone());
    child.set("number", Js::Number(13.0)).unwrap();
    assert_eq!(
        observed.borrow().len(),
        3,
        "WRITE_OBSERVER not copied, inherited or invoked by data definition"
    );
    assert!(weak.upgrade().is_some());
    object.set_write_observer(None).unwrap();
    assert!(
        weak.upgrade().is_none(),
        "WRITE_OBSERVER clearing releases native capture"
    );
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    object
        .set_write_observer(Some(Rc::new(move |_, _, _| {
            let _alive = &capture;
            Ok(())
        })))
        .unwrap();
    drop(child);
    drop(object);
    assert!(
        weak.upgrade().is_none(),
        "WRITE_OBSERVER final referent releases native capture"
    );
    assert!(Js::Null.set_write_observer(None).is_err());
    assert!(Js::array(vec![]).set_write_observer(None).is_err());
}
