use crate::{
    capabilities::{BaseCapabilityMethod, is_inert_capability},
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use serde_json::Value;

pub struct HistoricalPositionOptions {
    pub account: String,
    pub from: Option<f64>,
    pub to: Option<f64>,
}
/// Imported journal folding is bound by its owner; the exchange core never folds venue data itself.
pub trait JournalFolder {
    fn build_folded_position_records<'a>(
        &'a self,
        exchange: &'a dyn Exchange,
        account: &'a str,
        from: f64,
        to: f64,
        symbol_scoped: bool,
        resolve_settle_currency: Box<dyn Fn(&str) -> Result<String, ExchangeError>>,
    ) -> ExchangeFuture<'a, Vec<Value>>;
}
pub async fn fold_closed_position_records(
    venue: &dyn Exchange,
    folder: &dyn JournalFolder,
    options: HistoricalPositionOptions,
    symbol_scoped: bool,
) -> Result<Vec<Value>, ExchangeError> {
    if is_inert_capability(
        &venue.capabilities().fetch_historical_fills,
        BaseCapabilityMethod::FetchHistoricalFills,
    ) {
        return Err(ExchangeError::new(format!(
            "{}: no fills axis, cannot fold journal records",
            venue.class_name()
        )));
    }
    let base = venue.base();
    let to = options.to.unwrap_or_else(|| base.runtime.now_ms());
    let from = options
        .from
        .unwrap_or(to - 30.0 * 24.0 * 60.0 * 60.0 * 1000.0);
    let store = base.store.clone();
    folder
        .build_folded_position_records(
            venue,
            &options.account,
            from,
            to,
            symbol_scoped,
            Box::new(move |symbol| {
                Ok(store
                    .markets_by_symbol()?
                    .get(symbol)
                    .map(|m| m.quote.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "USDT".into()))
            }),
        )
        .await
}
