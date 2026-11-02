//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, Notice, Outcome, VenueAction, VenueError,
};
use support::*;

const ROW: &str = r#"{"code":0,"result":{"data":[{"instrument_name":"BTCUSD-PERP","quantity":"1","cost":"99","update_timestamp_ms":1}]}}"#;
const POSITIONS: &str = "get-positions";

fn started() -> Harness {
    let mut h = Harness::new();
    h.reply(POSITIONS, ROW);
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    h
}

/// The rest of the chain a positions read started: orders only every fourth cycle.
fn finish_cycle(h: &mut Harness) {
    h.reply("user-balance", BALANCE);
    for path in ["private/get-open-orders", "advanced/get-open-orders"] {
        let out = h
            .actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains(path)));
        if out {
            h.reply(path, EMPTY);
        }
    }
}

fn read(h: &mut Harness, command: u32) {
    h.event(Event::Command(Command::Venue {
        command,
        action: VenueAction::ReadPositions,
    }));
}

fn has(h: &Harness) -> bool {
    h.actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.contains(POSITIONS)))
}

fn done(h: &Harness, id: u32) -> Option<Result<String, VenueError>> {
    h.actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
            Some(Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
            Some(Err(error.clone()))
        }
        _ => None,
    })
}

fn failure(h: &Harness, id: u32) -> VenueError {
    match done(h, id) {
        Some(Err(error)) => error,
        other => panic!("expected a failure for {id}: {other:?}"),
    }
}

fn succeeded(h: &Harness, id: u32) -> bool {
    matches!(done(h, id), Some(Ok(result)) if result == "[]")
}

fn open(h: &Harness) -> f64 {
    h.engine
        .store()
        .positions()
        .iter()
        .map(|p| p.contracts)
        .sum()
}

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    let mut h = started();
    assert_eq!(open(&h), 1.0);
    read(&mut h, 40);
    assert!(has(&h), "a read is issued");
    assert!(done(&h, 40).is_none());
    h.reply(POSITIONS, EMPTY);
    assert!(succeeded(&h, 40));
    assert_eq!(open(&h), 0.0, "the store is flat");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    read(&mut h, 39);
    let stale = h.http(POSITIONS);
    read(&mut h, 41);
    assert!(!has(&h), "no second read yet");
    h.respond(&stale, ROW);
    assert!(succeeded(&h, 39), "its own read answers it");
    assert!(done(&h, 41).is_none(), "the older read cannot");
    assert_eq!(open(&h), 1.0);
    finish_cycle(&mut h);
    assert!(has(&h), "a new read follows once the cycle ends");
    h.reply(POSITIONS, EMPTY);
    assert!(succeeded(&h, 41));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn a_command_during_the_rest_of_a_cycle_waits_for_a_new_read() {
    let mut h = started();
    read(&mut h, 42);
    h.reply(POSITIONS, ROW);
    assert!(succeeded(&h, 42));
    read(&mut h, 43);
    assert!(!has(&h), "the chain is still running");
    finish_cycle(&mut h);
    assert!(done(&h, 43).is_none(), "the chain's own read predates it");
    assert!(has(&h), "a new read follows");
    h.reply(POSITIONS, EMPTY);
    assert!(succeeded(&h, 43));
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    read(&mut h, 44);
    h.reply(POSITIONS, r#"{"code":40103,"message":"IP_ILLEGAL"}"#);
    let error = failure(&h, 44);
    assert!(error.message.contains("IP_ILLEGAL"), "{error:?}");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(open(&h), 1.0, "the store stands");
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = started();
    read(&mut h, 45);
    h.reply(POSITIONS, "not json");
    assert_eq!(failure(&h, 45).outcome, Some(Outcome::NotSent));

    let mut h = started();
    read(&mut h, 46);
    h.reply(
        POSITIONS,
        r#"{"code":0,"result":{"data":[{"instrument_name":"BTCUSD-PERP","quantity":{}}]}}"#,
    );
    assert!(failure(&h, 46).message.contains("could not be mapped"));
    assert_eq!(failure(&h, 46).outcome, Some(Outcome::NotSent));
    assert_eq!(open(&h), 1.0, "the rejected row's stored copy stands");
}

#[test]
fn a_read_a_newer_stream_row_held_is_not_a_commit() {
    let mut h = started();
    read(&mut h, 47);
    h.ws(
        "user.positions",
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"3","cost":"300","open_position_pnl":"3","leverage":"10","liquidation_price":"80","update_timestamp_ms":5}"#,
    );
    assert_eq!(open(&h), 3.0);
    h.reply(POSITIONS, EMPTY);
    assert!(done(&h, 47).is_none(), "the row held that read");
    assert_eq!(open(&h), 3.0, "the row stands");
    finish_cycle(&mut h);
    let retry = h.now + 500.0;
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == retry => Some(*timer),
            _ => None,
        })
        .expect("a retry is armed");
    h.now = retry;
    h.event(Event::TimerFired { timer });
    assert!(has(&h), "a new read follows");
    h.reply(POSITIONS, EMPTY);
    assert!(succeeded(&h, 47), "{:?}", done(&h, 47));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn before_the_account_loads_the_command_refuses_without_sending() {
    let mut h = Harness::new();
    let out = h.actions.len();
    read(&mut h, 60);
    let error = failure(&h, 60);
    assert_ne!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(
        !h.actions[out..]
            .iter()
            .any(|a| matches!(a, Action::Http(_))),
        "nothing is sent"
    );
}
