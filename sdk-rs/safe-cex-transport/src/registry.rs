use crate::{DirectTransport, ResolvedRouting, RoutingOptions, TransportProvider};
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, sync::Arc};

// One registry per runtime realm, separate from serializable account options.
#[derive(Default)]
pub struct TransportRegistry {
    default: RefCell<Option<Arc<dyn TransportProvider>>>,
}
impl TransportRegistry {
    pub fn set_default_transport(&self, provider: Arc<dyn TransportProvider>) {
        *self.default.borrow_mut() = Some(provider);
    }
    pub fn get_transport(
        &self,
        instance: Option<&Arc<dyn TransportProvider>>,
    ) -> Arc<dyn TransportProvider> {
        instance
            .cloned()
            .or_else(|| self.default.borrow().clone())
            .unwrap_or_else(|| Arc::new(DirectTransport))
    }
    pub fn with_public_cors_proxy(
        &self,
        url: &str,
        instance: Option<&Arc<dyn TransportProvider>>,
    ) -> String {
        match self.get_transport(instance).public_cors_proxy() {
            Some(proxy) if !proxy.is_empty() => format!("{proxy}/{url}"),
            _ => url.to_owned(),
        }
    }
    pub fn resolve_transport_routing(
        &self,
        exchange: &ExchangeName,
        options: &RoutingOptions,
        instance: Option<&Arc<dyn TransportProvider>>,
    ) -> ResolvedRouting {
        self.get_transport(instance).ws_routing(exchange, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PrivateProxy, RestAdapter, RestAdapterContext, RestTransport, RestTransportContext,
        TransportError, WsConnection,
    };
    use safe_cex_types::ExchangeOptions;

    struct FakeProvider {
        name: &'static str,
        cors: &'static str,
    }
    impl TransportProvider for FakeProvider {
        fn name(&self) -> &str {
            self.name
        }
        fn public_cors_proxy(&self) -> Option<String> {
            Some(self.cors.into())
        }
        fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
            DirectTransport.rest(context)
        }
        fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
            DirectTransport.rest_adapter(context)
        }
        fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String {
            DirectTransport.proxy_prefix(context)
        }
        fn private_proxy(&self, exchange: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
            DirectTransport.private_proxy(exchange)
        }
        fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
            options
        }
        fn server_proxy_url(&self, exchange: &ExchangeName) -> Option<String> {
            DirectTransport.server_proxy_url(exchange)
        }
        fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
            DirectTransport.ws_routing(exchange, options)
        }
        fn ws_connection(&self, url: &str) -> Option<WsConnection> {
            DirectTransport.ws_connection(url)
        }
    }

    #[test]
    fn defaults_are_lazy_and_instance_override_wins() {
        let registry = TransportRegistry::default();
        assert_eq!(registry.get_transport(None).name(), "direct");
        let registered: Arc<dyn TransportProvider> = Arc::new(FakeProvider {
            name: "registered",
            cors: "https://cors",
        });
        let instance: Arc<dyn TransportProvider> = Arc::new(FakeProvider {
            name: "instance",
            cors: "",
        });
        registry.set_default_transport(registered.clone());
        assert!(Arc::ptr_eq(&registry.get_transport(None), &registered));
        assert!(Arc::ptr_eq(
            &registry.get_transport(Some(&instance)),
            &instance
        ));
        assert_eq!(
            registry.with_public_cors_proxy("https://venue", None),
            "https://cors/https://venue"
        );
        assert_eq!(
            registry.with_public_cors_proxy("https://venue", Some(&instance)),
            "https://venue"
        );
        registry.set_default_transport(instance);
        assert_eq!(registry.get_transport(None).name(), "instance");
        assert_eq!(
            registry
                .resolve_transport_routing(&ExchangeName::Grvt, &RoutingOptions::default(), None)
                .ws_private,
            crate::ResolvedWs::default()
        );
    }

    #[test]
    fn missed_registration_refuses_before_fake_venue_receives_anything() {
        struct FakeVenue {
            received: Vec<crate::RestRequest>,
        }
        impl FakeVenue {
            fn send(
                &mut self,
                provider: &dyn TransportProvider,
                exchange: &ExchangeName,
                options: &ExchangeOptions,
                request: &crate::RestRequest,
            ) -> Result<(), TransportError> {
                provider.rest(RestTransportContext {
                    exchange,
                    options,
                    config: request,
                })?;
                self.received.push(request.clone());
                Ok(())
            }
        }
        let mut venue = FakeVenue { received: vec![] };
        let registry = TransportRegistry::default();
        let provider = registry.get_transport(None);
        let request = crate::RestRequest {
            method: "POST".into(),
            url: "/order".into(),
            ..Default::default()
        };
        for flag in ["tealstreetProxy", "tealstreetTunnel"] {
            let options: ExchangeOptions =
                serde_json::from_value(serde_json::json!({"key":"k", "secret":"s", flag:true}))
                    .unwrap();
            assert!(matches!(
                venue.send(provider.as_ref(), &ExchangeName::Bybit, &options, &request),
                Err(TransportError::DirectRoutingRequested(_))
            ));
            assert!(venue.received.is_empty());
        }
        let options: ExchangeOptions =
            serde_json::from_value(serde_json::json!({"key":"k", "secret":"s"})).unwrap();
        venue
            .send(provider.as_ref(), &ExchangeName::Bybit, &options, &request)
            .unwrap();
        assert_eq!(venue.received, vec![request]);
    }
}
