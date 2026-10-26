//! Venue-reported closed positions: raw `get_history_positions` rows, paged by
//! `skip` and keyed by `positionId` exactly as TS `fetchBitunixHistoryPages`.
mod support;
use safe_cex_engine::{
    Action, ClosedPositionFormat, Command, ErrorKind, Event, HttpRequest, Notice, PositionHistory,
    Response, VenueAction, VenueError,
};
use support::*;

const PATH: &str = "/api/v1/futures/position/get_history_positions";

fn fetch(e: &mut TestEngine, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    e.handle(
        1.0,
        Event::Command(Command::Venue {
            command: 9,
            action: VenueAction::FetchClosedPositions {
                symbol: symbol.map(Into::into),
                from_ms: from,
                to_ms: to,
            },
        }),
    );
}

/// The one live page, from `actions` or else the next prepared batch.
fn page_request(e: &mut TestEngine, actions: Vec<Action>) -> HttpRequest {
    let actions = if actions.is_empty() {
        safe_cex_engine::test_support::prepared_actions(e, 100.0)
    } else {
        actions
    };
    let mut pages: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains(PATH) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(pages.len(), 1, "exactly one live page");
    pages.remove(0)
}

fn outcome(actions: &[Action]) -> Option<Result<String, VenueError>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: 9, result }) => Some(Ok(result.clone())),
        Action::Notice(Notice::CommandFailed { command: 9, error }) => Some(Err(error.clone())),
        _ => None,
    })
}

fn respond(e: &mut TestEngine, request: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        2.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, 100.0)
}

/// Answers the one live page.
fn answer(e: &mut TestEngine, previous: Vec<Action>, status: u16, body: &str) -> Vec<Action> {
    let request = page_request(e, previous);
    respond(e, &request, status, body)
}

fn row(id: &str, mtime: u64) -> String {
    format!(
        r#"{{"positionId":"{id}","symbol":"BTCUSDT","side":"BUY","qty":"0.5","maxQty":"0.5","entryPrice":"60000","closePrice":"61000","fee":"1.2","funding":"0","realizedPNL":"498.8","leverage":"10","ctime":"{}","mtime":"{mtime}"}}"#,
        mtime.saturating_sub(1000)
    )
}

fn page(rows: &[String]) -> String {
    format!(
        r#"{{"code":0,"msg":"ok","data":{{"positionList":[{}]}}}}"#,
        rows.join(",")
    )
}

fn no_page(actions: &[Action]) -> bool {
    actions
        .iter()
        .all(|a| !matches!(a, Action::Http(r) if r.url.contains(PATH)))
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("missing {name} header"))
}

fn rows(text: &str) -> Vec<serde_json::Value> {
    serde_json::from_str(text).unwrap()
}

#[test]
fn capabilities_declare_the_reported_closed_axis() {
    let (e, _) = startup();
    let capabilities = e.capabilities();
    assert!(capabilities.closed_positions);
    assert_eq!(capabilities.position_history, PositionHistory::Bitunix);
    assert_eq!(
        capabilities.closed_position_format,
        ClosedPositionFormat::Bitunix
    );
}

