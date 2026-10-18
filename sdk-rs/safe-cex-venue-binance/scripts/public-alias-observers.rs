// Read every native Ticker field before transporting assertion results as JSON.
pub fn ticker(row: &safe_cex_types::Ticker) -> serde_json::Value {
    fn bits(value: f64) -> String {
        format!("{:016x}", value.to_bits())
    }
    serde_json::json!({
        "id":row.id,"symbol":row.symbol,
        "bid":bits(row.bid),"ask":bits(row.ask),"last":bits(row.last),
        "mark":bits(row.mark),"index":bits(row.index.expect("numeric ticker source profile")),
        "midPrice":row.mid_price.map(bits),"percentage":bits(row.percentage),
        "openInterest":bits(row.open_interest),"fundingRate":bits(row.funding_rate),
        "fundingTime":bits(row.funding_time),"volume":bits(row.volume),
        "quoteVolume":bits(row.quote_volume),"timestamp":row.timestamp.map(bits),
        "infoOwn":row.info.is_some(),"info":row.info,
    })
}
