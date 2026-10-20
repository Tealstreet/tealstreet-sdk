//! Structured command and lifecycle failures. `message` is the user-facing
//! text hosts have always shown; the other fields let consumers classify a
//! failure without matching that text.
use serde::Serialize;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Rejected,
    InvalidInput,
    AuthInvalid,
    AuthExpired,
    AuthPermission,
    RateLimited,
    Network,
    Timeout,
    VenueUnavailable,
    Unsupported,
    Internal,
}

/// Whether a write reached the venue. `None` on [`VenueError`] means the site
/// has not classified it, never that it is safe to resend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    NotSent,
    Rejected,
    Unknown,
}

/// Orders a batch placed before it stopped at `failed_index`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Partial {
    pub order_ids: Vec<String>,
    pub total: usize,
    pub failed_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VenueError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ErrorKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venue_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venue_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<Box<Partial>>,
}

impl VenueError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Self::default()
        }
    }

    pub fn kind(mut self, kind: ErrorKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    pub fn venue_code(mut self, code: impl Into<String>) -> Self {
        self.venue_code = Some(code.into());
        self
    }

    pub fn venue_message(mut self, message: impl Into<String>) -> Self {
        self.venue_message = Some(message.into());
        self
    }

    /// A non-finite delay is dropped rather than serialized as `null`.
    pub fn retry_after_ms(mut self, ms: f64) -> Self {
        self.retry_after_ms = ms.is_finite().then_some(ms);
        self
    }

    /// Records accepted orders; an empty list leaves the error whole.
    pub fn partial(mut self, order_ids: &[String], total: usize, failed_index: usize) -> Self {
        if !order_ids.is_empty() {
            self.partial = Some(Box::new(Partial {
                order_ids: order_ids.to_vec(),
                total,
                failed_index,
            }));
        }
        self
    }

    /// The `error` string hosts send today: the message, or for a partial
    /// batch the JSON object `host.mjs` unpacks into `partialOrderIds`.
    pub fn legacy_text(&self) -> String {
        let Some(partial) = &self.partial else {
            return self.message.clone();
        };
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Legacy<'a> {
            message: &'a str,
            partial_order_ids: &'a [String],
            partial_order_count: usize,
            total_order_count: usize,
            failed_order_index: usize,
        }
        serde_json::to_string(&Legacy {
            message: &self.message,
            partial_order_ids: &partial.order_ids,
            partial_order_count: partial.order_ids.len(),
            total_order_count: partial.total,
            failed_order_index: partial.failed_index,
        })
        .unwrap_or_else(|_| self.message.clone())
    }
}

impl From<String> for VenueError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for VenueError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<&String> for VenueError {
    fn from(message: &String) -> Self {
        Self::new(message.as_str())
    }
}

impl fmt::Display for VenueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for VenueError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_errors_keep_their_text_and_serialize_only_the_message() {
        let error = VenueError::from("Order would trigger immediately");
        assert_eq!(error.legacy_text(), "Order would trigger immediately");
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"Order would trigger immediately"}"#
        );
    }

    #[test]
    fn partial_batches_keep_the_legacy_json_text_byte_for_byte() {
        let ids = ["a".to_owned(), "b".to_owned()];
        let error = VenueError::new("leg 3 rejected").partial(&ids, 4, 2);
        assert_eq!(
            error.legacy_text(),
            r#"{"message":"leg 3 rejected","partialOrderIds":["a","b"],"partialOrderCount":2,"totalOrderCount":4,"failedOrderIndex":2}"#
        );
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"leg 3 rejected","partial":{"orderIds":["a","b"],"total":4,"failedIndex":2}}"#
        );
    }

    #[test]
    fn an_empty_partial_is_a_whole_failure() {
        let error = VenueError::new("rejected").partial(&[], 3, 0);
        assert_eq!(error.partial, None);
        assert_eq!(error.legacy_text(), "rejected");
    }

    #[test]
    fn a_non_finite_retry_delay_is_left_out() {
        let error = VenueError::new("busy").retry_after_ms(f64::NAN);
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"busy"}"#
        );
    }

    #[test]
    fn classified_fields_serialize_in_camel_case() {
        let error = VenueError::new("Too many requests")
            .kind(ErrorKind::RateLimited)
            .outcome(Outcome::NotSent)
            .venue_code("10006")
            .venue_message("Too many visits")
            .retry_after_ms(1000.0);
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"Too many requests","kind":"rateLimited","outcome":"notSent","venueCode":"10006","venueMessage":"Too many visits","retryAfterMs":1000.0}"#
        );
    }
}
