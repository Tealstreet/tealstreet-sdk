mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_kraken::{Kraken, Options};
use support::{META, NOW, TICKERS, ready_with, reply, requests};

fn request(endpoint: &str) -> (Engine<Kraken>, HttpRequest) {
    let options = Options {
        public_data_only: true,
        ..Options::default()
    };
    let mut engine = safe_cex_engine::test_support::seeded_engine(Kraken::new(options.clone()));
    engine.start(NOW);
    let markets = requests(&mut engine).pop().unwrap();
    assert!(markets.url.ends_with("/instruments"));
    if endpoint == "instruments" {
        return (engine, markets);
    }
    if endpoint == "tickers" {
        reply(&mut engine, &markets, 200, META);
    } else {
        (engine, _) = ready_with(options);
        engine.handle(
            NOW,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "PF_XBTUSD".into(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: Some(2),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    let packet = requests(&mut engine).pop().unwrap();
    assert!(packet.url.contains(if endpoint == "tickers" {
        "/tickers"
    } else {
        "/api/charts/"
    }));
    (engine, packet)
}

fn refusal(status: u16, body: &str, reason: &str) {
    for endpoint in ["instruments", "tickers"] {
        let (mut engine, packet) = request(endpoint);
        let mut envelope: serde_json::Value = serde_json::from_str(body).unwrap();
        envelope["instruments"] = serde_json::json!([]);
        envelope["tickers"] = serde_json::json!([]);
        reply(&mut engine, &packet, status, &envelope.to_string());
        let messages: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::REQUEST_FAILED =>
                {
                    Some(event.message)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            messages.last(),
            Some(&format!(
                "Kraken public read failed: endpoint={endpoint} http_status={status} reason={reason}"
            ))
        );
        assert!(!engine.store().loaded().tickers);
        assert_eq!(engine.store().loaded().markets, endpoint == "tickers");
    }
}

#[test]
fn non_success_http_status_is_named_without_the_body() {
    refusal(
        503,
        r#"{"result":"private-result","error":"private-detail","symbol":"private-symbol","amount":123456}"#,
        "http_status",
    );
}
#[test]
fn malformed_typed_envelope_is_named_without_parse_details() {
    refusal(
        200,
        r#"{"result":null,"error":"private-detail"}"#,
        "invalid_envelope",
    );
}
#[test]
fn missing_or_rejected_result_is_named_without_the_result_value() {
    refusal(
        200,
        r#"{"result":"private-result","error":"private-detail"}"#,
        "result_not_success",
    );
    refusal(200, r#"{}"#, "result_not_success");
}
#[test]
fn venue_error_is_named_without_its_value() {
    refusal(
        200,
        r#"{"result":"success","error":"private-detail"}"#,
        "error_present",
    );
}
#[test]
fn unexpected_envelope_status_is_named_without_its_value() {
    refusal(
        200,
        r#"{"result":"success","status":"private-status"}"#,
        "status_not_success",
    );
}
#[test]
fn chart_http_refusal_keeps_the_endpoint_closed() {
    let (mut engine, packet) = request("candles");
    reply(
        &mut engine,
        &packet,
        429,
        r#"{"candles":[],"error":"private-detail"}"#,
    );
    assert!(engine.drain_actions().any(|a| matches!(a, Action::Notice(Notice::Event(event))
        if event.message == "Kraken public read failed: endpoint=candles http_status=429 reason=http_status")));
}
#[test]
fn successful_envelopes_keep_existing_header_and_chart_semantics() {
    for endpoint in ["instruments", "tickers", "candles"] {
        for headers in [
            "content-type: application/json",
            "content-type: application/json, charset=utf-8",
        ] {
            let (mut engine, packet) = request(endpoint);
            let body = match endpoint {
                "instruments" => META,
                "tickers" => TICKERS,
                _ => {
                    r#"{"candles":[{"time":1700000000000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}]}"#
                }
            };
            engine.handle(
                NOW,
                Event::HttpResponse {
                    request: packet.id,
                    response: Response {
                        status: 200,
                        headers,
                        body,
                    },
                },
            );
            assert!(engine.store().loaded().markets);
            assert_eq!(engine.store().loaded().tickers, endpoint != "instruments");
            let actions: Vec<_> = engine.drain_actions().collect();
            assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::REQUEST_FAILED)));
            if endpoint == "candles" {
                assert!(
                    actions.iter().any(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandDone { command: 7, .. })
                    ))
                );
            }
        }
    }
}

#[test]
fn missing_public_list_does_not_log_an_error_envelope_body() {
    for (body, reason) in [
        (
            r#"{"result":"failure","error":"private-detail"}"#,
            "result_not_success",
        ),
        (
            r#"{"result":"success","symbol":"private-symbol"}"#,
            "missing_list",
        ),
    ] {
        let (mut engine, packet) = request("instruments");
        reply(&mut engine, &packet, 200, body);
        let messages: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::REQUEST_FAILED =>
                {
                    Some(event.message)
                }
                _ => None,
            })
            .collect();
        assert!(
            messages.iter().all(|m| !m.contains("private-")),
            "{messages:?}"
        );
        assert_eq!(
            messages.last(),
            Some(&format!(
                "Kraken public read failed: endpoint=instruments http_status=200 reason={reason}"
            ))
        );
        assert!(!engine.store().loaded().markets);
    }
}
#[test]
fn malformed_public_json_does_not_log_a_body_preview() {
    let (mut engine, packet) = request("instruments");
    reply(&mut engine, &packet, 200, "{private-body-preview");
    let messages: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::REQUEST_FAILED =>
            {
                Some(event.message)
            }
            _ => None,
        })
        .collect();
    assert!(
        messages.iter().all(|m| !m.contains("private-body-preview")),
        "{messages:?}"
    );
    assert_eq!(
        messages.last().map(String::as_str),
        Some(
            "Kraken public read failed: endpoint=instruments http_status=200 reason=malformed_json"
        )
    );
    assert!(!engine.store().loaded().markets);
}
