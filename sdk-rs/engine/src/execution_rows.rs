//! Row-attributed execution projection: each observer receives only its own
//! declared rows of one shared write, never more certain than those rows' facts
//! (docs/plans/RUST_EXECUTION_GROUPS.md, "Row-attributed planned executions").
use crate::write_plan::{ElementOutcome, ElementOutcomes, ElementResult};
use crate::{Outcome, UNKNOWN_OUTCOME_TEXT, VenueError};
use std::ops::Range;

/// One observer's terminal from the shared terminal. A success requires the
/// venue's complete per-row outcomes; a failure without complete per-row
/// outcomes reaches every observer whole, exactly as an unattributed seal.
pub(crate) fn project(
    terminal: &Result<String, VenueError>,
    success: Option<&[ElementOutcome]>,
    rows: Range<usize>,
    total: usize,
) -> Result<String, VenueError> {
    let (outcomes, whole) = match terminal {
        Ok(_) => match success {
            Some(outcomes) if outcomes.len() == total => (outcomes, None),
            // A success without its rows cannot say whose orders were placed.
            _ => {
                return Err(VenueError::new(UNKNOWN_OUTCOME_TEXT).outcome(Outcome::Unknown));
            }
        },
        Err(error) => match error.elements.as_deref() {
            Some(outcomes) if outcomes.len() == total => (&outcomes.0[..], Some(error)),
            _ => return Err(error.clone()),
        },
    };
    let Some(own) = outcomes.get(rows) else {
        return Err(whole
            .cloned()
            .unwrap_or_else(|| VenueError::new(UNKNOWN_OUTCOME_TEXT).outcome(Outcome::Unknown)));
    };
    // Only a whole terminal that is itself definitive proves an unsettled row unsent.
    let definitive_whole = whole
        .is_none_or(|error| matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)));
    let mut placed = Vec::new();
    let mut refused: Vec<&VenueError> = Vec::new();
    let mut unknown = false;
    for row in own {
        match &row.result {
            ElementResult::Placed { order_ids } => placed.extend(order_ids.iter().cloned()),
            ElementResult::Applied { .. } => {}
            // A native per-row refusal is definitive on its own.
            ElementResult::NotSent { error: Some(error) }
                if error.outcome == Some(Outcome::Rejected) || definitive_whole =>
            {
                refused.push(error)
            }
            ElementResult::NotSent { error: None } if definitive_whole => {}
            _ => unknown = true,
        }
    }
    let elements = Some(Box::new(ElementOutcomes(
        own.iter()
            .cloned()
            .enumerate()
            .map(|(index, mut row)| {
                row.element.index = index;
                row
            })
            .collect(),
    )));
    if !unknown && refused.is_empty() && own.iter().all(settled) {
        return Ok(serde_json::to_string(&placed).expect("order IDs serialize"));
    }
    let mut error = match (unknown, refused.first(), whole) {
        (false, Some(first), _) if placed.is_empty() => (*first).clone(),
        (_, _, Some(whole)) => whole.clone(),
        _ => VenueError::new(UNKNOWN_OUTCOME_TEXT),
    };
    if unknown || !placed.is_empty() || whole.is_none() {
        // Some of this observer's rows took effect or may have: never unsent.
        error.outcome = Some(Outcome::Unknown);
    }
    if unknown {
        error.message = UNKNOWN_OUTCOME_TEXT.into();
    } else if !refused.is_empty() {
        error.message = refused
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join(", ");
    }
    error.partial = None;
    error.elements = elements;
    Err(error.partial_ids(&placed))
}

fn settled(row: &ElementOutcome) -> bool {
    matches!(
        row.result,
        ElementResult::Placed { .. } | ElementResult::Applied { .. }
    )
}

#[cfg(test)]
#[path = "execution_rows_tests.rs"]
mod tests;
