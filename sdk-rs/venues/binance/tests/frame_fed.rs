//! A frame-fed Binance USD-M engine (the browser's main-thread engine for one
//! account): public instrument metadata only, no sockets, no account reads or
//! polls. Fed a LOCAL engine's snapshot, each write sends exactly the requests
//! LOCAL sends, cancel-then-replace amends included, and never a read.
mod support;
use safe_cex_engine::model::{MarginType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, HttpRequest, Method};
use support::{Harness, Options};

const OPEN_ORDERS: &str = r#"[{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"100","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY","status":"PARTIALLY_FILLED","timeInForce":"GTC"}]"#;
const STOP_LIMIT: &str = r#"[{"algoId":43,"clientAlgoId":"stoplimit","algoType":"CONDITIONAL","orderType":"STOP","symbol":"BTCUSDT","side":"SELL","quantity":"7","price":"89","algoStatus":"NEW","triggerPrice":"90","workingType":"MARK_PRICE"}]"#;
const POSITION: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"7","entryPrice":"100","markPrice":"100","unRealizedProfit":"0","leverage":"20","marginType":"cross","notional":"700","liquidationPrice":"50"}]"#;
const SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":"false","leverage":20,"maxNotionalValue":"1000000"}]"#;
const BRACKETS: &str = r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":50,"notionalCap":5000,"notionalFloor":0,"maintMarginRatio":0.01,"cum":0}]}]"#;

fn keyed() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

/// The data source: a LOCAL USD-M account with one open limit order, one
/// stop-limit and one position.
fn local() -> Harness {
    local_with(POSITION, "[]")
}

/// A flat LOCAL account whose symbol config sets BTCUSDT at 20x cross.
fn flat() -> Harness {
    local_with("[]", SYMBOL_CONFIG)
}

fn local_with(positions: &str, symbol_config: &str) -> Harness {
    let mut h = Harness::started(keyed());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, BRACKETS);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"private-key"}"#);
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, positions);
    h.respond("/fapi/v1/symbolConfig", 200, symbol_config);
    h.respond("/fapi/v3/account", 200, r#"{"assets":[]}"#);
    h.respond("/fapi/v1/openOrders", 200, OPEN_ORDERS);
    h.respond("/fapi/v1/openAlgoOrders", 200, STOP_LIMIT);
    let private = h.socket("/private/ws").unwrap();
    h.event(Event::WsOpened { socket: private });
    h.answer_open_position_read();
    h.requests.clear();
    h
}

/// A frame-fed engine: one public instrument read, no socket, nothing after.
fn frame_fed() -> Harness {
    let mut engine = support::engine(keyed());
    engine.set_frame_fed().unwrap();
    let mut h = Harness::from_engine(engine);
    assert_eq!(h.paths().len(), 1, "{:?}", h.paths());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    assert!(
        h.requests.is_empty(),
        "nothing after metadata: {:?}",
        h.paths()
    );
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no polls: {:?}", h.paths());
    assert!(h.sockets.is_empty(), "no socket: {:?}", h.sockets);
    assert!(h.engine.store().loaded().markets);
    h
}

/// `fed` after ingesting `source`'s snapshot frame.
fn fed_from(source: &mut Harness) -> Harness {
    let mut fed = frame_fed();
    fed.now = source.now;
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    fed.engine.apply_frame(fed.now, &frame, true).unwrap();
    fed.collect();
    assert!(
        fed.requests.is_empty(),
        "ingest reads nothing: {:?}",
        fed.paths()
    );
    fed
}

/// Method, path and query without the per-request signature, clock and
/// generated client ids, then the body.
fn shape(request: &HttpRequest) -> (Method, String, Vec<String>, String) {
    let (path, query) = request.url.split_once('?').unwrap_or((&request.url, ""));
    let mut params: Vec<String> = query
        .split('&')
        .chain(request.body.as_deref().unwrap_or("").split('&'))
        .filter(|pair| {
            !pair.is_empty()
                && ![
                    "signature=",
                    "timestamp=",
                    "newClientOrderId=",
                    "clientAlgoId=",
                ]
                .iter()
                .any(|key| pair.starts_with(key))
        })
        .map(str::to_owned)
        .collect();
    params.sort();
    let path = path.rsplit_once(".com").map_or(path, |(_, path)| path);
    (request.method, path.to_owned(), params, String::new())
}

