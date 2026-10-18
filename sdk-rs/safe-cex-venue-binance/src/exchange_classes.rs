//! Binance exchange owner assembly. Every venue entry below resolves through
//! the corresponding body port; dependencies expose HTTP and imported helpers.
use crate::{
    class_jobs::{self, NativeFuture, Settlement},
    private_classes::{
        PrivateBase, PrivateDependencies, PrivateParent, PrivateSocket, PrivateTimers,
    },
    private_event_value::{
        EventArray, EventOps, EventValue, array_values, in_current_realm, native_json, number,
        record, string,
    },
    private_rest::{SnapshotClient, SnapshotRequestFuture},
    private_session::Venue,
    public_classes::{PublicDependencies, PublicKind, PublicSocket},
    request_weight::Failure,
};
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, LogSeverity},
    websocket::{BaseWebSocket, ReopenResync, WsParent, WsRuntime},
};
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::{Rc, Weak},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    Private,
    Primary,
    Secondary,
    Public,
    Unlimited,
    Fapi,
}
/// Imported Axios/secondary-key factory operations. Factories execute the real
/// API ports, while client handles and launched I/O belong to the transport host.
pub trait ExchangeImports {
    fn factory(
        &self,
        venue: Venue,
        kind: crate::private_session::Factory,
        opts: EventValue,
        owner: EventValue,
    ) -> Result<EventValue, Failure>;
    fn split(
        &self,
        venue: Venue,
        opts: EventValue,
        owner: EventValue,
        factory: Rc<dyn Fn(EventValue) -> Result<EventValue, Failure>>,
        rate: Option<crate::private_session::RateHook>,
    ) -> Result<EventValue, Failure>;
    fn rate_limit(&self, client: EventValue, max_rps: f64) -> Result<EventValue, Failure>;
    /// `args` is the query params for GET/DELETE (`xhr.get(path, { params })`) and the
    /// body for POST/PUT (`xhr.post(path, data)`); resolves to `response.data`.
    fn request(
        &self,
        client: EventValue,
        method: HttpMethod,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure>;
    fn register_weight_guard(
        &self,
        client: EventValue,
        fulfilled: crate::request_weight::Fulfilled<EventValue>,
        rejected: crate::request_weight::Rejected<EventValue, Failure>,
    ) -> Result<(), Failure>;
    /// Read imported response/header properties without a JSON projection.
    /// Hosts may retain executable getters behind the original native handle.
    fn weight_response_property(
        &self,
        value: &EventValue,
        key: &str,
    ) -> Result<EventValue, Failure> {
        value.read(key)
    }
    fn weight_header_get_member(
        &self,
        _headers: &EventValue,
    ) -> Result<crate::request_weight::HeaderGetMember, Failure> {
        Ok(crate::request_weight::HeaderGetMember::NotCallable)
    }
    fn weight_error_response(&self, error: &Failure) -> Result<Option<EventValue>, Failure> {
        Ok(error
            .retained_response_input()
            .map(safe_cex_exchange::js_runtime::value::from_retained_input))
    }
    fn external_provider(
        &self,
        options: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData>;
    fn private_base(&self, parent: Rc<dyn PrivateParent>) -> Result<Rc<dyn PrivateBase>, Failure>;
    fn listener_operations(&self) -> Rc<dyn crate::public_listeners::ListenerHost>;
    fn timers(&self) -> Rc<dyn PrivateTimers>;
    fn remote_sign(
        &self,
        signer: EventValue,
    ) -> Result<crate::api_factory::RemoteSignCall, Failure>;
    /// Receives the exchange before any client is created; hosts keep it weakly.
    fn bind_owner(&self, _owner: Weak<ExchangeState>) {}
    fn virtual_now(&self) -> f64;
    fn uuid(&self) -> String;
    fn dayjs(&self, value: Option<f64>) -> Result<f64, Failure>;
    fn subtract(&self, end: f64, amount: f64, unit: char) -> Result<f64, Failure>;
    fn paginate(
        &self,
        fetch: crate::rest_history::PageCall,
        opts: EventValue,
    ) -> Result<NativeFuture<EventValue>, Failure>;
    fn iso(&self, time: EventValue) -> Result<String, Failure>;
    fn fold_closed_positions(
        &self,
        opts: EventValue,
        symbol_scoped: bool,
    ) -> Result<NativeFuture<EventValue>, Failure>;
    fn loop_timer(
        &self,
        callback: crate::lifecycle::TickTimer,
        interval: EventValue,
    ) -> Result<(), Failure>;
}
pub struct ExchangeDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub imports: Rc<dyn ExchangeImports>,
    pub signers: safe_cex_exchange::runtime::ExchangeSigners,
    pub remote_signer_token: Option<EventValue>,
    pub payload_signer_token: Option<EventValue>,
}
#[path = "exchange_trait.rs"]
mod exchange_trait;

pub struct ExchangeOwner {
    capabilities: safe_cex_exchange::capabilities::ExchangeCapabilities,
    pub state: Rc<ExchangeState>,
    pub public: Rc<PublicSocket>,
    pub private: Rc<PrivateSocket>,
}
impl std::ops::Deref for ExchangeOwner {
    type Target = Rc<ExchangeState>;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
impl ExchangeOwner {
    pub fn new(
        venue: Venue,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        d: ExchangeDependencies,
    ) -> Result<Rc<Self>, Failure> {
        ExchangeState::construct(venue, name, options, store, d)
    }
    pub fn invoke(
        self: &Rc<Self>,
        method: Method,
        args: Vec<EventValue>,
    ) -> Settlement<EventValue> {
        let _binding = self.state.prototype_realm.bind();
        let owner = self.clone();
        class_jobs::start(
            self.runtime.clone(),
            Box::pin(async move { owner.state.execute(method, args).await }),
        )
    }
}
pub struct ExchangeState {
    pub venue: Venue,
    pub base: Rc<BaseExchange>,
    prototype_realm: Rc<safe_cex_num::PrototypeRealm>,
    pub clients: EventValue,
    public: OnceCell<Weak<PublicSocket>>,
    private: OnceCell<Weak<PrivateSocket>>,
    pub position_version: Cell<f64>,
    pub paused_until: Cell<f64>,
    pub times: crate::lifecycle::ReconciliationTimes,
    pub orders_stale: Cell<bool>,
    pub retry: Rc<crate::orders_retry::OrdersRetryState>,
    pub has_logged_balance: Cell<bool>,
    pub closed_positions_share_ledger_source: bool,
    options: EventValue,
    identity: EventValue,
    imports: Rc<dyn ExchangeImports>,
    runtime: Rc<dyn WsRuntime>,
    this: Weak<ExchangeState>,
    resync: Rc<ReopenResync>,
    root: OnceCell<Weak<ExchangeOwner>>,
}
fn failure(message: impl Into<String>) -> Failure {
    Rc::new(ExchangeError::new(message))
}
fn decoded(v: impl serde::Serialize) -> Result<EventValue, Failure> {
    safe_cex_exchange::js_runtime::value::from_typed(&v)
        .map(in_current_realm)
        .map_err(|error| failure(error.message()))
}

fn json_value(v: &EventValue) -> Result<Value, Failure> {
    let Some(text) =
        safe_cex_num::js_json_stringify(&native_json(v)).map_err(|e| failure(e.message))?
    else {
        return Err(failure("undefined has no JSON carrier"));
    };
    serde_json::from_str(&text).map_err(|e| failure(e.to_string()))
}

/// The typed IncomeRecord boundary: Binance sends tranId/tradeId as JSON numbers, which
/// TS keeps raw and every consumer interpolates; the typed record carries String(id).
pub(crate) fn income_records(
    value: &EventValue,
) -> Result<Vec<safe_cex_types::IncomeRecord>, Failure> {
    let rows = if let EventValue::Array(items) = value {
        let mut rows = Vec::new();
        for item in array_values(items)? {
            let item = if matches!(item, EventValue::Object(_)) {
                let next = item
                    .spread_object()
                    .map_err(|error| Rc::new(error.to_exchange_error()))?;
                for key in ["tranId", "tradeId"] {
                    if let EventValue::Number(id) = next.read(key)? {
                        next.write(key, string(safe_cex_num::js_number_string(id)));
                    }
                }
                next
            } else {
                item
            };
            rows.push(item);
        }
        EventValue::array(rows)
    } else {
        value.clone()
    };
    typed(&rows)
}
fn typed<T: serde::de::DeserializeOwned>(v: &EventValue) -> Result<T, Failure> {
    crate::class_decode::decode(v.clone()).map_err(|e| failure(e.to_string()))
}
impl ExchangeState {
    fn construct(
        venue: Venue,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        d: ExchangeDependencies,
    ) -> Result<Rc<ExchangeOwner>, Failure> {
        let mut settings = ExchangeSettings {
            supports_public_only: true,
            ..Default::default()
        };
        let mut rule = crate::exchange_classes::client_rule(venue);
        let mint = d.imports.clone();
        rule.mint = Some(Rc::new(move || format!("x-s3vFFRQp{}", mint.uuid())));
        settings.client_order_id_rule = Some(rule);
        let mut base = BaseExchange::new(name, options.clone(), store, d.runtime.clone(), settings);
        let opts = decoded(options)?;
        if d.signers.remote_signer.is_some() {
            opts.write(
                "remoteSigner",
                d.remote_signer_token
                    .clone()
                    .ok_or_else(|| failure("remote signer token required"))?,
            );
        }
        if d.signers.payload_signer.is_some() {
            opts.write(
                "payloadSigner",
                d.payload_signer_token
                    .clone()
                    .ok_or_else(|| failure("payload signer token required"))?,
            );
        }
        base.signers = d.signers;
        let base = Rc::new(base);
        base.bind_socket_watchdog();
        let identity = record([("name", decoded(&base.name)?), ("options", opts.clone())]);
        let owner = Rc::new_cyclic(|this: &Weak<ExchangeState>| Self {
            prototype_realm: safe_cex_num::PrototypeRealm::current(),
            venue,
            base,
            clients: identity.clone(),
            public: OnceCell::new(),
            private: OnceCell::new(),
            position_version: Cell::new(0.0),
            paused_until: Cell::new(0.0),
            times: Default::default(),
            orders_stale: Cell::new(false),
            retry: Rc::new(crate::orders_retry::OrdersRetryState {
                last_retry_at: Cell::new(0.0),
                is_disposed: crate::class_flags::LiveFlag::read({
                    let this = this.clone();
                    Rc::new(move || this.upgrade().is_none_or(|owner| owner.base.is_disposed()))
                }),
                snapshot_stale: crate::class_flags::LiveFlag::read({
                    let this = this.clone();
                    Rc::new(move || this.upgrade().is_none_or(|owner| owner.orders_stale.get()))
                }),
            }),
            has_logged_balance: Cell::new(false),
            closed_positions_share_ledger_source: false,
            options: opts,
            identity,
            imports: d.imports,
            runtime: d.runtime,
            this: this.clone(),
            resync: Default::default(),
            root: OnceCell::new(),
        });
        owner.imports.bind_owner(Rc::downgrade(&owner));
        let bindings = Rc::new(Constructor(owner.clone(), OnceCell::new(), OnceCell::new()));
        let name = decoded(&owner.base.name)?;
        let store = record([]);
        match venue {
            Venue::Futures => crate::private_session::construct_futures(
                name,
                owner.options.clone(),
                store,
                bindings.clone(),
            )?,
            Venue::PM => crate::private_session::construct_pm(
                name,
                owner.options.clone(),
                store,
                bindings.clone(),
            )?,
            Venue::Spot => crate::private_session::construct_spot(
                name,
                owner.options.clone(),
                store,
                bindings.clone(),
            )?,
        };
        let root = Rc::new(ExchangeOwner {
            capabilities: match venue {
                Venue::Futures => exchange_trait::capabilities::<BinanceExchange>(true),
                Venue::PM => exchange_trait::capabilities::<BinancePMExchange>(false),
                Venue::Spot => exchange_trait::capabilities::<BinanceSpotExchange>(false),
            },
            state: owner.clone(),
            public: bindings.1.get().expect("public child constructed").clone(),
            private: bindings.2.get().expect("private child constructed").clone(),
        });
        owner
            .root
            .set(Rc::downgrade(&root))
            .map_err(|_| failure("duplicate exchange root"))?;
        Ok(root)
    }
    pub fn public_socket(&self) -> Rc<PublicSocket> {
        self.public
            .get()
            .and_then(Weak::upgrade)
            .expect("live public child lease")
    }
    pub fn private_socket(&self) -> Rc<PrivateSocket> {
        self.private
            .get()
            .and_then(Weak::upgrade)
            .expect("live private child lease")
    }
    fn client(&self, client: Client) -> Result<EventValue, Failure> {
        self.clients.read(match client {
            Client::Private => {
                if self.venue == Venue::PM {
                    "papiXhr"
                } else {
                    "xhr"
                }
            }
            Client::Primary => "_primaryXhr",
            Client::Secondary => "_secondaryXhr",
            Client::Public => {
                if self.venue == Venue::Spot {
                    "xhr"
                } else {
                    "publicXhr"
                }
            }
            Client::Unlimited => "unlimitedXHR",
            Client::Fapi => "fapiXhr",
        })
    }
    fn request(
        &self,
        client: Client,
        method: HttpMethod,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        self.imports
            .request(self.client(client)?, method, path, args)
    }
    fn http(
        &self,
        client: Client,
        method: HttpMethod,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        let request = self.request(client, method, path, args)?;
        Ok(Box::pin(async move {
            class_jobs::reaction(request).await.result
        }))
    }
    fn store_field(&self, name: &str) -> Result<EventValue, Failure> {
        let store = self.base.store.as_ref();
        match name {
            "orders" => decoded(
                store
                    .orders()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .to_vec(),
            ),
            "positions" => decoded(
                store
                    .positions()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .to_vec(),
            ),
            "balance" => decoded(
                store
                    .balance()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .clone(),
            ),
            "markets" => decoded(
                store
                    .markets()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .to_vec(),
            ),
            "tickers" => decoded(
                store
                    .tickers()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .to_vec(),
            ),
            "tickersBySymbol" => decoded(&store.tickers_by_symbol().map_err(|error| {
                std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
            })?),
            "marketsBySymbol" => decoded(&store.markets_by_symbol().map_err(|error| {
                std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
            })?),
            "loaded" => decoded(
                store
                    .loaded()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .clone(),
            ),
            "fetchedLeverage" => decoded(
                store
                    .fetched_leverage()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .clone(),
            ),
            _ => Err(failure(format!("unknown Store field {name}"))),
        }
    }
    fn update_store(&self, patch: EventValue) -> Result<(), Failure> {
        let update: StoreUpdate = typed(&patch)?;
        self.base
            .store
            .update(update, false)
            .map_err(|error| Rc::new(error.into()))
    }
    pub(crate) fn emit(&self, message: EventValue) -> Result<(), Failure> {
        self.base
            .emit_and_log_error(
                safe_cex_exchange::error_reporting::ExchangeErrorInput::Raw(message),
                None,
                None,
            )
            .map_err(Rc::new)
    }
    pub fn mark_position_state_updated(&self) {
        let mut version = self.position_version.get();
        crate::reconciliation::mark_position_state_updated_by_websocket(&mut version);
        self.position_version.set(version)
    }
    pub fn position_state_version(&self) -> f64 {
        crate::reconciliation::get_position_state_version(self.position_version.get())
    }
    pub fn paused(&self) -> bool {
        crate::reconciliation::is_background_reconciliation_paused(
            self.paused_until.get(),
            self.runtime.now_ms(),
        )
    }
    pub fn pause(&self, duration: f64) {
        let mut until = self.paused_until.get();
        crate::reconciliation::pause_background_reconciliation(
            &mut until,
            self.runtime.now_ms(),
            duration,
        );
        self.paused_until.set(until)
    }
    pub fn dispose(&self) -> Result<(), Failure> {
        crate::exchange_dispose::dispose_binance_exchange(
            || self.base.dispose().map_err(Rc::new),
            || {
                self.public_socket().dispose();
                Ok(())
            },
            || self.private_socket().dispose(),
        )
    }
    pub fn invoke(
        self: &Rc<Self>,
        method: Method,
        args: Vec<EventValue>,
    ) -> Settlement<EventValue> {
        let _binding = self.prototype_realm.bind();
        let owner = self.clone();
        let root = self.root.get().and_then(Weak::upgrade);
        class_jobs::start(
            self.runtime.clone(),
            Box::pin(async move {
                let _root_lease = root;
                owner.execute(method, args).await
            }),
        )
    }
}
struct Constructor(
    Rc<ExchangeState>,
    OnceCell<Rc<PublicSocket>>,
    OnceCell<Rc<PrivateSocket>>,
);
impl crate::private_session::ConstructorBindings for Constructor {
    fn super_fields(
        &self,
        _name: EventValue,
        _opts: EventValue,
        _store: EventValue,
    ) -> Result<EventValue, Failure> {
        Ok(self.0.clients.clone())
    }
    fn split(
        &self,
        v: Venue,
        opts: EventValue,
        _owner: EventValue,
        factory: crate::private_session::Factory,
        rate: Option<crate::private_session::RateHook>,
    ) -> Result<EventValue, Failure> {
        let imports = self.0.imports.clone();
        let receiver = self.0.identity.clone();
        let create = Rc::new(move |opts| imports.factory(v, factory, opts, receiver.clone()));
        self.0
            .imports
            .split(v, opts, self.0.identity.clone(), create, rate)
    }
    fn api(
        &self,
        v: Venue,
        kind: crate::private_session::Factory,
        opts: EventValue,
        _owner: EventValue,
    ) -> Result<EventValue, Failure> {
        self.0
            .imports
            .factory(v, kind, opts, self.0.identity.clone())
    }
    fn rate_limit(&self, xhr: EventValue, max: f64) -> Result<EventValue, Failure> {
        self.0.imports.rate_limit(xhr, max)
    }
    fn guards(&self, _owner: EventValue, clients: EventValue) -> Result<(), Failure> {
        self.0.install_guards(clients)
    }
    fn child(
        &self,
        v: Venue,
        kind: crate::private_session::Child,
        _owner: EventValue,
    ) -> Result<EventValue, Failure> {
        match kind {
            crate::private_session::Child::Public => {
                let kind = match v {
                    Venue::Futures => PublicKind::FuturesPublic,
                    Venue::PM => PublicKind::PMPublic,
                    Venue::Spot => PublicKind::SpotPublic,
                };
                let child = PublicSocket::new(
                    kind,
                    1,
                    PublicDependencies {
                        parent: self.0.clone(),
                        runtime: self.0.runtime.clone(),
                        operations: self.0.imports.listener_operations(),
                    },
                );
                self.0.base.set_borrowed_socket(
                    safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
                    child.base(),
                );
                self.0
                    .public
                    .set(Rc::downgrade(&child))
                    .map_err(|_| failure("duplicate public child"))?;
                self.1
                    .set(child)
                    .map_err(|_| failure("duplicate public lease"))?
            }
            crate::private_session::Child::Private => {
                let imports = self.0.imports.clone();
                let time = self.0.imports.clone();
                let child = PrivateSocket::new(
                    v,
                    PrivateDependencies {
                        parent: self.0.clone(),
                        base_factory: Rc::new(move |parent| imports.private_base(parent)),
                        timers: self.0.imports.timers(),
                        runtime: self.0.runtime.clone(),
                        virtual_now: Rc::new(move || time.virtual_now()),
                    },
                )?;
                if let Some(socket) = child.base.core_socket() {
                    self.0.base.set_borrowed_socket(
                        safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
                        &socket,
                    );
                }
                self.0
                    .private
                    .set(Rc::downgrade(&child))
                    .map_err(|_| failure("duplicate private child"))?;
                self.2
                    .set(child)
                    .map_err(|_| failure("duplicate private lease"))?
            }
        }
        Ok(record([]))
    }
}

pub fn client_rule(venue: Venue) -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    use safe_cex_exchange::client_order_id::{
        ClientOrderIdRuleOverrides, pattern_client_order_id_rule,
    };
    let max = if venue == Venue::PM { 32 } else { 36 };
    let spot = venue == Venue::Spot;
    pattern_client_order_id_rule(
        match venue {
            Venue::Futures => "Binance",
            Venue::PM => "Binance PM",
            Venue::Spot => "Binance Spot",
        }
        .into(),
        if spot {
            "/^[A-Za-z0-9_-]{1,36}$/".into()
        } else {
            format!("/^[.A-Z:/a-z0-9_-]{{1,{max}}}$/")
        },
        Rc::new(move |id| {
            !id.is_empty()
                && id.len() <= max
                && id.bytes().all(|c| {
                    c.is_ascii_alphanumeric()
                        || b"_-".contains(&c)
                        || (!spot && b".:/".contains(&c))
                })
        }),
        ClientOrderIdRuleOverrides {
            is_honorable_shape: None,
            mint: None,
        },
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    GetAccount,
    ValidateAccount,
    Start,
    StartPrivate,
    StartPublicOnly,
    Tick,
    Markets,
    Tickers,
    Positions,
    PositionsBalance,
    Balance,
    Orders,
    RegularOrders,
    ConditionalOrders,
    RetryOrders,
    PositionMode,
    ChangePositionMode,
    ChangeMargin,
    Leverage,
    Cancel,
    CancelSymbol,
    UpdateOrder,
    PlaceOrder,
    PlaceOrders,
    RegularBatch,
    AlgoBatch,
    Batch,
    ConflictCancel,
    HistoricalFills,
    HistoricalOrders,
    IncomePage,
    IncomeHistory,
    IncomeRange,
    PublicTrades,
    HistoricalPositions,
    ClosedPositions,
    OHLCV,
    Overlay,
    FindOrder,
}
impl ExchangeState {
    fn install_guards(self: &Rc<Self>, clients: EventValue) -> Result<(), Failure> {
        let EventValue::Array(clients) = clients else {
            return Err(failure("weight clients must be an array"));
        };
        let mut objects = vec![];
        for value in EventValue::Array(clients.clone())
            .array_items()?
            .expect("array")
            .into_iter()
            .flatten()
        {
            if let EventValue::Object(object) = value {
                objects.push(object.clone())
            }
        }
        let owner = Rc::downgrade(self);
        let observe: crate::request_weight::ObserveGetter<EventValue> = Rc::new(move || {
            let owner = owner
                .upgrade()
                .ok_or_else(|| failure("exchange owner released"))?;
            Ok(Rc::new(move |response| {
                let response = response.unwrap_or(EventValue::Undefined);
                let status = if response.nullish() {
                    None
                } else {
                    match owner
                        .imports
                        .weight_response_property(&response, "status")?
                    {
                        EventValue::Number(429.0) => Some(json!(429)),
                        EventValue::Number(418.0) => Some(json!(418)),
                        _ => None,
                    }
                };
                let headers = if response.nullish() {
                    EventValue::Undefined
                } else {
                    owner
                        .imports
                        .weight_response_property(&response, "headers")?
                };
                crate::request_weight::observe_request_weight(
                    status.as_ref(),
                    |key| {
                        crate::request_weight::get_response_header(
                            headers.truthy(),
                            key,
                            || owner.imports.weight_header_get_member(&headers),
                            |name| {
                                let value =
                                    owner.imports.weight_response_property(&headers, name)?;
                                if value.nullish() {
                                    Ok(None)
                                } else {
                                    Ok(Some(json_value(&value)?))
                                }
                            },
                        )
                    },
                    |duration| {
                        owner.pause(duration);
                        Ok(())
                    },
                )
            }))
        });
        let imports = self.imports.clone();
        let rejected_imports = Rc::downgrade(&imports);
        crate::request_weight::install_request_weight_guards(
            &objects,
            observe,
            Rc::new(move |error: &Failure| {
                rejected_imports
                    .upgrade()
                    .ok_or_else(|| failure("exchange imports released"))?
                    .weight_error_response(error)
            }),
            move |client, good, bad| {
                imports.register_weight_guard(EventValue::Object(client.clone()), good, bad)
            },
        )
    }
    async fn execute(
        self: &Rc<Self>,
        method: Method,
        args: Vec<EventValue>,
    ) -> Result<EventValue, Failure> {
        let arg = |i: usize| args.get(i).cloned().unwrap_or(EventValue::Undefined);
        let mut context = Context(self.clone());
        match method {
            Method::FindOrder => {
                crate::private_rest::find_futures_order_by_client_id(
                    &arg(0).text()?,
                    &arg(1).text()?,
                    &mut context,
                )
                .await
            }
            Method::Orders => match self.venue {
                Venue::Futures => {
                    crate::private_rest::fetch_futures_orders(&self.orders_stale, &mut context)
                        .await
                }
                Venue::PM => {
                    crate::private_rest::fetch_pm_orders(
                        &self.orders_stale,
                        || {
                            Ok(Box::pin(class_jobs::reaction(
                                self.invoke(Method::RegularOrders, vec![]),
                            )))
                        },
                        || {
                            Ok(Box::pin(class_jobs::reaction(
                                self.invoke(Method::ConditionalOrders, vec![]),
                            )))
                        },
                        &mut context,
                    )
                    .await
                }
                Venue::Spot => crate::private_rest::fetch_spot_orders(&mut context).await,
            },
            Method::RegularOrders => {
                crate::private_rest::fetch_pm_regular_orders(&mut context).await
            }
            Method::ConditionalOrders => {
                crate::private_rest::fetch_pm_conditional_orders(&mut context).await
            }
            Method::Positions => match self.venue {
                Venue::Futures => {
                    crate::position_snapshots::fetch_futures_positions(self.clone()).await
                }
                Venue::PM => crate::position_snapshots::fetch_pm_positions(self.clone()).await,
                Venue::Spot => {
                    let future = self.invoke(Method::PositionsBalance, vec![]);
                    let positions =
                        crate::account_fetch::fetch_binance_spot_positions(|| async move {
                            let result = class_jobs::reaction(future)
                                .await
                                .map_err(|e| (*e).clone())?;
                            Ok(match result {
                                EventValue::Undefined => {
                                    crate::account_fetch::FetchedPositions::Undefined
                                }
                                EventValue::Null => crate::account_fetch::FetchedPositions::Null,
                                result => {
                                    let value =
                                        result.read("positions").map_err(|e| (*e).clone())?;
                                    crate::account_fetch::FetchedPositions::Present {
                                        positions: (!matches!(value, EventValue::Undefined))
                                            .then_some(value),
                                    }
                                }
                            })
                        })
                        .await
                        .map_err(Rc::new)?;
                    Ok(positions.unwrap_or(EventValue::Undefined))
                }
            },
            Method::PositionsBalance => {
                if self.venue == Venue::Spot {
                    crate::private_rest::fetch_spot_positions_balance(&mut context).await
                } else {
                    crate::position_snapshots::fetch_futures_positions_and_balance(
                        &self.has_logged_balance,
                        self.clone(),
                    )
                    .await
                }
            }
            Method::Balance => crate::private_rest::fetch_pm_balance(&mut context).await,
            Method::Cancel => match self.venue {
                Venue::Futures => {
                    crate::order_operations::cancel_futures(arg(0), &mut context).await
                }
                Venue::PM => crate::order_operations::cancel_pm(arg(0), &mut context).await,
                Venue::Spot => crate::order_operations::cancel_spot(arg(0), &mut context).await,
            },
            Method::UpdateOrder => match self.venue {
                Venue::Futures => {
                    crate::order_operations::update_futures(arg(0), &mut context).await
                }
                Venue::PM => crate::order_operations::update_pm(arg(0), &mut context).await,
                Venue::Spot => crate::order_operations::update_spot(arg(0), &mut context).await,
            },
            Method::PlaceOrder => match self.venue {
                Venue::Futures => {
                    crate::order_operations::place_futures(arg(0), &mut context).await
                }
                Venue::PM => crate::order_operations::place_pm(arg(0), &mut context).await,
                Venue::Spot => crate::order_operations::place_spot(arg(0), &mut context).await,
            },
            Method::PlaceOrders => match self.venue {
                Venue::Futures => crate::order_batches::many_futures(arg(0), &mut context).await,
                Venue::PM => crate::order_batches::many_pm(arg(0), &mut context).await,
                Venue::Spot => crate::order_batches::many_spot(arg(0), &mut context).await,
            },
            Method::RegularBatch => {
                crate::order_batches::regular_futures(arg(0), &mut context).await
            }
            Method::AlgoBatch => crate::order_batches::algo_futures(arg(0), &mut context).await,
            Method::Batch => {
                if self.venue == Venue::PM {
                    crate::order_batches::batch_pm(arg(0), &mut context).await
                } else {
                    crate::order_batches::batch_spot(arg(0), &mut context).await
                }
            }
            Method::ConflictCancel => {
                crate::order_batches::cancel_close(arg(0), &mut context).await?;
                Ok(EventValue::Undefined)
            }
            Method::HistoricalFills => {
                crate::rest_history::historical_fills(arg(0), &mut context).await
            }
            Method::HistoricalOrders => {
                crate::rest_history::historical_orders(arg(0), &mut context).await
            }
            Method::IncomePage => crate::rest_history::income_page(arg(0), &mut context).await,
            Method::IncomeHistory => {
                crate::rest_history::income_history(arg(0), &mut context).await
            }
            Method::IncomeRange => {
                crate::rest_history::income_range(arg(0), arg(1), arg(2), &mut context).await
            }
            Method::PublicTrades => {
                crate::rest_history::public_trades(arg(0), arg(1), &mut context).await
            }
            Method::HistoricalPositions => self.imports.fold_closed_positions(arg(0), true)?.await,
            Method::Start | Method::StartPrivate | Method::Tick => {
                match (method, self.venue) {
                    (Method::Start, Venue::Futures) => {
                        crate::lifecycle::start_futures(&mut context).await?
                    }
                    (Method::Start, Venue::PM) => crate::lifecycle::start_pm(&mut context).await?,
                    (Method::Start, Venue::Spot) => {
                        crate::lifecycle::start_spot(&mut context).await?
                    }
                    (Method::StartPrivate, Venue::Futures) => {
                        crate::lifecycle::start_private_futures(&mut context).await?
                    }
                    (Method::StartPrivate, Venue::PM) => {
                        crate::lifecycle::start_private_pm(&mut context).await?
                    }
                    (Method::StartPrivate, Venue::Spot) => {
                        crate::lifecycle::start_private_spot(&mut context).await?
                    }
                    (Method::Tick, Venue::Futures) => {
                        crate::lifecycle::tick_futures(&self.times, &mut context).await?
                    }
                    (Method::Tick, Venue::PM) => {
                        crate::lifecycle::tick_pm(arg(0), &mut context).await?
                    }
                    _ => crate::lifecycle::tick_spot(&mut context).await?,
                }
                Ok(EventValue::Undefined)
            }
            Method::StartPublicOnly => {
                match self.venue {
                    Venue::Futures => {
                        crate::public_start::start_binance_public_data_only(&self.base)
                    }
                    Venue::PM => crate::public_start::start_binance_pm_public_data_only(&self.base),
                    Venue::Spot => {
                        crate::public_start::start_binance_spot_public_data_only(&self.base)
                    }
                }
                .map_err(|error| match error.downcast::<ExchangeError>() {
                    Ok(error) => Rc::new(*error),
                    Err(error) => match error.downcast::<safe_cex_store::StoreMutationError>() {
                        Ok(error) => Rc::new((*error).into()),
                        Err(error) => failure(error.to_string()),
                    },
                })?;
                Ok(EventValue::Undefined)
            }
            Method::GetAccount => {
                let data = self.http(
                    Client::Private,
                    HttpMethod::Get,
                    if self.venue == Venue::PM {
                        "/papi/v1/balance"
                    } else {
                        "/fapi/v3/balance"
                    },
                    None,
                )?;
                let request = async move {
                    data.await
                        .and_then(|data| json_value(&data).map(Some))
                        .map_err(|e| (*e).clone())
                };
                let account = if self.venue == Venue::PM {
                    crate::account_fetch::get_binance_pm_account(|_| request).await
                } else {
                    crate::account_fetch::get_binance_account(|_| request).await
                }
                .map_err(Rc::new)?;
                Ok(record([(
                    "userId",
                    account
                        .user_id
                        .map_or(EventValue::Undefined, crate::private_session::native),
                )]))
            }
            Method::PositionMode => {
                let owner = self.clone();
                let get = move |path| {
                    let request = owner.http(Client::Private, HttpMethod::Get, path, None);
                    async move { json_value(&request?.await?).map(Some) }
                };
                let result = if self.venue == Venue::PM {
                    crate::position_mode::fetch_binance_pm_position_mode(|path| async {
                        get(path).await.map_err(|e| (*e).clone())
                    })
                    .await
                } else {
                    crate::position_mode::fetch_binance_position_mode(|path| async {
                        get(path).await.map_err(|e| (*e).clone())
                    })
                    .await
                }
                .map_err(Rc::new)?;
                Ok(EventValue::Bool(result))
            }
            Method::Markets => self.fetch_markets().await,
            Method::Tickers => self.fetch_tickers().await,
            Method::OHLCV => self.fetch_ohlcv(arg(0)).await,
            Method::ValidateAccount => self.validate_account().await,
            Method::Overlay => {
                self.load_overlay().await?;
                Ok(EventValue::Undefined)
            }
            Method::RetryOrders => {
                self.retry_orders().await?;
                Ok(EventValue::Undefined)
            }
            Method::CancelSymbol => {
                self.cancel_symbol(arg(0).text()?).await?;
                Ok(EventValue::Undefined)
            }
            Method::ChangePositionMode => {
                self.change_mode(
                    arg(0).truthy(),
                    if arg(1).nullish() {
                        None
                    } else {
                        Some(arg(1).text()?)
                    },
                )
                .await?;
                Ok(EventValue::Undefined)
            }
            Method::ChangeMargin => {
                self.change_margin(
                    arg(0).truthy(),
                    if arg(1).nullish() {
                        None
                    } else {
                        Some(arg(1).text()?)
                    },
                )
                .await?;
                Ok(EventValue::Undefined)
            }
            Method::Leverage => {
                self.set_leverage(arg(0).text()?, arg(1).number()?).await?;
                Ok(EventValue::Undefined)
            }
            Method::ClosedPositions => {
                let options = typed(&arg(0))?;
                let owner = self.clone();
                let rows = crate::closed_position_reader::derive_binance_closed_positions(
                    &options,
                    self.public_only(),
                    || self.runtime.now_ms(),
                    move |path, args| {
                        let request = owner.http(
                            Client::Private,
                            HttpMethod::Get,
                            path,
                            Some(crate::private_session::native(args)),
                        );
                        async move {
                            request
                                .map_err(|e| (*e).clone())?
                                .await
                                .and_then(|value| json_value(&value).map(Some))
                                .map_err(|e| (*e).clone())
                        }
                    },
                    |msg| emit_json(&self.base, &msg),
                )
                .await
                .map_err(Rc::new)?;
                decoded(rows)
            }
        }
    }
}
struct Context(Rc<ExchangeState>);
impl crate::private_rest::SnapshotBindings for Context {
    fn get(
        &mut self,
        c: SnapshotClient,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        self.0.request(
            match c {
                SnapshotClient::Private | SnapshotClient::Papi => Client::Private,
                SnapshotClient::Primary => Client::Primary,
            },
            HttpMethod::Get,
            path,
            args,
        )
    }
    fn capture(
        &mut self,
        m: crate::private_rest::SnapshotMethod,
    ) -> Result<crate::private_rest::SnapshotCall, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |args| {
            use crate::private_rest::SnapshotArgument as A;
            let value = match args.first() {
                Some(A::Value(v)) => v.clone(),
                Some(A::Error(e)) => string(&e.message),
                None => EventValue::Undefined,
            };
            match m {
                crate::private_rest::SnapshotMethod::ConsoleError => {
                    let values = args
                        .into_iter()
                        .map(|arg| match arg {
                            A::Value(value) => value,
                            A::Error(error) => {
                                safe_cex_exchange::js_runtime::value::Thrown::from((*error).clone())
                                    .value
                            }
                        })
                        .collect::<Vec<_>>();
                    owner.runtime.console_values("error", &values);
                    Ok(())
                }
                crate::private_rest::SnapshotMethod::Log => {
                    let severity = match args.get(1) {
                        Some(A::Value(value)) if !matches!(value, EventValue::Undefined) => {
                            value.text()?
                        }
                        _ => "info".into(),
                    };
                    owner.base.log_value(&value, &severity).map_err(Rc::new)
                }
                crate::private_rest::SnapshotMethod::Emit => owner.emit(value),
            }
        }))
    }
    fn now(&mut self) -> Result<f64, Failure> {
        Ok(self.0.imports.virtual_now())
    }
    fn orders(&mut self) -> Result<EventValue, Failure> {
        self.0.store_field("orders")
    }
    fn balance(&mut self) -> Result<EventValue, Failure> {
        self.0.store_field("balance")
    }
    fn positions(&mut self) -> Result<EventValue, Failure> {
        self.0.store_field("positions")
    }
    fn asset_price(&mut self, asset: EventValue) -> Result<f64, Failure> {
        self.0.asset_price(&asset.text()?)
    }
    fn market(&mut self, symbol: &str) -> Result<EventValue, Failure> {
        self.0.store_field("marketsBySymbol")?.read(symbol)
    }
    fn has_primary(&mut self) -> Result<bool, Failure> {
        Ok(self.0.client(Client::Primary)?.truthy())
    }
    fn carries_client_id(&mut self, order: &EventValue, id: &str) -> Result<bool, Failure> {
        safe_cex_exchange::client_order_id::order_carries_client_order_id(&typed(order)?, id)
            .map_err(|error| Rc::new(ExchangeError::from(error)))
    }
    fn map_history(&mut self, data: EventValue) -> Result<EventValue, Failure> {
        Ok(
            crate::order_history::map_binance_historical_order(&data, &self.0.prototype_realm)?
                .unwrap_or(EventValue::Undefined),
        )
    }
}
impl crate::position_snapshots::PositionSnapshotBindings for ExchangeState {
    fn get(&self, c: SnapshotClient, path: &'static str) -> Result<SnapshotRequestFuture, Failure> {
        self.request(
            match c {
                SnapshotClient::Private | SnapshotClient::Papi => Client::Private,
                SnapshotClient::Primary => Client::Primary,
            },
            HttpMethod::Get,
            path,
            None,
        )
    }
    fn positions(&self) -> Result<EventValue, Failure> {
        self.store_field("positions")
    }
    fn balance(&self) -> Result<EventValue, Failure> {
        self.store_field("balance")
    }
    fn markets(&self) -> Result<EventValue, Failure> {
        self.store_field("markets")
    }
    fn tickers(&self) -> Result<EventValue, Failure> {
        self.store_field("tickersBySymbol")
    }
    fn hedged(&self) -> Result<bool, Failure> {
        Ok(self
            .base
            .store
            .options()
            .map_err(|error| {
                std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
            })?
            .is_hedged)
    }
    fn id(
        &self,
        symbol: String,
        side: EventValue,
        mode: safe_cex_types::PositionMode,
    ) -> Result<String, Failure> {
        let side = typed::<safe_cex_types::PositionSide>(&side)?;
        Ok(safe_cex_types::model_utils::create_position_id(
            &symbol,
            Some(side),
            Some(mode),
        ))
    }
    fn capture_emit(&self) -> Result<crate::position_snapshots::Emit, Failure> {
        let owner = self.this.upgrade().expect("live owner");
        Ok(Rc::new(move |value| owner.emit(value)))
    }
    fn capture_log(&self) -> Result<crate::position_snapshots::Log, Failure> {
        let owner = self.this.upgrade().expect("live owner");
        Ok(Rc::new(move |value, severity| {
            owner.base.log(&value, severity.as_str()).map_err(Rc::new)
        }))
    }
}

