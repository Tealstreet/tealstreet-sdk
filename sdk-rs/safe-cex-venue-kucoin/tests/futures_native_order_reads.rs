#[allow(dead_code)]
#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::{
    futures_native_order_reads as port, futures_public_class::ConstructionOps,
    public_streams::StreamOps, spot_private_class::PrivateClassOps,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
use values::*;
fn same(a: &V, b: &V) {
    assert!(Rc::ptr_eq(a, b), "KUCOIN-BOOK original reference identity");
}
fn same_value_zero(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Js::U, Js::U) | (Js::Null, Js::Null) => true,
        (Js::B(a), Js::B(b)) => a == b,
        (Js::S(a), Js::S(b)) => a == b,
        (Js::N(a), Js::N(b)) => a == b || (a.is_nan() && b.is_nan()),
        _ => Rc::ptr_eq(a, b),
    }
}
fn rows(a: &V) -> Vec<V> {
    let Js::A(rows) = a.as_ref() else {
        panic!("KUCOIN-BOOK native array")
    };
    rows.borrow().clone()
}
fn at(a: &V, k: &str) -> V {
    get(a, k).unwrap()
}
fn keys(a: &V) -> Vec<String> {
    let Js::O(fields) = a.as_ref() else {
        panic!("native record")
    };
    fields.borrow().iter().map(|(key, _)| key.clone()).collect()
}
fn num(a: &V, want: f64) {
    let Js::N(value) = a.as_ref() else {
        panic!("KUCOIN-BOOK native number kind")
    };
    assert_eq!(
        value.to_bits(),
        want.to_bits(),
        "KUCOIN-BOOK native number bits"
    );
}
fn string(a: &V, want: &str) {
    let Js::S(value) = a.as_ref() else {
        panic!("KUCOIN-BOOK native string kind")
    };
    assert_eq!(value, want, "KUCOIN-BOOK native string value");
}
fn tag(a: &V) -> Value {
    match a.as_ref() {
        Js::N(value) => {
            json!({"number":safe_cex_num::js_number_string(*value),"negativeZero":value.to_bits()==(-0f64).to_bits()})
        }
        Js::A(rows) => Value::Array(rows.borrow().iter().map(tag).collect()),
        Js::O(fields) => Value::Object(
            fields
                .borrow()
                .iter()
                .map(|(key, value)| (key.clone(), tag(value)))
                .collect(),
        ),
        _ => literal(a),
    }
}
fn metadata(rt: &Runtime, f: &V, name: &str, length: usize) {
    let metas = rt.metas.borrow();
    let (_, actual, arity, asynchronous) =
        metas.iter().find(|(v, _, _, _)| Rc::ptr_eq(v, f)).unwrap();
    assert_eq!(
        (actual.as_str(), *arity, *asynchronous),
        (name, length, false),
        "KUCOIN-BOOK native callback metadata"
    );
}
fn frame(rt: &Rc<Runtime>, receiver: &V) -> Rc<StreamOps<V, E>> {
    let mut h = Rc::try_unwrap(rt.frame(receiver)).ok().unwrap();
    let before = h.get.clone();
    let runtime = rt.clone();
    h.get = Rc::new(move |value, key| {
        if matches!(value.as_ref(), Js::U | Js::Null) {
            return Err(Rc::new(format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(value.as_ref(), Js::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        if key == "flatMap" && matches!(value.as_ref(), Js::A(_)) {
            let special = runtime
                .getters
                .borrow()
                .iter()
                .find(|(object, property, _)| Rc::ptr_eq(object, value) && property == key)
                .map(|(_, _, getter)| getter.clone());
            if let Some(getter) = special {
                return getter();
            }
            return Ok(function(move |receiver, args| {
                let Js::A(values) = receiver.as_ref() else {
                    panic!("flatMap receiver")
                };
                let length = values.borrow().len();
                let mut out = Vec::new();
                for index in 0..length {
                    let value = values.borrow().get(index).cloned().unwrap_or_else(u);
                    let value = call(
                        args[0].clone(),
                        u(),
                        vec![value, n(index as f64), receiver.clone()],
                    )?;
                    if let Js::A(parts) = value.as_ref() {
                        out.extend(parts.borrow().iter().cloned());
                    } else {
                        out.push(value);
                    }
                }
                Ok(arr(out))
            }));
        }
        before(value, key)
    });
    let intrinsic = h.intrinsic.clone();
    h.intrinsic = Rc::new(move |name, args| match name {
        "destructure-data" => get(&args[0], "data"),
        "spread-array-into" => {
            let Js::A(out) = args[0].as_ref() else {
                panic!("spread destination")
            };
            out.borrow_mut().extend(rows(&args[1]));
            Ok(args[0].clone())
        }
        "lt" => Ok(b(number(&args[0]) < number(&args[1]))),
        _ => intrinsic(name, args),
    });
    Rc::new(h)
}
struct Fixture {
    rt: Rc<Runtime>,
    h: Rc<StreamOps<V, E>>,
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
    xhr: V,
    raw: V,
    emitted: Rc<RefCell<Vec<V>>>,
    requests: Rc<RefCell<Vec<Vec<V>>>>,
    reads: Rc<Cell<usize>>,
}
fn fixture() -> Fixture {
    let rt = Rc::new(Runtime::default());
    let receiver = obj(vec![]);
    let xhr = obj(vec![]);
    set(&receiver, "xhr", xhr.clone());
    let raw = obj(vec![("marker", n(71.))]);
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let calls = emitted.clone();
    let owner = receiver.clone();
    set(
        &receiver,
        "emitAndLogError",
        function(move |actual, args| {
            same(&actual, &owner);
            assert_eq!(args.len(), 1);
            calls.borrow_mut().push(args[0].clone());
            Ok(u())
        }),
    );
    for (key, value) in [
        (
            "ENDPOINTS",
            obj(vec![
                ("ORDERS", s("/api/v1/orders")),
                ("ALGO_ORDERS", s("/api/v1/stopOrders")),
            ]),
        ),
        (
            "Array",
            obj(vec![(
                "isArray",
                function(|_, args| Ok(b(matches!(args[0].as_ref(), Js::A(_))))),
            )]),
        ),
        (
            "OrderType",
            obj(vec![
                ("StopLoss", s("stop_market")),
                ("TakeProfit", s("take_profit_market")),
                ("TrailingStopLoss", s("trailing_stop_market")),
            ]),
        ),
        (
            "Number",
            obj(vec![(
                "isFinite",
                function(|_, args| {
                    Ok(b(
                        matches!(args[0].as_ref(),Js::N(value)if value.is_finite()),
                    ))
                }),
            )]),
        ),
        (
            "Math",
            obj(vec![(
                "min",
                function(|_, args| {
                    Ok(n(if number(&args[0]) == 0. && number(&args[1]) == 0. {
                        if number(&args[0]).is_sign_negative()
                            || number(&args[1]).is_sign_negative()
                        {
                            -0.
                        } else {
                            0.
                        }
                    } else {
                        number(&args[0]).min(number(&args[1]))
                    }))
                }),
            )]),
        ),
        (
            "multiply",
            function(|_, args| {
                Ok(n(safe_cex_num::multiply(
                    number(&args[0]),
                    number(&args[1]),
                )))
            }),
        ),
    ] {
        rt.globals.borrow_mut().insert(key.into(), value);
    }
    let mut language = Rc::try_unwrap(rt.operations()).ok().unwrap();
    let frames = rt.clone();
    language.frame = Rc::new(move |receiver| frame(&frames, receiver));
    let ops = Rc::new(language);
    let h = frame(&rt, &receiver);
    Fixture {
        rt,
        h,
        ops,
        receiver,
        xhr,
        raw,
        emitted,
        requests: Rc::new(RefCell::new(Vec::new())),
        reads: Rc::new(Cell::new(0)),
    }
}
fn run_book(kind: &str) -> Value {
    let f = fixture();
    let algo = kind.starts_with("algo-");
    let suffix = kind.split_once('-').unwrap().1;
    let input_a = obj(vec![("id", s("A")), ("marker", n(11.))]);
    let input_b = obj(vec![("id", s("B")), ("marker", n(13.))]);
    let items = arr(vec![input_a.clone(), input_b.clone()]);
    let mapped_a = obj(vec![("id", s("mapped-A")), ("marker", n(17.))]);
    let mapped_child = obj(vec![("id", s("mapped-child")), ("marker", n(19.))]);
    let mapped_b = obj(vec![("id", s("mapped-B")), ("marker", n(23.))]);
    let a = input_a.clone();
    let b = input_b.clone();
    let input = items.clone();
    let ma = mapped_a.clone();
    let child = mapped_child.clone();
    let mb = mapped_b.clone();
    let mapper = function(move |actual, args| {
        assert!(
            matches!(actual.as_ref(), Js::U),
            "KUCOIN-BOOK flatMap callback receiver"
        );
        assert_eq!(args.len(), 3, "KUCOIN-BOOK flatMap argument count");
        same(&args[2], &input);
        if Rc::ptr_eq(&args[0], &a) {
            num(&args[1], 0.);
            Ok(arr(vec![ma.clone(), child.clone()]))
        } else {
            same(&args[0], &b);
            num(&args[1], 1.);
            Ok(arr(vec![mb.clone()]))
        }
    });
    set(&f.receiver, "mapOrder", mapper.clone());
    let mut envelope = obj(vec![
        ("code", s("200000")),
        ("data", obj(vec![("items", items.clone())])),
    ]);
    if suffix == "numeric-code" {
        set(&envelope, "code", n(200000.));
        set(&envelope, "msg", f.raw.clone());
    }
    if suffix == "default" {
        set(&envelope, "code", s("no"));
        set(&envelope, "msg", n(0.));
    }
    if suffix == "nonarray" {
        set(
            &envelope,
            "data",
            obj(vec![("items", obj(vec![("marker", n(29.))]))]),
        );
    }
    if suffix == "missing-data" {
        envelope = obj(vec![("code", s("200000"))]);
    }
    if suffix == "live-data" {
        let count = f.reads.clone();
        let second = items.clone();
        f.rt.getters.borrow_mut().push((
            envelope.clone(),
            "data".into(),
            Rc::new(move || {
                count.set(count.get() + 1);
                Ok(obj(vec![(
                    "items",
                    if count.get() == 1 {
                        arr(vec![])
                    } else {
                        second.clone()
                    },
                )]))
            }),
        ));
    }
    if suffix == "mapper-get" {
        let receiver = f.receiver.clone();
        let original = mapper.clone();
        let count = f.reads.clone();
        f.rt.getters.borrow_mut().push((
            items.clone(),
            "flatMap".into(),
            Rc::new(move || {
                count.set(count.get() + 1);
                let captured = original.clone();
                set(&receiver, "mapOrder", original.clone());
                Ok(function(move |actual, args| {
                    same(&args[0], &captured);
                    assert_eq!(args.len(), 1);
                    let mut out = Vec::new();
                    for (index, row) in rows(&actual).into_iter().enumerate() {
                        out.extend(rows(&call(
                            args[0].clone(),
                            u(),
                            vec![row, n(index as f64), actual.clone()],
                        )?));
                    }
                    Ok(arr(out))
                }))
            }),
        ));
        set(
            &f.receiver,
            "mapOrder",
            function(|_, _| panic!("stale mapper")),
        );
    }
    let requests = f.requests.clone();
    let owner = f.xhr.clone();
    let response = envelope.clone();
    let reject = suffix == "transport";
    set(
        &f.xhr,
        "get",
        function(move |actual, args| {
            same(&actual, &owner);
            requests.borrow_mut().push(args);
            Ok(ready(if reject {
                Err(Rc::new("transport".into()))
            } else {
                Ok(obj(vec![("data", response.clone())]))
            }))
        }),
    );
    let result = if algo {
        settle(port::fetch_algo_orders(f.h.clone()))
    } else {
        settle(port::fetch_normal_orders(f.h.clone()))
    }
    .unwrap();
    let requests = f.requests.borrow();
    assert_eq!(requests.len(), 1, "KUCOIN-BOOK one request");
    assert_eq!(requests[0].len(), 2, "KUCOIN-BOOK request arity");
    string(
        &requests[0][0],
        if algo {
            "/api/v1/stopOrders"
        } else {
            "/api/v1/orders"
        },
    );
    assert_eq!(keys(&requests[0][1]), ["params"]);
    let params = at(&requests[0][1], "params");
    assert_eq!(keys(&params), ["status"]);
    string(&at(&params, "status"), "active");
    let failed = ["numeric-code", "default", "missing-data", "transport"].contains(&suffix);
    assert_eq!(
        f.emitted.borrow().len(),
        usize::from(failed),
        "KUCOIN-BOOK independent emission count"
    );
    if failed {
        assert!(
            matches!(result.as_ref(), Js::Null),
            "KUCOIN-BOOK native failure null"
        );
        let message = f.emitted.borrow()[0].clone();
        match suffix {
            "numeric-code" => {
                same(&message, &f.raw);
                assert_eq!(keys(&message), ["marker"]);
                num(&at(&message, "marker"), 71.);
                set(&f.raw, "marker", n(101.));
                num(&at(&message, "marker"), 101.);
            }
            "default" => string(
                &message,
                if algo {
                    "Failed to fetch algo orders"
                } else {
                    "Failed to fetch orders"
                },
            ),
            "transport" => string(&message, "transport"),
            _ => string(
                &message,
                "Cannot read properties of undefined (reading 'items')",
            ),
        }
    } else {
        let result = rows(&result);
        assert_eq!(
            result.len(),
            if suffix == "nonarray" { 0 } else { 3 },
            "KUCOIN-BOOK independent mapped count"
        );
        if suffix != "nonarray" {
            for (index, (actual, want)) in result
                .iter()
                .zip([mapped_a, mapped_child, mapped_b])
                .enumerate()
            {
                same(actual, &want);
                assert_eq!(keys(actual), ["id", "marker"]);
                string(
                    &at(actual, "id"),
                    ["mapped-A", "mapped-child", "mapped-B"][index],
                );
                num(&at(actual, "marker"), [17., 19., 23.][index]);
            }
        }
    }
    assert_eq!(
        f.reads.get(),
        if suffix == "live-data" {
            2
        } else if suffix == "mapper-get" {
            1
        } else {
            0
        },
        "KUCOIN-BOOK independent live getter count"
    );
    json!({"value":tag(&result),"emitted":f.emitted.borrow().iter().map(tag).collect::<Vec<_>>(),"requests":requests.iter().map(|args|args.iter().map(tag).collect::<Vec<_>>()).collect::<Vec<_>>(),"reads":f.reads.get()})
}

fn ready(result: Result<V, E>) -> V {
    Rc::new(Js::P(Rc::new(Promise {
        task: RefCell::new(None),
        result: RefCell::new(Some(result)),
    })))
}
fn settle(future: impl std::future::Future<Output = Result<V, E>>) -> Result<V, E> {
    let mut task = Box::pin(future);
    for _ in 0..32 {
        if let std::task::Poll::Ready(result) = task
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            return result;
        }
    }
    panic!("KUCOIN-BOOK unresolved finite promise");
}
fn row(id: &str, kind: &str, marker: f64) -> V {
    obj(vec![
        ("id", s(id)),
        ("type", s(kind)),
        ("marker", n(marker)),
    ])
}
fn spy_filter(rt: &Rc<Runtime>, array: &V, callbacks: Rc<RefCell<Vec<(String, usize)>>>) {
    let original = get(array, "filter").unwrap();
    let runtime = rt.clone();
    let owner = array.clone();
    rt.getters.borrow_mut().push((
        array.clone(),
        "filter".into(),
        Rc::new(move || {
            let ordinary = original.clone();
            let runtime = runtime.clone();
            let callbacks = callbacks.clone();
            let owner = owner.clone();
            Ok(function(move |actual, args| {
                same(&actual, &owner);
                assert_eq!(args.len(), 1);
                let metas = runtime.metas.borrow();
                let (_, name, length, asynchronous) = metas
                    .iter()
                    .find(|(v, _, _, _)| Rc::ptr_eq(v, &args[0]))
                    .unwrap();
                assert!(!asynchronous, "KUCOIN-BOOK ordinary filter callback");
                callbacks.borrow_mut().push((name.clone(), *length));
                drop(metas);
                call(ordinary.clone(), actual, args)
            }))
        }),
    ));
}
fn run_combined(kind: &str) -> Value {
    let f = fixture();
    let normal = row("fresh-normal", "limit", 29.);
    let stop = row("fresh-stop", "stop_market", 31.);
    let stored_normal = row("stored-normal", "limit", 11.);
    let stored_stop = row("stored-stop", "stop_market", 17.);
    let stored_profit = row("stored-profit", "take_profit_market", 19.);
    let stored_trailing = row("stored-trailing", "trailing_stop_market", 23.);
    let late = row("late-stop", "stop_market", 41.);
    let first = arr(vec![
        stored_normal.clone(),
        stored_stop.clone(),
        stored_profit.clone(),
        stored_trailing.clone(),
    ]);
    let second = if kind == "combined-live-store" {
        arr(vec![late.clone()])
    } else {
        first.clone()
    };
    let callbacks = Rc::new(RefCell::new(Vec::new()));
    spy_filter(&f.rt, &first, callbacks.clone());
    if !Rc::ptr_eq(&first, &second) {
        spy_filter(&f.rt, &second, callbacks.clone());
    }
    let store = obj(vec![]);
    set(&f.receiver, "store", store.clone());
    let reads = f.reads.clone();
    let a = first.clone();
    let b = second.clone();
    f.rt.getters.borrow_mut().push((
        store,
        "orders".into(),
        Rc::new(move || {
            reads.set(reads.get() + 1);
            Ok(if reads.get() == 1 {
                a.clone()
            } else {
                b.clone()
            })
        }),
    ));
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let captured = Rc::new(RefCell::new(Vec::<V>::new()));
    let result = obj(vec![("marker", n(37.))]);
    let failure = Rc::new("normal failure".to_owned());
    let pending_normal = pending();
    let normal_failed = [
        "combined-normal-failed",
        "combined-both-failed",
        "combined-live-store",
    ]
    .contains(&kind);
    let stop_failed = [
        "combined-stop-failed",
        "combined-both-failed",
        "combined-live-store",
    ]
    .contains(&kind);
    let owner = f.receiver.clone();
    let calls = trace.clone();
    let orders = arr(vec![normal.clone()]);
    let pending = pending_normal.clone();
    let reject = kind == "combined-reject";
    let pending_case = kind == "combined-pending";
    let error = failure.clone();
    set(
        &f.receiver,
        "fetchNormalOrders",
        function(move |actual, args| {
            same(&actual, &owner);
            assert_eq!(args.len(), 0);
            calls.borrow_mut().push("normal".into());
            if reject {
                return Ok(ready(Err(error.clone())));
            }
            Ok(if pending_case {
                pending.clone()
            } else {
                ready(Ok(if normal_failed {
                    Rc::new(Js::Null)
                } else {
                    orders.clone()
                }))
            })
        }),
    );
    let owner = f.receiver.clone();
    let calls = trace.clone();
    let orders = arr(vec![stop.clone()]);
    set(
        &f.receiver,
        "fetchAlgoOrders",
        function(move |actual, args| {
            same(&actual, &owner);
            assert_eq!(args.len(), 0);
            calls.borrow_mut().push("algo".into());
            Ok(ready(Ok(if stop_failed {
                Rc::new(Js::Null)
            } else {
                orders.clone()
            })))
        }),
    );
    let owner = f.receiver.clone();
    let calls = trace.clone();
    let payloads = captured.clone();
    let returned = result.clone();
    set(
        &f.receiver,
        "reconcileFetchedOrderRemainders",
        function(move |actual, args| {
            same(&actual, &owner);
            assert_eq!(args.len(), 1);
            calls.borrow_mut().push("reconcile".into());
            payloads.borrow_mut().push(args[0].clone());
            Ok(returned.clone())
        }),
    );
    let mut future = Box::pin(port::fetch_orders(f.ops.clone(), f.h.clone()));
    if pending_case {
        for _ in 0..3 {
            assert!(
                matches!(
                    std::future::Future::poll(
                        future.as_mut(),
                        &mut std::task::Context::from_waker(std::task::Waker::noop())
                    ),
                    std::task::Poll::Pending
                ),
                "KUCOIN-BOOK sequential normal await"
            );
        }
        assert_eq!(
            *trace.borrow(),
            ["normal"],
            "KUCOIN-BOOK no algo before normal settlement"
        );
        resolve(&pending_normal, Ok(arr(vec![normal.clone()])));
    }
    let value = settle(future);
    assert_eq!(
        f.emitted.borrow().len(),
        0,
        "KUCOIN-BOOK combined has no local catch"
    );
    if reject {
        let Err(error) = value else {
            panic!("expected child rejection")
        };
        assert!(
            Rc::ptr_eq(&error, &failure),
            "KUCOIN-BOOK original child rejection"
        );
        assert_eq!(error.as_str(), "normal failure");
        assert_eq!(*trace.borrow(), ["normal"]);
        assert!(captured.borrow().is_empty());
        assert_eq!(f.reads.get(), 0);
        return json!({"error":error.as_str(),"trace":*trace.borrow(),"payloads":[],"callbacks":[],"reads":0});
    }
    let value = value.unwrap();
    same(&value, &result);
    assert_eq!(keys(&value), ["marker"]);
    num(&at(&value, "marker"), 37.);
    assert_eq!(
        *trace.borrow(),
        ["normal", "algo", "reconcile"],
        "KUCOIN-BOOK sequential child and reconciliation order"
    );
    assert_eq!(captured.borrow().len(), 1, "KUCOIN-BOOK one reconciliation");
    let payload = rows(&captured.borrow()[0]);
    let expected = if kind == "combined-live-store" {
        vec![stored_normal, late]
    } else {
        let mut out = if normal_failed {
            vec![stored_normal]
        } else {
            vec![normal]
        };
        if stop_failed {
            out.extend([stored_stop, stored_profit, stored_trailing]);
        } else {
            out.push(stop);
        }
        out
    };
    let expected_values: &[(&str, &str, f64)] = match kind {
        "combined-normal-failed" => &[
            ("stored-normal", "limit", 11.),
            ("fresh-stop", "stop_market", 31.),
        ],
        "combined-stop-failed" => &[
            ("fresh-normal", "limit", 29.),
            ("stored-stop", "stop_market", 17.),
            ("stored-profit", "take_profit_market", 19.),
            ("stored-trailing", "trailing_stop_market", 23.),
        ],
        "combined-both-failed" => &[
            ("stored-normal", "limit", 11.),
            ("stored-stop", "stop_market", 17.),
            ("stored-profit", "take_profit_market", 19.),
            ("stored-trailing", "trailing_stop_market", 23.),
        ],
        "combined-live-store" => &[
            ("stored-normal", "limit", 11.),
            ("late-stop", "stop_market", 41.),
        ],
        _ => &[
            ("fresh-normal", "limit", 29.),
            ("fresh-stop", "stop_market", 31.),
        ],
    };
    assert_eq!(
        payload.len(),
        expected_values.len(),
        "KUCOIN-BOOK independent combined count"
    );
    for (index, value) in payload.iter().enumerate() {
        same(value, &expected[index]);
        assert_eq!(keys(value), ["id", "type", "marker"]);
        let (id, kind, marker) = expected_values[index];
        string(&at(value, "id"), id);
        string(&at(value, "type"), kind);
        num(&at(value, "marker"), marker);
    }
    let mut expected_callbacks = Vec::new();
    if normal_failed {
        expected_callbacks.push((String::new(), 1));
    }
    if stop_failed {
        expected_callbacks.push(("isStopType".to_owned(), 1));
    }
    assert_eq!(
        *callbacks.borrow(),
        expected_callbacks,
        "KUCOIN-BOOK filter callback metadata/order"
    );
    assert_eq!(
        f.reads.get(),
        usize::from(normal_failed) + usize::from(stop_failed),
        "KUCOIN-BOOK live store fallback count"
    );
    json!({"value":tag(&value),"trace":*trace.borrow(),"payloads":captured.borrow().iter().map(tag).collect::<Vec<_>>(),"callbacks":*callbacks.borrow(),"reads":f.reads.get()})
}
fn spy_map(rt: &Rc<Runtime>, array: &V, callbacks: Rc<Cell<usize>>) {
    let ordinary = get(array, "map").unwrap();
    let runtime = rt.clone();
    let owner = array.clone();
    rt.getters.borrow_mut().push((
        array.clone(),
        "map".into(),
        Rc::new(move || {
            let ordinary = ordinary.clone();
            let runtime = runtime.clone();
            let owner = owner.clone();
            let callbacks = callbacks.clone();
            Ok(function(move |receiver, args| {
                same(&receiver, &owner);
                assert_eq!(args.len(), 1);
                metadata(&runtime, &args[0], "", 1);
                callbacks.set(callbacks.get() + 1);
                call(ordinary.clone(), receiver, args)
            }))
        }),
    ));
}
fn assert_typed_snapshot_boundary() {
    use safe_cex_types::Order;
    let mut input = json!({"id":"same","symbol":"BTC","type":"limit","side":"buy","status":"open","price":11,"amount":3,"filled":0.5,"remaining":2.5,"reduceOnly":false,"lastUpdate":17,"extra":{"remainingNative":10},"info":{"marker":71}});
    let decoded: Order = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(decoded.id, "same");
    assert_eq!(decoded.symbol, "BTC");
    assert_eq!(decoded.price.to_number().to_bits(), 11f64.to_bits());
    assert_eq!(decoded.remaining.to_bits(), 2.5f64.to_bits());
    assert_eq!(decoded.extra.as_ref().unwrap().remaining_native, Some(10.));
    assert_eq!(decoded.info.as_ref().unwrap()["marker"].as_u64(), Some(71));
    // Observe the canonical Order ingress directly. The retired typed venue
    // reconcile copy was an identity with an empty Store at this boundary.
    let result = [decoded];
    assert_eq!(result.len(), 1, "KUCOIN-BOOK typed snapshot row floor");
    input["info"]["marker"] = json!(101);
    assert_eq!(input["info"]["marker"].as_u64(), Some(101));
    assert_eq!(
        result[0].info.as_ref().unwrap()["marker"].as_u64(),
        Some(71),
        "KUCOIN-BOOK typed ingress retains owned info snapshot"
    );
    assert_eq!(result[0].remaining.to_bits(), 2.5f64.to_bits());
}
fn run_reconcile(kind: &str) -> Value {
    let f = fixture();
    let previous = match kind {
        "reconcile-negative" => n(-1.),
        "reconcile-infinite" => n(f64::INFINITY),
        "reconcile-numeric-string" => s("3"),
        "reconcile-negative-zero" => n(-0.),
        "reconcile-equal" => n(10.),
        _ => n(3.),
    };
    let wanted_id = if kind == "reconcile-distinct-id" {
        "7"
    } else {
        "same"
    };
    let stored = obj(vec![
        (
            "id",
            if kind == "reconcile-distinct-id" {
                n(7.)
            } else {
                s("same")
            },
        ),
        ("extra", obj(vec![("remainingNative", previous)])),
    ]);
    let input_extra = obj(vec![("remainingNative", n(10.)), ("nested", f.raw.clone())]);
    let order = obj(vec![
        ("id", s(wanted_id)),
        ("symbol", s("BTC")),
        ("remaining", n(2.5)),
        ("extra", input_extra.clone()),
        ("info", f.raw.clone()),
    ]);
    let fetched = arr(vec![order.clone()]);
    let orders = if kind == "reconcile-no-existing" {
        arr(vec![])
    } else if kind == "reconcile-duplicate" {
        arr(vec![
            obj(vec![
                ("id", s("same")),
                ("extra", obj(vec![("remainingNative", n(1.))])),
            ]),
            stored.clone(),
        ])
    } else {
        arr(vec![stored.clone()])
    };
    set(&f.receiver, "store", obj(vec![("orders", orders.clone())]));
    let callbacks = Rc::new(Cell::new(0));
    spy_map(&f.rt, &orders, callbacks.clone());
    spy_map(&f.rt, &fetched, callbacks.clone());
    let contract_calls = Rc::new(Cell::new(0));
    let count = contract_calls.clone();
    let owner = f.receiver.clone();
    set(
        &f.receiver,
        "contractSizeFor",
        function(move |actual, args| {
            same(&actual, &owner);
            assert_eq!(args.len(), 1);
            string(&args[0], "BTC");
            count.set(count.get() + 1);
            Ok(n(0.25))
        }),
    );
    let constructs = Rc::new(Cell::new(0));
    let count = constructs.clone();
    let gets = Rc::new(RefCell::new(Vec::new()));
    let capture_gets = gets.clone();
    let ctor = function(move |_, args| {
        count.set(count.get() + 1);
        assert_eq!(args.len(), 1, "KUCOIN-BOOK Map constructor arity");
        let pairs = rows(&args[0])
            .iter()
            .map(|pair| {
                let tuple = rows(pair);
                assert_eq!(tuple.len(), 2, "KUCOIN-BOOK Map entry arity");
                (tuple[0].clone(), tuple[1].clone())
            })
            .collect::<Vec<_>>();
        let map = obj(vec![]);
        let owner = map.clone();
        let gets = capture_gets.clone();
        set(
            &map,
            "get",
            function(move |actual, args| {
                same(&actual, &owner);
                assert_eq!(args.len(), 1);
                gets.borrow_mut().push(args[0].clone());
                Ok(pairs
                    .iter()
                    .rev()
                    .find(|(key, _)| same_value_zero(key, &args[0]))
                    .map(|(_, value)| value.clone())
                    .unwrap_or_else(u))
            }),
        );
        Ok(map)
    });
    f.rt.globals.borrow_mut().insert("Map".into(), ctor.clone());
    let construction = ConstructionOps {
        derive: Rc::new(|_, _, _, _| panic!("no derive")),
        superclass: Rc::new(|_| panic!("no superclass")),
        super_construct: Rc::new(|_, _, _| panic!("no super")),
        construct: Rc::new(move |actual, args| {
            same(&actual, &ctor);
            call(actual, u(), args)
        }),
    };
    let result = port::reconcile_fetched_order_remainders(
        f.ops.clone(),
        &construction,
        f.h.clone(),
        fetched.clone(),
    )
    .unwrap();
    assert_eq!(constructs.get(), 1, "KUCOIN-BOOK Map constructor count");
    assert_eq!(callbacks.get(), 2, "KUCOIN-BOOK map callback count");
    assert_eq!(gets.borrow().len(), 1, "KUCOIN-BOOK Map get count");
    same(&gets.borrow()[0], &at(&order, "id"));
    string(&gets.borrow()[0], wanted_id);
    assert!(
        !Rc::ptr_eq(&result, &fetched),
        "KUCOIN-BOOK new outer array"
    );
    let result_rows = rows(&result);
    assert_eq!(result_rows.len(), 1, "KUCOIN-BOOK reconciled order count");
    let actual = &result_rows[0];
    assert_eq!(keys(actual), ["id", "symbol", "remaining", "extra", "info"]);
    string(&at(actual, "id"), wanted_id);
    string(&at(actual, "symbol"), "BTC");
    same(&at(actual, "info"), &f.raw);
    let changed = [
        "reconcile-clamp",
        "reconcile-duplicate",
        "reconcile-negative-zero",
    ]
    .contains(&kind);
    assert_eq!(
        contract_calls.get(),
        usize::from(changed),
        "KUCOIN-BOOK contract callback count"
    );
    let extra = at(actual, "extra");
    assert_eq!(keys(&extra), ["remainingNative", "nested"]);
    same(&at(&extra, "nested"), &f.raw);
    num(&at(&f.raw, "marker"), 71.);
    if changed {
        assert!(
            !Rc::ptr_eq(actual, &order),
            "KUCOIN-BOOK changed order copied"
        );
        assert!(
            !Rc::ptr_eq(&extra, &input_extra),
            "KUCOIN-BOOK changed extra copied"
        );
        num(
            &at(actual, "remaining"),
            if kind == "reconcile-negative-zero" {
                -0.
            } else {
                0.75
            },
        );
        num(
            &at(&extra, "remainingNative"),
            if kind == "reconcile-negative-zero" {
                -0.
            } else {
                3.
            },
        );
    } else {
        same(actual, &order);
        same(&extra, &input_extra);
        num(&at(actual, "remaining"), 2.5);
        num(&at(&extra, "remainingNative"), 10.);
    }
    num(&at(&order, "remaining"), 2.5);
    num(&at(&input_extra, "remainingNative"), 10.);
    set(&f.raw, "marker", n(101.));
    num(&at(&at(actual, "info"), "marker"), 101.);
    num(&at(&at(&extra, "nested"), "marker"), 101.);
    json!({"value":tag(&result),"original":tag(&order),"constructs":constructs.get(),"callbacks":callbacks.get(),"gets":gets.borrow().iter().map(tag).collect::<Vec<_>>(),"contractCalls":contract_calls.get()})
}
fn run_delegate(kind: &str) -> Value {
    let f = fixture();
    let socket = obj(vec![]);
    set(&f.receiver, "publicWebsocket", socket.clone());
    let argument = obj(vec![("marker", n(11.))]);
    let callback = function(|_, _| panic!("callback invoked eagerly"));
    let cleanup = obj(vec![("marker", n(17.))]);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let owner = socket.clone();
    let arg = argument.clone();
    let cb = callback.clone();
    let output = cleanup.clone();
    let name = if kind == "delegate-ohlcv" {
        "listenOHLCV"
    } else {
        "listenOrderBook"
    };
    set(
        &socket,
        name,
        function(move |actual, args| {
            same(&actual, &owner);
            assert_eq!(args.len(), 2);
            same(&args[0], &arg);
            same(&args[1], &cb);
            count.set(count.get() + 1);
            Ok(output.clone())
        }),
    );
    let result = if kind == "delegate-ohlcv" {
        port::listen_ohlcv(&f.h, argument, callback)
    } else {
        port::listen_order_book(&f.h, argument, callback)
    }
    .unwrap();
    same(&result, &cleanup);
    assert_eq!(keys(&result), ["marker"]);
    num(&at(&result, "marker"), 17.);
    assert_eq!(calls.get(), 1, "KUCOIN-BOOK one stream delegation");
    set(&cleanup, "marker", n(101.));
    num(&at(&result, "marker"), 101.);
    json!({"value":tag(&result),"calls":calls.get()})
}
#[test]
fn native_futures_book_values_precede_source_projection() {
    let cases = [
        "normal-success",
        "normal-numeric-code",
        "normal-default",
        "normal-nonarray",
        "normal-missing-data",
        "normal-live-data",
        "normal-mapper-get",
        "normal-transport",
        "algo-success",
        "algo-numeric-code",
        "algo-default",
        "algo-nonarray",
        "algo-missing-data",
        "algo-live-data",
        "algo-mapper-get",
        "algo-transport",
        "combined-success",
        "combined-normal-failed",
        "combined-stop-failed",
        "combined-both-failed",
        "combined-live-store",
        "combined-pending",
        "combined-reject",
        "reconcile-clamp",
        "reconcile-no-existing",
        "reconcile-duplicate",
        "reconcile-negative",
        "reconcile-infinite",
        "reconcile-numeric-string",
        "reconcile-negative-zero",
        "reconcile-equal",
        "reconcile-distinct-id",
        "delegate-ohlcv",
        "delegate-book",
    ];
    assert_eq!(cases.len(), 34, "independent authored case floor");
    assert_typed_snapshot_boundary();
    let native = cases.map(|kind| {
        if kind.starts_with("combined-") {
            run_combined(kind)
        } else if kind.starts_with("reconcile-") {
            run_reconcile(kind)
        } else if kind.starts_with("delegate-") {
            run_delegate(kind)
        } else {
            run_book(kind)
        }
    });
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/futures-native-order-reads.cjs"
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
        .write_all(serde_json::to_string(cases.as_slice()).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source book assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 34, "independent source case count");
    for (index, (native, source)) in native.iter().zip(source.iter()).enumerate() {
        assert_eq!(native, source, "source case {index}/{}", cases[index]);
    }
}
