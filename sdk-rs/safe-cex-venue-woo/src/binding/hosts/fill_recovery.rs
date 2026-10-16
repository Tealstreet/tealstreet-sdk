//! WOOXExchange members ported in `crate::legacy_fill_recovery`, over js_runtime values.
use crate::binding::{
    rt::{Rt, Shared},
    value::{Js, Thrown},
};
use std::rc::Weak;

pub(crate) fn bind(_rt: &Rt, _name: &str) -> Option<Result<Js, Thrown>> {
    None
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
