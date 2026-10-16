use super::*;
use safe_cex_types::OrderType;
#[derive(Clone)]
pub struct CrossTradePlaceOrderDeps {
    pub get_exchange: CrossTradeGetExchangeFn,
    pub runtime: Rc<dyn CrossTradeRuntime>,
}
pub fn debug(runtime: &dyn CrossTradeRuntime, message: &str) {
    runtime.debug(&format!("[CrossTradeManager] {message}"));
}
pub fn create_cross_trade_place_order(deps: CrossTradePlaceOrderDeps) -> CrossTradePlaceOrderFn {
    Rc::new(move |request| {
        // Everything before placeOrder's await runs at invocation, like JS async.
        let exchange = match (deps.get_exchange)(&request.account) {
            Ok(Some(exchange)) => exchange,
            Ok(None) => return done(Ok(None)),
            Err(error) => return done(Err(error)),
        };
        let symbol = &request.market.symbol;
        if request.copier.execution == CrossTradeExecution::Chase {
            let store = exchange.store();
            let active: Vec<_> = store
                .active_chasers
                .iter()
                .filter(|chaser| {
                    !chaser.is_stopped && chaser.symbol == *symbol && chaser.side == request.side
                })
                .collect();
            if active.len() > 1
                || active
                    .first()
                    .is_some_and(|chaser| chaser.size == request.amount)
            {
                debug(
                    deps.runtime.as_ref(),
                    &format!("Chaser already active for {symbol} {:?}", request.side),
                );
                return done(Ok(None));
            }
            if let Some(chaser) = active.first()
                && let Err(error) = exchange.chaser_terminate(&chaser.id)
            {
                return done(Err(error));
            }
            if !truthy(request.amount) {
                debug(deps.runtime.as_ref(), &format!("Amount is 0 for {symbol}"));
                return done(Ok(None));
            }
            return done(
                exchange
                    .chaser_start(ChaserStart {
                        symbol: symbol.clone(),
                        size: request.amount,
                        side: request.side,
                        max_attempts: 1000.0,
                        reduce_only: request.reduce_only,
                        delay: 1000.0,
                    })
                    .map(|_| None),
            );
        }
        if request.copier.execution == CrossTradeExecution::LimitSlippage
            && !request.reference_price.is_some_and(truthy)
        {
            debug(
                deps.runtime.as_ref(),
                &format!("No reference price for {symbol}"),
            );
            return done(Ok(None));
        }
        if !truthy(request.amount) {
            debug(deps.runtime.as_ref(), &format!("Amount is 0 for {symbol}"));
            return done(Ok(None));
        }
        let limit = request.copier.execution == CrossTradeExecution::LimitSlippage;
        let mut order = crate::order_operations::order_options(
            symbol.clone(),
            request.side,
            if limit {
                OrderType::Limit
            } else {
                OrderType::Market
            },
            request.amount,
        );
        order.reduce_only = Some(request.reduce_only);
        if limit {
            let slip = if request.side == OrderSide::Buy {
                1.0 + 0.009
            } else {
                1.0 - 0.009
            };
            let precision = request.market.precision.price;
            order.price = Some(safe_cex_num::adjust(
                request.reference_price.unwrap() * slip,
                if truthy(precision) { precision } else { 0.01 },
            ));
        }
        let pending = exchange.place_order(order);
        let runtime = deps.runtime.clone();
        let symbol = symbol.clone();
        Box::pin(async move {
            match pending.await {
                Ok(value) => Ok(value),
                Err(error) => {
                    runtime.error("CrossTradeManager Failed to place order", &error);
                    // Source Error.message rather than String(Error); no venue-local coercion.
                    exchange.warning(&format!(
                        "Cross Trade: failed to place {} order on {symbol}: {}",
                        if limit { "limit" } else { "market" },
                        error.message
                    ))?;
                    Err(error)
                }
            }
        })
    })
}
