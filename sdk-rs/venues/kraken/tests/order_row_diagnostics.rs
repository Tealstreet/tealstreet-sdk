mod support;

use safe_cex_engine::model::{OrderSide, OrderType, StopTriggerType, TimeInForce};
use safe_cex_engine::{Action, Engine, Event, Level, Notice, SocketId, log::LogEvent};
use safe_cex_venue_kraken::Kraken;
use serde_json::{Value, json};
use support::*;

fn row() -> Value {
    json!({"order_id":"synthetic-order-a", "instrument":"PF_XBTUSD", "qty":2,
        "filled":0, "type":"limit", "direction":0, "limit_price":100})
}

fn send(e: &mut Engine<Kraken>, socket: SocketId, row: &Value, snapshot: bool) {
    let body = if snapshot {
        json!({"feed":"open_orders_snapshot", "orders":[row]})
    } else {
        json!({"feed":"open_orders", "order":row, "is_cancel":false})
    };
    message(e, socket, NOW, &body.to_string());
}

fn reports(e: &mut Engine<Kraken>) -> Vec<LogEvent> {
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.code == "row.skipped" => Some(event),
            _ => None,
        })
        .collect()
}

#[test]
fn documented_ws_snapshot_and_delta_shapes_map_without_unfilled_size() {
    for typ in ["limit", "stop", "take_profit", "trailing_stop"] {
        for direction in [0, 1] {
            for snapshot in [false, true] {
                let (mut e, socket) = ready();
                let mut row = row();
                row["type"] = json!(typ);
                row["direction"] = json!(direction);
                row["stop_price"] = json!(90);
                row["reduce_only"] = json!(true);
                row["triggerSignal"] = json!("mark");
                send(&mut e, socket, &row, snapshot);
                let order = e
                    .store()
                    .order("synthetic-order-a")
                    .expect("documented WS order");
                assert_eq!(order.remaining, 2.0, "{typ}/{direction}/{snapshot}");
                assert_eq!(
                    order.kind,
                    match typ {
                        "stop" => OrderType::StopLoss,
                        "take_profit" => OrderType::TakeProfit,
                        "trailing_stop" => OrderType::TrailingStopLoss,
                        _ => OrderType::Limit,
                    }
                );
                assert_eq!(
                    order.side,
                    if direction == 0 {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    }
                );
                assert_eq!(order.trigger_type, Some(StopTriggerType::MarkPrice));
                assert!(order.reduce_only);
                assert!(reports(&mut e).is_empty());
            }
        }
    }
}

#[test]
fn documented_rest_types_and_fields_map() {
    for typ in [
        "lmt",
        "post",
        "ioc",
        "fok",
        "mkt",
        "stp",
        "take_profit",
        "trailing_stop",
    ] {
        for side in ["buy", "sell"] {
            let (mut e, socket) = ready();
            message(&mut e, socket, NOW, r#"{"feed":"fills","fills":[]}"#);
            let timer = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if at_ms > NOW && at_ms <= NOW + 1001.0 => {
                        Some((timer, at_ms))
                    }
                    _ => None,
                })
                .expect("reconciliation timer");
            e.handle(timer.1, Event::TimerFired { timer: timer.0 });
            let reqs = requests(&mut e);
            let request = reqs.iter().find(|r| r.url.contains("openorders")).unwrap();
            let row = json!({"order_id":"synthetic-order-a", "symbol":"PF_XBTUSD", "side":side,
                "orderType":typ, "limitPrice":100, "stopPrice":90, "unfilledSize":2,
                "filledSize":1, "reduceOnly":true, "triggerSignal":"last"});
            reply(
                &mut e,
                request,
                200,
                &json!({"result":"success", "openOrders":[row]}).to_string(),
            );
            let order = e
                .store()
                .order("synthetic-order-a")
                .expect("documented REST order");
            assert_eq!(order.amount, 3.0, "{typ}/{side}");
            assert_eq!(order.remaining, 2.0);
            assert_eq!(
                order.kind,
                match typ {
                    "stp" => OrderType::StopLoss,
                    "take_profit" => OrderType::TakeProfit,
                    "trailing_stop" => OrderType::TrailingStopLoss,
                    "mkt" => OrderType::Market,
                    _ => OrderType::Limit,
                }
            );
            assert_eq!(
                order.side,
                if side == "buy" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                }
            );
            assert_eq!(
                order.time_in_force,
                Some(match typ {
                    "post" => TimeInForce::PostOnly,
                    "ioc" => TimeInForce::ImmediateOrCancel,
                    "fok" => TimeInForce::FillOrKill,
                    _ => TimeInForce::GoodTillCancel,
                })
            );
            assert_eq!(order.trigger_type, Some(StopTriggerType::LastPrice));
            assert!(reports(&mut e).is_empty());
        }
    }
}

