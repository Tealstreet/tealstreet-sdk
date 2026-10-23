//! Original adoption controls use the real venue, trusted signer and host driver.
use safe_cex_engine::model::{Market, MinMax, PlaceOrderOpts, Precision};
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome,
    ReadCredentials, Response, VenueError,
};
use safe_cex_venue_weex::WeexFutures;
use serde_json::{Value, json};
mod support;

const COMMAND: u32 = 71;
const START: f64 = 1_000.;

fn engine() -> Engine<WeexFutures> {
    let mut engine = support::setup::engine(support::setup::Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    });
    engine.store_mut().set_markets(vec![Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: safe_cex_engine::model::Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    engine.store_mut().update_setting("BTCUSDT", |setting| {
        setting.leverage = 20.;
        setting.margin_type = Some(safe_cex_engine::model::MarginType::Cross);
    });
    engine
}

fn orders(count: usize) -> Vec<PlaceOrderOpts> {
    (0..count)
        .map(|index| {
            serde_json::from_value(json!({
                "symbol": "BTCUSDT", "type": "limit", "side": "buy",
                "amount": 0.01, "price": 69_000., "clientOrderId": format!("plan-{index}")
            }))
            .unwrap()
        })
        .collect()
}

fn place(engine: &mut Engine<WeexFutures>, count: usize) {
    engine.handle(
        START,
        Event::Command(Command::PlaceOrders {
            command: COMMAND,
            orders: orders(count),
        }),
    );
}

fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request)
                if request.method == Method::Post
                    && request.url.contains("/capi/v3/")
                    && (request.url.ends_with("/batchOrders")
                        || request.url.ends_with("/order")) =>
            {
                Some(request.clone())
            }
            _ => None,
        })
        .collect()
}

fn packet(actions: &[Action]) -> HttpRequest {
    let mut requests = writes(actions);
    assert_eq!(requests.len(), 1, "one native write at a time");
    requests.pop().unwrap()
}

fn collect(engine: &mut Engine<WeexFutures>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}

fn acknowledge(
    engine: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    now: f64,
    first: u64,
) -> Vec<Action> {
    let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let body = if let Some(rows) = payload["batchOrders"].as_array() {
        Value::Array(
            rows.iter()
                .enumerate()
                .rev()
                .map(|(index, row)| {
                    json!({
                        "orderId": (first + index as u64).to_string(),
                        "clientOrderId": row["newClientOrderId"], "success": true,
                        "errorCode": "", "errorMessage": ""
                    })
                })
                .collect(),
        )
    } else {
        json!({"orderId": first.to_string(), "clientOrderId": payload["newClientOrderId"], "success": true})
    };
    respond(engine, request, now, 200, &body.to_string())
}

fn respond(
    engine: &mut Engine<WeexFutures>,
    request: &HttpRequest,
    now: f64,
    status: u16,
    body: &str,
) -> Vec<Action> {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    collect(engine, now)
}

fn failures(actions: &[Action]) -> Vec<&VenueError> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect()
}

fn ids(actions: &[Action]) -> Vec<String> {
    let result = actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("one native command result");
    serde_json::from_str(result).unwrap()
}

#[test]
fn ordinary_write_requires_managed_preparation_before_physical_http() {
    let mut engine = engine();
    place(&mut engine, 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
                Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
            ))
            .count(),
        1,
        "ordinary write bypassed managed Prepare/Bind/Claim"
    );
    assert!(
        writes(&actions).is_empty(),
        "nothing releases before host preparation"
    );
}