#[test]
fn read_key_signs_skip_pages_and_raw_rows_pass_through_deduped_by_position_id() {
    let (mut e, _) = startup_with_engine(engine(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-material".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-material".into(),
            passphrase: None,
        }),
        ..Options::default()
    }));
    let from = 1_700_000_000_000.0;
    let to = 1_800_000_000_000.0;
    fetch(&mut e, Some("BTCUSDT"), Some(from), Some(to));
    let first = page_request(&mut e, vec![]);
    assert_eq!(first.method, safe_cex_engine::Method::Get);
    assert!(
        first.url.ends_with(&format!(
            "{PATH}?endTime=1800000000000&limit=100&startTime=1700000000000&symbol=BTCUSDT"
        )),
        "{}",
        first.url
    );
    assert!(first.body.is_none());
    assert_eq!(header(&first, "api-key"), "read-key");
    let digest = safe_cex_crypto::hex(&safe_cex_crypto::sha256(
        format!(
            "{}{}read-keyendTime1800000000000limit100startTime1700000000000symbolBTCUSDT",
            header(&first, "nonce"),
            header(&first, "timestamp")
        )
        .as_bytes(),
    ));
    assert_eq!(
        header(&first, "sign"),
        safe_cex_crypto::hex(&safe_cex_crypto::sha256(
            format!("{digest}read-material").as_bytes()
        ))
    );
    let full: Vec<String> = (0..100)
        .map(|i| row(&format!("p{i}"), 1_750_000_000_000 - i))
        .collect();
    let actions = respond(&mut e, &first, 200, &page(&full));
    assert!(outcome(&actions).is_none());
    let second = page_request(&mut e, actions);
    assert!(
        second.url.ends_with(&format!(
            "{PATH}?endTime=1800000000000&limit=100&skip=100&startTime=1700000000000&symbol=BTCUSDT"
        )),
        "{}",
        second.url
    );
    let fresh = row("p100", 1_740_000_000_000);
    let actions = respond(
        &mut e,
        &second,
        200,
        &page(&[row("p5", 1_740_000_000_000), fresh.clone()]),
    );
    let got = rows(&outcome(&actions).unwrap().unwrap());
    assert_eq!(got.len(), 101, "the repeated positionId is dropped");
    assert_eq!(
        got[0],
        serde_json::from_str::<serde_json::Value>(&full[0]).unwrap()
    );
    assert_eq!(
        got[100],
        serde_json::from_str::<serde_json::Value>(&fresh).unwrap()
    );
}

#[test]
fn window_bounds_filter_rows_and_reaching_the_start_ends_the_walk() {
    let (mut e, _) = startup();
    fetch(&mut e, None, Some(2_000.0), Some(5_000.0));
    let first = page_request(&mut e, vec![]);
    assert!(
        first
            .url
            .ends_with(&format!("{PATH}?endTime=5000&limit=100&startTime=2000")),
        "{}",
        first.url
    );
    // Newer than the window, inside it (mtime wins over ctime), missing times, then older.
    let mut page_rows = vec![
        row("late", 6_000),
        r#"{"positionId":"in","mtime":3000,"ctime":1}"#.to_string(),
        r#"{"positionId":"ctime-only","mtime":null,"ctime":"4000"}"#.to_string(),
        r#"{"positionId":"untimed"}"#.to_string(),
    ];
    page_rows.extend((0..96).map(|i| row(&format!("old{i}"), 1_500)));
    let actions = respond(&mut e, &first, 200, &page(&page_rows));
    let got = rows(&outcome(&actions).expect("the start ends the walk").unwrap());
    let ids: Vec<_> = got.iter().map(|r| r["positionId"].clone()).collect();
    assert_eq!(ids, ["in", "ctime-only", "untimed"]);
    assert!(no_page(&actions));
}

#[test]
fn a_page_adding_nothing_or_a_short_page_ends_the_walk() {
    let (mut e, _) = startup();
    fetch(&mut e, None, None, None);
    let full: Vec<String> = (0..100).map(|i| row(&format!("p{i}"), 1_000 + i)).collect();
    let actions = answer(&mut e, vec![], 200, &page(&full));
    let actions = answer(&mut e, actions, 200, &page(&full));
    assert_eq!(rows(&outcome(&actions).unwrap().unwrap()).len(), 100);
    assert!(no_page(&actions));

    fetch(&mut e, None, None, None);
    let actions = answer(&mut e, vec![], 200, &page(&full[..3]));
    assert_eq!(rows(&outcome(&actions).unwrap().unwrap()).len(), 3);
}

