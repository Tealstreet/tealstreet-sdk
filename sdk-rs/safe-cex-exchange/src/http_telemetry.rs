//! The maintained Axios factory's endpoint gate and JSON event messages.
use crate::{
    http::{HttpLogState, LAST_LOG_PERIODICY},
    js_runtime::{
        globals::stringify_json,
        iterator::IteratorRecord,
        ops,
        value::{Js, Thrown},
    },
};
use std::sync::{LazyLock, Mutex};

pub(crate) fn is_development() -> bool {
    static DEVELOPMENT: LazyLock<bool> = LazyLock::new(|| {
        safe_cex_store::remaining_utils::is_development(std::env::var("NODE_ENV").ok().as_deref())
    });
    *DEVELOPMENT
}

pub(crate) fn should_log(
    url: impl Fn() -> Result<Js, Thrown>,
    endpoints: impl Fn(&str) -> Result<Js, Thrown>,
    development: bool,
    now: impl Fn() -> f64,
    logs: &Mutex<HttpLogState>,
    response: bool,
) -> Result<bool, Thrown> {
    if development {
        return Ok(true);
    }
    let mut should = false;
    for (property, periodic) in [
        ("grafanaEndpointsEvery", false),
        ("grafanaEndpointsPeriodic", true),
    ] {
        let iterator = IteratorRecord::get(&endpoints(property)?)?;
        while let Some(endpoint) = iterator.step_value()? {
            let matches = (|| {
                let url = url()?;
                if url.is_nullish() {
                    Ok(false)
                } else {
                    url.call_method("includes", vec![endpoint.clone()])
                        .map(|v| v.truthy())
                }
            })()
            .map_err(|error| iterator.close_after_throw(error))?;
            if matches {
                if periodic {
                    let result = (|| {
                        let lookup = || {
                            let key = endpoint.to_js_string()?;
                            let previous = {
                                let state =
                                    logs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                                let times = if response {
                                    &state.response
                                } else {
                                    &state.request
                                };
                                times.get(key.as_ref()).copied()
                            };
                            previous.map_or_else(
                                || Js::object(vec![]).get_key(&key),
                                |previous| Ok(Js::Number(previous)),
                            )
                        };
                        if !lookup()?.truthy()
                            || now() - lookup()?.to_number()? >= LAST_LOG_PERIODICY as f64
                        {
                            let key = endpoint.to_js_string()?;
                            let time = now();
                            let mut state =
                                logs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                            let times = if response {
                                &mut state.response
                            } else {
                                &mut state.request
                            };
                            times.insert(key.to_vec(), time);
                            should = true;
                        }
                        Ok::<(), Thrown>(())
                    })();
                    result.map_err(|error| iterator.close_after_throw(error))?;
                } else {
                    should = true;
                }
                iterator.close()?;
                break;
            }
        }
    }
    Ok(should)
}

fn upper_method(config: &Js) -> Result<Js, Thrown> {
    let method = config.get("method")?;
    if method.is_nullish() {
        Ok(Js::Undefined)
    } else {
        method.call_method("toUpperCase", vec![])
    }
}

pub(crate) fn request_message(config: &Js) -> Result<Js, Thrown> {
    let summary = Js::object(vec![
        ("method", upper_method(config)?),
        ("url", config.get("url")?),
        ("body", config.get("data")?),
        ("params", config.get("params")?),
    ]);
    let summary = stringify_json(&summary)?.map_or(Js::Undefined, |text| Js::utf16(&text));
    let message = ops::add(&Js::str("Request "), &upper_method(config)?)?;
    let message = ops::add(&ops::add(&message, &Js::str(" "))?, &config.get("url")?)?;
    ops::add(&ops::add(&message, &Js::str(" "))?, &summary)
}

pub(crate) fn response_message(response: &Js) -> Result<Js, Thrown> {
    let summary = Js::object(vec![
        ("status", response.get("status")?),
        ("body", response.get("data")?),
    ]);
    let summary = stringify_json(&summary)?.map_or(Js::Undefined, |text| Js::utf16(&text));
    let message = ops::add(
        &Js::str("Response "),
        &upper_method(&response.get("config")?)?,
    )?;
    let message = ops::add(
        &ops::add(&message, &Js::str(" "))?,
        &response.get("config")?.get("url")?,
    )?;
    let message = ops::add(
        &ops::add(&message, &Js::str(" "))?,
        &response.get("status")?,
    )?;
    ops::add(&ops::add(&message, &Js::str(" "))?, &summary)
}
