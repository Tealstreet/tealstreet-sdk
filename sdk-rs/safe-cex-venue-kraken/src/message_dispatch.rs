//! Websocket control bodies with bound JavaScript values, parsing and side effects.
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use std::rc::Rc;

pub trait MessageHost {
    type Value: ?Sized;
    fn disposed(&self) -> Result<bool, ExchangeError>;
    fn track_received(&self) -> Result<(), ExchangeError>;
    fn parse_data(
        &self,
        data: &Self::Value,
        tolerant: bool,
    ) -> Result<Rc<Self::Value>, ExchangeError>;
    fn field(
        &self,
        value: &Self::Value,
        key: &'static str,
        optional: bool,
    ) -> Result<Rc<Self::Value>, ExchangeError>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn equals(&self, value: &Self::Value, text: &'static str) -> bool;
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, ExchangeError>;
    fn stringify(&self, value: &Self::Value) -> Result<Vec<u16>, ExchangeError>;
    fn includes(&self, value: &Self::Value, text: &'static str) -> Result<bool, ExchangeError>;
    fn stats(&self, feed: Rc<Self::Value>) -> Result<(), ExchangeError>;
    fn invoke(&self, method: &'static str, message: Rc<Self::Value>) -> Result<(), ExchangeError>;
    fn perform_auth(&self) -> Result<(), ExchangeError>;
    fn pending_challenge(&self, value: Vec<u16>) -> Result<(), ExchangeError>;
    fn waiting_challenge(&self, value: bool) -> Result<(), ExchangeError>;
    fn authenticated(&self, value: bool) -> Result<(), ExchangeError>;
    fn clear_token(&self) -> Result<(), ExchangeError>;
    fn log(&self, message: Vec<u16>, severity: LogSeverity) -> Result<(), ExchangeError>;
    fn console_subscription(
        &self,
        prefix: &'static str,
        value: Rc<Self::Value>,
    ) -> Result<(), ExchangeError>;
    fn console_parse(
        &self,
        prefix: &'static str,
        error: ExchangeError,
        data: Rc<Self::Value>,
    ) -> Result<(), ExchangeError>;
}

fn prefixed(prefix: &str, suffix: Vec<u16>) -> Vec<u16> {
    prefix.encode_utf16().chain(suffix).collect()
}

pub fn futures_public<H: MessageHost>(host: &H, data: Rc<H::Value>) -> Result<(), ExchangeError> {
    if host.disposed()? {
        return Ok(());
    }
    host.track_received()?;
    let body = || -> Result<(), ExchangeError> {
        let json = host.parse_data(&data, false)?;
        let mut feed = host.field(&json, "feed", true)?;
        if !host.truthy(&feed) {
            feed = host.field(&json, "channel", true)?;
        }
        if host.truthy(&*host.field(&json, "event", true)?) {
            if host.equals(&*host.field(&json, "event", false)?, "error") {
                let message = host.field(&json, "message", false)?;
                host.console_subscription(
                    "[Kraken WS] Subscription error:",
                    if host.truthy(&message) {
                        message
                    } else {
                        json.clone()
                    },
                )?;
            }
            return Ok(());
        }
        if !host.truthy(&feed) {
            return Ok(());
        }
        if !host.equals(&feed, "book_snapshot") && !host.equals(&feed, "book") {
            host.stats(feed.clone())?;
        }
        for (name, method) in [
            ("ticker", "handleTicker"),
            ("ohlc", "handleOHLC"),
            ("book_snapshot", "handleBookSnapshot"),
            ("book", "handleBookDelta"),
        ] {
            if host.equals(&feed, name) {
                host.invoke(method, json)?;
                break;
            }
        }
        Ok(())
    };
    if let Err(error) = body() {
        host.console_parse("kraken.ws-public parse error", error, data)?;
    }
    Ok(())
}

pub fn spot_public<H: MessageHost>(host: &H, data: Rc<H::Value>) -> Result<(), ExchangeError> {
    spot_message(host, data, false)
}

pub fn spot_private<H: MessageHost>(host: &H, data: Rc<H::Value>) -> Result<(), ExchangeError> {
    spot_message(host, data, true)
}

fn spot_message<H: MessageHost>(
    host: &H,
    data: Rc<H::Value>,
    private: bool,
) -> Result<(), ExchangeError> {
    if host.disposed()? {
        return Ok(());
    }
    host.track_received()?;
    let body = || -> Result<(), ExchangeError> {
        let json = host.parse_data(&data, false)?;
        if host.truthy(&*host.field(&json, "method", true)?) {
            if host.equals(&*host.field(&json, "method", false)?, "subscribe")
                && !host.truthy(&*host.field(&json, "success", false)?)
            {
                let error = host.field(&json, "error", false)?;
                if private {
                    host.log(
                        prefixed("WS subscription error: ", host.string(&error)?),
                        LogSeverity::Error,
                    )?;
                    let error = host.field(&json, "error", false)?;
                    if host.includes(&error, "token")? {
                        host.authenticated(false)?;
                        host.clear_token()?;
                    }
                } else {
                    host.console_subscription("[KrakenSpot WS] Subscription error:", error)?;
                }
            }
            return Ok(());
        }
        if host.equals(&*host.field(&json, "channel", true)?, "heartbeat") {
            return Ok(());
        }
        let channel = host.field(&json, "channel", true)?;
        if !host.truthy(&channel) {
            return Ok(());
        }
        if !host.equals(&*host.field(&json, "type", false)?, "snapshot")
            && (private || (!host.equals(&channel, "book") && !host.equals(&channel, "ohlc")))
        {
            host.stats(channel.clone())?;
        }
        let methods: &[(&str, &str)] = if private {
            &[
                ("executions", "handleExecutions"),
                ("balances", "handleBalances"),
            ]
        } else {
            &[
                ("ticker", "handleTicker"),
                ("book", "handleBook"),
                ("ohlc", "handleOHLC"),
            ]
        };
        for &(name, method) in methods {
            if host.equals(&channel, name) {
                host.invoke(method, json)?;
                break;
            }
        }
        Ok(())
    };
    if let Err(error) = body() {
        host.console_parse(
            if private {
                "krakenspot.ws-private parse error"
            } else {
                "krakenspot.ws-public parse error"
            },
            error,
            data,
        )?;
    }
    Ok(())
}

pub fn futures_private<H: MessageHost>(host: &H, data: Rc<H::Value>) -> Result<(), ExchangeError> {
    if host.disposed()? {
        return Ok(());
    }
    host.track_received()?;
    let msg = host.parse_data(&data, true)?;
    let mut feed = host.field(&msg, "feed", true)?;
    for key in ["channel", "event"] {
        if !host.truthy(&feed) {
            feed = host.field(&msg, key, true)?;
        }
    }
    if !host.truthy(&feed) {
        return Ok(());
    }
    if host.equals(&feed, "challenge") && host.truthy(&*host.field(&msg, "message", true)?) {
        host.pending_challenge(host.string(&*host.field(&msg, "message", false)?)?)?;
        host.waiting_challenge(false)?;
        host.perform_auth()?;
        return Ok(());
    }
    let auth_error = if host.equals(&feed, "error") {
        true
    } else if host.equals(&feed, "info") {
        host.includes(&*host.field(&msg, "message", true)?, "authentication")?
    } else {
        false
    };
    if auth_error {
        let message = host.field(&msg, "message", true)?;
        let text = if host.truthy(&message) {
            host.string(&message)?
        } else {
            host.stringify(&msg)?
        };
        host.log(
            prefixed("WebSocket authentication error: ", text),
            LogSeverity::Error,
        )?;
        host.authenticated(false)?;
        return Ok(());
    }
    if host.equals(&feed, "subscribed") && host.truthy(&*host.field(&msg, "feed", true)?) {
        return Ok(());
    }
    for (name, method) in [
        ("fills", "handleFills"),
        ("open_orders", "handleOpenOrders"),
        ("open_orders_snapshot", "handleOpenOrders"),
        ("open_positions", "handleOpenPositions"),
        ("open_positions_snapshot", "handleOpenPositions"),
        ("account_balances", "handleBalances"),
        ("account_balances_snapshot", "handleBalances"),
        ("balances", "handleBalances"),
    ] {
        if host.equals(&feed, name) {
            host.invoke(method, msg)?;
            break;
        }
    }
    Ok(())
}
