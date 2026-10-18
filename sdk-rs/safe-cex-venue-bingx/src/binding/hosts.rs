use super::exchange::BingxDependencies;
use crate::{
    data_reads::ReadTransport,
    exchange::{ApiFactory, BingxExchange as Bodies, ExchangeHost},
    placement::PlacementHost,
    read_history::HistoryHost,
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
use safe_cex_store::Store;
use safe_cex_types::ExchangeOptions;
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
    if let Some(response) = native.response.as_mut()
        && let Some(serde_json::Value::String(raw)) = response.get("data")
    {
        response["data"] = safe_cex_exchange::runtime::response_json_view(
            &safe_cex_transport::RestResponseBody::Transformed(
                safe_cex_transport::how::big_int_as_string_json_parse(raw),
            ),
        );
    }
    native.config = field("config");
    native.request = field("request");
    // The common thrown-error projection already retains code as a native value.
    native
}
pub(super) struct Host {
    pub scope: Rc<Shared>,
    pub local_wall_time_to_utc: Rc<dyn Fn(f64) -> f64>,
    pub inner: RefCell<Weak<Bodies>>,
    pub public: Js,
    pub private: Js,
    pub public_close: Js,
    pub private_close: Js,
    pub clients: RefCell<Vec<super::http::ClientEntry>>,
    pub next: Rc<Cell<u64>>,
    pub blobs: Rc<RefCell<std::collections::HashMap<u64, Js>>>,
    pub tasks: Rc<RefCell<Vec<(u64, Box<dyn ExchangeTask>)>>>,
    pub private_listeners: RefCell<Vec<(&'static str, Js)>>,
}
impl Host {
    pub fn new(
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BingxDependencies,
    ) -> Rc<Self> {
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
                safe_cex_exchange::js_runtime::value::from_typed(&options)
                    .expect("serializable options"),
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
            public_close: public.get("onClose").expect("base close"),
            private_close: private.get("onClose").expect("base close"),
            public,
            private,
            scope,
            local_wall_time_to_utc: deps.local_wall_time_to_utc,
            inner: RefCell::default(),
            clients: RefCell::default(),
            next: Rc::new(Cell::new(0)),
            blobs: Rc::default(),
            tasks: Rc::default(),
            private_listeners: RefCell::default(),
        })
    }
    pub fn base(&self) -> &Rc<BaseExchange> {
        self.scope
            .base
            .get()
            .expect("API creation supplies the exchange base")
    }
    pub fn disposed(&self) -> bool {
        self.scope.base.get().is_some_and(|b| b.is_disposed())
    }
    pub fn global(&self, name: &str, args: Vec<Js>) -> Result<Js, ExchangeError> {
        self.scope.globals.call_method(name, args).map_err(error)
    }
    pub fn core(&self, private: bool) -> Result<Rc<BaseWebSocket>, ExchangeError> {
        sockets::core_of(
            &self.scope.sockets,
            if private { &self.private } else { &self.public },
        )
        .expect("socket facade has a core")
        .base()
        .map_err(error)
    }
    pub fn cores(&self, private: bool) -> Vec<Rc<BaseWebSocket>> {
        sockets::core_of(
            &self.scope.sockets,
            if private { &self.private } else { &self.public },
        )
        .and_then(|c| c.base.get().cloned())
        .into_iter()
        .collect()
    }
    pub fn timer(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<u64, ExchangeError> {
        let id = self.next.get() + 1;
        self.next.set(id);
        let sleep = self
            .scope
            .runtime
            .sleep(if delay.is_finite() && delay > 0.0 {
                delay as u64
            } else {
                0
            });
        let runtime = self.scope.runtime.clone();
        let tasks = Rc::downgrade(&self.tasks);
        let task = self.scope.runtime.spawn_local(Box::pin(async move {
            let slept = sleep.await;
            if let Some(tasks) = tasks.upgrade() {
                tasks.borrow_mut().retain(|(key, _)| *key != id);
            }
            let result = slept.and_then(|_| callback());
            if let Err(e) = result {
                runtime
                    .report_exception(&e, &serde_json::json!({"operation":"BingX timer"}).into());
            }
        }));
        self.tasks.borrow_mut().push((id, task));
        Ok(id)
    }
    pub fn clear_timer(&self, id: u64) {
        let index = { self.tasks.borrow().iter().position(|(key, _)| *key == id) };
        if let Some(i) = index {
            let (_, task) = self.tasks.borrow_mut().remove(i);
            task.cancel();
        }
    }
    pub fn dispose_timers(&self) {
        for (_, task) in self.tasks.borrow_mut().drain(..) {
            task.cancel();
        }
    }
    pub fn spawn(&self, future: ExchangeFuture<'static, ()>) {
        let runtime = self.scope.runtime.clone();
        self.scope.runtime.spawn_local(Box::pin(async move {
            if let Err(e) = future.await {
                runtime.report_exception(
                    &e,
                    &serde_json::json!({"operation":"BingX callback"}).into(),
                );
            }
        }));
    }
}
impl ExchangeHost for Host {
    fn signers(&self) -> safe_cex_exchange::runtime::ExchangeSigners {
        safe_cex_exchange::runtime::ExchangeSigners {
            remote_signer: self.scope.http.hosted_signer().map(|hosted| hosted.0),
            payload_signer: None,
        }
    }
    fn secondary(
        &self,
        options: &ExchangeOptions,
        create: &ApiFactory<'_>,
        rps: f64,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError> {
        let primary = create(options.clone())?;
        self.configure_limit(&primary, rps)?;
        if options.secondary_key.as_deref().is_none_or(str::is_empty) {
            return Ok(primary);
        }
        let mut value =
            serde_json::to_value(options).map_err(|e| ExchangeError::new(e.to_string()))?;
        let object = value.as_object_mut().expect("options object");
        let secondary: Vec<_> = object
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("secondary_")
                    .map(|key| (key.to_owned(), v.clone()))
            })
            .collect();
        object.extend(secondary);
        let mut secondary_options: ExchangeOptions =
            serde_json::from_value(value).map_err(|e| ExchangeError::new(e.to_string()))?;
        // The source call supplies no otherOptions.proxyUrl. Always replace
        // the primary override, including when the result is undefined.
        secondary_options.override_proxy_url = options
            .secondary_proxy_url
            .as_ref()
            .filter(|proxy| !proxy.is_empty())
            .cloned()
            .or_else(|| self.scope.runtime.transport().public_cors_proxy());
        let reader = create(secondary_options)?;
        self.configure_signing(&reader)?;
        Ok(Rc::new(super::http::Secondary { primary, reader }))
    }
    fn init_external_public_data(&self, base: Rc<BaseExchange>) -> ExchangeFuture<'_, bool> {
        let factory = self.scope.extra.clone();
        let owner = base.clone();
        Box::pin(async move {
            if base.options().external_public_data.is_some() && factory.is_none() {
                return Err(ExchangeError::new(
                    "BingX external public data provider unavailable",
                ));
            }
            base.init_external_public_data(move |config| {
                factory.as_ref().expect("validated provider")(config, owner)
            })
            .await
        })
    }
    fn is_public_data_only(&self, base: &BaseExchange) -> bool {
        safe_cex_exchange::exchange::is_public_data_only_with(
            || base.options().public_data_only == Some(true),
            || base.can_sign(),
        )
    }
}
impl PlacementHost for Host {
    fn uuid(&self) -> Result<String, ExchangeError> {
        self.global("uuid", vec![]).map(|v| v.to_display())
    }
}
impl HistoryHost for Host {
    fn now_ms(&self) -> f64 {
        self.scope.runtime.now_ms()
    }
    fn subtract_date(&self, end: f64, amount: f64, unit: &str) -> Result<f64, ExchangeError> {
        self.global("dayjs", vec![Js::Number(end)])?
            .call_method("subtract", vec![Js::Number(amount), Js::str(unit)])
            .and_then(|d| d.call_method("valueOf", vec![]))
            .and_then(|n| n.to_number())
            .map_err(error)
    }
    fn local_to_utc(&self, ms: f64) -> f64 {
        (self.local_wall_time_to_utc)(ms)
    }
    fn is_public_data_only(&self) -> bool {
        ExchangeHost::is_public_data_only(self, self.base())
    }
    fn loop_after(
        &self,
        delay: Option<f64>,
        callback: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
    ) -> Result<(), ExchangeError> {
        let weak = self.inner.borrow().clone();
        let runtime = self.scope.runtime.clone();
        self.timer(
            delay
                .or_else(|| self.base().get_tick_interval())
                .unwrap_or(0.0),
            Rc::new(move || {
                if let Some(inner) = weak.upgrade() {
                    if !inner.base.is_disposed() {
                        let f = callback();
                        let rt = runtime.clone();
                        runtime.spawn_local(Box::pin(async move {
                            if let Err(e) = f.await {
                                rt.report_exception(
                                    &e,
                                    &serde_json::json!({"operation":"BingX tick"}).into(),
                                );
                            }
                        }));
                    }
                }
                Ok(())
            }),
        )?;
        Ok(())
    }
}
