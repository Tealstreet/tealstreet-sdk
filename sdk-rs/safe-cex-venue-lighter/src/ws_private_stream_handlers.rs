//! Private account dispatch, reconciliation, ping and authenticated subscriptions.
use crate::{
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_private_feed_helpers::LighterPrivateFeedRuntime,
};
use std::rc::Rc;

pub struct LighterPrivateStreamRuntime<H, V> {
    pub feed: Rc<LighterPrivateFeedRuntime<H, V>>,
    pub some: fn(&H, V, &mut dyn FnMut(V) -> SigningResult<V>) -> SigningResult<V>,
    pub for_each: fn(&H, V, &mut dyn FnMut(V) -> SigningResult<V>) -> SigningResult<V>,
    pub for_of: fn(&H, V, &mut dyn FnMut(V) -> SigningResult<()>) -> SigningResult<()>,
    pub spread: fn(&H, V) -> SigningResult<Vec<V>>,
    pub key_value: fn(&H, &V, &'static str) -> SigningResult<V>,
    pub number_value: fn(&V) -> Option<f64>,
    pub date_now: fn(&H) -> SigningResult<V>,
    pub performance_now: fn(&H) -> SigningResult<V>,
    pub constant: fn(&H, &'static str) -> SigningResult<V>,
    pub error_value: fn(&H, TransactionSigningError) -> V,
    pub template_string: fn(&H, V) -> SigningResult<Vec<u16>>,
    pub parse_float_value: fn(&H, V) -> SigningResult<f64>,
    pub subtract: fn(&H, V, V) -> SigningResult<V>,
    pub less_than: fn(&H, V, V) -> SigningResult<bool>,
    pub less_equal: fn(&H, V, V) -> SigningResult<bool>,
    pub greater_equal: fn(&H, V, V) -> SigningResult<bool>,
}
fn get<H, V>(r: &LighterPrivateStreamRuntime<H, V>, value: &V, key: &str) -> SigningResult<V> {
    (r.feed.get)(&r.feed.host, value, &key.encode_utf16().collect::<Vec<_>>())
}
fn set<H, V>(
    r: &LighterPrivateStreamRuntime<H, V>,
    value: &V,
    key: &str,
    field: V,
) -> SigningResult<()> {
    (r.feed.define)(
        &r.feed.host,
        value,
        &key.encode_utf16().collect::<Vec<_>>(),
        field,
    )
}
fn method<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    value: &V,
    key: &'static str,
) -> SigningResult<V> {
    (r.feed.method)(&r.feed.host, value.clone(), key)
}
fn call<H, V>(r: &LighterPrivateStreamRuntime<H, V>, callee: V, args: Vec<V>) -> SigningResult<V> {
    (r.feed.call)(&r.feed.host, callee, args)
}
fn invoke<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    key: &'static str,
    args: Vec<V>,
) -> SigningResult<V> {
    call(r, method(r, owner, key)?, args)
}
fn parent<H, V>(r: &LighterPrivateStreamRuntime<H, V>, owner: &V) -> SigningResult<V> {
    get(r, owner, "parent")
}
fn log<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    message: &str,
    level: &str,
) -> SigningResult<V> {
    let parent = parent(r, owner)?;
    let callee = method(r, &parent, "log")?;
    call(
        r,
        callee,
        vec![(r.feed.text)(message), (r.feed.text)(level)],
    )
}
fn alias<H, V>(
    r: &LighterPrivateStreamRuntime<H, V>,
    object: &V,
    keys: &[&str],
    fallback: V,
) -> SigningResult<V> {
    for key in keys {
        let value = get(r, object, key)?;
        if !(r.feed.nullish)(&value) {
            return Ok(value);
        }
    }
    Ok(fallback)
}
fn string_alias<H, V>(
    r: &LighterPrivateStreamRuntime<H, V>,
    object: &V,
    keys: &[&str],
    fallback: &str,
) -> SigningResult<V> {
    let value = alias(r, object, keys, (r.feed.text)(fallback))?;
    Ok((r.feed.utf16)((r.feed.to_string)(&r.feed.host, value)?))
}
fn eq<H, V>(r: &LighterPrivateStreamRuntime<H, V>, value: &V, word: &str) -> bool {
    (r.feed.strict_same)(value, &(r.feed.text)(word))
}
fn number<H, V>(r: &LighterPrivateStreamRuntime<H, V>, value: V) -> SigningResult<f64> {
    (r.feed.to_number)(&r.feed.host, value)
}
fn parse<H, V>(r: &LighterPrivateStreamRuntime<H, V>, value: V) -> SigningResult<V> {
    Ok((r.feed.number)((r.parse_float_value)(&r.feed.host, value)?))
}
fn fallback_keys<H, V>(
    r: &LighterPrivateStreamRuntime<H, V>,
    object: &V,
    keys: &[&'static str],
    fallback: V,
) -> SigningResult<V> {
    for key in keys {
        let value = (r.key_value)(&r.feed.host, object, key)?;
        if (r.feed.truthy)(&value) {
            return Ok(value);
        }
    }
    Ok(fallback)
}

pub fn handle_account_topic<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    message: &V,
) -> SigningResult<V> {
    for (key, handler) in [
        ("orders", "handleOrderTopic"),
        ("positions", "handlePositionTopic"),
        ("fills", "handleFillsTopic"),
    ] {
        if (r.feed.truthy)(&get(r, &get(r, message, "data")?, key)?) {
            let callee = method(r, owner, handler)?;
            call(r, callee, vec![get(r, &get(r, message, "data")?, key)?])?;
        }
    }
    Ok((r.feed.undefined)())
}
pub fn handle_account_all_topic<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    message: &V,
) -> SigningResult<V> {
    for (key, handler) in [
        ("orders", "handleOrderTopic"),
        ("positions", "handlePositionTopic"),
    ] {
        let by_market = get(r, message, key)?;
        if (r.feed.truthy)(&by_market) {
            let args = if key == "positions" {
                vec![by_market, (r.feed.boolean)(true)]
            } else {
                vec![by_market]
            };
            let flattened = invoke(r, owner, "flattenByMarketPayload", args)?;
            invoke(r, owner, handler, vec![flattened])?;
        }
    }
    let trades = get(r, message, "trades")?;
    if (r.feed.truthy)(&trades) {
        let fills = invoke(r, owner, "flattenByMarketPayload", vec![trades])?;
        let handler = method(r, owner, "handleFillsTopic")?;
        let map = method(r, &fills, "map")?;
        let mapped = (r.feed.map)(&r.feed.host, map, &mut |trade| {
            let fill_id = string_alias(r, &trade, &["trade_id", "fill_id"], "")?;
            let order_id = string_alias(r, &trade, &["order_id", "ask_id", "bid_id"], "")?;
            let trade_id = string_alias(r, &trade, &["trade_id"], "")?;
            let symbol = string_alias(r, &trade, &["symbol"], "")?;
            let sell = eq(r, &get(r, &trade, "side")?, "buy")
                || (r.feed.truthy)(&get(r, &trade, "is_maker_ask")?);
            let price = string_alias(r, &trade, &["price"], "0")?;
            let size = string_alias(r, &trade, &["size", "amount"], "0")?;
            let fee = string_alias(r, &trade, &["fee"], "0")?;
            let maker = (r.feed.truthy)(&get(r, &trade, "is_maker_ask")?);
            let timestamp = get(r, &trade, "timestamp")?;
            let timestamp = if (r.feed.nullish)(&timestamp) {
                (r.date_now)(&r.feed.host)?
            } else {
                timestamp
            };
            let timestamp = number(r, timestamp)?;
            Ok((r.feed.object)(vec![
                ("fillId", fill_id),
                ("orderId", order_id),
                ("tradeId", trade_id),
                ("symbol", symbol),
                ("side", (r.feed.text)(if sell { "SELL" } else { "BUY" })),
                ("price", price),
                ("size", size),
                ("fee", fee),
                ("isMaker", (r.feed.boolean)(maker)),
                ("timestamp", (r.feed.number)(timestamp)),
            ]))
        })?;
        call(r, handler, vec![mapped])?;
    }
    Ok((r.feed.undefined)())
}

