mod support;

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
    e.drain_actions().filter_map(|a| match a {
        Action::Notice(Notice::Event(event)) if event.code == "row.skipped" => Some(event),
        _ => None,
    }).collect()
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
                let order = e.store().order("synthetic-order-a").expect("documented WS order");
                assert_eq!(order.remaining, 2.0, "{typ}/{direction}/{snapshot}");
                assert!(order.reduce_only);
                assert!(reports(&mut e).is_empty());
            }
        }
    }
}

#[test]
fn documented_rest_types_and_fields_map() {
    for typ in ["lmt", "post", "ioc", "fok", "mkt", "stp", "take_profit", "trailing_stop"] {
        for side in ["buy", "sell"] {
            let (mut e, socket) = ready();
            message(&mut e, socket, NOW, r#"{"feed":"fills","fills":[]}"#);
            let timer = e.drain_actions().find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if at_ms > NOW && at_ms <= NOW + 1001.0 => Some((timer, at_ms)),
                _ => None,
            }).expect("reconciliation timer");
            e.handle(timer.1, Event::TimerFired { timer: timer.0 });
            let reqs = requests(&mut e);
            let request = reqs.iter().find(|r| r.url.contains("openorders")).unwrap();
            let row = json!({"order_id":"synthetic-order-a", "symbol":"PF_XBTUSD", "side":side,
                "orderType":typ, "limitPrice":100, "stopPrice":90, "unfilledSize":2,
                "filledSize":1, "reduceOnly":true, "triggerSignal":"last"});
            reply(&mut e, request, 200, &json!({"result":"success", "openOrders":[row]}).to_string());
            let order = e.store().order("synthetic-order-a").expect("documented REST order");
            assert_eq!(order.amount, 3.0, "{typ}/{side}");
            assert_eq!(order.remaining, 2.0);
            assert!(reports(&mut e).is_empty());
        }
    }
}

fn refused(mut row: Value, field: &str, value: Value, reason: &str) {
    row[field] = value;
    let (mut e, socket) = ready();
    e.drain_actions().for_each(drop);
    send(&mut e, socket, &row, true);
    assert!(e.store().orders().is_empty(), "a refusal cannot invent an order");
    let events = reports(&mut e);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, Level::Error);
    assert_eq!(events[0].message, format!("Kraken skipped unmappable order ({reason})"));
    assert_eq!(events[0].fields.iter().find(|(key, _)| *key == "order_row_reason").unwrap().1.as_str(), Some(reason));
}

#[test]
fn refused_type_names_its_predicate() { refused(row(), "type", json!("PRIVATE_TYPE_VALUE"), "order_type"); }
#[test]
fn refused_direction_names_its_predicate() { refused(row(), "direction", json!(2), "direction"); }
#[test]
fn refused_quantity_names_its_predicate() { refused(row(), "qty", json!(-2), "quantity"); }
#[test]
fn refused_missing_identity_names_its_predicate() { refused(row(), "order_id", json!(""), "missing_order_id"); }
#[test]
fn refused_missing_symbol_names_its_predicate() { refused(row(), "instrument", json!(""), "missing_symbol"); }
#[test]
fn refused_wire_schema_names_its_predicate() { refused(row(), "limit_price", Value::Null, "wire_decode"); }

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
    assert_eq!(b.len(), 1, "different identities retain independent suppression");
    assert_eq!(a[0].message, b[0].message, "grouping excludes identities and economic values");
    let shape = a[0].fields.iter().find(|(key, _)| *key == "order_row_fields").unwrap().1.as_str().unwrap();
    assert!(shape.contains("order_id:string"));
    assert!(shape.contains("instrument:string"));
    assert!(shape.contains("qty:number"));
    assert!(shape.contains("unfilledSize:missing"));
    assert!(shape.contains("cli_ord_id:string"));
    let emitted = format!("{a:?}{b:?}");
    for private in ["synthetic-order-a", "synthetic-order-b", "PRIVATE_TYPE_VALUE", "PRIVATE_BODY_VALUE", "PRIVATE_CLIENT_ID", "extra_private_key", "999"] {
        assert!(!emitted.contains(private), "leaked {private}");
    }
    send(&mut e, socket, &second, true);
    assert!(reports(&mut e).is_empty(), "same refused identity stays deduplicated");
}
