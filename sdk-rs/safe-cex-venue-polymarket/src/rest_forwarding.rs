//! Thin REST bodies over the receiver's injected Axios request boundary.
//! The client factory, authentication, retries and wire serialization stay separate.
//! The host starts and owns each operation; this body borrows its future only
//! to observe completion. Dropping that observation cannot drop the operation.
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

/// Source request `data`: absent, a newly built string-field object, or the
/// caller's live array. Strings retain JavaScript UTF-16 code units.
#[derive(Debug, Clone)]
pub enum MethodData {
    Absent,
    Fields(Vec<(&'static str, Vec<u16>)>),
    Strings(Rc<RefCell<Vec<Vec<u16>>>>),
}

/// Prepared source call, before Axios transforms it. `client` selects the
/// source receiver field, and `url` preserves unescaped template concatenation.
#[derive(Debug, Clone)]
pub struct MethodRequest {
    pub client: &'static str,
    pub method: &'static str,
    pub url: Vec<u16>,
    pub params: Option<Vec<(&'static str, Vec<u16>)>>,
    pub data: MethodData,
}

/// The source Axios response's `data` property is opaque to these ten bodies.
/// T may retain identity, undefined, ordinary records, and native numbers.
/// Root thenable assimilation is outside this ordinary response-data domain.
pub struct MethodResponse<T> {
    pub data: T,
}

fn endpoint(key: &str) -> Vec<u16> {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == key)
        .expect("source endpoint declaration")
        .1
        .encode_utf16()
        .collect()
}
fn request(client: &'static str, method: &'static str, url: Vec<u16>) -> MethodRequest {
    MethodRequest {
        client,
        method,
        url,
        params: None,
        data: MethodData::Absent,
    }
}
fn append(prefix: &str, suffix: &[u16]) -> Vec<u16> {
    prefix
        .encode_utf16()
        .chain(suffix.iter().copied())
        .collect()
}
fn forward<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    request: MethodRequest,
) -> impl Future<Output = Result<T, E>> + 'a {
    // JS invokes xhr synchronously before the first await. Do the same even
    // when the returned future has not yet been polled.
    let response = send(request);
    async move { response.await.map(|response| response.data) }
}
fn discard<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    request: MethodRequest,
) -> impl Future<Output = Result<(), E>> + 'a {
    let response = send(request);
    async move { response.await.map(|_| ()) }
}

/// Source PolymarketAPI.getSimplifiedMarkets.
pub fn get_simplified_markets<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<T>, E>> + 'a,
>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
) -> impl Future<Output = Result<T, E>> + 'a {
    forward(send, request("xhr", "get", endpoint("SIMPLIFIED_MARKETS")))
}

/// Source PolymarketAPI.getSpread.
pub fn get_spread<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    token_id: &[u16],
) -> impl Future<Output = Result<T, E>> + 'a {
    let mut config = request("xhr", "get", endpoint("SPREAD"));
    config.params = Some(vec![("token_id", token_id.to_vec())]);
    forward(send, config)
}

/// Source PolymarketAPI.getGammaMarket. No URL encoding is added.
pub fn get_gamma_market<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    condition_id: &[u16],
) -> impl Future<Output = Result<T, E>> + 'a {
    forward(
        send,
        request("gammaXhr", "get", append("/markets/", condition_id)),
    )
}

/// Source PolymarketAPI.getGammaMarketBySlug.
pub fn get_gamma_market_by_slug<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<T>, E>> + 'a,
>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    slug: &[u16],
) -> impl Future<Output = Result<T, E>> + 'a {
    forward(
        send,
        request("gammaXhr", "get", append("/markets/slug/", slug)),
    )
}

/// Source PolymarketAPI.searchGammaMarkets.
pub fn search_gamma_markets<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<T>, E>> + 'a,
>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    query: &[u16],
) -> impl Future<Output = Result<T, E>> + 'a {
    let mut config = request("gammaXhr", "get", "/markets".encode_utf16().collect());
    config.params = Some(vec![("search", query.to_vec())]);
    forward(send, config)
}

/// Source PolymarketAPI.cancelOrder. The source key is orderID, not orderId.
pub fn cancel_order<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    order_id: &[u16],
) -> impl Future<Output = Result<(), E>> + 'a {
    let mut config = request("xhr", "delete", endpoint("CANCEL"));
    config.data = MethodData::Fields(vec![("orderID", order_id.to_vec())]);
    discard(send, config)
}

/// Source PolymarketAPI.cancelOrders; pass the original array, not a copy or wrapper.
pub fn cancel_orders<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    order_ids: Rc<RefCell<Vec<Vec<u16>>>>,
) -> impl Future<Output = Result<(), E>> + 'a {
    let mut config = request("xhr", "delete", endpoint("CANCEL_ORDERS"));
    config.data = MethodData::Strings(order_ids);
    discard(send, config)
}

/// Source PolymarketAPI.cancelMarketOrders.
pub fn cancel_market_orders<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<T>, E>> + 'a,
>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
    market: &[u16],
) -> impl Future<Output = Result<(), E>> + 'a {
    let mut config = request("xhr", "delete", endpoint("CANCEL_MARKET_ORDERS"));
    config.data = MethodData::Fields(vec![("market", market.to_vec())]);
    discard(send, config)
}

/// Source PolymarketAPI.cancelAllOrders.
pub fn cancel_all_orders<
    'a,
    T: 'a,
    E: 'a,
    F: Future<Output = Result<MethodResponse<T>, E>> + 'a,
>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
) -> impl Future<Output = Result<(), E>> + 'a {
    discard(send, request("xhr", "delete", endpoint("CANCEL_ALL")))
}

/// Source PolymarketAPI.deleteApiKey.
pub fn delete_api_key<'a, T: 'a, E: 'a, F: Future<Output = Result<MethodResponse<T>, E>> + 'a>(
    send: impl FnOnce(MethodRequest) -> Pin<&'a mut F>,
) -> impl Future<Output = Result<(), E>> + 'a {
    discard(send, request("xhr", "delete", endpoint("DELETE_API_KEY")))
}
