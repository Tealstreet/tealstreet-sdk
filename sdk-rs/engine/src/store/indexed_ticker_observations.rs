use super::{Store, ticker_values};
use crate::model::Ticker;

impl Store {
    /// Update a known ticker and record explicitly reported fields at the same clock.
    /// Equivalent to update_ticker_at followed by observe_ticker_fields, without
    /// another symbol lookup or recording a changed reference timestamp twice.
    pub fn update_ticker_with_observations_at(
        &mut self,
        index: u32,
        observed: u16,
        update: impl FnOnce(&mut Ticker),
    ) {
        let Some(ticker) = self.tickers.get_mut(index as usize) else {
            return;
        };
        #[cfg(debug_assertions)]
        let symbol = ticker.symbol.clone();
        let before = ticker_values(ticker);
        update(ticker);
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            ticker.symbol, symbol,
            "ticker identity changes require set_tickers"
        );
        let after = ticker_values(ticker);
        let mut mask = 0u16;
        for (bit, (a, b)) in before.iter().zip(after.iter()).enumerate() {
            if a.to_bits() != b.to_bits() {
                mask |= 1 << bit;
            }
        }
        let seen = mask | observed;
        if seen != 0 {
            self.mark_references_seen(index, seen);
            self.mark_observations_seen(index, seen);
        }
        if mask == 0 {
            return;
        }
        self.ticker_revision = self.ticker_revision.wrapping_add(1);
        let slot = &mut self.ticker_masks[index as usize];
        if *slot == 0 {
            self.dirty_tickers.push(index);
        }
        *slot |= mask;
    }
}
