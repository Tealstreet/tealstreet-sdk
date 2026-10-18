//! ts: utils/signatureRejection. One re-signed resend on a signature rejection: a
//! refusal executed nothing, so writes resend as safely as reads.
use crate::{
    RestAdapter, RestExecution, RestFailure, RestRequest, RestResponse,
    rate_limit::RateLimitClock,
    retry::{ClassifiedRequest, only_reads},
};
use safe_cex_num::JsCoercionError;
use safe_cex_types::retained_json::RetainedJsonInput;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// ts: SIGNATURE_RETRY_CONFIG_KEY, the config property the source stamps.
pub const SIGNATURE_RETRY_CONFIG_KEY: &str = "safeCexSignatureRetry";

/// ts: IsSignatureRejection. Reads a venue's error body; coercion failures are the
/// source's thrown TypeErrors and stay visible.
pub type IsSignatureRejection = Rc<dyn Fn(&RetainedJsonInput) -> Result<bool, JsCoercionError>>;

/// The source stamps the request config, and both resend paths forward that same
/// config. `RestRequest` carries no free-form properties, so one call shares this
/// stamp across its attempts instead; clones observe the same stamp.
#[derive(Clone, Default)]
pub struct SignatureRetry(Rc<Cell<bool>>);
impl SignatureRetry {
    /// ts: hasRetriedSignature
    pub fn has_retried(&self) -> bool {
        self.0.get()
    }
    fn stamp(&self) {
        self.0.set(true);
    }
}

/// The body an Axios error or response exposes as `data`.
pub fn response_data(response: &RestResponse) -> RetainedJsonInput {
    match &response.body {
        crate::RestResponseBody::Encoded(bytes) => {
            let text = String::from_utf8_lossy(bytes)
                .encode_utf16()
                .collect::<Vec<_>>();
            RetainedJsonInput::parse_json(&text).unwrap_or(RetainedJsonInput::Text(text))
        }
        body => body.clone().into_data(),
    }
}

/// ts: isBinanceStyleSignatureRejection (Binance -1022, kept by Bitrue and AsterDEX).
pub fn is_binance_style_signature_rejection(
    data: &RetainedJsonInput,
) -> Result<bool, JsCoercionError> {
    let field = |key| data.get(key);
    if safe_cex_num::js_retained_number(field("code"))? == -1022.0 {
        return Ok(true);
    }
    let message = match field("msg").filter(|value| {
        !matches!(
            value,
            RetainedJsonInput::Null | RetainedJsonInput::Undefined
        )
    }) {
        Some(value) => safe_cex_types::js_coercion::js_retained_string_utf16(value)?,
        None => Vec::new(),
    };
    let pattern = "signature for this request is not valid"
        .encode_utf16()
        .collect::<Vec<_>>();
    Ok(message.windows(pattern.len()).any(|units| {
        units.iter().zip(&pattern).all(|(unit, expected)| {
            let folded = if (65..=90).contains(unit) {
                *unit + 32
            } else {
                *unit
            };
            folded == *expected
        })
    }))
}

fn coercion_failure(error: JsCoercionError, request: &RestRequest) -> RestFailure {
    RestFailure::from_transport(crate::TransportError::JavaScript(error), request.clone())
}

/// ts: signatureRejectedResponse(isRejection)(error), the axios-retry condition for a
/// non-2xx rejection. It stamps what it approves, so a second rejection surfaces.
pub fn signature_rejected_response(
    error: &RestFailure,
    retry: &SignatureRetry,
    is_rejection: &IsSignatureRejection,
) -> Result<bool, RestFailure> {
    let Some(response) = error.response.as_deref() else {
        return Ok(false);
    };
    if retry.has_retried()
        || !is_rejection(&response_data(response))
            .map_err(|cause| coercion_failure(cause, &error.request))?
    {
        return Ok(false);
    }
    retry.stamp();
    Ok(true)
}

/// ts: resendOnSignatureRejection for a rejection Axios resolves (a 2xx envelope, or
/// any status over HOW). `true` means resend `execution.config` through the whole
/// pipeline with this stamp, before any later interceptor side effect.
pub fn resend_on_signature_rejection(
    execution: &RestExecution,
    retry: &SignatureRetry,
    is_rejection: &IsSignatureRejection,
) -> Result<bool, RestFailure> {
    resend_on_rejected_data(&response_data(&execution.response), retry, is_rejection)
        .map_err(|cause| coercion_failure(cause, &execution.config))
}
/// The same decision for a client that surfaces only the resolved `response.data`.
pub fn resend_on_rejected_data(
    data: &RetainedJsonInput,
    retry: &SignatureRetry,
    is_rejection: &IsSignatureRejection,
) -> Result<bool, JsCoercionError> {
    if retry.has_retried() || !is_rejection(data)? {
        return Ok(false);
    }
    retry.stamp();
    Ok(true)
}

/// axios-retry with `retryCondition: networkErrorOnRead(...)(e) || signatureRejectedResponse(...)(e)`.
/// Reads keep the network retry budget; a write is resent only for the one signature
/// rejection, under the same shouldResetTimeout=false budget.
pub async fn retry_network_reads_and_signature_rejection(
    request: ClassifiedRequest,
    retries: usize,
    adapter: &dyn RestAdapter,
    clock: &dyn RateLimitClock,
    retry: &SignatureRetry,
    is_rejection: &IsSignatureRejection,
) -> Result<RestExecution, RestFailure> {
    match request {
        ClassifiedRequest::Read(read) => {
            let failure = Rc::new(RefCell::new(None));
            let condition_failure = failure.clone();
            let (retry, is_rejection) = (retry.clone(), is_rejection.clone());
            let result = only_reads(read, retries, move |_, result| {
                let Err(error) = result else {
                    return false;
                };
                if error.is_retryable_network() {
                    return true;
                }
                signature_rejected_response(error, &retry, &is_rejection).unwrap_or_else(|cause| {
                    *condition_failure.borrow_mut() = Some(cause);
                    false
                })
            })
            .execute_with_receipt(adapter, clock)
            .await;
            match failure.take() {
                Some(cause) => Err(cause),
                None => result,
            }
        }
        ClassifiedRequest::Write(write) => {
            let timeout = write.request().timeout_ms;
            let started = clock.now_ms();
            let error = match write.execute_with_receipt(adapter).await {
                Err(error) if signature_rejected_response(&error, retry, is_rejection)? => error,
                result => return result,
            };
            let mut next = (*error.request).clone();
            next.timeout_ms = timeout;
            if let Some(timeout) = timeout.filter(|timeout| *timeout != 0)
                && started != 0
            {
                let remaining =
                    i128::from(timeout) - (i128::from(clock.now_ms()) - i128::from(started));
                if remaining <= 0 {
                    return Err(error);
                }
                next.timeout_ms = Some(remaining.min(i128::from(i64::MAX)) as i64);
            }
            // axios-retry schedules even a zero-delay retry.
            clock
                .sleep_ms(0)
                .await
                .map_err(|cause| RestFailure::from_transport(cause, next.clone()))?;
            adapter.execute_with_receipt(next).await
        }
    }
}
