//! `readPositions`: success only after a read issued after the command commits.
mod support;
use safe_cex_engine::{Command, ErrorKind, Event, Outcome, Response, VenueAction};
use support::*;

const FLAT: &str = r#"{"code":"00000","data":[]}"#;
const SIZE_TWO: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"2","openPriceAvg":"90","markPrice":"100","leverage":"20","unrealizedPL":"10","liquidationPrice":"50","marginSize":"50","marginMode":"crossed","posMode":"one_way_mode","uTime":"200"}]}"#;
const CLASSIC: &str = "/all-position";
const UTA_FLAT: &str = r#"{"code":"00000","data":{"list":[]}}"#;
const UTA: &str = "/current-position";

/// A Classic account with the BTCUSDT position loaded, no read in flight.
fn classic() -> Harness {
    let mut h = Harness::classic();
    while has(&h, CLASSIC) {
        h.respond(CLASSIC, 200, POSITION);
    }
    h
}

/// A Unified account with the BTCUSDT position loaded.
fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    h.respond(UTA, 200, r#"{"code":"00000","data":{"list":[{"symbol":"BTCUSDT","posSide":"long","total":"1","size":"1","avgPrice":"90","markPrice":"100","leverage":"20","unrealisedPnl":"10","liqPrice":"50","marginSize":"50","marginMode":"crossed","posMode":"one_way_mode","updatedTime":"100"}]}}"#);
    h
}

fn read(h: &mut Harness, command: u32) {
    h.command(Command::Venue {
        command,
        action: VenueAction::ReadPositions,
    });
}

fn size(h: &Harness) -> f64 {
    h.engine
        .store()
        .positions()
        .iter()
        .find(|p| p.symbol == "BTCUSDT")
        .map_or(0.0, |p| p.contracts)
}

fn result(h: &Harness, id: u32) -> Option<&Result<String, String>> {
    h.done.iter().find(|(c, _)| *c == id).map(|(_, r)| r)
}

fn failure(h: &Harness, id: u32) -> (Option<ErrorKind>, Option<Outcome>) {
    assert!(matches!(result(h, id), Some(Err(_))), "{:?}", result(h, id));
    let error = h.failures.last().expect("a failure");
    (error.kind, error.outcome)
}

fn has(h: &Harness, path: &str) -> bool {
    h.requests.iter().any(|r| r.url.contains(path))
}

#[test]
fn classic_read_commits_then_it_succeeds() {
    let mut h = classic();
    assert_eq!(size(&h), 1.0);
    read(&mut h, 40);
    assert!(has(&h, CLASSIC));
    assert_eq!(result(&h, 40), None);
    h.respond(CLASSIC, 200, SIZE_TWO);
    assert_eq!(result(&h, 40), Some(&Ok("[]".to_owned())));
    assert_eq!(size(&h), 2.0);
}

#[test]
fn classic_read_in_flight_is_followed_by_a_new_one() {
    // The startup read is the one in flight when the command arrives.
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    let stale = h.take(CLASSIC);
    read(&mut h, 41);
    assert!(!has(&h, CLASSIC), "no second read while one is out");
    h.reply(stale.id, 200, SIZE_TWO);
    assert_eq!(result(&h, 41), None, "the older read cannot answer");
    assert_eq!(size(&h), 2.0, "it still commits");
    assert!(has(&h, CLASSIC), "a new read follows");
    h.respond(CLASSIC, 200, FLAT);
    assert_eq!(result(&h, 41), Some(&Ok("[]".to_owned())));
    assert_eq!(size(&h), 0.0);
}

#[test]
fn classic_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = classic();
    read(&mut h, 42);
    h.respond(
        CLASSIC,
        200,
        r#"{"code":"40014","msg":"Incorrect permissions","data":null}"#,
    );
    assert!(
        matches!(result(&h, 42), Some(Err(message)) if message.contains("Incorrect permissions")),
        "{:?}",
        result(&h, 42)
    );
    let (kind, outcome) = failure(&h, 42);
    assert_eq!(kind, Some(ErrorKind::AuthPermission));
    assert_eq!(outcome, Some(Outcome::NotSent));
    assert_eq!(size(&h), 1.0);
}