fn shapes(h: &Harness) -> Vec<(Method, String, Vec<String>, String)> {
    h.requests.iter().map(shape).collect()
}

/// Run `command` on both engines and answer each step identically; the
/// frame-fed engine must send what LOCAL sends, and read nothing.
fn same_writes(local: &mut Harness, fed: &mut Harness, command: Command, replies: &[&str]) {
    local.event(Event::Command(command.clone()));
    fed.event(Event::Command(command));
    for body in replies {
        assert_eq!(shapes(fed), shapes(local), "fed {:?}", fed.paths());
        assert_eq!(
            fed.requests.len(),
            1,
            "one request per step: {:?}",
            fed.paths()
        );
        assert_ne!(fed.requests[0].method, Method::Get, "never a read");
        let (l, f) = (local.requests.remove(0), fed.requests.remove(0));
        local.reply(l.id, 200, "", body);
        fed.reply(f.id, 200, "", body);
    }
    assert!(
        fed.requests.is_empty(),
        "no read after the reply: {:?}",
        fed.paths()
    );
}

fn limit() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":99}"#,
    )
    .unwrap()
}

const PLACED: &str = r#"{"symbol":"BTCUSDT","orderId":77,"clientOrderId":"x","origQty":"0.01","executedQty":"0","price":"99","type":"LIMIT","side":"BUY","status":"NEW"}"#;

fn canceled(filled: u32) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","origQty":"7","executedQty":"{filled}","status":"CANCELED"}}"#
    )
}

#[test]
fn a_frame_fed_engine_loads_public_metadata_once_and_opens_no_socket() {
    frame_fed();
}

#[test]
fn ingested_source_state_reaches_the_store_with_its_leverage_brackets() {
    let mut source = local();
    let fed = fed_from(&mut source);
    let store = fed.engine.store();
    assert_eq!(store.orders().len(), source.engine.store().orders().len());
    assert_eq!(store.positions().len(), 1);
    assert!(store.loaded().orders && store.loaded().positions);
    assert_eq!(store.market("BTCUSDT").unwrap().limits.leverage.max, 50.);
}

