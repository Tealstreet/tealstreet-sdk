use super::{exchange::ApexDependencies, http::ApiBridge, signer::NativeSigner};
use crate::{
    exchange::{ApexHost, ApexOmniExchange as Bodies, ApexRequest},
    lifecycle::{ApexServices, Cleanup},
    ws_private::ApexOmniPrivateWebsocket,
    ws_public::ApexOmniPublicWebsocket,
    zklink_host::{SdkInitialization, ZkLinkHost},
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    js_runtime::{
        globals, library,
        scope::ScopeShared,
        sockets,
        value::{Js, Thrown},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeTask},
    websocket::BaseWebSocket,
};
use safe_cex_num::JsJsonValue as J;
use safe_cex_store::Store;
use safe_cex_types::{Candle, ExchangeOptions, OHLCVOptions, OrderBook};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub(super) type Shared =
    ScopeShared<Option<safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory>>;
pub(super) fn error(e: Thrown) -> ExchangeError {
    let mut native = e.to_exchange_error();
    if e.native.is_some() {
        return native;
    }
    let field = |name: &str| {
        e.value
            .get(name)
            .ok()
            .filter(|v| !v.is_nullish())
            .and_then(|v| safe_cex_exchange::js_runtime::value::to_json(&v).ok())
    };
    native.is_axios_error = e.value.get("isAxiosError").is_ok_and(|v| v.truthy());
    native.response = field("response");
    native.config = field("config");
    native.request = field("request");
    native
}
pub(super) use crate::data::js;
pub(super) struct Host {
    pub scope: Rc<Shared>,
    pub public: Js,
    pub private: Js,
    pub public_body: RefCell<Option<Rc<ApexOmniPublicWebsocket>>>,
    pub private_body: RefCell<Option<Rc<ApexOmniPrivateWebsocket>>>,
    pub clients: RefCell<Vec<Rc<ApiBridge>>>,
    pub next: Cell<u64>,
    pub tasks: Rc<RefCell<Vec<(u64, Box<dyn ExchangeTask>)>>>,
    sdk: NativeSigner,
    initialization: SdkInitialization,
}
impl Host {
    pub fn new(options: ExchangeOptions, store: Rc<dyn Store>, deps: ApexDependencies) -> Rc<Self> {
        let scope = Rc::new(ScopeShared::new(
            deps.runtime,
            deps.http,
            store.clone(),
            Js::object(vec![]),
            deps.external_public_data,
        ));
        globals::install_builtins(&scope.globals, Rc::downgrade(&scope));
        library::install_library(&scope.globals, Rc::downgrade(&scope));
        let parent = Js::object(vec![
            (
                "options",
                safe_cex_exchange::js_runtime::value::from_typed(&options).expect("options"),
            ),
            ("store", safe_cex_exchange::js_runtime::store::facade(store)),
        ]);
        let weak = Rc::downgrade(&scope);
        parent.define_accessor(
            "isDisposed",
            Some(Rc::new(move |_| {
                Ok(Js::Bool(weak.upgrade().is_none_or(|s| {
                    s.base.get().is_some_and(|b| b.is_disposed())
                })))
            })),
            None,
        );
        let public = sockets::base_websocket(&scope.sockets, parent.clone());
        let private = sockets::base_websocket(&scope.sockets, parent);
        Rc::new(Self {
            sdk: NativeSigner::module(),
            scope,
            public,
            private,
            public_body: RefCell::default(),
            private_body: RefCell::default(),
            clients: RefCell::default(),
            next: Cell::new(0),
            tasks: Rc::default(),
            initialization: SdkInitialization::default(),
        })
    }
    pub fn base(&self) -> &Rc<BaseExchange> {
        self.scope
            .base
            .get()
            .expect("base supplied during API construction")
    }
    pub fn core(&self, private: bool) -> Result<Rc<BaseWebSocket>, ExchangeError> {
        sockets::core_of(
            &self.scope.sockets,
            if private { &self.private } else { &self.public },
        )
        .expect("socket core")
        .base()
        .map_err(error)
    }
    pub fn global(&self, name: &str, args: Vec<Js>) -> Result<Js, ExchangeError> {
        self.scope.globals.call_method(name, args).map_err(error)
    }
    pub fn schedule(&self, delay: f64, future: ExchangeFuture<'static, ()>) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        let sleep = self
            .scope
            .runtime
            .sleep(if delay.is_finite() && delay > 0. {
                delay as u64
            } else {
                0
            });
        let runtime = self.scope.runtime.clone();
        let tasks = Rc::downgrade(&self.tasks);
        let task = self.scope.runtime.spawn_local(Box::pin(async move {
            let result = async {
                sleep.await?;
                future.await
            }
            .await;
            if let Some(tasks) = tasks.upgrade() {
                tasks.borrow_mut().retain(|(key, _)| *key != id);
            }
            if let Err(error) = result {
                runtime.report_exception(
                    &error,
                    &serde_json::json!({"operation":"ApexOmni callback"}),
                );
            }
        }));
        self.tasks.borrow_mut().push((id, task));
        id
    }
    pub fn clear(&self, id: u64) {
        let index = { self.tasks.borrow().iter().position(|(key, _)| *key == id) };
        if let Some(index) = index {
            let (_, task) = self.tasks.borrow_mut().remove(index);
            task.cancel();
        }
    }
    pub fn dispose_tasks(&self) {
        for (_, task) in self.tasks.borrow_mut().drain(..) {
            task.cancel();
        }
    }
}
// Bodies retain this weak capability, while Host owns socket bodies. A socket's
// strong parent does not become a host/parent reference cycle.
pub(super) struct HostLink(pub Weak<Host>);
impl HostLink {
    fn owner(&self) -> Result<Rc<Host>, ExchangeError> {
        self.0
            .upgrade()
            .ok_or_else(|| ExchangeError::new("ApexOmni host released"))
    }
}
impl ApexHost for HostLink {
    fn services(&self) -> Result<Rc<dyn ApexServices>, ExchangeError> {
        Ok(self.owner()?)
    }
    fn create_api(
        &self,
        options: &ExchangeOptions,
        base: Rc<BaseExchange>,
        max_rps: Option<u8>,
    ) -> Result<(), ExchangeError> {
        let owner = self.owner()?;
        if owner.scope.base.get().is_none() {
            let _ = owner.scope.base.set(base.clone());
        }
        owner.clients.borrow_mut().push(ApiBridge::new(
            owner.scope.clone(),
            options.clone(),
            base,
            max_rps,
        )?);
        Ok(())
    }
    fn create_websocket(&self, parent: Weak<Bodies>, private: bool) -> Result<(), ExchangeError> {
        let owner = self.owner()?;
        let parent = parent
            .upgrade()
            .ok_or_else(|| ExchangeError::new("ApexOmni parent released"))?;
        let host = Rc::new(super::sockets::SocketHost {
            owner: Rc::downgrade(&owner),
            private,
        });
        if private {
            *owner.private_body.borrow_mut() = Some(ApexOmniPrivateWebsocket::new(parent, host)?);
        } else {
            *owner.public_body.borrow_mut() = Some(ApexOmniPublicWebsocket::new(parent, host)?);
        }
        Ok(())
    }
    fn dispose_websocket(&self, private: bool) -> Result<(), ExchangeError> {
        let owner = self.owner()?;
        if private {
            let body = owner.private_body.borrow_mut().take();
            if let Some(body) = body {
                body.dispose()?;
            }
        } else {
            let body = owner.public_body.borrow_mut().take();
            if let Some(body) = body {
                body.host.dispose()?;
            }
        }
        Ok(())
    }
    fn request(
        &self,
        request: ApexRequest,
    ) -> ExchangeFuture<'_, safe_cex_types::retained_json::RetainedJsonInput> {
        let owner = self.owner();
        Box::pin(async move {
            let owner = owner?;
            let client = owner.clients.borrow()[usize::from(request.unlimited)].clone();
            client.request(request).await
        })
    }
    fn date_parse(&self, text: &str) -> Result<f64, ExchangeError> {
        self.owner()?
            .scope
            .globals
            .get("Date")
            .and_then(|d| d.call_method("parse", vec![Js::str(text)]))
            .and_then(|v| v.to_number())
            .map_err(error)
    }
    fn console(
        &self,
        level: &str,
        message: &str,
        detail: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        let owner = self.owner()?;
        let mut args = vec![Js::str(message)];
        if let Some(detail) = detail {
            args.push(safe_cex_exchange::js_runtime::value::from_retained_input(
                detail.clone(),
            ));
        }
        owner
            .scope
            .globals
            .get("console")
            .and_then(|c| c.call_method(level, args))
            .map_err(error)?;
        Ok(())
    }
}
impl ApexServices for Host {
    fn init_external_public_data(&self) -> ExchangeFuture<'_, bool> {
        let base = self.base().clone();
        let factory = self.scope.extra.clone();
        let owner = base.clone();
        Box::pin(async move {
            base.init_external_public_data(move |config| {
                factory.as_ref().expect("validated provider")(config, owner)
            })
            .await
        })
    }
    fn public_data_only(&self) -> bool {
        safe_cex_exchange::exchange::is_public_data_only_with(
            || self.base().options().public_data_only == Some(true),
            || self.base().can_sign(),
        )
    }
    fn connect_websocket(&self, private: bool) -> Result<(), ExchangeError> {
        if private {
            let body = self
                .private_body
                .borrow()
                .clone()
                .ok_or_else(|| ExchangeError::new("private socket disposed"))?;
            body.connect_and_subscribe()
        } else {
            let body = self
                .public_body
                .borrow()
                .clone()
                .ok_or_else(|| ExchangeError::new("public socket disposed"))?;
            body.connect_and_subscribe(false)
        }
    }
    fn sdk_initialization(&self) -> &SdkInitialization {
        &self.initialization
    }
    fn sdk(&self) -> &dyn ZkLinkHost {
        &self.sdk
    }
    fn new_signer(&self, seed: Option<String>) -> ExchangeFuture<'_, Rc<dyn ZkLinkHost>> {
        Box::pin(async move {
            Ok(Rc::new(NativeSigner::from_seed(seed.as_deref())?) as Rc<dyn ZkLinkHost>)
        })
    }
    fn schedule_tick(
        &self,
        future: ExchangeFuture<'static, ()>,
        interval: Option<f64>,
    ) -> Result<(), ExchangeError> {
        self.schedule(interval.unwrap_or(0.), future);
        Ok(())
    }
    fn date_now_ms(&self) -> f64 {
        self.scope.runtime.now_ms()
    }
    fn throttle_fetch_orders(&self) -> Result<(), ExchangeError> {
        let body = self
            .private_body
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("socket disposed"))?;
        body.throttle_fetch_orders()
    }
    fn place_orders(
        &self,
        orders: Vec<safe_cex_types::PreparedOrderOpts>,
    ) -> ExchangeFuture<'_, Vec<String>> {
        let venue = self
            .scope
            .sockets
            .venue
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade);
        Box::pin(async move {
            let venue = venue.ok_or_else(|| ExchangeError::new("ApexOmni exchange released"))?;
            self.base().assert_caller_client_order_ids(&orders)?;
            let venue = (venue.as_ref() as &dyn std::any::Any)
                .downcast_ref::<super::exchange::ApexOmniExchange>()
                .ok_or_else(|| ExchangeError::new("ApexOmni placement receiver changed type"))?;
            let mut ids = Vec::new();
            for order in orders {
                ids.extend(venue.inner.place_order(order).await?);
            }
            Ok(ids)
        })
    }
    fn dayjs_value_ms(&self, input: Option<f64>) -> Result<f64, ExchangeError> {
        self.global("dayjs", input.map(Js::Number).into_iter().collect())?
            .call_method("valueOf", vec![])
            .and_then(|v| v.to_number())
            .map_err(error)
    }
    fn dayjs_subtract(&self, end: f64, amount: f64, unit: &str) -> Result<f64, ExchangeError> {
        self.global("dayjs", vec![Js::Number(end)])?
            .call_method("subtract", vec![Js::Number(amount), Js::str(unit)])
            .and_then(|d| d.call_method("valueOf", vec![]))
            .and_then(|v| v.to_number())
            .map_err(error)
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        let body = self
            .public_body
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("socket disposed"))?;
        body.listen_ohlcv(opts, callback)
    }
    fn listen_order_book(
        &self,
        symbol: &str,
        callback: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        let body = self
            .public_body
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("socket disposed"))?;
        body.listen_order_book(symbol, callback)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.dispose_tasks();
    }
}
