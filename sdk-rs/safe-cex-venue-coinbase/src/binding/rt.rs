use super::{
    crypto,
    error::{failure, from_thrown},
    value::{self},
};
use crate::{
    cdp_signing::CdpSigningCache,
    classes::CoinbaseExchange as NativeExchange,
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    public_data::PublicData,
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    js_runtime::{
        base_exchange::ExternalPublicDataFactory, client::JsHttpHost, promise::await_js,
        scope::ScopeShared, value::Js,
    },
    runtime::{ExchangeError, ExchangeRuntime},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions, object_map::JsObjectMap};
use serde_json::Value;
use std::{
    cell::{OnceCell, RefCell},
    future::Future,
    rc::{Rc, Weak},
};

pub(super) struct Extra {
    pub name: ExchangeName,
    pub local_wall_time_to_utc: Rc<dyn Fn(f64) -> f64>,
    pub health: crate::private_health::CoinbasePrivateUserStreamHealth,
    pub options: ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub signing_cache: Rc<CdpSigningCache>,
    pub spawn: CoinbaseStarbasePromiseSpawner,
    pub class: RefCell<Weak<NativeExchange<Host>>>,
    pub clients: RefCell<Vec<(Frame, Js)>>,
    pub public_socket: OnceCell<Js>,
    pub private_socket: OnceCell<Js>,
    pub daily_candles: RefCell<JsObjectMap<PublicData>>,
}

pub(super) type Shared = ScopeShared<Extra>;

#[derive(Clone)]
pub(super) struct Host {
    pub shared: Rc<Shared>,
}

impl Host {
    pub(super) fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        runtime: Rc<dyn WsRuntime>,
        http: Rc<dyn JsHttpHost>,
        external_public_data: Option<ExternalPublicDataFactory>,
        signing_cache: Rc<CdpSigningCache>,
        local_wall_time_to_utc: Rc<dyn Fn(f64) -> f64>,
    ) -> Self {
        let spawn = crypto::spawner(runtime.clone());
        let shared = Rc::new_cyclic(|weak: &Weak<Shared>| {
            let globals = Js::object(vec![]);
            safe_cex_exchange::js_runtime::globals::install_builtins(&globals, weak.clone());
            safe_cex_exchange::js_runtime::globals::install_enums(&globals);
            safe_cex_exchange::js_runtime::library::install_library(&globals, weak.clone());
            ScopeShared::new(
                runtime,
                http,
                store,
                globals,
                Extra {
                    name,
                    options,
                    external_public_data,
                    signing_cache,
                    spawn,
                    local_wall_time_to_utc,
                    health: Default::default(),
                    class: RefCell::default(),
                    clients: RefCell::default(),
                    public_socket: OnceCell::new(),
                    private_socket: OnceCell::new(),
                    daily_candles: RefCell::default(),
                },
            )
        });
        Self { shared }
    }
    pub(super) fn base(&self) -> &Rc<BaseExchange> {
        self.shared
            .base
            .get()
            .expect("Coinbase super constructor runs first")
    }
    pub(super) fn receiver(&self) -> Js {
        self.shared.exchange.borrow().clone()
    }
    pub(super) fn class(&self) -> Result<Rc<NativeExchange<Self>>, PortfolioFailure> {
        self.shared
            .extra
            .class
            .borrow()
            .upgrade()
            .ok_or_else(|| failure(ExchangeError::new("Coinbase class is not assembled")))
    }
    pub(super) fn promise<T: Clone + 'static>(
        &self,
        body: impl Future<Output = Result<T, PortfolioFailure>> + 'static,
    ) -> Promise<Result<T, PortfolioFailure>> {
        Promise::from_future(Box::pin(body), &self.shared.extra.spawn)
    }
    pub(super) fn register_client(&self, client: Js) -> Frame {
        let identity = Frame::Object(Rc::default());
        self.shared
            .extra
            .clients
            .borrow_mut()
            .push((identity.clone(), client));
        identity
    }
    pub(super) fn client(&self, identity: &Frame) -> Result<Js, PortfolioFailure> {
        self.shared
            .extra
            .clients
            .borrow()
            .iter()
            .find(|(candidate, _)| value::same(candidate, identity))
            .map(|(_, client)| client.clone())
            .ok_or_else(|| failure(ExchangeError::new("Unknown Coinbase HTTP client")))
    }
    pub(super) fn xhr(&self, unlimited: bool) -> Result<Js, PortfolioFailure> {
        let class = self.class()?;
        let identity = if unlimited {
            class.unlimited_xhr.borrow().clone()
        } else {
            class.xhr.borrow().clone()
        };
        self.client(&identity.ok_or_else(|| {
            failure(ExchangeError::new(
                "Coinbase HTTP client is not constructed",
            ))
        })?)
    }
    pub(super) fn request(
        &self,
        method: &'static str,
        endpoint: &'static str,
        payload: Option<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Promise<Result<safe_cex_types::retained_json::RetainedJsonInput, PortfolioFailure>> {
        let invoked = self.xhr(false).and_then(|client| {
            let mut args = vec![Js::str(endpoint)];
            if let Some(payload) = payload {
                let value = safe_cex_exchange::js_runtime::value::from_retained_input(payload);
                args.push(if method == "get" {
                    Js::object(vec![("params", value)])
                } else {
                    value
                });
            }
            client.call_method(method, args).map_err(from_thrown)
        });
        let runtime = self.runtime();
        self.promise(async move {
            let response = await_js(&runtime, invoked?).await.map_err(from_thrown)?;
            let data = response.get("data").map_err(from_thrown)?;
            safe_cex_exchange::js_runtime::value::to_retained_input(&data).map_err(from_thrown)
        })
    }
    pub(super) fn emit(&self, message: Option<Value>) -> Result<(), PortfolioFailure> {
        self.receiver()
            .call_method(
                "emitAndLogError",
                vec![
                    message
                        .as_ref()
                        .map(safe_cex_exchange::js_runtime::value::from_json)
                        .unwrap_or(Js::Undefined),
                ],
            )
            .map(|_| ())
            .map_err(from_thrown)
    }
    pub(super) fn options(&self) -> Frame {
        self.class()
            .map(|class| class.options.borrow().clone())
            .unwrap_or_else(|_| {
                Frame::from_json(
                    &serde_json::to_value(&self.shared.extra.options).expect("typed options"),
                )
            })
    }
    pub(super) fn json_error(name: &str, message: &str) -> PortfolioFailure {
        Rc::new(PortfolioThrown::from_value(Some(
            serde_json::json!({"name":name,"message":message}),
        )))
    }
    pub(super) fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.shared.runtime.clone()
    }
    pub(super) fn global(&self, name: &str, args: Vec<Js>) -> Result<Js, PortfolioFailure> {
        safe_cex_exchange::js_runtime::scope::Scope {
            shared: self.shared.clone(),
            this: self.receiver(),
        }
        .global(name)
        .and_then(|function| function.call(Js::Undefined, args))
        .map_err(from_thrown)
    }
    pub(super) fn public_only(&self) -> bool {
        self.class().is_ok_and(|class| class.is_public_data_only())
    }
    pub(super) fn now(&self) -> f64 {
        self.shared.runtime.now_ms()
    }
}
