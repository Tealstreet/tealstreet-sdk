//! Remaining REST readers and startup/polling bodies. Websocket behavior is supplied
//! by its native host and is not claimed as implemented by this module.
use crate::{
    order_reads::error_value,
    order_services::{ServiceError, ServiceHost, body, emit, field},
    types::{ENDPOINTS, GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC, INTERVAL},
    utility::{UtilityInput, is_blofin_linear_swap_market},
};
use safe_cex_num::{
    add, js_string, js_truthy, math_min, math_round, parse_float, parse_int, round_usd,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{RestAdapter, RestRequest};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    AnyTimeframe, Balance, Candle, ExchangeName, ExchangeOptions, ExtendedTimeframe, Market,
    MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision, OHLCVOptions, Order,
    Position, SecondsTimeframe, Ticker, Timeframe,
};
use serde_json::{Value, json};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
};

async fn later() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}
async fn get<E>(
    xhr: &dyn RestAdapter,
    url: String,
    params: Option<Value>,
) -> Result<UtilityInput, ServiceError<E>> {
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url,
            params,
            ..Default::default()
        })
        .await;
    later().await;
    body(&response.map_err(|e| ServiceError::Rest(Box::new(e)))?.body)
}
fn endpoint(name: &str) -> &str {
    ENDPOINTS[name].as_str().expect("static endpoint")
}
fn input<E>(message: impl Into<String>) -> ServiceError<E> {
    ServiceError::Input(message.into())
}
fn parse<E>(v: Option<&Value>) -> Result<f64, ServiceError<E>> {
    Ok(parse_float(&js_string(v).map_err(ServiceError::Coercion)?))
}
pub struct AccountValidation {
    pub error: Value,
    pub balance: Option<Balance>,
}
pub async fn validate_account<E: std::fmt::Display>(
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
) -> Result<AccountValidation, ServiceError<E>> {
    let result = async {
        let response = get::<E>(
            xhr,
            endpoint("BALANCE").into(),
            Some(json!({"accountType":"futures"})),
        )
        .await?;
        let data = response.json();
        if field(data, "code")?.and_then(Value::as_str) != Some("0") {
            return Err(input(match field(data, "msg")? {
                None => String::new(),
                Some(v) => js_string(Some(v)).map_err(ServiceError::Coercion)?,
            }));
        }
        let mut balance = None;
        if let Some(rows) = data.get("data").and_then(Value::as_array) {
            let mut available = None;
            let mut frozen = None;
            for row in rows {
                let v = parse(field(row, "available")?)?;
                available = Some(available.map_or(v, |sum| sum + v));
                let v = parse(field(row, "frozen")?)?;
                frozen = Some(frozen.map_or(v, |sum| sum + v));
            }
            let total = add(
                round_usd(available.unwrap_or(0.0)),
                round_usd(frozen.unwrap_or(0.0)),
            );
            if total.is_finite() {
                balance = Some(Balance {
                    total,
                    free: total,
                    used: 0.0,
                    upnl: 0.0,
                    coins: None,
                    currencies: None,
                    info: None,
                });
            }
        }
        Ok(AccountValidation {
            error: Value::String(String::new()),
            balance,
        })
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let hint = error_value(&e, false, None);
            emit(host, Some(&hint))?;
            let error = if js_truthy(Some(&hint)) {
                hint
            } else {
                Value::String("Invalid API key or secret".into())
            };
            Ok(AccountValidation {
                error,
                balance: None,
            })
        }
    }
}

