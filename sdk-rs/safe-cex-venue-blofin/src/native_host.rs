//! Concrete BlofinExchangeHost: binds the ported bodies to BaseExchange, BaseWebSocket and WsRuntime.
use crate::{
    exchange::{BlofinApiFactory, BlofinExchangeHost, ExchangeContext},
    order_services::ServiceError,
    private_stream_control::{self as pc, PrivateAuthError},
    private_stream_data::{BlofinPrivateWebsocket, PrivateDataError, PrivateDataHost, PrivateFill},
    public_stream_core::PublicStreamHost,
    public_stream_data::{BlofinPublicWebsocket, CloseCallback, PublicDataError, PublicDataHost},
    rest_lifecycle::{ConstructorFields, LifecycleFuture},
};
use safe_cex_exchange::{
    OwnedOperation,
    client_order_id::ClientOrderIdRule,
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
    socket_lifecycle::SocketSlot,
    websocket::{
        BaseWebSocket, ReopenResync, WsDiagnosticReceiveOptions, WsParent, WsProtocol, WsRuntime,
    },
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestExecution, RestFailure, RestFuture, RestRequest, RestResponse, RoutingOptions,
    TransportError, TransportFuture, rate_limit::RateLimitClock, secondary::SecondaryKeyClients,
    socket::SocketFrame,
};
use safe_cex_types::{ExchangeName, ExchangeOptions, Order, PreparedOrderOpts};
use serde_json::{Value, json};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub(crate) type HttpBuilder =
    Rc<dyn Fn(&ExchangeOptions, &str, i64) -> Result<Rc<dyn RestAdapter>, String>>;
pub(crate) type ExternalFactory =
    safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;
type PublicSocket = BlofinPublicWebsocket<PublicHost>;
type PrivateSocket = BlofinPrivateWebsocket<PrivateHost>;

pub(crate) fn coercion_error(e: safe_cex_types::js_coercion::JsCoercionError) -> ExchangeError {
    let mut error = ExchangeError::new(e.message);
    e.name.clone_into(&mut error.name);
    error
}
pub(crate) fn service_error(e: ServiceError<ExchangeError>) -> ExchangeError {
    match e {
        ServiceError::Host(e) => e,
        ServiceError::Rest(e) => ExchangeError::from(*e),
        ServiceError::Ingress(e) => ExchangeError::new(e.to_string()),
        ServiceError::Coercion(e) => coercion_error(e),
        ServiceError::Store(e) => ExchangeError::from(e),
        ServiceError::Input(message) => ExchangeError::new(message),
    }
}
pub(crate) fn public_error(e: PublicDataError<ExchangeError>) -> ExchangeError {
    match e {
        PublicDataError::Host(e) => e,
        PublicDataError::Coercion(e) => coercion_error(e),
        PublicDataError::Store(e) => ExchangeError::from(e),
        other => ExchangeError::new(other.to_string()),
    }
}
fn private_error(e: PrivateDataError<ExchangeError>) -> ExchangeError {
    match e {
        PrivateDataError::Host(e) => e,
        PrivateDataError::Coercion(e) => coercion_error(e),
        PrivateDataError::Store(e) => ExchangeError::from(e),
        other => ExchangeError::new(other.to_string()),
    }
}
fn auth_error(e: PrivateAuthError<ExchangeError>) -> ExchangeError {
    match e {
        PrivateAuthError::Host(e) => e,
        PrivateAuthError::Coercion(e) => coercion_error(e),
    }
}
fn transport_error(e: TransportError) -> ExchangeError {
    ExchangeError::new(e.to_string())
}
fn uuid() -> String {
    safe_cex_crypto::uuid::uuid().expect("BloFin uuid requires OS entropy")
}
fn next_socket_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// BaseExchange is finalized after the venue clientOrderIdRule is installed (initialize_fields).
pub(crate) struct NativeBase {
    pending: RefCell<Option<BaseExchange>>,
    ready: OnceCell<Rc<BaseExchange>>,
}
impl NativeBase {
    pub(crate) fn get(&self) -> &Rc<BaseExchange> {
        self.ready
            .get()
            .expect("BloFin BaseExchange is finalized during construction")
    }
}

