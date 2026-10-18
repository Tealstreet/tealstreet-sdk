//! Exchange lifecycle field bodies. Imported HTTP, BaseExchange, store and timer
//! operations are explicit host calls; asynchronous calls use hot JS promises.
use crate::{
    portfolios_balances::{Portfolio, PortfolioFailure},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use indexmap::IndexMap;
use safe_cex_num::js_truthy;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

pub type LifecycleTimer = Rc<RefCell<safe_cex_exchange::js_runtime::value::Js>>;
pub type LifecycleCallback = Rc<dyn Fn() -> Result<(), PortfolioFailure>>;
pub type LifecycleAsync<T> = Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>;
/// Host updates are native fields with retained value roots, before serialization.
pub type LifecycleUpdate = IndexMap<String, Frame>;
pub trait LifecycleHost {
    fn is_disposed(&self) -> bool;
    fn public_only(&self) -> bool;
    fn init_external_public_data(&self) -> LifecycleAsync<bool>;
    fn find_selected_portfolio(&self) -> LifecycleAsync<Portfolio>;
    fn fetch_markets(&self) -> LifecycleAsync<Frame>;
    fn fetch_tickers_intx(&self) -> LifecycleAsync<Frame>;
    fn fetch_balance(&self) -> LifecycleAsync<Frame>;
    fn fetch_open_orders_snapshot(&self) -> LifecycleAsync<Frame>;
    fn tick(&self) -> LifecycleAsync<()>;
    fn get_markets(&self, endpoint: &'static str) -> LifecycleAsync<()>;
    fn loaded(&self) -> Frame;
    fn update(&self, fields: LifecycleUpdate) -> LifecycleAsync<()>;
    fn connect_public(&self) -> Result<(), PortfolioFailure>;
    fn connect_private(&self) -> Result<(), PortfolioFailure>;
    fn dispose_base(&self) -> Result<(), PortfolioFailure>;
    fn dispose_public(&self) -> Result<(), PortfolioFailure>;
    fn dispose_private(&self) -> Result<(), PortfolioFailure>;
    fn clear_timeout(&self, timer: LifecycleTimer) -> Result<(), PortfolioFailure>;
    fn set_timeout(
        &self,
        callback: LifecycleCallback,
        delay_ms: f64,
    ) -> Result<LifecycleTimer, PortfolioFailure>;
    fn last_private_user_stream_message_at(&self) -> f64;
    fn mark_private_user_stream_degraded(
        &self,
        reason: &'static str,
        force_toast: bool,
    ) -> Result<(), PortfolioFailure>;
    fn merge_rest_orders_with_fresh_ws_state(
        &self,
        orders: Frame,
    ) -> Result<Frame, PortfolioFailure>;
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure>;
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure>;
    fn log(&self, message: &str, severity: Option<&str>) -> Result<(), PortfolioFailure>;
}
pub struct LifecycleReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
    pub selected: Rc<RefCell<Option<Portfolio>>>,
    pub order_write_reconcile_timeout: Rc<RefCell<Option<LifecycleTimer>>>,
    pub private_user_stream_write_check_timeout: Rc<RefCell<Option<LifecycleTimer>>>,
}
const DEGRADED: &str = "Coinbase private WS is not receiving live order updates. Orders will reconcile through REST; if your Coinbase key restricts IPs, whitelist both Tealstreet proxy IPs and your current browser IP.";
fn loaded<H: LifecycleHost>(r: &LifecycleReceiver<H>, keys: &[&str]) -> Frame {
    let old = r.host.loaded();
    let mut values = IndexMap::new();
    for key in old.own_keys() {
        if let Frame::Object(object) = &old {
            values.insert(key.clone(), object.borrow()[&key].clone());
        }
    }
    for key in keys {
        values.insert(key.encode_utf16().collect(), Frame::Bool(true));
    }
    Frame::Object(Rc::new(RefCell::new(values)))
}
fn fields(values: impl IntoIterator<Item = (&'static str, Frame)>) -> LifecycleUpdate {
    values
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}
fn error_prop(e: &PortfolioFailure, keys: &[&str]) -> Option<Value> {
    let mut value = e.value.as_ref();
    for key in keys {
        value = value.and_then(|v| v.get(*key));
    }
    value.cloned()
}
fn fallback(a: Option<Value>, b: impl FnOnce() -> Option<Value>) -> Option<Value> {
    if js_truthy(a.as_ref()) { a } else { b() }
}
pub fn dispose<H: LifecycleHost>(r: &LifecycleReceiver<H>) -> Result<(), PortfolioFailure> {
    r.host.dispose_base()?;
    let timer = r.order_write_reconcile_timeout.borrow().clone();
    if let Some(timer) = timer {
        r.host.clear_timeout(timer)?;
        *r.order_write_reconcile_timeout.borrow_mut() = None;
    }
    let timer = r.private_user_stream_write_check_timeout.borrow().clone();
    if let Some(timer) = timer {
        r.host.clear_timeout(timer)?;
        *r.private_user_stream_write_check_timeout.borrow_mut() = None;
    }
    r.host.dispose_public()?;
    r.host.dispose_private()
}
#[derive(Clone)]
pub struct LifecycleAccount {
    pub user_id: String,
}
pub type LifecycleAccountRoot = Rc<RefCell<LifecycleAccount>>;
pub fn get_account<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
) -> Promise<Result<LifecycleAccountRoot, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    get_account_from_selection(
        r.selected.clone(),
        move || r.host.find_selected_portfolio(),
        &spawn,
    )
}
/// The class and standalone lifecycle receiver execute the same field body.
pub fn get_account_from_selection(
    selected: Rc<RefCell<Option<Portfolio>>>,
    find: impl FnOnce() -> LifecycleAsync<Portfolio> + 'static,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> Promise<Result<LifecycleAccountRoot, PortfolioFailure>> {
    Promise::from_future(
        Box::pin(async move {
            let missing = selected.borrow().is_none();
            if missing {
                let portfolio = find()?.await?;
                *selected.borrow_mut() = Some(portfolio);
            }
            let selected = selected
                .borrow()
                .clone()
                .expect("assigned selected portfolio");
            let uuid = selected.borrow().uuid.clone();
            Ok(Rc::new(RefCell::new(LifecycleAccount { user_id: uuid })))
        }),
        spawn,
    )
}
/// Account-validation result: an absent balance is the source's own undefined value.
#[derive(Clone)]
pub struct LifecycleValidation {
    pub error: Value,
    pub balance: Option<Frame>,
    pub has_balance: bool,
}
pub fn validate_account<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
) -> Promise<Result<LifecycleValidation, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let validation = async {
                r.host.get_markets("/api/v3/brokerage/products")?.await?;
                // Calling fetchBalance may throw before returning a Promise. Only a
                // rejection of the returned Promise is swallowed by .catch().
                let balance = r.host.fetch_balance()?;
                let (caught, resolve) = Promise::pending();
                balance.observe(&r.spawn, move |value| resolve.resolve(value.ok()));
                let balance = caught.await;
                Ok::<_, PortfolioFailure>(LifecycleValidation {
                    error: Value::String(String::new()),
                    balance,
                    has_balance: true,
                })
            }
            .await;
            match validation {
                Ok(value) => Ok(value),
                Err(e) => {
                    r.host.emit_and_log_error(fallback(
                        error_prop(&e, &["response", "data", "message"]),
                        || error_prop(&e, &["message"]),
                    ))?;
                    let error = fallback(error_prop(&e, &["response", "data", "msg"]), || {
                        fallback(error_prop(&e, &["message"]), || {
                            Some(Value::String("Invalid API key or secret".into()))
                        })
                    })
                    .expect("fallback string");
                    Ok(LifecycleValidation {
                        error,
                        balance: None,
                        has_balance: false,
                    })
                }
            }
        }),
        &spawn,
    )
}
pub fn start<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let external = r.host.init_external_public_data()?.await?;
            if !r.host.public_only() {
                let selected = r.host.find_selected_portfolio()?.await?;
                *r.selected.borrow_mut() = Some(selected);
            }
            if !external {
                let markets = r.host.fetch_markets()?.await?;
                if r.host.is_disposed() {
                    return Ok(());
                }
                r.host
                    .update(fields([
                        ("markets", markets),
                        ("loaded", loaded(&r, &["markets"])),
                    ]))?
                    .await?;
                r.host.connect_public()?;
            }
            if !r.host.public_only() {
                // Source deliberately does not await the hot private startup.
                drop(start_private(r.clone()));
            } else {
                start_public_data_only(r.clone()).await?;
            }
            Ok(())
        }),
        &spawn,
    )
}
pub fn start_private<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            r.host.connect_private()?;
            if r.selected.borrow().is_none() {
                let selected = r.host.find_selected_portfolio()?.await?;
                *r.selected.borrow_mut() = Some(selected);
            }
            if r.host.is_disposed() {
                return Ok(());
            }
            r.host.tick()?.await?;
            if r.host.is_disposed() {
                return Ok(());
            }
            Ok(())
        }),
        &spawn,
    )
}
pub fn start_public_data_only<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            if r.host.is_disposed() {
                return Ok(());
            }
            r.host.log("Ready to watch public data on Coinbase", None)?;
            let tickers = r.host.fetch_tickers_intx()?.await?;
            let zero_balance =
                Frame::from_json(&serde_json::json!({"used":0,"free":0,"total":0,"upnl":0}));
            let empty = || Frame::Array(Rc::new(RefCell::new(Vec::new())));
            drop(r.host.update(fields([
                ("tickers", tickers),
                ("orders", empty()),
                ("balance", zero_balance),
                ("positions", empty()),
                (
                    "loaded",
                    loaded(&r, &["tickers", "orders", "balance", "positions"]),
                ),
            ]))?);
            Ok(())
        }),
        &spawn,
    )
}
pub fn schedule_private_user_stream_write_check<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
) -> Result<(), PortfolioFailure> {
    let last_at_schedule = r.host.last_private_user_stream_message_at();
    let timer = r.private_user_stream_write_check_timeout.borrow().clone();
    if let Some(timer) = timer {
        r.host.clear_timeout(timer)?;
    }
    let callback_receiver = r.clone();
    let timer = r.host.set_timeout(
        Rc::new(move || {
            *callback_receiver
                .private_user_stream_write_check_timeout
                .borrow_mut() = None;
            if callback_receiver.host.is_disposed()
                || callback_receiver.host.last_private_user_stream_message_at() > last_at_schedule
            {
                return Ok(());
            }
            callback_receiver
                .host
                .mark_private_user_stream_degraded(DEGRADED, true)
        }),
        8000.0,
    )?;
    *r.private_user_stream_write_check_timeout.borrow_mut() = Some(timer);
    Ok(())
}
pub fn reconcile_open_orders_from_rest<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
    reason: String,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let reconciliation = async {
                let orders = r.host.fetch_open_orders_snapshot()?.await?;
                if r.host.is_disposed() {
                    return Ok(());
                }
                let orders = r.host.merge_rest_orders_with_fresh_ws_state(orders)?;
                drop(r.host.update(fields([
                    ("orders", orders),
                    ("loaded", loaded(&r, &["orders"])),
                ]))?);
                Ok::<_, PortfolioFailure>(())
            }
            .await;
            if let Err(e) = reconciliation {
                r.host.log(
                    &format!("Coinbase {reason} REST order reconciliation failed"),
                    Some("warning"),
                )?;
                r.host.emit_orders_fetch_error(e)?;
            }
            Ok(())
        }),
        &spawn,
    )
}
pub fn schedule_open_orders_rest_reconciliation<H: LifecycleHost + 'static>(
    r: Rc<LifecycleReceiver<H>>,
    reason: String,
) -> Result<(), PortfolioFailure> {
    if r.host.public_only() {
        return Ok(());
    }
    let timer = r.order_write_reconcile_timeout.borrow().clone();
    if let Some(timer) = timer {
        r.host.clear_timeout(timer)?;
    }
    let callback_receiver = r.clone();
    let timer = r.host.set_timeout(
        Rc::new(move || {
            *callback_receiver.order_write_reconcile_timeout.borrow_mut() = None;
            drop(reconcile_open_orders_from_rest(
                callback_receiver.clone(),
                reason.clone(),
            ));
            Ok(())
        }),
        2000.0,
    )?;
    *r.order_write_reconcile_timeout.borrow_mut() = Some(timer);
    Ok(())
}
