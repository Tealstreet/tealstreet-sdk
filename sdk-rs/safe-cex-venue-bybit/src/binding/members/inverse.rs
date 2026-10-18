//! Standalone inverse receiver with dedicated USD-contract writes and mappings.
use crate::{
    binding::{
        host::{get_key, text},
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, arg, from_typed, to_json, to_typed},
    },
    inverse_read_surface as reads, variant_account_refresh as refresh,
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
fn fields(value: &Js) -> Result<Vec<Js>, Thrown> {
    value
        .array_items()
        .ok_or_else(|| Thrown::type_error("value is not iterable"))?
        .into_iter()
        .map(|v| Ok(v.unwrap_or(Js::Undefined)))
        .collect()
}
async fn get(rt: &Rt, endpoint: &str, params: Js) -> Result<Js, Thrown> {
    await_js(
        &rt.exchange_runtime(),
        rt.this.get("xhr")?.call_method(
            "get",
            vec![Js::str(endpoint), Js::object(vec![("params", params)])],
        )?,
    )
    .await?
    .get("data")
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    super::batch::install_inverse(rt)?;
    super::inverse_orders::install(rt)?;
    super::inverse_controls::install(rt)?;
    rt.this
        .define("supportedCategories", Js::array(vec![Js::str("inverse")]))?;
    method(rt, "getCategoryForSymbol", |rt, a| {
        let symbol = text(&arg(&a, 0))?;
        let parent = venue(&rt)?;
        let missing = {
            let markets = parent.base.store.markets();
            !markets.is_empty() && !markets.iter().any(|m| m.symbol == symbol)
        };
        if missing {
            rt.shared
                .runtime
                .console("warn", &format!("Market {symbol} not found"));
        }
        Ok(Js::str("inverse"))
    })?;
    method(rt, "getSettleCoinsForCategory", |rt, a| {
        let category = arg(&a, 0);
        if category.strict_equals(&Js::str("inverse")) {
            return Ok(Js::array(vec![Js::Undefined]));
        }
        if category.strict_equals(&Js::str("linear")) {
            return Ok(Js::array(
                if rt
                    .this
                    .get("accountType")?
                    .strict_equals(&Js::str("UNIFIED"))
                {
                    vec![Js::str("USDT"), Js::str("USDC")]
                } else {
                    vec![Js::str("USDT")]
                },
            ));
        }
        let message = format!("Unknown category {}", text(&category)?);
        rt.this
            .call_method("emitAndLogError", vec![Js::str(&message)])?;
        Err(error(message))
    })?;
    method(rt, "getTickerSymbolsForSubscription", |rt, a| {
        let available: Vec<String> = to_typed(&arg(&a, 0))?;
        from_typed(
            &venue(&rt)?
                .base
                .get_ticker_symbols_for_subscription(&available),
        )
    })?;
    method(rt, "getTickerSymbolsForPublicStream", |rt, _| {
        let markets: Vec<safe_cex_types::Market> =
            to_typed(&rt.this.get("store")?.get("markets")?)?;
        let available = crate::variant_accessors::inverse_get_ticker_symbols_for_public_stream(
            &markets,
            <[String]>::to_vec,
        );
        rt.this.call_method(
            "getTickerSymbolsForSubscription",
            vec![from_typed(&available)?],
        )
    })?;
    method(rt, "remainingInStoreUnits", |_, a| {
        Ok(super::inverse_mapping::remaining(
            arg(&a, 0).to_number()?,
            arg(&a, 1).to_number()?,
        ))
    })?;
    method(rt, "mapOrder", |rt, a| {
        super::inverse_mapping::map_order(&rt, &arg(&a, 0))
    })?;
    method(rt, "mapPosition", |rt, a| {
        super::inverse_mapping::map_position(&rt, &arg(&a, 0))
    })?;
    method(rt, "updateOrder", |rt, a| {
        super::amend::update_inverse_order(&rt, arg(&a, 0))
    })?;
    method(rt, "setLeverage", |rt, a| {
        super::controls::set_inverse_leverage(&rt, arg(&a, 0), arg(&a, 1))
    })?;
    method(rt, "fetchPositionMode", |rt, a| {
        super::orders::fetch_inverse_position_mode(&rt, arg(&a, 0))
    })?;
    method(rt, "changePositionMode", |rt, a| {
        let hedged = arg(&a, 0);
        Ok(rt.clone().spawn(Box::pin(async move {
            // Source deliberately discards symbol and changes linear USDT.
            await_js(
                &rt.exchange_runtime(),
                rt.this.call_method(
                    "changePositionModeHelper",
                    vec![hedged, Js::str("linear"), Js::str("USDT")],
                )?,
            )
            .await
        })))
    })?;
    method(rt, "fetchRawBalance", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::fetch_raw_balance(&rt, rt.this.clone()).await
        })))
    })?;
    method(rt, "formatBalance", |rt, a| {
        reads::format_balance(&rt, rt.this.clone(), arg(&a, 0))
    })?;
    method(rt, "fetchOrdersHelper", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::fetch_orders_helper(&rt, rt.this.clone(), arg(&a, 0), arg(&a, 1)).await
        })))
    })?;
    method(rt, "fetchOrders", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let mut out = vec![];
            for category in fields(&rt.this.get("supportedCategories")?)? {
                for coin in fields(
                    &rt.this
                        .call_method("getSettleCoinsForCategory", vec![category.clone()])?,
                )? {
                    out.extend(fields(
                        &await_js(
                            &rt.exchange_runtime(),
                            rt.this
                                .call_method("fetchOrdersHelper", vec![category.clone(), coin])?,
                        )
                        .await?,
                    )?);
                }
            }
            rt.this
                .call_method("preserveRecentlyPlacedOrders", vec![Js::array(out)])
        })))
    })?;
    method(rt, "fetchPositions", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let symbols = arg(&a, 0);
            if symbols.truthy() && symbols.get("length")?.to_number()? > 0. {
                return await_js(
                    &rt.exchange_runtime(),
                    rt.this.call_method("fetchPositionsHelper", vec![symbols])?,
                )
                .await;
            }
            let mut out = vec![];
            for category in fields(&rt.this.get("supportedCategories")?)? {
                for coin in fields(
                    &rt.this
                        .call_method("getSettleCoinsForCategory", vec![category.clone()])?,
                )? {
                    out.extend(fields(
                        &await_js(
                            &rt.exchange_runtime(),
                            rt.this.call_method(
                                "fetchPositionsHelper",
                                vec![Js::Undefined, category.clone(), coin],
                            )?,
                        )
                        .await?,
                    )?);
                }
            }
            Ok(Js::array(out))
        })))
    })?;
    method(rt, "fetchPositionsHelper", |rt, a| {
        Ok(crate::core_snapshot_readers::fetch_positions_helper(
            Rc::new(rt.clone()),
            rt.this.clone(),
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
            Js::Bool(false),
        ))
    })?;
    method(rt, "fetchMarkets", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            reads::fetch_markets(&rt, rt.this.clone()).await
        })))
    })?;
    method(rt, "fetchMarketsHelper", fetch_markets_helper)?;
    method(rt, "parseTickers", |rt, a| parse_tickers(&rt, arg(&a, 0)))?;
    method(rt, "fetchTickersRawHelper", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let data = get(
                &rt,
                "/v5/market/tickers",
                Js::object(vec![("category", arg(&a, 0))]),
            )
            .await?;
            let message = get_key(&data, "retMsg")?;
            if !message.strict_equals(&Js::str("OK")) {
                rt.this.call_method("emitAndLogError", vec![message])?;
                return Ok(Js::array(vec![Js::Null]));
            }
            data.get("result")?.get("list")
        })))
    })?;
    method(rt, "fetchTickersRaw", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let mut pending = vec![];
            for category in fields(&rt.this.get("supportedCategories")?)? {
                pending.push(
                    rt.this
                        .call_method("fetchTickersRawHelper", vec![category])?,
                );
            }
            let all = rt
                .global("Promise.all")?
                .call(rt.global("Promise")?, vec![Js::array(pending)])?;
            let mut tickers = vec![];
            for rows in fields(&await_js(&rt.exchange_runtime(), all).await?)? {
                tickers.extend(fields(&rows)?);
            }
            if tickers.iter().any(|row| row.strict_equals(&Js::Null)) {
                rt.this
                    .call_method("emitAndLogError", vec![Js::str("Failed to fetch tickers")])?;
                return Ok(Js::Null);
            }
            Ok(Js::array(tickers))
        })))
    })?;
    method(rt, "fetchTickersWithoutParse", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let raw = await_js(
                &rt.exchange_runtime(),
                rt.this.call_method("fetchTickersRaw", vec![])?,
            )
            .await?;
            if rt.this.get("isDisposed")?.truthy() {
                Ok(Js::array(vec![]))
            } else {
                Ok(raw)
            }
        })))
    })?;
    method(rt, "fetchTickers", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            let raw = await_js(
                &rt.exchange_runtime(),
                rt.this.call_method("fetchTickersRaw", vec![])?,
            )
            .await?;
            if rt.this.get("isDisposed")?.truthy() {
                rt.this.get("store")?.get("tickers")
            } else {
                rt.this.call_method("parseTickers", vec![raw])
            }
        })))
    })?;
    method(rt, "fetchAndCacheApiKeyInfo", fetch_and_cache_api_key_info)?;
    method(rt, "fetchOrdersPositionsAndBalance", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::inverse_account_refresh(&rt, rt.this.clone()).await
        })))
    })?;
    method(rt, "fetchAndSetPrivateSnapshot", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::inverse_private_snapshot(&rt, rt.this.clone()).await
        })))
    })?;
    method(rt, "fetchAndSetPrivateSnapshotHelper", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::inverse_snapshot_helper(&rt, &rt.this, arg(&a, 0)).await
        })))
    })?;
    method(rt, "tick", |rt, _| {
        Ok(rt.clone().spawn(Box::pin(async move {
            refresh::inverse_tick(&rt, &rt.this).await
        })))
    })?;
    method(rt, "start", start)?;
    Ok(())
}

