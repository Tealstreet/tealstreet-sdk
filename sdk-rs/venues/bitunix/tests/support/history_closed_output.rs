//! Venue-reported closed positions: raw `get_history_positions` rows, paged by
//! `skip` and keyed by `positionId` exactly as TS `fetchBitunixHistoryPages`.
#[path = "../support/mod.rs"]
mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Notice, Response, VenueError};
use support::*;

const PATH: &str = "/api/v1/futures/position/get_history_positions";

fn fetch(e: &mut TestEngine, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    e.handle(
        1.0,
        Event::Command(safe_cex_engine::command::decode_command(&serde_json::json!({"op":"fetchClosedPnl", "command":9, "symbol":symbol, "from":from, "to":to, "account":"fixture-account"}).to_string()).unwrap()),
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

#[test]
fn reported_closed_pnl_is_shaped_before_the_native_terminal() {
    let (mut e, _) = startup();
    fetch(
        &mut e,
        Some("BTCUSDT"),
        Some(1699999999000.),
        Some(1700000002000.),
    );
    let actions = answer(
        &mut e,
        vec![],
        200,
        &page(&[row("position-1", 1700000001000)]),
    );
    let result: serde_json::Value =
        serde_json::from_str(&outcome(&actions).unwrap().unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../engine/tests/fixtures/history-closed-output.json"
    ))
    .unwrap();
    assert_eq!(result, expected["bitunix"]);
}