pub(crate) struct NativeHost {
    pub(crate) runtime: Rc<dyn WsRuntime>,
    base: RefCell<Option<BaseExchange>>,
    http: HttpBuilder,
    external: ExternalFactory,
    account_name: Option<String>,
    context: OnceCell<Rc<ExchangeContext>>,
    pub(crate) parent: OnceCell<Rc<Parent>>,
    public: RefCell<Option<Rc<PublicHost>>>,
    private: RefCell<Option<Rc<PrivateHost>>>,
}
impl NativeHost {
    pub(crate) fn new(
        base: BaseExchange,
        http: HttpBuilder,
        runtime: Rc<dyn WsRuntime>,
        external: ExternalFactory,
        account_name: Option<String>,
    ) -> Self {
        Self {
            runtime,
            base: RefCell::new(Some(base)),
            http,
            external,
            account_name,
            context: OnceCell::new(),
            parent: OnceCell::new(),
            public: RefCell::new(None),
            private: RefCell::new(None),
        }
    }
    fn context(&self) -> Result<&Rc<ExchangeContext>, ExchangeError> {
        self.context
            .get()
            .ok_or_else(|| ExchangeError::new("BloFin exchange context is not constructed"))
    }
    fn parent(&self, base: &NativeBase) -> Rc<Parent> {
        self.parent
            .get_or_init(|| {
                let base = base.get();
                Rc::new(Parent {
                    base: Rc::downgrade(base),
                    store: base.store.clone(),
                    resync: Rc::default(),
                    private_socket: Cell::new(0),
                    venue: RefCell::new(None),
                })
            })
            .clone()
    }
}
fn copy_rule(rule: &ClientOrderIdRule) -> ClientOrderIdRule {
    ClientOrderIdRule {
        label: rule.label.clone(),
        requirement: rule.requirement.clone(),
        is_valid: rule.is_valid.clone(),
        is_honorable_shape: rule.is_honorable_shape.clone(),
        mint: rule.mint.clone(),
    }
}
impl BlofinExchangeHost for NativeHost {
    type Error = ExchangeError;
    type Base = NativeBase;
    type Public = PublicHost;
    type Private = PrivateHost;
    fn super_constructor(
        &self,
        context: &Rc<ExchangeContext>,
    ) -> Result<NativeBase, ExchangeError> {
        let base = self
            .base
            .borrow_mut()
            .take()
            .ok_or_else(|| ExchangeError::new("BloFin BaseExchange already consumed"))?;
        self.context
            .set(context.clone())
            .map_err(|_| ExchangeError::new("BloFin exchange context already constructed"))?;
        Ok(NativeBase {
            pending: RefCell::new(Some(base)),
            ready: OnceCell::new(),
        })
    }
    fn initialize_fields(
        &self,
        base: &NativeBase,
        fields: &ConstructorFields,
    ) -> Result<(), ExchangeError> {
        let mut pending = base
            .pending
            .borrow_mut()
            .take()
            .ok_or_else(|| ExchangeError::new("BloFin BaseExchange already finalized"))?;
        pending.settings.client_order_id_rule = Some(copy_rule(&fields.client_order_id_rule));
        let ready = Rc::new(pending);
        ready.bind_socket_watchdog();
        base.ready
            .set(ready)
            .map_err(|_| ExchangeError::new("BloFin BaseExchange already finalized"))
    }
    #[expect(
        clippy::arc_with_non_send_sync,
        reason = "shared secondary-key composition takes Arc adapters"
    )]
    fn create_xhr_with_secondary_key(
        &self,
        base: &NativeBase,
        options: &ExchangeOptions,
        factory: BlofinApiFactory,
    ) -> Result<Rc<dyn RestAdapter>, ExchangeError> {
        let base = base.get();
        let api_runtime: Rc<dyn crate::api::ApiRuntime> = Rc::new(ApiClock {
            base: Rc::downgrade(base),
            context: self.context()?.clone(),
            runtime: self.runtime.clone(),
        });
        let http = self.http.clone();
        let clients = base
            .create_xhr_with_secondary_key(
                options,
                self.runtime.transport().as_ref(),
                None,
                |options| {
                    let http = http.clone();
                    factory(
                        options,
                        api_runtime.clone(),
                        Box::new(move |o, url, budget| http(o, url, budget)),
                    )
                    .map(|api| Arc::new(Shared(api)) as Arc<dyn RestAdapter>)
                    .map_err(TransportError::Host)
                },
                None,
                None,
            )
            .map_err(transport_error)?;
        Ok(Rc::new(StartedRequests {
            base: base.clone(),
            client: Rc::new(SecondaryRouter(clients)),
        }))
    }
    fn assign_xhr(&self, _: &NativeBase, _: Rc<dyn RestAdapter>) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn public_host(
        &self,
        base: &NativeBase,
        context: &Rc<ExchangeContext>,
    ) -> Result<Rc<PublicHost>, ExchangeError> {
        let stream: Rc<RefCell<Weak<PublicSocket>>> = Rc::new(RefCell::new(Weak::new()));
        let socket = BaseWebSocket::new(
            next_socket_id(),
            self.parent(base),
            self.runtime.clone(),
            Rc::new(PublicProtocol(stream.clone())),
        );
        let host = Rc::new(PublicHost {
            socket,
            base: Rc::downgrade(base.get()),
            context: context.clone(),
            runtime: self.runtime.clone(),
            stream,
            disposed: Cell::new(false),
            markets_listener: RefCell::new(None),
        });
        *self.public.borrow_mut() = Some(host.clone());
        Ok(host)
    }
    fn private_host(
        &self,
        base: &NativeBase,
        context: &Rc<ExchangeContext>,
    ) -> Result<Rc<PrivateHost>, ExchangeError> {
        let stream: Rc<RefCell<Weak<PrivateSocket>>> = Rc::new(RefCell::new(Weak::new()));
        let parent = self.parent(base);
        let socket = BaseWebSocket::new(
            next_socket_id(),
            parent.clone(),
            self.runtime.clone(),
            Rc::new(PrivateProtocol(stream.clone())),
        );
        parent.private_socket.set(socket.id);
        let host = Rc::new(PrivateHost {
            socket,
            stream,
            base: Rc::downgrade(base.get()),
            parent,
            context: context.clone(),
            runtime: self.runtime.clone(),
            account_name: self.account_name.clone(),
            disposed: Cell::new(false),
        });
        *self.private.borrow_mut() = Some(host.clone());
        Ok(host)
    }
    fn assign_public_socket(
        &self,
        base: &NativeBase,
        socket: Rc<PublicSocket>,
    ) -> Result<(), ExchangeError> {
        let host = self
            .public
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("BloFin public host was not created"))?;
        *host.stream.borrow_mut() = Rc::downgrade(&socket);
        base.get()
            .set_socket(SocketSlot::Public, Some(host.socket.clone()));
        Ok(())
    }
    fn assign_private_socket(
        &self,
        base: &NativeBase,
        socket: Rc<PrivateSocket>,
    ) -> Result<(), ExchangeError> {
        let host = self
            .private
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("BloFin private host was not created"))?;
        *host.stream.borrow_mut() = Rc::downgrade(&socket);
        base.get()
            .set_socket(SocketSlot::Private, Some(host.socket.clone()));
        Ok(())
    }
    fn super_dispose(&self, base: &NativeBase) -> Result<(), ExchangeError> {
        self.context()?.disposed.set(true);
        base.get().dispose()
    }
    fn emit_and_log_error(
        &self,
        base: &NativeBase,
        message: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        let input = message.map_or(ExchangeErrorInput::Missing, |v| {
            ExchangeErrorInput::Raw(safe_cex_exchange::js_runtime::value::from_json(v))
        });
        base.get().emit_and_log_error(input, None, None)
    }
    fn log(
        &self,
        base: &NativeBase,
        message: &str,
        severity: LogSeverity,
    ) -> Result<(), ExchangeError> {
        base.get().log(message, severity.as_str())
    }
    fn console_warn(&self, message: &str) -> Result<(), ExchangeError> {
        self.runtime.console("warning", message);
        Ok(())
    }
    fn emit_detailed_error(
        &self,
        base: &NativeBase,
        error: &ServiceError<ExchangeError>,
        hint: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        let error = match error {
            ServiceError::Host(e) => e.clone(),
            ServiceError::Rest(e) => ExchangeError::from((**e).clone()),
            ServiceError::Ingress(e) => ExchangeError::new(e.to_string()),
            ServiceError::Coercion(e) => coercion_error(e.clone()),
            ServiceError::Store(e) => ExchangeError::new(e.to_string()),
            ServiceError::Input(m) => ExchangeError::new(m.clone()),
        };
        let hint = hint
            .filter(|v| safe_cex_num::js_truthy(Some(v)))
            .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned));
        base.get().emit_and_log_error(
            ExchangeErrorInput::Error(Box::new(error)),
            hint.as_deref(),
            None,
        )
    }
    fn uuid(&self) -> String {
        uuid()
    }
    fn nanoid(&self) -> String {
        self.runtime.unique_id()
    }
    fn clock_ms(&self) -> f64 {
        self.runtime.now_ms()
    }
    fn wall_clock_ms(&self) -> f64 {
        self.runtime.now_ms()
    }
    fn sleep(&self, delay: f64) -> LifecycleFuture<'_, (), ExchangeError> {
        let sleep = self.runtime.sleep(delay.max(0.0) as u64);
        Box::pin(async move { sleep.await.map_err(ServiceError::Host) })
    }
    fn init_external_public_data(
        &self,
        base: &NativeBase,
    ) -> LifecycleFuture<'_, bool, ExchangeError> {
        let base = base.get().clone();
        let external = self.external.clone();
        Box::pin(async move {
            let owner = base.clone();
            base.init_external_public_data(|options| external(options, owner))
                .await
                .map_err(ServiceError::Host)
        })
    }
    fn schedule_tick(
        &self,
        callback: Rc<dyn Fn() -> LifecycleFuture<'static, (), ExchangeError>>,
        delay: f64,
    ) -> Result<(), ExchangeError> {
        let context = self.context()?;
        let base = self
            .parent
            .get()
            .and_then(|p| p.base.upgrade())
            .ok_or_else(|| ExchangeError::new("BloFin BaseExchange is not constructed"))?;
        if context.disposed.get() {
            return Ok(());
        }
        let sleep = self.runtime.sleep(delay.max(0.0) as u64);
        let runtime = self.runtime.clone();
        let _ = base.start_owned_operation(async move {
            if sleep.await.is_err() {
                return;
            }
            if let Err(error) = callback().await {
                runtime.report_exception(
                    &service_error(error),
                    &json!({"operation":"BloFin tick"}).into(),
                );
            }
        });
        Ok(())
    }
    fn get_tick_interval(&self, base: &NativeBase) -> Option<f64> {
        base.get().get_tick_interval()
    }
    fn broker_id_enabled(&self) -> bool {
        // TS: process.env.NODE_ENV === 'production' || options.tealstreetProxy.
        !cfg!(debug_assertions)
            || self
                .context
                .get()
                .is_some_and(|c| c.options.tealstreet_proxy == Some(true))
    }
    fn resolve_caller_client_order_id(
        &self,
        base: &NativeBase,
        _: &ClientOrderIdRule,
        opts: &PreparedOrderOpts,
    ) -> Result<Option<String>, ExchangeError> {
        base.get().resolve_caller_client_order_id(opts)
    }
    fn assert_single_native_client_order_id(
        &self,
        base: &NativeBase,
        id: Option<&str>,
        count: usize,
    ) -> Result<(), ExchangeError> {
        base.get().assert_single_native_client_order_id(id, count)
    }
    fn assert_caller_client_order_ids(
        &self,
        base: &NativeBase,
        opts: &[PreparedOrderOpts],
    ) -> Result<(), ExchangeError> {
        base.get().assert_caller_client_order_ids(opts)
    }
    fn public_error(&self, error: PublicDataError<ExchangeError>) -> ExchangeError {
        public_error(error)
    }
    fn service_error(&self, error: ServiceError<ExchangeError>) -> ExchangeError {
        service_error(error)
    }
}

