//! `LighterPrivateWebsocket` (lighter.ws-private.ts) assembled on js_runtime from
//! `ws_private_class::LighterPrivateWebsocket::define`, extending the js_runtime
//! `BaseWebSocket` class, plus the module imports its bodies resolve by name.
mod assembly;
mod imports;
mod owner;
mod slots;
#[cfg(test)]
mod tests;

use super::{realm::Realm, value::to_thrown};
use crate::ws_private_class::LighterPrivateWebsocket;
use safe_cex_exchange::js_runtime::value::Thrown;

pub(crate) fn install(realm: &Realm) -> Result<(), Thrown> {
    imports::install(realm)?;
    let assembly = assembly::Assembly::new(realm.clone());
    LighterPrivateWebsocket::define(&assembly).map_err(to_thrown)?;
    Ok(())
}