// ts: BybitInverseExchange.fetchAndCacheApiKeyInfo — the cached user-data key is
// inverse-specific, while the maintained fetch and VIP table bodies are shared.
fn fetch_and_cache_api_key_info(rt: Rt, _: Vec<Js>) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin(async move {
        let result = async {
            await_js(
                &rt.exchange_runtime(),
                rt.this.call_method("fetchApiKeyInfo", vec![])?,
            )
            .await?;
            if rt.this.get("apiKeyInfo")?.truthy() {
                super::startup::update_batch_limit_based_on_vip_level(&rt)?;
                // Same typed user-data channel as the primary cache adapter.
                let mut data = serde_json::Map::new();
                data.insert(
                    "bybitinverseApiKeyInfo".into(),
                    to_json(&rt.this.get("apiKeyInfo")?)?,
                );
                venue(&rt)?
                    .base
                    .store
                    .update_user_data(Some(serde_json::Value::Object(data)))
                    .map_err(|error| {
                        Thrown::from(safe_cex_exchange::runtime::ExchangeError::from(error))
                    })?;
            }
            let info = rt.this.get("apiKeyInfo")?;
            let vip = if info.is_nullish() {
                Js::Undefined
            } else {
                info.get("vipLevel")?
            };
            let vip = if vip.truthy() {
                vip
            } else {
                Js::str("Unknown")
            };
            rt.this.call_method(
                "log",
                vec![Js::str(&format!(
                    "API key info cached. VIP Level: {}, Batch Limit: {}",
                    text(&vip)?,
                    text(&rt.this.get("batchLimit")?)?,
                ))],
            )?;
            Ok::<_, Thrown>(())
        }
        .await;
        if result.is_err() {
            rt.this.call_method(
                "log",
                vec![Js::str(
                    "Failed to fetch API key info, continuing without it",
                )],
            )?;
        }
        Ok(Js::Undefined)
    })))
}

