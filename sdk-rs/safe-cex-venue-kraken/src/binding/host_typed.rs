//! Exchange-level typed hosts: startup, the tick loop, disposal and Spot cancellation.
use super::{
    promise::{Promise, all, await_js},
    rt::Rt,
    value::{Js, Thrown},
};
use crate::{
    lifecycle::DisposalHost,
    periodic_sync::{TickCallback, TickHost, TickUpdate},
    spot_cancel::SpotCancelHost,
    startup::{SocketKind, StartupHost, StartupSocket, StartupState, StartupUpdate},
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::StoreDataLoaded;
use std::rc::Rc;

fn ex(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

/// `await this.member(...args)` as an owned future.
pub(crate) fn call_future(
    rt: &Rt,
    member: &str,
    args: Vec<Js>,
) -> Result<ExchangeFuture<'static, Js>, ExchangeError> {
    let promise = rt.this.call_method(member, args).map_err(ex)?;
    { let pending = await_js(&rt.exchange_runtime(), promise); Ok(Box::pin(async move { pending.await.map_err(ex) })) }
}

fn store(rt: &Rt) -> Result<Js, ExchangeError> {
    rt.this.get("store").map_err(ex)
}

fn loaded(rt: &Rt) -> StoreDataLoaded {
    rt.shared.store.loaded().clone()
}

fn update(rt: &Rt, fields: Vec<(&str, Js)>) -> Result<(), ExchangeError> {
    store(rt)?
        .call_method("update", vec![Js::object(fields)])
        .map_err(ex)?;
    Ok(())
}

fn loaded_js(flags: &StoreDataLoaded) -> Result<Js, ExchangeError> {
    super::value::from_typed(flags).map_err(ex)
}

impl StartupSocket for Js {
    fn connect_and_subscribe(&self) -> Result<(), ExchangeError> {
        self.call_method("connectAndSubscribe", vec![])
            .map_err(ex)?;
        Ok(())
    }
}

impl StartupHost for Rt {
    type Socket = Js;
    type Markets = Js;
    type Tickers = Js;
    /// The flags are refreshed from the live receiver on every read, as the source
    /// re-reads `this.isDisposed` / `this.isPublicDataOnly` after each await.
    fn state(&self) -> &StartupState<Js> {
        let state = &self.shared.extra.startup;
        state.disposed.set(self.base().is_disposed());
        state
            .public_data_only
            .set(self.this.get("isPublicDataOnly").is_ok_and(|v| v.truthy()));
        state
    }
    fn init_external_public_data(&self) -> Result<ExchangeFuture<'static, bool>, ExchangeError> {
        let base = self.base().clone();
        let owner = base.clone();
        let factory = self.shared.extra.external_public_data.clone();
        Ok(Box::pin(async move {
            base.init_external_public_data(|options| match &factory {
                Some(create) => create(options, owner),
                None => unreachable!("construction refuses externalPublicData without a provider"),
            })
            .await
        }))
    }
    fn fetch_markets(&self) -> Result<ExchangeFuture<'static, Rc<Js>>, ExchangeError> {
        let f = call_future(self, "fetchMarkets", vec![])?;
        Ok(Box::pin(async move { f.await.map(Rc::new) }))
    }
    fn fetch_tickers(&self) -> Result<ExchangeFuture<'static, Rc<Js>>, ExchangeError> {
        let f = call_future(self, "fetchTickers", vec![])?;
        Ok(Box::pin(async move { f.await.map(Rc::new) }))
    }
    fn loaded(&self) -> StoreDataLoaded {
        loaded(self)
    }
    fn update(&self, update: StartupUpdate<Js, Js>) -> Result<(), ExchangeError> {
        match update {
            StartupUpdate::Markets(markets, flags) => update_fields(
                self,
                vec![
                    ("markets", (*markets).clone()),
                    ("loaded", loaded_js(&flags)?),
                ],
            ),
            StartupUpdate::Tickers(tickers, flags) => update_fields(
                self,
                vec![
                    ("tickers", (*tickers).clone()),
                    ("loaded", loaded_js(&flags)?),
                ],
            ),
            StartupUpdate::Loaded(flags) => {
                update_fields(self, vec![("loaded", loaded_js(&flags)?)])
            }
        }
    }
    fn create_socket(&self, kind: SocketKind) -> Result<Rc<Js>, ExchangeError> {
        let (class, slot) = match kind {
            SocketKind::FuturesPublic => ("KrakenPublicWebsocket", "publicWebsocket"),
            SocketKind::FuturesPrivate => ("KrakenPrivateWebsocket", "privateWebsocket"),
            SocketKind::SpotPublic => ("KrakenSpotPublicWebsocket", "publicWebsocket"),
            SocketKind::SpotPrivate => ("KrakenSpotPrivateWebsocket", "privateWebsocket"),
        };
        let socket = self
            .global(class)
            .and_then(|c| c.construct(vec![self.this.clone()], None))
            .map_err(ex)?;
        self.this.set(slot, socket.clone()).map_err(ex)?;
        Ok(Rc::new(socket))
    }
    // ts: `this.tick(0)` — not awaited.
    fn start_tick(&self, count: f64) -> Result<(), ExchangeError> {
        self.this
            .call_method("tick", vec![Js::Number(count)])
            .map_err(ex)?;
        Ok(())
    }
}

