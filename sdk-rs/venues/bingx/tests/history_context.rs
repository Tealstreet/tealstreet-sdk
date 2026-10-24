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

#[test]
fn signed_history_without_read_credentials_uses_the_primary_snapshot() {
    let mut engine = support::engine(support::Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        ..Default::default()
    });
    let output = query(&mut engine, orders(), 1000);
    assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
    assert_eq!(output.requests.len(), 1);
    assert!(
        output.requests[0]
            .headers
            .iter()
            .any(|(key, value)| *key == "X-BX-APIKEY" && value == "primary-key")
    );
    let done = reply(
        &mut engine,
        NOW + 1.,
        &output.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(terminals(&done), [Ok("[]")]);
}

#[test]
fn every_native_page_uses_a_fresh_managed_child() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 1000);
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    engine.handle(
        NOW + 201.,
        Event::TimerFired {
            timer: delay(&accepted, NOW + 201.),
        },
    );
    let second = drain(&mut engine, NOW + 201.);
    assert_eq!((second.legacy, second.prepared, second.claimed), (0, 1, 1));
    assert_eq!(second.requests.len(), 1);
    assert_ne!(first.requests[0].id, second.requests[0].id);
    assert!(second.requests[0].url.contains("pageIndex=2"));
    let done = reply(
        &mut engine,
        NOW + 202.,
        &second.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert!(terminals(&done)[0].is_ok());
}

#[test]
fn the_single_signature_resend_also_uses_managed_release() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 1000);
    let second = reply(
        &mut engine,
        NOW + 1.,
        &first.requests[0],
        r#"{"code":100001,"msg":"signature verification failed","data":[]}"#,
    );
    assert_eq!((second.legacy, second.prepared, second.claimed), (0, 1, 1));
    assert_eq!(second.requests.len(), 1);
    let done = reply(
        &mut engine,
        NOW + 2.,
        &second.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(terminals(&done), [Ok("[]")]);
}

fn full_page(page: u32) -> String {
    format!(
        r#"{{"code":0,"data":{{"fill_orders":[{{"tradeId":"{page}a","orderId":"o","side":"BUY","price":"1","qty":"1","time":1702713617500}},{{"tradeId":"{page}b","orderId":"o","side":"BUY","price":"1","qty":"1","time":1702713616500}}]}}}}"#
    )
}

fn slow_walk(wait: Option<u32>) -> (Engine<Bingx>, HttpRequest, f64) {
    let mut engine = engine();
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: COMMAND,
                query: fills(),
            },
            wait,
        )
        .unwrap();
    let mut output = drain(&mut engine, NOW);
    let mut now = NOW;
    for page in 1..=6 {
        assert_eq!((output.legacy, output.prepared, output.claimed), (0, 1, 1));
        assert_eq!(output.requests.len(), 1);
        assert!(
            output.requests[0]
                .url
                .contains(&format!("pageIndex={page}"))
        );
        now += 19_000.;
        let accepted = reply(&mut engine, now, &output.requests[0], &full_page(page));
        assert!(terminals(&accepted).is_empty());
        let timer = delay(&accepted, now + 200.);
        now += 200.;
        engine.handle(now, Event::TimerFired { timer });
        output = drain(&mut engine, now);
    }
    assert_eq!(now, NOW + 115_200.);
    assert_eq!(output.requests.len(), 1);
    assert!(output.requests[0].url.contains("pageIndex=7"));
    (engine, output.requests.remove(0), now)
}

#[test]
fn default_receipt_caps_the_seventh_page_without_renewal() {
    let (mut engine, page, _) = slow_walk(None);
    assert_eq!(page.timeout_ms, Some(4800));
    let expired = reply(
        &mut engine,
        NOW + 120_000.,
        &page,
        r#"{"code":0,"data":[]}"#,
    );
    strict_timeout(&expired);
    let late = reply(&mut engine, NOW + 120_001., &page, FILLS);
    assert!(terminals(&late).is_empty());
    assert_eq!(markers(&late), 0);
    assert!(late.requests.is_empty());
}

