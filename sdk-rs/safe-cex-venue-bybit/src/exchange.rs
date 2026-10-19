//! Linear exchange composition: the typed core, its signed clients and sockets, plus the
//! js_runtime receiver the ported bodies run on (`crate::binding`).
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestRequest, RestResponse, TransportError,
    rate_limit::RateLimitedAdapter, secondary::SecondaryKeyClients,
};
use safe_cex_types::ExchangeOptions;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub(crate) enum Network {
    Livenet,
    Testnet,
}
impl Network {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Livenet => "livenet",
            Self::Testnet => "testnet",
        }
    }
}
#[derive(Clone, Copy)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "wired as the remaining receiver members land")
)]
pub(crate) enum PublicCategory {
    Linear,
    Inverse,
}
impl PublicCategory {
    fn key(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Inverse => "inverse",
        }
    }
}

pub struct BybitExchange {
    pub(crate) base: Rc<BaseExchange>,
    pub(crate) capabilities: ExchangeCapabilities,
    xhr: std::cell::OnceCell<Rc<SecondaryKeyClients>>,
    unlimited_xhr: std::cell::OnceCell<Arc<dyn RestAdapter>>,
    pub(crate) js: std::cell::OnceCell<crate::binding::rt::Rt>,
    pub(crate) order_state:
        crate::primary_order_lifecycle::PrimaryOrderState<safe_cex_exchange::js_runtime::value::Js>,
    /// ts: BybitExchange#debouncedPrivateSnapshotRefresh, over the receiver.
    pub(crate) snapshot_refresh: std::cell::OnceCell<
        Rc<crate::private_snapshot_refresh::PrivateSnapshotRefresh<crate::binding::rt::Rt>>,
    >,
    stream_topics: RefCell<std::collections::HashMap<String, crate::ws_public::streams::Topics>>,
    candle_recoveries:
        RefCell<std::collections::HashMap<String, Rc<crate::ws_public::candle_recovery::Recovery>>>,
    // fetchMarginAccountInfos assigns this wire field without coercion. Keep
    // absence and nonnumeric values: only the numeric value 1 means CONTRACT.
    pub(crate) unified_margin_status: RefCell<Option<serde_json::Value>>,
    pub(crate) margin_type: Cell<safe_cex_types::MarginType>,
    pub(crate) needs_reinit: Cell<bool>,
    pub(crate) last_reinit_at: Cell<f64>,
    pub(crate) reinit_retry_ms: Cell<f64>,
    pub(crate) position_mode: Cell<Option<safe_cex_types::PositionMode>>,
    pub(crate) last_websocket_positions_update: Cell<Option<f64>>,
    pub(crate) api_key_info: RefCell<Option<crate::types::BybitApiKeyInfo>>,
    pub(crate) batch_limit: Cell<f64>,
    pub(crate) force_unified_balance: Cell<bool>,
    pub(crate) execution_query_max_range_ms: f64,
    pub(crate) default_executions_lookback_ms: f64,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "wired as the remaining receiver members land")
)]
impl BybitExchange {
    pub(crate) fn candle_recovery(
        &self,
        socket: &Rc<safe_cex_exchange::websocket::BaseWebSocket>,
        category: &str,
    ) -> Rc<crate::ws_public::candle_recovery::Recovery> {
        self.candle_recoveries
            .borrow_mut()
            .entry(category.into())
            .or_insert_with(|| {
                crate::ws_public::candle_recovery::Recovery::new(
                    socket,
                    &self.base,
                    self.subscription_topics(category),
                )
            })
            .clone()
    }
    pub(crate) fn subscription_topics(&self, category: &str) -> crate::ws_public::streams::Topics {
        self.stream_topics
            .borrow_mut()
            .entry(category.into())
            .or_default()
            .clone()
    }
    pub(crate) fn xhr(&self) -> &Rc<SecondaryKeyClients> {
        self.xhr
            .get()
            .expect("Bybit clients are created during construction")
    }
    pub(crate) fn unlimited_xhr(&self) -> &Arc<dyn RestAdapter> {
        self.unlimited_xhr
            .get()
            .expect("Bybit clients are created during construction")
    }
    pub(crate) async fn reinit_on_throw<T, E>(
        &self,
        request: impl std::future::Future<Output = Result<T, E>>,
    ) -> Result<T, E> {
        match request.await {
            Ok(value) => Ok(value),
            Err(error) => {
                self.needs_reinit.set(true);
                Err(error)
            }
        }
    }