#[test]
fn eight_serial_calls_keep_each_native_twenty_second_attempt() {
    let mut engine = engine();
    place(&mut engine, 40);
    let mut now = START;
    let mut actions = collect(&mut engine, now);
    for index in 0..8 {
        let request = packet(&actions);
        assert_eq!(
            request.timeout_ms,
            Some(20_000),
            "call {index} needs its own admitted physical window"
        );
        now += 19_000.;
        actions = acknowledge(&mut engine, &request, now, 9_000 + index * 5);
    }
    assert_eq!(now - START, 152_000.);
    assert_eq!(
        ids(&actions),
        (9_000..9_040).map(|id| id.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn released_write_expires_once_with_unknown_and_late_ack_cannot_complete() {
    let mut engine = engine();
    place(&mut engine, 1);
    let actions = collect(&mut engine, START);
    let request = packet(&actions);
    let mut expired = Vec::new();
    for action in &actions {
        if let Action::SetTimer { timer, at_ms } = action
            && *at_ms <= START + 20_000.
        {
            engine.handle(START + 20_000., Event::TimerFired { timer: *timer });
            expired.extend(collect(&mut engine, START + 20_000.));
        }
    }
    let errors = failures(&expired);
    assert_eq!(
        errors.len(),
        1,
        "released ordinary write has no enforced engine deadline"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    let late = acknowledge(&mut engine, &request, START + 20_001., 9_000);
    assert!(failures(&late).is_empty());
    assert!(!late.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone {
            command: COMMAND,
            ..
        })
    )));
}

fn fail_second_after_prefix(status: Option<u16>) -> Vec<Action> {
    let mut engine = engine();
    place(&mut engine, 6);
    let first = packet(&collect(&mut engine, START));
    let actions = acknowledge(&mut engine, &first, START + 1., 9_000);
    let second = packet(&actions);
    if let Some(status) = status {
        respond(
            &mut engine,
            &second,
            START + 2.,
            status,
            r#"{"code":"40006","msg":"Invalid ACCESS_KEY"}"#,
        )
    } else {
        engine.handle(
            START + 2.,
            Event::HttpFailed {
                request: second.id,
                error: "transport lost after release",
            },
        );
        collect(&mut engine, START + 2.)
    }
}

#[test]
fn accepted_prefix_is_preserved_when_the_next_write_is_uncertain() {
    let actions = fail_second_after_prefix(None);
    let errors = failures(&actions);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    assert_eq!(
        errors[0]
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.clone()),
        Some((9_000..9_005).map(|id| id.to_string()).collect())
    );
    assert!(
        writes(&actions).is_empty(),
        "an uncertain write must never be retried"
    );
}

#[test]
fn accepted_prefix_is_known_partial_after_a_proven_later_refusal() {
    let actions = fail_second_after_prefix(Some(400));
    let errors = failures(&actions);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, None);
    assert_eq!(
        errors[0]
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.clone()),
        Some((9_000..9_005).map(|id| id.to_string()).collect())
    );
}

#[test]
fn a_system_error_code_does_not_prove_the_write_was_refused() {
    let mut engine = engine();
    place(&mut engine, 1);
    let request = packet(&collect(&mut engine, START));
    let actions = respond(
        &mut engine,
        &request,
        START + 1.,
        400,
        r#"{"code":"40015","msg":"System error"}"#,
    );
    let errors = failures(&actions);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    assert!(writes(&actions).is_empty());
}

#[test]
fn compatibility_primary_recipe_and_no_transport_retry() {
    let mut engine = engine();
    place(&mut engine, 1);
    let request = packet(&collect(&mut engine, START));
    let body = request.body.as_deref().unwrap();
    let stamp = request
        .headers
        .iter()
        .find(|(name, _)| *name == "ACCESS-TIMESTAMP")
        .unwrap()
        .1
        .clone();
    assert_eq!(
        request.headers,
        support::setup::oracle(
            "primary-key",
            "primary-secret",
            "primary-pass",
            Method::Post,
            "/capi/v3/batchOrders",
            body,
            &stamp,
        )
    );
    engine.handle(
        START + 1.,
        Event::HttpFailed {
            request: request.id,
            error: "transport lost",
        },
    );
    let actions = collect(&mut engine, START + 1.);
    assert_eq!(failures(&actions).len(), 1);
    assert!(writes(&actions).is_empty());
}

#[test]
fn compatibility_long_serial_success_preserves_native_ids() {
    let mut engine = engine();
    place(&mut engine, 40);
    let mut actions = collect(&mut engine, START);
    for index in 0..8 {
        let request = packet(&actions);
        actions = acknowledge(
            &mut engine,
            &request,
            START + (index + 1) as f64 * 19_000.,
            9_000 + index * 5,
        );
    }
    assert_eq!(
        ids(&actions),
        (9_000..9_040).map(|id| id.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn compatibility_invalid_later_order_refuses_before_any_write() {
    let mut engine = engine();
    let mut orders = orders(6);
    orders[5].amount = f64::NAN;
    engine.handle(
        START,
        Event::Command(Command::PlaceOrders {
            command: COMMAND,
            orders,
        }),
    );
    let actions = collect(&mut engine, START);
    assert!(writes(&actions).is_empty());
    assert_eq!(failures(&actions).len(), 1);
}
