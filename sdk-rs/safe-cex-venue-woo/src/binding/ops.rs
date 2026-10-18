//! The value/lifecycle/stream/class operation tables the generic WOO bodies run
//! on, instantiated over js_runtime values. Each table holds the shared scope
//! weakly: the tables live inside that scope's globals and class records.
use super::{
    classes,
    promise::{Promise, await_js},
    rt::{Rt, Shared},
    value::{Js, Thrown},
};
use crate::{
    class_construction::ConstructionOps, exchange_class::ExchangeClassOps,
    legacy_lifecycle::LifecycleOps, public_canonical::CanonicalOps, public_streams::StreamOps,
};
use safe_cex_exchange::js_runtime::{ops, scope::scope_of};
use std::rc::{Rc, Weak};

fn rt(weak: &Weak<Shared>) -> Result<Rt, Thrown> {
    scope_of(weak)
}

fn key(value: &Js) -> Result<Vec<u16>, Thrown> {
    Ok(value.to_js_string()?.to_vec())
}

/// IteratorStep/IteratorValue, with elisions advancing without reading value.
fn step(iterator: &Js, next: &Js, read: bool) -> Result<Option<Js>, Thrown> {
    let result = next.call(iterator.clone(), vec![])?;
    if !result.is_object() {
        return Err(Thrown::type_error("Iterator result is not an object"));
    }
    if result.get("done")?.truthy() {
        return Ok(None);
    }
    Ok(Some(if read {
        result.get("value")?
    } else {
        Js::Undefined
    }))
}

fn close(iterator: &Js) -> Result<(), Thrown> {
    let close = iterator.get("return")?;
    if close.is_nullish() {
        return Ok(());
    }
    let result = close.call(iterator.clone(), vec![])?;
    if !result.is_object() {
        return Err(Thrown::type_error("Iterator result is not an object"));
    }
    Ok(())
}

fn binding(value: Js, pattern: &[bool]) -> Result<Vec<Js>, Thrown> {
    let iterator = value.iterator()?;
    let next = iterator.get("next")?;
    let mut done = false;
    let mut values = Vec::with_capacity(pattern.len());
    for read in pattern {
        values.push(if done {
            Js::Undefined
        } else {
            match step(&iterator, &next, *read)? {
                Some(value) => value,
                None => {
                    done = true;
                    Js::Undefined
                }
            }
        });
    }
    if !done {
        close(&iterator)?;
    }
    Ok(values)
}

fn iterate(value: Js, body: &mut dyn FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
    let iterator = value.iterator()?;
    let next = iterator.get("next")?;
    while let Some(value) = step(&iterator, &next, true)? {
        if let Err(error) = body(value) {
            // The body throw wins over a failure in IteratorClose.
            let _ = close(&iterator);
            return Err(error);
        }
    }
    Ok(())
}

pub(crate) fn canonical(weak: Weak<Shared>) -> Rc<CanonicalOps<Js, Thrown>> {
    let globals = weak.clone();
    Rc::new(CanonicalOps {
        undefined: Rc::new(|| Js::Undefined),
        object: Rc::new(Js::object),
        array: Rc::new(Js::array),
        get: Rc::new(|v, k| v.get(k)),
        global: Rc::new(move |name| rt(&globals)?.global(name)),
        invoke: Rc::new(|f, this, args| {
            if !f.is_callable() {
                return Err(Thrown::type_error(&format!(
                    "{} is not a function",
                    f.to_display()
                )));
            }
            f.call(this, args)
        }),
        function: Rc::new(|name, _arity, body| Js::function(name, move |_, a| body(a))),
        spread: Rc::new(Js::iterable_values),
        truthy: Rc::new(Js::truthy),
        strict_equal: Rc::new(Js::strict_equals),
        string: Rc::new(Js::str),
        number: Rc::new(Js::Number),
        subtract: Rc::new(|a, b| ops::subtract(&a, &b)),
        get_key: Rc::new(|v, k| v.get_key(&key(k)?)),
        error: Rc::new(|message| Thrown::error("Error", message)),
    })
}

pub(crate) type Runtime = Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>;

