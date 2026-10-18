//! Captured ticker dictionaries and rows are the canonical Store referents.
use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown},
    runtime::ExchangeError,
};
use safe_cex_store::Store;
use safe_cex_types::{StoreReadError, StoreReadPhase};
use std::rc::{Rc, Weak};

pub type TickerReference = Js;
pub type Dictionary = Js;
#[derive(Clone)]
pub struct Target(pub Js);

pub(crate) fn native_error(error: Thrown) -> ExchangeError {
    match &error.native {
        Some(native) => (**native).clone(),
        None => StoreReadError::new(StoreReadPhase::Property, Rc::new(error)).into(),
    }
}

pub fn lookup(dictionary: &Dictionary, symbol: &str) -> Result<Option<Target>, ExchangeError> {
    let row = dictionary.get(symbol).map_err(native_error)?;
    Ok(row.truthy().then_some(Target(row)))
}

pub struct Capture {
    store: Weak<dyn Store>,
    facade: Js,
}
impl Capture {
    pub fn new(store: Rc<dyn Store>) -> Result<Rc<Self>, ExchangeError> {
        let facade =
            safe_cex_exchange::native_store::facade(store.clone()).map_err(native_error)?;
        Ok(Rc::new(Self {
            store: Rc::downgrade(&store),
            facade,
        }))
    }
    pub fn belongs_to(&self, store: &Rc<dyn Store>) -> bool {
        self.store
            .upgrade()
            .is_some_and(|original| Rc::ptr_eq(&original, store))
    }
    pub fn dictionary(&self) -> Result<Dictionary, ExchangeError> {
        self.facade.get("tickersBySymbol").map_err(native_error)
    }
}