struct ApiClock {
    base: Weak<BaseExchange>,
    context: Rc<ExchangeContext>,
    runtime: Rc<dyn WsRuntime>,
}
impl RateLimitClock for ApiClock {
    fn now_ms(&self) -> u64 {
        self.runtime.monotonic_now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        let sleep = self.runtime.sleep(ms);
        Box::pin(async move { sleep.await.map_err(|e| TransportError::Host(e.message)) })
    }
}
impl crate::api::ApiRuntime for ApiClock {
    fn timestamp(&self) -> f64 {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time()
    }
    fn nonce(&self) -> String {
        uuid()
    }
    fn remote_signer_present(&self) -> bool {
        self.base
            .upgrade()
            .is_some_and(|b| b.signers.remote_signer.is_some())
    }
    fn record_auth_failure(&self) {
        let streak = &self.context.auth_failure_streak;
        streak.set((streak.get() + 1.0).min(7.0));
        if let Some(base) = self.base.upgrade() {
            base.record_auth_failure();
        }
    }
    fn record_auth_success(&self) {
        self.context.auth_failure_streak.set(0.0);
        if let Some(base) = self.base.upgrade() {
            base.record_auth_success();
        }
    }
}

struct StartedRequests {
    base: Rc<BaseExchange>,
    client: Rc<dyn RestAdapter>,
}
impl RestAdapter for StartedRequests {
    fn query_in_url(&self) -> bool {
        self.client.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let pending = self.execute_with_receipt(request);
        Box::pin(async move { pending.await.map(|receipt| receipt.response) })
    }
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        let client = self.client.clone();
        let cancelled = request.clone();
        let pending = self.base.start_shared_operation(move |_| async move {
            Ok(client.execute_with_receipt(request).await)
        });
        Box::pin(async move {
            pending
                .wait()
                .await
                .map_err(|error| RestFailure::pre_transmit(error.message, cancelled))?
        })
    }
}
/// Rc venue API behind the shared Arc-based secondary-key composition.
struct Shared(Rc<dyn RestAdapter>);
impl RestAdapter for Shared {
    fn query_in_url(&self) -> bool {
        self.0.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.execute(request)
    }
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        self.0.execute_with_receipt(request)
    }
}
/// GET reads go to the secondary key's client; every other method stays primary.
struct SecondaryRouter(Rc<SecondaryKeyClients>);
impl RestAdapter for SecondaryRouter {
    fn query_in_url(&self) -> bool {
        self.0.primary.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        if request.method.eq_ignore_ascii_case("get") {
            self.0.get(request)
        } else {
            self.0.request(request)
        }
    }
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        if request.method.eq_ignore_ascii_case("get") && !self.0.shares_primary_adapter() {
            Box::pin(async move {
                let config = request.clone();
                self.0
                    .get(request)
                    .await
                    .map(|response| RestExecution { response, config })
            })
        } else {
            self.0.primary.execute_with_receipt(request)
        }
    }
}

