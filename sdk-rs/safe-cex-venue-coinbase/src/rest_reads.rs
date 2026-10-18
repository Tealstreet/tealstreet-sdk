use safe_cex_types::Ticker;
use safe_cex_types::retained_json::RetainedJsonInput as Input;

// ts: CoinbaseExchange.formatSpotCfmTicker
pub fn format_spot_cfm_tickers(data: &Input) -> Result<Vec<Ticker>, String> {
    let products = data
        .get("products")
        .and_then(Input::as_array)
        .ok_or_else(|| "products must be an array".to_owned())?;

    products
        .iter()
        .map(|product| {
            let price_value = product.get("price");
            let price = if price_value == Some(&Input::Text(vec![])) {
                0.0
            } else {
                parse_js_float(price_value)?
            };
            let details = product.get("future_product_details");
            let raw_contract_size = details.and_then(|value| value.get("contract_size"));
            let contract_size = if raw_contract_size.is_some_and(Input::is_truthy) {
                parse_js_float(raw_contract_size)?
            } else {
                1.0
            };

            let percentage_value = product.get("price_percentage_change_24h");
            let percentage = if percentage_value == Some(&Input::Text(vec![])) {
                0.0
            } else {
                parse_js_float(percentage_value)?
            };

            let volume_value = product.get("volume_24h");
            let volume = if volume_value == Some(&Input::Text(vec![])) {
                0.0
            } else {
                parse_js_float(volume_value)?
            };
            let quote_volume = safe_cex_num::display_round(price * volume * contract_size);

            let open_interest_value = details
                .and_then(|value| value.get("perpetual_details"))
                .and_then(|value| value.get("open_interest"));
            let open_interest = if open_interest_value == Some(&Input::Text(vec![])) {
                0.0
            } else {
                parse_js_float(open_interest_value)? * contract_size
            };

            let product_id = product
                .get("product_id")
                .and_then(Input::as_text)
                .and_then(|v| String::from_utf16(v).ok())
                .ok_or_else(|| "product_id must be a string".to_owned())?
                .to_owned();

            Ok::<Ticker, String>(Ticker {
                id: product_id.clone(),
                symbol: product_id,
                bid: price,
                ask: price,
                last: price,
                mark: price,
                index: Some(price),
                mid_price: None,
                percentage,
                open_interest,
                funding_rate: 0.0,
                funding_time: 0.0,
                volume,
                quote_volume,
                timestamp: None,
                info: None,
            })
        })
        .collect()
}

fn parse_js_float(value: Option<&Input>) -> Result<f64, String> {
    match value {
        None => Ok(f64::NAN),
        Some(value) => {
            safe_cex_num::js_retained_parse_float(Some(value)).map_err(|error| error.to_string())
        }
    }
}
