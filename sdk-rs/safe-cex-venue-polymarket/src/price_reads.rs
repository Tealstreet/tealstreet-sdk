//! Eight REST method bodies at the receiver's already-started Axios boundary.
//! Strings retain UTF-16 units; hosts supply ordinary typed response fields.
//! Returned object values are non-thenable; async thenable assimilation and
//! arbitrary getters/proxies are outside this injected ordinary-data boundary.
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

use crate::rest_forwarding::MethodResponse;

/// Prepared parameter values, including present undefined optional arguments.
#[derive(Debug, Clone)]
pub enum ParamValue {
    Undefined,
    String(Vec<u16>),
    Number(f64),
}
/// Native Axios arguments before interceptors or serialization.
#[derive(Debug, Clone)]
pub struct ReadRequest {
    pub client: &'static str,
    pub method: &'static str,
    pub url: Vec<u16>,
    pub params: Option<Vec<(&'static str, ParamValue)>>,
    pub headers: Option<Vec<(&'static str, Vec<u16>)>>,
    /// Each POST row is a fresh object; no alias to the caller's token array.
    pub data: Option<Vec<Vec<(&'static str, Vec<u16>)>>>,
}
/// Ordinary response fields read after the awaited Axios operation completes.
#[derive(Debug, Clone)]
pub struct PriceData {
    pub mid: Vec<u16>,
    pub price: Vec<u16>,
}
/// Ordinary Object.entries snapshot, unique own string keys in source order.
/// Results use the same entry projection of Object.fromEntries, including
/// numeric keys, __proto__ and lone-surrogate keys without prototype mutation.
#[derive(Debug, Clone)]
pub struct MidpointObject {
    pub entries: Vec<(Vec<u16>, Vec<u16>)>,
}
/// The unvalidated history property, retaining truthiness and native aliases.
#[derive(Debug, Clone)]
pub enum HistoryData<T> {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Rc<RefCell<Vec<T>>>),
    Object(Rc<RefCell<T>>),
}
/// Source response data's history property; absent is HistoryData::Undefined.
#[derive(Debug, Clone)]
pub struct HistoryResponse<T> {
    pub history: HistoryData<T>,
}

fn units(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
fn endpoint(key: &str) -> Vec<u16> {
    units(
        crate::constants::ENDPOINTS
            .iter()
            .find(|(name, _)| *name == key)
            .expect("source endpoint")
            .1,
    )
}
fn request(method: &'static str, url: Vec<u16>) -> ReadRequest {
    ReadRequest {
        client: "xhr",
        method,
        url,
        params: None,
        headers: None,
        data: None,
    }
}
// parseFloat only observes a whitespace/sign/decimal/Infinity prefix. Replacing
// a lone surrogate with U+FFFD preserves its non-whitespace/non-numeric boundary.
fn number(value: &[u16]) -> f64 {
    safe_cex_num::parse_float(&String::from_utf16_lossy(value))
}
fn optional_number(value: Option<f64>) -> ParamValue {
    value.map_or(ParamValue::Undefined, ParamValue::Number)
}
fn truthy<T>(value: &HistoryData<T>) -> bool {
    match value {
        HistoryData::Undefined | HistoryData::Null => false,
        HistoryData::Bool(value) => *value,
        HistoryData::Number(value) => *value != 0.0 && !value.is_nan(),
        HistoryData::String(value) => !value.is_empty(),
        HistoryData::Array(_) | HistoryData::Object(_) => true,
    }
}
fn forward<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    config: ReadRequest,
) -> impl Future<Output = Result<T, E>> + 'a {
    let pending = send(config);
    async move { pending.await.map(|response| response.data) }
}

/// Source PolymarketAPI.getMidpoint, including parseFloat's prefix semantics.
pub fn get_midpoint<'a, E: 'a, F: Future<Output = Result<MethodResponse<PriceData>, E>> + 'a>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    token_id: &[u16],
) -> impl Future<Output = Result<f64, E>> + 'a {
    let mut config = request("get", endpoint("MIDPOINT"));
    config.params = Some(vec![("token_id", ParamValue::String(token_id.to_vec()))]);
    let pending = send(config);
    async move { pending.await.map(|response| number(&response.data.mid)) }
}
/// Source PolymarketAPI.getMidpoints over an ordinary own-entry projection.
pub fn get_midpoints<
    'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<MidpointObject>, E>> + 'a,
