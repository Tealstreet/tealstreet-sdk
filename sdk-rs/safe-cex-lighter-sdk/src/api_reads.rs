//! Eager SDK read bodies over imported HTTP and ECMAScript primitives.
use crate::key_runtime::KeyFuture;
use num_bigint::BigInt;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Poll, Waker},
};

pub type Fields<V> = Vec<(Vec<u16>, V)>;
/// Imports must preserve JS Get/CopyDataProperties, truthiness, coercion and exceptions.
pub trait ReadHost {
    type Value: Clone + 'static;
    type Error: Clone + 'static;
    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()>>>);
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn number_200(&self, value: &Self::Value) -> bool;
    fn string_200(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn bigint(&self, value: &Self::Value) -> Result<BigInt, Self::Error>;
    fn text(&self, value: Vec<u16>) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn object(&self, fields: Fields<Self::Value>) -> Self::Value;
    fn undefined_value(&self) -> Self::Value;
    fn bigint_value(&self, value: BigInt) -> Self::Value;
    fn empty_array(&self) -> Self::Value;
    fn copy_properties(&self, value: &Self::Value) -> Result<Fields<Self::Value>, Self::Error>;
    fn now(&self) -> Result<f64, Self::Error>;
    fn api_error(
        &self,
        message: Self::Value,
        code: Self::Value,
    ) -> Result<Self::Error, Self::Error>;
    fn error(&self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn http_get(
        &self,
        axios: bool,
        url: Vec<u16>,
        config: Self::Value,
    ) -> Result<KeyFuture<Self::Value, Self::Error>, Self::Error>;
    fn promise_resolve(&self, value: Self::Value) -> KeyFuture<Self::Value, Self::Error>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn field<V>(s: &str, v: V) -> (Vec<u16>, V) {
    (units(s), v)
}
fn retained<H: ReadHost + 'static, T: Clone + 'static>(
    host: Rc<H>,
    future: KeyFuture<T, H::Error>,
) -> KeyFuture<T, H::Error> {
    let slot = Rc::new(RefCell::new(None));
    let waiters: Rc<RefCell<Vec<Waker>>> = Rc::new(RefCell::new(Vec::new()));
    let produced = slot.clone();
    let waking = waiters.clone();
    host.spawn(Box::pin(async move {
        *produced.borrow_mut() = Some(future.await);
        for w in waking.take() {
            w.wake();
        }
    }));
    Box::pin(std::future::poll_fn(move |cx| {
        if let Some(result) = slot.borrow().as_ref() {
            return Poll::Ready(result.clone());
        }
        let mut listeners = waiters.borrow_mut();
        if !listeners.iter().any(|w| w.will_wake(cx.waker())) {
            listeners.push(cx.waker().clone());
        }
        Poll::Pending
    }))
}
async fn continuation() {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}
fn optional<H: ReadHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined_value())
    } else {
        h.get(v, k)
    }
}
fn fallback<H: ReadHost>(h: &H, v: H::Value, default: &str) -> H::Value {
    if h.truthy(&v) {
        v
    } else {
        h.text(units(default))
    }
}
fn legacy<H: ReadHost>(
    h: &H,
    response: &H::Value,
    message: &str,
    code: &str,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.get(response, "success")?) || !h.truthy(&h.get(response, "data")?) {
        let error = optional(h, response, "error")?;
        let msg = fallback(h, optional(h, &error, "message")?, message);
        let error = optional(h, response, "error")?;
        let code = fallback(h, optional(h, &error, "code")?, code);
        return Err(h.api_error(msg, code)?);
    }
    h.get(response, "data")
}
fn axios_legacy<H: ReadHost>(
    h: &H,
    response: &H::Value,
    message: &str,
    code: &str,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.get(&h.get(response, "data")?, "success")?)
        || !h.truthy(&h.get(&h.get(response, "data")?, "data")?)
    {
        let error = h.get(&h.get(response, "data")?, "error")?;
        let msg = fallback(h, optional(h, &error, "message")?, message);
        let error = h.get(&h.get(response, "data")?, "error")?;
        let code = fallback(h, optional(h, &error, "code")?, code);
        return Err(h.api_error(msg, code)?);
    }
    h.get(&h.get(response, "data")?, "data")
}
fn config<H: ReadHost>(h: &H, params: Fields<H::Value>, auth: Option<Vec<u16>>) -> H::Value {
    let mut fields = vec![field("params", h.object(params))];
    if let Some(auth) = auth {
        let mut bearer = units("Bearer ");
        bearer.extend(auth);
        fields.push(field(
            "headers",
            h.object(vec![field("Authorization", h.text(bearer))]),
        ));
    }
    h.object(fields)
}
pub struct ReferralApi<H: ReadHost> {
    pub client: Rc<H>,
}
impl<H: ReadHost + 'static> ReferralApi<H> {
    pub fn new(client: Rc<H>) -> Self {
        Self { client }
    }
    pub fn get_referral_points(
        &self,
        account_index: BigInt,
        authorization: Option<Vec<u16>>,
    ) -> KeyFuture<H::Value, H::Error> {
        let h = self.client.clone();
        let auth = authorization.filter(|a| !a.is_empty());
        let cfg = config(
            h.as_ref(),
            vec![field(
                "accountIndex",
                h.text(units(&account_index.to_string())),
            )],
            auth,
        );
        let request = h.http_get(false, units("/api/v1/referral/points"), cfg);
        retained(
            h.clone(),
            Box::pin(async move {
                let response = request?.await?;
                continuation().await;
                let data = legacy(
                    h.as_ref(),
                    &response,
                    "Failed to get referral points",
                    "GET_REFERRAL_POINTS_FAILED",
                )?;
                h.promise_resolve(data).await
            }),
        )
    }
}
pub fn get_next_nonce<H: ReadHost + 'static>(
    h: Rc<H>,
    account_index: BigInt,
    api_key_index: f64,
) -> KeyFuture<BigInt, H::Error> {
    let cfg = config(
        h.as_ref(),
        vec![
            field("account_index", h.text(units(&account_index.to_string()))),
            field("api_key_index", h.number(api_key_index)),
        ],
        None,
    );
    let request = h.http_get(true, units("/api/v1/nextNonce"), cfg);
    retained(
        h.clone(),
        Box::pin(async move {
            let response = request?.await?;
            continuation().await;
            let payload = h.get(&response, "data")?;
            if h.truthy(&payload)
                && (h.number_200(&h.get(&payload, "code")?)
                    || h.string_200(&h.get(&payload, "code")?))
                && !h.undefined(&h.get(&payload, "nonce")?)
            {
                return h.bigint(&h.get(&payload, "nonce")?);
            }
            if h.truthy(&h.get(&payload, "success")?)
                && !h.undefined(&optional(
                    h.as_ref(),
                    &h.get(&payload, "data")?,
                    "nextNonce",
                )?)
            {
                return h.bigint(&h.get(&h.get(&payload, "data")?, "nextNonce")?);
            }
            let msg = optional(h.as_ref(), &payload, "message")?;
            let msg = if h.truthy(&msg) {
                msg
            } else {
                optional(
                    h.as_ref(),
                    &optional(h.as_ref(), &payload, "error")?,
                    "message",
                )?
            };
            let msg = fallback(h.as_ref(), msg, "Failed to get nonce");
            let code = optional(h.as_ref(), &payload, "code")?;
            let code = if h.truthy(&code) {
                code
            } else {
                optional(
                    h.as_ref(),
                    &optional(h.as_ref(), &payload, "error")?,
                    "code",
                )?
            };
            let code = fallback(h.as_ref(), code, "GET_NONCE_FAILED");
            let code = h.text(h.string(&code)?);
            Err(h.api_error(msg, code)?)
        }),
    )
}
pub fn get_api_keys<H: ReadHost + 'static>(
    h: Rc<H>,
    account_index: BigInt,
    api_key_index: f64,
) -> KeyFuture<H::Value, H::Error> {
    let url = units(&format!("/api/v1/accounts/{account_index}/api-keys"));
    let cfg = config(
        h.as_ref(),
        vec![field("apiKeyIndex", h.number(api_key_index))],
        None,
    );
    let request = h.http_get(true, url, cfg);
    retained(
        h.clone(),
        Box::pin(async move {
            let response = request?.await?;
            continuation().await;
            let data = axios_legacy(
                h.as_ref(),
                &response,
                "Failed to get API keys",
                "GET_API_KEYS_FAILED",
            )?;
            h.promise_resolve(h.get(&data, "apiKeys")?).await
        }),
    )
}
pub fn get_transfer_fee_info<H: ReadHost + 'static>(
    h: Rc<H>,
    account_index: BigInt,
    to_account_index: BigInt,
    auth: Vec<u16>,
) -> KeyFuture<H::Value, H::Error> {
    let url = units(&format!("/api/v1/accounts/{account_index}/transfer-fee"));
    let cfg = config(
        h.as_ref(),
        vec![field(
            "toAccountIndex",
            h.text(units(&to_account_index.to_string())),
        )],
        Some(auth),
    );
    let request = h.http_get(true, url, cfg);
    retained(
        h.clone(),
        Box::pin(async move {
            let response = request?.await?;
            continuation().await;
            let data = axios_legacy(
                h.as_ref(),
                &response,
                "Failed to get transfer fee info",
                "GET_TRANSFER_FEE_FAILED",
            )?;
            let fee = h.bigint_value(h.bigint(&h.get(&data, "fee")?)?);
            let min = h.bigint_value(h.bigint(&h.get(&data, "minAmount")?)?);
            let max = h.bigint_value(h.bigint(&h.get(&data, "maxAmount")?)?);
            let percentage = h.get(&data, "feePercentage")?;
            h.promise_resolve(h.object(vec![
                field("fee", fee),
                field("minAmount", min),
                field("maxAmount", max),
                field("feePercentage", percentage),
            ]))
            .await
        }),
    )
}
pub fn get_order_book_orders<H: ReadHost + 'static>(
    h: Rc<H>,
    market_id: f64,
    limit: Option<f64>,
    request_config: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    let prepared = (|| {
        let limit = limit.filter(|n| *n != 0.0 && !n.is_nan()).unwrap_or(50.0);
        let mut outer = h.copy_properties(&request_config)?;
        let params = optional(h.as_ref(), &request_config, "params")?;
        let mut fields = vec![field(
            "limit",
            h.text(units(&safe_cex_num::js_number_string(limit))),
        )];
        if h.truthy(&params) {
            fields.extend(h.copy_properties(&params)?);
        }
        fields.push(field(
            "market_id",
            h.text(units(&safe_cex_num::js_number_string(market_id))),
        ));
        outer.push(field("params", h.object(fields)));
        h.http_get(false, units("/api/v1/orderBookOrders"), h.object(outer))
    })();
    retained(
        h.clone(),
        Box::pin(async move {
            let response = prepared?.await?;
            continuation().await;
            if h.truthy(&optional(h.as_ref(), &response, "success")?)
                && h.truthy(&h.get(&response, "data")?)
            {
                return h.promise_resolve(h.get(&response, "data")?).await;
            }
            if h.number_200(&optional(h.as_ref(), &response, "code")?)
                && (h.is_array(&h.get(&response, "bids")?)
                    || h.is_array(&h.get(&response, "asks")?))
            {
                let bids = h.get(&response, "bids")?;
                let bids = if h.truthy(&bids) {
                    bids
                } else {
                    h.empty_array()
                };
                let asks = h.get(&response, "asks")?;
                let asks = if h.truthy(&asks) {
                    asks
                } else {
                    h.empty_array()
                };
                let timestamp = h.get(&response, "timestamp")?;
                let timestamp = if h.truthy(&timestamp) {
                    timestamp
                } else {
                    h.number(h.now()?)
                };
                return h
                    .promise_resolve(h.object(vec![
                        field("marketId", h.number(market_id)),
                        field("bids", bids),
                        field("asks", asks),
                        field("timestamp", timestamp),
                    ]))
                    .await;
            }
            let error = optional(h.as_ref(), &response, "error")?;
            let message = optional(h.as_ref(), &error, "message")?;
            let message = if h.truthy(&message) {
                message
            } else {
                optional(h.as_ref(), &response, "message")?
            };
            Err(h.error(fallback(
                h.as_ref(),
                message,
                "Failed to get order book orders",
            ))?)
        }),
    )
}
