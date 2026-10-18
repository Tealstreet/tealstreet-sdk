//! Venue-agnostic module scope: JS builtins and the safe-cex type enums.
//! Venues add their own protocol constants and imported helpers.
use super::{
    iterator::IteratorRecord,
    promise,
    scope::{ScopeShared, scope_of},
    value::{Js, Str, Thrown, arg, error_constructor},
};
use std::rc::{Rc, Weak};

use safe_cex_types::js_coercion::json_quote_utf16 as json_quote;

/// JSON.stringify's replacer argument: a ReplacerFunction or an array's PropertyList.
enum Replacer {
    None,
    Function(Js),
    PropertyList(Vec<Str>),
}

impl Replacer {
    /// Array items that are strings or numbers become keys, first occurrence kept.
    fn from_argument(replacer: Js) -> Result<Self, Thrown> {
        if replacer.is_callable() {
            return Ok(Replacer::Function(replacer));
        }
        let Some(items) = replacer.array_items()? else {
            return Ok(Replacer::None);
        };
        let mut keys: Vec<Str> = vec![];
        for item in items.into_iter().flatten() {
            if !matches!(item, Js::String(_) | Js::Number(_)) {
                continue;
            }
            let key: Str = item.to_js_string()?;
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        Ok(Replacer::PropertyList(keys))
    }
}

/// The shared JSON.stringify operation for native data without a replacer.
pub fn stringify_json(value: &Js) -> Result<Option<Vec<u16>>, Thrown> {
    json_stringify(
        &Js::object(vec![("", value.clone())]),
        &[],
        &Replacer::None,
        &mut Vec::new(),
    )
}

/// SerializeJSONProperty; `None` is undefined.
fn json_stringify(
    holder: &Js,
    key: &[u16],
    replacer: &Replacer,
    stack: &mut Vec<Js>,
) -> Result<Option<Vec<u16>>, Thrown> {
    let mut value = holder.get_key(key)?;
    if value.is_object()
        || value.is_array()
        || value.is_callable()
        || matches!(value, Js::BigInt(_))
    {
        let to_json_member = value.get("toJSON")?;
        if to_json_member.is_callable() {
            value = to_json_member.call(value.clone(), vec![Js::utf16(key)])?;
        }
    }
    if let Replacer::Function(replacer) = replacer {
        value = replacer.call(holder.clone(), vec![Js::utf16(key), value])?;
    }
    let container = value.is_object() || value.is_array();
    if container {
        if stack.iter().any(|ancestor| ancestor.same_reference(&value)) {
            return Err(Thrown::type_error("Converting circular structure to JSON"));
        }
        stack.push(value.clone());
    }
    let lit = |t: &str| Ok(Some(t.encode_utf16().collect()));
    let result = match &value {
        Js::Undefined | Js::SymbolIterator | Js::Function(_) => Ok(None),
        Js::Null => lit("null"),
        Js::Bool(b) => lit(if *b { "true" } else { "false" }),
        Js::Number(n) if n.is_finite() => lit(&safe_cex_num::js_number_string(*n)),
        Js::Number(_) => lit("null"),
        Js::BigInt(_) => Err(Thrown::type_error("Do not know how to serialize a BigInt")),
        Js::String(s) => {
            let mut out = vec![];
            json_quote(s, &mut out);
            Ok(Some(out))
        }
        Js::Array(items) => {
            // Capture length once, then read each live slot after earlier replacer calls.
            let length = items.len();
            let mut out = vec![b'[' as u16];
            for i in 0..length {
                if i > 0 {
                    out.push(b',' as u16);
                }
                let key: Vec<u16> = i.to_string().encode_utf16().collect();
                match json_stringify(&value, &key, replacer, stack)? {
                    Some(text) => out.extend(text),
                    None => out.extend("null".encode_utf16()),
                }
            }
            out.push(b']' as u16);
            Ok(Some(out))
        }
        Js::Builtin(reference) if reference.is_callable() => Ok(None),
        Js::Object(_) | Js::Builtin(_) => {
            let mut out = vec![b'{' as u16];
            let mut first = true;
            // Key enumeration precedes traversal; getters/replacers can alter later values.
            let keys = match replacer {
                Replacer::PropertyList(keys) => keys.clone(),
                _ => value.own_keys(),
            };
            for key in keys {
                let Some(text) = json_stringify(&value, &key, replacer, stack)? else {
                    continue;
                };
                if !first {
                    out.push(b',' as u16);
                }
                first = false;
                json_quote(&key, &mut out);
                out.push(b':' as u16);
                out.extend(text);
            }
            out.push(b'}' as u16);
            Ok(Some(out))
        }
        Js::Promise(_) | Js::Map(_) | Js::Set(_) => lit("{}"),
    };
    if container {
        stack.pop();
    }
    result
}

/// GetIterator once, preserving live iteration and abrupt completion.
fn iterate(value: &Js) -> Result<Option<IteratorRecord>, Thrown> {
    if value.is_nullish() {
        return Ok(None);
    }
    let method = value.get_property(&Js::SymbolIterator)?;
    if method.is_nullish() {
        return Ok(None);
    }
    let iterator = method.call(value.clone(), vec![])?;
    IteratorRecord::from_iterator(iterator).map(Some)
}

fn consume_iterator(
    iterator: IteratorRecord,
    mut visit: impl FnMut(Js, usize) -> Result<(), Thrown>,
) -> Result<(), Thrown> {
    let mut index = 0;
    while let Some(value) = iterator.step_value()? {
        if let Err(error) = visit(value, index) {
            return Err(iterator.close_after_throw(error));
        }
        index += 1;
    }
    Ok(())
}

/// How V8 names an operand in a TypeError (`number 5`, `string "x"`, `object null`).
fn v8_operand(value: &Js) -> String {
    match value {
        Js::Undefined => "undefined".to_owned(),
        Js::Null => "object null".to_owned(),
        Js::String(_) => format!("string \"{}\"", value.to_display()),
        Js::Bool(_) | Js::Number(_) | Js::BigInt(_) => {
            format!("{} {}", value.type_of(), value.to_display())
        }
        _ => value.type_of().to_owned(),
    }
}

pub(super) fn not_iterable(value: &Js) -> Thrown {
    let operand = if matches!(value, Js::BigInt(_)) {
        value.type_of().to_owned()
    } else {
        v8_operand(value)
    };
    Thrown::type_error(&format!(
        "{operand} is not iterable (cannot read property Symbol(Symbol.iterator))"
    ))
}

/// `new Map(iterable)`: AddEntriesFromIterable through Map.prototype.set.
fn map_from_entries(iterable: &Js) -> Result<Js, Thrown> {
    let map = Js::new_map();
    if iterable.is_nullish() {
        return Ok(map);
    }
    let iterator = iterate(iterable)?.ok_or_else(|| not_iterable(iterable))?;
    let set = map.get("set")?;
    consume_iterator(iterator, |entry, _| {
        if !entry.is_object() {
            return Err(Thrown::type_error(&format!(
                "Iterator value {} is not an entry object",
                entry.to_display()
            )));
        }
        let key = entry.get("0")?;
        let value = entry.get("1")?;
        set.call(map.clone(), vec![key, value])?;
        Ok(())
    })?;
    Ok(map)
}

/// `Array.from(items, mapFn, thisArg)` over iterables, else array-likes by `length`.
fn array_from(a: &[Js]) -> Result<Js, Thrown> {
    let items = arg(a, 0);
    let map_fn = arg(a, 1);
    let mapping = !map_fn.is_undefined();
    if mapping && !map_fn.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{} is not a function",
            v8_operand(&map_fn)
        )));
    }
    let this = arg(a, 2);
    let mut out = vec![];
    let mut visit = |value: Js, index: usize| {
        out.push(if mapping {
            map_fn.call(this.clone(), vec![value, Js::Number(index as f64)])?
        } else {
            value
        });
        Ok(())
    };
    if let Some(iterator) = iterate(&items)? {
        consume_iterator(iterator, &mut visit)?;
    } else {
        if items.is_nullish() {
            return Err(not_iterable(&items));
        }
        let length = items.get("length")?.to_number()?;
        let length = if length.is_nan() || length <= 0.0 {
            0.0
        } else {
            length.trunc().min(9_007_199_254_740_991.0)
        };
        if length > f64::from(u32::MAX) {
            return Err(Thrown::error("RangeError", "Invalid array length"));
        }
        for index in 0..length as usize {
            visit(items.get(&index.to_string())?, index)?;
        }
    }
    Ok(Js::array(out))
}

