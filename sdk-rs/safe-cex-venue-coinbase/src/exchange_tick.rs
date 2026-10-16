//! Polling field body on native retained ticker records and imported read/store
//! capabilities. Registry and arrays are captured exactly at the source call sites.
use crate::{
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use indexmap::IndexMap;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
pub type TickAsync<T> = Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>;
pub type TickUpdate = IndexMap<String, Frame>;
pub type TickCallback = Rc<dyn Fn(Frame) -> Result<(), PortfolioFailure>>;
pub type TickHandlers = Rc<RefCell<Vec<TickCallback>>>;
pub type TickRegistry = Rc<RefCell<IndexMap<Vec<u16>, TickHandlers>>>;
pub type TickLoopCallback = Rc<dyn Fn() -> Promise<Result<(), PortfolioFailure>>>;
pub trait TickHost {
    fn is_disposed(&self) -> bool;
    fn fetch_tickers(&self) -> TickAsync<Frame>;
    fn fetch_balance(&self) -> TickAsync<Frame>;
    fn fetch_positions(&self) -> TickAsync<Frame>;
    fn fetch_open_orders_snapshot(&self) -> TickAsync<Frame>;
    fn loaded(&self) -> Result<Frame, PortfolioFailure>;
    fn update(&self, fields: TickUpdate) -> Result<(), PortfolioFailure>;
    fn public_message_handlers(&self) -> Result<TickRegistry, PortfolioFailure>;
    fn should_poll_orders(&self) -> Result<bool, PortfolioFailure>;
    fn merge_rest_orders_with_fresh_ws_state(
        &self,
        orders: Frame,
    ) -> Result<Frame, PortfolioFailure>;
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure>;
    fn emit_and_log_error(
        &self,
        error: PortfolioFailure,
        message: Option<Value>,
    ) -> Result<(), PortfolioFailure>;
    fn get_tick_interval(&self) -> Result<f64, PortfolioFailure>;
    fn loop_again(&self, callback: TickLoopCallback, delay: f64) -> TickAsync<()>;
}
pub struct TickReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
fn type_error(message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":"TypeError","message":message}),
    )))
}
fn loaded(host: &impl TickHost, key: &str) -> Result<Frame, PortfolioFailure> {
    let old = host.loaded()?;
    let mut map = IndexMap::new();
    if let Frame::Object(values) = &old {
        for k in old.own_keys() {
            map.insert(k.clone(), values.borrow()[&k].clone());
        }
    }
    map.insert(key.encode_utf16().collect(), Frame::Bool(true));
    Ok(Frame::Object(Rc::new(RefCell::new(map))))
}
fn publish(host: &impl TickHost, key: &str, value: Frame) -> Result<(), PortfolioFailure> {
    let loaded = loaded(host, key)?;
    host.update(IndexMap::from([
        (key.into(), value),
        ("loaded".into(), loaded),
    ]))
}
fn callbacks(registry: &TickRegistry, key: &[u16], ticker: Frame) -> Result<(), PortfolioFailure> {
    let handlers = registry.borrow().get(key).cloned();
    if let Some(handlers) = handlers {
        let len = handlers.borrow().len();
        for i in 0..len {
            let cb = handlers.borrow().get(i).cloned();
            if let Some(cb) = cb {
                cb(ticker.clone())?;
            }
        }
    }
    Ok(())
}
fn topic(ticker: &Frame, prefix: &str) -> Result<Vec<u16>, PortfolioFailure> {
    if matches!(ticker, Frame::Null) {
        return Err(type_error(
            "Cannot read properties of null (reading 'symbol')",
        ));
    }
    let text =
        crate::public_data::string(ticker.get("symbol")).map_err(|e| type_error(e.message))?;
    let mut key = prefix.encode_utf16().collect::<Vec<_>>();
    key.extend(text);
    Ok(key)
}
pub fn tick<H: TickHost + 'static>(
    r: Rc<TickReceiver<H>>,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            if r.host.is_disposed() {
                return Ok(());
            }
            let result = async {
                let tickers = r.host.fetch_tickers()?.await?;
                if r.host.is_disposed() {
                    return Ok(true);
                }
                publish(&r.host, "tickers", tickers.clone())?;
                let handlers = r.host.public_message_handlers()?;
                let Frame::Array(tickers) = tickers else {
                    return Err(type_error("tickers.forEach is not a function"));
                };
                let len = tickers.borrow().len();
                for i in 0..len {
                    let Some(ticker) = tickers.borrow().get(i).cloned() else {
                        continue;
                    };
                    let key = topic(&ticker, "poll_book.")?;
                    callbacks(&handlers, &key, ticker.clone())?;
                    let key = topic(&ticker, "poll_kline.")?;
                    callbacks(&handlers, &key, ticker)?;
                }
                let balance = r.host.fetch_balance()?.await?;
                if r.host.is_disposed() {
                    return Ok(true);
                }
                publish(&r.host, "balance", balance)?;
                let positions = r.host.fetch_positions()?.await?;
                if r.host.is_disposed() {
                    return Ok(true);
                }
                publish(&r.host, "positions", positions)?;
                if r.host.should_poll_orders()? {
                    let result = async {
                        let orders = r.host.fetch_open_orders_snapshot()?.await?;
                        if r.host.is_disposed() {
                            return Ok(true);
                        }
                        let orders = r.host.merge_rest_orders_with_fresh_ws_state(orders)?;
                        publish(&r.host, "orders", orders)?;
                        Ok::<_, PortfolioFailure>(false)
                    }
                    .await;
                    match result {
                        Ok(true) => return Ok(true),
                        Ok(false) => (),
                        Err(e) => r.host.emit_orders_fetch_error(e)?,
                    }
                }
                Ok::<_, PortfolioFailure>(false)
            }
            .await;
            if matches!(&result, Ok(true)) {
                return Ok(());
            }
            if let Err(e) = result {
                let message = e.value.as_ref().and_then(|v| v.get("message")).cloned();
                r.host.emit_and_log_error(e, message)?;
            }
            if !r.host.is_disposed() {
                let next = r.clone();
                let callback: TickLoopCallback = Rc::new(move || tick(next.clone()));
                let delay = r.host.get_tick_interval()?;
                let _detached_loop = r.host.loop_again(callback, delay)?;
            }
            Ok(())
        }),
        &spawn,
    )
}
