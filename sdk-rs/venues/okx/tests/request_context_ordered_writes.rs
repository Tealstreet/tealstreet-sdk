#![allow(dead_code)]
mod support;

use safe_cex_engine::model::{Interval, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::request_context::{HttpAction, Preparation};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Response};
use safe_cex_venue_okx::Okx;
use support::{prepared, reply, trading_engine};

const OWNER: u32 = 41;

fn order(amount: u32) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"market","side":"buy","amount":{amount}}}"#
    ))
    .unwrap()
}

fn stored(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Sell,
        price: 38000.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn place(engine: &mut Engine<Okx>, orders: Vec<PlaceOrderOpts>) {
    engine.handle(
        5.,
        Event::Command(Command::PlaceOrders {
            command: OWNER,
            orders,
        }),
    );
}

fn post(output: &[Action]) -> &Action {
    output
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
        .expect("native physical write")
}

fn small_lots(engine: &mut Engine<Okx>) {
    let mut markets = engine.store().markets().to_vec();
    markets[0].limits.amount.max = 1.;
    markets[0].precision.amount = 1.;
    engine.store_mut().set_markets(markets);
}

#[test]
fn write_requires_managed_route_preparation_before_provider_or_release() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(
        output.iter().any(|a| matches!(a,
            Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
        )),
        "write must begin with genuine managed preparation"
    );
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
}

fn fill_capacity(engine: &mut Engine<Okx>) -> Vec<Preparation> {
    let mut held = Vec::new();
    for command in 1000..1600 {
        engine.handle(
            5.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let output: Vec<_> = engine.drain_actions().collect();
        if output.iter().any(|a| {
            matches!(a,
                Action::Notice(Notice::CommandFailed { command: id, error })
                    if *id == command && error.kind == Some(ErrorKind::Internal)
            )
        }) {
            assert!(!held.is_empty());
            return held;
        }
        held.push(
            output
                .iter()
                .find_map(|a| match a {
                    Action::ManagedHttp(action) => match action.as_ref() {
                        HttpAction::Prepare { preparation, .. } => Some(*preparation),
                        _ => None,
                    },
                    _ => None,
                })
                .expect("live peer preparation"),
        );
    }
    panic!("did not reach actual shared capacity")
}

#[test]
fn admission_capacity_refusal_precedes_optimistic_store_change() {
    let mut engine = trading_engine();
    engine.store_mut().upsert_order(stored("123"));
    let peers = fill_capacity(&mut engine);
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids: vec!["123".into()],
        }),
    );
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(peers.iter().all(|p| engine.http_preparation_pending(*p)));
    assert_eq!(
        engine.store().order("123").unwrap().status,
        OrderStatus::Open,
        "refused admission cannot publish optimistic Canceling"
    );
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: OWNER, error })
                    if error.kind == Some(ErrorKind::Internal)
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            ))
            .count(),
        1
    );
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Sign { .. }
            | Action::Notice(Notice::CommandProgress { command: OWNER, .. })
    )));
}

#[test]
fn native_per_input_split_maximum_and_maximum_plus_one() {
    for amount in [10_000, 10_001] {
        let mut engine = trading_engine();
        small_lots(&mut engine);
        place(&mut engine, vec![order(amount)]);
        let output = prepared(&mut engine, 5.);
        if amount == 10_000 {
            let Action::Http(request) = post(&output) else {
                unreachable!()
            };
            let rows: Vec<serde_json::Value> =
                serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(rows.len(), 20);
            assert!(rows.iter().all(|row| row["sz"] == "1"));
        } else {
            assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(output.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: OWNER, error })
                    if error.message == "order exceeds safe split bounds"
            )));
        }
    }
}

#[test]
fn native_split_bound_applies_per_input_not_to_whole_command() {
    let mut engine = trading_engine();
    small_lots(&mut engine);
    place(&mut engine, vec![order(10_000), order(10_000)]);
    let output = prepared(&mut engine, 5.);
    post(&output);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn native_all_refused_cancels_report_each_row_continue_then_finish_empty() {
    let mut engine = trading_engine();
    let ids: Vec<_> = (1..=21).map(|i| i.to_string()).collect();
    for id in &ids {
        engine.store_mut().upsert_order(stored(id));
    }
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids,
        }),
    );
    let first = prepared(&mut engine, 5.);
    let rows = [r#"{"sCode":"51400","sMsg":"Order does not exist"}"#; 20].join(",");
    reply(
        &mut engine,
        post(&first),
        &format!(r#"{{"code":"2","data":[{rows}]}}"#),
        6.,
    );
    let second = prepared(&mut engine, 6.);
    assert_eq!(second.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandError { command: OWNER, message }) if message.contains("51400")
    )).count(), 20);
    reply(
        &mut engine,
        post(&second),
        r#"{"code":"2","data":[{"sCode":"51400","sMsg":"Order does not exist"}]}"#,
        7.,
    );
    let output = prepared(&mut engine, 7.);
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandError { command: OWNER, .. })
            ))
            .count(),
        1
    );
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a,
                Action::Notice(Notice::CommandDone { command: OWNER, result }) if result == "[]"
            ))
            .count(),
        1
    );
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn native_50004_stops_protective_chain_and_starts_reconciliation_without_retry() {
    let mut engine = trading_engine();
    let mut entry = order(1);
    entry.stop_loss = Some(36000.);
    entry.take_profit = Some(42000.);
    place(&mut engine, vec![entry]);
    let output = prepared(&mut engine, 5.);
    let Action::Http(request) = post(&output) else {
        unreachable!()
    };
    engine.handle(
        6.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":"50004","msg":"timeout","data":[]}"#,
            },
        },
    );
    let output = prepared(&mut engine, 6.);
    assert!(!output.iter().any(|a| matches!(a,
        Action::Http(r) if r.method == safe_cex_engine::Method::Post
    )));
    assert!(output.iter().any(|a| matches!(a,
        Action::Http(r) if r.url.contains("orders-pending")
    )));
    assert!(output.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: OWNER, error })
            if error.outcome == Some(safe_cex_engine::Outcome::Unknown)
    )));
}
