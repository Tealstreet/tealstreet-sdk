use super::{
    error::{exchange_error, failure, from_thrown},
    reads::balance_fields,
    rt::Host,
    value::{self, to_js},
};
use crate::{
    exchange_lifecycle::*,
    exchange_tick::*,
    market_position_reads,
    portfolios_balances::{self, Portfolio, PortfolioFailure},
    public_market_reads,
    public_stream_dispatch::{PublicStreamHandler, PublicStreamMessage},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{
    js_runtime::{
        promise::{Promise as JsPromise, await_js},
        value::{Js, Thrown, from_typed, to_json},
    },
    runtime::ExchangeError,
};
use serde::Serialize;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

pub(super) fn frame<T: Serialize>(value: &T) -> Result<Frame, PortfolioFailure> {
    value::from_js(&from_typed(value).map_err(from_thrown)?)?
        .ok_or_else(|| Host::json_error("TypeError", "Undefined Coinbase native result"))
}
impl Host {
    pub(super) fn async_js<T: Clone + 'static>(
        &self,
        invoked: Result<Js, PortfolioFailure>,
        project: impl FnOnce(Js) -> Result<T, PortfolioFailure> + 'static,
    ) -> Promise<Result<T, PortfolioFailure>> {
        let runtime = self.runtime();
        self.promise(
            async move { project(await_js(&runtime, invoked?).await.map_err(from_thrown)?) },
        )
    }
    pub(super) fn loaded(&self) -> Result<Frame, PortfolioFailure> {
        frame(&*self.shared.store.loaded())
    }
    pub(super) fn publish(&self, fields: LifecycleUpdate) -> Result<(), PortfolioFailure> {
        let fields = Js::object(
            fields
                .iter()
                .map(|(key, value)| (key.as_str(), to_js(value)))
                .collect(),
        );
        self.receiver()
            .get("store")
            .and_then(|store| store.call_method("update", vec![fields]))
            .map_err(from_thrown)?;
        Ok(())
    }
    pub(super) fn tick(&self) -> Promise<Result<(), PortfolioFailure>> {
        crate::exchange_tick::tick(Rc::new(TickReceiver {
            host: self.clone(),
            spawn: self.shared.extra.spawn.clone(),
        }))
    }
    pub(super) fn market_values(
        &self,
    ) -> Promise<Result<Vec<safe_cex_types::Market>, PortfolioFailure>> {
        let result = public_market_reads::fetch_markets(self.public_read_receiver());
        self.promise(async move {
            Ok(result
                .await?
                .borrow()
                .iter()
                .map(|market| {
                    let market = market.borrow();
                    let mut fields = market.fields.clone();
                    fields.info = Some(market.info.borrow().clone());
                    fields
                })
                .collect())
        })
    }
    pub(super) fn ticker_values(
        &self,
        intx: bool,
    ) -> Result<Promise<Result<Vec<safe_cex_types::Ticker>, PortfolioFailure>>, PortfolioFailure>
    {
        let receiver = self.class()?.market_receiver(self.clone());
        let result = if intx {
            market_position_reads::fetch_tickers_intx(receiver)
        } else {
            market_position_reads::fetch_tickers(receiver)
        };
        Ok(self.promise(async move {
            Ok(result
                .await?
                .borrow()
                .iter()
                .map(|ticker| {
                    let mut fields = ticker.fields.clone();
                    if let Some(info) = &ticker.info {
                        fields.info = Some(info.borrow().clone());
                    }
                    fields
                })
                .collect())
        }))
    }
    pub(super) fn position_values(
        &self,
    ) -> Result<Promise<Result<Vec<safe_cex_types::Position>, PortfolioFailure>>, PortfolioFailure>
    {
        let result =
            market_position_reads::fetch_positions(self.class()?.market_receiver(self.clone()));
        Ok(self.promise(async move {
            Ok(result
                .await?
                .borrow()
                .iter()
                .map(|position| {
                    let mut fields = position.fields.clone();
                    if let Some(info) = &position.info {
                        fields.info = (Some(info.borrow().clone())).into();
                    }
                    fields
                })
                .collect())
        }))
    }
    pub(super) fn balance_value(
        &self,
    ) -> Result<Promise<Result<safe_cex_types::Balance, PortfolioFailure>>, PortfolioFailure> {
        let result = portfolios_balances::fetch_balance(
            self.class()?.portfolio_receiver(self.clone(), false),
        );
        Ok(self.promise(async move { Ok(balance_fields(&result.await?)) }))
    }
    pub(super) fn snapshot_values(
        &self,
    ) -> Promise<Result<Vec<safe_cex_types::Order>, PortfolioFailure>> {
        let result = crate::order_sync::fetch_open_orders_snapshot(self.order_snapshot_receiver());
        self.promise(async move { Ok(Self::native_orders(&result.await?)) })
    }
    fn socket(&self, private: bool) -> Result<Js, PortfolioFailure> {
        (if private {
            &self.shared.extra.private_socket
        } else {
            &self.shared.extra.public_socket
        })
        .get()
        .cloned()
        .ok_or_else(|| Host::json_error("Error", "Coinbase socket is not assembled"))
    }
}
impl LifecycleHost for Host {
    fn is_disposed(&self) -> bool {
        self.base().is_disposed()
    }
    fn public_only(&self) -> bool {
        self.public_only()
    }
    fn init_external_public_data(&self) -> LifecycleAsync<bool> {
        Ok(self.async_js(
            self.receiver()
                .call_method("initExternalPublicData", vec![])
                .map_err(from_thrown),
            |value| Ok(value.truthy()),
        ))
    }
    fn find_selected_portfolio(&self) -> LifecycleAsync<Portfolio> {
        Ok(self.class()?.find_selected())
    }
    fn fetch_markets(&self) -> LifecycleAsync<Frame> {
        let result = self.market_values();
        Ok(self.promise(async move { frame(&result.await?) }))
    }
    fn fetch_tickers_intx(&self) -> LifecycleAsync<Frame> {
        let result = self.ticker_values(true)?;
        Ok(self.promise(async move { frame(&result.await?) }))
    }
    fn fetch_balance(&self) -> LifecycleAsync<Frame> {
        let result = self.balance_value()?;
        Ok(self.promise(async move { frame(&result.await?) }))
    }
    fn fetch_open_orders_snapshot(&self) -> LifecycleAsync<Frame> {
        let result = self.snapshot_values();
        Ok(self.promise(async move { frame(&result.await?) }))
    }
    fn tick(&self) -> LifecycleAsync<()> {
        Ok(self.tick())
    }
    fn get_markets(&self, endpoint: &'static str) -> LifecycleAsync<()> {
        let result = self.request("get", endpoint, None);
        Ok(self.promise(async move { result.await.map(|_| ()) }))
    }
    fn loaded(&self) -> Frame {
        self.loaded().expect("typed loaded state")
    }
    fn update(&self, fields: LifecycleUpdate) -> LifecycleAsync<()> {
        let fields = Js::object(
            fields
                .iter()
                .map(|(key, value)| (key.as_str(), to_js(value)))
                .collect(),
        );
        let result = self
            .receiver()
            .get("store")
            .and_then(|store| store.call_method("update", vec![fields]))
            .map_err(from_thrown);
        Ok(self.async_js(result, |_| Ok(())))
    }
    fn connect_public(&self) -> Result<(), PortfolioFailure> {
        self.socket(false)?
            .call_method("connectAndSubscribe", vec![])
            .map_err(from_thrown)?;
        Ok(())
    }
    fn connect_private(&self) -> Result<(), PortfolioFailure> {
        self.socket(true)?
            .call_method("connectAndSubscribe", vec![])
            .map_err(from_thrown)?;
        Ok(())
    }
    fn dispose_base(&self) -> Result<(), PortfolioFailure> {
        self.base().dispose().map_err(failure)
    }
    fn dispose_public(&self) -> Result<(), PortfolioFailure> {
        self.socket(false)?
            .call_method("dispose", vec![])
            .map_err(from_thrown)?;
        Ok(())
    }
    fn dispose_private(&self) -> Result<(), PortfolioFailure> {
        self.socket(true)?
            .call_method("dispose", vec![])
            .map_err(from_thrown)?;
        Ok(())
    }
    fn clear_timeout(&self, timer: LifecycleTimer) -> Result<(), PortfolioFailure> {
        self.global(
            "clearTimeout",
            vec![safe_cex_exchange::js_runtime::value::from_json(
                &timer.borrow(),
            )],
        )?;
        Ok(())
    }
    fn set_timeout(
        &self,
        callback: LifecycleCallback,
        delay_ms: f64,
    ) -> Result<LifecycleTimer, PortfolioFailure> {
        let callback = Js::function("coinbaseTimer", move |_, _| {
            callback().map_err(|error| Thrown::from(exchange_error(&error)))?;
            Ok(Js::Undefined)
        });
        let timer = self.global("setTimeout", vec![callback, Js::Number(delay_ms)])?;
        Ok(Rc::new(RefCell::new(to_json(&timer).map_err(from_thrown)?)))
    }
    fn last_private_user_stream_message_at(&self) -> f64 {
        self.class()
            .map(|class| class.last_private_user_stream_message_at.get())
            .unwrap_or(0.0)
    }
    fn mark_private_user_stream_degraded(
        &self,
        reason: &'static str,
        force_toast: bool,
    ) -> Result<(), PortfolioFailure> {
        self.shared.extra.health.mark_private_user_stream_degraded(
            self.shared.store.as_ref(),
            Some(reason),
            force_toast,
            |message, severity| self.base().log(message, severity).map_err(failure),
            |message| {
                self.base()
                    .emit_args(
                        "warning",
                        &[
                            safe_cex_exchange::js_runtime::value::Js::str(message),
                            safe_cex_exchange::js_runtime::value::Js::object(vec![(
                                "forceToast",
                                safe_cex_exchange::js_runtime::value::Js::Bool(true),
                            )]),
                        ],
                    )
                    .map_err(failure)
            },
        )
    }
    fn merge_rest_orders_with_fresh_ws_state(
        &self,
        orders: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        self.merge_rest(orders)
    }
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure> {
        self.emit_orders_error(error)
    }
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure> {
        self.emit(message)
    }
    fn log(&self, message: &str, severity: Option<&str>) -> Result<(), PortfolioFailure> {
        self.base()
            .log(message, severity.unwrap_or("info"))
            .map_err(failure)
    }
}
impl TickHost for Host {
    fn is_disposed(&self) -> bool {
        self.base().is_disposed()
    }
    fn fetch_tickers(&self) -> TickAsync<Frame> {
        let result = self.ticker_values(false)?;
        Ok(self.promise(async move { frame(&result.await?) }))
    }
    fn fetch_balance(&self) -> TickAsync<Frame> {
        LifecycleHost::fetch_balance(self)
    }
    fn fetch_positions(&self) -> TickAsync<Frame> {
        let result = self.position_values()?;
        Ok(self.promise(async move { frame(&result.await?) }))
    }
    fn fetch_open_orders_snapshot(&self) -> TickAsync<Frame> {
        LifecycleHost::fetch_open_orders_snapshot(self)
    }
    fn loaded(&self) -> Result<Frame, PortfolioFailure> {
        self.loaded()
    }
    fn update(&self, fields: TickUpdate) -> Result<(), PortfolioFailure> {
        self.publish(fields)
    }
    fn public_message_handlers(&self) -> Result<TickRegistry, PortfolioFailure> {
        let class = self.class()?;
        let public = class
            .public_websocket
            .borrow()
            .clone()
            .ok_or_else(|| Host::json_error("Error", "Public socket not assembled"))?;
        let registry = public
            .receiver
            .message_handlers
            .borrow()
            .iter()
            .map(|(key, handlers)| {
                let callbacks: Vec<TickCallback> = handlers
                    .borrow()
                    .iter()
                    .filter_map(|handler| match handler {
                        PublicStreamHandler::Callback(callback) => {
                            let callback = callback.clone();
                            Some(Rc::new(move |frame| {
                                callback(PublicStreamMessage::Frame(frame))
                                    .map_err(|error| Host::json_error(&error.name, &error.message))
                            }) as TickCallback)
                        }
                        _ => None,
                    })
                    .collect();
                (
                    key.encode_utf16().collect(),
                    Rc::new(RefCell::new(callbacks)),
                )
            })
            .collect();
        Ok(Rc::new(RefCell::new(registry)))
    }
    fn should_poll_orders(&self) -> Result<bool, PortfolioFailure> {
        self.should_poll()
    }
    fn merge_rest_orders_with_fresh_ws_state(
        &self,
        orders: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        self.merge_rest(orders)
    }
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure> {
        self.emit_orders_error(error)
    }
    fn emit_and_log_error(
        &self,
        error: PortfolioFailure,
        message: Option<Value>,
    ) -> Result<(), PortfolioFailure> {
        self.emit_args(&[error.value.clone(), message])
    }
    fn get_tick_interval(&self) -> Result<f64, PortfolioFailure> {
        self.base()
            .get_tick_interval()
            .ok_or_else(|| failure(ExchangeError::new("Coinbase tick interval unavailable")))
    }
    fn loop_again(&self, callback: TickLoopCallback, delay: f64) -> TickAsync<()> {
        let runtime = self.runtime();
        let callback = Js::function("coinbaseTick", move |_, _| {
            let result = callback();
            Ok(Js::Promise(JsPromise::spawn(
                &runtime,
                Box::pin(async move {
                    result
                        .await
                        .map_err(|error| Thrown::from(exchange_error(&error)))?;
                    Ok(Js::Undefined)
                }),
            )))
        });
        Ok(self.async_js(
            self.global("loop", vec![callback, Js::Number(delay)]),
            |_| Ok(()),
        ))
    }
}