fn update_fields(rt: &Rt, fields: Vec<(&str, Js)>) -> Result<(), ExchangeError> {
    update(rt, fields)
}

impl TickHost for Rt {
    type Orders = Js;
    type Positions = Js;
    type Balance = Js;
    type Task = Js;
    fn is_disposed(&self) -> bool {
        self.base().is_disposed()
    }
    fn fetch_leverage_settings(&self) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let f = call_future(self, "fetchLeverageSettings", vec![])?;
        Ok(Box::pin(async move { f.await.map(|_| ()) }))
    }
    fn fetch_orders(&self) -> Result<ExchangeFuture<'static, Rc<Js>>, ExchangeError> {
        let f = call_future(self, "fetchOrders", vec![])?;
        Ok(Box::pin(async move { f.await.map(Rc::new) }))
    }
    fn fetch_positions(&self) -> Result<ExchangeFuture<'static, Rc<Js>>, ExchangeError> {
        let f = call_future(self, "fetchPositions", vec![])?;
        Ok(Box::pin(async move { f.await.map(Rc::new) }))
    }
    fn fetch_balance(&self) -> Result<ExchangeFuture<'static, Rc<Js>>, ExchangeError> {
        let f = call_future(self, "fetchBalance", vec![])?;
        Ok(Box::pin(async move { f.await.map(Rc::new) }))
    }
    fn balance_truthy(&self, balance: &Js) -> bool {
        balance.truthy()
    }
    fn loaded(&self) -> StoreDataLoaded {
        loaded(self)
    }
    fn update(&self, update: TickUpdate<Js, Js, Js>) -> Result<(), ExchangeError> {
        let store = store(self)?;
        match update {
            TickUpdate::Orders(orders) => {
                store
                    .call_method("addOrUpdateOrdersFull", vec![(*orders).clone()])
                    .map_err(ex)?;
            }
            TickUpdate::Positions(positions) => {
                store
                    .call_method("addOrUpdatePositionsFull", vec![(*positions).clone()])
                    .map_err(ex)?;
            }
            TickUpdate::Balance(balance, flags) => update_fields(
                self,
                vec![
                    ("balance", (*balance).clone()),
                    ("loaded", loaded_js(&flags)?),
                ],
            )?,
            TickUpdate::Loaded(flags) => update_fields(self, vec![("loaded", loaded_js(&flags)?)])?,
        }
        Ok(())
    }
    // ts: `promise.then(...)` — the continuation is started now and owned by the runtime.
    fn start_then(&self, task: ExchangeFuture<'static, ()>) -> Result<Js, ExchangeError> {
        Ok(Js::Promise(Promise::spawn(
            &self.exchange_runtime(),
            Box::pin(async move {
                task.await.map_err(Thrown::from)?;
                Ok(Js::Undefined)
            }),
        )))
    }
    fn promise_all(&self, tasks: Vec<Js>) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let promise = all(&self.exchange_runtime(), tasks);
        Ok(Box::pin(async move {
            promise.wait().await.map_err(ex)?;
            Ok(())
        }))
    }
    // ts: `this.emitAndLogError(err?.message || fallback)`.
    fn emit_tick_error(
        &self,
        error: ExchangeError,
        fallback: &'static str,
    ) -> Result<(), ExchangeError> {
        let message = if error.message.is_empty() {
            fallback.to_owned()
        } else {
            error.message
        };
        self.this
            .call_method("emitAndLogError", vec![Js::str(&message)])
            .map_err(ex)?;
        Ok(())
    }
    fn current_tick(&self, count: f64) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let f = call_future(self, "tick", vec![Js::Number(count)])?;
        Ok(Box::pin(async move { f.await.map(|_| ()) }))
    }
    // ts: utils/loop `loop(fn, delay)` — a setTimeout whose async callback is not awaited.
    fn schedule_loop(&self, callback: TickCallback, delay: f64) -> Result<(), ExchangeError> {
        let rt = self.clone();
        let fire: Rc<dyn Fn() -> Result<(), Thrown>> = Rc::new(move || {
            let future = callback().map_err(Thrown::from)?;
            let reporter = rt.clone();
            drop(Promise::spawn(
                &rt.exchange_runtime(),
                Box::pin(async move {
                    if let Err(error) = future.await {
                        reporter.report(&Thrown::from(error), "kraken tick");
                    }
                    Ok(Js::Undefined)
                }),
            ));
            Ok(())
        });
        self.set_timeout(fire, delay);
        Ok(())
    }
}

