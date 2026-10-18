//! Construction, startup, stop placement and remaining websocket entry bodies.
use crate::{
    method_tasks::{MethodObserver, observe, start},
    order_workflow::{
        OrderWorkflowHost, await_value, f, invoke, market, nullish, opt, or, own, t, template,
    },
};
use std::rc::Rc;
/// Native base/child constructors and member bindings use existing mapped Rust
/// bodies. No default member body or TypeScript method evaluation is permitted.
pub trait AssemblyHost: OrderWorkflowHost {
    fn base_exchange(
        &self,
        name: Self::Value,
        opts: Self::Value,
        store: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn member(&self, name: &'static str, owner: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn constructor(&self, name: &str) -> Self::Value;
    fn construct(
        &self,
        constructor: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn fresh_map(&self) -> Self::Value;
    fn resolved_promise(&self) -> Self::Value;
    fn finite(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn is_string_or_number(&self, value: &Self::Value) -> bool;
    fn less_equal(&self, a: Self::Value, b: Self::Value) -> Result<bool, Self::Error>;
    fn set_indexed(
        &self,
        object: &Self::Value,
        key: &Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn has_property(&self, value: &Self::Value, key: &str) -> Result<bool, Self::Error>;
    fn delete_indexed(&self, value: &Self::Value, key: &Self::Value) -> Result<(), Self::Error>;
    fn destructure_fields(
        &self,
        value: &Self::Value,
        keys: &[&str],
    ) -> Result<Vec<Self::Value>, Self::Error>;
}
pub const EXCHANGE_METHOD_FIELDS: &[&str] = &[
    "validateAccount",
    "getAccount",
    "start",
    "tick",
    "fetchMarkets",
    "fetchTickers",
    "fetchBalance",
    "fetchBalanceAndPositions",
    "fetchPositionMode",
    "fetchPositions",
    "fetchOrders",
    "fetchAndSetOrders",
    "fetchRegularOrders",
    "fetchAndSetRegularOrders",
    "fetchHistoricalFills",
    "fetchOHLCV",
    "listenOHLCV",
    "listenOrderBook",
    "fetchOrderBook",
    "placeOrder",
    "formatRegularOrderPayload",
    "assertRegularOrderSubmitSucceeded",
    "enqueueRegularOrderSubmit",
    "waitForRegularOrderSubmitSlot",
    "noteRegularOrderSubmitError",
    "placePlanOrder",
    "placePositionStopOrder",
    "findOpenPosition",
    "addOptimisticRegularOrder",
    "addOptimisticStopOrder",
    "markStopLikeOrderSeen",
    "notePlacedStopLikeOrder",
    "isRecentlyPlacedStopLikeOrder",
    "scheduleStopOrderSnapshotRefresh",
    "nativePositionId",
    "planOrderTriggerType",
    "stopOrderTrend",
    "placeOrders",
    "placeOrdersSequentially",
    "isRegularPlaceOrder",
    "placeRegularOrdersBatch",
    "isNativeBatchAccessDeniedError",
    "assertRegularOrderBatchSucceeded",
    "updateOrder",
    "getReplacementOrderAmount",
    "cancelOrders",
    "shouldPreserveStopLikeOrder",
    "shouldPreserveRecentRegularOrder",
    "isStopLikeOrder",
    "isStopOrder",
    "isPlanOrder",
    "noteRecentlyCanceledStopLikeOrders",
    "isRecentlyCanceledStopLikeOrder",
    "isRecentlyCanceledStopLikeOrderId",
    "filterRecentlyCanceledStopLikeOrders",
    "noteRejectedRegularOrder",
    "isRecentlyRejectedRegularOrder",
    "assertCommonSuccess",
    "assertRegularCancelSucceeded",
    "cancelSymbolOrders",
    "cancelAllOrders",
    "fetchLeverageForSymbol",
    "updateSymbolMetadata",
    "setLeverage",
    "changePositionMode",
    "changeMarginType",
    "updateFetchedLeverage",
    "formatChangeLeveragePayload",
    "deriveSymbolConfig",
    "getLeverageForOrder",
    "orderListEndpoint",
];
/// ts: MexcExchange.constructor; source field initializers precede body calls.
pub fn new_exchange<H: AssemblyHost>(
    h: &H,
    name: H::Value,
    opts: H::Value,
    store: H::Value,
) -> Result<H::Value, H::Error> {
    let owner = h.base_exchange(name, opts.clone(), store)?;
    for key in ["xhr", "publicWebsocket", "privateWebsocket"] {
        h.define_field(&owner, key, h.undefined())?;
    }
    h.define_field(
        &owner,
        "grafanaEndpointsEvery",
        h.constant("GRAFANA_ENDPOINTS_EVERY"),
    )?;
    h.define_field(
        &owner,
        "grafanaEndpointsPeriodic",
        h.constant("GRAFANA_ENDPOINTS_PERIODIC"),
    )?;
    h.define_field(&owner, "accountPositionMode", t(h, "hedged"))?;
    h.define_field(&owner, "symbolSettings", h.fresh_map())?;
    h.define_field(&owner, "regularOrderSubmitQueue", h.resolved_promise())?;
    h.define_field(&owner, "lastRegularOrderSubmitAt", h.number(0.))?;
    h.define_field(&owner, "regularOrderSubmitCooldownUntil", h.number(0.))?;
    h.define_field(&owner, "nativeBatchOrdersAvailable", h.undefined())?;
    for key in [
        "recentlyCanceledStopLikeOrderIds",
        "recentlyRejectedRegularOrderIds",
        "recentlyPlacedStopLikeOrderIds",
        "pendingStopSnapshotRefreshTimers",
    ] {
        h.define_field(&owner, key, h.fresh_map())?;
    }
    for key in EXCHANGE_METHOD_FIELDS {
        let value = h.member(key, &owner)?;
        h.define_field(&owner, key, value)?;
    }
    let xhr = invoke(h, owner.clone(), "createXhrWithSecondaryKey", || {
        Ok(vec![opts, h.constructor("createAPI")])
    })?;
    h.set_field(&owner, "xhr", xhr)?;
    let public = h.construct(h.constructor("MexcPublicWebsocket"), vec![owner.clone()])?;
    h.set_field(&owner, "publicWebsocket", public)?;
    let private = h.construct(h.constructor("MexcPrivateWebsocket"), vec![owner.clone()])?;
    h.set_field(&owner, "privateWebsocket", private)?;
    Ok(owner)
}
fn loaded<H: AssemblyHost>(h: &H, keys: &[&str]) -> Result<H::Value, H::Error> {
    let value = h.spread(&h.field(&f(h, "store")?, "loaded")?)?;
    for key in keys {
        h.define_field(&value, key, h.boolean(true))?;
    }
    Ok(value)
}
/// ts: MexcExchange.start.
pub fn start_exchange<H: AssemblyHost>(host: Rc<H>) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let external = await_value(
            h.as_ref(),
            own(h.as_ref(), "initExternalPublicData", || Ok(vec![])),
        )
        .await?;
        if !h.truthy(&external) {
            let markets =
                await_value(h.as_ref(), own(h.as_ref(), "fetchMarkets", || Ok(vec![]))).await?;
            if h.truthy(&f(h.as_ref(), "isDisposed")?) {
                return Ok(h.undefined());
            }
            invoke(h.as_ref(), f(h.as_ref(), "store")?, "update", || {
                Ok(vec![h.object(vec![
                    ("markets", markets.clone()),
                    ("loaded", loaded(h.as_ref(), &["markets"])?),
                ])])
            })?;
            let tickers =
                await_value(h.as_ref(), own(h.as_ref(), "fetchTickers", || Ok(vec![]))).await?;
            if h.truthy(&f(h.as_ref(), "isDisposed")?) {
                return Ok(h.undefined());
            }
            invoke(h.as_ref(), f(h.as_ref(), "store")?, "update", || {
                Ok(vec![h.object(vec![
                    ("tickers", tickers.clone()),
                    ("loaded", loaded(h.as_ref(), &["tickers"])?),
                ])])
            })?;
            invoke(
                h.as_ref(),
                f(h.as_ref(), "publicWebsocket")?,
                "connectAndSubscribe",
                || Ok(vec![]),
            )?;
            own(h.as_ref(), "log", || {
                let count =
                    h.minimum(h.field(&markets, "length")?, h.field(&tickers, "length")?)?;
                Ok(vec![template(
                    h.as_ref(),
                    "Loaded ",
                    count,
                    " MEXC markets",
                )?])
            })?;
        }
        if h.truthy(&f(h.as_ref(), "isPublicDataOnly")?) {
            invoke(h.as_ref(), f(h.as_ref(), "store")?, "update", || {
                Ok(vec![h.object(vec![(
                    "loaded",
                    loaded(h.as_ref(), &["balance", "positions", "orders"])?,
                )])])
            })?;
            return Ok(h.undefined());
        }
        invoke(
            h.as_ref(),
            f(h.as_ref(), "privateWebsocket")?,
            "connectAndSubscribe",
            || Ok(vec![]),
        )?;
        let position_result: Result<(), H::Error> = async {
            let mode = await_value(
                h.as_ref(),
                own(h.as_ref(), "fetchPositionMode", || Ok(vec![])),
            )
            .await?;
            h.set_field(&h.exchange(), "accountPositionMode", mode)?;
            invoke(h.as_ref(), f(h.as_ref(), "store")?, "setSetting", || {
                Ok(vec![
                    t(h.as_ref(), "isHedged"),
                    h.boolean(h.is_text(&f(h.as_ref(), "accountPositionMode")?, "hedged")),
                ])
            })?;
            Ok(())
        }
        .await;
        if let Err(error) = position_result {
            own(h.as_ref(), "log", || {
                let err = h.error_value(&error);
                let message = or(
                    h.as_ref(),
                    opt(
                        h.as_ref(),
                        opt(
                            h.as_ref(),
                            opt(h.as_ref(), err.clone(), "response")?,
                            "data",
                        )?,
                        "message",
                    )?,
                    || opt(h.as_ref(), err, "message"),
                )?;
                Ok(vec![
                    template(
                        h.as_ref(),
                        "Failed to load MEXC position mode: ",
                        message,
                        "",
                    )?,
                    t(h.as_ref(), "warning"),
                ])
            })?;
        }
        let pair = own(h.as_ref(), "fetchBalanceAndPositions", || Ok(vec![]))?;
        let cb = h.clone();
        let balance = invoke(h.as_ref(), pair, "then", || {
            Ok(vec![h.function(Rc::new(move |value| {
                let values = cb.destructure_fields(&value, &["balance", "positions"])?;
                if cb.truthy(&f(cb.as_ref(), "isDisposed")?) {
                    return Ok(cb.undefined());
                }
                invoke(cb.as_ref(), f(cb.as_ref(), "store")?, "update", || {
                    Ok(vec![cb.object(vec![
                        ("balance", values[0].clone()),
                        ("positions", values[1].clone()),
                        ("loaded", loaded(cb.as_ref(), &["balance", "positions"])?),
                    ])])
                })?;
                Ok(cb.undefined())
            }))])
        })?;
        let orders = own(h.as_ref(), "fetchOrders", || Ok(vec![]))?;
        let cb = h.clone();
        let orders = invoke(h.as_ref(), orders, "then", || {
            Ok(vec![h.function(Rc::new(move |orders| {
                if cb.truthy(&f(cb.as_ref(), "isDisposed")?) {
                    return Ok(cb.undefined());
                }
                invoke(cb.as_ref(), f(cb.as_ref(), "store")?, "update", || {
                    Ok(vec![cb.object(vec![
                        ("orders", orders),
                        ("loaded", loaded(cb.as_ref(), &["orders"])?),
                    ])])
                })?;
                Ok(cb.undefined())
            }))])
        })?;
        observe(h.promise_all(vec![balance, orders])).await?;
        await_value(h.as_ref(), own(h.as_ref(), "tick", || Ok(vec![]))).await?;
        own(h.as_ref(), "log", || {
            Ok(vec![t(h.as_ref(), "Ready to trade on MEXC")])
        })?;
        Ok(h.undefined())
    })
}
/// ts: MexcExchange.listenOHLCV.
pub fn listen_ohlcv<H: AssemblyHost>(
    h: &H,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&f(h, "externalPublicDataProvider")?) {
        invoke(
            h,
            f(h, "externalPublicDataProvider")?,
            "subscribeCandle",
            || Ok(vec![opts, callback]),
        )
    } else {
        invoke(h, f(h, "publicWebsocket")?, "listenOHLCV", || {
            Ok(vec![opts, callback])
        })
    }
}
/// ts: MexcExchange.listenOrderBook.
pub fn listen_order_book<H: AssemblyHost>(
    h: &H,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&f(h, "externalPublicDataProvider")?) {
        invoke(
            h,
            f(h, "externalPublicDataProvider")?,
            "subscribeOrderbook",
            || Ok(vec![symbol, callback]),
        )
    } else {
        invoke(h, f(h, "publicWebsocket")?, "listenOrderBook", || {
            Ok(vec![symbol, callback])
        })
    }
}
fn error_emit<H: AssemblyHost>(h: &H, message: H::Value) -> Result<H::Value, H::Error> {
    own(h, "emitAndLogError", || Ok(vec![message.clone()]))?;
    Err(h.new_error(h.abstract_string(&message)?))
}
fn stop_message<H: AssemblyHost>(
    h: &H,
    opts: &H::Value,
    prefix: &str,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    template(h, prefix, h.field(opts, "type")?, suffix)
}
fn clamp<H: AssemblyHost>(h: &H, opts: &H::Value, market: &H::Value) -> Result<H::Value, H::Error> {
    let leverage = own(h, "getLeverageForOrder", || Ok(vec![opts.clone()]))?;
    let leverage = h.maximum(
        leverage,
        h.field(&h.field(&h.field(market, "limits")?, "leverage")?, "min")?,
    )?;
    h.minimum(
        leverage,
        h.field(&h.field(&h.field(market, "limits")?, "leverage")?, "max")?,
    )
}
fn stop_error<H: AssemblyHost>(
    h: &H,
    error: H::Error,
    fallback: &str,
) -> Result<H::Value, H::Error> {
    let err = h.error_value(&error);
    let message = or(
        h,
        opt(
            h,
            opt(h, opt(h, err.clone(), "response")?, "data")?,
            "message",
        )?,
        || {
            or(
                h,
                opt(h, opt(h, opt(h, err.clone(), "response")?, "data")?, "msg")?,
                || or(h, opt(h, err, "message")?, || Ok(t(h, fallback))),
            )
        },
    )?;
    own(h, "emitAndLogError", || Ok(vec![message]))?;
    Err(error)
}
/// ts: MexcExchange#placePlanOrder.
pub fn place_plan_order<H: AssemblyHost>(
    host: Rc<H>,
    opts: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let market = market(h.as_ref(), &opts)?;
        if !h.truthy(&market) {
            return error_emit(
                h.as_ref(),
                template(
                    h.as_ref(),
                    "Market ",
                    h.field(&opts, "symbol")?,
                    " not found",
                )?,
            );
        }
        let trigger = nullish(h.as_ref(), h.field(&opts, "triggerPrice")?, || {
            h.field(&opts, "price")
        })?;
        if !h.finite(&trigger) || !h.truthy(&trigger) || !h.less(h.number(0.), trigger.clone())? {
            return error_emit(
                h.as_ref(),
                stop_message(
                    h.as_ref(),
                    &opts,
                    "MEXC ",
                    " requires a positive trigger price",
                )?,
            );
        }
        if h.equal(&h.field(&opts, "amount")?, &h.number(0.)) {
            return await_value(
                h.as_ref(),
                own(h.as_ref(), "placePositionStopOrder", || {
                    Ok(vec![opts, market, trigger])
                }),
            )
            .await;
        }
        let step = h.import(
            "toNumber",
            vec![
                opt(h.as_ref(), h.field(&market, "info")?, "volUnit")?,
                h.number(1.),
            ],
        )?;
        let vol = h.import(
            "toNumber",
            vec![
                h.import("adjust", vec![h.field(&opts, "amount")?, step])?,
                h.number(0.),
            ],
        )?;
        if !h.finite(&vol) || !h.less(h.number(0.), vol.clone())? {
            return error_emit(
                h.as_ref(),
                stop_message(
                    h.as_ref(),
                    &opts,
                    "MEXC ",
                    " requires a positive explicit amount",
                )?,
            );
        }
        let max = h.import(
            "toNumber",
            vec![
                nullish(
                    h.as_ref(),
                    opt(h.as_ref(), h.field(&market, "info")?, "maxVol")?,
                    || opt(h.as_ref(), h.field(&market, "info")?, "limitMaxVol"),
                )?,
                h.number(f64::INFINITY),
            ],
        )?;
        if h.finite(&max) && h.less(max.clone(), vol.clone())? {
            let message = stop_message(
                h.as_ref(),
                &opts,
                "MEXC ",
                " amount exceeds maximum native volume ",
            )?;
            return error_emit(
                h.as_ref(),
                template(h.as_ref(), "", message, &"").and_then(|v| {
                    let mut u = h.abstract_string(&v)?;
                    u.extend(h.abstract_string(&max)?);
                    Ok(h.text(u))
                })?,
            );
        }
        let leverage = clamp(h.as_ref(), &opts, &market)?;
        let config = or(
            h.as_ref(),
            invoke(h.as_ref(), f(h.as_ref(), "symbolSettings")?, "get", || {
                Ok(vec![h.field(&opts, "symbol")?])
            })?,
            || {
                own(h.as_ref(), "deriveSymbolConfig", || {
                    Ok(vec![h.field(&opts, "symbol")?])
                })
            },
        )?;
        let payload = h.omit_undefined(h.object(vec![
            ("symbol", h.field(&market, "id")?),
            ("vol", vol),
            ("leverage", leverage),
            (
                "side",
                h.field(
                    &h.indexed(&h.constant("REVERSE_ORDER_SIDE"), &h.field(&opts, "side")?)?,
                    "close",
                )?,
            ),
            (
                "openType",
                h.indexed(
                    &h.constant("REVERSE_MARGIN_TYPE"),
                    &h.field(&config, "marginType")?,
                )?,
            ),
            (
                "triggerPrice",
                template(
                    h.as_ref(),
                    "",
                    h.import(
                        "adjust",
                        vec![
                            trigger.clone(),
                            h.field(&h.field(&market, "precision")?, "price")?,
                        ],
                    )?,
                    "",
                )?,
            ),
            (
                "triggerType",
                own(h.as_ref(), "planOrderTriggerType", || {
                    Ok(vec![opts.clone()])
                })?,
            ),
            ("executeCycle", h.constant("MEXC_EXECUTE_CYCLE_SEVEN_DAYS")),
            (
                "orderType",
                h.indexed(&h.constant("REVERSE_ORDER_TYPE"), &t(h.as_ref(), "market"))?,
            ),
            (
                "trend",
                own(h.as_ref(), "stopOrderTrend", || Ok(vec![opts.clone()]))?,
            ),
            (
                "positionMode",
                h.indexed(
                    &h.constant("REVERSE_POSITION_MODE"),
                    &h.field(&config, "positionMode")?,
                )?,
            ),
            ("reduceOnly", h.boolean(true)),
        ]))?;
        let result: Result<H::Value, H::Error> = async {
            let response = await_value(
                h.as_ref(),
                invoke(h.as_ref(), f(h.as_ref(), "xhr")?, "post", || {
                    Ok(vec![h.endpoint("PLAN_ORDER_PLACE"), payload])
                }),
            )
            .await?;
            let data = h.field(&response, "data")?;
            let inner = h.import("extractData", vec![data.clone()])?;
            let id = h.import(
                "toIntegerString",
                vec![nullish(
                    h.as_ref(),
                    opt(h.as_ref(), inner, "orderId")?,
                    || h.import("extractData", vec![data]),
                )?],
            )?;
            if !h.truthy(&id) {
                return Err(h.new_error(
                    "MEXC plan order placement returned no order id"
                        .encode_utf16()
                        .collect(),
                ));
            }
            own(h.as_ref(), "addOptimisticStopOrder", || {
                Ok(vec![
                    opts.clone(),
                    market,
                    id.clone(),
                    trigger,
                    t(h.as_ref(), "__mexcPlanOrder"),
                ])
            })?;
            own(h.as_ref(), "scheduleStopOrderSnapshotRefresh", || {
                Ok(vec![h.field(&opts, "symbol")?])
            })?;
            Ok(id)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(err) => stop_error(h.as_ref(), err, "MEXC plan order placement failed"),
        }
    })
}
/// ts: MexcExchange#placePositionStopOrder.
pub fn place_position_stop_order<H: AssemblyHost>(
    host: Rc<H>,
    opts: H::Value,
    market: H::Value,
    trigger: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let side = t(
            h.as_ref(),
            if h.is_text(&h.field(&opts, "side")?, "sell") {
                "long"
            } else {
                "short"
            },
        );
        let mut position = own(h.as_ref(), "findOpenPosition", || {
            Ok(vec![
                h.field(&opts, "symbol")?,
                side.clone(),
                h.field(&f(h.as_ref(), "store")?, "positions")?,
            ])
        })?;
        if !h.truthy(&position)
            || !h.truthy(&own(h.as_ref(), "nativePositionId", || {
                Ok(vec![position.clone()])
            })?)
        {
            let receiver = h.exchange();
            let callee = h.field(&receiver, "findOpenPosition")?;
            let symbol = h.field(&opts, "symbol")?;
            let positions = await_value(
                h.as_ref(),
                own(h.as_ref(), "fetchPositions", || {
                    Ok(vec![h.array(vec![h.field(&opts, "symbol")?])])
                }),
            )
            .await?;
            position = h.call(callee, receiver, vec![symbol, side.clone(), positions])?;
        }
        if !h.truthy(&position) || h.less_equal(h.field(&position, "contracts")?, h.number(0.))? {
            let mut message = h.abstract_string(&stop_message(
                h.as_ref(),
                &opts,
                "MEXC full-position ",
                " requires an open ",
            )?)?;
            message.extend(h.abstract_string(&side)?);
            message.extend(" position on ".encode_utf16());
            message.extend(h.abstract_string(&h.field(&opts, "symbol")?)?);
            return error_emit(h.as_ref(), h.text(message));
        }
        let id = own(h.as_ref(), "nativePositionId", || {
            Ok(vec![position.clone()])
        })?;
        if !h.truthy(&id) {
            let message = stop_message(
                h.as_ref(),
                &opts,
                "MEXC full-position ",
                " requires a native position id for ",
            )?;
            let mut s = h.abstract_string(&message)?;
            s.extend(h.abstract_string(&h.field(&opts, "symbol")?)?);
            return error_emit(h.as_ref(), h.text(s));
        }
        let step = h.import(
            "toNumber",
            vec![
                opt(h.as_ref(), h.field(&market, "info")?, "volUnit")?,
                h.number(1.),
            ],
        )?;
        let native = nullish(
            h.as_ref(),
            opt(h.as_ref(), h.field(&position, "extra")?, "contractsNative")?,
            || {
                nullish(
                    h.as_ref(),
                    opt(h.as_ref(), h.field(&position, "info")?, "holdVol")?,
                    || {
                        nullish(
                            h.as_ref(),
                            opt(h.as_ref(), h.field(&position, "info")?, "positionVol")?,
                            || {
                                nullish(
                                    h.as_ref(),
                                    opt(h.as_ref(), h.field(&position, "info")?, "vol")?,
                                    || opt(h.as_ref(), h.field(&position, "info")?, "size"),
                                )
                            },
                        )
                    },
                )
            },
        )?;
        let native = h.import("toNumber", vec![native, h.number(0.)])?;
        let vol = h.import(
            "toNumber",
            vec![h.import("adjust", vec![native, step])?, h.number(0.)],
        )?;
        if !h.finite(&vol) || !h.less(h.number(0.), vol.clone())? {
            return error_emit(
                h.as_ref(),
                stop_message(
                    h.as_ref(),
                    &opts,
                    "MEXC full-position ",
                    " requires a positive native position volume",
                )?,
            );
        }
        let trend = own(h.as_ref(), "stopOrderTrend", || Ok(vec![opts.clone()]))?;
        let price = template(
            h.as_ref(),
            "",
            h.import(
                "adjust",
                vec![
                    trigger.clone(),
                    h.field(&h.field(&market, "precision")?, "price")?,
                ],
            )?,
            "",
        )?;
        let payload = h.omit_undefined(h.object(vec![
            ("positionId", id),
            ("vol", vol),
            (
                "lossTrend",
                if h.is_text(&h.field(&opts, "type")?, "stop_market") {
                    trend.clone()
                } else {
                    h.constant("MEXC_TRIGGER_TREND_LATEST_PRICE")
                },
            ),
            (
                "profitTrend",
                if h.is_text(&h.field(&opts, "type")?, "take_profit_market") {
                    trend
                } else {
                    h.constant("MEXC_TRIGGER_TREND_LATEST_PRICE")
                },
            ),
            (
                "stopLossPrice",
                if h.is_text(&h.field(&opts, "type")?, "stop_market") {
                    price.clone()
                } else {
                    h.undefined()
                },
            ),
            (
                "takeProfitPrice",
                if h.is_text(&h.field(&opts, "type")?, "take_profit_market") {
                    price
                } else {
                    h.undefined()
                },
            ),
            ("profitLossVolType", t(h.as_ref(), "SAME")),
            ("volType", h.number(2.)),
            (
                "takeProfitType",
                if h.is_text(&h.field(&opts, "type")?, "take_profit_market") {
                    h.number(0.)
                } else {
                    h.undefined()
                },
            ),
            (
                "stopLossType",
                if h.is_text(&h.field(&opts, "type")?, "stop_market") {
                    h.number(0.)
                } else {
                    h.undefined()
                },
            ),
        ]))?;
        let result: Result<H::Value, H::Error> = async {
            let response = await_value(
                h.as_ref(),
                invoke(h.as_ref(), f(h.as_ref(), "xhr")?, "post", || {
                    Ok(vec![h.endpoint("STOP_ORDER_PLACE"), payload])
                }),
            )
            .await?;
            let data = h.import("extractData", vec![h.field(&response, "data")?])?;
            let row = if h.is_array(&data) {
                h.field(&data, "0")?
            } else {
                data.clone()
            };
            let scalar = if h.is_string_or_number(&data) {
                data
            } else {
                h.undefined()
            };
            let id = h.import(
                "toIntegerString",
                vec![nullish(
                    h.as_ref(),
                    opt(h.as_ref(), row.clone(), "id")?,
                    || nullish(h.as_ref(), opt(h.as_ref(), row, "orderId")?, || Ok(scalar)),
                )?],
            )?;
            if !h.truthy(&id) {
                return Err(h.new_error(
                    "MEXC position TP/SL placement returned no order id"
                        .encode_utf16()
                        .collect(),
                ));
            }
            own(h.as_ref(), "addOptimisticStopOrder", || {
                Ok(vec![
                    opts.clone(),
                    market,
                    id.clone(),
                    trigger,
                    t(h.as_ref(), "__mexcStopOrder"),
                ])
            })?;
            own(h.as_ref(), "scheduleStopOrderSnapshotRefresh", || {
                Ok(vec![h.field(&opts, "symbol")?])
            })?;
            Ok(id)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(err) => stop_error(h.as_ref(), err, "MEXC position TP/SL placement failed"),
        }
    })
}
/// ts: MexcExchange.updateOrder.
pub fn update_order<H: AssemblyHost>(
    host: Rc<H>,
    input: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let values = h.destructure_fields(&input, &["order", "update"])?;
        let order = values[0].clone();
        let update = values[1].clone();
        let next = h.object(vec![
            ("symbol", h.field(&order, "symbol")?),
            ("type", h.field(&order, "type")?),
            ("side", h.field(&order, "side")?),
            (
                "amount",
                if h.has_property(&update, "amount")? {
                    h.field(&update, "amount")?
                } else {
                    own(h.as_ref(), "getReplacementOrderAmount", || {
                        Ok(vec![order.clone()])
                    })?
                },
            ),
            (
                "price",
                if h.has_property(&update, "price")? {
                    h.field(&update, "price")?
                } else {
                    h.field(&order, "price")?
                },
            ),
            ("reduceOnly", h.field(&order, "reduceOnly")?),
            ("timeInForce", h.field(&order, "timeInForce")?),
        ]);
        await_value(
            h.as_ref(),
            own(h.as_ref(), "cancelOrders", || {
                Ok(vec![h.array(vec![order])])
            }),
        )
        .await?;
        await_value(h.as_ref(), own(h.as_ref(), "placeOrder", || Ok(vec![next]))).await
    })
}