impl crate::order_operations::OrderBindings for Context {
    fn capture_delete(
        &mut self,
        _venue: crate::order_operations::Venue,
    ) -> Result<crate::order_operations::Delete, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |path, args| {
            owner.http(Client::Private, HttpMethod::Delete, path, Some(args))
        }))
    }
    fn capture_status(&mut self, single: bool) -> Result<crate::order_operations::Status, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |selector, status| {
            let ids = if single {
                vec![selector.read("id")?.text()?]
            } else {
                let EventValue::Array(rows) = selector else {
                    return Err(failure("order selectors must be an array"));
                };
                crate::private_event_value::array_values(&rows)?
                    .into_iter()
                    .map(|row| row.read("id")?.text())
                    .collect::<Result<Vec<_>, _>>()?
            };
            owner
                .base
                .store
                .update_orders_status(&ids.iter().map(String::as_str).collect::<Vec<_>>(), status)
                .map_err(|error| Rc::new(error.into()))
        }))
    }
    fn capture_emit(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |msg| {
            owner.emit(msg)?;
            Ok(EventValue::Undefined)
        }))
    }
    fn capture_is_algo(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        // SDK callers arrive with the decoded string spelling, class callers with the native enum.
        Ok(Rc::new(|value| {
            let kind = typed::<safe_cex_types::OrderType>(&value)?;
            Ok(EventValue::Bool(crate::order_logic::is_algo_order_type(
                kind,
            )))
        }))
    }
    fn capture_resolve_id(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |opts| {
            Ok(owner
                .base
                .resolve_caller_client_order_id(&typed::<safe_cex_types::PreparedOrderOpts>(&opts)?)
                .map_err(Rc::new)?
                .map_or(EventValue::Undefined, string))
        }))
    }
    fn capture_format(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |opts| owner.format_order(opts)))
    }
    fn capture_method(
        &mut self,
        method: crate::order_operations::Method,
    ) -> Result<crate::order_operations::Operation, Failure> {
        let owner = self.0.clone();
        let target = match method {
            crate::order_operations::Method::Cancel => Method::Cancel,
            crate::order_operations::Method::Place => Method::PlaceOrder,
            crate::order_operations::Method::ConflictCancel => Method::ConflictCancel,
            crate::order_operations::Method::RegularBatch => Method::RegularBatch,
            crate::order_operations::Method::AlgoBatch => Method::AlgoBatch,
            crate::order_operations::Method::Batch => Method::Batch,
        };
        Ok(Rc::new(move |arg| {
            Ok(Box::pin(class_jobs::reaction(
                owner.invoke(target, vec![arg]),
            )))
        }))
    }
}
impl crate::order_batches::BatchBindings for Context {
    fn capture_assert_ids(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |rows| {
            let opts: Vec<safe_cex_types::PlaceOrderOpts> = typed(&rows)?;
            owner
                .base
                .assert_caller_client_order_ids(&opts)
                .map_err(Rc::new)?;
            Ok(EventValue::Undefined)
        }))
    }
    fn capture_format(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        crate::order_operations::OrderBindings::capture_format(self)
    }
    fn capture_method(
        &mut self,
        method: crate::order_operations::Method,
    ) -> Result<crate::order_operations::Operation, Failure> {
        crate::order_operations::OrderBindings::capture_method(self, method)
    }
    fn capture_post(
        &mut self,
        _venue: crate::order_operations::Venue,
    ) -> Result<crate::order_batches::Post, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |path, args| {
            owner.http(Client::Unlimited, HttpMethod::Post, path, Some(args))
        }))
    }
    fn capture_emit(&mut self) -> Result<crate::order_operations::Sync, Failure> {
        crate::order_operations::OrderBindings::capture_emit(self)
    }
    fn capture_post_only(&mut self) -> Result<crate::order_batches::Classifier, Failure> {
        Ok(Rc::new(|rejection| {
            let value = match rejection {
                crate::order_batches::Rejection::Error(error) => {
                    json!({"response":error.response_json_view(),"code":error.metadata.code.as_ref().map(safe_cex_exchange::js_runtime::value::to_json).transpose().map_err(|e| Rc::new(e.to_exchange_error()))?,"message":error.message})
                }
                crate::order_batches::Rejection::Row(row) => json_value(&row)?,
            };
            crate::order_logic::is_post_only_rejection_error(Some(&value))
                .map_err(|e| failure(e.message))
        }))
    }
    fn stringify(&mut self, v: EventValue) -> Result<EventValue, Failure> {
        Ok(string(crate::public_classes::encode(&v)?))
    }
    fn positions(&mut self) -> Result<EventValue, Failure> {
        self.0.store_field("positions")
    }
    fn market(&mut self, opts: &EventValue) -> Result<EventValue, Failure> {
        self.0
            .store_field("marketsBySymbol")?
            .read(&opts.read("symbol")?.text()?)
    }
    fn adjust(&mut self, value: EventValue, precision: EventValue) -> Result<EventValue, Failure> {
        Ok(number(safe_cex_num::adjust(
            value.number()?,
            precision.number()?,
        )))
    }
    fn orders(&mut self) -> Result<EventValue, Failure> {
        self.0.store_field("orders")
    }
    fn capture_log(&mut self) -> Result<crate::order_batches::Log, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |msg, severity| {
            owner.base.log(&msg, severity.as_str()).map_err(Rc::new)
        }))
    }
}
impl crate::rest_history::HistoryBindings for Context {
    fn public_only(&mut self) -> Result<bool, Failure> {
        Ok(self.0.public_only())
    }
    fn capture_get(
        &mut self,
        client: crate::rest_history::HistoryClient,
    ) -> Result<crate::rest_history::HistoryGet, Failure> {
        let owner = self.0.clone();
        let client = match client {
            crate::rest_history::HistoryClient::Private => Client::Private,
            crate::rest_history::HistoryClient::Public => Client::Public,
        };
        Ok(Rc::new(move |path, args| {
            owner.http(client, HttpMethod::Get, path, Some(args))
        }))
    }
    fn capture_log(&mut self) -> Result<crate::rest_history::HistoryLog, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |msg, severity| {
            owner.base.log(&msg, severity.as_str()).map_err(Rc::new)
        }))
    }
    fn capture_emit(&mut self) -> Result<crate::rest_history::HistoryEmit, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |msg| owner.emit(msg)))
    }
    fn split(&mut self, from: EventValue, to: EventValue, max: f64) -> Result<EventValue, Failure> {
        decoded(safe_cex_candle_cache::calendar::split_time_range(
            from.number()?,
            to.number()?,
            max,
        ))
    }
    fn normalize_side(&mut self, raw: EventValue) -> Result<EventValue, Failure> {
        crate::rest_history::normalize_side(raw)
    }
    fn category(&mut self, raw: EventValue) -> Result<EventValue, Failure> {
        crate::rest_history::income_category(raw)
    }
    fn map_order(&mut self, raw: EventValue) -> Result<EventValue, Failure> {
        Ok(
            crate::order_history::map_binance_historical_order(&raw, &self.0.prototype_realm)?
                .unwrap_or(EventValue::Undefined),
        )
    }
    fn page_resolver(
        &mut self,
        method: crate::rest_history::IncomePageMethod,
    ) -> crate::rest_history::PageResolver {
        let owner = self.0.clone();
        Rc::new(move || {
            let owner = owner.clone();
            Ok(Rc::new(move |opts| {
                Ok(Box::pin(class_jobs::reaction(owner.invoke(
                    if method == crate::rest_history::IncomePageMethod::Rethrowing {
                        Method::IncomePage
                    } else {
                        Method::IncomeHistory
                    },
                    vec![opts],
                ))))
            }))
        })
    }
    fn paginate(
        &mut self,
        fetch: crate::rest_history::PageCall,
        opts: EventValue,
    ) -> Result<crate::rest_history::HistoryFuture, Failure> {
        self.0.imports.paginate(fetch, opts)
    }
    fn iso(&mut self, time: EventValue) -> Result<String, Failure> {
        self.0.imports.iso(time)
    }
}
impl crate::lifecycle::LifecycleBindings for Context {
    fn read(&mut self, key: crate::lifecycle::LifecycleRead) -> Result<EventValue, Failure> {
        use crate::lifecycle::LifecycleRead::{
            Disposed, Loaded, NeedsOverlay, OrdersStale, PrivateConnected, PublicOnly,
        };
        Ok(match key {
            NeedsOverlay => EventValue::Bool(self.0.needs_overlay()),
            Disposed => EventValue::Bool(self.0.base.is_disposed()),
            PublicOnly => EventValue::Bool(self.0.public_only()),
            PrivateConnected => EventValue::Bool(self.0.private_socket().base.connected()),
            OrdersStale => EventValue::Bool(self.0.orders_stale.get()),
            Loaded => self.0.store_field("loaded")?,
        })
    }
    fn capture(
        &mut self,
        key: crate::lifecycle::LifecycleSync,
    ) -> Result<crate::lifecycle::LifecycleCall, Failure> {
        use crate::lifecycle::LifecycleSync::{
            Emit, Log, Paused, PrivateConnect, PublicConnect, SetSetting, TickInterval, Update,
            Version,
        };
        let owner = self.0.clone();
        Ok(Rc::new(move |args| {
            let arg = |i| args.get(i).cloned().unwrap_or(EventValue::Undefined);
            match key {
                Update => owner.update_store(arg(0))?,
                Log => owner.base.log(&arg(0).text()?, "info").map_err(Rc::new)?,
                PublicConnect => owner
                    .public_socket()
                    .connect_and_subscribe()
                    .map_err(Rc::new)?,
                PrivateConnect => {
                    drop(owner.private_socket().connect_and_subscribe());
                }
                SetSetting => owner
                    .base
                    .store
                    .set_setting(&arg(0).text()?, arg(1).truthy())
                    .map_err(|error| Rc::new(error.into()))?,
                Paused => return Ok(EventValue::Bool(owner.paused())),
                Version => return Ok(number(owner.position_state_version())),
                TickInterval => {
                    return Ok(owner
                        .base
                        .get_tick_interval()
                        .map_or(EventValue::Undefined, number));
                }
                Emit => owner.emit(arg(0))?,
            }
            Ok(EventValue::Undefined)
        }))
    }
    fn capture_async(
        &mut self,
        key: crate::lifecycle::LifecycleAsync,
    ) -> Result<crate::lifecycle::LifecycleAsyncCall, Failure> {
        use crate::lifecycle::LifecycleAsync::{
            Balance, External, Markets, Orders, Overlay, PositionMode, Positions, PositionsBalance,
            RetryOrders, StartPrivate, StartPublicOnly, Tick, Tickers,
        };
        let owner = self.0.clone();
        Ok(Rc::new(move |args| {
            if key == External {
                return owner.external();
            }
            let method = match key {
                Overlay => Method::Overlay,
                Markets => Method::Markets,
                Tickers => Method::Tickers,
                StartPrivate => Method::StartPrivate,
                StartPublicOnly => Method::StartPublicOnly,
                PositionMode => Method::PositionMode,
                Tick => Method::Tick,
                Orders => Method::Orders,
                RetryOrders => Method::RetryOrders,
                Positions => Method::Positions,
                PositionsBalance => Method::PositionsBalance,
                Balance => Method::Balance,
                External => unreachable!(),
            };
            Ok(Box::pin(class_jobs::reaction(owner.invoke(method, args))))
        }))
    }
    fn now(&mut self) -> Result<f64, Failure> {
        Ok(self.0.runtime.now_ms())
    }
    fn tick_resolver(&mut self) -> crate::lifecycle::TickResolver {
        let owner = self.0.clone();
        Rc::new(move || {
            let owner = owner.clone();
            Ok(Rc::new(move |args| {
                Ok(Box::pin(class_jobs::reaction(
                    owner.invoke(Method::Tick, args),
                )))
            }))
        })
    }
    fn schedule(
        &mut self,
        callback: crate::lifecycle::TickTimer,
        interval: EventValue,
    ) -> Result<(), Failure> {
        let root = self.0.root.get().and_then(Weak::upgrade);
        self.0.imports.loop_timer(
            Rc::new(move || {
                let lease = root.clone();
                let work = callback()?;
                Ok(Box::pin(async move {
                    let _root_lease = lease;
                    work.await
                }))
            }),
            interval,
        )
    }
}

