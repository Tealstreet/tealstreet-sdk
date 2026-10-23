//! Retained bounded plan topology. Slot transitions carry no I/O or outcome
//! authority: the live owner must obtain journal proof before selecting them.
use super::{CallBound, PlanBound, RetryWindow, StepBound};
use crate::request_context::Error;
use std::sync::Arc;

/// Private, schedule-bound metadata ordinal; never an engine resource ID.
#[derive(Clone, Debug)]
pub struct Slot {
    issuer: Arc<()>,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallTerms {
    pub wait_ms: u32,
    pub timeout_ms: u32,
    pub attempts: u16,
    pub retry_window: RetryWindow,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Available,
    Active(u16),
    Branch,
    Complete,
}

struct Call {
    sequence: usize,
    terms: CallTerms,
    fallback: Option<usize>,
    state: State,
}

struct Sequence {
    steps: Vec<Vec<usize>>,
    cursor: usize,
    parent: Option<usize>,
}

/// Owns declarations and structural credits only. It cannot authorize a retry,
/// effect, timer, send or successful command result.
pub struct Schedule {
    issuer: Arc<()>,
    bound: PlanBound,
    calls: Vec<Call>,
    sequences: Vec<Sequence>,
    used: usize,
    reserved: usize,
    cancelled: bool,
}

impl Schedule {
    pub fn from_steps<'a>(steps: &'a [StepBound<'a>]) -> Result<Self, Error> {
        let bound = PlanBound::from_steps(steps)?;
        let mut calls = Vec::new();
        calls
            .try_reserve_exact(bound.logical_calls())
            .map_err(|_| Error::Capacity)?;
        let mut sequences = Vec::new();
        sequences
            .try_reserve_exact(bound.logical_calls() + 1)
            .map_err(|_| Error::Capacity)?;
        sequences.push(Sequence::new(None));
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(bound.logical_calls() + 1)
            .map_err(|_| Error::Capacity)?;
        pending.push((0, steps));
        // The validator already charged every occurrence before descending,
        // including aliases/cycles. Copy iteratively within that exact bound.
        while let Some((sequence, steps)) = pending.pop() {
            let mut retained = Vec::new();
            retained
                .try_reserve_exact(steps.len())
                .map_err(|_| Error::Capacity)?;
            for step in steps {
                let mut indices = Vec::new();
                indices
                    .try_reserve_exact(step.calls.len())
                    .map_err(|_| Error::Capacity)?;
                for call in step.calls {
                    let index = calls.len();
                    let fallback = if call.after_refusal.is_empty() {
                        None
                    } else {
                        let child = sequences.len();
                        sequences.push(Sequence::new(Some(index)));
                        pending.push((child, call.after_refusal));
                        Some(child)
                    };
                    calls.push(Call {
                        sequence,
                        terms: CallTerms::from_bound(step.wait_ms, call),
                        fallback,
                        state: State::Waiting,
                    });
                    indices.push(index);
                }
                retained.push(indices);
            }
            sequences[sequence].steps = retained;
        }
        let mut schedule = Self {
            issuer: Arc::new(()),
            bound,
            calls,
            sequences,
            used: 0,
            reserved: bound.physical_attempts(),
            cancelled: false,
        };
        schedule.activate(0);
        Ok(schedule)
    }

    pub fn bound(&self) -> PlanBound {
        self.bound
    }

    pub fn available(&self) -> impl Iterator<Item = Slot> + '_ {
        self.calls
            .iter()
            .enumerate()
            .filter(|(_, call)| !self.cancelled && call.state == State::Available)
            .map(|(index, _)| Slot {
                issuer: self.issuer.clone(),
                index,
            })
    }

    pub fn terms(&self, slot: &Slot) -> Result<CallTerms, Error> {
        Ok(self.calls[self.index(slot)?].terms)
    }

    pub(crate) fn ordinal(&self, slot: &Slot) -> Result<usize, Error> {
        self.index(slot)
    }

