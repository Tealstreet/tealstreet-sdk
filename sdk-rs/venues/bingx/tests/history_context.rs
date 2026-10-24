//! Original native positives precede the managed custody assertions.
mod support;

use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Query, ReadCredentials,
    Response, TimerId, Value, VenueError,
};
use safe_cex_venue_bingx::Bingx;
use std::collections::VecDeque;

const NOW: f64 = 1_702_713_615_000.;
const COMMAND: u32 = 42;
const FILLS: &str = r#"{"code":0,"data":{"fill_orders":[{"tradeId":"a","orderId":"o","side":"BUY","price":"1","qty":"1","time":1702713617500},{"tradeId":"b","orderId":"o","side":"BUY","price":"1","qty":"1","time":1702713616500}]}}"#;

#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    prepared: usize,
    claimed: usize,
}

// The same drain runs native and managed sources. It never substitutes a
// request or terminal, and checks the real Prepare/Bind/Claim handoff.
fn drain(engine: &mut Engine<Bingx>, now: f64) -> Output {
    let mut output = Output::default();
    let mut queue: VecDeque<_> = engine.drain_actions().collect();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::Http(request) => {
                output.legacy += 1;
                output.requests.push(request);
            }
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(engine.http_preparation_pending(preparation));
                    output.prepared += 1;
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation.attempt().request, request.id);
                    assert!(matches!(
                        engine.claim_http(now, permit),
                        Ok(Claim::Released(_))
                    ));
                    output.claimed += 1;
                    output.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
            },
            action => output.actions.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}

fn engine() -> Engine<Bingx> {
    support::engine(support::Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    })
}

fn query(engine: &mut Engine<Bingx>, query: Query, wait: u32) -> Output {
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: COMMAND,
                query,
            },
            Some(wait),
        )
        .unwrap();
    drain(engine, NOW)
}

fn fills() -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: Some("o".into()),
        from_ms: Some(NOW + 1000.),
        to_ms: Some(NOW + 3000.),
        limit: Some(2),
    }
}

fn orders() -> Query {
    Query::OrderHistory {
        symbol: Some("BTCUSDT".into()),
        from_ms: None,
        to_ms: None,
        limit: Some(2),
    }
}

fn income() -> Query {
    Query::Income {
        symbol: Some("BTCUSDT".into()),
        kind: Some("FUNDING_FEE".into()),
        from_ms: None,
        to_ms: None,
        limit: Some(2),
    }
}

fn public() -> Query {
    Query::PublicTrades {
        symbol: "BTCUSDT".into(),
        limit: Some(2),
    }
}

fn reply(engine: &mut Engine<Bingx>, now: f64, request: &HttpRequest, body: &str) -> Output {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    drain(engine, now)
}

fn terminals(output: &Output) -> Vec<Result<&str, &VenueError>> {
    output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => Some(Ok(result.as_str())),
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => Some(Err(error)),
            _ => None,
        })
        .collect()
}

fn markers(output: &Output) -> usize {
    output.actions.iter().filter(|action| matches!(action,
        Action::Notice(Notice::Event(event))
        if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        && event.fields.iter().any(|(key, value)| *key == "command" && *value == Value::Num(f64::from(COMMAND)))
    )).count()
}

fn delay(output: &Output, deadline: f64) -> TimerId {
    let timers: Vec<_> = output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(
        timers.len(),
        1,
        "one native page delay: {:?}",
        output.actions
    );
    timers[0]
}

fn strict_timeout(output: &Output) {
    let results = terminals(output);
    assert_eq!(results.len(), 1, "one exact root terminal: {results:?}");
    let error = results[0].expect_err("expiry must not publish rows");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    assert_eq!(markers(output), 0);
    assert!(output.requests.is_empty());
}

fn private_release(query_kind: Query, path: &str, required: &[&str]) {
    let mut engine = engine();
    let output = query(&mut engine, query_kind, 1000);
    assert_eq!(output.requests.len(), 1);
    let request = &output.requests[0];
    assert!(request.url.contains(path), "{}", request.url);
    for part in required {
        assert!(request.url.contains(part), "{} lacks {part}", request.url);
    }
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert!(request.body.is_none());
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "X-BX-APIKEY" && value == "read-key")
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "X-SOURCE-KEY" && value == "Tealstreet")
    );
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
}

#[test]
fn fills_release_preserves_native_query_and_secondary_selection() {
    private_release(
        fills(),
        "/openApi/swap/v2/trade/fillHistory?",
        &[
            "startTs=1702713616000",
            "endTs=1702713618000",
            "pageIndex=1",
            "pageSize=2",
            "orderId=o",
            "symbol=BTC-USDT",
            "signature=",
        ],
    );
}

