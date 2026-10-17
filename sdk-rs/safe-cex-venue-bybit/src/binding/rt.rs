//! Bybit's js_runtime scope: the shared host handle plus the typed venue it bridges.
use super::value::Thrown;
use crate::exchange::BybitExchange;
use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};
use std::rc::{Rc, Weak};

pub(crate) struct BybitExtra {
    pub venue: std::cell::RefCell<Weak<BybitExchange>>,
    pub private_protocol: std::cell::RefCell<Weak<crate::ws_private::protocol::PrivateProtocol>>,
}

pub(crate) type Shared = ScopeShared<BybitExtra>;
pub(crate) type Rt = Scope<BybitExtra>;

pub(crate) fn venue(rt: &Rt) -> Result<Rc<BybitExchange>, Thrown> {
    rt.shared
        .extra
        .venue
        .borrow()
        .upgrade()
        .ok_or_else(|| Thrown::error("Error", "Bybit exchange disposed"))
}