    pub(crate) fn get_settle_coins_for_category(
        &self,
        category: &str,
    ) -> Result<Vec<&'static str>, safe_cex_exchange::runtime::ExchangeError> {
        if category == "linear" {
            return Ok(if self.account_type() == "UNIFIED" {
                vec!["USDT", "USDC"]
            } else {
                vec!["USDT"]
            });
        }
        let message = format!("Unknown category {category}");
        self.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message.clone()),
            None,
            None,
        )?;
        Err(safe_cex_exchange::runtime::ExchangeError::new(message))
    }

    /// Options join this connection only when the account opted in.
    pub(crate) fn options_enabled(&self) -> bool {
        self.base.name != safe_cex_types::ExchangeName::Hyrotrader
            && self.base.options().enable_option_markets == Some(true)
    }
    /// A shared public data source has no inverse markets, so it cannot size inverse positions.
    pub(crate) fn inverse_enabled(&self) -> bool {
        let options = self.base.options();
        !matches!(
            self.base.name,
            safe_cex_types::ExchangeName::Bybitspot | safe_cex_types::ExchangeName::Hyrotrader
        ) && options.enable_inverse_markets == Some(true)
            && options.external_public_data.is_none()
    }

    // ts: BybitExchange.getCategoryForSymbol — routed by symbol so a row routes before its market loads.
    pub(crate) fn get_category_for_symbol(&self, symbol: &str) -> &'static str {
        if crate::options::is_bybit_option_symbol(symbol) {
            "option"
        } else if crate::inverse_symbol::is_bybit_inverse_symbol(symbol) {
            "inverse"
        } else {
            "linear"
        }
    }

    pub(crate) fn get_ticker_symbols_for_category(
        &self,
        category: PublicCategory,
    ) -> Result<Vec<String>, safe_cex_exchange::runtime::ExchangeError> {
        let symbols = self
            .base
            .store
            .markets()?
            .iter()
            .filter(|market| {
                if market.option.is_some() {
                    return false;
                }
                let linear = market.quote == "USDT" || market.quote == "USDC";
                match category {
                    PublicCategory::Linear => linear,
                    PublicCategory::Inverse => !linear,
                }
            })
            .map(|market| market.symbol.clone())
            .collect::<Vec<_>>();
        Ok(self.base.get_ticker_symbols_for_subscription(&symbols))
    }

    pub(crate) fn has_full_sized_position_opposite_side(
        &self,
        symbol: &str,
        side: safe_cex_types::OrderSide,
        amount: f64,
    ) -> Result<bool, safe_cex_exchange::runtime::ExchangeError> {
        let opposite = if side == safe_cex_types::OrderSide::Buy {
            safe_cex_types::PositionSide::Short
        } else {
            safe_cex_types::PositionSide::Long
        };
        // An inverse amount is USD contracts, so compare it with the position's native size.
        let inverse = crate::inverse_symbol::is_bybit_inverse_symbol(symbol);
        Ok(self.base.store.positions()?.iter().any(|position| {
            let size = if inverse {
                position
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.contracts_native)
                    .unwrap_or(0.0)
            } else {
                position.contracts
            };
            position.symbol == symbol
                && position.side == opposite
                && position.contracts != 0.0
                && size >= amount
        }))
    }

    pub(crate) fn infer_protective_order_side(
        &self,
        order: &serde_json::Value,
    ) -> Result<Option<safe_cex_types::OrderSide>, safe_cex_exchange::runtime::ExchangeError> {
        use safe_cex_types::{OrderSide, PositionSide};
        if !order.is_null() && !order.is_object() && !order.is_array() {
            safe_cex_num::get_key::v_unknown(Some(order), "positionIdx")
                .map_err(|error| type_error(error.message))?;
        }
        let index = safe_cex_num::js_number(
            crate::json_field::field(order, "positionIdx").map_err(type_error)?,
        )
        .map_err(|error| type_error(error.message))?;
        if index == 1. {
            return Ok(Some(OrderSide::Sell));
        }
        if index == 2. {
            return Ok(Some(OrderSide::Buy));
        }
        let Some(symbol) = crate::json_field::field(order, "symbol")
            .map_err(type_error)?
            .and_then(serde_json::Value::as_str)
            .filter(|symbol| !symbol.is_empty())
        else {
            return Ok(None);
        };
        let positions = self.base.store.positions()?;
        let open = positions
            .iter()
            .filter(|position| position.symbol == symbol && position.contracts.abs() > 0.)
            .collect::<Vec<_>>();
        if open.len() != 1 {
            return Ok(None);
        }
        Ok(Some(if open[0].side == PositionSide::Long {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        }))
    }

    pub(crate) fn requires_order_link_id(&self) -> bool {
        self.base.name == safe_cex_types::ExchangeName::Hyrotrader
    }

    pub(crate) fn generated_order_link_id(
        &self,
    ) -> Result<Option<String>, safe_cex_exchange::runtime::ExchangeError> {
        if !self.requires_order_link_id() {
            return Ok(None);
        }
        let rt = self.js.get().ok_or_else(|| {
            safe_cex_exchange::runtime::ExchangeError::new("Bybit receiver is unavailable")
        })?;
        let value = rt
            .shared
            .globals
            .get("nanoid")
            .and_then(|factory| factory.call(crate::binding::value::Js::Undefined, vec![]))
            .map_err(|error| error.to_exchange_error())?;
        crate::binding::value::to_typed(&value)
            .map(Some)
            .map_err(|error| error.to_exchange_error())
    }

    pub(crate) fn validate_order_capabilities(
        &self,
        options: &crate::order_params::BybitPlaceOrderOpts,
    ) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
        if self.base.name != safe_cex_types::ExchangeName::Hyrotrader {
            return Ok(());
        }
        let rt = self.js.get().ok_or_else(|| {
            safe_cex_exchange::runtime::ExchangeError::new("Bybit receiver is unavailable")
        })?;
        let options = crate::binding::value::from_typed(options)
            .map_err(|error| error.to_exchange_error())?;
        rt.this
            .call_method("validateOrderCapabilities", vec![options])
            .map(|_| ())
            .map_err(|error| error.to_exchange_error())
    }

    pub(crate) fn get_execution_query_windows(
        &self,
        from: Option<f64>,
        to: Option<f64>,
    ) -> Vec<(Option<f64>, Option<f64>)> {
        let (Some(start), Some(end)) = (from, to) else {
            return vec![(from, to)];
        };
        if end - start <= self.execution_query_max_range_ms {
            return vec![(from, to)];
        }
        let mut windows = Vec::new();
        let mut window_end = end;
        // Preserve the source strict guard, including its terminal-singleton
        // omission after stepping back to start exactly (manifest ts-bug).
        while window_end > start {
            let window_start =
                safe_cex_num::math_max(start, window_end - self.execution_query_max_range_ms);
            windows.push((Some(window_start), Some(window_end)));
            if window_start <= start {
                break;
            }
            window_end = window_start - 1.;
        }
        windows
    }

    pub(crate) fn cached_api_key_info(&self) -> Option<crate::types::BybitApiKeyInfo> {
        self.api_key_info.borrow().clone()
    }

    pub(crate) fn account_type(&self) -> &'static str {
        if self
            .unified_margin_status
            .borrow()
            .as_ref()
            .and_then(safe_cex_num::numeric_ingress::js_value_as_f64)
            == Some(1.)
        {
            "CONTRACT"
        } else {
            "UNIFIED"
        }
    }

    pub(crate) fn validate_candle_interval<T: ?Sized>(
        &self,
        _interval: &T,
    ) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
        // Concrete linear Bybit always supports trade-derived candles. Its
        // source short-circuit does not inspect interval; subclass hooks differ.
        Ok(())
    }

    pub(crate) fn supports_trade_derived_candles(&self) -> bool {
        self.base.name != safe_cex_types::ExchangeName::Hyrotrader
    }

    pub(crate) fn uses_bybit_demo_environment(&self) -> bool {
        self.base.name == safe_cex_types::ExchangeName::Bybitdemo || self.base.is_demo
    }
    pub(crate) fn get_bybit_rest_base_url(&self, network: Network) -> &'static str {
        let table = if self.uses_bybit_demo_environment() {
            &crate::types::DEMO_URL
        } else {
            &crate::types::BASE_URL
        };
        table[network.key()]
            .as_str()
            .expect("static Bybit REST URL")
    }
    pub(crate) fn get_bybit_public_ws_url(
        &self,
        category: PublicCategory,
        network: Network,
    ) -> Result<&'static str, safe_cex_exchange::runtime::ExchangeError> {
        if self.base.name == safe_cex_types::ExchangeName::Hyrotrader {
            if !matches!(category, PublicCategory::Linear) {
                return Err(safe_cex_exchange::runtime::ExchangeError::new(
                    "HyroTrader only supports linear public WebSocket data",
                ));
            }
            let (label, _, ws) =
                crate::hyro_configuration::get_configured_urls(&self.base.options());
            return ws["public"]["linear"][network.key()]
                .as_str()
                .filter(|url| !url.is_empty())
                .ok_or_else(|| {
                    safe_cex_exchange::runtime::ExchangeError::new(format!(
                        "HyroTrader {label} public WebSocket URL is not configured"
                    ))
                });
        }
        Ok(
            crate::types::BASE_WS_URL["public"][category.key()][network.key()]
                .as_str()
                .expect("static Bybit public WS URL"),
        )
    }
    pub(crate) fn get_bybit_private_ws_url(
        &self,
        network: Network,
    ) -> Result<&'static str, safe_cex_exchange::runtime::ExchangeError> {
        if self.base.name == safe_cex_types::ExchangeName::Hyrotrader {
            let (label, _, ws) =
                crate::hyro_configuration::get_configured_urls(&self.base.options());
            return ws["private"][network.key()]
                .as_str()
                .filter(|url| !url.is_empty())
                .ok_or_else(|| {
                    safe_cex_exchange::runtime::ExchangeError::new(format!(
                        "HyroTrader {label} private WebSocket URL is not configured"
                    ))
                });
        }
        let table = if self.uses_bybit_demo_environment() {
            &crate::types::DEMO_WS_URL
        } else {
            &crate::types::BASE_WS_URL
        };
        Ok(table["private"][network.key()]
            .as_str()
            .expect("static Bybit private WS URL"))
    }

    /// Mirrors the constructor's client/socket composition. Factories are
    /// explicit module dependencies, not substitutes for their parity claims.
    #[allow(
        clippy::arc_with_non_send_sync,
        reason = "RestAdapter supports browser-local hosts"
    )]
    pub(crate) fn new(
        base: Rc<BaseExchange>,
        runtime: Rc<dyn WsRuntime>,
        clock: Arc<dyn safe_cex_transport::rate_limit::RateLimitScheduler>,
        create_api: impl Fn(
            &ExchangeOptions,
            &BaseExchange,
            Weak<dyn Exchange>,
        ) -> Result<Arc<dyn RestAdapter>, TransportError>,
        public_protocol: impl Fn(Weak<Self>, &str) -> Rc<dyn WsProtocol>,
        private_protocol: impl FnOnce(Weak<Self>) -> Rc<dyn WsProtocol>,
    ) -> Result<Rc<Self>, TransportError> {
        let limit = |adapter| -> Arc<dyn RestAdapter> {
            Arc::new(RateLimitedAdapter::new(adapter, clock.clone(), 5, 1000))
        };
        let default_executions_lookback_ms =
            if base.name == safe_cex_types::ExchangeName::Bybitinverse {
                30.
            } else {
                7.
            } * 24.
                * 60.
                * 60.
                * 1000.;
        let capabilities = crate::binding::exchange::capabilities(&base.name);
        let venue = Rc::new(Self {
            base,
            capabilities,
            xhr: std::cell::OnceCell::new(),
            unlimited_xhr: std::cell::OnceCell::new(),
            js: std::cell::OnceCell::new(),
            order_state: Default::default(),
            snapshot_refresh: std::cell::OnceCell::new(),
            stream_topics: RefCell::new(Default::default()),
            candle_recoveries: RefCell::new(Default::default()),
            unified_margin_status: RefCell::new(Some(serde_json::json!(6))),
            margin_type: Cell::new(safe_cex_types::MarginType::Cross),
            needs_reinit: Cell::new(false),
            last_reinit_at: Cell::new(0.),
            reinit_retry_ms: Cell::new(15_000.),
            position_mode: Cell::new(None),
            last_websocket_positions_update: Cell::new(None),
            api_key_info: RefCell::new(None),
            batch_limit: Cell::new(10.),
            force_unified_balance: Cell::new(false),
            execution_query_max_range_ms: 7. * 24. * 60. * 60. * 1000. - 1.,
            default_executions_lookback_ms,
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        let owner = Rc::downgrade(&erased);
        let base = &venue.base;
        let xhr = base.create_xhr_with_secondary_key(
            &base.options(),
            runtime.transport().as_ref(),
            None,
            |options| create_api(options, base, owner.clone()),
            Some(&limit),
            None,
        )?;
        let unlimited_xhr = create_api(&base.options(), base, owner)?;
        let _ = venue.xhr.set(xhr);
        let _ = venue.unlimited_xhr.set(unlimited_xhr);
        let parent = WsExchangeParent::new(&erased);
        let dedicated_inverse = venue.base.name == safe_cex_types::ExchangeName::Bybitinverse;
        if !dedicated_inverse {
            let public = BaseWebSocket::new(
                1,
                parent.clone(),
                runtime.clone(),
                public_protocol(
                    Rc::downgrade(&venue),
                    if venue.base.name == safe_cex_types::ExchangeName::Bybitspot {
                        "spot"
                    } else {
                        "linear"
                    },
                ),
            );
            venue.base.set_socket(SocketSlot::Public, Some(public));
        }
        if dedicated_inverse || venue.inverse_enabled() {
            let inverse = BaseWebSocket::new(
                1,
                parent.clone(),
                runtime.clone(),
                public_protocol(Rc::downgrade(&venue), "inverse"),
            );
            venue
                .base
                .set_socket(SocketSlot::PublicSecondary, Some(inverse));
        }
        let private =
            BaseWebSocket::new(0, parent, runtime, private_protocol(Rc::downgrade(&venue)));
        venue.base.set_socket(SocketSlot::Private, Some(private));
        Ok(venue)
    }

    /// Production private protocol binding; the public protocol and HTTP factory
    /// remain explicit while their remaining implementation is completed.
    pub(crate) fn with_private_protocol(
        base: Rc<BaseExchange>,
        runtime: Rc<dyn WsRuntime>,
        clock: Arc<dyn safe_cex_transport::rate_limit::RateLimitScheduler>,
        create_api: impl Fn(
            &ExchangeOptions,
            &BaseExchange,
            Weak<dyn Exchange>,
        ) -> Result<Arc<dyn RestAdapter>, TransportError>,
        public_protocol: impl Fn(Weak<Self>, &str) -> Rc<dyn WsProtocol>,
        private: crate::ws_private::protocol::PrivateProtocolConfig,
    ) -> Result<Rc<Self>, TransportError> {
        let private_runtime = runtime.clone();
        Self::new(
            base,
            runtime,
            clock,
            create_api,
            public_protocol,
            move |parent| {
                Rc::new(crate::ws_private::protocol::PrivateProtocol::new(
                    parent,
                    private_runtime,
                    private,
                ))
            },
        )
    }

    /// Keep responses and failures structured until the API response interceptor
    /// decodes them. A POST never goes through the secondary client's GET proxy.
    pub(crate) async fn submit_create_order_requests<E>(
        &self,
        payloads: Vec<serde_json::Value>,
        report_failure: impl FnMut(RestFailure) -> Result<(), E>,
    ) -> Result<Vec<(RestResponse, serde_json::Value)>, E> {
        crate::submission::submit_create_orders(
            payloads,
            |payload| async move {
                let mut request = RestRequest {
                    method: "post".into(),
                    url: crate::types::ENDPOINTS["CREATE_ORDER"]
                        .as_str()
                        .expect("static endpoint")
                        .into(),
                    ..Default::default()
                };
                let data = safe_cex_types::retained_json::RetainedJsonInput::from_value(payload)
                    .map_err(|error| {
                        RestFailure::pre_transmit(error.to_string(), request.clone())
                    })?;
                request.body = Some(RestBody::Untransformed(data));
                self.xhr().request(request).await
            },
            report_failure,
        )
        .await
    }
}

fn type_error(message: impl Into<String>) -> safe_cex_exchange::runtime::ExchangeError {
    let mut error = safe_cex_exchange::runtime::ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
