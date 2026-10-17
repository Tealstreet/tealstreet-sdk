use crate::{RestAdapter, TransportError, TransportFuture};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

type ExpiryCallback = Rc<dyn Fn(&str) -> Result<bool, TransportError>>;
type ClockCallback = Rc<dyn Fn(f64)>;
type DomainsCallback = Rc<dyn Fn() -> Vec<String>>;
pub(crate) type JsonParser =
    Rc<dyn Fn(&str) -> Result<safe_cex_types::retained_json::RetainedJsonInput, TransportError>>;

#[derive(Clone)]
pub struct HOWConfig {
    pub get_proxy_token: Rc<dyn Fn() -> TransportFuture<'static, String>>,
    pub is_token_expired: ExpiryCallback,
    pub simulate_http_over_websocket: bool,
    pub sync_server_time: Option<ClockCallback>,
    pub get_unique_latency_domains: Option<DomainsCallback>,
    pub latency_domain_to_exchanges: Option<IndexMap<String, Vec<String>>>,
    pub direct_adapter: Option<Rc<dyn RestAdapter>>,
}
impl Default for HOWConfig {
    fn default() -> Self {
        Self {
            get_proxy_token: Rc::new(|| {
                Box::pin(async {
                    Err(TransportError::Host(
                        "HOW not configured: getProxyToken not set. Call configureHOW() first."
                            .into(),
                    ))
                })
            }),
            is_token_expired: Rc::new(|_| {
                Err(TransportError::Host(
                    "HOW not configured: isTokenExpired not set. Call configureHOW() first.".into(),
                ))
            }),
            simulate_http_over_websocket: false,
            sync_server_time: None,
            get_unique_latency_domains: None,
            latency_domain_to_exchanges: Some(IndexMap::new()),
            direct_adapter: None,
        }
    }
}

#[derive(Default)]
pub struct HOWConfigPatch {
    /// TS property names explicitly assigned undefined by the shallow patch (applied last).
    pub undefined_fields: Vec<String>,
    pub get_proxy_token: Option<Rc<dyn Fn() -> TransportFuture<'static, String>>>,
    pub is_token_expired: Option<ExpiryCallback>,
    pub simulate_http_over_websocket: Option<bool>,
    pub sync_server_time: Option<Option<ClockCallback>>,
    pub get_unique_latency_domains: Option<Option<DomainsCallback>>,
    pub latency_domain_to_exchanges: Option<Option<IndexMap<String, Vec<String>>>>,
    pub direct_adapter: Option<Option<Rc<dyn RestAdapter>>>,
}

struct Configuration {
    value: HOWConfig,
    token_override: bool,
    expiry_override: bool,
}
fn with_configuration<T>(callback: impl FnOnce(&RefCell<Configuration>) -> T) -> T {
    thread_local! {
        static CONFIG: RefCell<Configuration> = RefCell::new(Configuration {
            value: HOWConfig::default(), token_override: false, expiry_override: false,
        });
    }
    CONFIG.with(callback)
}

pub fn configure_how(patch: HOWConfigPatch) {
    with_configuration(|config| {
        let mut config = config.borrow_mut();
        if let Some(value) = patch.get_proxy_token {
            config.value.get_proxy_token = value;
            config.token_override = true;
        }
        if let Some(value) = patch.is_token_expired {
            config.value.is_token_expired = value;
            config.expiry_override = true;
        }
        if let Some(value) = patch.simulate_http_over_websocket {
            config.value.simulate_http_over_websocket = value;
        }
        if let Some(value) = patch.sync_server_time {
            config.value.sync_server_time = value;
        }
        if let Some(value) = patch.get_unique_latency_domains {
            config.value.get_unique_latency_domains = value;
        }
        if let Some(value) = patch.latency_domain_to_exchanges {
            config.value.latency_domain_to_exchanges = value;
        }
        if let Some(value) = patch.direct_adapter {
            config.value.direct_adapter = value;
        }
        for field in patch.undefined_fields {
            match field.as_str() {
                "getProxyToken" => {
                    config.token_override = true;
                    config.value.get_proxy_token = Rc::new(|| {
                        Box::pin(async {
                            Err(TransportError::Host(
                                "config.getProxyToken is not a function".into(),
                            ))
                        })
                    });
                }
                "isTokenExpired" => {
                    config.expiry_override = true;
                    config.value.is_token_expired = Rc::new(|_| {
                        Err(TransportError::Host(
                            "config.isTokenExpired is not a function".into(),
                        ))
                    });
                }
                "simulateHttpOverWebsocket" => config.value.simulate_http_over_websocket = false,
                "syncServerTime" => config.value.sync_server_time = None,
                "getUniqueLatencyDomains" => config.value.get_unique_latency_domains = None,
                "latencyDomainToExchanges" => config.value.latency_domain_to_exchanges = None,
                _ => {}
            }
        }
    });
}

pub fn get_how_config() -> HOWConfig {
    with_configuration(|config| config.borrow().value.clone())
}
pub fn get_proxy_token() -> TransportFuture<'static, String> {
    (get_how_config().get_proxy_token)()
}
pub fn is_token_expired(token: &str) -> Result<bool, TransportError> {
    (get_how_config().is_token_expired)(token)
}
pub fn should_simulate_http_over_websocket() -> bool {
    get_how_config().simulate_http_over_websocket
}
pub fn sync_server_time(time: f64) {
    if let Some(callback) = get_how_config().sync_server_time {
        callback(time);
    }
}
pub fn get_unique_latency_domains() -> Vec<String> {
    get_how_config()
        .get_unique_latency_domains
        .map(|callback| callback())
        .unwrap_or_default()
}
pub fn get_latency_domain_to_exchanges() -> IndexMap<String, Vec<String>> {
    get_how_config()
        .latency_domain_to_exchanges
        .unwrap_or_default()
}

