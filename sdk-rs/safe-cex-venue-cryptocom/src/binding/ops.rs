//! The language operations the ported bodies are written against, implemented
//! over js_runtime values: SocketOps, BindingOps, RuntimeOps and SocketClassOps.
use super::rt::{Class, Rt, Shared, class_of};
use crate::{
    native_bindings::BindingOps, runtime::RuntimeOps, socket_classes::SocketClassOps,
    socket_handlers::SocketOps,
};
use safe_cex_exchange::js_runtime::{
    ops,
    promise::{Promise, await_js},
    regexp,
    scope::scope_of,
    value::{Js, Thrown, arg, utf16},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

fn scope(weak: &Weak<Shared>) -> Result<Rt, Thrown> {
    scope_of(weak)
}

/// `key in value`: own data and accessors, then the prototype chain; arrays and
/// strings answer their indices and `length`; builtins answer their members.
fn has_property(value: &Js, key: &str) -> Result<bool, Thrown> {
    match value {
        Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::BigInt(_) | Js::String(_) => {
            Err(Thrown::type_error(&format!(
                "Cannot use 'in' operator to search for '{key}' in {}",
                value.to_display()
            )))
        }
        Js::Object(o) => {
            if value.has_own(key) {
                return Ok(true);
            }
            match o.proto.borrow().clone() {
                Some(proto) => has_property(&proto, key),
                None => Ok(false),
            }
        }
        Js::Array(_) => Ok(key == "length"
            || value.has_own(key)
            || safe_cex_exchange::js_runtime::builtins::member(value, &utf16(key)).is_some()),
        _ => Ok(value.has_own(key)
            || safe_cex_exchange::js_runtime::builtins::member(value, &utf16(key)).is_some()),
    }
}

/// `[...value]` consumes the shared iterator protocol and UTF-16 code points.
fn spread(value: &Js) -> Result<Vec<Js>, Thrown> {
    value.iterable_values()
}

pub(crate) fn socket_ops(weak: Weak<Shared>) -> Rc<SocketOps<Js, Thrown>> {
    let global = weak.clone();
    Rc::new(SocketOps {
        undefined: Rc::new(|| Js::Undefined),
        boolean: Rc::new(Js::Bool),
        number: Rc::new(Js::Number),
        literal: Rc::new(Js::str),
        object: Rc::new(Js::object),
        array: Rc::new(Js::array),
        get: Rc::new(|v, k| v.get(k)),
        index: Rc::new(|v, k| v.get_key(&k.to_js_string()?)),
        key: Rc::new(|v| Ok(Js::utf16(&v.to_js_string()?))),
        set: Rc::new(|v, k, x| v.set(k, x)),
        set_index: Rc::new(|v, k, x| v.set_key(&k.to_js_string()?, x)),
        delete: Rc::new(|v, k| {
            v.delete(&k.to_js_string()?);
            Ok(true)
        }),
        global: Rc::new(move |name| scope(&global)?.global(name)),
        invoke: Rc::new(|f, this, args| f.call(this, args)),
        function: Rc::new(|name, _length, body| Js::function(name, move |_, a| body(a))),
        truthy: Rc::new(Js::truthy),
        nullish: Rc::new(Js::is_nullish),
        equal: Rc::new(|a, b| a.strict_equals(b)),
        is_number: Rc::new(|v| matches!(v, Js::Number(_))),
        has: Rc::new(has_property),
        numeric: Rc::new(Js::to_number),
        text: Rc::new(|v| Ok(v.to_js_string()?.to_vec())),
        utf16: Rc::new(|units| Js::utf16(&units)),
        error_value: Rc::new(|e| e.value),
        spread: Rc::new(spread),
        spread_object: Rc::new(|v| {
            if v.is_nullish() {
                return Ok(vec![]);
            }
            Ok(v.own_entries()?
                .into_iter()
                .map(|(k, v)| (Js::utf16(&k), v))
                .collect())
        }),
        object_keys: Rc::new(|fields| {
            let out = Js::object(vec![]);
            for (k, v) in fields {
                if let Ok(key) = k.to_js_string() {
                    let _ = out.define_key(&key, v);
                }
            }
            out
        }),
        tuple_two: Rc::new(|value| {
            let record = iterator_record(value.clone())?;
            let first = iterator_next(&record)?.unwrap_or(Js::Undefined);
            let second = iterator_next(&record)?.unwrap_or(Js::Undefined);
            iterator_close(record)?;
            Ok((first, second))
        }),
        await_value: Rc::new(|v| Box::pin(await_js(v))),
    })
}

fn binary(op: &str, left: Js, right: Js) -> Result<Js, Thrown> {
    Ok(match op {
        "+" => ops::add(&left, &right)?,
        "-" => ops::subtract(&left, &right)?,
        "/" => ops::divide(&left, &right)?,
        "%" => ops::remainder(&left, &right)?,
        ">" => Js::Bool(ops::greater_than(&left, &right)?),
        ">=" => Js::Bool(ops::greater_equal(&left, &right)?),
        "<" => Js::Bool(ops::less_than(&left, &right)?),
        "<=" => Js::Bool(ops::less_equal(&left, &right)?),
        "ToNumeric" => ops::to_numeric(&left)?,
        "++" => ops::add(&ops::to_numeric(&left)?, &Js::Number(1.0))?,
        "unary-" => ops::negate(&left)?,
        other => {
            return Err(Thrown::error(
                "Error",
                &format!("Crypto.com binding has no operator {other}"),
            ));
        }
    })
}

/// Source new dispatch uses the class body or canonical native construction.
pub(crate) fn construct(weak: &Weak<Shared>, f: Js, args: Vec<Js>) -> Result<Js, Thrown> {
    let rt = scope(weak)?;
    if let Some(class) = class_of(&rt, &f) {
        return (class.body)(f.clone(), f, args);
    }
    f.construct(args, None)
}

// Retain the actual IteratorRecord: GetIterator and Get(next) happen once,
// while each next call remains interleaved with the source loop body.
fn iterator_record(value: Js) -> Result<Js, Thrown> {
    let iterator = value.iterator()?;
    let next = iterator.get("next")?;
    Ok(Js::object(vec![
        ("iterator", iterator),
        ("next", next),
        ("done", Js::Bool(false)),
    ]))
}

fn iterator_next(record: &Js) -> Result<Option<Js>, Thrown> {
    if record.get("done")?.truthy() {
        return Ok(None);
    }
    let result = (|| {
        let iterator = record.get("iterator")?;
        let step = record.get("next")?.call(iterator, vec![])?;
        if !step.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        if step.get("done")?.truthy() {
            Ok(None)
        } else {
            step.get("value").map(Some)
        }
    })();
    // IteratorStepValue marks the record done on any abrupt completion.
    if !matches!(result, Ok(Some(_))) {
        record.set("done", Js::Bool(true))?;
    }
    result
}

fn iterator_close(record: Js) -> Result<(), Thrown> {
    if record.get("done")?.truthy() {
        return Ok(());
    }
    record.set("done", Js::Bool(true))?;
    let iterator = record.get("iterator")?;
    let method = iterator.get("return")?;
    if method.is_nullish() {
        return Ok(());
    }
    let returned = method.call(iterator, vec![])?;
    if !returned.is_object() {
        return Err(Thrown::type_error(
            "Iterator return result is not an object",
        ));
    }
    Ok(())
}

pub(crate) fn binding_ops(weak: Weak<Shared>) -> Rc<BindingOps<Js, Thrown>> {
    let construct_weak = weak.clone();
    let promise_weak = weak.clone();
    Rc::new(BindingOps {
        null: Rc::new(|| Js::Null),
        is_string: Rc::new(Js::is_string),
        value: socket_ops(weak),
        binary: Rc::new(binary),
        construct: Rc::new(move |f, args| construct(&construct_weak, f, args)),
        throw_value: Rc::new(Thrown::new),
        promise: Rc::new(move |future| match scope(&promise_weak) {
            Ok(rt) => rt.spawn(future),
            Err(e) => Js::Promise(Promise::settled(&noop_runtime(), Err(e))),
        }),
        iterator: Rc::new(iterator_record),
        next: Rc::new(iterator_next),
        close: Rc::new(iterator_close),
        close_throw: Rc::new(|record, error| {
            let _ = iterator_close(record);
            error
        }),
    })
}

/// Only reached after the exchange is gone; a settled rejection needs no scheduling.
fn noop_runtime() -> Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> {
    struct Gone;
    impl safe_cex_exchange::runtime::ExchangeRuntime for Gone {
        fn monotonic_now_ms(&self) -> f64 {
            0.0
        }
        fn now_ms(&self) -> f64 {
            0.0
        }
        fn sleep(&self, _: u64) -> safe_cex_exchange::runtime::ExchangeFuture<'static, ()> {
            Box::pin(async {
                Err(safe_cex_exchange::runtime::ExchangeError::new(
                    "exchange runtime disposed",
                ))
            })
        }
        fn spawn_local(
            &self,
            _: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
        ) -> Box<dyn safe_cex_exchange::runtime::ExchangeTask> {
            struct Never;
            impl safe_cex_exchange::runtime::ExchangeTask for Never {
                fn cancel(&self) {}
            }
            Box::new(Never)
        }
        fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
        fn unique_id(&self) -> String {
            String::new()
        }
        fn is_worker(&self) -> bool {
            false
        }
        fn random(&self) -> f64 {
            0.0
        }
        fn report_exception(
            &self,
            _: &safe_cex_exchange::runtime::ExchangeError,
            _: &serde_json::Value,
        ) {
        }
        fn report_raw_exception(
            &self,
            _: &safe_cex_exchange::js_runtime::value::Js,
            _: &serde_json::Value,
        ) {
        }
        fn execute_worker_request(
            &self,
            _: serde_json::Value,
        ) -> safe_cex_exchange::runtime::ExchangeFuture<'static, serde_json::Value> {
            Box::pin(async {
                Err(safe_cex_exchange::runtime::ExchangeError::new(
                    "exchange runtime disposed",
                ))
            })
        }
    }
    Rc::new(Gone)
}

