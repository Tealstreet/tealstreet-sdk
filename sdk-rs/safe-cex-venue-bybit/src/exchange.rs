//! Linear exchange composition. Concrete API/protocol factories remain explicit
//! while their lifecycle implementations are completed; this is not yet exported.
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    client_order_id::{ClientOrderIdRuleOverrides, pattern_client_order_id_rule},
    exchange::{BaseExchange, Exchange},
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestRequest, RestResponse, TransportError,
    rate_limit::{RateLimitClock, RateLimitedAdapter},
    secondary::SecondaryKeyClients,
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

pub(crate) struct BybitExchange {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    pub(crate) xhr: Rc<SecondaryKeyClients>,
    pub(crate) unlimited_xhr: Arc<dyn RestAdapter>,
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

impl BybitExchange {
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

    pub(crate) fn get_ticker_symbols_for_category(&self, category: PublicCategory) -> Vec<String> {
        let symbols = self
            .base
            .store
            .markets()
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
        self.base.get_ticker_symbols_for_subscription(&symbols)
    }

    pub(crate) fn has_full_sized_position_opposite_side(
        &self,
        symbol: &str,
        side: safe_cex_types::OrderSide,
        amount: f64,
    ) -> bool {
        let opposite = if side == safe_cex_types::OrderSide::Buy {
            safe_cex_types::PositionSide::Short
        } else {
            safe_cex_types::PositionSide::Long
        };
        self.base.store.positions().iter().any(|position| {
            position.symbol == symbol
                && position.side == opposite
                && position.contracts != 0.0
                && position.contracts >= amount
        })
    }

    pub(crate) fn infer_protective_order_side(
        &self,
        order: &serde_json::Value,
    ) -> Result<Option<safe_cex_types::OrderSide>, String> {
        use safe_cex_types::{OrderSide, PositionSide};
        if !order.is_null() && !order.is_object() && !order.is_array() {
            return Err("TypeError: Bybit field lookup requires an object".into());
        }
        let index = safe_cex_num::js_number(crate::json_field::field(order, "positionIdx")?)
            .map_err(|error| error.message)?;
        if index == 1. {
            return Ok(Some(OrderSide::Sell));
        }
        if index == 2. {
            return Ok(Some(OrderSide::Buy));
        }
        let Some(symbol) = crate::json_field::field(order, "symbol")?
            .and_then(serde_json::Value::as_str)
            .filter(|symbol| !symbol.is_empty())
        else {
            return Ok(None);
        };
        let positions = self.base.store.positions();
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
        false
    }

    pub(crate) fn generated_order_link_id(&self) -> Option<String> {
        // The concrete linear hook never requires an ID; derived hooks have
        // their own dispatch and generation contract.
        None
    }

    pub(crate) fn validate_order_capabilities(
        &self,
        _options: &crate::order_params::BybitPlaceOrderOpts,
    ) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
        // The linear source hook is intentionally empty; basic validation and
        // derived venue restrictions belong to their own preflight operations.
        Ok(())
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

    pub(crate) fn validate_candle_interval(
        &self,
        _interval: &serde_json::Value,
    ) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
        // Concrete linear Bybit always supports trade-derived candles. Its
        // source short-circuit does not inspect interval; subclass hooks differ.
        Ok(())
    }

    pub(crate) fn supports_trade_derived_candles(&self) -> bool {
        true
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
    ) -> &'static str {
        crate::types::BASE_WS_URL["public"][category.key()][network.key()]
            .as_str()
            .expect("static Bybit public WS URL")
    }
    pub(crate) fn get_bybit_private_ws_url(&self, network: Network) -> &'static str {
        let table = if self.uses_bybit_demo_environment() {
            &crate::types::DEMO_WS_URL
        } else {
            &crate::types::BASE_WS_URL
        };
        table["private"][network.key()]
            .as_str()
            .expect("static Bybit private WS URL")
    }

    /// Mirrors the constructor's client/socket composition. Factories are
    /// explicit module dependencies, not substitutes for their parity claims.
    #[allow(
        clippy::arc_with_non_send_sync,
        reason = "RestAdapter supports browser-local hosts"
    )]
    pub(crate) fn new(
        mut base: BaseExchange,
        runtime: Rc<dyn WsRuntime>,
        clock: Arc<dyn RateLimitClock>,
        create_api: impl Fn(
            &ExchangeOptions,
            &BaseExchange,
        ) -> Result<Arc<dyn RestAdapter>, TransportError>,
        public_protocol: impl FnOnce(Weak<Self>, &str) -> Rc<dyn WsProtocol>,
        private_protocol: impl FnOnce(Weak<Self>) -> Rc<dyn WsProtocol>,
    ) -> Result<Rc<Self>, TransportError> {
        base.settings.client_order_id_rule = Some(pattern_client_order_id_rule(
            "Bybit".into(),
            "/^[A-Za-z0-9_-]{1,36}$/".into(),
            Rc::new(|id| {
                (1..=36).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            }),
            ClientOrderIdRuleOverrides::default(),
        ));
        let limit = |adapter| -> Arc<dyn RestAdapter> {
            Arc::new(RateLimitedAdapter::new(adapter, clock.clone(), 5, 1000))
        };
        let xhr = base.create_xhr_with_secondary_key(
            &base.options(),
            runtime.transport().as_ref(),
            None,
            |options| create_api(options, &base),
            Some(&limit),
            None,
        )?;
        let unlimited_xhr = create_api(&base.options(), &base)?;
        let venue = Rc::new(Self {
            base,
            // Placement registration follows the implementation of the handlers;
            // an absent implementation must retain the core's fallback brand.
            capabilities: ExchangeCapabilities::default(),
            xhr,
            unlimited_xhr,
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
            default_executions_lookback_ms: 7. * 24. * 60. * 60. * 1000.,
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        let parent = WsExchangeParent::new(&erased);
        let public = BaseWebSocket::new(
            1,
            parent.clone(),
            runtime.clone(),
            public_protocol(Rc::downgrade(&venue), "linear"),
        );
        venue.base.set_socket(SocketSlot::Public, Some(public));
        let private =
            BaseWebSocket::new(0, parent, runtime, private_protocol(Rc::downgrade(&venue)));
        venue.base.set_socket(SocketSlot::Private, Some(private));
        Ok(venue)
    }

    /// Production private protocol binding; the public protocol and HTTP factory
    /// remain explicit while their remaining implementation is completed.
    pub(crate) fn with_private_protocol(
        base: BaseExchange,
        runtime: Rc<dyn WsRuntime>,
        clock: Arc<dyn RateLimitClock>,
        create_api: impl Fn(
            &ExchangeOptions,
            &BaseExchange,
        ) -> Result<Arc<dyn RestAdapter>, TransportError>,
        public_protocol: impl FnOnce(Weak<Self>, &str) -> Rc<dyn WsProtocol>,
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
                Rc::new(crate::ws_private::protocol::PrivateProtocol {
                    parent,
                    runtime: private_runtime,
                    config: private,
                })
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
            |payload| {
                self.xhr.request(RestRequest {
                    method: "post".into(),
                    url: crate::types::ENDPOINTS["CREATE_ORDER"]
                        .as_str()
                        .expect("static endpoint")
                        .into(),
                    body: Some(RestBody::Untransformed(payload)),
                    ..Default::default()
                })
            },
            report_failure,
        )
        .await
    }
}

impl Exchange for BybitExchange {
    type PlaceOpts = crate::order_params::BybitPlaceOrderOpts;
    fn validate_order_capabilities(
        &self,
        options: &Self::PlaceOpts,
    ) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
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
}