#[test]
fn raised_receipt_allows_the_same_walk_to_complete_beyond_120_seconds() {
    let (mut engine, page, now) = slow_walk(Some(200_000));
    assert_eq!(page.timeout_ms, Some(20_000));
    let done = reply(&mut engine, now + 19_000., &page, r#"{"code":0,"data":[]}"#);
    let results = terminals(&done);
    assert_eq!(results.len(), 1);
    let rows: Vec<serde_json::Value> = serde_json::from_str(results[0].unwrap()).unwrap();
    assert_eq!(rows.len(), 12);
    assert_eq!(markers(&done), 0);
}

#[test]
fn fractional_root_deadline_remains_live_half_a_millisecond_before_expiry() {
    let mut engine = engine();
    engine
        .handle_command(
            NOW + 0.25,
            Command::Query {
                command: COMMAND,
                query: public(),
            },
            Some(1000),
        )
        .unwrap();
    let first = drain(&mut engine, NOW + 0.25);
    let done = reply(
        &mut engine,
        NOW + 999.75,
        &first.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(terminals(&done), [Ok("[]")]);
    assert_eq!(markers(&done), 0);
}

#[test]
fn a_physical_page_timeout_with_a_live_root_rejects_the_entire_query() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 200_000);
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    engine.handle(
        NOW + 201.,
        Event::TimerFired {
            timer: delay(&accepted, NOW + 201.),
        },
    );
    let second = drain(&mut engine, NOW + 201.);
    let timer = delay(&second, NOW + 20_201.);
    engine.handle(NOW + 20_201., Event::TimerFired { timer });
    strict_timeout(&drain(&mut engine, NOW + 20_201.));
}

#[test]
fn root_expiry_and_late_data_cannot_retire_a_reused_id_or_live_peer() {
    let mut engine = engine();
    let old = query(&mut engine, public(), 1000);
    engine
        .handle_command(
            NOW + 900.,
            Command::Query {
                command: 43,
                query: public(),
            },
            Some(5000),
        )
        .unwrap();
    let peer = drain(&mut engine, NOW + 900.);
    engine.prepare_dispatch(NOW + 1000.);
    strict_timeout(&drain(&mut engine, NOW + 1000.));
    engine
        .handle_command(
            NOW + 1001.,
            Command::Query {
                command: COMMAND,
                query: public(),
            },
            Some(5000),
        )
        .unwrap();
    let fresh = drain(&mut engine, NOW + 1001.);
    assert_eq!(fresh.requests.len(), 1);
    let late = reply(
        &mut engine,
        NOW + 1002.,
        &old.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert!(terminals(&late).is_empty());
    assert_eq!(markers(&late), 0);
    let peer_done = reply(
        &mut engine,
        NOW + 1003.,
        &peer.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(
        peer_done
            .actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 43, .. })))
            .count(),
        1
    );
    let done = reply(
        &mut engine,
        NOW + 1004.,
        &fresh.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(terminals(&done), [Ok("[]")]);
}

#[test]
fn a_route_preparation_refusal_after_accepted_rows_keeps_its_exact_error() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 1000);
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    engine.handle(
        NOW + 201.,
        Event::TimerFired {
            timer: delay(&accepted, NOW + 201.),
        },
    );
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("page 2 preparation");
    let cause = VenueError::new("route reservation refused")
        .kind(ErrorKind::Internal)
        .outcome(safe_cex_engine::Outcome::NotSent);
    engine.http_preparation_failed(
        NOW + 202.,
        preparation,
        safe_cex_engine::request_context::Error::Capacity,
        cause.clone(),
    );
    let failed = drain(&mut engine, NOW + 202.);
    assert_eq!(terminals(&failed), [Err(&cause)]);
    assert_eq!(markers(&failed), 0);
    assert!(failed.requests.is_empty());
}

struct GatedSigner {
    inner: safe_cex_engine::signing::Provider,
    refused: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl safe_cex_engine::signing::Signer for GatedSigner {
    fn sign(
        &self,
        request: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        if self.refused.load(std::sync::atomic::Ordering::SeqCst) {
            Box::pin(async { Err(safe_cex_engine::signing::SignError::Refused) })
        } else {
            self.inner.sign(request)
        }
    }
    fn immediate(&self) -> bool {
        self.inner.immediate()
    }
    fn sign_ready(
        &self,
        request: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        if self.refused.load(std::sync::atomic::Ordering::SeqCst) {
            Some(Err(safe_cex_engine::signing::SignError::Refused))
        } else {
            self.inner.sign_ready(request)
        }
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> safe_cex_engine::signing::Capability {
        self.inner.can_sign(key, algorithm)
    }
}

#[test]
fn provider_refusal_after_accepted_rows_is_typed_and_does_not_infer_auth() {
    let (venue, inner) = support::Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    }
    .build();
    let refused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine
        .set_signer(Some(std::sync::Arc::new(GatedSigner {
            inner,
            refused: refused.clone(),
        })))
        .unwrap();
    let first = query(&mut engine, fills(), 1000);
    assert_eq!((first.legacy, first.prepared, first.claimed), (0, 1, 1));
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    refused.store(true, std::sync::atomic::Ordering::SeqCst);
    engine.handle(
        NOW + 201.,
        Event::TimerFired {
            timer: delay(&accepted, NOW + 201.),
        },
    );
    let failed = drain(&mut engine, NOW + 201.);
    let cause = safe_cex_engine::signing::SignError::Refused.venue_error();
    assert_eq!(terminals(&failed), [Err(&cause)]);
    assert_eq!(markers(&failed), 0);
    assert!(failed.requests.is_empty());
    assert!(!failed.actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::AccountAuth { .. } | Notice::Deactivate)
    )));
    assert_eq!(engine.health().auth_failures, 0);
}