pub(crate) fn configured_proxy_token() -> Option<TransportFuture<'static, String>> {
    let callback = with_configuration(|config| {
        let config = config.borrow();
        config
            .token_override
            .then(|| config.value.get_proxy_token.clone())
    });
    callback.map(|callback| callback())
}
pub(crate) fn configured_token_expired(token: &str) -> Option<Result<bool, TransportError>> {
    let callback = with_configuration(|config| {
        let config = config.borrow();
        config
            .expiry_override
            .then(|| config.value.is_token_expired.clone())
    });
    callback.map(|callback| callback(token))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HOWRegion {
    JA,
    SG,
}

#[derive(Deserialize)]
#[serde(remote = "HOWRegion")]
enum HOWRegionWire {
    JA,
    SG,
}
impl<'de> Deserialize<'de> for HOWRegion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = String::deserialize(d)?;
        HOWRegionWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl From<HOWRegion> for crate::routing::HowEgress {
    fn from(value: HOWRegion) -> Self {
        match value {
            HOWRegion::JA => Self::Jp,
            HOWRegion::SG => Self::Sg,
        }
    }
}

#[derive(Default)]
pub struct CreateAxiosHttpOverWebsocketAdapterOptions {
    pub custom_parser: Option<JsonParser>,
    pub region: Option<HOWRegion>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyEntry {
    pub latency_ms: Value,
    pub source: Value,
    pub cached: Value,
    pub sample_count: Value,
    pub measured_at: Value,
    pub updated_at: u64,
}

#[derive(Deserialize)]
#[serde(remote = "LatencyEntry", rename_all = "camelCase")]
struct LatencyEntryWire {
    latency_ms: Value,
    source: Value,
    cached: Value,
    sample_count: Value,
    measured_at: Value,
    updated_at: u64,
}
impl<'de> Deserialize<'de> for LatencyEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        LatencyEntryWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Waker};

    #[test]
    fn explicit_undefined_patch_resets_required_callbacks_without_host_fallback() {
        configure_how(HOWConfigPatch {
            get_proxy_token: Some(Rc::new(|| Box::pin(async { Ok("token".into()) }))),
            is_token_expired: Some(Rc::new(|_| Ok(false))),
            simulate_http_over_websocket: Some(true),
            undefined_fields: vec![
                "getProxyToken".into(),
                "isTokenExpired".into(),
                "simulateHttpOverWebsocket".into(),
            ],
            ..Default::default()
        });
        assert!(!should_simulate_http_over_websocket());
        assert_eq!(
            is_token_expired("x"),
            Err(TransportError::Host(
                "config.isTokenExpired is not a function".into()
            ))
        );
        let mut token = get_proxy_token();
        assert_eq!(
            token.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            std::task::Poll::Ready(Err(TransportError::Host(
                "config.getProxyToken is not a function".into()
            )))
        );
        with_configuration(|config| {
            assert!(config.borrow().token_override);
            assert!(config.borrow().expiry_override);
        });
    }

    #[test]
    fn defaults_merge_callbacks_and_optional_clears_match_configuration() {
        assert!(!should_simulate_http_over_websocket());
        assert!(get_unique_latency_domains().is_empty());
        assert!(get_latency_domain_to_exchanges().is_empty());
        assert!(
            is_token_expired("token")
                .unwrap_err()
                .to_string()
                .contains("HOW not configured")
        );
        let mut token = get_proxy_token();
        assert!(
            token
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
        let times = Rc::new(RefCell::new(vec![]));
        let observed = times.clone();
        configure_how(HOWConfigPatch {
            get_proxy_token: Some(Rc::new(|| Box::pin(async { Ok("configured".into()) }))),
            is_token_expired: Some(Rc::new(|token| Ok(token == "old"))),
            simulate_http_over_websocket: Some(true),
            get_unique_latency_domains: Some(Some(Rc::new(|| vec!["venue.test".into()]))),
            sync_server_time: Some(Some(Rc::new(move |time| observed.borrow_mut().push(time)))),
            latency_domain_to_exchanges: Some(Some(IndexMap::from([(
                "venue.test".into(),
                vec!["venue".into()],
            )]))),
            ..Default::default()
        });
        configure_how(HOWConfigPatch {
            simulate_http_over_websocket: Some(false),
            ..Default::default()
        });
        assert!(!should_simulate_http_over_websocket());
        assert!(is_token_expired("old").unwrap());
        assert!(!is_token_expired("new").unwrap());
        assert_eq!(get_unique_latency_domains(), ["venue.test"]);
        assert_eq!(get_latency_domain_to_exchanges()["venue.test"], ["venue"]);
        sync_server_time(12.5);
        assert_eq!(*times.borrow(), [12.5]);
        let mut token = get_proxy_token();
        assert_eq!(
            token.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            std::task::Poll::Ready(Ok("configured".into()))
        );
        configure_how(HOWConfigPatch {
            sync_server_time: Some(None),
            get_unique_latency_domains: Some(None),
            latency_domain_to_exchanges: Some(None),
            ..Default::default()
        });
        sync_server_time(20.0);
        assert_eq!(*times.borrow(), [12.5]);
        assert!(get_unique_latency_domains().is_empty());
        assert!(get_latency_domain_to_exchanges().is_empty());
    }
}
