//! Full-plan validation and an indexed journal for venue-owned write drivers.
//! This module never performs I/O, signs, retries, cancels or rolls back a write.
use crate::{ErrorKind, Outcome, VenueError};
use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
}

/// One physical write; a native batch can contain several logical elements.
#[derive(Debug)]
pub struct PlannedWrite<T> {
    pub payload: T,
    pub elements: Vec<Element>,
}

/// Proven accepted effects that do not identify a newly created native order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppliedEffect {
    Cancel,
    Amend,
    TradingStop,
    Settings,
    DeferredChild,
    PositionTrigger,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ElementResult {
    Placed {
        #[serde(rename = "orderIds")]
        order_ids: Vec<String>,
    },
    /// Accepted without a new native order ID; targets identify existing state.
    Applied {
        effect: AppliedEffect,
        #[serde(rename = "targetOrderId", skip_serializing_if = "Option::is_none")]
        target_order_id: Option<String>,
        #[serde(rename = "positionKey", skip_serializing_if = "Option::is_none")]
        position_key: Option<String>,
    },
    /// A local refusal or definitive venue rejection; inspect `error.outcome`.
    NotSent {
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<Box<VenueError>>,
    },
    Unknown {
        error: Box<VenueError>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementOutcome {
    #[serde(flatten)]
    pub element: Element,
    #[serde(flatten)]
    pub result: ElementResult,
}

/// Complete outcomes stored behind one pointer in VenueError's size budget.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ElementOutcomes(pub Vec<ElementOutcome>);

impl std::ops::Deref for ElementOutcomes {
    type Target = [ElementOutcome];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    NoPendingWrite,
    WrongWrite,
    WrongElementCount,
    MissingOrderIdentity,
    EmptyEffectTarget,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoPendingWrite => "No write is pending",
            Self::WrongWrite => "Settlement does not match the pending write",
            Self::WrongElementCount => "Settlement must cover every element of the write",
            Self::MissingOrderIdentity => "Placed elements must retain their native identities",
            Self::EmptyEffectTarget => "Applied targets must be nonempty when supplied",
        })
    }
}
impl std::error::Error for PlanError {}

/// Validated, immutable wire payloads with at most one unsettled physical write.
/// Element indices must be consecutive in dispatch order, starting at zero.
#[derive(Debug)]
pub struct WritePlan<T> {
    writes: Vec<PlannedWrite<T>>,
    journal: Vec<ElementOutcome>,
    next: usize,
    pending: Option<usize>,
    stopped: bool,
}

fn refusal(message: &str) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::InvalidInput)
        .outcome(Outcome::NotSent)
}

fn result_for_failure(mut error: VenueError) -> ElementResult {
    if error
        .partial
        .as_ref()
        .is_some_and(|p| !p.order_ids.is_empty())
    {
        error.outcome = Some(Outcome::Unknown);
    }
    match error.outcome {
        Some(Outcome::NotSent | Outcome::Rejected) => ElementResult::NotSent {
            error: Some(Box::new(error)),
        },
        _ => {
            error.outcome = Some(Outcome::Unknown);
            ElementResult::Unknown {
                error: Box::new(error),
            }
        }
    }
}

impl<T> WritePlan<T> {
    /// Validate every exact payload before allowing the first dispatch.
    /// The callback must be pure; venue metadata, rounding and count rules stay
    /// with the venue. A refusal includes all logical elements as not sent.
    pub fn validate(
        writes: Vec<PlannedWrite<T>>,
        validate: impl Fn(&T) -> Result<(), VenueError>,
    ) -> Result<Self, VenueError> {
        let mut journal = Vec::new();
        let mut failure = None;
        for write in &writes {
            if write.elements.is_empty() && failure.is_none() {
                failure = Some(refusal("A write must contain at least one element"));
            }
            let error = validate(&write.payload).err().map(|mut e| {
                e.outcome = Some(Outcome::NotSent);
                e
            });
            for element in &write.elements {
                if element.index != journal.len() && failure.is_none() {
                    failure = Some(refusal("Write-plan element indices must be consecutive"));
                }
                journal.push(ElementOutcome {
                    element: element.clone(),
                    result: ElementResult::NotSent {
                        error: error.clone().map(Box::new),
                    },
                });
            }
            if failure.is_none() {
                failure = error;
            }
        }
        if let Some(mut error) = failure {
            error.elements = Some(Box::new(ElementOutcomes(journal)));
            return Err(error);
        }
        Ok(Self {
            writes,
            journal,
            next: 0,
            pending: None,
            stopped: false,
        })
    }

    /// Hand the next immutable payload to the venue's existing transport owner.
    /// Call only at physical dispatch. Pending writes cannot be released again.
    pub fn next_write(&mut self) -> Option<(usize, &PlannedWrite<T>)> {
        if self.stopped || self.pending.is_some() {
            return None;
        }
        self.observe_write(self.next)
    }

