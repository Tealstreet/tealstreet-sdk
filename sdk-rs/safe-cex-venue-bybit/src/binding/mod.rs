//! Production binding: a js_runtime receiver over the typed core, on which the
//! ported bodies run, and the `Exchange` methods that dispatch through it.
pub(crate) mod data;
pub(crate) mod exchange;
pub(crate) mod globals;
pub(crate) mod host;
pub(crate) mod http;
pub(crate) mod hyro_socket;
pub(crate) mod members;
pub(crate) mod receiver;
pub(crate) mod rt;
pub(crate) mod session;

pub use exchange::BybitDependencies;
pub(crate) use safe_cex_exchange::js_runtime::{promise, value};

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod delegated_socket_tests;
