//! Runs the crate's host-generic MEXC bodies (start, placement, leverage, mode,
//! margin, update, cancels, tick, dispose) on the shared js_runtime. Their `this`
//! is a facade whose members and state are the native `MexcExchange` owners.
use super::MexcExchange;
use crate::{
    exchange_helpers::{
        find_open_position, get_replacement_order_amount, is_native_batch_access_denied_error,
        is_recently_rejected_regular_order, is_regular_place_order, native_position_id,
        note_placed_stop_like_order, note_regular_order_submit_error, plan_order_trigger_type,
        stop_order_trend,
    },
    exchange_runtime::{self as account, Callback, ExchangeRuntimeHost, Iteration},
    method_tasks::{Completion, MethodObserver, Runtime, SessionRuntime, TaskHost},
    order_workflow::{self as workflow, Arguments, OrderWorkflowHost},
    runtime_assembly::{self as assembly, AssemblyHost},
    submit_validation::{
        MexcOrderSubmitError, assert_regular_order_batch_succeeded,
        assert_regular_order_submit_succeeded,
    },
    symbol_config::{
        MexcSymbolConfig, derive_symbol_config, format_change_leverage_payload,
        get_leverage_for_order,
    },
    types,
    utils::{native_volume_to_amount, to_integer_string, to_mexc_symbol, to_number},
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    exchange::Exchange,
    js_runtime::{
        ops,
        promise::{self, JsFuture, Promise},
        store,
        value::{Js, Thrown, arg, error_object, from_json, from_typed, to_json, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    socket_lifecycle::SocketSlot,
};
use safe_cex_num::JsJsonValue;
use safe_cex_transport::{RestBody, RestRequest};
use safe_cex_types::{
    FetchedLeverage, Market, Order, PlaceOrderOpts, Position, PositionMode, PositionSide,
};
use serde_json::Value;
use std::{
    cell::{OnceCell, RefCell},
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
};

// mexc.exchange.ts:103-108 and :125 module constants read by the ported bodies.
const MEXC_EXECUTE_CYCLE_SEVEN_DAYS: f64 = 2.0;
const MEXC_TRIGGER_TREND_LATEST_PRICE: f64 = 1.0;
const MEXC_REGULAR_ORDER_SUBMIT_GAP_MS: f64 = 450.0;
const MEXC_SCALED_ORDER_SUBMIT_GAP_MS: f64 = 1_500.0;
const MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS: f64 = 1_500.0;
// utils/loop.ts FALLBACK_MIN_INTERVAL_MS.
const LOOP_FALLBACK_MIN_INTERVAL_MS: f64 = 5_000.0;

pub(crate) struct Host {
    me: Weak<Host>,
    venue: Weak<MexcExchange>,
    runtime: Rc<dyn ExchangeRuntime>,
    this: OnceCell<Js>,
    timers: RefCell<Vec<(Js, Box<dyn ExchangeTask>)>>,
}

/// A started promise or settled value observed by the host-generic bodies.
pub(crate) struct Pending {
    future: RefCell<Option<JsFuture<'static>>>,
    result: RefCell<Option<Result<Js, Thrown>>>,
}

impl Pending {
    fn ready(result: Result<Js, Thrown>) -> Rc<Self> {
        Rc::new(Self {
            future: RefCell::new(None),
            result: RefCell::new(Some(result)),
        })
    }
    fn of(future: JsFuture<'static>) -> Rc<Self> {
        Rc::new(Self {
            future: RefCell::new(Some(future)),
            result: RefCell::new(None),
        })
    }
}

impl Completion<Js, Thrown> for Pending {
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<Js, Thrown>> {
        if let Some(result) = self.result.borrow().clone() {
            return Poll::Ready(result);
        }
        let mut slot = self.future.borrow_mut();
        let Some(future) = slot.as_mut() else {
            return Poll::Pending;
        };
        match future.as_mut().poll(cx) {
            Poll::Ready(result) => {
                *slot = None;
                *self.result.borrow_mut() = Some(result.clone());
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Native failure as the thrown value the bodies read `response.data` from.
pub(crate) fn thrown(error: ExchangeError) -> Thrown {
    let value = error_object(&error.name, &error.message);
    if let Some(response) = &error.response {
        let _ = value.define("response", from_json(response));
    }
    if let Some(code) = &error.code {
        let _ = value.define("code", code.clone());
    }
    Thrown {
        value,
        native: Some(Rc::new(error)),
    }
}

fn text(value: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&value.to_js_string()?))
}

fn json(value: &Js) -> Result<Option<Value>, Thrown> {
    if value.is_undefined() {
        Ok(None)
    } else {
        to_json(value).map(Some)
    }
}

fn symbols(value: &Js) -> Result<Option<Vec<String>>, Thrown> {
    if value.is_nullish() {
        Ok(None)
    } else {
        to_typed(value)
    }
}

fn js_json(value: JsJsonValue) -> Js {
    match value {
        JsJsonValue::Undefined => Js::Undefined,
        JsJsonValue::Null => Js::Null,
        JsJsonValue::Bool(b) => Js::Bool(b),
        JsJsonValue::Number(n) => Js::Number(n),
        JsJsonValue::String(s) => Js::str(&s),
        JsJsonValue::Array(items) => Js::array(items.into_iter().map(js_json).collect()),
        JsJsonValue::Object(fields) => {
            let out = Js::object(vec![]);
            for (key, field) in fields {
                let _ = out.define(&key, js_json(field));
            }
            out
        }
        JsJsonValue::BigInt(n) => Js::BigInt(Rc::new(n)),
    }
}

fn config_js(config: &MexcSymbolConfig) -> Result<Js, Thrown> {
    from_typed(config)
}

fn config_of(value: &Js) -> Result<MexcSymbolConfig, Thrown> {
    Ok(MexcSymbolConfig {
        margin_type: to_typed(&value.get("marginType")?)?,
        buy_leverage: value.get("buyLeverage")?.to_number()?,
        sell_leverage: value.get("sellLeverage")?.to_number()?,
        position_mode: to_typed(&value.get("positionMode")?)?,
    })
}

fn submit_error(error: MexcOrderSubmitError, data: &Js) -> Thrown {
    let value = error_object(error.name, &error.message);
    if error.response_data.is_some() {
        let _ = value.define("response", Js::object(vec![("data", data.clone())]));
    }
    if let Some(ids) = error.partial_order_ids {
        let _ = value.define(
            "partialOrderIds",
            Js::array(ids.iter().map(|id| Js::str(id)).collect()),
        );
    }
    if let Some(count) = error.partial_order_count {
        let _ = value.define("partialOrderCount", Js::Number(count as f64));
    }
    if let Some(count) = error.total_order_count {
        let _ = value.define("totalOrderCount", Js::Number(count));
    }
    if let Some(index) = error.failed_order_index {
        let _ = value.define("failedOrderIndex", Js::Number(index as f64));
    }
    Thrown::new(value)
}

type Member = fn(&Rc<Host>, Vec<Js>) -> Result<Js, Thrown>;

impl Host {
    pub(crate) fn new(venue: &Rc<MexcExchange>) -> Rc<Self> {
        let host = Rc::new_cyclic(|me| Self {
            me: me.clone(),
            venue: Rc::downgrade(venue),
            runtime: venue.base.runtime.clone(),
            this: OnceCell::new(),
            timers: RefCell::default(),
        });
        let this = host.facade(venue);
        let _ = host.this.set(this);
        host
    }

    fn venue(&self) -> Result<Rc<MexcExchange>, Thrown> {
        self.venue
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "MEXC exchange was dropped"))
    }

    fn spawn(&self, body: JsFuture<'static>) -> Js {
        Js::Promise(Promise::spawn(&self.runtime, body))
    }

    /// A native method future as the Promise its TypeScript member returns.
    fn native<T: serde::Serialize + 'static>(
        &self,
        run: impl for<'a> FnOnce(&'a MexcExchange) -> ExchangeFuture<'a, T> + 'static,
    ) -> Result<Js, Thrown> {
        let venue = self.venue()?;
        Ok(self.spawn(Box::pin(async move {
            let value = run(&venue).await.map_err(thrown)?;
            from_typed(&value)
        })))
    }

    fn native_unit(
        &self,
        run: impl for<'a> FnOnce(&'a MexcExchange) -> ExchangeFuture<'a, ()> + 'static,
    ) -> Result<Js, Thrown> {
        let venue = self.venue()?;
        Ok(self.spawn(Box::pin(async move {
            run(&venue).await.map_err(thrown)?;
            Ok(Js::Undefined)
        })))
    }

    fn observed(&self, observer: MethodObserver<Js, Thrown>) -> Js {
        self.spawn(Box::pin(observer))
    }

    fn cooldown(&self) -> Result<f64, Thrown> {
        self.exchange()
            .get("regularOrderSubmitCooldownUntil")?
            .to_number()
    }

    fn set_cooldown(&self, value: f64) -> Result<(), Thrown> {
        self.exchange()
            .set("regularOrderSubmitCooldownUntil", Js::Number(value))
    }

    fn positions(venue: &MexcExchange) -> Vec<Position> {
        venue.base.store.positions().to_vec()
    }

    fn settings(venue: &MexcExchange, symbol: &str) -> Option<MexcSymbolConfig> {
        venue.symbol_settings.borrow().get(symbol).cloned()
    }

    fn facade(&self, venue: &Rc<MexcExchange>) -> Js {
        let this = Js::object_with_class("MexcExchange", vec![]);
        let def = |key: &str, value: Js| {
            let _ = this.define(key, value);
        };
        def("store", store::facade(venue.base.store.clone()));
        def("xhr", self.xhr());
        def("publicWebsocket", self.socket(true));
        def("privateWebsocket", self.socket(false));
        def("symbolSettings", self.symbol_settings());
        def("regularOrderSubmitQueue", self.resolved_promise());
        def("lastRegularOrderSubmitAt", Js::Number(0.0));
        def("regularOrderSubmitCooldownUntil", Js::Number(0.0));
        def("nativeBatchOrdersAvailable", Js::Undefined);
        def("pendingStopSnapshotRefreshTimers", Js::new_map());
        {
            let venue = Rc::downgrade(venue);
            this.define_accessor(
                "isDisposed",
                Some(Rc::new(move |_| {
                    Ok(Js::Bool(
                        venue.upgrade().is_none_or(|v| v.base.is_disposed()),
                    ))
                })),
                None,
            )
            .expect("fresh extensible accessor target");
        }
        {
            let venue = Rc::downgrade(venue);
            this.define_accessor(
                "isPublicDataOnly",
                Some(Rc::new(move |_| {
                    let venue = venue
                        .upgrade()
                        .ok_or_else(|| Thrown::error("Error", "MEXC exchange was dropped"))?;
                    Ok(Js::Bool(Exchange::is_public_data_only(venue.as_ref())))
                })),
                None,
            )
            .expect("fresh extensible accessor target");
        }
        {
            let read = Rc::downgrade(venue);
            let write = Rc::downgrade(venue);
            this.define_accessor(
                "accountPositionMode",
                Some(Rc::new(move |_| match read.upgrade() {
                    Some(v) => from_typed(&*v.account_position_mode.borrow()),
                    None => Ok(Js::Undefined),
                })),
                Some(Rc::new(move |_, value| {
                    let mode: PositionMode = to_typed(&value)?;
                    if let Some(v) = write.upgrade() {
                        *v.account_position_mode.borrow_mut() = mode;
                    }
                    Ok(())
                })),
            )
            .expect("fresh extensible accessor target");
        }
        let members: &[(&str, Member)] = &[
            ("log", |h, a| {
                let severity = arg(&a, 1);
                let severity = if severity.is_undefined() {
                    "info".to_owned()
                } else {
                    text(&severity)?
                };
                h.venue()?.base.log(&text(&arg(&a, 0))?, &severity)?;
                Ok(Js::Undefined)
            }),
            ("emitAndLogError", |h, a| {
                let error = arg(&a, 0);
                let input = ExchangeErrorInput::Raw(error);
                h.venue()?.base.emit_and_log_error(input, None, None)?;
                Ok(Js::Undefined)
            }),
            ("initExternalPublicData", |h, _| {
                let venue = h.venue()?;
                Ok(h.spawn(Box::pin(async move {
                    let create = |options: &_| (venue.external)(options, venue.base.clone());
                    let external = venue
                        .base
                        .init_external_public_data(create)
                        .await
                        .map_err(thrown)?;
                    Ok(Js::Bool(external))
                })))
            }),
            ("fetchMarkets", |h, _| h.native(|v| v.fetch_markets())),
            ("fetchTickers", |h, _| h.native(|v| v.fetch_tickers())),
            ("fetchBalance", |h, _| h.native(|v| v.fetch_balance())),
            ("fetchPositionMode", |h, _| {
                h.native(|v| v.fetch_position_mode())
            }),
            ("fetchPositions", |h, a| {
                let symbols = symbols(&arg(&a, 0))?;
                h.native(move |v| v.fetch_positions(symbols))
            }),
            ("fetchOrders", |h, a| {
                let symbols = symbols(&arg(&a, 0))?;
                h.native(move |v| v.fetch_orders(symbols))
            }),
            ("fetchAndSetOrders", |h, a| {
                let symbols = symbols(&arg(&a, 0))?;
                h.native(move |v| v.fetch_and_set_orders(symbols))
            }),
            ("fetchAndSetRegularOrders", |h, a| {
                let symbols = symbols(&arg(&a, 0))?;
                h.native(move |v| v.fetch_and_set_regular_orders(symbols))
            }),
            ("cancelOrders", |h, a| {
                let orders: Vec<Order> = to_typed(&arg(&a, 0))?;
                h.native_unit(move |v| v.cancel_orders(orders))
            }),
            ("getTickInterval", |h, _| {
                Ok(h.venue()?
                    .base
                    .get_tick_interval()
                    .map_or(Js::Undefined, Js::Number))
            }),
            ("start", |h, _| {
                Ok(h.observed(assembly::start_exchange(h.clone())))
            }),
            ("tick", |h, _| Ok(h.observed(account::tick(h.clone())))),
            ("fetchBalanceAndPositions", |h, _| {
                Ok(h.observed(account::fetch_balance_and_positions(h.clone())))
            }),
            ("placeOrder", |h, a| {
                Ok(h.observed(workflow::place_order(h.clone(), arg(&a, 0))))
            }),
            ("placeOrders", |h, a| {
                Ok(h.observed(workflow::place_orders(h.clone(), arg(&a, 0))))
            }),
            ("placeOrdersSequentially", |h, a| {
                Ok(h.observed(workflow::place_orders_sequentially(
                    h.clone(),
                    arg(&a, 0),
                    arg(&a, 1),
                )))
            }),
            ("placeRegularOrdersBatch", |h, a| {
                Ok(h.observed(workflow::place_regular_orders_batch(h.clone(), arg(&a, 0))))
            }),
            ("formatRegularOrderPayload", |h, a| {
                workflow::format_regular_order_payload(h.clone(), arg(&a, 0))
            }),
            ("enqueueRegularOrderSubmit", |h, a| {
                Ok(h.observed(workflow::enqueue_regular_order_submit(
                    h.clone(),
                    arg(&a, 0),
                )))
            }),
            ("waitForRegularOrderSubmitSlot", |h, _| {
                Ok(h.observed(workflow::wait_for_regular_order_submit_slot(h.clone())))
            }),
            ("addOptimisticRegularOrder", |h, a| {
                workflow::add_optimistic_regular_order(h.clone(), arg(&a, 0), arg(&a, 1))
            }),
            ("addOptimisticStopOrder", |h, a| {
                workflow::add_optimistic_stop_order(
                    h.clone(),
                    arg(&a, 0),
                    arg(&a, 1),
                    arg(&a, 2),
                    arg(&a, 3),
                    arg(&a, 4),
                )
            }),
            ("scheduleStopOrderSnapshotRefresh", |h, a| {
                workflow::schedule_stop_order_snapshot_refresh(h.clone(), arg(&a, 0))
            }),
            ("placePlanOrder", |h, a| {
                Ok(h.observed(assembly::place_plan_order(h.clone(), arg(&a, 0))))
            }),
            ("placePositionStopOrder", |h, a| {
                Ok(h.observed(assembly::place_position_stop_order(
                    h.clone(),
                    arg(&a, 0),
                    arg(&a, 1),
                    arg(&a, 2),
                )))
            }),
            ("updateOrder", |h, a| {
                Ok(h.observed(assembly::update_order(h.clone(), arg(&a, 0))))
            }),
            ("setLeverage", |h, a| {
                Ok(h.observed(account::set_leverage(
                    h.clone(),
                    arg(&a, 0),
                    arg(&a, 1),
                    arg(&a, 2),
                )))
            }),
            ("changePositionMode", |h, a| {
                Ok(h.observed(account::change_position_mode(h.clone(), arg(&a, 0))))
            }),
            ("changeMarginType", |h, a| {
                Ok(h.observed(account::change_margin_type(
                    h.clone(),
                    arg(&a, 0),
                    arg(&a, 1),
                )))
            }),
            ("cancelSymbolOrders", |h, a| {
                Ok(h.observed(account::cancel_symbol_orders(h.clone(), arg(&a, 0))))
            }),
            ("cancelAllOrders", |h, _| {
                Ok(h.observed(account::cancel_all_orders(h.clone())))
            }),
            ("resolveCallerClientOrderId", |h, a| {
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(h.venue()?
                    .base
                    .resolve_caller_client_order_id(&opts)?
                    .map_or(Js::Undefined, |id| Js::str(&id)))
            }),
            ("assertCallerClientOrderIds", |h, a| {
                let orders: Vec<PlaceOrderOpts> = to_typed(&arg(&a, 0))?;
                h.venue()?.base.assert_caller_client_order_ids(&orders)?;
                Ok(Js::Undefined)
            }),
            ("deriveSymbolConfig", |h, a| {
                let venue = h.venue()?;
                let symbol = text(&arg(&a, 0))?;
                let config = derive_symbol_config(
                    &symbol,
                    &Self::positions(&venue),
                    Self::settings(&venue, &symbol).as_ref(),
                    *venue.account_position_mode.borrow(),
                )?;
                config_js(&config)
            }),
            ("getLeverageForOrder", |h, a| {
                let venue = h.venue()?;
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(Js::Number(get_leverage_for_order(
                    &opts,
                    &Self::positions(&venue),
                    Self::settings(&venue, &opts.symbol).as_ref(),
                    *venue.account_position_mode.borrow(),
                )?))
            }),
            ("updateFetchedLeverage", |h, a| {
                let config = config_of(&arg(&a, 1))?;
                MexcExchange::update_fetched_leverage(
                    &h.venue()?.base,
                    &text(&arg(&a, 0))?,
                    FetchedLeverage {
                        buy_leverage: config.buy_leverage,
                        sell_leverage: config.sell_leverage,
                        margin_type: Some(config.margin_type),
                        position_mode: Some(config.position_mode),
                        is_fetching: None,
                    },
                )?;
                Ok(Js::Undefined)
            }),
            ("formatChangeLeveragePayload", |h, a| {
                let venue = h.venue()?;
                let market: Market = to_typed(&arg(&a, 1))?;
                let options = arg(&a, 5);
                let include =
                    !options.is_nullish() && options.get("includeOpenTypeWithPositionId")?.truthy();
                Ok(js_json(format_change_leverage_payload(
                    &text(&arg(&a, 0))?,
                    &market,
                    arg(&a, 2).to_number()? as u8,
                    arg(&a, 3).to_number()?,
                    to_typed(&arg(&a, 4))?,
                    include,
                    &Self::positions(&venue),
                )?))
            }),
            ("assertCommonSuccess", |_, a| {
                crate::exchange_errors::assert_common_success(
                    json(&arg(&a, 0))?.as_ref(),
                    &text(&arg(&a, 1))?,
                )?;
                Ok(Js::Undefined)
            }),
            ("noteRegularOrderSubmitError", |h, a| {
                let mut cooldown = h.cooldown()?;
                let clock = h.venue()?.current_time_ms.clone();
                note_regular_order_submit_error(
                    &mut cooldown,
                    json(&arg(&a, 0))?.as_ref(),
                    &|| clock(),
                )?;
                h.set_cooldown(cooldown)?;
                Ok(Js::Undefined)
            }),
            ("assertRegularOrderSubmitSucceeded", |h, a| {
                let data = arg(&a, 0);
                let payload = json(&data)?.map(|v| Rc::new(RefCell::new(v)));
                let mut cooldown = h.cooldown()?;
                let clock = h.venue()?.current_time_ms.clone();
                let result =
                    assert_regular_order_submit_succeeded(payload.as_ref(), &mut cooldown, &|| {
                        clock()
                    });
                h.set_cooldown(cooldown)?;
                result.map_err(|e| submit_error(e, &data))?;
                Ok(Js::Undefined)
            }),
            ("assertRegularOrderBatchSucceeded", |_, a| {
                let data = arg(&a, 0);
                let payload = json(&data)?.map(|v| Rc::new(RefCell::new(v)));
                assert_regular_order_batch_succeeded(payload.as_ref(), arg(&a, 1).to_number()?)
                    .map_err(|e| submit_error(e, &data))?;
                Ok(Js::Undefined)
            }),
            ("isRecentlyRejectedRegularOrder", |h, a| {
                let venue = h.venue()?;
                let clock = venue.current_time_ms.clone();
                Ok(Js::Bool(is_recently_rejected_regular_order(
                    &mut venue.rejected.borrow_mut(),
                    &text(&arg(&a, 0))?,
                    &|| clock(),
                )))
            }),
            ("notePlacedStopLikeOrder", |h, a| {
                let venue = h.venue()?;
                let clock = venue.current_time_ms.clone();
                note_placed_stop_like_order(
                    &mut venue.placed.borrow_mut(),
                    &text(&arg(&a, 0))?,
                    &|| clock(),
                );
                Ok(Js::Undefined)
            }),
            ("planOrderTriggerType", |_, a| {
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(Js::Number(f64::from(plan_order_trigger_type(&opts))))
            }),
            ("stopOrderTrend", |_, a| {
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(Js::Number(f64::from(stop_order_trend(&opts))))
            }),
            ("findOpenPosition", |_, a| {
                let side: PositionSide = to_typed(&arg(&a, 1))?;
                let positions: Vec<Position> = to_typed(&arg(&a, 2))?;
                find_open_position(&text(&arg(&a, 0))?, side, &positions)
                    .map_or(Ok(Js::Undefined), from_typed)
            }),
            ("nativePositionId", |_, a| {
                let position: Position = to_typed(&arg(&a, 0))?;
                Ok(Js::str(&native_position_id(&position)?))
            }),
            ("getReplacementOrderAmount", |_, a| {
                let order: Order = to_typed(&arg(&a, 0))?;
                Ok(Js::Number(get_replacement_order_amount(&order)?))
            }),
            ("isRegularPlaceOrder", |_, a| {
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(Js::Bool(is_regular_place_order(&opts)))
            }),
            ("isNativeBatchAccessDeniedError", |_, a| {
                Ok(Js::Bool(is_native_batch_access_denied_error(
                    json(&arg(&a, 0))?.as_ref(),
                )?))
            }),
        ];
        for (name, member) in members {
            let host = self.me.clone();
            let member = *member;
            def(
                name,
                Js::function(name, move |_, a| {
                    let host = host
                        .upgrade()
                        .ok_or_else(|| Thrown::error("Error", "MEXC binding was dropped"))?;
                    member(&host, a)
                }),
            );
        }
        this
    }

    /// The Axios verbs the bodies call, over the venue's native REST adapter.
    fn xhr(&self) -> Js {
        let verb = |method: &'static str| {
            let host = self.me.clone();
            Js::function(method, move |_, a| {
                let host = host
                    .upgrade()
                    .ok_or_else(|| Thrown::error("Error", "MEXC binding was dropped"))?;
                let venue = host.venue()?;
                let (body, config) = if method == "post" {
                    (
                        Some(RestBody::Untransformed(
                            safe_cex_exchange::js_runtime::value::to_retained_input(&arg(&a, 1))?,
                        )),
                        arg(&a, 2),
                    )
                } else {
                    (None, arg(&a, 1))
                };
                let params = if config.is_nullish() {
                    None
                } else {
                    json(&config.get("params")?)?
                };
                let request = RestRequest {
                    method: method.into(),
                    url: text(&arg(&a, 0))?,
                    body,
                    params,
                    ..Default::default()
                };
                Ok(host.spawn(Box::pin(async move {
                    let response = venue
                        .xhr
                        .execute(request)
                        .await
                        .map_err(|e| thrown(ExchangeError::from(e)))?;
                    let data = response.body.json_value().map_err(|e| {
                        let mut error = ExchangeError::new(e.to_string());
                        error.name = "SyntaxError".into();
                        thrown(error)
                    })?;
                    Ok(Js::object(vec![
                        ("data", from_json(&data)),
                        ("status", Js::Number(f64::from(response.status))),
                    ]))
                })))
            })
        };
        Js::object_with_class("Axios", vec![("get", verb("get")), ("post", verb("post"))])
    }

    fn socket(&self, public: bool) -> Js {
        let venue = self.venue.clone();
        Js::object(vec![(
            "connectAndSubscribe",
            Js::function("connectAndSubscribe", move |_, _| {
                let venue = venue
                    .upgrade()
                    .ok_or_else(|| Thrown::error("Error", "MEXC exchange was dropped"))?;
                let slot = if public {
                    SocketSlot::Public
                } else {
                    SocketSlot::Private
                };
                if let Some(socket) = venue.base.get_socket(slot) {
                    socket.connect_and_subscribe(false)?;
                }
                Ok(Js::Undefined)
            }),
        )])
    }

    /// `Map<string, MexcSymbolConfig>` over the native per-symbol settings.
    fn symbol_settings(&self) -> Js {
        let settings = Js::object_with_class("MexcSymbolSettings", vec![]);
        let read = self.venue.clone();
        let _ = settings.define(
            "get",
            Js::function("get", move |_, a| {
                let Some(venue) = read.upgrade() else {
                    return Ok(Js::Undefined);
                };
                Self::settings(&venue, &text(&arg(&a, 0))?)
                    .map_or(Ok(Js::Undefined), |c| config_js(&c))
            }),
        );
        let write = self.venue.clone();
        let receiver = settings.clone();
        let _ = settings.define(
            "set",
            Js::function("set", move |_, a| {
                let config = config_of(&arg(&a, 1))?;
                if let Some(venue) = write.upgrade() {
                    venue
                        .symbol_settings
                        .borrow_mut()
                        .insert(text(&arg(&a, 0))?, config);
                }
                Ok(receiver.clone())
            }),
        );
        settings
    }

    fn resolved_promise(&self) -> Js {
        Js::Promise(Promise::settled(&self.runtime, Ok(Js::Undefined)))
    }

    fn timer(&self, delay_ms: u64, callback: Rc<dyn Fn() -> Result<Js, Thrown>>) -> Js {
        let handle = Js::object_with_class("Timeout", vec![]);
        let runtime = self.runtime.clone();
        let task = self.runtime.spawn_local(Box::pin(async move {
            if runtime.sleep(delay_ms).await.is_err() {
                return;
            }
            if let Err(error) = callback() {
                runtime.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source": "setTimeout"}).into(),
                );
            }
        }));
        self.timers.borrow_mut().push((handle.clone(), task));
        handle
    }
}

