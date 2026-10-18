use safe_cex_exchange::{exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_store::{StoreMutationError, StoreUpdate};
use safe_cex_types::Balance;

// ts: BinanceExchange._startPublicDataOnly
pub fn start_binance_public_data_only(
    base: &BaseExchange,
) -> Result<(), Box<dyn std::error::Error>> {
    start_public_data_only(base)
}
// ts: BinancePMExchange._startPublicDataOnly
pub fn start_binance_pm_public_data_only(
    base: &BaseExchange,
) -> Result<(), Box<dyn std::error::Error>> {
    start_public_data_only(base)
}
// ts: BinanceSpotExchange._startPublicDataOnly
pub fn start_binance_spot_public_data_only(
    base: &BaseExchange,
) -> Result<(), Box<dyn std::error::Error>> {
    start_public_data_only(base)
}

fn start_public_data_only(base: &BaseExchange) -> Result<(), Box<dyn std::error::Error>> {
    if base.is_disposed() {
        return Ok(());
    }
    base.log("Ready to watch public data on Binance", "info")
        .map_err(|error: ExchangeError| -> Box<dyn std::error::Error> { Box::new(error) })?;
    let mut loaded = base.store.loaded()?.clone();
    loaded.orders = true;
    loaded.balance = true;
    loaded.positions = true;
    let result: Result<(), StoreMutationError> = base.store.update(
        StoreUpdate {
            orders: Some(vec![]),
            balance: Some(Balance {
                used: 0.0,
                free: 0.0,
                total: 0.0,
                upnl: 0.0,
                info: None,
                currencies: None,
                coins: None,
            }),
            positions: Some(vec![]),
            loaded: Some(loaded),
            ..Default::default()
        },
        false,
    );
    result.map_err(|error: StoreMutationError| -> Box<dyn std::error::Error> { Box::new(error) })
}
