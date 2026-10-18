//! Native graph port of utils/axiosErrorEnhancer.ts, before venue interceptors.
use crate::js_runtime::value::{Js, Thrown, error_object};

pub(super) fn enhance_axios_error(error: &Js, exchange: &str, now: f64) -> Result<Js, Thrown> {
    let enhanced = if matches!(error, Js::Object(object) if object.class == "Error") {
        error.clone()
    } else {
        let value = error_object("Error", "");
        value.set("message", Js::utf16(&error.to_js_string()?))?;
        value.set("isAxiosError", Js::Bool(true))?;
        value
    };
    if !enhanced.get("isAxiosError")?.truthy() {
        enhanced.set("isAxiosError", Js::Bool(true))?;
    }
    let context = Js::object(vec![(
        "timestamp",
        Js::str(&safe_cex_num::js_date_to_iso_string(now)?),
    )]);
    if error.get("config")?.truthy() {
        context.set("url", error.get("config")?.get("url")?)?;
        let method = error.get("config")?.get("method")?;
        let method = if method.is_nullish() {
            Js::Undefined
        } else {
            method.call_method("toUpperCase", vec![])?
        };
        context.set("method", method)?;
    }
    let template = |parts: &[Js]| -> Result<Js, Thrown> {
        let mut units = vec![];
        for part in parts {
            units.extend_from_slice(&part.to_js_string()?);
        }
        Ok(Js::utf16(&units))
    };
    let suffix = |prefix: &str| -> Result<Js, Thrown> {
        if context.get("url")?.truthy() {
            template(&[Js::str(prefix), context.get("url")?])
        } else {
            Ok(Js::str(""))
        }
    };
    let message = if !error.get("response")?.truthy() && error.get("request")?.truthy() {
        context.set("isNetworkError", Js::Bool(true))?;
        if error.get("code")?.strict_equals(&Js::str("ECONNABORTED")) {
            context.set("isTimeout", Js::Bool(true))?;
            template(&[Js::str("Request timeout"), suffix(" to ")?])?
        } else if error.get("code")?.strict_equals(&Js::str("ERR_NETWORK"))
            || error
                .get("message")?
                .strict_equals(&Js::str("Network Error"))
        {
            template(&[Js::str("Network connection failed"), suffix(" to ")?])?
        } else if error.get("code")?.truthy() {
            context.set("errorCode", error.get("code")?)?;
            template(&[
                Js::str("Network error ("),
                error.get("code")?,
                Js::str(")"),
                suffix(" to ")?,
            ])?
        } else {
            template(&[Js::str("Network request failed"), suffix(" to ")?])?
        }
    } else if error.get("response")?.truthy() {
        context.set("statusCode", error.get("response")?.get("status")?)?;
        context.set("statusText", error.get("response")?.get("statusText")?)?;
        let data = error.get("response")?.get("data")?;
        let mut server = Js::Undefined;
        for key in ["msg", "message", "error", "retMsg", "code"] {
            server = if data.is_nullish() {
                Js::Undefined
            } else {
                data.get(key)?
            };
            if server.truthy() {
                break;
            }
        }
        let method = context.get("method")?;
        let url = context.get("url")?;
        template(&[
            if method.truthy() {
                method
            } else {
                Js::str("Request")
            },
            Js::str(" "),
            if url.truthy() { url } else { Js::str("to API") },
            Js::str(" failed with "),
            context.get("statusCode")?,
            Js::str(" "),
            context.get("statusText")?,
            if server.truthy() {
                template(&[Js::str(": "), server])?
            } else {
                Js::str("")
            },
        ])?
    } else {
        template(&[
            Js::str("Request setup error"),
            suffix(" for ")?,
            Js::str(": "),
            error.get("message")?,
        ])?
    };
    enhanced.set("enhancedMessage", message)?;
    if !exchange.is_empty() {
        enhanced.set(
            "enhancedMessage",
            template(&[
                Js::str("["),
                Js::str(exchange),
                Js::str("] "),
                enhanced.get("enhancedMessage")?,
            ])?,
        )?;
    }
    enhanced.set("context", context)?;
    let original = if let Js::Object(object) = error
        && object.class == "Error"
    {
        let snapshot = error_object("Error", "");
        let Js::Object(data) = &snapshot else {
            unreachable!("Error constructor")
        };
        *data.props.borrow_mut() = object.props.borrow().clone();
        *data.proto.borrow_mut() = object.proto.borrow().clone();
        data.props.borrow_mut().shift_remove(
            "originalError"
                .encode_utf16()
                .collect::<Vec<_>>()
                .as_slice(),
        );
        snapshot.define("message", enhanced.get("enhancedMessage")?)?;
        snapshot
    } else {
        error.clone()
    };
    enhanced.set("originalError", original.clone())?;
    enhanced.define_accessor(
        "message",
        Some(std::rc::Rc::new(move |this| {
            let message = this.get("enhancedMessage")?;
            if message.truthy() {
                Ok(message)
            } else {
                original.get("message")
            }
        })),
        None,
    )?;
    Ok(enhanced)
}
