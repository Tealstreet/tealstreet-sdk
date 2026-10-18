//! binanceinverse.exchange.ts read methods; raw response graphs remain native.
use crate::{Rt, ops::*};
use safe_cex_exchange::js_runtime::{
    promise::JsFuture,
    value::{Js, Thrown, arg},
};

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    install_async(rt, "fetchMarkets", fetch_markets)?;
    install_async(rt, "fetchTickers", fetch_tickers)?;
    install_async(rt, "fetchPositions", fetch_positions)?;
    install_async(rt, "fetchPositionsAndBalance", fetch_positions_and_balance)?;
    install_async(rt, "fetchOrders", fetch_orders)?;
    install_async(rt, "fetchPositionMode", fetch_position_mode)?;
    install_async(rt, "validateAccount", validate_account)?;
    install_async(rt, "fetchPublicTrades", fetch_public_trades)?;
    install_async(rt, "fetchOHLCV", fetch_ohlcv)?;
    install_sync(rt, "nativeContractsToBase", native_contracts_to_base)?;
    install_sync(
        rt,
        "nativeContractsToNotional",
        native_contracts_to_notional,
    )?;
    install_sync(rt, "mapPosition", map_position)?;
    install_sync(rt, "getAssetUsdPrice", asset_price)?;
    install_sync(rt, "usdValueForAssetBalances", usd_balances)?;
    Ok(())
}
fn fetch_markets(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
            let data = request(&rt, "publicXhr", "get", "MARKETS", vec![]).await?;
            let symbols = require_array(
                &Js::str("/dapi/v1/exchangeInfo.symbols"),
                optional(&data, "symbols")?,
            )?;
            let brackets = if rt.this.get("isPublicDataOnly")?.truthy() {
                Js::array(vec![])
            } else {
                request(&rt, "xhr", "get", "LEVERAGE_BRACKET", vec![]).await?
            };
            let brackets =
                require_array(&rt.global("ENDPOINTS")?.get("LEVERAGE_BRACKET")?, brackets)?;
            let mut markets = vec![];
            for market in symbols {
                if !v(&rt, &market, "contractType")?.strict_equals(&Js::str("PERPETUAL"))
                    || !v(&rt, &market, "contractStatus")?.strict_equals(&Js::str("TRADING"))
                {
                    continue;
                }
                let filters = market.get("filters")?.array_values().unwrap_or_default();
                let find = |kind: &str| -> Result<Js, Thrown> {
                    for f in &filters {
                        if v(&rt, f, "filterType")?.strict_equals(&Js::str(kind)) {
                            return Ok(f.clone());
                        }
                    }
                    Ok(Js::Undefined)
                };
                let price = find("PRICE_FILTER")?;
                let lot = find("LOT_SIZE")?;
                let market_lot = find("MARKET_LOT_SIZE")?;
                let mut bracket = Js::Undefined;
                for b in &brackets {
                    if b.get("symbol")?.strict_equals(&market.get("symbol")?) {
                        bracket = b.clone();
                        break;
                    }
                }
                let bracket = optional(&optional(&bracket, "brackets")?, "0")?;
                let base = v(&rt, &market, "baseAsset")?;
                let quote = or(v(&rt, &market, "quoteAsset")?, || Ok(Js::str("USD")))?;
                let margin = or(v(&rt, &market, "marginAsset")?, || Ok(base.clone()))?;
                let contract_size = parse(
                    &rt,
                    or(v(&rt, &market, "contractSize")?, || Ok(Js::str("1")))?,
                )?;
                let amount = or(Js::Number(parse(&rt, v(&rt, &lot, "stepSize")?)?), || {
                    Ok(Js::Number(1.0))
                })?;
                let price = Js::Number(parse(&rt, v(&rt, &price, "tickSize")?)?);
                let min1 = or(Js::Number(parse(&rt, v(&rt, &lot, "minQty")?)?), || {
                    Ok(Js::Number(1.0))
                })?;
                let min2 = or(
                    Js::Number(parse(&rt, v(&rt, &market_lot, "minQty")?)?),
                    || Ok(Js::Number(1.0)),
                )?;
                let max1 = or(Js::Number(parse(&rt, v(&rt, &lot, "maxQty")?)?), || {
                    Ok(Js::Number(f64::INFINITY))
                })?;
                let max2 = or(
                    Js::Number(parse(&rt, v(&rt, &market_lot, "maxQty")?)?),
                    || Ok(Js::Number(f64::INFINITY)),
                )?;
                markets.push(Js::object(vec![
                    (
                        "id",
                        Js::str(&format!(
                            "{}/{}:{}",
                            text(&base)?,
                            text(&quote)?,
                            text(&margin)?
                        )),
                    ),
                    ("symbol", market.get("symbol")?),
                    ("base", base),
                    ("quote", quote),
                    (
                        "active",
                        Js::Bool(
                            market
                                .get("contractStatus")?
                                .strict_equals(&Js::str("TRADING")),
                        ),
                    ),
                    (
                        "precision",
                        Js::object(vec![("amount", amount), ("price", price)]),
                    ),
                    (
                        "limits",
                        Js::object(vec![
                            (
                                "amount",
                                Js::object(vec![
                                    ("min", global(&rt, "Math.max", vec![min1, min2])?),
                                    ("max", global(&rt, "Math.min", vec![max1, max2])?),
                                ]),
                            ),
                            (
                                "leverage",
                                Js::object(vec![
                                    ("min", Js::Number(1.0)),
                                    (
                                        "max",
                                        or(v(&rt, &bracket, "initialLeverage")?, || {
                                            Ok(Js::Number(1.0))
                                        })?,
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("contractSize", Js::Number(contract_size)),
                    ("isInverse", Js::Bool(true)),
                    ("info", market),
                ]));
            }
            Ok::<_, Thrown>(Js::array(markets))
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                rt.call_path(
                    "emitAndLogError",
                    vec![error.value.clone(), error_message(&error.value)?],
                )?;
                store(&rt, "markets")
            }
        }
    })
}
fn fetch_tickers(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
            // Start all three requests before awaiting any, preserving Promise.all.
            let xhr = rt.this.get("publicXhr")?;
            let endpoints = rt.global("ENDPOINTS")?;
            let mut requests = vec![];
            for key in ["TICKERS_24H", "TICKERS_BOOK", "TICKERS_PRICE"] {
                requests.push(xhr.call_method("get", vec![endpoints.get(key)?])?);
            }
            let all = global(&rt, "Promise.all", vec![Js::array(requests)])?;
            let data = safe_cex_exchange::js_runtime::promise::await_js(all).await?;
            let daily = require_array(&endpoints.get("TICKERS_24H")?, data.get("0")?.get("data")?)?;
            let books =
                require_array(&endpoints.get("TICKERS_BOOK")?, data.get("1")?.get("data")?)?;
            let prices = require_array(
                &endpoints.get("TICKERS_PRICE")?,
                data.get("2")?.get("data")?,
            )?;
            let markets = store(&rt, "marketsBySymbol")?;
            let mut output = vec![];
            for book in books {
                let market = markets.get_property(&book.get("symbol")?)?;
                let mut d = Js::Undefined;
                let mut p = Js::Undefined;
                for row in &daily {
                    if row.get("symbol")?.strict_equals(&book.get("symbol")?) {
                        d = row.clone();
                        break;
                    }
                }
                for row in &prices {
                    if row.get("symbol")?.strict_equals(&book.get("symbol")?) {
                        p = row.clone();
                        break;
                    }
                }
                if !market.truthy() || !d.truthy() || !p.truthy() {
                    continue;
                }
                let row = Js::object(vec![
                    ("id", market.get("id")?),
                    ("symbol", market.get("symbol")?),
                ]);
                for (key, owner, field) in [
                    ("bid", &book, "bidPrice"),
                    ("ask", &book, "askPrice"),
                    ("last", &d, "lastPrice"),
                    ("mark", &p, "markPrice"),
                    ("index", &p, "indexPrice"),
                    ("percentage", &d, "priceChangePercent"),
                    ("fundingRate", &p, "lastFundingRate"),
                    ("fundingTime", &p, "nextFundingTime"),
                    ("volume", &d, "volume"),
                ] {
                    row.set(key, Js::Number(parse(&rt, v(&rt, owner, field)?)?))?;
                }
                row.set(
                    "quoteVolume",
                    Js::Number(parse(
                        &rt,
                        or(v(&rt, &d, "baseVolume")?, || {
                            or(v(&rt, &d, "quoteVolume")?, || Ok(Js::str("0")))
                        })?,
                    )?),
                )?;
                row.set("openInterest", Js::Number(0.0))?;
                output.push(row);
            }
            Ok::<_, Thrown>(Js::array(output))
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                rt.call_path(
                    "emitAndLogError",
                    vec![error.value.clone(), error_message(&error.value)?],
                )?;
                store(&rt, "tickers")
            }
        }
    })
}
fn native_contracts_to_base(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let symbol = arg(a, 0);
    let market = store(rt, "marketsBySymbol")?.get_property(&symbol)?;
    let ticker = store(rt, "tickersBySymbol")?.get_property(&symbol)?;
    let fallback = if arg(a, 2).is_undefined() {
        Js::Number(0.0)
    } else {
        arg(a, 2)
    };
    let price = or(optional(&ticker, "mark")?, || {
        or(optional(&ticker, "last")?, || Ok(fallback))
    })?;
    let size = or(optional(&market, "contractSize")?, || Ok(Js::Number(1.0)))?;
    if price.to_number()? > 0.0 {
        Ok(Js::Number(
            arg(a, 1).to_number()?.abs() * size.to_number()? / price.to_number()?,
        ))
    } else {
        Ok(Js::Number(0.0))
    }
}
fn native_contracts_to_notional(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let market = store(rt, "marketsBySymbol")?.get_property(&arg(a, 0))?;
    let size = or(optional(&market, "contractSize")?, || Ok(Js::Number(1.0)))?;
    Ok(Js::Number(arg(a, 1).to_number()?.abs() * size.to_number()?))
}
fn map_position(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let p = arg(a, 0);
    let contracts = parse_or_zero(rt, p.get("positionAmt")?)?;
    let p_side = p.get("positionSide")?;
    let table = rt.global("POSITION_SIDE")?;
    let key = text(&p_side)?;
    let side = or(
        if contains(&table, &key)? {
            table.get_property(&p_side)?
        } else {
            Js::Bool(false)
        },
        || {
            enum_value(
                rt,
                "PositionSide",
                if contracts >= 0.0 { "Long" } else { "Short" },
            )
        },
    )?;
    let mode = enum_value(
        rt,
        "PositionMode",
        if p.get("positionSide")?.strict_equals(&Js::str("BOTH")) {
            "OneWay"
        } else {
            "Hedged"
        },
    )?;
    let entry = parse_or_zero(rt, p.get("entryPrice")?)?;
    let mark = parse_or_zero(rt, p.get("markPrice")?)?;
    let notional = rt.call_path(
        "nativeContractsToNotional",
        vec![p.get("symbol")?, Js::Number(contracts)],
    )?;
    let leverage = parse_or_zero(rt, p.get("leverage")?)?;
    let isolated = parse_or_zero(rt, v(rt, &p, "isolatedMargin")?)?;
    let price = or(Js::Number(mark), || Ok(Js::Number(entry)))?;
    let margin = if isolated > 0.0 {
        Js::Number(isolated)
    } else if leverage > 0.0 && price.to_number()? > 0.0 {
        Js::Number(
            rt.call_path(
                "nativeContractsToBase",
                vec![p.get("symbol")?, Js::Number(contracts), price.clone()],
            )?
            .to_number()?
                / leverage,
        )
    } else {
        Js::Undefined
    };
    let info = p.spread_object()?;
    info.set("contractsNative", Js::Number(contracts.abs()))?;
    let margin_type = Js::utf16(&or(p.get("marginType")?, || Ok(Js::str("")))?.to_js_string()?)
        .call_method("toLowerCase", vec![])?;
    Ok(Js::object(vec![
        (
            "id",
            global(
                rt,
                "createPositionId",
                vec![p.get("symbol")?, side.clone(), mode.clone()],
            )?,
        ),
        ("symbol", p.get("symbol")?),
        ("side", side),
        ("entryPrice", Js::Number(entry)),
        ("notional", notional),
        ("leverage", Js::Number(leverage)),
        (
            "unrealizedPnl",
            Js::Number(parse_or_zero(rt, p.get("unRealizedProfit")?)?),
        ),
        (
            "contracts",
            rt.call_path(
                "nativeContractsToBase",
                vec![p.get("symbol")?, Js::Number(contracts), price],
            )?,
        ),
        (
            "liquidationPrice",
            Js::Number(parse_or_zero(rt, p.get("liquidationPrice")?)?),
        ),
        ("marginSize", margin),
        (
            "marginType",
            enum_value(
                rt,
                "MarginType",
                if margin_type.strict_equals(&Js::str("isolated")) {
                    "Isolated"
                } else {
                    "Cross"
                },
            )?,
        ),
        ("positionMode", mode),
        ("info", info),
    ]))
}
fn fetch_positions(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
            let data = request(&rt, "xhr", "get", "POSITION_RISK", vec![]).await?;
            let rows = require_array(&rt.global("ENDPOINTS")?.get("POSITION_RISK")?, data)?;
            let mode = enum_value(
                &rt,
                "PositionMode",
                if store(&rt, "options")?.get("isHedged")?.truthy() {
                    "Hedged"
                } else {
                    "OneWay"
                },
            )?;
            let mut output = vec![];
            for p in rows {
                output.push(rt.call_path("mapPosition", vec![p, mode.clone()])?);
            }
            let ids = Js::new_set();
            for position in &output {
                ids.call_method("add", vec![position.get("id")?])?;
            }
            for market in store(&rt, "markets")?.iterable_values()? {
                for key in ["Long", "Short"] {
                    let side = enum_value(&rt, "PositionSide", key)?;
                    let id = global(
                        &rt,
                        "createPositionId",
                        vec![market.get("symbol")?, side.clone(), mode.clone()],
                    )?;
                    let row = Js::object(vec![
                        ("id", id.clone()),
                        ("symbol", market.get("symbol")?),
                        ("side", side),
                        ("entryPrice", Js::Number(0.0)),
                        ("notional", Js::Number(0.0)),
                        ("leverage", Js::Number(0.0)),
                        ("unrealizedPnl", Js::Number(0.0)),
                        ("contracts", Js::Number(0.0)),
                        ("liquidationPrice", Js::Number(0.0)),
                        ("marginSize", Js::Number(0.0)),
                        ("marginType", enum_value(&rt, "MarginType", "Cross")?),
                        ("positionMode", mode.clone()),
                        ("info", Js::object(vec![("synthetic", Js::Bool(true))])),
                    ]);
                    if !ids.call_method("has", vec![id])?.truthy() {
                        output.push(row);
                    }
                }
            }
            global(&rt, "uniqBy", vec![Js::array(output), Js::str("id")])
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                emit(&rt, &error.value)?;
                store(&rt, "positions")
            }
        }
    })
}
fn asset_price(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let asset = arg(a, 0);
    if ["USD", "USDT", "USDC"]
        .iter()
        .any(|s| asset.strict_equals(&Js::str(s)))
    {
        return Ok(Js::Number(1.0));
    }
    let tickers = store(rt, "tickersBySymbol")?;
    let name = text(&asset)?;
    let direct = or(tickers.get(&format!("{name}USD_PERP"))?, || {
        or(tickers.get(&format!("{name}USD"))?, || {
            tickers.get(&format!("{name}USDT"))
        })
    })?;
    or(optional(&direct, "mark")?, || {
        or(optional(&direct, "last")?, || Ok(Js::Number(0.0)))
    })
}
fn usd_balances(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let mut sum = 0.0;
    let field = arg(a, 1);
    for asset in arg(a, 0).iterable_values()? {
        let amount = parse_or_zero(rt, asset.get_property(&field)?)?;
        if !Js::Number(amount).truthy() {
            continue;
        }
        let price = rt
            .call_path("getAssetUsdPrice", vec![asset.get("asset")?])?
            .to_number()?;
        sum += if price > 0.0 { amount * price } else { amount };
    }
    Ok(Js::Number(sum))
}
fn fetch_positions_and_balance(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
        let data = request(&rt, "xhr", "get", "ACCOUNT", vec![]).await?;
        let assets = optional(&data, "assets")?.array_values().unwrap_or_default();
        let (mut total, mut free, mut upnl, mut used) = (0.0, 0.0, 0.0, 0.0);
        let coins = Js::object(vec![]);
        for asset in assets {
            let price = rt.call_path("getAssetUsdPrice", vec![asset.get("asset")?])?.to_number()?;
            let wallet = parse_or_zero(&rt, asset.get("walletBalance")?)?;
            let available = parse_or_zero(&rt, asset.get("availableBalance")?)?;
            let unrealized = parse_or_zero(&rt, asset.get("unrealizedProfit")?)?;
            let initial = parse_or_zero(&rt, asset.get("initialMargin")?)?;
            if wallet == 0.0 && available == 0.0 && unrealized == 0.0 && initial == 0.0 { continue; }
            let usd = if price > 0.0 { price } else { 1.0 };
            total += wallet * usd; free += available * usd; used += initial * usd; upnl += unrealized * usd;
            coins.set_key(&asset.get("asset")?.to_js_string()?, Js::object(vec![("base", asset.get("asset")?), ("amount", Js::Number(wallet)), ("notional", Js::Number(wallet * usd))]))?;
        }
        let keys: Vec<_> = coins.own_keys().iter().map(|s| Js::utf16(s)).collect();
        if !rt.this.get("hasLoggedBalanceAggregation")?.truthy() && !keys.is_empty() {
            rt.this.set("hasLoggedBalanceAggregation", Js::Bool(true))?;
            let keys = Js::array(keys).call_method("join", vec![Js::str(", ")])?;
            let fixed = |n| -> Result<String, Thrown> { text(&Js::Number(n).call_method("toFixed", vec![Js::Number(2.0)])?) };
            rt.call_path("log", vec![Js::str(&format!("COIN-M balance aggregation: {}; Total: ${}, Free: ${}, Used: ${}, UPnL: ${}", text(&keys)?, fixed(total)?, fixed(free)?, fixed(used)?, fixed(upnl)?)), enum_value(&rt, "LogSeverity", "Info")?])?;
        }
        let info = data.spread_object()?; info.set("assets", Js::Undefined)?;
        Ok::<_, Thrown>(Js::object(vec![
            ("balance", Js::object(vec![("total", Js::Number(total)), ("free", Js::Number(free)), ("used", Js::Number(used)), ("upnl", Js::Number(upnl)), ("coins", coins), ("info", info)])),
            ("positions", store(&rt, "positions")?),
        ]))
    }.await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                emit(&rt, &error.value)?;
                Ok(Js::object(vec![
                    ("balance", store(&rt, "balance")?),
                    ("positions", store(&rt, "positions")?),
                ]))
            }
        }
    })
}
fn fetch_orders(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
            let data = request(&rt, "xhr", "get", "OPEN_ORDERS", vec![]).await?;
            let mut orders = vec![];
            for o in data.array_values().unwrap_or_default() {
                let kind = rt
                    .global("ORDER_TYPE")?
                    .get_property(&v(&rt, &o, "type")?)?;
                let id = v(&rt, &o, "orderId")?;
                let row = Js::object(vec![
                    (
                        "id",
                        if id.is_nullish() {
                            Js::Undefined
                        } else {
                            id.call_method("toString", vec![])?
                        },
                    ),
                    ("status", enum_value(&rt, "OrderStatus", "Open")?),
                    ("symbol", o.get("symbol")?),
                    ("type", kind.clone()),
                    (
                        "side",
                        rt.global("ORDER_SIDE")?.get_property(&o.get("side")?)?,
                    ),
                    (
                        "price",
                        or(Js::Number(parse(&rt, o.get("price")?)?), || {
                            or(Js::Number(parse(&rt, v(&rt, &o, "stopPrice")?)?), || {
                                Ok(Js::Number(0.0))
                            })
                        })?,
                    ),
                    ("amount", Js::Number(parse(&rt, v(&rt, &o, "origQty")?)?)),
                    (
                        "reduceOnly",
                        or(v(&rt, &o, "reduceOnly")?, || Ok(Js::Bool(false)))?,
                    ),
                    (
                        "close",
                        or(v(&rt, &o, "closePosition")?, || Ok(Js::Bool(false)))?,
                    ),
                    (
                        "filled",
                        Js::Number(parse(&rt, v(&rt, &o, "executedQty")?)?),
                    ),
                    (
                        "lastUpdate",
                        global(&rt, "virtualClock.getCurrentTimeMs", vec![])?,
                    ),
                    (
                        "remaining",
                        global(
                            &rt,
                            "subtract",
                            vec![v(&rt, &o, "origQty")?, v(&rt, &o, "executedQty")?],
                        )?,
                    ),
                    ("info", o.clone()),
                ]);
                for (key, value) in [
                    ("TakeProfit", "tpTriggerType"),
                    ("StopLoss", "slTriggerType"),
                ] {
                    if kind.strict_equals(&enum_value(&rt, "OrderType", key)?) {
                        row.set(
                            value,
                            rt.global("ORDER_TRIGGER_TYPE")?.get_property(&v(
                                &rt,
                                &o,
                                "workingType",
                            )?)?,
                        )?;
                    }
                }
                orders.push(row);
            }
            rt.this.set("ordersSnapshotStale", Js::Bool(false))?;
            Ok::<_, Thrown>(Js::array(orders))
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                rt.call_path(
                    "log",
                    vec![
                        Js::str(&format!(
                            "Binance COIN-M fetchOrders error: {}",
                            text(&or(optional(&error.value, "message")?, || Ok(error
                                .value
                                .clone()))?)?
                        )),
                        enum_value(&rt, "LogSeverity", "Error")?,
                    ],
                )?;
                rt.this.set("ordersSnapshotStale", Js::Bool(true))?;
                store(&rt, "orders")
            }
        }
    })
}
fn fetch_position_mode(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let data = request(&rt, "xhr", "get", "HEDGE_MODE", vec![]).await?;
        Ok(Js::Bool(
            data.get("dualSidePosition")?.strict_equals(&Js::Bool(true)),
        ))
    })
}
fn validate_account(rt: Rt, _: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
            let data = request(&rt, "_primaryXhr", "get", "ACCOUNT", vec![]).await?;
            let total = rt.call_path(
                "usdValueForAssetBalances",
                vec![
                    or(optional(&data, "assets")?, || Ok(Js::array(vec![])))?,
                    Js::str("walletBalance"),
                ],
            )?;
            if let Js::Number(n) = total
                && n.is_finite()
            {
                let free = rt.call_path(
                    "usdValueForAssetBalances",
                    vec![
                        or(optional(&data, "assets")?, || Ok(Js::array(vec![])))?,
                        Js::str("availableBalance"),
                    ],
                )?;
                let upnl = rt.call_path(
                    "usdValueForAssetBalances",
                    vec![
                        or(optional(&data, "assets")?, || Ok(Js::array(vec![])))?,
                        Js::str("unrealizedProfit"),
                    ],
                )?;
                return Ok(Js::object(vec![
                    ("total", Js::Number(n)),
                    ("used", Js::Number(n - free.to_number()?)),
                    ("free", free),
                    ("upnl", upnl),
                ]));
            }
            Ok::<_, Thrown>(Js::Undefined)
        }
        .await;
        let balance = match result {
            Ok(value) => value,
            Err(error) => {
                emit(&rt, &error.value)?;
                return Ok(Js::object(vec![(
                    "error",
                    or(error_message(&error.value)?, || {
                        Ok(Js::str("Invalid API key or secret"))
                    })?,
                )]));
            }
        };
        if rt.this.get("options")?.get("secondary_key")?.truthy() {
            if let Err(error) = request(&rt, "_secondaryXhr", "get", "ACCOUNT", vec![]).await {
                return Ok(Js::object(vec![(
                    "error",
                    Js::str(&format!(
                        "Secondary Key: {}",
                        text(&error_message(&error.value)?)?
                    )),
                )]));
            }
        }
        Ok(Js::object(vec![
            ("error", Js::str("")),
            ("balance", balance),
        ]))
    })
}
fn fetch_public_trades(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let symbol = arg(&a, 0);
        let limit = global(
            &rt,
            "Math.min",
            vec![
                or(arg(&a, 1), || Ok(Js::Number(1000.0)))?,
                Js::Number(1000.0),
            ],
        )?;
        let data = request(
            &rt,
            "publicXhr",
            "get",
            "TRADES",
            vec![Js::object(vec![(
                "params",
                Js::object(vec![("symbol", symbol.clone()), ("limit", limit)]),
            )])],
        )
        .await?;
        let mut output = vec![];
        for trade in data.array_values().unwrap_or_default() {
            output.push(Js::object(vec![
                ("id", trade.get("id")?.call_method("toString", vec![])?),
                ("symbol", symbol.clone()),
                ("price", Js::Number(parse(&rt, trade.get("price")?)?)),
                ("qty", Js::Number(parse(&rt, trade.get("qty")?)?)),
                (
                    "quoteQty",
                    Js::Number(parse(
                        &rt,
                        or(trade.get("baseQty")?, || {
                            or(trade.get("quoteQty")?, || Ok(Js::str("0")))
                        })?,
                    )?),
                ),
                ("timestamp", trade.get("time")?),
                (
                    "side",
                    enum_value(
                        &rt,
                        "OrderSide",
                        if trade.get("isBuyerMaker")?.truthy() {
                            "Sell"
                        } else {
                            "Buy"
                        },
                    )?,
                ),
            ]));
        }
        Ok(Js::array(output))
    })
}
fn fetch_ohlcv(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let opts = arg(&a, 0);
        let interval = opts.get("interval")?;
        let limit = global(
            &rt,
            "Math.min",
            vec![
                or(opts.get("limit")?, || Ok(Js::Number(500.0)))?,
                Js::Number(1500.0),
            ],
        )?;
        let regex = global(&rt, "RegExp", vec![Js::str("(\\d+)")])?;
        let parts = interval.call_method("split", vec![regex])?;
        let end = if opts.get("to")?.truthy() {
            global(&rt, "dayjs", vec![opts.get("to")?])?
        } else {
            global(&rt, "dayjs", vec![])?
        };
        let start = if !opts.get("limit")?.truthy() && opts.get("from")?.truthy() {
            global(&rt, "dayjs", vec![opts.get("from")?])?
        } else {
            end.call_method(
                "subtract",
                vec![
                    Js::Number(parse(&rt, parts.get("1")?)? * limit.to_number()?),
                    parts.get("2")?,
                ],
            )?
        };
        let params = Js::object(vec![
            ("symbol", opts.get("symbol")?),
            ("interval", interval),
            ("startTime", start.call_method("valueOf", vec![])?),
            ("endTime", end.call_method("valueOf", vec![])?),
        ]);
        if opts.get("limit")?.truthy() {
            params.set("limit", limit)?;
        }
        let data = request(
            &rt,
            "publicXhr",
            "get",
            "KLINE",
            vec![Js::object(vec![("params", params)])],
        )
        .await?;
        let mut candles = vec![];
        for row in data.array_values().unwrap_or_default() {
            let cells = row.iterable_values()?;
            let candle = Js::object(vec![(
                "timestamp",
                Js::Number(arg(&cells, 0).to_number()? / 1000.0),
            )]);
            for (i, key) in ["open", "high", "low", "close", "volume"]
                .iter()
                .enumerate()
            {
                candle.set(key, Js::Number(parse(&rt, arg(&cells, i + 1))?))?;
            }
            candles.push(candle);
        }
        Ok(Js::array(candles))
    })
}
