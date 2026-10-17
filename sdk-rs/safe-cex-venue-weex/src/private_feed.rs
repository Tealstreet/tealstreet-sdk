//! Private websocket transport, dispatch, native updates and refresh callbacks.
use safe_cex_types::OrderStatus;
use std::{future::Future, pin::Pin};

type RefreshFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeexRefreshKind {
    Orders,
    Positions,
    Balance,
}
#[derive(Debug)]
pub struct WeexPrivateFill<V> {
    pub id: Vec<u16>,
    pub order_id: Vec<u16>,
    pub side: V,
    pub symbol: V,
    pub price: f64,
    pub amount: f64,
    pub timestamp: f64,
    pub fee: f64,
    pub realized_pnl: f64,
}

pub struct WeexPrivateFeedRuntime<H, V, W, T, E> {
    pub disposed: fn(&H) -> Result<bool, E>,
    pub supported_runtime: fn(&H) -> Result<bool, E>,
    pub credential: fn(&H, &str) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub undefined: fn(&V) -> bool,
    pub string_equals: fn(&V, &str) -> bool,
    pub is_false: fn(&V) -> bool,
    pub make_string: fn(&H, &str) -> Result<V, E>,
    pub template_string: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub stringify: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub property: fn(&H, &V, &str) -> Result<V, E>,
    pub is_array: fn(&H, &V) -> Result<bool, E>,
    pub make_string_units: fn(&H, Vec<u16>) -> Result<V, E>,
    pub make_undefined: fn(&H) -> Result<V, E>,
    pub uppercase: fn(&H, &[u16]) -> Result<Vec<u16>, E>,
    pub map_length: fn(&H, &V) -> Result<usize, E>,
    pub find_length: fn(&H, &V) -> Result<usize, E>,
    pub array_element: fn(&H, &V, usize) -> Result<Option<V>, E>,
    pub empty_array: fn(&H) -> Result<V, E>,
    pub iterator: fn(&H, &V) -> Result<V, E>,
    pub iterator_next: fn(&H, &V) -> Result<Option<V>, E>,
    pub iterator_close: fn(&H, &V) -> Result<(), E>,
    pub parse: fn(&H, &[u16]) -> Result<V, E>,
    pub number: fn(&H, &V) -> Result<f64, E>,
    pub timestamp_seconds: fn(&H, &V) -> Result<f64, E>,
    pub markets_by_symbol: fn(&H) -> Result<V, E>,
    pub market: fn(&H, &V, &V) -> Result<V, E>,
    pub normalize_order: fn(&H, &V, &V) -> Result<V, E>,
    pub normalize_position: fn(&H, &V, &V) -> Result<V, E>,
    pub stored_orders: fn(&H) -> Result<V, E>,
    pub strict_equal: fn(&V, &V) -> bool,
    pub add_orders: fn(&H, &mut Vec<V>) -> Result<(), E>,
    pub order_status: fn(&V) -> Option<OrderStatus>,
    pub update_status: fn(&H, V, V) -> Result<(), E>,
    pub add_positions: fn(&H, Vec<V>) -> Result<(), E>,
    pub emit_fill: fn(&H, WeexPrivateFill<V>) -> Result<(), E>,
    pub track_received: fn(&H) -> Result<(), E>,
    pub subscription_id: fn(&H) -> Result<f64, E>,
    pub set_subscription_id: fn(&H, f64) -> Result<(), E>,
    pub send_pong: fn(&H, f64) -> Result<(), E>,
    pub handle_pong: fn(&H, f64) -> Result<(), E>,
    pub warning: fn(&H, Vec<u16>) -> Result<(), E>,
    pub record_stats: fn(&H, &V) -> Result<(), E>,
    pub log_unsupported: fn(&H, Option<&str>) -> Result<(), E>,
    pub auth_headers: fn(&H) -> Result<V, E>,
    pub create_socket:
        fn(&H, &'static str, Option<&[&str]>, fn(&H) -> Result<V, E>, bool) -> Result<W, E>,
    pub set_socket: fn(&H, W) -> Result<(), E>,
    pub socket: fn(&H) -> Result<W, E>,
    pub add_listener: fn(&H, &W, &'static str) -> Result<(), E>,
    pub header_unavailable: fn(&E) -> bool,
    pub timer: fn(&H, WeexRefreshKind) -> Result<Option<T>, E>,
    pub clear_timeout: fn(&H, Option<&T>) -> Result<(), E>,
    /// Kind identifies the run_refresh_timer callback; registration retains the host context.
    pub set_timeout: fn(&H, WeexRefreshKind, u64) -> Result<T, E>,
    pub assign_timer: fn(&H, WeexRefreshKind, T) -> Result<(), E>,
    pub fetch_refresh: for<'a> fn(&'a H, WeexRefreshKind) -> Result<RefreshFuture<'a, V, E>, E>,
    pub apply_refresh: fn(&H, WeexRefreshKind, V) -> Result<(), E>,
    pub base_dispose: fn(&H) -> Result<(), E>,
}

pub fn connect_and_subscribe<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
) -> Result<(), E> {
    if (rt.disposed)(host)? {
        return Ok(());
    }
    if !(rt.supported_runtime)(host)? {
        (rt.log_unsupported)(host, None)?;
        return Ok(());
    }
    for key in ["key", "secret", "passphrase"] {
        if !(rt.truthy)(&(rt.credential)(host, key)?) {
            (rt.log_unsupported)(host, Some("credentials are incomplete"))?;
            return Ok(());
        }
    }
    let testnet = (rt.truthy)(&(rt.credential)(host, "testnet")?);
    let url = crate::types::BASE_WS_URL
        .iter()
        .find(|(key, _)| *key == "private")
        .unwrap()
        .1
        .iter()
        .find(|(key, _)| *key == if testnet { "testnet" } else { "livenet" })
        .unwrap()
        .1;
    let created = (|| {
        let socket = (rt.create_socket)(host, url, None, rt.auth_headers, true)?;
        (rt.set_socket)(host, socket)
    })();
    if let Err(error) = created {
        if (rt.header_unavailable)(&error) {
            (rt.log_unsupported)(
                host,
                Some("a header-capable Node/Bun WebSocket transport is unavailable"),
            )?;
            return Ok(());
        }
        return Err(error);
    }
    for event in ["open", "message", "close"] {
        let socket = (rt.socket)(host)?;
        (rt.add_listener)(host, &socket, event)?;
    }
    Ok(())
}

