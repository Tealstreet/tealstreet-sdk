//! The JavaScript builtins the ported bodies call, installed on the intrinsic
//! prototypes and the global object. Each follows its ECMAScript algorithm for
//! the argument shapes the source uses.
use super::{
    Realm,
    convert::Hint,
    promise::settle,
    regex::Pattern,
    value::{
        FunctionData, IterSource, Kind, NativeConstruct, NativeFn, Obj, Prop, PropKey, Slot,
        Thrown, Value,
    },
};
use crate::spot_json::JsString;
use std::{cell::RefCell, rc::Rc};

pub(crate) fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}
fn units(text: &str) -> Rc<[u16]> {
    text.encode_utf16().collect::<Vec<_>>().into()
}
type Body = fn(&mut Realm, &Value, &[Value]) -> Result<Value, Thrown>;

fn def(realm: &Realm, target: &Obj, name: &str, length: u8, body: Body) {
    let f = realm.function(name, length, Rc::new(body));
    target.borrow_mut().props.push(Prop {
        key: PropKey::str(name),
        slot: Slot::Data(f),
        enumerable: false,
        writable: true,
        configurable: true,
    });
}
fn def_value(target: &Obj, name: &str, value: Value) {
    target.borrow_mut().props.push(Prop {
        key: PropKey::str(name),
        slot: Slot::Data(value),
        enumerable: false,
        writable: false,
        configurable: false,
    });
}
fn def_getter(realm: &Realm, target: &Obj, name: &str, body: Body) {
    let f = realm.function(&format!("get {name}"), 0, Rc::new(body));
    target.borrow_mut().props.push(Prop {
        key: PropKey::str(name),
        slot: Slot::Accessor {
            get: Some(f),
            set: None,
        },
        enumerable: false,
        writable: false,
        configurable: true,
    });
}
fn def_iterator(realm: &Realm, target: &Obj, body: Body) {
    let f = realm.function("[Symbol.iterator]", 0, Rc::new(body));
    let key = PropKey::Symbol(realm.intrinsics().symbol_iterator.clone());
    target.borrow_mut().props.push(Prop {
        key,
        slot: Slot::Data(f),
        enumerable: false,
        writable: true,
        configurable: true,
    });
}
/// A constructor function whose `prototype` is `proto`.
fn constructor(
    realm: &Realm,
    name: &str,
    length: u8,
    proto: &Obj,
    call: Option<Body>,
    construct: Option<fn(&mut Realm, &[Value], &Value) -> Result<Value, Thrown>>,
) -> Value {
    let f = realm.function_data(FunctionData {
        name: JsString(name.encode_utf16().collect()),
        length,
        call: call.map(|c| Rc::new(c) as NativeFn),
        construct: construct.map(|c| Rc::new(c) as NativeConstruct),
    });
    def_value(
        f.object().unwrap(),
        "prototype",
        Value::Object(proto.clone()),
    );
    proto
        .borrow_mut()
        .put(PropKey::str("constructor"), f.clone(), false);
    f
}

fn array_items(realm: &mut Realm, this: &Value) -> Result<Vec<Value>, Thrown> {
    match this
        .object()
        .map(|o| o.borrow())
        .as_deref()
        .map(|d| match &d.kind {
            Kind::Array(items) => Some(items.clone()),
            _ => None,
        }) {
        Some(Some(items)) => Ok(items),
        _ => {
            if this.is_nullish() {
                return Err(realm.type_error("Array.prototype method called on null or undefined"));
            }
            let len = realm.get_str(this, "length")?;
            let len = realm.to_number(&len)?;
            let len = if len.is_finite() && len > 0.0 {
                len as usize
            } else {
                0
            };
            (0..len)
                .map(|i| realm.get(this, &PropKey::str(&i.to_string())))
                .collect()
        }
    }
}
fn with_array<R>(this: &Value, f: impl FnOnce(&mut Vec<Value>) -> R) -> Option<R> {
    let o = this.object()?;
    let mut data = o.borrow_mut();
    match &mut data.kind {
        Kind::Array(items) => Some(f(items)),
        _ => None,
    }
}
fn callable(realm: &Realm, f: &Value) -> Result<(), Thrown> {
    if f.is_callable() {
        Ok(())
    } else {
        Err(realm.type_error(&format!("{f:?} is not a function")))
    }
}
fn relative_index(
    realm: &mut Realm,
    v: &Value,
    len: usize,
    default: usize,
) -> Result<usize, Thrown> {
    if matches!(v, Value::Undefined) {
        return Ok(default);
    }
    let n = realm.to_number(v)?;
    let n = if n.is_nan() { 0.0 } else { n.trunc() };
    Ok(if n < 0.0 {
        (len as f64 + n).max(0.0) as usize
    } else {
        n.min(len as f64) as usize
    })
}
fn this_units(realm: &mut Realm, this: &Value) -> Result<Rc<[u16]>, Thrown> {
    if this.is_nullish() {
        return Err(realm.type_error("String.prototype method called on null or undefined"));
    }
    realm.to_string(this)
}
fn find_units(hay: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(hay.len()));
    }
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| hay[i..].starts_with(needle))
}
fn map_cell(this: &Value) -> Option<Obj> {
    this.object()
        .filter(|o| matches!(o.borrow().kind, Kind::Map(_)))
        .cloned()
}
fn set_cell(this: &Value) -> Option<Obj> {
    this.object()
        .filter(|o| matches!(o.borrow().kind, Kind::Set(_)))
        .cloned()
}
fn normalize_key(v: &Value) -> Value {
    match v {
        Value::Number(n) if *n == 0.0 => Value::Number(0.0),
        other => other.clone(),
    }
}

pub(super) fn install(realm: &mut Realm) {
    install_object(realm);
    install_function(realm);
    install_array(realm);
    install_string(realm);
    install_number(realm);
    install_collections(realm);
    install_promise(realm);
    install_errors(realm);
    install_misc(realm);
}

fn install_object(realm: &mut Realm) {
    let p = realm.intrinsics().object_proto.clone();
    def(realm, &p, "hasOwnProperty", 1, |r, this, args| {
        let key = r.to_property_key(&arg(args, 0))?;
        Ok(Value::Bool(r.has_own(this, &key)))
    });
    def(realm, &p, "toString", 0, |r, this, _| {
        Ok(Value::str(match this {
            Value::Undefined => "[object Undefined]",
            Value::Null => "[object Null]",
            v if v.is_array() => {
                let items = array_items(r, v)?;
                return join(r, &items, &Value::Undefined);
            }
            _ => "[object Object]",
        }))
    });
    def(realm, &p, "valueOf", 0, |_, this, _| Ok(this.clone()));
    let object = constructor(
        realm,
        "Object",
        1,
        &p,
        Some(|r, _, args| {
            let v = arg(args, 0);
            Ok(if v.is_nullish() { r.new_object() } else { v })
        }),
        None,
    );
    let o = object.object().unwrap().clone();
    def(realm, &o, "keys", 1, |r, _, args| {
        let v = arg(args, 0);
        if v.is_nullish() {
            return Err(r.type_error("Cannot convert undefined or null to object"));
        }
        let keys = r
            .own_keys(&v, true)
            .into_iter()
            .map(|k| k.to_value())
            .collect();
        Ok(r.new_array(keys))
    });
    def(realm, &o, "values", 1, |r, _, args| {
        let v = arg(args, 0);
        if v.is_nullish() {
            return Err(r.type_error("Cannot convert undefined or null to object"));
        }
        let values = r.own_entries(&v)?.into_iter().map(|(_, v)| v).collect();
        Ok(r.new_array(values))
    });
    def(realm, &o, "entries", 1, |r, _, args| {
        let v = arg(args, 0);
        if v.is_nullish() {
            return Err(r.type_error("Cannot convert undefined or null to object"));
        }
        let entries = r
            .own_entries(&v)?
            .into_iter()
            .map(|(k, v)| r.new_array(vec![k.to_value(), v]))
            .collect();
        Ok(r.new_array(entries))
    });
    def(realm, &o, "assign", 2, |r, _, args| {
        let target = arg(args, 0);
        if target.is_nullish() {
            return Err(r.type_error("Cannot convert undefined or null to object"));
        }
        for source in args.iter().skip(1) {
            if source.is_nullish() {
                continue;
            }
            for (k, v) in r.own_entries(source)? {
                r.set(&target, &k, v)?;
            }
        }
        Ok(target)
    });
    def(realm, &o, "fromEntries", 1, |r, _, args| {
        let out = r.new_object();
        for entry in r.collect_iterable(&arg(args, 0))? {
            let k = r.get_str(&entry, "0")?;
            let v = r.get_str(&entry, "1")?;
            let key = r.to_property_key(&k)?;
            r.define(&out, key, v)?;
        }
        Ok(out)
    });
    def(realm, &o, "freeze", 1, |_, _, args| {
        let v = arg(args, 0);
        if let Some(o) = v.object() {
            let mut d = o.borrow_mut();
            d.extensible = false;
            for p in &mut d.props {
                p.writable = false;
                p.configurable = false;
            }
        }
        Ok(v)
    });
    def(realm, &o, "getPrototypeOf", 1, |r, _, args| {
        let v = arg(args, 0);
        Ok(match v.object() {
            Some(o) => o.borrow().proto.clone().map_or(Value::Null, Value::Object),
            None if v.is_nullish() => {
                return Err(r.type_error("Cannot convert undefined or null to object"));
            }
            None => Value::Undefined,
        })
    });
    def(realm, &o, "create", 2, |r, _, args| {
        let proto = match arg(args, 0) {
            Value::Object(o) => Some(o),
            Value::Null => None,
            _ => return Err(r.type_error("Object prototype may only be an Object or null")),
        };
        Ok(r.alloc(Kind::Ordinary, proto.as_ref()))
    });
    def(realm, &o, "hasOwn", 2, |r, _, args| {
        let key = r.to_property_key(&arg(args, 1))?;
        Ok(Value::Bool(r.has_own(&arg(args, 0), &key)))
    });
    realm.set_global("Object", object);
}