fn refused(mut row: Value, field: &str, value: Value, reason: &str) {
    row[field] = value;
    let (mut e, socket) = ready();
    e.drain_actions().for_each(drop);
    send(&mut e, socket, &row, true);
    assert!(
        e.store().orders().is_empty(),
        "a refusal cannot invent an order"
    );
    let events = reports(&mut e);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::Error);
    assert_eq!(
        events[0].message,
        format!("Kraken skipped unmappable order ({reason})")
    );
    assert_eq!(
        events[0]
            .fields
            .iter()
            .find(|(key, _)| *key == "order_row_reason")
            .unwrap()
            .1
            .as_str(),
        Some(reason)
    );
}

#[test]
fn refused_type_names_its_predicate() {
    refused(row(), "type", json!("PRIVATE_TYPE_VALUE"), "order_type");
}
#[test]
fn refused_direction_names_its_predicate() {
    refused(row(), "direction", json!(2), "direction");
}
#[test]
fn refused_quantity_names_its_predicate() {
    refused(row(), "qty", json!(-2), "quantity");
}
#[test]
fn refused_missing_identity_names_its_predicate() {
    refused(row(), "order_id", json!(""), "missing_order_id");
}
#[test]
fn refused_missing_symbol_names_its_predicate() {
    refused(row(), "instrument", json!(""), "missing_symbol");
}
#[test]
fn refused_wire_schema_names_its_predicate() {
    refused(row(), "limit_price", Value::Null, "wire_decode");
}

