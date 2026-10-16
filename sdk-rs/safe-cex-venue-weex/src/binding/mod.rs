//! Bind the existing WEEX native bodies to the shared JS host and typed Exchange.
mod actions;
mod api;
mod classes;
pub mod exchange;
mod reads;
mod rt;
mod sockets;
mod trading;
mod values;
use rt::Host;
use safe_cex_exchange::js_runtime::{
    promise::{JsFuture, await_js},
    value::{Js, Thrown, arg, from_typed, to_typed},
};
