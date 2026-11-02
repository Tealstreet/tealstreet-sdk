//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, Notice, Outcome, VenueAction, VenueError,
};
use support::*;

const FLAT: &str = r#"{"code":"0","data":[]}"#;
const POSITIONS: &str = "account/positions";
const BALANCES: &str = "asset/balances";
const WALLET: &str = r#"{"code":"0","data":[{"currency":"USDT","balance":"1000"}]}"#;

fn row() -> String {
    format!(r#"{{"code":"0","data":[{POSITION}]}}"#)
}

/// The private account loaded with the fixture position, no read in flight.
fn started() -> Harness {
    let mut h = Harness::new();
    h.reply(
        "margin-mode",
        r#"{"code":"0","data":{"marginMode":"cross"}}"#,
    );
    h.reply(
        "position-mode",
        r#"{"code":"0","data":{"positionMode":"net_mode"}}"#,
    );
    h.open_private();
    while h.pending.iter().any(|r| r.url.contains(POSITIONS)) {
        h.reply(POSITIONS, &row());
        h.reply(BALANCES, WALLET);
    }
    h.pending.clear();
    h
}

fn read(h: &mut Harness, command: u32) {
    h.handle(
        h.now,
        Event::Command(Command::Venue {
            command,
            action: VenueAction::ReadPositions,
        }),
    );
    h.collect();
}

fn has(h: &Harness) -> bool {
    h.pending.iter().any(|r| r.url.contains(POSITIONS))
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
    h.e.store().positions().iter().map(|p| p.contracts).sum()
}

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    let mut h = started();
    let before = open(&h);
    assert!(before > 0.0);
    read(&mut h, 40);
    assert!(has(&h), "a read is issued");
    h.reply(POSITIONS, FLAT);
    assert!(done(&h, 40).is_none(), "its balance half is still out");
    assert_eq!(open(&h), before, "nothing commits before the balance");
    h.reply(BALANCES, WALLET);
    assert!(succeeded(&h, 40));
    assert_eq!(open(&h), 0.0, "the store is flat");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    let before = open(&h);
    read(&mut h, 39);
    let stale = h.take(POSITIONS);
    read(&mut h, 41);
    assert!(!has(&h), "no second read yet");
    h.respond(stale, &row());
    h.reply(BALANCES, WALLET);
    assert!(succeeded(&h, 39), "its own read answers it");
    assert!(done(&h, 41).is_none(), "the older read cannot");
    assert_eq!(open(&h), before);
    assert!(has(&h), "a new read follows");
    h.reply(POSITIONS, FLAT);
    h.reply(BALANCES, WALLET);
    assert!(succeeded(&h, 41));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    for leg in [POSITIONS, BALANCES] {
        let mut h = started();
        let before = open(&h);
        read(&mut h, 42);
        if leg == BALANCES {
            h.reply(POSITIONS, FLAT);
        }
        h.reply(
            leg,
            r#"{"code":"152401","msg":"Access key does not exist"}"#,
        );
        let error = failure(&h, 42);
        assert!(error.message.contains("does not exist"), "{error:?}");
        assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(open(&h), before, "the store stands");
    }
}

#[test]
fn a_lost_read_keeps_the_engines_typed_cause() {
    let mut h = started();
    read(&mut h, 43);
    let request = h.take(POSITIONS);
    h.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.collect();
    assert_eq!(failure(&h, 43).kind, Some(ErrorKind::Network));
    assert_eq!(failure(&h, 43).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = started();
    read(&mut h, 44);
    h.reply(POSITIONS, "not json");
    assert_eq!(failure(&h, 44).outcome, Some(Outcome::NotSent));

    let mut h = started();
    read(&mut h, 45);
    h.reply(
        POSITIONS,
        r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","positionSide":"net","positions":null}]}"#,
    );
    assert!(failure(&h, 45).message.contains("could not be mapped"));
    assert_eq!(failure(&h, 45).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_read_a_newer_stream_frame_held_is_not_a_commit() {
    let mut h = started();
    read(&mut h, 46);
    let doubled = POSITION.replace("\"positions\":\"4\"", "\"positions\":\"8\"");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{doubled}]}}"#
    ));
    let framed = open(&h);
    h.reply(POSITIONS, FLAT);
    h.reply(BALANCES, WALLET);
    assert!(done(&h, 46).is_none(), "the frame held that read");
    assert_eq!(open(&h), framed, "the frame stands");
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
    h.handle(retry, Event::TimerFired { timer });
    h.collect();
    assert!(has(&h), "a new read follows");
    h.reply(POSITIONS, FLAT);
    h.reply(BALANCES, WALLET);
    assert!(succeeded(&h, 46));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn before_settings_load_the_command_refuses_without_sending() {
    let mut h = Harness::new();
    h.pending.clear();
    read(&mut h, 60);
    let error = failure(&h, 60);
    assert_ne!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(h.pending.is_empty());
}

#[test]
fn an_owned_row_outside_the_catalogue_is_named_and_a_flat_one_is_not() {
    let row = POSITION.replace("BTC-USDT", "DOGE-USDT");
    let mut h = started();
    read(&mut h, 70);
    h.reply(POSITIONS, &format!(r#"{{"code":"0","data":[{row}]}}"#));
    h.reply(BALANCES, WALLET);
    assert!(
        matches!(done(&h, 70), Some(Ok(body)) if body == r#"["DOGE-USDT"]"#),
        "{:?}",
        done(&h, 70)
    );

    let flat = row.replace("\"positions\":\"4\"", "\"positions\":\"0\"");
    let mut h = started();
    read(&mut h, 71);
    h.reply(POSITIONS, &format!(r#"{{"code":"0","data":[{flat}]}}"#));
    h.reply(BALANCES, WALLET);
    assert!(succeeded(&h, 71), "{:?}", done(&h, 71));
}