fn install_function(realm: &mut Realm) {
    let p = realm.intrinsics().function_proto.clone();
    def(realm, &p, "call", 1, |r, this, args| {
        r.call(this, &arg(args, 0), args.get(1..).unwrap_or_default())
    });
    def(realm, &p, "apply", 2, |r, this, args| {
        let list = arg(args, 1);
        let list = if list.is_nullish() {
            vec![]
        } else {
            array_items(r, &list)?
        };
        r.call(this, &arg(args, 0), &list)
    });
    def(realm, &p, "bind", 1, |r, this, args| {
        let target = this.clone();
        let bound_this = arg(args, 0);
        let bound: Vec<Value> = args.get(1..).unwrap_or_default().to_vec();
        let name = target
            .function()
            .map(|f| format!("bound {}", String::from_utf16_lossy(&f.name.0)))
            .unwrap_or_default();
        Ok(r.native(&name, 0, move |r, _, rest| {
            let mut all = bound.clone();
            all.extend_from_slice(rest);
            r.call(&target, &bound_this, &all)
        }))
    });
}

fn join(r: &mut Realm, items: &[Value], sep: &Value) -> Result<Value, Thrown> {
    let sep = if matches!(sep, Value::Undefined) {
        units(",")
    } else {
        r.to_string(sep)?
    };
    let mut out = vec![];
    for (i, v) in items.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(&sep);
        }
        if !v.is_nullish() {
            out.extend_from_slice(&r.to_string(v)?);
        }
    }
    Ok(Value::String(out.into()))
}

