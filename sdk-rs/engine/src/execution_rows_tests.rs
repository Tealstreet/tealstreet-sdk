use super::*;
use crate::ErrorKind;
use crate::write_plan::Element;

fn row(result: ElementResult) -> ElementOutcome {
    ElementOutcome {
        element: Element {
            index: 0,
            client_order_id: None,
        },
        result,
    }
}

fn placed(id: &str) -> ElementOutcome {
    row(ElementResult::Placed {
        order_ids: vec![id.into()],
    })
}

fn refused(message: &str) -> ElementOutcome {
    row(ElementResult::NotSent {
        error: Some(Box::new(
            VenueError::new(message)
                .kind(ErrorKind::Rejected)
                .outcome(Outcome::Rejected),
        )),
    })
}

fn unknown() -> ElementOutcome {
    row(ElementResult::Unknown {
        error: Box::new(VenueError::new("unproven").outcome(Outcome::Unknown)),
    })
}

fn failed(outcome: Outcome, rows: Vec<ElementOutcome>) -> Result<String, VenueError> {
    let mut error = VenueError::new("batch").outcome(outcome);
    error.elements = Some(Box::new(ElementOutcomes(rows)));
    Err(error)
}

#[test]
fn a_success_hands_each_observer_only_its_own_order_ids() {
    let rows = [placed("1"), placed("2"), placed("3")];
    let done = Ok("[\"1\",\"2\",\"3\"]".to_owned());
    assert_eq!(project(&done, Some(&rows), 0..1, 3).unwrap(), "[\"1\"]");
    assert_eq!(
        project(&done, Some(&rows), 1..3, 3).unwrap(),
        "[\"2\",\"3\"]"
    );
}

#[test]
fn a_success_without_complete_rows_is_unknown_for_everyone() {
    let done = Ok("[\"1\"]".to_owned());
    for rows in [None, Some(&[placed("1")][..])] {
        let error = project(&done, rows, 0..1, 2).unwrap_err();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }
}

#[test]
fn a_refused_row_fails_only_its_observer() {
    let terminal = failed(
        Outcome::Unknown,
        vec![
            placed("1"),
            refused("Insufficient margin to place order."),
            placed("3"),
        ],
    );
    assert_eq!(project(&terminal, None, 0..1, 3).unwrap(), "[\"1\"]");
    assert_eq!(project(&terminal, None, 2..3, 3).unwrap(), "[\"3\"]");
    let error = project(&terminal, None, 1..2, 3).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(error.message, "Insufficient margin to place order.");
    assert!(error.partial.is_none());
    let elements = error.elements.expect("own rows");
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].element.index, 0);
}

#[test]
fn an_unproven_row_is_unknown_only_for_its_observer() {
    let terminal = failed(Outcome::Unknown, vec![placed("1"), unknown()]);
    assert_eq!(project(&terminal, None, 0..1, 2).unwrap(), "[\"1\"]");
    let error = project(&terminal, None, 1..2, 2).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.message, UNKNOWN_OUTCOME_TEXT);
}

#[test]
fn placed_and_refused_rows_of_one_observer_are_unknown_with_partial_ids() {
    let terminal = failed(
        Outcome::Unknown,
        vec![placed("1"), refused("Order has invalid price.")],
    );
    let error = project(&terminal, None, 0..2, 2).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.expect("partial").order_ids, ["1"]);
}

#[test]
fn an_unsettled_row_is_unsent_only_when_the_whole_terminal_is_definitive() {
    let blank = || row(ElementResult::NotSent { error: None });
    let unsent = failed(Outcome::NotSent, vec![blank(), blank()]);
    let error = project(&unsent, None, 1..2, 2).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    let uncertain = failed(Outcome::Unknown, vec![blank(), blank()]);
    let error = project(&uncertain, None, 1..2, 2).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    // A sent-side NotSent fact without a native refusal proves nothing either.
    let local = row(ElementResult::NotSent {
        error: Some(Box::new(VenueError::new("local").outcome(Outcome::NotSent))),
    });
    let error = project(&failed(Outcome::Unknown, vec![local]), None, 0..1, 1).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
}

#[test]
fn a_failure_without_complete_rows_reaches_every_observer_whole() {
    let mut lost = VenueError::new("transport reply lost").outcome(Outcome::Unknown);
    assert_eq!(
        project(&Err(lost.clone()), None, 1..2, 2),
        Err(lost.clone())
    );
    lost.elements = Some(Box::new(ElementOutcomes(vec![placed("1")])));
    assert_eq!(project(&Err(lost.clone()), None, 1..2, 2), Err(lost));
}
