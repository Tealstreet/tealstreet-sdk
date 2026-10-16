#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Submission operation awaiting exchange class composition"
    )
)]
//! Sequential create-order orchestration; HTTP decoding remains the client's responsibility.
use safe_cex_transport::RestFailure;
use serde_json::Value;
use std::future::Future;

/// The source catches individual HTTP failures and continues the series. The
/// formatter receives the original failure, retaining response and message as
/// separate inputs. A formatter/emitter failure aborts the remaining series.
pub(crate) async fn submit_create_orders<P, F, E, T>(
    payloads: Vec<Value>,
    mut post: P,
    mut report_failure: impl FnMut(RestFailure) -> Result<(), E>,
) -> Result<Vec<(T, Value)>, E>
where
    P: FnMut(Value) -> F,
    F: Future<Output = Result<T, RestFailure>>,
{
    let mut responses = Vec::new();
    for payload in payloads {
        match post(payload.clone()).await {
            Ok(data) => responses.push((data, payload)),
            Err(error) => report_failure(error)?,
        }
    }
    Ok(responses)
}

#[cfg(test)]
mod tests;
