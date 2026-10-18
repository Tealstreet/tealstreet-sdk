use super::{PolymarketDependencies, PolymarketExchange};
use crate::{
    account_reads::AccountHost,
    api_factory::FactoryHost,
    auth_init::AuthHost,
    classes::ClassHost,
    exchange::ExchangeHost,
    order_adapters::OrderHost,
    state_sync::StateHost,
    ws_dispatch::{ConnectHost, MessageHost, Runtime},
    ws_lifecycle::SocketHost,
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        promise::{JsFuture, await_js},
        value::{Js, Thrown, arg},
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

struct NoHttp(Rc<dyn ExchangeRuntime>);
impl JsHttpHost for NoHttp {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("receiver witness must not perform HTTP")
    }
}
fn run(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(tokio::task::LocalSet::new().run_until(future));
}
fn venue() -> Rc<PolymarketExchange> {
    let runtime = native_runtime();
    PolymarketExchange::new(
        ExchangeName::Polymarket,
        serde_json::from_value(serde_json::json!({"key":"","secret":"","publicDataOnly":true}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        PolymarketDependencies {
            runtime: runtime.clone(),
            http: Rc::new(NoHttp(runtime)),
            external_public_data: None,
        },
    )
    .unwrap()
}
fn units(value: &Js) -> Vec<u16> {
    match value {
        Js::String(value) => value.to_vec(),
        _ => panic!("expected native text"),
    }
}

#[test]
fn native_receiver_properties_captures_literals_and_aggregation() {
    run(async {
        let venue = venue();
        let rt = &venue.rt;
        let child = Js::object(vec![
            ("opaque", Js::utf16(&[0xd800])),
            ("nan", Js::Number(f64::NAN)),
        ]);
        let table = Js::object(vec![
            ("child", child.clone()),
            ("ownUndefined", Js::Undefined),
        ]);
        table.define_key(&[0xdfff], Js::Number(-0.0)).unwrap();
        rt.shared
            .globals
            .define("RECEIVER_TABLE", table.clone())
            .unwrap();
        assert!(
            FactoryHost::table(rt, "RECEIVER_TABLE")
                .unwrap()
                .same_reference(&table)
        );
        assert!(
            OrderHost::enum_lookup(rt, "RECEIVER_TABLE", &Js::str("child"))
                .unwrap()
                .same_reference(&child)
        );
        assert_eq!(
            OrderHost::enum_lookup(rt, "RECEIVER_TABLE", &Js::utf16(&[0xdfff]))
                .unwrap()
                .to_number()
                .unwrap()
                .to_bits(),
            (-0.0f64).to_bits()
        );
        assert!(
            OrderHost::enum_lookup(rt, "RECEIVER_TABLE", &Js::str("absent"))
                .unwrap()
                .is_undefined()
        );
        assert_eq!(
            units(&OrderHost::literal(rt, "{invalid JSON\0}")),
            "{invalid JSON\0}".encode_utf16().collect::<Vec<_>>()
        );
        assert!(Runtime::is_text(rt, &Js::str("tag"), "tag"));
        assert!(!Runtime::is_text(rt, &Js::utf16(&[0xd800]), "tag"));
        assert!(!Runtime::is_text(rt, &Js::Number(1.0), "1"));
        assert!(
            Runtime::field(rt, &table, "child")
                .unwrap()
                .same_reference(&child)
        );
        assert!(
            Runtime::field(rt, &table, "ownUndefined")
                .unwrap()
                .is_undefined()
        );
        assert!(Runtime::field(rt, &Js::Null, "x").is_err());
        let thrown = Js::object(vec![("cause", child.clone())]);
        let error = thrown.clone();
        table
            .define_accessor(
                "throw",
                Some(Rc::new(move |_| Err(Thrown::new(error.clone())))),
                None,
            )
            .unwrap();
        assert!(
            Runtime::field(rt, &table, "throw")
                .unwrap_err()
                .value
                .same_reference(&thrown)
        );
        let object = AccountHost::object(
            rt,
            vec![("child", child.clone()), ("missing", Js::Undefined)],
        )
        .unwrap();
        assert!(object.get("child").unwrap().same_reference(&child));
        assert!(object.has_own("missing"));
        AccountHost::assign(rt, &object, "child", Js::Number(-0.0)).unwrap();
        assert_eq!(
            object.get("child").unwrap().to_number().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            units(&AccountHost::text(rt, "null\0")),
            [110, 117, 108, 108, 0]
        );
        assert_eq!(
            units(
                &AccountHost::new_error(rt, "native reason")
                    .value
                    .get("message")
                    .unwrap()
            ),
            "native reason".encode_utf16().collect::<Vec<_>>()
        );
        let loaded = Js::object(vec![("child", child.clone()), ("orders", Js::Bool(false))]);
        let spread =
            StateHost::spread_loaded(rt, loaded.clone(), &["orders", "positions"]).unwrap();
        assert!(!spread.same_reference(&loaded));
        assert!(spread.get("child").unwrap().same_reference(&child));
        assert!(!loaded.get("orders").unwrap().truthy());
        assert!(spread.get("orders").unwrap().truthy());
        assert!(spread.get("positions").unwrap().truthy());
        let rows = Js::array(vec![
            Js::object(vec![("amount", Js::Number(1.5))]),
            Js::object(vec![("amount", Js::Number(2.5))]),
        ]);
        assert_eq!(
            StateHost::sum_by(rt, &rows, "amount")
                .unwrap()
                .to_number()
                .unwrap()
                .to_bits(),
            4.0f64.to_bits()
        );
        let sink = Rc::new(RefCell::new(vec![]));
        let seen = sink.clone();
        let owner = Js::object(vec![(
            "log",
            Js::function("log", move |this, args| {
                seen.borrow_mut().push((this, args));
                Ok(Js::Undefined)
            }),
        )]);
        let scoped = rt.with_this(owner.clone());
        Runtime::log(&scoped, &[0xd800], "warning").unwrap();
        let captured = AccountHost::capture(&scoped, "exchange", "log").unwrap();
        owner.set("log", Js::Undefined).unwrap();
        AccountHost::invoke_log(&scoped, captured, &[0xdfff], "error").unwrap();
        let events = sink.borrow();
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|(this, _)| this.same_reference(&owner)));
        assert_eq!(units(&events[0].1[0]), [0xd800]);
        assert_eq!(units(&events[1].1[0]), [0xdfff]);
        assert_eq!(units(&events[0].1[1]), units(&Js::str("warning")));
        assert_eq!(units(&events[1].1[1]), units(&Js::str("error")));
        let method = Js::function("method", |this, args| {
            Ok(Js::array(vec![this, arg(&args, 0)]))
        });
        let object = Js::object(vec![("method", method.clone())]);
        let captured = AuthHost::capture_value(rt, object.clone(), "method").unwrap();
        object.set("method", Js::Undefined).unwrap();
        let value = AuthHost::call_sync(rt, captured, vec![child.clone()]).unwrap();
        assert!(value.get("0").unwrap().same_reference(&object));
        assert!(value.get("1").unwrap().same_reference(&child));
        let constructor = Js::constructor("Nested", None, |_, _| Ok(Js::object(vec![])));
        rt.shared
            .globals
            .define(
                "ReceiverNamespace",
                Js::object(vec![("Nested", constructor.clone())]),
            )
            .unwrap();
        assert!(
            AuthHost::capture_constructor(rt, "ReceiverNamespace.Nested")
                .unwrap()
                .same_reference(&constructor)
        );
        assert!(
            AuthHost::capture_constructor(rt, "ReceiverNamespace.absent")
                .unwrap()
                .is_undefined()
        );
    });
}

