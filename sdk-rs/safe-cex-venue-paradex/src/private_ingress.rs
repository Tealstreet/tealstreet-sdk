//! Private websocket method bodies over original property and intrinsic capabilities.
use safe_cex_types::{MarginType, OrderStatus, PositionMode, PositionSide};

// ts: ParadexPrivateWebSocket.constructor
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateHandler {
    Ping,
    Pong,
    Orders,
    Positions,
    Balance,
}

// ts: ParadexPrivateWebSocket.constructor
pub struct HandlerBinding {
    pub channel: &'static str,
    pub handler: PrivateHandler,
}

// ts: ParadexPrivateWebSocket.constructor
pub fn initialize_private_socket<P, M, U, E>(
    parent: &P,
    base: impl FnOnce(&P) -> Result<(), E>,
    read_urls: impl FnOnce() -> Result<M, E>,
    read_testnet: impl FnOnce(&P) -> Result<bool, E>,
    url: impl FnOnce(M, bool) -> Result<U, E>,
    write_url: impl FnOnce(U) -> Result<(), E>,
    write_handlers: impl FnOnce(Vec<HandlerBinding>) -> Result<(), E>,
) -> Result<(), E> {
    base(parent)?;
    let urls = read_urls()?;
    write_url(url(urls, read_testnet(parent)?)?)?;
    write_handlers(vec![
        HandlerBinding {
            channel: "ping",
            handler: PrivateHandler::Ping,
        },
        HandlerBinding {
            channel: "pong",
            handler: PrivateHandler::Pong,
        },
        HandlerBinding {
            channel: "orders.ALL",
            handler: PrivateHandler::Orders,
        },
        HandlerBinding {
            channel: "positions",
            handler: PrivateHandler::Positions,
        },
        HandlerBinding {
            channel: "balance_events",
            handler: PrivateHandler::Balance,
        },
    ])
}

// ts: ParadexPrivateWebSocket.constructor
pub fn invoke_private_handler<A, E>(
    handler: PrivateHandler,
    data: &A,
    pong: impl FnOnce() -> Result<(), E>,
    order: impl FnOnce(&A) -> Result<(), E>,
    position: impl FnOnce(&A) -> Result<(), E>,
    balance: impl FnOnce(&A) -> Result<(), E>,
) -> Result<(), E> {
    match handler {
        PrivateHandler::Ping | PrivateHandler::Pong => pong(),
        PrivateHandler::Orders => order(data),
        PrivateHandler::Positions => position(data),
        PrivateHandler::Balance => balance(data),
    }
}

// ts: ParadexPrivateWebSocket#mapOrderStatus
pub enum PrivateStatus<P> {
    Status(OrderStatus),
    Inherited(P),
}

// ts: ParadexPrivateWebSocket#mapOrderStatus
pub fn map_private_status<P, E>(
    status: &[u16],
    prototype: impl FnOnce(&[u16]) -> Result<Option<(P, bool)>, E>,
) -> Result<PrivateStatus<P>, E> {
    let matches = |s: &str| status.iter().copied().eq(s.encode_utf16());
    let mapped = if ["NEW", "OPEN", "PENDING", "UNTRIGGERED"]
        .iter()
        .any(|s| matches(s))
    {
        Some(OrderStatus::Open)
    } else if matches("FILLED") {
        Some(OrderStatus::Closed)
    } else if matches("CANCELED") || matches("REJECTED") {
        Some(OrderStatus::Canceled)
    } else {
        None
    };
    if let Some(status) = mapped {
        return Ok(PrivateStatus::Status(status));
    }
    if let Some((value, true)) = prototype(status)? {
        return Ok(PrivateStatus::Inherited(value));
    }
    Ok(PrivateStatus::Status(OrderStatus::Open))
}

// ts: ParadexPrivateWebSocket#formatCancelReason
pub enum CancelReason<P> {
    Text(Vec<u16>),
    Inherited(P),
}