struct RestContext(Rc<ExchangeState>);
impl crate::public_rest::RestBindings for RestContext {
    fn get(
        &mut self,
        client: crate::public_rest::RestClient,
        path: &'static str,
        params: Option<crate::public_rest::CandleParams>,
    ) -> Result<crate::public_rest::RestFuture<'_>, Failure> {
        let params = params.map(|p| {
            let value = record([
                ("symbol", string(p.symbol)),
                ("interval", string(p.interval)),
                ("startTime", number(p.start_time)),
                ("endTime", number(p.end_time)),
            ]);
            if let Some(limit) = p.limit {
                value.write("limit", number(limit));
            }
            value
        });
        let request = self.0.http(
            match client {
                crate::public_rest::RestClient::Public => Client::Public,
                _ => Client::Private,
            },
            HttpMethod::Get,
            path,
            params,
        )?;
        Ok(Box::pin(async move {
            let v = request.await?;
            if matches!(v, EventValue::Undefined) {
                Ok(None)
            } else {
                json_value(&v).map(Some)
            }
        }))
    }
    fn is_public_data_only(&mut self) -> Result<bool, Failure> {
        Ok(self.0.public_only())
    }
    fn markets_by_symbol(
        &mut self,
    ) -> Result<Rc<std::collections::BTreeMap<String, crate::public_rest::MarketIdentity>>, Failure>
    {
        Ok(Rc::new(
            self.0
                .base
                .store
                .markets_by_symbol()
                .map_err(|error| {
                    std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                })?
                .iter()
                .map(|(key, m)| {
                    (
                        key.clone(),
                        crate::public_rest::MarketIdentity {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                        },
                    )
                })
                .collect(),
        ))
    }
    fn emit(&mut self, error: crate::exchange_error::ErrorEmission) -> Result<(), Failure> {
        match error {
            crate::exchange_error::ErrorEmission::Error(error, prefix) => self
                .0
                .base
                .emit_and_log_error(
                    safe_cex_exchange::error_reporting::ExchangeErrorInput::Error(Box::new(
                        (*error).clone(),
                    )),
                    prefix.as_deref(),
                    None,
                )
                .map_err(Rc::new),
            crate::exchange_error::ErrorEmission::Message(msg) => {
                emit_json(&self.0.base, &msg.unwrap_or(Value::Null)).map_err(Rc::new)
            }
        }
    }
    fn fallback_markets(&mut self) -> Result<Rc<Vec<crate::public_rest::RestMarket>>, Failure> {
        Ok(Rc::new(
            self.0
                .base
                .store
                .markets()
                .map_err(|error| {
                    std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                })?
                .iter()
                .map(|m| crate::public_rest::RestMarket {
                    id: m.id.clone(),
                    symbol: Some(json!(m.symbol)),
                    base: Some(json!(m.base)),
                    quote: Some(json!(m.quote)),
                    active: m.active,
                    is_spot: m.is_spot,
                    precision: m.precision.clone(),
                    amount: m.limits.amount.clone(),
                    leverage_min: m.limits.leverage.min,
                    leverage_max: json!(m.limits.leverage.max),
                })
                .collect(),
        ))
    }
    fn fallback_tickers(&mut self) -> Result<Rc<Vec<safe_cex_types::Ticker>>, Failure> {
        Ok(Rc::new(
            self.0
                .base
                .store
                .tickers()
                .map_err(|error| {
                    std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                })?
                .to_vec(),
        ))
    }
    fn dayjs(&mut self, v: Option<f64>) -> Result<f64, Failure> {
        self.0.imports.dayjs(v)
    }
    fn subtract(&mut self, end: f64, amount: f64, unit: char) -> Result<f64, Failure> {
        self.0.imports.subtract(end, amount, unit)
    }
}
fn market_value(m: &crate::public_rest::RestMarket) -> Result<EventValue, Failure> {
    // Only the Spot literal writes isSpot (binancespot.exchange.ts:220); Futures/PM omit the key.
    let is_spot = m.is_spot.map(|v| ("isSpot", EventValue::Bool(v)));
    Ok(record(
        [
            ("id", string(&m.id)),
            (
                "symbol",
                m.symbol
                    .clone()
                    .map_or(EventValue::Undefined, crate::private_session::native),
            ),
            (
                "base",
                m.base
                    .clone()
                    .map_or(EventValue::Undefined, crate::private_session::native),
            ),
            (
                "quote",
                m.quote
                    .clone()
                    .map_or(EventValue::Undefined, crate::private_session::native),
            ),
            ("active", EventValue::Bool(m.active)),
        ]
        .into_iter()
        .chain(is_spot)
        .chain([
            ("precision", decoded(&m.precision)?),
            (
                "limits",
                record([
                    ("amount", decoded(&m.amount)?),
                    (
                        "leverage",
                        record([
                            ("min", number(m.leverage_min)),
                            (
                                "max",
                                crate::private_session::native(m.leverage_max.clone()),
                            ),
                        ]),
                    ),
                ]),
            ),
        ]),
    ))
}
impl ExchangeState {
    pub fn public_only(&self) -> bool {
        self.base.options().public_data_only == Some(true) || !self.base.can_sign()
    }
    fn needs_overlay(&self) -> bool {
        self.base.uses_external_public_data() && !self.public_only()
    }
    fn external(self: &Rc<Self>) -> Result<NativeFuture<EventValue>, Failure> {
        let owner = self.clone();
        Ok(Box::pin(async move {
            let imports = owner.imports.clone();
            let used = owner
                .base
                .init_external_public_data(move |options| imports.external_provider(options))
                .await
                .map_err(Rc::new)?;
            Ok(EventValue::Bool(used))
        }))
    }
    pub fn asset_price(&self, asset: &str) -> Result<f64, Failure> {
        crate::spot_valuation::get_asset_usd_price(asset, |symbol| {
            Ok(self
                .base
                .store
                .tickers_by_symbol()
                .map_err(|error| Rc::new(ExchangeError::from(error)))?
                .get(symbol)
                .map(|t| t.last))
        })
    }
    pub fn format_order(&self, opts: EventValue) -> Result<EventValue, Failure> {
        let options: safe_cex_types::PlaceOrderOpts = typed(&opts)?;
        match self.venue {
            Venue::Futures => decoded(
                crate::futures_orders::format_futures_create_order(&self.base, &options, || {
                    self.imports.uuid()
                })
                .map_err(Rc::new)?,
            ),
            Venue::PM => decoded(
                crate::pm_orders::format_pm_create_order(&self.base, &options, || {
                    self.imports.uuid()
                })
                .map_err(Rc::new)?,
            ),
            Venue::Spot => decoded(
                crate::spot_orders::format_spot_create_order(&self.base, &options, || {
                    self.imports.uuid()
                })
                .map_err(Rc::new)?,
            ),
        }
    }
    pub async fn fetch_markets(self: &Rc<Self>) -> Result<EventValue, Failure> {
        let mut context = RestContext(self.clone());
        let rows = match self.venue {
            Venue::Futures => crate::public_rest::fetch_futures_markets(&mut context).await?,
            Venue::PM => crate::public_rest::fetch_pm_markets(&mut context).await?,
            Venue::Spot => crate::public_rest::fetch_spot_markets(&mut context).await?,
        };
        let mut markets = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let mut invalid = [
                ("base", row.base.as_ref()),
                ("quote", row.quote.as_ref()),
                ("symbol", row.symbol.as_ref()),
            ]
            .into_iter()
            .filter(|(_, value)| value.and_then(Value::as_str).is_none())
            .map(|(field, value)| {
                format!(
                    "{field}={}: expected string",
                    value.map_or_else(|| "undefined".into(), Value::to_string)
                )
            })
            .collect::<Vec<_>>();
            if safe_cex_num::numeric_ingress::js_value_as_f64(&row.leverage_max).is_none() {
                invalid.push(format!(
                    "limits.leverage.max={}: expected number",
                    row.leverage_max
                ));
            }
            if !invalid.is_empty() {
                let symbol = row
                    .symbol
                    .as_ref()
                    .map_or_else(|| "undefined".into(), Value::to_string);
                emit_json(
                    &self.base,
                    &json!(format!(
                        "Refused Binance market row {index} symbol={symbol}: {}",
                        invalid.join(", ")
                    )),
                )
                .map_err(Rc::new)?;
                continue;
            }
            markets.push(market_value(row)?);
        }
        Ok(EventValue::array(markets))
    }
    pub async fn fetch_tickers(self: &Rc<Self>) -> Result<EventValue, Failure> {
        let mut context = RestContext(self.clone());
        let rows = match self.venue {
            Venue::Futures => crate::public_rest::fetch_futures_tickers(&mut context).await?,
            Venue::PM => crate::public_rest::fetch_pm_tickers(&mut context).await?,
            Venue::Spot => crate::public_rest::fetch_spot_tickers(&mut context).await?,
        };
        decoded(rows.as_ref())
    }
    pub async fn fetch_ohlcv(self: &Rc<Self>, options: EventValue) -> Result<EventValue, Failure> {
        let opts = typed(&options)?;
        let mut context = RestContext(self.clone());
        decoded(match self.venue {
            Venue::Futures => crate::public_rest::fetch_futures_ohlcv(&mut context, &opts).await?,
            Venue::PM => crate::public_rest::fetch_pm_ohlcv(&mut context, &opts).await?,
            Venue::Spot => crate::public_rest::fetch_spot_ohlcv(&mut context, &opts).await?,
        })
    }
    pub async fn load_overlay(self: &Rc<Self>) -> Result<(), Failure> {
        let owner = self.clone();
        let get = move |path| {
            let request = owner.http(Client::Private, HttpMethod::Get, path, None)?;
            Ok::<_, Failure>(async move {
                let value = request.await?;
                if matches!(value, EventValue::Undefined) {
                    Ok(None)
                } else {
                    json_value(&value).map(Some)
                }
            })
        };
        let store = self.base.clone();
        let load = move |mut loader: crate::account_leverage_overlay::LeverageLoader| async move {
            let future = loader();
            store
                .load_account_market_overlay(Box::pin(async move {
                    class_jobs::reaction(future)
                        .await
                        .map(|overlay| {
                            let overlay = Rc::new(overlay);
                            let transform: safe_cex_exchange::snapshots::AccountMarketOverlay =
                                Rc::new(move |markets| overlay.apply(markets));
                            Some(transform)
                        })
                        .map_err(|e| (*e).clone())
                }))
                .await
                .map_err(Rc::new)
        };
        if self.venue == Venue::PM {
            crate::account_leverage_overlay::load_pm_account_max_leverage(get, load).await
        } else {
            crate::account_leverage_overlay::load_futures_account_max_leverage(get, load).await
        }
    }
    pub async fn validate_account(self: &Rc<Self>) -> Result<EventValue, Failure> {
        use crate::account_validation::{
            AccountValidation, AccountValidationClient, FetchedAccountBalance,
            validate_binance_account, validate_binance_pm_account, validate_binance_spot_account,
        };
        let owner = self.clone();
        let get = move |client: AccountValidationClient, path| {
            let response = owner.http(
                match client {
                    AccountValidationClient::Primary => Client::Primary,
                    AccountValidationClient::Secondary => Client::Secondary,
                    AccountValidationClient::Spot => Client::Private,
                },
                HttpMethod::Get,
                path,
                None,
            );
            async move {
                response
                    .map_err(|e| (*e).clone())?
                    .await
                    .and_then(|v| {
                        if matches!(v, EventValue::Undefined) {
                            Ok(None)
                        } else {
                            json_value(&v).map(Some)
                        }
                    })
                    .map_err(|e| (*e).clone())
            }
        };
        let secondary = || {
            self.base
                .options()
                .secondary_key
                .as_ref()
                .is_some_and(|key| !key.is_empty())
        };
        let emit = |msg| emit_json(&self.base, &msg);
        let result = match self.venue {
            Venue::Futures => {
                let value = validate_binance_account(get, secondary, emit)
                    .await
                    .map_err(Rc::new)?;
                match value {
                    AccountValidation::Valid { balance } => AccountValidation::Valid {
                        balance: balance.map(decoded).transpose()?,
                    },
                    AccountValidation::Invalid { error } => AccountValidation::Invalid { error },
                }
            }
            Venue::PM | Venue::Spot => {
                let snapshot = || {
                    let method = if self.venue == Venue::PM {
                        Method::Balance
                    } else {
                        Method::PositionsBalance
                    };
                    let future = self.invoke(method, vec![]);
                    async move {
                        let result = class_jobs::reaction(future)
                            .await
                            .map_err(|e| (*e).clone())?;
                        let balance = result.read("balance").map_err(|e| (*e).clone())?;
                        Ok(Some(FetchedAccountBalance {
                            balance: (!matches!(balance, EventValue::Undefined)).then_some(balance),
                        }))
                    }
                };
                if self.venue == Venue::PM {
                    validate_binance_pm_account(get, secondary, snapshot, emit)
                        .await
                        .map_err(Rc::new)?
                } else {
                    validate_binance_spot_account(get, snapshot, emit)
                        .await
                        .map_err(Rc::new)?
                }
            }
        };
        Ok(match result {
            AccountValidation::Valid { balance } => record([
                ("error", string("")),
                ("balance", balance.unwrap_or(EventValue::Undefined)),
            ]),
            AccountValidation::Invalid { error } => {
                record([("error", crate::private_session::native(error))])
            }
        })
    }
}

