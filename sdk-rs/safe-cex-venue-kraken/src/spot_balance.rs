//! Spot balance read using shared Store, asset normalization and balance models.
use crate::{spot_types, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    TableHit, js_own_keys, js_string, js_truthy, numeric_ingress::JsNumberInput, parse_float,
};
use safe_cex_store::Store;
use safe_cex_types::{Balance, object_map::JsObjectMap};
use serde_json::{Value, json};
use std::future::Future;

pub async fn fetch_balance<F>(
    store: &dyn Store,
    read: impl FnOnce(&'static str) -> Result<F, ExchangeError>,
    emit_error: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<Balance, ExchangeError>
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
{
    let outcome: Result<Balance, ExchangeError> = async {
        let endpoint = spot_types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "BALANCE")
            .expect("balance endpoint")
            .1;
        let data = read(endpoint)?
            .await?
            .map(wire::decode)
            .transpose()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let balances = data
            .as_ref()
            .and_then(|d| d.get("result"))
            .filter(|v| js_truthy(Some(v)))
            .cloned()
            .unwrap_or(json!({}));
        let entries: Vec<(String, Value)> = match &balances {
            Value::Object(object) => js_own_keys(object.keys().map(String::as_str))
                .into_iter()
                .map(|k| (k.to_owned(), object[k].clone()))
                .collect(),
            Value::Array(array) => array
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
            Value::String(string) => string
                .encode_utf16()
                .enumerate()
                .map(|(i, c)| String::from_utf16(&[c]).map(|s| (i.to_string(), Value::String(s))))
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    ExchangeError::new("Kraken Spot balance UTF-16 unit is outside UTF-8 profile")
                })?,
            _ => Vec::new(),
        };
        let mut currencies = JsObjectMap::new();
        let mut total_usd = 0.0;
        let mut free_usd = 0.0;
        for (asset, balance) in entries {
            let input = if js_truthy(Some(&balance)) {
                Some(&balance)
            } else {
                None
            };
            let text = match input {
                Some(v) => js_string(Some(v)).map_err(|e| ExchangeError::new(e.message))?,
                None => "0".into(),
            };
            let amount = parse_float(&text);
            if amount <= 0.0 {
                continue;
            }
            if asset.contains('.') {
                continue;
            }
            let units: Vec<u16> = asset.encode_utf16().collect();
            let normalized = match spot_types::normalize_asset(&units) {
                TableHit::Mapped(s) => String::from_utf16(&s).expect("UTF-8 asset input"),
                TableHit::Inherited(member) => member.js_string().into(),
                TableHit::Missing => unreachable!("normalize returns input"),
            };
            currencies.insert(
                normalized.clone(),
                Balance {
                    free: amount,
                    used: 0.0,
                    total: amount,
                    upnl: 0.0,
                    info: Some(json!({"raw":asset,"balance":balance}).into()),
                    currencies: None,
                    coins: None,
                },
            );
            if ["USD", "USDT", "USDC"].contains(&normalized.as_str()) {
                total_usd += amount;
                free_usd += amount;
            }
        }
        for (asset, info) in &currencies {
            if ["USD", "USDT", "USDC"].contains(&asset.as_str()) {
                continue;
            }
            let usd = format!("{asset}/USD");
            let usdt = format!("{asset}/USDT");
            if let Some(ticker) = store
                .tickers()
                .iter()
                .find(|t| t.symbol == usd || t.symbol == usdt)
            {
                if ticker.last > 0.0 {
                    let value = info.total * ticker.last;
                    total_usd += value;
                    free_usd += value;
                }
            }
        }
        Ok(Balance {
            free: free_usd,
            used: 0.0,
            total: total_usd,
            upnl: 0.0,
            currencies: Some(
                currencies
                    .into_iter()
                    .map(|(key, balance)| (key, balance.into()))
                    .collect(),
            ),
            coins: None,
            info: Some(balances.into()),
        })
    }
    .await;
    match outcome {
        Ok(balance) => Ok(balance),
        Err(error) => {
            // ts-bug: error emission precedes the current Store fallback; emitter failure propagates.
            emit_error(if error.message.is_empty() {
                "Failed to fetch balance"
            } else {
                &error.message
            })?;
            Ok(store.balance().clone())
        }
    }
}
