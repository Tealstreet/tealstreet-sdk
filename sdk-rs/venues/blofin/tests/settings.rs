mod support;
use safe_cex_engine::{Command, Event, HttpRequest, Method, Outcome};
use support::*;

fn settings() -> [(Command, &'static str, &'static str); 3] {
    [
        (
            Command::SetLeverage {
                command: 7,
                symbol: "BTCUSDT".into(),
                leverage: 30.0,
                side: None,
            },
            "leverage",
            "set-leverage",
        ),
        (
            Command::SetMarginType {
                command: 7,
                symbol: "BTCUSDT".into(),
                isolated: true,
            },
            "margin mode",
            "set-margin-mode",
        ),
        (
            Command::SetPositionMode {
                command: 7,
                symbol: None,
                hedged: true,
            },
            "position mode",
            "set-position-mode",
        ),
    ]
}

fn issue(h: &mut Harness, command: Command, path: &str) -> HttpRequest {
    h.handle(1.0, Event::Command(command));
    h.collect();
    h.take(path)
}

fn assert_unknown_setting(command: Command, path: &str, status: u16, response: &str) {
    let mut h = Harness::new();
    h.seed();
    let before = h.e.store().setting("BTCUSDT").unwrap();
    let request = issue(&mut h, command, path);
    assert_eq!(request.method, Method::Post);
    h.respond_status(request, status, response);
    let [error] = h.failures.as_slice() else {
        panic!("{path}: expected one terminal failure: {:?}", h.failures);
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{path}: {error:?}");
    let text = error.legacy_text();
    assert!(text.contains("unknown"), "{path}: {text}");
    assert!(text.contains("Check Open Orders"), "{path}: {text}");
    assert!(!text.contains("rejected"), "{path}: {text}");
    let after = h.e.store().setting("BTCUSDT").unwrap();
    assert_eq!(after.leverage, before.leverage);
    assert_eq!(after.buy_leverage, before.buy_leverage);
    assert_eq!(after.sell_leverage, before.sell_leverage);
    assert_eq!(after.margin_type, before.margin_type);
    assert!(!h.e.store().hedged());
    assert!(!h.pending.iter().any(|r| r.method == Method::Post));
}

#[test]
fn blank_code_leverage_response_requires_uncertainty_guidance() {
    let (command, _, path) = settings().into_iter().next().unwrap();
    assert_unknown_setting(
        command,
        path,
        200,
        r#"{"code":"","msg":"internal failure","data":[]}"#,
    );
}

#[test]
fn blank_code_margin_mode_response_requires_uncertainty_guidance() {
    let (command, _, path) = settings().into_iter().nth(1).unwrap();
    assert_unknown_setting(
        command,
        path,
        200,
        r#"{"code":"","msg":"internal failure","data":[]}"#,
    );
}

#[test]
fn blank_code_position_mode_response_requires_uncertainty_guidance() {
    let (command, _, path) = settings().into_iter().nth(2).unwrap();
    assert_unknown_setting(
        command,
        path,
        200,
        r#"{"code":"","msg":"internal failure","data":[]}"#,
    );
}

#[test]
fn missing_and_unproven_native_setting_codes_keep_uncertainty_guidance() {
    for response in [
        r#"{"msg":"internal failure","data":[]}"#,
        r#"{"code":"103013","msg":"internal failure","data":[]}"#,
        r#"{"code":"999999","msg":"internal failure","data":[]}"#,
        r#"{"code":"0","data":[{"code":"103013","msg":"internal failure"}]}"#,
    ] {
        for (command, _, path) in settings() {
            assert_unknown_setting(command, path, 200, response);
        }
    }
}

#[test]
fn non2xx_setting_failures_without_refusal_proof_keep_uncertainty_guidance() {
    for response in [
        r#"{"code":"103013","msg":"internal failure","data":[]}"#,
        r#"{"code":"","msg":"internal failure","data":[]}"#,
        r#"{"code":"999999","msg":"internal failure","data":[]}"#,
        r#"{"msg":"internal failure","data":[]}"#,
        "Bad Request",
    ] {
        for (command, _, path) in settings() {
            assert_unknown_setting(command, path, 400, response);
        }
    }
}

#[test]
fn native_setting_refusals_name_the_setting_and_retain_the_actual_code_and_message() {
    for nested in [false, true] {
        for (command, kind, path) in settings() {
            let mut h = Harness::new();
            h.seed();
            let request = issue(&mut h, command, path);
            let body = if nested {
                r#"{"code":"0","msg":"success","data":[{"code":"110019","msg":"Cancel any open orders and close positions first"}]}"#
            } else {
                r#"{"code":"400","msg":"Bad Request"}"#
            };
            h.respond(request, body);
            let [error] = h.failures.as_slice() else {
                panic!("{:?}", h.failures)
            };
            let (code, message) = if nested {
                ("110019", "Cancel any open orders and close positions first")
            } else {
                ("400", "Bad Request")
            };
            assert_eq!(error.venue_code.as_deref(), Some(code), "{error:?}");
            assert_eq!(error.venue_message.as_deref(), Some(message), "{error:?}");
            assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
            assert!(!error.legacy_text().contains("unknown"), "{error:?}");
            assert!(
                error
                    .message
                    .contains(&format!("BloFin {kind} setting rejected")),
                "{error:?}"
            );
            assert!(error.message.contains("HTTP 200"), "{error:?}");
            assert!(error.message.contains(message), "{error:?}");
            assert!(!h.pending.iter().any(|r| r.method == Method::Post));
            assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
            assert!(!h.e.store().hedged());
        }
    }
}

#[test]
fn http_setting_failures_distinguish_transport_status_from_vendor_evidence() {
    for (body, expected) in [
        (
            r#"{"code":"152001","msg":"Invalid parameter"}"#,
            Some(("152001", "Invalid parameter")),
        ),
        (
            r#"{"code":"110019","msg":"Close positions first"}"#,
            Some(("110019", "Close positions first")),
        ),
        ("Bad Request", None),
    ] {
        for (command, kind, path) in settings() {
            let mut h = Harness::new();
            h.seed();
            let request = issue(&mut h, command, path);
            h.respond_status(request, 400, body);
            let [error] = h.failures.as_slice() else {
                panic!("{:?}", h.failures)
            };
            assert_eq!(
                error.venue_code.as_deref(),
                expected.map(|(code, _)| code),
                "{error:?}"
            );
            assert_eq!(
                error.venue_message.as_deref(),
                expected.map(|(_, message)| message),
                "{error:?}"
            );
            if expected.is_some() {
                assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
                assert!(
                    error
                        .message
                        .contains(&format!("BloFin {kind} setting rejected")),
                    "{error:?}"
                );
                assert!(error.message.contains("HTTP 400"), "{error:?}");
            } else {
                assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
                assert!(error.legacy_text().contains("unknown"), "{error:?}");
                assert!(
                    error.legacy_text().contains("Check Open Orders"),
                    "{error:?}"
                );
                assert!(matches!(
                    &error.elements.as_ref().unwrap()[0].result,
                    safe_cex_engine::write_plan::ElementResult::Unknown { error }
                        if error.message == "The outcome is unknown. Check Open Orders"
                ));
            }
            assert!(!h.pending.iter().any(|r| r.method == Method::Post));
        }
    }
}

#[test]
fn setting_payloads_match_the_native_instrument_and_documented_scope() {
    for (command, _, path) in settings() {
        let mut h = Harness::new();
        h.seed();
        let request = issue(&mut h, command, path);
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let expected = match path {
            "set-leverage" => {
                serde_json::json!({"instId":"BTC-USDT","leverage":"30","marginMode":"cross"})
            }
            "set-margin-mode" => serde_json::json!({"marginMode":"isolated"}),
            _ => serde_json::json!({"positionMode":"long_short_mode"}),
        };
        assert_eq!(body, expected);
    }
}

#[test]
fn leverage_scope_and_catalog_bounds_match_all_documented_modes() {
    for isolated in [false, true] {
        for hedged in [false, true] {
            for (requested, expected) in [(30.0, "30"), (1000.0, "150"), (0.5, "1")] {
                let mut h = Harness::new();
                let margin_mode = if isolated { "isolated" } else { "cross" };
                let position_mode = if hedged {
                    "long_short_mode"
                } else {
                    "net_mode"
                };
                h.reply(
                    "margin-mode",
                    &format!(r#"{{"code":"0","data":{{"marginMode":"{margin_mode}"}}}}"#),
                );
                h.reply(
                    "position-mode",
                    &format!(r#"{{"code":"0","data":{{"positionMode":"{position_mode}"}}}}"#),
                );
                let sides = if isolated && hedged {
                    vec![Some("long"), Some("short")]
                } else {
                    vec![None]
                };
                h.handle(
                    1.0,
                    Event::Command(Command::SetLeverage {
                        command: 7,
                        symbol: "BTCUSDT".into(),
                        leverage: requested,
                        side: None,
                    }),
                );
                h.collect();
                for side in sides {
                    let request = h.take("set-leverage");
                    let body: serde_json::Value =
                        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                    assert_eq!(body["instId"], "BTC-USDT");
                    assert_eq!(body["marginMode"], margin_mode);
                    assert_eq!(body["leverage"], expected);
                    assert_eq!(
                        body.get("positionSide").and_then(serde_json::Value::as_str),
                        side
                    );
                    h.respond(request, r#"{"code":"0","msg":"success"}"#);
                }
                assert!(!h.pending.iter().any(|r| r.url.contains("set-leverage")));
                assert!(h.failures.is_empty(), "{:?}", h.failures);
            }
        }
    }
}
