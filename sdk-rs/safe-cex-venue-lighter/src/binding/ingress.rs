//! `IngressOps` over js_runtime values: the ported public-ingress bodies are
//! generic in their value type, so they run on `Js` directly.
use super::{
    ops::{Arithmetic, JsIterator, Relation, arithmetic, relation, spread},
    realm::Realm,
};
use crate::public_ingress::IngressOps;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;

fn for_each(value: &Js, mut f: impl FnMut(Js) -> Result<bool, Thrown>) -> Result<(), Thrown> {
    let iterator = JsIterator::open(value)?;
    while let Some(item) = iterator.step()? {
        match f(item) {
            Ok(true) => {}
            Ok(false) => return iterator.close(),
            Err(error) => {
                // The body throw wins over IteratorClose failures.
                let _ = iterator.close();
                return Err(error);
            }
        }
    }
    Ok(())
}

pub(crate) fn ops(realm: &Realm) -> IngressOps<Js, Thrown> {
    let global_realm = realm.clone();
    IngressOps {
        undefined: Rc::new(|| Js::Undefined),
        boolean: Rc::new(Js::Bool),
        number: Rc::new(Js::Number),
        literal: Rc::new(Js::str),
        object: Rc::new(|fields| Js::object(fields)),
        array: Rc::new(Js::array),
        get: Rc::new(|o, k| o.get(k)),
        index: Rc::new(|o, k| o.get_key(&k.to_js_string()?)),
        key: Rc::new(|k| Ok(Js::utf16(&k.to_js_string()?))),
        set: Rc::new(|o, k, v| o.set(k, v)),
        set_index: Rc::new(|o, k, v| o.set_key(&k.to_js_string()?, v)),
        delete: Rc::new(|o, k| {
            o.delete(&k.to_js_string()?)?;
            Ok(true)
        }),
        global: Rc::new(move |name| global_realm.global(name)),
        invoke: Rc::new(|callee, this, args| {
            if !callee.is_callable() {
                return Err(Thrown::type_error(&format!(
                    "{} is not a function",
                    callee.to_display()
                )));
            }
            callee.call(this, args)
        }),
        function: Rc::new(|name, _length, callback| {
            Js::function(name, move |_, args| callback(args))
        }),
        truthy: Rc::new(Js::truthy),
        nullish: Rc::new(Js::is_nullish),
        equal: Rc::new(|a, b| a.strict_equals(b)),
        text: Rc::new(|v| Ok(v.to_js_string()?.to_vec())),
        utf16: Rc::new(|units| Js::utf16(&units)),
        error_value: Rc::new(|e: Thrown| e.value),
        throw: Rc::new(Thrown::new),
        spread: Rc::new(spread),
        tuple_two: Rc::new(|value| {
            let pair = JsIterator::open(value)?;
            let first = pair.step()?.unwrap_or(Js::Undefined);
            let second = pair.step()?.unwrap_or(Js::Undefined);
            pair.close()?;
            Ok((first, second))
        }),
        construct: Rc::new(|callee, args| callee.construct(args, None)),
        is_function: Rc::new(Js::is_callable),
        is_object: Rc::new(Js::is_object),
        for_of: Rc::new(|v, f| {
            for_each(&v, |item| {
                f(item)?;
                Ok(true)
            })
        }),
        less: Rc::new(|a, b| relation(Relation::Less, a, b)),
        greater: Rc::new(|a, b| relation(Relation::Greater, a, b)),
        less_equal: Rc::new(|a, b| relation(Relation::LessOrEqual, a, b)),
        for_of_until: Rc::new(|v, f| for_each(&v, |item| f(item))),
        subtract: Rc::new(|a, b| arithmetic(Arithmetic::Subtract, &a, &b)),
        multiply: Rc::new(|a, b| arithmetic(Arithmetic::Multiply, &a, &b)),
        add_primitive: Rc::new(|a, b| arithmetic(Arithmetic::Add, &a, &b)),
        tdz_error: Rc::new(|name| {
            Thrown::error(
                "ReferenceError",
                &format!("Cannot access '{name}' before initialization"),
            )
        }),
    }
}
