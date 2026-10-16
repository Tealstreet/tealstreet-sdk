//! Concrete JS-value runtime binding the ported Bitget bodies to the Exchange trait.
pub mod exchange;
pub mod getters;
pub mod globals;
pub mod hosts;
pub mod prototype;
pub mod rt;
pub mod targets;
pub use safe_cex_exchange::js_runtime::{builtins, client, promise, sockets, store, value};
