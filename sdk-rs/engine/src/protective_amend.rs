//! A stop or take-profit edit that cannot stand is refused before anything is
//! canceled. Most venues amend these by cancel-then-place, so a replacement the
//! venue refuses would leave the position without its protection.
use crate::model::{OrderSide, OrderStatus, OrderType, StopTriggerType};
use crate::store::Store;

/// A ticker older than this does not count as the price the venue will check.
pub const FRESH_TICKER_MS: f64 = 5_000.0;
/// Further than this from the symbol's own price is a price meant for something else.
const IMPLAUSIBLE_DISTANCE: f64 = 0.5;

/// The refusal as the caller sees it: invalid input, and nothing was sent.
pub fn refused(reason: Option<String>) -> crate::error::VenueError {
    crate::error::VenueError {
        kind: Some(crate::error::ErrorKind::InvalidInput),
        outcome: Some(crate::error::Outcome::NotSent),
        ..crate::error::VenueError::new(reason.unwrap_or_default())
    }
}

/// The symbol's ticker changed recently enough to stand for the venue's price.
pub fn fresh_ticker(store: &Store, symbol: &str, now: f64) -> bool {
    store
        .ticker_changed_at(symbol)
        .is_some_and(|at| now - at <= FRESH_TICKER_MS)
}

/// Why an amend of order `id` to `price` must not be sent, if it must not.
/// Where the venue `reclassifies` the order by its new price, only the
/// implausible-distance guard applies here; the venue judges the rest.
pub fn refusal(
    store: &Store,
    id: &str,
    price: Option<f64>,
    now: f64,
    reclassifies: impl FnOnce(&crate::model::Order) -> bool,
) -> Option<String> {
    let order = store.order(id)?;
    // An attached leg is a field of its parent order, amended in place: nothing is canceled.
    if order.parent_id.is_some() {
        return None;
    }
    let name = match order.kind {
        OrderType::StopLoss => "stop",
        OrderType::TakeProfit => "take profit",
        _ => return None,
    };
    let trigger = price.filter(|price| price.is_finite() && *price > 0.0)?;
    if order.status != OrderStatus::Open || trigger == order.price {
        return None;
    }
    let ticker = store.ticker(&order.symbol)?;
    let usable = |price: f64| (price.is_finite() && price > 0.0).then_some(price);
    let unchanged = format!("your {} {name} is unchanged", order.symbol);
    if let Some(own) = usable(ticker.mark).or(usable(ticker.last))
        && (trigger - own).abs() / own > IMPLAUSIBLE_DISTANCE
    {
        return Some(format!(
            "A {name} at {trigger} is more than {:.0}% from the {} price {own}; {unchanged}",
            IMPLAUSIBLE_DISTANCE * 100.0,
            order.symbol,
        ));
    }
    if !fresh_ticker(store, &order.symbol, now) || reclassifies(order) {
        return None;
    }
    let references: &[(&str, f64)] = match order.trigger_type {
        Some(StopTriggerType::MarkPrice) => &[("mark", ticker.mark)],
        Some(StopTriggerType::LastPrice) => &[("last", ticker.last)],
        Some(StopTriggerType::IndexPrice) => &[("index", ticker.index)],
        None => &[("mark", ticker.mark), ("last", ticker.last)],
    };
    // A sell closes a long: its stop fires at or below the price, its take profit at or above.
    let fires = |reference: f64| match (order.side, order.kind) {
        (OrderSide::Sell, OrderType::StopLoss) | (OrderSide::Buy, OrderType::TakeProfit) => {
            trigger >= reference
        }
        _ => trigger <= reference,
    };
    references.iter().find_map(|(label, reference)| {
        let reference = usable(*reference)?;
        fires(reference).then(|| {
            format!("A {name} at {trigger} would trigger at once against the {label} price {reference}; {unchanged}")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Order, Ticker};

    fn fixture(kind: OrderType, side: OrderSide, trigger: Option<StopTriggerType>) -> Store {
        let mut store = Store::default();
        store.set_clock(1_000.0);
        store.set_tickers(vec![Ticker {
            symbol: "X".into(),
            mark: 100.0,
            last: 101.0,
            index: 99.0,
            ..Ticker::default()
        }]);
        store.upsert_order(Order {
            id: "o".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "X".into(),
            kind,
            side,
            price: 90.0,
            amount: 1.0,
            filled: 0.0,
            remaining: 1.0,
            reduce_only: true,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: trigger,
            take_profit: None,
            stop_loss: None,
            extra: None,
        });
        store
    }

    #[test]
    fn refuses_a_trigger_on_the_firing_side_of_its_own_reference() {
        use OrderSide::*;
        use OrderType::*;
        use StopTriggerType::*;
        // (kind, side, trigger type, new trigger, refused)
        let cases = [
            (StopLoss, Sell, MarkPrice, 100.0, true),
            (StopLoss, Sell, MarkPrice, 99.0, false),
            (StopLoss, Sell, LastPrice, 100.5, false),
            (TakeProfit, Sell, LastPrice, 101.0, true),
            (TakeProfit, Sell, LastPrice, 102.0, false),
            (StopLoss, Buy, MarkPrice, 100.0, true),
            (StopLoss, Buy, MarkPrice, 101.0, false),
            (TakeProfit, Buy, LastPrice, 101.0, true),
            (TakeProfit, Buy, LastPrice, 100.0, false),
            (StopLoss, Sell, IndexPrice, 99.0, true),
        ];
        for (kind, side, trigger, price, refused) in cases {
            let store = fixture(kind, side, Some(trigger));
            assert_eq!(
                refusal(&store, "o", Some(price), 1_000.0, |_| false).is_some(),
                refused,
                "{kind:?} {side:?} {trigger:?} at {price}"
            );
        }
    }

    #[test]
    fn an_unknown_trigger_type_must_pass_both_mark_and_last() {
        let store = fixture(OrderType::StopLoss, OrderSide::Sell, None);
        assert!(refusal(&store, "o", Some(100.5), 1_000.0, |_| false).is_some());
        assert!(refusal(&store, "o", Some(99.0), 1_000.0, |_| false).is_none());
    }

    #[test]
    fn a_stale_ticker_refuses_nothing_but_the_implausible() {
        let store = fixture(OrderType::StopLoss, OrderSide::Sell, Some(StopTriggerType::MarkPrice));
        let later = 1_000.0 + FRESH_TICKER_MS + 1.0;
        assert!(refusal(&store, "o", Some(100.0), later, |_| false).is_none());
        assert!(refusal(&store, "o", Some(1_880.0), later, |_| false).is_some());
        assert!(refusal(&store, "o", Some(40.0), later, |_| false).is_some());
    }

    #[test]
    fn a_reclassifying_venue_keeps_only_the_implausible_guard() {
        let store = fixture(OrderType::StopLoss, OrderSide::Sell, Some(StopTriggerType::MarkPrice));
        assert!(refusal(&store, "o", Some(100.0), 1_000.0, |_| true).is_none());
        assert!(refusal(&store, "o", Some(1_880.0), 1_000.0, |_| true).is_some());
    }

    #[test]
    fn leaves_other_orders_and_missing_prices_alone() {
        let store = fixture(OrderType::Limit, OrderSide::Sell, None);
        assert!(refusal(&store, "o", Some(1.0), 1_000.0, |_| false).is_none());
        let store = fixture(OrderType::StopLoss, OrderSide::Sell, None);
        assert!(refusal(&store, "o", None, 1_000.0, |_| false).is_none());
        assert!(refusal(&store, "missing", Some(100.0), 1_000.0, |_| false).is_none());
    }
}