pub fn handle_order_topic<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    data: V,
) -> SigningResult<V> {
    if !(r.feed.truthy)(&data) {
        return Ok((r.feed.undefined)());
    }
    let some = method(r, &data, "some")?;
    let cancelled = (r.some)(&r.feed.host, some, &mut |order| {
        let status = invoke(r, owner, "normalizeOrderStatus", vec![order])?;
        Ok((r.feed.boolean)(
            eq(r, &status, "CANCELLED") || eq(r, &status, "CANCELED"),
        ))
    })?;
    let backfill = invoke(r, owner, "shouldUseRestOrderReconciliation", vec![])?;
    if !(r.feed.truthy)(&cancelled) && (r.feed.truthy)(&backfill) {
        invoke(
            r,
            owner,
            "throttleFetchOrders",
            vec![
                (r.feed.number)(500.0),
                (r.feed.object)(vec![("skipIfRecent", (r.feed.boolean)(true))]),
            ],
        )?;
    }
    let for_each = method(r, &data, "forEach")?;
    (r.for_each)(&r.feed.host, for_each, &mut |order| {
        let status = invoke(r, owner, "normalizeOrderStatus", vec![order.clone()])?;
        let id = string_alias(
            r,
            &order,
            &[
                "orderId",
                "order_id",
                "orderIndex",
                "order_index",
                "OrderId",
                "OrderIndex",
            ],
            "",
        )?;
        if (r.feed.truthy)(&id) {
            let parent = parent(r, owner)?;
            let set = get(r, &parent, "recentlyCancelledOrders")?;
            if (r.feed.truthy)(&invoke(r, &set, "has", vec![id])?) {
                return Ok((r.feed.undefined)());
            }
        }
        let orders = invoke(r, &parent(r, owner)?, "mapOrder", vec![order.clone()])?;
        if !(r.feed.truthy)(&get(r, &orders, "length")?) {
            return Ok((r.feed.undefined)());
        }
        let first = get(r, &orders, "0")?;
        let open = eq(r, &status, "OPEN")
            || eq(r, &status, "PENDING")
            || eq(r, &status, "PARTIALLY_FILLED")
            || (r.feed.strict_same)(
                &get(r, &first, "status")?,
                &(r.constant)(&r.feed.host, "OrderStatus.Open")?,
            );
        let cancelled = eq(r, &status, "CANCELLED")
            || eq(r, &status, "CANCELED")
            || (r.feed.strict_same)(
                &get(r, &first, "status")?,
                &(r.constant)(&r.feed.host, "OrderStatus.Canceled")?,
            );
        let filled = eq(r, &status, "FILLED");
        let price = if (r.number_value)(&get(r, &first, "price")?).is_some_and(f64::is_finite) {
            get(r, &first, "price")?
        } else {
            parse(
                r,
                fallback_keys(r, &order, &["price", "limit_price"], (r.feed.text)("0"))?,
            )?
        };
        let amount = if (r.number_value)(&get(r, &first, "amount")?).is_some_and(f64::is_finite) {
            get(r, &first, "amount")?
        } else {
            parse(
                r,
                fallback_keys(
                    r,
                    &order,
                    &["amount", "size", "initialBaseAmount", "initial_base_amount"],
                    (r.feed.text)("0"),
                )?,
            )?
        };
        let updated = number(
            r,
            fallback_keys(
                r,
                &order,
                &["updatedAt", "updated_at", "timestamp"],
                (r.feed.number)(0.0),
            )?,
        )?;
        let notify = invoke(
            r,
            owner,
            "isRecentStreamEvent",
            vec![(r.feed.number)(updated)],
        )?;
        if filled
            && (r.less_equal)(&r.feed.host, price.clone(), (r.feed.number)(0.0))?
            && (r.less_equal)(&r.feed.host, amount.clone(), (r.feed.number)(0.0))?
        {
            return Ok((r.feed.undefined)());
        }
        if filled && (r.feed.truthy)(&notify) {
            let emitter = get(r, owner, "emitter")?;
            let emit = method(r, &emitter, "emit")?;
            let payload = (r.feed.object)(vec![
                ("id", get(r, &first, "id")?),
                ("side", get(r, &first, "side")?),
                ("symbol", get(r, &first, "symbol")?),
                ("price", price),
                ("amount", amount),
            ]);
            call(r, emit, vec![(r.feed.text)("fill"), payload])?;
        }
        if filled || cancelled {
            let reason = string_alias(r, &order, &["cancelReason", "cancel_reason"], "")?;
            if !filled
                && (r.feed.truthy)(&notify)
                && (r.feed.truthy)(&reason)
                && !(r.feed.truthy)(&invoke(
                    r,
                    owner,
                    "isUserCancelledReason",
                    vec![reason.clone()],
                )?)
            {
                invoke(r, owner, "emitAndLogError", vec![reason])?;
            }
            let update = if cancelled {
                None
            } else {
                Some(method(r, &get(r, owner, "store")?, "addOrUpdateOrders")?)
            };
            let map = method(r, &orders, "map")?;
            let mapped = (r.feed.map)(&r.feed.host, map, &mut |safe| {
                if cancelled {
                    Ok((r.feed.object)(vec![("id", get(r, &safe, "id")?)]))
                } else {
                    let copied = (r.feed.copy)(&r.feed.host, safe)?;
                    set(
                        r,
                        &copied,
                        "status",
                        (r.constant)(&r.feed.host, "OrderStatus.Closed")?,
                    )?;
                    Ok(copied)
                }
            })?;
            let update = match update {
                Some(update) => update,
                None => method(r, &get(r, owner, "store")?, "removeOrders")?,
            };
            call(r, update, vec![mapped])?;
        } else if open {
            invoke(
                r,
                &get(r, owner, "store")?,
                "addOrUpdateOrders",
                vec![orders],
            )?;
        }
        Ok((r.feed.undefined)())
    })?;
    Ok((r.feed.undefined)())
}

