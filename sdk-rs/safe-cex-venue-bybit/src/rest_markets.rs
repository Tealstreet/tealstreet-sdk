//! Inverse and spot request sequence; the caller supplies the venue's existing client.
use crate::{
    api::truthy,
    market_mapping::{Family, map_market},
};
use safe_cex_types::Market;
use serde_json::{Value, json};
use std::future::Future;

pub async fn fetch_markets_helper<F, Fut>(
    category: &str,
    inverse: bool,
    public_data_only: bool,
    mut get: F,
    mut emit_error: impl FnMut(Option<&Value>) -> Result<(), String>,
) -> Result<Vec<Option<Market>>, String>
where
    F: FnMut(&'static str, Value) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    // No shared mutation or owned task: dropping this read cancels only this invocation.
    let fees = if inverse && !public_data_only {
        get("/v5/account/fee-rate", json!({"category":category}))
            .await
            .ok()
            .and_then(|v| v.get("result").and_then(|r| r.get("list")).cloned())
            .filter(truthy)
            .unwrap_or_else(|| json!([]))
    } else {
        json!([])
    };
    let data = get(
        "/v5/market/instruments-info",
        json!({"category":category,"limit":1000}),
    )
    .await?;
    // Inverse deliberately fetches PreLaunch before inspecting the first response.
    let prelaunch = if inverse {
        Some(
            get(
                "/v5/market/instruments-info",
                json!({"category":category,"limit":1000,"status":"PreLaunch"}),
            )
            .await?,
        )
    } else {
        None
    };
    for response in std::iter::once(&data).chain(prelaunch.iter()) {
        if !response.is_null() && !response.is_object() && !response.is_array() {
            let value =
                safe_cex_num::js_string(Some(response)).map_err(|e| e.message.to_owned())?;
            return Err(format!(
                "Cannot use 'in' operator to search for 'retMsg' in {value}"
            ));
        }
        let ret_msg = response.get("retMsg").or_else(|| response.get("ret_msg"));
        if ret_msg != Some(&Value::String("OK".into())) {
            emit_error(ret_msg)?;
            return Ok(vec![None]);
        }
    }
    let mut rows = vec![];
    for (response_index, response) in std::iter::once(&data).chain(prelaunch.iter()).enumerate() {
        let result = response
            .get("result")
            .filter(|v| !v.is_null())
            .ok_or_else(|| {
                format!(
                    "Cannot read properties of {} (reading 'list')",
                    if response.get("result").is_some() {
                        "null"
                    } else {
                        "undefined"
                    }
                )
            })?;
        let list = result.get("list");
        let list = list.and_then(Value::as_array).ok_or_else(|| {
            if inverse {
                format!(
                    "{}.result.list is not iterable",
                    if response_index == 0 { "data" } else { "data2" }
                )
            } else if list.is_none() || list == Some(&Value::Null) {
                format!(
                    "Cannot read properties of {} (reading 'map')",
                    if list.is_none() { "undefined" } else { "null" }
                )
            } else {
                "data.result.list.map is not a function".into()
            }
        })?;
        rows.extend(list.iter());
    }
    let mut mapped = Vec::with_capacity(rows.len());
    for row in rows {
        let fees = fees.as_array().ok_or("fees.find is not a function")?;
        mapped.push(map_market(
            &row.clone().into(),
            &fees.iter().cloned().map(Into::into).collect::<Vec<_>>(),
            category,
            if inverse {
                Family::Inverse
            } else {
                Family::Spot
            },
        )?);
    }
    let mut admitted = Vec::with_capacity(mapped.len());
    for market in mapped {
        match market {
            Ok(market) => admitted.push(Some(market)),
            Err(message) => emit_error(Some(&Value::String(message)))?,
        }
    }
    Ok(admitted)
}
#[cfg(test)]
mod tests;
