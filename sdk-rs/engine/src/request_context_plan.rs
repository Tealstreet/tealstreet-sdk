//! Pure native-plan envelopes. These declarations grant no execution authority.
use super::{Error, evidence::MAX_ATTEMPTS};

#[path = "request_context_schedule.rs"]
mod schedule;
pub use schedule::{CallTerms, Schedule, Slot};

/// Whether retries retain the first request's window or get a fresh native one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryWindow {
    Shared,
    FreshAttempt,
}

/// Raw-clock bounds only. This value grants no retry, delivery or I/O authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CallWindow {
    created_at: f64,
    deadline: f64,
    first_deadline: f64,
    timeout_ms: u32,
    retry_window: RetryWindow,
}

impl CallTerms {
    /// Project the declared call chain within an existing immutable root. The
    /// live owner separately enforces pacing, structural credits and authority.
    pub fn window(&self, raw_now: f64, root_deadline: f64) -> Result<CallWindow, Error> {
        if self.timeout_ms == 0 || self.attempts == 0 {
            return Err(Error::Invalid);
        }
        if usize::from(self.attempts) > MAX_ATTEMPTS {
            return Err(Error::Capacity);
        }
        if !raw_now.is_finite() || !root_deadline.is_finite() {
            return Err(Error::Invalid);
        }
        if raw_now >= root_deadline {
            return Err(Error::TimedOut);
        }
        let span = u64::from(self.timeout_ms)
            .checked_mul(match self.retry_window {
                RetryWindow::Shared => 1,
                RetryWindow::FreshAttempt => u64::from(self.attempts),
            })
            .ok_or(Error::Capacity)?;
        let deadline = project_deadline(raw_now, span)?.min(root_deadline);
        let first_deadline = project_deadline(raw_now, u64::from(self.timeout_ms))?.min(deadline);
        Ok(CallWindow {
            created_at: raw_now,
            deadline,
            first_deadline,
            timeout_ms: self.timeout_ms,
            retry_window: self.retry_window,
        })
    }
}

impl CallWindow {
    pub fn deadline(&self) -> f64 {
        self.deadline
    }

    /// Compute a physical cap at attempt creation, without selecting or granting
    /// an attempt. Capture it once in Lifecycle; delayed prepare/bind/claim and
    /// reprepare must retain that cap rather than recompute it.
    pub fn physical_deadline(&self, raw_now: f64) -> Result<f64, Error> {
        if !raw_now.is_finite() || raw_now < self.created_at {
            return Err(Error::Invalid);
        }
        let deadline = match self.retry_window {
            RetryWindow::Shared => self.first_deadline,
            RetryWindow::FreshAttempt => self.deadline,
        };
        if raw_now >= deadline {
            return Err(Error::TimedOut);
        }
        match self.retry_window {
            RetryWindow::Shared => Ok(deadline),
            RetryWindow::FreshAttempt => {
                Ok(project_deadline(raw_now, u64::from(self.timeout_ms))?.min(deadline))
            }
        }
    }
}

/// A predeclared terminal after one top-level serial step. This is bounded
/// declaration data, not an execution handle or permission to skip a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutationTerminalBound {
    pub after_step: usize,
}

/// Immutable all-Mutation preparation barrier for one top-level parallel step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutationBarrierBound {
    pub step: usize,
}

/// One logical call. `attempts` includes the initial attempt; all attempts count
/// against journal capacity even when they share one deadline. An optional
/// fallback follows the complete parent attempt chain once, never per retry.
pub struct CallBound<'a> {
    pub timeout_ms: u32,
    pub attempts: u16,
    pub retry_window: RetryWindow,
    pub after_refusal: &'a [StepBound<'a>],
}

/// Pacing followed by concurrent calls. A plan's steps run serially.
pub struct StepBound<'a> {
    pub wait_ms: u32,
    pub calls: &'a [CallBound<'a>],
}

/// Checked declaration only: no resource reservation, Context, timer, retry,
/// publication or completion permission is created by computing this envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanBound {
    span_ms: u64,
    logical_calls: usize,
    physical_attempts: usize,
    peak_concurrent: usize,
}

#[derive(Default)]
struct Span {
    ms: u64,
    peak: usize,
}

struct Frame<'a> {
    steps: &'a [StepBound<'a>],
    calls: &'a [CallBound<'a>],
    total: Span,
    step: Option<(u32, Span)>,
    parent_ms: Option<u64>,
}

impl<'a> Frame<'a> {
    fn new(steps: &'a [StepBound<'a>]) -> Self {
        Self {
            steps,
            calls: &[],
            total: Span::default(),
            step: None,
            parent_ms: None,
        }
    }

    fn call(&mut self, ms: u64, peak: usize) -> Result<(), Error> {
        let (_, step) = self.step.as_mut().expect("call belongs to a step");
        step.ms = step.ms.max(ms);
        step.peak = step.peak.checked_add(peak).ok_or(Error::Capacity)?;
        Ok(())
    }
}

impl PlanBound {
    #[cfg(feature = "serial-plans")]
    pub(super) fn from_single_call(terms: CallTerms) -> Result<Self, Error> {
        if terms.timeout_ms == 0 || terms.attempts == 0 {
            return Err(Error::Invalid);
        }
        let physical_attempts = usize::from(terms.attempts);
        if physical_attempts > MAX_ATTEMPTS {
            return Err(Error::Capacity);
        }
        let span_ms = u64::from(terms.timeout_ms)
            .checked_mul(match terms.retry_window {
                RetryWindow::Shared => 1,
                RetryWindow::FreshAttempt => u64::from(terms.attempts),
            })
            .and_then(|ms| ms.checked_add(u64::from(terms.wait_ms)))
            .ok_or(Error::Capacity)?;
        Ok(Self {
            span_ms,
            logical_calls: 1,
            physical_attempts,
            peak_concurrent: 1,
        })
    }

