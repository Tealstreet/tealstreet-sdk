//! Fresh flat-symbol leverage metadata; no orders or live account actions.
mod support;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_phemex::PhemexPerpetuals;
use serde_json::{Value, json};
use support::{Options, PreparedEngine as Engine};
const NOW: f64 = 1000.;
fn engine() -> Engine<PhemexPerpetuals> {
    support::prepared_engine(Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..Default::default()
    })
}
fn ask(e: &mut Engine<PhemexPerpetuals>, symbol: &str) -> HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::FetchLeverage {
            command: 42,
            symbol: symbol.into(),
        }),
    );
    let requests: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "one fresh read, even with no open positions"
    );
    assert_eq!(requests[0].method, Method::Get);
    requests.into_iter().next().unwrap()
}
fn reply(e: &mut Engine<PhemexPerpetuals>, request: &HttpRequest, body: Value) -> Vec<Action> {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body.to_string(),
            },
        },
    );
    e.drain_actions().collect()
}
fn done(actions: Vec<Action>) -> Value {
    let results: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 42,
                result,
            }) => Some(serde_json::from_str::<Value>(&result).unwrap()),
            Action::Notice(Notice::CommandFailed { error, .. }) => {
                panic!("unexpected failure: {error:?}")
            }
            Action::Http(_) => panic!("read must not start account reconciliation"),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    results.into_iter().next().unwrap()
}
#[test]
fn flat_signed_margin_and_hedged_legs_match_ts_context_without_mutating_account() {
    for (symbol, currency, mode, long, short, cross) in [
        ("ADAUSDT", "USDT", "OneWay", -10.0_f64, -10.0_f64, false),
        ("ADAUSDC", "USDC", "Hedged", 3., 7., false),
        ("ADAUSDT", "USDT", "Hedged", 5., 9., true),
    ] {
        let mut e = engine();
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = ask(&mut e, symbol);
        assert_eq!(
            r.url,
            format!(
                "https://api.phemex.com/g-accounts/accountPositions?currency={currency}&symbol={symbol}"
            )
        );
        let out = reply(
            &mut e,
            &r,
            json!({"code":0,"data":{"positions":[
                {"symbol":"OTHERUSDT","posSide":"Long","leverageRr":"99"},
                {"symbol":symbol,"posMode":mode,"posSide":"Short","size":"0","leverageRr":short.to_string(),"crossMargin":cross},
                {"symbol":symbol,"posMode":mode,"posSide":"Long","size":"0","leverageRr":long.to_string(),"crossMargin":cross}
            ]}}),
        );
        assert_eq!(
            done(out),
            json!({"buyLeverage":long.abs(),"sellLeverage":short.abs(),"marginType":if long < 0. || cross {"cross"} else {"isolated"},"positionMode":if mode == "Hedged" {"hedged"} else {"oneway"}})
        );
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn missing_leg_uses_first_matching_row_as_ts_does() {
    let mut e = engine();
    let r = ask(&mut e, "ADAUSDT");
    assert_eq!(
        done(reply(
            &mut e,
            &r,
            json!({"code":0,"data":{"positions":[
                {"symbol":"adausdt","posMode":"Hedged","posSide":"Short","leverageRr":"-7","size":"0"}
            ]}})
        )),
        json!({"buyLeverage":7.,"sellLeverage":7.,"marginType":"cross","positionMode":"hedged"})
    );
}
#[test]
fn absent_symbol_returns_null_for_sdk_stored_position_fallback() {
    for rows in [json!([]), json!([{"symbol":"OTHERUSDT","leverageRr":"3"}])] {
        let mut e = engine();
        let r = ask(&mut e, "ADAUSDT");
        assert_eq!(
            done(reply(
                &mut e,
                &r,
                json!({"code":0,"data":{"positions":rows}})
            )),
            Value::Null
        );
    }
}
#[test]
fn failed_read_settles_once_and_duplicate_delivery_is_inert() {
    let mut e = engine();
    let r = ask(&mut e, "ADAUSDT");
    let out = reply(&mut e, &r, json!({"code":10001,"msg":"fixture refusal"}));
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
            .count(),
        1
    );
    assert!(reply(&mut e, &r, json!({"code":0,"data":{"positions":[]}})).is_empty());
}

#[test]
fn non_success_http_status_fails_instead_of_returning_empty_context() {
    for status in [302, 199, 300, 307, 399] {
        let mut e = engine();
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = ask(&mut e, "ADAUSDT");
        e.handle(
            NOW,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body: r#"{"code":0}"#,
                },
            },
        );
        let out: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            out.iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
                .count(),
            1,
            "HTTP {status} must fail the read"
        );
        assert!(!out.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::CommandDone { command: 42, .. })
        )));
        assert_eq!(e.account_snapshot_frame(NOW), before);
        assert!(reply(&mut e, &r, json!({"code":0})).is_empty());
    }
}

#[test]
fn zero_leverage_and_unknown_mode_keep_ts_defaults_instead_of_requiring_an_open_position() {
    let mut e = engine();
    let r = ask(&mut e, "ADAUSDT");
    assert_eq!(
        done(reply(
            &mut e,
            &r,
            json!({"code":0,"data":{"positions":[
                {"symbol":"ADAUSDT","leverageRr":"0","size":"0"}
            ]}})
        )),
        json!({"buyLeverage":0.,"sellLeverage":0.,"marginType":"isolated","positionMode":"oneway"})
    );
}
#[test]
fn snake_case_wire_fields_are_accepted_like_ts_v() {
    let mut e = engine();
    let r = ask(&mut e, "ADAUSDT");
    assert_eq!(
        done(reply(
            &mut e,
            &r,
            json!({"code":0,"data":{"positions":[
                {"symbol":"ADAUSDT","pos_side":"Long","pos_mode":"Hedged","leverage_rr":"4","cross_margin":true}
            ]}})
        )),
        json!({"buyLeverage":4.,"sellLeverage":4.,"marginType":"cross","positionMode":"hedged"})
    );
}
#[test]
fn read_uses_secondary_credentials_and_refuses_public_only_before_network() {
    let mut e = support::prepared_engine(Options {
        key: Some("primary-fixture".into()),
        secret: Some("primary-material".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-fixture".into(),
            secret: "read-material".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    let r = ask(&mut e, "ADAUSDT");
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == "read-fixture")
    );
    for options in [
        Options::default(),
        Options {
            public_data_only: true,
            key: Some("fixture".into()),
            secret: Some("material".into()),
            ..Default::default()
        },
    ] {
        let mut e = support::prepared_engine(options);
        e.handle(
            NOW,
            Event::Command(Command::FetchLeverage {
                command: 42,
                symbol: "ADAUSDT".into(),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
                .count(),
            1
        );
    }
}
