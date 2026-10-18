//! Native Store reads preserve their original cause across the lower Store API.
use std::{any::Any, fmt, rc::Rc};

/// The boundary at which a Store read failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreReadPhase {
    Property,
    Projection,
}

/// An opaque native cause remains available to the exchange layer for rethrowing.
/// Diagnostics describe the boundary without inspecting the thrown value.
#[derive(Clone)]
pub struct StoreReadError {
    phase: StoreReadPhase,
    cause: Rc<dyn Any>,
}

impl StoreReadError {
    pub fn new(phase: StoreReadPhase, cause: Rc<dyn Any>) -> Self {
        Self { phase, cause }
    }

    pub fn phase(&self) -> StoreReadPhase {
        self.phase
    }

    pub fn native_cause(&self) -> &Rc<dyn Any> {
        &self.cause
    }
}

impl fmt::Debug for StoreReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreReadError")
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for StoreReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.phase {
            StoreReadPhase::Property => "Store property access failed",
            StoreReadPhase::Projection => "Store typed projection failed",
        })
    }
}

impl PartialEq for StoreReadError {
    fn eq(&self, other: &Self) -> bool {
        self.phase == other.phase && Rc::ptr_eq(&self.cause, &other.cause)
    }
}

impl Eq for StoreReadError {}
impl std::error::Error for StoreReadError {}