// ts: ParadexPrivateWebSocket#formatCancelReason
pub fn format_cancel_reason<P, E>(
    reason: &[u16],
    prototype: impl FnOnce(&[u16]) -> Result<Option<(P, bool)>, E>,
    lowercase: impl FnOnce(Vec<u16>) -> Result<Vec<u16>, E>,
) -> Result<CancelReason<P>, E> {
    let mapping = [
        ("ORDER_SIZE_BELOW_MIN", "Order size below minimum required"),
        ("INSUFFICIENT_MARGIN", "Insufficient margin/balance"),
        ("PRICE_OUT_OF_RANGE", "Price outside allowed range"),
        ("MARKET_CLOSED", "Market is closed"),
        (
            "POST_ONLY_WOULD_CROSS",
            "Post-only order would execute immediately",
        ),
        ("INVALID_PRICE", "Invalid order price"),
        ("INVALID_SIZE", "Invalid order size"),
        ("MAX_OPEN_ORDERS", "Maximum open orders reached"),
        ("SELF_TRADE_PREVENTION", "Self-trade prevention triggered"),
        ("USER_CANCELED", "Canceled by user"),
    ];
    if let Some((_, message)) = mapping
        .iter()
        .find(|(key, _)| reason.iter().copied().eq(key.encode_utf16()))
    {
        return Ok(CancelReason::Text(message.encode_utf16().collect()));
    }
    if let Some((value, true)) = prototype(reason)? {
        return Ok(CancelReason::Inherited(value));
    }
    let replaced = reason
        .iter()
        .map(|unit| if *unit == 95 { 32 } else { *unit })
        .collect();
    Ok(CancelReason::Text(lowercase(replaced)?))
}

