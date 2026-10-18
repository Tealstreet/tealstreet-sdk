//! Spot receiver members, using the separately witnessed Spot bodies.
use crate::binding::{
    host::text,
    promise::await_js,
    rt::Rt,
    value::{Js, Thrown, arg, from_typed, to_json, to_typed},
};
use crate::{
    spot_exchange_reads as reads, spot_order_writes as writes, variant_account_refresh as refresh,
};
use std::rc::Rc;

fn method(
    rt: &Rt,
    name: &'static str,
    body: fn(Rt, Vec<Js>) -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    let owned = rt.clone();
    rt.this.define(
        name,
        Js::function(name, move |_, args| body(owned.clone(), args)),
    )
}
fn error(message: impl AsRef<str>) -> Thrown {
    Thrown::error("Error", message.as_ref())
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    rt.this.define("isSpot", Js::Bool(true))?;
    method(rt, "getTickerSymbolsForSubscription", |rt, a| {
        let available: Vec<String> = to_typed(&arg(&a, 0))?;
        let base = rt
            .shared
            .base
            .get()
            .ok_or_else(|| error("Bybit base unavailable"))?;
        from_typed(&base.get_ticker_symbols_for_subscription(&available))
    })?;
    method(rt, "getTickerSymbolsForPublicStream", |rt, _| {
        let markets: Vec<safe_cex_types::Market> =
            to_typed(&rt.this.get("store")?.get("markets")?)?;
        let available = crate::variant_accessors::spot_get_ticker_symbols_for_public_stream(
            &markets,
            <[String]>::to_vec,
        );
        rt.this.call_method(
            "getTickerSymbolsForSubscription",
            vec![from_typed(&available)?],
        )
    })?;
    method(rt, "start", start)?;
    method(rt, "fetchPositions", |rt, _| {
        Ok(rt.spawn(Box::pin(async { Ok(Js::array(vec![])) })))
    })?;
    for name in ["setLeverage", "changeMarginType", "changePositionMode"] {
        method(rt, name, |rt, _| {
            Ok(rt.spawn(Box::pin(async { Err(error("Not implemented")) })))
        })?;
    }
    method(rt, "fetchRawBalance", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::fetch_raw_balance(&rt, &rt.this).await
        })))
    })?;
    method(rt, "formatBalance", |rt, a| {
        reads::format_balance(&rt, &rt.this, arg(&a, 0))
    })?;
    method(rt, "fetchOrders", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(
            async move { reads::fetch_orders(&rt, &rt.this).await },
        )))
    })?;
    method(rt, "fetchMarkets", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::fetch_markets(&rt, &rt.this).await
        })))
    })?;
    method(rt, "fetchMarketsHelper", |rt, a| {
        let category = text(&arg(&a, 0))?;
        Ok(rt.clone().spawn(Box::pin(async move {
            let client = rt.this.get("xhr")?;
            let data = crate::rest_markets::fetch_markets_helper(
                &category,
                false,
                rt.this.get("isPublicDataOnly")?.truthy(),
                |endpoint, params| {
                    let request = client.call_method(
                        "get",
                        vec![
                            Js::str(endpoint),
                            Js::object(vec![("params", crate::binding::value::from_json(&params))]),
                        ],
                    );
                    {
                        let await_runtime = rt.exchange_runtime();
                        async move {
                            let response =
                                await_js(&await_runtime, request.map_err(|e| e.message())?)
                                    .await
                                    .map_err(|e| e.message())?;
                            to_json(&response.get("data").map_err(|e| e.message())?)
                                .map_err(|e| e.message())
                        }
                    }
                },
                |message| {
                    rt.this
                        .call_method(
                            "emitAndLogError",
                            vec![message.map_or(Js::Undefined, crate::binding::value::from_json)],
                        )
                        .map(|_| ())
                        .map_err(|e| e.message())
                },
            )
            .await
            .map_err(error)?;
            from_typed(&data)
        })))
    })?;
    method(rt, "parseTickers", |rt, a| {
        let store = rt.this.get("store")?;
        let raw = arg(&a, 0);
        let raw = if raw.truthy() {
            Some(
                to_json(&raw)?
                    .as_array()
                    .cloned()
                    .ok_or_else(|| error("rawTickers is not iterable"))?,
            )
        } else {
            None
        };
        let markets: Vec<safe_cex_types::Market> = to_typed(&store.get("markets")?)?;
        let cached: Vec<safe_cex_types::Ticker> = to_typed(&store.get("tickers")?)?;
        from_typed(
            &crate::exchange_state::parse_spot_tickers(raw.as_deref(), &markets, &cached)
                .map_err(error)?,
        )
    })?;
    method(rt, "fetchTickersRaw", |rt, _| {
        Ok(rt.clone().spawn(Box::pin({
            let await_runtime = rt.exchange_runtime();
            async move {
                let response = await_js(
                    &await_runtime,
                    rt.this.get("xhr")?.call_method(
                        "get",
                        vec![
                            Js::str("/v5/market/tickers"),
                            Js::object(vec![(
                                "params",
                                Js::object(vec![("category", Js::str("spot"))]),
                            )]),
                        ],
                    )?,
                )
                .await?;
                let data = response.get("data")?;
                let message = crate::binding::host::get_key(&data, "retMsg")?;
                if !message.strict_equals(&Js::str("OK")) {
                    rt.this.call_method("emitAndLogError", vec![message])?;
                    return Ok(Js::array(vec![Js::Null]));
                }
                data.get("result")?.get("list")
            }
        })))
    })?;
    method(rt, "fetchTickers", |rt, _| {
        super::markets::fetch_tickers(&rt)
    })?;
    method(rt, "fetchTickersWithoutParse", |rt, _| {
        super::markets::fetch_tickers_without_parse(&rt)
    })?;
    method(rt, "mapOrder", |rt, a| {
        reads::map_order(&rt, &rt.this, arg(&a, 0))
    })?;
    method(rt, "placeOrder", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::place_order(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "placeOrders", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            writes::place_orders(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "updateOrder", |rt, a| {
        let input = arg(&a, 0);
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::update_order(&rt, &rt.this, input).await
        })))
    })?;
    method(rt, "cancelOrders", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            writes::cancel_orders(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "cancelOrdersHelper", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            writes::cancel_orders_helper(&rt, &rt.this, arg(&a, 0), arg(&a, 1)).await
        })))
    })?;
    method(rt, "cancelSymbolOrders", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            writes::cancel_symbol_orders(&rt, &rt.this, arg(&a, 0), arg(&a, 1)).await
        })))
    })?;
    method(rt, "getOrderExecutionParams", |rt, a| {
        let opts = to_typed(&arg(&a, 0))?;
        let value = crate::execution_guards::spot_get_order_execution_params(&opts, |message| {
            rt.this
                .call_method("emitAndLogError", vec![Js::str(message)])
                .map(|_| ())
                .map_err(|e| Rc::new(e.to_exchange_error()))
        })
        .map_err(|e| error(&e.message))?;
        from_typed(&value)
    })?;
    method(rt, "recordRecentlyPlacedOrder", |rt, a| {
        super::lifecycle::record_variant_recently_placed_order(&rt, arg(&a, 0))
    })?;
    method(rt, "recordRecentlyPlacedOrderFromOpts", |rt, a| {
        writes::record_recently_placed_order_from_opts(
            &rt,
            &rt.this,
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
        )
        .map(|_| Js::Undefined)
    })?;
    method(rt, "applyLocalOrderUpdateAfterRestWrite", |rt, a| {
        writes::apply_local_order_update_after_rest_write(&rt, &rt.this, arg(&a, 0), arg(&a, 1))
            .map(|_| Js::Undefined)
    })?;
    method(rt, "_hasFullSizedPositionOppositeSide", |rt, a| {
        writes::has_full_sized_position_opposite_side(
            &rt,
            &rt.this,
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
        )
        .map(Js::Bool)
    })?;
    method(rt, "placeStopLossOrTakeProfit", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            writes::place_stop_loss_or_take_profit(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "placeTrailingStopLoss", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            writes::place_trailing_stop_loss(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "getOrderPositionIdx", |rt, _| {
        Ok(rt.spawn(Box::pin(async { Ok(Js::Number(0.)) })))
    })?;
    method(rt, "getStopOrderPositionIdx", |rt, a| {
        let opts = arg(&a, 0).spread_object()?;
        opts.delete(&"reduceOnly".encode_utf16().collect::<Vec<_>>());
        let pending = rt.this.call_method("getOrderPositionIdx", vec![opts])?;
        Ok(rt.spawn(Box::pin({
            let await_runtime = rt.exchange_runtime();
            async move {
                let index = text(&await_js(&await_runtime, pending).await?)?;
                Js::object(vec![
                    ("0", Js::Number(0.)),
                    ("1", Js::Number(2.)),
                    ("2", Js::Number(1.)),
                ])
                .get(&index)
            }
        })))
    })?;
    method(rt, "fetchOrdersPositionsAndBalance", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::spot_account_refresh(&rt, rt.this.clone()).await
        })))
    })?;
    method(rt, "fetchAndSetPrivateSnapshot", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::spot_private_snapshot(&rt, rt.this.clone()).await
        })))
    })?;
    method(rt, "fetchAndSetPrivateSnapshotHelper", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::spot_snapshot_helper(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "tick", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(
            async move { refresh::spot_tick(&rt, &rt.this).await },
        )))
    })?;
    Ok(())
}

