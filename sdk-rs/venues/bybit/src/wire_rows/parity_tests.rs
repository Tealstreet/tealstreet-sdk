//! The hand-written row decoders against serde's derived ones: every
//! documented row (the replay tapes' V5 payloads) and every single-field
//! mutation of it decodes to the same fields, or fails with the same message.
use super::*;
use serde_json::Value;

const ORDERS: &str = include_str!("documented-orders.jsonl");
const POSITIONS: &str = include_str!("documented-positions.jsonl");

/// The pre-trim derived decoders, the parity reference.
mod derived {
    use super::super::{Opt, Str, flag, integer};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OrderRow<'a> {
        #[serde(borrow)]
        pub order_id: Str<'a>,
        #[serde(borrow, default)]
        pub order_link_id: Opt<'a>,
        #[serde(borrow)]
        pub symbol: Str<'a>,
        #[serde(borrow, default)]
        pub category: Opt<'a>,
        #[serde(borrow, default)]
        pub price: Opt<'a>,
        #[serde(borrow, default)]
        pub qty: Opt<'a>,
        #[serde(borrow, default)]
        pub side: Opt<'a>,
        #[serde(default, deserialize_with = "integer")]
        pub position_idx: Option<i64>,
        #[serde(borrow, default)]
        pub order_status: Str<'a>,
        #[serde(borrow, default)]
        pub reject_reason: Opt<'a>,
        #[serde(borrow, default)]
        pub avg_price: Opt<'a>,
        #[serde(borrow, default)]
        pub leaves_qty: Opt<'a>,
        #[serde(borrow, default)]
        pub cum_exec_qty: Opt<'a>,
        #[serde(borrow, default)]
        pub cum_exec_value: Opt<'a>,
        #[serde(borrow, default)]
        pub time_in_force: Opt<'a>,
        #[serde(borrow, default)]
        pub order_type: Opt<'a>,
        #[serde(borrow, default)]
        pub stop_order_type: Opt<'a>,
        #[serde(borrow, default)]
        pub trigger_price: Opt<'a>,
        #[serde(default, deserialize_with = "integer")]
        pub trigger_direction: Option<i64>,
        #[serde(borrow, default)]
        pub take_profit: Opt<'a>,
        #[serde(borrow, default)]
        pub stop_loss: Opt<'a>,
        #[serde(borrow, default)]
        pub tp_trigger_by: Opt<'a>,
        #[serde(borrow, default)]
        pub sl_trigger_by: Opt<'a>,
        #[serde(borrow, default)]
        pub trigger_by: Opt<'a>,
        #[serde(default, deserialize_with = "flag")]
        pub reduce_only: bool,
        #[serde(default, deserialize_with = "flag")]
        pub close_on_trigger: bool,
        #[serde(borrow, default)]
        pub tpsl_mode: Opt<'a>,
        #[serde(borrow, default)]
        pub create_type: Opt<'a>,
        #[serde(borrow, default)]
        pub created_time: Opt<'a>,
        #[serde(borrow, default)]
        pub updated_time: Opt<'a>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct PositionRow<'a> {
        #[serde(borrow)]
        pub symbol: Str<'a>,
        #[serde(borrow, default)]
        pub category: Opt<'a>,
        #[serde(borrow, default)]
        pub side: Opt<'a>,
        #[serde(borrow, default)]
        pub size: Opt<'a>,
        #[serde(default, deserialize_with = "integer")]
        pub position_idx: Option<i64>,
        #[serde(borrow, default)]
        pub avg_price: Opt<'a>,
        #[serde(borrow, default)]
        pub mark_price: Opt<'a>,
        #[serde(borrow, default)]
        pub entry_price: Opt<'a>,
        #[serde(borrow, default)]
        pub position_value: Opt<'a>,
        #[serde(borrow, default)]
        pub leverage: Opt<'a>,
        #[serde(borrow, default)]
        pub unrealised_pnl: Opt<'a>,
        #[serde(borrow, default)]
        pub cur_realised_pnl: Opt<'a>,
        #[serde(borrow, default)]
        pub liq_price: Opt<'a>,
        #[serde(rename = "positionIM", borrow, default)]
        pub position_im: Opt<'a>,
        #[serde(default, deserialize_with = "integer")]
        pub seq: Option<i64>,
        #[serde(borrow, default)]
        pub position_status: Opt<'a>,
        #[serde(borrow, default)]
        pub updated_time: Opt<'a>,
        #[serde(borrow, default)]
        pub delta: Opt<'a>,
        #[serde(borrow, default)]
        pub gamma: Opt<'a>,
        #[serde(borrow, default)]
        pub vega: Opt<'a>,
        #[serde(borrow, default)]
        pub theta: Opt<'a>,
    }
}