fn delay(value: &Js) -> Result<u64, Thrown> {
    let ms = value.to_number()?;
    Ok(if ms.is_finite() && ms > 0.0 {
        ms as u64
    } else {
        0
    })
}

impl Runtime for Host {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn same_value(&self, left: &Js, right: &Js) -> bool {
        left.same_value_zero(right)
    }
    fn is_text(&self, value: &Js, text: &str) -> bool {
        matches!(value, Js::String(s) if s.iter().copied().eq(text.encode_utf16()))
    }
    fn field(&self, value: &Js, key: &str) -> Result<Js, Thrown> {
        value.get(key)
    }
    fn abstract_string(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn global_string(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn log(&self, text: &[u16], severity: &'static str) -> Result<(), Thrown> {
        self.venue()?
            .base
            .log(&String::from_utf16_lossy(text), severity)?;
        Ok(())
    }
    fn new_error(&self, text: Vec<u16>) -> Thrown {
        Thrown::error("Error", &String::from_utf16_lossy(&text))
    }
}

impl SessionRuntime for Host {
    fn text(&self, units: Vec<u16>) -> Js {
        Js::utf16(&units)
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn json_parse(&self, value: &Js) -> Result<Js, Thrown> {
        let parsed: Value = serde_json::from_str(&text(value)?)
            .map_err(|e| Thrown::error("SyntaxError", &e.to_string()))?;
        Ok(from_json(&parsed))
    }
    fn stringify(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(to_json(value)?.to_string().encode_utf16().collect())
    }
    fn spread(&self, value: &Js) -> Result<Js, Thrown> {
        value.spread_object()
    }
    fn define_field(&self, value: &Js, key: &str, field: Js) -> Result<(), Thrown> {
        value.define(key, field)
    }
    fn set_field(&self, value: &Js, key: &str, field: Js) -> Result<(), Thrown> {
        value.set(key, field)
    }
}

impl TaskHost for Host {
    type Operation = Pending;
    type Resolution = Pending;
    fn resolve_return(&self, value: Js) -> Result<Rc<Pending>, Thrown> {
        Ok(match value {
            Js::Promise(p) => Pending::of(Box::pin(p.wait())),
            other => Pending::ready(Ok(other)),
        })
    }
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        let _ = Promise::spawn(
            &self.runtime,
            Box::pin(async move {
                task.await;
                Ok(Js::Undefined)
            }),
        );
    }
}

// The common for-of front door for arrays, maps, sets and iterator objects.
fn for_of_js(value: Js, visitor: &mut Iteration<'_, Js, Thrown>) -> Result<(), Thrown> {
    let iterator = value.iterator()?;
    let next = iterator.get("next")?;
    loop {
        let step = next.call(iterator.clone(), vec![])?;
        if !step.is_object() {
            return Err(Thrown::type_error("iterator result is not an object"));
        }
        if step.get("done")?.truthy() {
            return Ok(());
        }
        let completion = visitor(step.get("value")?);
        if matches!(completion, Ok(false)) {
            continue;
        }
        // A body throw wins over IteratorClose errors. A normal break
        // propagates return()/getter/shape failures.
        let close = (|| {
            let method = iterator.get("return")?;
            if method.is_nullish() {
                return Ok(());
            }
            let returned = method.call(iterator.clone(), vec![])?;
            if !returned.is_object() {
                return Err(Thrown::type_error(
                    "iterator return result is not an object",
                ));
            }
            Ok(())
        })();
        return match completion {
            Err(error) => Err(error),
            Ok(_) => close,
        };
    }
}

impl ExchangeRuntimeHost for Host {
    fn exchange(&self) -> Js {
        self.this.get().cloned().unwrap_or(Js::Undefined)
    }
    fn call(&self, callee: Js, receiver: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        callee.call(receiver, args)
    }
    fn function(&self, callback: Callback<Js, Thrown>) -> Js {
        Js::function("", move |_, a| callback(arg(&a, 0)))
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn indexed(&self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        value.get_key(&key.to_js_string()?)
    }
    fn destructure_pair(&self, value: Js) -> Result<(Js, Js), Thrown> {
        Ok((value.get("0")?, value.get("1")?))
    }
    fn maximum(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(safe_cex_num::math_max(
            a.to_number()?,
            b.to_number()?,
        )))
    }
    fn minimum(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(safe_cex_num::math_min(
            a.to_number()?,
            b.to_number()?,
        )))
    }
    fn endpoint(&self, name: &str) -> Js {
        types::ENDPOINTS[name]
            .as_str()
            .map_or(Js::Undefined, Js::str)
    }
    fn to_symbol(&self, value: Js) -> Result<Js, Thrown> {
        Ok(Js::str(&to_mexc_symbol(&text(&value)?)))
    }
    fn omit_undefined(&self, value: Js) -> Result<Js, Thrown> {
        let out = Js::object(vec![]);
        for (key, field) in value.own_entries()? {
            if !field.is_undefined() {
                out.define_key(&key, field)?;
            }
        }
        Ok(out)
    }
    fn reverse_position_mode(&self, mode: &Js) -> Result<Js, Thrown> {
        from_json(&types::REVERSE_POSITION_MODE).get_key(&mode.to_js_string()?)
    }
    fn promise_all(&self, values: Vec<Js>) -> Result<Rc<Pending>, Thrown> {
        Ok(Pending::of(Box::pin(
            promise::all(&self.runtime, values).wait(),
        )))
    }
    fn for_of(&self, value: Js, visitor: &mut Iteration<'_, Js, Thrown>) -> Result<(), Thrown> {
        // This opaque typed settings view has no JS object protocol. Preserve
        // its existing projection; actual JS values all use GetIterator below.
        if matches!(&value, Js::Object(object) if object.class == "MexcSymbolSettings") {
            let venue = self.venue()?;
            let entries: Vec<_> = venue
                .symbol_settings
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (symbol, config) in entries {
                if visitor(Js::array(vec![Js::str(&symbol), config_js(&config)?]))? {
                    break;
                }
            }
            return Ok(());
        }
        for_of_js(value, visitor)
    }
    // utils/loop.ts: sleep(minInterval), then a zero-delay setTimeout(fn).
    fn schedule_loop(
        &self,
        callback: Rc<dyn Fn() -> Result<Js, Thrown>>,
        delay: Js,
    ) -> Result<(), Thrown> {
        let interval = delay.to_number()?;
        if interval == f64::INFINITY {
            return Ok(());
        }
        let interval = if interval > 0.0 {
            interval
        } else {
            self.runtime.console(
                "warn",
                "[safe-cex] loop() called without a minInterval — defaulting to 5000ms. This is a safety fallback; the upstream caller should pass a real interval.",
            );
            LOOP_FALLBACK_MIN_INTERVAL_MS
        };
        let runtime = self.runtime.clone();
        let _ = Promise::spawn(
            &self.runtime,
            Box::pin(async move {
                runtime.sleep(interval as u64).await.map_err(thrown)?;
                runtime.sleep(0).await.map_err(thrown)?;
                if let Err(error) = callback() {
                    runtime.report_exception(
                        &error.to_exchange_error(),
                        &serde_json::json!({"source": "loop"}).into(),
                    );
                }
                Ok(Js::Undefined)
            }),
        );
        Ok(())
    }
    fn clear_timeout(&self, handle: Js) -> Result<(), Thrown> {
        let mut timers = self.timers.borrow_mut();
        if let Some(index) = timers.iter().position(|(h, _)| h.same_reference(&handle)) {
            let (_, task) = timers.remove(index);
            task.cancel();
        }
        Ok(())
    }
    fn base_dispose(&self) -> Result<(), Thrown> {
        self.venue()?.base.dispose()?;
        Ok(())
    }
}

impl OrderWorkflowHost for Host {
    fn import(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let a = |i| arg(&args, i);
        Ok(match name {
            "toNumber" => {
                let fallback = a(1);
                let fallback = if fallback.is_undefined() {
                    None
                } else {
                    Some(fallback.to_number()?)
                };
                Js::Number(to_number(json(&a(0))?.as_ref(), fallback)?)
            }
            "adjust" => {
                let step = match a(1) {
                    Js::String(s) => {
                        safe_cex_num::PrecisionInput::String(String::from_utf16_lossy(&s))
                    }
                    other => safe_cex_num::PrecisionInput::Number(other.to_number()?),
                };
                Js::Number(safe_cex_num::adjust(a(0).to_number()?, step))
            }
            "uuid" => {
                let id = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
                    for byte in bytes {
                        *byte = (self.runtime.random() * 64.0) as u8;
                    }
                    Ok(())
                })
                .map_err(|message| Thrown::error("Error", &message))?;
                Js::str(&id)
            }
            "now" => Js::Number((self.venue()?.current_time_ms)()),
            "sleep" => {
                let runtime = self.runtime.clone();
                let ms = delay(&a(0))?;
                self.spawn(Box::pin(async move {
                    runtime.sleep(ms).await.map_err(thrown)?;
                    Ok(Js::Undefined)
                }))
            }
            "extractData" => {
                let payload = a(0);
                let data = if payload.is_nullish() {
                    Js::Undefined
                } else {
                    payload.get("data")?
                };
                if data.is_nullish() { payload } else { data }
            }
            "extractRows" => {
                let rows = self.import("extractData", args.clone())?;
                if rows.is_array() {
                    rows
                } else {
                    Js::array(vec![])
                }
            }
            "toIntegerString" => Js::str(&to_integer_string(json(&a(0))?.as_ref())?),
            "nativeVolumeToAmount" => {
                let market: Option<Market> = if a(1).is_nullish() {
                    None
                } else {
                    Some(to_typed(&a(1))?)
                };
                Js::Number(native_volume_to_amount(
                    json(&a(0))?.as_ref(),
                    market.as_ref(),
                )?)
            }
            other => {
                return Err(Thrown::error(
                    "ReferenceError",
                    &format!("{other} is not bound for the MEXC exchange host"),
                ));
            }
        })
    }
    fn constant(&self, name: &str) -> Js {
        match name {
            "REVERSE_TIME_IN_FORCE" => from_json(&types::REVERSE_TIME_IN_FORCE),
            "REVERSE_ORDER_TYPE" => from_json(&types::REVERSE_ORDER_TYPE),
            "REVERSE_ORDER_SIDE" => from_json(&types::REVERSE_ORDER_SIDE),
            "REVERSE_MARGIN_TYPE" => from_json(&types::REVERSE_MARGIN_TYPE),
            "REVERSE_POSITION_MODE" => from_json(&types::REVERSE_POSITION_MODE),
            "GRAFANA_ENDPOINTS_EVERY" => from_json(&types::GRAFANA_ENDPOINTS_EVERY),
            "GRAFANA_ENDPOINTS_PERIODIC" => from_json(&types::GRAFANA_ENDPOINTS_PERIODIC),
            "MEXC_EXECUTE_CYCLE_SEVEN_DAYS" => Js::Number(MEXC_EXECUTE_CYCLE_SEVEN_DAYS),
            "MEXC_TRIGGER_TREND_LATEST_PRICE" => Js::Number(MEXC_TRIGGER_TREND_LATEST_PRICE),
            "MEXC_REGULAR_ORDER_SUBMIT_GAP_MS" => Js::Number(MEXC_REGULAR_ORDER_SUBMIT_GAP_MS),
            "MEXC_SCALED_ORDER_SUBMIT_GAP_MS" => Js::Number(MEXC_SCALED_ORDER_SUBMIT_GAP_MS),
            "MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS" => {
                Js::Number(MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS)
            }
            _ => Js::Undefined,
        }
    }
    fn function_args(&self, callback: Arguments<Js, Thrown>) -> Js {
        Js::function("", move |_, a| callback(a))
    }
    fn promise_value(&self, observer: MethodObserver<Js, Thrown>) -> Js {
        self.observed(observer)
    }
    fn add(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::add(&a, &b)
    }
    fn subtract(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::subtract(&a, &b)
    }
    fn less(&self, a: Js, b: Js) -> Result<bool, Thrown> {
        ops::less_than(&a, &b)
    }
    fn is_object(&self, value: &Js) -> bool {
        value.is_object()
    }
    fn property_key(&self, value: Js) -> Result<Js, Thrown> {
        Ok(Js::String(value.to_js_string()?))
    }
    fn define_key(&self, object: &Js, key: Js, value: Js) -> Result<(), Thrown> {
        object.define_key(&key.to_js_string()?, value)
    }
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> Result<Js, Thrown>>,
        delay_ms: Js,
    ) -> Result<Js, Thrown> {
        Ok(self.timer(delay(&delay_ms)?, callback))
    }
}

