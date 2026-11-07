//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, Notice, Outcome, SocketChannel, SocketId, VenueAction,
    VenueError,
};
use safe_cex_venue_paradex::Options;
use support::*;

const POSITION: &str = r#"{"market":"BTC-USD-PERP","status":"OPEN","side":"LONG","size":"2","average_entry_price":"100","unrealized_pnl":"4","realized_positional_pnl":"1","last_updated_at":1700000000000}"#;
const FLAT: &str = r#"{"results":[]}"#;

fn loaded() -> (Harness, SocketId) {
    let mut h = Harness::keyed(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        STARK_KEY,
    );
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    let socket = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.reply("/auth/", r#"{"jwt_token":"jwt-first"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", &format!("{{\"results\":[{POSITION}]}}"));
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"800"}"#,
    );
    h.reply("/orders", FLAT);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    assert!(h.pending.is_empty());
    (h, socket)
}

fn read(h: &mut Harness, command: u32) {
    h.command(Command::Venue {
        command,
        action: VenueAction::ReadPositions,
    });
}

fn has(h: &Harness) -> bool {
    h.pending.iter().any(|r| r.url.contains("/positions"))
}

fn finished(h: &Harness, id: u32) -> bool {
    h.actions.iter().any(|a| {
        matches!(a,
            Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. })
                if *command == id)
    })
}

fn failure(h: &Harness, id: u32) -> VenueError {
    h.actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(error.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a failure for {id}"))
}

fn open(h: &Harness) -> f64 {
    h.e.store().positions().iter().map(|p| p.contracts).sum()
}

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    let (mut h, _) = loaded();
    assert_eq!(open(&h), 2.0);
    read(&mut h, 40);
    assert!(has(&h), "a read is issued");
    assert!(!finished(&h, 40));
    h.reply("/positions", FLAT);
    assert_eq!(h.result(40), Ok(r#"{"skipped":[],"open":[]}"#));
    assert_eq!(open(&h), 0.0, "the store is flat");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let (mut h, _) = loaded();
    read(&mut h, 39);
    let stale = h.take("/positions");
    read(&mut h, 41);
    assert!(!has(&h), "no second read yet");
    h.respond(stale, 200, &format!("{{\"results\":[{POSITION}]}}"));
    assert_eq!(
        h.result(39),
        Ok(r#"{"skipped":[],"open":[{"symbol":"BTC-USD-PERP","side":"long","contracts":2.0}]}"#),
        "its own read answers it"
    );
    assert!(!finished(&h, 41), "the older read cannot");
    assert_eq!(open(&h), 2.0);
    assert!(has(&h), "a new read follows");
    h.reply("/positions", FLAT);
    assert_eq!(h.result(41), Ok(r#"{"skipped":[],"open":[]}"#));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let (mut h, _) = loaded();
    read(&mut h, 42);
    let request = h.take("/positions");
    h.respond(
        request,
        401,
        r#"{"error":"NOT_AUTHORIZED","message":"bad jwt"}"#,
    );
    let error = failure(&h, 42);
    assert!(error.message.contains("NOT_AUTHORIZED"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(open(&h), 2.0, "the store stands");
}

#[test]
fn a_lost_read_keeps_the_engines_typed_cause() {
    let (mut h, _) = loaded();
    read(&mut h, 43);
    let request = h.take("/positions");
    h.e.handle(
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
    let (mut h, _) = loaded();
    read(&mut h, 44);
    h.reply("/positions", "not json");
    assert_eq!(failure(&h, 44).outcome, Some(Outcome::NotSent));

    let (mut h, _) = loaded();
    read(&mut h, 45);
    h.reply(
        "/positions",
        r#"{"results":[{"market":"BTC-USD-PERP","status":"OPEN","side":"SIDEWAYS"}]}"#,
    );
    assert!(failure(&h, 45).message.contains("could not be mapped"));
    assert_eq!(failure(&h, 45).outcome, Some(Outcome::NotSent));
    assert_eq!(open(&h), 2.0, "the rejected row's stored copy stands");
}

#[test]
fn a_read_a_newer_stream_frame_held_is_not_a_commit() {
    let (mut h, socket) = loaded();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    while has(&h) {
        h.reply("/positions", &format!("{{\"results\":[{POSITION}]}}"));
    }
    read(&mut h, 46);
    let frame = POSITION
        .replace("\"2\"", "\"3\"")
        .replace("1700000000000", "1700000001000");
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &format!(
                r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"positions","data":{frame}}}}}"#
            ),
        },
    );
    h.collect();
    assert_eq!(open(&h), 3.0);
    h.reply("/positions", &format!("{{\"results\":[{POSITION}]}}"));
    assert!(!finished(&h, 46), "the frame held that read");
    assert_eq!(open(&h), 3.0, "the frame stands");
    let now = h.now + 500.0;
    h.timer(now);
    assert!(has(&h), "a new read follows");
    h.reply("/positions", FLAT);
    assert_eq!(h.result(46), Ok(r#"{"skipped":[],"open":[]}"#));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn a_public_engine_refuses_without_sending() {
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
    for market in ["BTC-USD-250101-100000-C", "OLD-USD-PERP"] {
        let row = POSITION.replace("BTC-USD-PERP", market);
        let (mut h, _) = loaded();
        read(&mut h, 70);
        h.reply("/positions", &format!("{{\"results\":[{row}]}}"));
        assert_eq!(
            h.result(70),
            Ok(format!(r#"{{"skipped":["{market}"],"open":[]}}"#).as_str())
        );
        assert_eq!(open(&h), 0.0, "the store is flat: its rows are absent");

        let flat = row.replace("\"size\":\"2\"", "\"size\":\"0\"");
        let (mut h, _) = loaded();
        read(&mut h, 71);
        h.reply("/positions", &format!("{{\"results\":[{flat}]}}"));
        assert_eq!(h.result(71), Ok(r#"{"skipped":[],"open":[]}"#), "{market}");
    }
}
