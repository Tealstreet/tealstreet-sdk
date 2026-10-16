use std::rc::Rc;

pub struct WooV3PublicRequestContext {
    symbol: Vec<u16>,
}
impl WooV3PublicRequestContext {
    pub fn new(symbol: Vec<u16>) -> Self {
        Self { symbol }
    }
    pub fn symbol(&self) -> &[u16] {
        &self.symbol
    }
}
pub struct WooV3KlineRequestContext {
    symbol: Vec<u16>,
    interval: Vec<u16>,
}
impl WooV3KlineRequestContext {
    pub fn new(symbol: Vec<u16>, interval: Vec<u16>) -> Self {
        Self { symbol, interval }
    }
    pub fn symbol(&self) -> &[u16] {
        &self.symbol
    }
    pub fn interval(&self) -> &[u16] {
        &self.interval
    }
}
pub type CanonicalCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub struct CanonicalOps<V, E> {
    pub undefined: Rc<dyn Fn() -> V>,
    pub object: Rc<dyn Fn(Vec<(&'static str, V)>) -> V>,
    pub array: Rc<dyn Fn(Vec<V>) -> V>,
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, E>>,
    pub global: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub invoke: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub function: Rc<dyn Fn(&str, usize, CanonicalCallback<V, E>) -> V>,
    pub spread: Rc<dyn Fn(&V) -> Result<Vec<V>, E>>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub strict_equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub string: Rc<dyn Fn(&str) -> V>,
    pub number: Rc<dyn Fn(f64) -> V>,
    pub subtract: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub get_key: Rc<dyn Fn(&V, &V) -> Result<V, E>>,
    pub error: Rc<dyn Fn(&str) -> E>,
}
pub struct CanonicalModule<V, E> {
    pub ops: Rc<CanonicalOps<V, E>>,
    mappers: [V; 3],
    level_mapper: V,
    candle_mapper: V,
    fixed_intervals: V,
}
impl<V: Clone + 'static, E: 'static> CanonicalModule<V, E> {
    pub(crate) fn candle_mapper(&self) -> V {
        self.candle_mapper.clone()
    }
    pub(crate) fn fixed_intervals(&self) -> V {
        self.fixed_intervals.clone()
    }
    pub fn new(ops: Rc<CanonicalOps<V, E>>) -> Result<Self, E> {
        let mut module = Self::bindings(ops);
        module.freeze_intervals()?;
        Ok(module)
    }
    pub(crate) fn bindings(ops: Rc<CanonicalOps<V, E>>) -> Self {
        let h = ops.clone();
        let market_mapper = (ops.function)(
            "canonicalMarket",
            1,
            Rc::new(move |a| canonical_market(&h, &argument(&h, &a, 0))),
        );
        let h = ops.clone();
        let legacy_ticker_mapper = (ops.function)(
            "canonicalLegacyTickerStats",
            1,
            Rc::new(move |a| canonical_legacy_ticker_stats(&h, &argument(&h, &a, 0))),
        );
        let h = ops.clone();
        let v3_ticker_mapper = (ops.function)(
            "canonicalV3TickerStats",
            1,
            Rc::new(move |a| canonical_v3_ticker_stats(&h, &argument(&h, &a, 0))),
        );
        let h = ops.clone();
        let level_mapper = (ops.function)(
            "canonicalOrderBookLevel",
            1,
            Rc::new(move |a| canonical_order_book_level(&h, &argument(&h, &a, 0))),
        );
        let h = ops.clone();
        let candle_mapper = (ops.function)(
            "canonicalCandle",
            1,
            Rc::new(move |a| canonical_candle(&h, &argument(&h, &a, 0))),
        );
        Self {
            fixed_intervals: (ops.undefined)(),
            ops,
            mappers: [market_mapper, legacy_ticker_mapper, v3_ticker_mapper],
            level_mapper,
            candle_mapper,
        }
    }
    pub(crate) fn freeze_intervals(&mut self) -> Result<(), E> {
        let ops = self.ops.clone();
        self.fixed_intervals = freeze(&ops, || {
            Ok((ops.object)(vec![
                ("1m", (ops.number)(60_000.)),
                ("3m", (ops.number)(180_000.)),
                ("5m", (ops.number)(300_000.)),
                ("15m", (ops.number)(900_000.)),
                ("30m", (ops.number)(1_800_000.)),
                ("1h", (ops.number)(3_600_000.)),
                ("2h", (ops.number)(7_200_000.)),
                ("4h", (ops.number)(14_400_000.)),
                ("6h", (ops.number)(21_600_000.)),
                ("12h", (ops.number)(43_200_000.)),
                ("1d", (ops.number)(86_400_000.)),
                ("1w", (ops.number)(604_800_000.)),
            ]))
        })?;
        Ok(())
    }
}
fn argument<V: Clone, E>(h: &CanonicalOps<V, E>, a: &[V], i: usize) -> V {
    a.get(i).cloned().unwrap_or_else(|| (h.undefined)())
}
fn get<V, E>(h: &CanonicalOps<V, E>, v: &V, k: &str) -> Result<V, E> {
    (h.get)(v, k)
}
fn path<V: Clone, E>(h: &CanonicalOps<V, E>, v: &V, keys: &[&str]) -> Result<V, E> {
    let mut value = v.clone();
    for key in keys {
        value = get(h, &value, key)?;
    }
    Ok(value)
}
fn method<V, E>(
    h: &CanonicalOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = get(h, &receiver, key)?;
    (h.invoke)(f, receiver, args()?)
}
pub fn canonical_string<V, E>(h: &CanonicalOps<V, E>, value: V) -> Result<V, E> {
    method(h, (h.global)("JSON")?, "stringify", || Ok(vec![value]))
}
pub fn canonical_market<V: Clone, E>(h: &CanonicalOps<V, E>, market: &V) -> Result<V, E> {
    Ok((h.object)(vec![
        ("id", get(h, market, "id")?),
        ("symbol", get(h, market, "symbol")?),
        ("base", get(h, market, "base")?),
        ("quote", get(h, market, "quote")?),
        ("active", get(h, market, "active")?),
        (
            "precision",
            (h.object)(vec![
                ("amount", path(h, market, &["precision", "amount"])?),
                ("price", path(h, market, &["precision", "price"])?),
            ]),
        ),
        (
            "limits",
            (h.object)(vec![
                (
                    "amount",
                    (h.object)(vec![
                        ("min", path(h, market, &["limits", "amount", "min"])?),
                        ("max", path(h, market, &["limits", "amount", "max"])?),
                    ]),
                ),
                (
                    "leverage",
                    (h.object)(vec![
                        ("min", path(h, market, &["limits", "leverage", "min"])?),
                        ("max", path(h, market, &["limits", "leverage", "max"])?),
                    ]),
                ),
            ]),
        ),
    ]))
}
fn sorted_by_symbol<V: Clone + 'static, E: 'static>(
    h: Rc<CanonicalOps<V, E>>,
    values: V,
) -> Result<V, E> {
    let array = (h.array)((h.spread)(&values)?);
    let ops = h.clone();
    method(&h, array, "sort", || {
        Ok(vec![(h.function)(
            "",
            2,
            Rc::new(move |args| {
                let symbol = get(&ops, &argument(&ops, &args, 0), "symbol")?;
                method(&ops, symbol, "localeCompare", || {
                    Ok(vec![get(&ops, &argument(&ops, &args, 1), "symbol")?])
                })
            }),
        )])
    })
}
pub fn canonicalize_markets<V: Clone + 'static, E: 'static>(
    module: &CanonicalModule<V, E>,
    markets: V,
) -> Result<V, E> {
    let h = &module.ops;
    let sorted = sorted_by_symbol(h.clone(), markets)?;
    canonical_string(
        h,
        method(h, sorted, "map", || Ok(vec![module.mappers[0].clone()]))?,
    )
}
pub fn canonical_legacy_ticker_stats<V: Clone, E>(
    h: &CanonicalOps<V, E>,
    ticker: &V,
) -> Result<V, E> {
    let mut fields = Vec::new();
    for name in [
        "id",
        "symbol",
        "index",
        "mark",
        "percentage",
        "openInterest",
        "fundingRate",
        "fundingTime",
        "volume",
        "quoteVolume",
    ] {
        fields.push((name, get(h, ticker, name)?));
    }
    Ok((h.object)(fields))
}
pub fn canonical_v3_ticker_stats<V: Clone, E>(h: &CanonicalOps<V, E>, ticker: &V) -> Result<V, E> {
    let mut fields = Vec::new();
    for (name, key) in [
        ("id", "id"),
        ("symbol", "symbol"),
        ("index", "index"),
        ("mark", "mark"),
        ("percentage", "percentage24h"),
        ("openInterest", "openInterest"),
        ("fundingRate", "estimatedFundingRate"),
        ("fundingTime", "fundingTime"),
        ("volume", "volume24h"),
        ("quoteVolume", "quoteVolume24h"),
    ] {
        fields.push((name, get(h, ticker, key)?));
    }
    Ok((h.object)(fields))
}
pub fn canonicalize_legacy_tickers<V: Clone + 'static, E: 'static>(
    module: &CanonicalModule<V, E>,
    tickers: V,
) -> Result<V, E> {
    let h = &module.ops;
    let sorted = sorted_by_symbol(h.clone(), tickers)?;
    canonical_string(
        h,
        method(h, sorted, "map", || Ok(vec![module.mappers[1].clone()]))?,
    )
}

pub fn canonicalize_v3_tickers<V: Clone + 'static, E: 'static>(
    module: &CanonicalModule<V, E>,
    tickers: V,
) -> Result<V, E> {
    let h = &module.ops;
    let sorted = sorted_by_symbol(h.clone(), tickers)?;
    canonical_string(
        h,
        method(h, sorted, "map", || Ok(vec![module.mappers[2].clone()]))?,
    )
}
pub fn canonical_order_book_level<V, E>(h: &CanonicalOps<V, E>, level: &V) -> Result<V, E> {
    Ok((h.object)(vec![
        ("price", get(h, level, "price")?),
        ("amount", get(h, level, "amount")?),
        ("total", get(h, level, "total")?),
    ]))
}
pub fn canonicalize_order_book_values<V: Clone + 'static, E: 'static>(
    m: &CanonicalModule<V, E>,
    request: &V,
    book: &V,
) -> Result<V, E> {
    let h = &m.ops;
    if !(h.truthy)(&get(h, request, "symbol")?) {
        return Err((h.error)("WOO orderbook request symbol must be non-empty"));
    }
    let symbol = get(h, request, "symbol")?;
    let bids = method(h, get(h, book, "bids")?, "map", || {
        Ok(vec![m.level_mapper.clone()])
    })?;
    let asks = method(h, get(h, book, "asks")?, "map", || {
        Ok(vec![m.level_mapper.clone()])
    })?;
    canonical_string(
        h,
        (h.object)(vec![("symbol", symbol), ("bids", bids), ("asks", asks)]),
    )
}
pub fn canonicalize_v3_order_book<V: Clone + 'static, E: 'static>(
    m: &CanonicalModule<V, E>,
    request: &V,
    snapshot: &V,
) -> Result<V, E> {
    canonicalize_order_book_values(m, request, snapshot)
}
pub fn canonical_candle<V, E>(h: &CanonicalOps<V, E>, candle: &V) -> Result<V, E> {
    let mut fields = vec![];
    for name in ["timestamp", "open", "high", "low", "close", "volume"] {
        fields.push((name, get(h, candle, name)?));
    }
    Ok((h.object)(fields))
}
pub fn canonicalize_candles_for_request<V: Clone + 'static, E: 'static>(
    m: &CanonicalModule<V, E>,
    request: &V,
    candles: V,
) -> Result<V, E> {
    let h = &m.ops;
    let symbol = get(h, request, "symbol")?;
    let interval = get(h, request, "interval")?;
    let array = (h.array)((h.spread)(&candles)?);
    let ops = h.clone();
    let sorted = method(h, array, "sort", || {
        Ok(vec![(h.function)(
            "",
            2,
            Rc::new(move |args| {
                let left = get(&ops, &argument(&ops, &args, 0), "timestamp")?;
                let right = get(&ops, &argument(&ops, &args, 1), "timestamp")?;
                (ops.subtract)(left, right)
            }),
        )])
    })?;
    let values = method(h, sorted, "map", || Ok(vec![m.candle_mapper.clone()]))?;
    canonical_string(
        h,
        (h.object)(vec![
            ("symbol", symbol),
            ("interval", interval),
            ("candles", values),
        ]),
    )
}
pub fn normalize_kline_interval<V: Clone, E>(
    m: &CanonicalModule<V, E>,
    interval: V,
) -> Result<V, E> {
    let h = &m.ops;
    let normalized = if (h.strict_equal)(&interval, &(h.string)("1M")) {
        (h.string)("1mon")
    } else {
        interval
    };
    if (h.strict_equal)(&normalized, &(h.string)("1mon"))
        || (h.strict_equal)(&normalized, &(h.string)("1y"))
    {
        return freeze(h, || {
            Ok((h.object)(vec![
                ("status", (h.string)("blocked")),
                ("normalized", normalized),
                ("reason", (h.string)("variable-calendar-duration")),
            ]))
        });
    }
    let object = (h.global)("Object")?;
    let own = method(h, object, "hasOwn", || {
        Ok(vec![m.fixed_intervals.clone(), normalized.clone()])
    })?;
    if (h.truthy)(&own) {
        freeze(h, || {
            let duration = (h.get_key)(&m.fixed_intervals, &normalized)?;
            Ok((h.object)(vec![
                ("status", (h.string)("ready")),
                ("interval", normalized),
                ("durationMs", duration),
            ]))
        })
    } else {
        freeze(h, || {
            Ok((h.object)(vec![
                ("status", (h.string)("blocked")),
                ("normalized", normalized),
                ("reason", (h.string)("unsupported")),
            ]))
        })
    }
}
fn freeze<V, E>(h: &CanonicalOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    method(h, (h.global)("Object")?, "freeze", || Ok(vec![value()?]))
}
pub fn canonicalize_legacy_candles<V: Clone + 'static, E: 'static>(
    m: &CanonicalModule<V, E>,
    request: &V,
    candles: V,
) -> Result<V, E> {
    let h = &m.ops;
    let interval = normalize_kline_interval(m, get(h, request, "interval")?)?;
    if !(h.truthy)(&get(h, request, "symbol")?)
        || (h.strict_equal)(&get(h, &interval, "status")?, &(h.string)("blocked"))
    {
        return Err((h.error)(
            "WOO legacy kline request cannot form an exact fixed-interval canonical",
        ));
    }
    let normalized = (h.object)(vec![
        ("symbol", get(h, request, "symbol")?),
        ("interval", get(h, &interval, "interval")?),
    ]);
    canonicalize_candles_for_request(m, &normalized, candles)
}
