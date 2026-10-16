//! API signing and interceptor bodies over original runtime/import bindings.
use crate::{
    account_order_reads::AccountReadHost,
    classes::{BoundMember, Return},
    ws_controls::{Result, Value},
};
use std::rc::Rc;
pub trait ApiHost: AccountReadHost + Clone + 'static {
    fn spread_object(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn delete(&self, value: Value<Self>, key: &str) -> Result<Self, ()>;
    fn uppercase_optional(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn includes_optional(&self, value: Value<Self>, needle: &str) -> Result<Self, bool>;
    fn keys_len(&self, value: Value<Self>) -> Result<Self, usize>;
    fn query(&self, value: Value<Self>, sorted: bool) -> Result<Self, Value<Self>>;
    fn join(&self, values: Vec<Value<Self>>) -> Result<Self, Value<Self>>;
    fn axios_headers(&self, fields: Value<Self>) -> Result<Self, Value<Self>>;
    fn function(&self, name: &str, body: BoundMember<Self>) -> Value<Self>;
    fn axios_create(
        &self,
        config: Value<Self>,
        options: Value<Self>,
        exchange: Value<Self>,
    ) -> Result<Self, Value<Self>>;
    fn network_retry_condition(&self) -> Result<Self, Value<Self>>;
    fn retry(&self, client: Value<Self>, options: Value<Self>) -> Result<Self, ()>;
    fn transport(
        &self,
        client: Value<Self>,
        options: Value<Self>,
        name: Value<Self>,
    ) -> Result<Self, ()>;
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: ApiHost>(h: &H, s: &str) -> Value<H> {
    h.text(&u(s))
}
fn get<H: ApiHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &u(key))
}
fn opt<H: ApiHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, key)
    }
}
fn default<H: ApiHost>(h: &H, v: Value<H>, s: &str) -> Value<H> {
    if h.truthy(&v) { v } else { text(h, s) }
}
fn eq<H: ApiHost>(h: &H, v: Value<H>, s: &str) -> bool {
    h.text_eq(&v, s)
}
pub fn sign_request<H: ApiHost>(h: &H, c: Value<H>, options: Value<H>) -> Result<H, Value<H>> {
    let next = h.spread_object(c.clone())?;
    let method = default(h, h.uppercase_optional(get(h, &c, "method")?)?, "GET");
    h.set(&next, &u("method"), method)?;
    if h.truthy(&*get(h, &next, "params")?) {
        let url = h.string(&*get(h, &next, "url")?)?;
        let query = h.query(get(h, &next, "params")?, false)?;
        let url = h.text(&[url, u("?"), h.string(&query)?].concat());
        h.set(&next, &u("url"), url)?;
        h.delete(next.clone(), "params")?;
    }
    let timestamp = h.virtual_now()?;
    let method = get(h, &next, "method")?;
    let url = get(h, &next, "url")?;
    let body = if h.truthy(&*get(h, &next, "data")?) && h.keys_len(get(h, &next, "data")?)? > 0 {
        h.stringify(get(h, &next, "data")?)?
    } else {
        text(h, "")
    };
    let message = h.join(vec![timestamp.clone(), method, url, body])?;
    let signature = h.sign(message.clone(), get(h, &options, "secret")?)?;
    let headers = h.spread_object(get(h, &next, "headers")?)?;
    for (key, value) in [
        ("orderly-timestamp", timestamp),
        ("orderly-signature", signature),
        ("orderly-textsign", message),
        ("orderly-key", get(h, &options, "key")?),
    ] {
        h.set(&headers, &u(key), value)?;
    }
    let content =
        if eq(h, get(h, &next, "method")?, "GET") || eq(h, get(h, &next, "method")?, "DELETE") {
            "application/x-www-form-urlencoded"
        } else {
            "application/json"
        };
    h.set(&headers, &u("Content-Type"), text(h, content))?;
    h.set(&headers, &u("Accept"), text(h, "*/*"))?;
    let headers = h.axios_headers(headers)?;
    let result = h.spread_object(next)?;
    h.set(&result, &u("headers"), headers)?;
    Ok(result)
}
pub fn intercept<H: ApiHost>(h: &H, options: Value<H>, config: Value<H>) -> Result<H, Value<H>> {
    if h.includes_optional(get(h, &config, "url")?, "/public/")? {
        return Ok(config);
    }
    if eq(h, h.uppercase_optional(get(h, &config, "method")?)?, "POST") {
        for (endpoint, key) in [("PLACE_ORDER", "broker_id"), ("ALGO_ORDER", "brokerId")] {
            let path = crate::types::ENDPOINTS
                .iter()
                .find(|(k, _)| *k == endpoint)
                .expect("endpoint")
                .1;
            if eq(h, get(h, &config, "url")?, path) {
                let data = get(h, &config, "data")?;
                let broker = if h.truthy(&*get(h, &options, "testnet")?) {
                    crate::types::TESTNET_BROKER_ID
                } else {
                    crate::types::BROKER_ID
                };
                h.set(&data, &u(key), text(h, broker))?;
            }
        }
    }
    sign_request(h, config, options)
}
pub fn create_api<H: ApiHost>(h: &H, options: Value<H>, exchange: Value<H>) -> Result<H, Value<H>> {
    let base = crate::types::BASE_URL[usize::from(h.truthy(&*get(h, &options, "testnet")?))].1;
    let timeout = opt(h, &opt(h, &options, "extra")?, "recvWindow")?;
    let timeout = if h.nullish(&timeout) {
        h.number(crate::types::RECV_WINDOW)
    } else {
        timeout
    };
    let clone = h.clone();
    let serialize = h.function(
        "serialize",
        Rc::new(move |args| {
            let params = args.first().cloned().unwrap_or_else(|| clone.undefined());
            Ok(Return::Value(clone.query(params, true)?))
        }),
    );
    let serializer = h.object(vec![("serialize", serialize)])?;
    let headers = h.object(vec![
        ("cache-control", text(h, "no-cache")),
        (
            "orderly-account-id",
            h.text(&h.string(&*get(h, &options, "applicationId")?)?),
        ),
    ])?;
    let config = h.object(vec![
        ("baseURL", text(h, base)),
        ("timeout", timeout),
        ("paramsSerializer", serializer),
        ("headers", headers),
    ])?;
    let client = h.axios_create(config, options.clone(), exchange.clone())?;
    let condition = h.network_retry_condition()?;
    let retry_options = h.object(vec![
        ("retries", h.number(3.0)),
        ("retryCondition", condition),
    ])?;
    h.retry(client.clone(), retry_options)?;
    if h.truthy(&*get(h, &options, "tealstreetTunnel")?) {
        h.transport(client.clone(), options.clone(), get(h, &exchange, "name")?)?;
    }
    let request = get(h, &get(h, &client, "interceptors")?, "request")?;
    let use_method = get(h, &request, "use")?;
    let clone = h.clone();
    let interceptor = h.function(
        "",
        Rc::new(move |args| {
            let config = args.first().cloned().unwrap_or_else(|| clone.undefined());
            Ok(Return::Value(intercept(&clone, options.clone(), config)?))
        }),
    );
    h.call(use_method, Some(request), vec![interceptor])?;
    Ok(client)
}