pub fn on_message<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
    data: &V,
) -> Result<(), E> {
    if (rt.disposed)(host)? {
        return Ok(());
    }
    (rt.track_received)(host)?;
    let json = (rt.parse)(host, &(rt.stringify)(host, data)?)?;
    if !(rt.truthy)(&json) {
        return Ok(());
    }
    if (rt.string_equals)(&(rt.property)(host, &json, "type")?, "ping") {
        let id = (rt.subscription_id)(host)?;
        (rt.set_subscription_id)(host, id + 1.0)?;
        (rt.send_pong)(host, id)?;
        (rt.handle_pong)(host, 15_000.0)?;
        return Ok(());
    }
    if !(rt.undefined)(&(rt.property)(host, &json, "result")?) {
        if (rt.is_false)(&(rt.property)(host, &json, "result")?) {
            let mut message = (rt.property)(host, &json, "msg")?;
            if !(rt.truthy)(&message) {
                message = (rt.make_string)(host, "unknown error")?;
            }
            let mut warning: Vec<u16> = "WEEX private WS subscription failed: "
                .encode_utf16()
                .collect();
            warning.extend((rt.template_string)(host, &message)?);
            (rt.warning)(host, warning)?;
        }
        return Ok(());
    }
    let event = (rt.property)(host, &json, "e")?;
    if !(rt.truthy)(&event) {
        return Ok(());
    }
    (rt.record_stats)(host, &event)?;
    if (rt.string_equals)(&event, "account") {
        throttled_fetch_balance(host, rt)
    } else if (rt.string_equals)(&event, "positions") {
        handle_positions(host, rt, &json)
    } else if (rt.string_equals)(&event, "orders") {
        handle_orders(host, rt, &json)
    } else if (rt.string_equals)(&event, "fill") {
        handle_fills(host, rt, &json)
    } else {
        Ok(())
    }
}

