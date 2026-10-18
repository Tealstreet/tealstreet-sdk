//! Startup bodies retain opaque fetched batches and host-bound socket dependencies.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::StoreDataLoaded;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketKind {
    FuturesPublic,
    FuturesPrivate,
    SpotPublic,
    SpotPrivate,
}

pub trait StartupSocket {
    fn connect_and_subscribe(&self) -> Result<(), ExchangeError>;
}

pub struct StartupState<S: ?Sized> {
    pub disposed: Cell<bool>,
    pub public_data_only: Cell<bool>,
    pub public_socket: RefCell<Option<Rc<S>>>,
    pub private_socket: RefCell<Option<Rc<S>>>,
}

pub enum StartupUpdate<M: ?Sized, T: ?Sized> {
    Markets(Rc<M>, StoreDataLoaded),
    Tickers(Rc<T>, StoreDataLoaded),
    Loaded(StoreDataLoaded),
}

pub trait StartupHost {
    type Socket: StartupSocket + ?Sized;
    type Markets: ?Sized;
    type Tickers: ?Sized;
    fn state(&self) -> &StartupState<Self::Socket>;
    fn init_external_public_data(&self) -> Result<ExchangeFuture<'static, bool>, ExchangeError>;
    fn fetch_markets(&self) -> Result<ExchangeFuture<'static, Rc<Self::Markets>>, ExchangeError>;
    fn fetch_tickers(&self) -> Result<ExchangeFuture<'static, Rc<Self::Tickers>>, ExchangeError>;
    fn loaded(&self) -> Result<StoreDataLoaded, ExchangeError>;
    fn update(
        &self,
        update: StartupUpdate<Self::Markets, Self::Tickers>,
    ) -> Result<(), ExchangeError>;
    fn create_socket(&self, kind: SocketKind) -> Result<Rc<Self::Socket>, ExchangeError>;
    fn start_tick(&self, count: f64) -> Result<(), ExchangeError>;
}

fn connect_current<S: StartupSocket + ?Sized>(
    slot: &RefCell<Option<Rc<S>>>,
) -> Result<(), ExchangeError> {
    let socket = slot
        .borrow()
        .clone()
        .ok_or_else(|| ExchangeError::new("Startup socket is undefined"))?;
    socket.connect_and_subscribe()
}

async fn start<H: StartupHost + ?Sized>(
    host: Rc<H>,
    public: SocketKind,
    private: SocketKind,
) -> Result<(), ExchangeError> {
    let external = host.init_external_public_data()?.await?;
    if !external {
        let markets = host.fetch_markets()?.await?;
        if host.state().disposed.get() {
            return Ok(());
        }
        let mut loaded = host.loaded()?;
        loaded.markets = true;
        host.update(StartupUpdate::Markets(markets, loaded))?;

        let tickers = host.fetch_tickers()?.await?;
        if host.state().disposed.get() {
            return Ok(());
        }
        let mut loaded = host.loaded()?;
        loaded.tickers = true;
        host.update(StartupUpdate::Tickers(tickers, loaded))?;

        let socket = host.create_socket(public)?;
        *host.state().public_socket.borrow_mut() = Some(socket);
        connect_current(&host.state().public_socket)?;
    }
    if !host.state().public_data_only.get() {
        let socket = host.create_socket(private)?;
        *host.state().private_socket.borrow_mut() = Some(socket);
        connect_current(&host.state().private_socket)?;
    }
    if host.state().public_data_only.get() {
        let mut loaded = host.loaded()?;
        loaded.balance = true;
        loaded.positions = true;
        loaded.orders = true;
        host.update(StartupUpdate::Loaded(loaded))?;
        return Ok(());
    }
    host.start_tick(0.0)?;
    Ok(())
}

pub async fn start_futures<H: StartupHost + ?Sized>(host: Rc<H>) -> Result<(), ExchangeError> {
    start(host, SocketKind::FuturesPublic, SocketKind::FuturesPrivate).await
}

pub async fn start_spot<H: StartupHost + ?Sized>(host: Rc<H>) -> Result<(), ExchangeError> {
    start(host, SocketKind::SpotPublic, SocketKind::SpotPrivate).await
}

pub async fn fetch_and_set_orders<
    B: ?Sized,
    F: std::future::Future<Output = Result<Rc<B>, ExchangeError>>,
>(
    fetch: impl FnOnce() -> Result<F, ExchangeError>,
    add_full: impl FnOnce(&Rc<B>) -> Result<(), ExchangeError>,
    loaded: impl FnOnce() -> Result<StoreDataLoaded, ExchangeError>,
    update: impl FnOnce(StoreDataLoaded) -> Result<(), ExchangeError>,
) -> Result<Rc<B>, ExchangeError> {
    let orders = fetch()?.await?;
    add_full(&orders)?;
    let mut flags = loaded()?;
    flags.orders = true;
    update(flags)?;
    Ok(orders)
}
