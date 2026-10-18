//! The language-capability records the generic class bodies take, over `Js`.
use super::host::{W, frame, has_property, member, shared};
use crate::{
    bootstrap_reads::{BootstrapAwait, BootstrapOps},
    futures_public_class::{ConstructionOps, Constructor},
    order_control::OrderControlOps,
    order_lifecycle::OrderLifecycleOps,
    public_streams::StreamOps,
    spot_bodies::SpotBodyOps,
    spot_private_class::{PrivateClassOps, Task},
};
use safe_cex_exchange::js_runtime::{
    promise::Promise,
    scope::scope_of,
    value::{Js, Thrown, arg},
};
use std::rc::Rc;

fn async_function(weak: &W, name: &str, body: Rc<dyn Fn(Vec<Js>) -> Task<Js, Thrown>>) -> Js {
    let weak = weak.clone();
    Js::function(name, move |_, a| {
        let scope = scope_of(&weak)?;
        Ok(Js::Promise(Promise::spawn(
            &scope.exchange_runtime(),
            body(a),
        )))
    })
}

fn values(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", v.to_display())))
}

fn nth(v: &Js, i: usize) -> Result<Js, Thrown> {
    Ok(values(v)?.get(i).cloned().unwrap_or(Js::Undefined))
}

/// Array iterator record `[array, nextIndex]`; each step re-reads the live array.
fn iterator(v: Js) -> Result<Js, Thrown> {
    values(&v)?;
    Ok(Js::array(vec![v, Js::Number(0.0)]))
}

fn next(it: &Js) -> Result<Option<Js>, Thrown> {
    let array = it.get("0")?;
    let index = it.get("1")?.to_number()? as usize;
    if index >= array.get("length")?.to_number()? as usize {
        return Ok(None);
    }
    it.set("1", Js::Number((index + 1) as f64))?;
    Ok(Some(array.get(&index.to_string())?))
}

