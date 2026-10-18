//! Production BybitExchange: the typed core assembled with its receiver, behind `Exchange`.
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "receiver_tests.rs"]
mod receiver_tests;

use super::{
    globals,
    promise::await_js,
    receiver,
    rt::{BybitExtra, Rt},
    value::{Js, Thrown, from_typed, to_typed},
};
use crate::{exchange::BybitExchange, order_params::BybitPlaceOrderOpts};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    client_order_id::{ClientOrderIdRuleOverrides, pattern_client_order_id_rule},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    http::{HttpClientOptions, HttpHost},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts, ExternalPublicDataFactory, HostedSigner},
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
    /// Account capability shared with the source receiver, HTTP clients and core.
    pub remote_signer: Option<HostedSigner>,
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

pub(crate) fn capabilities(name: &ExchangeName) -> ExchangeCapabilities {
    let mut capabilities = ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<BybitExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<BybitExchange>(Exchange::place_orders),
        ..Default::default()
    };
    if matches!(
        name,
        ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Bybitinverse
    ) {
        capabilities.fetch_historical_fills =
            CapabilityBinding::implemented(|venue, options| venue.fetch_historical_fills(options));
        capabilities.fetch_income_history =
            CapabilityBinding::implemented(|venue, options| venue.fetch_income_history(options));
        capabilities.fetch_income_history_range =
            CapabilityBinding::implemented(|venue, (start, end, kind)| {
                venue.fetch_income_history_range(start, end, kind)
            });
        capabilities.fetch_closed_pnl =
            CapabilityBinding::implemented(|venue, options| venue.fetch_closed_pnl(options));
    }
    if *name == ExchangeName::Hyrotrader {
        capabilities.fetch_historical_fills =
            CapabilityBinding::implemented(|venue, options| venue.fetch_historical_fills(options));
        capabilities.fetch_closed_pnl =
            CapabilityBinding::implemented(|venue, options| venue.fetch_closed_pnl(options));
    }
    if matches!(
        name,
        ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
    ) {
        capabilities.fetch_historical_orders =
            CapabilityBinding::implemented(|venue, options| venue.fetch_historical_orders(options));
        capabilities.fetch_option_chain =
            CapabilityBinding::implemented(|venue, query| venue.fetch_option_chain(query));
        capabilities.fetch_option_expiries = CapabilityBinding::implemented(|venue, underlying| {
            venue.fetch_option_expiries(underlying)
        });
        capabilities.fetch_account_greeks =
            CapabilityBinding::implemented(|venue, ()| venue.fetch_account_greeks());
    }
    capabilities
}