fn constructor_owned() -> Thrown {
    Thrown::error(
        "Error",
        "MexcExchange::new constructs the MEXC class natively",
    )
}

impl AssemblyHost for Host {
    fn base_exchange(&self, _: Js, _: Js, _: Js) -> Result<Js, Thrown> {
        Err(constructor_owned())
    }
    fn member(&self, name: &'static str, owner: &Js) -> Result<Js, Thrown> {
        owner.get(name)
    }
    fn constructor(&self, _: &str) -> Js {
        Js::Undefined
    }
    fn construct(&self, _: Js, _: Vec<Js>) -> Result<Js, Thrown> {
        Err(constructor_owned())
    }
    fn fresh_map(&self) -> Js {
        Js::new_map()
    }
    fn resolved_promise(&self) -> Js {
        Host::resolved_promise(self)
    }
    fn finite(&self, value: &Js) -> bool {
        matches!(value, Js::Number(n) if n.is_finite())
    }
    fn is_array(&self, value: &Js) -> bool {
        value.is_array()
    }
    fn is_string_or_number(&self, value: &Js) -> bool {
        matches!(value, Js::String(_) | Js::Number(_))
    }
    fn less_equal(&self, a: Js, b: Js) -> Result<bool, Thrown> {
        ops::less_equal(&a, &b)
    }
    fn set_indexed(&self, object: &Js, key: &Js, value: Js) -> Result<(), Thrown> {
        object.set_key(&key.to_js_string()?, value)
    }
    fn has_property(&self, value: &Js, key: &str) -> Result<bool, Thrown> {
        if !value.is_object() {
            return Err(Thrown::type_error(&format!(
                "Cannot use 'in' operator to search for '{key}' in {}",
                value.to_display()
            )));
        }
        let mut current = value.clone();
        loop {
            if current.has_own(key) {
                return Ok(true);
            }
            let next = match &current {
                Js::Object(o) => o.proto.borrow().clone(),
                _ => None,
            };
            match next {
                Some(proto) => current = proto,
                None => return Ok(false),
            }
        }
    }
    fn delete_indexed(&self, value: &Js, key: &Js) -> Result<(), Thrown> {
        value.delete(&key.to_js_string()?)?;
        Ok(())
    }
    fn destructure_fields(&self, value: &Js, keys: &[&str]) -> Result<Vec<Js>, Thrown> {
        if value.is_nullish() {
            return Err(Thrown::type_error(&format!(
                "Cannot destructure '{}' as it is {}.",
                value.to_display(),
                value.to_display()
            )));
        }
        keys.iter().map(|key| value.get(key)).collect()
    }
}