impl PrivateParent for ExchangeState {
    fn socket_parent(&self) -> Option<Rc<dyn WsParent>> {
        self.this.upgrade().map(|owner| owner as Rc<dyn WsParent>)
    }
    fn options(&self) -> Result<EventValue, Failure> {
        Ok(self.options.clone())
    }
    fn orders(&self) -> Result<EventArray, Failure> {
        let EventValue::Array(rows) = self.store_field("orders")? else {
            return Err(failure("orders array"));
        };
        Ok(rows)
    }
    fn positions(&self) -> Result<EventArray, Failure> {
        let EventValue::Array(rows) = self.store_field("positions")? else {
            return Err(failure("positions array"));
        };
        Ok(rows)
    }
    fn balance(&self) -> Result<EventValue, Failure> {
        self.store_field("balance")
    }
    fn loaded(&self) -> Result<EventValue, Failure> {
        self.store_field("loaded")
    }
    fn capture_store(
        &self,
        operation: crate::private_events::PrivateOperation,
    ) -> Result<crate::private_events::PrivateHandler, Failure> {
        let owner = self.this.upgrade().expect("live owner");
        Ok(Rc::new(move |args| {
            use crate::private_events::PrivateOperation::{
                AddOrder, AddPosition, Fill, MarkPosition, OrderStatus, ThrottleOrders,
                ThrottlePositions, UpdateOrder, UpdatePosition, UpdateStore,
            };
            let arg = |i| args.get(i).cloned().unwrap_or(EventValue::Undefined);
            let patch = |v: &EventValue| -> Result<ModelPatch, Failure> {
                let mut out = ModelPatch::default();
                for (k, v) in v.spread() {
                    if let EventValue::Number(n) = v {
                        out.set_number(k, n)
                    } else {
                        out.set_json(k, json_value(&v)?);
                    }
                }
                Ok(out)
            };
            match operation {
                UpdateStore => owner.update_store(arg(0)),
                AddOrder => owner
                    .base
                    .store
                    .add_or_update_order(typed(&arg(0))?)
                    .map_err(|error| Rc::new(error.into())),
                UpdateOrder => owner
                    .base
                    .store
                    .update_order(&arg(0).read("id")?.text()?, &patch(&arg(1))?)
                    .map_err(|error| Rc::new(error.into())),
                OrderStatus => {
                    let status = typed::<safe_cex_types::OrderStatus>(&arg(1))?;
                    owner
                        .base
                        .store
                        .update_order_status(&arg(0).read("id")?.text()?, status)
                        .map_err(|error| Rc::new(error.into()))
                }
                AddPosition => owner
                    .base
                    .store
                    .add_or_update_position(typed(&arg(0))?)
                    .map_err(|error| Rc::new(error.into())),
                UpdatePosition => {
                    let position = arg(0);
                    let mut changes = arg(1).spread();
                    if owner.venue == Venue::PM
                        && changes.get("info").is_some_and(|info| info.same(&position))
                    {
                        // The source creates position.info === position. The owned
                        // Store keeps the chosen depth-one snapshot of the updated
                        // fields, excluding info so subsequent frames stay bounded.
                        let mut snapshot = position.spread();
                        snapshot.remove("info");
                        for (field, value) in &changes {
                            if field != "info" {
                                snapshot.insert(field.clone(), value.clone());
                            }
                        }
                        changes.insert("info".into(), EventValue::record_object(snapshot));
                    }
                    owner
                        .base
                        .store
                        .update_position(
                            &position.read("id")?.text()?,
                            &patch(&EventValue::record_object(changes))?,
                        )
                        .map_err(|error| Rc::new(error.into()))
                }
                Fill => owner.fill(arg(0)),
                MarkPosition => {
                    owner.mark_position_state_updated();
                    Ok(())
                }
                ThrottleOrders => owner.private_socket().throttled_orders(),
                ThrottlePositions => owner.private_socket().throttled_positions(),
            }
        }))
    }
    fn fill(&self, payload: EventValue) -> Result<(), Failure> {
        self.base.emit("fill", payload).map_err(Rc::new)
    }
    fn asset_price(&self, asset: &EventValue) -> Result<f64, Failure> {
        self.asset_price(&asset.text()?)
    }
    fn mark_position(&self) -> Result<(), Failure> {
        self.mark_position_state_updated();
        Ok(())
    }
    fn position_version(&self) -> Result<f64, Failure> {
        Ok(self.position_state_version())
    }
    fn paused(&self) -> Result<bool, Failure> {
        Ok(self.paused())
    }
    fn orders_stale(&self) -> Result<bool, Failure> {
        Ok(self.orders_stale.get())
    }
    fn fetch_positions(&self) -> Result<NativeFuture<EventValue>, Failure> {
        let owner = self.this.upgrade().expect("live owner");
        Ok(Box::pin(owner.invoke(Method::Positions, vec![])))
    }
    fn fetch_orders(&self) -> Result<NativeFuture<EventValue>, Failure> {
        let owner = self.this.upgrade().expect("live owner");
        Ok(Box::pin(owner.invoke(Method::Orders, vec![])))
    }
    fn replace_positions(&self, positions: EventValue) -> Result<(), Failure> {
        self.base
            .store
            .add_or_update_positions_full(typed(&positions)?)
            .map_err(|error| Rc::new(error.into()))
    }
    fn post_key(&self, path: &'static str) -> Result<crate::private_classes::PinKey, Failure> {
        let request = self.http(Client::Private, HttpMethod::Post, path, None)?;
        Ok(Box::pin(async move {
            let data = request.await?;
            let value = data.read("listenKey")?;
            Ok(crate::listen_key::ListenKeyResponse::Data(
                crate::listen_key::ListenKeyData::Present(Rc::new(RefCell::new(
                    (!matches!(value, EventValue::Undefined)).then_some(value),
                ))),
            ))
        }))
    }
    fn put_key(&self, path: &'static str) -> Result<NativeFuture<()>, Failure> {
        let request = self.http(Client::Private, HttpMethod::Put, path, None)?;
        Ok(Box::pin(async move {
            request.await?;
            Ok(())
        }))
    }
    fn remote_sign(
        &self,
        signer: EventValue,
    ) -> Result<crate::api_factory::RemoteSignCall, Failure> {
        self.imports.remote_sign(signer)
    }
    fn log(&self, message: String, severity: LogSeverity) -> Result<(), Failure> {
        self.base.log(&message, severity.as_str()).map_err(Rc::new)
    }
}
impl WsParent for ExchangeState {
    fn name(&self) -> &str {
        match self.venue {
            Venue::Futures => "binance",
            Venue::PM => "binancepm",
            Venue::Spot => "binancespot",
        }
    }
    fn is_disposed(&self) -> bool {
        self.base.is_disposed()
    }
    fn options(&self) -> ExchangeOptions {
        self.base.options()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.base.store.clone()
    }
    fn is_private_socket(&self, socket: &BaseWebSocket) -> bool {
        socket.id == 3
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.resync.clone()
    }
    fn fetch_and_set_private_snapshot(
        &self,
        tries: usize,
    ) -> safe_cex_exchange::runtime::ExchangeFuture<'static, ()> {
        let owner = self.this.upgrade().expect("live owner");
        Box::pin(async move {
            if owner.public_only() || owner.base.is_disposed() {
                return Ok(());
            }
            for attempt in 0..tries {
                if owner.base.is_disposed() {
                    return Ok(());
                }
                let result: Result<(), Failure> = async {
                    let orders = class_jobs::reaction(owner.invoke(Method::Orders, vec![])).await?;
                    if owner.base.is_disposed() {
                        return Ok(());
                    }
                    let mut loaded = owner
                        .base
                        .store
                        .loaded()
                        .map_err(|error| {
                            std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                        })?
                        .clone();
                    loaded.orders = true;
                    owner.update_store(record([("orders", orders), ("loaded", decoded(loaded)?)]))
                }
                .await;
                match result {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        if owner.base.is_disposed() {
                            return Ok(());
                        }
                        owner.base.emit_and_log_error(
                            safe_cex_exchange::error_reporting::ExchangeErrorInput::Error(
                                Box::new((*error).clone()),
                            ),
                            None,
                            None,
                        )?;
                        if attempt + 1 < tries {
                            owner.runtime.sleep(1000).await?;
                        }
                    }
                }
            }
            Ok(())
        })
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.base.log(message, severity.as_str())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        self.base.emit(
            event,
            safe_cex_exchange::js_runtime::value::Js::str(message),
        )
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.base.sync_socket_health_to_store()
    }
    fn update_latency(&self, latency: f64) -> Result<(), ExchangeError> {
        self.base
            .store
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
        self.base.emit_ws_reconnected()
    }
    fn emit_and_log_error(
        &self,
        message: &str,
        formatted: Option<&str>,
        context: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message.into()),
            formatted,
            context,
        )
    }
}
impl ExchangeState {
    pub async fn retry_orders(self: &Rc<Self>) -> Result<(), Failure> {
        let state = self.retry.clone();
        let fetch = || Ok(class_jobs::reaction(self.invoke(Method::Orders, vec![])));
        let update = || {
            let owner = self.clone();
            Ok(move |orders, loaded| {
                owner.update_store(record([("orders", orders), ("loaded", decoded(loaded)?)]))
            })
        };
        let loaded = || {
            Ok(self
                .base
                .store
                .loaded()
                .map_err(|error| {
                    std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                })?
                .clone())
        };
        let now = || Ok(self.runtime.now_ms());
        if self.venue == Venue::PM {
            crate::orders_retry::retry_pm_orders_snapshot(&state, now, fetch, update, loaded).await
        } else {
            crate::orders_retry::retry_futures_orders_snapshot(&state, now, fetch, update, loaded)
                .await
        }
    }
    pub async fn set_leverage(
        self: &Rc<Self>,
        symbol: String,
        leverage: f64,
    ) -> Result<(), Failure> {
        let post = |path, request: crate::leverage::BinanceLeverageRequest| {
            let response = self.http(
                Client::Private,
                HttpMethod::Post,
                path,
                Some(record([
                    ("symbol", string(request.symbol)),
                    ("leverage", number(request.leverage)),
                ])),
            );
            async move {
                response
                    .map_err(|e| (*e).clone())?
                    .await
                    .map(|_| ())
                    .map_err(|e| (*e).clone())
            }
        };
        let emit = |msg| emit_json(&self.base, &msg);
        if self.venue == Venue::PM {
            crate::leverage::set_binance_pm_leverage(
                self.base.store.as_ref(),
                &symbol,
                leverage,
                post,
                emit,
            )
            .await
        } else {
            crate::leverage::set_binance_leverage(
                self.base.store.as_ref(),
                &symbol,
                leverage,
                post,
                emit,
            )
            .await
        }
        .map_err(Rc::new)
    }
    pub async fn change_margin(
        self: &Rc<Self>,
        isolated: bool,
        symbol: Option<String>,
    ) -> Result<(), Failure> {
        if self.venue == Venue::PM {
            return crate::margin_mode::change_binance_pm_margin_type(|message| {
                self.base.emit_and_log_error(
                    safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message.into()),
                    None,
                    None,
                )
            })
            .await
            .map_err(Rc::new);
        }
        crate::margin_mode::change_binance_margin_type(
            isolated,
            symbol.as_deref(),
            |path, request| {
                let response = self.http(
                    Client::Private,
                    HttpMethod::Post,
                    path,
                    Some(record([
                        ("symbol", string(request.symbol)),
                        (
                            "marginType",
                            string(match request.margin_type {
                                crate::margin_mode::BinanceMarginType::Isolated => "ISOLATED",
                                crate::margin_mode::BinanceMarginType::Crossed => "CROSSED",
                            }),
                        ),
                    ])),
                );
                async move {
                    response
                        .map_err(|e| (*e).clone())?
                        .await
                        .map(|_| ())
                        .map_err(|e| (*e).clone())
                }
            },
            |msg| emit_json(&self.base, &msg),
        )
        .await
        .map_err(Rc::new)
    }
    pub async fn change_mode(
        self: &Rc<Self>,
        hedged: bool,
        symbol: Option<String>,
    ) -> Result<(), Failure> {
        let post = |path, request: Value| {
            let response = self.http(
                Client::Private,
                HttpMethod::Post,
                path,
                Some(crate::private_session::native(request)),
            );
            async move {
                response
                    .map_err(|e| (*e).clone())?
                    .await
                    .map(|_| ())
                    .map_err(|e| (*e).clone())
            }
        };
        let emit = |msg| emit_json(&self.base, &msg);
        if self.venue == Venue::PM {
            crate::pm_position_mode::change_binance_pm_position_mode(
                self.base.store.as_ref(),
                hedged,
                post,
                emit,
            )
            .await
        } else {
            crate::futures_position_mode::change_binance_position_mode(
                self.base.store.as_ref(),
                hedged,
                symbol.as_deref(),
                post,
                |hedged, symbol| {
                    crate::private_session::apply_confirmed_mode(
                        hedged,
                        symbol.map(str::to_owned),
                        self.as_ref(),
                    )
                    .map_err(|e| (*e).clone())
                },
                emit,
            )
            .await
        }
        .map_err(Rc::new)
    }
    pub async fn cancel_symbol(self: &Rc<Self>, symbol: String) -> Result<(), Failure> {
        let view = || {
            Ok(self
                .base
                .store
                .orders()?
                .iter()
                .cloned()
                .map(|order| Rc::new(RefCell::new(order)))
                .collect::<Vec<_>>())
        };
        let delete = |path, request: crate::cancel_symbol_orders::CancelSymbolRequest| {
            let response = self.http(
                Client::Private,
                HttpMethod::Delete,
                path,
                Some(record([("symbol", string(request.symbol))])),
            )?;
            Ok::<_, Failure>(async move { response.await.map(|_| ()) })
        };
        let update = |rows: &[crate::cancel_symbol_orders::CancelOrderHandle], status| {
            let ids = rows
                .iter()
                .map(|r| r.borrow().id.clone())
                .collect::<Vec<_>>();
            self.base
                .store
                .update_orders_status(&ids.iter().map(String::as_str).collect::<Vec<_>>(), status)
        };
        let emit = |msg| self.emit(msg);
        match self.venue {
            Venue::Futures => {
                crate::cancel_symbol_orders::cancel_binance_symbol_orders(
                    view, &symbol, delete, update, emit,
                )
                .await
            }
            Venue::PM => {
                crate::cancel_symbol_orders::cancel_binance_pm_symbol_orders(
                    self.base.store.as_ref(),
                    &symbol,
                    delete,
                    emit,
                )
                .await
            }
            Venue::Spot => {
                crate::cancel_symbol_orders::cancel_binance_spot_symbol_orders(
                    self.base.store.as_ref(),
                    &symbol,
                    delete,
                    emit,
                )
                .await
            }
        }
        .map_err(|error| match error {
            crate::cancel_symbol_orders::CancelSymbolError::Request(error)
            | crate::cancel_symbol_orders::CancelSymbolError::Emission(error) => error,
            crate::cancel_symbol_orders::CancelSymbolError::Store(error) => {
                Rc::new((*error).into())
            }
        })
    }
}
impl crate::private_session::ModeBindings for ExchangeState {
    fn markets(&self) -> Result<EventValue, Failure> {
        self.store_field("markets")
    }
    fn leverage(&self) -> Result<EventValue, Failure> {
        self.store_field("fetchedLeverage")
    }
    fn positions(&self) -> Result<EventValue, Failure> {
        self.store_field("positions")
    }
    fn id(
        &self,
        symbol: EventValue,
        side: EventValue,
        mode: safe_cex_types::PositionMode,
    ) -> Result<EventValue, Failure> {
        crate::position_snapshots::PositionSnapshotBindings::id(self, symbol.text()?, side, mode)
            .map(string)
    }
    fn setting(&self, hedged: bool) -> Result<(), Failure> {
        self.base
            .store
            .set_setting("isHedged", hedged)
            .map_err(|error| Rc::new(error.into()))
    }
    fn update(&self, value: EventValue) -> Result<(), Failure> {
        self.update_store(value)
    }
}

