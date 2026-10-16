//! Process-local REST aggregates with ordered partial writes and getter failures.
use indexmap::IndexMap;
use regex::{Captures, Regex};
use safe_cex_num::{js_number, js_number_string, js_string, js_trim, math_round};
use safe_cex_types::object_map::JsObjectMap;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

const WINDOW_MS: f64 = 300_000.0;
const QUOTA_HEADERS: &[&str] = &[
    "x-bapi-limit",
    "x-bapi-limit-status",
    "x-bapi-limit-reset-timestamp",
    "x-mbx-used-weight-1m",
    "ratelimit-remaining",
    "retry-after",
];
const ORIGIN_HEADERS: &[&str] = &["server", "via", "x-cache", "x-amz-cf-id", "cf-ray"];

type HeaderGetter = Rc<dyn Fn(&str) -> Result<Option<Value>, String>>;

/// Axios-style accessor returns undefined/null as None; Err models a thrown getter.
#[derive(Clone, Default)]
pub struct DiagnosticHeaders {
    pub values: IndexMap<String, Value>,
    pub get: Option<HeaderGetter>,
}
/// Typed projection keeps non-finite diagnostic numbers, unlike a JSON-only config.
#[derive(Clone, Default)]
pub struct RestDiagnosticConfig {
    pub url: Option<String>,
    pub base_url: Option<String>,
    pub headers: DiagnosticHeaders,
    pub numbers: IndexMap<String, f64>,
    pub proxy_header: Option<String>,
    /// Existing modeled properties remain writable; new properties are refused.
    pub sealed: bool,
}
#[derive(Clone, Default)]
pub struct RestDiagnosticResponse {
    pub config: Option<RestDiagnosticConfig>,
    pub status: Option<f64>,
    pub headers: DiagnosticHeaders,
    pub data: Option<Value>,
}
#[derive(Clone, Default)]
pub struct RestDiagnosticError {
    pub config: Option<RestDiagnosticConfig>,
    pub response: Option<RestDiagnosticResponse>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub name: Option<String>,
}
#[derive(Debug, Clone)]
pub struct VenueRestFailureSample {
    pub at: f64,
    pub exchange: String,
    pub path: Option<String>,
    pub status: Option<f64>,
    pub code: Option<String>,
    pub total_ms: Option<f64>,
    pub sign_ms: Option<f64>,
    pub signature_age_ms: Option<f64>,
    pub body_kind: String,
    pub body_snippet: Option<String>,
    pub rest_proxy_header_sent: Option<String>,
    pub error_name: Option<String>,
    pub error_message: Option<String>,
    pub endpoint: Option<String>,
    pub headers: JsObjectMap<String>,
}
#[derive(Debug, Clone, Default)]
pub struct ExchangeDiagnostics {
    pub requests: f64,
    pub ok: f64,
    pub failures: f64,
    pub timestamps: Vec<f64>,
    pub total_ms: Vec<f64>,
    pub sign_ms: Vec<f64>,
    pub min_quota_remaining: Option<f64>,
    pub last_quota: Option<JsObjectMap<String>>,
    pub status_counts: JsObjectMap<f64>,
    pub code_counts: JsObjectMap<f64>,
    pub error_counts: JsObjectMap<f64>,
    pub last_endpoint: Option<String>,
    pub max_signature_age_ms: Option<f64>,
}
#[derive(Debug, Clone)]
pub struct VenueRestStats {
    pub requests: f64,
    pub ok: f64,
    pub failures: f64,
    pub per_minute: f64,
    pub tracked_samples: usize,
    pub total_p50_ms: Option<f64>,
    pub total_p95_ms: Option<f64>,
    pub sign_p50_ms: Option<f64>,
    pub sign_p95_ms: Option<f64>,
    pub min_quota_remaining: Option<f64>,
    pub last_quota: Option<JsObjectMap<String>>,
    pub max_signature_age_ms: Option<f64>,
    pub status_counts: JsObjectMap<f64>,
    pub code_counts: JsObjectMap<f64>,
    pub error_counts: JsObjectMap<f64>,
    pub last_endpoint: Option<String>,
}
#[derive(Default)]
struct State {
    by_exchange: IndexMap<String, Rc<RefCell<ExchangeDiagnostics>>>,
    failure_samples: Vec<VenueRestFailureSample>,
}
fn with_state<T>(operation: impl FnOnce(&mut State) -> T) -> T {
    thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
    STATE.with(|state| operation(&mut state.borrow_mut()))
}
pub fn empty_diagnostics() -> ExchangeDiagnostics {
    ExchangeDiagnostics::default()
}
pub fn for_exchange(
    by_exchange: &mut IndexMap<String, Rc<RefCell<ExchangeDiagnostics>>>,
    exchange: &str,
) -> Rc<RefCell<ExchangeDiagnostics>> {
    by_exchange
        .entry(exchange.into())
        .or_insert_with(|| Rc::new(RefCell::new(empty_diagnostics())))
        .clone()
}
pub fn push(values: &mut Vec<f64>, value: f64) {
    values.push(value);
    if values.len() > 200 {
        values.drain(..values.len() - 200);
    }
}
pub fn note_timestamp(diagnostics: &mut ExchangeDiagnostics, now: f64) {
    diagnostics.timestamps.push(now);
    let cutoff = now - WINDOW_MS;
    if diagnostics.timestamps[0] >= cutoff {
        return;
    }
    diagnostics.timestamps.retain(|at| *at >= cutoff);
}
pub fn bump(counts: &mut JsObjectMap<f64>, key: &str) {
    counts.insert(key.into(), counts.get(key).copied().unwrap_or(0.0) + 1.0);
}
pub fn read_config_number(config: Option<&RestDiagnosticConfig>, key: &str) -> Option<f64> {
    config.and_then(|config| config.numbers.get(key).copied())
}
fn slice_utf16(text: &str, limit: usize) -> String {
    String::from_utf16_lossy(&text.encode_utf16().take(limit).collect::<Vec<_>>())
}
pub fn path_of(url: Option<&str>) -> Option<String> {
    let path = url?.split('?').next()?;
    (!path.is_empty()).then(|| slice_utf16(path, 80))
}
pub fn looks_like_secret(run: &str) -> bool {
    run.encode_utf16().count() >= 32
        || (run.bytes().any(|c| c.is_ascii_digit()) && run.bytes().any(|c| c.is_ascii_alphabetic()))
}
pub fn scrub(text: &str) -> String {
    let re = Regex::new(r"[A-Za-z0-9_-]{12,}").expect("literal regex");
    let text = re.replace_all(text, |caps: &Captures<'_>| {
        if looks_like_secret(&caps[0]) {
            "<redacted>".into()
        } else {
            caps[0].to_owned()
        }
    });
    js_trim(&slice_utf16(&text, 200)).to_owned()
}
pub fn read_headers(
    source: &DiagnosticHeaders,
    names: &[&str],
) -> Result<JsObjectMap<String>, String> {
    let lower: IndexMap<_, _> = source
        .values
        .iter()
        .map(|(key, value)| (key.to_lowercase(), value))
        .collect();
    let mut out = JsObjectMap::new();
    for name in names {
        let from_get = match &source.get {
            Some(get) => get(name)?,
            None => None,
        };
        let value = from_get
            .as_ref()
            .filter(|value| !value.is_null())
            .or_else(|| lower.get(*name).copied());
        if let Some(value) = value.filter(|value| !value.is_null()) {
            out.insert(
                (*name).into(),
                slice_utf16(
                    &js_string(Some(value)).map_err(|err| format!("{err:?}"))?,
                    120,
                ),
            );
        }
    }
    Ok(out)
}
pub fn classify_body(data: Option<&Value>) -> (String, Option<String>) {
    let Some(data) = data.filter(|value| !value.is_null() && value.as_str() != Some("")) else {
        return ("empty".into(), None);
    };
    if data.is_object() || data.is_array() {
        let venue = ["retCode", "retMsg", "code"]
            .iter()
            .any(|key| data.get(*key).is_some());
        return (if venue { "venue-json" } else { "json" }.into(), None);
    }
    let text = js_string(Some(data)).expect("JSON primitive");
    let lower = text.to_lowercase();
    let kind = if lower.contains("cloudfront") {
        "cloudfront"
    } else if lower.contains("<html") || lower.contains("<!doctype") {
        "html"
    } else {
        "text"
    };
    (kind.into(), Some(scrub(&text)))
}
pub fn normalize_error_key(message: &str) -> String {
    let scrubbed = scrub(message);
    let digits = Regex::new(r"[0-9]+")
        .expect("literal regex")
        .replace_all(&scrubbed, "#")
        .into_owned();
    let spaces = Regex::new(r"[\t\n\v\f\r \u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]+").expect("literal regex").replace_all(&digits, " ").into_owned();
    let key = slice_utf16(js_trim(&spaces), 120);
    if key.is_empty() {
        "unknown".into()
    } else {
        key
    }
}
pub fn bump_bounded(counts: &mut JsObjectMap<f64>, key: &str) {
    if !counts.contains_key(key) && counts.len() >= 20 {
        bump(counts, "other");
    } else {
        bump(counts, key);
    }
}
pub fn endpoint_of(config: Option<&RestDiagnosticConfig>) -> Option<String> {
    let config = config?;
    let url = config.url.as_deref().unwrap_or("");
    let lower = url.to_ascii_lowercase();
    let absolute = if lower.starts_with("http://") || lower.starts_with("https://") {
        url
    } else {
        config.base_url.as_deref().unwrap_or("")
    };
    (!absolute.is_empty()).then(|| slice_utf16(absolute.split('?').next().unwrap_or(""), 120))
}
pub fn mark_rest_proxy_header<'a>(
    config: &'a mut RestDiagnosticConfig,
    shape: &str,
) -> &'a mut RestDiagnosticConfig {
    if !config.sealed || config.proxy_header.is_some() {
        config.proxy_header = Some(shape.into());
    }
    config
}
pub fn proxy_header_of(config: Option<&RestDiagnosticConfig>) -> Result<Option<String>, String> {
    let Some(config) =
        config.filter(|config| matches!(config.proxy_header.as_deref(), Some("present" | "empty")))
    else {
        return Ok(None);
    };
    let sent = read_headers(
        &config.headers,
        &["authorization", "tealstreet-authorization"],
    )?;
    let value = sent
        .get("authorization")
        .filter(|value| !value.is_empty())
        .or_else(|| {
            sent.get("tealstreet-authorization")
                .filter(|value| !value.is_empty())
        });
    Ok(Some(
        match value {
            None => "absent",
            Some(value) if value.starts_with("Bearer ") => "bearer",
            _ => "malformed",
        }
        .into(),
    ))
}
pub fn mark_rest_request_start(
    config: &mut RestDiagnosticConfig,
    now: f64,
) -> &mut RestDiagnosticConfig {
    if !config.sealed || config.numbers.contains_key("safeCexRestDiagStartedAt") {
        config
            .numbers
            .insert("safeCexRestDiagStartedAt".into(), now);
    }
    config
}
pub fn mark_rest_signed(
    config: &mut RestDiagnosticConfig,
    sign_ms: f64,
    now: f64,
) -> &mut RestDiagnosticConfig {
    let _ = stamp_signed(config, sign_ms, now);
    config
}
pub fn stamp_signed(
    config: &mut RestDiagnosticConfig,
    sign_ms: f64,
    now: f64,
) -> Result<(), String> {
    if config.sealed && !config.numbers.contains_key("safeCexRestDiagSignMs") {
        return Err("sealed config".into());
    }
    config
        .numbers
        .insert("safeCexRestDiagSignMs".into(), sign_ms);
    if config.sealed && !config.numbers.contains_key("safeCexRestDiagSignedAt") {
        return Err("sealed config".into());
    }
    config.numbers.insert("safeCexRestDiagSignedAt".into(), now);
    let stamped = match &config.headers.get {
        Some(get) => get("x-bapi-timestamp")?,
        None => config
            .headers
            .values
            .get("X-BAPI-TIMESTAMP")
            .filter(|value| !value.is_null())
            .or_else(|| config.headers.values.get("x-bapi-timestamp"))
            .cloned(),
    };
    let stamped_ms = js_number(stamped.as_ref()).map_err(|err| format!("{err:?}"))?;
    if stamped_ms.is_finite() && stamped_ms > 0.0 {
        if config.sealed && !config.numbers.contains_key("safeCexRestDiagSignatureAgeMs") {
            return Err("sealed config".into());
        }
        config
            .numbers
            .insert("safeCexRestDiagSignatureAgeMs".into(), now - stamped_ms);
    }
    Ok(())
}
pub fn note_rest_success(exchange: &str, response: Option<&RestDiagnosticResponse>, now: f64) {
    let cell = with_state(|state| for_exchange(&mut state.by_exchange, exchange));
    {
        let mut diagnostics = cell.borrow_mut();
        let config = response.and_then(|response| response.config.as_ref());
        let started_at = read_config_number(config, "safeCexRestDiagStartedAt");
        diagnostics.requests += 1.0;
        diagnostics.ok += 1.0;
        note_timestamp(&mut diagnostics, now);
        if let Some(started_at) = started_at {
            push(&mut diagnostics.total_ms, now - started_at);
        }
        if let Some(sign_ms) = read_config_number(config, "safeCexRestDiagSignMs") {
            push(&mut diagnostics.sign_ms, sign_ms);
        }
        if let Some(endpoint) = endpoint_of(config).filter(|value| !value.is_empty()) {
            diagnostics.last_endpoint = Some(endpoint);
        }
        drop(diagnostics);
        let Some(headers) = response.map(|response| &response.headers) else {
            return;
        };
        let Ok(quota) = read_headers(headers, QUOTA_HEADERS) else {
            return;
        };
        let mut diagnostics = cell.borrow_mut();
        if !quota.is_empty() {
            let remaining = safe_cex_num::number_from_string(
                quota
                    .get("x-bapi-limit-status")
                    .or_else(|| quota.get("ratelimit-remaining"))
                    .map(String::as_str)
                    .unwrap_or("undefined"),
            );
            diagnostics.last_quota = Some(quota);
            if remaining.is_finite() {
                diagnostics.min_quota_remaining = Some(
                    diagnostics
                        .min_quota_remaining
                        .map_or(remaining, |old| safe_cex_num::math_min(old, remaining)),
                );
            }
        }
    }
}
pub fn note_rest_failure(exchange: &str, error: Option<&RestDiagnosticError>, now: f64) {
    let cell = with_state(|state| for_exchange(&mut state.by_exchange, exchange));
    {
        let mut diagnostics = cell.borrow_mut();
        let config = error.and_then(|error| error.config.as_ref());
        let response = error.and_then(|error| error.response.as_ref());
        let total_ms = read_config_number(config, "safeCexRestDiagStartedAt").map(|at| now - at);
        let sign_ms = read_config_number(config, "safeCexRestDiagSignMs");
        let signature_age_ms = read_config_number(config, "safeCexRestDiagSignatureAgeMs");
        let status = response.and_then(|response| response.status);
        let code = error.and_then(|error| error.code.clone());
        diagnostics.requests += 1.0;
        diagnostics.failures += 1.0;
        note_timestamp(&mut diagnostics, now);
        if let Some(total_ms) = total_ms {
            push(&mut diagnostics.total_ms, total_ms);
        }
        if let Some(sign_ms) = sign_ms {
            push(&mut diagnostics.sign_ms, sign_ms);
        }
        if let Some(age) = signature_age_ms {
            diagnostics.max_signature_age_ms = Some(safe_cex_num::math_max(
                diagnostics.max_signature_age_ms.unwrap_or(0.0),
                age,
            ));
        }
        bump(
            &mut diagnostics.status_counts,
            &status.map_or_else(|| "none".into(), js_number_string),
        );
        if let Some(code) = code.as_deref().filter(|value| !value.is_empty()) {
            bump(&mut diagnostics.code_counts, code);
        }
        let message = error
            .and_then(|error| error.message.as_deref())
            .filter(|value| !value.is_empty());
        if response.is_none()
            && code.as_deref().is_none_or(str::is_empty)
            && let Some(message) = message
        {
            bump_bounded(&mut diagnostics.error_counts, &normalize_error_key(message));
        }
        let (body_kind, body_snippet) =
            classify_body(response.and_then(|response| response.data.as_ref()));
        drop(diagnostics);
        let Ok(rest_proxy_header_sent) = proxy_header_of(config) else {
            return;
        };
        let mut headers = JsObjectMap::new();
        if let Some(response) = response {
            let Ok(origin) = read_headers(&response.headers, ORIGIN_HEADERS) else {
                return;
            };
            let Ok(quota) = read_headers(&response.headers, QUOTA_HEADERS) else {
                return;
            };
            headers.extend(origin);
            headers.extend(quota);
        }
        with_state(|state| {
            state.failure_samples.push(VenueRestFailureSample {
                at: now,
                exchange: exchange.into(),
                path: path_of(config.and_then(|config| config.url.as_deref())),
                status,
                code,
                total_ms,
                sign_ms,
                signature_age_ms,
                body_kind,
                body_snippet,
                rest_proxy_header_sent,
                endpoint: endpoint_of(config),
                error_name: error.and_then(|error| error.name.clone()),
                error_message: message.map(scrub),
                headers,
            });
            if state.failure_samples.len() > 25 {
                state
                    .failure_samples
                    .drain(..state.failure_samples.len() - 25);
            }
        });
    }
}
pub fn percentile(values: &[f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    // Stable insertion sort models the JS comparator's NaN => equal behavior without
    // requiring a total order (sort_by's ordering contract would be violated).
    for i in 1..sorted.len() {
        let mut j = i;
        while j > 0 && sorted[j] - sorted[j - 1] < 0.0 {
            sorted.swap(j, j - 1);
            j -= 1;
        }
    }
    let index = safe_cex_num::math_min(
        (sorted.len() - 1) as f64,
        (sorted.len() as f64 * fraction).floor(),
    );
    if index.is_nan() || index < 0.0 {
        return Some(f64::NAN);
    }
    Some(math_round(sorted[index as usize]))
}
pub fn get_venue_rest_stats(now: f64) -> JsObjectMap<VenueRestStats> {
    with_state(|state| {
        state
            .by_exchange
            .iter()
            .map(|(exchange, diagnostics)| {
                let diagnostics = diagnostics.borrow();
                let recent = diagnostics
                    .timestamps
                    .iter()
                    .filter(|at| **at >= now - WINDOW_MS)
                    .count();
                (
                    exchange.clone(),
                    VenueRestStats {
                        requests: diagnostics.requests,
                        ok: diagnostics.ok,
                        failures: diagnostics.failures,
                        per_minute: math_round(recent as f64 / 5.0 * 10.0) / 10.0,
                        tracked_samples: diagnostics.timestamps.len(),
                        total_p50_ms: percentile(&diagnostics.total_ms, 0.5),
                        total_p95_ms: percentile(&diagnostics.total_ms, 0.95),
                        sign_p50_ms: percentile(&diagnostics.sign_ms, 0.5),
                        sign_p95_ms: percentile(&diagnostics.sign_ms, 0.95),
                        min_quota_remaining: diagnostics.min_quota_remaining,
                        last_quota: diagnostics.last_quota.clone(),
                        max_signature_age_ms: diagnostics.max_signature_age_ms,
                        status_counts: diagnostics.status_counts.clone(),
                        code_counts: diagnostics.code_counts.clone(),
                        error_counts: diagnostics.error_counts.clone(),
                        last_endpoint: diagnostics.last_endpoint.clone(),
                    },
                )
            })
            .collect()
    })
}
pub fn drain_venue_rest_failure_samples() -> Vec<VenueRestFailureSample> {
    with_state(|state| std::mem::take(&mut state.failure_samples))
}
pub fn reset_venue_rest_diagnostics() {
    with_state(|state| {
        state.by_exchange.clear();
        state.failure_samples.clear();
    });
}