// ts: BybitInverseExchange.fetchMarketsHelper — both requests are sequential and
// both complete before either retMsg is checked. There is no Trading retry.
fn fetch_markets_helper(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let category = arg(&a, 0);
    Ok(rt.clone().spawn(Box::pin(async move {
        let fees = if rt.this.get("isPublicDataOnly")?.truthy() {
            Js::array(vec![])
        } else {
            await_js(&rt.exchange_runtime(), rt.this.call_method("fetchFees", vec![category.clone()])?).await?
        };
        let data = get(
            &rt,
            "/v5/market/instruments-info",
            Js::object(vec![
                ("category", category.clone()),
                ("limit", Js::Number(1000.)),
            ]),
        )
        .await?;
        let prelaunch = get(
            &rt,
            "/v5/market/instruments-info",
            Js::object(vec![
                ("category", category.clone()),
                ("limit", Js::Number(1000.)),
                ("status", Js::str("PreLaunch")),
            ]),
        )
        .await?;
        for response in [&data, &prelaunch] {
            let message = get_key(response, "retMsg")?;
            if !message.strict_equals(&Js::str("OK")) {
                rt.this.call_method("emitAndLogError", vec![message])?;
                return Ok(Js::array(vec![Js::Null]));
            }
        }
        let mut rows = fields(&data.get("result")?.get("list")?)?;
        rows.extend(fields(&prelaunch.get("result")?.get("list")?)?);
        let mut out = vec![];
        for row in rows {
            let symbol = row.get("symbol")?;
            let mut fee = Js::Undefined;
            for candidate in fields(&fees)? {
                if candidate.get("symbol")?.strict_equals(&symbol) {
                    fee = candidate;
                    break;
                }
            }
            let lot = row.get("lotSizeFilter")?;
            let leverage = row.get("leverageFilter")?;
            let status = row.get("status")?;
            let market = Js::object(vec![
                ("id", symbol.clone()),
                ("symbol", symbol),
                ("base", row.get("baseCoin")?),
                ("quote", row.get("quoteCoin")?),
                (
                    "active",
                    Js::Bool(
                        status.strict_equals(&Js::str("Trading"))
                            || status.strict_equals(&Js::str("PreLaunch")),
                    ),
                ),
                (
                    "precision",
                    Js::object(vec![
                        ("amount", Js::Number(lot.get("qtyStep")?.parse_float()?)),
                        (
                            "price",
                            Js::Number(row.get("priceFilter")?.get("tickSize")?.parse_float()?),
                        ),
                    ]),
                ),
                (
                    "limits",
                    Js::object(vec![
                        (
                            "amount",
                            Js::object(vec![
                                ("min", Js::Number(lot.get("minOrderQty")?.parse_float()?)),
                                ("max", Js::Number(lot.get("maxOrderQty")?.parse_float()?)),
                            ]),
                        ),
                        (
                            "leverage",
                            Js::object(vec![
                                (
                                    "min",
                                    Js::Number(leverage.get("minLeverage")?.parse_float()?),
                                ),
                                (
                                    "max",
                                    Js::Number(leverage.get("maxLeverage")?.parse_float()?),
                                ),
                            ]),
                        ),
                    ]),
                ),
                (
                    "isInverse",
                    Js::Bool(category.strict_equals(&Js::str("inverse"))),
                ),
                (
                    "isExpiring",
                    Js::Bool(Js::Number(row.get("deliveryTime")?.parse_float()?).truthy()),
                ),
            ]);
            if fee.truthy() {
                market.set(
                    "takerFee",
                    Js::Number(fee.get("takerFeeRate")?.parse_float()?),
                )?;
                market.set(
                    "makerFee",
                    Js::Number(fee.get("makerFeeRate")?.parse_float()?),
                )?;
            }
            market.set("info", row)?;
            out.push(market);
        }
        let mut admitted = Vec::with_capacity(out.len());
        for market in out {
            let base = market.get("base")?;
            let quote = market.get("quote")?;
            let id = market.get("id")?;
            let symbol = market.get("symbol")?;
            let typed_string = |value: &Js| {
                matches!(value, Js::String(_)) && to_typed::<String>(value).is_ok()
            };
            if typed_string(&base) && typed_string(&quote) && typed_string(&id) && typed_string(&symbol) {
                admitted.push(market);
            } else {
                let message = format!(
                    "Refused Bybit market row id={id:?} symbol={symbol:?}: base={base:?} quote={quote:?}: expected strings"
                );
                rt.this.call_method("log", vec![Js::str(&message), Js::str("warning")])?;
            }
        }
        Ok(Js::array(admitted))
    })))
}