fn install_array(realm: &mut Realm) {
    let p = realm.intrinsics().array_proto.clone();
    def(realm, &p, "push", 1, |r, this, args| {
        match with_array(this, |items| {
            items.extend_from_slice(args);
            items.len()
        }) {
            Some(n) => Ok(Value::Number(n as f64)),
            None => Err(r.type_error("Array.prototype.push called on non-array")),
        }
    });
    def(realm, &p, "pop", 0, |_, this, _| {
        Ok(with_array(this, |items| items.pop())
            .flatten()
            .unwrap_or(Value::Undefined))
    });
    def(realm, &p, "shift", 0, |_, this, _| {
        Ok(
            with_array(this, |items| (!items.is_empty()).then(|| items.remove(0)))
                .flatten()
                .unwrap_or(Value::Undefined),
        )
    });
    def(realm, &p, "unshift", 1, |_, this, args| {
        Ok(Value::Number(
            with_array(this, |items| {
                for (i, v) in args.iter().enumerate() {
                    items.insert(i, v.clone());
                }
                items.len()
            })
            .unwrap_or(0) as f64,
        ))
    });
    def(realm, &p, "slice", 2, |r, this, args| {
        let items = array_items(r, this)?;
        let start = relative_index(r, &arg(args, 0), items.len(), 0)?;
        let end = relative_index(r, &arg(args, 1), items.len(), items.len())?;
        Ok(r.new_array(
            items
                .get(start..end.max(start))
                .unwrap_or_default()
                .to_vec(),
        ))
    });
    def(realm, &p, "splice", 2, |r, this, args| {
        let len = array_items(r, this)?.len();
        let start = relative_index(r, &arg(args, 0), len, 0)?;
        let count = if args.len() < 2 {
            len - start
        } else {
            let n = r.to_number(&args[1])?;
            (if n.is_nan() { 0.0 } else { n.trunc() }).clamp(0.0, (len - start) as f64) as usize
        };
        let inserted = args.get(2..).unwrap_or_default().to_vec();
        let removed = with_array(this, |items| {
            items
                .splice(start..start + count, inserted)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
        Ok(r.new_array(removed))
    });
    def(realm, &p, "concat", 1, |r, this, args| {
        let mut out = array_items(r, this)?;
        for a in args {
            if a.is_array() {
                out.extend(array_items(r, a)?);
            } else {
                out.push(a.clone());
            }
        }
        Ok(r.new_array(out))
    });
    def(realm, &p, "join", 1, |r, this, args| {
        let items = array_items(r, this)?;
        join(r, &items, &arg(args, 0))
    });
    def(realm, &p, "toString", 0, |r, this, _| {
        let items = array_items(r, this)?;
        join(r, &items, &Value::Undefined)
    });
    def(realm, &p, "reverse", 0, |_, this, _| {
        with_array(this, |items| items.reverse());
        Ok(this.clone())
    });
    def(realm, &p, "includes", 1, |r, this, args| {
        let items = array_items(r, this)?;
        let needle = arg(args, 0);
        Ok(Value::Bool(
            items.iter().any(|v| v.same_value_zero(&needle)),
        ))
    });
    def(realm, &p, "indexOf", 1, |r, this, args| {
        let items = array_items(r, this)?;
        let needle = arg(args, 0);
        Ok(Value::Number(
            items
                .iter()
                .position(|v| v.strict_equal(&needle))
                .map_or(-1.0, |i| i as f64),
        ))
    });
    // Callback iteration reads the live length each step, like the spec.
    fn each(
        r: &mut Realm,
        this: &Value,
        args: &[Value],
        mut visit: impl FnMut(&mut Realm, usize, Value, Value) -> Result<bool, Thrown>,
    ) -> Result<(), Thrown> {
        let f = arg(args, 0);
        callable(r, &f)?;
        let this_arg = arg(args, 1);
        let len = array_items(r, this)?.len();
        for i in 0..len {
            let key = PropKey::str(&i.to_string());
            if !r.has_property(this, &key)? {
                continue;
            }
            let v = r.get(this, &key)?;
            let result = r.call(
                &f,
                &this_arg,
                &[v.clone(), Value::Number(i as f64), this.clone()],
            )?;
            if !visit(r, i, v, result)? {
                break;
            }
        }
        Ok(())
    }
    def(realm, &p, "map", 1, |r, this, args| {
        let mut out = vec![Value::Undefined; array_items(r, this)?.len()];
        each(r, this, args, |_, i, _, result| {
            out[i] = result;
            Ok(true)
        })?;
        Ok(r.new_array(out))
    });
    def(realm, &p, "filter", 1, |r, this, args| {
        let mut out = vec![];
        each(r, this, args, |_, _, v, result| {
            if result.truthy() {
                out.push(v);
            }
            Ok(true)
        })?;
        Ok(r.new_array(out))
    });
    def(realm, &p, "forEach", 1, |r, this, args| {
        each(r, this, args, |_, _, _, _| Ok(true))?;
        Ok(Value::Undefined)
    });
    def(realm, &p, "some", 1, |r, this, args| {
        let mut found = false;
        each(r, this, args, |_, _, _, result| {
            found = result.truthy();
            Ok(!found)
        })?;
        Ok(Value::Bool(found))
    });
    def(realm, &p, "every", 1, |r, this, args| {
        let mut all = true;
        each(r, this, args, |_, _, _, result| {
            all = result.truthy();
            Ok(all)
        })?;
        Ok(Value::Bool(all))
    });
    // find/findIndex visit holes as undefined.
    fn find(r: &mut Realm, this: &Value, args: &[Value]) -> Result<Option<(usize, Value)>, Thrown> {
        let f = arg(args, 0);
        callable(r, &f)?;
        let this_arg = arg(args, 1);
        let len = array_items(r, this)?.len();
        for i in 0..len {
            let v = r.get(this, &PropKey::str(&i.to_string()))?;
            let hit = r.call(
                &f,
                &this_arg,
                &[v.clone(), Value::Number(i as f64), this.clone()],
            )?;
            if hit.truthy() {
                return Ok(Some((i, v)));
            }
        }
        Ok(None)
    }
    def(realm, &p, "find", 1, |r, this, args| {
        Ok(find(r, this, args)?.map_or(Value::Undefined, |(_, v)| v))
    });
    def(realm, &p, "findIndex", 1, |r, this, args| {
        Ok(Value::Number(
            find(r, this, args)?.map_or(-1.0, |(i, _)| i as f64),
        ))
    });
    def(realm, &p, "reduce", 2, |r, this, args| {
        let f = arg(args, 0);
        callable(r, &f)?;
        let len = array_items(r, this)?.len();
        let mut i = 0;
        let mut acc = if args.len() >= 2 {
            args[1].clone()
        } else {
            loop {
                if i >= len {
                    return Err(r.type_error("Reduce of empty array with no initial value"));
                }
                let key = PropKey::str(&i.to_string());
                i += 1;
                if r.has_property(this, &key)? {
                    break r.get(this, &key)?;
                }
            }
        };
        while i < len {
            let key = PropKey::str(&i.to_string());
            if r.has_property(this, &key)? {
                let v = r.get(this, &key)?;
                acc = r.call(
                    &f,
                    &Value::Undefined,
                    &[acc, v, Value::Number(i as f64), this.clone()],
                )?;
            }
            i += 1;
        }
        Ok(acc)
    });
    def(realm, &p, "sort", 1, |r, this, args| {
        let compare = arg(args, 0);
        let mut items = array_items(r, this)?;
        // Stable insertion sort; comparator and ToString errors propagate.
        let mut sorted: Vec<Value> = Vec::with_capacity(items.len());
        let undefined_count = items
            .iter()
            .filter(|v| matches!(v, Value::Undefined))
            .count();
        items.retain(|v| !matches!(v, Value::Undefined));
        for v in items {
            let mut pos = sorted.len();
            while pos > 0 {
                let before = sorted[pos - 1].clone();
                let after = if compare.is_callable() {
                    let x = r.call(&compare, &Value::Undefined, &[before, v.clone()])?;
                    r.to_number(&x)? > 0.0
                } else {
                    let a = r.to_string(&before)?;
                    let b = r.to_string(&v)?;
                    a[..] > b[..]
                };
                if !after {
                    break;
                }
                pos -= 1;
            }
            sorted.insert(pos, v);
        }
        sorted.extend(std::iter::repeat_n(Value::Undefined, undefined_count));
        with_array(this, |items| *items = sorted);
        Ok(this.clone())
    });
    def(realm, &p, "flat", 0, |r, this, args| {
        let depth = match arg(args, 0) {
            Value::Undefined => 1.0,
            d => r.to_number(&d)?,
        };
        fn flatten(
            r: &mut Realm,
            items: Vec<Value>,
            depth: f64,
            out: &mut Vec<Value>,
        ) -> Result<(), Thrown> {
            for v in items {
                if depth >= 1.0 && v.is_array() {
                    let inner = array_items(r, &v)?;
                    flatten(r, inner, depth - 1.0, out)?;
                } else {
                    out.push(v);
                }
            }
            Ok(())
        }
        let items = array_items(r, this)?;
        let mut out = vec![];
        flatten(r, items, depth, &mut out)?;
        Ok(r.new_array(out))
    });
    def(realm, &p, "flatMap", 1, |r, this, args| {
        let mut out = vec![];
        each(r, this, args, |r, _, _, result| {
            if result.is_array() {
                out.extend(array_items(r, &result)?);
            } else {
                out.push(result);
            }
            Ok(true)
        })?;
        Ok(r.new_array(out))
    });
    def(realm, &p, "at", 1, |r, this, args| {
        let items = array_items(r, this)?;
        let n = r.to_number(&arg(args, 0))?.trunc();
        let i = if n < 0.0 { items.len() as f64 + n } else { n };
        Ok(if i >= 0.0 {
            items.get(i as usize).cloned()
        } else {
            None
        }
        .unwrap_or(Value::Undefined))
    });
    def(realm, &p, "keys", 0, |r, this, _| {
        let len = array_items(r, this)?.len();
        let keys = (0..len).map(|i| Value::Number(i as f64)).collect();
        Ok(r.iterator_object(IterSource::Keys { keys, index: 0 }))
    });
    def(realm, &p, "values", 0, |r, this, _| match this.object() {
        Some(o) => Ok(r.iterator_object(IterSource::Array {
            target: o.clone(),
            index: 0,
        })),
        None => Err(r.type_error("Array.prototype.values called on non-object")),
    });
    def(realm, &p, "entries", 0, |r, this, _| {
        let items = array_items(r, this)?;
        let keys = items
            .into_iter()
            .enumerate()
            .map(|(i, v)| r.new_array(vec![Value::Number(i as f64), v]))
            .collect();
        Ok(r.iterator_object(IterSource::Keys { keys, index: 0 }))
    });
    def_iterator(realm, &p, |r, this, _| match this.object() {
        Some(o) => Ok(r.iterator_object(IterSource::Array {
            target: o.clone(),
            index: 0,
        })),
        None => Err(r.type_error("not iterable")),
    });
    let array = constructor(
        realm,
        "Array",
        1,
        &p,
        Some(|r, _, args| Ok(r.new_array(args.to_vec()))),
        Some(|r, args, _| Ok(r.new_array(args.to_vec()))),
    );
    let a = array.object().unwrap().clone();
    def(realm, &a, "isArray", 1, |_, _, args| {
        Ok(Value::Bool(arg(args, 0).is_array()))
    });
    def(realm, &a, "of", 0, |r, _, args| {
        Ok(r.new_array(args.to_vec()))
    });
    def(realm, &a, "from", 1, |r, _, args| {
        let source = arg(args, 0);
        let f = arg(args, 1);
        let items = if source.is_nullish() {
            return Err(r.type_error("Cannot convert undefined or null to object"));
        } else if source.object().is_some_and(|o| {
            matches!(
                o.borrow().kind,
                Kind::Array(_) | Kind::Map(_) | Kind::Set(_) | Kind::Iterator(_)
            )
        }) || matches!(source, Value::String(_))
        {
            r.collect_iterable(&source)?
        } else {
            array_items(r, &source)?
        };
        if f.is_callable() {
            let mut out = vec![];
            for (i, v) in items.into_iter().enumerate() {
                out.push(r.call(&f, &Value::Undefined, &[v, Value::Number(i as f64)])?);
            }
            return Ok(r.new_array(out));
        }
        Ok(r.new_array(items))
    });
    realm.set_global("Array", array);
    let it = realm.intrinsics().iterator_proto.clone();
    def(realm, &it, "next", 0, |r, this, _| {
        let v = r.iterator_step(this)?;
        Ok(r.object_from(vec![
            ("value", v.clone().unwrap_or(Value::Undefined)),
            ("done", Value::Bool(v.is_none())),
        ]))
    });
    def(realm, &it, "return", 0, |r, this, _| {
        r.iterator_close(this)?;
        Ok(r.object_from(vec![
            ("value", Value::Undefined),
            ("done", Value::Bool(true)),
        ]))
    });
    def_iterator(realm, &it, |_, this, _| Ok(this.clone()));
}

fn install_string(realm: &mut Realm) {
    let p = realm.intrinsics().string_proto.clone();
    def(realm, &p, "toString", 0, |r, this, _| {
        Ok(Value::String(this_units(r, this)?))
    });
    def(realm, &p, "valueOf", 0, |r, this, _| {
        Ok(Value::String(this_units(r, this)?))
    });
    def(realm, &p, "toLowerCase", 0, |r, this, _| {
        let s = String::from_utf16_lossy(&this_units(r, this)?);
        Ok(Value::str(&s.to_lowercase()))
    });
    def(realm, &p, "toUpperCase", 0, |r, this, _| {
        let s = String::from_utf16_lossy(&this_units(r, this)?);
        Ok(Value::str(&s.to_uppercase()))
    });
    def(realm, &p, "trim", 0, |r, this, _| {
        let s = String::from_utf16_lossy(&this_units(r, this)?);
        Ok(Value::str(safe_cex_num::js_trim(&s)))
    });
    def(realm, &p, "startsWith", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let needle = r.to_string(&arg(args, 0))?;
        let from = relative_index(r, &arg(args, 1), s.len(), 0)?;
        Ok(Value::Bool(s[from..].starts_with(&needle)))
    });
    def(realm, &p, "endsWith", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let needle = r.to_string(&arg(args, 0))?;
        Ok(Value::Bool(s.ends_with(&needle)))
    });
    def(realm, &p, "includes", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let needle = r.to_string(&arg(args, 0))?;
        Ok(Value::Bool(find_units(&s, &needle, 0).is_some()))
    });
    def(realm, &p, "indexOf", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let needle = r.to_string(&arg(args, 0))?;
        let from = relative_index(r, &arg(args, 1), s.len(), 0)?;
        Ok(Value::Number(
            find_units(&s, &needle, from).map_or(-1.0, |i| i as f64),
        ))
    });
    def(realm, &p, "slice", 2, |r, this, args| {
        let s = this_units(r, this)?;
        let start = relative_index(r, &arg(args, 0), s.len(), 0)?;
        let end = relative_index(r, &arg(args, 1), s.len(), s.len())?;
        Ok(Value::units(
            s.get(start..end.max(start)).unwrap_or_default(),
        ))
    });
    def(realm, &p, "substring", 2, |r, this, args| {
        let s = this_units(r, this)?;
        let clamp = |r: &mut Realm, v: &Value, d: usize| -> Result<usize, Thrown> {
            if matches!(v, Value::Undefined) {
                return Ok(d);
            }
            let n = r.to_number(v)?;
            Ok(if n.is_nan() {
                0
            } else {
                n.clamp(0.0, s.len() as f64) as usize
            })
        };
        let a = clamp(r, &arg(args, 0), 0)?;
        let b = clamp(r, &arg(args, 1), s.len())?;
        Ok(Value::units(&s[a.min(b)..a.max(b)]))
    });
    def(realm, &p, "substr", 2, |r, this, args| {
        let s = this_units(r, this)?;
        let start = relative_index(r, &arg(args, 0), s.len(), 0)?;
        let len = match arg(args, 1) {
            Value::Undefined => s.len() - start,
            v => {
                let n = r.to_number(&v)?;
                (if n.is_nan() { 0.0 } else { n.trunc() }).clamp(0.0, (s.len() - start) as f64)
                    as usize
            }
        };
        Ok(Value::units(&s[start..start + len]))
    });
    def(realm, &p, "charAt", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let i = r.to_number(&arg(args, 0))?;
        let i = if i.is_nan() { 0.0 } else { i.trunc() };
        Ok(if i >= 0.0 && (i as usize) < s.len() {
            Value::units(&s[i as usize..i as usize + 1])
        } else {
            Value::str("")
        })
    });
    def(realm, &p, "charCodeAt", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let i = r.to_number(&arg(args, 0))?;
        let i = if i.is_nan() { 0.0 } else { i.trunc() };
        Ok(Value::Number(if i >= 0.0 {
            s.get(i as usize).map_or(f64::NAN, |u| *u as f64)
        } else {
            f64::NAN
        }))
    });
    def(realm, &p, "padStart", 2, |r, this, args| {
        pad(r, this, args, true)
    });
    def(realm, &p, "padEnd", 2, |r, this, args| {
        pad(r, this, args, false)
    });
    def(realm, &p, "repeat", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let n = r.to_number(&arg(args, 0))?;
        if n < 0.0 || !n.is_finite() {
            return Err(r.error_of("RangeError", "Invalid count value"));
        }
        Ok(Value::units(&s.repeat(n as usize)))
    });
    def(realm, &p, "split", 2, |r, this, args| {
        let s = this_units(r, this)?;
        let sep = arg(args, 0);
        if let Some(p) = sep.object().and_then(|o| match &o.borrow().kind {
            Kind::RegExp(p) => Some(p.clone()),
            _ => None,
        }) {
            let parts = p
                .split(&s)
                .into_iter()
                .map(|p| p.map_or(Value::Undefined, |u| Value::units(&u)))
                .collect();
            return Ok(r.new_array(parts));
        }
        if matches!(sep, Value::Undefined) {
            return Ok(r.new_array(vec![Value::String(s)]));
        }
        let sep = r.to_string(&sep)?;
        if sep.is_empty() {
            let parts = s.iter().map(|u| Value::units(&[*u])).collect();
            return Ok(r.new_array(parts));
        }
        let mut parts = vec![];
        let mut last = 0;
        while let Some(i) = find_units(&s, &sep, last) {
            if i + sep.len() > s.len() {
                break;
            }
            parts.push(Value::units(&s[last..i]));
            last = i + sep.len();
        }
        parts.push(Value::units(&s[last..]));
        Ok(r.new_array(parts))
    });
    def(realm, &p, "replace", 2, |r, this, args| {
        replace(r, this, args, false)
    });
    def(realm, &p, "replaceAll", 2, |r, this, args| {
        replace(r, this, args, true)
    });
    def(realm, &p, "match", 1, |r, this, args| {
        let s = this_units(r, this)?;
        let p = match arg(args, 0).object().map(|o| match &o.borrow().kind {
            Kind::RegExp(p) => Some(p.clone()),
            _ => None,
        }) {
            Some(Some(p)) => p,
            _ => {
                let source = r.to_rust_string(&arg(args, 0))?;
                Rc::new(
                    Pattern::new(&regex_escape(&source), "")
                        .map_err(|e| r.error_of("SyntaxError", &e))?,
                )
            }
        };
        exec_result(r, &p, &s)
    });
    def(realm, &p, "concat", 1, |r, this, args| {
        let mut s = this_units(r, this)?.to_vec();
        for a in args {
            s.extend_from_slice(&r.to_string(a)?);
        }
        Ok(Value::String(s.into()))
    });
    def(realm, &p, "localeCompare", 1, |r, this, args| {
        let a = String::from_utf16_lossy(&this_units(r, this)?);
        let b = r.to_rust_string(&arg(args, 0))?;
        Ok(Value::Number(match a.cmp(&b) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        }))
    });
    def_iterator(realm, &p, |r, this, _| {
        let s = this_units(r, this)?;
        Ok(r.iterator_object(IterSource::String { units: s, index: 0 }))
    });
    let string = constructor(
        realm,
        "String",
        1,
        &p,
        Some(|r, _, args| {
            if args.is_empty() {
                return Ok(Value::str(""));
            }
            if let Value::Symbol(s) = &args[0] {
                return Ok(Value::str(&format!("Symbol({})", s.description)));
            }
            Ok(Value::String(r.to_string(&args[0])?))
        }),
        None,
    );
    realm.set_global("String", string);
}

