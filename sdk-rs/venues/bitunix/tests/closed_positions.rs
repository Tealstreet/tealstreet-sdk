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
