//! Lifecycle bodies use shared Store state and host-owned imported capabilities.
use crate::{
    data,
    exchange::{ApexOmniExchange, ApexRequest, failure},
    zklink_host::{SdkInitialization, ZkLinkHost},
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    js_runtime::{promise::Promise, value::Js},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::JsJsonValue as J;
use safe_cex_store::StoreUpdate;
use safe_cex_types::{AccountValidationError, Balance, Candle, OHLCVOptions, OrderBook, Ticker};
use std::rc::Rc;
pub type Cleanup = Rc<dyn Fn() -> Result<(), ExchangeError>>;
pub trait ApexServices {
    fn init_external_public_data(&self) -> ExchangeFuture<'_, bool>;
    fn public_data_only(&self) -> bool;
    fn connect_websocket(&self, private: bool) -> Result<(), ExchangeError>;
    fn sdk_initialization(&self) -> &SdkInitialization;
    fn sdk(&self) -> &dyn ZkLinkHost;
    fn new_signer(&self, seed: Option<String>) -> ExchangeFuture<'_, Rc<dyn ZkLinkHost>>;
    fn schedule_tick(
        &self,
        future: ExchangeFuture<'static, ()>,
        interval: Option<f64>,
    ) -> Result<(), ExchangeError>;
    fn date_now_ms(&self) -> f64;
    fn throttle_fetch_orders(&self) -> Result<(), ExchangeError>;
    fn place_orders(
        &self,
        orders: Vec<safe_cex_types::PreparedOrderOpts>,
    ) -> ExchangeFuture<'_, Vec<String>>;
    fn dayjs_value_ms(&self, input: Option<f64>) -> Result<f64, ExchangeError>;
    fn dayjs_subtract(&self, end_ms: f64, amount: f64, unit: &str) -> Result<f64, ExchangeError>;
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError>;
    fn listen_order_book(
        &self,
        symbol: &str,
        callback: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError>;
}
impl ApexOmniExchange {
    pub(crate) fn report(&self, error: ExchangeError) -> Result<(), ExchangeError> {
        self.base
            .emit_and_log_error(ExchangeErrorInput::Error(Box::new(error)), None, None)
    }
    // ts: ApexOmniExchange.validateAccount
    pub async fn validate_account(&self) -> Result<AccountValidationError, ExchangeError> {
        if let Err(error) = self
            .request("get", "/v3/user", J::Undefined, J::Undefined, false)
            .await
        {
            let response_view = error.response_json_view();
            let response_message = response_view
                .as_ref()
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("msg"));
            let message = response_message
                .filter(|value| safe_cex_num::js_truthy(Some(value)))
                .cloned()
                .unwrap_or_else(|| serde_json::Value::String(error.message.clone()));
            self.base.emit_and_log_error(
                ExchangeErrorInput::Raw(safe_cex_exchange::js_runtime::value::from_json(&message)),
                None,
                None,
            )?;
            return Ok(AccountValidationError::from_value(
                if safe_cex_num::js_truthy(Some(&message)) {
                    message
                } else {
                    serde_json::Value::String("Invalid API key or secret".into())
                },
            ));
        }
        let services = self.host.services()?;
        let result = async {
            services
                .sdk_initialization()
                .init_sdk(services.sdk())
                .await?;
            services
                .new_signer(self.base.options().omni_key_seed.clone())
                .await
                .map(|_| ())
        }
        .await;
        Ok(AccountValidationError::from(if result.is_err() {
            "Invalid OmniKey seed"
        } else {
            ""
        }))
    }
    fn mark_loaded(
        &self,
        balance: bool,
        positions: bool,
        orders: bool,
    ) -> Result<(), ExchangeError> {
        let mut loaded = self.base.store.loaded().clone();
        if balance {
            loaded.balance = true;
        }
        if positions {
            loaded.positions = true;
        }
        if orders {
            loaded.orders = true;
        }
        self.base.store.update(
            StoreUpdate {
                loaded: Some(loaded),
                ..Default::default()
            },
            false,
        )?;
        Ok(())
    }
    fn set_balance_loaded(&self, balance: Balance) -> Result<(), ExchangeError> {
        let mut loaded = self.base.store.loaded().clone();
        loaded.balance = true;
        self.base.store.update(
            StoreUpdate {
                balance: Some(balance),
                loaded: Some(loaded),
                ..Default::default()
            },
            false,
        )?;
        Ok(())
    }
    // ts: ApexOmniExchange.start
    pub async fn start(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let services = self.host.services()?;
        let external = services.init_external_public_data().await?;
        let public_only = services.public_data_only();
        if !external {
            let markets = self.fetch_markets().await?;
            if !self.base.is_disposed() {
                self.base.log(
                    &format!("Loaded {} ApeX Pro markets", markets.len()),
                    "info",
                )?;
                let tickers = markets
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        bid: 0.,
                        ask: 0.,
                        last: 0.,
                        mark: 0.,
                        index: 0.,
                        mid_price: None,
                        timestamp: None,
                        percentage: 0.,
                        open_interest: 0.,
                        funding_rate: 0.,
                        funding_time: 0.,
                        volume: 0.,
                        quote_volume: 0.,
                        info: None,
                    })
                    .collect();
                let mut loaded = self.base.store.loaded().clone();
                loaded.markets = true;
                loaded.tickers = true;
                self.base.store.update(
                    StoreUpdate {
                        markets: Some(markets),
                        tickers: Some(tickers),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )?;
            }
            if self.base.is_disposed() {
                return Ok(());
            }
            services.connect_websocket(false)?;
        }
        self.base.log("Ready to trade on ApeX Pro", "info")?;
        if public_only {
            return self.mark_loaded(true, true, true);
        }
        services.connect_websocket(true)?;
        // The source's fetchPositions().then(...) is a started promise. Its
        // Store continuation survives Promise.all rejecting on the balance leg.
        let venue = self.clone();
        let positions = Promise::spawn(
            &self.base.runtime,
            Box::pin(async move {
                let p = venue.fetch_positions().await?;
                if !venue.base.is_disposed() {
                    venue
                        .base
                        .store
                        .add_or_update_positions_full(p)
                        .map_err(ExchangeError::from)?;
                    venue.mark_loaded(false, true, true)?;
                }
                Ok(Js::Undefined)
            }),
        );
        let positions = async {
            positions
                .wait()
                .await
                .map(|_| ())
                .map_err(|error| error.to_exchange_error())
        };
        let result = futures::future::try_join(positions, self.fetch_raw_balance()).await;
        match result.map(|(_, balance)| balance) {
            Ok(balance) => {
                if self.base.is_disposed() {
                    return Ok(());
                }
                self.set_balance_loaded(balance)?;
            }
            Err(error) => self.report(error)?,
        }
        let venue = self.clone();
        self.base.start_owned_operation(async move {
            if let Err(error) = venue.tick().await {
                if let Err(report_error) = venue.report(error) {
                    venue.base.runtime.console(
                        "error",
                        &format!("Apex tick error reporting failed: {report_error}"),
                    );
                }
            }
        });
        Ok(())
    }
    // ts: ApexOmniExchange.fetchAndSetPrivateSnapshot
    pub async fn fetch_and_set_private_snapshot(&self) -> Result<(), ExchangeError> {
        self.fetch_and_set_private_snapshot_helper().await
    }
    // ts: ApexOmniExchange.fetchAndSetPrivateSnapshotHelper
    pub async fn fetch_and_set_private_snapshot_helper(&self) -> Result<(), ExchangeError> {
        let result = async {
            let (mut positions, orders) =
                futures::future::try_join(self.fetch_positions(), self.fetch_orders()).await?;
            if self.base.is_disposed() {
                return Ok(());
            }
            let now = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
            if self
                .last_websocket_positions_update
                .get()
                .is_some_and(|t| now - t < 1000.)
            {
                let existing = self.base.store.positions();
                for p in &mut positions {
                    if let Some(old) = existing.iter().find(|old| {
                        old.id == p.id
                            && old.last_update.is_some_and(|t| t != 0. && now - t <= 1000.)
                    }) {
                        *p = old.clone();
                    }
                }
                for p in existing.iter() {
                    if !positions.iter().any(|v| v.id == p.id) {
                        positions.push(p.clone());
                    }
                }
                let mut seen = std::collections::HashSet::new();
                positions.retain(|p| seen.insert(p.id.clone()));
            }
            self.base.store.add_or_update_positions_full(positions)?;
            self.base.store.add_or_update_orders_full(orders)?;
            self.mark_loaded(false, true, true)?;
            let balance = self.fetch_balance().await?;
            if self.base.is_disposed() {
                return Ok(());
            }
            self.set_balance_loaded(balance)
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(error) => self.report(error),
        }
    }
    // ts: ApexOmniExchange.tick
    pub async fn tick(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if !self.base.is_disposed() {
            if let Err(error) = self.fetch_and_set_private_snapshot_helper().await {
                self.base.emit_and_log_error(
                    ExchangeErrorInput::Message(error.message),
                    None,
                    None,
                )?;
            }
            let next = self.clone();
            self.host.services()?.schedule_tick(
                Box::pin(async move { Box::pin(next.tick()).await }),
                self.base.get_tick_interval(),
            )?;
        }
        Ok(())
    }
    // ts: ApexOmniExchange.fetchOHLCV
    pub async fn fetch_ohlcv(&self, opts: &OHLCVOptions) -> Result<Vec<Candle>, ExchangeError> {
        let key = serde_json::to_value(&opts.interval)
            .map_err(|e| failure(e.to_string()))?
            .as_str()
            .ok_or_else(|| failure("Invalid interval".into()))?
            .to_owned();
        let interval = crate::protocol::INTERVAL
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or("undefined");
        let limit = opts
            .limit
            .filter(|v| *v != 0. && !v.is_nan())
            .unwrap_or(500.)
            .min(1500.);
        let split = key.find(|c: char| !c.is_ascii_digit()).unwrap_or(key.len());
        let amount = key[..split].parse::<f64>().unwrap_or(f64::NAN);
        let services = self.host.services()?;
        let end = services.dayjs_value_ms(opts.to.filter(|v| *v != 0. && !v.is_nan()))?;
        let start = if !opts.limit.is_some_and(|v| v != 0. && !v.is_nan())
            && opts.from.is_some_and(|v| v != 0. && !v.is_nan())
        {
            services.dayjs_value_ms(opts.from)?
        } else {
            services.dayjs_subtract(end, amount * limit, &key[split..])?
        };
        let mut params = vec![
            ("symbol".into(), J::String(opts.symbol.clone())),
            ("start".into(), J::Number((start / 1000.).floor())),
            ("interval".into(), J::String(interval.into())),
        ];
        if interval != "D" {
            params.push(("end".into(), J::Number((end / 1000.).floor())));
        }
        let response = self
            .request("get", "/v3/klines", J::Object(params), J::Undefined, false)
            .await?;
        use crate::response::{self as data, float, numeric};
        let rows = data::get(data::get(&response, "data"), &opts.symbol);
        if !data::truthy(rows) {
            self.host.console(
                "error",
                &format!("No data for symbol {}", opts.symbol),
                Some(&response),
            )?;
            return Ok(Vec::new());
        }
        let rows = if let safe_cex_types::retained_json::RetainedJsonInput::Array(rows) = rows {
            rows
        } else {
            let map = data::property(rows, "map")?;
            if map.is_nullish() {
                return Ok(Vec::new());
            }
            let mut error = failure("data?.data?.[opts.symbol].map is not a function".into());
            error.name = "TypeError".into();
            return Err(error);
        };
        let mut candles = rows
            .iter()
            .map(|r| {
                Ok(Candle {
                    timestamp: numeric(data::property(r, "t")?)? / 1000.,
                    open: float(data::get(r, "o"))?,
                    high: float(data::get(r, "h"))?,
                    low: float(data::get(r, "l"))?,
                    close: float(data::get(r, "c"))?,
                    volume: float(data::get(r, "v"))?,
                    last_side: None,
                })
            })
            .collect::<Result<Vec<_>, ExchangeError>>()?;
        candles.sort_by(|a, b| {
            a.timestamp
                .is_nan()
                .cmp(&b.timestamp.is_nan())
                .then_with(|| {
                    a.timestamp
                        .partial_cmp(&b.timestamp)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        Ok(candles)
    }
    // ts: ApexOmniExchange.listenOHLCV
    pub fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        self.host.services()?.listen_ohlcv(opts, callback)
    }
    // ts: ApexOmniExchange.listenOrderBook
    pub fn listen_order_book(
        &self,
        symbol: &str,
        callback: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        self.host.services()?.listen_order_book(symbol, callback)
    }
    // ts: ApexOmniExchange.setLeverage
    pub async fn set_leverage(&self, symbol: &str, input: f64) -> Result<(), ExchangeError> {
        let market = self
            .base
            .store
            .markets_by_symbol()
            .get(symbol)
            .cloned()
            .ok_or_else(|| failure(format!("Market {symbol} not found")))?;
        let mut positions = self
            .base
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == symbol)
            .cloned()
            .collect::<Vec<_>>();
        if positions.is_empty() {
            let fetched = self.fetch_positions().await?;
            self.base.store.add_or_update_positions(fetched.clone())?;
            positions = fetched.into_iter().filter(|p| p.symbol == symbol).collect();
        }
        let position = positions.first().ok_or_else(|| {
            failure(format!(
                "Position state for {symbol} is unavailable; cannot set leverage"
            ))
        })?;
        let leverage = safe_cex_num::math_min(
            safe_cex_num::math_max(input, market.limits.leverage.min),
            market.limits.leverage.max,
        );
        if position.leverage != leverage {
            let rate =
                safe_cex_num::js_to_fixed(1. / leverage, 5.).map_err(|e| failure(e.into()))?;
            self.host
                .request(ApexRequest {
                    method: "post",
                    path: "/v3/set-initial-margin-rate",
                    params: J::Undefined,
                    data: data::object([
                        ("symbol", J::String(market.id)),
                        (
                            "token",
                            J::String(
                                if symbol.ends_with("USDT") {
                                    "USDT"
                                } else {
                                    "USDC"
                                }
                                .into(),
                            ),
                        ),
                        ("initialMarginRate", J::String(rate)),
                    ]),
                    content_type: Some("multipart/form-data"),
                    timeout_ms: None,
                    unlimited: true,
                })
                .await?;
        }
        Ok(())
    }
}

impl ApexOmniExchange {
    // ts: ApexOmniExchange.signOrder. The source body never reads its receiver.
    pub async fn sign_order(
        signer: &dyn ZkLinkHost,
        payload: &crate::transaction_records::SignContractPayload,
        _prefix: &J,
    ) -> Result<J, ExchangeError> {
        let signed = crate::transaction_signing::sign_contract(signer, payload).await?;
        let mut value = data::get(&signed, "tx");
        for _ in 0..2 {
            if matches!(value, J::Undefined | J::Null) {
                let mut error = ExchangeError::new(format!(
                    "Cannot read properties of {} (reading 'signature')",
                    if matches!(value, J::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                ));
                error.name = "TypeError".into();
                return Err(error);
            }
            value = data::get(value, "signature");
        }
        Ok(value.clone())
    }
}