fn regex_escape(text: &str) -> String {
    regex::escape(text)
}
fn exec_result(r: &mut Realm, p: &Pattern, s: &[u16]) -> Result<Value, Thrown> {
    let Some((start, groups)) = p.exec(s) else {
        return Ok(Value::Null);
    };
    let items = groups
        .into_iter()
        .map(|g| g.map_or(Value::Undefined, |u| Value::units(&u)))
        .collect();
    let out = r.new_array(items);
    r.define_str(&out, "index", Value::Number(start as f64))?;
    r.define_str(&out, "input", Value::units(s))?;
    Ok(out)
}
fn pad(r: &mut Realm, this: &Value, args: &[Value], start: bool) -> Result<Value, Thrown> {
    let s = this_units(r, this)?;
    let target = r.to_number(&arg(args, 0))?;
    let filler = match arg(args, 1) {
        Value::Undefined => units(" "),
        v => r.to_string(&v)?,
    };
    let target = if target.is_nan() {
        0
    } else {
        target.max(0.0) as usize
    };
    if target <= s.len() || filler.is_empty() {
        return Ok(Value::String(s));
    }
    let fill: Vec<u16> = filler
        .iter()
        .cycle()
        .take(target - s.len())
        .copied()
        .collect();
    let out: Vec<u16> = if start {
        fill.into_iter().chain(s.iter().copied()).collect()
    } else {
        s.iter().copied().chain(fill).collect()
    };
    Ok(Value::String(out.into()))
}
fn replace(r: &mut Realm, this: &Value, args: &[Value], all: bool) -> Result<Value, Thrown> {
    let s = String::from_utf16_lossy(&this_units(r, this)?);
    let pattern = arg(args, 0);
    let replacement = arg(args, 1);
    if replacement.is_callable() {
        return Err(r.error("replace with a function replacer is not supported in this realm"));
    }
    let replacement = r.to_rust_string(&replacement)?;
    let regex = pattern.object().and_then(|o| match &o.borrow().kind {
        Kind::RegExp(p) => Some(p.clone()),
        _ => None,
    });
    let out = match regex {
        Some(p) => {
            let global = p.flags.contains('g');
            if all && !global {
                return Err(r.type_error("replaceAll must be called with a global RegExp"));
            }
            let re = regex::Regex::new(&format!(
                "{}{}",
                if p.flags.contains('i') { "(?i)" } else { "" },
                p.source
            ))
            .map_err(|e| r.error_of("SyntaxError", &e.to_string()))?;
            let rust_replacement = replacement.replace("$&", "${0}");
            if global {
                re.replace_all(&s, rust_replacement.as_str()).into_owned()
            } else {
                re.replace(&s, rust_replacement.as_str()).into_owned()
            }
        }
        None => {
            let needle = r.to_rust_string(&pattern)?;
            if all {
                safe_cex_num::js_replace_all(&s, &needle, &replacement)
                    .map_err(|e| r.error_of(e.name, e.message))?
            } else {
                safe_cex_num::js_replace(&s, &needle, &replacement)
            }
        }
    };
    Ok(Value::str(&out))
}

