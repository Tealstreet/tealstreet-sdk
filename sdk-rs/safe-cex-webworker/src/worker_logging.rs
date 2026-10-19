use crate::{data_post::WorkerDataPayload, payload::StoreWorkerOp};
use safe_cex_exchange::js_runtime::{
    ops,
    value::{Js, Thrown, from_typed},
};

pub fn is_record(value: &Js) -> bool {
    value.is_object() && !value.is_callable() && !value.is_array()
}

pub fn get_worker_sentry_control(error: &Js) -> Result<Js, Thrown> {
    if !is_record(error) {
        return Ok(Js::object(vec![]));
    }
    let key = error.get("safeCexSentryReportKey")?;
    let key = if key.is_string() && error.get("safeCexSentryReportKey")?.truthy() {
        error.get("safeCexSentryReportKey")?
    } else {
        Js::Undefined
    };
    let ms = error.get("safeCexSentryReportCooldownMs")?;
    let ms = if matches!(ms, Js::Number(_))
        && matches!(error.get("safeCexSentryReportCooldownMs")?, Js::Number(n) if n.is_finite())
        && ops::greater_than(
            &error.get("safeCexSentryReportCooldownMs")?,
            &Js::Number(0.0),
        )? {
        error.get("safeCexSentryReportCooldownMs")?
    } else {
        Js::Undefined
    };
    let tags = error.get("safeCexSentryTags")?;
    let tags = if is_record(&tags) {
        error.get("safeCexSentryTags")?
    } else {
        Js::Undefined
    };
    let extra = error.get("safeCexSentryExtra")?;
    let extra = if is_record(&extra) {
        error.get("safeCexSentryExtra")?
    } else {
        Js::Undefined
    };
    Ok(Js::object(vec![
        ("reportKey", key),
        ("reportCooldownMs", ms),
        ("tags", tags),
        ("extra", extra),
    ]))
}

