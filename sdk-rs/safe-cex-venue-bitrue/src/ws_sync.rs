//! Synchronous websocket bodies at native primitive, property and callee boundaries.
use safe_cex_types::{OrderSide, OrderStatus, OrderType};
use std::rc::Rc;

pub trait ParseHost {
    type Value: Clone;
    type Error;
    fn string(&self, value: &Self::Value) -> Option<Vec<u16>>;
    fn number(&self, value: &Self::Value) -> Option<f64>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn empty_string(&self) -> Self::Value;
    fn template(&self, value: Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn uppercase(&self, value: Vec<u16>) -> Result<Vec<u16>, Self::Error>;
    fn lookup(&self, table: &'static str, key: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn order_type(&self, value: OrderType) -> Self::Value;
    fn order_side(&self, value: OrderSide) -> Self::Value;
    fn order_status(&self, value: OrderStatus) -> Self::Value;
}
fn equal(value: &[u16], literal: &str) -> bool {
    value.iter().copied().eq(literal.encode_utf16())
}
fn truthy_template<T: ParseHost>(host: &T, value: T::Value) -> Result<Vec<u16>, T::Error> {
    let value = if host.truthy(&value) {
        value
    } else {
        host.empty_string()
    };
    host.template(value)
}
pub fn parse_order_type<T: ParseHost>(host: &T, raw: T::Value) -> Result<T::Value, T::Error> {
    let value = if let Some(value) = host.string(&raw) {
        host.uppercase(value)?
    } else {
        truthy_template(host, raw)?
    };
    if ["1", "LIMIT", "IOC", "FOK", "POST_ONLY"]
        .iter()
        .any(|v| equal(&value, v))
    {
        return Ok(host.order_type(OrderType::Limit));
    }
    if equal(&value, "2") || equal(&value, "MARKET") {
        return Ok(host.order_type(OrderType::Market));
    }
    if equal(&value, "3") || equal(&value, "STOP_MARKET") {
        return Ok(host.order_type(OrderType::StopLoss));
    }
    if equal(&value, "TAKE_PROFIT_MARKET") {
        return Ok(host.order_type(OrderType::TakeProfit));
    }
    let found = host.lookup("ORDER_TYPE", value)?;
    Ok(if host.truthy(&found) {
        found
    } else {
        host.order_type(OrderType::Limit)
    })
}
pub fn parse_order_side<T: ParseHost>(host: &T, raw: T::Value) -> Result<T::Value, T::Error> {
    if let Some(value) = host.number(&raw) {
        return Ok(host.order_side(if value == 2.0 {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        }));
    }
    let value = host.uppercase(truthy_template(host, raw)?)?;
    let found = host.lookup("ORDER_SIDE", value.clone())?;
    Ok(if host.truthy(&found) {
        found
    } else {
        host.order_side(if equal(&value, "2") {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        })
    })
}
pub fn parse_order_status<T: ParseHost>(
    host: &T,
    execution: T::Value,
    status: T::Value,
) -> Result<T::Value, T::Error> {
    let status = host.uppercase(truthy_template(host, status)?)?;
    let found = host.lookup("ORDER_STATUS", status.clone())?;
    if host.truthy(&found) {
        return Ok(found);
    }
    let execution = host.uppercase(truthy_template(host, execution)?)?;
    if equal(&execution, "CANCEL") {
        return Ok(host.order_status(OrderStatus::Canceled));
    }
    if equal(&execution, "TRADE") && equal(&status, "FILLED") {
        return Ok(host.order_status(OrderStatus::Closed));
    }
    Ok(host.order_status(OrderStatus::Open))
}

pub trait SymbolHost: ParseHost {
    fn direct_market(&self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn replace_prefix(&self, symbol: Self::Value) -> Result<Self::Value, Self::Error>;
    fn replace_separators(&self, symbol: Self::Value) -> Result<Self::Value, Self::Error>;
    fn uppercase_value(&self, symbol: Self::Value) -> Result<Self::Value, Self::Error>;
    fn find_market<F: FnMut(Self::Value) -> Result<bool, Self::Error>>(
        &self,
        predicate: F,
    ) -> Result<Self::Value, Self::Error>;
    fn market_symbol(&self, market: Self::Value) -> Result<Self::Value, Self::Error>;
    fn optional_market_symbol(&self, market: Self::Value) -> Result<Self::Value, Self::Error>;
    fn strictly_equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
}
fn normalize_symbol<T: SymbolHost>(host: &T, value: T::Value) -> Result<T::Value, T::Error> {
    let value = host.replace_prefix(value)?;
    let value = host.replace_separators(value)?;
    host.uppercase_value(value)
}
pub fn resolve_market_symbol<T: SymbolHost>(
    host: &T,
    symbol: T::Value,
) -> Result<T::Value, T::Error> {
    let direct = host.direct_market(&symbol)?;
    if host.truthy(&direct) {
        return Ok(symbol);
    }
    let normalized = normalize_symbol(host, symbol.clone())?;
    let found = host.find_market(|market| {
        let value = host.market_symbol(market)?;
        let candidate = normalize_symbol(host, value)?;
        Ok(host.strictly_equal(&candidate, &normalized))
    })?;
    let found = host.optional_market_symbol(found)?;
    Ok(if host.truthy(&found) { found } else { symbol })
}

#[derive(Clone, Debug)]
pub enum SyncFrame<V> {
    Pong(V),
    EventPong,
    PrivateSubscription(V),
}
pub trait PingHost {
    type Value: Clone;
    type Error;
    type Sender;
    type Stringifier;
    type Return;
    fn now(&self) -> Result<f64, Self::Error>;
    fn set_ping_at(&self, value: f64) -> Result<(), Self::Error>;
    fn ping(&self, json: Self::Value) -> Result<Self::Value, Self::Error>;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn prepare_send(&self) -> Result<Self::Sender, Self::Error>;
    fn prepare_stringify(&self) -> Result<Self::Stringifier, Self::Error>;
    fn stringify(
        &self,
        prepared: Self::Stringifier,
        frame: SyncFrame<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn pong_text(&self) -> Self::Value;
    fn send(&self, sender: Self::Sender, value: Self::Value) -> Result<Self::Return, Self::Error>;
    fn handle_pong(&self, delay: f64) -> Result<Self::Return, Self::Error>;
}
pub fn handle_private_ping<T: PingHost>(host: &T, json: T::Value) -> Result<(), T::Error> {
    let now = host.now()?;
    host.set_ping_at(now)?;
    let ping = host.ping(json)?;
    let frame = if host.undefined(&ping) {
        SyncFrame::EventPong
    } else {
        SyncFrame::Pong(ping)
    };
    let send = host.prepare_send()?;
    let stringify = host.prepare_stringify()?;
    let text = host.stringify(stringify, frame)?;
    host.send(send, text)?;
    host.handle_pong(20_000.0)?;
    Ok(())
}
pub fn handle_public_ping<T: PingHost>(host: &T, json: T::Value) -> Result<(), T::Error> {
    let now = host.now()?;
    host.set_ping_at(now)?;
    let send = host.prepare_send()?;
    host.send(send, host.pong_text())?;
    let ping = host.ping(json.clone())?;
    if !host.undefined(&ping) {
        let send = host.prepare_send()?;
        let stringify = host.prepare_stringify()?;
        let ping = host.ping(json)?;
        let text = host.stringify(stringify, SyncFrame::Pong(ping))?;
        host.send(send, text)?;
    }
    host.handle_pong(20_000.0)?;
    Ok(())
}
pub trait SubscriptionHost: PingHost {
    fn channels_for_each<F: FnMut(Self::Value) -> Result<(), Self::Error>>(
        &self,
        channels: &[&str],
        callback: F,
    ) -> Result<Self::Return, Self::Error>;
    fn prepare_send_with_log(&self) -> Result<Self::Sender, Self::Error>;
    fn send_with_log(
        &self,
        sender: Self::Sender,
        text: Self::Value,
    ) -> Result<Self::Return, Self::Error>;
}
pub fn subscribe_private<T: SubscriptionHost>(host: &T) -> Result<(), T::Error> {
    host.channels_for_each(
        &[
            "user_account_update",
            "user_order_update",
            "user_balance_update",
        ],
        |channel| {
            let send = host.prepare_send_with_log()?;
            let stringify = host.prepare_stringify()?;
            let text = host.stringify(stringify, SyncFrame::PrivateSubscription(channel))?;
            host.send_with_log(send, text)?;
            Ok(())
        },
    )?;
    Ok(())
}

pub trait RefreshTimerHost {
    type Clearer;
    type Value: Clone;
    type Error;
    type Return;
    fn timeout(&self) -> Result<Self::Value, Self::Error>;
    fn truthy_timeout(&self, timeout: &Self::Value) -> bool;
    fn prepare_clear_timeout(&self) -> Result<Self::Clearer, Self::Error>;
    fn clear_timeout(
        &self,
        prepared: Self::Clearer,
        timeout: Self::Value,
    ) -> Result<(), Self::Error>;
    fn set_timeout<F: FnMut() -> Result<Self::Return, Self::Error> + 'static>(
        &self,
        callback: F,
        delay: f64,
    ) -> Result<Self::Value, Self::Error>;
    fn assign_timeout(&self, timeout: Self::Value) -> Result<(), Self::Error>;
    fn update_listen_key(&self, key: Self::Value) -> Result<Self::Return, Self::Error>;
    fn undefined_timeout(&self) -> Self::Value;
    fn base_dispose(&self) -> Result<Self::Return, Self::Error>;
}
pub fn schedule_listen_key_refresh<T: RefreshTimerHost + 'static>(
    host: Rc<T>,
    key: T::Value,
) -> Result<(), T::Error>
where
    T::Value: 'static,
{
    let timeout = host.timeout()?;
    if host.truthy_timeout(&timeout) {
        let clear = host.prepare_clear_timeout()?;
        host.clear_timeout(clear, host.timeout()?)?;
    }
    let callback_host = host.clone();
    let timeout = host.set_timeout(
        move || callback_host.update_listen_key(key.clone()),
        30.0 * 60.0 * 1000.0,
    )?;
    host.assign_timeout(timeout)?;
    Ok(())
}
pub fn dispose_private<T: RefreshTimerHost>(host: &T) -> Result<(), T::Error> {
    let timeout = host.timeout()?;
    if host.truthy_timeout(&timeout) {
        let clear = host.prepare_clear_timeout()?;
        host.clear_timeout(clear, host.timeout()?)?;
        host.assign_timeout(host.undefined_timeout())?;
    }
    host.base_dispose()?;
    Ok(())
}