fn rows<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
    json: &V,
) -> Result<V, E> {
    let first = (rt.property)(host, json, "d")?;
    if !(rt.is_array)(host, &first)? {
        return (rt.empty_array)(host);
    }
    (rt.property)(host, json, "d")
}

pub fn handle_orders<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
    json: &V,
) -> Result<(), E> {
    let mut orders = Vec::new();
    let array = rows(host, rt, json)?;
    let length = (rt.map_length)(host, &array)?;
    for index in 0..length {
        let Some(row) = (rt.array_element)(host, &array, index)? else {
            continue;
        };
        let markets = (rt.markets_by_symbol)(host)?;
        let symbol = (rt.property)(host, &row, "symbol")?;
        let market = (rt.market)(host, &markets, &symbol)?;
        let order = (rt.normalize_order)(host, &row, &market)?;
        if (rt.truthy)(&order) {
            orders.push(order);
        }
    }
    if !orders.is_empty() {
        (rt.add_orders)(host, &mut orders)?;
        let mut closed = Vec::new();
        for order in &orders {
            let first = (rt.property)(host, order, "status")?;
            if (rt.order_status)(&first) == Some(OrderStatus::Closed)
                || (rt.order_status)(&(rt.property)(host, order, "status")?)
                    == Some(OrderStatus::Canceled)
            {
                closed.push(order);
            }
        }
        for order in closed {
            let id = (rt.property)(host, order, "id")?;
            let status = (rt.property)(host, order, "status")?;
            (rt.update_status)(host, id, status)?;
        }
    }
    throttled_fetch_orders(host, rt)
}

pub fn handle_positions<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
    json: &V,
) -> Result<(), E> {
    let mut positions = Vec::new();
    let array = rows(host, rt, json)?;
    let length = (rt.map_length)(host, &array)?;
    for index in 0..length {
        let Some(row) = (rt.array_element)(host, &array, index)? else {
            continue;
        };
        let markets = (rt.markets_by_symbol)(host)?;
        let symbol = (rt.property)(host, &row, "symbol")?;
        let market = (rt.market)(host, &markets, &symbol)?;
        let position = (rt.normalize_position)(host, &row, &market)?;
        if (rt.truthy)(&position) {
            positions.push(position);
        }
    }
    if !positions.is_empty() {
        (rt.add_positions)(host, positions)?;
    }
    throttled_fetch_positions(host, rt)
}

