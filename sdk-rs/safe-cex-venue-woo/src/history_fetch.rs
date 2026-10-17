//! Transaction history request ordering over the supplied REST adapter.
use crate::legacy_fills::{WooLegacyFillsQuery, fetch_legacy_transaction_history_rows};
use safe_cex_transport::{RestAdapter, RestFailure, RestRequest};
use safe_cex_types::retained_json::RetainedJsonInput;
use serde_json::Value;

async fn request_rows(
    adapter: &dyn RestAdapter,
    query: &WooLegacyFillsQuery,
    symbol: Option<String>,
) -> Result<Vec<Value>, RestFailure> {
    let mut values = vec![(
        "limit".encode_utf16().collect(),
        RetainedJsonInput::Float(query.limit),
    )];
    if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
        values.push((
            "symbol".encode_utf16().collect(),
            RetainedJsonInput::Text(symbol.encode_utf16().collect()),
        ));
    }
    for (key, value) in [
        ("startTime", query.from_milliseconds),
        ("endTime", query.to_milliseconds),
    ] {
        if let Some(value) = value {
            values.push((
                key.encode_utf16().collect(),
                RetainedJsonInput::Float(value),
            ));
        }
    }
    let mut request = RestRequest {
        method: "GET".into(),
        url: "/v3/trade/transactionHistory".into(),
        ..RestRequest::default()
    };
    request.params = Some(match RetainedJsonInput::Object(values).into_value() {
        Ok(params) => params,
        Err(error) => return Err(RestFailure::pre_transmit(error.to_string(), request)),
    });
    let response = adapter.execute(request.clone()).await?;
    let data = match response.body.into_data() {
        RetainedJsonInput::Text(text) => {
            RetainedJsonInput::parse_json(&text).unwrap_or(RetainedJsonInput::Null)
        }
        native => native,
    };
    let data = data
        .into_value()
        .map_err(|error| RestFailure::pre_transmit(error.to_string(), request))?;
    Ok(crate::response::parse_transaction_rows_from_response(&data).to_vec())
}

// ts: WOOXExchange#fetchTransactionHistoryRows
pub async fn fetch_transaction_history_rows(
    adapter: &dyn RestAdapter,
    query: &WooLegacyFillsQuery,
) -> Result<Vec<Value>, RestFailure> {
    let Some(symbol) = query.symbol.as_deref().filter(|s| !s.is_empty()) else {
        return request_rows(adapter, query, None).await;
    };
    let mut candidates: Vec<_> = crate::response::get_transaction_history_symbol_candidates(symbol)
        .into_iter()
        .map(Some)
        .collect();
    candidates.push(None);
    let mut last_error = None;
    for candidate in candidates {
        match request_rows(adapter, query, candidate).await {
            Ok(rows) if !rows.is_empty() => return Ok(rows),
            Ok(_) => {}
            Err(error) => last_error = Some(error),
        }
    }
    let rows = fetch_legacy_transaction_history_rows(adapter, query).await?;
    if !rows.is_empty() {
        return Ok(rows);
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(Vec::new()),
    }
}
