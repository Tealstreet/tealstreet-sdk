//! Exchange and websocket lifecycle control over opaque host values.
use std::future::Future;

pub trait ExchangeSessionHost {
    type Value;
    type Socket;
    type Error;
    fn external_public(&mut self) -> impl Future<Output = Result<bool, Self::Error>>;
    fn fetch(
        &mut self,
        member: &'static str,
    ) -> impl Future<Output = Result<Self::Value, Self::Error>>;
    fn fetch_balance(
        &mut self,
    ) -> impl Future<Output = Result<(Self::Value, Self::Value), Self::Error>>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn public_only(&mut self) -> Result<bool, Self::Error>;
    fn loaded(&mut self) -> Result<Self::Value, Self::Error>;
    fn update(
        &mut self,
        fields: Vec<(&'static str, &Self::Value)>,
        loaded: Option<(Self::Value, Vec<&'static str>)>,
    ) -> Result<(), Self::Error>;
    fn connect(&mut self, socket: &'static str) -> Result<(), Self::Error>;
    fn configure_shards(&mut self, markets: &Self::Value)
    -> Result<Vec<Self::Socket>, Self::Error>;
    fn connect_shard(&mut self, socket: Self::Socket) -> Result<(), Self::Error>;
    fn length(&mut self, rows: &Self::Value) -> Result<usize, Self::Error>;
    fn log(&mut self, text: Vec<u16>) -> Result<(), Self::Error>;
    fn set_hedged(&mut self, mode: Self::Value) -> Result<(), Self::Error>;
    fn initial_tick(&mut self) -> impl Future<Output = Result<(), Self::Error>>;
    fn position_modes_equal(&mut self) -> Result<bool, Self::Error>;
    fn copy_position_mode(&mut self) -> Result<(), Self::Error>;
    fn report_message(&mut self, error: Self::Error) -> Result<(), Self::Error>;
    fn tick_interval(&mut self) -> Result<f64, Self::Error>;
    fn schedule_tick(&mut self, next_count: f64, interval: f64) -> Result<(), Self::Error>;
}

pub async fn start<H: ExchangeSessionHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.external_public().await? {
        let markets = host.fetch("fetchMarkets").await?;
        if host.disposed()? {
            return Ok(());
        }
        let loaded = host.loaded()?;
        host.update(vec![("markets", &markets)], Some((loaded, vec!["markets"])))?;
        host.connect("futuresWebsocket")?;
        let tickers = host.fetch("fetchTickers").await?;
        if host.disposed()? {
            return Ok(());
        }
        let ticker_len = host.length(&tickers)?;
        let count = ticker_len.min(host.length(&markets)?);
        host.log(
            format!("Loaded {count} Bitunix markets")
                .encode_utf16()
                .collect(),
        )?;
        let loaded = host.loaded()?;
        host.update(vec![("tickers", &tickers)], Some((loaded, vec!["tickers"])))?;
        for socket in host.configure_shards(&markets)? {
            host.connect_shard(socket)?;
        }
    }
    if host.public_only()? {
        let loaded = host.loaded()?;
        host.update(
            vec![],
            Some((loaded, vec!["balance", "positions", "orders"])),
        )?;
        return Ok(());
    }
    host.connect("privateWebsocket")?;
    let mode = host.fetch("fetchPositionMode").await?;
    host.set_hedged(mode)?;
    host.initial_tick().await?;
    if host.disposed()? {
        return Ok(());
    }
    host.log("Ready to trade on Bitunix".encode_utf16().collect())?;
    let orders = host.fetch("fetchOrders").await?;
    if host.disposed()? {
        return Ok(());
    }
    host.log("Loaded Bitunix orders".encode_utf16().collect())?;
    let loaded = host.loaded()?;
    host.update(vec![("orders", &orders)], Some((loaded, vec!["orders"])))
}

pub async fn tick<H: ExchangeSessionHost>(host: &mut H, count: f64) -> Result<(), H::Error> {
    if host.disposed()? || host.public_only()? {
        return Ok(());
    }
    let result = async {
        if count % 32.0 == 0.0 || !host.position_modes_equal()? {
            host.copy_position_mode()?;
            let positions = host.fetch("fetchPositions").await?;
            host.update(vec![("positions", &positions)], None)?;
            let orders = host.fetch("fetchOrders").await?;
            host.update(vec![("orders", &orders)], None)?;
            if host.disposed()? {
                return Ok(true);
            }
        }
        let (balance, positions) = host.fetch_balance().await?;
        if host.disposed()? {
            return Ok(true);
        }
        let loaded = host.loaded()?;
        host.update(
            vec![("balance", &balance), ("positions", &positions)],
            Some((loaded, vec!["balance", "positions"])),
        )?;
        Ok(false)
    }
    .await;
    match result {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(error) => host.report_message(error)?,
    }
    if !host.disposed()? {
        let interval = host.tick_interval()?;
        host.schedule_tick(count + 1.0, interval)?;
    }
    Ok(())
}

pub fn listen_ohlcv<O, C, R, E>(
    options: O,
    callback: C,
    symbol: impl FnOnce(&O) -> Result<Vec<u16>, E>,
    configured: impl FnOnce(Vec<u16>) -> Result<bool, E>,
    update_metadata: impl FnOnce(Vec<u16>) -> Result<(), E>,
    listen: impl FnOnce(O, C) -> Result<R, E>,
    second_symbol: impl FnOnce(&O) -> Result<Vec<u16>, E>,
) -> Result<R, E> {
    if !configured(symbol(&options)?)? {
        if let Ok(symbol) = second_symbol(&options) {
            match update_metadata(symbol) {
                Ok(()) | Err(_) => {}
            }
        }
    }
    listen(options, callback)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SocketKind {
    Public,
    Futures,
    Private,
}
impl SocketKind {
    pub fn routing_key(self) -> &'static str {
        match self {
            Self::Public => "wsPublic",
            Self::Futures => "wsBusiness",
            Self::Private => "wsPrivate",
        }
    }
    pub fn base_key(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Futures => "frontendfutures",
            Self::Private => "private",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Futures => "futures",
            Self::Private => "private",
        }
    }
}
#[derive(Clone, Copy)]
pub enum SocketListener {
    Open,
    Message,
    Close,
    Error,
}

pub trait ConnectionHost {
    type Url;
    type Socket;
    type Timer;
    type Error;
    fn flag(&mut self, field: &'static str) -> Result<bool, Self::Error>;
    fn number(&mut self, field: &'static str) -> Result<f64, Self::Error>;
    fn set_flag(&mut self, field: &'static str, value: bool) -> Result<(), Self::Error>;
    fn set_number(&mut self, field: &'static str, value: f64) -> Result<(), Self::Error>;
    fn clear_queue(&mut self) -> Result<(), Self::Error>;
    fn fallback(&mut self, scope: &'static str) -> Result<Self::Url, Self::Error>;
    fn url_truthy(&self, url: &Self::Url) -> bool;
    fn base_url(&mut self, scope: &'static str, testnet: bool) -> Result<Self::Url, Self::Error>;
    fn proxy_info(&mut self, message: Vec<u16>) -> Result<(), Self::Error>;
    fn create(&mut self, url: &Self::Url) -> Result<Self::Socket, Self::Error>;
    fn assign_socket(&mut self, socket: Self::Socket) -> Result<(), Self::Error>;
    fn socket(&mut self) -> Result<Option<Self::Socket>, Self::Error>;
    fn add_listener(
        &mut self,
        socket: Option<Self::Socket>,
        event: &'static str,
        listener: SocketListener,
        kind: SocketKind,
    ) -> Result<(), Self::Error>;
    fn closed(&mut self, socket: Option<Self::Socket>) -> Result<bool, Self::Error>;
    fn close_socket(&mut self, socket: Option<Self::Socket>) -> Result<(), Self::Error>;
    fn ping_timer(&mut self) -> Result<Self::Timer, Self::Error>;
    fn clear_timer(&mut self, timer: Self::Timer) -> Result<(), Self::Error>;
    fn on_close(&mut self) -> Result<(), Self::Error>;
}

fn connect<H: ConnectionHost>(host: &mut H, kind: SocketKind) -> Result<(), H::Error> {
    if host.flag("isDisposed")? {
        return Ok(());
    }
    if kind == SocketKind::Public {
        host.clear_queue()?;
    }
    if host.number("connectionAttempts")? >= host.number("MAX_ATTEMPTS")?
        && host.number("lastMessageCount")? == 0.0
        && host.number("messageCount")? == 0.0
    {
        host.set_flag("useFallbackUrl", true)?;
        host.set_number("connectionAttempts", 0.0)?;
    } else if !host.flag("useFallbackUrl")? {
        let attempts = host.number("connectionAttempts")?;
        host.set_number("connectionAttempts", attempts + 1.0)?;
    }
    let count = host.number("messageCount")?;
    host.set_number("lastMessageCount", count)?;
    host.set_number("messageCount", 0.0)?;
    let testnet = host.flag("testnet")?;
    let fallback = host.fallback(kind.routing_key())?;
    let base = if host.flag("useFallbackUrl")? && host.url_truthy(&fallback) {
        None
    } else {
        Some(host.base_url(kind.base_key(), testnet)?)
    };
    if host.flag("useFallbackUrl")? && host.url_truthy(&fallback) {
        match host.proxy_info(
            format!("Using tealstreet bitunix proxy ({})", kind.label())
                .encode_utf16()
                .collect(),
        ) {
            Ok(()) | Err(_) => {}
        }
    }
    let socket = host.create(base.as_ref().unwrap_or(&fallback))?;
    host.assign_socket(socket)?;
    let events = if kind == SocketKind::Private {
        ["message", "open", "close", "error"]
    } else {
        ["open", "message", "close", "error"]
    };
    for event in events {
        let listener = match event {
            "open" => SocketListener::Open,
            "message" => SocketListener::Message,
            "close" => SocketListener::Close,
            _ => SocketListener::Error,
        };
        let socket = host.socket()?;
        host.add_listener(socket, event, listener, kind)?;
    }
    Ok(())
}
pub fn connect_public<H: ConnectionHost>(host: &mut H) -> Result<(), H::Error> {
    connect(host, SocketKind::Public)
}
pub fn connect_futures<H: ConnectionHost>(host: &mut H) -> Result<(), H::Error> {
    connect(host, SocketKind::Futures)
}
pub async fn connect_private<H: ConnectionHost>(host: &mut H) -> Result<(), H::Error> {
    connect(host, SocketKind::Private)
}

pub fn connection_close<H: ConnectionHost>(host: &mut H, kind: SocketKind) -> Result<(), H::Error> {
    let timer = host.ping_timer()?;
    host.clear_timer(timer)?;
    if kind == SocketKind::Public {
        host.clear_queue()?;
    }
    host.on_close()
}
pub fn connection_error<H: ConnectionHost>(host: &mut H, kind: SocketKind) -> Result<(), H::Error> {
    if host.socket()?.is_some() {
        let socket = host.socket()?;
        if !host.closed(socket)? {
            let socket = host.socket()?;
            match host.close_socket(socket) {
                Ok(()) | Err(_) => {}
            }
        }
    }
    connection_close(host, kind)
}

pub trait PrivateOpenHost: Sized {
    type Timer;
    type Login: Future<Output = Result<(), Self::Error>>;
    type Error;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn ping(&mut self) -> Result<(), Self::Error>;
    fn timer(&mut self) -> Result<Self::Timer, Self::Error>;
    fn timer_truthy(&self, timer: &Self::Timer) -> bool;
    fn clear(&mut self, timer: Self::Timer) -> Result<(), Self::Error>;
    fn begin_login(&mut self) -> Result<Self::Login, Self::Error>;
    fn spawn_login(
        &mut self,
        login: Result<Self::Login, Self::Error>,
        after: fn(&mut Self) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn schedule_ping(
        &mut self,
        delay: f64,
        continuation: fn(&mut Self) -> Result<(), Self::Error>,
    ) -> Result<Self::Timer, Self::Error>;
    fn assign_timer(&mut self, timer: Self::Timer) -> Result<(), Self::Error>;
}
pub fn private_on_open<H: PrivateOpenHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        host.ping()?;
        let timer = host.timer()?;
        if host.timer_truthy(&timer) {
            let timer = host.timer()?;
            host.clear(timer)?;
        }
        let login = host.begin_login();
        host.spawn_login(login, private_after_login::<H>)?;
    }
    Ok(())
}
pub fn private_after_login<H: PrivateOpenHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        host.ping()?;
        let timer = host.schedule_ping(15_000.0, private_after_login::<H>)?;
        host.assign_timer(timer)?;
    }
    Ok(())
}
