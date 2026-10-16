//! `LighterPublicWebsocket` (lighter.ws-public.ts) on js_runtime: the ported
//! class assembled over the shared BaseWebSocket, plus the module bindings
//! its bodies resolve that no other installer binds.
mod bindings;
mod class;
mod host;
#[cfg(test)]
mod tests;

use super::{realm::Realm, value::to_thrown};
use crate::ws_public_class::LighterPublicWebsocket;
use safe_cex_exchange::js_runtime::value::Thrown;

pub(crate) fn install(realm: &Realm) -> Result<(), Thrown> {
    bindings::install(realm)?;
    let socket_realm = class::SocketRealm::new(realm.clone());
    LighterPublicWebsocket::define(&*socket_realm).map_err(to_thrown)?;
    Ok(())
}
