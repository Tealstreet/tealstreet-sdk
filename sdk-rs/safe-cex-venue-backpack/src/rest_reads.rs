//! REST method bodies over compatible value, transport, date and store operations.
use crate::public_control::JsString;
use safe_cex_types::{MarginType, OrderSide, OrderType, PositionMode, PositionSide};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc, task::Poll};

pub type Reply<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
pub type Task<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
#[derive(Clone, Debug)]
pub enum Param<V> {
    Value(V),
    Number(f64),
    Text(JsString),
    Bool(bool),
    Undefined,
}
#[derive(Clone, Debug)]
pub struct Request<V> {
    pub endpoint: &'static str,
    pub params: Vec<(&'static str, Param<V>)>,
    pub data: Vec<(&'static str, Param<V>)>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Array: Clone + 'static;
    fn array_length(&mut self, a: &Self::Array) -> Result<usize, Self::Error>;
    fn array_element(
        &mut self,
        a: &Self::Array,
        index: usize,
    ) -> Result<Option<Self::Value>, Self::Error>;
    fn array_set(
        &mut self,
        a: &Self::Array,
        index: usize,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn array_from(&mut self, values: Vec<Option<Self::Value>>) -> Self::Array;
    type Error: 'static;
    type LogTarget;
    type Order: 'static;
    type TransportTarget;
    type TriggerTarget;
    fn transport_target(&mut self, method: &str) -> Result<Self::TransportTarget, Self::Error>;
    fn trigger_target(&mut self) -> Result<Self::TriggerTarget, Self::Error>;
    fn field(&mut self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn array(&mut self, value: &Self::Value) -> Result<Self::Array, Self::Error>;
    fn is_array(&mut self, value: &Self::Value) -> bool;
    fn truthy(&mut self, value: &Self::Value) -> bool;
    fn nullish(&mut self, value: &Self::Value) -> bool;
    fn strict_equal(&mut self, left: &Self::Value, right: &Self::Value) -> bool;
    fn is_text(&mut self, value: &Self::Value, text: &str) -> bool;
    fn is_false(&mut self, value: &Self::Value) -> bool;
    fn string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn parse_float(&mut self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn number(&mut self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn undefined(&mut self) -> Self::Value;
    fn zero(&mut self) -> Self::Value;
    fn is_undefined(&mut self, value: &Self::Value) -> bool;
    fn is_null(&mut self, value: &Self::Value) -> bool;
    fn pair(&mut self, value: &Self::Value) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn spread(&mut self, values: &[Self::Value]) -> Result<Self::Value, Self::Error>;
    fn get(
        &mut self,
        target: Self::TransportTarget,
        request: Request<Self::Value>,
    ) -> Result<Reply<Self::Value, Self::Error>, Self::Error>;
    fn delete(
        &mut self,
        target: Self::TransportTarget,
        request: Request<Self::Value>,
    ) -> Result<Reply<Self::Value, Self::Error>, Self::Error>;
    fn post(
        &mut self,
        target: Self::TransportTarget,
        request: Request<Self::Value>,
    ) -> Result<Reply<Self::Value, Self::Error>, Self::Error>;
    fn log_target(&mut self) -> Result<Self::LogTarget, Self::Error>;
    fn log(
        &mut self,
        target: Self::LogTarget,
        text: JsString,
        severity: Severity,
    ) -> Result<(), Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
    fn error_message(&mut self, error: &Self::Error) -> Result<Option<JsString>, Self::Error>;
    fn stringify(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn error(&mut self, text: &str) -> Self::Error;
    fn date_parse(&mut self, value: JsString) -> Result<f64, Self::Error>;
    fn clock(&mut self) -> Result<f64, Self::Error>;
    fn public_only(&mut self) -> Result<bool, Self::Error>;
    fn market(&mut self, symbol: &[u16]) -> Result<Option<Self::Value>, Self::Error>;
    fn markets(&mut self) -> Result<Self::Array, Self::Error>;
    fn position_mode(&mut self) -> Result<Task<PositionMode, Self::Error>, Self::Error>;
    fn leverage(&mut self) -> Result<f64, Self::Error>;
    fn position_id(
        &mut self,
        symbol: &[u16],
        side: PositionSide,
        mode: PositionMode,
    ) -> Result<JsString, Self::Error>;
    fn repair(&mut self, orders: Self::Array) -> Result<Task<(), Self::Error>, Self::Error>;
    fn map_order(&mut self, order: Self::Value) -> Result<Self::Order, Self::Error>;
    fn trigger_type(
        &mut self,
        target: Self::TriggerTarget,
        symbol: Self::Value,
        side: OrderSide,
        price: f64,
    ) -> Result<OrderType, Self::Error>;
    fn trigger_by(&mut self, take_profit: bool) -> Result<JsString, Self::Error>;
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn cat(parts: &[&[u16]]) -> JsString {
    parts.iter().flat_map(|s| s.iter().copied()).collect()
}
fn request<V>(key: &str, params: Vec<(&'static str, Param<V>)>) -> Request<V> {
    Request {
        endpoint: crate::constants::ENDPOINTS
            .iter()
            .find(|(k, _)| *k == key)
            .expect("source endpoint")
            .1,
        params,
        data: vec![],
    }
}
fn get<H: Host>(
    h: &mut H,
    request: Request<H::Value>,
) -> Result<Reply<H::Value, H::Error>, H::Error> {
    let target = h.transport_target("get")?;
    h.get(target, request)
}
fn log<H: Host>(h: &mut H, text: JsString, severity: Severity) -> Result<(), H::Error> {
    let target = h.log_target()?;
    h.log(target, text, severity)
}
fn caught<H: Host, T>(h: &mut H, error: H::Error, prefix: &str) -> Result<T, H::Error> {
    let target = h.log_target()?;
    let text = h.error_text(&error)?;
    h.log(target, cat(&[&u(prefix), &text]), Severity::Error)?;
    Err(error)
}
fn field_num<H: Host>(h: &mut H, v: &H::Value, k: &str) -> Result<f64, H::Error> {
    let v = h.field(v, k)?;
    h.parse_float(&v)
}
fn field_text<H: Host>(h: &mut H, v: &H::Value, k: &str) -> Result<JsString, H::Error> {
    let v = h.field(v, k)?;
    h.string(&v)
}
fn fallback<H: Host>(
    h: &mut H,
    v: &H::Value,
    keys: &[&str],
    default: &str,
) -> Result<f64, H::Error> {
    for key in keys {
        let mut value = v.clone();
        for part in key.split('.') {
            if h.nullish(&value) {
                value = h.undefined();
                break;
            }
            value = h.field(&value, part)?;
        }
        if h.truthy(&value) {
            return h.parse_float(&value);
        }
    }
    Ok(safe_cex_num::parse_float(default))
}
fn map_array<H: Host, T>(
    h: &mut H,
    a: &H::Array,
    mut map: impl FnMut(&mut H, H::Value) -> Result<T, H::Error>,
) -> Result<Vec<Option<T>>, H::Error> {
    let len = h.array_length(&a)?;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let value = h.array_element(&a, i)?;
        out.push(match value {
            Some(v) => Some(map(h, v)?),
            None => None,
        });
    }
    Ok(out)
}
#[derive(Debug)]
pub struct Market<V> {
    pub id: JsString,
    pub symbol: JsString,
    pub base: JsString,
    pub quote: Option<JsString>,
    pub active: bool,
    pub kind: &'static str,
    pub spot: bool,
    pub swap: bool,
    pub future: bool,
    pub contract: bool,
    pub linear: bool,
    pub inverse: bool,
    pub contract_size: f64,
    pub is_spot: bool,
    pub precision: [f64; 4],
    pub limits: [[f64; 2]; 4],
    pub info: V,
}
// ts: BackpackExchange.fetchMarkets
pub fn fetch_markets<H: Host>(host: Rc<RefCell<H>>) -> Task<Vec<Market<H::Value>>, H::Error> {
    let response = get(&mut *host.borrow_mut(), request("MARKETS", vec![]));
    if let Err(e) = response {
        return Box::pin(std::future::ready(caught(
            &mut *host.borrow_mut(),
            e,
            "Error fetching markets: ",
        )));
    }
    Box::pin(async move {
        let result = async {
            let data = response.unwrap_or_else(|_| unreachable!()).await?;
            let a = host.borrow_mut().array(&data)?;
            let filtered = {
                let mut h = host.borrow_mut();
                let len = h.array_length(&a)?;
                let mut rows = vec![];
                for i in 0..len {
                    let Some(m) = h.array_element(&a, i)? else {
                        continue;
                    };
                    let symbol = field_text(&mut *h, &m, "symbol")?;
                    if symbol.contains(&95) {
                        let visible = h.field(&m, "visible")?;
                        if !h.is_false(&visible) {
                            rows.push(m);
                        }
                    }
                }
                rows
            };
            let mut h = host.borrow_mut();
            let mut out = vec![];
            for m in filtered {
                let symbol = field_text(&mut *h, &m, "symbol")?;
                let mut parts = symbol.split(|c| *c == 95);
                let base = parts.next().unwrap().to_vec();
                let quote = parts.next().map(<[u16]>::to_vec);
                let mt = h.field(&m, "marketType")?;
                let perp = h.is_text(&mt, "PERP");
                let imf = h.field(&m, "imfFunction")?;
                let b = if h.nullish(&imf) {
                    h.undefined()
                } else {
                    h.field(&imf, "base")?
                };
                let imf_base = if h.truthy(&b) {
                    let imf = h.field(&m, "imfFunction")?;
                    let b = h.field(&imf, "base")?;
                    h.parse_float(&b)?
                } else {
                    1.0
                };
                let leverage = if imf_base > 0.0 {
                    safe_cex_num::math_round(1.0 / imf_base)
                } else {
                    1.0
                };
                let id = field_text(&mut *h, &m, "symbol")?;
                let output_symbol = field_text(&mut *h, &m, "symbol")?;
                let precision = [
                    fallback(
                        &mut *h,
                        &m,
                        &["quantityIncrement", "filters.quantity.stepSize"],
                        "0.001",
                    )?,
                    fallback(
                        &mut *h,
                        &m,
                        &["priceIncrement", "filters.price.tickSize"],
                        "0.01",
                    )?,
                    8.0,
                    8.0,
                ];
                let limits = [
                    [
                        fallback(
                            &mut *h,
                            &m,
                            &["minQuantity", "filters.quantity.minQuantity"],
                            "0",
                        )?,
                        fallback(
                            &mut *h,
                            &m,
                            &["maxQuantity", "filters.quantity.maxQuantity"],
                            "0",
                        )?,
                    ],
                    [
                        fallback(&mut *h, &m, &["filters.price.minPrice"], "0")?,
                        fallback(&mut *h, &m, &["filters.price.maxPrice"], "0")?,
                    ],
                    [
                        fallback(&mut *h, &m, &["minNotional"], "0")?,
                        fallback(&mut *h, &m, &["maxNotional"], "0")?,
                    ],
                    [1.0, if perp { leverage } else { 1.0 }],
                ];
                out.push(Market {
                    id,
                    symbol: output_symbol,
                    base,
                    quote,
                    active: true,
                    kind: if perp { "swap" } else { "spot" },
                    spot: !perp,
                    swap: perp,
                    future: false,
                    contract: perp,
                    linear: perp,
                    inverse: false,
                    contract_size: 1.0,
                    is_spot: !perp,
                    precision,
                    limits,
                    info: m,
                });
            }
            Ok(out)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, "Error fetching markets: "),
        }
    })
}
#[derive(Debug)]
pub struct Ticker<V> {
    pub id: JsString,
    pub symbol: JsString,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub mark: f64,
    pub index: f64,
    pub percentage: f64,
    pub funding_rate: f64,
    pub funding_time: V,
    pub volume: f64,
    pub quote_volume: f64,
    pub open_interest: f64,
    pub info: V,
}
// ts: BackpackExchange.fetchTickers
pub fn fetch_tickers<H: Host>(
    host: Rc<RefCell<H>>,
) -> Task<Vec<Option<Ticker<H::Value>>>, H::Error> {
    let responses = (|| {
        let a = get(&mut *host.borrow_mut(), request("TICKERS", vec![]))?;
        let b = get(&mut *host.borrow_mut(), request("MARK_PRICES", vec![]))?;
        Ok::<_, H::Error>((a, b))
    })();
    let (mut a, mut b) = match responses {
        Ok(v) => v,
        Err(e) => {
            return Box::pin(std::future::ready(caught(
                &mut *host.borrow_mut(),
                e,
                "Error fetching tickers: ",
            )));
        }
    };
    Box::pin(async move {
        let result = async {
            let mut av = None;
            let mut bv = None;
            std::future::poll_fn(|cx| {
                if av.is_none() {
                    match a.as_mut().poll(cx) {
                        Poll::Ready(Ok(v)) => av = Some(v),
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => {}
                    }
                }
                if bv.is_none() {
                    if let Poll::Ready(v) = b.as_mut().poll(cx) {
                        bv = Some(v.ok());
                    }
                }
                if av.is_some() && bv.is_some() {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            })
            .await?;
            let mut h = host.borrow_mut();
            let tickers = h.array(&av.unwrap())?;
            let mut marks: Vec<(H::Value, H::Value)> = vec![];
            if let Some(v) = bv.unwrap() {
                let a = h.array(&v)?;
                let mut i = 0;
                loop {
                    if i >= h.array_length(&a)? {
                        break;
                    }
                    let m = h.array_element(&a, i)?;
                    i += 1;
                    let m = m.unwrap_or_else(|| h.undefined());
                    let s = h.field(&m, "symbol")?;
                    if let Some((_, old)) = marks.iter_mut().find(|(k, _)| h.strict_equal(k, &s)) {
                        *old = m;
                    } else {
                        marks.push((s, m));
                    }
                }
            }
            map_array(&mut *h, &tickers, |h, t| {
                let symbol = h.field(&t, "symbol")?;
                let mark = marks
                    .iter()
                    .find(|(s, _)| h.strict_equal(s, &symbol))
                    .map(|(_, v)| v.clone());
                let last = field_num(h, &t, "lastPrice")?;
                let id_symbol = field_text(h, &t, "symbol")?;
                let now = h.clock()?;
                let id = cat(&[
                    &id_symbol,
                    &u("-"),
                    &u(&safe_cex_num::js_number_string(now)),
                ]);
                let symbol = field_text(h, &t, "symbol")?;
                let mark_value = match &mark {
                    Some(v) => field_num(h, v, "markPrice")?,
                    None => last,
                };
                let index = match &mark {
                    Some(v) => field_num(h, v, "indexPrice")?,
                    None => last,
                };
                let percentage = field_num(h, &t, "priceChangePercent")? * 100.0;
                let funding_rate = match &mark {
                    Some(v) => field_num(h, v, "fundingRate")?,
                    None => 0.0,
                };
                let funding_time = match &mark {
                    Some(v) => h.field(v, "nextFundingTimestamp")?,
                    None => h.zero(),
                };
                let volume = field_num(h, &t, "volume")?;
                let quote_volume = field_num(h, &t, "quoteVolume")?;
                let info = match mark {
                    Some(v) => h.spread(&[t, v])?,
                    None => h.spread(&[t])?,
                };
                Ok(Ticker {
                    id,
                    symbol,
                    bid: 0.0,
                    ask: 0.0,
                    last,
                    mark: mark_value,
                    index,
                    percentage,
                    funding_rate,
                    funding_time,
                    volume,
                    quote_volume,
                    open_interest: 0.0,
                    info,
                })
            })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, "Error fetching tickers: "),
        }
    })
}
#[derive(Clone, Debug)]
pub struct OHLCVOptions {
    pub symbol: JsString,
    pub interval: JsString,
    pub from: Option<f64>,
    pub to: Option<f64>,
}
#[derive(Debug)]
pub struct Candle {
    pub timestamp: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}
fn truthy_number(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
// ts: BackpackExchange.fetchOHLCV
pub fn fetch_ohlcv<H: Host>(
    host: Rc<RefCell<H>>,
    opts: OHLCVOptions,
) -> Task<Vec<Option<Candle>>, H::Error> {
    let mut params = vec![
        ("symbol", Param::Text(opts.symbol)),
        ("interval", Param::Text(opts.interval)),
    ];
    if let Some(n) = opts.from.filter(|n| truthy_number(*n)) {
        params.push(("startTime", Param::Number((n / 1000.0).floor())));
    }
    if let Some(n) = opts.to.filter(|n| truthy_number(*n)) {
        params.push(("endTime", Param::Number((n / 1000.0).floor())));
    }
    let response = get(&mut *host.borrow_mut(), request("KLINES", params));
    if let Err(e) = response {
        return Box::pin(std::future::ready(caught(
            &mut *host.borrow_mut(),
            e,
            "Error fetching OHLCV: ",
        )));
    }
    Box::pin(async move {
        let result = async {
            let data = response.unwrap_or_else(|_| unreachable!()).await?;
            let mut h = host.borrow_mut();
            if !h.is_array(&data) {
                let target = h.log_target()?;
                let text = h.stringify(&data)?;
                h.log(
                    target,
                    cat(&[&u("OHLCV response is not an array: "), &text]),
                    Severity::Warning,
                )?;
                return Ok(vec![]);
            }
            let a = h.array(&data)?;
            map_array(&mut *h, &a, |h, k| {
                let start = field_text(h, &k, "start")?;
                Ok(Candle {
                    timestamp: h.date_parse(cat(&[&start, &u("Z")]))?,
                    open: field_num(h, &k, "open")?,
                    high: field_num(h, &k, "high")?,
                    low: field_num(h, &k, "low")?,
                    close: field_num(h, &k, "close")?,
                    volume: field_num(h, &k, "volume")?,
                })
            })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, "Error fetching OHLCV: "),
        }
    })
}
#[derive(Debug)]
pub struct Level {
    pub id: JsString,
    pub price: f64,
    pub amount: f64,
    pub total: f64,
}
#[derive(Debug)]
pub struct OrderBook {
    pub bids: Vec<Option<Level>>,
    pub asks: Vec<Option<Level>>,
}
// ts: BackpackExchange.fetchOrderBook
pub fn fetch_order_book<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: JsString,
) -> Task<OrderBook, H::Error> {
    let response = get(
        &mut *host.borrow_mut(),
        request("DEPTH", vec![("symbol", Param::Text(symbol))]),
    );
    if let Err(e) = response {
        return Box::pin(std::future::ready(caught(
            &mut *host.borrow_mut(),
            e,
            "Error fetching order book: ",
        )));
    }
    Box::pin(async move {
        let result = async {
            let data = response.unwrap_or_else(|_| unreachable!()).await?;
            let mut h = host.borrow_mut();
            let mut sides = vec![];
            for side in ["bids", "asks"] {
                let v = h.field(&data, side)?;
                let a = h.array(&v)?;
                sides.push(map_array(&mut *h, &a, |h, row| {
                    let pair = h.pair(&row)?;
                    let id = h.string(&pair.0)?;
                    Ok(Level {
                        id,
                        price: h.parse_float(&pair.0)?,
                        amount: h.parse_float(&pair.1)?,
                        total: 0.0,
                    })
                })?);
            }
            let asks = sides.pop().unwrap();
            let bids = sides.pop().unwrap();
            Ok(OrderBook { bids, asks })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, "Error fetching order book: "),
        }
    })
}
#[derive(Clone, Debug)]
pub struct ExecutionsOptions {
    pub symbol: Option<JsString>,
    pub order_id: Option<JsString>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub limit: Option<f64>,
}
#[derive(Debug)]
pub struct Execution {
    pub id: JsString,
    pub timestamp: f64,
    pub symbol: JsString,
    pub order_id: JsString,
    pub price: f64,
    pub qty: f64,
    pub side: OrderSide,
    pub commission: Option<f64>,
    pub commission_asset: Option<JsString>,
}
fn milliseconds(n: Option<f64>) -> Option<f64> {
    n.filter(|n| n.is_finite()).map(|n| {
        if n >= 1e12 {
            n.floor()
        } else {
            (n * 1000.0).floor()
        }
    })
}
// ts: BackpackExchange.fetchHistoricalFills
pub fn fetch_historical_fills<H: Host>(
    host: Rc<RefCell<H>>,
    opts: ExecutionsOptions,
) -> Task<Vec<Execution>, H::Error> {
    let prepared = (|| {
        let mut h = host.borrow_mut();
        if h.public_only()? || opts.symbol.as_ref().is_none_or(|s| s.is_empty()) {
            return Ok(None);
        }
        let Some(m) = h.market(opts.symbol.as_ref().unwrap())? else {
            return Ok(None);
        };
        let target = h.transport_target("get")?;
        let id = h.field(&m, "id")?;
        let limit = opts.limit.unwrap_or(100.0);
        let limit = if limit.is_nan() {
            limit
        } else {
            limit.clamp(1.0, 1000.0)
        };
        let number = |n: Option<f64>| n.map(Param::Number).unwrap_or(Param::Undefined);
        let params = vec![
            ("symbol", Param::Value(id)),
            (
                "orderId",
                opts.order_id
                    .clone()
                    .map(Param::Text)
                    .unwrap_or(Param::Undefined),
            ),
            ("from", number(milliseconds(opts.from))),
            ("to", number(milliseconds(opts.to))),
            ("limit", Param::Number(limit)),
            ("sortDirection", Param::Text(u("Desc"))),
        ];
        let response = h.get(target, request("FILLS_HISTORY", params))?;
        Ok::<_, H::Error>(Some((m, response)))
    })();
    Box::pin(async move {
        let Some((market, response)) = prepared? else {
            return Ok(vec![]);
        };
        let data = response.await?;
        let mut h = host.borrow_mut();
        if !h.is_array(&data) {
            return Err(h.error("Backpack fill history response is not an array"));
        }
        let a = h.array(&data)?;
        let len = h.array_length(&a)?;
        let mut out = vec![];
        for i in 0..len {
            let Some(fill) = h.array_element(&a, i)? else {
                continue;
            };
            let s = h.field(&fill, "symbol")?;
            let id = h.field(&market, "id")?;
            if !h.strict_equal(&s, &id) {
                continue;
            }
            if let Some(order_id) = opts.order_id.as_ref().filter(|s| !s.is_empty()) {
                let id = field_text(&mut *h, &fill, "orderId")?;
                if &id != order_id {
                    continue;
                }
            }
            let raw = h.field(&fill, "timestamp")?;
            let text = h.string(&raw)?;
            let date = h.date_parse(text)?;
            let timestamp = if date.is_finite() {
                (date / 1000.0).floor()
            } else {
                let n = h.number(&raw)?;
                if !n.is_finite() || n <= 0.0 {
                    f64::NAN
                } else if n >= 1e12 {
                    (n / 1000.0).floor()
                } else {
                    n.floor()
                }
            };
            let price = field_num(&mut *h, &fill, "price")?;
            let qty = field_num(&mut *h, &fill, "quantity")?;
            let side = h.field(&fill, "side")?;
            let side = if h.is_text(&side, "Bid") {
                Some(OrderSide::Buy)
            } else {
                let side = h.field(&fill, "side")?;
                if h.is_text(&side, "Ask") {
                    Some(OrderSide::Sell)
                } else {
                    None
                }
            };
            if !truthy_number(timestamp)
                || !price.is_finite()
                || price <= 0.0
                || !qty.is_finite()
                || qty <= 0.0
                || side.is_none()
            {
                continue;
            }
            let fee = field_num(&mut *h, &fill, "fee")?;
            let id = field_text(&mut *h, &fill, "tradeId")?;
            let symbol = field_text(&mut *h, &market, "symbol")?;
            let order_id = field_text(&mut *h, &fill, "orderId")?;
            let asset = h.field(&fill, "feeSymbol")?;
            let commission_asset = if h.truthy(&asset) {
                Some(h.string(&asset)?)
            } else {
                None
            };
            out.push(Execution {
                id,
                timestamp,
                symbol,
                order_id,
                price,
                qty,
                side: side.unwrap(),
                commission: fee.is_finite().then_some(fee.abs()),
                commission_asset,
            });
        }
        Ok(out)
    })
}
// ts: BackpackExchange.fetchOrders
pub fn fetch_orders<H: Host>(
    host: Rc<RefCell<H>>,
    symbols: Option<Vec<JsString>>,
) -> Task<Vec<Option<H::Order>>, H::Error> {
    let response = get(&mut *host.borrow_mut(), request("ORDERS", vec![]));
    if let Err(e) = response {
        return Box::pin(std::future::ready(caught(
            &mut *host.borrow_mut(),
            e,
            "Error fetching orders: ",
        )));
    }
    Box::pin(async move {
        let result = async {
            let data = response.unwrap_or_else(|_| unreachable!()).await?;
            let a = host.borrow_mut().array(&data)?;
            let orders = if let Some(symbols) = symbols {
                let mut h = host.borrow_mut();
                let len = h.array_length(&a)?;
                let mut rows = vec![];
                for i in 0..len {
                    let Some(v) = h.array_element(&a, i)? else {
                        continue;
                    };
                    let s = field_text(&mut *h, &v, "symbol")?;
                    if symbols.contains(&s) {
                        rows.push(Some(v));
                    }
                }
                h.array_from(rows)
            } else {
                a
            };
            let repair = host.borrow_mut().repair(orders.clone())?;
            repair.await?;
            let mut h = host.borrow_mut();
            map_array(&mut *h, &orders, |h, v| h.map_order(v))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, "Error fetching orders: "),
        }
    })
}
// ts: BackpackExchange#fixTriggerOrdersWithoutTriggerBy
pub fn fix_trigger_orders_without_trigger_by<H: Host>(
    host: Rc<RefCell<H>>,
    orders: H::Array,
) -> Task<(), H::Error> {
    let prepared = (|| {
        let mut h = host.borrow_mut();
        let len = h.array_length(&orders)?;
        let mut rows = vec![];
        for i in 0..len {
            let Some(o) = h.array_element(&orders, i)? else {
                continue;
            };
            let p = h.field(&o, "triggerPrice")?;
            let is_trigger = !h.is_undefined(&p) && {
                let p = h.field(&o, "triggerPrice")?;
                !h.is_null(&p)
            };
            let b = h.field(&o, "triggerBy")?;
            let missing = h.is_null(&b) || {
                let b = h.field(&o, "triggerBy")?;
                h.is_undefined(&b)
            };
            if is_trigger && missing {
                rows.push(o);
            }
        }
        if !rows.is_empty() {
            log(
                &mut *h,
                u(&format!(
                    "Found {} trigger order(s) with missing triggerBy - fixing them",
                    rows.len()
                )),
                Severity::Info,
            )?;
        }
        Ok::<_, H::Error>(rows)
    })();
    let first = prepared.and_then(|rows| {
        let mut index = 0;
        let pending = advance_repair(&mut *host.borrow_mut(), &rows, &mut index)?;
        Ok((rows, index, pending))
    });
    Box::pin(async move {
        let (rows, mut index, mut pending) = first?;
        while let Some((order, trigger_by, delete)) = pending.take() {
            let current_order = order.clone();
            let result = async {
                delete.await?;
                let response = {
                    let mut h = host.borrow_mut();
                    let amount = h.field(&order, "triggerQuantity")?;
                    let amount = if h.truthy(&amount) {
                        amount
                    } else {
                        h.field(&order, "quantity")?
                    };
                    let mut req = request("ORDER", vec![]);
                    req.data = vec![
                        ("symbol", Param::Value(h.field(&order, "symbol")?)),
                        ("side", Param::Value(h.field(&order, "side")?)),
                        ("orderType", Param::Value(h.field(&order, "orderType")?)),
                        (
                            "triggerPrice",
                            Param::Value(h.field(&order, "triggerPrice")?),
                        ),
                        ("triggerQuantity", Param::Value(amount)),
                        ("triggerBy", Param::Text(trigger_by.clone())),
                        ("reduceOnly", Param::Bool(true)),
                    ];
                    let t = h.field(&order, "orderType")?;
                    if h.is_text(&t, "Limit") {
                        let p = h.field(&order, "price")?;
                        if h.truthy(&p) {
                            req.data
                                .push(("price", Param::Value(h.field(&order, "price")?)));
                        }
                    }
                    let target = h.transport_target("post")?;
                    h.post(target, req)?
                };
                let new_order = response.await?;
                let mut h = host.borrow_mut();
                let target = h.log_target()?;
                let id = field_text(&mut *h, &order, "id")?;
                let new_id = field_text(&mut *h, &new_order, "id")?;
                h.log(
                    target,
                    cat(&[
                        &u("Fixed order "),
                        &id,
                        &u(" -> new order "),
                        &new_id,
                        &u(" with triggerBy="),
                        &trigger_by,
                    ]),
                    Severity::Info,
                )?;
                let len = h.array_length(&orders)?;
                for j in 0..len {
                    let v = h
                        .array_element(&orders, j)?
                        .unwrap_or_else(|| h.undefined());
                    let id = h.field(&v, "id")?;
                    let oid = h.field(&order, "id")?;
                    if h.strict_equal(&id, &oid) {
                        h.array_set(&orders, j, new_order)?;
                        break;
                    }
                }
                Ok::<_, H::Error>(())
            }
            .await;
            if let Err(e) = result {
                warn_repair(&mut *host.borrow_mut(), &current_order, &e)?;
            }
            index += 1;
            pending = advance_repair(&mut *host.borrow_mut(), &rows, &mut index)?;
        }
        Ok(())
    })
}
type Repair<H> = Result<
    (
        <H as Host>::Value,
        JsString,
        Reply<<H as Host>::Value, <H as Host>::Error>,
    ),
    <H as Host>::Error,
>;
fn warn_repair<H: Host>(h: &mut H, order: &H::Value, error: &H::Error) -> Result<(), H::Error> {
    let target = h.log_target()?;
    let id = field_text(h, order, "id")?;
    let text = h.error_text(error)?;
    h.log(
        target,
        cat(&[&u("Error fixing trigger order "), &id, &u(": "), &text]),
        Severity::Warning,
    )
}
fn advance_repair<H: Host>(
    h: &mut H,
    rows: &[H::Value],
    index: &mut usize,
) -> Result<Option<(H::Value, JsString, Reply<H::Value, H::Error>)>, H::Error> {
    while let Some(order) = rows.get(*index) {
        match begin_repair(h, order.clone()) {
            Ok(v) => return Ok(Some(v)),
            Err(e) => {
                warn_repair(h, order, &e)?;
                *index += 1;
            }
        }
    }
    Ok(None)
}
fn begin_repair<H: Host>(h: &mut H, order: H::Value) -> Repair<H> {
    let side = h.field(&order, "side")?;
    let side = if h.is_text(&side, "Bid") {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let price = field_num(h, &order, "triggerPrice")?;
    let target = h.trigger_target()?;
    let symbol = h.field(&order, "symbol")?;
    let kind = h.trigger_type(target, symbol, side, price)?;
    let by = h.trigger_by(kind == OrderType::TakeProfit)?;
    let target = h.log_target()?;
    let id = field_text(h, &order, "id")?;
    let symbol = field_text(h, &order, "symbol")?;
    let trigger = field_text(h, &order, "triggerPrice")?;
    let side = if side == OrderSide::Buy {
        "buy"
    } else {
        "sell"
    };
    h.log(
        target,
        cat(&[
            &u("Fixing order "),
            &id,
            &u(": "),
            &symbol,
            &u(" "),
            &u(side),
            &u(" @ "),
            &trigger,
            &u(" ("),
            &u(if kind == OrderType::TakeProfit {
                "TP"
            } else {
                "SL"
            }),
            &u(") -> triggerBy="),
            &by,
        ]),
        Severity::Info,
    )?;
    let target = h.transport_target("delete")?;
    let mut req = request("ORDER", vec![]);
    req.data = vec![
        ("symbol", Param::Value(h.field(&order, "symbol")?)),
        ("orderId", Param::Value(h.field(&order, "id")?)),
    ];
    let response = h.delete(target, req)?;
    Ok((order, by, response))
}
// ts: BackpackExchange.fetchPositions
pub fn fetch_positions<H: Host>(host: Rc<RefCell<H>>) -> Task<Vec<Position>, H::Error> {
    let prepared = (|| {
        let response = get(&mut *host.borrow_mut(), request("POSITIONS", vec![]));
        match response {
            Ok(f) => Ok((Some(f), None)),
            Err(e) => {
                let mut h = host.borrow_mut();
                if h.error_message(&e)?
                    .is_some_and(|s| s.windows(9).any(|w| w == u("not found")))
                {
                    log(
                        &mut *h,
                        u("Positions endpoint not available, using fake positions only"),
                        Severity::Info,
                    )?;
                    Ok((None, Some(h.position_mode()?)))
                } else {
                    Err(e)
                }
            }
        }
    })();
    let (response, mode) = match prepared {
        Ok(v) => v,
        Err(e) => {
            return Box::pin(std::future::ready(caught(
                &mut *host.borrow_mut(),
                e,
                "Error fetching positions: ",
            )));
        }
    };
    Box::pin(async move {
        let result = async {
            let data = if let Some(response) = response {
                match response.await {
                    Ok(v) => Some(v),
                    Err(e) => {
                        let mut h = host.borrow_mut();
                        if h.error_message(&e)?
                            .is_some_and(|s| s.windows(9).any(|w| w == u("not found")))
                        {
                            log(
                                &mut *h,
                                u("Positions endpoint not available, using fake positions only"),
                                Severity::Info,
                            )?;
                            None
                        } else {
                            return Err(e);
                        }
                    }
                }
            } else {
                None
            };
            let mode = match mode {
                Some(mode) => mode,
                None => host.borrow_mut().position_mode()?,
            };
            let mode = mode.await?;
            let mut h = host.borrow_mut();
            let leverage = h.leverage()?;
            let mut actual = vec![];
            if let Some(data) = data {
                let a = h.array(&data)?;
                let len = h.array_length(&a)?;
                let mut rows = vec![];
                for i in 0..len {
                    let Some(p) = h.array_element(&a, i)? else {
                        continue;
                    };
                    if field_num(&mut *h, &p, "netQuantity")? != 0.0 {
                        rows.push(p);
                    }
                }
                for p in rows {
                    let net = field_num(&mut *h, &p, "netQuantity")?;
                    let entry = field_num(&mut *h, &p, "entryPrice")?;
                    let side = if net > 0.0 {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    };
                    let symbol = field_text(&mut *h, &p, "symbol")?;
                    let id = h.position_id(&symbol, side, mode)?;
                    let symbol = field_text(&mut *h, &p, "symbol")?;
                    let notional = field_num(&mut *h, &p, "netExposureNotional")?;
                    let notional = if truthy_number(notional) {
                        notional
                    } else {
                        (net * entry).abs()
                    };
                    let unrealized_pnl = field_num(&mut *h, &p, "pnlUnrealized")?;
                    let liquidation = field_num(&mut *h, &p, "estLiquidationPrice")?;
                    let liquidation_price = if truthy_number(liquidation) {
                        liquidation
                    } else {
                        0.0
                    };
                    let last_update = h.clock()?;
                    actual.push(Position {
                        id,
                        symbol,
                        side,
                        entry_price: entry,
                        notional,
                        leverage,
                        unrealized_pnl,
                        contracts: net.abs(),
                        liquidation_price,
                        last_update,
                        position_mode: mode,
                        margin_type: MarginType::Cross,
                    });
                }
            }
            let existing: Vec<_> = actual.iter().map(|p| p.symbol.clone()).collect();
            let markets = h.markets()?;
            let mut i = 0;
            loop {
                if i >= h.array_length(&markets)? {
                    break;
                }
                let m = h
                    .array_element(&markets, i)?
                    .unwrap_or_else(|| h.undefined());
                i += 1;
                let active = h.field(&m, "active")?;
                if !h.truthy(&active) {
                    continue;
                }
                let spot = h.field(&m, "isSpot")?;
                if h.truthy(&spot) {
                    continue;
                }
                let symbol = field_text(&mut *h, &m, "symbol")?;
                if existing.contains(&symbol) {
                    continue;
                }
                for side in [PositionSide::Long, PositionSide::Short] {
                    let symbol = field_text(&mut *h, &m, "symbol")?;
                    let id = h.position_id(&symbol, side, mode)?;
                    let symbol = field_text(&mut *h, &m, "symbol")?;
                    let last_update = h.clock()?;
                    actual.push(Position {
                        id,
                        symbol,
                        side,
                        entry_price: 0.0,
                        notional: 0.0,
                        leverage,
                        unrealized_pnl: 0.0,
                        contracts: 0.0,
                        liquidation_price: 0.0,
                        last_update,
                        position_mode: mode,
                        margin_type: MarginType::Cross,
                    });
                }
            }
            let mut out: Vec<Position> = vec![];
            for p in actual {
                if !out.iter().any(|v| v.id == p.id) {
                    out.push(p);
                }
            }
            Ok(out)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, "Error fetching positions: "),
        }
    })
}
#[derive(Debug)]
pub struct Position {
    pub id: JsString,
    pub symbol: JsString,
    pub side: PositionSide,
    pub entry_price: f64,
    pub notional: f64,
    pub leverage: f64,
    pub unrealized_pnl: f64,
    pub contracts: f64,
    pub liquidation_price: f64,
    pub last_update: f64,
    pub position_mode: PositionMode,
    pub margin_type: MarginType,
}
