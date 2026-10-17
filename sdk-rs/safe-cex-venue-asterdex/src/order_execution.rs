//! Configured REST execution and open-interest ownership. API construction and
//! the complete exchange lifecycle remain separate manifest rows.
use crate::{builder, exchange_helpers as h, helpers, types::*};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::{adjust, js_number_string, js_string};
use safe_cex_transport::{RestAdapter, RestBody, RestRequest};
use safe_cex_types::{
    Order, OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PreparedOrderOpts,
    UpdateOrderOpts, UpdateOrderOptsUpdate,
};
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
// ts: AsterDEXExchange.OPEN_INTEREST_POLL_INTERVAL_MS / OPEN_INTEREST_BATCH_SIZE; the class constants alias these.
pub(crate) const OPEN_INTEREST_POLL_INTERVAL_MS: f64 = 15000.;
pub(crate) const OPEN_INTEREST_BATCH_SIZE: usize = 25;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ExecutionOrder {
    #[serde(flatten)]
    pub opts: AsterDEXPlaceOrderOpts,
    #[serde(rename = "postOnly", default)]
    pub post_only: Option<bool>,
}
/// Venue-only fields decorate the shared prepared carrier; the public extended
/// PlaceOrderOpts and its decoder retain their strict validation contract.
#[derive(Clone)]
pub struct PreparedExecutionOrder {
    pub opts: PreparedExecutionOptions,
    pub post_only: Option<bool>,
}
#[derive(Clone)]
pub struct PreparedExecutionOptions {
    pub order: PreparedOrderOpts,
    pub stp_mode: Option<AsterDEXStpMode>,
}
impl From<&ExecutionOrder> for PreparedExecutionOrder {
    fn from(input: &ExecutionOrder) -> Self {
        Self {
            opts: PreparedExecutionOptions {
                order: (&input.opts.order).into(),
                stp_mode: input.opts.stp_mode.clone(),
            },
            post_only: input.post_only,
        }
    }
}
impl From<ExecutionOrder> for PreparedExecutionOrder {
    fn from(input: ExecutionOrder) -> Self {
        Self::from(&input)
    }
}
impl From<&PreparedExecutionOrder> for PreparedExecutionOrder {
    fn from(input: &PreparedExecutionOrder) -> Self {
        input.clone()
    }
}
/// Insertion-ordered JS Map<string, number>, including nonfinite Numbers.
#[derive(Default, Clone)]
pub struct OpenInterestMap(Rc<RefCell<Vec<Option<(String, f64)>>>>);
impl OpenInterestMap {
    pub fn insert(&self, key: String, value: f64) {
        let mut entries = self.0.borrow_mut();
        if let Some((_, n)) = entries.iter_mut().flatten().find(|(k, _)| k == &key) {
            *n = value;
        } else {
            entries.push(Some((key, value)));
        }
    }
    pub fn delete(&self, key: &str) -> bool {
        let mut entries = self.0.borrow_mut();
        if let Some(slot) = entries
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|(k, _)| k == key))
        {
            *slot = None;
            true
        } else {
            false
        }
    }
    pub fn clear(&self) {
        for slot in self.0.borrow_mut().iter_mut() {
            *slot = None;
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.borrow().iter().all(Option::is_none)
    }
}
pub struct OrderExecution {
    pub owner: Rc<BaseExchange>,
    primary: Rc<dyn RestAdapter>,
    secondary: Rc<dyn RestAdapter>,
    public: Rc<dyn RestAdapter>,
    version: AsterDEXAPIVersion,
    pub open_interest_cursor: Cell<usize>,
}
fn coercion(e: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut e2 = ExchangeError::new(e.message);
    e2.name = e.name.into();
    e2
}
impl OrderExecution {
    pub fn new(
        owner: Rc<BaseExchange>,
        primary: Rc<dyn RestAdapter>,
        secondary: Rc<dyn RestAdapter>,
        public: Rc<dyn RestAdapter>,
        version: AsterDEXAPIVersion,
    ) -> Rc<Self> {
        Rc::new(Self {
            owner,
            primary,
            secondary,
            public,
            version,
            open_interest_cursor: Cell::new(0),
        })
    }
    fn path(&self, name: &str) -> String {
        h::get_fapi_path(name, &self.version, None)
    }
    async fn request(
        &self,
        adapter: &dyn RestAdapter,
        method: &str,
        url: String,
        body: Option<Value>,
        params: Option<Value>,
        suppress: bool,
    ) -> Result<Value, ExchangeError> {
        let mut request = RestRequest {
            method: method.into(),
            url,
            body: body.map(RestBody::Untransformed),
            params,
            ..Default::default()
        };
        if suppress {
            request.config_extensions.insert(
                "__suppressAsterUnknownOrderError".into(),
                safe_cex_types::retained_json::RetainedJsonInput::Bool(true),
            );
        }
        let response = adapter.execute(request).await.map_err(|failure| {
            let mut e = ExchangeError::new(failure.message);
            e.is_axios_error = true;
            e.code = failure.code.map(Value::String);
            e.context = failure.context.map(|c| *c);
            e.method = Some(failure.request.method);
            e.url = Some(failure.request.url);
            if let Some(r) = failure.response {
                e.response =
                    Some(json!({"status":r.status,"data":r.body.into_data(),"headers":r.headers}));
            }
            e
        })?;
        Ok(response
            .body
            .json_value()
            .unwrap_or_else(|_| response.body.into_data()))
    }
    // ts: AsterDEXExchange.validateAccount
    pub async fn validate_account(
        &self,
    ) -> Result<safe_cex_types::ValidateAccountResult, ExchangeError> {
        let result=async {
            let opts=self.owner.options();
            if opts.key.is_empty(){return Err(ExchangeError::new("Missing required credentials: API key"));}
            if opts.remote_signer.is_none() && self.owner.signers.remote_signer.is_none(){
                let auth=helpers::resolve_aster_dex_v3_auth_context(&opts);
                if self.version==AsterDEXAPIVersion::V1 && helpers::is_aster_dex_address(Some(&opts.key)) && helpers::is_aster_dex_v3_private_key(Some(&opts.secret)) && auth.user_address.is_none(){ return Err(ExchangeError::new(crate::tables::ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE)); }
                if self.version==AsterDEXAPIVersion::V3 {
                    if auth.user_address.is_none()||auth.signer_address.is_none()||auth.signer_private_key.is_none(){return Err(ExchangeError::new("Missing required Aster v3 auth fields: Main Wallet Address, API Wallet Address, and Generated Private Key"));}
                    if !helpers::is_aster_dex_v3_private_key(auth.signer_private_key.as_deref()){return Err(ExchangeError::new("Invalid Aster API Wallet private key format (expected 64-byte hex)"));}
                }else if opts.secret.is_empty(){return Err(ExchangeError::new("Missing required credentials: API secret"));}
            }
            // getAccount discards the alias here but still reads it (null throws).
            let account=match self.request(self.primary.as_ref(),"GET",h::get_account_endpoint(&self.version),None,None,false).await {
                Ok(v)=>v,Err(e)=>{self.owner.log(&format!("Failed to get account: {}",e.message),"error")?;return Err(e);}
            };
            if account.is_null(){let e=coercion(safe_cex_num::JsCoercionError{name:"TypeError",message:"Cannot read properties of null (reading 'accountAlias')"});self.owner.log(&format!("Failed to get account: {}",e.message),"error")?;return Err(e);}
            self.owner.log("Account validated successfully","info")?;Ok(safe_cex_types::ValidateAccountResult{error:"".into(),balance:None})
        }.await;
        if let Err(e) = &result {
            self.owner.log(
                &format!("Account validation failed: {}", e.message),
                "error",
            )?;
        }
        result
    }
    fn capacity(&self, symbol: &str, count: f64) -> Result<(), ExchangeError> {
        let result = {
            let markets = self.owner.store.markets_by_symbol();
            h::assert_algo_order_capacity(
                symbol,
                count,
                markets.get(symbol),
                &self.owner.store.orders(),
            )
        };
        if let Err((name, message)) = result {
            let mut e = ExchangeError::new(message);
            e.name = name.into();
            return Err(e);
        }
        Ok(())
    }
    // ts: AsterDEXExchange.placeOrder
    pub fn place_order<'a>(
        &'a self,
        input: impl Into<PreparedExecutionOrder> + 'a,
        skip: bool,
    ) -> ExchangeFuture<'a, Vec<String>> {
        let input = input.into();
        Box::pin(async move {
            let opts = &input.opts.order;
            self.owner.resolve_caller_client_order_id(opts)?;
            if opts.r#type == OrderType::TrailingStopLoss {
                return self.owner.reject_preflight("AsterDEX trailing stop-loss orders are unsupported because callbackRate is unavailable".into());
            }
            if input.opts.stp_mode.is_some() {
                h::assert_v3_feature(
                    &self.owner,
                    self.version.clone(),
                    "per-order self-trade prevention",
                )?;
            }
            let truthy = |v: Option<f64>| v.is_some_and(|v| v != 0.0 && !v.is_nan());
            if (truthy(opts.take_profit) || truthy(opts.stop_loss))
                && matches!(
                    opts.r#type.into_value(),
                    Some(OrderType::Limit | OrderType::Market)
                )
            {
                let fmt = |v: Option<f64>| {
                    if truthy(v) {
                        js_number_string(v.unwrap())
                    } else {
                        "N/A".into()
                    }
                };
                self.owner.log(
                    &format!(
                        "Placing bracket order: {} {} {} with TP={} SL={}",
                        opts.symbol,
                        if opts.side == OrderSide::Buy {
                            "buy"
                        } else {
                            "sell"
                        },
                        if opts.r#type == OrderType::Limit {
                            "limit"
                        } else {
                            "market"
                        },
                        fmt(opts.take_profit),
                        fmt(opts.stop_loss)
                    ),
                    "info",
                )?;
                self.capacity(
                    &opts.symbol,
                    (truthy(opts.take_profit) as u8 + truthy(opts.stop_loss) as u8) as f64,
                )?;
                let mut main = input.clone();
                main.opts.order.take_profit = None;
                main.opts.order.stop_loss = None;
                let mut orders = vec![main];
                for (kind, trigger) in [
                    (OrderType::StopLoss, opts.stop_loss),
                    (OrderType::TakeProfit, opts.take_profit),
                ] {
                    if !truthy(trigger) {
                        continue;
                    }
                    let new: PreparedOrderOpts = PreparedOrderOpts {
                        symbol: opts.symbol.clone(),
                        r#type: kind.into(),
                        side: if opts.side == OrderSide::Buy {
                            OrderSide::Sell.into()
                        } else {
                            OrderSide::Buy.into()
                        },
                        amount: opts.amount,
                        price: None,
                        trigger_price: trigger,
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: true.into(),
                        time_in_force: None,
                        trigger_type: None,
                        tp_trigger_type: if kind == OrderType::TakeProfit {
                            opts.tp_trigger_type
                        } else {
                            None
                        },
                        sl_trigger_type: if kind == OrderType::StopLoss {
                            opts.sl_trigger_type
                        } else {
                            None
                        },
                        client_order_id: None,
                        close: None,
                        max_slippage_pct: None,
                        implied_volatility: None,
                    };
                    orders.push(PreparedExecutionOrder {
                        opts: PreparedExecutionOptions {
                            order: new,
                            stp_mode: None,
                        },
                        post_only: None,
                    });
                }
                return self.place_orders(&orders, true).await;
            }
            if !skip
                && matches!(
                    opts.r#type.into_value(),
                    Some(OrderType::StopLoss | OrderType::TakeProfit)
                )
            {
                self.capacity(&opts.symbol, 1.0)?;
            }
            let result = async {
                let kind = opts
                    .r#type
                    .into_value()
                    .map(h::map_order_type)
                    .transpose()
                    .map_err(ExchangeError::new)?;
                let market = self
                    .owner
                    .store
                    .markets_by_symbol()
                    .get(&opts.symbol)
                    .cloned()
                    .ok_or_else(|| {
                        ExchangeError::new(format!("Market {} not found", opts.symbol))
                    })?;
                let max = market.limits.amount.max;
                if matches!(
                    opts.r#type.into_value(),
                    Some(OrderType::Limit | OrderType::Market)
                ) && max.is_finite()
                    && max > 0.0
                    && opts.amount > max
                {
                    let total = adjust(opts.amount, market.precision.amount);
                    let max = adjust(max, market.precision.amount);
                    if total > max {
                        // Source loops forever when adjusted max is zero. Refuse that
                        // malformed precision boundary instead of hanging the host.
                        if max <= 0.0 || !total.is_finite() {
                            let mut e = ExchangeError::new(
                                "AsterDEX order split cannot make finite progress",
                            );
                            e.name = "RepresentationError".into();
                            return Err(e);
                        }
                        let mut orders = Vec::new();
                        let mut remaining = total;
                        while remaining > max {
                            let mut chunk = input.clone();
                            chunk.opts.order.amount = max;
                            orders.push(chunk);
                            let next = adjust(remaining - max, market.precision.amount);
                            if next >= remaining {
                                let mut e = ExchangeError::new(
                                    "AsterDEX order split cannot make finite progress",
                                );
                                e.name = "RepresentationError".into();
                                return Err(e);
                            }
                            remaining = next;
                        }
                        if remaining > 0.0 {
                            let mut chunk = input.clone();
                            chunk.opts.order.amount = remaining;
                            orders.push(chunk);
                        }
                        self.owner.log(
                            &format!(
                                "Splitting oversized order for {}: {} into {} chunk(s) (max {})",
                                opts.symbol,
                                js_number_string(total),
                                orders.len(),
                                js_number_string(max)
                            ),
                            "info",
                        )?;
                        return self.place_orders(&orders, skip).await;
                    }
                }
                let mut params = json!({"symbol":opts.symbol});
                if let Some(side) = opts.side.into_value() {
                    params["side"] = json!(match side {
                        OrderSide::Buy => "BUY",
                        OrderSide::Sell => "SELL",
                    });
                }
                if let Some(kind) = kind {
                    params["type"] = json!(kind);
                }
                if let Some(mode) = &input.opts.stp_mode {
                    params["stpMode"] = serde_json::to_value(mode).unwrap();
                }
                if self.version == AsterDEXAPIVersion::V3 {
                    if let Some(config) =
                        builder::resolve_aster_builder_fee_config(&self.owner.options())
                            .map_err(ExchangeError::new)?
                    {
                        if let Some(rate) = builder::get_aster_builder_fee_rate_for_order(
                            opts,
                            input.post_only,
                            kind.unwrap_or(""),
                            &config,
                        ) {
                            params["builder"] = json!(config.builder_address);
                            params["feeRate"] = json!(rate);
                        }
                    }
                }
                let position_side =
                    h::get_order_position_side(opts, self.owner.store.options().is_hedged);
                if position_side != "BOTH" {
                    params["positionSide"] = json!(position_side);
                }
                if opts.amount == 0.0
                    && matches!(
                        opts.r#type.into_value(),
                        Some(OrderType::TakeProfit | OrderType::StopLoss)
                    )
                {
                    params["closePosition"] = json!("true");
                } else {
                    params["quantity"] = json!(js_number_string(adjust(
                        opts.amount,
                        market.precision.amount
                    )));
                    if opts.reduce_only == Some(true) && !self.owner.store.options().is_hedged {
                        params["reduceOnly"] = json!("true");
                    }
                }
                if matches!(kind, Some("LIMIT" | "STOP" | "TAKE_PROFIT")) {
                    params["timeInForce"] = json!(match opts
                        .time_in_force
                        .unwrap_or(OrderTimeInForce::GoodTillCancel)
                    {
                        OrderTimeInForce::GoodTillCancel => "GTC",
                        OrderTimeInForce::ImmediateOrCancel => "IOC",
                        OrderTimeInForce::FillOrKill => "FOK",
                        OrderTimeInForce::PostOnly => "GTX",
                    });
                }
                let tick = h::get_tick_size(&market).map_err(coercion)?;
                if opts.r#type == OrderType::Limit && truthy(opts.price) {
                    params["price"] = json!(js_number_string(adjust(opts.price.unwrap(), tick)));
                }
                if matches!(
                    opts.r#type.into_value(),
                    Some(OrderType::StopLoss | OrderType::TakeProfit)
                ) && truthy(opts.trigger_price)
                {
                    params["stopPrice"] =
                        json!(js_number_string(adjust(opts.trigger_price.unwrap(), tick)));
                    params["workingType"] = json!(if opts.r#type == OrderType::TakeProfit {
                        if opts.tp_trigger_type == Some(OrderStopTriggerType::MarkPrice) {
                            "MARK_PRICE"
                        } else {
                            "CONTRACT_PRICE"
                        }
                    } else if opts.sl_trigger_type == Some(OrderStopTriggerType::LastPrice) {
                        "CONTRACT_PRICE"
                    } else {
                        "MARK_PRICE"
                    });
                }
                let response = self
                    .request(
                        self.secondary.as_ref(),
                        "POST",
                        self.path("order"),
                        Some(params),
                        None,
                        false,
                    )
                    .await?;
                if response.is_null() {
                    return Err(coercion(safe_cex_num::JsCoercionError {
                        name: "TypeError",
                        message: "Cannot read properties of null (reading 'orderId')",
                    }));
                }
                let id = response
                    .get("orderId")
                    .filter(|v| !v.is_null())
                    .or_else(|| response.get("data").and_then(|v| v.get("orderId")));
                if id.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
                    return Err(ExchangeError::new(
                        "AsterDEX order response did not include orderId",
                    ));
                }
                let id = js_string(id).map_err(coercion)?;
                self.owner.log(&format!("Order placed: {id}"), "info")?;
                Ok(vec![id])
            }
            .await;
            if let Err(e) = &result {
                self.owner
                    .log(&format!("Failed to place order: {}", e.message), "error")?;
            }
            result
        })
    }
    // ts: AsterDEXExchange.placeOrders
    pub async fn place_orders(
        &self,
        orders: &[impl Clone + Into<PreparedExecutionOrder>],
        skip: bool,
    ) -> Result<Vec<String>, ExchangeError> {
        let orders: Vec<PreparedExecutionOrder> = orders.iter().cloned().map(Into::into).collect();
        self.owner.assert_caller_client_order_ids(
            &orders
                .iter()
                .map(|o| o.opts.order.clone())
                .collect::<Vec<_>>(),
        )?;
        let mut ids = Vec::new();
        let mut error = String::new();
        for opts in &orders {
            match self.place_order(opts, skip).await {
                Ok(v) => ids.extend(v),
                Err(e) => {
                    error = if e.message.is_empty() {
                        e.js_error_string()
                    } else {
                        e.message
                    };
                    self.owner
                        .log(&format!("Failed to place order: {error}"), "error")?;
                }
            }
        }
        if !error.is_empty() && ids.is_empty() {
            return Err(ExchangeError::new(error));
        }
        if !ids.is_empty() {
            self.owner
                .log(&format!("Orders placed: {}", ids.join(", ")), "info")?;
        }
        Ok(ids)
    }
    // ts: AsterDEXExchange.updateOrder
    pub async fn update_order(&self, opts: &UpdateOrderOpts) -> Result<Vec<String>, ExchangeError> {
        let old = &opts.order;
        let (amount, price) = match &opts.update {
            UpdateOrderOptsUpdate::Variant0(v) => (v.amount, v.price.or(old.price.into_value())),
            UpdateOrderOptsUpdate::Variant1(v) => (v.amount.unwrap_or(old.amount), Some(v.price)),
        };
        let algo = matches!(
            old.r#type,
            safe_cex_types::ModelField::Value(OrderType::TakeProfit)
                | safe_cex_types::ModelField::Value(OrderType::StopLoss)
        );
        let new = PreparedExecutionOrder {
            opts: PreparedExecutionOptions {
                order: PreparedOrderOpts {
                    symbol: old.symbol.clone(),
                    r#type: old.r#type.into_property(),
                    side: old.side.into_property(),
                    amount,
                    reduce_only: old.reduce_only.into_property(),
                    price: (!algo).then_some(price).flatten(),
                    trigger_price: algo.then_some(price).flatten(),
                    take_profit: None,
                    stop_loss: None,
                    tp_trigger_type: (old.tp_trigger_type).into_value(),
                    sl_trigger_type: (old.sl_trigger_type).into_value(),
                    time_in_force: (old.time_in_force).into_value(),
                    trigger_type: None,
                    client_order_id: None,
                    close: None,
                    max_slippage_pct: None,
                    implied_volatility: None,
                },
                stp_mode: None,
            },
            post_only: None,
        };
        self.cancel_orders(std::slice::from_ref(old)).await?;
        self.place_order(&new, false).await
    }
    // ts: AsterDEXExchange.cancelOrders
    pub async fn cancel_orders(&self, orders: &[Order]) -> Result<(), ExchangeError> {
        let mut ids = Vec::new();
        let mut error = String::new();
        for order in orders {
            match self
                .request(
                    self.secondary.as_ref(),
                    "DELETE",
                    self.path("order"),
                    None,
                    Some(json!({"symbol":order.symbol,"orderId":order.id})),
                    true,
                )
                .await
            {
                Ok(_) => ids.push(order.id.clone()),
                Err(e) => {
                    if h::is_aster_dex_unknown_order_error(None, Some(&e.message))
                        .map_err(coercion)?
                    {
                        self.owner.log(&format!("Order {} was already gone on AsterDEX; treating cancel as complete",order.id),"info")?;
                        ids.push(order.id.clone());
                        continue;
                    }
                    error = if e.message.is_empty() {
                        e.js_error_string()
                    } else {
                        e.message
                    };
                    self.owner.log(
                        &format!("Failed to cancel order {}: {error}", order.id),
                        "error",
                    )?;
                }
            }
        }
        if !error.is_empty() && ids.is_empty() {
            return Err(ExchangeError::new(error));
        }
        if !ids.is_empty() {
            self.owner
                .log(&format!("Orders cancelled: {}", ids.join(", ")), "info")?;
        }
        Ok(())
    }
    // ts: AsterDEXExchange#updateStoreOpenInterest
    pub fn update_store_open_interest(
        &self,
        values: &OpenInterestMap,
    ) -> Result<(), ExchangeError> {
        let mut index = 0;
        loop {
            let entry = {
                let entries = values.0.borrow();
                if index >= entries.len() {
                    break;
                }
                entries[index].clone()
            };
            index += 1;
            let Some((symbol, n)) = entry else {
                continue;
            };
            let id = {
                let tickers = self.owner.store.tickers_by_symbol();
                let entries: Vec<_> = tickers.iter().map(|(k, v)| (k.as_str(), v)).collect();
                match safe_cex_num::js_table_lookup(&entries, &symbol) {
                    safe_cex_num::TableHit::Mapped(ticker) => {
                        if ticker.open_interest == n {
                            continue;
                        }
                        ticker.id.clone()
                    }
                    // A normal inherited prototype member has undefined id and OI.
                    // Source updateTicker coerces that id to the property "undefined".
                    safe_cex_num::TableHit::Inherited(_) => "undefined".into(),
                    safe_cex_num::TableHit::Missing => continue,
                }
            };
            let mut patch = safe_cex_store::model_patch::ModelPatch::default();
            patch.set_number("openInterest", n);
            self.owner.store.update_ticker(&id, &patch)?;
        }
        Ok(())
    }
    async fn interest_data(&self, symbol: Option<&str>) -> Result<Value, ExchangeError> {
        let response = self
            .public
            .execute(RestRequest {
                method: "GET".into(),
                url: self.path("openInterest"),
                params: symbol.map(|s| json!({"symbol":s})),
                timeout_ms: Some(2000),
                ..Default::default()
            })
            .await
            .map_err(|e| ExchangeError::new(e.message))?;
        Ok(response
            .body
            .json_value()
            .unwrap_or_else(|_| response.body.into_data()))
    }
    async fn interest(
        &self,
        symbols: Option<&[String]>,
        fallback: bool,
    ) -> Result<OpenInterestMap, ExchangeError> {
        let mut raw = Map::new();
        let bulk = match self.interest_data(None).await {
            Ok(data) => {
                h::collect_open_interest_from_payload(Some(&data), &mut raw).map_err(coercion)
            }
            Err(e) => Err(e),
        };
        if let Err(e) = bulk {
            self.owner.log(
                &format!("Bulk open-interest fetch failed: {}", e.message),
                "warning",
            )?;
        }
        if raw.is_empty() && fallback {
            if let Some(symbols) = symbols {
                for symbol in symbols.iter().filter(|s| !s.is_empty()) {
                    if let Ok(data) = self.interest_data(Some(symbol)).await {
                        let _ = h::collect_open_interest_from_payload(Some(&data), &mut raw);
                    }
                }
            }
        }
        let values = OpenInterestMap::default();
        for (key, value) in raw {
            if let Some(n) = value.as_f64() {
                values.insert(key, n);
            }
        }
        Ok(values)
    }
    // ts: AsterDEXExchange#pollOpenInterest
    pub async fn poll_open_interest(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.owner.is_disposed() {
            return Ok(());
        }
        let result = async {
            let symbols: Vec<_> = self
                .owner
                .store
                .tickers()
                .iter()
                .map(|t| t.symbol.clone())
                .filter(|s| !s.is_empty())
                .collect();
            if symbols.is_empty() {
                return Ok(());
            }
            let mut values = self.interest(None, false).await?;
            if values.is_empty() {
                let size = OPEN_INTEREST_BATCH_SIZE.min(symbols.len());
                let start = self.open_interest_cursor.get() % symbols.len();
                let batch: Vec<_> = (0..size)
                    .map(|i| symbols[(start + i) % symbols.len()].clone())
                    .collect();
                self.open_interest_cursor
                    .set((start + size) % symbols.len());
                values = self.interest(Some(&batch), true).await?;
            }
            self.update_store_open_interest(&values)
        }
        .await;
        let logged = match result {
            Ok(()) => Ok(()),
            Err(e) => self.owner.log(
                &format!("Open-interest poll failed: {}", e.message),
                "warning",
            ),
        };
        if !self.owner.is_disposed() {
            let weak = Rc::downgrade(self);
            self.owner
                .start_timeout(OPEN_INTEREST_POLL_INTERVAL_MS as u64, move || {
                    if let Some(this) = weak.upgrade() {
                        this.start_open_interest_polling();
                    }
                    Ok(())
                })?;
        }
        logged
    }
    // ts: AsterDEXExchange#startOpenInterestPolling
    pub fn start_open_interest_polling(self: &Rc<Self>) {
        let this = self.clone();
        self.owner.start_owned_operation(async move {
            if let Err(e) = this.poll_open_interest().await {
                this.owner.runtime.report_exception(&e, &Value::Null);
            }
        });
    }
}
