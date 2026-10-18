//! COIN-M reconciliation state and request-weight guards.
use super::{
    Rt,
    ops::{
        enum_value, global, install_async, install_sync, optional, or, request, require_array,
        store, text, v,
    },
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    install_async(rt, "loadAccountMaxLeverage", load_overlay)?;
    install_async(rt, "start", start)?;
    install_async(rt, "_startPrivate", start_private)?;
    install_async(rt, "_startPublicDataOnly", start_public)?;
    install_async(rt, "retryOrdersSnapshot", retry_orders)?;
    install_async(rt, "tick", tick)?;
    install_sync(rt, "listenOrderBook", listen_book)?;
    install_sync(rt, "listenOHLCV", listen_candle)?;
    let parent_dispose = rt.this.get("dispose")?;
    let weak = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        "dispose",
        Js::function("dispose", move |_, _| {
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            parent_dispose.call(rt.this.clone(), vec![])?;
            rt.call_path("publicWebsocket.dispose", vec![])?;
            rt.call_path("privateWebsocket.dispose", vec![])?;
            Ok(Js::Undefined)
        }),
    )?;
    install_sync(rt, "markPositionStateUpdatedByWebsocket", mark_updated)?;
    install_sync(rt, "getPositionStateVersion", version)?;
    install_sync(rt, "isBackgroundReconciliationPaused", paused)?;
    install_sync(rt, "getResponseHeader", header)?;
    install_sync(rt, "pauseBackgroundReconciliation", pause)?;
    install_sync(rt, "observeRequestWeight", observe)?;
    install_sync(rt, "installRequestWeightGuards", guards)?;
    Ok(())
}
fn mark_updated(rt: &Rt, _: &[Js]) -> Result<Js, Thrown> {
    let next = rt.this.get("positionStateVersion")?.to_number()? + 1.0;
    rt.this.set("positionStateVersion", Js::Number(next))?;
    Ok(Js::Undefined)
}
fn version(rt: &Rt, _: &[Js]) -> Result<Js, Thrown> {
    rt.this.get("positionStateVersion")
}
fn paused(rt: &Rt, _: &[Js]) -> Result<Js, Thrown> {
    Ok(Js::Bool(
        rt.now()
            < rt.this
                .get("backgroundReconciliationPausedUntil")?
                .to_number()?,
    ))
}
fn header(_: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let headers = arg(a, 0);
    let name = arg(a, 1);
    if !headers.truthy() {
        return Ok(Js::Undefined);
    }
    if headers.get("get")?.is_callable() {
        return headers.call_method("get", vec![name]);
    }
    let value = headers.get_property(&name)?;
    if !value.is_nullish() {
        return Ok(value);
    }
    let value = headers.get_property(&name.call_method("toLowerCase", vec![])?)?;
    if !value.is_nullish() {
        return Ok(value);
    }
    headers.get_property(&name.call_method("toUpperCase", vec![])?)
}
fn pause(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let until = global(
        rt,
        "Math.max",
        vec![
            rt.this.get("backgroundReconciliationPausedUntil")?,
            Js::Number(rt.now() + arg(a, 0).to_number()?),
        ],
    )?;
    rt.this.set("backgroundReconciliationPausedUntil", until)?;
    Ok(Js::Undefined)
}
fn observe(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let response = arg(a, 0);
    let status = optional(&response, "status")?;
    if status.strict_equals(&Js::Number(429.0)) || status.strict_equals(&Js::Number(418.0)) {
        let retry = rt
            .call_path(
                "getResponseHeader",
                vec![optional(&response, "headers")?, Js::str("retry-after")],
            )?
            .to_number()?;
        rt.call_path(
            "pauseBackgroundReconciliation",
            vec![Js::Number(if retry.is_finite() && retry > 0.0 {
                retry * 1000.0
            } else {
                60_000.0
            })],
        )?;
        return Ok(Js::Undefined);
    }
    let weight = rt
        .call_path(
            "getResponseHeader",
            vec![
                optional(&response, "headers")?,
                Js::str("x-mbx-used-weight-1m"),
            ],
        )?
        .to_number()?;
    if weight.is_finite() && weight >= 1920.0 {
        rt.call_path("pauseBackgroundReconciliation", vec![Js::Number(60_000.0)])?;
    }
    Ok(Js::Undefined)
}
fn guards(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let mut seen: Vec<Js> = vec![];
    for client in arg(a, 0).iterable_values()? {
        if seen.iter().any(|prior| prior.strict_equals(&client)) {
            continue;
        }
        seen.push(client.clone());
        let weak = std::rc::Rc::downgrade(&rt.shared);
        let fulfilled = Js::function("observeRequestWeight", move |_, a| {
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            let response = arg(&a, 0);
            rt.call_path("observeRequestWeight", vec![response.clone()])?;
            Ok(response)
        });
        let weak = std::rc::Rc::downgrade(&rt.shared);
        let rejected = Js::function("observeRequestWeight", move |_, a| {
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            let error = arg(&a, 0);
            rt.call_path("observeRequestWeight", vec![optional(&error, "response")?])?;
            Err(Thrown::new(error))
        });
        client
            .get("interceptors")?
            .get("response")?
            .call_method("use", vec![fulfilled, rejected])?;
    }
    Ok(Js::Undefined)
}

