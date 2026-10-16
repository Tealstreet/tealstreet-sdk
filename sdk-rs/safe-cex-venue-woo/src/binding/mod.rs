//! Concrete JS-value runtime binding the ported WOO bodies to the Exchange trait.
pub(crate) mod classes;
pub mod exchange;
pub(crate) mod globals;
pub(crate) mod hosts;
pub(crate) mod ops;
pub(crate) mod rt;
pub(crate) use safe_cex_exchange::js_runtime::{client, promise, store, value};
