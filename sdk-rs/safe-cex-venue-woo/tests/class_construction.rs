#[path = "support/lifecycle.rs"]
mod fixture;
use fixture::*;
use safe_cex_venue_woo::class_construction::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
struct Harness {
    ops: Rc<ConstructionOps<V, E>>,
    trace: Rc<RefCell<Vec<String>>>,
    receiver: V,
    options: V,
    store: V,
    active: V,
    target: V,
    clients: [V; 4],
    sockets: [V; 2],
    handler: V,
    error: E,
    metadata: Rc<RefCell<Vec<(String, usize)>>>,
}
fn record(trace: &Rc<RefCell<Vec<String>>>, error: &E, stop: &str, key: &str) -> Result<(), E> {
    trace.borrow_mut().push(key.into());
    if stop == key {
        Err(error.clone())
    } else {
        Ok(())
    }
}
fn native(stop: &str) -> Harness {
    let runtime = Rc::new(Runtime::default());
    let mut life = runtime.operations();
    let trace = Rc::new(RefCell::new(vec![]));
    let error: E = Rc::new("constructor sentinel".into());
    let stop = stop.to_owned();
    let options = obj(vec![("id", s("original-options"))]);
    let first = obj(vec![("id", s("first-options-read"))]);
    let second = obj(vec![("id", s("second-options-read"))]);
    let proxy_options = obj(vec![("id", s("proxy-options"))]);
    let store = obj(vec![]);
    let active = obj(vec![]);
    let target = obj(vec![]);
    let handler = function(|_, _| Ok(u()));
    let receiver = obj(vec![("handleFillForPositionSync", handler.clone())]);
    let clients = [obj(vec![]), obj(vec![]), obj(vec![]), obj(vec![])];
    let sockets = [obj(vec![]), obj(vec![])];
    let returned = arr(clients[..3].to_vec());
    let inner_result = obj(vec![]);
    let outer_result = obj(vec![]);
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let handle = handler.clone();
    let instance = receiver.clone();
    set(
        &receiver,
        "on",
        function(move |r, a| {
            assert!(
                Rc::ptr_eq(&r, &instance),
                "WOO-CONSTRUCTOR native on receiver"
            );
            assert_eq!(text(&a[0]), "fill");
            assert!(
                Rc::ptr_eq(&a[1], &handle),
                "WOO-CONSTRUCTOR native fill handler identity"
            );
            record(&log, &failure, &fail, "on:fill")?;
            Ok(u())
        }),
    );
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let instance = receiver.clone();
    let one = first.clone();
    let two = second.clone();
    let reads = Rc::new(RefCell::new(0));
    let ops = Rc::get_mut(&mut life).unwrap();
    let value = Rc::get_mut(&mut ops.value).unwrap();
    let metadata = Rc::new(RefCell::new(vec![]));
    let meta = metadata.clone();
    value.function = Rc::new(move |name, arity, body| {
        meta.borrow_mut().push((name.to_owned(), arity));
        function(move |_, args| body(args))
    });
    value.get = Rc::new(move |r, key| {
        if Rc::ptr_eq(r, &instance) && key == "options" {
            record(&log, &failure, &fail, "get:options")?;
            let count = *reads.borrow();
            *reads.borrow_mut() += 1;
            return Ok(if count == 0 { one.clone() } else { two.clone() });
        }
        if key == "requireProxy"
            || (Rc::ptr_eq(r, &instance) && matches!(key, "on" | "handleFillForPositionSync"))
        {
            record(&log, &failure, &fail, &format!("get:{key}"))?;
        }
        get(r, key)
    });
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let handles = clients.clone();
    let expected_iterable = returned.clone();
    ops.binding = Rc::new(move |v, pattern| {
        assert_eq!(
            pattern,
            &[true, true, true],
            "WOO-CONSTRUCTOR native three iterator reads"
        );
        assert!(
            Rc::ptr_eq(&v, &expected_iterable),
            "WOO-CONSTRUCTOR native factory iterable identity"
        );
        for i in 0..3 {
            record(&log, &failure, &fail, &format!("next:{i}"))?;
        }
        record(&log, &failure, &fail, "iterator:return")?;
        Ok(handles[..3].to_vec())
    });
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let instance = receiver.clone();
    let input = options.clone();
    let outputs = clients.clone();
    let rate = function({
        let log = trace.clone();
        let failure = error.clone();
        let fail = stop.clone();
        let initial = clients[0].clone();
        let inner = inner_result.clone();
        let outer = outer_result.clone();
        move |_, a| {
            let max = number(&get(&a[1], "maxRequests")?);
            let period = number(&get(&a[1], "perMilliseconds")?);
            let Js::O(fields) = a[1].as_ref() else {
                panic!("rate options object")
            };
            assert_eq!(
                fields
                    .borrow()
                    .iter()
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>(),
                if max == 4.0 {
                    vec!["perMilliseconds", "maxRequests"]
                } else {
                    vec!["maxRequests", "perMilliseconds"]
                },
                "WOO-CONSTRUCTOR native rate option key order"
            );
            assert!(
                matches!((max, period), (4.0, 1000.0) | (20.0, 6000.0)),
                "WOO-CONSTRUCTOR native rate values"
            );
            let key = format!("rate:{}:{}", max as i32, period as i32);
            record(&log, &failure, &fail, &key)?;
            if max == 4.0 {
                assert!(
                    Rc::ptr_eq(&a[0], &initial),
                    "WOO-CONSTRUCTOR native inner limiter input"
                );
                Ok(inner.clone())
            } else {
                assert!(
                    Rc::ptr_eq(&a[0], &inner),
                    "WOO-CONSTRUCTOR native outer limiter input"
                );
                Ok(outer.clone())
            }
        }
    });
    let api_slot = Rc::new(RefCell::new(None::<V>));
    let expected_api = api_slot.clone();
    let factory = function(move |_, a| {
        assert_eq!(a.len(), 4);
        assert!(
            Rc::ptr_eq(&a[2], expected_api.borrow().as_ref().unwrap()),
            "WOO-CONSTRUCTOR native API factory identity"
        );
        assert!(Rc::ptr_eq(&a[0], &input));
        assert!(Rc::ptr_eq(&a[1], &instance));
        record(&log, &failure, &fail, "factory")?;
        let limiter = get(&a[3], "rateLimit")?;
        let limited = call(limiter, u(), vec![outputs[0].clone()])?;
        assert!(
            Rc::ptr_eq(&limited, &outer_result),
            "WOO-CONSTRUCTOR native limiter identity"
        );
        Ok(returned.clone())
    });
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let instance = receiver.clone();
    let proxy = proxy_options.clone();
    let output = clients[3].clone();
    let api = function(move |_, a| {
        assert_eq!(a.len(), 2);
        assert!(Rc::ptr_eq(&a[0], &proxy));
        assert!(Rc::ptr_eq(&a[1], &instance));
        record(&log, &failure, &fail, "createAPI:proxy")?;
        Ok(output.clone())
    });
    *api_slot.borrow_mut() = Some(api.clone());
    let transport = obj(vec![]);
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let transport_ref = transport.clone();
    let two = second.clone();
    let proxy = proxy_options.clone();
    set(
        &transport,
        "requireProxy",
        function(move |r, a| {
            assert!(Rc::ptr_eq(&r, &transport_ref));
            assert!(Rc::ptr_eq(&a[0], &two));
            record(&log, &failure, &fail, "requireProxy")?;
            Ok(proxy.clone())
        }),
    );
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let one = first.clone();
    let get_transport = function(move |_, a| {
        assert!(Rc::ptr_eq(&a[0], &one));
        record(&log, &failure, &fail, "getTransport")?;
        Ok(transport.clone())
    });
    let imports = obj(vec![
        ("createApiWithSecondaryKey", factory),
        ("createAPI", api),
        ("rateLimit", rate),
        ("getTransport", get_transport),
        ("WooPublicWebsocket", s("public")),
        ("WooPrivateWebscoket", s("private")),
    ]);
    ops.import = Rc::new(move |name| get(&imports, name));
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let instance = receiver.clone();
    let expected_active = active.clone();
    let expected_target = target.clone();
    let input = options.clone();
    let expected_store = store.clone();
    let super_construct = Rc::new(move |a: V, t: V, args: Vec<V>| {
        assert!(Rc::ptr_eq(&a, &expected_active));
        assert!(Rc::ptr_eq(&t, &expected_target));
        assert_eq!(args.len(), 3);
        assert_eq!(text(&args[0]), "woo");
        assert!(Rc::ptr_eq(&args[1], &input));
        assert!(Rc::ptr_eq(&args[2], &expected_store));
        record(&log, &failure, &fail, "super")?;
        Ok(instance.clone())
    });
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop.clone();
    let instance = receiver.clone();
    let objects = sockets.clone();
    let construct = Rc::new(move |c: V, a: Vec<V>| {
        assert_eq!(a.len(), 1);
        assert!(Rc::ptr_eq(&a[0], &instance));
        let name = text(&c);
        record(&log, &failure, &fail, &format!("construct:{name}"))?;
        Ok(objects[usize::from(name == "private")].clone())
    });
    let log = trace.clone();
    let failure = error.clone();
    let fail = stop;
    let instance = receiver.clone();
    let set_value = Rc::new(move |r: &V, key: &str, v: V| {
        assert!(Rc::ptr_eq(r, &instance));
        record(&log, &failure, &fail, &format!("set:{key}"))?;
        set(r, key, v);
        Ok(())
    });
    Harness {
        ops: Rc::new(ConstructionOps {
            value: life,
            super_construct,
            construct,
            set: set_value,
        }),
        trace,
        receiver,
        options,
        store,
        active,
        target,
        clients,
        sockets,
        handler,
        error,
        metadata,
    }
}
fn expected() -> Vec<&'static str> {
    vec![
        "super",
        "factory",
        "rate:4:1000",
        "rate:20:6000",
        "next:0",
        "next:1",
        "next:2",
        "iterator:return",
        "set:xhr",
        "set:_primaryXhr",
        "set:_secondaryXhr",
        "get:options",
        "getTransport",
        "get:requireProxy",
        "get:options",
        "requireProxy",
        "createAPI:proxy",
        "set:_proxyEnforcedXhr",
        "construct:public",
        "set:publicWebsocket",
        "construct:private",
        "set:privateWebsocket",
        "get:on",
        "get:handleFillForPositionSync",
        "on:fill",
    ]
}
#[test]
fn native_constructor_setup_and_failure_order() {
    let stops = [
        "",
        "super",
        "factory",
        "rate:4:1000",
        "rate:20:6000",
        "next:1",
        "iterator:return",
        "set:_primaryXhr",
        "requireProxy",
        "createAPI:proxy",
        "construct:public",
        "construct:private",
        "on:fill",
    ];
    let mut observed = Vec::new();
    for stop in stops {
        let h = native(stop);
        let result = exchange_constructor(
            h.ops.clone(),
            h.active.clone(),
            h.target.clone(),
            vec![
                s("woo"),
                h.options.clone(),
                h.store.clone(),
                s("ignored-extra"),
            ],
        );
        assert_eq!(
            *h.metadata.borrow(),
            if stop == "super" {
                vec![]
            } else {
                vec![("rateLimit".to_owned(), 1)]
            },
            "WOO-CONSTRUCTOR native callback name/arity"
        );
        let full = expected();
        let count = if stop.is_empty() {
            25
        } else {
            full.iter().position(|v| *v == stop).unwrap() + 1
        };
        assert_eq!(
            *h.trace.borrow(),
            full[..count],
            "WOO-CONSTRUCTOR native call order"
        );
        if stop.is_empty() {
            assert!(Rc::ptr_eq(&result.unwrap(), &h.receiver));
            for (i, key) in ["xhr", "_primaryXhr", "_secondaryXhr", "_proxyEnforcedXhr"]
                .iter()
                .enumerate()
            {
                assert!(
                    Rc::ptr_eq(&get(&h.receiver, key).unwrap(), &h.clients[i]),
                    "WOO-CONSTRUCTOR native client identity"
                );
            }
            for (i, key) in ["publicWebsocket", "privateWebsocket"].iter().enumerate() {
                assert!(
                    Rc::ptr_eq(&get(&h.receiver, key).unwrap(), &h.sockets[i]),
                    "WOO-CONSTRUCTOR native socket identity"
                );
            }
            assert!(Rc::ptr_eq(
                &get(&h.receiver, "handleFillForPositionSync").unwrap(),
                &h.handler
            ));
        } else {
            assert!(
                Rc::ptr_eq(&result.err().unwrap(), &h.error),
                "WOO-CONSTRUCTOR native thrown identity"
            );
        }
        observed.push(json!({"stop":stop,"trace":&*h.trace.borrow()}));
    }
    assert_eq!(observed.len(), 13);
    let reference = source(&stops);
    assert_eq!(reference.as_array().unwrap().len(), 13);
    assert_eq!(json!(observed), reference);
}
fn source(cases: &[&str]) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/class-construction-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