#[test]
fn non_success_pages_are_typed_failures_and_later_failures_keep_accepted_rows() {
    let (mut e, _) = startup();
    fetch(&mut e, None, None, None);
    let actions = answer(&mut e, vec![], 503, "Service Unavailable");
    let error = outcome(&actions).unwrap().unwrap_err();
    assert_eq!(error.message, "Bitunix closed positions: Bitunix HTTP 503");
    assert!(error.kind.is_some(), "a classified failure, not untyped");

    fetch(&mut e, None, None, None);
    let actions = answer(
        &mut e,
        vec![],
        200,
        r#"{"code":10007,"msg":"Signature Error","data":null}"#,
    );
    let error = outcome(&actions).unwrap().unwrap_err();
    assert_eq!(error.message, "Bitunix closed positions: Signature Error");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));

    fetch(&mut e, None, None, None);
    let full: Vec<String> = (0..100).map(|i| row(&format!("p{i}"), 9_000 - i)).collect();
    let actions = answer(&mut e, vec![], 200, &page(&full));
    // Rows streamed before a failed envelope verdict are never admitted.
    let actions = answer(&mut e, actions, 500, &page(&[row("untrusted", 100)]));
    let got = rows(&outcome(&actions).unwrap().unwrap());
    assert_eq!(got.len(), 100);
    assert!(got.iter().all(|r| r["positionId"] != "untrusted"));
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event))
            if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}

#[test]
fn public_sessions_read_nothing() {
    let mut e = engine(Options::default());
    e.start(0.0);
    fetch(&mut e, Some("BTCUSDT"), None, None);
    let actions = safe_cex_engine::test_support::prepared_now(&mut e);
    assert!(
        actions
            .iter()
            .all(|a| !matches!(a, Action::Http(r) if r.url.contains(PATH)))
    );
    assert_eq!(outcome(&actions), Some(Ok("[]".into())));
}

/// dazzy3 nit 1: an unquoted exponent time is JS `String(3e3)` = "3000", so
/// `parseInt` reads 3000, not 3; the row stays in the window.
#[test]
fn an_exponent_number_time_reads_as_js_string_does() {
    let (mut e, _) = startup();
    fetch(&mut e, None, Some(2000.0), Some(5000.0));
    let first = page_request(&mut e, vec![]);
    let page_rows = vec![
        r#"{"positionId":"exp","mtime":3e3}"#.to_string(),
        r#"{"positionId":"frac","mtime":2.5e3}"#.to_string(),
        r#"{"positionId":"tiny","mtime":1e-7}"#.to_string(),
    ];
    let actions = respond(&mut e, &first, 200, &page(&page_rows));
    let got = rows(
        &outcome(&actions)
            .expect("a short page ends the walk")
            .unwrap(),
    );
    let ids: Vec<_> = got.iter().map(|r| r["positionId"].clone()).collect();
    // `String(1e-7)` is "1e-7": parseInt reads 1, before the window.
    assert_eq!(ids, ["exp", "frac"]);
}

fn incomplete(actions: &[Action]) -> bool {
    actions.iter().any(|a| {
        matches!(a, Action::Notice(Notice::Event(event))
            if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)
    })
}

/// dazzy3 nit 3a: the 100-page cap (TS `fetchBitunixHistoryPages`) ends the
/// walk with every row and marks the result INCOMPLETE.
#[test]
fn the_page_cap_returns_every_row_and_marks_incomplete() {
    let (mut e, _) = startup();
    fetch(&mut e, None, None, None);
    let mut actions = vec![];
    for p in 0..100u64 {
        let full: Vec<String> = (0..100u64)
            .map(|i| row(&format!("p{p}-{i}"), 10_000_000 - p * 100 - i))
            .collect();
        actions = answer(&mut e, actions, 200, &page(&full));
        if p < 99 {
            assert!(outcome(&actions).is_none(), "page {p} continues");
        }
    }
    assert_eq!(rows(&outcome(&actions).unwrap().unwrap()).len(), 10_000);
    assert!(no_page(&actions), "the cap sends no 101st page");
    assert!(incomplete(&actions), "the cap is marked INCOMPLETE");
}