pub fn handle_position_topic<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    data: V,
) -> SigningResult<V> {
    if !(r.feed.truthy)(&data) {
        return Ok((r.feed.undefined)());
    }
    let updates = (r.feed.array)(vec![]);
    (r.for_of)(&r.feed.host, data, &mut |raw| {
        let result = (|| {
            let mapped = invoke(r, &parent(r, owner)?, "mapPosition", vec![raw.clone()])?;
            let push = method(r, &updates, "push")?;
            let values = if (r.feed.truthy)(&mapped) {
                vec![(r.feed.array)(vec![mapped.clone(), mapped])]
            } else {
                let closes = invoke(
                    r,
                    owner,
                    "resolveExplicitlyClosedPositionUpdates",
                    vec![raw],
                )?;
                (r.spread)(&r.feed.host, closes)?
            };
            call(r, push, values)?;
            Ok(())
        })();
        if let Err(error) = result {
            let log = method(r, &parent(r, owner)?, "log")?;
            let error = (r.error_value)(&r.feed.host, error);
            let message = if (r.feed.nullish)(&error) {
                (r.feed.undefined)()
            } else {
                get(r, &error, "message")?
            };
            let message = if (r.feed.truthy)(&message) {
                message
            } else {
                (r.feed.utf16)((r.feed.to_string)(&r.feed.host, error)?)
            };
            let mut units: Vec<u16> = "[Lighter][PWS] Failed to map position update: "
                .encode_utf16()
                .collect();
            units.extend((r.template_string)(&r.feed.host, message)?);
            call(
                r,
                log,
                vec![(r.feed.utf16)(units), (r.feed.text)("warning")],
            )?;
        }
        Ok(())
    })?;
    if !(r.feed.truthy)(&get(r, &updates, "length")?) {
        return Ok((r.feed.undefined)());
    }
    invoke(
        r,
        &get(r, owner, "store")?,
        "updatePositions",
        vec![updates],
    )?;
    let parent = parent(r, owner)?;
    let now = (r.feed.now)(&r.feed.host)?;
    set(
        r,
        &parent,
        "lastWebsocketPositionsUpdate",
        (r.feed.number)(now),
    )?;
    Ok((r.feed.undefined)())
}
pub fn handle_fills_topic<H, V: Clone>(
    r: &LighterPrivateStreamRuntime<H, V>,
    owner: &V,
    data: V,
) -> SigningResult<V> {
    if !(r.feed.truthy)(&data) {
        return Ok((r.feed.undefined)());
    }
    let for_each = method(r, &data, "forEach")?;
    (r.for_each)(&r.feed.host, for_each, &mut |fill| {
        let recent = method(r, owner, "isRecentStreamEvent")?;
        if (r.feed.truthy)(&call(r, recent, vec![get(r, &fill, "timestamp")?])?) {
            let emit = method(r, &get(r, owner, "emitter")?, "emit")?;
            let id = get(r, &fill, "orderId")?;
            let side = if eq(r, &get(r, &fill, "side")?, "BUY") {
                "buy"
            } else {
                "sell"
            };
            let symbol = get(r, &fill, "symbol")?;
            let price = parse(r, get(r, &fill, "price")?)?;
            let amount = parse(r, get(r, &fill, "size")?)?;
            call(
                r,
                emit,
                vec![
                    (r.feed.text)("fill"),
                    (r.feed.object)(vec![
                        ("id", id),
                        ("side", (r.feed.text)(side)),
                        ("symbol", symbol),
                        ("price", price),
                        ("amount", amount),
                    ]),
                ],
            )?;
        }
        Ok((r.feed.undefined)())
    })?;
    if (r.feed.truthy)(&get(r, owner, "useReadonlyFallback")?) {
        invoke(
            r,
            owner,
            "throttleFetchPositions",
            vec![(r.feed.number)(250.0)],
        )?;
    }
    Ok((r.feed.undefined)())
}