fn emit_json(base: &BaseExchange, value: &Value) -> Result<(), ExchangeError> {
    // The source forwards raw msg; Base owns its kind-dependent formatting and reporting.
    base.emit_and_log_error(
        safe_cex_exchange::error_reporting::ExchangeErrorInput::Raw(
            crate::private_session::native(value.clone()),
        ),
        None,
        None,
    )
}

impl ExchangeState {
    pub fn get_order_position_side(
        &self,
        opts: &safe_cex_types::PlaceOrderOpts,
    ) -> Result<&'static str, ExchangeError> {
        Ok(if self.venue == Venue::PM {
            crate::pm_order_logic::get_order_position_side(
                self.base.store.options()?.is_hedged,
                opts,
            )
        } else {
            crate::order_logic::get_order_position_side(self.base.store.options()?.is_hedged, opts)
        })
    }
    pub fn format_trailing(
        &self,
        opts: &safe_cex_types::PlaceOrderOpts,
    ) -> Result<EventValue, Failure> {
        let market = self
            .base
            .store
            .markets_by_symbol()
            .map_err(|error| {
                std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
            })?
            .get(&opts.symbol)
            .cloned();
        let ticker = self
            .base
            .store
            .tickers_by_symbol()
            .map_err(|error| {
                std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
            })?
            .get(&opts.symbol)
            .cloned();
        let positions = self
            .base
            .store
            .positions()
            .map_err(|error| {
                std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
            })?
            .to_vec();
        let result = if self.venue == Venue::PM {
            crate::trailing_orders::format_pm_create_trailing_stop_loss_order(
                opts,
                market.as_ref(),
                ticker.as_ref(),
                &positions,
                self.base
                    .store
                    .options()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .is_hedged,
                || self.imports.uuid(),
            )
        } else {
            crate::trailing_orders::format_create_trailing_stop_loss_order(
                opts,
                market.as_ref(),
                ticker.as_ref(),
                &positions,
                self.base
                    .store
                    .options()
                    .map_err(|error| {
                        std::rc::Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?
                    .is_hedged,
                || self.imports.uuid(),
            )
        }
        .map_err(failure)?;
        decoded(result)
    }
    pub fn is_algo_order_type(&self, kind: safe_cex_types::OrderType) -> bool {
        crate::order_logic::is_algo_order_type(kind)
    }
    pub fn apply_confirmed_mode(
        &self,
        hedged: bool,
        symbol: Option<String>,
    ) -> Result<(), Failure> {
        crate::private_session::apply_confirmed_mode(hedged, symbol, self)
    }
    pub fn after_method_called_in_worker(
        &self,
        method: &str,
        args: &[Value],
    ) -> Result<(), Failure> {
        crate::position_mode::after_binance_method_called_in_worker(
            method,
            args,
            |hedged, symbol| self.apply_confirmed_mode(hedged, symbol.map(str::to_owned)),
        )
    }
    pub fn grafana_endpoints_every(&self) -> &'static [&'static str] {
        match self.venue {
            Venue::Futures => crate::binance_types::GRAFANA_ENDPOINTS_EVERY,
            Venue::PM => crate::binancepm_types::GRAFANA_ENDPOINTS_EVERY,
            Venue::Spot => crate::binancespot_types::GRAFANA_ENDPOINTS_EVERY,
        }
    }
    pub fn grafana_endpoints_periodic(&self) -> &'static [&'static str] {
        match self.venue {
            Venue::Futures => crate::binance_types::GRAFANA_ENDPOINTS_PERIODIC,
            Venue::PM => crate::binancepm_types::GRAFANA_ENDPOINTS_PERIODIC,
            Venue::Spot => crate::binancespot_types::GRAFANA_ENDPOINTS_PERIODIC,
        }
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: String,
        callback: Rc<dyn Fn(crate::public_listeners::Book) -> Result<(), Failure>>,
    ) -> Result<crate::public_listeners::Unsubscribe, Failure> {
        if self.venue == Venue::Futures {
            let provider = self.base.external_public_data_provider.borrow().clone();
            if let Some(provider) = provider {
                let runtime = self.runtime.clone();
                let callback = Rc::new(move |book: safe_cex_types::OrderBook| {
                    if let Err(error) = callback(Rc::new(RefCell::new(book))) {
                        runtime.report_exception(&error, &Value::Null.into())
                    }
                });
                let cleanup = provider
                    .listen_order_book(symbol, callback)
                    .map_err(Rc::new)?;
                return Ok(once_cleanup(cleanup));
            }
        }
        self.public_socket().listen_order_book(symbol, callback)
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: EventValue,
        callback: crate::rest_history::CandleCallback,
    ) -> Result<crate::rest_history::CandleCleanup, Failure> {
        if self.venue == Venue::Futures {
            crate::rest_history::listen_candle(opts, callback, &mut Context(self.clone()))
        } else {
            self.public_socket()
                .listen_ohlcv(opts, Rc::new(move |candle| callback(decoded(candle)?)))
        }
    }
}
impl crate::rest_history::CandleBindings for Context {
    fn external(&mut self) -> Result<bool, Failure> {
        Ok(self.0.base.external_public_data_provider.borrow().is_some())
    }
    fn capture_external(&mut self) -> Result<crate::rest_history::CandleListen, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |opts, callback| {
            let provider = owner
                .base
                .external_public_data_provider
                .borrow()
                .clone()
                .ok_or_else(|| failure("external provider absent"))?;
            let runtime = owner.runtime.clone();
            let cleanup = provider
                .listen_ohlcv(
                    typed(&opts)?,
                    Rc::new(move |candle| {
                        if let Err(error) = decoded(candle).and_then(|candle| callback(candle)) {
                            runtime.report_exception(&error, &Value::Null.into())
                        }
                    }),
                )
                .map_err(Rc::new)?;
            Ok(once_cleanup(cleanup))
        }))
    }
    fn capture_public(&mut self) -> Result<crate::rest_history::CandleListen, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |opts, callback| {
            owner
                .public_socket()
                .listen_ohlcv(opts, Rc::new(move |candle| callback(decoded(candle)?)))
        }))
    }
    fn capture_fetch(&mut self) -> Result<crate::rest_history::CandleFetch, Failure> {
        let owner = self.0.clone();
        Ok(Rc::new(move |opts| {
            Ok(Box::pin(class_jobs::reaction(
                owner.invoke(Method::OHLCV, vec![opts]),
            )))
        }))
    }
    fn seed_owner(&mut self) -> crate::rest_history::SeedOwner {
        let owner = self.0.clone();
        let log = owner.clone();
        crate::rest_history::SeedOwner {
            disposed: Rc::new(move || Ok(owner.base.is_disposed())),
            capture_log: Rc::new(move || {
                let owner = log.clone();
                Ok(Rc::new(move |message, severity| {
                    owner.base.log(&message, severity.as_str()).map_err(Rc::new)
                }))
            }),
        }
    }
    fn launch(&mut self, task: crate::rest_history::SeedFuture) -> Result<(), Failure> {
        drop(class_jobs::start(self.0.runtime.clone(), task));
        Ok(())
    }
}