>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    token_ids: &[Vec<u16>],
) -> impl Future<Output = Result<Vec<(Vec<u16>, f64)>, E>> + 'a {
    let mut config = request("post", endpoint("MIDPOINTS"));
    config.data = Some(
        token_ids
            .iter()
            .map(|id| vec![("token_id", id.clone())])
            .collect(),
    );
    let pending = send(config);
    async move {
        pending.await.map(|response| {
            response
                .data
                .entries
                .into_iter()
                .map(|(id, price)| (id, number(&price)))
                .collect()
        })
    }
}
/// Source PolymarketAPI.getPrice; side is the source BUY/SELL string argument.
pub fn get_price<'a, E: 'a, F: Future<Output = Result<MethodResponse<PriceData>, E>> + 'a>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    token_id: &[u16],
    side: &[u16],
) -> impl Future<Output = Result<f64, E>> + 'a {
    let mut config = request("get", endpoint("PRICE"));
    config.params = Some(vec![
        ("token_id", ParamValue::String(token_id.to_vec())),
        ("side", ParamValue::String(side.to_vec())),
    ]);
    let pending = send(config);
    async move { pending.await.map(|response| number(&response.data.price)) }
}
/// Source PolymarketAPI.getLastTradePrice.
pub fn get_last_trade_price<
    'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<PriceData>, E>> + 'a,
>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    token_id: &[u16],
) -> impl Future<Output = Result<f64, E>> + 'a {
    let mut config = request("get", endpoint("LAST_TRADE_PRICE"));
    config.params = Some(vec![("token_id", ParamValue::String(token_id.to_vec()))]);
    let pending = send(config);
    async move { pending.await.map(|response| number(&response.data.price)) }
}
/// Source PolymarketAPI.getPriceHistory. Undefined arguments remain own keys;
/// every falsy history produces a fresh empty array; truthy values retain identity.
pub fn get_price_history<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<HistoryResponse<T>>, E>> + 'a,
>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    token_id: &[u16],
    start_ts: Option<f64>,
    end_ts: Option<f64>,
    interval: Option<&[u16]>,
    fidelity: Option<f64>,
) -> impl Future<Output = Result<HistoryData<T>, E>> + 'a {
    let mut config = request("get", endpoint("PRICE_HISTORY"));
    config.params = Some(vec![
        ("market", ParamValue::String(token_id.to_vec())),
        ("startTs", optional_number(start_ts)),
        ("endTs", optional_number(end_ts)),
        (
            "interval",
            interval.map_or(ParamValue::Undefined, |v| ParamValue::String(v.to_vec())),
        ),
        ("fidelity", optional_number(fidelity)),
    ]);
    let pending = send(config);
    async move {
        let history = pending.await?.data.history;
        Ok(if truthy(&history) {
            history
        } else {
            HistoryData::Array(Rc::new(RefCell::new(Vec::new())))
        })
    }
}
/// Source PolymarketAPI.getOrder; no URL escaping is added.
pub fn get_order<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    order_id: &[u16],
) -> impl Future<Output = Result<T, E>> + 'a {
    let mut url = endpoint("GET_ORDER");
    url.extend_from_slice(order_id);
    forward(send, request("get", url))
}
/// Source PolymarketAPI.getBalanceAllowance; Number is not narrowed to an enum.
pub fn get_balance_allowance<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<T>, E>> + 'a,
>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    signature_type: f64,
) -> impl Future<Output = Result<T, E>> + 'a {
    let mut config = request("get", endpoint("BALANCE_ALLOWANCE"));
    config.params = Some(vec![
        ("asset_type", ParamValue::String(units("COLLATERAL"))),
        ("signature_type", ParamValue::Number(signature_type)),
    ]);
    forward(send, config)
}
/// Source PolymarketAPI.deriveApiKey; original toString header order/spelling.
pub fn derive_api_key<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(ReadRequest) -> Pin<&'a mut F>,
    address: &[u16],
    nonce: f64,
    signature: &[u16],
    timestamp: f64,
) -> impl Future<Output = Result<T, E>> + 'a {
    let mut config = request("get", endpoint("DERIVE_API_KEY"));
    config.headers = Some(vec![
        ("POLY_ADDRESS", address.to_vec()),
        ("POLY_SIGNATURE", signature.to_vec()),
        (
            "POLY_TIMESTAMP",
            units(&safe_cex_num::js_number_string(timestamp)),
        ),
        ("POLY_NONCE", units(&safe_cex_num::js_number_string(nonce))),
    ]);
    forward(send, config)
}
