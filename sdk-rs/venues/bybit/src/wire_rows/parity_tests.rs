//! The hand-written row decoders against serde's derived ones: every
//! documented row (the replay tapes' V5 payloads) and every single-field
//! mutation of it decodes to the same fields, or fails with the same message.
use super::*;
use serde_json::value::RawValue;

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

fn order_parity(text: &str) {
    let shipped = serde_json::from_str::<OrderRow>(text);
    let derived = serde_json::from_str::<derived::OrderRow>(text);
    match (shipped, derived) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a.order_id, b.order_id, "order_id on {text}");
            assert_eq!(a.order_link_id, b.order_link_id, "order_link_id on {text}");
            assert_eq!(a.symbol, b.symbol, "symbol on {text}");
            assert_eq!(a.category, b.category, "category on {text}");
            assert_eq!(a.price, b.price, "price on {text}");
            assert_eq!(a.qty, b.qty, "qty on {text}");
            assert_eq!(a.side, b.side, "side on {text}");
            assert_eq!(a.position_idx, b.position_idx, "position_idx on {text}");
            assert_eq!(a.order_status, b.order_status, "order_status on {text}");
            assert_eq!(a.reject_reason, b.reject_reason, "reject_reason on {text}");
            assert_eq!(a.avg_price, b.avg_price, "avg_price on {text}");
            assert_eq!(a.leaves_qty, b.leaves_qty, "leaves_qty on {text}");
            assert_eq!(a.cum_exec_qty, b.cum_exec_qty, "cum_exec_qty on {text}");
            assert_eq!(
                a.cum_exec_value, b.cum_exec_value,
                "cum_exec_value on {text}"
            );
            assert_eq!(a.time_in_force, b.time_in_force, "time_in_force on {text}");
            assert_eq!(a.order_type, b.order_type, "order_type on {text}");
            assert_eq!(
                a.stop_order_type, b.stop_order_type,
                "stop_order_type on {text}"
            );
            assert_eq!(a.trigger_price, b.trigger_price, "trigger_price on {text}");
            assert_eq!(
                a.trigger_direction, b.trigger_direction,
                "trigger_direction on {text}"
            );
            assert_eq!(a.take_profit, b.take_profit, "take_profit on {text}");
            assert_eq!(a.stop_loss, b.stop_loss, "stop_loss on {text}");
            assert_eq!(a.tp_trigger_by, b.tp_trigger_by, "tp_trigger_by on {text}");
            assert_eq!(a.sl_trigger_by, b.sl_trigger_by, "sl_trigger_by on {text}");
            assert_eq!(a.trigger_by, b.trigger_by, "trigger_by on {text}");
            assert_eq!(a.reduce_only, b.reduce_only, "reduce_only on {text}");
            assert_eq!(
                a.close_on_trigger, b.close_on_trigger,
                "close_on_trigger on {text}"
            );
            assert_eq!(a.tpsl_mode, b.tpsl_mode, "tpsl_mode on {text}");
            assert_eq!(a.create_type, b.create_type, "create_type on {text}");
            assert_eq!(a.created_time, b.created_time, "created_time on {text}");
            assert_eq!(a.updated_time, b.updated_time, "updated_time on {text}");
        }
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
        (Ok(a), Ok(b)) => {
            assert_eq!(a.symbol, b.symbol, "symbol on {text}");
            assert_eq!(a.category, b.category, "category on {text}");
            assert_eq!(a.side, b.side, "side on {text}");
            assert_eq!(a.size, b.size, "size on {text}");
            assert_eq!(a.position_idx, b.position_idx, "position_idx on {text}");
            assert_eq!(a.avg_price, b.avg_price, "avg_price on {text}");
            assert_eq!(a.mark_price, b.mark_price, "mark_price on {text}");
            assert_eq!(a.entry_price, b.entry_price, "entry_price on {text}");
            assert_eq!(
                a.position_value, b.position_value,
                "position_value on {text}"
            );
            assert_eq!(a.leverage, b.leverage, "leverage on {text}");
            assert_eq!(
                a.unrealised_pnl, b.unrealised_pnl,
                "unrealised_pnl on {text}"
            );
            assert_eq!(
                a.cur_realised_pnl, b.cur_realised_pnl,
                "cur_realised_pnl on {text}"
            );
            assert_eq!(a.liq_price, b.liq_price, "liq_price on {text}");
            assert_eq!(a.position_im, b.position_im, "position_im on {text}");
            assert_eq!(a.seq, b.seq, "seq on {text}");
            assert_eq!(
                a.position_status, b.position_status,
                "position_status on {text}"
            );
            assert_eq!(a.updated_time, b.updated_time, "updated_time on {text}");
            assert_eq!(a.delta, b.delta, "delta on {text}");
            assert_eq!(a.gamma, b.gamma, "gamma on {text}");
            assert_eq!(a.vega, b.vega, "vega on {text}");
            assert_eq!(a.theta, b.theta, "theta on {text}");
        }
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

/// A row's fields in order, each value as its raw JSON text.
struct Entries(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: serde::Deserializer<'de>>(raw: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Entries;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Entries, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, Box<RawValue>>()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }
        raw.deserialize_map(Visit)
    }
}

fn object(entries: &[(String, String)]) -> String {
    let body: Vec<String> = entries
        .iter()
        .map(|(key, value)| format!("{}:{value}", serde_json::to_string(key).unwrap()))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// Each documented row, then every single-field mutation of it.
fn corpus(rows: &str) -> Vec<String> {
    let replacements = [
        "null",
        "7",
        "-1.5",
        "true",
        "false",
        "\"text\"",
        "\"1\"",
        "\"a\\\"b\\u00e9\"",
        "{}",
        "[]",
    ];
    let mut out = Vec::new();
    for row in rows.lines().filter(|line| !line.trim().is_empty()) {
        out.push(row.to_owned());
        let entries: Vec<(String, String)> = serde_json::from_str::<Entries>(row)
            .expect("a documented row is an object")
            .0
            .into_iter()
            .map(|(key, value)| (key, value.get().to_owned()))
            .collect();
        for (index, (key, _)) in entries.iter().enumerate() {
            for replacement in replacements {
                let mut mutated = entries.clone();
                mutated[index].1 = replacement.to_owned();
                out.push(object(&mutated));
            }
            let mut removed = entries.clone();
            removed.remove(index);
            out.push(object(&removed));
            // A duplicated key, first and last.
            let quoted = serde_json::to_string(key).unwrap();
            out.push(format!("{{{quoted}:\"dup\",{}", &row[1..]));
            out.push(format!("{},{quoted}:\"dup\"}}", &row[..row.len() - 1]));
        }
        let mut unknown = entries.clone();
        unknown.push(("neverDocumented".into(), "{\"nested\":[1,2]}".into()));
        out.push(object(&unknown));
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
