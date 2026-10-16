use crate::socket_handlers::{self as bodies, SocketFuture, SocketOps};
use std::rc::Rc;

pub struct SocketClassOps<V, E> {
    pub body: Rc<SocketOps<V, E>>,
    pub define: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub superclass: Rc<dyn Fn(&V) -> Result<V, E>>,
    pub super_construct: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub derive:
        Rc<dyn Fn(&str, V, usize, Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>) -> Result<V, E>>,
    pub binding: Rc<crate::native_bindings::BindingOps<V, E>>,
    pub promise: Rc<dyn Fn(SocketFuture<V, E>) -> V>,
}
fn argument<V: Clone, E>(h: &SocketClassOps<V, E>, args: &[V], i: usize) -> V {
    args.get(i).cloned().unwrap_or_else(|| (h.body.undefined)())
}
fn native_member<V: Clone + 'static, E: 'static>(
    h: Rc<SocketClassOps<V, E>>,
    receiver: V,
    name: &'static str,
    private: bool,
) -> Result<V, E> {
    if name == "handleTickerUpdate" {
        return bodies::initialize_ticker_handler(h.body.clone(), receiver);
    }
    if name == "handleBookUpdate" {
        return bodies::initialize_book_handler(h.body.clone(), receiver);
    }
    let ops = h.clone();
    let length = if matches!(name, "listenOHLCV" | "listenOrderBook") {
        2
    } else {
        1
    };
    Ok((h.body.function)(
        name,
        length,
        Rc::new(move |args| {
            let first = argument(&ops, &args, 0);
            match name {
                "onMessage" if private => bodies::private_on_message(&ops.body, &receiver, &first),
                "onMessage" => bodies::public_on_message(&ops.body, &receiver, &first),
                "handleTradeUpdate" => bodies::trade_update(&ops.body, &first),
                "handleOrderUpdate" => bodies::order_update(&ops.body, &receiver, &first),
                "handlePositionUpdate" => Ok((ops.promise)(bodies::position_update(
                    ops.body.clone(),
                    receiver.clone(),
                    first,
                ))),
                "handleBalanceUpdate" => bodies::balance_update(&ops.body, &receiver, &first),
                "listenOHLCV" => bodies::listen_ohlcv(
                    ops.body.clone(),
                    receiver.clone(),
                    first,
                    argument(&ops, &args, 1),
                ),
                "listenOrderBook" => bodies::listen_order_book(
                    ops.body.clone(),
                    receiver.clone(),
                    first,
                    argument(&ops, &args, 1),
                ),
                _ => unreachable!("only source native members enter this factory"),
            }
        }),
    ))
}
fn initialize_public<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<SocketClassOps<V, E>>,
    receiver: V,
) -> Result<(), E> {
    let value = (h.body.boolean)(true);
    (h.define)(&receiver, "treatAnyMessageAsPong", value)?;
    let value = {
        let ctor = (h.body.global)("Set")?;
        (h.construct)(ctor, vec![])?
    };
    (h.define)(&receiver, "subscriptions", value)?;
    let value = {
        let ctor = (h.body.global)("Map")?;
        (h.construct)(ctor, vec![])?
    };
    (h.define)(&receiver, "marketIdsByIndexInstrument", value)?;
    let value = (h.body.number)(1.);
    (h.define)(&receiver, "messageId", value)?;
    let value = (h.body.undefined)();
    (h.define)(&receiver, "reconnectSubscriptionTimer", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "connectAndSubscribe",
        false,
    );
    (h.define)(&receiver, "connectAndSubscribe", value)?;
    let value = crate::class_socket::member(h.binding.clone(), receiver.clone(), "onOpen", false);
    (h.define)(&receiver, "onOpen", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "shouldEnforcePongMissReconnect",
        false,
    );
    (h.define)(&receiver, "shouldEnforcePongMissReconnect", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "getWatchdogStaleThresholdMs",
        false,
    );
    (h.define)(&receiver, "getWatchdogStaleThresholdMs", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "getWatchdogPingThresholdMs",
        false,
    );
    (h.define)(&receiver, "getWatchdogPingThresholdMs", value)?;
    let value = crate::class_socket::member(h.binding.clone(), receiver.clone(), "ping", false);
    (h.define)(&receiver, "ping", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "resubscribe", false);
    (h.define)(&receiver, "resubscribe", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "sendSubscriptionFrames",
        false,
    );
    (h.define)(&receiver, "sendSubscriptionFrames", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "registerAndSubscribe",
        false,
    );
    (h.define)(&receiver, "registerAndSubscribe", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToOrderBook",
        false,
    );
    (h.define)(&receiver, "subscribeToOrderBook", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToTicker",
        false,
    );
    (h.define)(&receiver, "subscribeToTicker", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToTickers",
        false,
    );
    (h.define)(&receiver, "subscribeToTickers", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToMarketData",
        false,
    );
    (h.define)(&receiver, "subscribeToMarketData", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToTrades",
        false,
    );
    (h.define)(&receiver, "subscribeToTrades", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToCandlestick",
        false,
    );
    (h.define)(&receiver, "subscribeToCandlestick", value)?;
    let value = native_member(h.clone(), receiver.clone(), "onMessage", false)?;
    (h.define)(&receiver, "onMessage", value)?;
    let value = native_member(h.clone(), receiver.clone(), "handleTickerUpdate", false)?;
    (h.define)(&receiver, "handleTickerUpdate", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "handleValuationUpdate",
        false,
    );
    (h.define)(&receiver, "handleValuationUpdate", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "getMarketIdsByIndexInstrument",
        false,
    );
    (h.define)(&receiver, "getMarketIdsByIndexInstrument", value)?;
    let value = native_member(h.clone(), receiver.clone(), "handleBookUpdate", false)?;
    (h.define)(&receiver, "handleBookUpdate", value)?;
    let value = native_member(h.clone(), receiver.clone(), "handleTradeUpdate", false)?;
    (h.define)(&receiver, "handleTradeUpdate", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "handleCandlestickUpdate",
        false,
    );
    (h.define)(&receiver, "handleCandlestickUpdate", value)?;
    let value = native_member(h.clone(), receiver.clone(), "listenOHLCV", false)?;
    (h.define)(&receiver, "listenOHLCV", value)?;
    let value = native_member(h.clone(), receiver.clone(), "listenOrderBook", false)?;
    (h.define)(&receiver, "listenOrderBook", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "unsubscribe", false);
    (h.define)(&receiver, "unsubscribe", value)?;
    let value = crate::class_socket::member(h.binding.clone(), receiver.clone(), "dispose", false);
    (h.define)(&receiver, "dispose", value)?;
    Ok(())
}
pub fn public_constructor<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<SocketClassOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let superclass = (h.superclass)(&active)?;
    let receiver = (h.super_construct)(superclass, target, vec![argument(&h, &args, 0)])?;
    initialize_public(h, receiver.clone())?;
    Ok(receiver)
}
pub fn define_public_socket<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<SocketClassOps<V, E>>,
) -> Result<V, E> {
    let base = (h.body.global)("BaseWebSocket")?;
    let ops = h.clone();
    (h.derive)(
        "CryptocomPublicWebSocket",
        base,
        1,
        Rc::new(move |active, target, args| public_constructor(ops.clone(), active, target, args)),
    )
}
fn initialize_private<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<SocketClassOps<V, E>>,
    receiver: V,
) -> Result<(), E> {
    let value = (h.body.boolean)(true);
    (h.define)(&receiver, "treatAnyMessageAsPong", value)?;
    let value = {
        let ctor = (h.body.global)("Set")?;
        (h.construct)(ctor, vec![])?
    };
    (h.define)(&receiver, "subscriptions", value)?;
    let value = (h.body.number)(1.);
    (h.define)(&receiver, "messageId", value)?;
    let value = (h.body.boolean)(false);
    (h.define)(&receiver, "isAuthenticated", value)?;
    let value = (h.body.undefined)();
    (h.define)(&receiver, "reconnectAuthTimer", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "connectAndSubscribe",
        true,
    );
    (h.define)(&receiver, "connectAndSubscribe", value)?;
    let value = crate::class_socket::member(h.binding.clone(), receiver.clone(), "onOpen", true);
    (h.define)(&receiver, "onOpen", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "shouldEnforcePongMissReconnect",
        true,
    );
    (h.define)(&receiver, "shouldEnforcePongMissReconnect", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "getWatchdogStaleThresholdMs",
        true,
    );
    (h.define)(&receiver, "getWatchdogStaleThresholdMs", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "getWatchdogPingThresholdMs",
        true,
    );
    (h.define)(&receiver, "getWatchdogPingThresholdMs", value)?;
    let value = crate::class_socket::member(h.binding.clone(), receiver.clone(), "ping", true);
    (h.define)(&receiver, "ping", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "authenticate", true);
    (h.define)(&receiver, "authenticate", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "resubscribe", true);
    (h.define)(&receiver, "resubscribe", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToOrderUpdates",
        true,
    );
    (h.define)(&receiver, "subscribeToOrderUpdates", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToPositionUpdates",
        true,
    );
    (h.define)(&receiver, "subscribeToPositionUpdates", value)?;
    let value = crate::class_socket::member(
        h.binding.clone(),
        receiver.clone(),
        "subscribeToBalanceUpdates",
        true,
    );
    (h.define)(&receiver, "subscribeToBalanceUpdates", value)?;
    let value = native_member(h.clone(), receiver.clone(), "onMessage", true)?;
    (h.define)(&receiver, "onMessage", value)?;
    let value = native_member(h.clone(), receiver.clone(), "handleOrderUpdate", true)?;
    (h.define)(&receiver, "handleOrderUpdate", value)?;
    let value = native_member(h.clone(), receiver.clone(), "handlePositionUpdate", true)?;
    (h.define)(&receiver, "handlePositionUpdate", value)?;
    let value = native_member(h.clone(), receiver.clone(), "handleBalanceUpdate", true)?;
    (h.define)(&receiver, "handleBalanceUpdate", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "mapOrderType", true);
    (h.define)(&receiver, "mapOrderType", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "mapOrderStatus", true);
    (h.define)(&receiver, "mapOrderStatus", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "mapTimeInForce", true);
    (h.define)(&receiver, "mapTimeInForce", value)?;
    let value =
        crate::class_socket::member(h.binding.clone(), receiver.clone(), "unsubscribe", true);
    (h.define)(&receiver, "unsubscribe", value)?;
    let value = crate::class_socket::member(h.binding.clone(), receiver.clone(), "dispose", true);
    (h.define)(&receiver, "dispose", value)?;
    Ok(())
}
pub fn private_constructor<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<SocketClassOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let superclass = (h.superclass)(&active)?;
    let receiver = (h.super_construct)(superclass, target, vec![argument(&h, &args, 0)])?;
    initialize_private(h, receiver.clone())?;
    Ok(receiver)
}
pub fn define_private_socket<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<SocketClassOps<V, E>>,
) -> Result<V, E> {
    let base = (h.body.global)("BaseWebSocket")?;
    let ops = h.clone();
    (h.derive)(
        "CryptocomPrivateWebSocket",
        base,
        1,
        Rc::new(move |active, target, args| private_constructor(ops.clone(), active, target, args)),
    )
}