#[test]
fn a_placement_is_the_local_request_and_nothing_follows() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::PlaceOrders {
            command: 5,
            orders: vec![limit()],
        },
        &[PLACED],
    );
    assert!(matches!(fed.result(5), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_cancel_is_the_local_request_and_nothing_follows() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::CancelOrders {
            command: 6,
            ids: vec!["12".into()],
        },
        &[&canceled(2)],
    );
    assert!(matches!(fed.result(6), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn an_amend_cancels_then_replaces_exactly_as_local() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(101.0),
            amount: None,
        },
        &[&canceled(2), PLACED],
    );
    assert!(matches!(fed.result(7), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_leverage_set_clamps_to_the_ingested_bracket_as_local() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    same_writes(
        &mut source,
        &mut fed,
        Command::SetLeverage {
            command: 8,
            symbol: "BTCUSDT".into(),
            leverage: 75.0,
            side: None,
        },
        &[r#"{"symbol":"BTCUSDT","leverage":50,"maxNotionalValue":"5000"}"#],
    );
    assert!(matches!(fed.result(8), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_stop_limit_amend_is_refused_from_the_ingested_venue_state() {
    let mut source = local();
    let stop = source
        .engine
        .store()
        .orders()
        .iter()
        .find(|order| order.trigger_type.is_some() || order.price == 89.)
        .map(|order| order.id.clone())
        .expect("the stop-limit is stored");
    let mut fed = fed_from(&mut source);
    for h in [&mut source, &mut fed] {
        h.event(Event::Command(Command::AmendOrder {
            command: 9,
            id: stop.clone(),
            price: Some(88.0),
            amount: None,
        }));
        assert!(h.requests.is_empty(), "{:?}", h.paths());
        let failed = h.result(9).cloned();
        assert!(
            matches!(&failed, Some(Err(e)) if e.contains("stop-limit")),
            "{failed:?}"
        );
    }
}

#[test]
fn an_unknown_placement_outcome_is_never_re_sent_or_followed_by_a_read() {
    let mut source = local();
    let mut fed = fed_from(&mut source);
    fed.event(Event::Command(Command::PlaceOrders {
        command: 10,
        orders: vec![limit()],
    }));
    let placed = fed.take("/fapi/v1/order");
    assert_eq!(placed.method, Method::Post);
    fed.reply(placed.id, 502, "", "<html>bad gateway</html>");
    fed.advance(120_000.0);
    assert!(
        fed.requests.is_empty(),
        "no re-send, no read: {:?}",
        fed.paths()
    );
}

#[test]
fn engine_unknown_hook_emits_once_without_followup_writes() {
    use safe_cex_engine::{Outcome, Value};
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [408, 429, 503] {
                let mut source = local();
                let mut h = if frame_fed {
                    fed_from(&mut source)
                } else {
                    source
                };
                h.event(Event::Command(Command::PlaceOrders {
                    command: 811,
                    orders: vec![limit()],
                }));
                let request = h.take("/fapi/v1/order");
                assert!(request.url.contains("signature="));
                let body = "https://leak.test/order?signature=signed-payload&apiKey=never-log";
                if streamed {
                    h.event(Event::HttpStart {
                        request: request.id,
                        status,
                        headers: "",
                    });
                    h.event(Event::HttpChunk {
                        request: request.id,
                        data: body.as_bytes(),
                    });
                    h.event(Event::HttpEnd {
                        request: request.id,
                    });
                } else {
                    h.reply(request.id, status, "", body);
                }
                h.reply(request.id, status, "", body);
                let error = &h.failures.iter().find(|(id, _)| *id == 811).unwrap().1;
                assert_eq!(error.outcome, Some(Outcome::Unknown));
                let events: Vec<_> = h
                    .events
                    .iter()
                    .filter(|event| event.code == "order.outcome_unknown")
                    .collect();
                assert_eq!(
                    events.len(),
                    1,
                    "frame_fed={frame_fed} streamed={streamed} status={status}"
                );
                let event = events[0];
                assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
                assert!(
                    event
                        .fields
                        .contains(&("command", Value::from("binance-811")))
                );
                assert!(
                    event
                        .fields
                        .contains(&("request", Value::from(request.id.0.to_string())))
                );
                assert!(event.fields.contains(&("status", Value::from(status))));
                assert_safe_unknown_fields(event);
                assert!(h.requests.iter().all(|r| r.method == Method::Get));
                if frame_fed {
                    assert!(h.requests.is_empty(), "no frame-fed reads");
                }
            }
        }
    }
}

fn assert_safe_unknown_fields(event: &safe_cex_engine::LogEvent) {
    let request = event
        .fields
        .iter()
        .find(|(key, _)| *key == "request")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    // Exactly ^[0-9]{1,20}$, without permitting whitespace or Unicode digits.
    assert!((1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()));
    for (key, value) in &event.fields {
        assert!(
            [
                "command",
                "request",
                "status",
                "outcome",
                "placedOrderIds",
                "unknownIndices"
            ]
            .contains(key)
        );
        let text = format!("{key} {value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "/order",
            "?",
            "&",
            "=",
            "signature",
            "apikey",
            "signed-payload",
            "never-log",
            "endpoint",
            "query",
            "body",
        ] {
            assert!(
                !text.contains(forbidden),
                "{key} leaked {forbidden}: {value:?}"
            );
        }
    }
}

fn assert_native_unknown_event(h: &Harness, request: safe_cex_engine::RequestId, status: u16) {
    let events: Vec<_> = h
        .events
        .iter()
        .filter(|event| event.code == "order.outcome_unknown")
        .collect();
    assert_eq!(events.len(), 1, "{:?}", h.events);
    let event = events[0];
    assert_safe_unknown_fields(event);
    let mut names: Vec<_> = event.fields.iter().map(|(key, _)| *key).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "command",
            "outcome",
            "placedOrderIds",
            "request",
            "status",
            "unknownIndices"
        ]
    );
    assert!(
        event
            .fields
            .contains(&("request", request.0.to_string().into()))
    );
    assert!(event.fields.contains(&("status", status.into())));
    assert!(event.fields.contains(&("outcome", "unknown".into())));
}

#[test]
fn native_unknown_hook_emits_once_with_closed_fields_in_both_feed_modes() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for code in [-1006, -1007, 0, -2019] {
                let mut source = local();
                let mut h = if frame_fed {
                    fed_from(&mut source)
                } else {
                    source
                };
                h.event(Event::Command(Command::PlaceOrders {
                    command: 813,
                    orders: vec![limit()],
                }));
                let request = h.take("/fapi/v1/order");
                let body = if code == 0 {
                    "{}".to_owned()
                } else {
                    format!(
                        r#"{{"code":{code},"msg":"https://never-log.test/order?signature=signed-payload&apiKey=never-log"}}"#
                    )
                };
                let status = if code == -2019 { 400 } else { 200 };
                if streamed {
                    h.event(Event::HttpStart {
                        request: request.id,
                        status,
                        headers: "",
                    });
                    h.event(Event::HttpChunk {
                        request: request.id,
                        data: body.as_bytes(),
                    });
                    h.event(Event::HttpEnd {
                        request: request.id,
                    });
                } else {
                    h.reply(request.id, status, "", &body);
                }
                let error = &h.failures.iter().find(|(id, _)| *id == 813).unwrap().1;
                assert_eq!(
                    error.outcome,
                    Some(if code == -2019 {
                        safe_cex_engine::Outcome::Rejected
                    } else {
                        safe_cex_engine::Outcome::Unknown
                    })
                );
                if code == -2019 {
                    assert!(!h.events.iter().any(|e| e.code == "order.outcome_unknown"));
                } else {
                    assert_native_unknown_event(&h, request.id, status);
                }
                h.reply(request.id, status, "", &body);
                assert_eq!(
                    h.events
                        .iter()
                        .filter(|e| e.code == "order.outcome_unknown")
                        .count(),
                    usize::from(code != -2019)
                );
                assert!(h.requests.iter().all(|r| r.method == Method::Get));
                if frame_fed {
                    assert!(h.requests.is_empty());
                }
            }
        }
    }
}

