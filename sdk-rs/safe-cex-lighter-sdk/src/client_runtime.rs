//! HTTP-client bodies over current receiver members and original runtime imports.
use crate::{
    api_reads::{Fields, ReadHost},
    key_runtime::KeyFuture,
};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Poll, Waker},
};
pub type NativeMethod<H> = Rc<
    dyn Fn(
        <H as ReadHost>::Value,
        Vec<<H as ReadHost>::Value>,
    ) -> Result<<H as ReadHost>::Value, <H as ReadHost>::Error>,
>;
/// Imports preserve Get/Call/iteration, constructors, coercion and original helper effects.
pub trait ClientHost: ReadHost {
    fn boolean(&self, value: bool) -> Self::Value;
    fn function(&self, body: NativeMethod<Self>) -> Self::Value
    where
        Self: Sized;
    fn call(
        &self,
        function: &Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn call_method(
        &self,
        receiver: &Self::Value,
        member: &str,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn receiver(&self, methods: Fields<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn existing_method(&self, name: &str) -> Result<Self::Value, Self::Error>;
    fn define_field(
        &self,
        receiver: &Self::Value,
        key: &str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn set(&self, receiver: &Self::Value, key: &str, value: Self::Value)
    -> Result<(), Self::Error>;
    fn axios_factory(&self) -> Result<Self::Value, Self::Error>;
    fn call_bound(
        &self,
        function: &Self::Value,
        receiver: &Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn iterator(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&self, iterator: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn logger(&self, error: bool) -> Result<Self::Value, Self::Error>;
    fn reject(&self, reason: Self::Value) -> Result<Self::Value, Self::Error>;
    fn publish(&self, future: KeyFuture<Self::Value, Self::Error>) -> Self::Value;
    fn api_error_value(
        &self,
        message: Self::Value,
        code: Self::Value,
        details: Self::Value,
        request_id: Self::Value,
        status: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn string_length(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn strict_number(&self, value: &Self::Value, n: f64) -> bool;
    fn strict_text(&self, value: &Self::Value, s: &str) -> bool;
    fn template_string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn safe_number(&self, value: Self::Value, label: &str) -> Result<Self::Value, Self::Error>;
    fn safe_integer_bigint(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn signature_base64(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn tx_type_table(&self) -> Self::Value;
    fn computed_get(
        &self,
        object: &Self::Value,
        key: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn now_value(&self) -> Result<Self::Value, Self::Error>;
    fn add_number(&self, value: Self::Value, number: f64) -> Result<Self::Value, Self::Error>;
    fn bigint_call(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn bigint_zero(&self) -> Self::Value;
    fn number_call(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn stringify(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn url_search_params_factory(&self) -> Result<Self::Value, Self::Error>;
    fn json_stringifier(&self) -> Result<Self::Value, Self::Error>;
    fn every_string(&self, value: &Self::Value) -> Result<bool, Self::Error>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn field<V>(s: &str, v: V) -> (Vec<u16>, V) {
    (units(s), v)
}
fn text<H: ReadHost>(h: &H, s: &str) -> H::Value {
    h.text(units(s))
}
fn object<H: ReadHost>(h: &H, fields: Vec<(&str, H::Value)>) -> H::Value {
    h.object(fields.into_iter().map(|(k, v)| field(k, v)).collect())
}
fn optional<H: ReadHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined_value())
    } else {
        h.get(v, k)
    }
}
fn nullish<H: ReadHost>(h: &H, v: H::Value, default: impl FnOnce() -> H::Value) -> H::Value {
    if h.nullish(&v) { default() } else { v }
}
fn fallback<H: ReadHost>(h: &H, v: H::Value, default: impl FnOnce() -> H::Value) -> H::Value {
    if h.truthy(&v) { v } else { default() }
}
fn ready<T: 'static, E: 'static>(result: Result<T, E>) -> KeyFuture<T, E> {
    Box::pin(async move { result })
}
fn retained<H: ReadHost + 'static>(
    h: Rc<H>,
    future: KeyFuture<H::Value, H::Error>,
) -> KeyFuture<H::Value, H::Error> {
    let slot = Rc::new(RefCell::new(None));
    let waiters: Rc<RefCell<Vec<Waker>>> = Rc::new(RefCell::new(vec![]));
    let produced = slot.clone();
    let waking = waiters.clone();
    h.spawn(Box::pin(async move {
        *produced.borrow_mut() = Some(future.await);
        for w in waking.take() {
            w.wake();
        }
    }));
    Box::pin(std::future::poll_fn(move |cx| {
        if let Some(r) = slot.borrow().as_ref() {
            return Poll::Ready(r.clone());
        }
        let mut w = waiters.borrow_mut();
        if !w.iter().any(|w| w.will_wake(cx.waker())) {
            w.push(cx.waker().clone());
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
pub struct LighterApiClient<H: ClientHost> {
    pub host: Rc<H>,
    pub receiver: H::Value,
}
impl<H: ClientHost + 'static> LighterApiClient<H> {
    pub fn new(h: Rc<H>, config: H::Value) -> Result<Self, H::Error> {
        // Keep registrations explicit so production source discovery sees each callback.
        let mut methods = Vec::new();
        for name in [
            "addRequestInterceptor",
            "addResponseInterceptor",
            "addErrorInterceptor",
            "getNextNonce",
            "getApiKeys",
            "getTransferFeeInfo",
            "get",
            "post",
            "put",
            "delete",
            "getBaseUrl",
            "getConfig",
        ] {
            methods.push(field(name, h.existing_method(name)?));
        }
        {
            let runtime = h.clone();
            methods.push(field(
                "setupDefaultInterceptors",
                h.function(Rc::new(move |r, _| {
                    setup_default_interceptors(runtime.clone(), r)?;
                    Ok(runtime.undefined_value())
                })),
            ));
        }
        {
            let runtime = h.clone();
            let callback = h.function(Rc::new(move |r, args| {
                handle_api_error(
                    runtime.clone(),
                    r,
                    args.first()
                        .cloned()
                        .unwrap_or_else(|| runtime.undefined_value()),
                )
            }));
            methods.push(field("handleApiError", callback));
        }
        {
            let runtime = h.clone();
            let callback = h.function(Rc::new(move |_, a| {
                build_l2_tx_attributes_payload(
                    runtime.as_ref(),
                    a.first()
                        .cloned()
                        .unwrap_or_else(|| runtime.undefined_value()),
                )
            }));
            methods.push(field("buildL2TxAttributesPayload", callback));
        }
        {
            let runtime = h.clone();
            let callback = h.function(Rc::new(move |_, a| {
                build_order_info_payload(
                    runtime.as_ref(),
                    a.first()
                        .cloned()
                        .unwrap_or_else(|| runtime.undefined_value()),
                )
            }));
            methods.push(field("buildOrderInfoPayload", callback));
        }
        {
            let runtime = h.clone();
            let callback = h.function(Rc::new(move |r, args| {
                serialize_tx_info(
                    runtime.clone(),
                    r,
                    args.first()
                        .cloned()
                        .unwrap_or_else(|| runtime.undefined_value()),
                )
            }));
            methods.push(field("serializeTxInfo", callback));
        }
        {
            let runtime = h.clone();
            methods.push(field(
                "sendRawTx",
                h.function(Rc::new(move |r, a| {
                    Ok(runtime.publish(send_raw_tx(
                        runtime.clone(),
                        r,
                        a.first()
                            .cloned()
                            .unwrap_or_else(|| runtime.undefined_value()),
                    )))
                })),
            ));
        }
        {
            let runtime = h.clone();
            methods.push(field(
                "sendRawTxBatch",
                h.function(Rc::new(move |r, a| {
                    Ok(runtime.publish(send_raw_tx_batch(
                        runtime.clone(),
                        r,
                        a.first()
                            .cloned()
                            .unwrap_or_else(|| runtime.undefined_value()),
                    )))
                })),
            ));
        }
        let receiver = h.receiver(methods)?;
        constructor(h.clone(), receiver.clone(), config)?;
        Ok(Self { host: h, receiver })
    }
}
pub fn constructor<H: ClientHost + 'static>(
    h: Rc<H>,
    r: H::Value,
    config: H::Value,
) -> Result<(), H::Error> {
    h.define_field(&r, "axiosInstance", h.undefined_value())?;
    h.define_field(&r, "config", h.undefined_value())?;
    h.define_field(
        &r,
        "interceptors",
        object(
            h.as_ref(),
            vec![
                ("request", h.empty_array()),
                ("response", h.empty_array()),
                ("error", h.empty_array()),
            ],
        ),
    )?;
    let normalized = object(
        h.as_ref(),
        vec![
            ("baseUrl", h.get(&config, "baseUrl")?),
            (
                "timeout",
                nullish(h.as_ref(), h.get(&config, "timeout")?, || h.number(30000.0)),
            ),
            (
                "headers",
                nullish(h.as_ref(), h.get(&config, "headers")?, || h.object(vec![])),
            ),
            (
                "debug",
                nullish(h.as_ref(), h.get(&config, "debug")?, || h.boolean(false)),
            ),
        ],
    );
    h.set(&r, "config", normalized)?;
    let factory = h.axios_factory()?;
    let base = h.get(&h.get(&r, "config")?, "baseUrl")?;
    let timeout = h.get(&h.get(&r, "config")?, "timeout")?;
    let mut headers = vec![field("Content-Type", text(h.as_ref(), "application/json"))];
    headers.extend(h.copy_properties(&h.get(&h.get(&r, "config")?, "headers")?)?);
    let axios = h.call(
        &factory,
        vec![object(
            h.as_ref(),
            vec![
                ("baseURL", base),
                ("timeout", timeout),
                ("headers", h.object(headers)),
            ],
        )],
    )?;
    h.set(&r, "axiosInstance", axios)?;
    h.call_method(&r, "setupDefaultInterceptors", vec![])?;
    Ok(())
}
pub fn setup_default_interceptors<H: ClientHost + 'static>(
    h: Rc<H>,
    r: H::Value,
) -> Result<(), H::Error> {
    let request = h.get(
        &h.get(&h.get(&r, "axiosInstance")?, "interceptors")?,
        "request",
    )?;
    let success = {
        let h = h.clone();
        let r = r.clone();
        let owner = h.clone();
        owner.function(Rc::new(move |_, a| {
            intercept(
                h.clone(),
                r.clone(),
                a.first().cloned().unwrap_or_else(|| h.undefined_value()),
                0,
            )
        }))
    };
    let failure = {
        let h = h.clone();
        let r = r.clone();
        let owner = h.clone();
        owner.function(Rc::new(move |_, a| {
            intercept(
                h.clone(),
                r.clone(),
                a.first().cloned().unwrap_or_else(|| h.undefined_value()),
                1,
            )
        }))
    };
    h.call_method(&request, "use", vec![success, failure])?;
    let response = h.get(
        &h.get(&h.get(&r, "axiosInstance")?, "interceptors")?,
        "response",
    )?;
    let success = {
        let h = h.clone();
        let r = r.clone();
        let owner = h.clone();
        owner.function(Rc::new(move |_, a| {
            intercept(
                h.clone(),
                r.clone(),
                a.first().cloned().unwrap_or_else(|| h.undefined_value()),
                2,
            )
        }))
    };
    let failure = {
        let h = h.clone();
        let r = r.clone();
        let owner = h.clone();
        owner.function(Rc::new(move |_, a| {
            intercept(
                h.clone(),
                r.clone(),
                a.first().cloned().unwrap_or_else(|| h.undefined_value()),
                3,
            )
        }))
    };
    h.call_method(&response, "use", vec![success, failure])?;
    Ok(())
}
fn intercept<H: ClientHost + 'static>(
    h: Rc<H>,
    r: H::Value,
    value: H::Value,
    kind: u8,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.get(&h.get(&r, "config")?, "debug")?) {
        let error = kind == 1 || kind == 3;
        let logger = h.logger(error)?;
        let (_, label, detail) = match kind {
            0 => (
                false,
                "[LighterApiClient] Request:",
                object(
                    h.as_ref(),
                    vec![
                        ("method", h.get(&value, "method")?),
                        ("url", h.get(&value, "url")?),
                        ("params", h.get(&value, "params")?),
                        ("data", h.get(&value, "data")?),
                    ],
                ),
            ),
            1 => (true, "[LighterApiClient] Request Error:", value.clone()),
            2 => (
                false,
                "[LighterApiClient] Response:",
                object(
                    h.as_ref(),
                    vec![
                        ("status", h.get(&value, "status")?),
                        ("data", h.get(&value, "data")?),
                    ],
                ),
            ),
            _ => (true, "[LighterApiClient] Response Error:", value.clone()),
        };
        h.call(&logger, vec![text(h.as_ref(), label), detail])?;
    }
    if kind == 1 {
        return h.reject(value);
    }
    let key = match kind {
        0 => "request",
        2 => "response",
        _ => "error",
    };
    let callbacks = h.get(&h.get(&r, "interceptors")?, key)?;
    let callbacks = nullish(h.as_ref(), callbacks, || h.empty_array());
    let iter = h.iterator(callbacks)?;
    let mut modified = value;
    while let Some(callback) = h.next(&iter)? {
        let returned = h.call(&callback, vec![modified.clone()])?;
        if h.truthy(&returned) {
            modified = returned;
        }
    }
    if kind == 3 {
        let error = h.call_method(&r, "handleApiError", vec![modified])?;
        h.reject(error)
    } else {
        Ok(modified)
    }
}
pub fn handle_api_error<H: ClientHost + 'static>(
    h: Rc<H>,
    _: H::Value,
    error: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.get(&error, "response")?) {
        let response = h.get(&error, "response")?;
        let body = h.get(&response, "data")?;
        let explicit = optional(h.as_ref(), &body, "error")?;
        let message = optional(h.as_ref(), &body, "message")?;
        let code = optional(h.as_ref(), &body, "code")?;
        let data = if h.truthy(&explicit) {
            explicit
        } else {
            let code = if h.truthy(&code) {
                h.text(h.string(&code)?)
            } else {
                text(h.as_ref(), "HTTP_ERROR")
            };
            let flat = if h.is_string(&message) && h.string_length(&message)? > 0.0 {
                message
            } else {
                h.undefined_value()
            };
            let message = if h.truthy(&flat) {
                flat
            } else {
                fallback(h.as_ref(), h.get(&error, "message")?, || {
                    text(h.as_ref(), "HTTP request failed")
                })
            };
            object(h.as_ref(), vec![("code", code), ("message", message)])
        };
        return h.api_error_value(
            h.get(&data, "message")?,
            h.get(&data, "code")?,
            h.get(&data, "details")?,
            h.get(&data, "requestId")?,
            h.get(&response, "status")?,
        );
    }
    if h.truthy(&h.get(&error, "request")?) {
        return h.api_error_value(
            text(h.as_ref(), "No response received from server"),
            text(h.as_ref(), "NO_RESPONSE"),
            object(
                h.as_ref(),
                vec![("originalError", h.get(&error, "message")?)],
            ),
            h.undefined_value(),
            h.undefined_value(),
        );
    }
    let message = fallback(h.as_ref(), h.get(&error, "message")?, || {
        text(h.as_ref(), "Request setup failed")
    });
    let original = h.call_method(&error, "toString", vec![])?;
    h.api_error_value(
        message,
        text(h.as_ref(), "REQUEST_SETUP_ERROR"),
        object(h.as_ref(), vec![("originalError", original)]),
        h.undefined_value(),
        h.undefined_value(),
    )
}
pub fn build_l2_tx_attributes_payload<H: ClientHost>(
    h: &H,
    p: H::Value,
) -> Result<H::Value, H::Error> {
    if h.undefined(&h.get(&p, "integratorAccountIndex")?)
        || h.undefined(&h.get(&p, "integratorMakerFee")?)
        || h.undefined(&h.get(&p, "integratorTakerFee")?)
    {
        return Ok(h.undefined_value());
    }
    Ok(object(
        h,
        vec![
            (
                "1",
                h.safe_number(
                    h.get(&p, "integratorAccountIndex")?,
                    "integratorAccountIndex",
                )?,
            ),
            ("2", h.get(&p, "integratorTakerFee")?),
            ("3", h.get(&p, "integratorMakerFee")?),
        ],
    ))
}
fn expiry<H: ClientHost>(h: &H, add: f64) -> Result<H::Value, H::Error> {
    let now = h.now_value()?;
    let sum = h.add_number(now, add)?;
    h.bigint_call(sum)
}
pub fn build_order_info_payload<H: ClientHost>(h: &H, p: H::Value) -> Result<H::Value, H::Error> {
    let default = expiry(h, 2419200000.0)?;
    let order_type = nullish(h, h.get(&p, "orderTypeValue")?, || h.number(0.0));
    let tif = nullish(h, h.get(&p, "timeInForceValue")?, || {
        h.number(if h.strict_number(&order_type, 1.0) {
            0.0
        } else {
            1.0
        })
    });
    let trigger = nullish(h, h.get(&p, "triggerPrice")?, || h.bigint_zero());
    let conditional = [2.0, 3.0, 4.0, 5.0]
        .iter()
        .any(|n| h.strict_number(&order_type, *n));
    let immediate = h.strict_number(&tif, 0.0) && !conditional;
    let expiration = nullish(h, h.get(&p, "orderExpiry")?, || {
        if immediate { h.bigint_zero() } else { default }
    });
    Ok(object(
        h,
        vec![
            ("MarketIndex", h.get(&p, "marketId")?),
            (
                "ClientOrderIndex",
                h.safe_number(h.get(&p, "orderId")?, "clientOrderIndex")?,
            ),
            (
                "BaseAmount",
                h.safe_number(h.get(&p, "amount")?, "baseAmount")?,
            ),
            ("Price", h.safe_number(h.get(&p, "price")?, "price")?),
            (
                "IsAsk",
                h.number(if h.truthy(&h.get(&p, "isBuy")?) {
                    0.0
                } else {
                    1.0
                }),
            ),
            ("Type", order_type),
            ("TimeInForce", tif),
            (
                "ReduceOnly",
                h.number(if h.truthy(&h.get(&p, "reduceOnly")?) {
                    1.0
                } else {
                    0.0
                }),
            ),
            ("TriggerPrice", h.safe_number(trigger, "triggerPrice")?),
            ("OrderExpiry", h.safe_number(expiration, "orderExpiry")?),
        ],
    ))
}
fn attributes<H: ClientHost>(
    h: &H,
    r: &H::Value,
    p: &H::Value,
    fields: &mut Fields<H::Value>,
) -> Result<(), H::Error> {
    let test = h.call_method(r, "buildL2TxAttributesPayload", vec![p.clone()])?;
    if h.truthy(&test) {
        fields.push(field(
            "L2TxAttributes",
            h.call_method(r, "buildL2TxAttributesPayload", vec![p.clone()])?,
        ));
    }
    Ok(())
}
fn common<H: ClientHost>(h: &H, tx: &H::Value) -> Result<Fields<H::Value>, H::Error> {
    Ok(vec![
        field(
            "AccountIndex",
            h.safe_number(h.get(tx, "accountIndex")?, "accountIndex")?,
        ),
        field("ApiKeyIndex", h.get(tx, "apiKeyIndex")?),
    ])
}
fn ending<H: ClientHost>(
    h: &H,
    tx: &H::Value,
    expired: H::Value,
    fields: &mut Fields<H::Value>,
) -> Result<(), H::Error> {
    fields.push(field("ExpiredAt", h.safe_number(expired, "expiredAt")?));
    fields.push(field("Nonce", h.safe_number(h.get(tx, "nonce")?, "nonce")?));
    fields.push(field("Sig", h.signature_base64(h.get(tx, "signature")?)?));
    Ok(())
}
pub fn serialize_tx_info<H: ClientHost + 'static>(
    h: Rc<H>,
    r: H::Value,
    tx: H::Value,
) -> Result<H::Value, H::Error> {
    let tx_type = h.computed_get(&h.tx_type_table(), h.get(&tx, "type")?)?;
    if !h.truthy(&tx_type) {
        let mut m = units("Unsupported tx type for sendTx: ");
        m.extend(h.template_string(&h.get(&tx, "type")?)?);
        return Err(h.api_error(h.text(m), text(h.as_ref(), "UNSUPPORTED_TX_TYPE"))?);
    }
    let default = expiry(h.as_ref(), 600000.0)?;
    let expired = nullish(h.as_ref(), h.get(&tx, "expiredAt")?, || default);
    let create = h.get(&tx, "orderDetails")?;
    let grouped = h.get(&tx, "groupedOrderDetails")?;
    let cancel = h.get(&tx, "cancelDetails")?;
    let leverage = h.get(&tx, "leverageDetails")?;
    let approve = h.get(&tx, "approveIntegratorDetails")?;
    let mut json = h.undefined_value();
    let mut payload = h.undefined_value();
    if h.strict_text(&h.get(&tx, "type")?, "create_order") && h.truthy(&create) {
        let mut f = common(h.as_ref(), &tx)?;
        let order = h.call_method(&r, "buildOrderInfoPayload", vec![create.clone()])?;
        f.extend(h.copy_properties(&order)?);
        ending(h.as_ref(), &tx, expired.clone(), &mut f)?;
        attributes(h.as_ref(), &r, &create, &mut f)?;
        payload = h.object(f);
    } else if h.strict_text(&h.get(&tx, "type")?, "create_grouped_orders") && h.truthy(&grouped) {
        let mut f = common(h.as_ref(), &tx)?;
        f.push(field("GroupingType", h.get(&grouped, "groupingType")?));
        let orders = h.get(&grouped, "orders")?;
        let runtime = h.clone();
        let owner = r.clone();
        let map = h.function(Rc::new(move |_, a| {
            runtime.call_method(
                &owner,
                "buildOrderInfoPayload",
                vec![
                    a.first()
                        .cloned()
                        .unwrap_or_else(|| runtime.undefined_value()),
                ],
            )
        }));
        f.push(field("Orders", h.call_method(&orders, "map", vec![map])?));
        ending(h.as_ref(), &tx, expired.clone(), &mut f)?;
        attributes(h.as_ref(), &r, &grouped, &mut f)?;
        payload = h.object(f);
    } else if h.strict_text(&h.get(&tx, "type")?, "cancel_order") && h.truthy(&cancel) {
        let mut f = common(h.as_ref(), &tx)?;
        let market = h.get(&cancel, "marketId")?;
        let expire = nullish(h.as_ref(), h.get(&cancel, "expiredAt")?, || expired.clone());
        let expire = h.safe_number(expire, "expiredAt")?;
        let nonce = h.safe_number(h.get(&tx, "nonce")?, "nonce")?;
        let signature = h.signature_base64(h.get(&tx, "signature")?)?;
        if h.truthy(&h.safe_integer_bigint(h.get(&cancel, "orderId")?)?) {
            f.push(field("MarketIndex", market));
            f.push(field("Index", h.number_call(h.get(&cancel, "orderId")?)?));
            f.push(field("ExpiredAt", expire));
            f.push(field("Nonce", nonce));
            f.push(field("Sig", signature));
            payload = h.object(f);
        } else {
            let mut raw = units("{\"AccountIndex\":");
            raw.extend(h.template_string(&f[0].1)?);
            raw.extend(units(",\"ApiKeyIndex\":"));
            raw.extend(h.template_string(&f[1].1)?);
            raw.extend(units(",\"MarketIndex\":"));
            raw.extend(h.template_string(&market)?);
            raw.extend(units(",\"Index\":"));
            let index = h.call_method(&h.get(&cancel, "orderId")?, "toString", vec![])?;
            raw.extend(h.template_string(&index)?);
            raw.extend(units(",\"ExpiredAt\":"));
            raw.extend(h.template_string(&expire)?);
            raw.extend(units(",\"Nonce\":"));
            raw.extend(h.template_string(&nonce)?);
            raw.extend(units(",\"Sig\":"));
            raw.extend(h.template_string(&h.stringify(signature)?)?);
            raw.push(125);
            json = h.text(raw);
        }
    } else if h.strict_text(&h.get(&tx, "type")?, "update_leverage") && h.truthy(&leverage) {
        let mut f = common(h.as_ref(), &tx)?;
        let market = h.get(&leverage, "marketId")?;
        let margin = h.get(&leverage, "initialMarginFraction")?;
        let mode = h.get(&leverage, "marginMode")?;
        let expire = nullish(h.as_ref(), h.get(&leverage, "expiredAt")?, || {
            expired.clone()
        });
        let expire = h.safe_number(expire, "expiredAt")?;
        let nonce = h.safe_number(h.get(&tx, "nonce")?, "nonce")?;
        let signature = h.signature_base64(h.get(&tx, "signature")?)?;
        f.extend([
            field("MarketIndex", market),
            field("InitialMarginFraction", margin),
            field("MarginMode", mode),
            field("ExpiredAt", expire),
            field("Nonce", nonce),
            field("Sig", signature),
        ]);
        payload = h.object(f);
    } else if h.strict_text(&h.get(&tx, "type")?, "approve_integrator") && h.truthy(&approve) {
        let mut f = common(h.as_ref(), &tx)?;
        f.push(field(
            "IntegratorAccountIndex",
            h.safe_number(
                h.get(&approve, "integratorAccountIndex")?,
                "integratorAccountIndex",
            )?,
        ));
        for (key, out) in [
            ("maxPerpsTakerFee", "MaxPerpsTakerFee"),
            ("maxPerpsMakerFee", "MaxPerpsMakerFee"),
            ("maxSpotTakerFee", "MaxSpotTakerFee"),
            ("maxSpotMakerFee", "MaxSpotMakerFee"),
        ] {
            f.push(field(out, h.get(&approve, key)?));
        }
        f.push(field(
            "ApprovalExpiry",
            h.safe_number(h.get(&approve, "approvalExpiry")?, "approvalExpiry")?,
        ));
        let expire = nullish(h.as_ref(), h.get(&approve, "expiredAt")?, || {
            expired.clone()
        });
        ending(h.as_ref(), &tx, expire, &mut f)?;
        if h.truthy(&h.get(&approve, "l1Signature")?) {
            f.push(field("L1Sig", h.get(&approve, "l1Signature")?));
        }
        payload = h.object(f);
    } else {
        let mut f = common(h.as_ref(), &tx)?;
        f.push(field(
            "Nonce",
            h.safe_number(h.get(&tx, "nonce")?, "nonce")?,
        ));
        f.push(field("Data", h.get(&tx, "data")?));
        f.push(field("Sig", h.signature_base64(h.get(&tx, "signature")?)?));
        payload = h.object(f);
    }
    if !h.truthy(&json) {
        json = h.stringify(payload)?;
    }
    Ok(object(
        h.as_ref(),
        vec![("txType", tx_type), ("txInfoJson", json)],
    ))
}
fn send_error<H: ClientHost>(
    h: &H,
    p: &H::Value,
    message: &str,
    code: &str,
) -> Result<H::Error, H::Error> {
    let m = optional(h, p, "message")?;
    let m = if h.truthy(&m) {
        m
    } else {
        optional(h, &h.get(p, "error")?, "message")?
    };
    let m = fallback(h, m, || text(h, message));
    let c = optional(h, p, "code")?;
    let c = if h.truthy(&c) {
        c
    } else {
        optional(h, &h.get(p, "error")?, "code")?
    };
    let c = fallback(h, c, || text(h, code));
    h.api_error(m, h.text(h.string(&c)?))
}
pub fn send_raw_tx<H: ClientHost + 'static>(
    h: Rc<H>,
    r: H::Value,
    tx: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    let prepared = (|| {
        let serialized = h.call_method(&r, "serializeTxInfo", vec![tx])?;
        let typ = h.get(&serialized, "txType")?;
        let json = h.get(&serialized, "txInfoJson")?;
        let factory = h.url_search_params_factory()?;
        let body = h.call(
            &factory,
            vec![object(
                h.as_ref(),
                vec![("tx_type", h.text(h.string(&typ)?)), ("tx_info", json)],
            )],
        )?;
        let axios = h.get(&r, "axiosInstance")?;
        let post = h.get(&axios, "post")?;
        let body = h.call_method(&body, "toString", vec![])?;
        h.call_bound(
            &post,
            &axios,
            vec![
                text(h.as_ref(), "/api/v1/sendTx"),
                body,
                object(
                    h.as_ref(),
                    vec![(
                        "headers",
                        object(
                            h.as_ref(),
                            vec![(
                                "Content-Type",
                                text(h.as_ref(), "application/x-www-form-urlencoded"),
                            )],
                        ),
                    )],
                ),
            ],
        )
    })();
    let pending = match prepared {
        Ok(v) => h.promise_resolve(v),
        Err(e) => return ready(Err(e)),
    };
    retained(
        h.clone(),
        Box::pin(async move {
            let response = pending.await;
            continuation().await;
            let response = response?;
            let p = h.get(&response, "data")?;
            if (h.number_200(&h.get(&p, "code")?) || h.string_200(&h.get(&p, "code")?))
                && h.is_string(&h.get(&p, "tx_hash")?)
            {
                return h.promise_resolve(h.get(&p, "tx_hash")?).await;
            }
            if h.truthy(&h.get(&p, "success")?)
                && h.is_string(&optional(h.as_ref(), &h.get(&p, "data")?, "txHash")?)
            {
                return h
                    .promise_resolve(h.get(&h.get(&p, "data")?, "txHash")?)
                    .await;
            }
            Err(send_error(
                h.as_ref(),
                &p,
                "Failed to send transaction",
                "SEND_TX_FAILED",
            )?)
        }),
    )
}
pub fn send_raw_tx_batch<H: ClientHost + 'static>(
    h: Rc<H>,
    r: H::Value,
    txs: H::Value,
) -> KeyFuture<H::Value, H::Error> {
    let prepared = (|| {
        if h.strict_number(&h.get(&txs, "length")?, 0.0) {
            return Ok(None);
        }
        let runtime = h.clone();
        let owner = r.clone();
        let callback = h.function(Rc::new(move |_, a| {
            runtime.call_method(
                &owner,
                "serializeTxInfo",
                vec![
                    a.first()
                        .cloned()
                        .unwrap_or_else(|| runtime.undefined_value()),
                ],
            )
        }));
        let serialized = h.call_method(&txs, "map", vec![callback])?;
        let factory = h.url_search_params_factory()?;
        let stringifier = h.json_stringifier()?;
        let runtime = h.clone();
        let types = h.function(Rc::new(move |_, a| runtime.get(&a[0], "txType")));
        let types = h.call_method(&serialized, "map", vec![types])?;
        let types = h.call(&stringifier, vec![types])?;
        let stringifier = h.json_stringifier()?;
        let runtime = h.clone();
        let infos = h.function(Rc::new(move |_, a| runtime.get(&a[0], "txInfoJson")));
        let infos = h.call_method(&serialized, "map", vec![infos])?;
        let infos = h.call(&stringifier, vec![infos])?;
        let body = h.call(
            &factory,
            vec![object(
                h.as_ref(),
                vec![("tx_types", types), ("tx_infos", infos)],
            )],
        )?;
        let axios = h.get(&r, "axiosInstance")?;
        let post = h.get(&axios, "post")?;
        let body = h.call_method(&body, "toString", vec![])?;
        let sent = h.call_bound(
            &post,
            &axios,
            vec![
                text(h.as_ref(), "/api/v1/sendTxBatch"),
                body,
                object(
                    h.as_ref(),
                    vec![(
                        "headers",
                        object(
                            h.as_ref(),
                            vec![(
                                "Content-Type",
                                text(h.as_ref(), "application/x-www-form-urlencoded"),
                            )],
                        ),
                    )],
                ),
            ],
        )?;
        Ok(Some(sent))
    })();
    let sent = match prepared {
        Ok(Some(v)) => h.promise_resolve(v),
        Ok(None) => {
            let value = object(h.as_ref(), vec![("txHashes", h.empty_array())]);
            return retained(h.clone(), h.promise_resolve(value));
        }
        Err(e) => return ready(Err(e)),
    };
    retained(
        h.clone(),
        Box::pin(async move {
            let response = sent.await;
            continuation().await;
            let p = h.get(&response?, "data")?;
            if h.number_200(&h.get(&p, "code")?) || h.string_200(&h.get(&p, "code")?) {
                let hashes = if h.is_array(&h.get(&p, "tx_hash")?) {
                    h.get(&p, "tx_hash")?
                } else if h.is_array(&h.get(&p, "txHash")?) {
                    h.get(&p, "txHash")?
                } else {
                    h.undefined_value()
                };
                if !h.nullish(&hashes) && h.every_string(&hashes)? {
                    let predicted = h.get(&p, "predicted_execution_time_ms")?;
                    let predicted = if h.nullish(&predicted) {
                        h.get(&p, "predictedExecutionTimeMs")?
                    } else {
                        predicted
                    };
                    let quota = h.get(&p, "volume_quota_remaining")?;
                    let quota = if h.nullish(&quota) {
                        h.get(&p, "volumeQuotaRemaining")?
                    } else {
                        quota
                    };
                    return h
                        .promise_resolve(object(
                            h.as_ref(),
                            vec![
                                ("txHashes", hashes),
                                ("predictedExecutionTimeMs", predicted),
                                ("volumeQuotaRemaining", quota),
                            ],
                        ))
                        .await;
                }
            }
            let hashes = optional(h.as_ref(), &h.get(&p, "data")?, "txHashes")?;
            let hashes = if h.nullish(&hashes) {
                optional(h.as_ref(), &h.get(&p, "data")?, "txHash")?
            } else {
                hashes
            };
            if h.truthy(&h.get(&p, "success")?) && !h.nullish(&hashes) && h.every_string(&hashes)? {
                return h
                    .promise_resolve(object(h.as_ref(), vec![("txHashes", hashes)]))
                    .await;
            }
            Err(send_error(
                h.as_ref(),
                &p,
                "Failed to send transaction batch",
                "SEND_TX_BATCH_FAILED",
            )?)
        }),
    )
}
