use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_types::*;
use safe_cex_venue_blofin::{
    exchange::*, order_services::ServiceError, private_stream_control as pc,
    private_stream_data as pd, public_stream_core as uc, public_stream_data as ud,
    rest_lifecycle as rl,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
pub fn ready<F: Future>(future: F) -> F::Output {
    let mut f = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("poll floor100 exhausted")
}
pub type Timer = Rc<dyn Fn() -> pc::PrivateStreamFuture<()>>;
pub struct Adapter {
    pub requests: RefCell<Vec<RestRequest>>,
    pub trace: Rc<RefCell<Vec<Value>>>,
    pub responses: RefCell<BTreeMap<String, Value>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, req: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(req.clone());
        self.trace.borrow_mut().push(json!([
            "request",
            req.method,
            req.url,
            req.params,
            req.body
                .as_ref()
                .map(|b| b.untransformed().unwrap().clone().into_value().unwrap())
        ]));
        let data = self
            .responses
            .borrow()
            .get(&req.url)
            .cloned()
            .unwrap_or(json!({"code":"0","data":[]}));
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(serde_json::to_vec(&data).unwrap()),
            })
        })
    }
}
pub struct Runtime {
    pub fill_statuses: RefCell<Vec<Option<OrderStatus>>>,
    pub xhr: RefCell<Option<Rc<dyn RestAdapter>>>,
    pub context: Rc<ExchangeContext>,
    pub store: Rc<DefaultStore>,
    pub trace: Rc<RefCell<Vec<Value>>>,
    pub fills: RefCell<Vec<pd::PrivateFill>>,
    pub timers: RefCell<BTreeMap<u32, Timer>>,
    pub next: Cell<u32>,
    pub initial: Value,
    pub any_pong: Cell<bool>,
    pub reentry: RefCell<Option<Rc<dyn Fn()>>>,
    pub adapter: Rc<Adapter>,
}
impl Runtime {
    pub fn new(corpus: &Value, i: &Value) -> Rc<Self> {
        let store = Rc::new(DefaultStore::default());
        let mut market = corpus["market"].clone();
        if let Some(v) = i.get("contractSize") {
            market["contractSize"] = v.clone();
        }
        if let Some(v) = i.get("contractValue") {
            market["info"]["contractValue"] = v.clone();
        }
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(if i["missingMarket"] == true {
                    vec![]
                } else {
                    vec![serde_json::from_value(market).unwrap()]
                }),
                balance: Some(serde_json::from_value(corpus["balance"].clone()).unwrap()),
                loaded: Some(StoreDataLoaded {
                    markets: true,
                    tickers: false,
                    positions: false,
                    orders: false,
                    balance: true,
                }),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let trace = Rc::default();
        let adapter = Rc::new(Adapter {
            trace: Rc::clone(&trace),
            responses: RefCell::default(),
            requests: RefCell::default(),
        });
        Rc::new(Self {
            context: Rc::new(ExchangeContext {
                name: ExchangeName::Blofin,
                options: serde_json::from_value(json!({"key":"k","secret":"s","passphrase":"p"}))
                    .unwrap(),
                store: store.clone(),
                auth_failure_streak: Cell::new(0.0),
                disposed: Cell::new(false),
            }),
            store,
            trace,
            fills: RefCell::default(),
            fill_statuses: RefCell::default(),
            timers: RefCell::default(),
            next: Cell::new(1),
            initial: i.clone(),
            any_pong: Cell::new(false),
            reentry: RefCell::default(),
            adapter,
            xhr: RefCell::default(),
        })
    }
    pub fn event(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
    pub fn fail(&self, label: &str) -> Result<(), String> {
        if self.initial["fail"] == label {
            Err(format!("{label} failed"))
        } else {
            Ok(())
        }
    }
}
pub struct Socket {
    pub disposed: Cell<bool>,
    pub runtime: Rc<Runtime>,
    pub context: Rc<ExchangeContext>,
    pub public: bool,
}
impl pc::PrivateStreamHost for Socket {
    type Error = String;
    type Timer = u32;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_testnet(&self) -> bool {
        self.context.options.testnet == Some(true)
    }
    fn resolve_private_route(&self) -> Result<pc::PrivateRoute, String> {
        Ok(pc::PrivateRoute {
            tunnelled: false,
            host: None,
        })
    }
    fn create_and_assign_socket(&self, url: Option<&str>) -> Result<(), String> {
        self.runtime.event(json!(["create", url]));
        Ok(())
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), String> {
        self.runtime.event(json!(["listener", event]));
        Ok(())
    }
    fn super_dispose(&self) -> Result<(), String> {
        self.runtime.fail("private-dispose")?;
        self.disposed.set(true);
        self.runtime.event(json!(["socket-dispose"]));
        Ok(())
    }
    fn timer_is_truthy(&self, t: &u32) -> bool {
        *t != 0
    }
    fn clear_timeout(&self, t: &u32) -> Result<(), String> {
        self.runtime.timers.borrow_mut().remove(t);
        self.runtime.event(json!(["clear", t]));
        Ok(())
    }
    fn set_timeout(&self, cb: Timer, delay: f64) -> Result<u32, String> {
        let n = self.runtime.next.get();
        self.runtime.next.set(n + 1);
        self.runtime.timers.borrow_mut().insert(n, cb);
        self.runtime.event(json!(["timer", n, delay]));
        Ok(n)
    }
    fn send_with_log(&self, frame: &str) -> Result<(), String> {
        self.runtime.event(json!(["send", frame]));
        Ok(())
    }
    fn performance_now(&self) -> f64 {
        123.5
    }
    fn nonce(&self) -> String {
        "nonce".into()
    }
    fn timestamp_ms(&self) -> f64 {
        1710000000000.125
    }
    fn remote_signer_present(&self) -> bool {
        false
    }
    fn remote_account_name(&self) -> Option<Value> {
        None
    }
    fn request_id(&self) -> String {
        "nano".into()
    }
    fn sign_remote(
        &self,
        _: pc::RemoteAuthRequest,
    ) -> pc::PrivateStreamStart<pc::RemoteAuthResult, String> {
        Err("unused remote signer".into())
    }
    fn secret(&self) -> String {
        self.context.options.secret.clone()
    }
    fn key(&self) -> String {
        self.context.options.key.clone()
    }
    fn passphrase(&self) -> Option<Value> {
        self.context.options.passphrase.clone().map(Value::String)
    }
    fn emit_signing_error(&self, message: &str) -> Result<(), String> {
        self.runtime.event(json!(["mapping-error", message]));
        Ok(())
    }
    fn launch_auth(&self, task: pc::PrivateStreamFuture<Result<(), pc::PrivateAuthError<String>>>) {
        ready(task).unwrap();
    }
    fn track_last_received(&self) -> Result<(), String> {
        self.runtime.event(json!(["track"]));
        Ok(())
    }
    fn diagnostic_receive(&self, _: &str) -> Result<(), String> {
        self.runtime.event(json!(["receive"]));
        Ok(())
    }
    fn log_last_log(&self, _: &str) -> Result<(), String> {
        self.runtime.event(json!(["log"]));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), String> {
        self.runtime.event(json!(["pong"]));
        Ok(())
    }
    fn reconnecting_enabled(&self) -> bool {
        true
    }
    fn fetch_and_set_private_snapshot(&self) -> Result<(), String> {
        Ok(())
    }
    fn handle_order_topic(&self, _: &Value) -> Result<(), String> {
        panic!("concrete bridge must bind order body")
    }
    fn handle_position_topic(&self, _: &Value) -> Result<(), String> {
        panic!("concrete bridge must bind position body")
    }
    fn handle_account_topic(&self, _: &Value) -> Result<(), String> {
        panic!("concrete bridge must bind account body")
    }
    fn fetch_orders(&self) -> pc::PrivateStreamStart<Vec<Order>, String> {
        panic!("exchange must bind actual parent orders reader")
    }
    fn store(&self) -> &dyn Store {
        self.context.store.as_ref()
    }
}
impl pd::PrivateDataHost for Socket {
    fn super_construct(&self) -> Result<(), String> {
        self.runtime.event(json!(["socket-construct"]));
        Ok(())
    }
    fn set_treat_any_message_as_pong(&self, enabled: bool) {
        self.runtime.any_pong.set(enabled)
    }
    fn wall_clock_ms(&self) -> f64 {
        1710000000555.0
    }
    fn emit_fill(&self, fill: pd::PrivateFill) -> Result<(), String> {
        self.runtime.fail("fill")?;
        self.runtime.fill_statuses.borrow_mut().push(
            self.context
                .store
                .orders()
                .unwrap()
                .iter()
                .find(|o| o.id == fill.order_id)
                .and_then(|o| o.status.into_value()),
        );
        self.runtime.fills.borrow_mut().push(fill);
        self.runtime.event(json!(["fill"]));
        let callback = self.runtime.reentry.borrow().clone();
        if let Some(cb) = callback {
            cb();
        }
        Ok(())
    }
    fn emit_mapping_error(&self, message: &str) -> Result<(), String> {
        self.runtime.event(json!(["mapping-error", message]));
        Ok(())
    }
    fn body_error(&self, e: pd::PrivateDataError<String>) -> String {
        e.to_string()
    }
    fn map_position_rows(
        &self,
        rows: &Value,
    ) -> Result<Vec<Position>, pd::PrivateDataError<String>> {
        if self.runtime.initial["mappingError"] == true {
            return Err(pd::PrivateDataError::Host("mapping failed".into()));
        }
        let input = safe_cex_num::numeric_ingress::js_from_value::<
            safe_cex_venue_blofin::positions::PositionsInput,
        >(rows.clone())
        .unwrap();
        safe_cex_venue_blofin::positions::map_positions(
            &input,
            self.context.store.as_ref(),
            |message| pd::PrivateDataHost::emit_mapping_error(self, message),
        )
        .map_err(|e| match e {
            safe_cex_venue_blofin::positions::PositionMapError::Store(e) => {
                pd::PrivateDataError::Store(e.into())
            }
            safe_cex_venue_blofin::positions::PositionMapError::Coercion(e) => {
                pd::PrivateDataError::Coercion(e)
            }
            safe_cex_venue_blofin::positions::PositionMapError::Emitter(e) => {
                pd::PrivateDataError::Host(e)
            }
        })
    }
}
impl uc::PublicStreamHost for Socket {
    type Error = String;
    type Timer = u32;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_testnet(&self) -> bool {
        self.context.options.testnet == Some(true)
    }
    fn market_ids(&self) -> Result<Vec<std::string::String>, std::string::String> {
        Ok(self
            .context
            .store
            .markets()
            .unwrap()
            .iter()
            .map(|m| m.id.clone())
            .collect())
    }
    fn has_socket(&self) -> bool {
        true
    }
    fn create_and_assign_socket(&self, url: &str) -> Result<(), String> {
        self.runtime.event(json!(["create", url]));
        Ok(())
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), String> {
        self.runtime.event(json!(["listener", event]));
        Ok(())
    }
    fn send(&self, frame: &str) -> Result<(), String> {
        self.runtime.event(json!(["send", frame]));
        Ok(())
    }
    fn performance_now(&self) -> f64 {
        123.5
    }
    fn markets_changed_off(&self) -> Result<(), String> {
        Ok(())
    }
    fn markets_changed_on(&self) -> Result<(), String> {
        Ok(())
    }
    fn reconnecting_enabled(&self) -> bool {
        true
    }
    fn emit_ws_reconnected(&self) -> Result<(), String> {
        Ok(())
    }
    fn set_timeout(
        &self,
        _: Rc<dyn Fn() -> Result<(), String>>,
        delay: f64,
    ) -> Result<u32, String> {
        self.runtime.event(json!(["public-timer", delay]));
        Ok(1)
    }
    fn timer_is_truthy(&self, t: &u32) -> bool {
        *t != 0
    }
    fn clear_timeout(&self, _: &u32) -> Result<(), String> {
        Ok(())
    }
    fn log_warning(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn emit_warning(&self, _: &str, _: bool) -> Result<(), String> {
        Ok(())
    }
}
impl ud::PublicDataHost for Socket {
    fn store(&self) -> &dyn Store {
        self.context.store.as_ref()
    }
    fn is_connected(&self) -> bool {
        true
    }
    fn super_construct(&self) -> Result<(), String> {
        assert!(self.public);
        self.runtime.event(json!(["public"]));
        Ok(())
    }
    fn set_treat_any_message_as_pong(&self, enabled: bool) {
        self.runtime.any_pong.set(enabled)
    }
    fn base_close_handler(&self) -> ud::CloseCallback<String> {
        Rc::new(|_| Ok(()))
    }
    fn super_dispose(&self) -> Result<(), String> {
        self.runtime.fail("public-dispose")?;
        self.disposed.set(true);
        self.runtime.event(json!(["public-dispose"]));
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), String> {
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), String> {
        Ok(())
    }
    fn cleanup_message_handler_stats(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn scheduled_error(&self, e: ud::PublicDataError<String>) -> String {
        e.to_string()
    }
}
pub struct ExchangeHost {
    pub runtime: Rc<Runtime>,
}
impl BlofinExchangeHost for ExchangeHost {
    type Error = String;
    type Base = Rc<ExchangeContext>;
    type Public = Socket;
    type Private = Socket;
    fn super_constructor(&self, context: &Rc<ExchangeContext>) -> Result<Self::Base, String> {
        self.runtime.event(json!(["base", "blofin"]));
        Ok(context.clone())
    }
    fn initialize_fields(
        &self,
        _: &Self::Base,
        fields: &rl::ConstructorFields,
    ) -> Result<(), String> {
        assert_eq!(fields.client_order_id_rule.label, "BloFin");
        Ok(())
    }
    fn create_xhr_with_secondary_key(
        &self,
        base: &Self::Base,
        opts: &ExchangeOptions,
        factory: BlofinApiFactory,
    ) -> Result<Rc<dyn RestAdapter>, String> {
        self.runtime.event(json!(["xhr"]));
        assert!(
            std::ptr::fn_addr_eq(factory, create_native_api as BlofinApiFactory),
            "native factory identity"
        );
        let adapter = self.runtime.adapter.clone();
        factory(
            opts,
            Rc::new(ApiClock {
                context: base.clone(),
            }),
            Box::new(move |_, url, budget| {
                assert_eq!(url, "https://openapi.blofin.com");
                assert_eq!(budget, 20000, "native factory default budget");
                Ok(adapter)
            }),
        )
    }
    fn assign_xhr(&self, _: &Self::Base, xhr: Rc<dyn RestAdapter>) -> Result<(), String> {
        *self.runtime.xhr.borrow_mut() = Some(xhr);
        Ok(())
    }
    fn public_host(
        &self,
        _: &Self::Base,
        context: &Rc<ExchangeContext>,
    ) -> Result<Rc<Socket>, String> {
        Ok(Rc::new(Socket {
            runtime: self.runtime.clone(),
            context: context.clone(),
            public: true,
            disposed: Cell::new(false),
        }))
    }
    fn private_host(
        &self,
        _: &Self::Base,
        context: &Rc<ExchangeContext>,
    ) -> Result<Rc<Socket>, String> {
        Ok(Rc::new(Socket {
            runtime: self.runtime.clone(),
            context: context.clone(),
            public: false,
            disposed: Cell::new(false),
        }))
    }
    fn assign_public_socket(
        &self,
        _: &Self::Base,
        _: Rc<ud::BlofinPublicWebsocket<Socket>>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn assign_private_socket(
        &self,
        _: &Self::Base,
        _: Rc<pd::BlofinPrivateWebsocket<Socket>>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn super_dispose(&self, base: &Self::Base) -> Result<(), String> {
        self.runtime.fail("base-dispose")?;
        base.disposed.set(true);
        self.runtime.event(json!(["base-dispose"]));
        Ok(())
    }
    fn emit_and_log_error(&self, _: &Self::Base, message: Option<&Value>) -> Result<(), String> {
        self.runtime.event(json!(["mapping-error", message]));
        Ok(())
    }
    fn log(&self, _: &Self::Base, _: &str, _: LogSeverity) -> Result<(), String> {
        Ok(())
    }
    fn console_warn(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn emit_detailed_error(
        &self,
        _: &Self::Base,
        _: &ServiceError<String>,
        _: Option<&Value>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn uuid(&self) -> String {
        "0123456789abcdef0123456789abcdef".into()
    }
    fn nanoid(&self) -> String {
        "nano".into()
    }
    fn clock_ms(&self) -> f64 {
        1710000000000.125
    }
    fn wall_clock_ms(&self) -> f64 {
        1710000000555.0
    }
    fn sleep(&self, _: f64) -> rl::LifecycleFuture<'_, (), String> {
        Box::pin(async { Ok(()) })
    }
    fn start_async<T: 'static>(
        &self,
        _: &Self::Base,
        body: rl::LifecycleFuture<'static, T, String>,
    ) -> rl::LifecycleFuture<'static, T, String> {
        // This bounded assembly fixture drives all children through ready(); the
        // production scheduling contract is tested with NativeHost separately.
        body
    }
    fn init_external_public_data(&self, _: &Self::Base) -> rl::LifecycleFuture<'_, bool, String> {
        Box::pin(async { Ok(false) })
    }
    fn schedule_tick(
        &self,
        _: Rc<dyn Fn() -> rl::LifecycleFuture<'static, (), String>>,
        delay: f64,
    ) -> Result<(), String> {
        self.runtime.event(json!(["schedule", delay]));
        Ok(())
    }
    fn get_tick_interval(&self, base: &Self::Base) -> Option<f64> {
        safe_cex_transport::polling::get_tick_interval(
            "blofin",
            Some(2000.0),
            base.auth_failure_streak.get() as u32,
        )
    }
    fn broker_id_enabled(&self) -> bool {
        true
    }
    fn resolve_caller_client_order_id(
        &self,
        _: &Self::Base,
        _: &safe_cex_exchange::client_order_id::ClientOrderIdRule,
        opts: &safe_cex_types::PreparedOrderOpts,
    ) -> Result<Option<String>, String> {
        Ok(opts.client_order_id.clone())
    }
    fn assert_single_native_client_order_id(
        &self,
        _: &Self::Base,
        _: Option<&str>,
        _: usize,
    ) -> Result<(), String> {
        Ok(())
    }
    fn assert_caller_client_order_ids(
        &self,
        _: &Self::Base,
        _: &[safe_cex_types::PreparedOrderOpts],
    ) -> Result<(), String> {
        Ok(())
    }
    fn public_error(&self, e: ud::PublicDataError<String>) -> String {
        e.to_string()
    }
    fn service_error(&self, e: ServiceError<String>) -> String {
        format!("{e:?}")
    }
}
pub fn exchange(r: &Rc<Runtime>) -> Rc<BlofinExchange<ExchangeHost>> {
    BlofinExchange::new(
        Rc::new(ExchangeHost { runtime: r.clone() }),
        ExchangeName::Blofin,
        r.context.options.clone(),
        r.store.clone(),
    )
    .unwrap()
}

struct ApiClock {
    context: Rc<ExchangeContext>,
}
impl safe_cex_transport::rate_limit::RateLimitClock for ApiClock {
    fn now_ms(&self) -> u64 {
        1710000000000
    }
    fn sleep_ms(&self, _: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
impl safe_cex_venue_blofin::api::ApiRuntime for ApiClock {
    fn timestamp(&self) -> f64 {
        1710000000000.125
    }
    fn nonce(&self) -> String {
        "0123456789abcdef0123456789abcdef".into()
    }
    fn remote_signer_present(&self) -> bool {
        false
    }
    fn record_auth_failure(&self) {
        self.context
            .auth_failure_streak
            .set((self.context.auth_failure_streak.get() + 1.0).min(7.0));
    }
    fn record_auth_success(&self) {
        self.context.auth_failure_streak.set(0.0)
    }
}