#[test]
fn orders_release_preserves_native_query_and_secondary_selection() {
    private_release(
        orders(),
        "/openApi/swap/v2/trade/allOrders?",
        &["limit=2", "symbol=BTC-USDT", "signature="],
    );
}

#[test]
fn income_release_preserves_native_query_and_secondary_selection() {
    private_release(
        income(),
        "/openApi/swap/v2/user/income?",
        &[
            "incomeType=FUNDING_FEE",
            "limit=2",
            "symbol=BTC-USDT",
            "signature=",
        ],
    );
}

#[test]
fn public_release_stays_unsigned_and_uses_managed_custody() {
    let mut engine = engine();
    let output = query(&mut engine, public(), 1000);
    assert_eq!(output.requests.len(), 1);
    let request = &output.requests[0];
    assert!(
        request
            .url
            .ends_with("/openApi/swap/v2/quote/trades?limit=2&symbol=BTC-USDT")
    );
    assert!(!request.headers.iter().any(|(key, _)| *key == "X-BX-APIKEY"));
    assert!(!request.url.contains("signature="));
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "X-SOURCE-KEY" && value == "Tealstreet")
    );
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
}

#[test]
fn parked_fill_root_expires_without_issuing_the_next_page() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 1000);
    assert_eq!(first.requests.len(), 1);
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    assert!(terminals(&accepted).is_empty());
    let timer = delay(&accepted, NOW + 201.);
    engine.prepare_dispatch(NOW + 1000.);
    strict_timeout(&drain(&mut engine, NOW + 1000.));
    engine.handle(NOW + 1000., Event::TimerFired { timer });
    let stale = drain(&mut engine, NOW + 1000.);
    assert!(stale.requests.is_empty());
    assert!(terminals(&stale).is_empty());
    assert_eq!(markers(&stale), 0);
}

#[test]
fn public_data_at_the_original_receipt_deadline_cannot_complete() {
    let mut engine = engine();
    let first = query(&mut engine, public(), 1000);
    assert_eq!(first.requests.len(), 1);
    let output = reply(
        &mut engine,
        NOW + 1000.,
        &first.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    strict_timeout(&output);
}

#[test]
fn signed_data_at_the_original_receipt_deadline_cannot_complete() {
    let mut engine = engine();
    let first = query(&mut engine, income(), 1000);
    assert_eq!(first.requests.len(), 1);
    let output = reply(
        &mut engine,
        NOW + 1000.,
        &first.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    strict_timeout(&output);
}

#[test]
fn raised_wait_allows_an_in_bound_unsigned_result() {
    let mut engine = engine();
    let first = query(&mut engine, public(), 200_000);
    let output = reply(
        &mut engine,
        NOW + 19_000.,
        &first.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(terminals(&output), [Ok("[]")]);
    assert_eq!(markers(&output), 0);
    assert!(output.requests.is_empty());
}

#[test]
fn a_later_page_failure_is_strict_even_after_accepted_rows() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 10_000);
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    assert!(terminals(&accepted).is_empty());
    let timer = delay(&accepted, NOW + 201.);
    engine.handle(NOW + 201., Event::TimerFired { timer });
    let next = drain(&mut engine, NOW + 201.);
    assert_eq!(next.requests.len(), 1);
    assert!(next.requests[0].url.contains("pageIndex=2"));
    let output = reply(
        &mut engine,
        NOW + 202.,
        &next.requests[0],
        r#"{"code":109400,"msg":"invalid query","data":[]}"#,
    );
    let results = terminals(&output);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].unwrap_err().message, "109400: invalid query");
    assert_eq!(markers(&output), 0);
    assert!(output.requests.is_empty());
}

#[test]
fn signature_rejection_resigns_once_without_changing_the_page_or_read_key() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 10_000);
    let rejection = r#"{"code":100001,"msg":"signature verification failed","data":[]}"#;
    let second = reply(&mut engine, NOW + 1., &first.requests[0], rejection);
    assert!(terminals(&second).is_empty());
    assert_eq!(second.requests.len(), 1);
    let request = &second.requests[0];
    assert!(request.url.contains("pageIndex=1"));
    assert!(request.url.contains("startTs=1702713616000"));
    assert!(
        request
            .headers
            .iter()
            .any(|(key, value)| *key == "X-BX-APIKEY" && value == "read-key")
    );
    assert_ne!(first.requests[0].id, request.id);
    let output = reply(&mut engine, NOW + 2., request, rejection);
    let results = terminals(&output);
    assert_eq!(results.len(), 1);
    assert!(results[0].unwrap_err().message.contains("100001"));
    assert!(output.requests.is_empty());
    assert_eq!(markers(&output), 0);
}