/// ScopeShared requires a generic client host; Bybit's bodies send through `this.xhr`.
struct NoGenericClient(Rc<dyn ExchangeRuntime>, Option<HostedSigner>);
impl JsHttpHost for NoGenericClient {
    fn hosted_signer(&self) -> Option<HostedSigner> {
        self.1.clone()
    }
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
    fn listen_public_ohlcv(
        &self,
        options: safe_cex_types::OHLCVOptions,
        callback: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        if self.base.name == ExchangeName::Hyrotrader {
            self.rt()
                .this
                .call_method("validateCandleInterval", vec![js(&options.interval)?])
                .map_err(err)?;
        }
        use safe_cex_exchange::socket_lifecycle::SocketSlot;
        let websocket = |property: &str| {
            let slot = match property {
                "publicWebsocket" => SocketSlot::Public,
                "publicInverseWebsocket" => SocketSlot::PublicSecondary,
                _ => return Err(ExchangeError::new("Unknown Bybit public socket")),
            };
            self.base
                .get_socket(slot)
                .ok_or_else(|| ExchangeError::new("Bybit public socket unavailable"))
        };
        let call = |socket: Rc<safe_cex_exchange::websocket::BaseWebSocket>,
                    _: &str,
                    options,
                    callback| {
            let primary = matches!(
                self.base.name,
                ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
            );
            let secondary = self
                .base
                .get_socket(SocketSlot::PublicSecondary)
                .is_some_and(|other| Rc::ptr_eq(&socket, &other));
            let category = if secondary {
                "inverse"
            } else if self.base.name == ExchangeName::Bybitspot {
                "spot"
            } else {
                "linear"
            };
            crate::ws_public::single_candles::listen_ohlcv(
                socket.clone(),
                self.subscription_topics(category),
                options,
                self.base.clone(),
                if primary {
                    Some(self.candle_recovery(&socket, category))
                } else {
                    None
                },
                callback,
            )
        };
        let symbol = options.symbol.clone();
        match self.base.name {
            ExchangeName::Bybitinverse => {
                crate::stream_delegation::listen_inverse_ohlcv(options, callback, websocket, call)
            }
            ExchangeName::Bybitspot => {
                crate::stream_delegation::listen_ohlcv(options, callback, websocket, call)
            }
            _ => crate::stream_delegation::listen_primary_ohlcv(
                options,
                &symbol,
                callback,
                websocket,
                || Ok(self.base.get_socket(SocketSlot::PublicSecondary)),
                call,
            ),
        }
    }
    fn listen_public_ohlcvs(
        &self,
        options: safe_cex_types::OHLCVSOptions,
        callback: Rc<dyn Fn(safe_cex_exchange::exchange::SubscribedCandle)>,
    ) -> Result<(), ExchangeError> {
        if self.base.name == ExchangeName::Hyrotrader {
            self.rt()
                .this
                .call_method("validateCandleInterval", vec![js(&options.interval)?])
                .map_err(err)?;
        }
        use safe_cex_exchange::socket_lifecycle::SocketSlot;
        let websocket = |property: &str| {
            let slot = match property {
                "publicWebsocket" => SocketSlot::Public,
                "publicInverseWebsocket" => SocketSlot::PublicSecondary,
                _ => return Err(ExchangeError::new("Unknown Bybit public socket")),
            };
            self.base
                .get_socket(slot)
                .ok_or_else(|| ExchangeError::new("Bybit public socket unavailable"))
        };
        let call = |socket: Rc<safe_cex_exchange::websocket::BaseWebSocket>,
                    _: &str,
                    options,
                    callback| {
            let secondary = self
                .base
                .get_socket(SocketSlot::PublicSecondary)
                .is_some_and(|other| Rc::ptr_eq(&socket, &other));
            let category = if secondary { "inverse" } else { "linear" };
            crate::ws_public::candles::listen_ohlcvs(
                socket,
                self.subscription_topics(category),
                options,
                callback,
            )
        };
        match self.base.name {
            ExchangeName::Bybitinverse => {
                crate::stream_delegation::listen_inverse_ohlcvs(options, callback, websocket, call)
            }
            ExchangeName::Bybitspot => Ok(()),
            _ => crate::stream_delegation::listen_primary_ohlcvs(
                Ok(options.symbols.clone()),
                callback,
                websocket,
                || Ok(self.base.get_socket(SocketSlot::PublicSecondary)),
                Rc::ptr_eq,
                |symbols| {
                    let mut group = options.clone();
                    group.symbols = symbols;
                    group
                },
                call,
            ),
        }
    }
    fn listen_public_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        use safe_cex_exchange::socket_lifecycle::SocketSlot;
        let websocket = |property: &str| {
            let slot = match property {
                "publicWebsocket" => SocketSlot::Public,
                "publicInverseWebsocket" => SocketSlot::PublicSecondary,
                _ => return Err(ExchangeError::new("Unknown Bybit public socket")),
            };
            self.base
                .get_socket(slot)
                .ok_or_else(|| ExchangeError::new("Bybit public socket unavailable"))
        };
        let call = |socket: Rc<safe_cex_exchange::websocket::BaseWebSocket>,
                    _: &str,
                    symbol: &str,
                    callback| {
            let secondary = self
                .base
                .get_socket(SocketSlot::PublicSecondary)
                .is_some_and(|other| Rc::ptr_eq(&socket, &other));
            let spot = self.base.name == ExchangeName::Bybitspot;
            let category = if secondary {
                "inverse"
            } else if spot {
                "spot"
            } else {
                "linear"
            };
            crate::ws_public::streams::listen_order_book(
                socket,
                self.subscription_topics(category),
                symbol,
                if spot { 200 } else { 1000 },
                callback,
            )
        };
        match self.base.name {
            ExchangeName::Bybitinverse => crate::stream_delegation::listen_inverse_order_book(
                &symbol, callback, websocket, call,
            ),
            ExchangeName::Bybitspot => {
                crate::stream_delegation::listen_order_book(&symbol, callback, websocket, call)
            }
            _ => crate::stream_delegation::listen_primary_order_book(
                &symbol,
                callback,
                websocket,
                || Ok(self.base.get_socket(SocketSlot::PublicSecondary)),
                call,
            ),
        }
    }
    // ts: BybitExchange.constructor (bybitdemo by name)
    pub fn create(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BybitDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if !matches!(
            name,
            ExchangeName::Bybit
                | ExchangeName::Bybitdemo
                | ExchangeName::Bybitspot
                | ExchangeName::Bybitinverse
                | ExchangeName::Hyrotrader
        ) {
            return Err(ExchangeError::new(format!(
                "BybitExchange cannot construct {}",
                serde_json::to_value(&name)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            )));
        }
        let hyro_rest = if name == ExchangeName::Hyrotrader {
            let network = if options.testnet == Some(true) {
                "testnet"
            } else {
                "livenet"
            };
            let (label, rest, _) = crate::hyro_configuration::get_configured_urls(&options);
            let origin = rest[network].as_str().unwrap_or_default();
            if origin.is_empty() {
                let hint = if crate::hyro_configuration::hyrotrader_mode(&options) == "production" {
                    " Set NEXT_PUBLIC_HYROTRADER_PROD_REST_URL, or NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL while v1 is simulator-backed."
                } else {
                    ""
                };
                return Err(ExchangeError::new(format!(
                    "HyroTrader {label} REST URL is not configured.{hint}"
                )));
            }
            Some(origin.to_owned())
        } else {
            None
        };
        let runtime = deps.runtime.clone();
        let exchange_runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
        let clock = deps.clock.clone();
        let shared = Rc::new_cyclic(|weak: &Weak<super::rt::Shared>| {
            ScopeShared::new(
                runtime.clone(),
                Rc::new(NoGenericClient(
                    exchange_runtime.clone(),
                    deps.remote_signer.clone(),
                )),
                store.clone(),
                globals::build(weak.clone(), clock.clone()),
                BybitExtra {
                    venue: RefCell::new(Weak::new()),
                    private_protocol: RefCell::new(Weak::new()),
                },
            )
        });
        let mut class_settings = settings();
        if name == ExchangeName::Hyrotrader {
            class_settings.client_order_id_rule = None;
        }
        if name == ExchangeName::Bybitspot {
            class_settings.positions_supported = Some(false);
            class_settings
                .client_order_id_rule
                .as_mut()
                .expect("Bybit rule")
                .label = "Bybit spot".into();
        }
        if name == ExchangeName::Bybitinverse {
            class_settings
                .client_order_id_rule
                .as_mut()
                .expect("Bybit rule")
                .label = "Bybit inverse".into();
        }
        let (receiver, core) = base_exchange::create(
            &shared.sockets,
            BaseExchangeParts {
                name: name.clone(),
                options: options.clone(),
                store: store.clone(),
                settings: class_settings,
                external_public_data: deps.external_public_data.clone(),
            },
            from_typed(&name).map_err(err)?,
            from_typed(&options).map_err(err)?,
            store::facade(store.clone()),
            |this| {
                // bybitdemo.exchange.ts initializes this field after super().
                if name == ExchangeName::Bybitdemo {
                    this.define("isDemo", Js::Bool(true))?;
                }
                if name == ExchangeName::Hyrotrader {
                    this.define(
                        "isDemo",
                        Js::Bool(crate::hyro_configuration::hyrotrader_mode(&options) == "demo"),
                    )?;
                }
                // bybitspot.exchange.ts initializes its own isSpot field.
                if name == ExchangeName::Bybitspot {
                    this.define("isSpot", Js::Bool(true))?;
                }
                Ok(())
            },
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
        let account_name = deps
            .remote_signer
            .as_ref()
            .and_then(|(_, name)| name.clone());
        let api_signer = deps.remote_signer.as_ref().map(|(signer, _)| {
            safe_cex_exchange::js_runtime::client::transport_signer(signer.clone())
        });
        let api_account_name = account_name.clone();
        let relay_enabled = deps.relay_enabled;
        let public_base = core.clone();
        let private_shared = shared.clone();
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
                            signer: api_signer.clone(),
                            account_name: api_account_name.clone(),
                            adapter: wire.clone(),
                            host: http_host.clone(),
                            runtime: api_runtime.clone(),
                            store: Some(api_store.clone()),
                            stats: Some(Default::default()),
                            logs: Some(Default::default()),
                            every: vec![],
                            periodic: vec![],
                            #[allow(
                                clippy::arc_with_non_send_sync,
                                reason = "browser-local runtime, as everywhere in this crate"
                            )]
                            next_id: Arc::new(move || next_id.unique_id()),
                            signing_now: Arc::new(move || {
                                signing_clock.get_current_time_ms() as u64
                            }),
                        },
                        owner,
                        is_demo: base.name == ExchangeName::Bybitdemo || base.is_demo,
                        rest_base_url: hyro_rest.clone(),
                        clock: api_clock.clone(),
                        retry_clock: retry_clock.clone(),
                        payload_signer: base.signers.payload_signer.clone(),
                    },
                    &TransportRegistry::default(),
                )?;
                #[allow(
                    clippy::arc_with_non_send_sync,
                    reason = "RestAdapter supports browser-local hosts"
                )]
                let api: Arc<dyn RestAdapter> = Arc::new(api);
                Ok(api)
            },
            {
                let runtime = runtime.clone();
                move |parent, category| {
                    Rc::new(crate::ws_public::PublicProtocol::new(
                        parent,
                        runtime.clone(),
                        match category {
                            "inverse" => "inverse",
                            "spot" => "spot",
                            _ => "linear",
                        },
                        &public_base,
                    ))
                }
            },
            move |parent| {
                let protocol = Rc::new(crate::ws_private::protocol::PrivateProtocol::new(
                    parent,
                    runtime.clone(),
                    crate::ws_private::protocol::PrivateProtocolConfig {
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
                ));
                *private_shared.extra.private_protocol.borrow_mut() = Rc::downgrade(&protocol);
                protocol
            },
        )
        .map_err(|error| ExchangeError::new(error.to_string()))?;
        if venue.base.name == ExchangeName::Hyrotrader {
            venue.position_mode.set(Some(PositionMode::OneWay));
        }
        for (slot, category) in [
            (
                safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
                if venue.base.name == ExchangeName::Bybitspot {
                    "spot"
                } else {
                    "linear"
                },
            ),
            (
                safe_cex_exchange::socket_lifecycle::SocketSlot::PublicSecondary,
                "inverse",
            ),
        ] {
            if let Some(socket) = venue.base.get_socket(slot) {
                crate::ws_public::PublicProtocol::register_handlers(&socket, category);
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
        self.js
            .get()
            .expect("BybitExchange::create assembles the receiver")
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt().this.clone();
        Box::pin(async move {
            let value = await_js(
                &self.rt().exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt().this.clone();
        Box::pin(async move {
            await_js(
                &self.rt().exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
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
        if self.base.name == ExchangeName::Hyrotrader {
            return "HyrotraderExchange";
        }
        if self.base.name == ExchangeName::Bybitspot {
            "BybitSpotExchange"
        } else if self.base.name == ExchangeName::Bybitinverse {
            "BybitInverseExchange"
        } else {
            "BybitExchange"
        }
    }
    // ts: BybitExchange.dispose
    fn dispose(&self) -> Result<(), ExchangeError> {
        if self.base.name == ExchangeName::Bybitinverse {
            return dispose_inverse_binding(self);
        }
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
    // Bybit sends userID/inviterID as numbers, which ExchangeAccount's open-String
    // fields carry as their interpolated text (MANIFEST-SPEC closed typed output boundary).
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        let this = self.rt().this.clone();
        Box::pin(async move {
            let account = await_js(
                &self.rt().exchange_runtime(),
                this.call_method("getAccount", vec![]).map_err(err)?,
            )
            .await
            .map_err(err)?;
            for key in ["userId", "affiliateId"] {
                let value = account.get(key).map_err(err)?;
                if !value.is_nullish() && !value.is_string() {
                    account
                        .set(key, Js::String(value.to_js_string().map_err(err)?))
                        .map_err(err)?;
                }
            }
            to_typed(&account).map_err(err)
        })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchPositions", a)
    }
    fn fetch_and_set_private_snapshot(&self, _tries: u32) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("fetchAndSetPrivateSnapshot", vec![])
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        // ts: BybitExchange.fetchOrders takes no argument.
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn listen_ohlcv(
        &self,
        options: safe_cex_types::OHLCVOptions,
        callback: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen_public_ohlcv(options, callback)
    }
    fn listen_ohlcvs(
        &self,
        options: safe_cex_types::OHLCVSOptions,
        callback: Rc<dyn Fn(safe_cex_exchange::exchange::SubscribedCandle)>,
    ) {
        if let Err(error) = self.listen_public_ohlcvs(options, callback) {
            self.base.runtime.report_exception(
                &error,
                &serde_json::json!({"operation":"Bybit listenOHLCVs"}).into(),
            );
        }
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen_public_order_book(symbol, callback)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.invoke("fetchOHLCV", args!(js(&options)))
    }
    fn fetch_historical_positions(
        &self,
        options: safe_cex_exchange::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        if matches!(
            self.base.name,
            ExchangeName::Bybitspot | ExchangeName::Bybitinverse
        ) {
            return Box::pin(async move {
                Err(ExchangeError::new(format!(
                    "{}: no journal records path declared. Override fetchHistoricalPositions — fold from fills via foldClosedPositionRecords, or map this venue's reported closed axis.",
                    self.class_name()
                )))
            });
        }
        let opts = Js::object(vec![
            ("account", Js::str(&options.account)),
            ("from", options.from.map_or(Js::Undefined, Js::Number)),
            ("to", options.to.map_or(Js::Undefined, Js::Number)),
        ]);
        self.invoke("fetchHistoricalPositions", vec![opts])
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        self.invoke("fetchHistoricalFills", args!(js(&options)))
    }
    fn fetch_historical_orders(
        &self,
        options: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        self.invoke("fetchHistoricalOrders", args!(opt_js(options)))
    }
    fn fetch_income_history(
        &self,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        self.invoke("fetchIncomeHistory", args!(opt_js(options)))
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        self.invoke(
            "fetchIncomeHistoryRange",
            args!(js(&start), js(&end), opt_js(kind)),
        )
    }
    fn fetch_closed_pnl(
        &self,
        options: HistoricalTradeOptions,
    ) -> ExchangeFuture<'_, Vec<ClosedPnlEntry>> {
        self.invoke("fetchClosedPnl", args!(js(&options)))
    }
    fn fetch_option_chain(
        &self,
        query: OptionChainQuery,
    ) -> ExchangeFuture<'_, Vec<OptionChainRow>> {
        if matches!(
            self.base.name,
            ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
        ) {
            self.invoke("fetchOptionChain", args!(js(&query)))
        } else {
            ExchangeCapabilities::default().fetch_option_chain(self, query)
        }
    }
    fn fetch_option_expiries(&self, underlying: String) -> ExchangeFuture<'_, Vec<f64>> {
        if matches!(
            self.base.name,
            ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
        ) {
            self.invoke("fetchOptionExpiries", args!(js(&underlying)))
        } else {
            ExchangeCapabilities::default().fetch_option_expiries(self, underlying)
        }
    }
    fn fetch_account_greeks(&self) -> ExchangeFuture<'_, Vec<AccountGreeks>> {
        if matches!(
            self.base.name,
            ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
        ) {
            self.invoke("fetchAccountGreeks", vec![])
        } else {
            ExchangeCapabilities::default().fetch_account_greeks(self)
        }
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
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelSymbolOrders", args!(js(&symbol)))
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

// ts: BybitInverseExchange.dispose — source super, dedicated inverse socket,
// private socket; no primary reconciliation-timer phase.
fn dispose_inverse_binding(venue: &BybitExchange) -> Result<(), ExchangeError> {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    crate::stream_delegation::dispose_inverse_exchange(
        || venue.base.dispose(),
        |name| {
            Ok(venue.base.get_socket(if name == "privateWebsocket" {
                SocketSlot::Private
            } else {
                SocketSlot::PublicSecondary
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
