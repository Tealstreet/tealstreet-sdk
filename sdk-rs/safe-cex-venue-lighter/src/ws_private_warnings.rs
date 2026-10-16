//! Private-stream warning bodies; the websocket owner supplies parent/clock/route
//! operations and the shared class-wide account cooldown map.
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub const GEORESTRICTED_MESSAGE: &str = "Lighter georestrictions detected; connectivity status is degraded. Private updates may rely on read-only/REST fallback from this IP. Use a VPN or Tealstreet WS proxy; account tier changes will not fix this.";
const READONLY_WARNING_COOLDOWN_MS: f64 = 30_000.0;
/// One shared map per source LighterPrivateWebsocket class, across instances.
pub type ReadonlyWarningTimes = Rc<RefCell<HashMap<Vec<u16>, f64>>>;
/// Source parent operations remain bound to their parent receiver. Implementors
/// must preserve original thrown values and route-resolver property access.
pub trait PrivateWarningHost {
    fn now_ms(&self) -> SigningResult<f64>;
    fn network(&self) -> SigningResult<Vec<u16>>;
    fn account_index(&self) -> SigningResult<Vec<u16>>;
    fn rate_limit_guidance(&self) -> SigningResult<Vec<u16>>;
    fn supports_ws_proxy(&self) -> SigningResult<bool>;
    fn ws_route_option(&self) -> SigningResult<Option<bool>>;
    fn testnet_option(&self) -> SigningResult<Option<bool>>;
    /// Evaluate resolveTransportRouting(parent.name, parent.options).wsPrivate.host.
    fn resolve_private_host(&self) -> SigningResult<AccountValue>;
    fn log_warning(&self, message: &[u16]) -> SigningResult<()>;
    /// Emit ('warning', message, { forceToast: true }) on the parent emitter.
    fn emit_warning(&self, message: &[u16], force_toast: bool) -> SigningResult<()>;
}
/// Getter-aware source params; repeated reads intentionally remain separate.
pub trait PrivateStreamWarningParams {
    fn policy_restriction(&self) -> SigningResult<bool>;
    fn rate_limit(&self) -> SigningResult<bool>;
    fn error_message(&self) -> SigningResult<Vec<u16>>;
}
/// Read the current source owner flags after route callbacks return. A host may
/// mutate that owner during a getter/resolver call; do not snapshot these flags.
pub trait PrivateWarningFlags {
    fn readonly_fallback(&self) -> SigningResult<bool>;
    fn readonly_failures(&self) -> SigningResult<f64>;
}
#[derive(Default)]
pub struct PrivateWarningState {
    pub use_readonly_fallback: bool,
    pub readonly_fallback_failures: f64,
    pub rate_limit_first_seen_at: f64,
    pub rate_limit_error_count: f64,
}
impl PrivateWarningFlags for PrivateWarningState {
    fn readonly_fallback(&self) -> SigningResult<bool> {
        Ok(self.use_readonly_fallback)
    }
    fn readonly_failures(&self) -> SigningResult<f64> {
        Ok(self.readonly_fallback_failures)
    }
}
// ts: LighterPrivateWebsocket#getRateLimitWarningMessage
pub fn get_rate_limit_warning_message(host: &dyn PrivateWarningHost) -> SigningResult<Vec<u16>> {
    host.rate_limit_guidance()
}
// ts: LighterPrivateWebsocket#getReadonlyFallbackWarningKey
pub fn get_readonly_fallback_warning_key(host: &dyn PrivateWarningHost) -> SigningResult<Vec<u16>> {
    let mut environment = host.network()?;
    let account_index = host.account_index()?;
    environment.push(b':' as u16);
    environment.extend(account_index);
    Ok(environment)
}
// ts: LighterPrivateWebsocket#emitReadonlyFallbackWarning
pub fn emit_readonly_fallback_warning(
    host: &dyn PrivateWarningHost,
    times: &ReadonlyWarningTimes,
    log_message: bool,
) -> SigningResult<()> {
    let now = host.now_ms()?;
    let warning_key = get_readonly_fallback_warning_key(host)?;
    let last_warning_at = times
        .borrow()
        .get(&warning_key)
        .copied()
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(0.0);
    if now - last_warning_at < READONLY_WARNING_COOLDOWN_MS {
        return Ok(());
    }
    // Set before log/emit; side-effect failure must retain the cooldown update.
    times.borrow_mut().insert(warning_key, now);
    let message: Vec<u16> = GEORESTRICTED_MESSAGE.encode_utf16().collect();
    if log_message {
        host.log_warning(&message)?;
    }
    host.emit_warning(&message, true)
}
// ts: LighterPrivateWebsocket#isWsProxyRouteEnabled
pub fn is_ws_proxy_route_enabled(host: &dyn PrivateWarningHost) -> SigningResult<bool> {
    Ok(host.supports_ws_proxy()?
        && host.ws_route_option()?.unwrap_or(false)
        && !host.testnet_option()?.unwrap_or(false)
        && host.resolve_private_host()?.truthy())
}
// ts: LighterPrivateWebsocket#shouldUseGeorestrictedConnectivityWarning
pub fn should_use_georestricted_connectivity_warning(
    host: &dyn PrivateWarningHost,
    state: &dyn PrivateWarningFlags,
    is_policy_restriction_error: bool,
) -> SigningResult<bool> {
    Ok(!is_ws_proxy_route_enabled(host)?
        && (state.readonly_fallback()?
            || state.readonly_failures()? > 0.0
            || is_policy_restriction_error))
}
// ts: LighterPrivateWebsocket#getPrivateStreamErrorWarningMessage
pub fn get_private_stream_error_warning_message(
    host: &dyn PrivateWarningHost,
    state: &dyn PrivateWarningFlags,
    params: &dyn PrivateStreamWarningParams,
) -> SigningResult<Vec<u16>> {
    if params.policy_restriction()?
        || (params.rate_limit()?
            && should_use_georestricted_connectivity_warning(host, state, false)?)
    {
        return Ok(GEORESTRICTED_MESSAGE.encode_utf16().collect());
    }
    if params.rate_limit()? {
        return get_rate_limit_warning_message(host);
    }
    let prefix = if is_ws_proxy_route_enabled(host)? {
        "Lighter private WS route returned an error: "
    } else {
        "Lighter private websocket returned an error: "
    };
    let mut message: Vec<u16> = prefix.encode_utf16().collect();
    message.extend(params.error_message()?);
    message.extend(". Falling back to REST reconciliation for private updates.".encode_utf16());
    Ok(message)
}
impl PrivateWarningState {
    // ts: LighterPrivateWebsocket#notePrivateStreamRateLimitError
    pub fn note_private_stream_rate_limit_error(&mut self, now: f64) {
        if self.rate_limit_error_count == 0.0 {
            self.rate_limit_first_seen_at = now;
        }
        self.rate_limit_error_count += 1.0;
    }
    // ts: LighterPrivateWebsocket#clearPrivateStreamRateLimitErrorState
    pub fn clear_private_stream_rate_limit_error_state(&mut self) {
        self.rate_limit_first_seen_at = 0.0;
        self.rate_limit_error_count = 0.0;
    }
}