// ts: ParadexPrivateWebSocket#throttledFetchPositions
pub fn throttled_fetch_positions<'a, E: 'a>(
    read_now: impl FnOnce() -> Result<f64, E>,
    read_disposed: impl Fn() -> Result<bool, E> + 'a,
    read_scheduled: impl FnOnce() -> Result<bool, E>,
    read_last: impl FnOnce() -> Result<f64, E>,
    mut read_period: impl FnMut() -> Result<f64, E>,
    write_scheduled: impl Fn(bool) -> Result<(), E> + 'a,
    read_pending: impl FnOnce() -> Result<bool, E> + 'a,
    execute: impl FnOnce() -> Result<(), E> + 'a,
    schedule: impl FnOnce(Box<dyn FnOnce() -> Result<(), E> + 'a>, f64) -> Result<(), E>,
) -> Result<(), E> {
    let now = read_now()?;
    if read_disposed()? {
        return Ok(());
    }
    if read_scheduled()? {
        return Ok(());
    }
    let elapsed = now - read_last()?;
    if elapsed >= read_period()? {
        execute()
    } else {
        write_scheduled(true)?;
        let callback = Box::new(move || {
            write_scheduled(false)?;
            if !read_disposed()? && read_pending()? {
                execute()?;
            }
            Ok(())
        });
        schedule(callback, read_period()? - elapsed)
    }
}

// ts: ParadexPrivateWebSocket#handleOrderUpdate
pub struct PrivateOrder<V> {
    pub id: V,
    pub symbol: V,
    pub side: V,
    pub order_type: V,
    pub status: V,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub reduce_only: V,
    pub time_in_force: V,
    pub last_update: f64,
    pub info: V,
}

// ts: ParadexPrivateWebSocket#handlePositionUpdate
pub struct PrivatePosition<V> {
    pub id: V,
    pub symbol: V,
    pub side: PositionSide,
    pub contracts: f64,
    pub notional: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub leverage: f64,
    pub entry_price: f64,
    pub liquidation_price: f64,
    pub margin_type: MarginType,
    pub position_mode: PositionMode,
    pub last_update: V,
    pub info: V,
}

// ts: ParadexPrivateWebSocket#handleBalanceUpdate
pub struct PrivateBalance {
    pub total: f64,
    pub free: f64,
    pub used: f64,
    pub upnl: f64,
}

// ts: ParadexPrivateWebSocket.onMessage; ParadexPrivateWebSocket#handleOrderUpdate; ParadexPrivateWebSocket#handlePositionUpdate; ParadexPrivateWebSocket#handleBalanceUpdate
pub trait PrivateIngressHost {
    type Value: Clone;
    type Error;
    type Log;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn object(&self, value: &Self::Value) -> bool;
    fn array(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn boolean(&self, value: &Self::Value) -> Option<bool>;
    fn literal(&self, text: &str) -> Self::Value;
    fn boolean_value(&self, value: bool) -> Self::Value;
    fn number_template(&self, value: f64) -> Result<Vec<u16>, Self::Error>;
    fn equals(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn parse_float(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn string(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn template(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn select_log(&self) -> Result<Self::Log, Self::Error>;
    fn log(
        &self,
        bound: Self::Log,
        text: Vec<u16>,
        severity: &'static str,
    ) -> Result<(), Self::Error>;
    fn log_caught(&self, prefix: &'static str, error: &Self::Error) -> Result<(), Self::Error>;
    fn disposed(&self) -> Result<bool, Self::Error>;
    fn track_received(&self) -> Result<(), Self::Error>;
    fn parse_event(&self) -> Result<Self::Value, Self::Error>;
    fn handlers(&self, topic: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn stats(&self, topic: &Self::Value) -> Result<(), Self::Error>;
    fn for_each_handler(
        &self,
        handlers: &Self::Value,
        callback: impl FnMut(&Self::Value) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn call_handler(&self, handler: &Self::Value, data: &Self::Value) -> Result<(), Self::Error>;
    fn auth_id(&self) -> Result<Self::Value, Self::Error>;
    fn authenticated(&self) -> Result<bool, Self::Error>;
    fn write_authenticated(&self, value: bool) -> Result<(), Self::Error>;
    fn connected(&self) -> Result<bool, Self::Error>;
    fn close_socket(&self) -> Result<(), Self::Error>;
    fn subscribe_private(&self) -> Result<(), Self::Error>;
    fn side_lookup(&self, side: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn map_status(&self, order: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn flags_include_reduce_only(&self, flags: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn reverse_type(
        &self,
        order: &Self::Value,
        reduce_only: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn is_trigger_type(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn now(&self) -> Result<f64, Self::Error>;
    fn format_reason(&self, order: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn emit_rejection(&self, message: Vec<u16>) -> Result<(), Self::Error>;
    fn store_order(&self, update: PrivateOrder<Self::Value>) -> Result<(), Self::Error>;
    fn refresh_orders(&self) -> Result<(), Self::Error>;
    fn write_pending_positions(&self, value: bool) -> Result<(), Self::Error>;
    fn position_id(
        &self,
        symbol: &Self::Value,
        side: PositionSide,
        mode: PositionMode,
    ) -> Result<Self::Value, Self::Error>;
    fn store_position(&self, update: PrivatePosition<Self::Value>) -> Result<(), Self::Error>;
    fn refresh_positions(&self) -> Result<(), Self::Error>;
    fn find_object(
        &self,
        array: &Self::Value,
        predicate: impl FnMut(&Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn stringify_data(&self, data: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn positions_length(&self) -> Result<f64, Self::Error>;
    fn store_balance(&self, balance: PrivateBalance) -> Result<(), Self::Error>;
}
fn is<H: PrivateIngressHost>(h: &H, v: &H::Value, s: &str) -> bool {
    h.equals(v, &h.literal(s))
}
fn append<H: PrivateIngressHost>(
    h: &H,
    out: &mut Vec<u16>,
    value: &H::Value,
) -> Result<(), H::Error> {
    out.extend(h.template(value)?);
    Ok(())
}
fn text(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn log_literal<H: PrivateIngressHost>(
    h: &H,
    message: &str,
    severity: &'static str,
) -> Result<(), H::Error> {
    h.log(h.select_log()?, text(message), severity)
}

// ts: ParadexPrivateWebSocket.onMessage
pub fn on_private_message<H: PrivateIngressHost>(h: &H) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    h.track_received()?;
    let data = h.parse_event()?;
    if !h.truthy(&data) {
        return Ok(());
    }
    if is(h, &h.get(&data, "jsonrpc")?, "2.0") {
        if is(h, &h.get(&data, "method")?, "subscription") && h.truthy(&h.get(&data, "params")?) {
            let channel = h.get(&h.get(&data, "params")?, "channel")?;
            let handlers = h.handlers(&channel)?;
            if h.truthy(&handlers) {
                h.stats(&channel)?;
                h.for_each_handler(&handlers, |handler| {
                    h.call_handler(handler, &h.get(&data, "params")?)
                })?;
            } else {
                let log = h.select_log()?;
                let mut msg = text("No handler registered for channel: ");
                append(h, &mut msg, &channel)?;
                h.log(log, msg, "warning")?;
            }
        }
        if !h.undefined(&h.get(&data, "result")?) {
            if !h.undefined(&h.auth_id()?)
                && h.equals(&h.get(&data, "id")?, &h.auth_id()?)
                && !h.authenticated()?
            {
                h.write_authenticated(true)?;
                log_literal(h, "Paradex private WebSocket authenticated", "info")?;
                if !h.connected()? {
                    log_literal(
                        h,
                        "WebSocket disconnected after authentication, closing connection",
                        "warning",
                    )?;
                    h.close_socket()?;
                    return Ok(());
                }
                h.subscribe_private()?;
            }
            if h.truthy(&h.get(&h.get(&data, "result")?, "channel")?) {
                let log = h.select_log()?;
                let mut msg = text("Successfully subscribed to channel: ");
                append(h, &mut msg, &h.get(&h.get(&data, "result")?, "channel")?)?;
                h.log(log, msg, "info")?;
            }
        }
        if h.truthy(&h.get(&data, "error")?) {
            let log = h.select_log()?;
            let mut msg = text("WebSocket error for request ");
            append(h, &mut msg, &h.get(&data, "id")?)?;
            msg.extend(text(": "));
            append(h, &mut msg, &h.get(&h.get(&data, "error")?, "message")?)?;
            h.log(log, msg, "error")?;
            if !h.undefined(&h.auth_id()?) && h.equals(&h.get(&data, "id")?, &h.auth_id()?) {
                h.write_authenticated(false)?;
            }
        }
    } else {
        let kind = h.get(&data, "type")?;
        let topic = if h.truthy(&kind) {
            kind
        } else {
            h.get(&data, "channel")?
        };
        let handlers = h.handlers(&topic)?;
        if h.truthy(&handlers) {
            h.stats(&topic)?;
            h.for_each_handler(&handlers, |handler| h.call_handler(handler, &data))?;
        }
    }
    Ok(())
}

// ts: ParadexPrivateWebSocket#handleOrderUpdate
pub fn handle_order_update<H: PrivateIngressHost>(h: &H, data: &H::Value) -> Result<(), H::Error> {
    let result = (|| {
        let order = h.get(data, "data")?;
        let side = h.side_lookup(&h.get(&order, "side")?)?;
        let status = h.map_status(&order)?;
        let candidate = h.get(&order, "reduce_only")?;
        let reduce_only = if h.boolean(&candidate).is_some() {
            h.get(&order, "reduce_only")?
        } else if h.array(&h.get(&order, "flags")?)? {
            h.flags_include_reduce_only(&h.get(&order, "flags")?)?
        } else {
            h.boolean_value(false)
        };
        let order_type = h.reverse_type(&order, &reduce_only)?;
        let stop = h.is_trigger_type(&h.get(&order, "type")?)?;
        let price = if stop && h.truthy(&h.get(&order, "trigger_price")?) {
            h.parse_float(&h.get(&order, "trigger_price")?)?
        } else if h.truthy(&h.get(&order, "price")?) {
            h.parse_float(&h.get(&order, "price")?)?
        } else {
            0.0
        };
        let update = PrivateOrder {
            id: h.get(&order, "id")?,
            symbol: h.get(&order, "market")?,
            side,
            order_type,
            status: status.clone(),
            price,
            amount: h.parse_float(&h.get(&order, "size")?)?,
            filled: h.parse_float(&h.get(&order, "size")?)?
                - h.parse_float(&h.get(&order, "remaining_size")?)?,
            remaining: h.parse_float(&h.get(&order, "remaining_size")?)?,
            reduce_only,
            time_in_force: h.get(&order, "time_in_force")?,
            last_update: h.now()?,
            info: order.clone(),
        };
        let reason = h.get(&order, "cancel_reason")?;
        if h.truthy(&reason) && !is(h, &h.get(&order, "cancel_reason")?, "") {
            let formatted = h.format_reason(&order)?;
            let mut rejection = text("Order ");
            append(h, &mut rejection, &h.get(&order, "market")?)?;
            rejection.extend(text(" "));
            append(h, &mut rejection, &h.get(&order, "side")?)?;
            rejection.extend(text(" rejected: "));
            append(h, &mut rejection, &formatted)?;
            h.emit_rejection(rejection)?;
            let log = h.select_log()?;
            let mut msg = text("Order ");
            append(h, &mut msg, &h.get(&order, "id")?)?;
            msg.extend(text(" cancel reason: "));
            append(h, &mut msg, &h.get(&order, "cancel_reason")?)?;
            h.log(log, msg, "warning")?;
        }
        h.store_order(update)?;
        let log = h.select_log()?;
        let mut msg = text("Order ");
        append(h, &mut msg, &h.get(&order, "id")?)?;
        msg.extend(text(" status updated to "));
        append(h, &mut msg, &status)?;
        msg.extend(text(" (raw: "));
        append(h, &mut msg, &h.get(&order, "status")?)?;
        msg.extend(text(")"));
        h.log(log, msg, "info")?;
        h.refresh_orders()
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => h.log_caught("Error handling order update: ", &error),
    }
}

// ts: ParadexPrivateWebSocket#handlePositionUpdate
pub fn handle_position_update<H: PrivateIngressHost>(
    h: &H,
    data: &H::Value,
) -> Result<(), H::Error> {
    let result = (|| {
        let position = h.get(data, "data")?;
        if !h.truthy(&position) || !h.truthy(&h.get(&position, "market")?) {
            log_literal(
                h,
                "Invalid position update - missing data or market",
                "warning",
            )?;
            return Ok(());
        }
        if is(h, &h.get(&position, "status")?, "CLOSED")
            || h.parse_float(&h.get(&position, "size")?)? == 0.0
        {
            let log = h.select_log()?;
            let mut msg = text("Position ");
            append(h, &mut msg, &h.get(&position, "market")?)?;
            msg.extend(text(" closed or zero size"));
            h.log(log, msg, "debug")?;
            return Ok(());
        }
        h.write_pending_positions(true)?;
        let entry = h.parse_float(&h.get(&position, "average_entry_price")?)?;
        let contracts = h.parse_float(&h.get(&position, "size")?)?.abs();
        let id_symbol = h.get(&position, "market")?;
        let id_side = if is(h, &h.get(&position, "side")?, "LONG") {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let id = h.position_id(&id_symbol, id_side, PositionMode::OneWay)?;
        let symbol = h.get(&position, "market")?;
        let side = if is(h, &h.get(&position, "side")?, "LONG") {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let zero = h.literal("0");
        let read_or_zero = |key: &str| -> Result<f64, H::Error> {
            let value = h.get(&position, key)?;
            h.parse_float(if h.truthy(&value) { &value } else { &zero })
        };
        let unrealized = h.parse_float(&h.get(&position, "unrealized_pnl")?)?;
        let realized = read_or_zero("realized_positional_pnl")?;
        let leverage = read_or_zero("leverage")?;
        let liquidation = read_or_zero("liquidation_price")?;
        let last = h.get(&position, "last_updated_at")?;
        let last = if h.truthy(&last) {
            last
        } else {
            h.get(&position, "created_at")?
        };
        let update = PrivatePosition {
            id,
            symbol,
            side,
            contracts,
            notional: contracts * entry,
            unrealized_pnl: unrealized,
            realized_pnl: realized,
            leverage,
            entry_price: entry,
            liquidation_price: liquidation,
            margin_type: MarginType::Cross,
            position_mode: PositionMode::OneWay,
            last_update: last,
            info: position,
        };
        let log = h.select_log()?;
        let mut msg = text("Position update: ");
        append(h, &mut msg, &update.symbol)?;
        msg.extend(text(if side == PositionSide::Long {
            " long "
        } else {
            " short "
        }));
        msg.extend(h.number_template(contracts)?);
        msg.extend(text(" @ "));
        msg.extend(h.number_template(entry)?);
        h.log(log, msg, "debug")?;
        h.store_position(update)?;
        h.refresh_positions()
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => h.log_caught("Error handling position update: ", &error),
    }
}

// ts: ParadexPrivateWebSocket#handleBalanceUpdate
pub fn handle_balance_update<H: PrivateIngressHost>(
    h: &H,
    data: &H::Value,
) -> Result<(), H::Error> {
    let result = (|| {
        let nested = if h.nullish(data) {
            None
        } else {
            Some(h.get(data, "data")?)
        };
        let payload = nested
            .filter(|v| !h.nullish(v))
            .unwrap_or_else(|| data.clone());
        let account = if h.array(&payload)? {
            h.find_object(&payload, |entry| Ok(h.truthy(entry) && h.object(entry)))?
        } else {
            let nested = if h.nullish(&payload) {
                None
            } else {
                Some(h.get(&payload, "data")?)
            };
            if nested.as_ref().is_some_and(|v| h.truthy(v)) && h.object(&h.get(&payload, "data")?) {
                h.get(&payload, "data")?
            } else {
                payload
            }
        };
        if !h.truthy(&account) || !h.object(&account) {
            let log = h.select_log()?;
            let mut msg = text("Invalid balance payload: ");
            msg.extend(h.stringify_data(data)?);
            h.log(log, msg, "warning")?;
            return Ok(());
        }
        let read_number = |names: &[&str]| -> Result<Option<f64>, H::Error> {
            let values = names
                .iter()
                .map(|key| h.get(&account, key))
                .collect::<Result<Vec<_>, _>>()?;
            for value in values {
                if h.nullish(&value) {
                    continue;
                }
                let parsed = h.parse_float(&h.string(&value)?)?;
                if parsed.is_finite() {
                    return Ok(Some(parsed));
                }
            }
            Ok(None)
        };
        let total = read_number(&[
            "total_collateral",
            "account_value",
            "equity",
            "margin_balance",
            "wallet_balance",
        ])?
        .unwrap_or(0.0);
        let free = read_number(&[
            "free_collateral",
            "free",
            "available_margin",
            "available_collateral",
        ])?
        .unwrap_or(0.0);
        let used = read_number(&[
            "initial_margin_requirement",
            "position_margin",
            "used_margin",
        ])?
        .unwrap_or_else(|| {
            let difference = total - free;
            if difference.is_nan() {
                f64::NAN
            } else {
                if difference > 0.0 { difference } else { 0.0 }
            }
        });
        let balance = PrivateBalance {
            total,
            free,
            used,
            upnl: 0.0,
        };
        let _positions_nonempty = h.positions_length()? > 0.0;
        h.store_balance(balance)
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => h.log_caught("Error handling balance update: ", &error),
    }
}