pub(crate) struct MapValues {
    iterator: Js,
    next: Js,
    done: bool,
}
impl Iterator for MapValues {
    type Item = Result<Rc<Js>, ExchangeError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let result = (|| {
            let step = self.next.call(self.iterator.clone(), vec![])?;
            if !step.is_object() {
                return Err(Thrown::type_error("Iterator result is not an object"));
            }
            if step.get("done")?.truthy() {
                Ok(None)
            } else {
                step.get("value").map(|value| Some(Rc::new(value)))
            }
        })();
        match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(error) => {
                self.done = true;
                Some(Err(ex(error)))
            }
        }
    }
}
fn map_values(map: &Js, method: &str) -> Result<MapValues, ExchangeError> {
    let iterator = map.call_method(method, vec![]).map_err(ex)?;
    let next = iterator.get("next").map_err(ex)?;
    Ok(MapValues {
        iterator,
        next,
        done: false,
    })
}

impl DisposalHost for Rt {
    type Timeout = Js;
    type Key = Js;
    type Timeouts = MapValues;
    type Keys = MapValues;
    // ts: `for (const [, timeoutId] of this.ohlcvPollingTimeouts)`.
    fn polling_timeouts(&self) -> Result<Self::Timeouts, ExchangeError> {
        let map = self.this.get("ohlcvPollingTimeouts").map_err(ex)?;
        map_values(&map, "values")
    }
    fn clear_timeout(&self, timeout: &Rc<Js>) -> Result<(), ExchangeError> {
        Rt::clear_timeout(self, timeout);
        Ok(())
    }
    fn active_keys(&self) -> Result<Self::Keys, ExchangeError> {
        let map = self.this.get("ohlcvPollingActive").map_err(ex)?;
        map_values(&map, "keys")
    }
    fn set_inactive(&self, key: &Rc<Js>) -> Result<(), ExchangeError> {
        self.this
            .get("ohlcvPollingActive")
            .and_then(|m| m.call_method("set", vec![(**key).clone(), Js::Bool(false)]))
            .map_err(ex)?;
        Ok(())
    }
    fn clear_timeouts(&self) -> Result<(), ExchangeError> {
        clear(self, "ohlcvPollingTimeouts")
    }
    fn clear_active(&self) -> Result<(), ExchangeError> {
        clear(self, "ohlcvPollingActive")
    }
    fn clear_candles(&self) -> Result<(), ExchangeError> {
        clear(self, "lastCandleCache")
    }
    // ts: `super.dispose()`.
    fn dispose_base(&self) -> Result<(), ExchangeError> {
        self.base().dispose()
    }
}

