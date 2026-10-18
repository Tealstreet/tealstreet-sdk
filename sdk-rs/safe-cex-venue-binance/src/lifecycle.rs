//! Extracted startup/private startup/tick bodies at current caller-bound methods.
//! Futures are handles to owner-retained jobs; concrete class/Store/socket/loop
//! wiring, discarded-promise rejection/drop/unwind ownership and arbitrary
//! executable graph properties remain outside this body boundary. Exceptions are
//! normalized immutable Error/message values; non-Error rejection values, mutable
//! exception metadata and concrete runtime handling of discarded rejections are
//! unclaimed. Timer resolvers retain the same owner and resolve its current method
//! and receiver at fire; acquiring that owner handle is non-observable. Dense
//! decoded graphs, own undefined, stock coercion/properties, and observed shared
//! field mutations are supported; symbol keys, arbitrary executable graph hooks,
//! non-BMP string spreads/lone UTF16 and broad prototype mutation are unclaimed.
use crate::{
    private_event_value::*,
    request_weight::{Failure, type_error},
};
use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};
const HEALTH_INTERVAL: f64 = 3000.0;
const ACCOUNT_INTERVAL: f64 = 25000.0;
const DEGRADED_INTERVAL: f64 = 3000.0;
const RISK_INTERVAL: f64 = 75000.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleRead {
    NeedsOverlay,
    Disposed,
    PublicOnly,
    PrivateConnected,
    OrdersStale,
    Loaded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleSync {
    Update,
    Log,
    PublicConnect,
    PrivateConnect,
    SetSetting,
    Paused,
    Version,
    TickInterval,
    Emit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleAsync {
    Overlay,
    External,
    Markets,
    Tickers,
    StartPrivate,
    StartPublicOnly,
    PositionMode,
    Tick,
    Orders,
    RetryOrders,
    Positions,
    PositionsBalance,
    Balance,
}
pub type LifecycleFuture = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
pub type LifecycleCall = Rc<dyn Fn(Vec<EventValue>) -> Result<EventValue, Failure>>;
pub type LifecycleAsyncCall = Rc<dyn Fn(Vec<EventValue>) -> Result<LifecycleFuture, Failure>>;
/// The resolver reads the owner's CURRENT tick method/receiver at timer fire.
/// Obtaining this non-observable owner handle at registration reads no TS method.
pub type TickResolver = Rc<dyn Fn() -> Result<LifecycleAsyncCall, Failure>>;
pub type TickTimer = Rc<dyn Fn() -> Result<LifecycleFuture, Failure>>;
pub trait LifecycleBindings {
    fn read(&mut self, key: LifecycleRead) -> Result<EventValue, Failure>;
    fn capture(&mut self, key: LifecycleSync) -> Result<LifecycleCall, Failure>;
    fn capture_async(&mut self, key: LifecycleAsync) -> Result<LifecycleAsyncCall, Failure>;
    fn now(&mut self) -> Result<f64, Failure>;
    fn tick_resolver(&mut self) -> TickResolver;
    /// Invoke configured imported loop with original callback/raw interval. The
    /// runtime owns any discarded async loop/job rejection; synchronous binding
    /// failures alone are Err. No fallback interval or sleep/RAF policy is copied.
    fn schedule(&mut self, timer: TickTimer, interval: EventValue) -> Result<(), Failure>;
}
/// Source private numeric data fields, shared with caller reentrant callbacks.
#[derive(Default)]
pub struct ReconciliationTimes {
    pub risk: Cell<f64>,
    pub account: Cell<f64>,
}
fn yes(b: &mut impl LifecycleBindings, k: LifecycleRead) -> Result<bool, Failure> {
    Ok(b.read(k)?.truthy())
}
fn sync(
    b: &mut impl LifecycleBindings,
    k: LifecycleSync,
    args: Vec<EventValue>,
) -> Result<EventValue, Failure> {
    b.capture(k)?(args)
}
async fn call(b: &mut impl LifecycleBindings, k: LifecycleAsync) -> Result<EventValue, Failure> {
    b.capture_async(k)?(vec![])?.await
}
fn loaded(b: &mut impl LifecycleBindings, keys: &[&str]) -> Result<EventValue, Failure> {
    let out = EventValue::record_object(b.read(LifecycleRead::Loaded)?.spread());
    for key in keys {
        out.write(key, EventValue::Bool(true))
    }
    Ok(out)
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 && (a.is_sign_negative() || b.is_sign_negative()) {
        -0.0
    } else {
        a.min(b)
    }
}
async fn start(b: &mut impl LifecycleBindings, overlay: bool) -> Result<(), Failure> {
    if overlay && yes(b, LifecycleRead::NeedsOverlay)? {
        call(b, LifecycleAsync::Overlay).await?;
        if yes(b, LifecycleRead::Disposed)? {
            return Ok(());
        }
    }
    let external = call(b, LifecycleAsync::External).await?;
    if !external.truthy() {
        let markets = call(b, LifecycleAsync::Markets).await?;
        if yes(b, LifecycleRead::Disposed)? {
            return Ok(());
        }
        let update = b.capture(LifecycleSync::Update)?;
        let flags = loaded(b, &["markets"])?;
        update(vec![record([
            ("markets", markets.clone()),
            ("loaded", flags),
        ])])?;
        let tickers = call(b, LifecycleAsync::Tickers).await?;
        if yes(b, LifecycleRead::Disposed)? {
            return Ok(());
        }
        let log = b.capture(LifecycleSync::Log)?;
        let ticker_length = tickers.read("length")?;
        let market_length = markets.read("length")?;
        let count = min(ticker_length.number()?, market_length.number()?);
        log(vec![string(format!(
            "Loaded {} Binance markets",
            safe_cex_num::js_number_string(count)
        ))])?;
        let update = b.capture(LifecycleSync::Update)?;
        let flags = loaded(b, &["tickers"])?;
        update(vec![record([("tickers", tickers), ("loaded", flags)])])?;
        sync(b, LifecycleSync::PublicConnect, vec![])?;
    }
    let key = if !yes(b, LifecycleRead::PublicOnly)? {
        LifecycleAsync::StartPrivate
    } else {
        LifecycleAsync::StartPublicOnly
    };
    call(b, key).await?;
    Ok(())
}
// ts: BinanceExchange.start
pub async fn start_futures(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    start(b, true).await
}
// ts: BinancePMExchange.start
pub async fn start_pm(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    start(b, true).await
}
// ts: BinanceSpotExchange.start
pub async fn start_spot(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    start(b, false).await
}
async fn start_private(b: &mut impl LifecycleBindings, mode: bool) -> Result<(), Failure> {
    sync(b, LifecycleSync::PrivateConnect, vec![])?;
    if mode {
        let setter = b.capture(LifecycleSync::SetSetting)?;
        let mode = call(b, LifecycleAsync::PositionMode).await?;
        setter(vec![string("isHedged"), mode])?;
    }
    call(b, LifecycleAsync::Tick).await?;
    if yes(b, LifecycleRead::Disposed)? {
        return Ok(());
    }
    sync(
        b,
        LifecycleSync::Log,
        vec![string("Ready to trade on Binance")],
    )?;
    let orders = call(b, LifecycleAsync::Orders).await?;
    if yes(b, LifecycleRead::Disposed)? {
        return Ok(());
    }
    if mode && yes(b, LifecycleRead::OrdersStale)? {
        return Ok(());
    }
    sync(b, LifecycleSync::Log, vec![string("Loaded Binance orders")])?;
    let update = b.capture(LifecycleSync::Update)?;
    let flags = loaded(b, &["orders"])?;
    update(vec![record([("orders", orders), ("loaded", flags)])])?;
    Ok(())
}
// ts: BinanceExchange._startPrivate
pub async fn start_private_futures(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    start_private(b, true).await
}
// ts: BinancePMExchange._startPrivate
pub async fn start_private_pm(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    start_private(b, true).await
}
// ts: BinanceSpotExchange._startPrivate
pub async fn start_private_spot(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    start_private(b, false).await
}
fn unpack(v: EventValue) -> Result<(EventValue, EventValue), Failure> {
    if v.nullish() {
        return Err(type_error(format!(
            "Cannot destructure property 'balance' of '(intermediate value)' as it is {}.",
            if matches!(v, EventValue::Null) {
                "null"
            } else {
                "undefined"
            }
        )));
    }
    Ok((v.read("balance")?, v.read("positions")?))
}
fn plus_one(v: &EventValue) -> Result<EventValue, Failure> {
    match v {
        EventValue::String(_)
        | EventValue::Array(_)
        | EventValue::Object(_)
        | EventValue::Builtin(_) => Ok(string(format!("{}1", v.text()?))),
        _ => Ok(number(v.number()? + 1.0)),
    }
}
fn schedule(
    b: &mut impl LifecycleBindings,
    count: Option<EventValue>,
    interval: EventValue,
) -> Result<(), Failure> {
    let resolve = b.tick_resolver();
    let timer: TickTimer = Rc::new(move || {
        let tick = resolve()?;
        let args = if let Some(count) = &count {
            vec![plus_one(count)?]
        } else {
            vec![]
        };
        tick(args)
    });
    b.schedule(timer, interval)
}
fn emit(b: &mut impl LifecycleBindings, e: &Failure) -> Result<(), Failure> {
    sync(b, LifecycleSync::Emit, vec![string(&e.message)])?;
    Ok(())
}
// ts: BinanceExchange.tick
pub async fn tick_futures(
    times: &ReconciliationTimes,
    b: &mut impl LifecycleBindings,
) -> Result<(), Failure> {
    if !yes(b, LifecycleRead::Disposed)? {
        let now = b.now()?;
        let interval = if yes(b, LifecycleRead::PrivateConnected)? {
            ACCOUNT_INTERVAL
        } else {
            DEGRADED_INTERVAL
        };
        let risk = !sync(b, LifecycleSync::Paused, vec![])?.truthy()
            && (times.risk.get() == 0.0 || now - times.risk.get() >= RISK_INTERVAL);
        let account = !sync(b, LifecycleSync::Paused, vec![])?.truthy()
            && (times.account.get() == 0.0 || now - times.account.get() >= interval);
        let result = async {
            if yes(b, LifecycleRead::OrdersStale)? {
                call(b, LifecycleAsync::RetryOrders).await?;
            }
            if yes(b, LifecycleRead::Disposed)? {
                return Ok(true);
            }
            if risk {
                let version = sync(b, LifecycleSync::Version, vec![])?;
                let positions = call(b, LifecycleAsync::Positions).await?;
                if version.same(&sync(b, LifecycleSync::Version, vec![])?) {
                    sync(
                        b,
                        LifecycleSync::Update,
                        vec![record([("positions", positions)])],
                    )?;
                }
                times.risk.set(b.now()?);
                if yes(b, LifecycleRead::Disposed)? {
                    return Ok(true);
                }
            }
            if account && !sync(b, LifecycleSync::Paused, vec![])?.truthy() {
                let version = sync(b, LifecycleSync::Version, vec![])?;
                let data = call(b, LifecycleAsync::PositionsBalance).await?;
                let (balance, positions) = unpack(data)?;
                if yes(b, LifecycleRead::Disposed)? {
                    return Ok(true);
                }
                let update = b.capture(LifecycleSync::Update)?;
                let out = record([("balance", balance)]);
                if version.same(&sync(b, LifecycleSync::Version, vec![])?) {
                    out.write("positions", positions)
                }
                out.write("loaded", loaded(b, &["balance", "positions"])?);
                update(vec![out])?;
                times.account.set(b.now()?);
            }
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) => emit(b, &e)?,
        }
        if !yes(b, LifecycleRead::Disposed)? {
            schedule(b, None, number(HEALTH_INTERVAL))?;
        }
    }
    Ok(())
}
// ts: BinancePMExchange.tick
pub async fn tick_pm(count: EventValue, b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    let count = if matches!(count, EventValue::Undefined) {
        number(0.0)
    } else {
        count
    };
    if !yes(b, LifecycleRead::Disposed)? {
        let result = async {
            if yes(b, LifecycleRead::OrdersStale)? {
                call(b, LifecycleAsync::RetryOrders).await?;
            }
            if yes(b, LifecycleRead::Disposed)? {
                return Ok(true);
            }
            if count.number()? % 16.0 == 0.0 {
                let positions = call(b, LifecycleAsync::Positions).await?;
                sync(
                    b,
                    LifecycleSync::Update,
                    vec![record([("positions", positions)])],
                )?;
                if yes(b, LifecycleRead::Disposed)? {
                    return Ok(true);
                }
            }
            let data = call(b, LifecycleAsync::Balance).await?;
            let (balance, _) = unpack(data)?;
            if yes(b, LifecycleRead::Disposed)? {
                return Ok(true);
            }
            let update = b.capture(LifecycleSync::Update)?;
            let flags = loaded(b, &["balance", "positions"])?;
            update(vec![record([("balance", balance), ("loaded", flags)])])?;
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) => emit(b, &e)?,
        }
        if !yes(b, LifecycleRead::Disposed)? {
            schedule(b, Some(count), number(5000.0))?;
        }
    }
    Ok(())
}
// ts: BinanceSpotExchange.tick
pub async fn tick_spot(b: &mut impl LifecycleBindings) -> Result<(), Failure> {
    if !yes(b, LifecycleRead::Disposed)? {
        let result = async {
            let data = call(b, LifecycleAsync::PositionsBalance).await?;
            let (balance, positions) = unpack(data)?;
            if yes(b, LifecycleRead::Disposed)? {
                return Ok(true);
            }
            let update = b.capture(LifecycleSync::Update)?;
            let flags = loaded(b, &["balance", "positions"])?;
            update(vec![record([
                ("balance", balance),
                ("positions", positions),
                ("loaded", flags),
            ])])?;
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) => emit(b, &e)?,
        }
        if !yes(b, LifecycleRead::Disposed)? {
            let interval = sync(b, LifecycleSync::TickInterval, vec![])?;
            schedule(b, None, interval)?;
        }
    }
    Ok(())
}