/// V8 DoubleToRadixCString: shortest digits that round-trip, for any radix.
pub(crate) fn radix_string(value: f64, radix: u32) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if value == 0.0 {
        return "0".into();
    }
    let chars: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let negative = value < 0.0;
    let value = value.abs();
    let mut integer = value.floor();
    let mut fraction = value - integer;
    let next_up = f64::from_bits(value.to_bits() + 1);
    let mut delta = (0.5 * (next_up - value)).max(f64::from_bits(1));
    let mut digits: Vec<char> = vec![];
    if fraction >= delta {
        loop {
            fraction *= radix as f64;
            delta *= radix as f64;
            let digit = fraction as u32;
            digits.push(chars[digit as usize]);
            fraction -= digit as f64;
            if (fraction > 0.5 || (fraction == 0.5 && digit & 1 == 1)) && fraction + delta > 1.0 {
                // Round up, propagating carries into the integer part.
                loop {
                    match digits.pop() {
                        None => {
                            integer += 1.0;
                            break;
                        }
                        Some(c) => {
                            let d = c.to_digit(36).unwrap();
                            if d + 1 < radix {
                                digits.push(chars[(d + 1) as usize]);
                                break;
                            }
                        }
                    }
                }
                break;
            }
            if fraction < delta {
                break;
            }
        }
    }
    let mut int_digits: Vec<char> = vec![];
    // Beyond 2^53 the low digits are not representable; V8 emits zeros for them.
    while integer / radix as f64 >= 9007199254740992.0 {
        integer /= radix as f64;
        int_digits.push('0');
    }
    while integer >= 1.0 {
        let remainder = integer % radix as f64;
        int_digits.push(chars[remainder as usize]);
        integer = (integer - remainder) / radix as f64;
    }
    if int_digits.is_empty() {
        int_digits.push('0');
    }
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.extend(int_digits.iter().rev());
    if !digits.is_empty() {
        out.push('.');
        out.extend(digits);
    }
    out
}

fn this_number(r: &mut Realm, this: &Value) -> Result<f64, Thrown> {
    match this {
        Value::Number(n) => Ok(*n),
        _ => Err(r.type_error("Number.prototype method called on non-number")),
    }
}

fn install_number(realm: &mut Realm) {
    let p = realm.intrinsics().number_proto.clone();
    def(realm, &p, "toFixed", 1, |r, this, args| {
        let n = this_number(r, this)?;
        let digits = r.to_number(&arg(args, 0))?;
        safe_cex_num::js_to_fixed(n, if digits.is_nan() { 0.0 } else { digits })
            .map(|s| Value::str(&s))
            .map_err(|e| r.error_of("RangeError", e))
    });
    def(realm, &p, "toString", 1, |r, this, args| {
        let n = this_number(r, this)?;
        match arg(args, 0) {
            Value::Undefined => Ok(Value::str(&safe_cex_num::js_number_string(n))),
            radix => {
                let radix = r.to_number(&radix)?;
                if radix == 10.0 {
                    return Ok(Value::str(&safe_cex_num::js_number_string(n)));
                }
                if (2.0..=36.0).contains(&radix) && radix.fract() == 0.0 {
                    return Ok(Value::str(&radix_string(n, radix as u32)));
                }
                Err(r.error_of("RangeError", "toString() radix must be between 2 and 36"))
            }
        }
    });
    def(realm, &p, "valueOf", 0, |r, this, _| {
        Ok(Value::Number(this_number(r, this)?))
    });
    let number = constructor(
        realm,
        "Number",
        1,
        &p,
        Some(|r, _, args| {
            if args.is_empty() {
                return Ok(Value::Number(0.0));
            }
            Ok(Value::Number(r.to_number(&args[0])?))
        }),
        None,
    );
    let n = number.object().unwrap().clone();
    def(realm, &n, "isFinite", 1, |_, _, args| {
        Ok(Value::Bool(
            arg(args, 0).as_number().is_some_and(f64::is_finite),
        ))
    });
    def(realm, &n, "isNaN", 1, |_, _, args| {
        Ok(Value::Bool(
            arg(args, 0).as_number().is_some_and(f64::is_nan),
        ))
    });
    def(realm, &n, "isInteger", 1, |_, _, args| {
        Ok(Value::Bool(
            arg(args, 0)
                .as_number()
                .is_some_and(|n| n.is_finite() && n.fract() == 0.0),
        ))
    });
    def(realm, &n, "isSafeInteger", 1, |_, _, args| {
        Ok(Value::Bool(arg(args, 0).as_number().is_some_and(|n| {
            n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0
        })))
    });
    def(realm, &n, "parseFloat", 1, parse_float);
    def(realm, &n, "parseInt", 2, parse_int);
    def_value(&n, "MAX_SAFE_INTEGER", Value::Number(9007199254740991.0));
    def_value(&n, "EPSILON", Value::Number(f64::EPSILON));
    def_value(&n, "POSITIVE_INFINITY", Value::Number(f64::INFINITY));
    def_value(&n, "NEGATIVE_INFINITY", Value::Number(f64::NEG_INFINITY));
    realm.set_global("Number", number);
    let b = realm.intrinsics().boolean_proto.clone();
    def(realm, &b, "toString", 0, |_, this, _| {
        Ok(Value::str(if this.truthy() { "true" } else { "false" }))
    });
    def(realm, &b, "valueOf", 0, |_, this, _| Ok(this.clone()));
    let boolean = constructor(
        realm,
        "Boolean",
        1,
        &b,
        Some(|_, _, args| Ok(Value::Bool(arg(args, 0).truthy()))),
        None,
    );
    realm.set_global("Boolean", boolean);

    let math = realm.new_object();
    let m = math.object().unwrap().clone();
    fn unary(r: &mut Realm, args: &[Value], f: fn(f64) -> f64) -> Result<Value, Thrown> {
        Ok(Value::Number(f(r.to_number(&arg(args, 0))?)))
    }
    def(realm, &m, "abs", 1, |r, _, a| unary(r, a, f64::abs));
    def(realm, &m, "floor", 1, |r, _, a| unary(r, a, f64::floor));
    def(realm, &m, "ceil", 1, |r, _, a| unary(r, a, f64::ceil));
    def(realm, &m, "trunc", 1, |r, _, a| unary(r, a, f64::trunc));
    def(realm, &m, "sqrt", 1, |r, _, a| unary(r, a, f64::sqrt));
    def(realm, &m, "log", 1, |r, _, a| unary(r, a, f64::ln));
    def(realm, &m, "log10", 1, |r, _, a| {
        unary(r, a, safe_cex_num::math_log10)
    });
    def(realm, &m, "round", 1, |r, _, a| {
        unary(r, a, safe_cex_num::math_round)
    });
    def(realm, &m, "sign", 1, |r, _, a| {
        unary(r, a, |n| {
            if n.is_nan() || n == 0.0 {
                n
            } else {
                n.signum()
            }
        })
    });
    def(realm, &m, "pow", 2, |r, _, a| {
        let x = r.to_number(&arg(a, 0))?;
        let y = r.to_number(&arg(a, 1))?;
        Ok(Value::Number(safe_cex_num::math_pow(x, y)))
    });
    def(realm, &m, "max", 2, |r, _, a| {
        let mut acc = f64::NEG_INFINITY;
        for v in a {
            acc = safe_cex_num::math_max(acc, r.to_number(v)?);
        }
        Ok(Value::Number(acc))
    });
    def(realm, &m, "min", 2, |r, _, a| {
        let mut acc = f64::INFINITY;
        for v in a {
            acc = safe_cex_num::math_min(acc, r.to_number(v)?);
        }
        Ok(Value::Number(acc))
    });
    def(realm, &m, "random", 0, |r, _, _| {
        Ok(Value::Number(r.runtime().random()))
    });
    realm.set_global("Math", math);
}

