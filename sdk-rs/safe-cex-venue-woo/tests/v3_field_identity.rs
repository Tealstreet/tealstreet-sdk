#[path = "support/field_identity.rs"]
mod identity;
use safe_cex_venue_woo::v3_public::*;
use serde_json::Value;

macro_rules! strings {
    ($value:ident, $input:ident, $($field:ident => $wire:literal),+ $(,)?) => {$(
        assert_eq!($value.$field, $input[$wire].as_str().unwrap(), concat!(stringify!($field), " native field identity"));
    )+};
}
macro_rules! numbers {
    ($value:ident, $input:ident, $($field:ident => $wire:literal),+ $(,)?) => {$(
        assert_eq!($value.$field.to_bits(), $input[$wire].as_f64().unwrap().to_bits(), concat!(stringify!($field), " native field identity"));
    )+};
}
#[test]
fn public_native_field_identity() {
    let records: Value = serde_json::from_str(&identity::capture("public")).unwrap();
    let input = &records["WooV3FuturesRow"];
    let value: WooV3FuturesRow = serde_json::from_value(input.clone()).unwrap();
    strings!(value, input, symbol => "symbol", index_price => "indexPrice", mark_price => "markPrice", est_funding_rate => "estFundingRate", last_funding_rate => "lastFundingRate", open_interest => "openInterest", open_24h => "24hOpen", close_24h => "24hClose", high_24h => "24hHigh", low_24h => "24hLow", volume_24h => "24hVolume", amount_24h => "24hAmount");
    numbers!(value, input, next_funding_time => "nextFundingTime");
    let input = &records["WooV3OrderBookLevel"];
    let value: WooV3OrderBookLevel = serde_json::from_value(input.clone()).unwrap();
    strings!(value, input, price => "price", quantity => "quantity");
    let input = &records["WooV3KlineRow"];
    let value: WooV3KlineRow = serde_json::from_value(input.clone()).unwrap();
    strings!(value, input, open => "open", close => "close", low => "low", high => "high", volume => "volume", amount => "amount", symbol => "symbol", r#type => "type");
    numbers!(value, input, start_timestamp => "startTimestamp", end_timestamp => "endTimestamp");
    let input = &records["WooV3FuturesStats"];
    let value: WooV3FuturesStats = serde_json::from_value(input.clone()).unwrap();
    numbers!(value, input, index => "index", mark => "mark", open24h => "open24h", close24h => "close24h", high24h => "high24h", low24h => "low24h", percentage24h => "percentage24h", open_interest => "openInterest", estimated_funding_rate => "estimatedFundingRate", last_funding_rate => "lastFundingRate", funding_time => "fundingTime", volume24h => "volume24h", quote_volume24h => "quoteVolume24h");
    assert_eq!(
        value.id.as_deref(),
        input["id"].as_str(),
        "id native field identity"
    );
    assert_eq!(
        value.symbol.as_deref(),
        input["symbol"].as_str(),
        "symbol native field identity"
    );
    let input = &records["WooV3OrderBookSnapshot"];
    let value: WooV3OrderBookSnapshot = serde_json::from_value(input.clone()).unwrap();
    numbers!(value, input, timestamp => "timestamp");
    for (side, wire) in [(&value.asks, "asks"), (&value.bids, "bids")] {
        let expected = input[wire].as_array().unwrap();
        assert_eq!(side.len(), expected.len(), "book side length identity");
        for (value, input) in side.iter().zip(expected) {
            strings!(value, input, price => "price", quantity => "quantity");
        }
    }
    let input = &records["WooV3MappedOrderBookSnapshot"];
    let value: WooV3MappedOrderBookSnapshot = serde_json::from_value(input.clone()).unwrap();
    numbers!(value, input, timestamp => "timestamp");
    for (side, wire) in [(&value.book.asks, "asks"), (&value.book.bids, "bids")] {
        let expected = input[wire].as_array().unwrap();
        assert_eq!(
            side.len(),
            expected.len(),
            "mapped book side length identity"
        );
        for (value, input) in side.iter().zip(expected) {
            numbers!(value, input, price => "price", amount => "amount", total => "total");
        }
    }
}

#[test]
fn request_flags_and_nested_meta_native_identity() {
    use safe_cex_venue_woo::v3_contracts::*;
    let fixtures: Value = serde_json::from_str(&identity::capture("contracts")).unwrap();
    for case in fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["accepted"] == true)
    {
        let input = &case["input"];
        match case["name"].as_str().unwrap() {
            "WooV3PlaceOrderRequest" => {
                let value: WooV3PlaceOrderRequest = serde_json::from_value(input.clone()).unwrap();
                assert_eq!(
                    value.reduce_only,
                    input["reduceOnly"].as_bool(),
                    "reduce_only native field identity"
                );
                assert_eq!(
                    value.post_only_adjusted,
                    input["postOnlyAdjusted"].as_bool(),
                    "post_only_adjusted native field identity"
                );
                assert_eq!(
                    value.order_tag.as_deref(),
                    input["orderTag"].as_str(),
                    "order_tag native field identity"
                );
                strings!(value, input, symbol => "symbol");
                for (actual, wire) in [
                    (value.price.as_deref(), "price"),
                    (value.quantity.as_deref(), "quantity"),
                    (value.amount.as_deref(), "amount"),
                    (value.visible_quantity.as_deref(), "visibleQuantity"),
                ] {
                    assert_eq!(actual, input[wire].as_str(), "{wire} native field identity");
                }
            }
            "WooV3PageMeta" => {
                let value: WooV3PageMeta = serde_json::from_value(input.clone()).unwrap();
                numbers!(value, input, total => "total", records_per_page => "recordsPerPage", current_page => "currentPage");
            }
            "WooV3Rows" => {
                let value: WooV3Rows<Vec<String>> = serde_json::from_value(input.clone()).unwrap();
                assert_eq!(
                    value.rows,
                    serde_json::from_value::<Vec<Vec<String>>>(input["rows"].clone()).unwrap(),
                    "rows native field identity"
                );
                assert_eq!(
                    value.meta.is_some(),
                    input.get("meta").is_some(),
                    "meta presence identity"
                );
                if let Some(value) = value.meta {
                    let input = &input["meta"];
                    numbers!(value, input, total => "total", records_per_page => "recordsPerPage", current_page => "currentPage");
                }
            }
            _ => {}
        }
    }
}