/// One parent for both sockets so reopen resyncs coalesce across siblings.
pub(crate) struct Parent {
    base: Weak<BaseExchange>,
    store: Rc<dyn Store>,
    resync: Rc<ReopenResync>,
    private_socket: Cell<u64>,
    pub(crate) venue: RefCell<Option<Weak<dyn Exchange>>>,
}
impl Parent {
    fn live(&self) -> Result<Rc<BaseExchange>, ExchangeError> {
        self.base
            .upgrade()
            .ok_or_else(|| ExchangeError::new("BloFin parent exchange was dropped"))
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "blofin"
    }
    fn is_disposed(&self) -> bool {
        self.base.upgrade().is_none_or(|b| b.is_disposed())
    }
    fn options(&self) -> ExchangeOptions {
        self.base.upgrade().map(|b| b.options()).unwrap_or_else(|| {
            serde_json::from_value(json!({"key":"","secret":""})).expect("empty options")
        })
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, socket: &BaseWebSocket) -> bool {
        socket.id == self.private_socket.get()
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.resync.clone()
    }
    fn fetch_and_set_private_snapshot(&self, retries: usize) -> ExchangeFuture<'static, ()> {
        let venue = self.venue.borrow().as_ref().and_then(Weak::upgrade);
        Box::pin(async move {
            let venue =
                venue.ok_or_else(|| ExchangeError::new("BloFin parent exchange was dropped"))?;
            if venue.base().is_disposed() {
                return Ok(());
            }
            venue
                .fetch_and_set_private_snapshot(u32::try_from(retries).unwrap_or(u32::MAX))
                .await
        })
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.live()?.log(message, severity.as_str())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        self.live()?.emit(
            event,
            safe_cex_exchange::js_runtime::value::Js::str(message),
        )
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.live()?.sync_socket_health_to_store()
    }
    fn update_latency(&self, latency: f64) -> Result<(), ExchangeError> {
        if self.is_disposed() {
            return Ok(());
        }
        self.store
            .update(
                StoreUpdate {
                    latency: Some(latency),
                    ..Default::default()
                },
                false,
            )
            .map_err(Into::into)
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.live()?.emit_ws_reconnected()
    }
    fn emit_and_log_error(
        &self,
        error: &str,
        message: Option<&str>,
        extra: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.live()?
            .emit_and_log_error(ExchangeErrorInput::Message(error.into()), message, extra)
    }
}

