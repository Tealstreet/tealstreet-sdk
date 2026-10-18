//! Read tokens own their request: neither mutation nor swapping in a write is possible.
use crate::rate_limit::RateLimitClock;
use std::sync::Arc;

use crate::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};

#[derive(Clone)]
pub enum ReadClassification {
    Methods,
    PostedPaths(Vec<String>),
    Hyperliquid,
    Grvt,
    Custom(Arc<dyn Fn(&RestRequest) -> bool>),
}

/// The private read token cannot be forged from a write.
/// ```compile_fail
/// use safe_cex_transport::{RestRequest, retry::ReadRequest};
/// let read = ReadRequest(RestRequest { method: "POST".into(), ..Default::default() });
/// ```
pub struct ReadRequest(RestRequest, ReadClassification);
pub struct WriteRequest(RestRequest);
pub enum ClassifiedRequest {
    Read(ReadRequest),
    Write(WriteRequest),
}

pub fn is_read_only_method(request: &RestRequest) -> bool {
    request.method.eq_ignore_ascii_case("get") || request.method.eq_ignore_ascii_case("head")
}
pub fn reads_and_posted(paths: Vec<String>) -> ReadClassification {
    ReadClassification::PostedPaths(paths)
}
pub fn is_hyperliquid_read(request: &RestRequest) -> bool {
    request.url.split('?').next() != Some("/exchange")
}
pub fn is_grvt_read(request: &RestRequest) -> bool {
    matches!(
        request
            .url
            .split('?')
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or(""),
        "all_instruments"
            | "instrument"
            | "ticker"
            | "mini"
            | "book"
            | "trade_history"
            | "kline"
            | "funding"
            | "login"
            | "account_summary"
            | "positions"
            | "open_orders"
            | "order_history"
            | "fill_history"
            | "get_all_initial_leverage"
            | "get_authorized_builders"
            | "builder_fill_history"
            | "aggregated_account_summary"
            | "funding_account_summary"
    )
}
impl ReadClassification {
    pub fn classify(&self, request: RestRequest) -> ClassifiedRequest {
        let read = match self {
            Self::Methods => is_read_only_method(&request),
            // ts: readsAndPosted deliberately ignores the method for listed paths.
            Self::PostedPaths(paths) => {
                is_read_only_method(&request)
                    || paths
                        .iter()
                        .any(|p| request.url.split('?').next() == Some(p))
            }
            Self::Hyperliquid => is_hyperliquid_read(&request),
            Self::Grvt => is_grvt_read(&request),
            Self::Custom(is_read) => is_read(&request),
        };
        if read {
            ClassifiedRequest::Read(ReadRequest(request, self.clone()))
        } else {
            ClassifiedRequest::Write(WriteRequest(request))
        }
    }
}

type RetryCondition =
    dyn Fn(&RestRequest, &Result<RestResponse, RestFailure>) -> Result<bool, RestFailure>;

struct RetryDelay {
    milliseconds: Box<dyn Fn(usize) -> u64>,
}

/// A policy cannot be used as an adapter for arbitrary requests.
/// ```compile_fail
/// use safe_cex_transport::{RestAdapter, retry::RetryPolicy};
/// fn adapter(policy: &RetryPolicy) -> &dyn RestAdapter { policy }
/// ```
pub struct RetryPolicy {
    request: ReadRequest,
    retries: usize,
    condition: Box<RetryCondition>,
    delay: Option<RetryDelay>,
}
/// Construction binds the policy to an immutable, classified read.
/// ```compile_fail
/// use safe_cex_transport::retry::{WriteRequest, network_error_on_read};
/// fn retry_write(write: WriteRequest) { let _ = network_error_on_read(write, 3); }
/// ```
pub fn network_error_on_read(request: ReadRequest, retries: usize) -> RetryPolicy {
    only_reads(request, retries, |_, result| {
        result
            .as_ref()
            .err()
            .is_some_and(|error| error.is_retryable_network())
    })
}
/// Installed axios-retry network-or-idempotent predicate; read classification
/// remains a separate required boundary, including for idempotent PUT/DELETE.
pub fn is_network_or_idempotent_request_error(error: &RestFailure) -> bool {
    (error.response.is_none()
        && crate::NetworkFailure::from_code(error.message.clone(), error.code.clone())
            .retry_allowed())
        || (error.code.as_deref() != Some("ECONNABORTED")
            && matches!(
                error.request.method.as_str(),
                "get" | "head" | "options" | "put" | "delete"
            )
            && error.response.as_ref().is_none_or(|response| {
                response.status == 429 || (500..=599).contains(&response.status)
            }))
}