/// dazzy3 nit 3e: a page adding nothing ends the walk as complete, not INCOMPLETE.
#[test]
fn a_page_adding_nothing_is_complete() {
    let (mut e, _) = startup();
    fetch(&mut e, None, None, None);
    let full: Vec<String> = (0..100).map(|i| row(&format!("p{i}"), 1_000 + i)).collect();
    let actions = answer(&mut e, vec![], 200, &page(&full));
    let actions = answer(&mut e, actions, 200, &page(&full));
    assert_eq!(rows(&outcome(&actions).unwrap().unwrap()).len(), 100);
    assert!(!incomplete(&actions));
}

/// dazzy3 nit 3c: a from-only window drops older rows and ends at the start; a
/// to-only window drops newer rows and keeps walking older ones.
#[test]
fn from_only_and_to_only_windows_filter_their_own_edge() {
    let (mut e, _) = startup();
    fetch(&mut e, None, Some(2_000.0), None);
    let actions = answer(
        &mut e,
        vec![],
        200,
        &page(&[row("new", 9_000), row("edge", 2_000), row("old", 1_999)]),
    );
    let ids: Vec<_> = rows(&outcome(&actions).unwrap().unwrap())
        .iter()
        .map(|r| r["positionId"].clone())
        .collect();
    assert_eq!(ids, ["new", "edge"], "from is inclusive");

    fetch(&mut e, None, None, Some(5_000.0));
    let actions = answer(
        &mut e,
        vec![],
        200,
        &page(&[row("late", 5_001), row("edge", 5_000), row("old", 10)]),
    );
    let ids: Vec<_> = rows(&outcome(&actions).unwrap().unwrap())
        .iter()
        .map(|r| r["positionId"].clone())
        .collect();
    assert_eq!(ids, ["edge", "old"], "to is inclusive");
}

/// dazzy3 nit 3d: rows without a `positionId` are not deduped against each other.
#[test]
fn id_less_rows_are_not_deduped() {
    let (mut e, _) = startup();
    fetch(&mut e, None, None, None);
    let actions = answer(
        &mut e,
        vec![],
        200,
        &page(&[
            r#"{"mtime":"3000","realizedPNL":"1"}"#.to_string(),
            r#"{"mtime":"2000","realizedPNL":"2"}"#.to_string(),
        ]),
    );
    assert_eq!(rows(&outcome(&actions).unwrap().unwrap()).len(), 2);
}

/// dazzy3 nit 3b: a page delivered again after its command finished, or for a
/// superseded fetch, is ignored; the live page still completes its own command.
#[test]
fn late_and_stale_page_deliveries_are_ignored() {
    let (mut e, _) = startup();
    fetch(&mut e, None, None, None);
    let first = page_request(&mut e, vec![]);
    let done = respond(&mut e, &first, 200, &page(&[row("a", 3_000)]));
    assert_eq!(rows(&outcome(&done).unwrap().unwrap()).len(), 1);
    let late = respond(&mut e, &first, 200, &page(&[row("late", 2_000)]));
    assert!(
        outcome(&late).is_none(),
        "a late delivery completes nothing"
    );
    assert!(no_page(&late));

    fetch(&mut e, None, None, None);
    let live = page_request(&mut e, vec![]);
    let stale = respond(&mut e, &first, 200, &page(&[row("stale", 2_000)]));
    assert!(
        outcome(&stale).is_none(),
        "the finished page cannot answer a new fetch"
    );
    let actions = respond(&mut e, &live, 200, &page(&[row("b", 3_000)]));
    let ids: Vec<_> = rows(&outcome(&actions).unwrap().unwrap())
        .iter()
        .map(|r| r["positionId"].clone())
        .collect();
    assert_eq!(ids, ["b"]);
}
