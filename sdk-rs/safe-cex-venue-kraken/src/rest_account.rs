//! REST read bodies over original parsed values, current Store slots and JS intrinsics.
use std::{future::Future, rc::Rc};

pub type Handle<H> = Rc<<H as ReadHost>::Value>;
pub type ReadResult<H, T> = Result<T, <H as ReadHost>::Error>;
pub type Entries<H> = Vec<(Vec<u16>, Handle<H>)>;

pub trait ReadHost {
    type Value: ?Sized;
    type Error;
    fn field(
        &self,
        value: &Self::Value,
        key: &[u16],
        optional: bool,
    ) -> ReadResult<Self, Handle<Self>>;
    fn keyed(&self, value: &Self::Value, key: &str) -> ReadResult<Self, Handle<Self>>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn strict_eq(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn number(&self, value: f64) -> Handle<Self>;
    fn boolean(&self, value: bool) -> Handle<Self>;
    fn text(&self, value: &[u16]) -> Handle<Self>;
    fn string(&self, value: &Self::Value) -> ReadResult<Self, Vec<u16>>;
    fn lower(&self, value: &[u16]) -> Vec<u16>;
    fn upper(&self, value: &[u16]) -> Vec<u16>;
    fn starts_with(&self, value: &Self::Value, prefix: &str) -> ReadResult<Self, bool>;
    fn split_pair(&self, value: &Self::Value) -> ReadResult<Self, (Handle<Self>, Handle<Self>)>;
    fn normalize_asset(&self, value: &Self::Value) -> ReadResult<Self, Handle<Self>>;
    fn order_type(&self, value: &Self::Value) -> ReadResult<Self, Handle<Self>>;
    fn parse_float(&self, value: &Self::Value) -> ReadResult<Self, f64>;
    fn numeric(&self, value: &Self::Value) -> ReadResult<Self, f64>;
    fn amount_precision(&self, value: &Self::Value) -> ReadResult<Self, f64>;
    fn includes(&self, value: &Self::Value, text: &str) -> ReadResult<Self, bool>;
    fn items(&self, value: &Self::Value) -> ReadResult<Self, Vec<Handle<Self>>>;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn entries(&self, value: &Self::Value) -> ReadResult<Self, Entries<Self>>;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn array(&self, values: Vec<Handle<Self>>) -> Handle<Self>;
    fn object(&self, values: Entries<Self>) -> ReadResult<Self, Handle<Self>>;
    fn spread(&self, value: &Self::Value, values: Entries<Self>) -> ReadResult<Self, Handle<Self>>;
    fn assign(
        &self,
        object: &Self::Value,
        key: &[u16],
        value: Handle<Self>,
    ) -> ReadResult<Self, ()>;
    fn current(&self, slot: &str) -> ReadResult<Self, Handle<Self>>;
    fn preference(&self, cache: &str, symbol: &Self::Value) -> ReadResult<Self, Handle<Self>>;
    fn set_preference(&self, symbol: Handle<Self>, margin: Handle<Self>) -> ReadResult<Self, ()>;
    fn now(&self) -> ReadResult<Self, f64>;
    fn read(
        &self,
        spot: bool,
        endpoint: &str,
        post: bool,
        body: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn fetch_balance(
        &self,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn update_balance(&self, patch: Handle<Self>) -> ReadResult<Self, ()>;
    fn trace(&self, error: &Self::Error) -> ReadResult<Self, ()>;
    fn error_message(&self, error: &Self::Error, fallback: &str) -> ReadResult<Self, Handle<Self>>;
    fn error(&self, message: Handle<Self>) -> Self::Error;
    fn emit_error(&self, message: Handle<Self>) -> ReadResult<Self, ()>;
    fn map_fills(&self, rows: Handle<Self>) -> ReadResult<Self, Vec<Handle<Self>>>;
    fn iso(&self, milliseconds: f64) -> ReadResult<Self, Vec<u16>>;
    fn warn_fills(&self, page: usize, from_iso: Vec<u16>) -> ReadResult<Self, ()>;
    fn slice_limit(
        &self,
        values: Vec<Handle<Self>>,
        limit: &Self::Value,
    ) -> ReadResult<Self, Handle<Self>>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: ReadHost>(h: &H, s: &str) -> Handle<H> {
    h.text(&units(s))
}
fn field<H: ReadHost>(h: &H, v: &H::Value, k: &str) -> ReadResult<H, Handle<H>> {
    h.field(v, &units(k), false)
}
fn opt<H: ReadHost>(h: &H, v: &H::Value, k: &str) -> ReadResult<H, Handle<H>> {
    h.field(v, &units(k), true)
}
fn eq<H: ReadHost>(h: &H, v: &H::Value, s: &str) -> bool {
    h.strict_eq(v, &text(h, s))
}
fn obj<H: ReadHost>(h: &H, values: Vec<(&str, Handle<H>)>) -> ReadResult<H, Handle<H>> {
    h.object(values.into_iter().map(|(k, v)| (units(k), v)).collect())
}
fn or<H: ReadHost>(
    h: &H,
    v: &H::Value,
    keys: &[&str],
    fallback: Handle<H>,
    keyed: bool,
) -> ReadResult<H, Handle<H>> {
    for k in keys {
        let x = if keyed {
            h.keyed(v, k)?
        } else {
            field(h, v, k)?
        };
        if h.truthy(&x) {
            return Ok(x);
        }
    }
    Ok(fallback)
}
fn pf<H: ReadHost>(h: &H, v: &H::Value, keys: &[&str]) -> ReadResult<H, f64> {
    h.parse_float(&*or(h, v, keys, h.number(0.0), false)?)
}
fn coalesce<H: ReadHost>(
    h: &H,
    v: Handle<H>,
    fallback: impl FnOnce() -> ReadResult<H, Handle<H>>,
) -> ReadResult<H, Handle<H>> {
    if h.nullish(&v) { fallback() } else { Ok(v) }
}
fn max0(x: f64) -> f64 {
    if x.is_nan() {
        x
    } else if x <= 0.0 {
        0.0
    } else {
        x
    }
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}
fn empty<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    obj(h, vec![])
}
fn endpoint(spot: bool, key: &str) -> &'static str {
    let table = if spot {
        crate::spot_types::ENDPOINTS
    } else {
        crate::types::ENDPOINTS
    };
    table
        .iter()
        .find(|(k, _)| *k == key)
        .expect("source endpoint")
        .1
}
fn trace_fallback<H: ReadHost>(
    h: &H,
    result: ReadResult<H, Handle<H>>,
    slot: &str,
    false_empty: bool,
) -> ReadResult<H, Handle<H>> {
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            h.trace(&e)?;
            let v = h.current(slot)?;
            Ok(if false_empty && !h.truthy(&v) {
                h.array(vec![])
            } else {
                v
            })
        }
    }
}
fn limits<H: ReadHost>(h: &H, minimum: f64, leverage: Handle<H>) -> ReadResult<H, Handle<H>> {
    obj(
        h,
        vec![
            (
                "amount",
                obj(
                    h,
                    vec![
                        ("min", h.number(minimum)),
                        ("max", h.number(9007199254740991.0)),
                    ],
                )?,
            ),
            (
                "leverage",
                obj(h, vec![("min", h.number(1.0)), ("max", leverage)])?,
            ),
        ],
    )
}

