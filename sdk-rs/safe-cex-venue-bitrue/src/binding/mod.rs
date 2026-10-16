//! Concrete host for the existing Bitrue bodies over the shared JS runtime.
mod classes;
mod exchange;
mod host;
mod private;
mod sockets;

pub use exchange::{BitrueDependencies, BitrueExchange};
use safe_cex_exchange::js_runtime::{
    scope::{Scope, ScopeShared},
    value::Js,
};
use std::cell::RefCell;

pub(super) type Rt = Scope<Extra>;
pub(super) type Shared = ScopeShared<Extra>;
pub(super) struct Extra {
    pub parts: RefCell<Option<safe_cex_exchange::js_runtime::base_exchange::BaseExchangeParts>>,
    pub supers: RefCell<Vec<(Js, Js)>>,
    pub metadata: RefCell<Vec<(String, crate::position_metadata::PositionMetadataHandle)>>,
}