#[test]
fn diagnostic_grouping_and_field_shapes_never_contain_row_values() {
    let (mut e, socket) = ready();
    e.drain_actions().for_each(drop);
    let mut first = row();
    first["type"] = json!("PRIVATE_TYPE_VALUE");
    first["extra_private_key"] = json!({"secret":"PRIVATE_BODY_VALUE"});
    first["cli_ord_id"] = json!("PRIVATE_CLIENT_ID");
    let mut second = first.clone();
    second["order_id"] = json!("synthetic-order-b");
    second["qty"] = json!(999);
    send(&mut e, socket, &first, true);
    let a = reports(&mut e);
    send(&mut e, socket, &second, true);
    let b = reports(&mut e);
    assert_eq!(a.len(), 1);
    assert_eq!(
        b.len(),
        1,
        "different identities retain independent suppression"
    );
    assert_eq!(
        a[0].message, b[0].message,
        "grouping excludes identities and economic values"
    );
    let shape = a[0]
        .fields
        .iter()
        .find(|(key, _)| *key == "order_row_fields")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert_eq!(
        shape.len(),
        29,
        "one closed type per documented fixed field"
    );
    assert_eq!(shape.as_bytes()[0], b's', "order_id:string");
    assert_eq!(shape.as_bytes()[4], b's', "instrument:string");
    assert_eq!(shape.as_bytes()[17], b'n', "qty:number");
    assert_eq!(shape.as_bytes()[14], b'-', "unfilledSize:missing");
    assert_eq!(shape.as_bytes()[23], b's', "cli_ord_id:string");
    let fields = a[0]
        .fields
        .iter()
        .map(|(key, value)| (*key, value.as_str().unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let event = json!({"level":"error", "code":a[0].code, "message":a[0].message, "fields":fields})
        .to_string();
    assert!(
        event.len() < 500,
        "complete event survives the consuming host's extra cap"
    );
    let emitted = format!("{a:?}{b:?}");
    for private in [
        "synthetic-order-a",
        "synthetic-order-b",
        "PRIVATE_TYPE_VALUE",
        "PRIVATE_BODY_VALUE",
        "PRIVATE_CLIENT_ID",
        "extra_private_key",
        "999",
    ] {
        assert!(!emitted.contains(private), "leaked {private}");
    }
    send(&mut e, socket, &second, true);
    assert!(
        reports(&mut e).is_empty(),
        "same refused identity stays deduplicated"
    );
}

#[test]
fn diagnostic_types_are_closed_for_every_json_kind() {
    for (value, kind) in [
        (Value::Null, b'z'),
        (json!(false), b'b'),
        (json!(123), b'n'),
        (json!("123"), b's'),
        (json!(["PRIVATE_ARRAY_VALUE"]), b'a'),
        (json!({"PRIVATE_KEY":"PRIVATE_VALUE"}), b'o'),
    ] {
        let (mut e, socket) = ready();
        e.drain_actions().for_each(drop);
        let mut bad = row();
        bad["type"] = json!("PRIVATE_TYPE_VALUE");
        bad["limit_price"] = value;
        send(&mut e, socket, &bad, true);
        let events = reports(&mut e);
        assert_eq!(events.len(), 1);
        let shape = events[0]
            .fields
            .iter()
            .find(|(key, _)| *key == "order_row_fields")
            .unwrap()
            .1
            .as_str()
            .unwrap();
        assert_eq!(shape.as_bytes()[10], kind, "limit_price JSON type");
        assert!(shape.bytes().all(|t| b"-zbnsao".contains(&t)));
        assert!(!format!("{events:?}").contains("PRIVATE"));
    }
}

#[test]
fn type_only_collection_matches_the_original_json_value_projection() {
    const FIELDS: [&str; 29] = [
        "order_id",
        "orderId",
        "uid",
        "symbol",
        "instrument",
        "orderType",
        "type",
        "side",
        "direction",
        "limitPrice",
        "limit_price",
        "price",
        "stopPrice",
        "stop_price",
        "unfilledSize",
        "filledSize",
        "filled",
        "qty",
        "quantity",
        "size",
        "reduceOnly",
        "reduce_only",
        "cliOrdId",
        "cli_ord_id",
        "trigger_signal",
        "triggerSignal",
        "lastUpdateTime",
        "last_update_time",
        "lastUpdateTimestamp",
    ];
    let mut cases = Vec::new();
    for field in FIELDS {
        for value in [
            json!(null),
            json!(true),
            json!(-2.5),
            json!("PRIVATE_STRING"),
            json!([true, {"nested":null}]),
            json!({"private_nested":[1, "SECRET"]}),
        ] {
            let mut bad = row();
            bad["type"] = json!("unsupported");
            bad[field] = value;
            // Every case refuses even when it changes a field the mapper otherwise ignores.
            bad[if field == "limit_price" {
                "stop_price"
            } else {
                "limit_price"
            }] = json!({"invalid":true});
            cases.push(bad.to_string());
        }
    }
    cases.extend([
        r#"{"\u006frder_id":"escaped-private-id","instrument":"PF_XBTUSD","qty":2,"direction":0,"type":"unsupported"}"#.into(),
        r#"{"order_id":"private-id","instrument":"PF_XBTUSD","qty":2,"type":"limit","type":false}"#.into(),
        r#"{"order_id":"private-id","instrument":"PF_XBTUSD","qty":1e999,"type":"unsupported"}"#.into(),
        r#"{"order_id":"private-id","instrument":"PF_XBTUSD","qty":2,"type":"unsupported","unknown":[{"overflow":1e999}]}"#.into(),
        "null".into(), "[]".into(), "false".into(),
    ]);
    for raw in cases {
        let reference = serde_json::from_str::<Value>(&raw).ok();
        let expected: String = FIELDS
            .iter()
            .map(
                |name| match reference.as_ref().and_then(|row| row.get(name)) {
                    None => '-',
                    Some(Value::Null) => 'z',
                    Some(Value::Bool(_)) => 'b',
                    Some(Value::Number(_)) => 'n',
                    Some(Value::String(_)) => 's',
                    Some(Value::Array(_)) => 'a',
                    Some(Value::Object(_)) => 'o',
                },
            )
            .collect();
        let (mut e, socket) = ready();
        e.drain_actions().for_each(drop);
        message(
            &mut e,
            socket,
            NOW,
            &format!(r#"{{"feed":"open_orders_snapshot","orders":[{raw}]}}"#),
        );
        let events = reports(&mut e);
        assert_eq!(events.len(), 1, "{raw}");
        let actual = events[0]
            .fields
            .iter()
            .find(|(key, _)| *key == "order_row_fields")
            .unwrap()
            .1
            .as_str()
            .unwrap();
        assert_eq!(actual, expected, "{raw}");
        assert!(
            e.store().orders().is_empty(),
            "diagnostic decoding cannot admit a refused row"
        );
        assert!(!format!("{events:?}").contains("private-id"));
    }
}