pub fn handle_fills<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
    json: &V,
) -> Result<(), E> {
    let array = rows(host, rt, json)?;
    let iterator = (rt.iterator)(host, &array)?;
    while let Some(fill) = (rt.iterator_next)(host, &iterator)? {
        let result = (|| {
            let value = (rt.number)(host, &(rt.property)(host, &fill, "fillValue")?)?;
            let price = if value != 0.0 && !value.is_nan() {
                let size = (rt.number)(host, &(rt.property)(host, &fill, "fillSize")?)?;
                if size != 0.0 && !size.is_nan() {
                    (rt.number)(host, &(rt.property)(host, &fill, "fillValue")?)?
                        / (rt.number)(host, &(rt.property)(host, &fill, "fillSize")?)?
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let amount = (rt.number)(host, &(rt.property)(host, &fill, "fillSize")?)?.abs();
            let mut matched = None;
            let orders = (rt.stored_orders)(host)?;
            let length = (rt.find_length)(host, &orders)?;
            for index in 0..length {
                let order = match (rt.array_element)(host, &orders, index)? {
                    Some(order) => order,
                    None => (rt.make_undefined)(host)?,
                };
                let id = (rt.property)(host, &order, "id")?;
                let wanted = (rt.stringify)(host, &(rt.property)(host, &fill, "orderId")?)?;
                let wanted = (rt.make_string_units)(host, wanted)?;
                if (rt.strict_equal)(&id, &wanted) {
                    matched = Some(order);
                    break;
                }
            }
            let mut side = match matched {
                Some(order) => (rt.property)(host, &order, "side")?,
                None => (rt.make_undefined)(host)?,
            };
            if !(rt.truthy)(&side) {
                let raw = (rt.stringify)(host, &(rt.property)(host, &fill, "orderSide")?)?;
                side = (rt.make_string)(
                    host,
                    if (rt.uppercase)(host, &raw)?
                        .iter()
                        .copied()
                        .eq("SELL".encode_utf16())
                    {
                        "sell"
                    } else {
                        "buy"
                    },
                )?;
            }
            if amount <= 0.0 {
                return Ok(());
            }
            let mut id = (rt.property)(host, &fill, "id")?;
            if !(rt.truthy)(&id) {
                id = (rt.property)(host, &fill, "orderId")?;
            }
            let id = (rt.stringify)(host, &id)?;
            let mut order_id = (rt.property)(host, &fill, "orderId")?;
            if !(rt.truthy)(&order_id) {
                order_id = (rt.make_string)(host, "")?;
            }
            let order_id = (rt.stringify)(host, &order_id)?;
            let symbol = (rt.property)(host, &fill, "symbol")?;
            let mut time = (rt.property)(host, &fill, "createdTime")?;
            if !(rt.truthy)(&time) {
                time = (rt.property)(host, json, "E")?;
            }
            let timestamp = (rt.timestamp_seconds)(host, &time)?;
            let fee = (rt.number)(host, &(rt.property)(host, &fill, "fillFee")?)?.abs();
            let realized_pnl = (rt.number)(host, &(rt.property)(host, &fill, "realizePnl")?)?;
            (rt.emit_fill)(
                host,
                WeexPrivateFill {
                    id,
                    order_id,
                    side,
                    symbol,
                    price,
                    amount,
                    timestamp,
                    fee,
                    realized_pnl,
                },
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            match (rt.iterator_close)(host, &iterator) {
                Ok(()) | Err(_) => return Err(error),
            }
        }
    }
    throttled_fetch_orders(host, rt)
}

fn throttle<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
    kind: WeexRefreshKind,
) -> Result<(), E> {
    let timer = (rt.timer)(host, kind)?;
    (rt.clear_timeout)(host, timer.as_ref())?;
    let timer = (rt.set_timeout)(host, kind, 250)?;
    (rt.assign_timer)(host, kind, timer)
}
pub fn throttled_fetch_orders<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
) -> Result<(), E> {
    throttle(host, rt, WeexRefreshKind::Orders)
}
pub fn throttled_fetch_positions<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
) -> Result<(), E> {
    throttle(host, rt, WeexRefreshKind::Positions)
}
pub fn throttled_fetch_balance<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
) -> Result<(), E> {
    throttle(host, rt, WeexRefreshKind::Balance)
}

async fn refresh_reaction<T>(result: T) -> T {
    let mut resumed = false;
    std::future::poll_fn(|cx| {
        if resumed {
            std::task::Poll::Ready(())
        } else {
            resumed = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
    result
}

pub fn run_refresh_timer<'a, H, V: 'a, W: 'a, T: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexPrivateFeedRuntime<H, V, W, T, E>,
    kind: WeexRefreshKind,
) -> RefreshFuture<'a, (), E> {
    let fetch = match (rt.fetch_refresh)(host, kind) {
        Ok(fetch) => fetch,
        Err(_) => return Box::pin(std::future::ready(Ok(()))),
    };
    Box::pin(async move {
        let _: Result<(), E> = async {
            let value = refresh_reaction(fetch.await).await?;
            if !(rt.disposed)(host)? {
                (rt.apply_refresh)(host, kind, value)?;
            }
            Ok(())
        }
        .await;
        Ok(())
    })
}

pub fn dispose<H, V, W, T, E>(
    host: &H,
    rt: &WeexPrivateFeedRuntime<H, V, W, T, E>,
) -> Result<(), E> {
    for kind in [
        WeexRefreshKind::Orders,
        WeexRefreshKind::Positions,
        WeexRefreshKind::Balance,
    ] {
        let timer = (rt.timer)(host, kind)?;
        (rt.clear_timeout)(host, timer.as_ref())?;
    }
    (rt.base_dispose)(host)
}