fn load_overlay(rt: Rt, _: Vec<Js>) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        let loader = Js::function("loadMaxLeverage", move |_, _| {
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            Ok(rt.clone().spawn(Box::pin(async move {
                let data = request(&rt, "xhr", "get", "LEVERAGE_BRACKET", vec![]).await?;
                let rows = require_array(&rt.global("ENDPOINTS")?.get("LEVERAGE_BRACKET")?, data)?;
                let values = Js::new_map();
                for row in rows {
                    let symbol = row.get("symbol")?;
                    let max = v(
                        &rt,
                        &optional(&optional(&row, "brackets")?, "0")?,
                        "initialLeverage",
                    )?
                    .to_number()?;
                    if max > 0.0 {
                        values.call_method("set", vec![symbol, Js::Number(max)])?;
                    }
                }
                Ok(Js::function("applyMaxLeverage", move |_, a| {
                    let values = values.clone();
                    arg(&a, 0).call_method(
                        "map",
                        vec![Js::function("marketLeverage", move |_, a| {
                            let market = arg(&a, 0);
                            let max = values.call_method("get", vec![market.get("symbol")?])?;
                            if !max.truthy() {
                                return Ok(market);
                            }
                            let result = market.spread_object()?;
                            let limits = market.get("limits")?.spread_object()?;
                            let leverage =
                                market.get("limits")?.get("leverage")?.spread_object()?;
                            leverage.set("max", max)?;
                            limits.set("leverage", leverage)?;
                            result.set("limits", limits)?;
                            Ok(result)
                        })],
                    )
                }))
            })))
        });
        safe_cex_exchange::js_runtime::promise::await_js(
            &rt.exchange_runtime(),
            rt.call_path("loadAccountMarketOverlay", vec![loader])?,
        )
        .await
    })
}
fn start(rt: Rt, _: Vec<Js>) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move {
        use safe_cex_exchange::js_runtime::promise::await_js;
        if rt.this.get("needsAccountMarketOverlay")?.truthy() {
            await_js(
                &rt.exchange_runtime(),
                rt.call_path("loadAccountMaxLeverage", vec![])?,
            )
            .await?;
            if rt.this.get("isDisposed")?.truthy() {
                return Ok(Js::Undefined);
            }
        }
        let external = await_js(
            &rt.exchange_runtime(),
            rt.call_path("initExternalPublicData", vec![])?,
        )
        .await?;
        if !external.truthy() {
            let markets = await_js(
                &rt.exchange_runtime(),
                rt.call_path("fetchMarkets", vec![])?,
            )
            .await?;
            if rt.this.get("isDisposed")?.truthy() {
                return Ok(Js::Undefined);
            }
            let loaded = store(&rt, "loaded")?.spread_object()?;
            loaded.set("markets", Js::Bool(true))?;
            rt.call_path(
                "store.update",
                vec![Js::object(vec![
                    ("markets", markets.clone()),
                    ("loaded", loaded),
                ])],
            )?;
            let tickers = await_js(
                &rt.exchange_runtime(),
                rt.call_path("fetchTickers", vec![])?,
            )
            .await?;
            if rt.this.get("isDisposed")?.truthy() {
                return Ok(Js::Undefined);
            }
            let count = global(
                &rt,
                "Math.min",
                vec![tickers.get("length")?, markets.get("length")?],
            )?;
            rt.call_path(
                "log",
                vec![Js::str(&format!(
                    "Loaded {} Binance COIN-M markets",
                    text(&count)?
                ))],
            )?;
            let loaded = store(&rt, "loaded")?.spread_object()?;
            loaded.set("tickers", Js::Bool(true))?;
            rt.call_path(
                "store.update",
                vec![Js::object(vec![("tickers", tickers), ("loaded", loaded)])],
            )?;
            rt.call_path("publicWebsocket.connectAndSubscribe", vec![])?;
        }
        let method = if rt.this.get("isPublicDataOnly")?.truthy() {
            "_startPublicDataOnly"
        } else {
            "_startPrivate"
        };
        await_js(&rt.exchange_runtime(), rt.call_path(method, vec![])?).await?;
        Ok(Js::Undefined)
    })
}
fn start_private(rt: Rt, _: Vec<Js>) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move {
        use safe_cex_exchange::js_runtime::promise::await_js;
        rt.call_path("privateWebsocket.connectAndSubscribe", vec![])?;
        let mode = await_js(
            &rt.exchange_runtime(),
            rt.call_path("fetchPositionMode", vec![])?,
        )
        .await?;
        rt.call_path("store.setSetting", vec![Js::str("isHedged"), mode])?;
        await_js(&rt.exchange_runtime(), rt.call_path("tick", vec![])?).await?;
        if rt.this.get("isDisposed")?.truthy() {
            return Ok(Js::Undefined);
        }
        rt.call_path("log", vec![Js::str("Ready to trade on Binance COIN-M")])?;
        let orders = await_js(&rt.exchange_runtime(), rt.call_path("fetchOrders", vec![])?).await?;
        if rt.this.get("isDisposed")?.truthy() || rt.this.get("ordersSnapshotStale")?.truthy() {
            return Ok(Js::Undefined);
        }
        rt.call_path("log", vec![Js::str("Loaded Binance COIN-M orders")])?;
        let loaded = store(&rt, "loaded")?.spread_object()?;
        loaded.set("orders", Js::Bool(true))?;
        rt.call_path(
            "store.update",
            vec![Js::object(vec![("orders", orders), ("loaded", loaded)])],
        )?;
        Ok(Js::Undefined)
    })
}
fn start_public(rt: Rt, _: Vec<Js>) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move {
        rt.call_path(
            "log",
            vec![Js::str("Ready to watch public data on Binance COIN-M")],
        )?;
        let loaded = store(&rt, "loaded")?.spread_object()?;
        for key in ["orders", "balance", "positions"] {
            loaded.set(key, Js::Bool(true))?;
        }
        rt.call_path(
            "store.update",
            vec![Js::object(vec![
                ("orders", Js::array(vec![])),
                ("positions", Js::array(vec![])),
                (
                    "balance",
                    Js::object(vec![
                        ("used", Js::Number(0.0)),
                        ("free", Js::Number(0.0)),
                        ("total", Js::Number(0.0)),
                        ("upnl", Js::Number(0.0)),
                    ]),
                ),
                ("loaded", loaded),
            ])],
        )?;
        Ok(Js::Undefined)
    })
}
fn retry_orders(rt: Rt, _: Vec<Js>) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move {
        use safe_cex_exchange::js_runtime::promise::await_js;
        if rt.now() - rt.this.get("lastOrdersRetryAt")?.to_number()? < 10_000.0 {
            return Ok(Js::Undefined);
        }
        rt.this.set("lastOrdersRetryAt", Js::Number(rt.now()))?;
        let orders = await_js(&rt.exchange_runtime(), rt.call_path("fetchOrders", vec![])?).await?;
        if rt.this.get("isDisposed")?.truthy() || rt.this.get("ordersSnapshotStale")?.truthy() {
            return Ok(Js::Undefined);
        }
        let loaded = store(&rt, "loaded")?.spread_object()?;
        loaded.set("orders", Js::Bool(true))?;
        rt.call_path(
            "store.update",
            vec![Js::object(vec![("orders", orders), ("loaded", loaded)])],
        )?;
        Ok(Js::Undefined)
    })
}
fn tick(rt: Rt, _: Vec<Js>) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move {
        use safe_cex_exchange::js_runtime::promise::await_js;
        if rt.this.get("isDisposed")?.truthy() {
            return Ok(Js::Undefined);
        }
        let now = rt.now();
        let interval = if rt
            .this
            .get("privateWebsocket")?
            .get("isConnected")?
            .truthy()
        {
            25_000.0
        } else {
            3_000.0
        };
        let risk = !rt
            .call_path("isBackgroundReconciliationPaused", vec![])?
            .truthy()
            && (rt
                .this
                .get("lastPositionRiskReconciliationAt")?
                .strict_equals(&Js::Number(0.0))
                || now
                    - rt.this
                        .get("lastPositionRiskReconciliationAt")?
                        .to_number()?
                    >= 75_000.0);
        let account = !rt
            .call_path("isBackgroundReconciliationPaused", vec![])?
            .truthy()
            && (rt
                .this
                .get("lastAccountReconciliationAt")?
                .strict_equals(&Js::Number(0.0))
                || now - rt.this.get("lastAccountReconciliationAt")?.to_number()? >= interval);
        let result = async {
            if rt.this.get("ordersSnapshotStale")?.truthy() {
                await_js(
                    &rt.exchange_runtime(),
                    rt.call_path("retryOrdersSnapshot", vec![])?,
                )
                .await?;
            }
            if rt.this.get("isDisposed")?.truthy() {
                return Ok::<_, Thrown>(false);
            }
            if risk {
                let version = rt.call_path("getPositionStateVersion", vec![])?;
                let positions = await_js(
                    &rt.exchange_runtime(),
                    rt.call_path("fetchPositions", vec![])?,
                )
                .await?;
                if version.strict_equals(&rt.call_path("getPositionStateVersion", vec![])?) {
                    rt.call_path(
                        "store.update",
                        vec![Js::object(vec![("positions", positions)])],
                    )?;
                }
                rt.this
                    .set("lastPositionRiskReconciliationAt", Js::Number(rt.now()))?;
            }
            if account
                && !rt
                    .call_path("isBackgroundReconciliationPaused", vec![])?
                    .truthy()
            {
                let version = rt.call_path("getPositionStateVersion", vec![])?;
                let result = await_js(
                    &rt.exchange_runtime(),
                    rt.call_path("fetchPositionsAndBalance", vec![])?,
                )
                .await?;
                let balance = result.get("balance")?;
                let positions = result.get("positions")?;
                if rt.this.get("isDisposed")?.truthy() {
                    return Ok(false);
                }
                let update = Js::object(vec![("balance", balance)]);
                if version.strict_equals(&rt.call_path("getPositionStateVersion", vec![])?) {
                    update.set("positions", positions)?;
                }
                let loaded = store(&rt, "loaded")?.spread_object()?;
                loaded.set("balance", Js::Bool(true))?;
                loaded.set("positions", Js::Bool(true))?;
                update.set("loaded", loaded)?;
                rt.call_path("store.update", vec![update])?;
                rt.this
                    .set("lastAccountReconciliationAt", Js::Number(rt.now()))?;
            }
            Ok(true)
        }
        .await;
        match result {
            Ok(false) => return Ok(Js::Undefined),
            Err(error) => {
                rt.call_path("emitAndLogError", vec![optional(&error.value, "message")?])?;
            }
            _ => {}
        }
        if !rt.this.get("isDisposed")?.truthy() {
            let weak = std::rc::Rc::downgrade(&rt.shared);
            let callback = Js::function("tick", move |_, _| {
                safe_cex_exchange::js_runtime::scope::scope_of(&weak)?.call_path("tick", vec![])
            });
            global(&rt, "loop", vec![callback, Js::Number(3_000.0)])?;
        }
        Ok(Js::Undefined)
    })
}
fn listen_book(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    if rt.this.get("externalPublicDataProvider")?.truthy() {
        rt.call_path("externalPublicDataProvider.subscribeOrderbook", a.to_vec())
    } else {
        rt.call_path("publicWebsocket.listenOrderBook", a.to_vec())
    }
}
fn listen_candle(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    use safe_cex_exchange::js_runtime::promise::await_js;
    if rt.this.get("externalPublicDataProvider")?.truthy() {
        return rt.call_path("externalPublicDataProvider.subscribeCandle", a.to_vec());
    }
    let opts = arg(a, 0);
    let callback = arg(a, 1);
    let disposed = std::rc::Rc::new(std::cell::Cell::new(false));
    let unsubscribe = rt.call_path(
        "publicWebsocket.listenOHLCV",
        vec![opts.clone(), callback.clone()],
    )?;
    let seed = opts.spread_object()?;
    seed.set("limit", Js::Number(1.0))?;
    let pending = rt.call_path("fetchOHLCV", vec![seed])?;
    let state = disposed.clone();
    let scope = rt.clone();
    rt.spawn(Box::pin(async move {
        let result = async {
            let candles = await_js(&scope.exchange_runtime(), pending).await?;
            if state.get() || scope.this.get("isDisposed")?.truthy() {
                return Ok::<_, Thrown>(());
            }
            let latest =
                candles.get_property(&Js::Number(candles.get("length")?.to_number()? - 1.0))?;
            if latest.truthy() {
                callback.call(Js::Undefined, vec![latest])?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            scope.call_path(
                "log",
                vec![
                    Js::str(&format!(
                        "Unable to seed Binance COIN-M live candle: {}",
                        text(&or(optional(&error.value, "message")?, || Ok(error
                            .value
                            .clone()))?)?
                    )),
                    enum_value(&scope, "LogSeverity", "Debug")?,
                ],
            )?;
        }
        Ok(Js::Undefined)
    }));
    Ok(Js::function("unsubscribe", move |_, _| {
        disposed.set(true);
        unsubscribe.call(Js::Undefined, vec![])?;
        Ok(Js::Undefined)
    }))
}