/// `class Name extends base { constructor(...) { body } }`.
fn derive(
    weak: &Weak<Shared>,
    name: &str,
    base: Js,
    body: super::rt::ClassBody,
) -> Result<Js, Thrown> {
    let rt = scope(weak)?;
    let label = name.to_owned();
    let class = Js::function(name, move |_, _| {
        Err(Thrown::type_error(&format!(
            "Class constructor {label} cannot be invoked without 'new'"
        )))
    });
    let prototype = Js::object(vec![]);
    if let Some(parent) = class_of(&rt, &base)
        && let Js::Object(o) = &prototype
    {
        *o.proto.borrow_mut() = Some(parent.prototype.clone());
    }
    class.define("prototype", prototype.clone())?;
    rt.shared.extra.classes.borrow_mut().push(Rc::new(Class {
        class: class.clone(),
        base,
        body,
        prototype,
        getters: RefCell::default(),
    }));
    Ok(class)
}

/// The derived-class chain from `target` up, nearest first.
fn chain(rt: &Rt, target: &Js) -> Vec<Rc<Class>> {
    let mut out = vec![];
    let mut current = class_of(rt, target);
    while let Some(class) = current {
        current = class_of(rt, &class.base);
        out.push(class);
    }
    out
}

/// `super(...args)` for an intrinsic base: the instance takes `new.target`'s
/// prototype. Getters are installed per instance because the runtime's [[Get]]
/// invokes prototype accessors with the prototype, not the receiver, as `this`.
fn super_construct(weak: &Weak<Shared>, base: Js, target: Js, args: Vec<Js>) -> Result<Js, Thrown> {
    let rt = scope(weak)?;
    if let Some(class) = class_of(&rt, &base) {
        return (class.body)(base, target, args);
    }
    let instance = if rt.global("BaseExchange")?.same_reference(&base) {
        super::globals::base_exchange(&rt, arg(&args, 0), arg(&args, 1), arg(&args, 2))?
    } else if rt.global("BaseWebSocket")?.same_reference(&base) {
        safe_cex_exchange::js_runtime::sockets::base_websocket(&rt.shared.sockets, arg(&args, 0))
    } else {
        return Err(Thrown::type_error(
            "Crypto.com binding: unsupported superclass",
        ));
    };
    let classes = chain(&rt, &target);
    if let (Some(nearest), Js::Object(o)) = (classes.first(), &instance) {
        *o.proto.borrow_mut() = Some(nearest.prototype.clone());
    }
    for class in classes.iter().rev() {
        for (name, getter) in class.getters.borrow().iter() {
            let getter = getter.clone();
            instance.define_accessor(name, Some(Rc::new(move |this| getter(this.clone()))), None);
        }
    }
    Ok(instance)
}

