// ts: BinanceSpotExchange.getAssetUsdPrice
pub fn get_asset_usd_price<E>(
    asset: &str,
    mut ticker_last: impl FnMut(&str) -> Result<Option<f64>, E>,
) -> Result<f64, E> {
    if matches!(asset, "USD" | "USDT" | "USDC") {
        return Ok(1.0);
    }
    for quote in ["USDT", "USDC", "USD"] {
        if let Some(last) = ticker_last(&format!("{asset}{quote}"))?
            && last != 0.0
            && !last.is_nan()
        {
            return Ok(last);
        }
    }
    Ok(0.0)
}
