//! Concrete Spot Exchange assembled from the existing Spot class bodies.
use crate::{
    binding::{Binding, HyperliquidDependencies, RuntimeClock, base_exchange_rejection, opt},
    cloid::hyperliquid_client_order_id_rule,
    host::{
        Host,
        facade_exchange::base_exchange_class,
        facade_rest::{RestDependencies, bind_api_factories},
        facade_socket::{SocketDependencies, base_websocket_class},
        facade_store::store_object,
        imports,
    },
    spot_runtime,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ListenerCleanup},
    ws_parent::WsExchangeParent,
};
use safe_cex_store::Store;
use safe_cex_types::{
    Candle, ExchangeAccount, ExchangeName, ExchangeOptions, Market, OHLCVOptions, Order, OrderBook,
    PlaceOrderOpts, Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts,
    ValidateAccountResult,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct HyperliquidSpotExchange {
    base: Rc<BaseExchange>,
    capabilities: ExchangeCapabilities,
    pub(crate) binding: RefCell<Option<Binding>>,
}
impl HyperliquidSpotExchange {
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new(
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: HyperliquidDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let runtime: Rc<dyn ExchangeRuntime> = deps.runtime.clone();
        let mut base = BaseExchange::new(
            ExchangeName::Hyperliquidspot,
            options.clone(),
            store.clone(),
            runtime.clone(),
            ExchangeSettings {
                tick_interval: Some(2000.0),
                supports_public_only: true,
                client_order_id_rule: Some(hyperliquid_client_order_id_rule()),
                ..Default::default()
            },
        );
        base.is_spot = true;
        base.signers = deps.signers;
        let venue = Rc::new(Self {
            base: Rc::new(base),
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|venue, ()| venue.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
                ..Default::default()
            },
            binding: RefCell::new(None),
        });
        let exchange: Rc<dyn Exchange> = venue.clone();
        let mut realm = Host::new(deps.runtime.clone(), deps.http.clone(), store.clone());
        imports::bind(&realm);
        bind_api_factories(
            &realm,
            venue.base.clone(),
            RestDependencies {
                http: deps.http,
                limiter: deps.limiter,
                clock: Arc::new(RuntimeClock::new(deps.runtime.clone())),
            },
        );
        let base = base_exchange_class(&mut realm, venue.base.clone(), deps.external_public_data);
        realm.bind_import("BaseExchange", base);
        let websocket = base_websocket_class(
            &mut realm,
            SocketDependencies {
                runtime: deps.runtime,
                parent: WsExchangeParent::new(&exchange),
                base: venue.base.clone(),
            },
        );
        realm.bind_import("BaseWebSocket", websocket);
        for (alias, name) in [
            ("perp.createAPI", "createExchangeAPI"),
            ("spot.GRAFANA_ENDPOINTS_EVERY", "GRAFANA_ENDPOINTS_EVERY"),
            (
                "spot.GRAFANA_ENDPOINTS_PERIODIC",
                "GRAFANA_ENDPOINTS_PERIODIC",
            ),
            ("spot.ENDPOINTS", "ENDPOINTS"),
            ("spot.BASE_URLS", "BASE_URLS"),
        ] {
            let value = realm
                .global_binding(name)
                .map_err(|error| realm.exchange_error(&error))?;
            realm.bind_import(alias, value);
        }
        let class = spot_runtime::spot_exchange_class(&mut realm)
            .map_err(|error| realm.exchange_error(&error))?;
        let options = realm
            .encode_typed(&options)
            .map_err(|error| realm.exchange_error(&error))?;
        let store = store_object(&realm, store).map_err(|error| error.to_exchange_error())?;
        let mut realm = realm.with_lexical(&class.constructor);
        let instance = realm
            .construct(
                &class.constructor,
                &[Js::str("hyperliquidspot"), options, store],
                None,
            )
            .map_err(|error| realm.exchange_error(&error))?;
        *venue.binding.borrow_mut() = Some(Binding { realm, instance });
        Ok(venue)
    }
    fn bound(&self) -> Result<Binding, ExchangeError> {
        self.binding
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("Hyperliquid Spot construction is incomplete"))
    }
    fn launch<T: 'static, F: std::future::Future<Output = Result<T, ExchangeError>> + 'static>(
        &self,
        body: impl FnOnce(Binding) -> F + 'static,
    ) -> ExchangeFuture<'static, T> {
        let binding = self.bound();
        safe_cex_exchange::eager::start_eager(
            &self.base.runtime,
            async move { body(binding?).await },
        )
    }
    fn typed<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &'static str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown> + 'static,
    ) -> ExchangeFuture<'static, T> {
        self.launch(move |binding| async move { binding.typed(name, args).await })
    }
    fn unit(
        &self,
        name: &'static str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown> + 'static,
    ) -> ExchangeFuture<'static, ()> {
        self.launch(move |binding| async move { binding.unit(name, args).await })
    }
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &str,
        first: impl serde::Serialize,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let Binding {
            mut realm,
            instance,
        } = self.bound()?;
        let listener = realm.native("callback", 1, move |realm, _, args| {
            let value = args.first().cloned().unwrap_or(Js::Undefined);
            callback(realm.to_typed::<T>(&value)?);
            Ok(Js::Undefined)
        });
        let first = realm
            .encode_typed(&first)
            .map_err(|error| realm.exchange_error(&error))?;
        let cleanup = realm
            .call_method(&instance, name, &[first, listener])
            .map_err(|error| realm.exchange_error(&error))?;
        Ok(Box::new(move || {
            realm
                .call(&cleanup, &Js::Undefined, &[])
                .map(|_| ())
                .map_err(|error| realm.exchange_error(&error))
        }))
    }
    fn order_ids(
        &self,
        name: &'static str,
        options: impl serde::Serialize + 'static,
    ) -> ExchangeFuture<'static, Vec<String>> {
        self.launch(move |binding| Self::order_ids_bound(binding, name, options))
    }
    async fn order_ids_bound(
        binding: Binding,
        name: &str,
        options: impl serde::Serialize,
    ) -> Result<Vec<String>, ExchangeError> {
        let (mut realm, ids) = binding
            .invoke(name, |realm| Ok(vec![realm.encode_typed(&options)?]))
            .await?;
        // Source omitUndefined is lodash/omitBy: its array input produces an
        // object whose enumerable numeric keys retain the original indices.
        // Native Exchange returns Vec<String>; lower those values in own-key
        // order, without changing the source realm's object or filling holes.
        let mut result = vec![];
        for (key, value) in realm
            .own_entries(&ids)
            .map_err(|e| realm.exchange_error(&e))?
        {
            let key = key
                .as_str()
                .ok_or_else(|| ExchangeError::new("Spot order ID key must be a numeric index"))?;
            let key = String::from_utf16(&key)
                .map_err(|_| ExchangeError::new("Spot order ID key must be a numeric index"))?;
            let index = key
                .parse::<u32>()
                .map_err(|_| ExchangeError::new("Spot order ID key must be a numeric index"))?;
            if index == u32::MAX || index.to_string() != key {
                return Err(ExchangeError::new(
                    "Spot order ID key must be a numeric index",
                ));
            }
            result.push(match value {
                Js::Number(value) => safe_cex_num::js_number_string(value),
                Js::String(units) => String::from_utf16(&units).map_err(|_| {
                    ExchangeError::new("Spot order ID exceeds the native Unicode boundary")
                })?,
                _ => {
                    return Err(ExchangeError::new(
                        "Spot order ID must be a string or number",
                    ));
                }
            });
        }
        Ok(result)
    }
}
impl Exchange for HyperliquidSpotExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        let mut binding = self.bound()?;
        binding
            .realm
            .call_method(&binding.instance, "dispose", &[])
            .map(|_| ())
            .map_err(|error| binding.realm.exchange_error(&error))
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", options, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", symbol, callback)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(self.typed("fetchOHLCV", move |r| Ok(vec![r.encode_typed(&options)?])))
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("start", |_| Ok(vec![])))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let checked = self.base.resolve_caller_client_order_id(order.options());
        self.launch(move |binding| async move {
            checked?;
            Self::order_ids_bound(binding, "placeOrder", order.options()).await
        })
    }
    fn place_orders(&self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        self.launch(move |binding| async move {
            let mut result = vec![];
            for order in orders.orders() {
                result.extend(
                    Self::order_ids_bound(binding.clone(), "placeOrder", order.options()).await?,
                );
            }
            Ok(result)
        })
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(self.typed("validateAccount", |_| Ok(vec![])))
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(self.typed("fetchPositions", move |r| Ok(vec![opt(r, &symbols)?])))
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(self.typed("fetchOrders", move |r| Ok(vec![opt(r, &symbols)?])))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("setLeverage", move |r| {
            Ok(vec![Js::str(&symbol), Js::Number(leverage), opt(r, &side)?])
        }))
    }
    fn change_position_mode(&self, _: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("changeMarginType", move |r| {
            Ok(vec![Js::Bool(isolated), opt(r, &symbol)?])
        }))
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(self.order_ids("updateOrder", options))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("cancelOrders", move |r| Ok(vec![r.encode_typed(&orders)?])))
    }
    fn schedule_cancel(&self, _: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn fetch_public_trades(
        &self,
        _: String,
        _: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        base_exchange_rejection(&self.base.runtime)
    }
}
