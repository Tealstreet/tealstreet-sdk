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

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rejected => "rejected",
            Self::InvalidInput => "invalidInput",
            Self::AuthInvalid => "authInvalid",
            Self::AuthExpired => "authExpired",
            Self::AuthPermission => "authPermission",
            Self::RateLimited => "rateLimited",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::VenueUnavailable => "venueUnavailable",
            Self::Unsupported => "unsupported",
            Self::Internal => "internal",
        }
    }
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

/// Exact native order identities retained for partial or uncertain commands.
/// With Unknown, identities are reconciliation evidence, not proof of attributed
/// caller-leg success. Codecs unable to map rows to caller orders leave counts unset.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Partial {
    pub order_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_index: Option<usize>,
}

/// Definitive builder approval refusal; unknown/failed checks never carry this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuilderAuthorizationReason {
    Unauthorized,
    CapTooLow,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderAuthorization {
    pub code: &'static str,
    pub reason: BuilderAuthorizationReason,
    pub required_raw: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_raw: Option<u32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_authorization: Option<Box<BuilderAuthorization>>,
}

impl VenueError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::from_message(message.into())
    }

    // Out of line so call sites share one struct initialisation (wasm size budget).
    #[inline(never)]
    fn from_message(message: String) -> Self {
        Self {
            message,
            ..Self::default()
        }
    }

    pub fn builder_authorization(mut self, detail: BuilderAuthorization) -> Self {
        self.builder_authorization = Some(Box::new(detail));
        self
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
    pub fn partial(self, order_ids: &[String], total: usize, failed_index: usize) -> Self {
        self.partial_with(order_ids, Some(total), Some(failed_index))
    }

    /// Records accepted orders whose position in the caller's batch is unknown.
    pub fn partial_ids(self, order_ids: &[String]) -> Self {
        self.partial_with(order_ids, None, None)
    }

    fn partial_with(
        mut self,
        order_ids: &[String],
        total: Option<usize>,
        failed_index: Option<usize>,
    ) -> Self {
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
            #[serde(skip_serializing_if = "Option::is_none")]
            total_order_count: Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            failed_order_index: Option<usize>,
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

/// Local operation failure: this call dispatched nothing new. `NotSent` never proves
/// the whole command unsent; the mutation journal overrides it from earlier
/// releases, and an earlier released read may still have reached the venue.
impl crate::request_context::Error {
    pub fn kind(self) -> ErrorKind {
        match self {
            Self::TimedOut => ErrorKind::Timeout,
            #[cfg(feature = "http-plans")]
            Self::CapacityDeferred => ErrorKind::RateLimited,
            Self::Invalid | Self::Revoked | Self::Duplicate | Self::RetryForbidden => {
                ErrorKind::InvalidInput
            }
            Self::Capacity | Self::State | Self::Stale => ErrorKind::Internal,
        }
    }

    pub fn venue_error(self) -> VenueError {
        self.read_venue_error().outcome(Outcome::NotSent)
    }

    /// Read admission failures carry no financial outcome.
    pub fn read_venue_error(self) -> VenueError {
        let error = VenueError::new(self.to_string()).kind(self.kind());
        #[cfg(feature = "http-plans")]
        if self == Self::CapacityDeferred {
            return error.retry_after_ms(1000.);
        }
        error
    }
}

impl fmt::Display for crate::request_context::Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::request_context::Error;
        f.write_str(match self {
            Error::Invalid => "Invalid request context",
            Error::Capacity => "Engine request capacity reached",
            #[cfg(feature = "http-plans")]
            Error::CapacityDeferred => {
                "Engine request capacity temporarily occupied by background reads"
            }
            Error::Duplicate => "Request is already in flight",
            Error::State => "Request is not in a sendable state",
            Error::TimedOut => "Request context timed out",
            Error::Stale => "Request context is stale",
            Error::Revoked => "Request context was revoked",
            Error::RetryForbidden => "Request may not be retried",
        })
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
    fn builder_approval_details_are_optional_and_keep_the_legacy_text() {
        let error = VenueError::new("Builder cap too low")
            .builder_authorization(BuilderAuthorization {
                code: "GRVT_BUILDER_UNAUTHORIZED",
                reason: BuilderAuthorizationReason::CapTooLow,
                required_raw: 300,
                authorized_raw: Some(100),
            })
            .kind(ErrorKind::InvalidInput)
            .outcome(Outcome::NotSent);
        assert_eq!(error.legacy_text(), "Builder cap too low");
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"Builder cap too low","kind":"invalidInput","outcome":"notSent","builderAuthorization":{"code":"GRVT_BUILDER_UNAUTHORIZED","reason":"cap-too-low","requiredRaw":300,"authorizedRaw":100}}"#
        );
    }

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
    fn unmapped_partials_omit_the_counts_they_cannot_know() {
        let ids = ["a".to_owned()];
        let error = VenueError::new("leg rejected").partial_ids(&ids);
        assert_eq!(
            error.legacy_text(),
            r#"{"message":"leg rejected","partialOrderIds":["a"],"partialOrderCount":1}"#
        );
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"leg rejected","partial":{"orderIds":["a"]}}"#
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
    fn request_context_refusals_are_typed_not_sent_failures() {
        use crate::request_context::Error;
        let error = Error::TimedOut.venue_error();
        assert_eq!(error.message, "Request context timed out");
        assert_eq!(error.kind, Some(ErrorKind::Timeout));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        for (reason, kind) in [
            (Error::Invalid, ErrorKind::InvalidInput),
            (Error::Revoked, ErrorKind::InvalidInput),
            (Error::Duplicate, ErrorKind::InvalidInput),
            (Error::RetryForbidden, ErrorKind::InvalidInput),
            (Error::Capacity, ErrorKind::Internal),
            (Error::State, ErrorKind::Internal),
            (Error::Stale, ErrorKind::Internal),
        ] {
            let error = reason.venue_error();
            assert_eq!(error.kind, Some(kind), "{reason:?}");
            assert_eq!(error.outcome, Some(Outcome::NotSent), "{reason:?}");
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn read_admission_refusals_keep_kind_and_text_without_financial_outcome() {
        use crate::request_context::Error;
        for reason in [
            Error::TimedOut,
            Error::Invalid,
            Error::Revoked,
            Error::Duplicate,
            Error::RetryForbidden,
            Error::Capacity,
            Error::State,
            Error::Stale,
        ] {
            let read = reason.read_venue_error();
            let generic = reason.venue_error();
            assert_eq!(read.message, generic.message);
            assert_eq!(read.kind, generic.kind);
            assert_eq!(read.outcome, None);
            assert_eq!(generic.outcome, Some(Outcome::NotSent));
        }
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
