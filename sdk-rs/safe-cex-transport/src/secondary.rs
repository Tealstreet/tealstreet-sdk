use crate::{RestAdapter, RestRequest, RestResponse, TransportError, TransportProvider};
use safe_cex_types::ExchangeOptions;
use std::sync::Arc;

pub type RateLimitWrapper<'a> = dyn Fn(Arc<dyn RestAdapter>) -> Arc<dyn RestAdapter> + 'a;

/// Secondary execution is exposed only through `get`.
/// ```compile_fail
/// use safe_cex_transport::{RestRequest, secondary::SecondaryKeyClients};
/// fn write_with_read_key(clients: &SecondaryKeyClients) {
///     let _ = clients.secondary.execute(RestRequest { method: "POST".into(), ..Default::default() });
/// }
/// ```
pub struct SecondaryKeyClients {
    pub primary: Arc<dyn RestAdapter>,
    secondary: Arc<dyn RestAdapter>,
}
pub fn create_api_with_secondary_key(
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    proxy_url: Option<&str>,
    create: impl Fn(&ExchangeOptions) -> Result<Arc<dyn RestAdapter>, TransportError>,
    rate_limit: Option<&RateLimitWrapper<'_>>,
    secondary_rate_limit: Option<&RateLimitWrapper<'_>>,
) -> Result<SecondaryKeyClients, TransportError> {
    let proxy = secondary_proxy(options, provider, proxy_url);
    let primary = create(options)?;
    let primary = rate_limit.map_or_else(|| primary.clone(), |limit| limit(primary.clone()));
    if !has_secondary_key(options) {
        return Ok(SecondaryKeyClients {
            secondary: primary.clone(),
            primary,
        });
    }
    let secondary_options = apply_secondary_options(options, proxy);
    let secondary = create(&secondary_options)?;
    let secondary =
        secondary_rate_limit.map_or_else(|| secondary.clone(), |limit| limit(secondary.clone()));
    Ok(SecondaryKeyClients { primary, secondary })
}
fn has_secondary_key(options: &ExchangeOptions) -> bool {
    options
        .secondary_key
        .as_deref()
        .is_some_and(|key| !key.is_empty())
}

fn secondary_proxy(
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    proxy_url: Option<&str>,
) -> Option<String> {
    options
        .secondary_proxy_url
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(proxy_url.filter(|s| !s.is_empty()))
        .map(str::to_owned)
        .or_else(|| provider.public_cors_proxy())
}

fn apply_secondary_options(options: &ExchangeOptions, proxy: Option<String>) -> ExchangeOptions {
    // TS spreads the original options, preserving native numbers and nested data.
    // These are the complete secondary_* fields in the typed options carrier;
    // None means absent, while an explicitly present empty string still overrides.
    let mut secondary = options.clone();
    if let Some(key) = &options.secondary_key {
        secondary.key.clone_from(key);
    }
    if let Some(secret) = &options.secondary_secret {
        secondary.secret.clone_from(secret);
    }
    if let Some(passphrase) = &options.secondary_passphrase {
        secondary.passphrase = Some(passphrase.clone());
    }
    secondary.override_proxy_url = proxy;
    secondary
}

impl SecondaryKeyClients {
    pub fn shares_primary_adapter(&self) -> bool {
        Arc::ptr_eq(&self.primary, &self.secondary)
    }

    pub fn get(&self, mut request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        request.method = "get".into();
        self.secondary.execute(request)
    }
    pub fn get_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> crate::RestFuture<'_, crate::RestExecution> {
        request.method = "get".into();
        self.secondary.execute_with_receipt(request)
    }
    pub fn request_with_receipt(
        &self,
        request: RestRequest,
    ) -> crate::RestFuture<'_, crate::RestExecution> {
        self.primary.execute_with_receipt(request)
    }
    // Generic request calls, HEAD, and writes remain primary, matching the TS get-only Proxy.
    pub fn request(&self, request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        self.primary.execute(request)
    }
}