/// application/x-www-form-urlencoded byte serializer (URLSearchParams).
fn form_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn enum_object(members: &[(&str, &str)]) -> Js {
    Js::object(members.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

/// The types.ts enums by member name.
pub fn install_enums(g: &Js) {
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    def(
        "OrderType",
        enum_object(&[
            ("Market", "market"),
            ("Limit", "limit"),
            ("Conditional", "conditional"),
            ("StopLoss", "stop_market"),
            ("TakeProfit", "take_profit_market"),
            ("TrailingStopLoss", "trailing_stop_market"),
        ]),
    );
    def(
        "OrderStatus",
        enum_object(&[
            ("Open", "open"),
            ("Closed", "closed"),
            ("Canceled", "canceled"),
            ("Creating", "creating"),
            ("Updating", "updating"),
            ("Canceling", "canceling"),
        ]),
    );
    def(
        "OrderSide",
        enum_object(&[("Buy", "buy"), ("Sell", "sell")]),
    );
    def(
        "OrderTimeInForce",
        enum_object(&[
            ("GoodTillCancel", "GoodTillCancel"),
            ("ImmediateOrCancel", "ImmediateOrCancel"),
            ("FillOrKill", "FillOrKill"),
            ("PostOnly", "PostOnly"),
        ]),
    );
    def(
        "OrderStopTriggerType",
        enum_object(&[
            ("MarkPrice", "mark_price"),
            ("LastPrice", "last_price"),
            ("IndexPrice", "index_price"),
        ]),
    );
    def(
        "PositionSide",
        enum_object(&[("Long", "long"), ("Short", "short")]),
    );
    def(
        "PositionMode",
        enum_object(&[("Hedged", "hedged"), ("OneWay", "oneway")]),
    );
    def(
        "MarginType",
        enum_object(&[("Isolated", "isolated"), ("Cross", "cross")]),
    );
    def(
        "LogSeverity",
        enum_object(&[
            ("Debug", "debug"),
            ("Info", "info"),
            ("Warning", "warning"),
            ("Error", "error"),
        ]),
    );
}

fn construct_promise(
    runtime: &Rc<dyn crate::runtime::ExchangeRuntime>,
    executor: Js,
) -> Result<Js, Thrown> {
    if !executor.is_callable() {
        return Err(Thrown::type_error("Promise resolver is not a function"));
    }
    let promise = promise::Promise::pending(runtime);
    let (resolve, reject, called) = promise::resolving_functions(promise.clone());
    if let Err(error) = executor.call(Js::Undefined, vec![resolve, reject]) {
        if !called.replace(true) {
            promise.settle(Err(error));
        }
    }
    Ok(Js::Promise(promise))
}

/// The JS builtins venue bodies resolve by name. `rt` is weak: the globals
/// object lives inside the shared scope state.
pub fn install_builtins<X: 'static>(g: &Js, rt: Weak<ScopeShared<X>>) {
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    // Number.parseFloat === parseFloat: one function object under both names.
    let parse_float = Js::function("parseFloat", |_, a| {
        Ok(Js::Number(arg(&a, 0).parse_float()?))
    });
    def("parseFloat", parse_float.clone());
    def(
        "isNaN",
        Js::function("isNaN", |_, args| {
            Ok(Js::Bool(arg(&args, 0).to_number()?.is_nan()))
        }),
    );
    def(
        "parseInt",
        Js::function("parseInt", |_, a| {
            let text = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
            let radix = arg(&a, 1);
            let radix = if radix.is_undefined() {
                None
            } else {
                Some(radix.to_number()? as i32)
            };
            Ok(Js::Number(safe_cex_num::parse_int(&text, radix)))
        }),
    );
    let symbol = Js::function("Symbol", |_, _| {
        Err(Thrown::type_error(
            "Creating arbitrary Symbols is not supported",
        ))
    });
    symbol
        .define_property(
            &"iterator".encode_utf16().collect::<Vec<_>>(),
            &Js::object(vec![("value", Js::SymbolIterator)]),
        )
        .expect("well-known iterator symbol data descriptor");
    def("Symbol", symbol);
    def(
        "String",
        Js::function("String", |_, a| {
            Ok(if a.is_empty() {
                Js::str("")
            } else if matches!(a[0], Js::SymbolIterator) {
                Js::str("Symbol(Symbol.iterator)")
            } else {
                Js::String(a[0].to_js_string()?)
            })
        }),
    );
    // ts: Number(value) is ToNumeric, then a BigInt rounds to the nearest double.
    let number = Js::function("Number", |_, a| {
        Ok(Js::Number(match a.first() {
            None => 0.0,
            Some(value) => match super::ops::to_numeric(value)? {
                Js::BigInt(n) => n.to_string().parse().unwrap_or(f64::NAN),
                numeric => numeric.to_number()?,
            },
        }))
    });
    for (key, value) in [
        (
            "isFinite",
            Js::function("isFinite", |_, a| {
                Ok(Js::Bool(
                    matches!(arg(&a, 0), Js::Number(n) if n.is_finite()),
                ))
            }),
        ),
        (
            "isSafeInteger",
            Js::function("isSafeInteger", |_, a| {
                Ok(Js::Bool(matches!(
                    arg(&a, 0),
                    Js::Number(n) if n.is_finite() && n.trunc() == n && n.abs() <= 9_007_199_254_740_991.0
                )))
            }),
        ),
        ("MAX_SAFE_INTEGER", Js::Number(9_007_199_254_740_991.0)),
        ("NaN", Js::Number(f64::NAN)),
        ("parseFloat", parse_float),
    ] {
        let _ = number.define(key, value);
    }
    def("Number", number);
    def(
        "Boolean",
        Js::function("Boolean", |_, a| Ok(Js::Bool(arg(&a, 0).truthy()))),
    );
    def("Error", error_constructor("Error"));
    def("SyntaxError", error_constructor("SyntaxError"));
    def(
        "BigInt",
        Js::function("BigInt", |_, a| {
            let v = arg(&a, 0);
            let parsed = match &v {
                Js::BigInt(n) => Some((**n).clone()),
                Js::Bool(b) => Some(num_bigint::BigInt::from(u8::from(*b))),
                Js::Number(n) if n.is_finite() && n.fract() == 0.0 => {
                    format!("{n:.0}").parse().ok()
                }
                Js::Number(n) => {
                    return Err(Thrown::error(
                        "RangeError",
                        &format!(
                            "The number {} cannot be converted to a BigInt because it is not an integer",
                            safe_cex_num::js_number_string(*n)
                        ),
                    ));
                }
                Js::String(s) => {
                    let t = String::from_utf16_lossy(s);
                    let t = safe_cex_num::js_trim(&t);
                    if t.is_empty() {
                        Some(num_bigint::BigInt::default())
                    } else {
                        t.parse().ok()
                    }
                }
                _ => None,
            };
            parsed.map(|n| Js::BigInt(Rc::new(n))).ok_or_else(|| {
                Thrown::error(
                    "SyntaxError",
                    &format!("Cannot convert {} to a BigInt", v.to_display()),
                )
            })
        }),
    );
    def(
        "URLSearchParams",
        Js::constructor("URLSearchParams", None, |_, a| {
            let source = arg(&a, 0);
            let mut pairs = vec![];
            if !source.is_nullish() {
                for (k, v) in source.own_entries()? {
                    pairs.push((
                        String::from_utf16_lossy(&k),
                        String::from_utf16_lossy(&v.to_js_string()?),
                    ));
                }
            }
            let text = pairs
                .iter()
                .map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v)))
                .collect::<Vec<_>>()
                .join("&");
            Ok(Js::object_with_class(
                "URLSearchParams",
                vec![(
                    "toString",
                    Js::function("toString", move |_, _| Ok(Js::str(&text))),
                )],
            ))
        }),
    );
    def("TypeError", error_constructor("TypeError"));
    def(
        "Array",
        Js::object(vec![
            ("prototype", super::value::array::prototype()),
            (
                "isArray",
                Js::function("isArray", |_, a| Ok(Js::Bool(arg(&a, 0).is_array()))),
            ),
            ("from", Js::function("from", |_, a| array_from(&a))),
        ]),
    );
    let object = Js::object(vec![
        (
            "setPrototypeOf",
            Js::function("setPrototypeOf", |_, args| {
                arg(&args, 0).set_prototype(arg(&args, 1))
            }),
        ),
        (
            "freeze",
            Js::function("freeze", |_, args| arg(&args, 0).freeze()),
        ),
        ("seal", Js::function("seal", |_, args| arg(&args, 0).seal())),
        (
            "preventExtensions",
            Js::function("preventExtensions", |_, args| {
                arg(&args, 0).prevent_extensions()
            }),
        ),
        (
            "isExtensible",
            Js::function("isExtensible", |_, args| {
                Ok(Js::Bool(arg(&args, 0).is_extensible()?))
            }),
        ),
        (
            "isFrozen",
            Js::function("isFrozen", |_, args| {
                Ok(Js::Bool(arg(&args, 0).is_frozen()?))
            }),
        ),
        (
            "getOwnPropertyDescriptor",
            Js::function("getOwnPropertyDescriptor", |_, args| {
                let target = arg(&args, 0);
                if target.is_nullish() {
                    return Err(Thrown::type_error(
                        "Cannot convert undefined or null to object",
                    ));
                }
                let key = arg(&args, 1);
                target.own_property_descriptor_for(&key)
            }),
        ),
        (
            "defineProperty",
            Js::function("defineProperty", |_, a| {
                let target = arg(&a, 0);
                if !matches!(target, Js::Object(_) | Js::Function(_) | Js::Array(_)) {
                    return Err(Thrown::type_error(
                        "Object.defineProperty target must be an ordinary object or function",
                    ));
                }
                let key = arg(&a, 1);
                target.define_property_for(&key, &arg(&a, 2))?;
                Ok(target)
            }),
        ),
        (
            "keys",
            Js::function("keys", |_, a| {
                let v = arg(&a, 0);
                if v.is_nullish() {
                    return Err(Thrown::type_error(
                        "Cannot convert undefined or null to object",
                    ));
                }
                Ok(Js::array(
                    v.own_keys().into_iter().map(Js::String).collect(),
                ))
            }),
        ),
        (
            "getOwnPropertyNames",
            Js::function("getOwnPropertyNames", |_, a| {
                let v = arg(&a, 0);
                if v.is_nullish() {
                    return Err(Thrown::type_error(
                        "Cannot convert undefined or null to object",
                    ));
                }
                Ok(Js::array(
                    v.own_property_names()?
                        .into_iter()
                        .map(Js::String)
                        .collect(),
                ))
            }),
        ),
        (
            "values",
            Js::function("values", |_, a| {
                let v = arg(&a, 0);
                if v.is_nullish() {
                    return Err(Thrown::type_error(
                        "Cannot convert undefined or null to object",
                    ));
                }
                Ok(Js::array(
                    v.own_entries()?.into_iter().map(|(_, v)| v).collect(),
                ))
            }),
        ),
        (
            "entries",
            Js::function("entries", |_, a| {
                let v = arg(&a, 0);
                if v.is_nullish() {
                    return Err(Thrown::type_error(
                        "Cannot convert undefined or null to object",
                    ));
                }
                Ok(Js::array(
                    v.own_entries()?
                        .into_iter()
                        .map(|(k, v)| Js::array(vec![Js::String(k), v]))
                        .collect(),
                ))
            }),
        ),
        (
            "assign",
            Js::function("assign", |_, a| {
                let target = arg(&a, 0);
                for source in a.iter().skip(1) {
                    if source.is_nullish() {
                        continue;
                    }
                    for (k, v) in source.own_entries()? {
                        target.set_key(&k, v)?;
                    }
                }
                Ok(target)
            }),
        ),
    ]);
    object
        .define_property(
            &super::value::utf16("prototype"),
            &Js::object(vec![
                (
                    "value",
                    Js::Builtin(
                        safe_cex_num::PrototypeRealm::current()
                            .reference(safe_cex_num::ProtoMember::Prototype),
                    ),
                ),
                ("writable", Js::Bool(false)),
                ("enumerable", Js::Bool(false)),
                ("configurable", Js::Bool(false)),
            ]),
        )
        .expect("new Object intrinsic prototype descriptor");
    def("Object", object);
    def(
        "JSON",
        Js::object(vec![
            (
                "stringify",
                Js::function("stringify", |_, a| {
                    let v = arg(&a, 0);
                    let replacer = Replacer::from_argument(arg(&a, 1))?;
                    let holder = Js::object(vec![("", v)]);
                    Ok(json_stringify(&holder, &[], &replacer, &mut Vec::new())?
                        .map_or(Js::Undefined, |t| Js::utf16(&t)))
                }),
            ),
            (
                "parse",
                Js::function("parse", |_, a| {
                    safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                        &arg(&a, 0).to_js_string()?,
                    )
                    .map(super::value::from_retained_input)
                    .map_err(|error| Thrown::error("SyntaxError", &error.to_string()))
                }),
            ),
        ]),
    );
    let random_rt = rt.clone();
    let math = |name: &'static str, f: fn(f64) -> f64| {
        (
            name,
            Js::function(name, move |_, a| Ok(Js::Number(f(arg(&a, 0).to_number()?)))),
        )
    };
    def(
        "Math",
        Js::object(vec![
            math("abs", f64::abs),
            math("floor", f64::floor),
            math("ceil", f64::ceil),
            math("trunc", f64::trunc),
            math("round", safe_cex_num::math_round),
            math("sign", |v| {
                if v.is_nan() || v == 0.0 {
                    v
                } else {
                    v.signum()
                }
            }),
            math("log10", safe_cex_num::math_log10),
            (
                "max",
                Js::function("max", |_, a| {
                    let mut out = f64::NEG_INFINITY;
                    for v in &a {
                        out = safe_cex_num::math_max(out, v.to_number()?);
                    }
                    Ok(Js::Number(out))
                }),
            ),
            (
                "min",
                Js::function("min", |_, a| {
                    let mut out = f64::INFINITY;
                    for v in &a {
                        out = safe_cex_num::math_min(out, v.to_number()?);
                    }
                    Ok(Js::Number(out))
                }),
            ),
            (
                "pow",
                Js::function("pow", |_, a| {
                    Ok(Js::Number(safe_cex_num::math_pow(
                        arg(&a, 0).to_number()?,
                        arg(&a, 1).to_number()?,
                    )))
                }),
            ),
            (
                "random",
                Js::function("random", move |_, _| {
                    Ok(Js::Number(
                        scope_of(&random_rt)?.exchange_runtime().random(),
                    ))
                }),
            ),
        ]),
    );
    let promise_rt = rt.clone();
    let settled_rt = rt.clone();
    let race_rt = rt.clone();
    let resolve_rt = rt.clone();
    let reject_rt = rt.clone();
    let constructor_rt = rt.clone();
    let constructor = Js::constructor("Promise", None, move |_, args| {
        construct_promise(
            &scope_of(&constructor_rt)?.exchange_runtime(),
            arg(&args, 0),
        )
    });
    for (name, method) in [
        (
            "race",
            Js::function("race", move |_, args| {
                let runtime = scope_of(&race_rt)?.exchange_runtime();
                Ok(Js::Promise(promise::race_iterable(
                    &runtime,
                    &arg(&args, 0),
                )))
            }),
        ),
        (
            "all",
            Js::function("all", move |_, a| {
                let rt = scope_of(&promise_rt)?;
                let runtime = rt.exchange_runtime();
                Ok(Js::Promise(promise::all_iterable(
                    &runtime,
                    &arg(&a, 0),
                    false,
                )))
            }),
        ),
        (
            "allSettled",
            Js::function("allSettled", move |_, a| {
                let rt = scope_of(&settled_rt)?;
                let runtime = rt.exchange_runtime();
                Ok(Js::Promise(promise::all_iterable(
                    &runtime,
                    &arg(&a, 0),
                    true,
                )))
            }),
        ),
        (
            "resolve",
            Js::function("resolve", move |_, a| {
                let rt = scope_of(&resolve_rt)?;
                Ok(match arg(&a, 0) {
                    p @ Js::Promise(_) => p,
                    v => Js::Promise(promise::Promise::resolve(&rt.exchange_runtime(), v)),
                })
            }),
        ),
        (
            "reject",
            Js::function("reject", move |_, a| {
                let rt = scope_of(&reject_rt)?;
                Ok(Js::Promise(promise::Promise::settled(
                    &rt.exchange_runtime(),
                    Err(Thrown::new(arg(&a, 0))),
                )))
            }),
        ),
    ] {
        constructor
            .define(name, method)
            .expect("Promise static method registration");
    }
    def("Promise", constructor);
    let timeout_rt = rt.clone();
    def(
        "setTimeout",
        Js::function("setTimeout", move |_, a| {
            let rt = scope_of(&timeout_rt)?;
            let callback = arg(&a, 0);
            let delay = arg(&a, 1).to_number().unwrap_or(0.0);
            Ok(rt.set_timeout(
                Rc::new(move || callback.call(Js::Undefined, vec![]).map(|_| ())),
                delay,
            ))
        }),
    );
    let clear_rt = rt.clone();
    def(
        "clearTimeout",
        Js::function("clearTimeout", move |_, a| {
            scope_of(&clear_rt)?.clear_timeout(&arg(&a, 0));
            Ok(Js::Undefined)
        }),
    );
    let date_rt = rt.clone();
    def(
        "Date",
        Js::object(vec![
            (
                "now",
                Js::function("now", move |_, _| {
                    Ok(Js::Number(scope_of(&date_rt)?.now().floor()))
                }),
            ),
            (
                "parse",
                Js::function("parse", |_, a| {
                    // ExchangeRuntime has no host-timezone hook, so offset-less date-times
                    // (local time per ECMAScript) are read as UTC: a UTC-host assumption.
                    let text = arg(&a, 0).to_js_string()?;
                    Ok(Js::Number(safe_cex_num::js_date_parse_utf16(&text, |ms| {
                        ms
                    })))
                }),
            ),
        ]),
    );
    let performance_rt = rt.clone();
    def(
        "performance",
        Js::object(vec![(
            "now",
            Js::function("now", move |_, _| {
                Ok(Js::Number(
                    scope_of(&performance_rt)?
                        .exchange_runtime()
                        .monotonic_now_ms(),
                ))
            }),
        )]),
    );
    let console = |severity: &'static str| {
        let rt = rt.clone();
        Js::function(severity, move |_, a| {
            scope_of(&rt)?
                .exchange_runtime()
                .console_values(severity, &a);
            Ok(Js::Undefined)
        })
    };
    def(
        "console",
        Js::object(vec![
            ("error", console("error")),
            ("warn", console("warn")),
            ("log", console("log")),
            ("info", console("info")),
            ("debug", console("debug")),
        ]),
    );
    def(
        "virtualClock",
        Js::object(vec![(
            "getCurrentTimeMs",
            Js::function("getCurrentTimeMs", move |_, _| {
                Ok(Js::Number(
                    safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
                ))
            }),
        )]),
    );
    def(
        "Map",
        Js::constructor("Map", None, |_, a| map_from_entries(&arg(&a, 0))),
    );
    def(
        "WsDirectFallback",
        Js::constructor("WsDirectFallback", None, |_, a| {
            let max = arg(&a, 0);
            let max = if max.is_undefined() {
                2.0
            } else {
                max.to_number()?
            };
            let state = Rc::new(std::cell::RefCell::new(
                safe_cex_transport::fallback::WsDirectFallback::new(max.max(0.0) as usize),
            ));
            let (resolve, message, silence) = (state.clone(), state.clone(), state);
            Ok(Js::object_with_class(
                "WsDirectFallback",
                vec![
                    (
                        "resolveUrl",
                        Js::function("resolveUrl", move |_, a| {
                            let primary = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
                            let fallbacks: Vec<Option<String>> = a
                                .iter()
                                .skip(1)
                                .map(|v| v.as_str().map(|s| String::from_utf16_lossy(s)))
                                .collect();
                            let refs: Vec<Option<&str>> =
                                fallbacks.iter().map(|f| f.as_deref()).collect();
                            Ok(Js::str(resolve.borrow_mut().resolve_url(&primary, &refs)))
                        }),
                    ),
                    (
                        "onMessage",
                        Js::function("onMessage", move |_, _| {
                            message.borrow_mut().on_message();
                            Ok(Js::Undefined)
                        }),
                    ),
                    (
                        "disregardSilence",
                        Js::function("disregardSilence", move |_, _| {
                            silence.borrow_mut().disregard_silence();
                            Ok(Js::Undefined)
                        }),
                    ),
                ],
            ))
        }),
    );
    def(
        "Set",
        Js::constructor("Set", None, |_, a| {
            let set = Js::new_set();
            let values = arg(&a, 0);
            if !values.is_nullish() {
                let iterator = iterate(&values)?.ok_or_else(|| not_iterable(&values))?;
                let add = set.get("add")?;
                consume_iterator(iterator, |value, _| {
                    add.call(set.clone(), vec![value])?;
                    Ok(())
                })?;
            }
            Ok(set)
        }),
    );
}
