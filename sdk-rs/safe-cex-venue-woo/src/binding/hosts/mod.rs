//! Each TS member name -> the callable running its ported Rust body on `rt.this`.
use super::{
    rt::{Rt, Shared},
    value::{Js, Thrown},
};
use std::rc::Weak;

pub(crate) mod data_reads;
pub(crate) mod fill_recovery;
pub(crate) mod market_reads;
pub(crate) mod members;
pub(crate) mod metadata;
pub(crate) mod order_control;
pub(crate) mod order_mapping;
pub(crate) mod order_writes;
pub(crate) mod socket_control;

type Binder = fn(&Rt, &str) -> Option<Result<Js, Thrown>>;

const BINDERS: &[Binder] = &[
    market_reads::bind,
    metadata::bind,
    order_control::bind,
    order_writes::bind,
    data_reads::bind,
    fill_recovery::bind,
    members::bind,
];

pub(crate) fn bind(rt: &Rt, name: &'static str) -> Result<Js, Thrown> {
    for binder in BINDERS {
        if let Some(callable) = binder(rt, name) {
            return callable;
        }
    }
    Ok(Js::function(name, move |_, _| {
        Err(Thrown::error(
            "Error",
            &format!("WOO binding has no adapter for WOOXExchange.{name}"),
        ))
    }))
}

/// Imported helpers each host module reads from module scope.
pub(crate) fn register_all(g: &Js, weak: Weak<Shared>) {
    market_reads::register(g, weak.clone());
    metadata::register(g, weak.clone());
    order_control::register(g, weak.clone());
    order_writes::register(g, weak.clone());
    data_reads::register(g, weak.clone());
    fill_recovery::register(g, weak.clone());
    socket_control::register(g, weak.clone());
    members::register(g, weak);
}