pub fn split_worker_log_extra(extra: Option<&Js>) -> Result<Js, Thrown> {
    let empty = Js::Undefined;
    let extra = extra.unwrap_or(&empty);
    if !is_record(extra) {
        return Ok(Js::object(vec![
            ("extra", Js::object(vec![])),
            ("sentryTags", Js::object(vec![])),
            ("sentryExtra", Js::object(vec![])),
            ("suppressConsoleLog", Js::Bool(false)),
        ]));
    }
    let tags = extra.get("sentryTags")?;
    let tags = if is_record(&tags) {
        extra.get("sentryTags")?
    } else {
        Js::object(vec![])
    };
    let sentry_extra = extra.get("sentryExtra")?;
    let sentry_extra = if is_record(&sentry_extra) {
        extra.get("sentryExtra")?
    } else {
        Js::object(vec![])
    };
    let suppress = extra.get("suppressConsoleLog")?.truthy();
    let plain = Js::object(vec![]);
    // Object.entries evaluates every enumerable getter before filtering the keys.
    for (key, value) in extra.own_entries()? {
        if !["sentryTags", "sentryExtra", "suppressConsoleLog"]
            .iter()
            .any(|name| key.as_ref() == name.encode_utf16().collect::<Vec<_>>().as_slice())
        {
            plain.define_key(&key, value)?;
        }
    }
    Ok(Js::object(vec![
        ("extra", plain),
        ("sentryTags", tags),
        ("sentryExtra", sentry_extra),
        ("suppressConsoleLog", Js::Bool(suppress)),
    ]))
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerLogLevel {
    Info,
    Warn,
    Error,
}

fn spread_into(target: &Js, value: &Js) -> Result<(), Thrown> {
    if !value.is_nullish() {
        for (key, value) in value.own_entries()? {
            target.define_key(&key, value)?;
        }
    }
    Ok(())
}

pub fn forward_log<E: From<Thrown>>(
    exchange_name: safe_cex_types::ExchangeName,
    account_name: &str,
    level: WorkerLogLevel,
    message: &Js,
    context: Option<&Js>,
    post: impl FnOnce(WorkerDataPayload) -> Result<(), E>,
) -> Result<(), E> {
    let fields = Js::object(vec![
        ("exchange", from_typed(&exchange_name)?),
        ("account", Js::str(account_name)),
    ]);
    if let Some(context) = context {
        spread_into(&fields, context)?;
    }
    post(WorkerDataPayload {
        op: StoreWorkerOp::Log,
        data: Js::object(vec![
            ("level", from_typed(&level)?),
            ("message", message.clone()),
            ("context", fields),
        ]),
    })
}

pub fn forward_error<E: From<Thrown>>(
    cooldowns: &Js,
    identity: (safe_cex_types::ExchangeName, &str),
    message: &Js,
    error: &Js,
    extra: Option<&Js>,
    send_to_sentry: bool,
    callbacks: (
        impl FnOnce() -> Result<f64, E>,
        impl FnOnce() -> Result<Js, E>,
        impl FnOnce(WorkerDataPayload) -> Result<(), E>,
        impl FnOnce(&str, &Js, &Js, E) -> Result<(), E>,
    ),
) -> Result<(), E> {
    let (clock, materialize_error, post, fallback) = callbacks;
    let control = get_worker_sentry_control(error)?;
    let split = split_worker_log_extra(extra)?;
    let mut effective_send = send_to_sentry;
    let mut suppress_console = split.get("suppressConsoleLog")?.truthy();
    let mut suppressed_reports = Js::Number(0.0);
    let key = control.get("reportKey")?;
    let cooldown = control.get("reportCooldownMs")?;
    if send_to_sentry && key.truthy() && cooldown.truthy() {
        let now = clock()?;
        let previous = cooldowns.call_method("get", vec![key.clone()])?;
        let should_report = previous.is_undefined()
            || ops::greater_equal(
                &ops::subtract(&Js::Number(now), &previous.get("lastReportedAt")?)?,
                &cooldown,
            )?;
        let next = if should_report {
            if !previous.is_undefined() {
                let count = previous.get("suppressedCount")?;
                if !count.is_nullish() {
                    suppressed_reports = count;
                }
            }
            (Js::Number(now), Js::Number(0.0))
        } else {
            effective_send = false;
            suppress_console = true;
            (
                previous.get("lastReportedAt")?,
                ops::add(&previous.get("suppressedCount")?, &Js::Number(1.0))?,
            )
        };
        cooldowns.call_method(
            "set",
            vec![
                key,
                Js::object(vec![
                    ("lastReportedAt", next.0),
                    ("suppressedCount", next.1),
                ]),
            ],
        )?;
    }
    let serializable_error = materialize_error()?;
    // These spreads are inside the source try, after error materialization.
    let deliver = || -> Result<(), E> {
        let tags = Js::object(vec![
            ("source", Js::str("webworker")),
            ("module", Js::str("ExchangeWorker")),
            ("exchange", from_typed(&identity.0)?),
            ("account", Js::str(identity.1)),
        ]);
        for value in [split.get("sentryTags")?, control.get("tags")?] {
            spread_into(&tags, &value)?;
        }
        let extra = Js::object(vec![]);
        for value in [
            split.get("extra")?,
            split.get("sentryExtra")?,
            control.get("extra")?,
        ] {
            spread_into(&extra, &value)?;
        }
        if ops::greater_than(&suppressed_reports, &Js::Number(0.0))? {
            extra.define("suppressedRepeatedReports", suppressed_reports)?;
        }
        post(WorkerDataPayload {
            op: StoreWorkerOp::Error,
            data: Js::array(vec![
                message.clone(),
                Js::object(vec![
                    ("sendToSentry", Js::Bool(effective_send)),
                    ("suppressConsoleLog", Js::Bool(suppress_console)),
                    (
                        "sentryData",
                        Js::object(vec![
                            ("error", serializable_error),
                            ("tags", tags),
                            ("extra", extra),
                        ]),
                    ),
                ]),
            ]),
        })
    };
    match deliver() {
        Ok(()) => Ok(()),
        Err(cause) => fallback(
            "[ExchangeWorker] Failed to send error to main thread:",
            message,
            error,
            cause,
        ),
    }
}
