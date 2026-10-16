//! WooPublicWebsocket / WooPrivateWebscoket members ported in
//! `crate::legacy_socket_control`, and the two socket class definitions.
use crate::binding::{
    ops::Runtime,
    rt::{Rt, Shared},
    value::{Js, Thrown},
};
use std::rc::Weak;

pub(crate) fn define_socket_classes(rt: &Rt, _weak: Weak<Shared>, _runtime: Runtime) -> Result<(), Thrown> {
    let base = rt.global("BaseWebSocket")?;
    for name in ["WooPublicWebsocket", "WooPrivateWebscoket"] {
        let base = base.clone();
        rt.shared.globals.define(
            name,
            Js::constructor(name, move |_, args| base.call(Js::Undefined, args)),
        )?;
    }
    Ok(())
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
