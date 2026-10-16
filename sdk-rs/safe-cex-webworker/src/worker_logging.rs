use serde_json::{Map, Value, json};

// ts: ExchangeWorker.isRecord; the host materializes objects at the JSON boundary.
pub fn is_record(value: &Value) -> bool {
    value.is_object()
}

pub fn get_worker_sentry_control(error: &Value) -> Value {
    let mut result = Map::new();
    if !is_record(error) {
        return Value::Object(result);
    }
    if let Some(key) = error["safeCexSentryReportKey"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        result.insert("reportKey".into(), json!(key));
    }
    if let Some(ms) = error["safeCexSentryReportCooldownMs"]
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
    {
        result.insert("reportCooldownMs".into(), json!(ms));
    }
    for (source, target) in [
        ("safeCexSentryTags", "tags"),
        ("safeCexSentryExtra", "extra"),
    ] {
        if is_record(&error[source]) {
            result.insert(target.into(), error[source].clone());
        }
    }
    Value::Object(result)
}

pub fn split_worker_log_extra(extra: Option<&Value>) -> Value {
    let Some(Value::Object(extra)) = extra else {
        return json!({"extra":{},"sentryTags":{},"sentryExtra":{},"suppressConsoleLog":false});
    };
    let tags = extra
        .get("sentryTags")
        .filter(|v| is_record(v))
        .cloned()
        .unwrap_or(json!({}));
    let sentry_extra = extra
        .get("sentryExtra")
        .filter(|v| is_record(v))
        .cloned()
        .unwrap_or(json!({}));
    let suppress = safe_cex_num::js_truthy(extra.get("suppressConsoleLog"));
    let plain: Map<String, Value> = extra
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "sentryTags" | "sentryExtra" | "suppressConsoleLog"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    json!({"extra":plain,"sentryTags":tags,"sentryExtra":sentry_extra,"suppressConsoleLog":suppress})
}

// Named form of ExchangeWorker.log's inline literal union; no Deserialize path.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerLogLevel {
    Info,
    Warn,
    Error,
}

// ts: ExchangeWorker.log. The host owns the actual structured-clone/post boundary.
pub fn forward_log<E>(
    exchange_name: safe_cex_types::ExchangeName,
    account_name: &str,
    level: WorkerLogLevel,
    message: &str,
    context: Option<&Map<String, Value>>,
    post: impl FnOnce(crate::payload::StoreWorkerPayload) -> Result<(), E>,
) -> Result<(), E> {
    let mut fields = Map::new();
    fields.insert("exchange".into(), json!(exchange_name));
    fields.insert("account".into(), json!(account_name));
    if let Some(context) = context {
        fields.extend(context.clone());
    }
    // JS objects enumerate integer keys first, including objects nested in context.
    let mut context = Value::Object(fields);
    safe_cex_num::normalize_json_order(&mut context);
    post(crate::payload::StoreWorkerPayload {
        op: crate::payload::StoreWorkerOp::Log,
        instance_id: None,
        data: Some(json!({"level":level,"message":message,"context":context})),
    })
}

/// Runs ExchangeWorker.logError's cooldown and delivery sequence. The host
/// materializes its Error/unknown value before delivery; that operation may fail.
/// The callback tuple is (clock, error materialization, post, console fallback).
// ts: ExchangeWorker.logError
pub fn forward_error<E>(
    cooldowns: &std::cell::RefCell<std::collections::HashMap<String, (f64, f64)>>,
    identity: (safe_cex_types::ExchangeName, &str),
    message: &str,
    error: &Value,
    extra: Option<&Value>,
    send_to_sentry: bool,
    callbacks: (
        impl FnOnce() -> Result<f64, E>,
        impl FnOnce() -> Result<Value, E>,
        impl FnOnce(crate::payload::StoreWorkerPayload) -> Result<(), E>,
        impl FnOnce(&str, &str, &Value, E) -> Result<(), E>,
    ),
) -> Result<(), E> {
    let (clock, materialize_error, post, fallback) = callbacks;
    let control = get_worker_sentry_control(error);
    let split = split_worker_log_extra(extra);
    let mut effective_send = send_to_sentry;
    let mut suppress_console = split["suppressConsoleLog"].as_bool().unwrap_or(false);
    let mut suppressed_reports = 0.0;
    if send_to_sentry
        && let (Some(key), Some(cooldown)) = (
            control["reportKey"].as_str(),
            control["reportCooldownMs"].as_f64(),
        )
    {
        // The clock can re-enter; do not borrow the cooldown map across it.
        let now = clock()?;
        let previous = cooldowns.borrow().get(key).copied();
        let should_report = previous.is_none_or(|(last, _)| now - last >= cooldown);
        let next = if should_report {
            suppressed_reports = previous.map_or(0.0, |(_, count)| count);
            (now, 0.0)
        } else {
            effective_send = false;
            suppress_console = true;
            let (last, count) = previous.unwrap_or((now, 0.0));
            (last, count + 1.0)
        };
        cooldowns.borrow_mut().insert(key.to_owned(), next);
    }

    // In the source this is outside try: a coercion/getter failure propagates
    // after the cooldown mutation, without posting or invoking console fallback.
    let serializable_error = materialize_error()?;
    let mut tags = Map::from_iter([
        ("source".into(), json!("webworker")),
        ("module".into(), json!("ExchangeWorker")),
        ("exchange".into(), json!(identity.0)),
        ("account".into(), json!(identity.1)),
    ]);
    for value in [&split["sentryTags"], &control["tags"]] {
        if let Some(object) = value.as_object() {
            tags.extend(object.clone());
        }
    }
    let mut extra = Map::new();
    for value in [&split["extra"], &split["sentryExtra"], &control["extra"]] {
        if let Some(object) = value.as_object() {
            extra.extend(object.clone());
        }
    }
    if suppressed_reports > 0.0 {
        extra.insert(
            "suppressedRepeatedReports".into(),
            json!(suppressed_reports),
        );
    }
    let mut data = json!([message, {
        "sendToSentry": effective_send,
        "suppressConsoleLog": suppress_console,
        "sentryData": {"error": serializable_error, "tags": tags, "extra": extra}
    }]);
    safe_cex_num::normalize_json_order(&mut data);
    let payload = crate::payload::StoreWorkerPayload {
        op: crate::payload::StoreWorkerOp::Error,
        instance_id: None,
        data: Some(data),
    };
    match post(payload) {
        Ok(()) => Ok(()),
        Err(cause) => fallback(
            "[ExchangeWorker] Failed to send error to main thread:",
            message,
            error,
            cause,
        ),
    }
}