#[test]
fn classic_malformed_reply_and_unmappable_row_fail_the_command() {
    let mut h = classic();
    read(&mut h, 43);
    h.respond(CLASSIC, 200, "<html>");
    assert!(
        matches!(result(&h, 43), Some(Err(_))),
        "{:?}",
        result(&h, 43)
    );
    assert_eq!(failure(&h, 43).1, Some(Outcome::NotSent));

    let mut h = classic();
    read(&mut h, 44);
    h.respond(CLASSIC, 200, r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","posMode":"one_way_mode"}]}"#);
    assert!(
        matches!(result(&h, 44), Some(Err(message)) if message.contains("could not be mapped")),
        "{:?}",
        result(&h, 44)
    );
    assert_eq!(failure(&h, 44).1, Some(Outcome::NotSent));
    assert_eq!(size(&h), 1.0, "the cached row is retained");
}

#[test]
fn a_read_the_stream_held_is_not_a_commit() {
    let mut h = classic();
    // A stream row marks the position; a flat read cannot speak for it for 5 s.
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"update","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","openPriceAvg":"90","posMode":"one_way_mode","marginMode":"crossed","uTime":"200"}]}"#);
    read(&mut h, 45);
    h.respond(CLASSIC, 200, FLAT);
    assert_eq!(result(&h, 45), None, "the row was held for the stream");
    assert_eq!(size(&h), 1.0);
    let framed_at = h.now;
    while result(&h, 45).is_none() {
        assert!(h.now - framed_at <= 7_000.0, "the guard lapses");
        h.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (timer, at) = h.timers.remove(0);
        h.now = at;
        h.event(Event::TimerFired { timer });
        while has(&h, CLASSIC) {
            h.respond(CLASSIC, 200, FLAT);
        }
    }
    assert!(h.now - framed_at > 5_000.0, "committed only past the guard");
    assert_eq!(result(&h, 45), Some(&Ok("[]".to_owned())));
    assert_eq!(size(&h), 0.0);
}

#[test]
fn a_rate_limited_account_refuses_without_sending() {
    let mut h = classic();
    read(&mut h, 46);
    let request = h.take(CLASSIC);
    h.event(Event::HttpResponse {
        request: request.id,
        response: Response {
            status: 429,
            headers: "retry-after: 30\n",
            body: r#"{"code":"429","msg":"Too Many Requests"}"#,
        },
    });
    assert!(matches!(result(&h, 46), Some(Err(_))));
    h.requests.clear();
    read(&mut h, 47);
    let (kind, outcome) = failure(&h, 47);
    assert_eq!(kind, Some(ErrorKind::RateLimited));
    assert_eq!(outcome, Some(Outcome::NotSent));
    assert!(!has(&h, CLASSIC));
}

#[test]
fn uta_read_commits_then_it_succeeds() {
    let mut h = uta();
    read(&mut h, 50);
    assert!(has(&h, UTA));
    h.respond(UTA, 200, UTA_FLAT);
    assert_eq!(result(&h, 50), Some(&Ok("[]".to_owned())));
    assert_eq!(size(&h), 0.0);
}

#[test]
fn uta_read_in_flight_is_followed_by_a_new_one() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    // The startup read is the one in flight when the command arrives.
    let stale = h.take(UTA);
    read(&mut h, 51);
    assert!(!has(&h, UTA));
    h.reply(stale.id, 200, UTA_FLAT);
    assert_eq!(result(&h, 51), None);
    assert!(has(&h, UTA));
    h.respond(UTA, 200, UTA_FLAT);
    assert_eq!(result(&h, 51), Some(&Ok("[]".to_owned())));
}

#[test]
fn uta_failed_read_fails_the_command() {
    let mut h = uta();
    read(&mut h, 52);
    h.respond(
        UTA,
        200,
        r#"{"code":"40014","msg":"Incorrect permissions","data":null}"#,
    );
    let (kind, outcome) = failure(&h, 52);
    assert_eq!(kind, Some(ErrorKind::AuthPermission));
    assert_eq!(outcome, Some(Outcome::NotSent));
}

#[test]
fn a_public_engine_refuses_without_sending() {
    let mut h = Harness::public();
    read(&mut h, 60);
    let (kind, outcome) = failure(&h, 60);
    assert_ne!(kind, Some(ErrorKind::Unsupported));
    assert_eq!(outcome, Some(Outcome::NotSent));
}
