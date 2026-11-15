//! Reporting-only outcomes for writes dispatched by the venue.
//! This module never validates, schedules, stops, retries or performs a write.
use crate::VenueError;
use serde::Serialize;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
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
    UnknownElement,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("No reporting element has this index")
    }
}
impl std::error::Error for PlanError {}

/// Passive indexed outcomes; the venue owns all validation and dispatch policy.
#[derive(Debug)]
pub struct WritePlan {
    journal: Vec<ElementOutcome>,
}

impl WritePlan {
    /// Describe logical elements without validating or retaining wire payloads.
    pub fn new(elements: Vec<Element>) -> Self {
        Self {
            journal: elements
                .into_iter()
                .map(|element| ElementOutcome {
                    element,
                    result: ElementResult::NotSent { error: None },
                })
                .collect(),
        }
    }

    /// Record exactly the supplied fact, including after an earlier refusal.
    /// An unknown index is a reporting error and grants no dispatch authority.
    pub fn record(&mut self, index: usize, result: ElementResult) -> Result<(), PlanError> {
        let row = self
            .journal
            .iter_mut()
            .find(|row| row.element.index == index)
            .ok_or(PlanError::UnknownElement)?;
        row.result = result;
        Ok(())
    }

    pub fn outcomes(&self) -> Vec<ElementOutcome> {
        self.journal.clone()
    }

    /// Add only the optional journal; retain every existing aggregate error fact.
    pub fn attach(&self, mut error: VenueError) -> VenueError {
        error.elements = Some(Box::new(ElementOutcomes(self.outcomes())));
        error
    }
}