pub fn only_reads(
    request: ReadRequest,
    retries: usize,
    condition: impl Fn(&RestRequest, &Result<RestResponse, RestFailure>) -> bool + 'static,
) -> RetryPolicy {
    RetryPolicy {
        request,
        retries,
        condition: Box::new(move |request, result| Ok(condition(request, result))),
        delay: None,
    }
}
impl RetryPolicy {
    pub fn with_delay(mut self, milliseconds: impl Fn(usize) -> u64 + 'static) -> Self {
        self.delay = Some(RetryDelay {
            milliseconds: Box::new(milliseconds),
        });
        self
    }

    pub fn execute<'a>(
        &'a self,
        adapter: &'a dyn RestAdapter,
        clock: &'a dyn RateLimitClock,
    ) -> RestFuture<'a, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(adapter, clock)
                .await
                .map(|execution| execution.response)
        })
    }
    pub fn execute_with_receipt<'a>(
        &'a self,
        adapter: &'a dyn RestAdapter,
        clock: &'a dyn RateLimitClock,
    ) -> RestFuture<'a, crate::RestExecution> {
        Box::pin(async move {
            // Remaining time is derived inside the engine, never supplied per attempt.
            let mut request = self.request.0.clone();
            for attempt in 0..=self.retries {
                let started = clock.now_ms();
                let (result, config) = match adapter.execute_with_receipt(request.clone()).await {
                    Ok(execution) => (Ok(execution.response), Some(execution.config)),
                    Err(error) => (Err(error), None),
                };
                if attempt == self.retries || !(self.condition)(&request, &result)? {
                    return result.map(|response| crate::RestExecution {
                        response,
                        config: config.expect("successful adapter receipt"),
                    });
                }
                let delay = self
                    .delay
                    .as_ref()
                    .map_or(0, |delay| (delay.milliseconds)(attempt + 1));
                // axios-retry defaults shouldResetTimeout=false. Its truthy guard
                // treats timeout=0 as unlimited and epoch-zero start as absent.
                if let Some(timeout) = request.timeout_ms.filter(|timeout| *timeout != 0)
                    && started != 0
                {
                    let remaining = i128::from(timeout)
                        - (i128::from(clock.now_ms()) - i128::from(started))
                        - i128::from(delay);
                    if remaining <= 0 {
                        return result.map(|response| crate::RestExecution {
                            response,
                            config: config.expect("successful adapter receipt"),
                        });
                    }
                    request.timeout_ms = Some(remaining.min(i128::from(i64::MAX)) as i64);
                }
                if let Err(error) = &result {
                    let mut next = (*error.request).clone();
                    // The failed config is untrusted: reapply the ORIGINAL policy,
                    // never inherit an adapter's claim that the new target is a read.
                    if !matches!(
                        self.request.1.classify(next.clone()),
                        ClassifiedRequest::Read(_)
                    ) {
                        return result.map(|response| crate::RestExecution {
                            response,
                            config: config.expect("successful adapter receipt"),
                        });
                    }
                    // Remaining timeout comes from our measured budget, not error.config.
                    next.timeout_ms = request.timeout_ms;
                    request = next;
                }
                // axios-retry schedules the next attempt even when its delay is
                // zero. Hosts must observe that scheduling boundary on every retry.
                clock
                    .sleep_ms(delay)
                    .await
                    .map_err(|error| RestFailure::from_transport(error, request.clone()))?;
            }
            unreachable!()
        })
    }
}
impl WriteRequest {
    pub fn request(&self) -> &RestRequest {
        &self.0
    }
    pub fn execute(self, adapter: &dyn RestAdapter) -> RestFuture<'_, RestResponse> {
        adapter.execute(self.0)
    }
    pub fn execute_with_receipt(
        self,
        adapter: &dyn RestAdapter,
    ) -> RestFuture<'_, crate::RestExecution> {
        adapter.execute_with_receipt(self.0)
    }
}
/// Installed axios-retry network predicate, before any read-token restriction.
pub fn is_network_error(error: &RestFailure) -> bool {
    error.response.is_none()
        && crate::NetworkFailure::from_code(error.message.clone(), error.code.clone())
            .retry_allowed()
}
