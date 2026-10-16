// Reuse finite language operations without constructing a socket in this target.
#[allow(dead_code)]
#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::{
    futures_native_helpers as port, order_control::OrderControlOps, public_streams::StreamOps,
    spot_private_class::PrivateClassOps,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
use values::*;
fn at(value: &V, key: &str) -> V {
    get(value, key).unwrap()
}
fn numeric(value: &V, want: f64) {
    let Js::N(actual) = value.as_ref() else {
        panic!("KUCOIN-HELPER native number kind")
    };
    if want.is_nan() {
        assert!(actual.is_nan(), "KUCOIN-HELPER native NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            want.to_bits(),
            "KUCOIN-HELPER native number bits"
        );
    }
}
fn string(value: &V, want: &str) {
    let Js::S(actual) = value.as_ref() else {
        panic!("KUCOIN-HELPER native string kind")
    };
    assert_eq!(actual, want, "KUCOIN-HELPER native string value");
}
fn same(a: &V, b: &V) {
    assert!(
        Rc::ptr_eq(a, b),
        "KUCOIN-HELPER original reference identity"
    );
}
fn entries(value: &V) -> Vec<V> {
    let Js::A(values) = value.as_ref() else {
        panic!("native array")
    };
    values.borrow().clone()
}
fn keys(value: &V) -> Vec<String> {
    let Js::O(fields) = value.as_ref() else {
        panic!("native object")
    };
    fields.borrow().iter().map(|(key, _)| key.clone()).collect()
}
fn to_text(value: &V) -> String {
    match value.as_ref() {
        Js::B(value) => value.to_string(),
        Js::Null => "null".into(),
        Js::O(_) => "[object Object]".into(),
        _ => text(value),
    }
}
fn pack(value: &V) -> Value {
    match value.as_ref() {
        Js::N(value) => {
            json!({"number":safe_cex_num::js_number_string(*value),"negativeZero":value.to_bits()==(-0f64).to_bits()})
        }
        Js::A(values) => Value::Array(values.borrow().iter().map(pack).collect()),
        Js::O(fields) => Value::Object(
            fields
                .borrow()
                .iter()
                .map(|(key, value)| (key.clone(), pack(value)))
                .collect(),
        ),
        _ => literal(value),
    }
}
fn frame(rt: &Rc<Runtime>, receiver: &V) -> Rc<StreamOps<V, E>> {
    let mut h = Rc::try_unwrap(rt.frame(receiver)).ok().unwrap();
    let get = h.get.clone();
    h.get = Rc::new(move |value, key| {
        if matches!(value.as_ref(), Js::S(_)) {
            match key {
                "toUpperCase" => {
                    return Ok(function(|receiver, _| {
                        Ok(s(&text(&receiver).to_uppercase()))
                    }));
                }
                "endsWith" => {
                    return Ok(function(|receiver, args| {
                        Ok(b(text(&receiver).ends_with(&text(&args[0]))))
                    }));
                }
                _ => {}
            }
        }
        get(value, key)
    });
    let before = h.intrinsic.clone();
    h.intrinsic = Rc::new(move |name, args| {
        if name == "gte" {
            Ok(b(number(&args[0]) >= number(&args[1])))
        } else {
            before(name, args)
        }
    });
    Rc::new(h)
}
struct Fixture {
    rt: Rc<Runtime>,
    ops: Rc<PrivateClassOps<V, E>>,
    h: Rc<StreamOps<V, E>>,
    control: Rc<OrderControlOps<V, E>>,
    refs: BTreeMap<&'static str, V>,
    reads: Rc<Cell<usize>>,
    constructed: Rc<RefCell<Vec<E>>>,
}
fn fixture() -> Fixture {
    let rt = Rc::new(Runtime::default());
    let raw = obj(vec![("marker", n(71.))]);
    let long = obj(vec![
        ("symbol", s("BTC")),
        ("side", s("long")),
        ("contracts", n(1.)),
        ("marker", n(11.)),
    ]);
    let short = obj(vec![
        ("symbol", s("BTC")),
        ("side", s("short")),
        ("contracts", n(0.5)),
        ("marker", n(13.)),
    ]);
    let decimal = obj(vec![
        ("symbol", s("DEC")),
        ("side", s("short")),
        ("contracts", n(0.3)),
        ("marker", n(17.)),
    ]);
    let positions = arr(vec![long.clone(), short.clone(), decimal]);
    let callback_rt = rt.clone();
    rt.getters.borrow_mut().push((
        positions.clone(),
        "find".into(),
        Rc::new(move || {
            let runtime = callback_rt.clone();
            Ok(function(move |receiver, args| {
                let metas = runtime.metas.borrow();
                let (_, name, length, asynchronous) = metas
                    .iter()
                    .find(|(v, _, _, _)| Rc::ptr_eq(v, &args[0]))
                    .unwrap();
                assert_eq!(
                    (name.as_str(), *length, *asynchronous),
                    ("", 1, false),
                    "KUCOIN-HELPER find callback metadata"
                );
                drop(metas);
                let Js::A(rows) = receiver.as_ref() else {
                    panic!("native find receiver")
                };
                let length = rows.borrow().len();
                for index in 0..length {
                    let row = rows.borrow().get(index).cloned().unwrap_or_else(u);
                    let result = call(
                        args[0].clone(),
                        u(),
                        vec![row.clone(), n(index as f64), receiver.clone()],
                    )?;
                    if truthy(&result) {
                        return Ok(row);
                    }
                }
                Ok(u())
            }))
        }),
    ));
    let market = |id: V, size: V| obj(vec![("id", id), ("contractSize", size)]);
    let store = obj(vec![
        ("positions", positions),
        (
            "marketsBySymbol",
            obj(vec![
                ("BTC", market(s("API"), n(0.25))),
                ("ZERO", market(s(""), n(-0.))),
                ("NEG", market(s("NEG"), n(-1.))),
                ("INF", market(s("INF"), n(f64::INFINITY))),
                ("NAN", market(s("NAN"), n(f64::NAN))),
                ("RAW", market(raw.clone(), raw.clone())),
                ("DEC", market(s("DEC"), n(0.1))),
            ]),
        ),
    ]);
    let receiver = obj(vec![("store", store)]);
    for (key, value) in [
        ("String", function(|_, args| Ok(s(&to_text(&args[0]))))),
        (
            "divide",
            function(|_, args| {
                Ok(n(safe_cex_num::divide(
                    number(&args[0]),
                    number(&args[1]),
                    None,
                )))
            }),
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
        (
            "toBinance",
            function(|_, args| {
                string(&args[1], "kucoin");
                Ok(s(&safe_cex_types::symbol_converter::to_binance(
                    &text(&args[0]),
                    &safe_cex_types::ExchangeName::Kucoin,
                )))
            }),
        ),
        (
            "MarginType",
            obj(vec![("Cross", s("cross")), ("Isolated", s("isolated"))]),
        ),
        (
            "PositionMode",
            obj(vec![("Hedged", s("hedged")), ("OneWay", s("oneway"))]),
        ),
        (
            "PositionSide",
            obj(vec![("Long", s("long")), ("Short", s("short"))]),
        ),
        (
            "OrderSide",
            obj(vec![("Buy", s("buy")), ("Sell", s("sell"))]),
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
            "OrderStopTriggerType",
            obj(vec![
                ("MarkPrice", s("mark_price")),
                ("IndexPrice", s("index_price")),
            ]),
        ),
        ("Error", function(|_, _| Ok(u()))),
    ] {
        rt.globals.borrow_mut().insert(key.into(), value);
    }
    let mut ops = Rc::try_unwrap(rt.operations()).ok().unwrap();
    let frames = rt.clone();
    ops.frame = Rc::new(move |receiver| frame(&frames, receiver));
    ops.template = Rc::new(|value| Ok(to_text(value).encode_utf16().collect()));
    let ops = Rc::new(ops);
    let h = frame(&rt, &receiver);
    let native = h.clone();
    set(
        &receiver,
        "toKucoinSymbol",
        function(move |_, args| Ok(port::to_kucoin_symbol(&native, args[0].clone()))),
    );
    let native = h.clone();
    set(
        &receiver,
        "contractSizeFor",
        function(move |_, args| port::contract_size_for(&native, args[0].clone())),
    );
    let native = h.clone();
    set(
        &receiver,
        "parsePositionMode",
        function(move |_, args| port::parse_position_mode(&native, args[0].clone())),
    );
    let native = h.clone();
    let language = ops.clone();
    set(
        &receiver,
        "isAlgoOrder",
        function(move |_, args| port::is_algo_order(&language, &native, args[0].clone())),
    );
    let constructed = Rc::new(RefCell::new(Vec::new()));
    let created = constructed.clone();
    let constructor = rt.globals.borrow()["Error"].clone();
    let control = Rc::new(OrderControlOps {
        value: h.clone(),
        pair: Rc::new(|_| panic!("unused pair")),
        close_throw: Rc::new(|_, _| panic!("unused close")),
        has: Rc::new(|_, _| panic!("unused has")),
        boolean: Rc::new(b),
        new_error: Rc::new(move |actual, message| {
            same(&actual, &constructor);
            let error = Rc::new(to_text(&message));
            created.borrow_mut().push(error.clone());
            Ok(error)
        }),
        iterator: Rc::new(|_| panic!("unused iterator")),
        next: Rc::new(|_| panic!("unused next")),
        some: Rc::new(|_, _| panic!("unused some")),
    });
    Fixture {
        rt,
        ops,
        h,
        control,
        refs: BTreeMap::from([
            ("raw", raw),
            ("long", long),
            ("short", short),
            (
                "replacement",
                obj(vec![
                    ("symbol", s("BTC")),
                    ("side", s("short")),
                    ("contracts", n(2.)),
                    ("marker", n(31.)),
                ]),
            ),
        ]),
        reads: Rc::new(Cell::new(0)),
        constructed,
    }
}
fn decode(value: &Value, refs: &BTreeMap<&str, V>) -> V {
    if let Some(object) = value.as_object() {
        if object.len() == 1 {
            if object.contains_key("$u") {
                return u();
            }
            if object.contains_key("$inf") {
                return n(f64::INFINITY);
            }
            if object.contains_key("$nan") {
                return n(f64::NAN);
            }
            if object.contains_key("$negativeZero") {
                return n(-0.);
            }
            if let Some(name) = object.get("$ref") {
                return refs[name.as_str().unwrap()].clone();
            }
        }
        return Rc::new(Js::O(RefCell::new(
            object
                .iter()
                .map(|(k, v)| (k.clone(), decode(v, refs)))
                .collect(),
        )));
    }
    if let Some(array) = value.as_array() {
        return arr(array.iter().map(|v| decode(v, refs)).collect());
    }
    values::decode(value)
}
enum Want {
    S(&'static str),
    N(f64),
    B(bool),
    U,
    Pair(Box<Want>, Box<Want>),
    Ref(&'static str),
    Error(&'static str),
}
struct Case {
    name: &'static str,
    method: &'static str,
    args: Value,
    want: Want,
    reads: usize,
}
fn pair(a: Want, b: Want) -> Want {
    Want::Pair(Box::new(a), Box::new(b))
}
fn assert_value(actual: &V, want: &Want, f: &Fixture) {
    match want {
        Want::S(want) => string(actual, want),
        Want::N(want) => numeric(actual, *want),
        Want::B(want) => assert!(
            matches!(actual.as_ref(),Js::B(value)if value==want),
            "KUCOIN-HELPER native boolean"
        ),
        Want::U => assert!(
            matches!(actual.as_ref(), Js::U),
            "KUCOIN-HELPER native undefined"
        ),
        Want::Pair(first, second) => {
            let values = entries(actual);
            assert_eq!(values.len(), 2, "KUCOIN-HELPER native tuple count");
            assert_value(&values[0], first, f);
            assert_value(&values[1], second, f);
        }
        Want::Ref(name) => {
            same(actual, &f.refs[name]);
            if *name == "raw" {
                assert_eq!(keys(actual), ["marker"]);
                numeric(&at(actual, "marker"), 71.);
            } else {
                assert_eq!(keys(actual), ["symbol", "side", "contracts", "marker"]);
                string(&at(actual, "symbol"), "BTC");
                string(
                    &at(actual, "side"),
                    if *name == "replacement" {
                        "short"
                    } else {
                        name
                    },
                );
                numeric(
                    &at(actual, "contracts"),
                    if *name == "long" {
                        1.
                    } else if *name == "replacement" {
                        2.
                    } else {
                        0.5
                    },
                );
                numeric(
                    &at(actual, "marker"),
                    if *name == "long" {
                        11.
                    } else if *name == "replacement" {
                        31.
                    } else {
                        13.
                    },
                );
            }
            set(&f.refs[name], "marker", n(101.));
            numeric(&at(actual, "marker"), 101.);
        }
        Want::Error(_) => panic!("expected native rejection"),
    }
}
fn run(case: &Case) -> Value {
    let f = fixture();
    let args = case
        .args
        .as_array()
        .unwrap()
        .iter()
        .map(|value| decode(value, &f.refs))
        .collect::<Vec<_>>();
    if matches!(
        case.name,
        "internal-override" | "api-override" | "remaining-override"
    ) {
        let receiver = (f.h.capture)("this").unwrap();
        let owner = receiver.clone();
        let count = f.reads.clone();
        let (method, argument, result) = match case.name {
            "internal-override" => ("toKucoinSymbol", args[0].clone(), s("XBTUSDTM")),
            "api-override" => ("toKucoinSymbol", args[0].clone(), f.refs["raw"].clone()),
            _ => ("contractSizeFor", at(&args[0], "symbol"), n(0.5)),
        };
        set(
            &receiver,
            method,
            function(move |actual, arguments| {
                same(&actual, &owner);
                assert_eq!(arguments.len(), 1, "KUCOIN-HELPER virtual argument count");
                same(&arguments[0], &argument);
                count.set(count.get() + 1);
                Ok(result.clone())
            }),
        );
    }
    if case.name == "position-live" {
        let positions = (f.h.capture)("this.store.positions").unwrap();
        let replacement = f.refs["replacement"].clone();
        let reads = f.reads.clone();
        f.rt.getters.borrow_mut().push((
            args[0].clone(),
            "symbol".into(),
            Rc::new(move || {
                let index = reads.get();
                reads.set(index + 1);
                if index == 0 {
                    let Js::A(rows) = positions.as_ref() else {
                        panic!("live positions")
                    };
                    rows.borrow_mut()[1] = replacement.clone();
                }
                Ok(s("BTC"))
            }),
        ));
    }
    let property = match case.name {
        "row-live" => Some((0, "positionSide", vec!["long", "BOTH"])),
        "v2-live" => Some((0, "side", vec!["buy", "sell"])),
        "virtual-live" => Some((0, "id", vec!["plain", "x__take_profit"])),
        "algo-live" => Some((0, "type", vec!["limit", "limit", "trailing_stop_market"])),
        "constructor-live" => Some((0, "side", vec!["bad", "bad", "bad"])),
        _ => None,
    };
    if let Some((index, key, values)) = property {
        let count = f.reads.clone();
        let runtime = f.rt.clone();
        let replacement = case.name == "constructor-live";
        f.rt.getters.borrow_mut().push((
            args[index].clone(),
            key.into(),
            Rc::new(move || {
                let i = count.get();
                count.set(i + 1);
                if replacement && i == 2 {
                    runtime
                        .globals
                        .borrow_mut()
                        .insert("Error".into(), function(|_, _| panic!("late constructor")));
                }
                Ok(s(values[i]))
            }),
        ));
    }
    let arg = |i| args.get(i).cloned().unwrap_or_else(u);
    let result = match case.method {
        "toKucoinSymbol" => Ok(port::to_kucoin_symbol(&f.h, arg(0))),
        "toInternalSymbol" => port::to_internal_symbol(&f.h, arg(0)),
        "toApiSymbol" => port::to_api_symbol(&f.h, arg(0)),
        "contractSizeFor" => port::contract_size_for(&f.h, arg(0)),
        "nativeAmountForCancelReplace" => port::native_amount_for_cancel_replace(&f.h, arg(0)),
        "parseMarginType" => port::parse_margin_type(&f.ops, &f.h, arg(0), arg(1)),
        "parsePositionMode" => port::parse_position_mode(&f.h, arg(0)),
        "parseRowPositionMode" => port::parse_row_position_mode(&f.h, arg(0)),
        "getOrderSideV2" => port::get_order_side_v2(&f.h, arg(0), arg(1)),
        "getOrderSide" => port::get_order_side(&f.ops, &f.control, arg(0), arg(1)),
        "getAlgoOrderSide" => port::get_algo_order_side(&f.ops, &f.control, arg(0)),
        "getKucoinMarginMode" => port::get_kucoin_margin_mode(&f.h, arg(0)),
        "getKucoinPositionSide" => port::get_kucoin_position_side(&f.h, arg(0), arg(1)),
        "mapStopPriceType" => port::map_stop_price_type(&f.h, arg(0)),
        "getKucoinStopDirection" => port::get_kucoin_stop_direction(&f.ops, &f.control, arg(0)),
        "isVirtualTPSLOrder" => port::is_virtual_tpsl_order(&f.h, arg(0)),
        "isAlgoOrder" => port::is_algo_order(&f.ops, &f.h, arg(0)),
        "getOrderPlanType" => port::get_order_plan_type(&f.ops, &f.control, arg(0)),
        "getPositionForOrder" => port::get_position_for_order(&f.ops, f.h.clone(), arg(0)),
        "getOrderPlanTypeAndAmount" => {
            port::get_order_plan_type_and_amount(&f.ops, f.control.clone(), arg(0))
        }
        _ => panic!("unknown authored method"),
    };
    assert_eq!(
        f.reads.get(),
        case.reads,
        "KUCOIN-HELPER independent live read/call count"
    );
    match result {
        Ok(actual) => {
            assert_eq!(f.constructed.borrow().len(), 0);
            assert_value(&actual, &case.want, &f);
            json!({"value":pack(&actual),"reads":f.reads.get(),"constructed":0})
        }
        Err(error) => {
            let Want::Error(message) = &case.want else {
                panic!("unexpected native rejection: {error}")
            };
            assert_eq!(
                error.as_str(),
                *message,
                "KUCOIN-HELPER native rejection message"
            );
            assert_eq!(
                f.constructed.borrow().len(),
                1,
                "KUCOIN-HELPER constructed error count"
            );
            assert!(
                Rc::ptr_eq(&error, &f.constructed.borrow()[0]),
                "KUCOIN-HELPER original constructed error handle"
            );
            json!({"error":error.as_str(),"reads":f.reads.get(),"constructed":1})
        }
    }
}

fn cases() -> Vec<Case> {
    use Want::*;
    let mut cases = Vec::new();
    let mut add = |name, method, args, want, reads| {
        cases.push(Case {
            name,
            method,
            args,
            want,
            reads,
        })
    };
    add("symbol-empty", "toKucoinSymbol", json!([null]), S(""), 0);
    add(
        "symbol-original",
        "toKucoinSymbol",
        json!([{"$ref": "raw"}]),
        Ref("raw"),
        0,
    );
    add(
        "symbol-string",
        "toKucoinSymbol",
        json!(["BTC"]),
        S("BTC"),
        0,
    );
    add("internal-empty", "toInternalSymbol", json!([]), S(""), 0);
    add(
        "internal-xbt",
        "toInternalSymbol",
        json!(["XBTUSDTM"]),
        S("BTCUSDT"),
        0,
    );
    add("api-known", "toApiSymbol", json!(["BTC"]), S("API"), 0);
    add("api-empty-id", "toApiSymbol", json!(["ZERO"]), S("ZERO"), 0);
    add(
        "api-missing",
        "toApiSymbol",
        json!(["MISSING"]),
        S("MISSING"),
        0,
    );
    add("api-falsy", "toApiSymbol", json!([false]), S(""), 0);
    add("api-alias", "toApiSymbol", json!(["RAW"]), Ref("raw"), 0);
    add("size-known", "contractSizeFor", json!(["BTC"]), N(0.25), 0);
    add("size-zero", "contractSizeFor", json!(["ZERO"]), N(1.), 0);
    add(
        "size-negative",
        "contractSizeFor",
        json!(["NEG"]),
        N(-1.),
        0,
    );
    add(
        "size-inf",
        "contractSizeFor",
        json!(["INF"]),
        N(f64::INFINITY),
        0,
    );
    add("size-nan", "contractSizeFor", json!(["NAN"]), N(1.), 0);
    add("size-raw", "contractSizeFor", json!(["RAW"]), Ref("raw"), 0);
    add("size-falsy", "contractSizeFor", json!([false]), N(1.), 0);
    add(
        "remaining-zero",
        "nativeAmountForCancelReplace",
        json!([{"extra": {"remainingNative": 0}}]),
        N(0.),
        0,
    );
    add(
        "remaining-negative-zero",
        "nativeAmountForCancelReplace",
        json!([{"extra": {"remainingNative": {"$negativeZero": true}}}]),
        N(-0.),
        0,
    );
    add(
        "remaining-false",
        "nativeAmountForCancelReplace",
        json!([{"extra": {"remainingNative": false}}]),
        B(false),
        0,
    );
    add(
        "remaining-nan",
        "nativeAmountForCancelReplace",
        json!([{"extra": {"remainingNative": {"$nan": true}}}]),
        N(f64::NAN),
        0,
    );
    add(
        "remaining-fallback",
        "nativeAmountForCancelReplace",
        json!([{"symbol": "BTC", "remaining": 0.75, "extra": {"remainingNative": null}}]),
        N(3.),
        0,
    );
    add(
        "margin-isolated",
        "parseMarginType",
        json!(["isolated"]),
        S("isolated"),
        0,
    );
    add(
        "margin-fixed",
        "parseMarginType",
        json!(["fIxEd", true]),
        S("isolated"),
        0,
    );
    add(
        "margin-cross",
        "parseMarginType",
        json!(["cross", false]),
        S("cross"),
        0,
    );
    add(
        "margin-crossed",
        "parseMarginType",
        json!(["CROSSED"]),
        S("cross"),
        0,
    );
    add(
        "margin-false",
        "parseMarginType",
        json!(["unknown", false]),
        S("isolated"),
        0,
    );
    add(
        "margin-zero",
        "parseMarginType",
        json!(["unknown", 0]),
        S("cross"),
        0,
    );
    add(
        "margin-default",
        "parseMarginType",
        json!([]),
        S("cross"),
        0,
    );
    add(
        "mode-number",
        "parsePositionMode",
        json!([1]),
        S("hedged"),
        0,
    );
    add(
        "mode-hedge",
        "parsePositionMode",
        json!(["hedge"]),
        S("hedged"),
        0,
    );
    add(
        "mode-hedge-mode",
        "parsePositionMode",
        json!(["hedge_mode"]),
        S("hedged"),
        0,
    );
    add(
        "mode-double",
        "parsePositionMode",
        json!(["double_hold"]),
        S("hedged"),
        0,
    );
    add(
        "mode-enum-unrecognized",
        "parsePositionMode",
        json!(["hedged"]),
        S("oneway"),
        0,
    );
    add(
        "mode-default",
        "parsePositionMode",
        json!([]),
        S("oneway"),
        0,
    );
    add(
        "row-side",
        "parseRowPositionMode",
        json!([{"positionSide": "long"}]),
        S("hedged"),
        0,
    );
    add(
        "row-both",
        "parseRowPositionMode",
        json!([{"positionSide": "BOTH", "positionMode": "hedge"}]),
        S("hedged"),
        0,
    );
    add(
        "row-fallback",
        "parseRowPositionMode",
        json!([{"positionMode": "", "holdMode": 1}]),
        S("hedged"),
        0,
    );
    add(
        "row-live",
        "parseRowPositionMode",
        json!([{"positionMode": "oneway"}]),
        S("oneway"),
        2,
    );
    add(
        "v2-oneway",
        "getOrderSideV2",
        json!([{"side": "buy"}, "oneway"]),
        pair(S("Buy"), U),
        0,
    );
    add(
        "v2-open",
        "getOrderSideV2",
        json!([{"side": "sell"}, "hedged"]),
        pair(S("Sell"), S("Open")),
        0,
    );
    add(
        "v2-close-buy",
        "getOrderSideV2",
        json!([{"side": "buy", "reduceOnly": true}, "hedged"]),
        pair(S("Sell"), S("Close")),
        0,
    );
    add(
        "v2-close-sell",
        "getOrderSideV2",
        json!([{"side": "sell", "reduceOnly": true}, "hedged"]),
        pair(S("Buy"), S("Close")),
        0,
    );
    add(
        "v2-live",
        "getOrderSideV2",
        json!([{"reduceOnly": true}, "hedged"]),
        pair(S("Buy"), S("Close")),
        2,
    );
    add(
        "side-single-buy",
        "getOrderSide",
        json!([{"side": "buy"}, "oneway"]),
        S("buy_single"),
        0,
    );
    add(
        "side-single-unknown",
        "getOrderSide",
        json!([{"side": "bad"}, "oneway"]),
        S("sell_single"),
        0,
    );
    add(
        "side-open-buy",
        "getOrderSide",
        json!([{"side": "buy"}, "hedged"]),
        S("open_long"),
        0,
    );
    add(
        "side-open-sell",
        "getOrderSide",
        json!([{"side": "sell"}, "hedged"]),
        S("open_short"),
        0,
    );
    add(
        "side-close-buy",
        "getOrderSide",
        json!([{"side": "buy", "reduceOnly": true}, "hedged"]),
        S("close_short"),
        0,
    );
    add(
        "side-close-sell",
        "getOrderSide",
        json!([{"side": "sell", "reduceOnly": true}, "hedged"]),
        S("close_long"),
        0,
    );
    add(
        "side-error",
        "getOrderSide",
        json!([{"side": "bad"}, "hedged"]),
        Error("Unknown order side: bad"),
        0,
    );
    add(
        "constructor-live",
        "getOrderSide",
        json!([{}, "hedged"]),
        Error("Unknown order side: bad"),
        3,
    );
    add(
        "algo-profit-buy",
        "getAlgoOrderSide",
        json!([{"type": "take_profit_market", "side": "buy"}]),
        S("short"),
        0,
    );
    add(
        "algo-profit-sell",
        "getAlgoOrderSide",
        json!([{"type": "take_profit_market", "side": "sell"}]),
        S("long"),
        0,
    );
    add(
        "algo-stop-buy",
        "getAlgoOrderSide",
        json!([{"type": "stop_market", "side": "buy"}]),
        S("short"),
        0,
    );
    add(
        "algo-stop-sell",
        "getAlgoOrderSide",
        json!([{"type": "stop_market", "side": "sell"}]),
        S("long"),
        0,
    );
    add(
        "algo-trailing-buy",
        "getAlgoOrderSide",
        json!([{"type": "trailing_stop_market", "side": "buy"}]),
        S("close_short"),
        0,
    );
    add(
        "algo-trailing-sell",
        "getAlgoOrderSide",
        json!([{"type": "trailing_stop_market", "side": "sell"}]),
        S("close_long"),
        0,
    );
    add(
        "algo-error",
        "getAlgoOrderSide",
        json!([{"type": "limit", "side": "buy"}]),
        Error("Unknown algo order side: limit - buy"),
        0,
    );
    add(
        "margin-mode-cross",
        "getKucoinMarginMode",
        json!([{"marginType": "cross"}]),
        S("CROSS"),
        0,
    );
    add(
        "margin-mode-isolated",
        "getKucoinMarginMode",
        json!([{"marginType": "isolated"}]),
        S("ISOLATED"),
        0,
    );
    add(
        "margin-mode-default",
        "getKucoinMarginMode",
        json!([]),
        U,
        0,
    );
    add(
        "position-side-none",
        "getKucoinPositionSide",
        json!([{"side": "buy"}]),
        U,
        0,
    );
    add(
        "position-side-open",
        "getKucoinPositionSide",
        json!([{"side": "buy"}, {"positionMode": "hedged"}]),
        S("LONG"),
        0,
    );
    add(
        "position-side-close",
        "getKucoinPositionSide",
        json!([{"side": "buy", "reduceOnly": true}, {"positionMode": "hedged"}]),
        S("SHORT"),
        0,
    );
    add(
        "position-side-algo",
        "getKucoinPositionSide",
        json!([{"side": "sell", "type": "stop_market"}, {"positionMode": "hedged"}]),
        S("LONG"),
        0,
    );
    add(
        "stop-price-mark",
        "mapStopPriceType",
        json!(["mark_price"]),
        S("MP"),
        0,
    );
    add(
        "stop-price-index",
        "mapStopPriceType",
        json!(["index_price"]),
        S("IP"),
        0,
    );
    add(
        "stop-price-default",
        "mapStopPriceType",
        json!([]),
        S("TP"),
        0,
    );
    add(
        "virtual-stop",
        "isVirtualTPSLOrder",
        json!([{"id": "x__stop_loss"}]),
        B(true),
        0,
    );
    add(
        "virtual-profit",
        "isVirtualTPSLOrder",
        json!([{"id": "x__take_profit"}]),
        B(true),
        0,
    );
    add(
        "virtual-plain",
        "isVirtualTPSLOrder",
        json!([{"id": "plain"}]),
        B(false),
        0,
    );
    add(
        "virtual-live",
        "isVirtualTPSLOrder",
        json!([{}]),
        B(true),
        2,
    );
    add(
        "is-algo-stop",
        "isAlgoOrder",
        json!([{"type": "stop_market"}]),
        B(true),
        0,
    );
    add(
        "is-algo-profit",
        "isAlgoOrder",
        json!([{"type": "take_profit_market"}]),
        B(true),
        0,
    );
    add(
        "is-algo-trailing",
        "isAlgoOrder",
        json!([{"type": "trailing_stop_market"}]),
        B(true),
        0,
    );
    add(
        "is-algo-plain",
        "isAlgoOrder",
        json!([{"type": "limit"}]),
        B(false),
        0,
    );
    add("algo-live", "isAlgoOrder", json!([{}]), B(true), 3);
    add(
        "plan-stop",
        "getOrderPlanType",
        json!([{"type": "stop_market"}]),
        S("pos_loss"),
        0,
    );
    add(
        "plan-profit",
        "getOrderPlanType",
        json!([{"type": "take_profit_market"}]),
        S("pos_profit"),
        0,
    );
    add(
        "plan-trailing",
        "getOrderPlanType",
        json!([{"type": "trailing_stop_market"}]),
        S("track_plan"),
        0,
    );
    add(
        "plan-error",
        "getOrderPlanType",
        json!([{"type": "limit"}]),
        Error("Unknown order type: limit"),
        0,
    );
    add(
        "direction-stop-buy",
        "getKucoinStopDirection",
        json!([{"type": "stop_market", "side": "buy"}]),
        S("up"),
        0,
    );
    add(
        "direction-stop-sell",
        "getKucoinStopDirection",
        json!([{"type": "stop_market", "side": "sell"}]),
        S("down"),
        0,
    );
    add(
        "direction-profit-buy",
        "getKucoinStopDirection",
        json!([{"type": "take_profit_market", "side": "buy"}]),
        S("down"),
        0,
    );
    add(
        "direction-profit-sell",
        "getKucoinStopDirection",
        json!([{"type": "take_profit_market", "side": "sell"}]),
        S("up"),
        0,
    );
    add(
        "direction-error",
        "getKucoinStopDirection",
        json!([{"type": "limit"}]),
        Error("Unsupported KuCoin stop direction for order type: limit"),
        0,
    );
    add(
        "position-open",
        "getPositionForOrder",
        json!([{"symbol": "BTC", "side": "buy", "type": "limit"}]),
        Ref("long"),
        0,
    );
    add(
        "position-close",
        "getPositionForOrder",
        json!([{"symbol": "BTC", "side": "buy", "reduceOnly": true}]),
        Ref("short"),
        0,
    );
    add(
        "position-algo",
        "getPositionForOrder",
        json!([{"symbol": "BTC", "side": "sell", "type": "stop_market"}]),
        Ref("long"),
        0,
    );
    add(
        "position-missing",
        "getPositionForOrder",
        json!([{"symbol": "MISSING", "side": "buy"}]),
        U,
        0,
    );
    add(
        "amount-trailing",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "trailing_stop_market", "amount": {"$ref": "raw"}}]),
        pair(S("track_plan"), Ref("raw")),
        0,
    );
    add(
        "amount-zero",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "stop_market", "amount": 0}]),
        pair(S("pos_loss"), N(0.)),
        0,
    );
    add(
        "amount-negative-zero",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "take_profit_market", "amount": {"$negativeZero": true}}]),
        pair(S("pos_profit"), N(0.)),
        0,
    );
    add(
        "amount-insufficient",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "stop_market", "amount": 3, "symbol": "BTC", "side": "buy"}]),
        pair(S("pos_loss"), N(0.)),
        0,
    );
    add(
        "amount-threshold",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "stop_market", "amount": 2, "symbol": "BTC", "side": "buy"}]),
        pair(S("loss_plan"), N(2.)),
        0,
    );
    add(
        "amount-profit",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "take_profit_market", "amount": 4, "symbol": "BTC", "side": "sell"}]),
        pair(S("profit_plan"), N(4.)),
        0,
    );
    add(
        "amount-decimal",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "take_profit_market", "amount": 3, "symbol": "DEC", "side": "buy"}]),
        pair(S("profit_plan"), N(3.)),
        0,
    );
    add(
        "amount-error",
        "getOrderPlanTypeAndAmount",
        json!([{"type": "limit", "amount": 0}]),
        Error("Unknown order type: limit"),
        0,
    );
    add(
        "position-live",
        "getPositionForOrder",
        json!([{"side":"buy","reduceOnly":true}]),
        Ref("replacement"),
        2,
    );
    add(
        "internal-override",
        "toInternalSymbol",
        json!(["original"]),
        S("BTCUSDT"),
        1,
    );
    add(
        "api-override",
        "toApiSymbol",
        json!(["MISSING"]),
        Ref("raw"),
        1,
    );
    add(
        "remaining-override",
        "nativeAmountForCancelReplace",
        json!([{"symbol":"BTC","remaining":0.75}]),
        N(1.5),
        1,
    );
    cases
}
#[test]
fn native_futures_helper_values_precede_source_projection() {
    let cases = cases();
    assert_eq!(cases.len(), 103, "independent authored case floor");
    assert_eq!(
        cases
            .iter()
            .map(|c| c.method)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        20,
        "independent method floor"
    );
    let native = cases.iter().map(run).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/futures-native-helpers.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = cases
        .iter()
        .map(|c| json!({"name":c.name,"method":c.method,"args":c.args}))
        .collect::<Vec<_>>();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source helper assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 103, "source case floor");
    for (index, (actual, expected)) in native.iter().zip(source.iter()).enumerate() {
        assert_eq!(
            actual, expected,
            "source comparison case {index}/{}",
            cases[index].name
        );
    }
}
