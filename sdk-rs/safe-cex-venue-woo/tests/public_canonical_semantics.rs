use safe_cex_venue_woo::public_canonical::{
    self as port, CanonicalCallback, CanonicalModule, CanonicalOps,
};
use serde_json::Value;
use std::fmt;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Clone)]
struct Callback(CanonicalCallback<V, Rc<()>>);
impl fmt::Debug for Callback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Callback")
    }
}
#[derive(Debug, Clone)]
enum Js {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    Big,
    Function(Callback),
    Builtin(&'static str, u8),
    O(Vec<(String, V)>),
    A(Vec<V>),
}
type V = Rc<RefCell<Js>>;
fn v(x: Js) -> V {
    Rc::new(RefCell::new(x))
}
fn text(s: &str) -> V {
    v(Js::S(s.encode_utf16().collect()))
}
fn decode(x: &Value) -> V {
    v(match x {
        Value::Null => Js::Null,
        Value::Bool(b) => Js::B(*b),
        Value::Number(n) => Js::N(n.as_f64().unwrap()),
        Value::String(s) => Js::S(s.encode_utf16().collect()),
        Value::Array(a) => Js::A(a.iter().map(decode).collect()),
        Value::Object(o) => Js::O(o.iter().map(|(k, x)| (k.clone(), decode(x))).collect()),
    })
}
fn field(o: &V, k: &str) -> V {
    let Js::O(fields) = &*o.borrow() else {
        panic!("native object field")
    };
    fields
        .iter()
        .find(|(s, _)| s == k)
        .map(|(_, x)| x.clone())
        .unwrap_or_else(|| v(Js::U))
}
fn keys(o: &V) -> Vec<String> {
    let Js::O(fields) = &*o.borrow() else {
        panic!("native object keys")
    };
    fields.iter().map(|(k, _)| k.clone()).collect()
}
fn units(x: &V) -> Vec<u16> {
    let Js::S(s) = &*x.borrow() else {
        panic!("native string")
    };
    s.clone()
}
fn number(x: &V) -> f64 {
    let Js::N(n) = *x.borrow() else {
        panic!("native number")
    };
    n
}
fn quote(s: &[u16]) -> String {
    let mut out = String::from("\"");
    for c in char::decode_utf16(s.iter().copied()) {
        match c {
            Ok(c) => {
                let part = serde_json::to_string(&c.to_string()).unwrap();
                out.push_str(&part[1..part.len() - 1]);
            }
            Err(e) => out.push_str(&format!("\\u{:04x}", e.unpaired_surrogate())),
        }
    }
    out.push('"');
    out
}
fn stringify(x: &V) -> Result<Option<String>, ()> {
    Ok(match &*x.borrow() {
        Js::U | Js::Function(_) | Js::Builtin(_, _) => None,
        Js::Null => Some("null".into()),
        Js::B(b) => Some(b.to_string()),
        Js::N(n) => Some(if n.is_finite() {
            safe_cex_num::js_number_string(*n)
        } else {
            "null".into()
        }),
        Js::S(s) => Some(quote(s)),
        Js::Big => return Err(()),
        Js::A(a) => {
            let mut parts = vec![];
            for x in a {
                parts.push(stringify(x)?.unwrap_or("null".into()));
            }
            Some(format!("[{}]", parts.join(",")))
        }
        Js::O(o) => {
            let mut parts = vec![];
            for (k, x) in o {
                if let Some(s) = stringify(x)? {
                    parts.push(format!(
                        "{}:{s}",
                        quote(&k.encode_utf16().collect::<Vec<_>>())
                    ));
                }
            }
            Some(format!("{{{}}}", parts.join(",")))
        }
    })
}
#[derive(Default, Clone)]
struct Host {
    trace: Rc<RefCell<Vec<String>>>,
    prepared: Rc<RefCell<Option<V>>>,
    error: Rc<()>,
    mode: Rc<Cell<u8>>,
    dynamic: Rc<RefCell<Option<V>>>,
    reads: Rc<Cell<usize>>,
    map_receiver: Rc<RefCell<Option<V>>>,
    member_version: Rc<Cell<u8>>,
    retained_compare: Rc<RefCell<Option<V>>>,
    retained_project: Rc<RefCell<Option<V>>>,
}
impl Host {
    fn get(&self, x: &V, key: &str) -> Result<V, Rc<()>> {
        match (&*x.borrow(), key) {
            (Js::Builtin("JSON", _), "stringify") => return Ok(v(Js::Builtin("stringify", 0))),
            (Js::Builtin("Object", _), "freeze") => return Ok(v(Js::Builtin("freeze", 0))),
            (Js::A(_), "sort") => return Ok(v(Js::Builtin("sort", 0))),
            (Js::A(_), "map") => return Ok(v(Js::Builtin("map", 0))),
            (_, "map") if self.mode.get() == 3 => return Ok(v(Js::Builtin("map", 0))),
            (_, "localeCompare") => {
                let (_, version) = self.prepare_locale_compare(x.clone())?;
                return Ok(v(Js::Builtin("localeCompare", version)));
            }
            _ => {}
        }
        self.trace.borrow_mut().push(key.into());
        if self.mode.get() == 1 && key == "quote" {
            return Err(self.error.clone());
        }
        if self.mode.get() == 2 && key == "symbol" {
            let side = field(x, "side");
            if number(&side) == 0. {
                self.trace.borrow_mut().pop();
                self.trace.borrow_mut().push("left-symbol".into());
                return Ok(x.clone());
            }
            self.trace.borrow_mut().pop();
            self.trace.borrow_mut().push("right-symbol".into());
            self.member_version.set(1);
            return Ok(text("R"));
        }
        if key == "precision"
            && self
                .dynamic
                .borrow()
                .as_ref()
                .is_some_and(|d| Rc::ptr_eq(d, x))
        {
            let n = self.reads.get() + 1;
            self.reads.set(n);
            return Ok(self.object(vec![
                ("amount", v(Js::N(if n == 1 { 41. } else { 99. }))),
                ("price", v(Js::N(if n == 2 { 42. } else { 99. }))),
            ]));
        }
        if !matches!(*x.borrow(), Js::O(_)) {
            return Err(self.error.clone());
        }
        Ok(field(x, key))
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        v(Js::O(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }
    fn json_stringify(&self, x: V) -> Result<V, Rc<()>> {
        *self.prepared.borrow_mut() = Some(x.clone());
        match stringify(&x) {
            Ok(Some(s)) => Ok(text(&s)),
            Ok(None) => Ok(v(Js::U)),
            Err(()) => Err(self.error.clone()),
        }
    }
    fn spread(&self, x: V) -> Result<V, Rc<()>> {
        let Js::A(a) = &*x.borrow() else {
            return Err(self.error.clone());
        };
        Ok(v(Js::A(a.clone())))
    }
    fn sort(&self, x: V, cmp: V) -> Result<V, Rc<()>> {
        let Js::A(mut rows) = x.borrow().clone() else {
            return Err(self.error.clone());
        };
        if self.mode.get() == 2 {
            let n = self.invoke(
                cmp.clone(),
                v(Js::U),
                vec![rows[0].clone(), rows[1].clone()],
            )?;
            assert_eq!(number(&n), 7., "native captured localeCompare member");
            return Err(self.error.clone());
        }
        if self.mode.get() == 3 {
            let result = self.object(vec![("marker", v(Js::N(99.)))]);
            *self.map_receiver.borrow_mut() = Some(result.clone());
            return Ok(result);
        }
        if self.mode.get() == 4 {
            *self.retained_compare.borrow_mut() = Some(cmp);
            return Ok(x);
        }
        for i in 1..rows.len() {
            let mut j = i;
            while j > 0 {
                let n = self.invoke(
                    cmp.clone(),
                    v(Js::U),
                    vec![rows[j].clone(), rows[j - 1].clone()],
                )?;
                if number(&n) >= 0. {
                    break;
                }
                rows.swap(j, j - 1);
                j -= 1;
            }
        }
        *x.borrow_mut() = Js::A(rows);
        Ok(x)
    }
    fn map(&self, x: V, project: V) -> Result<V, Rc<()>> {
        if self.mode.get() == 4 {
            *self.retained_project.borrow_mut() = Some(project);
            return Ok(v(Js::A(vec![])));
        }
        let rows = if self.mode.get() == 3 {
            assert!(
                Rc::ptr_eq(&x, self.map_receiver.borrow().as_ref().unwrap()),
                "native map receiver is actual sort return"
            );
            vec![self.dynamic.borrow().as_ref().unwrap().clone()]
        } else {
            let Js::A(a) = &*x.borrow() else {
                return Err(self.error.clone());
            };
            a.clone()
        };
        let mut out = vec![];
        for row in rows {
            out.push(self.invoke(project.clone(), v(Js::U), vec![row])?);
        }
        Ok(v(Js::A(out)))
    }
    fn prepare_locale_compare(&self, x: V) -> Result<(V, u8), Rc<()>> {
        if self.mode.get() == 2 {
            self.trace.borrow_mut().push("prepare".into());
        }
        Ok((x, self.member_version.get()))
    }
    fn invoke_locale_compare(&self, p: (V, u8), arg: V) -> Result<V, Rc<()>> {
        if self.mode.get() == 2 {
            self.trace.borrow_mut().push("invoke-old:R".into());
            return Ok(v(Js::N(if p.1 == 0 { 7. } else { 99. })));
        }
        let a = String::from_utf16(&units(&p.0)).unwrap();
        let b = String::from_utf16(&units(&arg)).unwrap();
        let c = safe_cex_num::JsRootCollator::try_new()
            .unwrap()
            .compare(&a, &b);
        Ok(v(Js::N(match c {
            std::cmp::Ordering::Less => -1.,
            std::cmp::Ordering::Equal => 0.,
            std::cmp::Ordering::Greater => 1.,
        })))
    }
}
impl Host {
    fn invoke(&self, f: V, receiver: V, args: Vec<V>) -> Result<V, Rc<()>> {
        match &*f.borrow() {
            Js::Function(callback) => (callback.0)(args),
            Js::Builtin("stringify", _) => self.json_stringify(args[0].clone()),
            Js::Builtin("freeze", _) => Ok(args[0].clone()),
            Js::Builtin("sort", _) => self.sort(receiver, args[0].clone()),
            Js::Builtin("map", _) => self.map(receiver, args[0].clone()),
            Js::Builtin("localeCompare", version) => {
                self.invoke_locale_compare((receiver, *version), args[0].clone())
            }
            _ => Err(self.error.clone()),
        }
    }
    fn ops(&self) -> Rc<CanonicalOps<V, Rc<()>>> {
        let properties = self.clone();
        let objects = self.clone();
        let calls = self.clone();
        let iterator = self.clone();
        let error = self.error.clone();
        Rc::new(CanonicalOps {
            undefined: Rc::new(|| v(Js::U)),
            object: Rc::new(move |fields| objects.object(fields)),
            array: Rc::new(|rows| v(Js::A(rows))),
            get: Rc::new(move |value, key| properties.get(value, key)),
            global: Rc::new(|name| {
                Ok(v(Js::Builtin(
                    match name {
                        "JSON" => "JSON",
                        "Object" => "Object",
                        _ => panic!("fixture global {name}"),
                    },
                    0,
                )))
            }),
            invoke: Rc::new(move |f, receiver, args| calls.invoke(f, receiver, args)),
            function: Rc::new(|_, _, callback| v(Js::Function(Callback(callback)))),
            spread: Rc::new(move |value| {
                let copy = iterator.spread(value.clone())?;
                let Js::A(rows) = &*copy.borrow() else {
                    panic!("fixture spread result")
                };
                Ok(rows.clone())
            }),
            truthy: Rc::new(|value| match &*value.borrow() {
                Js::U | Js::Null => false,
                Js::B(b) => *b,
                Js::N(n) => *n != 0. && !n.is_nan(),
                Js::S(s) => !s.is_empty(),
                _ => true,
            }),
            strict_equal: Rc::new(|a, b| match (&*a.borrow(), &*b.borrow()) {
                (Js::S(a), Js::S(b)) => a == b,
                (Js::N(a), Js::N(b)) => a == b,
                _ => Rc::ptr_eq(a, b),
            }),
            string: Rc::new(text),
            number: Rc::new(|n| v(Js::N(n))),
            subtract: Rc::new(|a, b| Ok(v(Js::N(number(&a) - number(&b))))),
            get_key: Rc::new(|value, key| {
                Ok(field(value, &String::from_utf16(&units(key)).unwrap()))
            }),
            error: Rc::new(move |_| error.clone()),
        })
    }
}
fn canonical_string(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonical_string(&h.ops(), input)
}
fn canonical_market(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonical_market(&h.ops(), &input)
}
fn canonical_legacy_ticker_stats(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonical_legacy_ticker_stats(&h.ops(), &input)
}
fn canonical_v3_ticker_stats(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonical_v3_ticker_stats(&h.ops(), &input)
}
fn canonical_order_book_level(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonical_order_book_level(&h.ops(), &input)
}
fn canonical_candle(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonical_candle(&h.ops(), &input)
}
fn canonicalize_woo_v3_markets(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonicalize_markets(&CanonicalModule::new(h.ops())?, input)
}
fn canonicalize_woo_legacy_ticker_stats(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonicalize_legacy_tickers(&CanonicalModule::new(h.ops())?, input)
}
fn canonicalize_woo_v3_ticker_stats(h: &Host, input: V) -> Result<V, Rc<()>> {
    port::canonicalize_v3_tickers(&CanonicalModule::new(h.ops())?, input)
}
fn source() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-canonical-semantics.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn project(h: &Host, id: &str, input: V) -> V {
    match id {
        "market" => canonical_market(h, input),
        "legacy" => canonical_legacy_ticker_stats(h, input),
        "stats" => canonical_v3_ticker_stats(h, input),
        "level" => canonical_order_book_level(h, input),
        "candle" => canonical_candle(h, input),
        _ => panic!(),
    }
    .unwrap()
}
fn numeric_fields(out: &V, names: &[&str], start: f64) {
    for (i, k) in names.iter().enumerate() {
        assert_eq!(
            number(&field(out, k)).to_bits(),
            (start + i as f64).to_bits(),
            "native projected field {k}"
        );
    }
}
#[test]
fn decoded_native_projector_fields_and_getter_order() {
    let src = source();
    let rows = src["projections"].as_array().unwrap();
    assert_eq!(rows.len(), 5, "independent projector floor");
    let mut wires = vec![];
    for (i, row) in rows.iter().enumerate() {
        let h = Host::default();
        let input = decode(&row["input"]);
        let out = project(&h, row["id"].as_str().unwrap(), input.clone());
        let expected: Vec<&str> = match i {
            0 => vec![
                "id",
                "symbol",
                "base",
                "quote",
                "active",
                "precision",
                "limits",
            ],
            1 | 2 => vec![
                "id",
                "symbol",
                "index",
                "mark",
                "percentage",
                "openInterest",
                "fundingRate",
                "fundingTime",
                "volume",
                "quoteVolume",
            ],
            3 => vec!["price", "amount", "total"],
            4 => vec!["timestamp", "open", "high", "low", "close", "volume"],
            _ => panic!(),
        };
        assert_eq!(keys(&out), expected, "native projection ordered own keys");
        if i < 3 {
            assert_eq!(
                units(&field(&out, "id")),
                "id".encode_utf16().collect::<Vec<_>>(),
                "native projected id"
            );
            assert_eq!(
                units(&field(&out, "symbol")),
                "OTHER".encode_utf16().collect::<Vec<_>>(),
                "native projected symbol"
            );
        }
        match i {
            0 => {
                assert_eq!(
                    units(&field(&out, "base")),
                    [u16::from(b'B')],
                    "native projected base"
                );
                assert_eq!(
                    units(&field(&out, "quote")),
                    [u16::from(b'Q')],
                    "native projected quote"
                );
                assert!(
                    matches!(*field(&out, "active").borrow(), Js::B(true)),
                    "native projected active"
                );
                numeric_fields(&field(&out, "precision"), &["amount", "price"], 11.);
                numeric_fields(
                    &field(&field(&out, "limits"), "amount"),
                    &["min", "max"],
                    13.,
                );
                numeric_fields(
                    &field(&field(&out, "limits"), "leverage"),
                    &["min", "max"],
                    15.,
                );
                assert_eq!(
                    h.trace.borrow().len(),
                    21,
                    "independent market access floor"
                );
            }
            1 | 2 => {
                numeric_fields(
                    &out,
                    &[
                        "index",
                        "mark",
                        "percentage",
                        "openInterest",
                        "fundingRate",
                        "fundingTime",
                        "volume",
                        "quoteVolume",
                    ],
                    1.,
                );
                assert!(
                    Rc::ptr_eq(&field(&out, "id"), &field(&input, "id")),
                    "native retained field handle"
                );
            }
            3 => numeric_fields(&out, &["price", "amount", "total"], 21.),
            4 => numeric_fields(
                &out,
                &["timestamp", "open", "high", "low", "close", "volume"],
                31.,
            ),
            _ => panic!(),
        }
        let expected_trace: Vec<String> = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        assert_eq!(
            *h.trace.borrow(),
            expected_trace,
            "native projection access order"
        );
        wires.push((out, row["wire"].as_str().unwrap().to_owned()));
    }
    // All decoded/prepared native field assertions above precede output-wire comparisons.
    for (out, expected) in wires {
        assert_eq!(
            stringify(&out).unwrap().unwrap(),
            expected,
            "secondary projection wire"
        );
    }
}
#[test]
fn native_alias_utf16_repeated_reads_and_original_errors() {
    let src = source();
    let h = Host::default();
    let input = decode(&src["projections"][1]["input"]);
    let alias = h.object(vec![("live", v(Js::N(1.)))]);
    {
        let Js::O(fields) = &mut *input.borrow_mut() else {
            panic!()
        };
        fields.iter_mut().find(|(k, _)| k == "id").unwrap().1 = v(Js::S(vec![0xd800]));
        fields.iter_mut().find(|(k, _)| k == "index").unwrap().1 = alias.clone();
    }
    let out = canonical_legacy_ticker_stats(&h, input).unwrap();
    assert_eq!(
        units(&field(&out, "id")),
        [0xd800],
        "native unpaired UTF16 survives projection"
    );
    assert!(
        Rc::ptr_eq(&field(&out, "index"), &alias),
        "native live compound alias"
    );
    *field(&alias, "live").borrow_mut() = Js::N(9.);
    assert_eq!(number(&field(&field(&out, "index"), "live")), 9.);
    assert_eq!(src["alias"]["live"], 9);
    let h = Host::default();
    let input = decode(&src["projections"][0]["input"]);
    *h.dynamic.borrow_mut() = Some(input.clone());
    let out = canonical_market(&h, input).unwrap();
    assert_eq!(h.reads.get(), 2, "native repeated precision getter");
    assert_eq!(number(&field(&field(&out, "precision"), "amount")), 41.);
    assert_eq!(number(&field(&field(&out, "precision"), "price")), 42.);
    assert_eq!(src["dynamic"]["reads"], 2);
    let h = Host::default();
    h.mode.set(1);
    let err = canonical_market(&h, decode(&src["projections"][0]["input"])).unwrap_err();
    assert!(Rc::ptr_eq(&err, &h.error), "native original getter error");
    assert_eq!(
        *h.trace.borrow(),
        ["id", "symbol", "base", "quote"],
        "native exception stops later fields"
    );
    assert_eq!(src["failure"]["identity"], true);
}
#[test]
fn native_list_stable_order_copy_and_prepared_fields() {
    let src = source();
    assert_eq!(
        src["lists"].as_array().unwrap().len(),
        3,
        "independent list floor"
    );
    let mut wires = vec![];
    for row in src["lists"].as_array().unwrap() {
        let h = Host::default();
        let input = decode(&row["input"]);
        let original = if let Js::A(a) = &*input.borrow() {
            a.clone()
        } else {
            panic!()
        };
        let result = match row["id"].as_str().unwrap() {
            "markets" => canonicalize_woo_v3_markets(&h, input.clone()),
            "legacy" => canonicalize_woo_legacy_ticker_stats(&h, input.clone()),
            "stats" => canonicalize_woo_v3_ticker_stats(&h, input.clone()),
            _ => panic!(),
        }
        .unwrap();
        let prepared = h.prepared.borrow().as_ref().unwrap().clone();
        let Js::A(out) = &*prepared.borrow() else {
            panic!()
        };
        assert_eq!(out.len(), 3, "independent prepared row count");
        let ids: Vec<Vec<u16>> = out.iter().map(|o| units(&field(o, "id"))).collect();
        assert_eq!(
            ids,
            ["number", "first", "last"].map(|s| s.encode_utf16().collect::<Vec<_>>()),
            "native sorted stable IDs"
        );
        let Js::A(a) = &*input.borrow() else { panic!() };
        assert_eq!(a.len(), 3, "independent unchanged input length");
        assert_eq!(original.len(), 3, "independent original input length");
        for (x, y) in a.iter().zip(&original) {
            assert!(Rc::ptr_eq(x, y), "native input array remains unchanged");
        }
        if row["id"] == "stats" {
            numeric_fields(
                &out[0],
                &[
                    "index",
                    "mark",
                    "percentage",
                    "openInterest",
                    "fundingRate",
                    "fundingTime",
                    "volume",
                    "quoteVolume",
                ],
                1.,
            );
        }
        wires.push((result, row["wire"].as_str().unwrap().to_owned()));
    }
    for (out, wire) in wires {
        assert_eq!(
            String::from_utf16(&units(&out)).unwrap(),
            wire,
            "secondary list canonical bytes"
        );
    }
}
#[test]
fn native_stringify_undefined_numbers_utf16_and_error() {
    let src = source();
    let h = Host::default();
    let undef = canonical_string(&h, v(Js::U)).unwrap();
    assert!(
        matches!(*undef.borrow(), Js::U),
        "native stringify undefined result"
    );
    let nan = canonical_string(&h, v(Js::N(f64::NAN))).unwrap();
    assert_eq!(units(&nan), "null".encode_utf16().collect::<Vec<_>>());
    let zero = canonical_string(&h, v(Js::N(-0.))).unwrap();
    assert_eq!(units(&zero), [u16::from(b'0')]);
    let lone = canonical_string(&h, v(Js::S(vec![0xd800]))).unwrap();
    assert_eq!(
        units(&lone),
        "\"\\ud800\"".encode_utf16().collect::<Vec<_>>(),
        "native stringify escaped lone unit"
    );
    let err = canonical_string(&h, v(Js::Big)).unwrap_err();
    assert!(
        Rc::ptr_eq(&err, &h.error),
        "native stringify original error"
    );
    assert_eq!(src["strings"]["undefined"], true);
    assert_eq!(src["strings"]["bigError"], true);
}
#[test]
fn native_current_comparator_member_and_actual_sort_return() {
    let src = source();
    let h = Host::default();
    h.mode.set(2);
    let rows = v(Js::A(vec![
        h.object(vec![("side", v(Js::N(0.)))]),
        h.object(vec![("side", v(Js::N(1.)))]),
    ]));
    let err = canonicalize_woo_v3_markets(&h, rows).unwrap_err();
    assert!(Rc::ptr_eq(&err, &h.error), "native sort error identity");
    assert_eq!(
        *h.trace.borrow(),
        ["left-symbol", "prepare", "right-symbol", "invoke-old:R"],
        "native comparator member captured before rhs"
    );
    assert!(
        h.prepared.borrow().is_none(),
        "native stops before map/stringify"
    );
    assert_eq!(src["lookup"]["value"], 7);
    assert_eq!(src["lookup"]["identity"], true);
    let h = Host::default();
    h.mode.set(3);
    let row = decode(&src["projections"][1]["input"]);
    *h.dynamic.borrow_mut() = Some(row.clone());
    let result = canonicalize_woo_legacy_ticker_stats(&h, v(Js::A(vec![row]))).unwrap();
    let native = h.prepared.borrow().as_ref().unwrap().clone();
    let Js::A(rows) = &*native.borrow() else {
        panic!()
    };
    assert_eq!(rows.len(), 1, "native custom map row count");
    numeric_fields(
        &rows[0],
        &[
            "index",
            "mark",
            "percentage",
            "openInterest",
            "fundingRate",
            "fundingTime",
            "volume",
            "quoteVolume",
        ],
        1.,
    );
    assert_eq!(
        String::from_utf16(&units(&result)).unwrap(),
        src["custom"]["wire"],
        "secondary custom sort-return wire"
    );
}

#[test]
fn native_callbacks_can_outlive_canonicalization() {
    let src = source();
    let h = Host::default();
    h.mode.set(4);
    let input = decode(&src["projections"][0]["input"]);
    let out = canonicalize_woo_v3_markets(&h, v(Js::A(vec![input.clone()]))).unwrap();
    assert_eq!(
        units(&out),
        [u16::from(b'['), u16::from(b']')],
        "native custom empty map result"
    );
    let compare = h.retained_compare.borrow().as_ref().unwrap().clone();
    let result = h
        .invoke(
            compare,
            v(Js::U),
            vec![
                h.object(vec![("symbol", text("OTHER"))]),
                h.object(vec![("symbol", text("42"))]),
            ],
        )
        .unwrap();
    assert_eq!(
        number(&result),
        1.,
        "native retained comparator after return"
    );
    let project = h.retained_project.borrow().as_ref().unwrap().clone();
    let output = h.invoke(project, v(Js::U), vec![input]).unwrap();
    assert_eq!(
        units(&field(&output, "symbol")),
        "OTHER".encode_utf16().collect::<Vec<_>>(),
        "native retained projector after return"
    );
    numeric_fields(&field(&output, "precision"), &["amount", "price"], 11.);
    assert_eq!(src["retained"]["compare"], 1);
    assert_eq!(
        stringify(&output).unwrap().unwrap(),
        src["retained"]["wire"],
        "secondary retained projector wire"
    );
}
