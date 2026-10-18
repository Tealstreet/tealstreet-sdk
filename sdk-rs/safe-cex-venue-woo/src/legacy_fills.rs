//! Legacy history query names; these deliberately differ from V3.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq)]
pub struct WooLegacyFillsQuery {
    pub symbol: Option<String>,
    pub from_milliseconds: Option<f64>,
    pub to_milliseconds: Option<f64>,
    pub limit: f64,
    pub page: Option<f64>,
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooLegacyFillsQuery", rename_all = "camelCase")]
struct QueryWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    symbol: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    from_milliseconds: Option<f64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    to_milliseconds: Option<f64>,
    limit: f64,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    page: Option<f64>,
}
impl Serialize for WooLegacyFillsQuery {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        QueryWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooLegacyFillsQuery {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|decoder| {
                QueryWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WooLegacyFillsParams {
    pub size: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_t: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_t: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
}
// ts: buildWooLegacyFillsParams
pub fn build_woo_legacy_fills_params(query: &WooLegacyFillsQuery) -> WooLegacyFillsParams {
    WooLegacyFillsParams {
        size: query.limit,
        symbol: query.symbol.clone().filter(|s| !s.is_empty()),
        start_t: query.from_milliseconds,
        end_t: query.to_milliseconds,
        page: query.page,
    }
}

// Explicit registrations keep the complete output graph visible to parity indexing.
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooLegacyFillsQuery {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, from_milliseconds, to_milliseconds, limit, page });
    }
}

fn native_params(params: WooLegacyFillsParams) -> Result<serde_json::Value, serde_json::Error> {
    use safe_cex_types::retained_json::RetainedJsonInput;
    let mut values = vec![(
        "size".encode_utf16().collect(),
        RetainedJsonInput::Float(params.size),
    )];
    if let Some(symbol) = params.symbol {
        values.push((
            "symbol".encode_utf16().collect(),
            RetainedJsonInput::Text(symbol.encode_utf16().collect()),
        ));
    }
    for (key, value) in [
        ("start_t", params.start_t),
        ("end_t", params.end_t),
        ("page", params.page),
    ] {
        if let Some(value) = value {
            values.push((
                key.encode_utf16().collect(),
                RetainedJsonInput::Float(value),
            ));
        }
    }
    RetainedJsonInput::Object(values).into_value()
}

// ts: WOOXExchange#fetchLegacyTransactionHistoryRows
pub async fn fetch_legacy_transaction_history_rows(
    adapter: &dyn safe_cex_transport::RestAdapter,
    query: &WooLegacyFillsQuery,
) -> Result<Vec<serde_json::Value>, safe_cex_transport::RestFailure> {
    use safe_cex_transport::{RestFailure, RestRequest};
    let mut candidates: Vec<Option<String>> = query
        .symbol
        .as_deref()
        .filter(|symbol| !symbol.is_empty())
        .map(crate::response::get_transaction_history_symbol_candidates)
        .unwrap_or_default()
        .into_iter()
        .map(Some)
        .collect();
    candidates.push(None);
    let mut last_error = None;
    for symbol in candidates {
        let mut attempt = query.clone();
        attempt.symbol = symbol;
        let mut request = RestRequest {
            method: "GET".into(),
            url: "/v1/client/trades".into(),
            ..RestRequest::default()
        };
        match native_params(build_woo_legacy_fills_params(&attempt)) {
            Ok(params) => request.params = Some(params),
            Err(error) => {
                last_error = Some(RestFailure::pre_transmit(error.to_string(), request));
                continue;
            }
        }
        match adapter.execute(request.clone()).await {
            Ok(response) => {
                let data = match response.body.into_data() {
                    safe_cex_types::retained_json::RetainedJsonInput::Text(text) => {
                        safe_cex_types::retained_json::RetainedJsonInput::parse_json(&text)
                            .unwrap_or(safe_cex_types::retained_json::RetainedJsonInput::Null)
                    }
                    native => native,
                };
                let data = data
                    .into_value()
                    .map_err(|error| RestFailure::pre_transmit(error.to_string(), request))?;
                let rows = crate::response::parse_transaction_rows_from_response(&data);
                if !rows.is_empty() {
                    return Ok(rows.to_vec());
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(Vec::new()),
    }
}