fn parse_float(r: &mut Realm, _: &Value, args: &[Value]) -> Result<Value, Thrown> {
    let s = r.to_rust_string(&arg(args, 0))?;
    Ok(Value::Number(safe_cex_num::parse_float(&s)))
}
fn parse_int(r: &mut Realm, _: &Value, args: &[Value]) -> Result<Value, Thrown> {
    let s = r.to_rust_string(&arg(args, 0))?;
    let radix = match arg(args, 1) {
        Value::Undefined => None,
        v => Some(crate::realm::convert::to_int32(r.to_number(&v)?)),
    };
    Ok(Value::Number(safe_cex_num::parse_int(&s, radix)))
}

fn install_collections(realm: &mut Realm) {
    let p = realm.intrinsics().map_proto.clone();
    def(realm, &p, "get", 1, |r, this, args| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.get called on incompatible receiver"))?;
        let k = arg(args, 0);
        let d = o.borrow();
        let Kind::Map(entries) = &d.kind else {
            unreachable!()
        };
        Ok(entries
            .iter()
            .find(|(key, _)| key.same_value_zero(&k))
            .map_or(Value::Undefined, |(_, v)| v.clone()))
    });
    def(realm, &p, "set", 2, |r, this, args| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.set called on incompatible receiver"))?;
        let k = normalize_key(&arg(args, 0));
        let v = arg(args, 1);
        let mut d = o.borrow_mut();
        let Kind::Map(entries) = &mut d.kind else {
            unreachable!()
        };
        match entries.iter_mut().find(|(key, _)| key.same_value_zero(&k)) {
            Some(e) => e.1 = v,
            None => entries.push((k, v)),
        }
        Ok(this.clone())
    });
    def(realm, &p, "has", 1, |r, this, args| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.has called on incompatible receiver"))?;
        let k = arg(args, 0);
        let d = o.borrow();
        let Kind::Map(entries) = &d.kind else {
            unreachable!()
        };
        Ok(Value::Bool(
            entries.iter().any(|(key, _)| key.same_value_zero(&k)),
        ))
    });
    def(realm, &p, "delete", 1, |r, this, args| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.delete called on incompatible receiver"))?;
        let k = arg(args, 0);
        let mut d = o.borrow_mut();
        let Kind::Map(entries) = &mut d.kind else {
            unreachable!()
        };
        let before = entries.len();
        entries.retain(|(key, _)| !key.same_value_zero(&k));
        Ok(Value::Bool(entries.len() != before))
    });
    def(realm, &p, "clear", 0, |_, this, _| {
        if let Some(o) = map_cell(this)
            && let Kind::Map(entries) = &mut o.borrow_mut().kind
        {
            entries.clear();
        }
        Ok(Value::Undefined)
    });
    def_getter(realm, &p, "size", |r, this, _| {
        let o = map_cell(this).ok_or_else(|| r.type_error("Map size on incompatible receiver"))?;
        let Kind::Map(entries) = &o.borrow().kind else {
            unreachable!()
        };
        Ok(Value::Number(entries.len() as f64))
    });
    def(realm, &p, "forEach", 1, |r, this, args| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.forEach on incompatible receiver"))?;
        let f = arg(args, 0);
        callable(r, &f)?;
        let mut i = 0;
        loop {
            let entry = match &o.borrow().kind {
                Kind::Map(entries) => entries.get(i).cloned(),
                _ => None,
            };
            let Some((k, v)) = entry else { break };
            r.call(&f, &arg(args, 1), &[v, k, this.clone()])?;
            i += 1;
        }
        Ok(Value::Undefined)
    });
    def(realm, &p, "keys", 0, |r, this, _| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.keys on incompatible receiver"))?;
        Ok(r.iterator_object(IterSource::MapKeys {
            target: o,
            index: 0,
        }))
    });
    def(realm, &p, "values", 0, |r, this, _| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.values on incompatible receiver"))?;
        Ok(r.iterator_object(IterSource::MapValues {
            target: o,
            index: 0,
        }))
    });
    def(realm, &p, "entries", 0, |r, this, _| {
        let o = map_cell(this)
            .ok_or_else(|| r.type_error("Map.prototype.entries on incompatible receiver"))?;
        Ok(r.iterator_object(IterSource::MapEntries {
            target: o,
            index: 0,
        }))
    });
    def_iterator(realm, &p, |r, this, _| {
        let o =
            map_cell(this).ok_or_else(|| r.type_error("Map iterator on incompatible receiver"))?;
        Ok(r.iterator_object(IterSource::MapEntries {
            target: o,
            index: 0,
        }))
    });
    let map = constructor(
        realm,
        "Map",
        0,
        &p,
        None,
        Some(|r, args, target| {
            let proto = r.prototype_of_target(target, &r.intrinsics().map_proto.clone())?;
            let m = r.alloc(Kind::Map(vec![]), Some(&proto));
            let init = arg(args, 0);
            if !init.is_nullish() {
                let set = r.get_str(&m, "set")?;
                for entry in r.collect_iterable(&init)? {
                    let k = r.get_str(&entry, "0")?;
                    let v = r.get_str(&entry, "1")?;
                    r.call(&set, &m, &[k, v])?;
                }
            }
            Ok(m)
        }),
    );
    realm.set_global("Map", map);

    let p = realm.intrinsics().set_proto.clone();
    def(realm, &p, "add", 1, |r, this, args| {
        let o = set_cell(this)
            .ok_or_else(|| r.type_error("Set.prototype.add called on incompatible receiver"))?;
        let v = normalize_key(&arg(args, 0));
        let mut d = o.borrow_mut();
        let Kind::Set(items) = &mut d.kind else {
            unreachable!()
        };
        if !items.iter().any(|x| x.same_value_zero(&v)) {
            items.push(v);
        }
        Ok(this.clone())
    });
    def(realm, &p, "has", 1, |r, this, args| {
        let o = set_cell(this)
            .ok_or_else(|| r.type_error("Set.prototype.has called on incompatible receiver"))?;
        let v = arg(args, 0);
        let Kind::Set(items) = &o.borrow().kind else {
            unreachable!()
        };
        Ok(Value::Bool(items.iter().any(|x| x.same_value_zero(&v))))
    });
    def(realm, &p, "delete", 1, |r, this, args| {
        let o = set_cell(this)
            .ok_or_else(|| r.type_error("Set.prototype.delete called on incompatible receiver"))?;
        let v = arg(args, 0);
        let mut d = o.borrow_mut();
        let Kind::Set(items) = &mut d.kind else {
            unreachable!()
        };
        let before = items.len();
        items.retain(|x| !x.same_value_zero(&v));
        Ok(Value::Bool(before != items.len()))
    });
    def(realm, &p, "clear", 0, |_, this, _| {
        if let Some(o) = set_cell(this)
            && let Kind::Set(items) = &mut o.borrow_mut().kind
        {
            items.clear();
        }
        Ok(Value::Undefined)
    });
    def_getter(realm, &p, "size", |r, this, _| {
        let o = set_cell(this).ok_or_else(|| r.type_error("Set size on incompatible receiver"))?;
        let Kind::Set(items) = &o.borrow().kind else {
            unreachable!()
        };
        Ok(Value::Number(items.len() as f64))
    });
    def(realm, &p, "forEach", 1, |r, this, args| {
        let o = set_cell(this)
            .ok_or_else(|| r.type_error("Set.prototype.forEach on incompatible receiver"))?;
        let f = arg(args, 0);
        callable(r, &f)?;
        let mut i = 0;
        loop {
            let item = match &o.borrow().kind {
                Kind::Set(items) => items.get(i).cloned(),
                _ => None,
            };
            let Some(v) = item else { break };
            r.call(&f, &arg(args, 1), &[v.clone(), v, this.clone()])?;
            i += 1;
        }
        Ok(Value::Undefined)
    });
    for name in ["values", "keys"] {
        def(realm, &p, name, 0, |r, this, _| {
            let o = set_cell(this)
                .ok_or_else(|| r.type_error("Set iterator on incompatible receiver"))?;
            Ok(r.iterator_object(IterSource::SetValues {
                target: o,
                index: 0,
            }))
        });
    }
    def_iterator(realm, &p, |r, this, _| {
        let o =
            set_cell(this).ok_or_else(|| r.type_error("Set iterator on incompatible receiver"))?;
        Ok(r.iterator_object(IterSource::SetValues {
            target: o,
            index: 0,
        }))
    });
    let set = constructor(
        realm,
        "Set",
        0,
        &p,
        None,
        Some(|r, args, target| {
            let proto = r.prototype_of_target(target, &r.intrinsics().set_proto.clone())?;
            let s = r.alloc(Kind::Set(vec![]), Some(&proto));
            let init = arg(args, 0);
            if !init.is_nullish() {
                let add = r.get_str(&s, "add")?;
                for v in r.collect_iterable(&init)? {
                    r.call(&add, &s, &[v])?;
                }
            }
            Ok(s)
        }),
    );
    realm.set_global("Set", set);
}