fn some(v: Js, f: Rc<dyn Fn(Js) -> Result<bool, Thrown>>) -> Result<bool, Thrown> {
    for row in v.array_items().unwrap_or_default().into_iter().flatten() {
        if f(row)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn control(h: Rc<StreamOps<Js, Thrown>>) -> Rc<OrderControlOps<Js, Thrown>> {
    Rc::new(OrderControlOps {
        value: h,
        pair: Rc::new(|v| Ok((nth(&v, 0)?, nth(&v, 1)?))),
        close_throw: Rc::new(|_, e| e),
        has: Rc::new(has_property),
        boolean: Rc::new(Js::Bool),
        new_error: Rc::new(|ctor, message| {
            Ok(Thrown::new(ctor.call(Js::Undefined, vec![message])?))
        }),
        iterator: Rc::new(iterator),
        next: Rc::new(next),
        some: Rc::new(some),
    })
}

pub(crate) fn lifecycle(h: Rc<StreamOps<Js, Thrown>>) -> Rc<OrderLifecycleOps<Js, Thrown>> {
    Rc::new(OrderLifecycleOps {
        base: control(h),
        member: Rc::new(|v, k| member(&v, k)),
        mapper: Rc::new(|f| Js::function("", move |_, a| f(arg(&a, 0)))),
    })
}

pub(crate) fn bootstrap(weak: &W, receiver: &Js) -> Rc<BootstrapOps<Js, Thrown>> {
    let asynchronous = weak.clone();
    Rc::new(BootstrapOps {
        value: frame(weak, receiver),
        member: Rc::new(|v, k| member(&v, k)),
        raw_filter: Rc::new(|input, f| {
            values(&input)?;
            let mut out = vec![];
            for row in input
                .array_items()
                .unwrap_or_default()
                .into_iter()
                .flatten()
            {
                if f(row.clone())?.truthy() {
                    out.push(row);
                }
            }
            Ok(Js::array(out))
        }),
        boolean: Rc::new(Js::Bool),
        mapper: Rc::new(|f| Js::function("", move |_, a| f(arg(&a, 0)))),
        async_mapper: Rc::new(move |f: Rc<dyn Fn(Js) -> BootstrapAwait<Js, Thrown>>| {
            async_function(&asynchronous, "", Rc::new(move |a| f(arg(&a, 0))))
        }),
        callback: Rc::new(|f| Js::function("", move |_, _| f())),
        response_pair: Rc::new(|v| Ok((nth(&v, 0)?.get("data")?, nth(&v, 1)?.get("data")?))),
        interval_parts: Rc::new(|v| {
            let text = super::host::text(&v)?;
            let start = text.find(|c: char| c.is_ascii_digit());
            let Some(start) = start else {
                return Ok((Js::Undefined, Js::Undefined));
            };
            let end = text[start..]
                .find(|c: char| !c.is_ascii_digit())
                .map_or(text.len(), |i| start + i);
            let unit_end = text[end..]
                .find(|c: char| c.is_ascii_digit())
                .map_or(text.len(), |i| end + i);
            Ok((Js::str(&text[start..end]), Js::str(&text[end..unit_end])))
        }),
        iterator: Rc::new(iterator),
        next: Rc::new(next),
        close_throw: Rc::new(|_, e| e),
        some: Rc::new(some),
    })
}

pub(crate) fn spot_body(weak: &W, receiver: &Js) -> Rc<SpotBodyOps<Js, Thrown>> {
    let asynchronous = weak.clone();
    Rc::new(SpotBodyOps {
        base: bootstrap(weak, receiver),
        async_arrow: Rc::new(move |name, _, body| async_function(&asynchronous, name, body)),
        triple: Rc::new(|v| Ok((nth(&v, 0)?, nth(&v, 1)?, nth(&v, 2)?))),
        has: Rc::new(has_property),
        new_error: Rc::new(|ctor, message| {
            Ok(Thrown::new(ctor.call(Js::Undefined, vec![message])?))
        }),
        comparator: Rc::new(|f| Js::function("", move |_, a| f(arg(&a, 0), arg(&a, 1)))),
        spread_arguments: Rc::new(|v| values(&v)),
        assignment_reference: Rc::new(|object, key| Ok(Js::array(vec![object, key]))),
        assign: Rc::new(|reference, value| {
            let key = reference.get("1")?.to_js_string()?;
            reference.get("0")?.set_key(&key, value)
        }),
    })
}

/// A class value: callable constructor whose `prototype` chains to the base's.
/// The constructor receives the prototype as both active function and newTarget.
pub(crate) fn derive_class(
    name: &str,
    base: Js,
    ctor: Constructor<Js, Thrown>,
) -> Result<Js, Thrown> {
    let proto = Js::object(vec![]);
    let base_proto = base.get("prototype")?;
    if let (Js::Object(o), true) = (&proto, base_proto.is_object()) {
        *o.proto.borrow_mut() = Some(base_proto);
    }
    let superclass = base;
    let own: Rc<std::cell::OnceCell<Js>> = Rc::default();
    let slot = own.clone();
    let class = Js::constructor(name, None, move |target, a| {
        let active = slot
            .get()
            .cloned()
            .ok_or_else(|| Thrown::type_error("class not initialized"))?;
        ctor(active, target, a)
    });
    own.set(class.clone())
        .map_err(|_| Thrown::type_error("class already initialized"))?;
    class.define("prototype", proto)?;
    class.set("__proto__", superclass)?;
    Ok(class)
}

pub(crate) fn super_construct(base: Js, target: Js, args: Vec<Js>) -> Result<Js, Thrown> {
    let receiver = base.construct(args, Some(&target))?;
    match &receiver {
        Js::Object(o) => *o.proto.borrow_mut() = Some(target.get("prototype")?),
        other => {
            return Err(Thrown::type_error(&format!(
                "super constructor returned {}",
                other.to_display()
            )));
        }
    }
    Ok(receiver)
}

pub(crate) fn construction() -> Rc<ConstructionOps<Js, Thrown>> {
    Rc::new(ConstructionOps {
        derive: Rc::new(|name, base, _, ctor| derive_class(name, base, ctor)),
        superclass: Rc::new(|active| active.get("__proto__")),
        super_construct: Rc::new(super_construct),
        construct: Rc::new(|ctor, args| ctor.construct(args, None)),
    })
}

/// The base member a subclass field shadowed, bound to the actual receiver.
pub(crate) fn super_member(weak: &W, home: &Js, receiver: &Js, key: &str) -> Result<Js, Thrown> {
    let shared = shared(weak)?;
    let stashed = shared
        .extra
        .supers
        .borrow()
        .iter()
        .find(|(r, _)| r.same_reference(receiver))
        .map(|(_, s)| s.clone());
    let f = match stashed {
        Some(supers) if supers.has_own(key) => supers.get(key)?,
        _ => {
            let proto = home.get("prototype")?;
            let parent = match &proto {
                Js::Object(o) => o.proto.borrow().clone().unwrap_or(Js::Undefined),
                _ => Js::Undefined,
            };
            parent.get(key)?
        }
    };
    let receiver = receiver.clone();
    Ok(Js::function(key, move |_, a| f.call(receiver.clone(), a)))
}

pub(crate) fn private_class(weak: &W) -> Rc<PrivateClassOps<Js, Thrown>> {
    let frames = weak.clone();
    let orders = weak.clone();
    let asynchronous = weak.clone();
    let supers = weak.clone();
    Rc::new(PrivateClassOps {
        value: frame(weak, &Js::Undefined),
        frame: Rc::new(move |r| frame(&frames, r)),
        orders: Rc::new(move |r| lifecycle(frame(&orders, r))),
        derive: Rc::new(|name, base, init| {
            derive_class(
                name,
                base,
                Rc::new(move |active, target, args| {
                    let receiver = super_construct(active.get("__super")?, target, args)?;
                    init(receiver.clone())?;
                    Ok(receiver)
                }),
            )
        }),
        define: Rc::new(|r, k, v| r.define(k, v)),
        arrow: Rc::new(|name, _, body| Js::function(name, move |_, a| body(a))),
        async_arrow: Rc::new(move |name, _, body| async_function(&asynchronous, name, body)),
        method: Rc::new(|name, _, body| Js::function(name, move |this, a| body(this, a))),
        define_prototype: Rc::new(|class, name, f| class.get("prototype")?.define(name, f)),
        super_member: Rc::new(move |home, r, key| super_member(&supers, home, r, key)),
        template: Rc::new(|v| Ok(v.to_js_string()?.to_vec())),
        string_value: Rc::new(|units| Js::String(units.into())),
        boolean: Rc::new(Js::Bool),
        null: Rc::new(|| Js::Null),
        error: Rc::new(|message| Thrown::error("Error", message)),
    })
}