pub(crate) fn runtime_ops(weak: Weak<Shared>) -> Rc<RuntimeOps<Js, Thrown>> {
    let binding = binding_ops(weak.clone());
    let (w1, w2, w3, w4, w5, w6) = (
        weak.clone(),
        weak.clone(),
        weak.clone(),
        weak.clone(),
        weak.clone(),
        weak.clone(),
    );
    Rc::new(RuntimeOps {
        value: binding.value.clone(),
        is_string: Rc::new(Js::is_string),
        regexp: Rc::new(|pattern, flags| regexp::regexp(&utf16(pattern), &utf16(flags))),
        binary: Rc::new(binary),
        construct: Rc::new(move |f, args| construct(&w1, f, args)),
        throw_value: Rc::new(Thrown::new),
        promise: binding.promise.clone(),
        define: Rc::new(|receiver, key, value| receiver.define(key, value)),
        superclass: Rc::new(move |active| {
            let rt = scope(&w2)?;
            class_of(&rt, active)
                .map(|c| c.base.clone())
                .ok_or_else(|| Thrown::type_error("Crypto.com binding: not a derived class"))
        }),
        super_construct: Rc::new(move |base, target, args| {
            super_construct(&w3, base, target, args)
        }),
        derive: Rc::new(move |name, base, _length, body| derive(&w4, name, base, body)),
        prototype_method: Rc::new(move |class, name, _length, method| {
            let rt = scope(&w5)?;
            let class = class_of(&rt, class)
                .ok_or_else(|| Thrown::type_error("Crypto.com binding: not a derived class"))?;
            class
                .prototype
                .define(name, Js::function(name, move |this, a| method(this, a)))
        }),
        prototype_getter: Rc::new(move |class, name, getter| {
            let rt = scope(&w6)?;
            let class = class_of(&rt, class)
                .ok_or_else(|| Thrown::type_error("Crypto.com binding: not a derived class"))?;
            class.getters.borrow_mut().push((name.to_owned(), getter));
            Ok(())
        }),
        binding,
    })
}

pub(crate) fn socket_class_ops(weak: Weak<Shared>) -> Rc<SocketClassOps<Js, Thrown>> {
    let runtime = runtime_ops(weak);
    Rc::new(SocketClassOps {
        body: runtime.value.clone(),
        define: runtime.define.clone(),
        superclass: runtime.superclass.clone(),
        super_construct: runtime.super_construct.clone(),
        construct: runtime.construct.clone(),
        derive: runtime.derive.clone(),
        binding: runtime.binding.clone(),
        promise: runtime.promise.clone(),
    })
}
