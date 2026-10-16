#[allow(dead_code)]
#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::{
    futures_native_formatters as formatting, futures_native_mapping as mapping,
    order_control::OrderControlOps, public_streams::StreamOps, spot_private_class::PrivateClassOps,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
use values::*;
fn text(value: &V) -> String {
    match value.as_ref() {
        Js::B(value) => value.to_string(),
        Js::Null => "null".into(),
        _ => values::text(value),
    }
}
fn same(a: &V, b: &V) {
    assert!(
        Rc::ptr_eq(a, b),
        "KUCOIN-MAPPING original reference identity"
    );
}
fn at(v: &V, k: &str) -> V {
    get(v, k).unwrap()
}
fn rows(v: &V) -> Vec<V> {
    let Js::A(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
fn keys(v: &V) -> Vec<String> {
    let Js::O(a) = v.as_ref() else {
        panic!("native object")
    };
    a.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn num(v: &V, w: f64) {
    let Js::N(n) = v.as_ref() else {
        panic!("KUCOIN-MAPPING native number kind")
    };
    if w.is_nan() {
        assert!(n.is_nan(), "KUCOIN-MAPPING native NaN");
    } else {
        assert_eq!(
            n.to_bits(),
            w.to_bits(),
            "KUCOIN-MAPPING native number bits"
        );
    }
}
fn strval(v: &V, w: &str) {
    let Js::S(s) = v.as_ref() else {
        panic!("KUCOIN-MAPPING native string kind")
    };
    assert_eq!(s, w, "KUCOIN-MAPPING native string value");
}
fn boolval(v: &V, w: bool) {
    let Js::B(b) = v.as_ref() else {
        panic!("KUCOIN-MAPPING native bool kind")
    };
    assert_eq!(*b, w);
}
fn tag(v: &V) -> Value {
    match v.as_ref() {
        Js::N(n) => {
            json!({"number":safe_cex_num::js_number_string(*n),"negativeZero":n.to_bits()==(-0f64).to_bits()})
        }
        Js::A(_) => Value::Array(rows(v).iter().map(tag).collect()),
        Js::O(a) => Value::Object(
            a.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), tag(v)))
                .collect(),
        ),
        _ => literal(v),
    }
}
fn frame(rt: &Rc<Runtime>, receiver: &V) -> Rc<StreamOps<V, E>> {
    let mut h = Rc::try_unwrap(rt.frame(receiver)).ok().unwrap();
    let original = h.get.clone();
    h.get = Rc::new(move |v, k| {
        if matches!(v.as_ref(), Js::Null | Js::U) {
            return Err(Rc::new(format!(
                "Cannot read properties of {} (reading '{k}')",
                if matches!(v.as_ref(), Js::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        if matches!(v.as_ref(), Js::S(_)) {
            match k {
                "slice" => {
                    return Ok(function(|r, a| {
                        let units = text(&r).encode_utf16().collect::<Vec<_>>();
                        let lo = (number(&a[0]) as usize).min(units.len());
                        let hi = (number(&a[1]) as usize).min(units.len());
                        Ok(s(&String::from_utf16(&units[lo..hi.max(lo)]).unwrap()))
                    }));
                }
                "toLowerCase" => {
                    return Ok(function(|r, a| {
                        assert_eq!(a.len(), 0);
                        Ok(s(&text(&r).to_lowercase()))
                    }));
                }
                _ => {}
            }
        }
        original(v, k)
    });
    let original = h.intrinsic.clone();
    h.intrinsic = Rc::new(move |name, a| match name {
        "strict-null" => Ok(b(matches!(a[0].as_ref(), Js::Null))),
        _ => original(name, a),
    });
    Rc::new(h)
}
struct Fixture {
    rt: Rc<Runtime>,
    receiver: V,
    h: Rc<StreamOps<V, E>>,
    ops: Rc<PrivateClassOps<V, E>>,
    control: Rc<OrderControlOps<V, E>>,
    ids: Rc<Cell<usize>>,
    clock_calls: Rc<Cell<usize>>,
    errors: Rc<RefCell<Vec<E>>>,
    calls: Rc<RefCell<Vec<String>>>,
    position: V,
    opts: Rc<RefCell<V>>,
}
fn fixture() -> Fixture {
    let rt = Rc::new(Runtime::default());
    let receiver = obj(vec![]);
    let h = frame(&rt, &receiver);
    let ops = rt.operations();
    let clock_calls = Rc::new(Cell::new(0));
    let clock_count = clock_calls.clone();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let ids = Rc::new(Cell::new(0));
    let count = ids.clone();
    for (name, value) in [
        (
            "Boolean",
            function(|_, a| {
                assert_eq!(a.len(), 1);
                Ok(b(truthy(&a[0])))
            }),
        ),
        (
            "OrderType",
            obj(vec![
                ("Limit", s("limit")),
                ("Market", s("market")),
                ("StopLoss", s("stop_market")),
                ("TakeProfit", s("take_profit_market")),
                ("TrailingStopLoss", s("trailing_stop_market")),
            ]),
        ),
        (
            "OrderSide",
            obj(vec![("Buy", s("buy")), ("Sell", s("sell"))]),
        ),
        (
            "OrderTimeInForce",
            obj(vec![
                ("GoodTillCancel", s("GoodTillCancel")),
                ("ImmediateOrCancel", s("ImmediateOrCancel")),
                ("FillOrKill", s("FillOrKill")),
                ("PostOnly", s("PostOnly")),
            ]),
        ),
        (
            "ORDER_STATUS",
            obj(vec![("new", s("open")), ("done", s("closed"))]),
        ),
        (
            "ORDER_TYPE",
            obj(vec![
                ("limit", s("limit")),
                ("market", s("market")),
                ("stop", s("stop_market")),
            ]),
        ),
        (
            "TIME_IN_FORCE",
            obj(vec![
                ("GTC", s("GoodTillCancel")),
                ("IOC", s("ImmediateOrCancel")),
            ]),
        ),
        (
            "virtualClock",
            obj(vec![(
                "getCurrentTimeMs",
                function(move |_, a| {
                    assert_eq!(a.len(), 0);
                    clock_count.set(clock_count.get() + 1);
                    Ok(n(97.))
                }),
            )]),
        ),
        (
            "parseFloat",
            function(|_, a| {
                assert_eq!(a.len(), 1);
                Ok(n(safe_cex_num::parse_float(&text(&a[0]))))
            }),
        ),
        (
            "parseInt",
            function(|_, a| {
                assert_eq!(a.len(), 2);
                num(&a[1], 10.);
                Ok(n(safe_cex_num::parse_int(&text(&a[0]), Some(10))))
            }),
        ),
        (
            "multiply",
            function(|_, a| {
                assert_eq!(a.len(), 2);
                Ok(n(safe_cex_num::multiply(number(&a[0]), number(&a[1]))))
            }),
        ),
        (
            "subtract",
            function(|_, a| {
                assert_eq!(a.len(), 2);
                Ok(n(safe_cex_num::subtract(number(&a[0]), number(&a[1]))))
            }),
        ),
        (
            "String",
            function(|_, a| {
                assert_eq!(a.len(), 1);
                Ok(s(&text(&a[0])))
            }),
        ),
        (
            "adjust",
            function(|_, a| {
                assert_eq!(a.len(), 2);
                Ok(n(safe_cex_num::adjust(number(&a[0]), number(&a[1]))))
            }),
        ),
        (
            "omitUndefined",
            function(|_, a| {
                assert_eq!(a.len(), 1);
                let Js::O(fields) = a[0].as_ref() else {
                    panic!("omit object")
                };
                Ok(obj(fields
                    .borrow()
                    .iter()
                    .filter(|(_, v)| !matches!(v.as_ref(), Js::U))
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect()))
            }),
        ),
        (
            "uuid",
            function(move |_, a| {
                assert_eq!(a.len(), 0);
                count.set(count.get() + 1);
                Ok(s("uuid-17"))
            }),
        ),
        ("Error", obj(vec![("marker", n(89.))])),
    ] {
        rt.globals.borrow_mut().insert(name.into(), value);
    }
    let owner = receiver.clone();
    set(
        &receiver,
        "toInternalSymbol",
        function(move |r, a| {
            same(&r, &owner);
            assert_eq!(a.len(), 1);
            Ok(if text(&a[0]) == "XBTUSDTM" {
                s("BTCUSDT")
            } else {
                a[0].clone()
            })
        }),
    );
    let owner = receiver.clone();
    set(
        &receiver,
        "contractSizeFor",
        function(move |r, a| {
            same(&r, &owner);
            assert_eq!(a.len(), 1);
            strval(&a[0], "BTCUSDT");
            Ok(n(0.25))
        }),
    );
    let control = (ops.orders)(&receiver).base.clone();
    let mut control = OrderControlOps {
        value: h.clone(),
        pair: control.pair.clone(),
        close_throw: control.close_throw.clone(),
        has: control.has.clone(),
        boolean: control.boolean.clone(),
        new_error: control.new_error.clone(),
        iterator: control.iterator.clone(),
        next: control.next.clone(),
        some: control.some.clone(),
    };
    let error_ctor = rt.globals.borrow()["Error"].clone();
    let constructed_errors = errors.clone();
    control.new_error = Rc::new(move |ctor, message| {
        same(&ctor, &error_ctor);
        let error = Rc::new(text(&message));
        constructed_errors.borrow_mut().push(error.clone());
        Ok(error)
    });
    let calls = Rc::new(RefCell::new(Vec::new()));
    let opts = Rc::new(RefCell::new(u()));
    let position = obj(vec![("leverage", n(7.)), ("marker", n(71.))]);
    for name in [
        "getPositionForOrder",
        "getKucoinMarginMode",
        "getKucoinPositionSide",
        "resolveCallerClientOrderId",
        "mapStopPriceType",
        "getKucoinStopDirection",
    ] {
        let owner = receiver.clone();
        let position = position.clone();
        let expected_opts = opts.clone();
        let calls = calls.clone();
        set(
            &receiver,
            name,
            function(move |r, a| {
                same(&r, &owner);
                calls.borrow_mut().push(name.into());
                match name {
                    "getPositionForOrder" => {
                        assert_eq!(a.len(), 1);
                        same(&a[0], &expected_opts.borrow());
                        Ok(position.clone())
                    }
                    "getKucoinMarginMode" => {
                        assert_eq!(a.len(), 1);
                        same(&a[0], &position);
                        Ok(s("CROSS"))
                    }
                    "getKucoinPositionSide" => {
                        assert_eq!(a.len(), 2);
                        same(&a[0], &expected_opts.borrow());
                        same(&a[1], &position);
                        Ok(s("BOTH"))
                    }
                    "resolveCallerClientOrderId" => {
                        assert_eq!(a.len(), 1);
                        same(&a[0], &expected_opts.borrow());
                        Ok(at(&a[0], "clientOrderId"))
                    }
                    "mapStopPriceType" => {
                        assert_eq!(a.len(), 1);
                        Ok(if matches!(a[0].as_ref(), Js::U) {
                            s("MP")
                        } else {
                            a[0].clone()
                        })
                    }
                    "getKucoinStopDirection" => {
                        assert_eq!(a.len(), 1);
                        same(&a[0], &expected_opts.borrow());
                        Ok(s("down"))
                    }
                    _ => unreachable!(),
                }
            }),
        );
    }
    Fixture {
        rt,
        receiver,
        h,
        ops,
        control: Rc::new(control),
        ids,
        clock_calls,
        errors,
        calls,
        position,
        opts,
    }
}
fn mapping_case(kind: &str) -> Value {
    let f = fixture();
    let raw = obj(vec![
        ("id", s("order")),
        ("symbol", s("XBTUSDTM")),
        ("type", s("limit")),
        ("status", s("new")),
        ("side", s("buy")),
        ("price", s("11.5")),
        ("size", s("10")),
        ("filledSize", s("2")),
        ("createdAt", s("1700000000123456")),
        ("reduceOnly", b(false)),
        ("timeInForce", s("GTC")),
    ]);
    match kind {
        "map-zero" => set(&raw, "remainSize", n(0.)),
        "map-negative-zero" => set(&raw, "remainSize", s("-0")),
        "map-null" => set(&raw, "remainSize", Rc::new(Js::Null)),
        "map-string" => set(&raw, "remainSize", s("3junk")),
        "map-nan" => set(&raw, "remainSize", s("bad")),
        "map-clock" => set(&raw, "createdAt", n(0.)),
        "map-update" => set(&raw, "updateTime", s("1700000000999999")),
        "map-ts" => set(&raw, "ts", s("1700000000888888")),
        "map-order-time" => set(&raw, "orderTime", s("1700000000777777")),
        "map-sell" => set(&raw, "side", s("sell")),
        "map-fallback" => {
            set(&raw, "type", s("unknown"));
            set(&raw, "status", s("unknown"));
            set(&raw, "timeInForce", s("unknown"));
        }
        "map-market" => set(&raw, "orderType", s("market")),
        "map-empty-stop" => {
            set(&raw, "stop", s(""));
            set(&raw, "stopPrice", s("19"));
        }
        "map-stop-loss" => {
            set(&raw, "stop", s("down"));
            set(&raw, "stopPrice", s("19"));
        }
        "map-stop-profit" => {
            set(&raw, "stop", s("up"));
            set(&raw, "stopPrice", s("23"));
        }
        "map-stop-type" => {
            set(&raw, "orderType", s("stop"));
            set(&raw, "stopType", s("LOSS"));
            set(&raw, "stopPrice", s("19"));
        }
        "map-trailing" => {
            set(&raw, "stop", b(true));
            set(&raw, "stopPriceType", s("TP"));
            set(&raw, "stopPrice", s("29"));
        }
        "map-attached-buy" | "map-attached-sell" => {
            set(&raw, "triggerStopDownPrice", s("31"));
            set(&raw, "triggerStopUpPrice", s("37"));
            if kind == "map-attached-sell" {
                set(&raw, "side", s("sell"));
            }
        }
        "map-legacy-attached" => {
            set(&raw, "stopLoss", s("41"));
            set(&raw, "takeProfit", s("43"));
        }
        "map-base" | "map-live-remain" | "map-status-capture" | "map-tif-capture" => {}
        _ => panic!("unknown mapping case"),
    }
    let remain_reads = Rc::new(Cell::new(0));
    if kind == "map-live-remain" {
        set(&raw, "remainSize", n(17.));
        let reads = remain_reads.clone();
        f.rt.getters.borrow_mut().push((
            raw.clone(),
            "remainSize".into(),
            Rc::new(move || {
                reads.set(reads.get() + 1);
                Ok(n(match reads.get() {
                    1 => 17.,
                    2 => 23.,
                    3 => 29.,
                    _ => panic!("unexpected extra remainder read"),
                }))
            }),
        ));
    }
    let table_reads = Rc::new(Cell::new(0));
    if kind == "map-status-capture" || kind == "map-tif-capture" {
        let (field, table, key) = if kind == "map-status-capture" {
            ("status", "ORDER_STATUS", "new")
        } else {
            ("timeInForce", "TIME_IN_FORCE", "GTC")
        };
        let runtime = f.rt.clone();
        let reads = table_reads.clone();
        f.rt.getters.borrow_mut().push((
            raw.clone(),
            field.into(),
            Rc::new(move || {
                reads.set(reads.get() + 1);
                runtime
                    .globals
                    .borrow_mut()
                    .insert(table.into(), obj(vec![(key, s("wrong-table"))]));
                Ok(s(key))
            }),
        ));
    }
    let result = mapping::map_order(&f.ops, &f.h, raw.clone()).unwrap();
    let rows = rows(&result);
    let attached = kind.starts_with("map-attached-") || kind == "map-legacy-attached";
    assert_eq!(
        rows.len(),
        if attached { 3 } else { 1 },
        "KUCOIN-MAPPING independent order count"
    );
    let base = &rows[0];
    assert_eq!(
        keys(base),
        [
            "id",
            "status",
            "symbol",
            "type",
            "side",
            "price",
            "amount",
            "filled",
            "lastUpdate",
            "reduceOnly",
            "remaining",
            "timeInForce",
            "extra",
            "info"
        ]
    );
    strval(&at(base, "id"), "order");
    strval(&at(base, "status"), "open");
    strval(&at(base, "symbol"), "BTCUSDT");
    let ty = match kind {
        "map-market" => "market",
        "map-stop-loss" | "map-stop-type" => "stop_market",
        "map-stop-profit" => "take_profit_market",
        "map-trailing" => "trailing_stop_market",
        _ => "limit",
    };
    strval(&at(base, "type"), ty);
    let sell = kind == "map-sell" || kind == "map-attached-sell";
    strval(&at(base, "side"), if sell { "sell" } else { "buy" });
    num(
        &at(base, "price"),
        match kind {
            "map-stop-loss" | "map-stop-type" => 19.,
            "map-stop-profit" => 23.,
            "map-trailing" => 29.,
            _ => 11.5,
        },
    );
    num(&at(base, "amount"), 2.5);
    num(&at(base, "filled"), 0.5);
    num(
        &at(base, "lastUpdate"),
        match kind {
            "map-clock" => 97.,
            "map-update" => 1700000000999.,
            "map-ts" => 1700000000888.,
            "map-order-time" => 1700000000777.,
            _ => 1700000000123.,
        },
    );
    boolval(&at(base, "reduceOnly"), false);
    strval(&at(base, "timeInForce"), "GoodTillCancel");
    let remain = match kind {
        "map-zero" => 0.,
        "map-negative-zero" => -0.,
        "map-string" => 3.,
        "map-nan" => f64::NAN,
        "map-live-remain" => 29.,
        _ => 8.,
    };
    let extra = at(base, "extra");
    assert_eq!(keys(&extra), ["remainingNative", "filledInPlacementUnits"]);
    num(&at(&extra, "remainingNative"), remain);
    num(&at(&extra, "filledInPlacementUnits"), 2.);
    num(
        &at(base, "remaining"),
        if remain == 0. { remain } else { remain * 0.25 },
    );
    same(&at(base, "info"), &raw);
    for index in 1..rows.len() {
        let order = &rows[index];
        assert_eq!(
            keys(order),
            [
                "id",
                "status",
                "symbol",
                "type",
                "side",
                "price",
                "amount",
                "filled",
                "lastUpdate",
                "reduceOnly",
                "remaining",
                "timeInForce",
                "extra",
                "info"
            ]
        );
        strval(
            &at(order, "id"),
            if index == 1 {
                "order__stop_loss"
            } else {
                "order__take_profit"
            },
        );
        strval(&at(order, "side"), if sell { "buy" } else { "sell" });
        strval(
            &at(order, "type"),
            if index == 1 {
                "stop_market"
            } else {
                "take_profit_market"
            },
        );
        num(
            &at(order, "price"),
            if kind == "map-legacy-attached" {
                if index == 1 { 41. } else { 43. }
            } else if sell {
                if index == 1 { 37. } else { 31. }
            } else if index == 1 {
                31.
            } else {
                37.
            },
        );
        strval(&at(order, "status"), "open");
        strval(&at(order, "symbol"), "BTCUSDT");
        num(&at(order, "amount"), 2.5);
        num(&at(order, "filled"), 0.5);
        num(&at(order, "remaining"), 2.);
        num(&at(order, "lastUpdate"), 1700000000123.);
        strval(&at(order, "timeInForce"), "GoodTillCancel");
        boolval(&at(order, "reduceOnly"), true);
        same(&at(order, "info"), &raw);
        same(&at(order, "extra"), &extra);
    }
    if kind == "map-live-remain" {
        assert_eq!(
            remain_reads.get(),
            3,
            "KUCOIN-MAPPING live native remainder reads"
        );
        f.rt.getters
            .borrow_mut()
            .retain(|(object, key, _)| !Rc::ptr_eq(object, &raw) || key != "remainSize");
        set(&raw, "remainSize", n(29.));
    }
    if kind == "map-status-capture" || kind == "map-tif-capture" {
        assert_eq!(table_reads.get(), 1, "KUCOIN-MAPPING one table key getter");
        f.rt.getters
            .borrow_mut()
            .retain(|(object, _, _)| !Rc::ptr_eq(object, &raw));
    }
    assert_eq!(
        f.clock_calls.get(),
        usize::from(kind == "map-clock"),
        "KUCOIN-MAPPING native clock call floor"
    );
    assert_eq!(f.errors.borrow().len(), 0);
    set(&raw, "marker", n(101.));
    for order in &rows {
        num(&at(&at(order, "info"), "marker"), 101.);
    }
    json!({"value":tag(&result)})
}
fn formatter_case(kind: &str) -> Value {
    let f = fixture();
    let market = obj(vec![
        ("id", s("XBTUSDTM")),
        ("precision", obj(vec![("price", n(0.1)), ("amount", n(1.))])),
    ]);
    set(
        &f.receiver,
        "store",
        obj(vec![(
            "marketsBySymbol",
            obj(vec![("BTCUSDT", market.clone())]),
        )]),
    );
    let opts = obj(vec![
        ("symbol", s("BTCUSDT")),
        ("type", s("limit")),
        ("side", s("buy")),
        ("amount", n(3.)),
        ("price", n(11.56)),
        ("reduceOnly", b(false)),
        ("timeInForce", s("GoodTillCancel")),
    ]);
    *f.opts.borrow_mut() = opts.clone();
    let algo = kind.starts_with("algo-");
    match kind {
        "format-market" => set(&opts, "type", s("market")),
        "format-sell" => set(&opts, "side", s("sell")),
        "format-ioc" => set(&opts, "timeInForce", s("ImmediateOrCancel")),
        "format-fok" => set(&opts, "timeInForce", s("FillOrKill")),
        "format-post" => set(&opts, "timeInForce", s("PostOnly")),
        "format-attached" => {
            set(&opts, "takeProfit", n(13.56));
            set(&opts, "stopLoss", n(9.56));
            set(&opts, "tpTriggerType", s("TP"));
        }
        "format-caller" => set(&opts, "clientOrderId", s("caller-11")),
        "format-zero" => {
            set(&opts, "amount", n(0.));
            set(&opts, "price", n(0.));
        }
        "format-leverage" => set(&f.position, "leverage", n(0.)),
        "format-missing" | "algo-missing" => set(&opts, "symbol", s("MISSING")),
        "algo-full" => set(&opts, "amount", n(0.)),
        "algo-loss" => {
            set(&opts, "type", s("stop_market"));
            set(&opts, "slTriggerType", s("SL"));
        }
        "algo-profit" => {
            set(&opts, "type", s("take_profit_market"));
            set(&opts, "tpTriggerType", s("TP"));
        }
        "algo-trailing" => set(&opts, "type", s("trailing_stop_market")),
        "algo-default-reduce" => set(&opts, "reduceOnly", Rc::new(Js::Null)),
        "algo-sell" => set(&opts, "side", s("sell")),
        "format-base" | "algo-base" => {}
        _ => panic!("unknown formatter case"),
    }
    let result = if algo {
        formatting::format_algo_order(&f.ops, &f.control, opts.clone())
    } else {
        formatting::format_create_order(&f.ops, &f.control, opts.clone())
    };
    let missing = kind.ends_with("missing");
    let trailing = kind == "algo-trailing";
    if missing || trailing {
        let error = result
            .err()
            .expect("KUCOIN-MAPPING expected formatter error");
        assert_eq!(
            f.errors.borrow().len(),
            1,
            "KUCOIN-MAPPING one Error construction"
        );
        assert!(
            Rc::ptr_eq(&error, &f.errors.borrow()[0]),
            "KUCOIN-MAPPING original constructed error identity"
        );
        assert_eq!(
            error.as_str(),
            if missing {
                "Market MISSING not found"
            } else {
                "KuCoin Futures trailing stop placement is not exposed in the current REST documentation"
            }
        );
        assert_eq!(
            f.ids.get(),
            usize::from(trailing),
            "KUCOIN-MAPPING UUID count before refusal"
        );
        assert_eq!(f.calls.borrow().len(), 0);
        return json!({"error":error.as_str(),"ids":f.ids.get(),"calls":f.calls.borrow().clone()});
    }
    assert_eq!(
        f.errors.borrow().len(),
        0,
        "KUCOIN-MAPPING no Error construction on success"
    );
    let result = result.unwrap();
    let output = if algo {
        result.clone()
    } else {
        let rows = rows(&result);
        assert_eq!(rows.len(), 1);
        rows[0].clone()
    };
    let expected_keys = if algo {
        if kind == "algo-full" {
            vec![
                "symbol",
                "clientOid",
                "side",
                "type",
                "reduceOnly",
                "closeOrder",
            ]
        } else if kind == "algo-loss" || kind == "algo-profit" {
            vec![
                "symbol",
                "clientOid",
                "side",
                "type",
                "reduceOnly",
                "size",
                "stop",
                "stopPrice",
                "stopPriceType",
                "marginMode",
                "positionSide",
            ]
        } else {
            vec!["symbol", "clientOid", "side", "type", "reduceOnly", "size"]
        }
    } else {
        let mut keys = vec!["symbol", "type", "side"];
        if kind != "format-market" && kind != "format-zero" {
            keys.push("price");
        }
        if kind != "format-zero" {
            keys.push("size");
        }
        keys.extend(["leverage", "reduceOnly", "marginMode", "positionSide"]);
        if kind != "format-market" {
            keys.push("timeInForce");
        }
        keys.push("clientOid");
        if kind == "format-attached" {
            keys.extend([
                "triggerStopUpPrice",
                "triggerStopDownPrice",
                "stopPriceType",
            ]);
        }
        if kind == "format-post" {
            keys.push("postOnly");
        }
        keys
    };
    assert_eq!(
        keys(&output),
        expected_keys,
        "KUCOIN-MAPPING complete payload properties"
    );
    strval(&at(&output, "symbol"), "XBTUSDTM");
    strval(
        &at(&output, "side"),
        if kind.ends_with("sell") {
            "sell"
        } else {
            "buy"
        },
    );
    strval(
        &at(&output, "type"),
        if algo || kind == "format-market" {
            "market"
        } else {
            "limit"
        },
    );
    boolval(&at(&output, "reduceOnly"), kind == "algo-default-reduce");
    strval(
        &at(&output, "clientOid"),
        if kind == "format-caller" {
            "caller-11"
        } else {
            "uuid-17"
        },
    );
    assert_eq!(
        f.ids.get(),
        usize::from(kind != "format-caller"),
        "KUCOIN-MAPPING UUID call floor"
    );
    if kind == "algo-full" {
        boolval(&at(&output, "closeOrder"), true);
    } else if kind != "format-zero" {
        strval(&at(&output, "size"), "3");
    }
    let triggered = kind == "algo-loss" || kind == "algo-profit";
    if !algo || triggered {
        strval(&at(&output, "marginMode"), "CROSS");
        strval(&at(&output, "positionSide"), "BOTH");
    }
    if !algo {
        num(
            &at(&output, "leverage"),
            if kind == "format-leverage" { 1. } else { 7. },
        );
        if kind != "format-market" {
            strval(
                &at(&output, "timeInForce"),
                match kind {
                    "format-ioc" => "IOC",
                    "format-fok" => "FOK",
                    _ => "GTC",
                },
            );
        }
        if kind != "format-market" && kind != "format-zero" {
            strval(&at(&output, "price"), "11.6");
        }
    }
    if triggered {
        strval(&at(&output, "stop"), "down");
        strval(&at(&output, "stopPrice"), "11.6");
        strval(
            &at(&output, "stopPriceType"),
            if kind == "algo-loss" { "SL" } else { "TP" },
        );
    }
    if kind == "format-attached" {
        strval(&at(&output, "triggerStopUpPrice"), "13.6");
        strval(&at(&output, "triggerStopDownPrice"), "9.6");
        strval(&at(&output, "stopPriceType"), "TP");
    }
    if kind == "format-post" {
        boolval(&at(&output, "postOnly"), true);
    }
    let wanted = if triggered {
        vec![
            "getPositionForOrder",
            "getKucoinStopDirection",
            "mapStopPriceType",
            "getKucoinMarginMode",
            "getKucoinPositionSide",
        ]
    } else if algo {
        vec![]
    } else {
        let mut calls = vec![
            "getPositionForOrder",
            "getKucoinMarginMode",
            "getKucoinPositionSide",
            "resolveCallerClientOrderId",
        ];
        if kind == "format-attached" {
            calls.push("mapStopPriceType");
        }
        calls
    };
    assert_eq!(
        *f.calls.borrow(),
        wanted,
        "KUCOIN-MAPPING virtual helper call order"
    );
    json!({"value":tag(&result),"ids":f.ids.get(),"calls":f.calls.borrow().clone()})
}
#[test]
fn native_mapping_and_payload_values_precede_source() {
    let cases = [
        "map-base",
        "map-live-remain",
        "map-status-capture",
        "map-tif-capture",
        "map-zero",
        "map-negative-zero",
        "map-null",
        "map-string",
        "map-nan",
        "map-clock",
        "map-update",
        "map-ts",
        "map-order-time",
        "map-sell",
        "map-fallback",
        "map-market",
        "map-empty-stop",
        "map-stop-loss",
        "map-stop-profit",
        "map-stop-type",
        "map-trailing",
        "map-attached-buy",
        "map-attached-sell",
        "map-legacy-attached",
        "format-base",
        "format-market",
        "format-sell",
        "format-ioc",
        "format-fok",
        "format-post",
        "format-attached",
        "format-caller",
        "format-zero",
        "format-leverage",
        "format-missing",
        "algo-base",
        "algo-full",
        "algo-loss",
        "algo-profit",
        "algo-trailing",
        "algo-default-reduce",
        "algo-sell",
        "algo-missing",
    ];
    assert_eq!(cases.len(), 43, "KUCOIN-MAPPING independent case floor");
    let native = cases.map(|kind| {
        if kind.starts_with("map-") {
            mapping_case(kind)
        } else {
            formatter_case(kind)
        }
    });
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/futures-native-mapping-formatters.cjs"
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source.len(), 43, "KUCOIN-MAPPING source case floor");
    for index in 0..43 {
        assert_eq!(
            native[index], source[index],
            "source case {index}/{}",
            cases[index]
        );
    }
}
