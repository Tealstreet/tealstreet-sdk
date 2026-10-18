use super::{
    error::{failure, from_thrown},
    reads::items,
    rt::Host,
    value::{self, to_js},
};
use crate::{
    market_position_reads::{ReadItem, ReadMarket},
    order_create::{
        self, CoinbaseOptimisticOrder, CoinbaseOrderFailure, CoinbaseOrderHost,
        CoinbaseOrderPolling, CoinbaseOrderResponse,
    },
    order_edit::*,
    order_sync::*,
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use indexmap::IndexMap;
use safe_cex_exchange::js_runtime::value::{from_typed, to_typed};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{Market, Order, OrderStatus, PreparedOrderOpts};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn order_error(error: PortfolioFailure) -> CoinbaseOrderFailure {
    CoinbaseOrderFailure {
        value: error.value.clone(),
        native: Some(super::error::exchange_error(&error)),
    }
}
fn portfolio_error(error: CoinbaseOrderFailure) -> PortfolioFailure {
    match error.native {
        Some(native) => failure(native),
        None => Rc::new(PortfolioThrown::from_value(error.value)),
    }
}

pub(super) struct OrderHost {
    host: Host,
    markets: IndexMap<String, Rc<Market>>,
}
impl OrderHost {
    fn new(host: Host) -> Self {
        Self {
            host,
            markets: IndexMap::new(),
        }
    }
}
impl Host {
    pub(super) fn order_snapshot_receiver(&self) -> Rc<OrderSnapshotReceiver<Self>> {
        Rc::new(OrderSnapshotReceiver {
            host: self.clone(),
            spawn: self.shared.extra.spawn.clone(),
        })
    }
    pub(super) fn order_host(&self) -> Rc<RefCell<OrderHost>> {
        Rc::new(RefCell::new(OrderHost::new(self.clone())))
    }
    pub(super) fn schedule_reconcile(&self, reason: &str) -> Result<(), PortfolioFailure> {
        crate::exchange_lifecycle::schedule_open_orders_rest_reconciliation(
            self.class()?.lifecycle_receiver(self.clone()),
            reason.into(),
        )
    }
    pub(super) fn schedule_write_check(&self) -> Result<(), PortfolioFailure> {
        crate::exchange_lifecycle::schedule_private_user_stream_write_check(
            self.class()?.lifecycle_receiver(self.clone()),
        )
    }
    pub(super) fn synced_order(order: Order) -> SyncedOrderRoot {
        let mut fields = order;
        let info =
            Rc::new(RefCell::new(fields.info.take().unwrap_or(
                safe_cex_types::retained_json::RetainedJsonInput::Null,
            )));
        Rc::new(RefCell::new(SyncedOrder { fields, info }))
    }
    pub(super) fn native_orders(orders: &SyncedOrders) -> Vec<Order> {
        orders
            .borrow()
            .iter()
            .map(|order| {
                let order = order.borrow();
                let mut fields = order.fields.clone();
                fields.info = (Some(order.info.borrow().clone())).into();
                fields
            })
            .collect()
    }
    pub(super) fn emit_orders_error(
        &self,
        error: PortfolioFailure,
    ) -> Result<(), PortfolioFailure> {
        let class = self.class()?;
        let mut state = CoinbaseOrderPolling {
            next_orders_poll_at: class.next_orders_poll_at.get(),
        };
        let result = order_create::emit_orders_fetch_error(
            &mut state,
            &mut OrderHost::new(self.clone()),
            error.value.clone(),
        )
        .map_err(portfolio_error);
        class.next_orders_poll_at.set(state.next_orders_poll_at);
        result
    }
    pub(super) fn should_poll(&self) -> Result<bool, PortfolioFailure> {
        let class = self.class()?;
        let mut state = CoinbaseOrderPolling {
            next_orders_poll_at: class.next_orders_poll_at.get(),
        };
        let result =
            order_create::should_poll_orders(&mut state, &mut OrderHost::new(self.clone()))
                .map_err(portfolio_error);
        class.next_orders_poll_at.set(state.next_orders_poll_at);
        result
    }
    pub(super) fn merge_rest(&self, orders: Frame) -> Result<Frame, PortfolioFailure> {
        let orders: Vec<Order> = to_typed(&to_js(&orders)).map_err(from_thrown)?;
        let merged = order_create::merge_rest_orders_with_fresh_ws_state(
            &mut OrderHost::new(self.clone()),
            Rc::new(orders.into_iter().map(Rc::new).collect()),
        )
        .map_err(portfolio_error)?;
        value::from_js(&from_typed(merged.as_ref()).map_err(from_thrown)?)?
            .ok_or_else(|| Host::json_error("Error", "Undefined merged orders"))
    }
    pub(super) fn place(
        &self,
        options: impl Into<PreparedOrderOpts>,
    ) -> Promise<Result<Vec<String>, PortfolioFailure>> {
        let result =
            order_create::place_order(self.order_host(), options, &self.shared.extra.spawn);
        self.promise(async move { result.await.map_err(portfolio_error) })
    }
    pub(super) fn cancel(&self, orders: Vec<Order>) -> Promise<Result<(), PortfolioFailure>> {
        let result = order_create::cancel_orders(
            self.order_host(),
            orders.into_iter().map(Rc::new).collect(),
            &self.shared.extra.spawn,
        );
        self.promise(async move { result.await.map_err(portfolio_error) })
    }
}
impl CoinbaseOrderHost for OrderHost {
    fn now_ms(&mut self) -> f64 {
        self.host.now()
    }
    fn orders_loaded(&mut self) -> Result<bool, CoinbaseOrderFailure> {
        Ok(self
            .host
            .shared
            .store
            .loaded()
            .map_err(|error| order_error(failure(error.into())))?
            .orders)
    }
    fn orders(&mut self) -> Result<Vec<Rc<Order>>, CoinbaseOrderFailure> {
        Ok(self
            .host
            .shared
            .store
            .orders()
            .map_err(|error| order_error(failure(error.into())))?
            .iter()
            .cloned()
            .map(Rc::new)
            .collect())
    }
    fn market_by_symbol(
        &mut self,
        symbol: &str,
    ) -> Result<Option<Rc<Market>>, CoinbaseOrderFailure> {
        Ok(self
            .host
            .shared
            .store
            .markets_by_symbol()
            .map_err(|error| order_error(failure(error.into())))?
            .get(symbol)
            .cloned()
            .map(Rc::new))
    }
    fn markets_by_id(&mut self) -> Result<&IndexMap<String, Rc<Market>>, CoinbaseOrderFailure> {
        self.markets = self
            .host
            .shared
            .store
            .markets_by_id()
            .map_err(|error| order_error(failure(error.into())))?
            .iter()
            .map(|(key, value)| (key.clone(), Rc::new(value.clone())))
            .collect();
        Ok(&self.markets)
    }
    fn resolve_caller_client_order_id(
        &mut self,
        options: &PreparedOrderOpts,
    ) -> Result<Option<String>, CoinbaseOrderFailure> {
        self.host
            .base()
            .resolve_caller_client_order_id(options)
            .map_err(|error| order_error(failure(error)))
    }
    fn uuid(&mut self) -> String {
        super::crypto::uuid(self.host.shared.runtime.as_ref())
    }
    fn post_create_order(
        &mut self,
        endpoint: &'static str,
        payload: &crate::types::PlaceOrderRequest,
    ) -> Promise<Result<CoinbaseOrderResponse, CoinbaseOrderFailure>> {
        let request = self.host.request(
            "post",
            endpoint,
            Some(
                serde_json::to_value(payload)
                    .expect("outbound create DTO")
                    .into(),
            ),
        );
        let (promise, resolve) = Promise::pending();
        request.observe(&self.host.shared.extra.spawn, move |result| { resolve.resolve(result.map_err(order_error).and_then(|data| data.as_object().map(|fields| CoinbaseOrderResponse::from_retained(fields.to_vec())).ok_or_else(|| CoinbaseOrderFailure { value: Some(json!({"name":"TypeError","message":"Coinbase create response is not a record"})) , native: None }))); });
        promise
    }
    fn post_cancel_orders(
        &mut self,
        endpoint: &'static str,
        ids: &[String],
    ) -> Promise<Result<(), CoinbaseOrderFailure>> {
        let request = self.host.request(
            "post",
            endpoint,
            Some(Input::from(json!({"order_ids":ids}))),
        );
        let (promise, resolve) = Promise::pending();
        request.observe(&self.host.shared.extra.spawn, move |result| {
            resolve.resolve(result.map(|_| ()).map_err(order_error));
        });
        promise
    }
    fn add_or_update_order(
        &mut self,
        order: Rc<CoinbaseOptimisticOrder>,
    ) -> Result<(), CoinbaseOrderFailure> {
        let info = safe_cex_types::retained_json::RetainedJsonInput::Object(
            order
                .info
                .fields
                .borrow()
                .iter()
                .map(|(key, value)| (key.clone(), value.borrow().clone()))
                .collect(),
        );
        self.host
            .shared
            .store
            .add_or_update_order(Order {
                id: order.id.clone(),
                parent_id: (Some(order.parent_id.clone())).into(),
                status: (order.status).into(),
                symbol: order.symbol.clone(),
                r#type: (order.r#type).into(),
                side: (order.side).into(),
                price: (order.price).into(),
                amount: order.amount,
                filled: order.filled,
                remaining: order.remaining,
                extra: (Some(order.extra.clone())).into(),
                reduce_only: order.reduce_only.into(),
                info: (Some(info)).into(),
                last_update: order.last_update,
                close: (None).into(),
                client_order_id: (None).into(),
                time_in_force: (None).into(),
                trigger_type: (None).into(),
                tp_trigger_type: (None).into(),
                sl_trigger_type: (None).into(),
                take_profit: (None).into(),
                stop_loss: (None).into(),
            })
            .map_err(|error| order_error(Rc::new(error.into())))
    }
    fn update_orders_status(
        &mut self,
        ids: &[String],
        status: OrderStatus,
    ) -> Result<(), CoinbaseOrderFailure> {
        self.host
            .shared
            .store
            .update_orders_status(&ids.iter().map(String::as_str).collect::<Vec<_>>(), status)
            .map_err(|error| order_error(Rc::new(error.into())))
    }
    fn schedule_open_orders_rest_reconciliation(
        &mut self,
        reason: &'static str,
    ) -> Result<(), CoinbaseOrderFailure> {
        self.host.schedule_reconcile(reason).map_err(order_error)
    }
    fn schedule_private_user_stream_write_check(&mut self) -> Result<(), CoinbaseOrderFailure> {
        self.host.schedule_write_check().map_err(order_error)
    }
    fn log_warning(&mut self, message: &'static str) -> Result<(), CoinbaseOrderFailure> {
        self.host
            .base()
            .log(message, "warning")
            .map_err(|error| order_error(failure(error)))
    }
    fn emit_and_log_error(&mut self, args: &[Option<&Value>]) -> Result<(), CoinbaseOrderFailure> {
        self.host
            .emit_args(&args.iter().map(|value| value.cloned()).collect::<Vec<_>>())
            .map_err(order_error)
    }
}
impl OrderSnapshotHost for Host {
    fn public_only(&self) -> bool {
        self.public_only()
    }
    fn supported_types(&self) -> OrderSyncAsync<Rc<RefCell<Vec<String>>>> {
        Ok(self.class()?.supported_types())
    }
    fn get_orders(
        &self,
        endpoint: &'static str,
        query: OrderPageQuery,
    ) -> OrderSyncAsync<OrderPage> {
        let mut params = Vec::from_iter([
            (
                "order_status".encode_utf16().collect(),
                Input::from(json!(query.order_status)),
            ),
            ("limit".encode_utf16().collect(), Input::Float(query.limit)),
        ]);
        for (key, value) in [
            (
                "product_type",
                query.product_type.map(|v| Input::from(Value::from(v))),
            ),
            (
                "contract_expiry_type",
                query
                    .contract_expiry_type
                    .map(|v| Input::from(Value::from(v))),
            ),
            ("cursor", query.cursor),
        ] {
            if let Some(value) = value {
                params.push((key.encode_utf16().collect(), value));
            }
        }
        let result = self.request("get", endpoint, Some(Input::Object(params)));
        Ok(self.promise(async move {
            let data = result.await?;
            Ok(OrderPage {
                orders: items(&data, "orders")?,
                has_next: data.get("has_next").cloned(),
                cursor: data.get("cursor").cloned(),
            })
        }))
    }
    fn resolve_market_by_product(
        &self,
        product_id: String,
    ) -> Result<Option<ReadMarket>, PortfolioFailure> {
        Ok(order_create::resolve_market_by_coinbase_product_id(
            &mut OrderHost::new(self.clone()),
            &product_id,
        )
        .map_err(portfolio_error)?
        .map(|market| Rc::new(RefCell::new((*market).clone()))))
    }
    fn map_rest_order(
        &self,
        raw: ReadItem,
        market: ReadMarket,
        now: f64,
    ) -> Result<Option<SyncedOrderRoot>, PortfolioFailure> {
        Ok(
            crate::orders::map_coinbase_rest_order(&raw.borrow(), &market.borrow(), now).map(
                |mut fields| {
                    fields.info = (None).into();
                    Rc::new(RefCell::new(SyncedOrder {
                        fields,
                        info: raw.clone(),
                    }))
                },
            ),
        )
    }
    fn now_ms(&self) -> Result<f64, PortfolioFailure> {
        Ok(self.now())
    }
    fn emit_and_log_error(&self, message: String) -> Result<(), PortfolioFailure> {
        self.emit(Some(Value::String(message)))
    }
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure> {
        self.emit_orders_error(error)
    }
}
impl OrderEditHost for Host {
    fn selected_portfolio_type(&self) -> Result<Option<String>, PortfolioFailure> {
        Ok(self
            .class()?
            .selected_portfolio
            .borrow()
            .as_ref()
            .map(|portfolio| portfolio.borrow().r#type.clone()))
    }
    fn orders(&self) -> Result<SyncedOrders, PortfolioFailure> {
        Ok(Rc::new(RefCell::new(
            self.shared
                .store
                .orders()
                .map_err(|error| failure(error.into()))?
                .iter()
                .cloned()
                .map(Self::synced_order)
                .collect(),
        )))
    }
    fn market_by_symbol(&self, symbol: String) -> Result<Option<ReadMarket>, PortfolioFailure> {
        Ok(self
            .shared
            .store
            .markets_by_symbol()
            .map_err(|error| failure(error.into()))?
            .get(&symbol)
            .cloned()
            .map(|market| Rc::new(RefCell::new(market))))
    }
    fn cancel_orders(&self, orders: SyncedOrders) -> EditAsync<()> {
        Ok(self.cancel(Self::native_orders(&orders)))
    }
    fn place_order(&self, order: EditReplacement) -> EditAsync<Vec<String>> {
        Ok(self.place(PreparedOrderOpts {
            symbol: order.symbol,
            r#type: order.r#type,
            side: order.side,
            price: order.price,
            amount: order.amount.unwrap_or(f64::NAN),
            reduce_only: order.reduce_only.into_property(),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            time_in_force: None,
            trigger_type: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            max_slippage_pct: None,
            close: None,
            implied_volatility: None,
        }))
    }
    fn post_edit(
        &self,
        endpoint: &'static str,
        payload: crate::types::EditOrderRequest,
    ) -> EditAsync<safe_cex_types::retained_json::RetainedJsonInput> {
        Ok(self.request(
            "post",
            endpoint,
            Some(
                serde_json::to_value(payload)
                    .expect("outbound edit DTO")
                    .into(),
            ),
        ))
    }
    fn schedule_reconciliation(&self, reason: &'static str) -> Result<(), PortfolioFailure> {
        self.schedule_reconcile(reason)
    }
    fn schedule_write_check(&self) -> Result<(), PortfolioFailure> {
        self.schedule_write_check()
    }
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure> {
        self.emit(message)
    }
}