    /// Observe a branch selected by an existing venue driver without changing
    /// its continuation policy. Skipped writes stay unsent; indices never repeat.
    pub fn observe_write(&mut self, index: usize) -> Option<(usize, &PlannedWrite<T>)> {
        if self.pending.is_some() || index < self.next {
            return None;
        }
        let write = self.writes.get(index)?;
        self.next = index + 1;
        self.pending = Some(index);
        for element in &write.elements {
            self.journal[element.index].result = result_for_failure(
                VenueError::new("Write dispatched; awaiting outcome").outcome(Outcome::Unknown),
            );
        }
        Some((index, write))
    }

    /// Record explicit per-row facts, including mixed native batch responses.
    /// Refused or unknown rows stop `next_write`; reporting observers follow
    /// only their existing venue driver's explicit branch selection.
    pub fn settle(&mut self, write: usize, results: Vec<ElementResult>) -> Result<(), PlanError> {
        let pending = self.pending.ok_or(PlanError::NoPendingWrite)?;
        if pending != write {
            return Err(PlanError::WrongWrite);
        }
        let planned = &self.writes[pending];
        if results.len() != planned.elements.len() {
            return Err(PlanError::WrongElementCount);
        }
        if results
            .iter()
            .any(|r| matches!(r, ElementResult::Placed { order_ids } if order_ids.is_empty() || order_ids.iter().any(String::is_empty)))
        {
            return Err(PlanError::MissingOrderIdentity);
        }
        if results.iter().any(|r| {
            matches!(r, ElementResult::Applied { target_order_id, position_key, .. }
            if target_order_id.as_ref().is_some_and(String::is_empty)
                || position_key.as_ref().is_some_and(String::is_empty))
        }) {
            return Err(PlanError::EmptyEffectTarget);
        }
        for (element, result) in planned.elements.iter().zip(results) {
            let result = match result {
                ElementResult::Unknown { error } => {
                    result_for_failure((*error).outcome(Outcome::Unknown))
                }
                ElementResult::NotSent { error: Some(error) } => result_for_failure(*error),
                other => other,
            };
            self.stopped |= !matches!(
                result,
                ElementResult::Placed { .. } | ElementResult::Applied { .. }
            );
            self.journal[element.index].result = result;
        }
        self.pending = None;
        Ok(())
    }

    /// Fail a whole physical write when no finer per-row evidence is available.
    /// Unclassified transport errors become unknown, never a safe resend signal.
    pub fn fail(&mut self, write: usize, error: VenueError) -> Result<(), PlanError> {
        let pending = self.pending.ok_or(PlanError::NoPendingWrite)?;
        if pending != write {
            return Err(PlanError::WrongWrite);
        }
        let results = self.writes[pending]
            .elements
            .iter()
            .map(|_| result_for_failure(error.clone()))
            .collect();
        self.settle(write, results)
    }

    pub fn outcomes(&self) -> Vec<ElementOutcome> {
        self.journal.clone()
    }

    /// Attach complete indexed facts and accepted IDs to a terminal failure.
    /// An accepted prefix or an unresolved dispatched write prevents an aggregate
    /// notSent/rejected classification from describing the whole command.
    pub fn failure(&mut self, mut error: VenueError) -> VenueError {
        self.stopped = true;
        self.next = self.writes.len();
        let mut ids = error
            .partial
            .as_ref()
            .map_or_else(Vec::new, |partial| partial.order_ids.clone());
        let mut has_effect = !ids.is_empty();
        for element in &self.journal {
            match &element.result {
                ElementResult::Placed { order_ids } => {
                    has_effect = true;
                    for id in order_ids {
                        if !ids.contains(id) {
                            ids.push(id.clone());
                        }
                    }
                }
                ElementResult::Applied { .. } => has_effect = true,
                ElementResult::Unknown { error: detail }
                | ElementResult::NotSent {
                    error: Some(detail),
                } => {
                    has_effect |= matches!(element.result, ElementResult::Unknown { .. });
                    if let Some(partial) = &detail.partial {
                        has_effect |= !partial.order_ids.is_empty();
                        for id in &partial.order_ids {
                            if !ids.contains(id) {
                                ids.push(id.clone());
                            }
                        }
                        if error.partial.is_none() {
                            error.partial = Some(partial.clone());
                        }
                    }
                }
                ElementResult::NotSent { error: None } => {}
            }
        }
        if has_effect {
            error.outcome = Some(Outcome::Unknown);
        }
        if let Some(partial) = &mut error.partial {
            partial.order_ids = ids;
        } else {
            error = error.partial_ids(&ids);
        }
        error.elements = Some(Box::new(ElementOutcomes(self.outcomes())));
        error
    }
}
