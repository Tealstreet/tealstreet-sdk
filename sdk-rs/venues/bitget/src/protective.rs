use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, StopTriggerType};
use safe_cex_engine::{ErrorKind, Outcome, Store, VenueError};

pub(crate) struct Guard {
    pub sources: Vec<Order>,
    pub trailing: Vec<(String, (String, f64))>,
    pub target: String,
    pub price: f64,
    pub size: f64,
    pub entry_price: Option<f64>,
    pub hedged: bool,
    pub frame_fed: bool,
    pub quote: Option<Quote>,
}

#[derive(Default)]
pub(crate) struct Quote {
    pub last: Option<(f64, f64)>,
    pub mark: Option<(f64, f64)>,
}
impl Quote {
    pub fn update(&mut self, last: Option<f64>, mark: Option<f64>, at: f64) {
        if let Some(price) = last {
            self.last = Some((price, at));
        }
        if let Some(price) = mark {
            self.mark = Some((price, at));
        }
    }
    fn fresh(&self, reference: StopTriggerType, now: f64) -> Option<f64> {
        let (price, at) = match reference {
            StopTriggerType::LastPrice => self.last,
            StopTriggerType::MarkPrice => self.mark,
            StopTriggerType::IndexPrice => None,
        }?;
        (price.is_finite() && price > 0.0 && at <= now && now - at <= 5_000.0).then_some(price)
    }
}

pub(crate) fn refusal() -> VenueError {
    VenueError::new(
        "Bitget protective edit could not be validated; no cancel sent, refresh before editing",
    )
    .kind(ErrorKind::InvalidInput)
    .outcome(Outcome::NotSent)
}

impl Guard {
    pub fn validate(
        &self,
        store: &Store,
        now: f64,
        observed: Option<&Quote>,
    ) -> Result<(), VenueError> {
        let source = self.sources.first().ok_or_else(refusal)?;
        let market = store.market(&source.symbol).ok_or_else(refusal)?;
        if store.hedged() != self.hedged
            || !now.is_finite()
            || !self.size.is_finite()
            || self.size < 0.0
            || (self.size > 0.0
                && (self.size < market.limits.amount.min
                    || self.size > market.limits.amount.max
                    || safe_cex_engine::num::adjust(self.size, market.precision.amount)
                        != self.size))
        {
            return Err(refusal());
        }
        for original in &self.sources {
            let current = store.order(&original.id).ok_or_else(refusal)?;
            if !matches!(
                current.status,
                OrderStatus::Open | OrderStatus::Updating | OrderStatus::Canceling
            ) || current.symbol != original.symbol
                || current.kind != original.kind
                || current.side != original.side
                || current.parent_id != original.parent_id
                || current.price != original.price
                || current.amount != original.amount
                || current.filled != original.filled
                || current.reduce_only != original.reduce_only
                || current.trigger_type != original.trigger_type
                || current.extra != original.extra
            {
                return Err(refusal());
            }
        }
        let pending_entry = self
            .sources
            .iter()
            .find(|o| o.kind == OrderType::Limit && o.filled == 0.0);
        for source in self
            .sources
            .iter()
            .filter(|o| crate::account::is_algo(o.kind))
        {
            let reference = reference(source).ok_or_else(refusal)?;
            let quote = self.fresh(observed, reference, now).ok_or_else(refusal)?;
            let price = if source.id == self.target {
                self.price
            } else {
                source.price
            };
            if !price.is_finite() || price <= 0.0 || price / quote > 10.0 || quote / price > 10.0 {
                return Err(refusal());
            }
            if source.kind != OrderType::TrailingStopLoss {
                let standing = pending_entry.map_or(quote, |entry| {
                    if entry.id == self.target {
                        self.price
                    } else {
                        entry.price
                    }
                });
                let fires = match (source.side, source.kind) {
                    (OrderSide::Sell, OrderType::StopLoss)
                    | (OrderSide::Buy, OrderType::TakeProfit) => price >= standing,
                    _ => price <= standing,
                };
                if fires {
                    return Err(refusal());
                }
            }
        }
        Ok(())
    }

    fn fresh(&self, observed: Option<&Quote>, reference: StopTriggerType, now: f64) -> Option<f64> {
        self.quote
            .as_ref()
            .or(if self.frame_fed { None } else { observed })?
            .fresh(reference, now)
    }
}

pub(crate) fn reference(source: &Order) -> Option<StopTriggerType> {
    source.trigger_type.or_else(|| {
        source.extra.as_ref().and_then(|extra| match source.kind {
            OrderType::StopLoss => extra.sl_trigger_type,
            OrderType::TakeProfit => extra.tp_trigger_type,
            _ => None,
        })
    })
}