pub async fn fetch_markets<E: std::fmt::Display>(
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
    nanoid: impl FnOnce() -> String,
) -> Result<Vec<Market>, ServiceError<E>> {
    let result = async {
        let response = get::<E>(xhr, format!("{}?{}", endpoint("MARKETS"), nanoid()), None).await?;
        let data = field(response.json(), "data")?
            .ok_or_else(|| input("Cannot read properties of undefined (reading 'filter')"))?;
        let rows = data.as_array().ok_or_else(|| {
            input(if data.is_null() {
                "Cannot read properties of null (reading 'filter')"
            } else {
                "data.filter is not a function"
            })
        })?;
        // Source completes filter() before invoking any mapper.
        let mut filtered = Vec::new();
        for row in rows {
            if is_blofin_linear_swap_market(&UtilityInput::from_retained(row.clone()))
                .map_err(ServiceError::Coercion)?
            {
                filtered.push(row);
            }
        }
        let mut markets = Vec::new();
        for row in filtered {
            let max_amount = math_min(
                parse(row.get("maxLimitSize"))?,
                parse(row.get("maxMarketSize"))?,
            );
            let contract = parse(row.get("contractValue"))?;
            let lot = parse(
                row.get("lotSize")
                    .filter(|v| js_truthy(Some(v)))
                    .or_else(|| row.get("minSize")),
            )?;
            let id = row.get("instId").and_then(Value::as_str).ok_or_else(|| {
                input(match row.get("instId") {
                    None => "Cannot read properties of undefined (reading 'replace')",
                    Some(Value::Null) => "Cannot read properties of null (reading 'replace')",
                    _ => "m.instId.replace is not a function",
                })
            })?;
            let base = row
                .get("baseCurrency")
                .and_then(Value::as_str)
                .ok_or_else(|| input("Market baseCurrency requires a native String"))?;
            let quote = row
                .get("quoteCurrency")
                .and_then(Value::as_str)
                .ok_or_else(|| input("Market quoteCurrency requires a native String"))?;
            let mut info: Vec<(Vec<u16>, Input)> = row
                .as_object()
                .expect("filtered object")
                .iter()
                .map(|(key, value)| (key.encode_utf16().collect(), value.clone().into()))
                .collect();
            for (key, value) in [("contractValue", contract), ("lotSize", lot)] {
                if let Some((_, slot)) = info
                    .iter_mut()
                    .find(|(name, _)| name.iter().copied().eq(key.encode_utf16()))
                {
                    *slot = Input::Float(value);
                } else {
                    info.push((key.encode_utf16().collect(), Input::Float(value)));
                }
            }
            markets.push(Market {
                id: id.into(),
                symbol: id.strip_suffix("-SWAP").unwrap_or(id).replace('-', ""),
                pretty_symbol: None,
                base: base.into(),
                quote: quote.into(),
                active: row.get("state").and_then(Value::as_str) == Some("live"),
                precision: MarketPrecision {
                    amount: lot,
                    price: parse(row.get("tickSize"))?,
                },
                limits: MarketLimits {
                    amount: MarketLimitsAmount {
                        min: parse(row.get("minSize"))? * contract,
                        max: max_amount,
                    },
                    leverage: MarketLimitsLeverage {
                        min: 1.0,
                        max: parse(row.get("maxLeverage"))?,
                    },
                    price: None,
                },
                contract_size: Some(contract),
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                info: Some(Input::Object(info)),
                taker_fee: None,
                maker_fee: None,
                option: None,
            });
        }
        Ok(markets)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host, Some(&error_value(&e, false, None)))?;
            Ok(store.markets().to_vec())
        }
    }
}
fn timeframe(v: &AnyTimeframe) -> (&'static str, f64, char) {
    match v {
        AnyTimeframe::Variant1(Timeframe::M1) => ("1m", 1.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M3) => ("3m", 3.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M5) => ("5m", 5.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M15) => ("15m", 15.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::M30) => ("30m", 30.0, 'm'),
        AnyTimeframe::Variant1(Timeframe::H1) => ("1h", 1.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H2) => ("2h", 2.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H4) => ("4h", 4.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H6) => ("6h", 6.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::H12) => ("12h", 12.0, 'h'),
        AnyTimeframe::Variant1(Timeframe::D1) => ("1d", 1.0, 'd'),
        AnyTimeframe::Variant1(Timeframe::W1) => ("1w", 1.0, 'w'),
        AnyTimeframe::Variant0(SecondsTimeframe::S1) => ("1s", 1.0, 's'),
        AnyTimeframe::Variant0(SecondsTimeframe::S5) => ("5s", 5.0, 's'),
        AnyTimeframe::Variant0(SecondsTimeframe::S15) => ("15s", 15.0, 's'),
        AnyTimeframe::Variant0(SecondsTimeframe::S30) => ("30s", 30.0, 's'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H3) => ("3h", 3.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H5) => ("5h", 5.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H7) => ("7h", 7.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H8) => ("8h", 8.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H9) => ("9h", 9.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H10) => ("10h", 10.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H11) => ("11h", 11.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::H24) => ("24h", 24.0, 'h'),
        AnyTimeframe::Variant2(ExtendedTimeframe::D2) => ("2d", 2.0, 'd'),
        AnyTimeframe::Variant2(ExtendedTimeframe::D3) => ("3d", 3.0, 'd'),
        AnyTimeframe::Variant2(ExtendedTimeframe::MN1) => ("1M", 1.0, 'M'),
    }
}
fn truth_number(v: Option<f64>) -> Option<f64> {
    v.filter(|v| *v != 0.0 && !v.is_nan())
}
fn date_clip(v: f64) -> f64 {
    if !v.is_finite() || v.abs() > 8.64e15 {
        f64::NAN
    } else {
        let v = v.trunc();
        if v == 0.0 { 0.0 } else { v }
    }
}
pub async fn fetch_ohlcv<E: std::fmt::Display>(
    opts: &OHLCVOptions,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &ServiceHost<'_, E>,
    now: impl FnOnce() -> f64,
) -> Result<Vec<Candle>, ServiceError<E>> {
    let limit = math_min(truth_number(opts.limit).unwrap_or(1440.0), 1440.0);
    let (interval, amount, unit) = timeframe(&opts.interval);
    let end = date_clip(truth_number(opts.to).unwrap_or_else(now));
    // UTC/fixed-offset calendar profile; local-zone DST is deliberately excluded.
    let start = if truth_number(opts.limit).is_none() && truth_number(opts.from).is_some() {
        date_clip(opts.from.expect("truthy from"))
    } else {
        let delta = match unit {
            'd' => math_round(-amount * limit) * 86_400_000.0,
            'w' => math_round(-amount * limit * 7.0) * 86_400_000.0,
            'h' => -amount * limit * 3_600_000.0,
            _ => -amount * limit * 60_000.0,
        };
        date_clip(end + delta)
    };
    let market = store.markets_by_symbol().get(&opts.symbol).cloned();
    let Some(market) = market else {
        emit(
            host,
            Some(&Value::String(format!(
                "Market {} not found on Blofin",
                opts.symbol
            ))),
        )?;
        return Ok(Vec::new());
    };
    let result = async {
        if !start.is_finite() || !end.is_finite() {
            return Err(input(
                "Nonfinite candle query times are outside native JSON Numbers",
            ));
        }
        let response = get::<E>(
            xhr,
            endpoint("KLINE").into(),
            Some(json!({"instId":market.id,"bar":INTERVAL[interval],"before":start,"after":end})),
        )
        .await?;
        let data = field(response.json(), "data")?
            .ok_or_else(|| input("Cannot read properties of undefined (reading 'map')"))?;
        let rows = data.as_array().ok_or_else(|| {
            input(if data.is_null() {
                "Cannot read properties of null (reading 'map')"
            } else {
                "data.map is not a function"
            })
        })?;
        let mut candles = Vec::new();
        for c in rows {
            if c.is_null() {
                return Err(input("Cannot read properties of null (reading '0')"));
            }
            // JS c[n] also accepts own numeric properties on non-array objects.
            let item = |i: usize| match c {
                Value::Array(a) => a.get(i).cloned(),
                Value::Object(m) => m.get(&i.to_string()).cloned(),
                Value::String(s) => s
                    .encode_utf16()
                    .nth(i)
                    .and_then(|u| char::from_u32(u.into()))
                    .map(|v| Value::String(v.to_string())),
                _ => None,
            };
            candles.push(Candle {
                timestamp: parse_int(
                    &js_string(item(0).as_ref()).map_err(ServiceError::Coercion)?,
                    Some(10),
                ) / 1000.0,
                open: parse(item(1).as_ref())?,
                high: parse(item(2).as_ref())?,
                low: parse(item(3).as_ref())?,
                close: parse(item(4).as_ref())?,
                volume: parse(item(7).as_ref())?,
                last_side: None,
            });
        }
        // Stable insertion sort reproduces a JS subtraction comparator where NaN equals.
        for i in 1..candles.len() {
            let mut j = i;
            while j > 0 && candles[j].timestamp - candles[j - 1].timestamp < 0.0 {
                candles.swap(j, j - 1);
                j -= 1;
            }
        }
        Ok(candles)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host, Some(&error_value(&e, false, None)))?;
            Ok(Vec::new())
        }
    }
}