// ts: BybitInverseExchange.parseTickers — native binary64 values never pass JSON.
fn parse_tickers(rt: &Rt, raw: Js) -> Result<Js, Thrown> {
    let store = rt.this.get("store")?;
    if !raw.truthy() {
        return store.get("tickers");
    }
    let markets = store.get("marketsBySymbol")?;
    let mut tickers = vec![];
    for index in 0..raw.get("length")?.to_number()? as usize {
        let row = raw.get(&index.to_string())?;
        let market = markets.get_key(&row.get("symbol")?.to_js_string()?)?;
        if !market.truthy() {
            continue;
        }
        let last = row.get("lastPrice")?.parse_float()?;
        let volume = row.get("volume24h")?.parse_float()?;
        let expiring = market.get("isExpiring")?.truthy();
        tickers.push(Js::object(vec![
            ("id", market.get("id")?),
            ("symbol", market.get("symbol")?),
            ("bid", Js::Number(row.get("bid1Price")?.parse_float()?)),
            ("ask", Js::Number(row.get("ask1Price")?.parse_float()?)),
            ("last", Js::Number(last)),
            ("mark", Js::Number(row.get("markPrice")?.parse_float()?)),
            ("index", Js::Number(row.get("indexPrice")?.parse_float()?)),
            (
                "percentage",
                Js::Number(row.get("price24hPcnt")?.parse_float()? * 100.),
            ),
            (
                "openInterest",
                Js::Number(row.get("openInterest")?.parse_float()?),
            ),
            (
                "fundingRate",
                Js::Number(if expiring {
                    0.
                } else {
                    row.get("fundingRate")?.parse_float()?
                }),
            ),
            (
                "fundingTime",
                Js::Number(if expiring {
                    0.
                } else {
                    row.get("nextFundingTime")?.parse_float()?
                }),
            ),
            ("volume", Js::Number(volume)),
            ("quoteVolume", Js::Number(volume * last)),
        ]));
    }
    Ok(Js::array(tickers))
}