pub(crate) fn lifecycle(weak: Weak<Shared>, runtime: Runtime) -> Rc<LifecycleOps<Js, Thrown>> {
    let imports = weak.clone();
    Rc::new(LifecycleOps {
        value: canonical(weak),
        await_value: Rc::new(|v| Box::pin(await_js(v))),
        promise: Rc::new(move |task| Js::Promise(Promise::spawn(&runtime, task))),
        import: Rc::new(move |name| rt(&imports)?.global(name)),
        spread_object: Rc::new(|v, fields| {
            let out = v.spread_object()?;
            for (k, field) in fields {
                out.define(k, field)?;
            }
            Ok(out)
        }),
        is_nullish: Rc::new(Js::is_nullish),
        is_undefined: Rc::new(Js::is_undefined),
        boolean: Rc::new(Js::Bool),
        add: Rc::new(|a, b| ops::add(&a, &b)),
        remainder: Rc::new(|a, b| ops::remainder(&a, &b)),
        less: Rc::new(|a, b| ops::less_than(&a, &b)),
        error_value: Rc::new(|e| e.value),
        binding: Rc::new(binding),
        iterate: Rc::new(iterate),
    })
}

pub(crate) fn stream(weak: Weak<Shared>, runtime: Runtime) -> Rc<StreamOps<Js, Thrown>> {
    Rc::new(StreamOps {
        lifecycle: lifecycle(weak, runtime),
        set: Rc::new(|o, k, v| o.set_key(&key(&k)?, v)),
        delete: Rc::new(|o, k| {
            o.delete(&key(&k)?);
            Ok(())
        }),
        divide: Rc::new(|a, b| ops::divide(&a, &b)),
        greater: Rc::new(|a, b| ops::greater_than(&a, &b)),
        template: Rc::new(|parts, values| {
            let mut out: Vec<u16> = vec![];
            for (i, part) in parts.iter().enumerate() {
                out.extend(part.encode_utf16());
                if let Some(value) = values.get(i) {
                    out.extend_from_slice(&value.to_js_string()?);
                }
            }
            Ok(Js::utf16(&out))
        }),
        null: Rc::new(|| Js::Null),
        define: Rc::new(|o, k, v| o.define(k, v)),
    })
}

pub(crate) fn construction(
    weak: Weak<Shared>,
    runtime: Runtime,
) -> Rc<ConstructionOps<Js, Thrown>> {
    let supers = weak.clone();
    Rc::new(ConstructionOps {
        value: lifecycle(weak, runtime),
        super_construct: Rc::new(move |active, target, args| {
            classes::super_construct(&rt(&supers)?, &active, &target, args)
        }),
        construct: Rc::new(|class, args| class.construct(args, None)),
        set: Rc::new(|o, k, v| o.set(k, v)),
    })
}

pub(crate) fn exchange_class(
    weak: Weak<Shared>,
    runtime: Runtime,
) -> Rc<ExchangeClassOps<Js, Thrown>> {
    let derives = weak.clone();
    let members = weak.clone();
    let prototypes = weak.clone();
    Rc::new(ExchangeClassOps {
        construction: construction(weak, runtime),
        derive: Rc::new(move |name, superclass, _arity, constructor, initialize| {
            classes::derive(&rt(&derives)?, name, superclass, constructor, initialize)
        }),
        define: Rc::new(|o, k, v| o.define(k, v)),
        unbound_member: Rc::new(move |receiver, name, _arity, _asynchronous| {
            super::hosts::bind(&rt(&members)?.with_this(receiver.clone()), name)
        }),
        unbound_prototype: Rc::new(move |class, name, _arity, _asynchronous| {
            classes::prototype_member(&rt(&prototypes)?, class, name)
        }),
        regexp: Rc::new(|pattern, flags| {
            let pattern: Vec<u16> = pattern.encode_utf16().collect();
            let flags: Vec<u16> = flags.encode_utf16().collect();
            safe_cex_exchange::js_runtime::regexp::regexp(&pattern, &flags)
        }),
        multiply: Rc::new(|a, b| ops::multiply(&a, &b)),
        null: Rc::new(|| Js::Null),
        is_string: Rc::new(Js::is_string),
    })
}