/// Constructor host owns base/secondary-key/common-HTTP and the as-yet-unported
/// socket implementations. The sequence and assignments below are the venue body.
pub struct ConstructorFields {
    pub account_modes: crate::modes::AccountModes,
    pub client_order_id_rule: safe_cex_exchange::client_order_id::ClientOrderIdRule,
    pub every: &'static Value,
    pub periodic: &'static Value,
}
pub trait ConstructorHost {
    type Error;
    type Base;
    type Xhr;
    type Socket;
    fn super_constructor(
        &mut self,
        name: ExchangeName,
        opts: &ExchangeOptions,
        store: &dyn Store,
    ) -> Result<Self::Base, Self::Error>;
    fn uuid(&self) -> std::rc::Rc<dyn Fn() -> String>;
    fn initialize_fields(
        &mut self,
        base: &Self::Base,
        fields: ConstructorFields,
    ) -> Result<(), Self::Error>;
    fn create_xhr_with_secondary_key(
        &mut self,
        base: &Self::Base,
        opts: &ExchangeOptions,
    ) -> Result<Self::Xhr, Self::Error>;
    fn assign_xhr(&mut self, base: &Self::Base, xhr: Self::Xhr) -> Result<(), Self::Error>;
    fn create_public_socket(&mut self, base: &Self::Base) -> Result<Self::Socket, Self::Error>;
    fn assign_public_socket(
        &mut self,
        base: &Self::Base,
        socket: Self::Socket,
    ) -> Result<(), Self::Error>;
    fn create_private_socket(&mut self, base: &Self::Base) -> Result<Self::Socket, Self::Error>;
    fn assign_private_socket(
        &mut self,
        base: &Self::Base,
        socket: Self::Socket,
    ) -> Result<(), Self::Error>;
}
pub fn construct_exchange<H: ConstructorHost>(
    host: &mut H,
    name: ExchangeName,
    opts: &ExchangeOptions,
    store: &dyn Store,
) -> Result<H::Base, H::Error> {
    let base = host.super_constructor(name, opts, store)?;
    let uuid = host.uuid();
    let fields = ConstructorFields {
        account_modes: Default::default(),
        every: &GRAFANA_ENDPOINTS_EVERY,
        periodic: &GRAFANA_ENDPOINTS_PERIODIC,
        client_order_id_rule: safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
            "BloFin".into(),
            "/^[A-Za-z0-9]{1,32}$/".into(),
            std::rc::Rc::new(|id| {
                !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric())
            }),
            safe_cex_exchange::client_order_id::ClientOrderIdRuleOverrides {
                mint: Some(std::rc::Rc::new(move || {
                    format!("{}{}", crate::types::BROKER_ID, uuid())
                        .chars()
                        .take(32)
                        .collect()
                })),
                ..Default::default()
            },
        ),
    };
    host.initialize_fields(&base, fields)?;
    let xhr = host.create_xhr_with_secondary_key(&base, opts)?;
    host.assign_xhr(&base, xhr)?;
    let public = host.create_public_socket(&base)?;
    host.assign_public_socket(&base, public)?;
    let private = host.create_private_socket(&base)?;
    host.assign_private_socket(&base, private)?;
    Ok(base)
}
pub trait DisposeHost {
    type Error;
    fn super_dispose(&self) -> Result<(), Self::Error>;
    fn public_dispose(&self) -> Result<(), Self::Error>;
    fn private_dispose(&self) -> Result<(), Self::Error>;
}
pub fn dispose<H: DisposeHost>(host: &H) -> Result<(), H::Error> {
    host.super_dispose()?;
    host.public_dispose()?;
    host.private_dispose()
}
pub type LifecycleFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, ServiceError<E>>> + 'a>>;
pub trait LifecycleHost {
    type Error: std::fmt::Display;
    fn store(&self) -> &dyn Store;
    fn service(&self) -> &ServiceHost<'_, Self::Error>;
    fn init_external_public_data(&self) -> LifecycleFuture<'_, bool, Self::Error>;
    fn public_data_only(&self) -> bool;
    fn disposed(&self) -> bool;
    fn auth_failure_streak(&self) -> f64;
    fn tick_interval(&self) -> Option<f64>;
    fn connect_public(&self) -> Result<(), ServiceError<Self::Error>>;
    fn connect_private(&self) -> Result<(), ServiceError<Self::Error>>;
    fn fetch_markets(&self) -> LifecycleFuture<'_, Vec<Market>, Self::Error>;
    fn fetch_tickers_raw(
        &self,
    ) -> LifecycleFuture<'_, Option<crate::tickers::TickerInput>, Self::Error>;
    fn format_tickers(
        &self,
        raw: Option<crate::tickers::TickerInput>,
    ) -> Result<Vec<Ticker>, ServiceError<Self::Error>>;
    fn refetch_margin_type(&self) -> LifecycleFuture<'_, (), Self::Error>;
    fn refetch_position_mode(&self) -> LifecycleFuture<'_, (), Self::Error>;
    fn fetch_balance_positions(&self)
    -> LifecycleFuture<'_, (Balance, Vec<Position>), Self::Error>;
    fn fetch_orders(&self) -> LifecycleFuture<'_, Vec<Order>, Self::Error>;
    /// Host installs a callback invoking tick(next_count) after the specified delay.
    /// It must capture this same host and must not eagerly execute the callback.
    fn schedule_tick(
        &self,
        next_count: Option<f64>,
        delay: f64,
    ) -> Result<(), ServiceError<Self::Error>>;
}
async fn pair<A, B, E>(
    a: impl Future<Output = Result<A, E>>,
    b: impl Future<Output = Result<B, E>>,
) -> Result<(A, B), E> {
    let a = async {
        let result = a.await;
        if result.is_err() {
            later().await;
        }
        result
    };
    let b = async {
        let result = b.await;
        if result.is_err() {
            later().await;
        }
        result
    };
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let (mut av, mut bv) = (None, None);
    poll_fn(|cx| {
        if av.is_none()
            && let Poll::Ready(v) = a.as_mut().poll(cx)
        {
            av = Some(v);
        }
        if bv.is_none()
            && let Poll::Ready(v) = b.as_mut().poll(cx)
        {
            bv = Some(v);
        }
        if av.as_ref().is_some_and(Result::is_err) {
            return Poll::Ready(Err(av.take().unwrap().err().unwrap()));
        }
        if bv.as_ref().is_some_and(Result::is_err) {
            return Poll::Ready(Err(bv.take().unwrap().err().unwrap()));
        }
        if av.is_some() && bv.is_some() {
            Poll::Ready(Ok((
                av.take().unwrap().ok().unwrap(),
                bv.take().unwrap().ok().unwrap(),
            )))
        } else {
            Poll::Pending
        }
    })
    .await
}
pub async fn start<H: LifecycleHost>(host: &H) -> Result<(), ServiceError<H::Error>> {
    let external = host.init_external_public_data().await?;
    later().await;
    let public_only = host.public_data_only();
    if !external {
        let markets = host.fetch_markets();
        let raw = host.fetch_tickers_raw();
        let market_task = async {
            let markets = markets.await?;
            later().await;
            let mut loaded = host.store().loaded().clone();
            loaded.markets = true;
            host.store()
                .update(
                    StoreUpdate {
                        markets: Some(markets),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ServiceError::Store)?;
            host.connect_public()?;
            Ok(())
        };
        let (_, raw) = pair(market_task, raw).await?;
        later().await;
        if host.disposed() {
            return Ok(());
        }
        let tickers = host.format_tickers(raw)?;
        let mut loaded = host.store().loaded().clone();
        loaded.tickers = true;
        host.store()
            .update(
                StoreUpdate {
                    tickers: Some(tickers),
                    loaded: Some(loaded),
                    ..Default::default()
                },
                false,
            )
            .map_err(ServiceError::Store)?;
    }
    if public_only {
        return Ok(());
    }
    host.connect_private()?;
    pair(host.refetch_margin_type(), host.refetch_position_mode()).await?;
    later().await;
    pair(
        async {
            let (balance, positions) = host.fetch_balance_positions().await?;
            later().await;
            let mut loaded = host.store().loaded().clone();
            loaded.balance = true;
            loaded.positions = true;
            host.store()
                .update(
                    StoreUpdate {
                        balance: Some(balance),
                        positions: Some(positions),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ServiceError::Store)
        },
        async {
            let orders = host.fetch_orders().await?;
            later().await;
            let mut loaded = host.store().loaded().clone();
            loaded.orders = true;
            host.store()
                .update(
                    StoreUpdate {
                        orders: Some(orders),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ServiceError::Store)
        },
    )
    .await?;
    later().await;
    if host.auth_failure_streak() > 0.0 {
        host.schedule_tick(None, host.tick_interval().unwrap_or(10_000.0))?;
        return Ok(());
    }
    tick(host, 0.0).await
}
pub async fn tick<H: LifecycleHost>(host: &H, count: f64) -> Result<(), ServiceError<H::Error>> {
    if !host.disposed() {
        let result = async {
            let (balance, positions) = host.fetch_balance_positions().await?;
            later().await;
            if host.disposed() {
                return Ok(true);
            }
            host.store()
                .add_or_update_positions_full(positions)
                .map_err(ServiceError::Store)?;
            let mut loaded = host.store().loaded().clone();
            loaded.balance = true;
            loaded.positions = true;
            host.store()
                .update(
                    StoreUpdate {
                        balance: Some(balance),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ServiceError::Store)?;
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => (),
            Err(e) => emit(host.service(), Some(&error_value(&e, false, None)))?,
        }
        let result = async {
            let orders = host.fetch_orders().await?;
            later().await;
            if host.disposed() {
                return Ok(true);
            }
            if !orders.is_empty() || host.store().orders().is_empty() {
                host.store()
                    .update(
                        StoreUpdate {
                            orders: Some(orders),
                            ..Default::default()
                        },
                        false,
                    )
                    .map_err(ServiceError::Store)?;
            }
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => (),
            Err(e) => emit(host.service(), Some(&error_value(&e, false, None)))?,
        }
        if !host.disposed() {
            host.schedule_tick(Some(count + 1.0), host.tick_interval().unwrap_or(10_000.0))?;
        }
    }
    Ok(())
}
