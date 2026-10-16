//! Own arrow-field factories bind actual Rust bodies to their original lexical receiver.
use crate::{
    class_admin as admin,
    native_bindings::{BindingOps, BoundHost},
    order_writes as write, rest_reads as read,
};
use std::rc::Rc;
fn arg<V: Clone, E>(ops: &BindingOps<V, E>, args: &[V], i: usize) -> V {
    args.get(i)
        .cloned()
        .unwrap_or_else(|| (ops.value.undefined)())
}
pub fn member<V: Clone + 'static, E: Clone + 'static>(
    ops: Rc<BindingOps<V, E>>,
    receiver: V,
    name: &'static str,
) -> V {
    let h = Rc::new(BoundHost {
        ops: ops.clone(),
        receiver,
    });
    let length = match name {
        "dispose" | "getAccount" | "validateAccount" | "fetchMarkets" | "fetchTickers"
        | "fetchOrders" | "fetchPositions" | "cancelAllOrders" | "fetchBalance" => 0,
        "changeMarginType" | "listenOHLCV" | "listenOrderBook" => 2,
        "setLeverage" => 3,
        _ => 1,
    };
    (ops.value.function)(
        name,
        length,
        Rc::new(move |args| {
            let first = arg(&h.ops, &args, 0);
            let future: crate::socket_handlers::SocketFuture<V, E> = match name {
                "dispose" => return admin::dispose(&h),
                "getAccount" => admin::get_account(h.clone()),
                "validateAccount" => admin::validate_account(h.clone()),
                "fetchMarkets" => Box::pin(read::fetch_markets(h.clone())),
                "fetchTickers" => Box::pin(read::fetch_tickers(h.clone())),
                "fetchOHLCV" => Box::pin(read::fetch_ohlcv(h.clone(), first)),
                "fetchOrderBook" => Box::pin(read::fetch_order_book(h.clone(), first)),
                "fetchOrders" => Box::pin(read::fetch_orders(h.clone())),
                "fetchPositions" => Box::pin(read::fetch_positions(h.clone())),
                "fetchBalance" => Box::pin(read::fetch_balance(h.clone())),
                "mapOrder" => return read::map_order(&*h, first),
                "placeOrder" => Box::pin(write::place_order(h.clone(), first)),
                "cancelOrder" => Box::pin(write::cancel_order(h.clone(), first)),
                "cancelOrders" => Box::pin(write::cancel_orders(h.clone(), first)),
                "cancelSymbolOrders" => Box::pin(write::cancel_symbol_orders(h.clone(), first)),
                "updateOrder" => Box::pin(write::update_order(h.clone(), first)),
                "placeOrders" => Box::pin(write::place_orders(h.clone(), first)),
                "setLeverage" => admin::admin_post(h.clone(), name, args),
                "cancelAllOrders" => admin::admin_post(h.clone(), name, args),
                "changePositionMode" => admin::change_position_mode(h.clone()),
                "changeMarginType" => admin::change_margin_type(h.clone(), args),
                "listenOHLCV" => return write::listen_ohlcv(&*h, first, arg(&h.ops, &args, 1)),
                "listenOrderBook" => {
                    return write::listen_order_book(&*h, first, arg(&h.ops, &args, 1));
                }
                "mapOrderStatus"
                | "mapOrderType"
                | "mapCryptocomOrderType"
                | "mapTimeInForce"
                | "mapStopTriggerType"
                | "mapCryptocomTimeInForce" => {
                    return crate::class_mappings::member(&h, name, first, false);
                }
                _ => unreachable!("every exchange arrow field binds a Rust body"),
            };
            Ok((h.ops.promise)(future))
        }),
    )
}