/// then(onFulfilled, onRejected): a derived promise settled by the reaction.
fn then(r: &mut Realm, this: &Value, fulfilled: Value, rejected: Value) -> Result<Value, Thrown> {
    let Some(_) = this.promise() else {
        return Err(r.type_error("Method Promise.prototype.then called on incompatible receiver"));
    };
    let source = r.await_value(this.clone());
    let realm = r.clone();
    Ok(r.spawn_value(Box::pin(async move {
        let mut realm = realm;
        match source.await {
            Ok(v) if fulfilled.is_callable() => {
                let out = realm.call(&fulfilled, &Value::Undefined, &[v])?;
                realm.await_value(out).await
            }
            Ok(v) => Ok(v),
            Err(e) if rejected.is_callable() => {
                let out = realm.call(&rejected, &Value::Undefined, &[e])?;
                realm.await_value(out).await
            }
            Err(e) => Err(e),
        }
    })))
}

/// Settles with the first input to settle, in input order on ties.
struct Race(Vec<super::promise::ValueFuture>);
impl std::future::Future for Race {
    type Output = Result<Value, Thrown>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        for f in self.0.iter_mut() {
            if let std::task::Poll::Ready(v) = f.as_mut().poll(cx) {
                return std::task::Poll::Ready(v);
            }
        }
        std::task::Poll::Pending
    }
}

fn install_promise(realm: &mut Realm) {
    let p = realm.intrinsics().promise_proto.clone();
    def(realm, &p, "then", 2, |r, this, args| {
        then(r, this, arg(args, 0), arg(args, 1))
    });
    def(realm, &p, "catch", 1, |r, this, args| {
        then(r, this, Value::Undefined, arg(args, 0))
    });
    def(realm, &p, "finally", 1, |r, this, args| {
        let f = arg(args, 0);
        let source = r.await_value(this.clone());
        let realm = r.clone();
        Ok(r.spawn_value(Box::pin(async move {
            let mut realm = realm;
            let result = source.await;
            if f.is_callable() {
                let out = realm.call(&f, &Value::Undefined, &[])?;
                realm.await_value(out).await?;
            }
            result
        })))
    });
    let promise = constructor(
        realm,
        "Promise",
        1,
        &p,
        None,
        Some(|r, args, _| {
            let executor = arg(args, 0);
            callable(r, &executor)?;
            let (p, cell) = r.new_promise();
            let resolve_cell = cell.clone();
            let resolve = r.native("", 1, move |r, _, a| {
                r.resolve_promise(&resolve_cell, arg(a, 0));
                Ok(Value::Undefined)
            });
            let reject_cell = cell.clone();
            let reject = r.native("", 1, move |_, _, a| {
                settle(&reject_cell, Err(arg(a, 0)));
                Ok(Value::Undefined)
            });
            if let Err(e) = r.call(&executor, &Value::Undefined, &[resolve, reject]) {
                settle(&cell, Err(e));
            }
            Ok(p)
        }),
    );
    let c = promise.object().unwrap().clone();
    def(realm, &c, "resolve", 1, |r, _, args| {
        let v = arg(args, 0);
        if v.promise().is_some() {
            return Ok(v);
        }
        let (p, cell) = r.new_promise();
        r.resolve_promise(&cell, v);
        Ok(p)
    });
    def(realm, &c, "reject", 1, |r, _, args| {
        Ok(r.settled_promise(Err(arg(args, 0))))
    });
    def(realm, &c, "all", 1, |r, _, args| {
        let items = r.collect_iterable(&arg(args, 0))?;
        let futures: Vec<_> = items.into_iter().map(|v| r.await_value(v)).collect();
        let realm = r.clone();
        Ok(r.spawn_value(Box::pin(async move {
            let mut out = vec![];
            for f in futures {
                out.push(f.await?);
            }
            Ok(realm.new_array(out))
        })))
    });
    def(realm, &c, "allSettled", 1, |r, _, args| {
        let items = r.collect_iterable(&arg(args, 0))?;
        let futures: Vec<_> = items.into_iter().map(|v| r.await_value(v)).collect();
        let realm = r.clone();
        Ok(r.spawn_value(Box::pin(async move {
            let mut out = vec![];
            for f in futures {
                out.push(match f.await {
                    Ok(v) => {
                        realm.object_from(vec![("status", Value::str("fulfilled")), ("value", v)])
                    }
                    Err(e) => {
                        realm.object_from(vec![("status", Value::str("rejected")), ("reason", e)])
                    }
                });
            }
            Ok(realm.new_array(out))
        })))
    });
    def(realm, &c, "race", 1, |r, _, args| {
        let items = r.collect_iterable(&arg(args, 0))?;
        let futures: Vec<_> = items.into_iter().map(|v| r.await_value(v)).collect();
        Ok(r.spawn_value(Box::pin(Race(futures))))
    });
    realm.set_global("Promise", promise);
}

fn install_errors(realm: &mut Realm) {
    fn error_construct(
        class: &'static str,
    ) -> fn(&mut Realm, &[Value], &Value) -> Result<Value, Thrown> {
        match class {
            "TypeError" => |r, args, t| make_error(r, "TypeError", args, t),
            "RangeError" => |r, args, t| make_error(r, "RangeError", args, t),
            "SyntaxError" => |r, args, t| make_error(r, "SyntaxError", args, t),
            _ => |r, args, t| make_error(r, "Error", args, t),
        }
    }
    fn make_error(
        r: &mut Realm,
        class: &str,
        args: &[Value],
        target: &Value,
    ) -> Result<Value, Thrown> {
        let fallback = r.error_proto_for(class);
        let proto = r.prototype_of_target(target, &fallback)?;
        let e = r.alloc(Kind::Error, Some(&proto));
        let message = arg(args, 0);
        if !matches!(message, Value::Undefined) {
            let m = r.to_string(&message)?;
            r.define_hidden(&e, PropKey::str("message"), Value::String(m));
        }
        let options = arg(args, 1);
        if options.object().is_some() && r.has_property(&options, &PropKey::str("cause"))? {
            let cause = r.get_str(&options, "cause")?;
            r.define_hidden(&e, PropKey::str("cause"), cause);
        }
        Ok(e)
    }
    let classes = [
        ("Error", realm.intrinsics().error_proto.clone()),
        ("TypeError", realm.intrinsics().type_error_proto.clone()),
        ("RangeError", realm.intrinsics().range_error_proto.clone()),
        ("SyntaxError", realm.intrinsics().syntax_error_proto.clone()),
    ];
    let mut base: Option<Obj> = None;
    for (name, proto) in classes {
        proto
            .borrow_mut()
            .put(PropKey::str("name"), Value::str(name), false);
        proto
            .borrow_mut()
            .put(PropKey::str("message"), Value::str(""), false);
        let construct = error_construct(name);
        let f = realm.function_data(FunctionData {
            name: JsString(name.encode_utf16().collect()),
            length: 1,
            call: Some(Rc::new(move |r: &mut Realm, _: &Value, args: &[Value]| {
                let target = r.global_binding(name)?;
                construct(r, args, &target)
            })),
            construct: Some(Rc::new(construct)),
        });
        def_value(
            f.object().unwrap(),
            "prototype",
            Value::Object(proto.clone()),
        );
        proto
            .borrow_mut()
            .put(PropKey::str("constructor"), f.clone(), false);
        if let Some(base) = &base
            && let Some(o) = f.object()
        {
            o.borrow_mut().proto = Some(base.clone());
        }
        if name == "Error" {
            base = f.object().cloned();
            def(realm, &proto, "toString", 0, |r, this, _| {
                let name = r.get_str(this, "name")?;
                let name = if matches!(name, Value::Undefined) {
                    units("Error")
                } else {
                    r.to_string(&name)?
                };
                let message = r.get_str(this, "message")?;
                let message = if matches!(message, Value::Undefined) {
                    units("")
                } else {
                    r.to_string(&message)?
                };
                Ok(Value::String(match (name.is_empty(), message.is_empty()) {
                    (_, true) => name,
                    (true, false) => message,
                    _ => {
                        let mut out = name.to_vec();
                        out.extend(": ".encode_utf16());
                        out.extend_from_slice(&message);
                        out.into()
                    }
                }))
            });
        }
        realm.set_global(name, f);
    }
}