fn frame_text(frame: &SocketFrame) -> Option<&str> {
    match frame {
        SocketFrame::Text(text) => Some(text),
        _ => None,
    }
}
fn owner<T>(slot: &RefCell<Weak<T>>) -> Result<Rc<T>, ExchangeError> {
    slot.borrow()
        .upgrade()
        .ok_or_else(|| ExchangeError::new("BloFin socket owner was dropped"))
}
struct PublicProtocol(Rc<RefCell<Weak<PublicSocket>>>);
impl WsProtocol for PublicProtocol {
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        owner(&self.0)?.ping().map_err(public_error)
    }
    fn connect_and_subscribe(&self, _: Rc<BaseWebSocket>, _: bool) -> Result<(), ExchangeError> {
        owner(&self.0)?
            .connect_and_subscribe()
            .map_err(public_error)
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = owner(&self.0).and_then(|s| s.on_open().map_err(public_error));
        Box::pin(std::future::ready(result))
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        match frame_text(frame) {
            Some(text) => owner(&self.0)?.on_message(text).map_err(public_error),
            None => Ok(()),
        }
    }
}
struct PrivateProtocol(Rc<RefCell<Weak<PrivateSocket>>>);
impl WsProtocol for PrivateProtocol {
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        owner(&self.0)?.ping()
    }
    fn connect_and_subscribe(&self, _: Rc<BaseWebSocket>, _: bool) -> Result<(), ExchangeError> {
        owner(&self.0)?.connect_and_subscribe()
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = owner(&self.0).and_then(|s| s.on_open());
        Box::pin(std::future::ready(result))
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        match frame_text(frame) {
            Some(text) => owner(&self.0)?.on_message(text),
            None => Ok(()),
        }
    }
}