/// Run a body observer to its typed result for the `Exchange` trait.
pub(crate) fn typed<T: serde::de::DeserializeOwned + 'static>(
    host: Option<Rc<Host>>,
    body: impl FnOnce(Rc<Host>) -> Result<MethodObserver<Js, Thrown>, Thrown> + 'static,
) -> ExchangeFuture<'static, T> {
    let observer = host
        .ok_or_else(unbuilt)
        .map_err(Thrown::from)
        .and_then(body);
    Box::pin(async move {
        let value = observer
            .map_err(ExchangeError::from)?
            .await
            .map_err(ExchangeError::from)?;
        to_typed(&value).map_err(ExchangeError::from)
    })
}

pub(crate) fn unit(
    host: Option<Rc<Host>>,
    body: impl FnOnce(Rc<Host>) -> Result<MethodObserver<Js, Thrown>, Thrown> + 'static,
) -> ExchangeFuture<'static, ()> {
    let observer = host
        .ok_or_else(unbuilt)
        .map_err(Thrown::from)
        .and_then(body);
    Box::pin(async move {
        observer
            .map_err(ExchangeError::from)?
            .await
            .map_err(ExchangeError::from)?;
        Ok(())
    })
}

pub(crate) fn unbuilt() -> ExchangeError {
    ExchangeError::new("MexcExchange was not built by MexcExchange::new")
}

