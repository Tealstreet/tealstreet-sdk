//! Deferred access to the same captured native ticker-symbol lookup value.
use safe_cex_types::StoreReadError;

pub trait TickerLookup {
    fn is_truthy(&self) -> bool;
    fn update_id(&self) -> Result<Option<String>, StoreReadError>;
}
