//! Market catalog and historical candle field bodies over retained plain reply
//! items. HTTP, dayjs and cache operations are explicit imported capabilities.
use crate::{
    market_position_reads::{ReadItem, ReadItems, ReadReply},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    public_data::PublicData,
    starbase_auth::{
        CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner, queue_job,
    },
    utils::{normalize_cfm_contract_size, normalize_coinbase_usd_equivalent_product_id},
};
use safe_cex_num::{js_number, js_truthy, parse_float};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    Candle, Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub struct CatalogMarket {
    pub fields: Market,
    pub info: ReadItem,
}
pub type CatalogMarkets = Rc<RefCell<Vec<Rc<RefCell<CatalogMarket>>>>>;
pub type CatalogSupportedTypes = Rc<RefCell<Vec<String>>>;
pub type PublicReadAsync<T> = Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>;
#[derive(Debug, Clone)]
pub struct PublicOHLCVOptions {
    pub symbol: String,
    pub interval: String,
    pub limit: Option<Value>,
    pub from: Option<Value>,
    pub to: Option<Value>,
}
pub type PublicOHLCVOptionsRoot = Rc<RefCell<PublicOHLCVOptions>>;
pub type PublicCandles = Rc<RefCell<Vec<Rc<RefCell<Candle>>>>>;
#[derive(Debug, Clone)]
pub struct PublicCandleQuery {
    pub product_id: String,
    pub granularity: &'static str,
    pub start: f64,
    pub end: f64,
}
#[derive(Debug, Clone)]
pub enum PublicMarketQuery {
    Catalog(Value),
    Candles(PublicCandleQuery),
}
pub trait PublicMarketReadHost {
    fn public_only(&self) -> bool;
    fn get_supported_types(&self) -> PublicReadAsync<CatalogSupportedTypes>;
    fn get(
        &self,
        endpoint: &'static str,
        params: Option<PublicMarketQuery>,
    ) -> PublicReadAsync<ReadReply>;
    fn error_response_items(&self, error: &PortfolioFailure) -> Option<ReadItems>;
    fn cached_markets(&self) -> Result<CatalogMarkets, PortfolioFailure>;
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure>;
    /// Imported dayjs(value) / dayjs(); None means omitted argument.
    fn dayjs_value_of(&self, value: Option<Value>) -> Result<f64, PortfolioFailure>;
    fn dayjs_subtract(&self, end_ms: f64, amount: f64, unit: &str)
    -> Result<f64, PortfolioFailure>;
    /// Ordinary-object lookup, including inherited names. An inherited function
    /// or Object.prototype can be represented by a truthy empty Generated record.
    fn daily_candle(&self, symbol: &str) -> Option<PublicData>;
    fn set_daily_candle(&self, symbol: String, candle: PublicData) -> Result<(), PortfolioFailure>;
}
pub struct PublicMarketReadReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
const PRODUCTS: &str = "/api/v3/brokerage/products";
const INSTRUMENTS: &str = "https://api.international.coinbase.com/api/v1/instruments/";
const CANDLES: &str = "/api/v3/brokerage/products/{product_id}/candles";
fn error(name: &str, message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":name,"message":message}),
    )))
}
fn float(v: Option<&Input>) -> Result<f64, PortfolioFailure> {
    safe_cex_num::js_retained_parse_float(v).map_err(|e| error(&e.name, &e.message))
}
fn text(v: Option<&Input>) -> Result<String, PortfolioFailure> {
    safe_cex_num::js_retained_string(v).map_err(|e| error(&e.name, &e.message))
}
fn js_round(v: f64) -> f64 {
    if !v.is_finite() || v == 0.0 {
        return v;
    }
    if (-0.5..0.0).contains(&v) {
        return -0.0;
    }
    let floor = v.floor();
    if v - floor < 0.5 { floor } else { floor + 1.0 }
}
fn detail(e: &PortfolioFailure) -> Option<Value> {
    let message = e
        .value
        .as_ref()
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("message"));
    if js_truthy(message) {
        message.cloned()
    } else {
        e.value.as_ref().and_then(|v| v.get("message")).cloned()
    }
}
fn all(
    replies: Vec<Promise<Result<ReadReply, PortfolioFailure>>>,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> Promise<Result<Vec<ReadReply>, PortfolioFailure>> {
    let (p, resolve) = Promise::pending();
    if replies.is_empty() {
        resolve.resolve(Ok(vec![]));
        return p;
    }
    let remaining = Rc::new(std::cell::Cell::new(replies.len()));
    let values = Rc::new(RefCell::new(vec![None; replies.len()]));
    let failed = Rc::new(std::cell::Cell::new(false));
    for (i, reply) in replies.into_iter().enumerate() {
        let (values, remaining, failed, resolve) = (
            values.clone(),
            remaining.clone(),
            failed.clone(),
            resolve.clone(),
        );
        reply.observe(spawn, move |reply| match reply {
            Err(e) => {
                if !failed.replace(true) {
                    resolve.resolve(Err(e));
                }
            }
            Ok(value) => {
                values.borrow_mut()[i] = Some(value);
                remaining.set(remaining.get() - 1);
                if remaining.get() == 0 && !failed.get() {
                    resolve.resolve(Ok(values
                        .borrow()
                        .iter()
                        .map(|v| v.clone().expect("settled reply"))
                        .collect()));
                }
            }
        });
    }
    p
}
fn base_market(
    id: String,
    base: String,
    quote: String,
    active: bool,
    amount: f64,
    price: f64,
    min: f64,
    max: f64,
    leverage: f64,
    contract: f64,
    spot: Option<bool>,
    expiring: Option<bool>,
) -> Market {
    Market {
        runtime: Default::default(),
        symbol: id.clone(),
        id,
        base,
        quote,
        active,
        pretty_symbol: None,
        precision: MarketPrecision {
            base: None,
            quote: None,
            amount,
            price,
        },
        limits: MarketLimits {
            cost: None,
            amount: MarketLimitsAmount { min, max },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: leverage,
            },
            price: None,
        },
        contract_size: Some(contract),
        is_spot: spot,
        is_expiring: expiring,
        is_inverse: None,
        is_perpetual: None,
        is_read_only: None,
        info: None,
        taker_fee: None,
        maker_fee: None,
        option: None,
    }
}
pub fn fetch_markets<H: PublicMarketReadHost + 'static>(
    r: Rc<PublicMarketReadReceiver<H>>,
) -> Promise<Result<CatalogMarkets, PortfolioFailure>> {
    if r.host.public_only() {
        // Source return helper(), not return await helper(): adoption is deferred,
        // and a rejected helper Promise escapes the outer try/catch.
        let child = fetch_markets_from_intx_instruments(r.clone());
        let (p, resolve) = Promise::pending();
        let jobs = r.spawn.clone();
        queue_job(&r.spawn, move || {
            child.observe(&jobs, move |value| resolve.resolve(value))
        });
        return p;
    }
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let result = async {
                let types = r.host.get_supported_types()?.await?;
                let len = types.borrow().len();
                let mut requests = Vec::with_capacity(len);
                let mut holes = Vec::with_capacity(len);
                for index in 0..len {
                    let kind = types.borrow().get(index).cloned();
                    holes.push(kind.is_none());
                    let Some(kind) = kind else {
                        requests.push(Promise::resolved(Ok(ReadReply {
                            items: Rc::new(RefCell::new(vec![])),
                        })));
                        continue;
                    };
                    let params = if kind == "spot" {
                        json!({})
                    } else if kind == "intx" {
                        json!({"product_type":"FUTURE","contract_expiry_type":"PERPETUAL"})
                    } else {
                        json!({"product_type":"FUTURE"})
                    };
                    requests.push(
                        r.host
                            .get(PRODUCTS, Some(PublicMarketQuery::Catalog(params)))?,
                    );
                }
                let replies = all(requests, &r.spawn).await?;
                let mut items = Vec::new();
                for (i, reply) in replies.into_iter().enumerate() {
                    if holes[i] {
                        return Err(error(
                            "TypeError",
                            "Cannot read properties of undefined (reading 'data')",
                        ));
                    }
                    items.extend(reply.items.borrow().clone());
                }
                let mut markets = Vec::new();
                for item in items {
                    // No borrow survives an imported callback: fields after an invalid
                    // CFM skip are read from the next retained item at iteration time.
                    let row = item.borrow().clone();
                    let details = row.get("future_product_details");
                    if row.is_nullish() {
                        return Err(error(
                            "TypeError",
                            "Cannot read properties of null (reading 'price_increment')",
                        ));
                    }
                    let spot = details.is_none();
                    let expiring = details
                        .and_then(|v| v.get("contract_expiry_type"))
                        .and_then(Input::as_text)
                        .and_then(|v| String::from_utf16(v).ok())
                        .as_deref()
                        == Some("EXPIRING");
                    let cfm = if expiring {
                        details
                            .and_then(|v| v.get("contract_size"))
                            .and_then(normalize_cfm_contract_size)
                    } else {
                        None
                    };
                    if expiring && cfm.is_none() {
                        r.host.emit_and_log_error(Some(Value::String(format!(
                            "Coinbase CFM market skipped: invalid contract size for {}",
                            text(row.get("product_id"))?
                        ))))?;
                        continue;
                    }
                    let contract = if let Some(v) = cfm {
                        v
                    } else {
                        let size = details.and_then(|v| v.get("contract_size"));
                        let default = Input::from(json!("1"));
                        float(if size.is_some_and(Input::is_truthy) {
                            size
                        } else {
                            Some(&default)
                        })?
                    };
                    let id = text(row.get("product_id"))?;
                    let root = details.and_then(|v| v.get("contract_root_unit"));
                    let base = row.get("base_currency_id");
                    let base = if root.is_some_and(Input::is_truthy) {
                        text(root)?
                    } else if base.is_some_and(Input::is_truthy) {
                        text(base)?
                    } else {
                        let product = row.get("product_id");
                        match product {
                            Some(v @ Input::Text(_)) => {
                                text(Some(v))?.split('-').next().unwrap_or("").into()
                            }
                            None => {
                                return Err(error(
                                    "TypeError",
                                    "Cannot read properties of undefined (reading 'split')",
                                ));
                            }
                            Some(Input::Null | Input::Undefined) => {
                                return Err(error(
                                    "TypeError",
                                    "Cannot read properties of null (reading 'split')",
                                ));
                            }
                            _ => {
                                return Err(error(
                                    "TypeError",
                                    "m.product_id.split is not a function",
                                ));
                            }
                        }
                    };
                    let fields = base_market(
                        id,
                        base,
                        text(row.get("quote_currency_id"))?,
                        row.get("status")
                            .and_then(Input::as_text)
                            .and_then(|v| String::from_utf16(v).ok())
                            .as_deref()
                            == Some("TRADING"),
                        float(row.get("base_increment"))?,
                        float(row.get("price_increment"))?,
                        float(row.get("base_min_size"))?,
                        float(row.get("base_max_size"))?,
                        if spot { 1.0 } else { 10.0 },
                        contract,
                        Some(spot),
                        Some(expiring),
                    );
                    markets.push(Rc::new(RefCell::new(CatalogMarket { fields, info: item })));
                }
                Ok::<_, PortfolioFailure>(Rc::new(RefCell::new(markets)))
            }
            .await;
            match result {
                Ok(v) => Ok(v),
                Err(e) => {
                    r.host.emit_and_log_error(detail(&e))?;
                    Ok(r.host.cached_markets()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_markets_from_intx_instruments<H: PublicMarketReadHost + 'static>(
    r: Rc<PublicMarketReadReceiver<H>>,
) -> Promise<Result<CatalogMarkets, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let result = async {
                let reply = match async { r.host.get(INSTRUMENTS, None)?.await }.await {
                    Ok(reply) => reply.items,
                    Err(e) => {
                        if e.value.is_none() || e.value.as_ref().is_some_and(Value::is_null) {
                            return Err(error(
                                "TypeError",
                                &format!(
                                    "Cannot read properties of {} (reading 'response')",
                                    if e.value.is_none() {
                                        "undefined"
                                    } else {
                                        "null"
                                    }
                                ),
                            ));
                        }
                        let items = r.host.error_response_items(&e);
                        match items {
                            Some(items) => {
                                if items.borrow().is_empty() {
                                    return Err(e);
                                }
                                items
                            }
                            _ => {
                                let data = e
                                    .value
                                    .as_ref()
                                    .and_then(|v| v.get("response"))
                                    .and_then(|v| v.get("data"));
                                let length = data.and_then(|v| match v {
                                    Value::String(v) => Some(Value::from(v.encode_utf16().count())),
                                    Value::Array(_) => None,
                                    Value::Object(v) => v.get("length").cloned(),
                                    _ => None,
                                });
                                if js_truthy(length.as_ref()) {
                                    return Err(error(
                                        "TypeError",
                                        "data.filter is not a function",
                                    ));
                                }
                                return Err(e);
                            }
                        }
                    }
                };
                // filter captures initial length and retains original accepted roots.
                let len = reply.borrow().len();
                let mut selected = Vec::new();
                for i in 0..len {
                    let item = reply.borrow()[i].clone();
                    let row = item.borrow();
                    if row.is_nullish() {
                        return Err(error(
                            "TypeError",
                            "Cannot read properties of null (reading 'trading_state')",
                        ));
                    }
                    if row
                        .get("trading_state")
                        .and_then(Input::as_text)
                        .and_then(|v| String::from_utf16(v).ok())
                        .as_deref()
                        == Some("TRADING")
                        && row
                            .get("type")
                            .and_then(Input::as_text)
                            .and_then(|v| String::from_utf16(v).ok())
                            .as_deref()
                            == Some("PERP")
                    {
                        selected.push(item.clone());
                    }
                }
                let mut markets = Vec::new();
                for item in selected {
                    let row = item.borrow().clone();
                    let min = row.get("min_quantity");
                    let notional = row.get("min_notional_value");
                    let default = Input::from(json!("0.001"));
                    let min = if min.is_some_and(Input::is_truthy) {
                        min
                    } else if notional.is_some_and(Input::is_truthy) {
                        notional
                    } else {
                        Some(&default)
                    };
                    let fields = base_market(
                        format!("{}-INTX", text(row.get("symbol"))?),
                        text(row.get("base_asset_name"))?,
                        text(row.get("quote_asset_name"))?,
                        true,
                        float(row.get("base_increment"))?,
                        float(row.get("quote_increment"))?,
                        float(min)?,
                        1e9,
                        10.0,
                        1.0,
                        None,
                        None,
                    );
                    markets.push(Rc::new(RefCell::new(CatalogMarket { fields, info: item })));
                }
                Ok::<_, PortfolioFailure>(Rc::new(RefCell::new(markets)))
            }
            .await;
            match result {
                Ok(v) => Ok(v),
                Err(e) => {
                    r.host.emit_and_log_error(detail(&e))?;
                    Ok(r.host.cached_markets()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_ohlcv<H: PublicMarketReadHost + 'static>(
    r: Rc<PublicMarketReadReceiver<H>>,
    opts: PublicOHLCVOptionsRoot,
) -> Promise<Result<PublicCandles, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let interval = opts.borrow().interval.clone();
            let granularity = match interval.as_str() {
                "1m" => "ONE_MINUTE",
                "5m" => "FIVE_MINUTE",
                "15m" => "FIFTEEN_MINUTE",
                "30m" => "THIRTY_MINUTE",
                "1h" => "ONE_HOUR",
                "2h" => "TWO_HOUR",
                "6h" => "SIX_HOUR",
                "1d" => "ONE_DAY",
                _ => return Err(error("Error", &format!("Invalid interval {interval}"))),
            };
            let limit = opts.borrow().limit.clone();
            let limit = if js_truthy(limit.as_ref()) {
                js_number(limit.as_ref()).map_err(|e| error(&e.name, &e.message))?
            } else {
                300.0
            };
            let limit = if limit.is_nan() {
                f64::NAN
            } else {
                limit.min(300.0)
            };
            let first = interval
                .find(|c: char| c.is_ascii_digit())
                .expect("mapped interval digits");
            let end_digits = interval[first..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|i| first + i)
                .unwrap_or(interval.len());
            let amount = parse_float(&interval[first..end_digits]);
            let unit = &interval[end_digits..];
            let to = opts.borrow().to.clone();
            let end = r
                .host
                .dayjs_value_of(if js_truthy(to.as_ref()) { to } else { None })?;
            let from = opts.borrow().from.clone();
            let start = if (limit == 0.0 || limit.is_nan()) && js_truthy(from.as_ref()) {
                r.host.dayjs_value_of(from)?
            } else {
                r.host.dayjs_subtract(end, amount * limit, unit)?
            };
            let symbol = normalize_coinbase_usd_equivalent_product_id(&opts.borrow().symbol);
            let params = PublicMarketQuery::Candles(PublicCandleQuery {
                product_id: symbol,
                granularity,
                start: js_round(start / 1000.0),
                end: js_round(end / 1000.0),
            });
            let reply = r.host.get(CANDLES, Some(params))?.await?;
            let len = reply.items.borrow().len();
            let mut candles = Vec::with_capacity(len);
            for i in 0..len {
                let item = reply.items.borrow()[i].clone();
                let row = item.borrow().clone();
                if row.is_nullish() {
                    return Err(error(
                        "TypeError",
                        "Cannot read properties of null (reading 'start')",
                    ));
                }
                let candle = Rc::new(RefCell::new(Candle {
                    timestamp: safe_cex_num::js_retained_number(row.get("start"))
                        .map_err(|e| error(&e.name, &e.message))?,
                    open: float(row.get("open"))?,
                    high: float(row.get("high"))?,
                    low: float(row.get("low"))?,
                    close: float(row.get("close"))?,
                    volume: float(row.get("volume"))?,
                    last_side: None,
                }));
                if interval == "1d" {
                    let symbol = opts.borrow().symbol.clone();
                    let cached = r.host.daily_candle(&symbol);
                    let timestamp = cached.as_ref().and_then(|v| v.get("timestamp"));
                    let old = crate::public_data::number(timestamp)
                        .map_err(|e| error(e.name, e.message))?;
                    if cached.is_none() || old < candle.borrow().timestamp {
                        r.host
                            .set_daily_candle(symbol, PublicData::Candle(candle.clone()))?;
                    }
                }
                candles.push(candle);
            }
            candles.reverse();
            Ok(Rc::new(RefCell::new(candles)))
        }),
        &spawn,
    )
}
