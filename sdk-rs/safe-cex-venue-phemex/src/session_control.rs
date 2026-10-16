//! Exchange session control with opaque store payloads and platform effects.
use std::future::Future;
pub trait SessionHost {
    type Value;
    type Error;
    fn base_dispose(&mut self) -> Result<(), Self::Error>;
    fn socket_dispose(&mut self, socket: &'static str) -> Result<(), Self::Error>;
    fn connect_private(&mut self) -> Result<(), Self::Error>;
    fn set_setting(&mut self, key: &'static str, value: bool) -> Result<(), Self::Error>;
    fn initial_tick(&mut self) -> impl Future<Output = Result<(), Self::Error>>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn log(&mut self, text: &'static str) -> Result<(), Self::Error>;
    fn empty_array(&mut self) -> Result<Self::Value, Self::Error>;
    fn zero_balance(
        &mut self,
        fields: [(&'static str, f64); 4],
    ) -> Result<Self::Value, Self::Error>;
    fn loaded(&mut self) -> Result<Self::Value, Self::Error>;
    fn update(
        &mut self,
        fields: Vec<(&'static str, Self::Value)>,
        loaded: Self::Value,
        flags: Vec<&'static str>,
    ) -> Result<(), Self::Error>;
    fn fetch_positions_balance(
        &mut self,
    ) -> impl Future<Output = Result<(Self::Value, Self::Value), Self::Error>>;
    fn add_positions(&mut self, positions: Self::Value) -> Result<(), Self::Error>;
    fn report_message(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
    fn tick_interval(&mut self) -> Result<Self::Value, Self::Error>;
    fn schedule_tick(
        &mut self,
        interval: Self::Value,
        method: &'static str,
    ) -> Result<(), Self::Error>;
}
pub fn dispose<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    host.base_dispose()?;
    host.socket_dispose("publicWebsocket")?;
    host.socket_dispose("privateWebsocket")
}
pub async fn start_private<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    host.connect_private()?;
    host.set_setting("isHedged", true)?;
    host.initial_tick().await?;
    if host.disposed()? {
        return Ok(());
    }
    host.log("Ready to trade on Phemex")
}
pub async fn start_public_only<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    if host.disposed()? {
        return Ok(());
    }
    host.log("Ready to watch public data on Binance")?;
    let orders = host.empty_array()?;
    let balance =
        host.zero_balance([("used", 0.0), ("free", 0.0), ("total", 0.0), ("upnl", 0.0)])?;
    let positions = host.empty_array()?;
    let loaded = host.loaded()?;
    host.update(
        vec![
            ("orders", orders),
            ("balance", balance),
            ("positions", positions),
        ],
        loaded,
        vec!["orders", "balance", "positions"],
    )
}
pub async fn tick<H: SessionHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        let result = async {
            let (balance, positions) = host.fetch_positions_balance().await?;
            if host.disposed()? {
                return Ok(true);
            }
            host.add_positions(positions)?;
            let loaded = host.loaded()?;
            host.update(
                vec![("balance", balance)],
                loaded,
                vec!["balance", "positions"],
            )?;
            Ok(false)
        }
        .await;
        match result {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => host.report_message(&error)?,
        }
        if !host.disposed()? {
            let interval = host.tick_interval()?;
            host.schedule_tick(interval, "tick")?;
        }
    }
    Ok(())
}
pub trait ListenerHost {
    type Options;
    type Symbol;
    type Callback;
    type Socket;
    type Handler;
    type Result;
    type Error;
    fn public_socket(&mut self) -> Result<Self::Socket, Self::Error>;
    fn handler(
        &mut self,
        socket: &Self::Socket,
        member: &'static str,
    ) -> Result<Self::Handler, Self::Error>;
    fn spread_options(&mut self, options: &Self::Options) -> Result<Self::Options, Self::Error>;
    fn symbol(&mut self, options: &Self::Options) -> Result<Self::Symbol, Self::Error>;
    fn format_symbol(&mut self, symbol: Self::Symbol) -> Result<Self::Symbol, Self::Error>;
    fn assign_symbol(
        &mut self,
        options: &mut Self::Options,
        symbol: Self::Symbol,
    ) -> Result<(), Self::Error>;
    fn call_ohlcv(
        &mut self,
        socket: Self::Socket,
        handler: Self::Handler,
        options: Self::Options,
        callback: Self::Callback,
    ) -> Result<Self::Result, Self::Error>;
    fn call_book(
        &mut self,
        socket: Self::Socket,
        handler: Self::Handler,
        symbol: Self::Symbol,
        callback: Self::Callback,
    ) -> Result<Self::Result, Self::Error>;
}
pub fn listen_ohlcv<H: ListenerHost>(
    host: &mut H,
    options: H::Options,
    callback: H::Callback,
) -> Result<H::Result, H::Error> {
    let socket = host.public_socket()?;
    let handler = host.handler(&socket, "listenOHLCV")?;
    let mut spread = host.spread_options(&options)?;
    let symbol = host.symbol(&options)?;
    let symbol = host.format_symbol(symbol)?;
    host.assign_symbol(&mut spread, symbol)?;
    host.call_ohlcv(socket, handler, spread, callback)
}
pub fn listen_order_book<H: ListenerHost>(
    host: &mut H,
    symbol: H::Symbol,
    callback: H::Callback,
) -> Result<H::Result, H::Error> {
    let socket = host.public_socket()?;
    let handler = host.handler(&socket, "listenOrderBook")?;
    let symbol = host.format_symbol(symbol)?;
    host.call_book(socket, handler, symbol, callback)
}