pub async fn futures_markets<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    let result = async {
        let data = h
            .read(false, endpoint(false, "MARKETS"), false, empty(h)?)?
            .await?;
        let source = opt(h, &data, "instruments")?;
        let rows = if h.truthy(&source) {
            h.items(&source)?
        } else {
            vec![]
        };
        let mut selected = vec![];
        for m in rows {
            if h.strict_eq(&*h.keyed(&m, "tradeable")?, &h.boolean(false)) {
                continue;
            }
            let symbol = or(h, &m, &["symbol", "symbolName"], text(h, ""), true)?;
            if h.starts_with(&symbol, "PF_")? {
                selected.push(m);
            }
        }
        let mut out = vec![];
        for m in selected {
            let symbol = or(h, &m, &["symbol", "symbolName"], text(h, ""), true)?;
            let base = or(h, &m, &["base", "underlying"], text(h, ""), true)?;
            let quote = or(h, &m, &["quote", "quoteCurrency"], text(h, "USD"), true)?;
            let decimals = h.keyed(&m, "contractValueTradePrecision")?;
            let tick = h.keyed(&m, "tickSize")?;
            let contract = or(h, &m, &["contractSize"], h.number(1.0), true)?;
            let amount = if h.nullish(&decimals) {
                1.0
            } else {
                h.amount_precision(&decimals)?
            };
            let price = if h.truthy(&tick) {
                tick
            } else {
                h.number(0.01)
            };
            let active = !eq(h, &*h.keyed(&m, "status")?, "cancel_only");
            let pretty = if h.starts_with(&symbol, "PF_")? {
                h.text(&h.string(&symbol)?[3..])
            } else {
                symbol.clone()
            };
            let levels = h.keyed(&m, "marginLevels")?;
            let mut leverage = 100.0;
            if h.is_array(&levels) {
                let levels = h.items(&levels)?;
                if let Some(first) = levels.first() {
                    let margin = or(h, first, &["initialMargin"], h.number(0.01), true)?;
                    let n = h.parse_float(&margin)?;
                    if n > 0.0 {
                        leverage = (1.0 / n).floor();
                    }
                }
            }
            let id = h.text(
                &h.string(&base)?
                    .into_iter()
                    .chain([47])
                    .chain(h.string(&quote)?)
                    .collect::<Vec<_>>(),
            );
            out.push(obj(
                h,
                vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("prettySymbol", pretty),
                    ("base", base),
                    ("quote", quote),
                    ("active", h.boolean(active)),
                    (
                        "precision",
                        obj(h, vec![("amount", h.number(amount)), ("price", price)])?,
                    ),
                    ("limits", limits(h, 0.0, h.number(leverage))?),
                    ("contractSize", contract),
                    (
                        "isExpiring",
                        h.boolean(h.truthy(&*h.keyed(&m, "lastTradingTime")?)),
                    ),
                    (
                        "isInverse",
                        h.boolean(eq(h, &*h.keyed(&m, "contractType")?, "inverse")),
                    ),
                    ("isSpot", h.boolean(false)),
                    ("info", m),
                ],
            )?);
        }
        Ok(h.array(out))
    }
    .await;
    trace_fallback(h, result, "markets", false)
}
pub async fn spot_markets<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    let result = async {
        let data = h
            .read(true, endpoint(true, "ASSET_PAIRS"), false, empty(h)?)?
            .await?;
        let pairs = opt(h, &data, "result")?;
        let pairs = if h.truthy(&pairs) { pairs } else { empty(h)? };
        let mut out = vec![];
        for (name, p) in h.entries(&pairs)? {
            if name.ends_with(&units(".d")) {
                continue;
            }
            let symbol = field(h, &p, "wsname")?;
            if !h.truthy(&symbol) {
                continue;
            }
            let (raw_base, raw_quote) = h.split_pair(&symbol)?;
            let base = h.normalize_asset(&raw_base)?;
            let quote = h.normalize_asset(&raw_quote)?;
            if !["USD", "USDT", "USDC"].iter().any(|s| eq(h, &quote, s)) {
                continue;
            }
            let price = field(h, &p, "pair_decimals")?;
            let price = if h.nullish(&price) {
                0.01
            } else {
                10.0_f64.powf(-h.numeric(&price)?)
            };
            let amount = field(h, &p, "lot_decimals")?;
            let amount = if h.nullish(&amount) {
                0.00000001
            } else {
                10.0_f64.powf(-h.numeric(&amount)?)
            };
            let minimum = h.parse_float(&*or(h, &p, &["ordermin"], text(h, "0"), false)?)?;
            let id = h.text(
                &h.string(&base)?
                    .into_iter()
                    .chain([47])
                    .chain(h.string(&quote)?)
                    .collect::<Vec<_>>(),
            );
            let pretty = h.text(
                &h.string(&base)?
                    .into_iter()
                    .chain(h.string(&quote)?)
                    .collect::<Vec<_>>(),
            );
            let info = h.spread(&p, vec![(units("pairName"), h.text(&name))])?;
            out.push(obj(
                h,
                vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("prettySymbol", pretty),
                    ("base", base),
                    ("quote", quote),
                    (
                        "active",
                        h.boolean(eq(h, &*field(h, &p, "status")?, "online")),
                    ),
                    (
                        "precision",
                        obj(
                            h,
                            vec![("amount", h.number(amount)), ("price", h.number(price))],
                        )?,
                    ),
                    ("limits", limits(h, minimum, h.number(1.0))?),
                    ("contractSize", h.number(1.0)),
                    ("isSpot", h.boolean(true)),
                    ("isInverse", h.boolean(false)),
                    ("info", info),
                ],
            )?);
        }
        Ok(h.array(out))
    }
    .await;
    trace_fallback(h, result, "markets", false)
}
pub async fn spot_tickers<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    let result = async {
        let data = h
            .read(true, endpoint(true, "TICKER"), false, empty(h)?)?
            .await?;
        let result = opt(h, &data, "result")?;
        let result = if h.truthy(&result) { result } else { empty(h)? };
        let mut out = vec![];
        for market in h.items(&*h.current("markets")?)? {
            let pair = opt(h, &*field(h, &market, "info")?, "pairName")?;
            if !h.truthy(&pair) {
                continue;
            }
            let t = h.field(&result, &h.string(&pair)?, false)?;
            if !h.truthy(&t) {
                continue;
            }
            let number = |key, index: &str| -> ReadResult<H, f64> {
                let x = field(h, &t, key)?;
                let x = h.field(&x, &units(index), true)?;
                h.parse_float(&*if h.truthy(&x) { x } else { text(h, "0") })
            };
            let ask = number("a", "0")?;
            let bid = number("b", "0")?;
            let last = number("c", "0")?;
            let volume = number("v", "1")?;
            let open = h.parse_float(&*or(h, &t, &["o"], text(h, "0"), false)?)?;
            let percentage = if open > 0.0 {
                ((last - open) / open) * 100.0
            } else {
                0.0
            };
            out.push(obj(
                h,
                vec![
                    ("id", field(h, &market, "id")?),
                    ("symbol", field(h, &market, "symbol")?),
                    ("bid", h.number(bid)),
                    ("ask", h.number(ask)),
                    ("last", h.number(last)),
                    ("mark", h.number(last)),
                    ("index", h.number(last)),
                    ("percentage", h.number(percentage)),
                    ("fundingRate", h.number(0.0)),
                    ("fundingTime", h.number(0.0)),
                    ("volume", h.number(volume)),
                    ("quoteVolume", h.number(volume * last)),
                    ("openInterest", h.number(0.0)),
                    ("info", t),
                ],
            )?);
        }
        Ok(h.array(out))
    }
    .await;
    trace_fallback(h, result, "tickers", false)
}
pub async fn spot_orders<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    let result = async {
        let data = h
            .read(true, endpoint(true, "OPEN_ORDERS"), true, empty(h)?)?
            .await?;
        let open = opt(h, &*opt(h, &data, "result")?, "open")?;
        let open = if h.truthy(&open) { open } else { empty(h)? };
        let mut identities: Vec<(Vec<u16>, Handle<H>)> = vec![];
        for market in h.items(&*h.current("markets")?)? {
            let info = field(h, &market, "info")?;
            for identity in [
                field(h, &market, "symbol")?,
                opt(h, &info, "pairName")?,
                opt(h, &info, "altname")?,
                field(h, &market, "prettySymbol")?,
            ] {
                if h.truthy(&identity) {
                    let key = h.string(&identity)?;
                    if let Some(row) = identities.iter_mut().find(|(k, _)| *k == key) {
                        row.1 = market.clone();
                    } else {
                        identities.push((key, market.clone()));
                    }
                }
            }
        }
        let mut out = vec![];
        for (txid, o) in h.entries(&open)? {
            let descr = or(h, &o, &["descr"], empty(h)?, false)?;
            let pair = field(h, &descr, "pair")?;
            let market = identities
                .iter()
                .find(|(k, _)| h.strict_eq(&h.text(k), &pair))
                .map(|(_, m)| m.clone());
            let typ = h.order_type(&*field(h, &descr, "ordertype")?)?;
            let side = if eq(h, &*field(h, &descr, "type")?, "buy") {
                "buy"
            } else {
                "sell"
            };
            let price = h.parse_float(&*or(h, &descr, &["price"], text(h, "0"), false)?)?;
            let volume = h.parse_float(&*or(h, &o, &["vol"], text(h, "0"), false)?)?;
            let filled = h.parse_float(&*or(h, &o, &["vol_exec"], text(h, "0"), false)?)?;
            let flags = field(h, &o, "oflags")?;
            let tif = if !h.nullish(&flags) && h.includes(&flags, "post")? {
                "PostOnly"
            } else {
                "GoodTillCancel"
            };
            let time = field(h, &o, "opentm")?;
            let time = if h.truthy(&time) {
                h.numeric(&*field(h, &o, "opentm")?)? * 1000.0
            } else {
                h.now()?
            };
            let symbol = match market {
                Some(m) => or(
                    h,
                    &m,
                    &["symbol"],
                    or(h, &descr, &["pair"], text(h, ""), false)?,
                    false,
                )?,
                None => or(h, &descr, &["pair"], text(h, ""), false)?,
            };
            out.push(obj(
                h,
                vec![
                    ("id", h.text(&txid)),
                    ("status", text(h, "open")),
                    ("symbol", symbol),
                    ("type", typ),
                    ("side", text(h, side)),
                    ("price", h.number(price)),
                    ("amount", h.number(volume)),
                    ("filled", h.number(filled)),
                    ("remaining", h.number(max0(volume - filled))),
                    ("reduceOnly", h.boolean(false)),
                    ("info", o),
                    ("timeInForce", text(h, tif)),
                    ("lastUpdate", h.number(time)),
                ],
            )?);
        }
        Ok(h.array(out))
    }
    .await;
    trace_fallback(h, result, "orders", true)
}