    /// Count every occurrence, including aliased fallback slices. Charge before
    /// descending so cyclic declarations refuse at the existing journal bound.
    /// Iterative traversal keeps stack use independent of declaration depth.
    pub fn from_steps<'a>(steps: &'a [StepBound<'a>]) -> Result<Self, Error> {
        if steps.is_empty() {
            return Err(Error::Invalid);
        }
        let mut stack = vec![Frame::new(steps)];
        let mut logical_calls = 0usize;
        let mut physical_attempts = 0usize;
        loop {
            let frame = stack.last_mut().expect("root or fallback frame");
            if let Some((call, remaining)) = frame.calls.split_first() {
                if call.timeout_ms == 0 || call.attempts == 0 {
                    return Err(Error::Invalid);
                }
                physical_attempts = physical_attempts
                    .checked_add(usize::from(call.attempts))
                    .filter(|&count| count <= MAX_ATTEMPTS)
                    .ok_or(Error::Capacity)?;
                logical_calls = logical_calls.checked_add(1).ok_or(Error::Capacity)?;
                frame.calls = remaining;
                let ms = u64::from(call.timeout_ms)
                    .checked_mul(match call.retry_window {
                        RetryWindow::Shared => 1,
                        RetryWindow::FreshAttempt => u64::from(call.attempts),
                    })
                    .ok_or(Error::Capacity)?;
                if call.after_refusal.is_empty() {
                    frame.call(ms, 1)?;
                } else {
                    frame.parent_ms = Some(ms);
                    stack.push(Frame::new(call.after_refusal));
                }
            } else if let Some((wait, step)) = frame.step.take() {
                frame.total.ms = frame
                    .total
                    .ms
                    .checked_add(u64::from(wait))
                    .and_then(|ms| ms.checked_add(step.ms))
                    .ok_or(Error::Capacity)?;
                frame.total.peak = frame.total.peak.max(step.peak);
            } else if let Some((step, remaining)) = frame.steps.split_first() {
                if step.calls.is_empty() {
                    return Err(Error::Invalid);
                }
                frame.steps = remaining;
                frame.calls = step.calls;
                frame.step = Some((step.wait_ms, Span::default()));
            } else {
                let complete = stack.pop().expect("completed frame").total;
                if let Some(parent) = stack.last_mut() {
                    let ms = parent
                        .parent_ms
                        .take()
                        .expect("fallback has a parent call")
                        .checked_add(complete.ms)
                        .ok_or(Error::Capacity)?;
                    parent.call(ms, complete.peak.max(1))?;
                } else {
                    return Ok(Self {
                        span_ms: complete.ms,
                        logical_calls,
                        physical_attempts,
                        peak_concurrent: complete.peak,
                    });
                }
            }
        }
    }

    pub fn span_ms(&self) -> u64 {
        self.span_ms
    }

    pub fn logical_calls(&self) -> usize {
        self.logical_calls
    }

    pub fn physical_attempts(&self) -> usize {
        self.physical_attempts
    }

    /// Safe upper bound: concurrent parents can enter different fallback phases
    /// while peers remain live. Sum their individual peaks, rather than taking
    /// only the largest fallback width. Serial steps contribute their maximum.
    pub fn peak_concurrent(&self) -> usize {
        self.peak_concurrent
    }

    /// Project onto a finite raw clock without shortening the declared span.
    /// Refuse nonfinite clocks or unrepresentable deadlines; this issues no lease.
    pub fn deadline(&self, raw_now: f64) -> Result<f64, Error> {
        project_deadline(raw_now, self.span_ms)
    }
}

fn project_deadline(raw_now: f64, span_ms: u64) -> Result<f64, Error> {
    if !raw_now.is_finite() {
        return Err(Error::Invalid);
    }
    let span = span_ms as f64;
    let mut deadline = raw_now + span;
    if !deadline.is_finite() || deadline <= raw_now {
        return Err(Error::Capacity);
    }
    // Error-free TwoSum residual detects downward rounding even when the
    // subtraction `deadline - raw_now` itself rounds back up to `span`.
    let virtual_span = deadline - raw_now;
    let residual = (raw_now - (deadline - virtual_span)) + (span - virtual_span);
    if residual > 0. {
        deadline = deadline.next_up();
    }
    if !deadline.is_finite() || deadline - raw_now < span {
        return Err(Error::Capacity);
    }
    Ok(deadline)
}

#[cfg(test)]
#[path = "request_context_plan_tests.rs"]
mod tests;

/// Authentication declared for one serial call; never inferred from its recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CallAuth {
    /// The Preflight's write credential or its separately declared read role.
    #[default]
    Preflight,
    /// The root's exact live login Session, with no provider signing authority.
    Session,
}

/// One call of a streamed serial Plan. It is admitted only from the previous
/// call's exact DATA custody, or its delayed-continuation callback.
#[cfg(feature = "serial-plans")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialCall {
    pub auth: CallAuth,
    /// Read or Mutation, under the Plan's admitted credential.
    pub operation: super::Operation,
    /// The per-call bound: `now + timeout_ms`. The root has no wall-clock.
    pub timeout_ms: u32,
    /// 1, or 2 for a native single resend of a proven refusal.
    pub attempts: u8,
}

/// Admission terms of a streamed serial Plan; there is no declared length.
#[cfg(feature = "serial-plans")]
#[derive(Clone, Debug, PartialEq)]
pub struct SerialPolicy {
    pub credential: Option<super::Credential>,
    pub pacing: Option<super::Pacing>,
    /// Definitive refusals permit `done_plan` (native `done []`); never an
    /// override of `fail_plan`.
    pub refusals_complete: bool,
}
