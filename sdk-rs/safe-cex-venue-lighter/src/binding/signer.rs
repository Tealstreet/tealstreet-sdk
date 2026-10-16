//! Wave-2 global: see the owning manifest rows for what this file assembles.
use super::realm::Realm;
use safe_cex_exchange::js_runtime::value::Thrown;

pub(crate) fn install(realm: &Realm) -> Result<(), Thrown> {
    let _ = realm;
    Ok(())
}