// ts: BybitSpotExchange.start — preserve the source's sequential awaited array operands.
pub(crate) fn start(rt: Rt, _: Vec<Js>) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin({ let await_runtime = rt.exchange_runtime(); async move {
        let this = &rt.this;
        await_js(&await_runtime, rt.shared.globals.get("virtualClock")?.call_method("start", vec![])?).await?;
        let public_only = this.get("isPublicDataOnly")?.truthy();
        let external = await_js(&await_runtime, this.call_method("initExternalPublicData", vec![])?).await?;
        let probe = if public_only { Js::Null } else {
            this.call_method("fetchMarginAccountInfos", vec![])?.call_method("then", vec![
                Js::function("privateProbeSuccess", |_, _| Ok(Js::Undefined)),
                Js::function("privateProbeFailure", |_, args| {
                    let failure = arg(&args, 0);
                    let message = if failure.is_nullish() { Js::Undefined } else { failure.get("message")? };
                    let message = text(&if message.truthy() { message } else { failure })?;
                    Ok(Js::str(if message.is_empty() { "request failed" } else { &message }))
                }),
            ])?
        };
        if !external.truthy() {
            let markets = await_js(&await_runtime, this.call_method("fetchMarkets", vec![])?).await?;
            if !this.get("isDisposed")?.truthy() {
                this.call_method("log", vec![Js::str(&format!("Loaded {} Bybit markets", text(&markets.get("length")?)?))])?;
                publish_loaded(this, vec![("markets", markets)], &["markets"])?;
                this.get("publicWebsocket")?.call_method("connectAndSubscribe", vec![])?;
            }
            let raw = await_js(&await_runtime, this.call_method("fetchTickersWithoutParse", vec![])?).await?;
            let tickers = this.call_method("parseTickers", vec![raw])?;
            if this.get("isDisposed")?.truthy() { return Ok(Js::Undefined); }
            publish_loaded(this, vec![("tickers", tickers)], &["tickers"])?;
        }
        let failure = if probe.truthy() { await_js(&await_runtime, probe).await? } else { Js::Undefined };
        if failure.truthy() {
            this.call_method("log", vec![Js::str(&format!("Bybit market data is ready, but the private API was rejected ({}) — trading is unavailable", text(&failure)?)), Js::str("warning")])?;
        } else { this.call_method("log", vec![Js::str("Ready to trade on Bybit")])?; }
        if public_only {
            publish_loaded(this, vec![], &["balance", "positions", "orders"])?;
            return Ok(Js::Undefined);
        }
        this.get("privateWebsocket")?.call_method("connectAndSubscribe", vec![])?;
        await_js(&await_runtime, this.call_method("fetchOrdersPositionsAndBalance", vec![])?).await?;
        this.call_method("tick", vec![])?;
        Ok(Js::Undefined)
    } })))
}
fn publish_loaded(
    this: &Js,
    mut fields: Vec<(&'static str, Js)>,
    flags: &[&str],
) -> Result<(), Thrown> {
    let store = this.get("store")?;
    let loaded = store.get("loaded")?.spread_object()?;
    for flag in flags {
        loaded.set(flag, Js::Bool(true))?;
    }
    fields.push(("loaded", loaded));
    store.call_method("update", vec![Js::object(fields)])?;
    Ok(())
}