fn clear(rt: &Rt, map: &str) -> Result<(), ExchangeError> {
    rt.this
        .get(map)
        .and_then(|m| m.call_method("clear", vec![]))
        .map_err(ex)?;
    Ok(())
}

impl SpotCancelHost for Rt {
    type Orders = Js;
    type Order = Js;
    type Id = Js;
    type Items = std::vec::IntoIter<Result<Rc<Js>, ExchangeError>>;
    fn length(&self, orders: &Js) -> Result<usize, ExchangeError> {
        Ok(orders
            .get("length")
            .and_then(|l| l.to_number())
            .map_err(ex)? as usize)
    }
    fn first(&self, orders: &Js) -> Result<Rc<Js>, ExchangeError> {
        Ok(Rc::new(orders.get("0").map_err(ex)?))
    }
    fn items(&self, orders: Rc<Js>) -> Result<Self::Items, ExchangeError> {
        let items = orders
            .array_values()
            .ok_or_else(|| ExchangeError::new("orders is not iterable"))?;
        Ok(items
            .into_iter()
            .map(|o| Ok(Rc::new(o)))
            .collect::<Vec<_>>()
            .into_iter())
    }
    fn id(&self, order: &Js) -> Result<Rc<Js>, ExchangeError> {
        Ok(Rc::new(order.get("id").map_err(ex)?))
    }
    fn post_cancel(
        &self,
        endpoint: &'static str,
        field: &'static str,
        id: Rc<Js>,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let xhr = self.this.get("unlimitedXHR").map_err(ex)?;
        let promise = xhr
            .call_method(
                "post",
                vec![Js::str(endpoint), Js::object(vec![(field, (*id).clone())])],
            )
            .map_err(ex)?;
        Ok(Box::pin(async move {
            await_js(&self.exchange_runtime(), promise).await.map_err(ex)?;
            Ok(())
        }))
    }
    fn error_message(
        &self,
        error: &ExchangeError,
        fallback: &'static str,
    ) -> Result<String, ExchangeError> {
        Ok(if error.message.is_empty() {
            fallback.to_owned()
        } else {
            error.message.clone()
        })
    }
    fn emit_error(&self, message: &str, context: &'static str) -> Result<(), ExchangeError> {
        self.this
            .call_method("emitAndLogError", vec![Js::str(message), Js::str(context)])
            .map_err(ex)?;
        Ok(())
    }
}

#[cfg(test)]
mod collection_iterator_tests {
    use super::*;
    #[test]
    fn disposal_map_values_keep_live_iterator_updates() {
        let map = Js::new_map();
        map.call_method("set", vec![Js::str("first"), Js::Number(11.)])
            .unwrap();
        map.call_method("set", vec![Js::str("second"), Js::Number(22.)])
            .unwrap();
        let mut values = map_values(&map, "values").unwrap();
        assert_eq!(values.next().unwrap().unwrap().as_f64(), Some(11.));
        map.call_method("delete", vec![Js::str("second")]).unwrap();
        map.call_method("set", vec![Js::str("late"), Js::Number(33.)])
            .unwrap();
        assert_eq!(values.next().unwrap().unwrap().as_f64(), Some(33.));
        assert!(values.next().is_none());
        map.call_method("set", vec![Js::str("after-done"), Js::Number(44.)])
            .unwrap();
        assert!(values.next().is_none());
    }
}
