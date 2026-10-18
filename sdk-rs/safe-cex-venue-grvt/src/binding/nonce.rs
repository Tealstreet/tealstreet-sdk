//! The constructor's nonce caches alias the native maps used by reconciliation.
use super::runtime::Host;
use crate::composition::{NonceIds, NonceTimes};
use indexmap::IndexMap;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}
fn view<T: Clone + 'static>(
    map: Rc<RefCell<IndexMap<String, T>>>,
    encode: Rc<dyn Fn(&T) -> Js>,
    decode: Rc<dyn Fn(Js) -> Result<T, Thrown>>,
) -> Js {
    let object = Js::object_with_class("Map", vec![]);
    let read = map.clone();
    object
        .define_accessor(
            "size",
            Some(Rc::new(move |_| Ok(Js::Number(read.borrow().len() as f64)))),
            None,
        )
        .expect("fresh extensible accessor target");
    let read = map.clone();
    let enc = encode.clone();
    object
        .define(
            "get",
            Js::function("get", move |_, args| {
                let key = text(&arg(&args, 0))?;
                Ok(read.borrow().get(&key).map_or(Js::Undefined, enc.as_ref()))
            }),
        )
        .expect("own data field");
    let read = map.clone();
    object
        .define(
            "has",
            Js::function("has", move |_, args| {
                let key = text(&arg(&args, 0))?;
                Ok(Js::Bool(read.borrow().contains_key(&key)))
            }),
        )
        .expect("own data field");
    let write = map.clone();
    object
        .define(
            "set",
            Js::function("set", move |this, args| {
                let key = text(&arg(&args, 0))?;
                let value = decode(arg(&args, 1))?;
                write.borrow_mut().insert(key, value);
                Ok(this)
            }),
        )
        .expect("own data field");
    let write = map.clone();
    object
        .define(
            "delete",
            Js::function("delete", move |_, args| {
                let key = text(&arg(&args, 0))?;
                Ok(Js::Bool(write.borrow_mut().shift_remove(&key).is_some()))
            }),
        )
        .expect("own data field");
    let write = map.clone();
    object
        .define(
            "clear",
            Js::function("clear", move |_, _| {
                write.borrow_mut().clear();
                Ok(Js::Undefined)
            }),
        )
        .expect("own data field");
    object
        .define(
            "keys",
            Js::function("keys", move |_, _| {
                let index = Cell::new(0usize);
                let map = map.clone();
                Ok(Js::object(vec![(
                    "next",
                    Js::function("next", move |_, _| {
                        let key = map.borrow().get_index(index.get()).map(|(k, _)| k.clone());
                        index.set(index.get() + 1);
                        Ok(Js::object(vec![
                            ("done", Js::Bool(key.is_none())),
                            ("value", key.map_or(Js::Undefined, |v| Js::str(&v))),
                        ]))
                    }),
                )]))
            }),
        )
        .expect("own data field");
    object
}
pub(super) fn ids(h: &Host, map: NonceIds) -> Js {
    *h.0.shared.extra.nonce_ids.borrow_mut() = Some(map.clone());
    view(
        map,
        Rc::new(|v| Js::str(v)),
        Rc::new(|v| {
            let Js::String(v) = v else {
                return Err(Thrown::error(
                    "RepresentationError",
                    "Native GRVT nonce map requires string order IDs",
                ));
            };
            Ok(String::from_utf16_lossy(&v))
        }),
    )
}
pub(super) fn times(h: &Host, map: NonceTimes) -> Js {
    *h.0.shared.extra.nonce_times.borrow_mut() = Some(map.clone());
    view(
        map,
        Rc::new(|v| v.map_or(Js::Undefined, Js::Number)),
        Rc::new(|v| {
            if v.is_undefined() {
                Ok(None)
            } else {
                v.as_f64().map(Some).ok_or_else(|| {
                    Thrown::error(
                        "RepresentationError",
                        "Native GRVT nonce times require Number or undefined",
                    )
                })
            }
        }),
    )
}
pub(super) fn record(h: &Host, args: Vec<Js>) -> Result<Js, Thrown> {
    let nonce = text(&arg(&args, 0))?;
    let real = arg(&args, 1);
    let real = if real.truthy() {
        let Js::String(real) = real else {
            return Err(Thrown::error(
                "RepresentationError",
                "Native GRVT nonce map requires string order IDs",
            ));
        };
        Some(String::from_utf16_lossy(&real))
    } else {
        None
    };
    let ids =
        h.0.shared
            .extra
            .nonce_ids
            .borrow()
            .clone()
            .expect("constructor cache");
    let times =
        h.0.shared
            .extra
            .nonce_times
            .borrow()
            .clone()
            .expect("constructor cache");
    crate::snapshots::record_nonce(
        &mut times.borrow_mut(),
        &mut ids.borrow_mut(),
        &nonce,
        real.as_deref(),
        arg(&args, 2).as_f64(),
    );
    Ok(Js::Undefined)
}
