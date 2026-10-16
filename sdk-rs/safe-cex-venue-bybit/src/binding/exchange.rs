//! Production BybitExchange: the typed core assembled with its receiver, behind `Exchange`.
use super::{
    globals,
    promise::await_js,
    receiver,
    rt::{BybitExtra, Rt},
    value::{Js, Thrown, from_typed, to_typed},
};
use crate::{exchange::BybitExchange, order_params::BybitPlaceOrderOpts};
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding},
    client_order_id::{ClientOrderIdRuleOverrides, pattern_client_order_id_rule},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    http::{HttpClientOptions, HttpHost},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts, ExternalPublicDataFactory},
        client::JsHttpHost,
        promise::JsFuture,
        scope::ScopeShared,
        store,
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
    websocket::WsRuntime,
};
use safe_cex_store::{Store, virtual_clock::VirtualClock};
use safe_cex_transport::{RestAdapter, TransportRegistry, rate_limit::RateLimitClock};
use safe_cex_types::*;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::Arc,
};

/// Host capabilities the venue cannot construct itself.
pub struct BybitDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// The physical wire under every signed client the venue creates.
    pub wire: Arc<dyn RestAdapter>,
    pub http_host: Rc<dyn HttpHost>,
    /// Clock behind the 5 req/s client limiter.
    pub rate_limit_clock: Arc<dyn RateLimitClock>,
    /// Clock behind the signed clients' retry delays.
    pub retry_clock: Rc<dyn RateLimitClock>,
    /// utils/virtualClock: signing timestamps and the private socket's time sync.
    pub clock: VirtualClock,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
    /// Offer the tradetunnel relay hop to the private socket (`WS_RELAY_PRIVATE`).
    pub relay_enabled: bool,
}

pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: None,
        positions_supported: Some(true),
        supports_public_only: true,
        client_order_id_rule: Some(pattern_client_order_id_rule(
            "Bybit".into(),
            "/^[A-Za-z0-9_-]{1,36}$/".into(),
            Rc::new(|id| {
                (1..=36).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            }),
            ClientOrderIdRuleOverrides::default(),
        )),
    }
}

pub(crate) fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<BybitExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<BybitExchange>(Exchange::place_orders),
        ..Default::default()
    }
}

/// ScopeShared requires a generic client host; Bybit's bodies send through `this.xhr`.
struct NoGenericClient(Rc<dyn ExchangeRuntime>);
impl JsHttpHost for NoGenericClient {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _config: Js) -> JsFuture<'static> {
        Box::pin(async {
            Err(Thrown::error(
                "Error",
                "Bybit sends through this.xhr; the generic js_runtime client is not wired",
            ))
        })
    }
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

impl BybitExchange {
    // ts: BybitExchange.constructor (bybitdemo by name)
    pub fn create(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BybitDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if !matches!(name, ExchangeName::Bybit | ExchangeName::Bybitdemo) {
            return Err(ExchangeError::new(format!(
                "BybitExchange cannot construct {}",
                serde_json::to_value(&name)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            )));
        }
        let runtime = deps.runtime.clone();
        let exchange_runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
        let clock = deps.clock.clone();
        let shared = Rc::new_cyclic(|weak: &Weak<super::rt::Shared>| {
            ScopeShared::new(
                runtime.clone(),
                Rc::new(NoGenericClient(exchange_runtime.clone())),
                store.clone(),
                globals::build(weak.clone(), clock.clone()),
                BybitExtra {
                    venue: RefCell::new(Weak::new()),
                },
            )
        });
        let (receiver, core) = base_exchange::create(
            &shared.sockets,
            BaseExchangeParts {
                name: name.clone(),
                options: options.clone(),
                store: store.clone(),
                settings: settings(),
                external_public_data: deps.external_public_data.clone(),
            },
            from_typed(&name).map_err(err)?,
            from_typed(&options).map_err(err)?,
            store::facade(store.clone()),
        )
        .map_err(err)?;
        let _ = shared.base.set(core.clone());
        let wire = deps.wire.clone();
        let http_host = deps.http_host.clone();
        let retry_clock = deps.retry_clock.clone();
        let api_clock = deps.clock.clone();
        let api_runtime = exchange_runtime.clone();
        let api_store = store.clone();
        let ws_runtime = runtime.clone();
        let private_clock = deps.clock.clone();
        let now_clock = deps.clock.clone();
        // The remote signer names the account; local signing has none.
        let account_name: Option<String> = None;
        let relay_enabled = deps.relay_enabled;
        let venue = BybitExchange::new(
            core,
            runtime.clone(),
            deps.rate_limit_clock.clone(),
            move |options, base, owner| {
                let next_id = api_runtime.clone();
                let signing_clock = api_clock.clone();
                let api = crate::api_factory::create_api(
                    crate::api_factory::BybitApiOptions {
                        http: HttpClientOptions {
                            params_serializer: None,
                            response_transform: None,
                            exchange: base.name.clone(),
                            options: options.clone(),
                            request: Default::default(),
                            provider: Some(ws_runtime.transport()),
                            signer: None,
                            account_name: None,
                            adapter: wire.clone(),
                            host: http_host.clone(),
                            runtime: api_runtime.clone(),
                            store: Some(api_store.clone()),
                            stats: Some(Default::default()),
                            logs: Some(Default::default()),
                            every: vec![],
                            periodic: vec![],
                            #[allow(clippy::arc_with_non_send_sync, reason = "browser-local runtime, as everywhere in this crate")]
                            next_id: Arc::new(move || next_id.unique_id()),
                            signing_now: Arc::new(move || signing_clock.get_current_time_ms() as u64),
                        },
                        owner,
                        is_demo: base.is_demo,
                        rest_base_url: None,
                        clock: api_clock.clone(),
                        retry_clock: retry_clock.clone(),
                        payload_signer: base.signers.payload_signer.clone(),
                    },
                    &TransportRegistry::default(),
                )?;
                #[allow(clippy::arc_with_non_send_sync, reason = "RestAdapter supports browser-local hosts")]
                let api: Arc<dyn RestAdapter> = Arc::new(api);
                Ok(api)
            },
            {
                let runtime = runtime.clone();
                move |parent, category| {
                    Rc::new(crate::ws_public::PublicProtocol {
                        parent,
                        runtime: runtime.clone(),
                        category: if category == "inverse" { "inverse" } else { "linear" },
                    })
                }
            },
            move |parent| {
                Rc::new(crate::ws_private::protocol::PrivateProtocol {
                    parent,
                    runtime: runtime.clone(),
                    config: crate::ws_private::protocol::PrivateProtocolConfig {
                        sync_clock: Rc::new(move || {
                            let clock = private_clock.clone();
                            Box::pin(async move {
                                clock.sync_bybit_client_server_time(None).await;
                                Ok(())
                            })
                        }),
                        now_ms: Rc::new(move || now_clock.get_current_time_ms()),
                        account_name,
                        relay_enabled,
                    },
                })
            },
        )
        .map_err(|error| ExchangeError::new(error.to_string()))?;
        for slot in [
            safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
            safe_cex_exchange::socket_lifecycle::SocketSlot::PublicSecondary,
        ] {
            if let Some(socket) = venue.base.get_socket(slot) {
                crate::ws_public::PublicProtocol::register_handlers(&socket);
            }
        }
        *shared.extra.venue.borrow_mut() = Rc::downgrade(&venue);
        *shared.exchange.borrow_mut() = receiver.clone();
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        let rt = Rt {
            shared,
            this: receiver,
        };
        receiver::install(&rt).map_err(err)?;
        let _ = venue.snapshot_refresh.set(
            crate::private_snapshot_refresh::PrivateSnapshotRefresh::new(
                Rc::new(rt.clone()),
                rt.this.clone(),
                exchange_runtime,
            ),
        );
        let _ = venue.js.set(rt);
        Ok(venue)
    }