#[test]
fn native_unknown_hook_waits_for_siblings_and_deduplicates_engine_failures() {
    for frame_fed in [false, true] {
        for second in ["native", "accepted", "http", "transport"] {
            let mut source = local();
            let mut h = if frame_fed {
                fed_from(&mut source)
            } else {
                source
            };
            h.event(Event::Command(Command::CancelSymbolOrders {
                command: 814,
                symbol: "BTCUSDT".into(),
            }));
            let first = h.take("/fapi/v1/allOpenOrders");
            let sibling = h.take("/fapi/v1/algoOpenOrders");
            assert_eq!(first.method, Method::Delete);
            assert_eq!(sibling.method, Method::Delete);
            h.reply(
                first.id,
                200,
                "",
                r#"{"code":-1007,"msg":"synthetic uncertainty"}"#,
            );
            assert!(!h.events.iter().any(|e| e.code == "order.outcome_unknown"));
            assert!(!h.failures.iter().any(|(id, _)| *id == 814));
            match second {
                "transport" => h.event(Event::HttpFailed {
                    request: sibling.id,
                    error: "synthetic transport loss",
                }),
                "http" => h.reply(sibling.id, 503, "", "{}"),
                "accepted" => h.reply(sibling.id, 200, "", r#"{"code":200,"msg":"success"}"#),
                _ => h.reply(
                    sibling.id,
                    200,
                    "",
                    r#"{"code":-1006,"msg":"synthetic uncertainty"}"#,
                ),
            }
            assert_eq!(
                h.failures
                    .iter()
                    .filter(|(id, error)| *id == 814
                        && error.outcome == Some(safe_cex_engine::Outcome::Unknown))
                    .count(),
                1
            );
            let events: Vec<_> = h
                .events
                .iter()
                .filter(|e| e.code == "order.outcome_unknown")
                .collect();
            assert_eq!(events.len(), 1, "mode={frame_fed}/{second}: {:?}", h.events);
            assert_safe_unknown_fields(events[0]);
            assert!(events[0].fields.contains(&("placedOrderIds", "".into())));
            if second == "native" || second == "accepted" {
                assert_native_unknown_event(&h, first.id, 200);
            } else {
                assert!(
                    events[0]
                        .fields
                        .contains(&("request", sibling.id.0.to_string().into()))
                );
                if second == "http" {
                    assert_native_unknown_event(&h, sibling.id, 503);
                } else {
                    assert!(!events[0].fields.iter().any(|(key, _)| *key == "status"));
                }
            }
            h.reply(first.id, 200, "", r#"{"code":-1007,"msg":"late"}"#);
            h.reply(sibling.id, 503, "", "{}");
            assert_eq!(
                h.events
                    .iter()
                    .filter(|e| e.code == "order.outcome_unknown")
                    .count(),
                1
            );
            assert!(h.requests.iter().all(|r| r.method == Method::Get));
            if frame_fed {
                assert!(h.requests.is_empty());
            }
        }
    }
}

#[test]
fn native_unknown_hook_preserves_only_confirmed_placement_ids() {
    for frame_fed in [false, true] {
        let mut source = local();
        let mut h = if frame_fed {
            fed_from(&mut source)
        } else {
            source
        };
        h.event(Event::Command(Command::PlaceOrders {
            command: 815,
            orders: vec![limit(), limit()],
        }));
        let request = h.take("/fapi/v1/batchOrders");
        h.reply(request.id, 200, "", r#"[{"orderId":123},{"code":-1007,"msg":"https://never-log.test/order?signature=signed-payload"}]"#);
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 815)
                .unwrap()
                .1
                .outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert_native_unknown_event(&h, request.id, 200);
        let event = h
            .events
            .iter()
            .find(|e| e.code == "order.outcome_unknown")
            .unwrap();
        assert!(event.fields.contains(&("placedOrderIds", "123".into())));
        assert!(event.fields.contains(&("unknownIndices", "".into())));
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
        if frame_fed {
            assert!(h.requests.is_empty());
        }
    }
}

#[test]
fn engine_transport_unknown_emits_once_and_definite_refusal_emits_none() {
    use safe_cex_engine::Outcome;
    for frame_fed in [false, true] {
        for transport in [false, true] {
            let mut source = local();
            let mut h = if frame_fed {
                fed_from(&mut source)
            } else {
                source
            };
            h.event(Event::Command(Command::PlaceOrders {
                command: 812,
                orders: vec![limit()],
            }));
            let request = h.take("/fapi/v1/order");
            if transport {
                h.event(Event::HttpFailed {
                    request: request.id,
                    error: "https://leak.test/order?signature=signed-payload&apiKey=never-log",
                });
            } else {
                h.reply(
                    request.id,
                    400,
                    "",
                    r#"{"code":-2019,"msg":"Margin is insufficient"}"#,
                );
            }
            let error = &h.failures.iter().find(|(id, _)| *id == 812).unwrap().1;
            assert_eq!(
                error.outcome,
                Some(if transport {
                    Outcome::Unknown
                } else {
                    Outcome::Rejected
                })
            );
            let events: Vec<_> = h
                .events
                .iter()
                .filter(|event| event.code == "order.outcome_unknown")
                .collect();
            assert_eq!(events.len(), usize::from(transport));
            for event in events {
                assert_safe_unknown_fields(event);
            }
            assert!(h.requests.iter().all(|r| r.method == Method::Get));
            if frame_fed {
                assert!(h.requests.is_empty());
            }
        }
    }
}

#[test]
fn engine_unknown_projection_discards_untrusted_error_details() {
    use safe_cex_engine::{EngineUnknownOutcome, RequestId, Venue, VenueError};
    let tainted = "https://leak.test/order?signature=signed-payload&apiKey=never-log";
    let error = VenueError {
        message: tainted.into(),
        venue_message: Some(tainted.into()),
        venue_code: Some(tainted.into()),
        partial: Some(Box::new(safe_cex_engine::error::Partial {
            order_ids: vec![tainted.into()],
            total: None,
            failed_index: None,
        })),
        ..VenueError::default()
    };
    let hook =
        <safe_cex_venue_binance::BinanceFutures as Venue>::unknown_outcome_telemetry().unwrap();
    for id in [0, u64::MAX] {
        let event = hook(&EngineUnknownOutcome {
            now: 123.,
            command: 811,
            request: RequestId(id),
            status: None,
            error: &error,
        });
        assert_safe_unknown_fields(&event);
        assert!(!event.fields.iter().any(|(key, _)| *key == "status"));
    }
}

/// The source's current snapshot, ingested by `fed`: stale until the source re-reads.
fn ingest(source: &mut Harness, fed: &mut Harness) {
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    fed.engine.apply_frame(fed.now, &frame, true).unwrap();
    fed.collect();
    assert!(
        fed.requests.is_empty(),
        "ingest reads nothing: {:?}",
        fed.paths()
    );
}

/// Run a settings write on `fed` alone and confirm it: one request, nothing after.
fn confirm_write(fed: &mut Harness, command: Command, body: &str) {
    let id = match &command {
        Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. } => *command,
        _ => unreachable!("a settings write"),
    };
    fed.event(Event::Command(command));
    assert_eq!(fed.requests.len(), 1, "{:?}", fed.paths());
    assert_eq!(fed.requests[0].method, Method::Post);
    let request = fed.requests.remove(0);
    fed.reply(request.id, 200, "", body);
    assert!(matches!(fed.result(id), Some(Ok(_))), "{:?}", fed.done);
    assert!(
        fed.requests.is_empty(),
        "nothing follows: {:?}",
        fed.paths()
    );
}

fn hedge(command: u32) -> Command {
    Command::SetPositionMode {
        command,
        hedged: true,
        symbol: None,
    }
}

/// The `positionSide` of the next limit order `h` places, in its one request.
fn placed_side(h: &mut Harness, command: u32, path: &str) -> String {
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![limit()],
    }));
    assert_eq!(h.requests.len(), 1, "{:?}", h.paths());
    let request = h.take(path);
    let (_, _, params, _) = shape(&request);
    params
        .iter()
        .find_map(|pair| pair.strip_prefix("positionSide="))
        .expect("positionSide is sent")
        .to_owned()
}