pub(crate) fn dispose(host: &Host) -> Result<(), ExchangeError> {
    account::dispose(host).map_err(ExchangeError::from)
}

pub(crate) fn value<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, Thrown> {
    from_typed(value)
}

#[cfg(test)]
mod iterator_tests {
    use super::*;

    #[test]
    fn mexc_map_iterator_empty_admission() {
        for method in ["values", "keys", "entries", "map"] {
            let map = Js::new_map();
            let input = if method == "map" {
                map
            } else {
                map.call_method(method, vec![]).unwrap()
            };
            let mut called = false;
            let result = for_of_js(input, &mut |_| {
                called = true;
                Ok(false)
            });
            assert!(
                result.is_ok(),
                "mexc_empty_map_iterator_is_iterable {method}: {result:?}"
            );
            assert!(!called, "empty iterator must not call its visitor");
        }
    }

    #[test]
    fn mexc_map_iterator_nonempty_admission_and_live_mutation() {
        for method in ["values", "keys", "entries", "map"] {
            let map = Js::new_map();
            map.call_method("set", vec![Js::str("a"), Js::Number(-0.0)])
                .unwrap();
            map.call_method("set", vec![Js::str("b"), Js::str("removed")])
                .unwrap();
            let live = Js::object(vec![("identity", Js::str("live"))]);
            let input = if method == "map" {
                map.clone()
            } else {
                map.call_method(method, vec![]).unwrap()
            };
            let mut rows = vec![];
            let result = for_of_js(input, &mut |value| {
                rows.push(value);
                if rows.len() == 1 {
                    map.call_method("delete", vec![Js::str("b")])?;
                    map.call_method("set", vec![Js::str("c"), live.clone()])?;
                    map.call_method("set", vec![Js::str("d"), Js::Number(f64::NAN)])?;
                }
                Ok(false)
            });
            assert!(
                result.is_ok(),
                "mexc_nonempty_map_iterator_is_iterable {method}: {result:?}"
            );
            assert_eq!(
                rows.len(),
                3,
                "mexc iterator visits additions and skips deletion {method}"
            );
            if method == "keys" {
                for (row, expected) in rows.iter().zip(["a", "c", "d"]) {
                    assert!(row.strict_equals(&Js::str(expected)));
                }
            } else {
                let values: Vec<_> = rows
                    .iter()
                    .map(|row| {
                        if method == "values" {
                            row.clone()
                        } else {
                            row.get("1").unwrap()
                        }
                    })
                    .collect();
                assert_eq!(
                    values[0].to_number().unwrap().to_bits(),
                    (-0.0_f64).to_bits()
                );
                assert!(
                    values[1].strict_equals(&live),
                    "native reference identity survives iteration"
                );
                assert!(
                    values[2].to_number().unwrap().is_nan(),
                    "native NaN survives iteration"
                );
            }
        }
    }
}