fn publish_loaded(this: &Js, mut values: Vec<(&str, Js)>, flags: &[&str]) -> Result<(), Thrown> {
    let store = this.get("store")?;
    let loaded = store.get("loaded")?.spread_object()?;
    for flag in flags {
        loaded.set(flag, Js::Bool(true))?;
    }
    values.push(("loaded", loaded));
    store.call_method("update", vec![Js::object(values)])?;
    Ok(())
}

// ts: BybitInverseExchange.start
fn start(rt: Rt, _: Vec<Js>) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin(async move {
        let this = &rt.this;
        await_js(&rt.exchange_runtime(), rt.shared.globals.get("virtualClock")?.call_method("start", vec![])?).await?;
        this.set("batchLimit", Js::Number(10.))?;
        let public_only = this.get("isPublicDataOnly")?.truthy();
        if this.get("needsAccountMarketOverlay")?.truthy() {
            let overlay_rt = rt.clone();
            let loader = Js::function("inverseAccountFeeOverlay", move |_, _| {
                let rt = overlay_rt.clone();
                Ok(rt.clone().spawn(Box::pin(async move {
                    let mut pending = vec![];
                    for category in fields(&rt.this.get("supportedCategories")?)? { pending.push(rt.this.call_method("fetchFees", vec![category])?); }
                    let all = rt.global("Promise.all")?.call(rt.global("Promise")?, vec![Js::array(pending)])?;
                    let fees = await_js(&rt.exchange_runtime(), all).await?.call_method("flat", vec![])?;
                    rt.global("bybitFeeOverlay")?.call(Js::Undefined, vec![fees])
                })))
            });
            await_js(&rt.exchange_runtime(), this.call_method("loadAccountMarketOverlay", vec![loader])?).await?;
            if this.get("isDisposed")?.truthy() { return Ok(Js::Undefined); }
        }
        let external = await_js(&rt.exchange_runtime(), this.call_method("initExternalPublicData", vec![])?).await?;
        let probe = if public_only { Js::Null } else {
            this.call_method("fetchMarginAccountInfos", vec![])?.call_method("then", vec![
                Js::function("privateProbeSuccess", |_, _| Ok(Js::Undefined)),
                Js::function("privateProbeFailure", |_, a| {
                    let failure = arg(&a, 0);
                    let message = if failure.is_nullish() { Js::Undefined } else { failure.get("message")? };
                    let message = text(&if message.truthy() { message } else { failure })?;
                    Ok(Js::str(if message.is_empty() { "request failed" } else { &message }))
                }),
            ])?
        };
        if !public_only { this.call_method("fetchAndCacheApiKeyInfo", vec![])?.call_method("catch", vec![Js::function("inverseKeyCacheCatch", |_, _| Ok(Js::Undefined))])?; }
        if !external.truthy() {
            let markets = await_js(&rt.exchange_runtime(), this.call_method("fetchMarkets", vec![])?).await?;
            if !this.get("isDisposed")?.truthy() {
                this.call_method("log", vec![Js::str(&format!("Loaded {} BybitInverse markets", text(&markets.get("length")?)?))])?;
                publish_loaded(this, vec![("markets", markets)], &["markets"])?;
                this.get("publicInverseWebsocket")?.call_method("connectAndSubscribe", vec![])?;
            }
            let raw = await_js(&rt.exchange_runtime(), this.call_method("fetchTickersWithoutParse", vec![])?).await?;
            let tickers = this.call_method("parseTickers", vec![raw])?;
            if this.get("isDisposed")?.truthy() { return Ok(Js::Undefined); }
            publish_loaded(this, vec![("tickers", tickers)], &["tickers"])?;
        }
        let failure = if probe.truthy() { await_js(&rt.exchange_runtime(), probe).await? } else { Js::Undefined };
        if failure.truthy() { this.call_method("log", vec![Js::str(&format!("BybitInverse market data is ready, but the private API was rejected ({}) — trading is unavailable", text(&failure)?)), Js::str("warning")])?; }
        else { this.call_method("log", vec![Js::str("Ready to trade on BybitInverse")])?; }
        if public_only { publish_loaded(this, vec![], &["balance", "positions", "orders"])?; return Ok(Js::Undefined); }
        this.get("privateWebsocket")?.call_method("connectAndSubscribe", vec![])?;
        await_js(&rt.exchange_runtime(), this.call_method("fetchOrdersPositionsAndBalance", vec![])?).await?;
        this.call_method("tick", vec![])?;
        Ok(Js::Undefined)
    })))
}
