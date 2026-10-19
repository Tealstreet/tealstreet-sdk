//! Store access retains the canonical map, row and callback references.
use super::Host;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_store::Store;
use std::rc::Rc;
pub(crate) fn store_object(_: &Host, store: Rc<dyn Store>) -> Result<Js, Thrown> {
    safe_cex_exchange::native_store::facade(store)
}
