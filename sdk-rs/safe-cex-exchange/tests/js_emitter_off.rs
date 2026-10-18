#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown, from_json},
    },
    native::native_runtime,
};
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc};

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
fn facade() -> (Js, Rc<ScopeShared<()>>, Rc<dyn Exchange>) {
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
    let options = serde_json::json!({"key": "key", "secret":"secret"});
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
fn native_emitter_off_matches_installed_tiny_emitter() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_emitter_off_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let mut rows = vec![];
        for row in &expected {
            rows.push(native_emitter_case(row));
        }
        assert_eq!(rows, expected);
    });
}

fn native_emitter_case(row: &serde_json::Value) -> serde_json::Value {
    let kind = row["kind"].as_str().unwrap().to_owned();
    let (this, _scope, owner) = facade();
    let emitter = this.get("emitter").unwrap();
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let seen = trace.clone();
    let second = Js::function("second", move |_, _| {
        seen.borrow_mut().push("second".into());
        Ok(Js::Undefined)
    });
    let seen = trace.clone();
    let action = emitter.clone();
    let callback = second.clone();
    let behavior = kind.clone();
    let first = Js::function("first", move |_, _| {
        seen.borrow_mut().push("first".into());
        if behavior == "remove-during" {
            action.call_method("off", vec![Js::str("fill"), callback.clone()])?;
        }
        if behavior == "clear-during" {
            action.call_method("off", vec![Js::str("fill")])?;
        }
        Ok(Js::Undefined)
    });
    for listener in [first.clone(), second.clone(), second.clone()] {
        emitter
            .call_method("on", vec![Js::str("fill"), listener])
            .unwrap();
    }
    let seen = trace.clone();
    emitter
        .call_method(
            "on",
            vec![
                Js::str("other"),
                Js::function("other", move |_, _| {
                    seen.borrow_mut().push("other".into());
                    Ok(Js::Undefined)
                }),
            ],
        )
        .unwrap();
    if kind.ends_with("-native") || kind == "native" {
        let seen = trace.clone();
        owner.base().on_args(
            "fill",
            Rc::new(move |_, _| {
                seen.borrow_mut().push("native".into());
                Ok(())
            }),
        );
    }
    let cause = Js::object(vec![("kind", Js::str(&kind))]);
    if kind == "alias" {
        let seen = trace.clone();
        let wrapper = Js::function("wrapper", move |_, _| {
            seen.borrow_mut().push("wrapper".into());
            Ok(Js::Undefined)
        });
        wrapper.define("_", second.clone()).unwrap();
        emitter
            .call_method("on", vec![Js::str("fill"), wrapper])
            .unwrap();
    }
    if kind.starts_with("alias-") {
        let cause = cause.clone();
        let action = emitter.clone();
        let behavior = kind.clone();
        let seen = trace.clone();
        first
            .define_accessor(
                "_",
                Some(Rc::new(move |_| {
                    if behavior == "alias-throws" {
                        return Err(Thrown::new(cause.clone()));
                    }
                    if behavior.starts_with("alias-clears") {
                        action.call_method("off", vec![Js::str("fill")])?;
                    }
                    if behavior == "alias-adds" {
                        let seen = seen.clone();
                        action.call_method(
                            "on",
                            vec![
                                Js::str("fill"),
                                Js::function("late", move |_, _| {
                                    seen.borrow_mut().push("late".into());
                                    Ok(Js::Undefined)
                                }),
                            ],
                        )?;
                    }
                    Ok(Js::Undefined)
                })),
                None,
            )
            .expect("test accessor target permits definition");
    }
    let mut args = vec![Js::str(if kind == "missing" { "absent" } else { "fill" })];
    if kind != "omitted" {
        args.push(match kind.as_str() {
            "undefined" | "clear-native" => Js::Undefined,
            "null" => Js::Null,
            "false" => Js::Bool(false),
            "zero" => Js::Number(0.0),
            "empty" => Js::str(""),
            "nan" => Js::Number(f64::NAN),
            "different" => Js::function("different", |_, _| Ok(Js::Undefined)),
            "noncallable" => Js::Number(1.0),
            _ => second,
        });
    }
    let mut chained = true;
    let mut caught = false;
    if !kind.ends_with("-during") {
        match emitter.call_method("off", args) {
            Ok(value) => chained = value.same_reference(&emitter),
            Err(error) => caught = error.value.same_reference(&cause),
        }
    }
    emitter.call_method("emit", vec![Js::str("fill")]).unwrap();
    if kind.ends_with("-during") {
        emitter.call_method("emit", vec![Js::str("fill")]).unwrap();
    }
    emitter.call_method("emit", vec![Js::str("other")]).unwrap();

    let result = serde_json::json!({"kind":kind,"trace":trace.borrow().clone(),"chained":chained,"caught":caught});
    emitter.call_method("off", vec![Js::str("fill")]).unwrap();
    emitter.call_method("off", vec![Js::str("other")]).unwrap();
    owner.dispose().unwrap();
    result
}

#[test]
fn native_log_facade_retains_utf16_before_console_and_event() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let (this, _shared, owner) = facade();
        let observed = Rc::new(RefCell::new(vec![]));
        let captured = observed.clone();
        owner.base().on_args(
            "log",
            Rc::new(move |_, args| {
                captured.borrow_mut().push(args.to_vec());
                Ok(())
            }),
        );
        for units in [
            vec![0xd800],
            vec![0xdc00],
            vec![0x61, 0xd800, 0x62, 0xdc00],
            vec![0xd83d, 0xde80],
            vec![],
        ] {
            let message = Js::utf16(&units);
            this.call_method("log", vec![message, Js::str("warning")])
                .unwrap();
            let events = observed.borrow();
            let args = events.last().unwrap();
            let mut prefixed = "[bybit] ".encode_utf16().collect::<Vec<_>>();
            prefixed.extend(units);
            assert_eq!(args[0].as_str().unwrap().as_ref(), prefixed);
            assert_eq!(
                args[1].as_str().unwrap().as_ref(),
                "warning".encode_utf16().collect::<Vec<_>>()
            );
        }
        owner.base().dispose().unwrap();
    });
}