/// The source re-reads its position mode and reports `hedged`.
fn source_reads_mode(source: &mut Harness, command: u32, hedged: bool) {
    source.event(Event::Command(Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::RefreshSettings { symbol: None },
    }));
    source.respond(
        "/fapi/v1/positionSide/dual",
        200,
        &format!(r#"{{"dualSidePosition":{hedged}}}"#),
    );
    assert_eq!(source.engine.store().hedged(), hedged);
}

const MODE_SET: &str = r#"{"code":200,"msg":"success"}"#;

#[test]
fn settings_writes_are_the_local_request_and_nothing_follows() {
    let mut source = flat();
    let mut fed = fed_from(&mut source);
    same_writes(&mut source, &mut fed, hedge(20), &[MODE_SET]);
    assert!(matches!(fed.result(20), Some(Ok(_))), "{:?}", fed.done);
    same_writes(
        &mut source,
        &mut fed,
        Command::SetMarginType {
            command: 21,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
        &[MODE_SET],
    );
    assert!(matches!(fed.result(21), Some(Ok(_))), "{:?}", fed.done);
    same_writes(
        &mut source,
        &mut fed,
        Command::SetLeverage {
            command: 22,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
            side: None,
        },
        &[r#"{"symbol":"BTCUSDT","leverage":10,"maxNotionalValue":"5000"}"#],
    );
    assert!(matches!(fed.result(22), Some(Ok(_))), "{:?}", fed.done);
}

#[test]
fn a_confirmed_hedge_outlasts_a_stale_one_way_frame() {
    let mut source = flat();
    let mut fed = fed_from(&mut source);
    assert!(!fed.engine.store().hedged());
    confirm_write(&mut fed, hedge(23), MODE_SET);
    ingest(&mut source, &mut fed);
    assert!(
        fed.engine.store().hedged(),
        "the stale frame kept the hedge"
    );
    assert_eq!(placed_side(&mut fed, 24, "/fapi/v1/order"), "LONG");
}

#[test]
fn a_hedged_source_frame_releases_the_hold() {
    let mut source = flat();
    let mut fed = fed_from(&mut source);
    confirm_write(&mut fed, hedge(25), MODE_SET);
    source_reads_mode(&mut source, 26, true);
    ingest(&mut source, &mut fed);
    assert!(fed.engine.store().hedged());
    // Released: the source's later one-way mode applies again.
    source_reads_mode(&mut source, 27, false);
    ingest(&mut source, &mut fed);
    assert!(!fed.engine.store().hedged());
    assert_eq!(placed_side(&mut fed, 28, "/fapi/v1/order"), "BOTH");
}

#[test]
fn confirmed_leverage_and_margin_type_outlast_a_stale_frame() {
    let mut source = flat();
    let mut fed = fed_from(&mut source);
    assert_eq!(fed.engine.store().setting("BTCUSDT").unwrap().leverage, 20.);
    let leverage = |command| Command::SetLeverage {
        command,
        symbol: "BTCUSDT".into(),
        leverage: 10.0,
        side: None,
    };
    confirm_write(
        &mut fed,
        leverage(29),
        r#"{"symbol":"BTCUSDT","leverage":10,"maxNotionalValue":"5000"}"#,
    );
    confirm_write(
        &mut fed,
        Command::SetMarginType {
            command: 30,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
        MODE_SET,
    );
    ingest(&mut source, &mut fed);
    let setting = fed.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.leverage, 10.);
    assert_eq!(setting.margin_type, Some(MarginType::Isolated));
    // The kept leverage is the one the next set compares against: no re-send.
    fed.event(Event::Command(leverage(31)));
    assert!(fed.requests.is_empty(), "{:?}", fed.paths());
    assert!(matches!(fed.result(31), Some(Ok(_))), "{:?}", fed.done);
}

fn read_paths(h: &Harness) -> Vec<String> {
    shapes(h).into_iter().map(|(_, path, _, _)| path).collect()
}

#[test]
fn refresh_settings_reads_the_mode_and_the_symbol_config_and_writes_nothing() {
    let mut source = flat();
    source.event(Event::Command(Command::Venue {
        command: 32,
        action: safe_cex_engine::VenueAction::RefreshSettings { symbol: None },
    }));
    assert!(
        matches!(source.result(32), Some(Ok(_))),
        "{:?}",
        source.done
    );
    assert_eq!(read_paths(&source), ["/fapi/v1/positionSide/dual"]);
    assert!(source.requests.iter().all(|r| r.method == Method::Get));
    source.requests.clear();
    source.event(Event::Command(Command::Venue {
        command: 33,
        action: safe_cex_engine::VenueAction::RefreshSettings {
            symbol: Some("BTCUSDT".into()),
        },
    }));
    assert_eq!(
        read_paths(&source),
        [
            "/fapi/v1/positionSide/dual",
            "/fapi/v3/positionRisk",
            "/fapi/v1/symbolConfig"
        ]
    );
    assert!(source.requests.iter().all(|r| r.method == Method::Get));
    source.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    source.respond("/fapi/v3/positionRisk", 200, "[]");
    source.respond(
        "/fapi/v1/symbolConfig",
        200,
        &SYMBOL_CONFIG.replace("\"leverage\":20", "\"leverage\":10"),
    );
    assert_eq!(
        source.engine.store().setting("BTCUSDT").unwrap().leverage,
        10.
    );
    assert!(source.requests.is_empty(), "{:?}", source.paths());
}

#[test]
fn refresh_settings_on_a_frame_fed_engine_reads_nothing() {
    let mut source = flat();
    let mut fed = fed_from(&mut source);
    fed.event(Event::Command(Command::Venue {
        command: 34,
        action: safe_cex_engine::VenueAction::RefreshSettings {
            symbol: Some("BTCUSDT".into()),
        },
    }));
    assert!(fed.requests.is_empty(), "{:?}", fed.paths());
}

/// A flat portfolio-margin data source, one-way.
fn portfolio_margin() -> Harness {
    let mut h = Harness::started(Options {
        portfolio_margin: true,
        ..keyed()
    });
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/papi/v1/um/leverageBracket", 200, BRACKETS);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"pmkey"}"#);
    h.respond(
        "/papi/v1/um/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/papi/v1/um/positionRisk", 200, "[]");
    h.respond("/papi/v1/um/symbolConfig", 200, "[]");
    h.respond("/papi/v1/balance", 200, "[]");
    h.respond("/papi/v1/um/openOrders", 200, "[]");
    h.respond("/papi/v1/um/algo/openAlgoOrders", 200, "[]");
    h.requests.clear();
    h
}

#[test]
fn portfolio_margin_holds_a_confirmed_hedge_and_leverage() {
    let mut source = portfolio_margin();
    let mut engine = support::engine(Options {
        portfolio_margin: true,
        ..keyed()
    });
    engine.set_frame_fed().unwrap();
    let mut fed = Harness::from_engine(engine);
    fed.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    assert!(fed.requests.is_empty(), "{:?}", fed.paths());
    fed.now = source.now;
    ingest(&mut source, &mut fed);
    confirm_write(&mut fed, hedge(35), MODE_SET);
    confirm_write(
        &mut fed,
        Command::SetLeverage {
            command: 36,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
            side: None,
        },
        r#"{"symbol":"BTCUSDT","leverage":10,"maxNotionalValue":"5000"}"#,
    );
    ingest(&mut source, &mut fed);
    assert!(fed.engine.store().hedged());
    assert_eq!(fed.engine.store().setting("BTCUSDT").unwrap().leverage, 10.);
    assert_eq!(placed_side(&mut fed, 37, "/papi/v1/um/order"), "LONG");
}
