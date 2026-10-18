//! Runtime-owned loader cache; hosts install real venue constructors in the registry.
use crate::options_patch::{
    ExchangeOptionsPatch, apply_extra_patch, apply_patch, empty_extra_options, empty_options,
};
use indexmap::IndexMap;
use safe_cex_store::{Store, store::DefaultStore, virtual_clock::VirtualClock};
use safe_cex_transport::{TransportProvider, TransportRegistry};
use safe_cex_types::{ExchangeName, ExchangeOptions, exchange_metadata::get_exchange_metadata};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

pub type ExchangeConstructor<I> =
    Rc<dyn Fn(ExchangeName, ExchangeOptions, Rc<dyn Store>) -> Result<I, String>>;
pub type ExchangeLoader<I> =
    Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<ExchangeConstructor<I>, String>>>>>;
pub struct ExchangeFactory<I> {
    pub loaders: IndexMap<String, ExchangeLoader<I>>,
    cache: RefCell<IndexMap<String, ExchangeConstructor<I>>>,
    pub clock: VirtualClock,
    pub transport: TransportRegistry,
}
impl<I> ExchangeFactory<I> {
    pub fn new(
        loaders: IndexMap<String, ExchangeLoader<I>>,
        clock: VirtualClock,
        transport: TransportRegistry,
    ) -> Self {
        Self {
            loaders,
            cache: RefCell::new(IndexMap::new()),
            clock,
            transport,
        }
    }
    pub async fn load_exchange(&self, name: &str) -> Result<ExchangeConstructor<I>, String> {
        if let Some(cached) = self.cache.borrow().get(name).cloned() {
            return Ok(cached);
        }
        let loader = self
            .loaders
            .get(name)
            .ok_or_else(|| format!("Unknown exchange: {name}"))?
            .clone();
        let exchange = loader().await?;
        self.cache
            .borrow_mut()
            .insert(name.into(), exchange.clone());
        Ok(exchange)
    }
    pub async fn create_exchange(
        &self,
        name: &str,
        options: ExchangeOptions,
        store: Option<Rc<dyn Store>>,
    ) -> Result<I, String> {
        // Like the TS async call without await, run startup to its first
        // suspension. The clock's host scheduler owns any continuation so
        // neither a pending time request nor this caller's lifetime blocks it.
        let clock = self.clock.clone();
        let mut startup = Box::pin(async move { clock.start().await });
        if startup
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            (self.clock.schedule)(0, startup);
        }
        let constructor = self.load_exchange(name).await?;
        let exchange_name = serde_json::from_value(serde_json::Value::String(name.into()))
            .map_err(|_| format!("Unknown exchange: {name}"))?;
        constructor(
            exchange_name,
            options,
            store.unwrap_or_else(|| Rc::new(DefaultStore::new())),
        )
    }
    pub fn build_public_exchange_options(
        &self,
        name: &str,
        patch: Option<&ExchangeOptionsPatch>,
        force_all_proxies_off: bool,
        instance: Option<&Arc<dyn TransportProvider>>,
    ) -> Result<ExchangeOptions, String> {
        let metadata = get_exchange_metadata(name, false)?;
        let needs_proxy = !force_all_proxies_off
            && (metadata.requires_cors || metadata.enforce_ws_tunnel == Some(true));
        let mut options = empty_options();
        if needs_proxy {
            options.override_proxy_url = self
                .transport
                .get_transport(instance)
                .server_proxy_url(&metadata.exchange_name)
                .filter(|url| !url.is_empty());
        }
        if let Some(patch) = patch {
            apply_patch(patch, &mut options);
            if let Some(extra) = &patch.extra {
                let mut value = empty_extra_options();
                apply_extra_patch(extra, &mut value);
                options.extra = Some(value);
            }
        }
        if force_all_proxies_off {
            options.override_proxy_url = None;
            options.tealstreet_proxy = Some(false);
            options.tealstreet_tunnel = Some(false);
            options.tealstreet_ws_route = Some(false);
        }
        options.public_data_only = Some(true);
        Ok(options)
    }
    pub async fn preload_exchange(&self, name: &str) -> Result<(), String> {
        self.load_exchange(name).await.map(|_| ())
    }
    pub async fn preload_exchanges(&self, names: &[String]) -> Result<(), String> {
        let mut pending: Vec<_> = names
            .iter()
            .map(|name| Some(Box::pin(self.load_exchange(name))))
            .collect();
        poll_fn(|cx| {
            let mut waiting = false;
            for future in &mut pending {
                if let Some(task) = future {
                    match task.as_mut().poll(cx) {
                        Poll::Ready(Ok(_)) => *future = None,
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Pending => waiting = true,
                    }
                }
            }
            if waiting {
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        })
        .await
    }
    pub fn is_exchange_loaded(&self, name: &str) -> bool {
        self.cache.borrow().contains_key(name)
    }
    pub fn get_available_exchanges(&self) -> Vec<String> {
        self.loaders.keys().cloned().collect()
    }
    pub async fn create_public_exchange(
        &self,
        name: &str,
        patch: Option<&ExchangeOptionsPatch>,
        force_all_proxies_off: bool,
        instance: Option<&Arc<dyn TransportProvider>>,
        store: Option<Rc<dyn Store>>,
    ) -> Result<I, String> {
        let options =
            self.build_public_exchange_options(name, patch, force_all_proxies_off, instance)?;
        self.create_exchange(name, options, store).await
    }
}
