//! Shared book snapshot/delta state over the single retained handler graph.
use safe_cex_exchange::js_runtime::{ops, value::Js};
use safe_cex_types::{OrderBook, OrderBookOrders};

pub(crate) struct BookState {
    pub book: OrderBook,
    pub last_update: Js,
    pub resyncing: bool,
}
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Publish,
    Resync,
    Suppress,
}
impl BookState {
    pub(crate) fn new() -> Self {
        Self {
            book: OrderBook {
                bids: vec![],
                asks: vec![],
            },
            last_update: Js::Number(0.),
            resyncing: false,
        }
    }
    pub(crate) fn apply(&mut self, frame: &Js) -> Result<Outcome, String> {
        let data = frame.get("data").map_err(fault)?;
        if frame
            .get("type")
            .map_err(fault)?
            .strict_equals(&Js::str("snapshot"))
        {
            self.book.bids.clear();
            self.book.asks.clear();
            self.last_update = update_id(&data)?;
            self.apply_sides(&data, true)?;
        }
        if frame
            .get("type")
            .map_err(fault)?
            .strict_equals(&Js::str("delta"))
        {
            let update = update_id(&data)?;
            if ops::greater_than(&self.last_update, &Js::Number(0.)).map_err(fault)?
                && exceeds_next(&update, &self.last_update)?
            {
                if self.resyncing {
                    return Ok(Outcome::Suppress);
                }
                self.resyncing = true;
                self.last_update = Js::Number(0.);
                self.book.bids.clear();
                self.book.asks.clear();
                return Ok(Outcome::Resync);
            }
            self.last_update = update;
            self.apply_sides(&data, false)?;
        }
        safe_cex_store::orderbook::calc_order_book_total(&mut self.book);
        Ok(Outcome::Publish)
    }
    fn apply_sides(&mut self, data: &Js, snapshot: bool) -> Result<(), String> {
        for (key, bids) in [("a", false), ("b", true)] {
            let rows = optional(data, key).map_err(fault)?;
            if !rows.truthy() {
                continue;
            }
            let mut index = 0;
            while ops::less_than(
                &Js::Number(index as f64),
                &rows.get("length").map_err(fault)?,
            )
            .map_err(fault)?
            {
                let row = rows.get(&index.to_string()).map_err(fault)?;
                index += 1;
                let price = row.get("0").map_err(fault)?.parse_float().map_err(fault)?;
                let amount = row.get("1").map_err(fault)?.parse_float().map_err(fault)?;
                if snapshot {
                    let levels = if bids {
                        &mut self.book.bids
                    } else {
                        &mut self.book.asks
                    };
                    levels.push(OrderBookOrders {
                        price,
                        amount,
                        total: 0.,
                    });
                } else {
                    safe_cex_store::update_order_book::update_order_book_by_price(
                        &mut self.book,
                        bids,
                        price,
                        amount,
                        true,
                    );
                }
            }
        }
        Ok(())
    }
}
fn fault(error: safe_cex_exchange::js_runtime::value::Thrown) -> String {
    error.to_exchange_error().message
}
fn optional(value: &Js, key: &str) -> Result<Js, safe_cex_exchange::js_runtime::value::Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
fn update_id(data: &Js) -> Result<Js, String> {
    let value = optional(data, "u").map_err(fault)?;
    Ok(if value.truthy() {
        value
    } else {
        Js::Number(0.)
    })
}
fn exceeds_next(update: &Js, last: &Js) -> Result<bool, String> {
    let last = ops::to_primitive(last).map_err(fault)?;
    let next = ops::add(&last, &Js::Number(1.)).map_err(fault)?;
    ops::greater_than(update, &next).map_err(fault)
}
#[cfg(test)]
mod tests;