pub fn ping<H, V: Clone>(r: &LighterPrivateStreamRuntime<H, V>, owner: &V) -> SigningResult<V> {
    if (r.feed.truthy)(&get(r, owner, "isDisposed")?) {
        return Ok((r.feed.undefined)());
    }
    let now = (r.feed.now)(&r.feed.host)?;
    if (r.feed.truthy)(&invoke(r, owner, "isWsRateLimited", vec![])?) {
        let elapsed = (r.subtract)(
            &r.feed.host,
            (r.feed.number)(now),
            get(r, owner, "lastRateLimitedPingAt")?,
        )?;
        let interval = (r.constant)(&r.feed.host, "RATE_LIMIT_PING_INTERVAL_MS")?;
        if (r.less_than)(&r.feed.host, elapsed, interval)? {
            let since_log = (r.subtract)(
                &r.feed.host,
                (r.feed.number)(now),
                get(r, owner, "lastRateLimitedPingSkipLogAt")?,
            )?;
            let cooldown = (r.constant)(&r.feed.host, "RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS")?;
            if (r.greater_equal)(&r.feed.host, since_log, cooldown)? {
                set(
                    r,
                    owner,
                    "lastRateLimitedPingSkipLogAt",
                    (r.feed.number)(now),
                )?;
                log(
                    r,
                    owner,
                    "[Lighter][PWS] Deferring ping while server rate-limit cooldown is active.",
                    "info",
                )?;
            }
            return Ok((r.feed.undefined)());
        }
        set(r, owner, "lastRateLimitedPingAt", (r.feed.number)(now))?;
    }
    set(r, owner, "pingAt", (r.performance_now)(&r.feed.host)?)?;
    let send = method(r, owner, "sendWithLog")?;
    let payload = (r.feed.object)(vec![
        ("type", (r.feed.text)("ping")),
        ("timestamp", (r.feed.number)(now)),
    ]);
    let serialized = (r.feed.stringify)(&r.feed.host, payload)?;
    call(r, send, vec![serialized])?;
    Ok((r.feed.undefined)())
}
pub fn on_open<H, V: Clone>(r: &LighterPrivateStreamRuntime<H, V>, owner: &V) -> SigningResult<V> {
    if (r.feed.truthy)(&get(r, owner, "isDisposed")?) {
        return Ok((r.feed.undefined)());
    }
    for (key, value) in [
        ("openedSinceLastConnect", (r.feed.boolean)(true)),
        ("lastStreamRecoveryAttemptAt", (r.feed.number)(0.0)),
        (
            "hasPendingAccountAllOrdersSubscribe",
            (r.feed.boolean)(false),
        ),
        ("hasReceivedAccountAllOrdersAck", (r.feed.boolean)(false)),
    ] {
        set(r, owner, key, value)?;
    }
    invoke(r, owner, "clearStreamRecoveryReconnectTimeout", vec![])?;
    invoke(r, owner, "clearRateLimitResubscribeTimeout", vec![])?;
    set(
        r,
        owner,
        "lastConnectTime",
        (r.feed.number)((r.feed.now)(&r.feed.host)?),
    )?;
    for key in [
        "lastRateLimitedPingAt",
        "lastRateLimitedPingSkipLogAt",
        "lastIdlePingAt",
    ] {
        set(r, owner, key, (r.feed.number)(0.0))?;
    }
    if (r.feed.truthy)(&get(r, owner, "useReadonlyFallback")?)
        && !(r.feed.truthy)(&get(r, owner, "hasWarnedReadonlyFallback")?)
    {
        set(
            r,
            owner,
            "hasWarnedReadonlyFallback",
            (r.feed.boolean)(true),
        )?;
        invoke(r, owner, "emitReadonlyFallbackWarning", vec![])?;
    }
    invoke(r, owner, "beginDiagnosticsLoop", vec![])?;
    invoke(r, owner, "ping", vec![])?;
    invoke(r, owner, "authenticateAndSubscribe", vec![])?;
    if (r.feed.truthy)(&(r.constant)(
        &r.feed.host,
        "ENABLE_RECONNECTING_WEBSOCKET",
    )?) {
        invoke(r, &parent(r, owner)?, "fetchAndSetPrivateSnapshot", vec![])?;
    }
    Ok((r.feed.undefined)())
}
pub fn authenticate_and_subscribe<H: 'static, V: Clone + 'static>(
    r: &Rc<LighterPrivateStreamRuntime<H, V>>,
    owner: &V,
) -> NativePromise<V> {
    let (promise, out) = NativePromise::pending(r.feed.promise_runtime.clone());
    let started = (|| {
        let index = invoke(r, &parent(r, owner)?, "getPrivateWsAccountIndex", vec![])?;
        let mut channel: Vec<u16> = "account_all/".encode_utf16().collect();
        channel.extend((r.template_string)(&r.feed.host, index.clone())?);
        let payload = (r.feed.object)(vec![
            ("type", (r.feed.text)("subscribe")),
            ("channel", (r.feed.utf16)(channel)),
        ]);
        invoke(
            r,
            owner,
            "sendSubscribePayload",
            vec![payload, (r.feed.boolean)(false)],
        )?;
        if !(r.feed.truthy)(&get(r, owner, "accountAllOrdersChannelEnabled")?)
            || (r.feed.truthy)(&get(r, owner, "hasPendingAccountAllOrdersSubscribe")?)
            || (r.feed.truthy)(&get(r, owner, "hasReceivedAccountAllOrdersAck")?)
        {
            return Ok(None);
        }
        if (r.feed.truthy)(&invoke(r, owner, "isWsRateLimited", vec![])?) {
            log(
                r,
                owner,
                "[Lighter][PWS] Skipping account_all_orders subscription during server rate-limit cooldown.",
                "warning",
            )?;
            invoke(r, owner, "scheduleRateLimitResubscribe", vec![])?;
            return Ok(None);
        }
        let fetched = (|| {
            let token = method(r, &parent(r, owner)?, "getPrivateWsAuthToken")?;
            (r.feed.call_async)(&r.feed.host, token, vec![(r.feed.number)(60.0)])
        })();
        Ok(Some((index, fetched)))
    })();
    match started {
        Err(error) => out.settle(Err(error)),
        Ok(None) => out.settle(Ok((r.feed.undefined)())),
        Ok(Some((index, fetch))) => {
            let r = r.clone();
            let owner = owner.clone();
            let finish = move |token: SigningResult<V>| {
                let completed = (|| {
                    let token = match token {
                        Ok(token) => token,
                        Err(_) => {
                            log(
                                &r,
                                &owner,
                                "[Lighter][PWS] Auth token unavailable, subscribing without auth.",
                                "warning",
                            )?;
                            (r.feed.undefined)()
                        }
                    };
                    if !(r.feed.truthy)(&token) {
                        log(
                            &r,
                            &owner,
                            "[Lighter][PWS] Skipping account_all_orders subscription because auth token is unavailable.",
                            "warning",
                        )?;
                        return Ok((r.feed.undefined)());
                    }
                    let mut channel: Vec<u16> = "account_all_orders/".encode_utf16().collect();
                    channel.extend((r.template_string)(&r.feed.host, index)?);
                    let payload = (r.feed.object)(vec![
                        ("type", (r.feed.text)("subscribe")),
                        ("channel", (r.feed.utf16)(channel)),
                        ("auth", token),
                    ]);
                    set(
                        &r,
                        &owner,
                        "hasPendingAccountAllOrdersSubscribe",
                        (r.feed.boolean)(true),
                    )?;
                    invoke(
                        &r,
                        &owner,
                        "sendSubscribePayload",
                        vec![payload, (r.feed.boolean)(true)],
                    )?;
                    Ok((r.feed.undefined)())
                })();
                out.settle(completed);
            };
            match fetch {
                Ok(fetch) => fetch.subscribe(move |token| finish(token.map(|v| (*v).clone()))),
                Err(error) => finish(Err(error)),
            }
        }
    }
    promise
}
