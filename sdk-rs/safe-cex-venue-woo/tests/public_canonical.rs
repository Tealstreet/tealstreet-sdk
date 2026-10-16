use safe_cex_venue_woo::public_canonical::{
    self as port, CanonicalCallback, CanonicalModule, CanonicalOps, WooV3KlineRequestContext,
    WooV3PublicRequestContext,
};
use safe_cex_venue_woo::snapshot_parity::{
    self as diagnostic, SnapshotParityOps, SnapshotPrimitives,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type V = Rc<Native>;
type E = Rc<String>;
enum Native {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(Vec<(String, V)>),
    Array(RefCell<Vec<V>>),
    Map(RefCell<Vec<(V, V)>>),
    Entries(V),
    Function(CanonicalCallback<V, E>),
    Builtin(&'static str),
}
fn undef() -> V {
    Rc::new(Native::Undefined)
}
fn string(s: &str) -> V {
    Rc::new(Native::String(s.into()))
}
fn decode(value: &Value) -> V {
    match value {
        Value::Bool(b) => Rc::new(Native::Bool(*b)),
        Value::Number(n) => Rc::new(Native::Number(n.as_f64().unwrap())),
        Value::String(s) => string(s),
        Value::Array(v) => Rc::new(Native::Array(RefCell::new(v.iter().map(decode).collect()))),
        Value::Object(o) if o.len() == 1 && o.contains_key("$nativeNumber") => {
            Rc::new(Native::Number(match o["$nativeNumber"].as_str().unwrap() {
                "NaN" => f64::NAN,
                "-0" => -0.,
                _ => panic!("fixture number"),
            }))
        }
        Value::Object(o) => Rc::new(Native::Object(
            o.iter().map(|(k, v)| (k.clone(), decode(v))).collect(),
        )),
        Value::Null => Rc::new(Native::Null),
    }
}
fn get(value: &V, key: &str) -> Result<V, E> {
    match value.as_ref() {
        Native::Object(fields) => Ok(fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef)),
        Native::String(_) if key == "localeCompare" => {
            Ok(Rc::new(Native::Builtin("localeCompare")))
        }
        Native::Array(rows) if key == "length" => {
            Ok(Rc::new(Native::Number(rows.borrow().len() as f64)))
        }
        Native::Array(rows) if key.parse::<usize>().is_ok() => Ok(rows
            .borrow()
            .get(key.parse::<usize>().unwrap())
            .cloned()
            .unwrap_or_else(undef)),
        Native::Array(_)
            if ["sort", "map", "flatMap", "push", "join", "entries"].contains(&key) =>
        {
            Ok(Rc::new(Native::Builtin(match key {
                "sort" => "sort",
                "map" => "map",
                "flatMap" => "flatMap",
                "push" => "push",
                "join" => "join",
                _ => "entries",
            })))
        }
        Native::Map(_) if ["has", "get", "set"].contains(&key) => {
            Ok(Rc::new(Native::Builtin(match key {
                "has" => "has",
                "get" => "get",
                _ => "set",
            })))
        }
        Native::Builtin("Number") if key == "isSafeInteger" || key == "isFinite" => {
            Ok(Rc::new(Native::Builtin(if key == "isFinite" {
                "isFinite"
            } else {
                "isSafeInteger"
            })))
        }
        Native::Undefined | Native::Null => Err(Rc::new("property access".into())),
        _ => Ok(undef()),
    }
}
fn native_json(value: &V) -> safe_cex_num::JsJsonValue {
    use safe_cex_num::JsJsonValue as J;
    match value.as_ref() {
        Native::Undefined => J::Undefined,
        Native::Null => J::Null,
        Native::Bool(b) => J::Bool(*b),
        Native::Number(n) => J::Number(*n),
        Native::String(s) => J::String(s.clone()),
        Native::Object(o) => {
            J::Object(o.iter().map(|(k, v)| (k.clone(), native_json(v))).collect())
        }
        Native::Array(a) => J::Array(a.borrow().iter().map(native_json).collect()),
        _ => panic!("callable passed to finite JSON fixture"),
    }
}
fn fields(value: &V) -> Vec<(String, V)> {
    let Native::Object(fields) = value.as_ref() else {
        panic!("CANONICAL-NATIVE object")
    };
    fields.clone()
}
fn text(value: &V) -> String {
    let Native::String(s) = value.as_ref() else {
        panic!("CANONICAL-NATIVE string")
    };
    s.clone()
}
fn number(value: &V) -> f64 {
    let Native::Number(n) = value.as_ref() else {
        panic!("CANONICAL-NATIVE number")
    };
    *n
}
fn assert_value(value: &V, expected: &Value) {
    match value.as_ref() {
        Native::Undefined => assert_eq!(expected, &json!({"undefined":true})),
        Native::Null => assert!(expected.is_null()),
        Native::Array(rows) => {
            let rows = rows.borrow();
            let expected = expected.as_array().unwrap();
            assert_eq!(rows.len(), expected.len());
            for (value, expected) in rows.iter().zip(expected) {
                assert_value(value, expected);
            }
        }
        Native::Bool(b) => assert_eq!(Some(*b), expected.as_bool()),
        Native::Number(n) => match expected.get("number").and_then(Value::as_str) {
            Some("NaN") => assert!(n.is_nan()),
            Some("-0") => assert_eq!(n.to_bits(), (-0.0f64).to_bits()),
            _ => assert_eq!(n.to_bits(), expected.as_f64().unwrap().to_bits()),
        },
        Native::String(s) => assert_eq!(Some(s.as_str()), expected.as_str()),
        Native::Object(fields) => {
            let expected = expected["fields"].as_array().unwrap();
            assert_eq!(fields.len(), expected.len(), "CANONICAL-NATIVE field floor");
            for ((key, value), expected) in fields.iter().zip(expected) {
                assert_eq!(key, expected[0].as_str().unwrap());
                assert_value(value, &expected[1]);
            }
        }
        _ => panic!("unobserved fixture kind"),
    }
}
struct Host {
    calls: RefCell<Vec<V>>,
    error: E,
    events: RefCell<Vec<String>>,
    frozen: RefCell<Vec<V>>,
}
impl Host {
    fn invoke(&self, callee: V, receiver: V, args: Vec<V>) -> Result<V, E> {
        if let Native::Builtin(name) = callee.as_ref() {
            self.events.borrow_mut().push(format!("call:{name}"));
        }
        match callee.as_ref() {
            Native::Function(f) => f(args),
            Native::Builtin("stringify") => {
                let encoded = safe_cex_num::js_json_stringify(&native_json(&args[0])).unwrap();
                Ok(encoded.map(|s| string(&s)).unwrap_or_else(undef))
            }
            Native::Builtin("freeze") => {
                self.frozen.borrow_mut().push(args[0].clone());
                Ok(args[0].clone())
            }
            Native::Builtin("hasOwn") => {
                let key = text(&args[1]);
                Ok(Rc::new(Native::Bool(
                    fields(&args[0]).iter().any(|(k, _)| k == &key),
                )))
            }
            Native::Builtin("localeCompare") => {
                let a = text(&receiver);
                let b = text(&args[0]);
                Ok(Rc::new(Native::Number(if a < b {
                    -1.
                } else if a > b {
                    1.
                } else {
                    0.
                })))
            }
            Native::Builtin("sort") => {
                let Native::Array(values) = receiver.as_ref() else {
                    panic!("sort")
                };
                let mut sorted = values.borrow().clone();
                let mut error = None;
                sorted.sort_by(|a, b| {
                    match self.invoke(args[0].clone(), undef(), vec![a.clone(), b.clone()]) {
                        Ok(value) => number(&value)
                            .partial_cmp(&0.)
                            .unwrap_or(std::cmp::Ordering::Equal),
                        Err(e) => {
                            error = Some(e);
                            std::cmp::Ordering::Equal
                        }
                    }
                });
                if let Some(e) = error {
                    return Err(e);
                }
                values.replace(sorted);
                Ok(receiver.clone())
            }
            Native::Builtin("map") | Native::Builtin("flatMap") => {
                self.calls.borrow_mut().push(args[0].clone());
                let Native::Array(values) = receiver.as_ref() else {
                    panic!("map")
                };
                let input = values.borrow().clone();
                let mut out = vec![];
                for (value, index) in input.into_iter().zip(0..) {
                    let mapped = self.invoke(
                        args[0].clone(),
                        undef(),
                        vec![
                            value,
                            Rc::new(Native::Number(index as f64)),
                            receiver.clone(),
                        ],
                    )?;
                    if matches!(callee.as_ref(), Native::Builtin("flatMap")) {
                        if let Native::Array(values) = mapped.as_ref() {
                            out.extend(values.borrow().iter().cloned());
                        } else {
                            out.push(mapped);
                        }
                    } else {
                        out.push(mapped);
                    }
                }
                Ok(Rc::new(Native::Array(RefCell::new(out))))
            }
            Native::Builtin("push") => {
                let Native::Array(rows) = receiver.as_ref() else {
                    panic!("push")
                };
                rows.borrow_mut().extend(args);
                Ok(Rc::new(Native::Number(rows.borrow().len() as f64)))
            }
            Native::Builtin("join") => {
                let Native::Array(rows) = receiver.as_ref() else {
                    panic!("join")
                };
                Ok(string(
                    &rows
                        .borrow()
                        .iter()
                        .map(native_text)
                        .collect::<Vec<_>>()
                        .join(&text(&args[0])),
                ))
            }
            Native::Builtin("entries") => Ok(Rc::new(Native::Entries(receiver))),
            Native::Builtin("keys") => Ok(Rc::new(Native::Array(RefCell::new(
                fields(&args[0]).iter().map(|(k, _)| string(k)).collect(),
            )))),
            Native::Builtin("is") => Ok(Rc::new(Native::Bool(native_equal(
                &args[0], &args[1], true,
            )))),
            Native::Builtin("Number") => Ok(Rc::new(Native::Number(native_number(&args[0])))),
            Native::Builtin("String") => Ok(string(&native_text(&args[0]))),
            Native::Builtin("isFinite") => Ok(Rc::new(Native::Bool(
                matches!(args[0].as_ref(),Native::Number(n) if n.is_finite()),
            ))),
            Native::Builtin("isSafeInteger") => Ok(Rc::new(Native::Bool(
                matches!(args[0].as_ref(),Native::Number(n) if n.is_finite() && n.fract()==0. && n.abs()<=9007199254740991.),
            ))),
            Native::Builtin("max") => {
                let a = native_number(&args[0]);
                let b = native_number(&args[1]);
                Ok(Rc::new(Native::Number(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                })))
            }
            Native::Builtin("has") | Native::Builtin("get") | Native::Builtin("set") => {
                let Native::Map(entries) = receiver.as_ref() else {
                    panic!("Map")
                };
                let index = entries
                    .borrow()
                    .iter()
                    .position(|(key, _)| native_equal(key, &args[0], false));
                match callee.as_ref() {
                    Native::Builtin("has") => Ok(Rc::new(Native::Bool(index.is_some()))),
                    Native::Builtin("get") => Ok(index
                        .map(|i| entries.borrow()[i].1.clone())
                        .unwrap_or_else(undef)),
                    _ => {
                        if let Some(i) = index {
                            entries.borrow_mut()[i].1 = args[1].clone();
                        } else {
                            entries
                                .borrow_mut()
                                .push((args[0].clone(), args[1].clone()));
                        }
                        Ok(receiver)
                    }
                }
            }
            _ => Err(self.error.clone()),
        }
    }
}
fn operations() -> (Rc<CanonicalOps<V, E>>, Rc<Host>) {
    let h = Rc::new(Host {
        calls: RefCell::new(vec![]),
        error: Rc::new("native-call-error".into()),
        events: RefCell::new(vec![]),
        frozen: RefCell::new(vec![]),
    });
    let calls = h.clone();
    let objects = h.clone();
    let properties = h.clone();
    let globals = h.clone();
    let dynamic = h.clone();
    let ops = Rc::new(CanonicalOps {
        undefined: Rc::new(undef),
        object: Rc::new(move |fields| {
            objects.events.borrow_mut().push(format!(
                "object:{}",
                fields
                    .iter()
                    .map(|(key, _)| *key)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            Rc::new(Native::Object(
                fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
            ))
        }),
        array: Rc::new(|v| Rc::new(Native::Array(RefCell::new(v)))),
        get: Rc::new(move |value, key| {
            properties.events.borrow_mut().push(format!("get:{key}"));
            get(value, key)
        }),
        global: Rc::new(move |name| {
            globals.events.borrow_mut().push(format!("global:{name}"));
            let methods: &[&'static str] = match name {
                "JSON" => &["stringify"],
                "Object" => &["freeze", "hasOwn", "keys", "is"],
                "Math" => &["max"],
                "Number" | "String" | "Map" | "Error" => {
                    return Ok(Rc::new(Native::Builtin(match name {
                        "Number" => "Number",
                        "String" => "String",
                        "Map" => "Map",
                        _ => "Error",
                    })));
                }
                _ => panic!("global {name}"),
            };
            Ok(Rc::new(Native::Object(
                methods
                    .iter()
                    .map(|key| (key.to_string(), Rc::new(Native::Builtin(key))))
                    .collect(),
            )))
        }),
        invoke: Rc::new(move |f, r, a| calls.invoke(f, r, a)),
        function: Rc::new(|_, _, f| Rc::new(Native::Function(f))),
        spread: Rc::new(|value| {
            let Native::Array(v) = value.as_ref() else {
                panic!("spread")
            };
            Ok(v.borrow().clone())
        }),
        truthy: Rc::new(|value| match value.as_ref() {
            Native::Undefined | Native::Null => false,
            Native::Bool(b) => *b,
            Native::Number(n) => *n != 0. && !n.is_nan(),
            Native::String(s) => !s.is_empty(),
            _ => true,
        }),
        strict_equal: Rc::new(|a, b| match (a.as_ref(), b.as_ref()) {
            (Native::String(a), Native::String(b)) => a == b,
            (Native::Number(a), Native::Number(b)) => a == b,
            (Native::Undefined, Native::Undefined) | (Native::Null, Native::Null) => true,
            (Native::Bool(a), Native::Bool(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }),
        string: Rc::new(string),
        number: Rc::new(|n| Rc::new(Native::Number(n))),
        subtract: Rc::new(|a, b| Ok(Rc::new(Native::Number(number(&a) - number(&b))))),
        get_key: Rc::new(move |value, key| {
            dynamic
                .events
                .borrow_mut()
                .push(format!("get_key:{}", native_text(key)));
            get(value, &native_text(key))
        }),
        error: Rc::new(|message| Rc::new(message.to_owned())),
    });
    (ops, h)
}
fn module() -> (CanonicalModule<V, E>, Rc<Host>) {
    let (ops, h) = operations();
    (CanonicalModule::new(ops).unwrap(), h)
}
fn source(cases: &[Value]) -> Value {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-canonical-oracle.cjs"
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
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "source canonical oracle: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn market(symbol: &str) -> Value {
    json!({"id":"distinct-id","symbol":symbol,"base":"BASE","quote":"QUOTE","active":true,"precision":{"amount":0.125,"price":0.25},"limits":{"amount":{"min":3,"max":17},"leverage":{"min":2,"max":29}},"info":{"ignored":true}})
}
#[test]
fn request_contexts_retain_distinct_native_code_units() {
    let request = WooV3PublicRequestContext::new(vec![0xd800, 65]);
    assert_eq!(
        request.symbol(),
        &[0xd800, 65],
        "CANONICAL-NATIVE request UTF16"
    );
    let kline = WooV3KlineRequestContext::new(vec![66, 0xdfff], vec![49, 109]);
    assert_eq!(
        kline.symbol(),
        &[66, 0xdfff],
        "CANONICAL-NATIVE kline symbol"
    );
    assert_eq!(
        kline.interval(),
        &[49, 109],
        "CANONICAL-NATIVE kline interval"
    );
}
#[test]
fn canonical_native_fields_and_source_projection() {
    let (m, _) = module();
    let input = decode(&market("BTC"));
    let output = port::canonical_market(&m.ops, &input).unwrap_or_else(|e| panic!("{e}"));
    let names = fields(&output)
        .into_iter()
        .map(|(k, _)| k)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "id",
            "symbol",
            "base",
            "quote",
            "active",
            "precision",
            "limits"
        ],
        "CANONICAL-NATIVE market ordered fields"
    );
    for (key, expected) in [
        ("id", "distinct-id"),
        ("symbol", "BTC"),
        ("base", "BASE"),
        ("quote", "QUOTE"),
    ] {
        assert_eq!(
            text(&get(&output, key).unwrap()),
            expected,
            "CANONICAL-NATIVE market {key}"
        );
    }
    assert!(
        matches!(get(&output, "active").unwrap().as_ref(), Native::Bool(true)),
        "CANONICAL-NATIVE market active"
    );
    let precision = get(&output, "precision").unwrap();
    assert_eq!(
        fields(&precision)
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["amount", "price"],
        "CANONICAL-NATIVE precision order"
    );
    assert_eq!(
        number(&get(&precision, "amount").unwrap()),
        0.125,
        "CANONICAL-NATIVE amount precision"
    );
    assert_eq!(
        number(&get(&precision, "price").unwrap()),
        0.25,
        "CANONICAL-NATIVE price precision"
    );
    let limits = get(&output, "limits").unwrap();
    assert_eq!(
        fields(&limits)
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["amount", "leverage"],
        "CANONICAL-NATIVE limits order"
    );
    let amount = get(&limits, "amount").unwrap();
    let leverage = get(&limits, "leverage").unwrap();
    for (label, value) in [("amount", &amount), ("leverage", &leverage)] {
        assert_eq!(
            fields(value)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["min", "max"],
            "CANONICAL-NATIVE {label} order"
        );
    }
    assert_eq!(
        number(&get(&amount, "min").unwrap()),
        3.,
        "CANONICAL-NATIVE amount min"
    );
    assert_eq!(
        number(&get(&amount, "max").unwrap()),
        17.,
        "CANONICAL-NATIVE amount max"
    );
    assert_eq!(
        number(&get(&leverage, "min").unwrap()),
        2.,
        "CANONICAL-NATIVE leverage min"
    );
    assert_eq!(
        number(&get(&leverage, "max").unwrap()),
        29.,
        "CANONICAL-NATIVE leverage max"
    );
    assert!(
        matches!(get(&output, "info").unwrap().as_ref(), Native::Undefined),
        "CANONICAL-NATIVE omit info"
    );
    let reference = source(&[json!({"op":"canonicalMarket","input":market("BTC")})]);
    assert_value(&output, &reference[0]);
}
#[test]
fn canonical_sorted_copy_and_persistent_mapper() {
    let (m, h) = module();
    let a = market("ETH");
    let b = market("BTC");
    let input = decode(&json!([a, b]));
    let one = port::canonicalize_markets(&m, input.clone()).unwrap_or_else(|e| panic!("{e}"));
    let two = port::canonicalize_markets(&m, input.clone()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(h.calls.borrow().len(), 2, "CANONICAL-NATIVE map count");
    assert!(
        Rc::ptr_eq(&h.calls.borrow()[0], &h.calls.borrow()[1]),
        "CANONICAL-NATIVE persistent mapper identity"
    );
    let Native::Array(rows) = input.as_ref() else {
        panic!("array")
    };
    assert_eq!(
        text(&get(&rows.borrow()[0], "symbol").unwrap()),
        "ETH",
        "CANONICAL-NATIVE input remains unsorted"
    );
    let encoded = text(&one);
    assert!(
        encoded.find("BTC").unwrap() < encoded.find("ETH").unwrap(),
        "CANONICAL-NATIVE sorted result"
    );
    assert_eq!(text(&two), encoded);
    let expected =
        source(&[json!({"op":"canonicalizeWooV3Markets","input":[market("ETH"),market("BTC")]})]);
    assert_eq!(encoded, expected[0]);
}
#[test]
fn ticker_statistics_keep_distinct_native_fields() {
    let (m, _) = module();
    let input = json!({"id":"id","symbol":"BTC","index":11,"mark":13,"percentage":17,"percentage24h":19,"openInterest":23,"fundingRate":29,"estimatedFundingRate":31,"fundingTime":37,"volume":41,"volume24h":43,"quoteVolume":47,"quoteVolume24h":53,"last":59,"bid":61,"ask":67});
    let value = decode(&input);
    let legacy =
        port::canonical_legacy_ticker_stats(&m.ops, &value).unwrap_or_else(|e| panic!("{e}"));
    let v3 = port::canonical_v3_ticker_stats(&m.ops, &value).unwrap_or_else(|e| panic!("{e}"));
    for (label, value) in [("legacy", &legacy), ("V3", &v3)] {
        assert_eq!(
            fields(value)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            [
                "id",
                "symbol",
                "index",
                "mark",
                "percentage",
                "openInterest",
                "fundingRate",
                "fundingTime",
                "volume",
                "quoteVolume"
            ],
            "CANONICAL-NATIVE {label} ordered fields"
        );
        assert_eq!(
            text(&get(value, "id").unwrap()),
            "id",
            "CANONICAL-NATIVE {label} id"
        );
        assert_eq!(
            text(&get(value, "symbol").unwrap()),
            "BTC",
            "CANONICAL-NATIVE {label} symbol"
        );
        for (key, expected) in [
            ("index", 11.),
            ("mark", 13.),
            ("openInterest", 23.),
            ("fundingTime", 37.),
        ] {
            assert_eq!(
                number(&get(value, key).unwrap()),
                expected,
                "CANONICAL-NATIVE {label} field {key}"
            );
        }
    }
    for (key, lv, vv) in [
        ("percentage", 17., 19.),
        ("fundingRate", 29., 31.),
        ("volume", 41., 43.),
        ("quoteVolume", 47., 53.),
    ] {
        assert_eq!(
            number(&get(&legacy, key).unwrap()),
            lv,
            "CANONICAL-NATIVE legacy field {key}"
        );
        assert_eq!(
            number(&get(&v3, key).unwrap()),
            vv,
            "CANONICAL-NATIVE V3 field {key}"
        );
    }
    assert_eq!(
        fields(&legacy).len(),
        10,
        "CANONICAL-NATIVE legacy field floor"
    );
    assert_eq!(fields(&v3).len(), 10, "CANONICAL-NATIVE V3 field floor");
    let rows = source(&[
        json!({"op":"canonicalLegacyTickerStats","input":input}),
        json!({"op":"canonicalV3TickerStats","input":input}),
    ]);
    assert_value(&legacy, &rows[0]);
    assert_value(&v3, &rows[1]);
}
#[test]
fn canonical_string_and_legacy_ticker_sort() {
    let (m, h) = module();
    let result = port::canonical_string(&m.ops, undef()).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(result.as_ref(), Native::Undefined),
        "CANONICAL-NATIVE undefined stringify result"
    );
    let input = json!([{"symbol":"ETH","id":"e","mark":13},{"symbol":"BTC","id":"b","mark":17}]);
    let value =
        port::canonicalize_legacy_tickers(&m, decode(&input)).unwrap_or_else(|e| panic!("{e}"));
    let literal = "[{\"id\":\"b\",\"symbol\":\"BTC\",\"mark\":17},{\"id\":\"e\",\"symbol\":\"ETH\",\"mark\":13}]";
    assert_eq!(
        text(&value),
        literal,
        "CANONICAL-NATIVE independent ticker canonical"
    );
    assert_eq!(h.calls.borrow().len(), 1);
    let expected = source(&[json!({"op":"canonicalizeWooLegacyTickerStats","input":input})]);
    assert_eq!(text(&value), expected[0]);
}
#[test]
fn order_level_and_candle_native_fields() {
    let (m, _) = module();
    let level = json!({"price":11,"amount":13,"total":17,"ignored":19});
    let candle = json!({"timestamp":101,"open":103,"high":107,"low":109,"close":113,"volume":127,"ignored":131});
    let level_value = port::canonical_order_book_level(&m.ops, &decode(&level)).unwrap();
    let candle_value = port::canonical_candle(&m.ops, &decode(&candle)).unwrap();
    for (label, value, expected) in [
        (
            "level",
            &level_value,
            vec![("price", 11.), ("amount", 13.), ("total", 17.)],
        ),
        (
            "candle",
            &candle_value,
            vec![
                ("timestamp", 101.),
                ("open", 103.),
                ("high", 107.),
                ("low", 109.),
                ("close", 113.),
                ("volume", 127.),
            ],
        ),
    ] {
        assert_eq!(
            fields(value)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            expected.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            "VALUES-NATIVE {label} ordered keys"
        );
        for (key, expected) in expected {
            assert_eq!(
                number(&get(value, key).unwrap()),
                expected,
                "VALUES-NATIVE {label} {key}"
            );
        }
        assert!(matches!(
            get(value, "ignored").unwrap().as_ref(),
            Native::Undefined
        ));
    }
    let expected = source(&[
        json!({"op":"canonicalOrderBookLevel","input":level}),
        json!({"op":"canonicalCandle","input":candle}),
    ]);
    assert_value(&level_value, &expected[0]);
    assert_value(&candle_value, &expected[1]);
}
#[test]
fn order_book_native_canonical_and_empty_request_errors() {
    let (m, h) = module();
    let request = json!({"symbol":"BTC"});
    let book = json!({"bids":[{"price":11,"amount":13,"total":17}],"asks":[{"price":19,"amount":23,"total":29}],"timestamp":31});
    let a = port::canonicalize_order_book_values(&m, &decode(&request), &decode(&book)).unwrap();
    let b = port::canonicalize_v3_order_book(&m, &decode(&request), &decode(&book)).unwrap();
    let literal = "{\"symbol\":\"BTC\",\"bids\":[{\"price\":11,\"amount\":13,\"total\":17}],\"asks\":[{\"price\":19,\"amount\":23,\"total\":29}]}";
    assert_eq!(text(&a), literal, "VALUES-NATIVE order book");
    assert_eq!(text(&b), literal, "VALUES-NATIVE V3 order book");
    assert_eq!(
        h.calls.borrow().len(),
        4,
        "VALUES-NATIVE book mapping calls"
    );
    assert!(
        h.calls
            .borrow()
            .iter()
            .all(|v| Rc::ptr_eq(v, &h.calls.borrow()[0])),
        "VALUES-NATIVE stable level mapper"
    );
    let mut cases = vec![
        json!({"op":"canonicalizeOrderBookValues","args":[request,book]}),
        json!({"op":"canonicalizeWooV3OrderBook","args":[request,book]}),
    ];
    for request in [json!({"symbol":""}), json!({})] {
        let error = port::canonicalize_order_book_values(&m, &decode(&request), &undef())
            .err()
            .unwrap();
        assert_eq!(
            error.as_str(),
            "WOO orderbook request symbol must be non-empty",
            "VALUES-NATIVE invalid book request"
        );
        cases.push(json!({"op":"canonicalizeOrderBookValues","args":[request,null],"throws":true}));
    }
    let expected = source(&cases);
    assert_eq!(expected[0], literal);
    assert_eq!(expected[1], literal);
    for row in &expected.as_array().unwrap()[2..] {
        assert_eq!(
            row["throwMessage"],
            "WOO orderbook request symbol must be non-empty"
        );
    }
}
#[test]
fn interval_table_and_blocked_reasons_are_native() {
    let (m, h) = module();
    let mut results = vec![];
    let mut cases = vec![];
    for (interval, duration) in [
        ("1m", 60000.),
        ("3m", 180000.),
        ("5m", 300000.),
        ("15m", 900000.),
        ("30m", 1800000.),
        ("1h", 3600000.),
        ("2h", 7200000.),
        ("4h", 14400000.),
        ("6h", 21600000.),
        ("12h", 43200000.),
        ("1d", 86400000.),
        ("1w", 604800000.),
    ] {
        h.events.borrow_mut().clear();
        let value = port::normalize_kline_interval(&m, string(interval)).unwrap();
        let trace = std::mem::take(&mut *h.events.borrow_mut());
        if interval == "1m" {
            assert_eq!(
                trace,
                [
                    "global:Object",
                    "get:hasOwn",
                    "call:hasOwn",
                    "global:Object",
                    "get:freeze",
                    "get_key:1m",
                    "object:status,interval,durationMs",
                    "call:freeze"
                ],
                "VALUES-NATIVE freeze callee before duration lookup"
            );
        }
        assert_eq!(
            fields(&value)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["status", "interval", "durationMs"],
            "VALUES-NATIVE ready order"
        );
        assert_eq!(text(&get(&value, "status").unwrap()), "ready");
        assert_eq!(
            text(&get(&value, "interval").unwrap()),
            interval,
            "VALUES-NATIVE exact interval"
        );
        assert_eq!(
            number(&get(&value, "durationMs").unwrap()),
            duration,
            "VALUES-NATIVE duration {interval}"
        );
        cases.push(json!({"op":"normalizeKlineInterval","input":interval}));
        results.push(value);
    }
    for (interval, normalized, reason) in [
        ("1M", "1mon", "variable-calendar-duration"),
        ("1mon", "1mon", "variable-calendar-duration"),
        ("1y", "1y", "variable-calendar-duration"),
        ("bad", "bad", "unsupported"),
        ("__proto__", "__proto__", "unsupported"),
    ] {
        let value = port::normalize_kline_interval(&m, string(interval)).unwrap();
        assert_eq!(
            fields(&value)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["status", "normalized", "reason"],
            "VALUES-NATIVE blocked order"
        );
        for (key, expected) in [
            ("status", "blocked"),
            ("normalized", normalized),
            ("reason", reason),
        ] {
            assert_eq!(
                text(&get(&value, key).unwrap()),
                expected,
                "VALUES-NATIVE blocked {interval} {key}"
            );
        }
        cases.push(json!({"op":"normalizeKlineInterval","input":interval}));
        results.push(value);
    }
    assert_eq!(results.len(), 17, "VALUES-NATIVE interval fixture floor");
    let expected = source(&cases);
    for (value, expected) in results.iter().zip(expected.as_array().unwrap()) {
        assert_value(value, expected);
    }
}
#[test]
fn candle_request_sort_and_legacy_refusal_native() {
    let (m, h) = module();
    let request = json!({"symbol":"BTC","interval":"1m"});
    let candles = json!([{"timestamp":3,"open":5,"high":7,"low":11,"close":13,"volume":17},{"timestamp":2,"open":19,"high":23,"low":29,"close":31,"volume":37}]);
    let input = decode(&candles);
    let a = port::canonicalize_candles_for_request(&m, &decode(&request), input.clone()).unwrap();
    let b = port::canonicalize_legacy_candles(&m, &decode(&request), input.clone()).unwrap();
    let literal = "{\"symbol\":\"BTC\",\"interval\":\"1m\",\"candles\":[{\"timestamp\":2,\"open\":19,\"high\":23,\"low\":29,\"close\":31,\"volume\":37},{\"timestamp\":3,\"open\":5,\"high\":7,\"low\":11,\"close\":13,\"volume\":17}]}";
    assert_eq!(text(&a), literal, "VALUES-NATIVE candle sort");
    assert_eq!(text(&b), literal, "VALUES-NATIVE legacy candle wrapper");
    let Native::Array(rows) = input.as_ref() else {
        panic!("array")
    };
    assert_eq!(
        number(&get(&rows.borrow()[0], "timestamp").unwrap()),
        3.,
        "VALUES-NATIVE candles input unchanged"
    );
    assert!(
        Rc::ptr_eq(&h.calls.borrow()[0], &h.calls.borrow()[1]),
        "VALUES-NATIVE stable candle mapper"
    );
    let mut cases = vec![
        json!({"op":"canonicalizeCandlesForRequest","args":[request,candles]}),
        json!({"op":"canonicalizeWooLegacyCandles","args":[request,candles]}),
    ];
    for request in [
        json!({"symbol":"","interval":"1m"}),
        json!({"symbol":"BTC","interval":"1M"}),
    ] {
        let error = port::canonicalize_legacy_candles(&m, &decode(&request), undef())
            .err()
            .unwrap();
        assert_eq!(
            error.as_str(),
            "WOO legacy kline request cannot form an exact fixed-interval canonical",
            "VALUES-NATIVE invalid candle request"
        );
        cases.push(json!({"op":"canonicalizeWooLegacyCandles","args":[request,[]],"throws":true}));
    }
    let expected = source(&cases);
    assert_eq!(expected[0], literal);
    assert_eq!(expected[1], literal);
    for row in &expected.as_array().unwrap()[2..] {
        assert_eq!(
            row["throwMessage"],
            "WOO legacy kline request cannot form an exact fixed-interval canonical"
        );
    }
}
#[test]
fn v3_ticker_canonical_uses_v3_mapper_native() {
    let (m, h) = module();
    let input = json!([{"symbol":"ETH","estimatedFundingRate":31,"fundingRate":99},{"symbol":"BTC","estimatedFundingRate":17,"fundingRate":98}]);
    let a = port::canonicalize_v3_tickers(&m, decode(&input)).unwrap();
    let b = port::canonicalize_v3_tickers(&m, decode(&input)).unwrap();
    let literal =
        "[{\"symbol\":\"BTC\",\"fundingRate\":17},{\"symbol\":\"ETH\",\"fundingRate\":31}]";
    assert_eq!(text(&a), literal, "VALUES-NATIVE V3 ticker mapper");
    assert_eq!(text(&b), literal);
    assert!(
        Rc::ptr_eq(&h.calls.borrow()[0], &h.calls.borrow()[1]),
        "VALUES-NATIVE persistent V3 mapper"
    );
    let expected = source(&[json!({"op":"canonicalizeWooV3TickerStats","input":input})]);
    assert_eq!(expected[0], literal);
}
fn native_number(value: &V) -> f64 {
    match value.as_ref() {
        Native::Number(n) => *n,
        Native::Null => 0.,
        Native::Undefined => f64::NAN,
        Native::Bool(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        Native::String(s) => safe_cex_num::js_number(Some(&Value::String(s.clone()))).unwrap(),
        _ => f64::NAN,
    }
}
fn native_text(value: &V) -> String {
    match value.as_ref() {
        Native::String(s) => s.clone(),
        Native::Number(n) => safe_cex_num::js_number_string(*n),
        Native::Bool(b) => b.to_string(),
        Native::Null => "null".into(),
        Native::Undefined => "undefined".into(),
        _ => "[object Object]".into(),
    }
}
fn native_equal(a: &V, b: &V, object_is: bool) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::Number(a), Native::Number(b)) => {
            (a.is_nan() && b.is_nan())
                || (a == b
                    && (!object_is || *a != 0. || a.is_sign_negative() == b.is_sign_negative()))
        }
        (Native::Bool(a), Native::Bool(b)) => a == b,
        (Native::String(a), Native::String(b)) => a == b,
        (Native::Null, Native::Null) | (Native::Undefined, Native::Undefined) => true,
        _ => Rc::ptr_eq(a, b),
    }
}
fn snapshot_module() -> (Rc<SnapshotParityOps<V, E>>, Rc<Host>) {
    let (value, h) = operations();
    let language = SnapshotPrimitives {
        template: Rc::new(|parts, values| {
            assert_eq!(parts.len(), values.len() + 1);
            let mut result = parts[0].to_owned();
            for (v, part) in values.iter().zip(&parts[1..]) {
                result.push_str(&native_text(v));
                result.push_str(part);
            }
            Ok(string(&result))
        }),
        construct: Rc::new(|callee, args| match callee.as_ref() {
            Native::Builtin("Map") => Ok(Rc::new(Native::Map(RefCell::new(vec![])))),
            Native::Builtin("Error") => Ok(Rc::new(Native::Object(vec![(
                "message".into(),
                args[0].clone(),
            )]))),
            _ => panic!("constructor"),
        }),
        throw_value: Rc::new(|v| Rc::new(text(&get(&v, "message").unwrap()))),
        is_undefined: Rc::new(|v| matches!(v.as_ref(), Native::Undefined)),
        is_nullish: Rc::new(|v| matches!(v.as_ref(), Native::Undefined | Native::Null)),
        less_than: Rc::new(|a, b| Ok(native_number(&a) < native_number(&b))),
        add: Rc::new(|a, b| {
            Ok(
                if matches!(a.as_ref(), Native::String(_))
                    || matches!(b.as_ref(), Native::String(_))
                {
                    string(&format!("{}{}", native_text(&a), native_text(&b)))
                } else {
                    Rc::new(Native::Number(native_number(&a) + native_number(&b)))
                },
            )
        }),
        divide: Rc::new(|a, b| {
            Ok(Rc::new(Native::Number(
                native_number(&a) / native_number(&b),
            )))
        }),
        iterate: Rc::new(|value, callback| {
            let mut i = 0;
            loop {
                let next = match value.as_ref() {
                    Native::Array(rows) => rows.borrow().get(i).cloned(),
                    Native::Entries(array) => {
                        let Native::Array(rows) = array.as_ref() else {
                            panic!("entries array")
                        };
                        rows.borrow().get(i).cloned().map(|v| {
                            Rc::new(Native::Array(RefCell::new(vec![
                                Rc::new(Native::Number(i as f64)),
                                v,
                            ])))
                        })
                    }
                    Native::Map(rows) => rows
                        .borrow()
                        .get(i)
                        .cloned()
                        .map(|(k, v)| Rc::new(Native::Array(RefCell::new(vec![k, v])))),
                    _ => panic!("finite iterable"),
                };
                let Some(next) = next else {
                    break;
                };
                callback(next)?;
                i += 1;
            }
            Ok(())
        }),
        binding: Rc::new(|value, count| {
            let Native::Array(rows) = value.as_ref() else {
                panic!("binding")
            };
            Ok((0..count)
                .map(|i| rows.borrow().get(i).cloned().unwrap_or_else(undef))
                .collect())
        }),
    };
    (SnapshotParityOps::new(value, language).unwrap(), h)
}
fn array(value: &V) -> Vec<V> {
    let Native::Array(rows) = value.as_ref() else {
        panic!("SNAPSHOT-NATIVE array")
    };
    rows.borrow().clone()
}
fn expected_record(fields: Vec<(&str, Value)>) -> Value {
    json!({"fields":fields.into_iter().map(|(k,v)|json!([k,v])).collect::<Vec<_>>()})
}
fn expected_mismatch(
    kind: &str,
    path: &str,
    left: Value,
    right: Value,
    lp: &str,
    rp: &str,
) -> Value {
    expected_record(vec![
        ("kind", json!(kind)),
        ("path", json!(path)),
        ("legacyValue", left),
        ("v3Value", right),
        (
            "provenance",
            expected_record(vec![("legacy", json!(lp)), ("v3", json!(rp))]),
        ),
    ])
}
fn assert_native_frozen(h: &Host, value: &V, label: &str) {
    assert!(
        h.frozen.borrow().iter().any(|v| Rc::ptr_eq(v, value)),
        "SNAPSHOT-NATIVE required freeze {label}"
    );
}
fn assert_native_mismatch_freezes(h: &Host, rows: &V) {
    for row in array(rows) {
        assert_native_frozen(h, &row, "mismatch");
        assert_native_frozen(h, &get(&row, "provenance").unwrap(), "provenance");
    }
}
fn assert_native_result_freezes(h: &Host, value: &V) {
    assert_native_frozen(h, value, "result");
    let mismatches = get(value, "mismatches").unwrap();
    let blockers = get(value, "blockers").unwrap();
    assert_native_frozen(h, &mismatches, "mismatches array");
    assert_native_frozen(h, &blockers, "blockers array");
    assert_native_mismatch_freezes(h, &mismatches);
    for row in array(&blockers) {
        assert_native_frozen(h, &row, "blocker");
    }
}
fn native_result_header(
    value: &V,
    status: &str,
    route: &str,
    legacy: &V,
    v3: &V,
    mismatches: usize,
    blockers: usize,
) {
    assert_eq!(
        fields(value)
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        [
            "status",
            "routeId",
            "authoritative",
            "observed",
            "legacyCanonical",
            "v3Canonical",
            "mismatches",
            "blockers",
            "storeAction"
        ],
        "SNAPSHOT-NATIVE result ordered keys"
    );
    assert_eq!(
        text(&get(value, "status").unwrap()),
        status,
        "SNAPSHOT-NATIVE result status"
    );
    assert_eq!(text(&get(value, "routeId").unwrap()), route);
    assert!(
        Rc::ptr_eq(&get(value, "authoritative").unwrap(), legacy),
        "SNAPSHOT-NATIVE authoritative alias"
    );
    assert!(
        Rc::ptr_eq(&get(value, "observed").unwrap(), v3),
        "SNAPSHOT-NATIVE observed alias"
    );
    assert_eq!(
        array(&get(value, "mismatches").unwrap()).len(),
        mismatches,
        "SNAPSHOT-NATIVE mismatch floor"
    );
    assert_eq!(
        array(&get(value, "blockers").unwrap()).len(),
        blockers,
        "SNAPSHOT-NATIVE blocker floor"
    );
    assert_eq!(text(&get(value, "storeAction").unwrap()), "retain-legacy");
}
fn literal_market(symbol: &str, id: &str, price: &str) -> String {
    format!(
        "{{\"id\":\"{id}\",\"symbol\":\"{symbol}\",\"base\":\"BASE\",\"quote\":\"QUOTE\",\"active\":true,\"precision\":{{\"amount\":0.125,\"price\":{price}}},\"limits\":{{\"amount\":{{\"min\":3,\"max\":17}},\"leverage\":{{\"min\":2,\"max\":29}}}}}}"
    )
}
#[test]
fn snapshot_initialization_and_unsupported_interval_native() {
    let (m, h) = snapshot_module();
    let sequence = h
        .events
        .borrow()
        .iter()
        .filter(|e| e.as_str() == "call:map" || e.as_str() == "call:freeze")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        sequence,
        ["call:map", "call:freeze", "call:map"],
        "SNAPSHOT-NATIVE table and interval initialization"
    );
    let mut native = vec![];
    let mut cases = vec![];
    for (interval, normalized, reason, detail) in [
        (
            "1M",
            "1mon",
            "variable-calendar-duration",
            "WOO 1mon uses variable calendar boundaries; fixed endTimestamp parity is intentionally blocked",
        ),
        (
            "bad",
            "bad",
            "unsupported",
            "Unsupported WOO kline interval bad",
        ),
    ] {
        let input = json!({"status":"blocked","normalized":normalized,"reason":reason});
        let output = diagnostic::unsupported_interval_blocker(
            &m,
            string("v3"),
            string(interval),
            &decode(&input),
        )
        .unwrap();
        let expected = expected_record(vec![
            ("code", json!("unsupported-kline-interval")),
            ("detail", json!(detail)),
            ("path", json!("kline.request.v3.interval")),
            (
                "expected",
                json!([
                    "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "12h", "1d", "1w"
                ]),
            ),
            ("actual", json!(interval)),
        ]);
        assert_value(&output, &expected);
        assert_native_frozen(&h, &output, "unsupported interval blocker");
        native.push(output);
        cases.push(json!({"op":"unsupportedIntervalBlocker","args":["v3",interval,input]}));
    }
    let rows = source(&cases);
    for (value, expected) in native.iter().zip(rows.as_array().unwrap()) {
        assert_value(value, expected);
    }
}
#[test]
fn kline_context_blockers_and_addition_are_native() {
    let (m, h) = snapshot_module();
    let cases = [
        (
            json!({"legacyRequest":{"symbol":"BTC","interval":"1m"},"v3Request":{"symbol":"BTC","interval":"1m"},"v3Rows":[{"symbol":"BTC","type":"1m","startTimestamp":0,"endTimestamp":60000}]}),
            vec![],
        ),
        (
            json!({"legacyRequest":{"symbol":"","interval":"1M"},"v3Request":{"symbol":"","interval":"5m"},"v3Rows":[{"symbol":"OTHER","type":"1m","startTimestamp":0,"endTimestamp":1}]}),
            vec![
                "invalid-request-context",
                "unsupported-kline-interval",
                "v3-kline-row-symbol-mismatch",
                "v3-kline-row-interval-mismatch",
                "v3-kline-row-end-timestamp-mismatch",
            ],
        ),
        (
            json!({"legacyRequest":{"symbol":"BTC","interval":"1m"},"v3Request":{"symbol":"ETH","interval":"3m"},"v3Rows":[{"symbol":"ETH","type":"3m","startTimestamp":0,"endTimestamp":180000}]}),
            vec!["request-symbol-mismatch", "request-interval-mismatch"],
        ),
        (
            json!({"legacyRequest":{"symbol":"BTC","interval":"1m"},"v3Request":{"symbol":"BTC","interval":"1y"},"v3Rows":[]}),
            vec!["unsupported-kline-interval"],
        ),
        (
            json!({"legacyRequest":{"symbol":"BTC","interval":"1m"},"v3Request":{"symbol":"BTC","interval":"1m"},"v3Rows":[{"symbol":"BTC","type":"1m","startTimestamp":{"$nativeNumber":"NaN"},"endTimestamp":0}]}),
            vec!["v3-kline-row-end-timestamp-mismatch"],
        ),
        (
            json!({"legacyRequest":{"symbol":"BTC","interval":"1m"},"v3Request":{"symbol":"BTC","interval":"1m"},"v3Rows":[{"symbol":"BTC","type":"1m","startTimestamp":"1000","endTimestamp":61000}]}),
            vec!["v3-kline-row-end-timestamp-mismatch"],
        ),
    ];
    let mut native = vec![];
    let mut reference = vec![];
    for (index, (input, codes)) in cases.iter().enumerate() {
        let output = diagnostic::validate_kline_context(&m, &decode(input)).unwrap();
        assert_value(&output, &expected_validation_case(index));
        assert_native_frozen(&h, &output, "validation result");
        for key in ["blockers", "legacyInterval", "v3Interval"] {
            assert_native_frozen(&h, &get(&output, key).unwrap(), key);
        }
        for row in array(&get(&output, "blockers").unwrap()) {
            assert_native_frozen(&h, &row, "validation blocker");
        }
        assert_eq!(
            fields(&output)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["blockers", "legacyInterval", "v3Interval"],
            "SNAPSHOT-NATIVE validation ordered keys"
        );
        let blockers = array(&get(&output, "blockers").unwrap());
        assert_eq!(
            blockers.len(),
            codes.len(),
            "SNAPSHOT-NATIVE validation blocker floor {index}"
        );
        for (row, code) in blockers.iter().zip(codes) {
            assert_eq!(
                text(&get(row, "code").unwrap()),
                *code,
                "SNAPSHOT-NATIVE blocker order {index}"
            );
        }
        if index == 1 {
            let paths = [
                "kline.request.symbol",
                "kline.request.legacy.interval",
                "kline.v3Rows[0].symbol",
                "kline.v3Rows[0].type",
                "kline.v3Rows[0].endTimestamp",
            ];
            for (row, path) in blockers.iter().zip(paths) {
                assert_eq!(text(&get(row, "path").unwrap()), path);
            }
            assert_eq!(
                number(&get(&blockers[4], "expected").unwrap()),
                300000.,
                "SNAPSHOT-NATIVE end timestamp expectation"
            );
            assert_eq!(number(&get(&blockers[4], "actual").unwrap()), 1.);
            let actual = get(&blockers[0], "actual").unwrap();
            assert_eq!(text(&get(&actual, "legacy").unwrap()), "");
            assert_eq!(text(&get(&actual, "v3").unwrap()), "");
        }
        if index == 2 {
            assert_eq!(text(&get(&blockers[0], "expected").unwrap()), "BTC");
            assert_eq!(text(&get(&blockers[0], "actual").unwrap()), "ETH");
            assert_eq!(text(&get(&blockers[1], "expected").unwrap()), "1m");
            assert_eq!(text(&get(&blockers[1], "actual").unwrap()), "3m");
        }
        if index == 4 {
            assert!(
                number(&get(&blockers[0], "expected").unwrap()).is_nan(),
                "SNAPSHOT-NATIVE unsafe NaN timestamp"
            );
        }
        if index == 5 {
            assert_eq!(
                text(&get(&blockers[0], "expected").unwrap()),
                "100060000",
                "SNAPSHOT-NATIVE source addition retains string"
            );
        }
        native.push(output);
        reference.push(json!({"op":"validateKlineContext","input":input}));
    }
    assert_eq!(native.len(), 6, "SNAPSHOT-NATIVE context case floor");
    let rows = source(&reference);
    for (value, expected) in native.iter().zip(rows.as_array().unwrap()) {
        assert_value(value, expected);
    }
}
#[test]
fn v3_rows_native_mapping_canonical_and_refusals() {
    let (m, _h) = snapshot_module();
    let request = json!({"symbol":"BTC","interval":"1m"});
    let rows = json!([{"symbol":"BTC","type":"1m","startTimestamp":3000,"endTimestamp":63000,"open":"11","high":"13","low":"17","close":"19","volume":"23"},{"symbol":"BTC","type":"1m","startTimestamp":1000,"endTimestamp":61000,"open":"29","high":"31","low":"37","close":"41","volume":"43"}]);
    let mapped = diagnostic::map_v3_candles(m.clone(), decode(&rows)).unwrap();
    let values = array(&mapped);
    assert_eq!(values.len(), 2, "SNAPSHOT-NATIVE mapped row floor");
    for (value, expected) in values
        .iter()
        .zip([[1., 29., 31., 37., 41., 43.], [3., 11., 13., 17., 19., 23.]])
    {
        assert_eq!(
            fields(value)
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["timestamp", "open", "high", "low", "close", "volume"]
        );
        for (key, expected) in ["timestamp", "open", "high", "low", "close", "volume"]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(
                number(&get(value, key).unwrap()),
                expected,
                "SNAPSHOT-NATIVE mapped candle {key}"
            );
        }
    }
    let canonical =
        diagnostic::canonicalize_v3_kline_rows(m.clone(), &decode(&request), decode(&rows))
            .unwrap();
    let literal = "{\"symbol\":\"BTC\",\"interval\":\"1m\",\"candles\":[{\"timestamp\":1,\"open\":29,\"high\":31,\"low\":37,\"close\":41,\"volume\":43},{\"timestamp\":3,\"open\":11,\"high\":13,\"low\":17,\"close\":19,\"volume\":23}]}";
    assert_eq!(text(&canonical), literal, "SNAPSHOT-NATIVE V3 canonical");
    let mut wrong = rows.clone();
    wrong[0]["type"] = json!("3m");
    let error =
        diagnostic::canonicalize_v3_kline_rows(m.clone(), &decode(&request), decode(&wrong))
            .err()
            .unwrap();
    assert_eq!(
        error.as_str(),
        "WOO V3 kline rows cannot form an exact canonical: v3-kline-row-interval-mismatch",
        "SNAPSHOT-NATIVE validation refusal"
    );
    let month = json!({"symbol":"BTC","interval":"1M"});
    let empty = json!([]);
    let error = diagnostic::canonicalize_v3_kline_rows(m.clone(), &decode(&month), decode(&empty))
        .err()
        .unwrap();
    assert_eq!(
        error.as_str(),
        "WOO V3 kline rows cannot form an exact canonical: unsupported-kline-interval,unsupported-kline-interval",
        "SNAPSHOT-NATIVE blocked request codes"
    );
    let mut negative = rows.clone();
    negative[0]["open"] = json!("-1");
    let error = diagnostic::map_v3_candles(m, decode(&negative))
        .err()
        .unwrap();
    assert_eq!(
        error.as_str(),
        "WOO V3 kline[0].open must be at least 0",
        "SNAPSHOT-NATIVE negative field refusal"
    );
    let expected = source(&[
        json!({"op":"mapWooV3Candles","input":rows}),
        json!({"op":"canonicalizeWooV3KlineRows","args":[request,rows]}),
        json!({"op":"canonicalizeWooV3KlineRows","args":[request,wrong],"throws":true}),
        json!({"op":"canonicalizeWooV3KlineRows","args":[month,empty],"throws":true}),
        json!({"op":"mapWooV3Candles","input":negative,"throws":true}),
    ]);
    assert_value(&mapped, &expected[0]);
    assert_eq!(expected[1], literal);
    assert_eq!(
        expected[2]["throwMessage"],
        "WOO V3 kline rows cannot form an exact canonical: v3-kline-row-interval-mismatch"
    );
    assert_eq!(
        expected[3]["throwMessage"],
        "WOO V3 kline rows cannot form an exact canonical: unsupported-kline-interval,unsupported-kline-interval"
    );
    assert_eq!(
        expected[4]["throwMessage"],
        "WOO V3 kline[0].open must be at least 0"
    );
}
#[test]
fn orderbook_side_native_values_missing_levels_and_same_value() {
    let (m, h) = snapshot_module();
    let left = json!([{"price":{"$nativeNumber":"-0"},"amount":11,"total":13},{"price":19,"amount":23,"total":29}]);
    let right = json!([{"price":0,"amount":17,"total":13}]);
    let output =
        diagnostic::compare_order_book_side(&m, string("bids"), decode(&left), decode(&right))
            .unwrap();
    let rows = array(&output);
    assert_eq!(rows.len(), 3, "SNAPSHOT-NATIVE side mismatch floor");
    assert_value(
        &rows[0],
        &expected_mismatch(
            "value",
            "orderbook.bids[0].price",
            json!({"number":"-0"}),
            json!(0),
            "v1 bids[].price",
            "V3 bids[].price",
        ),
    );
    assert_value(
        &rows[1],
        &expected_mismatch(
            "value",
            "orderbook.bids[0].amount",
            json!(11),
            json!(17),
            "v1 bids[].quantity",
            "V3 bids[].quantity",
        ),
    );
    assert_value(
        &rows[2],
        &expected_mismatch(
            "missing-in-v3",
            "orderbook.bids[1]",
            expected_record(vec![
                ("price", json!(19)),
                ("amount", json!(23)),
                ("total", json!(29)),
            ]),
            json!({"undefined":true}),
            "v1 price/quantity normalized with cumulative total",
            "V3 price/quantity normalized with cumulative total",
        ),
    );
    let unexpected =
        diagnostic::compare_order_book_side(&m, string("asks"), decode(&json!([])), decode(&right))
            .unwrap();
    let extra = array(&unexpected);
    assert_eq!(extra.len(), 1, "SNAPSHOT-NATIVE unexpected level floor");
    assert_value(
        &extra[0],
        &expected_mismatch(
            "unexpected-in-v3",
            "orderbook.asks[0]",
            json!({"undefined":true}),
            expected_record(vec![
                ("price", json!(0)),
                ("amount", json!(17)),
                ("total", json!(13)),
            ]),
            "v1 price/quantity normalized with cumulative total",
            "V3 price/quantity normalized with cumulative total",
        ),
    );
    let nan = json!([{"price":{"$nativeNumber":"NaN"},"amount":11,"total":13}]);
    let equal = diagnostic::compare_order_book_side(&m, string("bids"), decode(&nan), decode(&nan))
        .unwrap();
    assert_eq!(array(&equal).len(), 0, "SNAPSHOT-NATIVE NaN Object.is");
    let empty = diagnostic::compare_order_book_side(
        &m,
        string("asks"),
        decode(&json!([])),
        decode(&json!([])),
    )
    .unwrap();
    assert_eq!(array(&empty).len(), 0);
    assert_native_mismatch_freezes(&h, &output);
    assert_native_mismatch_freezes(&h, &unexpected);
    let expected = source(&[
        json!({"op":"compareOrderBookSide","args":["bids",left,right]}),
        json!({"op":"compareOrderBookSide","args":["asks",[],right]}),
        json!({"op":"compareOrderBookSide","args":["bids",nan,nan]}),
        json!({"op":"compareOrderBookSide","args":["asks",[],[]]}),
    ]);
    for (value, expected) in [&output, &unexpected, &equal, &empty]
        .into_iter()
        .zip(expected.as_array().unwrap())
    {
        assert_value(value, expected);
    }
}
#[test]
fn orderbook_comparison_native_status_canonical_and_aliases() {
    let (m, h) = snapshot_module();
    let book = json!({"timestamp":17,"bids":[{"price":11,"amount":13,"total":17}],"asks":[]});
    let v3 = json!({"timestamp":19,"bids":[{"price":11,"amount":13,"total":17}],"asks":[]});
    let mut native = vec![];
    let mut cases = vec![];
    for (symbol, other, status, blocker_code) in [
        ("BTC", "BTC", "match", None),
        ("", "BTC", "blocked", Some("invalid-request-context")),
        ("BTC", "ETH", "blocked", Some("request-symbol-mismatch")),
    ] {
        let input = json!({"legacyRequest":{"symbol":symbol},"v3Request":{"symbol":other},"legacyOrderBook":book,"v3OrderBook":v3});
        let value = decode(&input);
        let left = get(&value, "legacyOrderBook").unwrap();
        let right = get(&value, "v3OrderBook").unwrap();
        let result = diagnostic::compare_order_book(&m, &value).unwrap();
        assert_native_result_freezes(&h, &result);
        native_result_header(
            &result,
            status,
            "orderbook",
            &left,
            &right,
            0,
            if blocker_code.is_some() { 1 } else { 0 },
        );
        let expected = if symbol.is_empty() {
            "{\"request\":{\"symbol\":\"\"}}".to_owned()
        } else {
            format!(
                "{{\"symbol\":\"{symbol}\",\"bids\":[{{\"price\":11,\"amount\":13,\"total\":17}}],\"asks\":[]}}"
            )
        };
        assert_eq!(
            text(&get(&result, "legacyCanonical").unwrap()),
            expected,
            "SNAPSHOT-NATIVE legacy book canonical"
        );
        assert_eq!(
            text(&get(&result, "v3Canonical").unwrap()),
            format!(
                "{{\"symbol\":\"{other}\",\"bids\":[{{\"price\":11,\"amount\":13,\"total\":17}}],\"asks\":[]}}"
            )
        );
        if let Some(code) = blocker_code {
            let blockers = array(&get(&result, "blockers").unwrap());
            assert_eq!(text(&get(&blockers[0], "code").unwrap()), code);
            assert_eq!(
                text(&get(&blockers[0], "path").unwrap()),
                "orderbook.request.symbol"
            );
            let expected = if code == "invalid-request-context" {
                expected_blocker(
                    code,
                    "Orderbook request symbols must be non-empty",
                    "orderbook.request.symbol",
                    json!("non-empty exact WOO symbol"),
                    expected_record(vec![("legacy", json!("")), ("v3", json!("BTC"))]),
                )
            } else {
                expected_blocker(
                    code,
                    "Legacy and V3 orderbook requests target different symbols",
                    "orderbook.request.symbol",
                    json!("BTC"),
                    json!("ETH"),
                )
            };
            assert_value(&blockers[0], &expected);
        }
        if symbol == other {
            let bids = get(&left, "bids").unwrap();
            let Native::Array(rows) = bids.as_ref() else {
                panic!("array")
            };
            rows.borrow_mut()
                .push(decode(&json!({"price":23,"amount":29,"total":31})));
            assert_eq!(
                array(&get(&get(&result, "authoritative").unwrap(), "bids").unwrap()).len(),
                2,
                "SNAPSHOT-NATIVE authoritative remains live"
            );
        }
        native.push(result);
        cases.push(json!({"op":"compareWooV3OrderBook","input":input,"mutateAfter":symbol==other}));
    }
    let rows = source(&cases);
    for (value, expected) in native.iter().zip(rows.as_array().unwrap()) {
        assert_value(value, expected);
    }
}
#[test]
fn market_comparison_native_duplicate_field_and_key_order() {
    let (m, h) = snapshot_module();
    let btc = market("BTC");
    let mut duplicate = market("BTC");
    duplicate["id"] = json!("duplicate-id");
    let eth = market("ETH");
    let ltc = market("LTC");
    let mut changed = market("BTC");
    changed["precision"]["price"] = json!(0.5);
    let left = json!([btc, duplicate, eth]);
    let right = json!([changed, ltc]);
    let legacy = decode(&left);
    let v3 = decode(&right);
    let output = diagnostic::compare_markets(m, legacy.clone(), v3.clone()).unwrap();
    assert_native_result_freezes(&h, &output);
    native_result_header(&output, "mismatch", "markets", &legacy, &v3, 4, 0);
    let rows = array(&get(&output, "mismatches").unwrap());
    let lp = "GET /v1/public/info mapped perpetual rows";
    let rp = "GET /v3/public/instruments mapped perpetual rows";
    for (value, expected) in rows.iter().zip([
        expected_mismatch(
            "duplicate-key",
            "markets[BTC]",
            json!("BTC"),
            json!({"undefined":true}),
            lp,
            rp,
        ),
        expected_mismatch(
            "value",
            "markets[BTC].precision.price",
            json!(0.25),
            json!(0.5),
            "v1 quote_tick",
            "V3 quoteTick",
        ),
        expected_mismatch(
            "missing-in-v3",
            "markets[ETH]",
            json!("ETH"),
            json!({"undefined":true}),
            lp,
            rp,
        ),
        expected_mismatch(
            "unexpected-in-v3",
            "markets[LTC]",
            json!({"undefined":true}),
            json!("LTC"),
            lp,
            rp,
        ),
    ]) {
        assert_value(value, &expected);
    }
    assert_eq!(
        text(&get(&output, "legacyCanonical").unwrap()),
        format!(
            "[{},{},{}]",
            literal_market("BTC", "distinct-id", "0.25"),
            literal_market("BTC", "duplicate-id", "0.25"),
            literal_market("ETH", "distinct-id", "0.25")
        ),
        "SNAPSHOT-NATIVE duplicate retained in canonical"
    );
    assert_eq!(
        text(&get(&output, "v3Canonical").unwrap()),
        format!(
            "[{},{}]",
            literal_market("BTC", "distinct-id", "0.5"),
            literal_market("LTC", "distinct-id", "0.25")
        )
    );
    let expected = source(&[json!({"op":"compareWooV3Markets","args":[left,right]})]);
    assert_value(&output, &expected[0]);
}
#[test]
fn ticker_comparison_native_all_nine_value_fields_and_blocker() {
    let (m, h) = snapshot_module();
    let left = json!([{"id":"legacy","symbol":"BTC","index":11,"mark":13,"percentage":17,"openInterest":19,"fundingRate":23,"fundingTime":29,"volume":31,"quoteVolume":37,"last":999}]);
    let right = json!([{"id":"v3","symbol":"BTC","index":41,"mark":43,"percentage24h":47,"openInterest":53,"estimatedFundingRate":59,"fundingTime":61,"volume24h":67,"quoteVolume24h":71}]);
    let legacy = decode(&left);
    let v3 = decode(&right);
    let output = diagnostic::compare_ticker_stats(m, legacy.clone(), v3.clone()).unwrap();
    assert_native_result_freezes(&h, &output);
    native_result_header(&output, "blocked", "tickers", &legacy, &v3, 9, 1);
    let rows = array(&get(&output, "mismatches").unwrap());
    for (value, (field, a, b, lp, rp)) in rows.iter().zip([
        (
            "id",
            json!("legacy"),
            json!("v3"),
            "legacy mapped market",
            "V3 mapped market",
        ),
        (
            "index",
            json!(11),
            json!(41),
            "v1 index_price",
            "V3 indexPrice",
        ),
        (
            "mark",
            json!(13),
            json!(43),
            "v1 mark_price",
            "V3 markPrice",
        ),
        (
            "percentage",
            json!(17),
            json!(47),
            "v1 24h_open/24h_close",
            "V3 24hOpen/24hClose",
        ),
        (
            "openInterest",
            json!(19),
            json!(53),
            "v1 open_interest",
            "V3 openInterest",
        ),
        (
            "fundingRate",
            json!(23),
            json!(59),
            "legacy safe-cex fundingRate from estfundingrate stream",
            "V3 estFundingRate",
        ),
        (
            "fundingTime",
            json!(29),
            json!(61),
            "v1 next_funding_time",
            "V3 nextFundingTime",
        ),
        (
            "volume",
            json!(31),
            json!(67),
            "v1 24h_volume",
            "V3 24hVolume",
        ),
        (
            "quoteVolume",
            json!(37),
            json!(71),
            "v1 24h_amount",
            "V3 24hAmount",
        ),
    ]) {
        assert_value(
            value,
            &expected_mismatch("value", &format!("tickers[BTC].{field}"), a, b, lp, rp),
        );
    }
    let blockers = array(&get(&output, "blockers").unwrap());
    assert_value(
        &blockers[0],
        &expected_record(vec![
            ("code", json!("v3-futures-rest-has-no-live-quotes")),
            (
                "detail",
                json!(
                    "Legacy REST fills bid/ask/last from 24h_close, while V3 REST has no best-bid, best-ask, or last-trade fields; V3 ticker authority requires coherent BBO and Trade streams."
                ),
            ),
        ]),
    );
    assert_eq!(
        text(&get(&output, "legacyCanonical").unwrap()),
        "[{\"id\":\"legacy\",\"symbol\":\"BTC\",\"index\":11,\"mark\":13,\"percentage\":17,\"openInterest\":19,\"fundingRate\":23,\"fundingTime\":29,\"volume\":31,\"quoteVolume\":37}]"
    );
    assert_eq!(
        text(&get(&output, "v3Canonical").unwrap()),
        "[{\"id\":\"v3\",\"symbol\":\"BTC\",\"index\":41,\"mark\":43,\"percentage\":47,\"openInterest\":53,\"fundingRate\":59,\"fundingTime\":61,\"volume\":67,\"quoteVolume\":71}]"
    );
    let expected = source(&[json!({"op":"compareWooV3TickerStats","args":[left,right]})]);
    assert_value(&output, &expected[0]);
}
#[test]
fn candle_comparison_native_match_and_blocked_fallback() {
    let (m, h) = snapshot_module();
    let legacy = json!([{"timestamp":3,"open":11,"high":13,"low":17,"close":19,"volume":23},{"timestamp":1,"open":29,"high":31,"low":37,"close":41,"volume":43}]);
    let rows = json!([{"symbol":"BTC","type":"1m","startTimestamp":3000,"endTimestamp":63000,"open":"11","high":"13","low":"17","close":"19","volume":"23"},{"symbol":"BTC","type":"1m","startTimestamp":1000,"endTimestamp":61000,"open":"29","high":"31","low":"37","close":"41","volume":"43"}]);
    let request = json!({"symbol":"BTC","interval":"1m"});
    let input = json!({"routeId":"historical-klines","legacyRequest":request,"v3Request":request,"legacyCandles":legacy,"v3Rows":rows});
    let value = decode(&input);
    let left = get(&value, "legacyCandles").unwrap();
    let right = get(&value, "v3Rows").unwrap();
    let output = diagnostic::compare_candles(m.clone(), &value).unwrap();
    assert_native_result_freezes(&h, &output);
    native_result_header(&output, "match", "historical-klines", &left, &right, 0, 0);
    let literal = "{\"symbol\":\"BTC\",\"interval\":\"1m\",\"candles\":[{\"timestamp\":1,\"open\":29,\"high\":31,\"low\":37,\"close\":41,\"volume\":43},{\"timestamp\":3,\"open\":11,\"high\":13,\"low\":17,\"close\":19,\"volume\":23}]}";
    assert_eq!(text(&get(&output, "legacyCanonical").unwrap()), literal);
    assert_eq!(text(&get(&output, "v3Canonical").unwrap()), literal);
    assert_eq!(
        number(&get(&array(&left)[0], "timestamp").unwrap()),
        3.,
        "SNAPSHOT-NATIVE original candles remain unsorted"
    );
    let mut blocked = input.clone();
    blocked["v3Request"]["symbol"] = json!("ETH");
    blocked["v3Rows"] = json!([{"symbol":"OTHER","type":"1m","startTimestamp":1000,"endTimestamp":61000,"open":"29","high":"31","low":"37","close":"47","volume":"43"}]);
    let value = decode(&blocked);
    let left = get(&value, "legacyCandles").unwrap();
    let right = get(&value, "v3Rows").unwrap();
    let rejected = diagnostic::compare_candles(m, &value).unwrap();
    assert_native_result_freezes(&h, &rejected);
    native_result_header(
        &rejected,
        "blocked",
        "historical-klines",
        &left,
        &right,
        2,
        2,
    );
    assert_eq!(text(&get(&rejected, "legacyCanonical").unwrap()), literal);
    assert_eq!(
        text(&get(&rejected, "v3Canonical").unwrap()),
        "{\"request\":{\"symbol\":\"ETH\",\"interval\":\"1m\"},\"rows\":[{\"symbol\":\"OTHER\",\"type\":\"1m\",\"startTimestamp\":1000,\"endTimestamp\":61000}]}",
        "SNAPSHOT-NATIVE blocked row provenance fallback"
    );
    let differences = array(&get(&rejected, "mismatches").unwrap());
    assert_value(
        &differences[1],
        &expected_mismatch(
            "value",
            "candles[1].close",
            json!(41),
            json!(47),
            "v1 close",
            "V3 close",
        ),
    );
    assert_value(
        &differences[0],
        &expected_mismatch(
            "missing-in-v3",
            "candles[3]",
            json!("3"),
            json!({"undefined":true}),
            "GET legacy historical kline rows",
            "GET /v3/public/klineHistory rows",
        ),
    );
    let blockers = array(&get(&rejected, "blockers").unwrap());
    assert_value(
        &blockers[0],
        &expected_blocker(
            "request-symbol-mismatch",
            "Legacy and V3 kline requests target different symbols",
            "kline.request.symbol",
            json!("BTC"),
            json!("ETH"),
        ),
    );
    assert_value(
        &blockers[1],
        &expected_blocker(
            "v3-kline-row-symbol-mismatch",
            "V3 kline row does not match its request symbol",
            "kline.v3Rows[0].symbol",
            json!("ETH"),
            json!("OTHER"),
        ),
    );
    assert_eq!(
        text(&get(&blockers[0], "code").unwrap()),
        "request-symbol-mismatch"
    );
    assert_eq!(
        text(&get(&blockers[1], "code").unwrap()),
        "v3-kline-row-symbol-mismatch"
    );
    let expected = source(&[
        json!({"op":"compareWooV3Candles","input":input}),
        json!({"op":"compareWooV3Candles","input":blocked}),
    ]);
    assert_value(&output, &expected[0]);
    assert_value(&rejected, &expected[1]);
}
fn expected_blocker(code: &str, detail: &str, path: &str, expected: Value, actual: Value) -> Value {
    expected_record(vec![
        ("code", json!(code)),
        ("detail", json!(detail)),
        ("path", json!(path)),
        ("expected", expected),
        ("actual", actual),
    ])
}
fn expected_ready(interval: &str, duration: f64) -> Value {
    expected_record(vec![
        ("status", json!("ready")),
        ("interval", json!(interval)),
        ("durationMs", json!(duration)),
    ])
}
fn expected_calendar(interval: &str) -> Value {
    expected_record(vec![
        ("status", json!("blocked")),
        ("normalized", json!(interval)),
        ("reason", json!("variable-calendar-duration")),
    ])
}
fn expected_validation_case(index: usize) -> Value {
    let intervals = json!([
        "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "12h", "1d", "1w"
    ]);
    let end = |expected, actual| {
        expected_blocker(
            "v3-kline-row-end-timestamp-mismatch",
            "V3 kline endTimestamp must equal startTimestamp plus the fixed requested interval",
            "kline.v3Rows[0].endTimestamp",
            expected,
            actual,
        )
    };
    let (blockers, legacy, v3) = match index {
        0 => (
            vec![],
            expected_ready("1m", 60000.),
            expected_ready("1m", 60000.),
        ),
        1 => (
            vec![
                expected_blocker(
                    "invalid-request-context",
                    "Kline request symbols must be non-empty",
                    "kline.request.symbol",
                    json!("non-empty exact WOO symbol"),
                    expected_record(vec![("legacy", json!("")), ("v3", json!(""))]),
                ),
                expected_blocker(
                    "unsupported-kline-interval",
                    "WOO 1mon uses variable calendar boundaries; fixed endTimestamp parity is intentionally blocked",
                    "kline.request.legacy.interval",
                    intervals,
                    json!("1M"),
                ),
                expected_blocker(
                    "v3-kline-row-symbol-mismatch",
                    "V3 kline row does not match its request symbol",
                    "kline.v3Rows[0].symbol",
                    json!(""),
                    json!("OTHER"),
                ),
                expected_blocker(
                    "v3-kline-row-interval-mismatch",
                    "V3 kline row does not match its normalized request interval",
                    "kline.v3Rows[0].type",
                    json!("5m"),
                    json!("1m"),
                ),
                end(json!(300000), json!(1)),
            ],
            expected_calendar("1mon"),
            expected_ready("5m", 300000.),
        ),
        2 => (
            vec![
                expected_blocker(
                    "request-symbol-mismatch",
                    "Legacy and V3 kline requests target different symbols",
                    "kline.request.symbol",
                    json!("BTC"),
                    json!("ETH"),
                ),
                expected_blocker(
                    "request-interval-mismatch",
                    "Legacy and V3 kline requests target different intervals",
                    "kline.request.interval",
                    json!("1m"),
                    json!("3m"),
                ),
            ],
            expected_ready("1m", 60000.),
            expected_ready("3m", 180000.),
        ),
        3 => (
            vec![expected_blocker(
                "unsupported-kline-interval",
                "WOO 1y uses variable calendar boundaries; fixed endTimestamp parity is intentionally blocked",
                "kline.request.v3.interval",
                intervals,
                json!("1y"),
            )],
            expected_ready("1m", 60000.),
            expected_calendar("1y"),
        ),
        4 => (
            vec![end(json!({"number":"NaN"}), json!(0))],
            expected_ready("1m", 60000.),
            expected_ready("1m", 60000.),
        ),
        5 => (
            vec![end(json!("100060000"), json!(61000))],
            expected_ready("1m", 60000.),
            expected_ready("1m", 60000.),
        ),
        _ => panic!("independent validation fixture index"),
    };
    expected_record(vec![
        ("blockers", json!(blockers)),
        ("legacyInterval", legacy),
        ("v3Interval", v3),
    ])
}
