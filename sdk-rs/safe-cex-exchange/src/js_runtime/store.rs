//! Facade over the canonical material Store owner; no snapshot fallback.
use super::value::{Js, Thrown};
use safe_cex_store::Store;
use std::rc::Rc;

pub fn facade(store: Rc<dyn Store>) -> Result<Js, Thrown> {
    super::native_store::facade(store)
}