#[test]
fn native_receiver_class_allocation_and_bound_methods() {
    run(async {
        let venue = venue();
        let rt = &venue.rt;
        let target = Js::object(vec![]);
        let child = Js::object(vec![("opaque", Js::utf16(&[0xd800]))]);
        ExchangeHost::define(rt, &target, "child", child.clone()).unwrap();
        assert!(target.get("child").unwrap().same_reference(&child));
        let prototype = Js::object(vec![("inherited", Js::Undefined)]);
        if let Js::Object(data) = &target {
            *data.proto.borrow_mut() = Some(prototype);
        }
        assert!(ExchangeHost::has(rt, &target, "inherited").unwrap());
        assert!(!ExchangeHost::has(rt, &target, "absent").unwrap());
        let seen = Rc::new(RefCell::new(None));
        let observed = seen.clone();
        let base = Js::constructor("Base", None, move |target, args| {
            *observed.borrow_mut() = Some(target);
            Ok(Js::object(vec![("label", arg(&args, 0))]))
        });
        let getter = Js::function("name", |this, _| this.get("label"));
        let derived = ExchangeHost::derive(
            rt,
            base.clone(),
            vec![("name", getter), ("child", child.clone())],
        )
        .unwrap();
        let object = derived.construct(vec![Js::utf16(&[0xdfff])], None).unwrap();
        assert!(seen.borrow().as_ref().unwrap().same_reference(&derived));
        assert_eq!(units(&object.get("name").unwrap()), [0xdfff]);
        assert!(object.get("child").unwrap().same_reference(&child));
        let standalone =
            ClassHost::define_class(rt, "Witness", None, vec![("child", child.clone())]).unwrap();
        assert!(
            standalone
                .construct(vec![], None)
                .unwrap()
                .get("child")
                .unwrap()
                .same_reference(&child)
        );
        let derived = ClassHost::define_class(
            rt,
            "DerivedWitness",
            Some(base),
            vec![("child", child.clone())],
        )
        .unwrap();
        let object = derived.construct(vec![Js::str("original")], None).unwrap();
        assert_eq!(
            units(&object.get("label").unwrap()),
            "original".encode_utf16().collect::<Vec<_>>()
        );
        assert!(object.get("child").unwrap().same_reference(&child));
        let calls = Rc::new(RefCell::new(vec![]));
        let observed = calls.clone();
        let bound = Js::object(vec![(
            "log",
            Js::function("log", move |this, args| {
                observed.borrow_mut().push((this, args));
                Ok(Js::Undefined)
            }),
        )]);
        let method = ExchangeHost::method(rt, &bound, "setLeverage").unwrap();
        let value = method.call(Js::Null, vec![]).unwrap();
        await_js(&rt.exchange_runtime(), value).await.unwrap();
        let unbound = ExchangeHost::prototype_method(rt, "setLeverage").unwrap();
        let value = unbound.call(bound.clone(), vec![]).unwrap();
        await_js(&rt.exchange_runtime(), value).await.unwrap();
        assert_eq!(calls.borrow().len(), 2);
        assert!(
            calls
                .borrow()
                .iter()
                .all(|(this, _)| this.same_reference(&bound))
        );
        for (_, args) in calls.borrow().iter() {
            assert_eq!(
                units(&args[0]),
                units(&Js::str(
                    "Leverage is not supported on Polymarket (always 1x)"
                ))
            );
            assert_eq!(units(&args[1]), units(&Js::str("warning")));
        }
        let payload = Js::object(vec![("opaque", child)]);
        let result = payload.clone();
        let xhr = Js::object(vec![(
            "get",
            Js::function("get", move |_, args| {
                assert_eq!(
                    units(&arg(&args, 0)),
                    "/time".encode_utf16().collect::<Vec<_>>()
                );
                Ok(Js::object(vec![("data", result.clone())]))
            }),
        )]);
        let receiver = Js::object(vec![("xhr", xhr)]);
        let method =
            ClassHost::class_method(rt, "PolymarketAPI", "getServerTime", Some(receiver)).unwrap();
        let value = method.call(Js::Undefined, vec![]).unwrap();
        assert!(
            await_js(&rt.exchange_runtime(), value)
                .await
                .unwrap()
                .same_reference(&payload)
        );
    });
}

