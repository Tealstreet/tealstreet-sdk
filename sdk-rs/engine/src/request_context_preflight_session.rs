//! A retained Preflight read parked on a login Session before it is issued.
use super::*;
use crate::request_context::{RetryEvidence, Session, plan::CallTerms};

impl<P> Cx<'_, P> {
    /// Park this Preflight's next prerequisite read on a login Session before
    /// it consumes an attempt. `session_ready` issues it under the same
    /// Preflight generation; nothing is spent while parked.
    pub fn preflight_session_wait(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
        session: &Session,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let _ = (handle, policy, terms, session);
        Err((pending, Error::State))
    }

    /// Park a failed read's declared retry on a login Session, in the failed
    /// delivery's custody, without selecting the next attempt.
    pub fn preflight_retry_session_wait(
        &mut self,
        handle: &Preflight,
        context: &Context,
        evidence: RetryEvidence,
        session: &Session,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let _ = (handle, context, evidence, session);
        Err((pending, Error::State))
    }
}

#[cfg(test)]
#[path = "request_context_preflight_session_tests.rs"]
mod tests;
