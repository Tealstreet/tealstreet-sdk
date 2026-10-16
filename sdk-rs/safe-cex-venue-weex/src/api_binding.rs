//! Axios assembly and interceptor bodies over imported transport and value operations.
use crate::types::{BASE_URL, PUBLIC_ENDPOINTS, RECV_WINDOW};

#[derive(Clone)]
pub enum WeexApiCallback<V> {
    Params,
    Request { options: V },
    Response,
}
pub struct WeexApiRuntime<H, V, E> {
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub optional_get: fn(&H, &V, &str) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub nullish: fn(&V) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub is_string: fn(&V) -> bool,
    pub text: fn(&str) -> V,
    pub string_units: fn(Vec<u16>) -> V,
    pub number: fn(f64) -> V,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub template: fn(&H, V) -> Result<Vec<u16>, E>,
    pub copy: fn(&H, &V) -> Result<V, E>,
    pub define: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub starts_with: fn(&H, &V, &str) -> Result<bool, E>,
    /// Source value?.toUpperCase?.(), retaining getter, receiver and call errors.
    pub uppercase_method: fn(&H, &V) -> Result<V, E>,
    pub locale_compare: fn(&H, V, V) -> Result<V, E>,
    pub qs_stringify: fn(&H, V, &'static str, fn(&H, V, V) -> Result<V, E>) -> Result<V, E>,
    pub stringify: fn(&H, V) -> Result<V, E>,
    pub callback: fn(&H, WeexApiCallback<V>) -> Result<V, E>,
    pub transform_response: fn(&H) -> Result<V, E>,
    pub create: fn(&H, V, V, V) -> Result<V, E>,
    pub install_retry: fn(&H, &V) -> Result<(), E>,
    pub register: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub clock_ms: fn(&H) -> Result<V, E>,
    pub sign: fn(&H, V, Vec<u16>) -> Result<V, E>,
    pub axios_headers: fn(&H, V) -> Result<V, E>,
    pub new_error: fn(&H, V) -> Result<V, E>,
    pub throw: fn(V) -> E,
}
pub fn serialize_weex_params<H, V, E>(
    h: &H,
    r: &WeexApiRuntime<H, V, E>,
    params: V,
) -> Result<V, E> {
    (r.qs_stringify)(h, params, "repeat", r.locale_compare)
}
fn text<H, V, E>(h: &H, r: &WeexApiRuntime<H, V, E>, v: V) -> Result<Vec<u16>, E> {
    (r.template)(h, v)
}
fn truthy_or<H, V, E>(
    r: &WeexApiRuntime<H, V, E>,
    v: V,
    fallback: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (r.truthy)(&v) { Ok(v) } else { fallback() }
}
pub fn create_api<H, V: Clone, E>(
    h: &H,
    r: &WeexApiRuntime<H, V, E>,
    options: V,
    exchange: V,
) -> Result<V, E> {
    if !(r.truthy)(&(r.get)(h, &options, "passphrase")?)
        && !(r.truthy)(&(r.get)(h, &options, "publicDataOnly")?)
    {
        let error = (r.new_error)(h, (r.text)("WEEX requires a passphrase"))?;
        return Err((r.throw)(error));
    }
    let network = if (r.truthy)(&(r.get)(h, &options, "testnet")?) {
        "testnet"
    } else {
        "livenet"
    };
    let url = BASE_URL.iter().find(|(k, _)| *k == network).unwrap().1;
    let extra = (r.optional_get)(h, &options, "extra")?;
    let window = (r.optional_get)(h, &extra, "recvWindow")?;
    let timeout = if (r.nullish)(&window) {
        (r.number)(RECV_WINDOW)
    } else {
        window
    };
    let serializer = (r.callback)(h, WeexApiCallback::Params)?;
    let transform = (r.transform_response)(h)?;
    let config = (r.object)(vec![
        ("baseURL", (r.text)(url)),
        ("timeout", timeout),
        (
            "paramsSerializer",
            (r.object)(vec![("serialize", serializer)]),
        ),
        (
            "headers",
            (r.object)(vec![("Content-Type", (r.text)("application/json"))]),
        ),
        ("transformResponse", transform),
    ]);
    let xhr = (r.create)(h, config, options.clone(), exchange)?;
    (r.install_retry)(h, &xhr)?;
    let request = (r.callback)(h, WeexApiCallback::Request { options })?;
    (r.register)(h, &xhr, "request", request)?;
    let response = (r.callback)(h, WeexApiCallback::Response)?;
    (r.register)(h, &xhr, "response", response)?;
    Ok(xhr)
}
pub fn run_api_callback<H, V: Clone, E>(
    h: &H,
    r: &WeexApiRuntime<H, V, E>,
    callback: WeexApiCallback<V>,
    value: V,
) -> Result<V, E> {
    match callback {
        WeexApiCallback::Params => serialize_weex_params(h, r, value),
        WeexApiCallback::Request { options } => {
            let url = truthy_or(r, (r.get)(h, &value, "url")?, || Ok((r.text)("")))?;
            let mut public = false;
            for endpoint in PUBLIC_ENDPOINTS {
                if (r.starts_with)(h, &url, endpoint)? {
                    public = true;
                    break;
                }
            }
            if public || (r.truthy)(&(r.get)(h, &options, "publicDataOnly")?) {
                return Ok(value);
            }
            let next = (r.copy)(h, &value)?;
            if (r.truthy)(&(r.get)(h, &options, "remoteSigner")?)
                || !(r.truthy)(&(r.get)(h, &options, "key")?)
                || !(r.truthy)(&(r.get)(h, &options, "secret")?)
            {
                return Ok(next);
            }
            let method_value = (r.get)(h, &value, "method")?;
            let method = truthy_or(r, (r.uppercase_method)(h, &method_value)?, || {
                Ok((r.text)("GET"))
            })?;
            let params = (r.get)(h, &value, "params")?;
            let query = if (r.truthy)(&params) {
                serialize_weex_params(h, r, (r.get)(h, &value, "params")?)?
            } else {
                (r.text)("")
            };
            let data = (r.get)(h, &value, "data")?;
            // Axios replays its already serialized body. Signing a JSON string
            // again would add quotes/escapes absent from the transmitted bytes.
            let body = if (r.is_string)(&data) {
                (r.get)(h, &value, "data")?
            } else if (r.truthy)(&(r.get)(h, &value, "data")?) {
                (r.stringify)(h, (r.get)(h, &value, "data")?)?
            } else {
                (r.text)("")
            };
            let mut path = text(h, r, url)?;
            if (r.truthy)(&query) {
                path.push(b'?' as u16);
                path.extend(text(h, r, query)?);
            }
            let timestamp = text(h, r, (r.clock_ms)(h)?)?;
            let secret = (r.get)(h, &options, "secret")?;
            let mut message = timestamp.clone();
            message.extend(text(h, r, method)?);
            message.extend(path);
            message.extend(text(h, r, body)?);
            let signature = (r.sign)(h, secret, message)?;
            let headers = (r.copy)(h, &(r.get)(h, &next, "headers")?)?;
            let key = (r.get)(h, &options, "key")?;
            (r.define)(h, &headers, "ACCESS-KEY", key)?;
            let pass = truthy_or(r, (r.get)(h, &options, "passphrase")?, || Ok((r.text)("")))?;
            (r.define)(h, &headers, "ACCESS-PASSPHRASE", pass)?;
            (r.define)(h, &headers, "ACCESS-TIMESTAMP", (r.string_units)(timestamp))?;
            (r.define)(h, &headers, "ACCESS-SIGN", signature)?;
            (r.define)(h, &headers, "Content-Type", (r.text)("application/json"))?;
            let headers = (r.axios_headers)(h, headers)?;
            let result = (r.copy)(h, &next)?;
            (r.define)(h, &result, "headers", headers)?;
            Ok(result)
        }
        WeexApiCallback::Response => {
            let data = (r.get)(h, &value, "data")?;
            let code = (r.optional_get)(h, &data, "code")?;
            if !(r.is_undefined)(&code) {
                if text(h, r, code.clone())? != "0".encode_utf16().collect::<Vec<_>>()
                    && text(h, r, code.clone())? != "200".encode_utf16().collect::<Vec<_>>()
                {
                    let data = (r.get)(h, &value, "data")?;
                    let msg = (r.optional_get)(h, &data, "msg")?;
                    let msg = truthy_or(r, msg, || {
                        let data = (r.get)(h, &value, "data")?;
                        truthy_or(r, (r.optional_get)(h, &data, "message")?, || {
                            let mut s: Vec<u16> = "WEEX API error (code ".encode_utf16().collect();
                            s.extend(text(h, r, code)?);
                            s.push(b')' as u16);
                            Ok((r.string_units)(s))
                        })
                    })?;
                    let error = (r.new_error)(h, msg)?;
                    (r.define)(h, &error, "response", value)?;
                    return Err((r.throw)(error));
                }
            }
            Ok(value)
        }
    }
}