    fn rt(&self) -> &Rt {
        self.js.get().expect("BybitExchange::create assembles the receiver")
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt().this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt().this.clone();
        Box::pin(async move {
            await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

macro_rules! args {
    ($($e:expr),*) => {
        match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($e?),*]) })() {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        }
    };
}

// ts: base.ts:799 / :989 / :1138 — inherited fallbacks that await a rejection.
fn not_implemented<T: 'static>() -> ExchangeFuture<'static, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

impl Exchange for BybitExchange {
    type PlaceOpts = BybitPlaceOrderOpts;
    fn validate_order_capabilities(&self, options: &Self::PlaceOpts) -> Result<(), ExchangeError> {
        BybitExchange::validate_order_capabilities(self, options)
    }
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn class_name(&self) -> &'static str {
        "BybitExchange"
    }
    // ts: BybitExchange.dispose
    fn dispose(&self) -> Result<(), ExchangeError> {
        crate::stream_delegation::dispose_primary_exchange(
            || {
                if let Some(refresh) = self.snapshot_refresh.get() {
                    refresh.cancel();
                }
                Ok(())
            },
            || {
                Ok(self
                    .order_state
                    .timers()
                    .into_iter()
                    .map(|(order_id, _)| Ok(order_id)))
            },
            |order_id| {
                crate::primary_order_lifecycle::clear_recently_placed_order_reconciliation_timers(
                    &self.order_state,
                    &order_id,
                    |timer: Js| {
                        if let Some(rt) = self.js.get() {
                            rt.clear_timeout(&timer);
                        }
                        Ok::<(), ExchangeError>(())
                    },
                )
            },
            || self.base.dispose(),
            |name| {
                Ok(self.base.get_socket(match name {
                    "privateWebsocket" => safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
                    _ => safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
                }))
            },
            |socket| {
                if let Some(socket) = socket {
                    socket.dispose();
                }
                Ok(())
            },
        )
    }
    // ts: BybitExchange.placeOrder — the ValidatedOrder carries the client-order-ID
    // contract, enforced by BaseExchange.place_order before this call.
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let list: Vec<BybitPlaceOrderOpts> = orders
            .orders()
            .iter()
            .map(|o| o.options().clone())
            .collect();
        let a = args!(js(&list));
        self.invoke("placeOrders", a)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("start", vec![])
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchPositions", a)
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        // ts: BybitExchange.fetchOrders takes no argument.
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        not_implemented()
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol), js(&leverage), opt_js(side));
        self.invoke_unit("setLeverage", a)
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged), opt_js(symbol));
        self.invoke_unit("changePositionMode", a)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&isolated), opt_js(symbol));
        self.invoke_unit("changeMarginType", a)
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        not_implemented()
    }
}
