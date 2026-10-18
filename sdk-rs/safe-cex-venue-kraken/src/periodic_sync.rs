//! Periodic method bodies; the host owns Promise tasks and loop scheduling.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::StoreDataLoaded;
use std::rc::Rc;

pub enum TickUpdate<O: ?Sized, P: ?Sized, B: ?Sized> {
    Orders(Rc<O>),
    Positions(Rc<P>),
    Balance(Rc<B>, StoreDataLoaded),
    Loaded(StoreDataLoaded),
}

pub type TickCallback = Rc<dyn Fn() -> Result<ExchangeFuture<'static, ()>, ExchangeError>>;

pub trait TickHost {
    type Orders: ?Sized;
    type Positions: ?Sized;
    type Balance: ?Sized;
    type Task;
    fn is_disposed(&self) -> bool;
    fn fetch_leverage_settings(&self) -> Result<ExchangeFuture<'static, ()>, ExchangeError>;
    fn fetch_orders(&self) -> Result<ExchangeFuture<'static, Rc<Self::Orders>>, ExchangeError>;
    fn fetch_positions(
        &self,
    ) -> Result<ExchangeFuture<'static, Rc<Self::Positions>>, ExchangeError>;
    fn fetch_balance(&self) -> Result<ExchangeFuture<'static, Rc<Self::Balance>>, ExchangeError>;
    fn balance_truthy(&self, balance: &Self::Balance) -> bool;
    fn loaded(&self) -> Result<StoreDataLoaded, ExchangeError>;
    fn update(
        &self,
        update: TickUpdate<Self::Orders, Self::Positions, Self::Balance>,
    ) -> Result<(), ExchangeError>;
    fn start_then(&self, task: ExchangeFuture<'static, ()>) -> Result<Self::Task, ExchangeError>;
    fn promise_all(
        &self,
        tasks: Vec<Self::Task>,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError>;
    fn emit_tick_error(
        &self,
        error: ExchangeError,
        fallback: &'static str,
    ) -> Result<(), ExchangeError>;
    fn current_tick(&self, count: f64) -> Result<ExchangeFuture<'static, ()>, ExchangeError>;
    fn schedule_loop(&self, callback: TickCallback, delay: f64) -> Result<(), ExchangeError>;
}

async fn sync<H: TickHost + ?Sized + 'static>(host: Rc<H>) -> Result<(), ExchangeError> {
    let fetched = host.fetch_orders()?;
    let orders_host = host.clone();
    let orders = host.start_then(Box::pin(async move {
        let orders = fetched.await?;
        orders_host.update(TickUpdate::Orders(orders))?;
        let mut loaded = orders_host.loaded()?;
        loaded.orders = true;
        orders_host.update(TickUpdate::Loaded(loaded))
    }))?;
    let fetched = host.fetch_positions()?;
    let positions_host = host.clone();
    let positions = host.start_then(Box::pin(async move {
        let positions = fetched.await?;
        positions_host.update(TickUpdate::Positions(positions))?;
        let mut loaded = positions_host.loaded()?;
        loaded.positions = true;
        positions_host.update(TickUpdate::Loaded(loaded))
    }))?;
    let fetched = host.fetch_balance()?;
    let balance_host = host.clone();
    let balance = host.start_then(Box::pin(async move {
        let balance = fetched.await?;
        if balance_host.balance_truthy(&balance) {
            let mut loaded = balance_host.loaded()?;
            loaded.balance = true;
            balance_host.update(TickUpdate::Balance(balance, loaded))?;
        }
        Ok(())
    }))?;
    host.promise_all(vec![orders, positions, balance])?.await
}

pub async fn futures_tick<H: TickHost + ?Sized + 'static>(
    host: Rc<H>,
    count: Option<f64>,
) -> Result<(), ExchangeError> {
    let count = count.unwrap_or(0.0);
    if !host.is_disposed() {
        let result = async {
            if count == 0.0 || count % 20.0 == 0.0 {
                host.fetch_leverage_settings()?.await?;
                sync(host.clone()).await?;
            }
            Ok::<_, ExchangeError>(())
        }
        .await;
        if let Err(error) = result {
            host.emit_tick_error(error, "Tick data fetch failed")?;
        }
        if !host.is_disposed() {
            let next = host.clone();
            host.schedule_loop(Rc::new(move || next.current_tick(count + 1.0)), 500.0)?;
        }
    }
    Ok(())
}

pub async fn spot_tick<H: TickHost + ?Sized + 'static>(
    host: Rc<H>,
    count: Option<f64>,
) -> Result<(), ExchangeError> {
    let count = count.unwrap_or(0.0);
    if !host.is_disposed() {
        let result = async {
            if count == 0.0 || count % 20.0 == 0.0 {
                sync(host.clone()).await?;
            }
            Ok::<_, ExchangeError>(())
        }
        .await;
        if let Err(error) = result {
            host.emit_tick_error(error, "Tick data fetch failed")?;
        }
        if !host.is_disposed() {
            let next = host.clone();
            host.schedule_loop(Rc::new(move || next.current_tick(count + 1.0)), 500.0)?;
        }
    }
    Ok(())
}
