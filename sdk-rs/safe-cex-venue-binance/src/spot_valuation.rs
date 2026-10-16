// ts: BinanceSpotExchange.getAssetUsdPrice
pub fn get_asset_usd_price(asset: &str, mut ticker_last: impl FnMut(&str) -> Option<f64>) -> f64 {
    if matches!(asset, "USD" | "USDT" | "USDC") {
        return 1.0;
    }
    for quote in ["USDT", "USDC", "USD"] {
        if let Some(last) = ticker_last(&format!("{asset}{quote}"))
            && last != 0.0
            && !last.is_nan()
        {
            return last;
        }
    }
    0.0
}