pub(crate) struct PublicHost {
    socket: Rc<BaseWebSocket>,
    base: Weak<BaseExchange>,
    context: Rc<ExchangeContext>,
    runtime: Rc<dyn WsRuntime>,
    stream: Rc<RefCell<Weak<PublicSocket>>>,
    disposed: Cell<bool>,
    markets_listener: RefCell<Option<String>>,
}
impl PublicHost {
    fn base(&self) -> Result<Rc<BaseExchange>, ExchangeError> {
        self.base
            .upgrade()
            .ok_or_else(|| ExchangeError::new("BloFin parent exchange was dropped"))
    }
}
impl PublicStreamHost for PublicHost {
    type Error = ExchangeError;
    type Timer = Rc<OwnedOperation>;
    fn is_disposed(&self) -> bool {
        self.disposed.get() || self.socket.is_disposed()
    }
    fn is_testnet(&self) -> bool {
        self.context.options.testnet == Some(true)
    }
    fn market_ids(&self) -> Result<Vec<String>, ExchangeError> {
        Ok(self
            .context
            .store
            .markets()?
            .iter()
            .map(|m| m.id.clone())
            .collect())
    }
    fn has_socket(&self) -> bool {
        self.socket.has_websocket()
    }
    fn create_and_assign_socket(&self, url: &str) -> Result<(), ExchangeError> {
        self.socket.start_websocket(url.into(), vec![], None);
        Ok(())
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), ExchangeError> {
        // open/message route through PublicProtocol; close runs before the base close handler.
        if event == "close" {
            let stream = self.stream.clone();
            let runtime = self.runtime.clone();
            self.socket.set_before_close(Rc::new(move || {
                if let Some(stream) = stream.borrow().upgrade()
                    && let Err(error) = stream.on_close(None)
                {
                    runtime.report_exception(
                        &public_error(error),
                        &json!({"operation":"BloFin public close"}).into(),
                    );
                }
            }));
        }
        Ok(())
    }
    fn send(&self, frame: &str) -> Result<(), ExchangeError> {
        if frame == "ping" {
            // Share the exact timestamp sampled by the venue ping body with
            // BaseWebSocket, which owns pong latency and its next-ping timer.
            self.socket
                .set_ping_at(owner(&self.stream)?.core.ping_at.get());
        }
        self.socket.send(frame)
    }
    fn performance_now(&self) -> f64 {
        self.runtime.monotonic_now_ms()
    }
    fn markets_changed_off(&self) -> Result<(), ExchangeError> {
        let id = self.markets_listener.borrow_mut().take();
        if let Some(id) = id {
            self.base()?.off("markets:changed", Some(&id));
        }
        Ok(())
    }
    fn markets_changed_on(&self) -> Result<(), ExchangeError> {
        let stream = self.stream.clone();
        let id = self.base()?.on(
            "markets:changed",
            Rc::new(move |_, data| {
                let Some(stream) = stream.borrow().upgrade() else {
                    return Ok(());
                };
                let added = data
                    .get("added")?
                    .array_values()?
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|row| row.get("id").ok())
                    .filter_map(|id| id.as_str().map(|units| String::from_utf16_lossy(units)))
                    .collect::<Vec<_>>();
                stream.on_markets_changed(&added).map_err(public_error)
            }),
        );
        *self.markets_listener.borrow_mut() = Some(id);
        Ok(())
    }
    fn reconnecting_enabled(&self) -> bool {
        self.runtime.enable_reconnecting()
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.socket.emit_ws_reconnected()
    }
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
        delay_ms: f64,
    ) -> Result<Rc<OwnedOperation>, ExchangeError> {
        self.socket
            .start_timeout(delay_ms.max(0.0) as u64, move || callback())
            .map(Rc::new)
    }
    fn timer_is_truthy(&self, timer: &Rc<OwnedOperation>) -> bool {
        timer.is_active()
    }
    fn clear_timeout(&self, timer: &Rc<OwnedOperation>) -> Result<(), ExchangeError> {
        timer.cancel();
        Ok(())
    }
    fn log_warning(&self, message: &str) -> Result<(), ExchangeError> {
        self.base()?.log(message, "warning")
    }
    fn emit_warning(&self, message: &str, force_toast: bool) -> Result<(), ExchangeError> {
        self.base()?.emit_args(
            "warning",
            &[
                safe_cex_exchange::js_runtime::value::Js::str(message),
                safe_cex_exchange::js_runtime::value::Js::object(vec![(
                    "forceToast",
                    safe_cex_exchange::js_runtime::value::Js::Bool(force_toast),
                )]),
            ],
        )
    }
}
impl PublicDataHost for PublicHost {
    fn store(&self) -> &dyn Store {
        self.context.store.as_ref()
    }
    fn is_connected(&self) -> bool {
        self.socket.is_connected()
    }
    fn super_construct(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn set_treat_any_message_as_pong(&self, enabled: bool) {
        self.socket.treat_any_message_as_pong.set(enabled);
    }
    fn base_close_handler(&self) -> CloseCallback<ExchangeError> {
        // BaseWebSocket::on_close processes the close itself, right after before_close.
        Rc::new(|_| Ok(()))
    }
    fn super_dispose(&self) -> Result<(), ExchangeError> {
        self.disposed.set(true);
        self.socket.dispose();
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.socket.track_last_received()
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.socket.handle_pong_event(5_000)
    }
    fn cleanup_message_handler_stats(&self, key: &str) -> Result<(), ExchangeError> {
        self.socket.cleanup_message_handler_stats(key)
    }
    fn scheduled_error(&self, error: PublicDataError<ExchangeError>) -> ExchangeError {
        public_error(error)
    }
}

pub(crate) struct PrivateHost {
    socket: Rc<BaseWebSocket>,
    stream: Rc<RefCell<Weak<PrivateSocket>>>,
    base: Weak<BaseExchange>,
    parent: Rc<Parent>,
    context: Rc<ExchangeContext>,
    runtime: Rc<dyn WsRuntime>,
    account_name: Option<String>,
    disposed: Cell<bool>,
}
impl PrivateHost {
    fn base(&self) -> Result<Rc<BaseExchange>, ExchangeError> {
        self.base
            .upgrade()
            .ok_or_else(|| ExchangeError::new("BloFin parent exchange was dropped"))
    }
}
impl pc::PrivateStreamHost for PrivateHost {
    type Error = ExchangeError;
    type Timer = Rc<OwnedOperation>;
    fn is_disposed(&self) -> bool {
        self.disposed.get() || self.socket.is_disposed()
    }
    fn is_testnet(&self) -> bool {
        self.context.options.testnet == Some(true)
    }
    fn resolve_private_route(&self) -> Result<pc::PrivateRoute, ExchangeError> {
        let route = self
            .runtime
            .transport()
            .ws_routing(
                &ExchangeName::Blofin,
                &RoutingOptions::from(&self.context.options),
            )
            .ws_private;
        Ok(pc::PrivateRoute {
            tunnelled: route.tunnelled,
            host: route.host,
        })
    }
    fn create_and_assign_socket(&self, url: Option<&str>) -> Result<(), ExchangeError> {
        let url =
            url.ok_or_else(|| ExchangeError::new("BloFin tunnelled private route has no host"))?;
        self.socket.start_websocket(url.into(), vec![], None);
        Ok(())
    }
    fn add_socket_listener(&self, _: &str) -> Result<(), ExchangeError> {
        // open/message route through PrivateProtocol; close is the base close handler.
        Ok(())
    }
    fn super_dispose(&self) -> Result<(), ExchangeError> {
        self.disposed.set(true);
        self.socket.dispose();
        Ok(())
    }
    fn timer_is_truthy(&self, timer: &Rc<OwnedOperation>) -> bool {
        timer.is_active()
    }
    fn clear_timeout(&self, timer: &Rc<OwnedOperation>) -> Result<(), ExchangeError> {
        timer.cancel();
        Ok(())
    }
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> pc::PrivateStreamFuture<()>>,
        delay_ms: f64,
    ) -> Result<Rc<OwnedOperation>, ExchangeError> {
        let sleep = self.runtime.sleep(delay_ms.max(0.0) as u64);
        Ok(Rc::new(self.socket.start_message_operation(async move {
            sleep.await?;
            callback().await;
            Ok(())
        })))
    }
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError> {
        if frame == "ping" {
            // Share the exact timestamp sampled by the venue ping body with
            // BaseWebSocket, which owns pong latency and its next-ping timer.
            self.socket
                .set_ping_at(owner(&self.stream)?.core.ping_at.get());
        }
        self.socket.send_with_log(frame)
    }
    fn performance_now(&self) -> f64 {
        self.runtime.monotonic_now_ms()
    }
    fn nonce(&self) -> String {
        uuid()
    }
    fn timestamp_ms(&self) -> f64 {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
    }
    fn remote_signer_present(&self) -> bool {
        self.base
            .upgrade()
            .is_some_and(|b| b.signers.remote_signer.is_some())
    }
    fn remote_account_name(&self) -> Option<Value> {
        self.account_name.clone().map(Value::String)
    }
    fn request_id(&self) -> String {
        self.runtime.unique_id()
    }
    fn sign_remote(
        &self,
        request: pc::RemoteAuthRequest,
    ) -> pc::PrivateStreamStart<pc::RemoteAuthResult, ExchangeError> {
        use safe_cex_transport::signing::{RemoteSignerRequest, RemoteSignerResponse};
        use safe_cex_types::ws_auth::{WsAuth, WsAuthRequest};
        let signer = self
            .base()?
            .signers
            .remote_signer
            .clone()
            .ok_or_else(|| ExchangeError::new("BloFin remote signer is not configured"))?;
        let extra = serde_json::value::RawValue::from_string(format!(
            "{{\"timestamp\":{},\"nonce\":{}}}",
            safe_cex_num::js_number_string(request.timestamp),
            Value::String(request.nonce)
        ))
        .map_err(|e| ExchangeError::new(e.to_string()))?;
        let signed = RemoteSignerRequest::WsAuth {
            exchange_name: ExchangeName::Blofin,
            account_name: request
                .account_name
                .and_then(|v| v.as_str().map(str::to_owned)),
            request: WsAuthRequest {
                id: Some(Value::String(request.id)),
                request_type: None,
                ws_auth: Some(WsAuth {
                    to_sign: Some(request.to_sign),
                    output_format: Some(Value::String(request.output_format.into())),
                    extra_data: Some(extra),
                }),
            },
        };
        Ok(Box::pin(async move {
            match signer.sign(signed).await.map_err(transport_error)? {
                RemoteSignerResponse::WsAuth(response) => Ok(pc::RemoteAuthResult {
                    error: response.error.map(Value::String),
                    signature: response.ws_auth_signature.map(Value::String),
                }),
                RemoteSignerResponse::Http(_) => Err(ExchangeError::new(
                    "BloFin WS auth received an HTTP signer response",
                )),
            }
        }))
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
    fn emit_signing_error(&self, message: &str) -> Result<(), ExchangeError> {
        self.base()?
            .emit_and_log_error(ExchangeErrorInput::Message(message.into()), None, None)
    }
    fn launch_auth(
        &self,
        task: pc::PrivateStreamFuture<Result<(), PrivateAuthError<ExchangeError>>>,
    ) {
        // Shared P4 contract: auth settles before any subscribe; failure reconnects without sending.
        let weak = Rc::downgrade(&self.socket);
        let auth: ExchangeFuture<'static, ()> =
            Box::pin(async move { task.await.map_err(auth_error) });
        let _ = self.socket.start_message_operation(async move {
            let Some(socket) = weak.upgrade() else {
                return Ok(());
            };
            socket.authenticate_and_subscribe(auth, |_| Ok(())).await
        });
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.socket.track_last_received()
    }
    fn diagnostic_receive(&self, data: &str) -> Result<(), ExchangeError> {
        self.socket.emit_ws_diagnostic_receive(
            &Value::String(data.into()),
            &WsDiagnosticReceiveOptions::default(),
        )
    }
    fn log_last_log(&self, data: &str) -> Result<(), ExchangeError> {
        self.socket.log_last_log(data);
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.socket.handle_pong_event(5_000)
    }
    fn reconnecting_enabled(&self) -> bool {
        self.runtime.enable_reconnecting()
    }
    fn fetch_and_set_private_snapshot(&self) -> Result<(), ExchangeError> {
        let snapshot = self.parent.fetch_and_set_private_snapshot(1);
        let runtime = self.runtime.clone();
        let _ = self.socket.start_message_operation(async move {
            if let Err(error) = snapshot.await {
                runtime.report_exception(
                    &error,
                    &json!({"operation":"BloFin private snapshot"}).into(),
                );
            }
            Ok(())
        });
        Ok(())
    }
    fn handle_order_topic(&self, _: &Value) -> Result<(), ExchangeError> {
        Err(ExchangeError::new(
            "BloFin order topics dispatch through the private bridge",
        ))
    }
    fn handle_position_topic(&self, _: &Value) -> Result<(), ExchangeError> {
        Err(ExchangeError::new(
            "BloFin position topics dispatch through the private bridge",
        ))
    }
    fn handle_account_topic(&self, _: &Value) -> Result<(), ExchangeError> {
        Err(ExchangeError::new(
            "BloFin account topics dispatch through the private bridge",
        ))
    }
    fn fetch_orders(&self) -> pc::PrivateStreamStart<Vec<Order>, ExchangeError> {
        Err(ExchangeError::new(
            "BloFin private orders read binds to the parent exchange",
        ))
    }
    fn store(&self) -> &dyn Store {
        self.context.store.as_ref()
    }
}
impl PrivateDataHost for PrivateHost {
    fn super_construct(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn set_treat_any_message_as_pong(&self, enabled: bool) {
        self.socket.treat_any_message_as_pong.set(enabled);
    }
    fn wall_clock_ms(&self) -> f64 {
        self.runtime.now_ms()
    }
    fn emit_fill(&self, fill: PrivateFill) -> Result<(), ExchangeError> {
        self.base()?.emit(
            "fill",
            safe_cex_exchange::js_runtime::value::from_typed(&fill)?,
        )
    }

    fn emit_mapping_error(&self, message: &str) -> Result<(), ExchangeError> {
        self.base()?
            .emit_and_log_error(ExchangeErrorInput::Message(message.into()), None, None)
    }
    fn body_error(&self, error: PrivateDataError<ExchangeError>) -> ExchangeError {
        private_error(error)
    }
}
