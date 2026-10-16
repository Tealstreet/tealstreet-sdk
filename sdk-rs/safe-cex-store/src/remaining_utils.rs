//! Remaining pure utilities. Runtime capabilities live in the exchange/crypto layers.
use safe_cex_num::{js_number, js_string, js_truthy};
use serde_json::Value;
use std::{collections::HashSet, rc::Rc};

pub const EXCHANGES_WITHOUT_SECONDARY_KEY: &[&str] = &[
    "apexomni",
    "breakoutprop",
    "grvt",
    "hyperliquid",
    "hyperliquidspot",
    "lighter",
    "lighter-robinhood",
    "paradex",
    "polymarket",
    "woofi",
    "kodiakfi",
    "mode",
    "salsadex",
    "tealstreet-dex",
    "txflow",
    "what",
];
pub fn exchange_supports_secondary_key(exchange: &str) -> bool {
    !EXCHANGES_WITHOUT_SECONDARY_KEY.contains(&exchange)
}
pub fn is_development(node_env: Option<&str>) -> bool {
    node_env != Some("production")
}
pub fn is_node_or_bun_runtime(process: Option<&Value>) -> bool {
    process.is_some_and(|process| {
        js_truthy(
            process
                .get("versions")
                .and_then(|versions| versions.get("node")),
        ) || js_truthy(process.get("isBun"))
    })
}
pub const SIGNING_CAPTURE_FLAG: &str = "__tealstreetSigningCapture";
pub fn is_signing_capture_request(config: Option<&Value>) -> bool {
    js_truthy(config.and_then(|config| config.get(SIGNING_CAPTURE_FLAG)))
}

/// Rc retains the source update/existing object identity, including duplicate IDs.
pub fn merge_updates_using_last_update<T>(
    updates: &[Rc<T>],
    existing: &[Rc<T>],
    now: f64,
    cutoff: f64,
    id: impl Fn(&T) -> &str,
    last_update: impl Fn(&T) -> Option<f64>,
) -> Vec<Rc<T>> {
    let safe_updates = updates
        .iter()
        .map(|update| {
            let Some(found) = existing.iter().find(|entry| id(entry) == id(update)) else {
                return update.clone();
            };
            if !last_update(found).is_some_and(|time| time != 0.0 && !time.is_nan()) {
                return update.clone();
            }
            let time = last_update(found).unwrap_or(f64::NAN);
            if now - time > cutoff {
                update.clone()
            } else {
                found.clone()
            }
        })
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    safe_updates
        .into_iter()
        .chain(existing.iter().cloned())
        .filter(|entry| seen.insert(id(entry).to_owned()))
        .collect()
}

/// ts: unknownSendOutcome.isUnsentSigningFailure. A hosted request that failed while
/// getting its signature never reached the venue.
pub fn is_unsent_signing_failure(message: Option<&str>) -> bool {
    regex::Regex::new(r"(Signing request [^\t\n\v\f\r \u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]+ timed out|Remote signer unavailable|Failed to send signing request|signing aborted)")
        .expect("source regex")
        .is_match(message.unwrap_or(""))
}
pub fn is_unknown_outcome(error: &Value) -> Result<bool, String> {
    use regex::Regex;
    if !error.is_object() && !error.is_array() {
        return Ok(false);
    }
    let string = |value: Option<&Value>| {
        js_string(
            value
                .filter(|value| !value.is_null())
                .or(Some(&Value::String(String::new()))),
        )
        .map_err(|error| format!("{error:?}"))
    };
    let message = string(error.get("message"))?;
    if is_unsent_signing_failure(Some(&message)) {
        return Ok(false);
    }
    if let Some(response) = error
        .get("response")
        .filter(|response| js_truthy(Some(response)))
    {
        let status = response.get("status").filter(|value| !value.is_null());
        let status = match status {
            Some(status) => js_number(Some(status)),
            None => Ok(0.0),
        }
        .map_err(|error| format!("{error:?}"))?;
        if status >= 500.0 {
            return Ok(true);
        }
        let code = response.get("data").and_then(|data| data.get("code"));
        return Ok(js_number(code).map_err(|error| format!("{error:?}"))? == -1007.0);
    }
    if error
        .get("code")
        .and_then(Value::as_str)
        .is_some_and(|code| {
            ["ECONNRESET", "ECONNABORTED", "ETIMEDOUT", "ERR_NETWORK"].contains(&code)
        })
    {
        return Ok(true);
    }
    let unknown = Regex::new(r"(?i-u:timeout|timed out|socket hang up|network error|network (connection|request) failed|tradetunnel|connection (lost|closed|reset)|failed to fetch|send status unknown|econnreset|econnaborted|etimedout|status code 5[0-9][0-9]|bad gateway|gateway time-?out|service (temporarily )?unavailable|upstream)").expect("source regex");
    if unknown.is_match(&message) {
        return Ok(true);
    }
    Ok(Regex::new("(?i-u:tradetunnel)")
        .expect("source regex")
        .is_match(&string(error.get("name"))?))
}