    pub(crate) fn declarations(&self) -> impl Iterator<Item = CallTerms> + '_ {
        self.calls.iter().map(|call| call.terms)
    }

    pub(crate) fn replacement_slots(&self, slot: &Slot) -> Result<Vec<Slot>, Error> {
        let call = &self.calls[self.index(slot)?];
        if !matches!(call.state, State::Active(_)) {
            return Err(Error::State);
        }
        let sequence = call.fallback.ok_or(Error::Invalid)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(self.calls.len())
            .map_err(|_| Error::Capacity)?;
        for (index, call) in self.calls.iter().enumerate() {
            if call.sequence == sequence {
                slots.push(Slot {
                    issuer: self.issuer.clone(),
                    index,
                })
            }
        }
        Ok(slots)
    }

    pub fn start(&mut self, slot: &Slot) -> Result<(), Error> {
        let index = self.index(slot)?;
        if self.calls[index].state != State::Available {
            return Err(Error::State);
        }
        self.calls[index].state = State::Active(1);
        self.use_credit();
        Ok(())
    }

    /// Structural credit only. This does not classify a native reply or grant
    /// effects-free replay; the live owner must validate its journal first.
    pub fn retry(&mut self, slot: &Slot) -> Result<(), Error> {
        let index = self.index(slot)?;
        let State::Active(used) = self.calls[index].state else {
            return Err(Error::State);
        };
        if used >= self.calls[index].terms.attempts {
            return Err(Error::Capacity);
        }
        self.calls[index].state = State::Active(used + 1);
        self.use_credit();
        Ok(())
    }

    /// Settle one structural call and release all unselected fallback credits.
    /// No accepted/rejected/unknown result is inferred by this operation.
    pub fn settle(&mut self, slot: &Slot) -> Result<(), Error> {
        let index = self.index(slot)?;
        let State::Active(used) = self.calls[index].state else {
            return Err(Error::State);
        };
        self.reserved -= usize::from(self.calls[index].terms.attempts - used);
        if let Some(fallback) = self.calls[index].fallback {
            self.release_unselected(fallback);
        }
        self.calls[index].state = State::Complete;
        self.advance(self.calls[index].sequence);
        Ok(())
    }

    /// Select the declared branch after the existing journal proves a parent
    /// refusal. The branch follows the parent once, never once per retry.
    pub fn replace(&mut self, slot: &Slot) -> Result<(), Error> {
        let index = self.index(slot)?;
        let State::Active(used) = self.calls[index].state else {
            return Err(Error::State);
        };
        let child = self.calls[index].fallback.ok_or(Error::Invalid)?;
        self.reserved -= usize::from(self.calls[index].terms.attempts - used);
        self.calls[index].state = State::Branch;
        self.activate(child);
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.reserved = 0;
    }

    pub fn is_complete(&self) -> bool {
        !self.cancelled && self.sequences[0].cursor == self.sequences[0].steps.len()
    }

    pub fn used_attempts(&self) -> usize {
        self.used
    }

    pub fn reserved_attempts(&self) -> usize {
        self.reserved
    }

    pub fn released_attempts(&self) -> usize {
        self.bound.physical_attempts() - self.used - self.reserved
    }

    fn index(&self, slot: &Slot) -> Result<usize, Error> {
        if !Arc::ptr_eq(&slot.issuer, &self.issuer) {
            return Err(Error::Revoked);
        }
        if self.cancelled {
            return Err(Error::State);
        }
        self.calls
            .get(slot.index)
            .ok_or(Error::Revoked)
            .map(|_| slot.index)
    }

    fn use_credit(&mut self) {
        self.reserved -= 1;
        self.used += 1;
    }

    fn activate(&mut self, sequence: usize) {
        let state = &self.sequences[sequence];
        for &index in &state.steps[state.cursor] {
            debug_assert!(self.calls[index].state == State::Waiting);
            self.calls[index].state = State::Available;
        }
    }

    fn advance(&mut self, mut sequence: usize) {
        loop {
            let state = &mut self.sequences[sequence];
            if !state.steps[state.cursor]
                .iter()
                .all(|&index| self.calls[index].state == State::Complete)
            {
                return;
            }
            state.cursor += 1;
            if state.cursor < state.steps.len() {
                self.activate(sequence);
                return;
            }
            let Some(parent) = state.parent else { return };
            debug_assert!(self.calls[parent].state == State::Branch);
            self.calls[parent].state = State::Complete;
            sequence = self.calls[parent].sequence;
        }
    }

    fn release_unselected(&mut self, sequence: usize) {
        // At most one entry per retained call. No recursive stack or allocation
        // during a state transition; selected siblings cannot occur here.
        for index in 0..self.calls.len() {
            let mut ancestor = self.calls[index].sequence;
            loop {
                if ancestor == sequence {
                    debug_assert!(self.calls[index].state == State::Waiting);
                    self.reserved -= usize::from(self.calls[index].terms.attempts);
                    break;
                }
                let Some(parent) = self.sequences[ancestor].parent else {
                    break;
                };
                ancestor = self.calls[parent].sequence;
            }
        }
    }
}

impl Sequence {
    fn new(parent: Option<usize>) -> Self {
        Self {
            steps: Vec::new(),
            cursor: 0,
            parent,
        }
    }
}

impl CallTerms {
    fn from_bound(wait_ms: u32, call: &CallBound<'_>) -> Self {
        Self {
            wait_ms,
            timeout_ms: call.timeout_ms,
            attempts: call.attempts,
            retry_window: call.retry_window,
        }
    }
}

#[cfg(test)]
#[path = "request_context_schedule_tests.rs"]
mod tests;
