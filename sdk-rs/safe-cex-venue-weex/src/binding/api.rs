use super::values as v;
use super::*;
use crate::{
    api_binding::{self as api, WeexApiRuntime},
    headers::{self, WeexHeaderRuntime, WeexHeaderValueKind as Kind},
};
use safe_cex_transport::{RestFailure, RestResponse, RestResponseBody};

pub(super) fn factory(h: &Host) -> Result<Js, Thrown> {
    let weak = h.weak();
    Ok(Js::function("createAPI", move |_, a| {
        api::create_api(&weak()?, &runtime(), arg(&a, 0), arg(&a, 1))
    }))
}
pub(super) fn runtime() -> WeexApiRuntime<Host, Js, Thrown> {
    WeexApiRuntime {
        get: v::get,
        optional_get: v::optional,
        truthy: Js::truthy,
        nullish: Js::is_nullish,
        is_undefined: Js::is_undefined,
        is_string: Js::is_string,
        text: Js::str,
        string_units: |v| Js::utf16(&v),
        number: Js::Number,
        object: Js::object,
        template: |_, value| v::units(&value),
        copy: |_, value| {
            let copy = value.spread_object()?;
            if let (Js::Object(source), Js::Object(target)) = (value, &copy) {
                if source.class == "AxiosHeaders" {
                    *target.proto.borrow_mut() = source.proto.borrow().clone();
                }
            }
            Ok(copy)
        },
        define: v::define,
        starts_with: |_, value, prefix| {
            Ok(value
                .call_method("startsWith", std::vec![Js::str(prefix)])?
                .truthy())
        },
        uppercase_method: |_, value| {
            if value.is_nullish() {
                Ok(Js::Undefined)
            } else {
                let method = value.get("toUpperCase")?;
                if method.is_nullish() {
                    Ok(Js::Undefined)
                } else {
                    method.call(value.clone(), std::vec![])
                }
            }
        },
        locale_compare: |_, left, right| {
            let comparator = safe_cex_num::JsRootCollator::try_new().map_err(v::coercion)?;
            Ok(Js::Number(
                match comparator.compare(&left.to_display(), &right.to_display()) {
                    std::cmp::Ordering::Less => -1.0,
                    std::cmp::Ordering::Equal => 0.0,
                    std::cmp::Ordering::Greater => 1.0,
                },
            ))
        },
        qs_stringify: |_, params, _, _| {
            use safe_cex_transport::query::{
                QueryArrayFormat, QueryKeyOrder, QueryProfile, render_query,
            };
            let query = render_query(
                &v::query_value(&params)?,
                QueryProfile {
                    arrays: QueryArrayFormat::Repeat,
                    encode: true,
                    key_order: QueryKeyOrder::JsRoot,
                },
            )
            .map_err(v::coercion)?;
            Ok(Js::str(&query))
        },
        stringify: |h, value| {
            h.global("JSON.stringify")?
                .call(Js::Undefined, std::vec![value])
        },
        callback: |h, callback| {
            let weak = h.weak();
            Ok(Js::function("", move |_, a| {
                api::run_api_callback(&weak()?, &runtime(), callback.clone(), arg(&a, 0))
            }))
        },
        transform_response: |_| {
            Ok(Js::function("bigIntAsStringJsonParse", |_, a| {
                let value = arg(&a, 0);
                if !value.is_string() {
                    return Ok(value);
                }
                Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
                    safe_cex_transport::how::big_int_as_string_json_parse(&value.to_display()),
                ))
            }))
        },
        create: |h, config, options, _exchange| {
            let xhr = safe_cex_exchange::js_runtime::client::factory::create(
                h.0.shared.http.clone(),
                h.0.shared.runtime.transport().as_ref(),
                config,
                &options,
                h.0.base(),
            )?;
            Ok(xhr)
        },
        install_retry,
        register: |_, client, kind, callback| {
            client
                .get("interceptors")?
                .get(kind)?
                .call_method("use", std::vec![callback])?;
            Ok(())
        },
        clock_ms: |_| {
            Ok(Js::Number(
                safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
            ))
        },
        sign: |_, secret, message| {
            Ok(Js::str(&crate::signing::sign_weex_message(
                &v::units(&secret)?,
                &message,
            )))
        },
        axios_headers: |_, value| {
            // The client merges into its common AxiosHeaders before each attempt.
            Ok(value)
        },
        new_error: |_, message| Ok(Thrown::error("Error", &message.to_display()).value),
        throw: Thrown::new,
    }
}
fn install_retry(h: &Host, client: &Js) -> Result<(), Thrown> {
    let weak = h.weak();
    client.call_method(
        "__configureRetry",
        std::vec![Js::object(std::vec![
            (
                "retries",
                Js::Number(crate::retry::WEEX_RETRY_CONFIG.retries as f64),
            ),
            (
                "shouldResetTimeout",
                Js::Bool(crate::retry::WEEX_RETRY_CONFIG.should_reset_timeout),
            ),
            (
                "retryCondition",
                Js::function("weexRetryCondition", |_, a| {
                    let error = arg(&a, 0);
                    let config = v::optional_dummy(&error, "config")?;
                    let method = v::optional_dummy(&config, "method")?;
                    let url = v::optional_dummy(&config, "url")?;
                    let response = v::optional_dummy(&error, "response")?;
                    let status = v::optional_dummy(&response, "status")?;
                    let state = v::optional_dummy(&config, "axios-retry")?;
                    let count = v::optional_dummy(&state, "retryCount")?;
                    let method = (!method.is_nullish())
                        .then(|| v::units(&method))
                        .transpose()?;
                    let url = (!url.is_nullish()).then(|| v::units(&url)).transpose()?;
                    let status = (!status.is_nullish())
                        .then(|| status.to_number())
                        .transpose()?;
                    let count = (!count.is_nullish())
                        .then(|| count.to_number())
                        .transpose()?;
                    let code = v::optional_dummy(&error, "code")?;
                    let failure = RestFailure {
                        code: (!code.is_undefined()).then(|| code.to_display()),
                        message: v::optional_dummy(&error, "message")?.to_display(),
                        transport_error: None,
                        response: (!response.is_nullish()).then(|| {
                            Box::new(RestResponse {
                                status: status.unwrap_or(0.0) as u16,
                                reason: None,
                                headers: Default::default(),
                                body: RestResponseBody::Untransformed(
                                    safe_cex_types::retained_json::RetainedJsonInput::Null,
                                ),
                            })
                        }),
                        request: Box::default(),
                        context: None,
                    };
                    Ok(Js::Bool(crate::retry::should_retry_weex_request(
                        method.as_deref(),
                        url.as_deref(),
                        status,
                        count,
                        || safe_cex_transport::retry::is_network_error(&failure),
                    )))
                }),
            ),
            (
                "retryDelay",
                Js::function("weexRetryDelay", move |_, a| {
                    let error = arg(&a, 1);
                    let response = v::optional_dummy(&error, "response")?;
                    let status = v::optional_dummy(&response, "status")?;
                    if !status.strict_equals(&Js::Number(429.0)) {
                        return Ok(Js::Number(0.0));
                    }
                    let mut h = weak()?;
                    let headers = v::optional_dummy(&response, "headers")?;
                    headers::get_weex_rate_limit_retry_delay_ms(&headers, &mut h, &header_runtime())
                        .map(Js::Number)
                }),
            ),
        ])],
    )?;
    Ok(())
}
fn header_runtime() -> WeexHeaderRuntime<Host, Js, Thrown> {
    WeexHeaderRuntime {
        kind: |v| match v {
            Js::Undefined => Kind::Undefined,
            Js::Null => Kind::Null,
            Js::Bool(_) => Kind::Boolean,
            Js::Number(_) => Kind::Number,
            Js::String(_) => Kind::String,
            Js::Function(_) => Kind::Function,
            Js::BigInt(_) => Kind::BigInt,
            _ => Kind::Object,
        },
        undefined: || Js::Undefined,
        has_property: |_, v, key| {
            Ok(v.has_own(&String::from_utf16_lossy(key)) || !v.get_key(key)?.is_undefined())
        },
        get_property: |_, v, key| v.get_key(key),
        invoke: |_, f, this, value| {
            f.call(
                this.clone(),
                value.map(|s| std::vec![Js::utf16(s)]).unwrap_or_default(),
            )
        },
        entries: |_, v| {
            Ok(v.own_entries()?
                .into_iter()
                .map(|(k, v)| (k.to_vec(), v))
                .collect())
        },
        lowercase: |_, s| {
            String::from_utf16_lossy(s)
                .to_lowercase()
                .encode_utf16()
                .collect()
        },
        uppercase: |_, s| {
            String::from_utf16_lossy(s)
                .to_uppercase()
                .encode_utf16()
                .collect()
        },
        number_value: |v| if let Js::Number(n) = v { *n } else { f64::NAN },
        string_value: |v| v.to_js_string().unwrap().to_vec(),
        number: |_, v| v.to_number(),
        maximum: |_, values| {
            Ok(values
                .iter()
                .fold(f64::NEG_INFINITY, |a, b| safe_cex_num::math_max(a, *b)))
        },
        now_ms: |h| h.0.now(),
        parse_date: |h, s| {
            safe_cex_num::js_date_parse_utf16(s, |ms| (h.0.shared.extra.local_to_utc)(ms))
        },
    }
}
