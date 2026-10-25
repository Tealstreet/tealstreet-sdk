//! One bounded selected physical call, shared by retained HTTP owner phases.
use super::*;
use crate::TimerId;
use crate::request_context::plan::{CallTerms, CallWindow};
use std::ops::Range;

pub(super) struct Call {
    pub(super) context: Context,
    pub(super) terms: CallTerms,
    pub(super) window: CallWindow,
    pub(super) ids: Range<u64>,
    pub(super) signing: Option<Range<u64>>,
    pub(super) selected: u16,
    pub(super) admission: Admission,
    pub(super) stage: Stage,
    pub(super) released_failure: Option<ReleasedReadFailure>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) enum ReleasedReadFailure {
    Network,
    PhysicalTimeout,
}

pub(super) enum Stage {
    Ready,
    Active {
        failed: bool,
    },
    /// A failed call whose retry waits on a login Session: nothing is selected
    /// until its resume, and the default failure tail leaves it alone.
    Parked,
}

/// Private one-use queue/failure custody. A Context clone cannot construct it.
#[derive(Clone)]
pub(in super::super) struct Admission {
    pub(in super::super) context: Context,
    pub(in super::super) attempt: Attempt,
    pub(in super::super) timer: TimerId,
    pub(in super::super) signing: Option<RequestId>,
    pub(in super::super) deadline: f64,
    pub(super) predecessor: Option<Delivery>,
}

impl Call {
    /// A logical call with its first physical attempt selected. Shared by every
    /// owner phase so the construction exists once.
    #[inline(never)]
    pub(super) fn open(
        context: Context,
        terms: CallTerms,
        window: CallWindow,
        ids: Range<u64>,
        signing: Option<Range<u64>>,
        now: f64,
        predecessor: Option<Delivery>,
    ) -> Result<Self, Error> {
        let mut call = Call {
            context: context.clone(),
            terms,
            window,
            ids,
            signing,
            selected: 0,
            admission: Admission {
                context,
                attempt: Attempt {
                    request: RequestId(0),
                    generation: 0,
                },
                timer: TimerId(0),
                signing: None,
                deadline: window.deadline(),
                predecessor: None,
            },
            stage: Stage::Ready,
            released_failure: None,
        };
        call.select(now, predecessor)?;
        Ok(call)
    }

    pub(super) fn matches(&self, delivery: &Delivery) -> bool {
        self.context.same_issuer(&delivery.context)
            && self.context == delivery.context
            && self.admission.attempt == delivery.attempt
    }

    pub(super) fn select(&mut self, now: f64, predecessor: Option<Delivery>) -> Result<(), Error> {
        if self.selected >= self.terms.attempts || self.ids.end - self.ids.start < 3 {
            return Err(Error::RetryForbidden);
        }
        let deadline = self.window.physical_deadline(now)?;
        let first = self.ids.start;
        self.ids.start += 3;
        let signing = self.signing.as_mut().map(|ids| {
            let id = RequestId(ids.start);
            ids.start += 1;
            id
        });
        self.admission = Admission {
            context: self.context.clone(),
            attempt: Attempt {
                request: RequestId(first),
                generation: first + 1,
            },
            timer: TimerId(first + 2),
            signing,
            deadline,
            predecessor,
        };
        self.selected += 1;
        self.stage = Stage::Ready;
        self.released_failure = None;
        Ok(())
    }
}