fn install_misc(realm: &mut Realm) {
    let json = realm.new_object();
    let j = json.object().unwrap().clone();
    def(realm, &j, "parse", 2, |r, _, args| {
        let text = r.to_string(&arg(args, 0))?;
        r.parse_json(&text)
    });
    def(realm, &j, "stringify", 3, |r, _, args| {
        Ok(match r.stringify(&arg(args, 0))? {
            Some(units) => Value::String(units.into()),
            None => Value::Undefined,
        })
    });
    realm.set_global("JSON", json);

    let p = realm.intrinsics().regexp_proto.clone();
    def(realm, &p, "test", 1, |r, this, args| {
        let pattern = this.object().and_then(|o| match &o.borrow().kind {
            Kind::RegExp(p) => Some(p.clone()),
            _ => None,
        });
        let Some(pattern) = pattern else {
            return Err(r.type_error("RegExp.prototype.test called on incompatible receiver"));
        };
        let s = r.to_string(&arg(args, 0))?;
        Ok(Value::Bool(pattern.test(&s)))
    });
    def(realm, &p, "exec", 1, |r, this, args| {
        let pattern = this.object().and_then(|o| match &o.borrow().kind {
            Kind::RegExp(p) => Some(p.clone()),
            _ => None,
        });
        let Some(pattern) = pattern else {
            return Err(r.type_error("RegExp.prototype.exec called on incompatible receiver"));
        };
        let s = r.to_string(&arg(args, 0))?;
        exec_result(r, &pattern, &s)
    });

    let d = realm.intrinsics().date_proto.clone();
    fn time(r: &mut Realm, this: &Value) -> Result<f64, Thrown> {
        match this.object().map(|o| match &o.borrow().kind {
            Kind::Date(t) => Some(*t),
            _ => None,
        }) {
            Some(Some(t)) => Ok(t),
            _ => Err(r.type_error("this is not a Date object.")),
        }
    }
    def(realm, &d, "getTime", 0, |r, this, _| {
        Ok(Value::Number(time(r, this)?))
    });
    def(realm, &d, "valueOf", 0, |r, this, _| {
        Ok(Value::Number(time(r, this)?))
    });
    def(realm, &d, "toISOString", 0, |r, this, _| {
        let t = time(r, this)?;
        if !t.is_finite() {
            return Err(r.error_of("RangeError", "Invalid time value"));
        }
        Ok(Value::str(&iso_string(t)))
    });
    def(realm, &d, "toJSON", 1, |r, this, _| {
        let t = time(r, this)?;
        Ok(if t.is_finite() {
            Value::str(&iso_string(t))
        } else {
            Value::Null
        })
    });
    let date = constructor(
        realm,
        "Date",
        7,
        &d,
        Some(|r, _, _| {
            let t = safe_cex_num::js_time_clip(r.runtime().now_ms());
            Ok(Value::str(&iso_string(t)))
        }),
        Some(|r, args, target| {
            let t = match args.first() {
                None => safe_cex_num::js_time_clip(r.runtime().now_ms()),
                Some(Value::String(u)) => safe_cex_num::js_date_parse_utf16(u, |t| t),
                Some(v) => {
                    let t = r.to_primitive(v, Hint::Default)?;
                    match t {
                        Value::String(u) => safe_cex_num::js_date_parse_utf16(&u, |t| t),
                        other => safe_cex_num::js_time_clip(r.to_number(&other)?),
                    }
                }
            };
            let proto = r.prototype_of_target(target, &r.intrinsics().date_proto.clone())?;
            Ok(r.alloc(Kind::Date(t), Some(&proto)))
        }),
    );
    def(realm, date.object().unwrap(), "now", 0, |r, _, _| {
        Ok(Value::Number(safe_cex_num::js_time_clip(
            r.runtime().now_ms(),
        )))
    });
    realm.set_global("Date", date);

    let realm_copy = realm.clone();
    let globals = realm.state.globals.clone();
    def(&realm_copy, &globals, "parseFloat", 1, parse_float);
    def(&realm_copy, &globals, "parseInt", 2, parse_int);
    def(&realm_copy, &globals, "isNaN", 1, |r, _, args| {
        Ok(Value::Bool(r.to_number(&arg(args, 0))?.is_nan()))
    });
    def(&realm_copy, &globals, "isFinite", 1, |r, _, args| {
        Ok(Value::Bool(r.to_number(&arg(args, 0))?.is_finite()))
    });
    realm.set_global("Infinity", Value::Number(f64::INFINITY));
    realm.set_global("NaN", Value::Number(f64::NAN));
    realm.set_global("undefined", Value::Undefined);
    install_timers(realm);
    install_console(realm);
}

/// setTimeout/clearTimeout on the runtime's monotonic sleep. Handles are numbers.
fn install_timers(realm: &mut Realm) {
    let timers: Rc<RefCell<(f64, Vec<(f64, Rc<std::cell::Cell<bool>>)>)>> = Rc::default();
    let set_timers = timers.clone();
    let set_timeout = realm.native("setTimeout", 2, move |r, _, args| {
        let callback = arg(args, 0);
        let delay = r.to_number(&arg(args, 1))?;
        let delay = if delay.is_finite() && delay > 0.0 {
            delay as u64
        } else {
            0
        };
        let rest: Vec<Value> = args.get(2..).unwrap_or_default().to_vec();
        let cancelled = Rc::new(std::cell::Cell::new(false));
        let id = {
            let mut t = set_timers.borrow_mut();
            t.0 += 1.0;
            let id = t.0;
            t.1.push((id, cancelled.clone()));
            id
        };
        let sleep = r.runtime().sleep(delay);
        let realm = r.clone();
        let timers = set_timers.clone();
        r.spawn(Box::pin(async move {
            let mut realm = realm;
            let _ = sleep.await;
            timers.borrow_mut().1.retain(|(i, _)| *i != id);
            if cancelled.get() || !callback.is_callable() {
                return;
            }
            if let Err(e) = realm.call(&callback, &Value::Undefined, &rest) {
                realm.report_thrown(&e, "setTimeout callback");
            }
        }));
        Ok(Value::Number(id))
    });
    let clear = realm.native("clearTimeout", 1, move |r, _, args| {
        let id = arg(args, 0);
        if let Value::Number(id) = id {
            let _ = r;
            if let Some((_, c)) = timers.borrow().1.iter().find(|(i, _)| *i == id) {
                c.set(true);
            }
        }
        Ok(Value::Undefined)
    });
    realm.set_global("setTimeout", set_timeout);
    realm.set_global("clearTimeout", clear);
}

fn install_console(realm: &mut Realm) {
    let console = realm.new_object();
    let c = console.object().unwrap().clone();
    for (name, severity) in [
        ("log", "info"),
        ("info", "info"),
        ("debug", "debug"),
        ("warn", "warning"),
        ("error", "error"),
    ] {
        let f = realm.native(name, 0, move |r, _, args| {
            let mut parts = vec![];
            for a in args {
                parts.push(r.to_rust_string(a).unwrap_or_else(|_| "[object]".into()));
            }
            r.runtime().console(severity, &parts.join(" "));
            Ok(Value::Undefined)
        });
        c.borrow_mut().put(PropKey::str(name), f, false);
    }
    realm.set_global("console", console);
    let performance = realm.new_object();
    let p = performance.object().unwrap().clone();
    def(realm, &p, "now", 0, |r, _, _| {
        Ok(Value::Number(r.runtime().monotonic_now_ms()))
    });
    realm.set_global("performance", performance);
}

/// Date.prototype.toISOString for a finite time value (proleptic Gregorian, UTC).
pub(crate) fn iso_string(t: f64) -> String {
    let ms = t as i64;
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let (h, m, s, milli) = (
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000,
    );
    // civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", -year)
    } else {
        format!("+{year:06}")
    };
    format!("{year}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}.{milli:03}Z")
}
