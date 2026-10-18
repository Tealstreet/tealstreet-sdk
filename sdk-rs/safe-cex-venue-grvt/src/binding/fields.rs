//! Internal field descriptors alias the existing native state, without shadow cells.
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
use std::rc::Rc;
pub(super) type Read = Rc<dyn Fn() -> Result<Js, Thrown>>;
pub(super) type Write = Rc<dyn Fn(Js) -> Result<(), Thrown>>;
pub(super) fn view(read: Read, write: Write) -> Js {
    Js::object_with_class(
        "GrvtFieldView",
        vec![
            ("read", Js::function("read", move |_, _| read())),
            (
                "write",
                Js::function("write", move |_, args| {
                    write(arg(&args, 0))?;
                    Ok(Js::Undefined)
                }),
            ),
        ],
    )
}
fn is_view(v: &Js) -> bool {
    matches!(v,Js::Object(v) if v.class=="GrvtFieldView")
}
pub(super) fn read(v: Js) -> Result<Js, Thrown> {
    if is_view(&v) {
        v.call_method("read", vec![])
    } else {
        Ok(v)
    }
}
pub(super) fn write(v: &Js, field: Js) -> Result<bool, Thrown> {
    if is_view(v) {
        v.call_method("write", vec![field])?;
        Ok(true)
    } else {
        Ok(false)
    }
}
pub(super) fn readonly(value: Js) -> Js {
    let read = value.clone();
    view(
        Rc::new(move || Ok(read.clone())),
        Rc::new(move |_| {
            Err(Thrown::error(
                "RepresentationError",
                "Untyped GRVT cache replacement is outside the native composition contract",
            ))
        }),
    )
}
pub(super) fn number(get: Rc<dyn Fn() -> f64>, set: Rc<dyn Fn(f64)>) -> Js {
    view(
        Rc::new(move || Ok(Js::Number(get()))),
        Rc::new(move |v| {
            let Js::Number(n) = v else {
                return Err(Thrown::type_error("GRVT state counter requires Number"));
            };
            set(n);
            Ok(())
        }),
    )
}
pub(super) fn boolean(get: Rc<dyn Fn() -> bool>, set: Rc<dyn Fn(bool)>) -> Js {
    view(
        Rc::new(move || Ok(Js::Bool(get()))),
        Rc::new(move |v| {
            set(v.truthy());
            Ok(())
        }),
    )
}
pub(super) type Entries = Rc<dyn Fn() -> Result<Vec<(Js, Js)>, Thrown>>;
/// Read views retain the native cache; source member bodies mutate its typed owner.
/// Untyped replacement/edit of protocol records is the existing composition boundary.
pub(super) fn map(entries: Entries, clear: Rc<dyn Fn()>) -> Js {
    let map = Js::object_with_class("Map", vec![]);
    let read = entries.clone();
    map.define_accessor(
        "size",
        Some(Rc::new(move |_| Ok(Js::Number(read()?.len() as f64)))),
        None,
    )
    .expect("fresh extensible accessor target");
    for name in ["get", "has"] {
        let read = entries.clone();
        map.define(
            name,
            Js::function(name, move |_, args| {
                let key = arg(&args, 0);
                let found = read()?.into_iter().find(|(k, _)| k.same_value_zero(&key));
                Ok(if name == "has" {
                    Js::Bool(found.is_some())
                } else {
                    found.map_or(Js::Undefined, |(_, v)| v)
                })
            }),
        )
        .expect("own data field");
    }
    map.define(
        "clear",
        Js::function("clear", move |_, _| {
            clear();
            Ok(Js::Undefined)
        }),
    )
    .expect("own data field");
    map.define(
        "set",
        Js::function("set", |_, _| {
            Err(Thrown::error(
                "RepresentationError",
                "Untyped GRVT protocol cache writes are outside the native composition contract",
            ))
        }),
    )
    .expect("own data field");
    map
}
pub(super) fn set(values: Rc<dyn Fn() -> Vec<Js>>, clear: Rc<dyn Fn()>) -> Js {
    let set = Js::object_with_class("Set", vec![]);
    let read = values.clone();
    set.define_accessor(
        "size",
        Some(Rc::new(move |_| Ok(Js::Number(read().len() as f64)))),
        None,
    )
    .expect("fresh extensible accessor target");
    set.define(
        "has",
        Js::function("has", move |_, args| {
            Ok(Js::Bool(
                values().iter().any(|v| v.same_value_zero(&arg(&args, 0))),
            ))
        }),
    )
    .expect("own data field");
    set.define(
        "clear",
        Js::function("clear", move |_, _| {
            clear();
            Ok(Js::Undefined)
        }),
    )
    .expect("own data field");
    set
}
