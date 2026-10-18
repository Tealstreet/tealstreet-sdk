//! HTTP signing and private-socket host capabilities.
use super::{
    Rt,
    host::{find, global_call, text},
};
use crate::{collection_reads::Callback, private_auth::PrivateHost};
use safe_cex_exchange::js_runtime::{
    client, ops,
    value::{Js, Thrown, arg},
};
use std::cell::RefCell;

fn predicate(data: &Js) -> Result<bool, Thrown> {
    crate::private_auth::is_bitrue_signature_rejection(
        &safe_cex_exchange::js_runtime::value::to_retained_input(data)?,
    )
    .map_err(|e| Thrown::error(e.name, e.message))
}
fn signature_condition(error: &Js) -> Result<bool, Thrown> {
    let response = error.get("response")?;
    if response.is_nullish() {
        return Ok(false);
    }
    let config = error.get("config")?;
    if config.is_nullish() || config.get("safeCexSignatureRetry")?.truthy() {
        return Ok(false);
    }
    if !predicate(&response.get("data")?)? {
        return Ok(false);
    }
    config.set("safeCexSignatureRetry", Js::Bool(true))?;
    Ok(true)
}
impl PrivateHost for Rt {
    fn contains(&self, v: Js, needle: &str) -> Result<bool, Thrown> {
        Ok(v.call_method("includes", vec![Js::str(needle)])?.truthy())
    }
    fn less_equal(&self, a: Js, b: Js) -> Result<bool, Thrown> {
        ops::less_equal(&a, &b)
    }
    fn null_is(&self, v: &Js) -> bool {
        matches!(v, Js::Null)
    }
    fn add(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::add(&a, &b)
    }
    fn copy(&self, v: Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn find(&self, v: Js, f: impl FnMut(Js) -> Result<bool, Thrown>) -> Result<Js, Thrown> {
        find(v, f)
    }
    fn is_string(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn starts_with(&self, v: Js, prefix: &str) -> Result<bool, Thrown> {
        Ok(v.call_method("startsWith", vec![Js::str(prefix)])?.truthy())
    }
    fn split(&self, v: Js, separator: &str) -> Result<Js, Thrown> {
        v.call_method("split", vec![Js::str(separator)])
    }
    fn index(&self, v: Js, n: usize) -> Result<Js, Thrown> {
        v.get(&n.to_string())
    }
    fn stringify(&self, v: Js) -> Result<Js, Thrown> {
        global_call(self, "JSON.stringify", vec![v])
    }
    fn join(&self, v: Vec<Js>) -> Result<Js, Thrown> {
        Js::array(v).call_method("join", vec![Js::str("")])
    }
    fn query(&self, v: Js) -> Result<Js, Thrown> {
        global_call(self, "qs.stringify", vec![v])
    }
    fn now(&self) -> Result<Js, Thrown> {
        Ok(Js::Number(self.shared.runtime.now_ms()))
    }
    fn virtual_now(&self) -> Result<Js, Thrown> {
        Ok(Js::Number(
            safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
        ))
    }
    fn parse_message(&self, data: Js) -> Result<Js, Thrown> {
        global_call(self, "bigIntAsStringJsonParse", vec![data])
    }
    fn error_instance(&self, error: &Js) -> bool {
        matches!(error,Js::Object(o) if o.class=="Error")
    }
    fn console_args(&self, args: Vec<Js>) -> Result<(), Thrown> {
        global_call(self, "console.error", args)?;
        Ok(())
    }
    fn delays(&self) -> Result<Js, Thrown> {
        self.global("listenKeyRetryDelays")
    }
    fn delay_promise(&self, delay: Js) -> Result<Js, Thrown> {
        global_call(self, "sleep", vec![delay])
    }
    fn reverse_symbol(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::str(&crate::utils::reverse_ws_symbol(&text(&v)?)))
    }
    fn create_incremental(&self) -> Result<Js, Thrown> {
        let fills = RefCell::new(safe_cex_store::incremental_fills::create_incremental_fills(
            500.,
        ));
        Ok(Js::function("incrementalFills", move |_, a| {
            let delta = fills.borrow_mut()(&text(&arg(&a, 0))?, arg(&a, 1).to_number()?, None);
            Ok(Js::object(vec![
                ("qty", Js::Number(delta.qty)),
                ("value", delta.value.map_or(Js::Undefined, Js::Number)),
            ]))
        }))
    }
    fn axios_headers(&self, v: Js) -> Result<Js, Thrown> {
        Ok(v)
    }
    fn axios_create(&self, config: Js, options: Js, _exchange: Js) -> Result<Js, Thrown> {
        client::factory::create(
            self.shared.http.clone(),
            self.shared.runtime.transport().as_ref(),
            config,
            &options,
            self.base(),
        )
    }
    fn transport(&self, client: Js, options: Js, name: Js, parser: &str) -> Result<(), Thrown> {
        let variant = match parser {
            "bigInt" => safe_cex_transport::RestAdapterVariant::BigInt,
            _ => return Err(Thrown::type_error("Unexpected Bitrue transport parser")),
        };
        client::factory::install_transport_adapter(
            self.shared.http.clone(),
            self.shared.runtime.transport().as_ref(),
            &client,
            &options,
            safe_cex_exchange::js_runtime::value::to_typed(&name)?,
            variant,
        )
    }
    fn clean_float(&self, value: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(safe_cex_store::incremental_fills::clean_float(
            value.to_number()?,
        )))
    }
    fn retry_condition(&self) -> Result<Js, Thrown> {
        Ok(Js::function("isRetryable", |_, a| {
            let error = arg(&a, 0);
            let config = error.get("config")?;
            if !config.is_nullish() {
                let method = config.get("method")?;
                let read = method.strict_equals(&Js::str("get"))
                    || method.strict_equals(&Js::str("head"))
                    || method.strict_equals(&Js::str("options"))
                    || (method.strict_equals(&Js::str("post"))
                        && config.get("url")?.strict_equals(&Js::str(
                            crate::types::ENDPOINTS
                                .iter()
                                .find(|(k, _)| *k == "LISTEN_KEY")
                                .unwrap()
                                .1,
                        )));
                let code = error.get("code")?;
                let response = error.get("response")?;
                let status = if response.is_nullish() {
                    None
                } else {
                    Some(response.get("status")?.to_number()?)
                };
                let network = response.is_nullish()
                    && safe_cex_transport::RestFailure::from_transport(
                        safe_cex_transport::TransportError::Network(
                            safe_cex_transport::NetworkFailure::from_code(
                                String::new(),
                                if code.is_undefined() {
                                    None
                                } else {
                                    Some(text(&code)?)
                                },
                            ),
                        ),
                        Default::default(),
                    )
                    .is_retryable_network();
                let idempotent = !code.strict_equals(&Js::str("ECONNABORTED"))
                    && (method.strict_equals(&Js::str("get"))
                        || method.strict_equals(&Js::str("head"))
                        || method.strict_equals(&Js::str("options"))
                        || method.strict_equals(&Js::str("put"))
                        || method.strict_equals(&Js::str("delete")))
                    && status.is_none_or(|s| s == 429. || (500. ..=599.).contains(&s));
                if read && (network || idempotent) {
                    return Ok(Js::Bool(true));
                }
            }
            Ok(Js::Bool(signature_condition(&error)?))
        }))
    }
    fn signature_rejection(&self) -> Result<Js, Thrown> {
        Ok(Js::function("isBitrueSignatureRejection", |_, a| {
            Ok(Js::Bool(predicate(&arg(&a, 0))?))
        }))
    }
    fn resend_on_signature_rejection(
        &self,
        client: Js,
        response: Js,
        predicate: Js,
    ) -> Result<Js, Thrown> {
        let config = response.get("config")?;
        if !config.is_nullish() && config.get("safeCexSignatureRetry")?.truthy() {
            return Ok(Js::Undefined);
        }
        if !predicate
            .call(Js::Undefined, vec![response.get("data")?])?
            .truthy()
        {
            return Ok(Js::Undefined);
        }
        let config = config.spread_object()?;
        config.define("safeCexSignatureRetry", Js::Bool(true))?;
        client.call_method("request", vec![config])
    }
    fn retry(&self, client: Js, options: Js) -> Result<(), Thrown> {
        client.call_method("__configureRetry", vec![options])?;
        Ok(())
    }
    fn create_hmac(&self, algorithm: &str, secret: Js) -> Result<Js, Thrown> {
        global_call(self, "createHmac", vec![Js::str(algorithm), secret])
    }
    fn method_uppercase_optional(&self, v: Js) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.call_method("toUpperCase", vec![])
        }
    }
    fn super_socket(&self, args: Vec<Js>) -> Result<Js, Thrown> {
        super::sockets::base(self, arg(&args, 0))
    }
    fn define(&self, r: Js, key: &str, v: Js) -> Result<(), Thrown> {
        r.define(key, v)
    }
    fn member(&self, name: &'static str, _target: &'static str, r: Js) -> Js {
        super::sockets::private_member(self, r, name)
    }
    fn class_environment(&self, _initial_delays: &[f64], _disposal_target: &'static str) {}
    fn named_function(&self, name: &str, f: Callback<Self>) -> Js {
        Js::function(name, move |_, a| f(a))
    }
}