fn margin<H: ReadHost>(
    h: &H,
    p: &H::Value,
    symbol: Handle<H>,
) -> ReadResult<H, (Handle<H>, Handle<H>)> {
    let cached = h.preference("margin", &symbol)?;
    let maximum = field(h, p, "maxFixedLeverage")?;
    let maximum = if h.undefined(&maximum) {
        None
    } else {
        Some(h.parse_float(&*field(h, p, "maxFixedLeverage")?)?)
    };
    let detected = text(
        h,
        if maximum.is_some() {
            "isolated"
        } else {
            "cross"
        },
    );
    let mode = if h.nullish(&cached) {
        detected.clone()
    } else {
        cached.clone()
    };
    if !h.truthy(&cached) {
        h.set_preference(symbol.clone(), detected)?;
    }
    // eq is an observable host read and the source's `&&` evaluates it first,
    // so the Option is narrowed only after it has run.
    let fixed = if eq(h, &mode, "isolated") {
        maximum
    } else {
        None
    };
    let leverage = if let Some(maximum) = fixed {
        h.number(maximum)
    } else {
        let cached = h.preference("leverage", &symbol)?;
        if !h.undefined(&cached) {
            cached
        } else {
            let markets = h.current("marketsBySymbol")?;
            let market = h.field(&markets, &h.string(&symbol)?, false)?;
            let max = if h.nullish(&market) {
                text(h, "undefined")
            } else {
                field(
                    h,
                    &*field(h, &*field(h, &market, "limits")?, "leverage")?,
                    "max",
                )?
            };
            if h.nullish(&market) || h.nullish(&max) {
                h.number(5.0)
            } else {
                max
            }
        }
    };
    Ok((mode, leverage))
}
fn fake_position<H: ReadHost>(
    h: &H,
    market: &H::Value,
    side: &str,
    mode: Handle<H>,
    leverage: Handle<H>,
) -> ReadResult<H, Handle<H>> {
    let symbol = field(h, market, "symbol")?;
    let id = h.text(
        &h.string(&symbol)?
            .into_iter()
            .chain([58])
            .chain(side.encode_utf16())
            .collect::<Vec<_>>(),
    );
    obj(
        h,
        vec![
            ("id", id),
            ("symbol", symbol),
            ("side", text(h, side)),
            ("entryPrice", h.number(0.0)),
            ("notional", h.number(0.0)),
            ("leverage", leverage),
            ("unrealizedPnl", h.number(0.0)),
            ("contracts", h.number(0.0)),
            ("liquidationPrice", h.number(0.0)),
            ("marginType", mode),
            ("positionMode", text(h, "oneway")),
            ("info", empty(h)?),
        ],
    )
}
pub async fn futures_positions<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    let result = async {
        let data = h
            .read(false, endpoint(false, "POSITIONS"), false, empty(h)?)?
            .await?;
        let rows = opt(h, &data, "openPositions")?;
        let rows = if h.truthy(&rows) {
            h.items(&rows)?
        } else {
            vec![]
        };
        let mut out = vec![];
        let mut symbols: Vec<Handle<H>> = vec![];
        for p in rows {
            let symbol = h.text(&h.string(&*or(h, &p, &["symbol"], text(h, ""), false)?)?);
            let side = if h.lower(&h.string(&*or(h, &p, &["side"], text(h, "long"), false)?)?)
                == units("long")
            {
                "long"
            } else {
                "short"
            };
            let id = h.text(
                &h.string(&symbol)?
                    .into_iter()
                    .chain([58])
                    .chain(side.encode_utf16())
                    .collect::<Vec<_>>(),
            );
            let mut existing = None;
            for pos in h.items(&*h.current("positions")?)? {
                if h.strict_eq(&*field(h, &pos, "id")?, &id) {
                    existing = Some(pos);
                    break;
                }
            }
            let size = coalesce(h, field(h, &p, "size")?, || Ok(h.number(0.0)))?;
            let contracts = h.parse_float(&size)?;
            let mut mark = None;
            for t in h.items(&*h.current("tickers")?)? {
                if h.strict_eq(&*field(h, &t, "symbol")?, &symbol) {
                    mark = Some(field(h, &t, "mark")?);
                    break;
                }
            }
            let (mode, leverage) = margin(h, &p, symbol.clone())?;
            let old_notional = existing
                .as_ref()
                .map(|v| field(h, v, "notional"))
                .transpose()?;
            let notional = match old_notional {
                Some(v) if !h.nullish(&v) => v,
                _ => h.number(match mark {
                    Some(m) if h.truthy(&m) => (contracts * h.numeric(&m)?).abs(),
                    _ => 0.0,
                }),
            };
            let liquidation = existing
                .as_ref()
                .map(|v| field(h, v, "liquidationPrice"))
                .transpose()?;
            let liquidation = match liquidation {
                Some(v) if !h.nullish(&v) => v,
                _ => h.number(0.0),
            };
            let entry =
                h.parse_float(&*coalesce(h, field(h, &p, "price")?, || Ok(h.number(0.0)))?)?;
            let pnl = h.parse_float(&*coalesce(h, field(h, &p, "unrealizedPnl")?, || {
                Ok(h.number(0.0))
            })?)?;
            symbols.push(symbol.clone());
            out.push(obj(
                h,
                vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("side", text(h, side)),
                    ("entryPrice", h.number(entry)),
                    ("notional", notional),
                    ("leverage", leverage),
                    ("unrealizedPnl", h.number(pnl)),
                    ("contracts", h.number(contracts)),
                    ("liquidationPrice", liquidation),
                    ("marginType", mode),
                    ("positionMode", text(h, "oneway")),
                    ("info", p),
                ],
            )?);
        }
        for market in h.items(&*h.current("markets")?)? {
            if !h.truthy(&*field(h, &market, "active")?) || h.truthy(&*field(h, &market, "isSpot")?)
            {
                continue;
            }
            let symbol = field(h, &market, "symbol")?;
            if symbols.iter().any(|s| h.strict_eq(s, &symbol)) {
                continue;
            }
            let mode = coalesce(h, h.preference("margin", &symbol)?, || Ok(text(h, "cross")))?;
            let leverage = h.preference("leverage", &symbol)?;
            let leverage = if !h.undefined(&leverage) {
                leverage
            } else {
                field(
                    h,
                    &*field(h, &*field(h, &market, "limits")?, "leverage")?,
                    "max",
                )?
            };
            for side in ["long", "short"] {
                out.push(fake_position(
                    h,
                    &market,
                    side,
                    mode.clone(),
                    leverage.clone(),
                )?);
            }
        }
        Ok(h.array(out))
    }
    .await;
    trace_fallback(h, result, "positions", true)
}
fn currency<H: ReadHost>(
    h: &H,
    free: f64,
    used: f64,
    total: f64,
    pnl: f64,
    info: Handle<H>,
) -> ReadResult<H, Handle<H>> {
    obj(
        h,
        vec![
            ("free", h.number(free)),
            ("used", h.number(used)),
            ("total", h.number(total)),
            ("upnl", h.number(pnl)),
            ("info", info),
        ],
    )
}
pub async fn futures_balance<H: ReadHost>(h: &H) -> ReadResult<H, Handle<H>> {
    let result = async {
        let data = h
            .read(false, endpoint(false, "ACCOUNT"), false, empty(h)?)?
            .await?;
        let result = opt(h, &data, "result")?;
        if !eq(h, &result, "success") {
            return Err(h.error(if h.truthy(&result) {
                opt(h, &data, "result")?
            } else {
                text(h, "Failed to fetch balance")
            }));
        }
        let accounts = opt(h, &data, "accounts")?;
        let accounts = if h.truthy(&accounts) {
            accounts
        } else {
            empty(h)?
        };
        let mut total = 0.0;
        let mut free = 0.0;
        let mut used = 0.0;
        let mut pnl = 0.0;
        let currencies = empty(h)?;
        let cash = or(h, &accounts, &["cash", "cashAccount"], text(h, ""), false)?;
        let balances = opt(h, &cash, "balances")?;
        if h.truthy(&balances) {
            for (name, balance) in h.entries(&*field(h, &cash, "balances")?)? {
                let balance = if h.truthy(&balance) {
                    balance
                } else {
                    h.number(0.0)
                };
                let amount = h.parse_float(&h.text(&h.string(&balance)?))?;
                if amount > 0.0 {
                    let name = h.upper(&name);
                    let info = obj(h, vec![("balance", h.number(amount))])?;
                    h.assign(
                        &currencies,
                        &name,
                        currency(h, amount, 0.0, amount, 0.0, info)?,
                    )?;
                    if ["USD", "USDT", "USDC"].iter().any(|s| name == units(s)) {
                        total += amount;
                        free += amount;
                    }
                }
            }
        }
        let flex = or(
            h,
            &accounts,
            &["flex", "multiCollateralMarginAccount"],
            text(h, ""),
            false,
        )?;
        if h.truthy(&flex)
            && eq(
                h,
                &*field(h, &flex, "type")?,
                "multiCollateralMarginAccount",
            )
        {
            let portfolio = pf(h, &flex, &["portfolioValue"])?;
            let available = pf(h, &flex, &["availableMargin"])?;
            let balance = pf(h, &flex, &["balanceValue"])?;
            let profit = pf(h, &flex, &["pnl"])?;
            let initial = pf(h, &flex, &["initialMargin"])?;
            let maintenance = pf(h, &flex, &["maintenanceMargin"])?;
            if h.truthy(&*field(h, &flex, "currencies")?) {
                for (name, details) in h.entries(&*field(h, &flex, "currencies")?)? {
                    if !h.is_object(&details) || h.nullish(&details) {
                        continue;
                    }
                    let quantity = pf(h, &details, &["quantity"])?;
                    let value = pf(h, &details, &["value"])?;
                    let available = pf(h, &details, &["available"])?;
                    let collateral = pf(h, &details, &["collateral"])?;
                    if quantity > 0.0 || value > 0.0 {
                        let info = obj(
                            h,
                            vec![
                                ("free", h.number(available)),
                                ("used", h.number(quantity - available)),
                                ("total", h.number(quantity)),
                                ("upnl", h.number(0.0)),
                                ("value", h.number(value)),
                                ("collateral", h.number(collateral)),
                                ("info", details),
                            ],
                        )?;
                        h.assign(&currencies, &h.upper(&name), info)?;
                    }
                }
            }
            let chosen = if portfolio > 0.0 {
                Some(portfolio)
            } else if balance > 0.0 {
                Some(balance)
            } else {
                None
            };
            if let Some(chosen) = chosen {
                let consumed = initial + maintenance;
                let available = max0(available);
                let old = field(h, &currencies, "USD")?;
                let info = opt(h, &old, "info")?;
                let info = if h.truthy(&info) { info } else { empty(h)? };
                let summary = h.spread(
                    &old,
                    vec![
                        (units("free"), h.number(available)),
                        (units("used"), h.number(consumed)),
                        (units("total"), h.number(chosen)),
                        (units("upnl"), h.number(profit)),
                        (units("info"), info),
                    ],
                )?;
                h.assign(&currencies, &units("USD"), summary)?;
                total = chosen;
                free = available;
                used = consumed;
                pnl = profit;
            }
        }
        for (name, account) in h.entries(&accounts)? {
            if name.starts_with(&units("fi_")) && h.is_object(&account) {
                let aux = or(h, &account, &["auxiliary"], empty(h)?, false)?;
                let value = pf(h, &aux, &["pv"])?;
                let available = pf(h, &aux, &["af"])?;
                let profit = pf(h, &aux, &["pnl"])?;
                if value > 0.0 {
                    let denomination = or(h, &aux, &["currency"], text(h, "USD"), false)?;
                    let key = h.string(&denomination)?;
                    if !h.truthy(&*h.field(&currencies, &key, false)?) {
                        h.assign(
                            &currencies,
                            &key,
                            currency(h, 0.0, 0.0, 0.0, 0.0, empty(h)?)?,
                        )?;
                    }
                    let entry = h.field(&currencies, &key, false)?;
                    for (key, increment) in
                        [("total", value), ("free", available), ("upnl", profit)]
                    {
                        let old = h.numeric(&*field(h, &entry, key)?)?;
                        h.assign(&entry, &units(key), h.number(old + increment))?;
                    }
                    if eq(h, &denomination, "USD") {
                        total += value;
                        free += available;
                        pnl += profit;
                    }
                }
            }
        }
        if total == 0.0 {
            for (_, account) in h.entries(&accounts)? {
                if h.is_object(&account) && h.truthy(&*field(h, &account, "auxiliary")?) {
                    let aux = field(h, &account, "auxiliary")?;
                    let value = pf(h, &aux, &["pv", "portfolioValue"])?;
                    let available = pf(h, &aux, &["af", "availableMargin"])?;
                    if value > 0.0 {
                        total = value;
                        free = available;
                        used = value - available;
                        pnl = pf(h, &aux, &["pnl"])?;
                        h.assign(
                            &currencies,
                            &units("USD"),
                            currency(h, free, used, total, pnl, account)?,
                        )?;
                        break;
                    }
                }
            }
        }
        obj(
            h,
            vec![
                ("free", h.number(free)),
                ("used", h.number(used)),
                ("total", h.number(total)),
                ("upnl", h.number(pnl)),
                ("currencies", currencies),
                ("info", opt(h, &data, "accounts")?),
            ],
        )
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            h.emit_error(h.error_message(&e, "Failed to fetch balance")?)?;
            h.current("balance")
        }
    }
}
pub async fn spot_refresh_balance<H: ReadHost>(h: &H) -> ReadResult<H, ()> {
    let result = async {
        let balance = h.fetch_balance()?.await?;
        let loaded = h.current("loaded")?;
        let loaded = h.spread(&loaded, vec![(units("balance"), h.boolean(true))])?;
        h.update_balance(obj(h, vec![("balance", balance), ("loaded", loaded)])?)
    }
    .await;
    // Preserve the source's retry-on-next-tick catch, including update failures.
    match result {
        Ok(()) | Err(_) => Ok(()),
    }
}
pub async fn futures_fills<H: ReadHost>(h: &H, opts: &H::Value) -> ReadResult<H, Handle<H>> {
    if h.truthy(&*h.current("publicDataOnly")?) {
        return Ok(h.array(vec![]));
    }
    let to = coalesce(h, field(h, opts, "to")?, || Ok(h.number(h.now()?)))?;
    let from = coalesce(h, field(h, opts, "from")?, || {
        Ok(h.number(h.numeric(&to)? - 604800000.0))
    })?;
    let mut cursor: Option<Vec<u16>> = None;
    let mut out = vec![];
    let mut seen: Vec<Handle<H>> = vec![];
    let mut page = 0;
    let mut reached = false;
    let result = async {
        while page < 50 {
            let params = match cursor.as_ref() {
                Some(c) => obj(h, vec![("lastFillTime", h.text(c))])?,
                None => empty(h)?,
            };
            let data = h
                .read(false, endpoint(false, "FILLS"), false, params)?
                .await?;
            if h.truthy(&*h.current("disposed")?) {
                return Ok(h.array(vec![]));
            }
            if !h.is_array(&*opt(h, &data, "fills")?) {
                let detail = coalesce(h, opt(h, &data, "error")?, || {
                    coalesce(h, opt(h, &data, "result")?, || {
                        let d = if h.nullish(&data) {
                            empty(h)?
                        } else {
                            data.clone()
                        };
                        let keys = h
                            .entries(&d)?
                            .into_iter()
                            .map(|(k, _)| k)
                            .collect::<Vec<_>>();
                        Ok(h.text(
                            &keys
                                .into_iter()
                                .enumerate()
                                .flat_map(|(i, k)| if i == 0 { k } else { [vec![44], k].concat() })
                                .collect::<Vec<_>>(),
                        ))
                    })
                })?;
                let message = h.text(
                    &units("kraken fills did not return an array — ")
                        .into_iter()
                        .chain(h.string(&detail)?)
                        .collect::<Vec<_>>(),
                );
                return Err(h.error(message));
            }
            let rows = field(h, &data, "fills")?;
            let len = h.items(&rows)?.len();
            if len == 0 {
                reached = true;
                break;
            }
            let mut oldest = f64::INFINITY;
            let mut fresh = 0;
            for fill in h.map_fills(rows)? {
                let milliseconds = h.numeric(&*field(h, &fill, "timestamp")?)? * 1000.0;
                oldest = min(oldest, milliseconds);
                if milliseconds < h.numeric(&from)? || milliseconds > h.numeric(&to)? {
                    continue;
                }
                let id = field(h, &fill, "id")?;
                if seen.iter().any(|s| h.strict_eq(s, &id)) {
                    continue;
                }
                seen.push(id);
                out.push(fill);
                fresh += 1;
            }
            if !oldest.is_finite() {
                break;
            }
            if oldest <= h.numeric(&from)? {
                reached = true;
                break;
            }
            if fresh == 0 && len < 100 {
                reached = true;
                break;
            }
            cursor = Some(h.iso(oldest + 1.0)?);
            page += 1;
        }
        if !reached {
            h.warn_fills(page, h.iso(h.numeric(&from)?)?)?;
        }
        let mut keyed = Vec::with_capacity(out.len());
        for fill in out {
            keyed.push((h.numeric(&*field(h, &fill, "timestamp")?)?, fill));
        }
        keyed.sort_by(|a, b| {
            let d = b.0 - a.0;
            if d.is_nan() {
                std::cmp::Ordering::Equal
            } else {
                d.partial_cmp(&0.0).expect("not NaN")
            }
        });
        let sorted = keyed.into_iter().map(|(_, fill)| fill).collect::<Vec<_>>();
        let limit = field(h, opts, "limit")?;
        if h.truthy(&limit) && h.numeric(&*field(h, opts, "limit")?)? > 0.0 {
            h.slice_limit(sorted, &*field(h, opts, "limit")?)
        } else {
            Ok(h.array(sorted))
        }
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            h.emit_error(h.error_message(&e, "Failed to fetch executions")?)?;
            Ok(h.array(vec![]))
        }
    }
}
