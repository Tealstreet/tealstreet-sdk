//! Bootstrap and market-data transitions over original properties and JS intrinsics.
use std::{future::Future, pin::Pin};
pub type DataFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait MarketDataHost {
    type Value: Clone;
    type Error: Clone;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn state(&self, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn reduce(
        &self,
        value: Self::Value,
        initial: Self::Value,
        callback: &mut dyn FnMut(Self::Value, Self::Value) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn for_each(
        &self,
        value: Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn literal(&self, text: &str) -> Self::Value;
    fn number(&self, n: f64) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn template(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn intrinsic(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn numeric(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn map(
        &self,
        value: Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn filter(
        &self,
        value: Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn sort(
        &self,
        value: Self::Value,
        compare: &mut dyn FnMut(Self::Value, Self::Value) -> Result<f64, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn select_log(&self) -> Result<Self::Value, Self::Error>;
    fn log(&self, logger: Self::Value, text: Vec<u16>, severity: &str) -> Result<(), Self::Error>;
    fn operation(
        &self,
        name: &str,
        args: Vec<Self::Value>,
    ) -> Result<DataFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn update(
        &self,
        fields: Vec<(&'static str, Self::Value)>,
        loaded: &[&str],
    ) -> Result<(), Self::Error>;
    fn write(&self, value: &Self::Value, key: &str, item: Self::Value) -> Result<(), Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn new_error(&self, message: &str) -> Result<Self::Error, Self::Error>;
    fn schedule_tick(&self, delay: f64) -> Result<(), Self::Error>;
    fn callbacks(&self, symbol: Self::Value) -> Result<Self::Value, Self::Error>;
    fn broadcast(&self, callbacks: Self::Value, book: Self::Value) -> Result<(), Self::Error>;
    fn update_ticker(
        &self,
        symbol: Self::Value,
        fields: Vec<(&'static str, Self::Value)>,
    ) -> Result<(), Self::Error>;
}
macro_rules! log {
    ($h:expr,$severity:expr,$($part:expr),+ $(,)?) => {{
        let logger=$h.select_log()?;
        let mut text=Vec::new(); $(text.extend($h.template(&$part)?);)+
        $h.log(logger,text,$severity)
    }};
}
fn len<H: MarketDataHost>(h: &H, v: &H::Value) -> Result<f64, H::Error> {
    h.numeric(&h.get(v, "length")?)
}
fn either<H: MarketDataHost>(
    h: &H,
    v: H::Value,
    fallback: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { fallback() }
}
fn state_get<H: MarketDataHost>(h: &H, key: &str, prop: &str) -> Result<H::Value, H::Error> {
    h.get(&h.state(key)?, prop)
}
fn call<H: MarketDataHost>(h: &H, name: &str, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    h.intrinsic(name, args)
}
fn parsed<H: MarketDataHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    call(h, "parseFloat", vec![v])
}
fn pf<H: MarketDataHost>(h: &H, v: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    parsed(h, either(h, h.get(v, key)?, || Ok(h.literal("0")))?)
}
fn eq<H: MarketDataHost>(h: &H, v: H::Value, text: &str) -> bool {
    h.equal(&v, &h.literal(text))
}
fn details<H: MarketDataHost>(h: &H, error: &H::Error, what: &str) -> Result<(), H::Error> {
    log!(
        h,
        "error",
        h.literal(&format!("Error fetching {what}: ")),
        h.error_value(error)
    )?;
    log!(
        h,
        "debug",
        h.literal(&format!("{} error details: ", capitalize(what))),
        call(h, "describeError", vec![h.error_value(error)])?
    )
}
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().unwrap().to_uppercase().collect::<String>() + c.as_str()
}
fn pair<H: MarketDataHost>(h: &H, min: f64, max: H::Value) -> H::Value {
    h.object(vec![("min", h.number(min)), ("max", max)])
}
fn num<H: MarketDataHost>(h: &H, v: H::Value) -> Result<f64, H::Error> {
    h.numeric(&v)
}

pub async fn start<H: MarketDataHost>(h: &H) -> Result<H::Value, H::Error> {
    let result = async {
        log!(h, "info", h.literal("Starting Paradex exchange..."))?;
        let external = h.operation("initExternalPublicData", vec![])?.await?;
        let public = h.state("isPublicDataOnly")?;
        if !h.truthy(&public) && h.truthy(&h.state("sdkInitPromise")?) {
            log!(h, "debug", h.literal("Waiting for SDK initialization..."))?;
            h.operation("awaitSDK", vec![h.state("sdkInitPromise")?])?
                .await?;
            log!(h, "debug", h.literal("SDK initialization complete"))?;
        }
        if !h.truthy(&public) && !h.truthy(&h.state("requestSigner")?) {
            log!(
                h,
                "warning",
                h.literal("WARNING: Request signer not available - authentication may fail")
            )?;
        }
        if !h.truthy(&external) {
            log!(h, "debug", h.literal("Fetching markets..."))?;
            let markets = h.operation("fetchMarkets", vec![])?.await?;
            if h.truthy(&h.state("isDisposed")?) {
                return Ok(h.undefined());
            }
            log!(
                h,
                "debug",
                h.literal("Fetched "),
                h.get(&markets, "length")?,
                h.literal(" markets")
            )?;
            h.update(vec![("markets", markets.clone())], &["markets"])?;
            log!(h, "debug", h.literal("Fetching tickers..."))?;
            let tickers = h.operation("fetchTickers", vec![])?.await?;
            if h.truthy(&h.state("isDisposed")?) {
                return Ok(h.undefined());
            }
            log!(
                h,
                "debug",
                h.literal("Fetched "),
                h.get(&tickers, "length")?,
                h.literal(" tickers")
            )?;
            log!(
                h,
                "info",
                h.literal("Loaded "),
                call(
                    h,
                    "Math.min",
                    vec![h.get(&tickers, "length")?, h.get(&markets, "length")?]
                )?,
                h.literal(" Paradex markets")
            )?;
            h.update(vec![("tickers", tickers)], &["tickers"])?;
            log!(h, "debug", h.literal("Connecting to public WebSocket..."))?;
            call(h, "connectPublic", vec![])?;
        }
        if h.truthy(&public) {
            h.update(vec![], &["balance", "positions", "orders"])?;
            log!(
                h,
                "info",
                h.literal("Paradex exchange started in public-only mode")
            )?;
            return Ok(h.undefined());
        }
        if h.truthy(&h.state("jwtManager")?) {
            log!(h, "debug", h.literal("Connecting to private WebSocket..."))?;
            call(h, "connectPrivate", vec![])?;
        } else {
            log!(
                h,
                "debug",
                h.literal("Skipping private WebSocket connection - no authentication available")
            )?;
        }
        if len(h, &state_get(h, "store", "tickers")?)? == 0.0 {
            log!(
                h,
                "warning",
                h.literal("No tickers loaded yet - continuing with private account initialization")
            )?;
        }
        log!(
            h,
            "debug",
            h.literal("Fetching initial leverage settings...")
        )?;
        h.operation("fetchLeverageSettings", vec![])?.await?;
        if h.truthy(&h.state("isDisposed")?) {
            return Ok(h.undefined());
        }
        log!(
            h,
            "debug",
            h.literal("Running initial tick (fetching positions and balance)...")
        )?;
        h.operation("tick", vec![])?.await?;
        if h.truthy(&h.state("isDisposed")?) {
            return Ok(h.undefined());
        }
        log!(h, "info", h.literal("Ready to trade on Paradex"))?;
        log!(h, "debug", h.literal("Fetching orders..."))?;
        let orders = h.operation("fetchOrders", vec![])?.await?;
        if h.truthy(&h.state("isDisposed")?) {
            return Ok(h.undefined());
        }
        log!(
            h,
            "debug",
            h.literal("Fetched "),
            h.get(&orders, "length")?,
            h.literal(" orders")
        )?;
        log!(h, "info", h.literal("Loaded Paradex orders"))?;
        h.update(vec![("orders", orders)], &["orders"])?;
        log!(
            h,
            "info",
            h.literal("Paradex exchange started successfully")
        )?;
        Ok(h.undefined())
    }
    .await;
    if let Err(e) = &result {
        log!(
            h,
            "error",
            h.literal("Error starting Paradex exchange: "),
            h.error_value(e)
        )?;
    }
    result
}
pub async fn tick<H: MarketDataHost>(h: &H) -> Result<H::Value, H::Error> {
    if h.truthy(&h.state("isDisposed")?) {
        return Ok(h.undefined());
    }
    let result = async {
        log!(h, "debug", h.literal("Tick: Fetching positions..."))?;
        let positions = h.operation("fetchPositions", vec![])?.await?;
        if h.truthy(&h.state("isDisposed")?) {
            return Ok(true);
        }
        log!(
            h,
            "debug",
            h.literal("Tick: Fetched "),
            h.get(&positions, "length")?,
            h.literal(" positions")
        )?;
        h.update(vec![("positions", positions.clone())], &["positions"])?;
        log!(h, "debug", h.literal("Tick: Fetching balance..."))?;
        let balance = h.operation("fetchBalance", vec![])?.await?;
        if h.truthy(&h.state("isDisposed")?) {
            return Ok(true);
        }
        log!(
            h,
            "debug",
            h.literal("Tick: Balance fetched - total: "),
            h.get(&balance, "total")?,
            h.literal(", free: "),
            h.get(&balance, "free")?
        )?;
        let pnl = if len(h, &positions)? > 0.0 {
            call(h, "sumBy", vec![positions, h.literal("unrealizedPnl")])?
        } else {
            h.number(0.0)
        };
        h.write(&balance, "upnl", pnl)?;
        h.update(vec![("balance", balance)], &["balance"])?;
        log!(
            h,
            "debug",
            h.literal("Tick: Fetching orders (periodic refresh)...")
        )?;
        let orders = h.operation("fetchOrders", vec![])?.await?;
        if h.truthy(&h.state("isDisposed")?) {
            return Ok(true);
        }
        log!(
            h,
            "debug",
            h.literal("Tick: Fetched "),
            h.get(&orders, "length")?,
            h.literal(" orders")
        )?;
        h.update(vec![("orders", orders)], &["orders"])?;
        log!(
            h,
            "debug",
            h.literal("Tick: Refreshing leverage settings...")
        )?;
        h.operation("fetchLeverageSettings", vec![])?.await?;
        Ok(h.truthy(&h.state("isDisposed")?))
    }
    .await;
    match result {
        Ok(true) => return Ok(h.undefined()),
        Ok(false) => {}
        Err(e) => {
            log!(
                h,
                "error",
                h.literal("Error in tick: "),
                h.optional(&h.error_value(&e), "message")?
            )?;
            log!(
                h,
                "debug",
                h.literal("Tick error stack: "),
                h.optional(&h.error_value(&e), "stack")?
            )?;
        }
    }
    if !h.truthy(&h.state("isDisposed")?) {
        h.schedule_tick(10_000.0)?;
    }
    Ok(h.undefined())
}
pub async fn fetch_markets<H: MarketDataHost>(h: &H) -> Result<H::Value, H::Error> {
    let result = async {
        log!(h, "debug", h.literal("Fetching markets from API..."))?;
        let response = h.operation("get", vec![h.literal("/markets")])?.await?;
        let data = h.get(&response, "data")?;
        if !h.truthy(&data) || !h.truthy(&h.get(&data, "results")?) {
            log!(
                h,
                "error",
                h.literal("Markets API returned invalid data structure")
            )?;
            log!(
                h,
                "debug",
                h.literal("Data received: "),
                call(h, "JSON.stringify", vec![data])?
            )?;
            return Ok(h.array(vec![]));
        }
        log!(
            h,
            "debug",
            h.literal("Markets API response: "),
            h.get(&h.get(&data, "results")?, "length")?,
            h.literal(" total markets")
        )?;
        if len(h, &h.get(&data, "results")?)? > 0.0 {
            log!(
                h,
                "debug",
                h.literal("Sample market fields: "),
                call(
                    h,
                    "keys.join",
                    vec![h.get(&h.get(&data, "results")?, "0")?, h.literal(", ")]
                )?
            )?;
            log!(
                h,
                "debug",
                h.literal("Sample market data: "),
                call(
                    h,
                    "JSON.stringify",
                    vec![h.get(&h.get(&data, "results")?, "0")?]
                )?
            )?;
        }
        // Original reduce keeps inherited accumulator properties and repeated key getters.
        let statuses = distribution(h, h.get(&data, "results")?, "status")?;
        log!(
            h,
            "debug",
            h.literal("Market status distribution: "),
            call(h, "JSON.stringify", vec![statuses])?
        )?;
        let kinds = distribution(h, h.get(&data, "results")?, "asset_kind")?;
        log!(
            h,
            "debug",
            h.literal("Asset kind distribution: "),
            call(h, "JSON.stringify", vec![kinds])?
        )?;
        let active = h.filter(h.get(&data, "results")?, &mut |m| {
            let symbol = either(h, h.get(&m, "market")?, || h.get(&m, "symbol"))?;
            let asset = call(
                h,
                "String.upper",
                vec![either(h, h.get(&m, "asset_kind")?, || Ok(h.literal("")))?],
            )?;
            let kind = call(
                h,
                "String.upper",
                vec![either(h, h.get(&m, "market_kind")?, || Ok(h.literal("")))?],
            )?;
            let option = h.truthy(&call(
                h,
                "includes",
                vec![asset.clone(), h.literal("OPTION")],
            )?) || h.truthy(&call(h, "includes", vec![kind, h.literal("OPTION")])?)
                || h.truthy(&call(
                    h,
                    "optional.endsWith",
                    vec![symbol.clone(), h.literal("-C")],
                )?)
                || h.truthy(&call(
                    h,
                    "optional.endsWith",
                    vec![symbol.clone(), h.literal("-P")],
                )?);
            let perp = !h.truthy(&asset)
                || eq(h, asset, "PERP")
                || h.truthy(&call(
                    h,
                    "optional.endsWith",
                    vec![symbol.clone(), h.literal("-PERP")],
                )?);
            let active = !h.truthy(&h.get(&m, "status")?) || eq(h, h.get(&m, "status")?, "ACTIVE");
            Ok(h.truthy(&symbol) && active && perp && !option)
        })?;
        log!(
            h,
            "debug",
            h.literal("Active perpetual futures markets (excluding options): "),
            h.get(&active, "length")?
        )?;
        h.map(active, &mut |m| {
            let symbol = either(h, h.get(&m, "market")?, || h.get(&m, "symbol"))?;
            let mut max = 100.0;
            if h.truthy(&h.optional(&h.get(&m, "delta1_cross_margin_params")?, "imf_base")?) {
                let imf = num(
                    h,
                    parsed(
                        h,
                        h.get(&h.get(&m, "delta1_cross_margin_params")?, "imf_base")?,
                    )?,
                )?;
                if imf > 0.0 {
                    max = (1.0 / imf).floor();
                }
            } else if h.truthy(&h.get(&m, "initial_margin_fraction")?) {
                let imf = num(h, parsed(h, h.get(&m, "initial_margin_fraction")?)?)?;
                if imf > 0.0 {
                    max = (1.0 / imf).floor();
                }
            }
            let base = h.get(&m, "base_currency")?;
            let quote = h.get(&m, "quote_currency")?;
            let active = !h.truthy(&h.get(&m, "status")?) || eq(h, h.get(&m, "status")?, "ACTIVE");
            let amount = parsed(
                h,
                either(h, h.get(&m, "step_size")?, || {
                    h.get(&m, "order_size_increment")
                })?,
            )?;
            let price = parsed(
                h,
                either(h, h.get(&m, "tick_size")?, || h.get(&m, "price_tick_size"))?,
            )?;
            let min = parsed(
                h,
                either(h, h.get(&m, "min_order_size")?, || {
                    either(h, h.get(&m, "min_notional")?, || Ok(h.literal("0")))
                })?,
            )?;
            let max_amount = parsed(
                h,
                either(h, h.get(&m, "max_order_size")?, || {
                    either(h, h.get(&m, "max_order_size")?, || Ok(h.literal("0")))
                })?,
            )?;
            let maker = pf(h, &m, "maker_fee")?;
            let taker = pf(h, &m, "taker_fee")?;
            Ok(h.object(vec![
                ("id", symbol.clone()),
                ("symbol", symbol),
                ("base", base),
                ("quote", quote),
                ("active", h.boolean(active)),
                ("type", h.literal("swap")),
                ("spot", h.boolean(false)),
                ("swap", h.boolean(true)),
                ("future", h.boolean(false)),
                ("option", h.boolean(false)),
                ("contract", h.boolean(true)),
                ("linear", h.boolean(true)),
                ("inverse", h.boolean(false)),
                ("contractSize", h.number(1.0)),
                (
                    "precision",
                    h.object(vec![
                        ("amount", amount),
                        ("price", price),
                        ("base", h.number(8.0)),
                        ("quote", h.number(8.0)),
                    ]),
                ),
                (
                    "limits",
                    h.object(vec![
                        ("amount", h.object(vec![("min", min), ("max", max_amount)])),
                        ("price", pair(h, 0.0, h.number(0.0))),
                        ("cost", pair(h, 0.0, h.number(0.0))),
                        ("leverage", pair(h, 1.0, h.number(max))),
                    ]),
                ),
                ("fees", h.object(vec![("maker", maker), ("taker", taker)])),
                ("info", m),
            ]))
        })
    }
    .await;
    if let Err(e) = &result {
        details(h, e, "markets")?;
    }
    result
}
pub async fn fetch_tickers<H: MarketDataHost>(h: &H) -> Result<H::Value, H::Error> {
    let result = async {
        log!(h, "debug", h.literal("Fetching tickers from API..."))?;
        let response = h
            .operation("get", vec![h.literal("/markets/summary?market=ALL")])?
            .await?;
        let data = h.get(&response, "data")?;
        if !h.truthy(&data) || !h.truthy(&h.get(&data, "results")?) {
            log!(
                h,
                "error",
                h.literal("Tickers API returned invalid data structure")
            )?;
            log!(
                h,
                "debug",
                h.literal("Data received: "),
                call(h, "JSON.stringify", vec![data])?
            )?;
            return Ok(h.array(vec![]));
        }
        log!(
            h,
            "debug",
            h.literal("Tickers API response: "),
            h.get(&h.get(&data, "results")?, "length")?,
            h.literal(" total tickers")
        )?;
        let perps = h.filter(h.get(&data, "results")?, &mut |t| {
            let option = h.truthy(&call(
                h,
                "endsWith",
                vec![h.get(&t, "symbol")?, h.literal("-C")],
            )?) || h.truthy(&call(
                h,
                "endsWith",
                vec![h.get(&t, "symbol")?, h.literal("-P")],
            )?);
            Ok(h.truthy(&call(
                h,
                "endsWith",
                vec![h.get(&t, "symbol")?, h.literal("-PERP")],
            )?) && !option)
        })?;
        log!(
            h,
            "debug",
            h.literal("Perpetual futures tickers (excluding options): "),
            h.get(&perps, "length")?
        )?;
        h.map(perps, &mut |t| {
            let mut fields = vec![
                ("id", h.get(&t, "symbol")?),
                ("symbol", h.get(&t, "symbol")?),
            ];
            for (target, key) in [
                ("bid", "bid"),
                ("ask", "ask"),
                ("last", "last_traded_price"),
                ("mark", "mark_price"),
            ] {
                fields.push((target, pf(h, &t, key)?));
            }
            fields.push((
                "index",
                parsed(
                    h,
                    either(h, h.get(&t, "underlying_price")?, || {
                        either(h, h.get(&t, "mark_price")?, || Ok(h.literal("0")))
                    })?,
                )?,
            ));
            fields.push((
                "percentage",
                h.number(num(h, pf(h, &t, "price_change_rate_24h")?)? * 100.0),
            ));
            fields.push(("fundingRate", pf(h, &t, "funding_rate")?));
            fields.push(("fundingTime", h.number(0.0)));
            for (target, key) in [
                ("volume", "volume_24h"),
                ("quoteVolume", "volume_24h"),
                ("openInterest", "open_interest"),
            ] {
                fields.push((target, pf(h, &t, key)?));
            }
            fields.push(("timestamp", call(h, "virtualClock", vec![])?));
            fields.push(("info", t));
            Ok(h.object(fields))
        })
    }
    .await;
    if let Err(e) = &result {
        details(h, e, "tickers")?;
    }
    result
}
pub async fn fetch_ohlcv<H: MarketDataHost>(h: &H, opts: H::Value) -> Result<H::Value, H::Error> {
    let result = async {
        let table = h.object(vec![
            ("1m", h.literal("1")),
            ("3m", h.literal("3")),
            ("5m", h.literal("5")),
            ("15m", h.literal("15")),
            ("30m", h.literal("30")),
            ("1h", h.literal("60")),
        ]);
        let resolution = call(h, "member", vec![table, h.get(&opts, "interval")?])?;
        if h.truthy(&resolution) {
            let to = h.get(&opts, "to")?;
            let end = if h.nullish(&to) {
                call(h, "virtualClock", vec![])?
            } else {
                to
            };
            let interval = num(h, call(h, "timeframe", vec![h.get(&opts, "interval")?])?)? * 1000.0;
            let from = h.get(&opts, "from")?;
            let start = if h.nullish(&from) {
                h.number(num(h, end.clone())? - interval * 2000.0)
            } else {
                from
            };
            let documented = async {
                let response = h
                    .operation(
                        "get",
                        vec![
                            h.literal("/markets/klines"),
                            h.object(vec![(
                                "params",
                                h.object(vec![
                                    ("symbol", h.get(&opts, "symbol")?),
                                    ("resolution", resolution),
                                    ("start_at", start),
                                    ("end_at", end),
                                    ("price_kind", h.literal("last")),
                                ]),
                            )]),
                        ],
                    )?
                    .await?;
                let data = h.get(&response, "data")?;
                if !h.is_array(&h.optional(&data, "results")?) {
                    return Ok(None);
                }
                let mapped = h.map(h.get(&data, "results")?, &mut |row| {
                    if !h.is_array(&row) || len(h, &row)? < 6.0 {
                        return Ok(h.array(vec![]));
                    }
                    let numbers = h.map(row, &mut |v| call(h, "Number", vec![v]))?;
                    let mut fields = vec![];
                    for (i, key) in ["timestamp", "open", "high", "low", "close", "volume"]
                        .into_iter()
                        .enumerate()
                    {
                        fields.push((key, h.get(&numbers, &i.to_string())?));
                    }
                    let mut finite = true;
                    for (_, v) in &fields {
                        if !h.truthy(&call(h, "Number.isFinite", vec![v.clone()])?) {
                            finite = false;
                            break;
                        }
                    }
                    if !finite {
                        return Ok(h.array(vec![]));
                    }
                    Ok(h.array(vec![h.object(fields)]))
                })?;
                Ok::<_, H::Error>(Some(call(h, "flat", vec![mapped])?))
            }
            .await;
            match documented {
                Ok(Some(c)) => return Ok(c),
                Ok(None) => {}
                Err(e) => {
                    log!(
                        h,
                        "warning",
                        h.literal("Documented OHLCV endpoint failed for "),
                        h.get(&opts, "symbol")?,
                        h.literal("; trying legacy TradingView: "),
                        either(h, h.optional(&h.error_value(&e), "message")?, || Ok(
                            h.error_value(&e)
                        ))?
                    )?;
                }
            }
        }
        let resolution = call(h, "tradingViewResolution", vec![h.get(&opts, "interval")?])?;
        let symbol = h.get(&opts, "symbol")?;
        let from = if h.truthy(&h.get(&opts, "from")?) {
            h.number((num(h, h.get(&opts, "from")?)? / 1000.0).floor())
        } else {
            h.undefined()
        };
        let to = if h.truthy(&h.get(&opts, "to")?) {
            h.number((num(h, h.get(&opts, "to")?)? / 1000.0).floor())
        } else {
            h.undefined()
        };
        let params = call(
            h,
            "omitUndefined",
            vec![h.object(vec![
                ("symbol", symbol),
                ("resolution", resolution),
                ("from", from),
                ("to", to),
                ("countback", h.number(2000.0)),
                ("price_kind", h.literal("last")),
            ])],
        )?;
        let response = h
            .operation(
                "get",
                vec![
                    h.literal("/tradingview/history"),
                    h.object(vec![("params", params)]),
                ],
            )?
            .await?;
        let data = h.get(&response, "data")?;
        if !h.truthy(&data) || !eq(h, h.get(&data, "s")?, "ok") {
            log!(
                h,
                "error",
                h.literal("OHLCV API returned error status: "),
                either(h, h.optional(&data, "s")?, || Ok(h.literal("unknown")))?
            )?;
            return Ok(h.array(vec![]));
        }
        if !h.truthy(&h.get(&data, "t")?) || !h.is_array(&h.get(&data, "t")?) {
            log!(
                h,
                "error",
                h.literal("OHLCV API returned invalid data structure")
            )?;
            return Ok(h.array(vec![]));
        }
        let mut candles = vec![];
        let mut i = 0usize;
        while (i as f64) < len(h, &h.get(&data, "t")?)? {
            let timestamp = h.number(num(h, h.get(&h.get(&data, "t")?, &i.to_string())?)? * 1000.0);
            let mut fields = vec![("timestamp", timestamp)];
            for (target, key) in [
                ("open", "o"),
                ("high", "h"),
                ("low", "l"),
                ("close", "c"),
                ("volume", "v"),
            ] {
                fields.push((
                    target,
                    parsed(h, h.get(&h.get(&data, key)?, &i.to_string())?)?,
                ));
            }
            candles.push(h.object(fields));
            i += 1;
        }
        Ok(h.array(candles))
    }
    .await;
    if let Err(e) = &result {
        log!(
            h,
            "error",
            h.literal("Error fetching OHLCV for "),
            h.get(&opts, "symbol")?,
            h.literal(": "),
            either(h, h.get(&h.error_value(e), "message")?, || Ok(
                h.error_value(e)
            ))?
        )?;
    }
    result
}
fn leverage<H: MarketDataHost>(h: &H, symbol: H::Value) -> Result<H::Value, H::Error> {
    either(h, call(h, "leverage.get", vec![symbol])?, || {
        Ok(h.number(10.0))
    })
}
fn position_id<H: MarketDataHost>(
    h: &H,
    symbol: H::Value,
    side: H::Value,
) -> Result<H::Value, H::Error> {
    call(
        h,
        "createPositionId",
        vec![symbol, side, h.literal("oneway")],
    )
}
pub async fn fetch_positions<H: MarketDataHost>(h: &H) -> Result<H::Value, H::Error> {
    let result=async{
  if h.truthy(&h.state("sdkInitPromise")?){h.operation("awaitSDK",vec![h.state("sdkInitPromise")?])?.await?;}
  if !h.truthy(&h.state("jwtManager")?){log!(h,"warning",h.literal("Cannot fetch positions: JWT manager not initialized"))?;return Ok(h.array(vec![]));}
  log!(h,"debug",h.literal("Fetching positions from API..."))?;
  let response=h.operation("get",vec![h.literal("/positions")])?.await?;let data=h.get(&response,"data")?;
  if !h.truthy(&data)||!h.truthy(&h.get(&data,"results")?){log!(h,"error",h.literal("Positions API returned invalid data structure"))?;log!(h,"debug",h.literal("Data received: "),call(h,"JSON.stringify",vec![data])?)?;return Ok(h.array(vec![]));}
  log!(h,"debug",h.literal("Positions API response: "),h.get(&h.get(&data,"results")?,"length")?,h.literal(" total positions"))?;
  let open=h.filter(h.get(&data,"results")?,&mut |p|Ok(eq(h,h.get(&p,"status")?,"OPEN")))?;
  log!(h,"debug",h.literal("Open positions: "),h.get(&open,"length")?)?;
  let actual=h.map(open,&mut |p|{
   let entry=parsed(h,h.get(&p,"average_entry_price")?)?;
   let contracts=call(h,"Math.abs",vec![parsed(h,h.get(&p,"size")?)?])?;
   let api=pf(h,&p,"leverage")?;
   let lev=if num(h,api.clone())?>0.0{api}else{leverage(h,h.get(&p,"market")?)?};
   let id=position_id(h,h.get(&p,"market")?,h.literal(if eq(h,h.get(&p,"side")?,"LONG"){"long"}else{"short"}))?;
   let symbol=h.get(&p,"market")?;let side=h.literal(if eq(h,h.get(&p,"side")?,"LONG"){"long"}else{"short"});
   let unreal=parsed(h,h.get(&p,"unrealized_pnl")?)?;let realized=pf(h,&p,"realized_positional_pnl")?;
   let notional=h.number(num(h,contracts.clone())?*num(h,entry.clone())?);
   let liquidation=pf(h,&p,"liquidation_price")?;let updated=h.get(&p,"last_updated_at")?;
   Ok(h.object(vec![("id",id),("symbol",symbol),("side",side),("contracts",contracts),("unrealizedPnl",unreal),("realizedPnl",realized),("notional",notional),("leverage",lev),("entryPrice",entry),("liquidationPrice",liquidation),("marginType",h.literal("cross")),("positionMode",h.literal("oneway")),("lastUpdate",updated),("info",p)]))
  })?;
  if !h.truthy(&state_get(h,"store","markets")?)||len(h,&state_get(h,"store","markets")?)?==0.0{
   log!(h,"warning",h.literal("WARNING: Markets not loaded yet - cannot create fake positions. Returning only actual positions."))?;
   log!(h,"debug",h.literal("This usually happens if fetchPositions is called before markets are loaded during exchange startup."))?;
   return Ok(actual);
  }
  let count=len(h,&h.filter(state_get(h,"store","markets")?,&mut |m|Ok(h.truthy(&h.get(&m,"active")?)))?)?;
  log!(h,"debug",h.literal("Markets: total="),h.get(&state_get(h,"store","markets")?,"length")?,h.literal(", active="),h.number(count))?;
  let active=h.filter(state_get(h,"store","markets")?,&mut |m|Ok(h.truthy(&h.get(&m,"active")?)))?;
  let fake=h.map(active,&mut |m|{
   let id=position_id(h,h.get(&m,"symbol")?,h.literal("long"))?;let symbol=h.get(&m,"symbol")?;
   let lev=leverage(h,h.get(&m,"symbol")?)?;
   Ok(h.object(vec![("id",id),("symbol",symbol),("side",h.literal("long")),("entryPrice",h.number(0.0)),("notional",h.number(0.0)),("leverage",lev),("unrealizedPnl",h.number(0.0)),("contracts",h.number(0.0)),("liquidationPrice",h.number(0.0)),("marginType",h.literal("cross")),("positionMode",h.literal("oneway")),("info",h.object(vec![]))]))
  })?;
  log!(h,"debug",h.literal("Created "),h.get(&fake,"length")?,h.literal(" fake positions for all "),h.get(&state_get(h,"store","markets")?,"length")?,h.literal(" active markets"))?;
  let map=call(h,"new Map",vec![])?;
  for rows in [fake, actual] { h.for_each(rows, &mut |pos| { call(h,"Map.set",vec![map.clone(),h.get(&pos,"symbol")?,pos])?; Ok(()) })?; }
  call(h,"Array.from.values",vec![map])
 }.await;
    if let Err(e) = &result {
        details(h, e, "positions")?;
    }
    result
}
fn book_levels<H: MarketDataHost>(
    h: &H,
    levels: H::Value,
    tuple: bool,
) -> Result<H::Value, H::Error> {
    h.map(levels, &mut |level| {
        let selected = if tuple {
            call(h, "destructurePair", vec![level])?
        } else {
            level
        };
        let captured = if tuple {
            Some((h.get(&selected, "0")?, h.get(&selected, "1")?))
        } else {
            None
        };
        let price_value = || {
            if let Some((p, _)) = &captured {
                Ok(p.clone())
            } else {
                h.get(&selected, "price")
            }
        };
        let size_value = || {
            if let Some((_, v)) = &captured {
                Ok(v.clone())
            } else {
                h.get(&selected, "size")
            }
        };
        let price = parsed(h, price_value()?)?;
        let amount = parsed(h, size_value()?)?;
        let p = parsed(h, price_value()?)?;
        let s = parsed(h, size_value()?)?;
        Ok(h.object(vec![
            ("price", price),
            ("amount", amount),
            ("total", h.number(num(h, p)? * num(h, s)?)),
        ]))
    })
}
pub async fn fetch_order_book<H: MarketDataHost>(
    h: &H,
    symbol: H::Value,
    depth: H::Value,
) -> Result<H::Value, H::Error> {
    let depth = if h.is_undefined(&depth) {
        h.number(20.0)
    } else {
        depth
    };
    let result = async {
        log!(
            h,
            "debug",
            h.literal("Fetching orderbook for "),
            symbol.clone(),
            h.literal(" with depth="),
            depth.clone()
        )?;
        let endpoint = call(
            h,
            "replace",
            vec![
                h.literal("/orderbook/{market}"),
                h.literal("{market}"),
                symbol.clone(),
            ],
        )?;
        log!(
            h,
            "debug",
            h.literal("Orderbook endpoint: GET "),
            endpoint.clone(),
            h.literal("?depth="),
            depth.clone()
        )?;
        let start = num(h, call(h, "Date.now", vec![])?)?;
        let response = h
            .operation(
                "get",
                vec![
                    endpoint,
                    h.object(vec![("params", h.object(vec![("depth", depth)]))]),
                ],
            )?
            .await?;
        let data = h.get(&response, "data")?;
        let duration = num(h, call(h, "Date.now", vec![])?)? - start;
        if !h.truthy(&data)
            || !h.truthy(&h.get(&data, "bids")?)
            || !h.truthy(&h.get(&data, "asks")?)
        {
            log!(
                h,
                "error",
                h.literal("Orderbook API returned invalid data structure for "),
                symbol.clone()
            )?;
            log!(
                h,
                "debug",
                h.literal("Data received: "),
                call(h, "JSON.stringify", vec![data])?
            )?;
            return Err(h.new_error("Invalid orderbook data structure")?);
        }
        let bids = book_levels(h, h.get(&data, "bids")?, true)?;
        let asks = book_levels(h, h.get(&data, "asks")?, true)?;
        let book = h.object(vec![("bids", bids.clone()), ("asks", asks.clone())]);
        log!(
            h,
            "info",
            h.literal("Fetched orderbook for "),
            symbol.clone(),
            h.literal(" in "),
            h.number(duration),
            h.literal("ms: "),
            h.get(&bids, "length")?,
            h.literal(" bids, "),
            h.get(&asks, "length")?,
            h.literal(" asks")
        )?;
        if len(h, &bids)? > 0.0 && len(h, &asks)? > 0.0 {
            let bid = h.get(&bids, "0")?;
            let ask = h.get(&asks, "0")?;
            let spread = num(h, h.get(&ask, "price")?)? - num(h, h.get(&bid, "price")?)?;
            let percentage = spread / num(h, h.get(&bid, "price")?)? * 100.0;
            log!(
                h,
                "debug",
                h.literal("Orderbook "),
                symbol.clone(),
                h.literal(": Best Bid="),
                h.get(&bid, "price")?,
                h.literal(" ("),
                h.get(&bid, "amount")?,
                h.literal("), Best Ask="),
                h.get(&ask, "price")?,
                h.literal(" ("),
                h.get(&ask, "amount")?,
                h.literal("), Spread="),
                call(h, "toFixed", vec![h.number(spread), h.number(2.0)])?,
                h.literal(" ("),
                call(h, "toFixed", vec![h.number(percentage), h.number(3.0)])?,
                h.literal("%)")
            )?;
        } else {
            log!(
                h,
                "warning",
                h.literal("Orderbook "),
                symbol.clone(),
                h.literal(": Empty orderbook received")
            )?;
        }
        Ok(book)
    }
    .await;
    if let Err(e) = &result {
        log!(
            h,
            "error",
            h.literal("Error fetching orderbook for "),
            symbol,
            h.literal(": "),
            either(h, h.get(&h.error_value(e), "message")?, || Ok(
                h.error_value(e)
            ))?
        )?;
        log!(
            h,
            "debug",
            h.literal("Orderbook error details: "),
            call(h, "describeError", vec![h.error_value(e)])?
        )?;
        if h.truthy(&h.get(&h.error_value(e), "response")?) {
            log!(
                h,
                "debug",
                h.literal("Orderbook response status: "),
                h.get(&h.get(&h.error_value(e), "response")?, "status")?
            )?;
            log!(
                h,
                "debug",
                h.literal("Orderbook response data: "),
                call(
                    h,
                    "JSON.stringify",
                    vec![h.get(&h.get(&h.error_value(e), "response")?, "data")?]
                )?
            )?;
        }
    }
    result
}
fn normalize_levels<H: MarketDataHost>(
    h: &H,
    all: &H::Value,
    raw: H::Value,
    side: &str,
) -> Result<H::Value, H::Error> {
    if len(h, all)? > 0.0 {
        h.filter(all.clone(), &mut |level| {
            Ok(eq(h, h.get(&level, "side")?, side))
        })
    } else {
        h.map(raw, &mut |level| {
            let (price, size) = if h.is_array(&level) {
                (h.get(&level, "0")?, h.get(&level, "1")?)
            } else {
                (
                    h.optional(&level, "price")?,
                    either(h, h.optional(&level, "size")?, || {
                        h.optional(&level, "amount")
                    })?,
                )
            };
            Ok(h.object(vec![
                ("side", h.literal(side)),
                ("price", price),
                ("size", size),
            ]))
        })
    }
}
pub fn handle_orderbook_update<H: MarketDataHost>(
    h: &H,
    data: H::Value,
) -> Result<H::Value, H::Error> {
    let result = (|| {
        let raw = if h.is_array(&h.optional(&data, "data")?) {
            h.get(&h.get(&data, "data")?, "0")?
        } else {
            either(h, h.optional(&data, "data")?, || Ok(data.clone()))?
        };
        let channel = call(
            h,
            "String",
            vec![either(h, h.optional(&data, "channel")?, || {
                Ok(h.literal(""))
            })?],
        )?;
        let channel_symbol = call(
            h,
            "split.member",
            vec![channel, h.literal("."), h.number(1.0)],
        )?;
        let raw = if h.truthy(&raw) && h.is_object(&raw) {
            raw
        } else {
            h.object(vec![])
        };
        let symbol = call(
            h,
            "String",
            vec![either(h, h.get(&raw, "market")?, || {
                either(h, channel_symbol, || Ok(h.literal("")))
            })?],
        )?;
        if !h.truthy(&symbol) {
            return Ok(h.undefined());
        }
        let inserts = if h.is_array(&h.get(&raw, "inserts")?) {
            h.get(&raw, "inserts")?
        } else {
            h.array(vec![])
        };
        let updates = if h.is_array(&h.get(&raw, "updates")?) {
            h.get(&raw, "updates")?
        } else {
            h.array(vec![])
        };
        let all = call(h, "spread.concat", vec![inserts, updates])?;
        let raw_bids = if h.is_array(&h.get(&raw, "bids")?) {
            h.get(&raw, "bids")?
        } else {
            h.array(vec![])
        };
        let raw_asks = if h.is_array(&h.get(&raw, "asks")?) {
            h.get(&raw, "asks")?
        } else {
            h.array(vec![])
        };
        let bids = normalize_levels(h, &all, raw_bids, "BUY")?;
        let asks = normalize_levels(h, &all, raw_asks, "SELL")?;
        if len(h, &bids)? > 0.0 && len(h, &asks)? > 0.0 {
            let sorted_bids = h.sort(bids.clone(), &mut |a, b| {
                Ok(num(h, parsed(h, h.get(&b, "price")?)?)?
                    - num(h, parsed(h, h.get(&a, "price")?)?)?)
            })?;
            let sorted_asks = h.sort(asks.clone(), &mut |a, b| {
                Ok(num(h, parsed(h, h.get(&a, "price")?)?)?
                    - num(h, parsed(h, h.get(&b, "price")?)?)?)
            })?;
            let bid = parsed(h, h.get(&h.get(&sorted_bids, "0")?, "price")?)?;
            let ask = parsed(h, h.get(&h.get(&sorted_asks, "0")?, "price")?)?;
            h.update_ticker(symbol.clone(), vec![("bid", bid), ("ask", ask)])?;
        }
        let callbacks = h.callbacks(symbol)?;
        if h.truthy(&callbacks) && num(h, h.get(&callbacks, "size")?)? > 0.0 {
            let book = h.object(vec![
                ("bids", book_levels(h, bids, false)?),
                ("asks", book_levels(h, asks, false)?),
            ]);
            h.broadcast(callbacks, book)?;
        }
        Ok(h.undefined())
    })();
    if let Err(e) = result {
        log!(
            h,
            "error",
            h.literal("Error handling orderbook update: "),
            h.error_value(&e)
        )?;
    }
    Ok(h.undefined())
}

fn distribution<H: MarketDataHost>(h: &H, rows: H::Value, key: &str) -> Result<H::Value, H::Error> {
    h.reduce(rows, h.object(vec![]), &mut |acc, row| {
        let write_key = either(h, h.get(&row, key)?, || Ok(h.literal("undefined")))?;
        let read_key = either(h, h.get(&row, key)?, || Ok(h.literal("undefined")))?;
        let old = either(h, call(h, "member", vec![acc.clone(), read_key])?, || {
            Ok(h.number(0.0))
        })?;
        let value = call(h, "add", vec![old, h.number(1.0)])?;
        call(h, "assignMember", vec![acc.clone(), write_key, value])?;
        Ok(acc)
    })
}
