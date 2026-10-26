//! Native declarations and classified facts; settlement stays in the engine.
use super::*;
use safe_cex_engine::classify::{CodeTable, Intent, Key, Row, SatisfiedIntent, Verdict};
use safe_cex_engine::request_context::{
    Credential, Operation, PlanPolicy, Policy, Preflight, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound},
};

/// Each ordered write has its native 20 s bound; 100001 permits one resend.
pub(super) const SERIAL: SerialCall = SerialCall {
    auth: safe_cex_engine::request_context::plan::CallAuth::Preflight,
    operation: Operation::Mutation,
    timeout_ms: 20_000,
    attempts: 2,
};

// BingX Swap V2 common error codes: https://bingx-api.github.io/docs/#/en-us/swapV2/base-info.html
// 100001 is signature refusal; 80014 invalid parameter; 80016 order does not exist.
pub(super) const TABLE: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(100001),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(101204),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: Some(Intent::Place),
        },
        Row {
            key: Key::Int(100400),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        // Official common-code reference: gateway refusals precede backend service dispatch.
        // https://github.com/BingX-API/api-ai-skills/blob/main/skills/references/error-codes.md#common-error-codes
        Row {
            key: Key::Int(100410),
            verdict: Verdict::Refused(ErrorKind::RateLimited),
            only: None,
        },
        Row {
            key: Key::Int(80012),
            verdict: Verdict::Unknown(Some(ErrorKind::VenueUnavailable)),
            only: None,
        },
        Row {
            key: Key::Int(80014),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: Some(Intent::Place),
        },
        Row {
            key: Key::Int(80016),
            verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
            only: Some(Intent::Cancel),
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

pub(super) enum Entry {
    Preflight(Preflight, Credential),
    Terminal(Context),
}
impl Entry {
    pub(super) fn begin(
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
    ) -> Option<Self> {
        if let Some(credentials) = credentials {
            let credential = Credential {
                slot: safe_cex_engine::CredentialSlot::Primary,
                reference: credentials.primary.credential.clone(),
            };
            cx.begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential.clone()),
            })
            .ok()
            .map(|owner| Self::Preflight(owner, credential))
        } else {
            cx.admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .ok()
            .map(Self::Terminal)
        }
    }
    pub(super) fn reject(&self, cx: &mut Cx<'_, Pending>, error: impl Into<VenueError>) {
        match self {
            Self::Preflight(owner, _) => {
                let _ = cx.fail_preflight(owner, error);
            }
            Self::Terminal(owner) => {
                let _ = cx.fail_context(owner, error);
            }
        }
    }
    /// Ordered writes stream: each next call is admitted from the previous
    /// call's DATA, so a native split is never capped by a declared length.
    pub(super) fn admit_serial(
        &self,
        cx: &mut Cx<'_, Pending>,
        count: usize,
    ) -> Option<(Plan, Slot)> {
        let Self::Preflight(owner, credential) = self else {
            return None;
        };
        if count == 0 {
            let _ = cx.done_empty_plan(owner, "[]".into());
            return None;
        }
        let policy = SerialPolicy {
            credential: Some(credential.clone()),
            pacing: None,
            refusals_complete: false,
        };
        match cx.admit_serial_plan(owner, SERIAL, policy) {
            Ok(admitted) => Some(admitted),
            Err(error) => {
                self.reject(cx, error.venue_error());
                None
            }
        }
    }
    /// Native `continue_on_error` cancels: independent sibling calls.
    pub(super) fn admit_independent(&self, cx: &mut Cx<'_, Pending>, count: usize) -> Option<Plan> {
        let Self::Preflight(owner, credential) = self else {
            return None;
        };
        if count == 0 {
            let _ = cx.done_empty_plan(owner, "[]".into());
            return None;
        }
        let call = [CallBound {
            timeout_ms: 20_000,
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps = (0..count)
            .map(|_| StepBound {
                wait_ms: 0,
                calls: &call,
            })
            .collect::<Vec<_>>();
        match cx.admit_plan(
            owner,
            &steps,
            PlanPolicy {
                operation: Operation::Mutation,
                credential: Some(credential.clone()),
                pacing: None,
                retry: RetryPolicy::DefinitiveRefusal,
                independent_calls: true,
            },
        ) {
            Ok(plan) => Some(plan),
            Err(error) => {
                self.reject(cx, error.venue_error());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bingx_table_is_well_formed() {
        safe_cex_engine::classify::assert_table(&super::TABLE);
    }
}
