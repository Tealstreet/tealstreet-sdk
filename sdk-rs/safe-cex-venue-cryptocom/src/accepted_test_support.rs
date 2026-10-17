//! Shared native test operations. Unused capabilities fail closed.
use crate::{native_bindings::BindingOps, socket_handlers::SocketOps};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;
pub(crate) fn socket(globals: Js) -> SocketOps<Js, Thrown> {
    SocketOps {
        undefined: Rc::new(|| Js::Undefined),
        boolean: Rc::new(Js::Bool),
        number: Rc::new(Js::Number),
        literal: Rc::new(Js::str),
        object: Rc::new(Js::object),
        array: Rc::new(Js::array),
        get: Rc::new(|v, k| v.get(k)),
        index: Rc::new(|_, _| panic!("unused index")),
        key: Rc::new(|_| panic!("unused key")),
        set: Rc::new(|_, _, _| panic!("unused set")),
        set_index: Rc::new(|_, _, _| panic!("unused set_index")),
        delete: Rc::new(|_, _| panic!("unused delete")),
        global: Rc::new(move |k| globals.get(k)),
        invoke: Rc::new(|f, this, args| f.call(this, args)),
        function: Rc::new(|_, _, _| panic!("unused function")),
        truthy: Rc::new(Js::truthy),
        nullish: Rc::new(Js::is_nullish),
        equal: Rc::new(|a, b| a.strict_equals(b)),
        is_number: Rc::new(|_| panic!("unused is_number")),
        has: Rc::new(|_, _| panic!("unused has")),
        numeric: Rc::new(|_| panic!("unused numeric")),
        text: Rc::new(|v| Ok(v.to_js_string()?.to_vec())),
        utf16: Rc::new(|v| Js::utf16(&v)),
        error_value: Rc::new(|e| e.value),
        spread: Rc::new(|_| panic!("unused spread")),
        spread_object: Rc::new(|_| panic!("unused spread_object")),
        object_keys: Rc::new(|_| panic!("unused object_keys")),
        tuple_two: Rc::new(|_| panic!("unused tuple_two")),
        await_value: Rc::new(|_| panic!("unused await_value")),
    }
}

pub(crate) fn binding(globals: Js) -> BindingOps<Js, Thrown> {
    BindingOps {
        null: Rc::new(|| Js::Null),
        is_string: Rc::new(Js::is_string),
        value: Rc::new(socket(globals)),
        binary: Rc::new(|_, _, _| panic!("unused binary")),
        construct: Rc::new(|_, _| panic!("unused construct")),
        throw_value: Rc::new(Thrown::new),
        promise: Rc::new(|_| panic!("unused promise")),
        iterator: Rc::new(|_| panic!("unused iterator")),
        next: Rc::new(|_| panic!("unused next")),
        close: Rc::new(|_| panic!("unused close")),
        close_throw: Rc::new(|_, _| panic!("unused close_throw")),
    }
}