// ts: BinanceExchange
pub struct BinanceExchange(pub Rc<ExchangeOwner>);
impl BinanceExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: ExchangeDependencies,
    ) -> Result<Self, Failure> {
        ExchangeOwner::new(Venue::Futures, name, options, store, deps).map(Self)
    }
}
impl std::ops::Deref for BinanceExchange {
    type Target = Rc<ExchangeOwner>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePMExchange
pub struct BinancePMExchange(pub Rc<ExchangeOwner>);
impl BinancePMExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: ExchangeDependencies,
    ) -> Result<Self, Failure> {
        ExchangeOwner::new(Venue::PM, name, options, store, deps).map(Self)
    }
}
impl std::ops::Deref for BinancePMExchange {
    type Target = Rc<ExchangeOwner>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinanceSpotExchange
pub struct BinanceSpotExchange(pub Rc<ExchangeOwner>);
impl BinanceSpotExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: ExchangeDependencies,
    ) -> Result<Self, Failure> {
        ExchangeOwner::new(Venue::Spot, name, options, store, deps).map(Self)
    }
}
impl std::ops::Deref for BinanceSpotExchange {
    type Target = Rc<ExchangeOwner>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn once_cleanup(
    cleanup: safe_cex_exchange::runtime::ListenerCleanup,
) -> crate::rest_history::CandleCleanup {
    let cleanup = RefCell::new(Some(cleanup));
    Rc::new(move || {
        let pending = cleanup.borrow_mut().take();
        if let Some(cleanup) = pending {
            cleanup().map_err(Rc::new)?;
        }
        Ok(())
    })
}
