//! Native declarations and classified facts; settlement stays in the engine.
use super::*;
use safe_cex_engine::classify::{CodeTable, Intent, Key, Row, SatisfiedIntent, Verdict};
use safe_cex_engine::request_context::{
    Credential, Operation, PlanPolicy, Policy, Preflight, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound},
};

/// The initial write has a native 20 s bound and one proven-refusal resend.
/// Successors keep the bound with one attempt after an effective prefix.
pub(super) const SERIAL: SerialCall = SerialCall {
    auth: safe_cex_engine::request_context::plan::CallAuth::Preflight,
    operation: Operation::Mutation,
    timeout_ms: 20_000,
    attempts: 2,
    pacing: None,
};

// BingX Swap V2 common error codes: https://bingx-api.github.io/docs/#/en-us/swapV2/base-info.html
// 100001 is signature refusal; 80014 invalid parameter; 80016 order does not exist.
const fn refused(code: i64, kind: ErrorKind) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Refused(kind),
        only: None,
    }
}

/// A documented bar on placing this order; other intents keep the unlisted verdict.
const fn place_refused(key: Key, kind: ErrorKind) -> Row {
    Row {
        key,
        verdict: Verdict::Refused(kind),
        only: Some(Intent::Place),
    }
}

pub(super) const TABLE: CodeTable = CodeTable {
    rows: &[
        // github.com/BingX-API/api-ai-skills: skills/references/error-codes.md, Futures.
        // Not here, so Unknown: 101481/109201 (duplicates), 102201 (canceled), busy, internal.
        place_refused(Key::Range(101201, 101202), ErrorKind::Rejected),
        place_refused(Key::Int(101206), ErrorKind::InsufficientBalance),
        place_refused(Key::Range(101209, 101210), ErrorKind::Rejected),
        place_refused(Key::Int(101214), ErrorKind::Rejected),
        place_refused(Key::Int(101219), ErrorKind::Rejected),
        place_refused(Key::Int(101222), ErrorKind::Rejected),
        place_refused(Key::Range(101412, 101415), ErrorKind::Rejected),
        place_refused(Key::Int(101419), ErrorKind::Rejected),
        place_refused(Key::Int(101429), ErrorKind::Rejected),
        place_refused(Key::Int(101434), ErrorKind::Rejected),
        place_refused(Key::Range(101460, 101461), ErrorKind::Rejected),
        place_refused(Key::Int(101479), ErrorKind::Rejected),
        place_refused(Key::Range(101483, 101485), ErrorKind::Rejected),
        place_refused(Key::Int(109417), ErrorKind::Rejected),
        place_refused(Key::Int(109425), ErrorKind::InvalidInput),
        place_refused(Key::Int(110203), ErrorKind::Rejected),
        place_refused(Key::Int(110206), ErrorKind::Rejected),
        place_refused(Key::Int(110280), ErrorKind::Rejected),
        place_refused(Key::Int(110287), ErrorKind::Rejected),
        place_refused(Key::Int(110400), ErrorKind::InvalidInput),
        place_refused(Key::Int(110402), ErrorKind::InvalidInput),
        place_refused(Key::Range(110406, 110407), ErrorKind::Rejected),
        place_refused(Key::Range(110411, 110415), ErrorKind::InvalidInput),
        place_refused(Key::Range(110418, 110420), ErrorKind::InvalidInput),
        place_refused(Key::Int(110424), ErrorKind::InsufficientBalance),
        place_refused(Key::Range(110426, 110428), ErrorKind::InvalidInput),
        place_refused(Key::Int(112414), ErrorKind::Rejected),
        // Trading-rule refusals: nothing was placed.
        refused(101205, ErrorKind::Rejected),
        refused(101211, ErrorKind::InvalidInput),
        refused(101212, ErrorKind::InsufficientBalance),
        refused(101215, ErrorKind::Rejected),
        refused(101253, ErrorKind::InsufficientBalance),
        refused(101290, ErrorKind::Rejected),
        refused(101400, ErrorKind::InvalidInput),
        refused(109400, ErrorKind::InvalidInput),
        refused(109421, ErrorKind::InvalidInput),
        refused(110422, ErrorKind::Rejected),
        refused(110425, ErrorKind::Rejected),
        Row {
            key: Key::Int(100001),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(100004),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(100413),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(100419),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
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

/// Selected only for missing-order replies on cancels or edits, never generic 109400.
pub(super) const MISSING_ORDER_TABLE: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(109400),
            verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
            only: Some(Intent::Cancel),
        },
        Row {
            key: Key::Int(109421),
            verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
            only: Some(Intent::Cancel),
        },
        refused(109400, ErrorKind::Rejected),
        refused(109421, ErrorKind::Rejected),
    ],
    success: &[],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

pub(super) fn missing_order(code: i64, message: &str) -> bool {
    code == 109421 || (code == 109400 && message.trim().eq_ignore_ascii_case("order not exist"))
}

/// 101400 also covers a duplicate client order id, whose original may be live.
pub(super) const DUPLICATE_TABLE: CodeTable = CodeTable {
    rows: &[],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

/// 109400 naming the timestamp: the request aged out of BingX's window before
/// any processing, so its one engine-granted resend, freshly stamped, cannot
/// double-place.
pub(super) const TIMESTAMP_TABLE: CodeTable = CodeTable {
    rows: &[Row {
        key: Key::Int(109400),
        verdict: Verdict::RefusedResend(ErrorKind::InvalidInput),
        only: None,
    }],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

pub(super) fn names_duplicate_client_id(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("duplicate") && message.contains("client")
}

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
    fn documented_trading_rule_refusals_are_definite() {
        use super::TABLE;
        use safe_cex_engine::classify::{Intent, Native, NativeCode, Verdict};
        for code in [
            101205, 101211, 101212, 101215, 101253, 101290, 101400, 109400, 109421, 110422, 110425,
        ] {
            let native = Native {
                code: NativeCode::Int(code),
                message: "",
            };
            assert!(
                matches!(TABLE.verdict(&native, Intent::Place), Verdict::Refused(_)),
                "{code}"
            );
        }
    }

    #[test]
    fn bingx_table_is_well_formed() {
        safe_cex_engine::classify::assert_table(&super::TABLE);
        safe_cex_engine::classify::assert_table(&super::TIMESTAMP_TABLE);
    }
}

#[cfg(test)]
mod key_refusal_tests {
    use super::*;
    use safe_cex_engine::classify::{Native, NativeCode};

    /// Every key refusal the read side rejects on is an auth refusal on writes.
    #[test]
    fn every_read_key_refusal_is_an_auth_refusal_on_writes() {
        for code in KEY_REFUSALS {
            let native = Native {
                code: NativeCode::Int(code),
                message: "native",
            };
            let verdict = TABLE.verdict(&native, Intent::Place);
            assert!(
                matches!(
                    verdict,
                    Verdict::Refused(ErrorKind::AuthInvalid | ErrorKind::AuthPermission)
                        | Verdict::RefusedResend(ErrorKind::AuthInvalid)
                ),
                "{code}: {verdict:?}"
            );
        }
    }
}
