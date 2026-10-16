//! Concrete JS-value host binding the host-generic KuCoin class bodies to the
//! shared js_runtime, and through it to the typed Exchange trait.
pub(crate) mod futures;
pub(crate) mod globals;
pub(crate) mod host;
pub(crate) mod http;
pub(crate) mod ops;

use safe_cex_exchange::js_runtime::{scope::ScopeShared, value::Js};
use std::cell::RefCell;

pub(crate) type Shared = ScopeShared<Extra>;

/// Venue-only scope state.
pub(crate) struct Extra {
    /// Receiver -> object holding the base-class members a subclass field shadows.
    pub supers: RefCell<Vec<(Js, Js)>>,
    /// JS emitter listener -> core listener id, for `emitter.off(event, fn)`.
    pub listeners: RefCell<Vec<(String, Js, String)>>,
    pub parts: RefCell<Option<safe_cex_exchange::js_runtime::base_exchange::BaseExchangeParts>>,
    pub external: Option<crate::kucoin_exchange::ExternalPublicDataFactory>,
}
