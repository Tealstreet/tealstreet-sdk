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
    let secondary_options = apply_secondary_options(options, proxy)?;
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

fn apply_secondary_options(
    options: &ExchangeOptions,
    proxy: Option<String>,
) -> Result<ExchangeOptions, TransportError> {
    // TS copies every secondary_* property, including explicitly present empty strings.
    let mut value =
        serde_json::to_value(options).map_err(|error| TransportError::Host(error.to_string()))?;
    let object = value.as_object_mut().ok_or_else(|| {
        TransportError::Host("ExchangeOptions must serialize as an object".into())
    })?;
    let overrides: Vec<_> = object
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("secondary_")
                .map(|key| (key.to_owned(), value.clone()))
        })
        .collect();
    object.extend(overrides);
    if let Some(proxy) = proxy {
        object.insert("overrideProxyUrl".into(), proxy.into());
    } else {
        object.remove("overrideProxyUrl");
    }
    serde_json::from_value(value).map_err(|error| TransportError::Host(error.to_string()))
}

impl SecondaryKeyClients {
    pub fn shares_primary_adapter(&self) -> bool {
        Arc::ptr_eq(&self.primary, &self.secondary)
    }

    pub fn get(&self, mut request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        request.method = "get".into();
        self.secondary.execute(request)
    }
    // Generic request calls, HEAD, and writes remain primary, matching the TS get-only Proxy.
    pub fn request(&self, request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        self.primary.execute(request)
    }
}