#[test]
fn native_receiver_socket_forwarding_and_refresh_effects() {
    run(async {
        let venue = venue();
        let calls = Rc::new(RefCell::new(vec![]));
        let seen = calls.clone();
        let log = Js::function("log", move |this, args| {
            seen.borrow_mut().push((this, args));
            Ok(Js::Undefined)
        });
        let refreshes = Rc::new(Cell::new(0));
        let count = refreshes.clone();
        let parent = Js::object(vec![
            ("log", log),
            (
                "fetchAndSetPositions",
                Js::function("fetchAndSetPositions", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::Undefined)
                }),
            ),
        ]);
        let sent = Rc::new(RefCell::new(vec![]));
        let seen = sent.clone();
        let open = Js::function("onOpen", |_, _| Ok(Js::Undefined));
        let receiver = Js::object(vec![
            ("parent", parent.clone()),
            ("onOpen", open.clone()),
            (
                "send",
                Js::function("send", move |_, args| {
                    seen.borrow_mut().push(args);
                    Ok(Js::Undefined)
                }),
            ),
        ]);
        let rt = venue.rt.with_this(receiver);
        assert!(
            ConnectHost::listener(&rt, "open")
                .unwrap()
                .same_reference(&open)
        );
        assert!(ConnectHost::listener(&rt, "unsupported").is_err());
        let listeners = Rc::new(RefCell::new(vec![]));
        let seen = listeners.clone();
        let socket = Js::object(vec![(
            "addEventListener",
            Js::function("addEventListener", move |this, args| {
                seen.borrow_mut().push((this, args));
                Ok(Js::Undefined)
            }),
        )]);
        ConnectHost::add_listener(&rt, socket.clone(), "open", open.clone()).unwrap();
        assert!(listeners.borrow()[0].0.same_reference(&socket));
        assert!(listeners.borrow()[0].1[1].same_reference(&open));
        assert_eq!(
            units(&listeners.borrow()[0].1[0]),
            "open".encode_utf16().collect::<Vec<_>>()
        );
        SocketHost::log(&rt, "literal {not JSON}\0", "warning").unwrap();
        assert!(calls.borrow()[0].0.same_reference(&parent));
        assert_eq!(
            units(&calls.borrow()[0].1[0]),
            "literal {not JSON}\0".encode_utf16().collect::<Vec<_>>()
        );
        SocketHost::send(&rt, "{bad json}\0", None).unwrap();
        assert_eq!(
            units(&sent.borrow()[0][0]),
            "{bad json}\0".encode_utf16().collect::<Vec<_>>()
        );
        MessageHost::refresh_positions(&rt, "native receiver").unwrap();
        assert_eq!(
            refreshes.get(),
            1,
            "actual refresh helper invokes the current parent eagerly"
        );
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
        assert_eq!(refreshes.get(), 1);
    });
}
