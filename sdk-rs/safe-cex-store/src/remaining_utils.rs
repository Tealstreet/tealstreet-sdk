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
    let message = Value::String(message.unwrap_or("").into());
    JsonOutcome(Some(&message))
        .outcome_test(NOT_SENT_PATTERN, false)
        .expect("string source test")
}
/// Property and coercion capabilities for the shared lost-send decision.
pub trait UnknownOutcomeValue: Sized {
    type Error;
    fn outcome_object(&self) -> bool;
    fn outcome_get(&self, key: &str) -> Result<Self, Self::Error>;
    fn outcome_nullish(&self) -> bool;
    fn outcome_truthy(&self) -> bool;
    fn outcome_number(&self) -> Result<f64, Self::Error>;
    fn outcome_at_least(&self, minimum: f64) -> Result<bool, Self::Error>;
    fn outcome_network_code(&self) -> bool;
    fn outcome_test(&self, pattern: &str, ignore_case: bool) -> Result<bool, Self::Error>;
}

const NOT_SENT_PATTERN: &str = r"(Signing request \S+ timed out|Remote signer unavailable|Failed to send signing request|signing aborted)";
const UNKNOWN_OUTCOME_PATTERN: &str = r"timeout|timed out|socket hang up|network error|network (connection|request) failed|tradetunnel|connection (lost|closed|reset)|failed to fetch|send status unknown|econnreset|econnaborted|etimedout|status code 5\d\d|bad gateway|gateway time-?out|service (temporarily )?unavailable|upstream";

pub fn is_unknown_outcome_value<V: UnknownOutcomeValue>(error: &V) -> Result<bool, V::Error> {
    if !error.outcome_object() {
        return Ok(false);
    }
    // Source destructuring gets all four properties before any coercion.
    let code = error.outcome_get("code")?;
    let message = error.outcome_get("message")?;
    let name = error.outcome_get("name")?;
    let response = error.outcome_get("response")?;
    if message.outcome_test(NOT_SENT_PATTERN, false)? {
        return Ok(false);
    }
    if response.outcome_truthy() {
        let status = response.outcome_get("status")?;
        if !status.outcome_nullish() && status.outcome_at_least(500.0)? {
            return Ok(true);
        }
        let data = response.outcome_get("data")?;
        return Ok(
            !data.outcome_nullish() && data.outcome_get("code")?.outcome_number()? == -1007.0
        );
    }
    Ok(code.outcome_network_code()
        || message.outcome_test(UNKNOWN_OUTCOME_PATTERN, true)?
        || name.outcome_test("tradetunnel", true)?)
}

struct JsonOutcome<'a>(Option<&'a Value>);
impl UnknownOutcomeValue for JsonOutcome<'_> {
    type Error = String;
    fn outcome_object(&self) -> bool {
        self.0
            .is_some_and(|value| value.is_object() || value.is_array())
    }
    fn outcome_get(&self, key: &str) -> Result<Self, String> {
        Ok(Self(self.0.and_then(|value| value.get(key))))
    }
    fn outcome_nullish(&self) -> bool {
        self.0.is_none_or(Value::is_null)
    }
    fn outcome_truthy(&self) -> bool {
        js_truthy(self.0)
    }
    fn outcome_number(&self) -> Result<f64, String> {
        js_number(self.0).map_err(|error| format!("{error:?}"))
    }
    fn outcome_at_least(&self, minimum: f64) -> Result<bool, String> {
        Ok(self.outcome_number()? >= minimum)
    }
    fn outcome_network_code(&self) -> bool {
        self.0.and_then(Value::as_str).is_some_and(|code| {
            ["ECONNRESET", "ECONNABORTED", "ETIMEDOUT", "ERR_NETWORK"].contains(&code)
        })
    }
    fn outcome_test(&self, pattern: &str, ignore_case: bool) -> Result<bool, String> {
        let text = js_string(
            self.0
                .filter(|v| !v.is_null())
                .or(Some(&Value::String(String::new()))),
        )
        .map_err(|error| format!("{error:?}"))?;
        // Match JS's non-Unicode case folding and exact whitespace set.
        let pattern = pattern.replace(r"\d", "[0-9]").replace(r"\S", r"[^\t\n\v\f\r \u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]");
        let pattern = if ignore_case {
            format!("(?i-u:{pattern})")
        } else {
            pattern
        };
        Ok(regex::Regex::new(&pattern)
            .expect("source regex")
            .is_match(&text))
    }
}

pub fn is_unknown_outcome(error: &Value) -> Result<bool, String> {
    is_unknown_outcome_value(&JsonOutcome(Some(error)))
}