#[test]
fn a_delayed_bind_is_refused_at_root_expiry_without_a_physical_release() {
    let mut engine = engine();
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: COMMAND,
                query: fills(),
            },
            Some(1000),
        )
        .unwrap();
    let mut queue: VecDeque<_> = engine.drain_actions().collect();
    let permit = loop {
        let action = queue.pop_front().expect("actual managed bind");
        if let Action::ManagedHttp(action) = action {
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => engine.http_prepared(
                    NOW,
                    preparation,
                    Prepared {
                        id: request.id.0,
                        expires_at: None,
                    },
                ),
                HttpAction::Bind { permit, .. } => break permit,
                HttpAction::Cancel { .. } => panic!("live bind cannot cancel"),
            }
        }
        queue.extend(engine.drain_actions());
    };
    assert!(engine.claim_http(NOW + 1000., permit).is_err());
    let expired = drain(&mut engine, NOW + 1000.);
    strict_timeout(&expired);
    assert_eq!(expired.claimed, 0);
    assert_eq!(expired.legacy, 0);
}

#[test]
fn data_at_the_physical_deadline_cannot_cancel_a_live_roots_timeout() {
    let mut engine = engine();
    let first = query(&mut engine, public(), 200_000);
    let expired = reply(
        &mut engine,
        NOW + 20_000.,
        &first.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    strict_timeout(&expired);
}

#[test]
fn child_capacity_refusal_after_accepted_rows_is_strict_and_releases_its_root() {
    let mut engine = engine();
    let first = query(&mut engine, fills(), 10_000);
    let accepted = reply(&mut engine, NOW + 1., &first.requests[0], FILLS);
    let timer = delay(&accepted, NOW + 201.);
    let mut peers = Vec::new();
    for command in 1000..1255 {
        engine
            .handle_command(NOW + 2., Command::FetchAccount { command }, Some(10_000))
            .unwrap();
        let peer = drain(&mut engine, NOW + 2.);
        assert_eq!(
            peer.requests.len(),
            1,
            "all peers must be genuinely admitted"
        );
        assert!(
            peer.requests[0]
                .url
                .contains("/openApi/swap/v3/user/balance")
        );
        peers.push(peer.requests.into_iter().next().unwrap());
    }
    assert_eq!(peers.len(), 255);
    engine
        .handle_command(
            NOW + 3.,
            Command::FetchAccount { command: 2000 },
            Some(10_000),
        )
        .unwrap();
    let control = drain(&mut engine, NOW + 3.);
    let capacity: Vec<_> = control
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 2000,
                error,
            }) => Some(error.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(capacity.len(), 1);
    assert_eq!(capacity[0].kind, Some(ErrorKind::Internal));
    assert_eq!(capacity[0].outcome, None);
    engine.handle(NOW + 201., Event::TimerFired { timer });
    let refused = drain(&mut engine, NOW + 201.);
    assert_eq!(terminals(&refused), [Err(&capacity[0])]);
    assert_eq!(markers(&refused), 0);
    assert_eq!(
        (refused.legacy, refused.prepared, refused.claimed),
        (0, 0, 0)
    );
    assert!(refused.requests.is_empty());
    let peer_done = reply(
        &mut engine,
        NOW + 202.,
        &peers[0],
        r#"{"code":0,"data":[{"asset":"USDT","userId":"still-owned"}]}"#,
    );
    assert_eq!(
        peer_done
            .actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1000, .. })))
            .count(),
        1
    );
    engine
        .handle_command(
            NOW + 203.,
            Command::Query {
                command: COMMAND,
                query: fills(),
            },
            Some(10_000),
        )
        .unwrap();
    let fresh = drain(&mut engine, NOW + 203.);
    assert_eq!((fresh.legacy, fresh.prepared, fresh.claimed), (0, 1, 1));
    let done = reply(
        &mut engine,
        NOW + 204.,
        &fresh.requests[0],
        r#"{"code":0,"data":[]}"#,
    );
    assert_eq!(terminals(&done), [Ok("[]")]);
}