macro_rules! same_fields {
    ($a:expr, $b:expr, $text:expr; $($field:ident),+ $(,)?) => {
        {
            $(assert_eq!($a.$field, $b.$field, "{} on {}", stringify!($field), $text);)+
        }
    };
}

fn order_parity(text: &str) {
    let shipped = serde_json::from_str::<OrderRow>(text);
    let derived = serde_json::from_str::<derived::OrderRow>(text);
    match (shipped, derived) {
        (Ok(a), Ok(b)) => same_fields!(a, b, text;
            order_id, order_link_id, symbol, category, price, qty, side, position_idx,
            order_status, reject_reason, avg_price, leaves_qty, cum_exec_qty, cum_exec_value,
            time_in_force, order_type, stop_order_type, trigger_price, trigger_direction,
            take_profit, stop_loss, tp_trigger_by, sl_trigger_by, trigger_by, reduce_only,
            close_on_trigger, tpsl_mode, create_type, created_time, updated_time),
        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "error on {text}"),
        (a, b) => panic!(
            "outcome differs on {text}: shipped ok={} derived ok={}",
            a.is_ok(),
            b.is_ok()
        ),
    }
    let listed = format!("[{text}]");
    let shipped = serde_json::from_str::<Vec<OrderRow>>(&listed).map(|rows| rows.len());
    let derived = serde_json::from_str::<Vec<derived::OrderRow>>(&listed).map(|rows| rows.len());
    assert_eq!(
        shipped.map_err(|e| e.to_string()),
        derived.map_err(|e| e.to_string()),
        "listed {text}"
    );
}

fn position_parity(text: &str) {
    let shipped = serde_json::from_str::<PositionRow>(text);
    let derived = serde_json::from_str::<derived::PositionRow>(text);
    match (shipped, derived) {
        (Ok(a), Ok(b)) => same_fields!(a, b, text;
            symbol, category, side, size, position_idx, avg_price, mark_price, entry_price,
            position_value, leverage, unrealised_pnl, cur_realised_pnl, liq_price, position_im,
            seq, position_status, updated_time, delta, gamma, vega, theta),
        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "error on {text}"),
        (a, b) => panic!(
            "outcome differs on {text}: shipped ok={} derived ok={}",
            a.is_ok(),
            b.is_ok()
        ),
    }
    let listed = format!("[{text}]");
    let shipped = serde_json::from_str::<Vec<PositionRow>>(&listed).map(|rows| rows.len());
    let derived = serde_json::from_str::<Vec<derived::PositionRow>>(&listed).map(|rows| rows.len());
    assert_eq!(
        shipped.map_err(|e| e.to_string()),
        derived.map_err(|e| e.to_string()),
        "listed {text}"
    );
}

/// Each documented row, then every single-field mutation of it.
fn corpus(rows: &str) -> Vec<String> {
    let mut out = Vec::new();
    for row in rows.lines().filter(|line| !line.trim().is_empty()) {
        out.push(row.to_owned());
        let Value::Object(object) = serde_json::from_str::<Value>(row).unwrap() else {
            panic!("a documented row is an object");
        };
        for key in object.keys() {
            for replacement in [
                Value::Null,
                Value::from(7),
                Value::from(-1.5),
                Value::from(true),
                Value::from(false),
                Value::from("text"),
                Value::from("1"),
                Value::from("a\"b\\u00e9"),
                serde_json::json!({}),
                serde_json::json!([]),
            ] {
                let mut mutated = object.clone();
                mutated.insert(key.clone(), replacement);
                out.push(Value::Object(mutated).to_string());
            }
            let mut removed = object.clone();
            removed.remove(key);
            out.push(Value::Object(removed).to_string());
            // A duplicated key, first and last.
            let quoted = serde_json::to_string(key).unwrap();
            out.push(format!("{{{quoted}:\"dup\",{}", &row[1..]));
            out.push(format!("{},{quoted}:\"dup\"}}", &row[..row.len() - 1]));
        }
        let mut unknown = object.clone();
        unknown.insert(
            "neverDocumented".into(),
            serde_json::json!({"nested": [1, 2]}),
        );
        out.push(Value::Object(unknown).to_string());
    }
    for odd in [
        "{}",
        "[]",
        "null",
        "\"row\"",
        "7",
        "[\"a\",\"b\"]",
        "{\"orderId\":1}",
        "{\"symbol\":\"X\"",
    ] {
        out.push(odd.to_owned());
    }
    out
}

#[test]
fn documented_order_rows_and_every_mutation_decode_identically() {
    let cases = corpus(ORDERS);
    assert!(cases.len() > 1_000, "{} cases", cases.len());
    for case in &cases {
        order_parity(case);
    }
}

#[test]
fn documented_position_rows_and_every_mutation_decode_identically() {
    let cases = corpus(POSITIONS);
    assert!(cases.len() > 300, "{} cases", cases.len());
    for case in &cases {
        position_parity(case);
    }
}
