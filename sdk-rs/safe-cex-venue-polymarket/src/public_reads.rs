//! Public method bodies over an injected shared HTTP client. Factory remains pending.
use safe_cex_exchange::http::HttpClient;
use safe_cex_num::numeric_ingress::{JsNumberOutput, js_from_str, js_from_value_ref};
use safe_cex_transport::{RestBody, RestFailure, RestRequest, RestResponseBody};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

use crate::types::{PolymarketClobMarket, PolymarketOrderBook, PolymarketPrice};

fn endpoint(key: &str) -> String {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == key)
        .expect("endpoint declaration")
        .1
        .into()
}

/// Source `PolymarketAPI.getMarkets`; records remain wire metadata.
pub async fn get_markets(client: &HttpClient) -> Result<Vec<PolymarketClobMarket>, RestFailure> {
    read(
        client,
        RestRequest {
            method: "get".into(),
            url: endpoint("MARKETS"),
            ..RestRequest::default()
        },
    )
    .await
}

/// Source `PolymarketAPI.getOrderBook`.
pub async fn get_order_book(
    client: &HttpClient,
    token_id: &str,
) -> Result<PolymarketOrderBook, RestFailure> {
    read(
        client,
        RestRequest {
            method: "get".into(),
            url: endpoint("ORDER_BOOK"),
            params: Some(json!({"token_id": token_id})),
            ..RestRequest::default()
        },
    )
    .await
}

/// Source `PolymarketAPI.getOrderBooks`; POST is the source read endpoint.
pub async fn get_order_books(
    client: &HttpClient,
    token_ids: &[String],
) -> Result<Vec<PolymarketOrderBook>, RestFailure> {
    read(
        client,
        RestRequest {
            method: "post".into(),
            url: endpoint("ORDER_BOOKS"),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(
                    token_ids
                        .iter()
                        .map(|token_id| json!({"token_id": token_id}))
                        .collect::<Vec<_>>()
                ))
                .expect("bounded UTF-8 token-ID request"),
            )),
            ..RestRequest::default()
        },
    )
    .await
}

/// Source `PolymarketAPI.getPrices`; preserve comma-join behavior, including empties.
pub async fn get_prices(
    client: &HttpClient,
    token_ids: &[String],
) -> Result<Vec<PolymarketPrice>, RestFailure> {
    read(
        client,
        RestRequest {
            method: "get".into(),
            url: endpoint("PRICES"),
            params: Some(json!({"token_ids": token_ids.join(",")})),
            ..RestRequest::default()
        },
    )
    .await
}

// Decode CURRENT adapter data without guessing a second transform. The typed
// projection deliberately refuses unchecked shapes and retains refusal metadata.
async fn read<T: DeserializeOwned + Serialize + JsNumberOutput>(
    client: &HttpClient,
    request: RestRequest,
) -> Result<T, RestFailure> {
    let response = client.execute(request.clone()).await?;
    let decoded = match &response.body {
        RestResponseBody::Encoded(bytes) => std::str::from_utf8(bytes)
            .map_err(|error| error.to_string())
            .and_then(|text| js_from_str::<T>(text).map_err(|error| error.to_string())),
        RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => value
            .clone()
            .into_value()
            .and_then(|value| js_from_value_ref::<T>(&value))
            .map_err(|error| error.to_string()),
    };
    decoded.map_err(|message| RestFailure {
        code: None,
        transport_error: None,
        message,
        response: Some(Box::new(response)),
        request: Box::new(request),
        context: None,
    })
}

/// Source `PolymarketAPI.getServerTime`; JSON number ingress is explicit.
pub async fn get_server_time(client: &HttpClient) -> Result<f64, RestFailure> {
    let request = RestRequest {
        method: "get".into(),
        url: crate::constants::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "TIME")
            .expect("TIME declaration")
            .1
            .into(),
        ..RestRequest::default()
    };
    read(client, request).await
}
