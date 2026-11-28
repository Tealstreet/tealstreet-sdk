//! One info line per order-affecting command, so "who placed this order" is
//! answerable from the journal. Every field comes from an allowlist: keys,
//! signatures and proxy tokens can never reach a line.
use safe_cex_engine::engine::Command;
use safe_cex_engine::error::VenueError;
use serde_json::{Map, Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// The account a command acts on.
#[derive(Clone, Debug, Default)]
pub struct Who {
    pub user: String,
    pub account: String,
    pub venue: String,
}

const ENGINE_OPS: [&str; 13] = [
    "placeOrders",
    "cancelOrders",
    "cancelOrdersWithSymbols",
    "cancelSymbolOrders",
    "amendOrder",
    "amendOrderWithSymbol",
    "setLeverage",
    "setMarginType",
    "setPositionMode",
    "setStpMode",
    "setAccountTier",
    "cancelAccountOrders",
    // Hyperliquid's dead-man switch: it cancels every open order at `time`.
    "scheduleCancel",
];

/// Hosted `callMethod`s that change orders or account settings.
pub const ORDER_METHODS: [&str; 19] = [
    "placeOrders",
    "placeOrder",
    "placeOrderWithClientOrderId",
    "updateOrder",
    "cancelOrders",
    "cancelSymbolOrders",
    "cancelAllOrders",
    "setLeverage",
    "setAllLeverage",
    "changeMarginType",
    "changePositionMode",
    "scheduleCancel",
    "nuke",
    "nukePositions",
    "nukeOrders",
    "stopAccountAlgos",
    "delegatedCancelAccountOrders",
    "delegatedSetStpMode",
    "delegatedSetAccountTier",
];

const COMMAND_FIELDS: [&str; 14] = [
    "symbol",
    "side",
    "type",
    "amount",
    "price",
    "triggerPrice",
    "reduceOnly",
    "clientOrderId",
    "id",
    "leverage",
    "isolated",
    "hedged",
    "mode",
    "time",
];

const ALGO_FIELDS: [&str; 13] = [
    "id",
    "twapId",
    "chaserId",
    "clientId",
    "symbol",
    "side",
    "size",
    "amount",
    "price",
    "reduceOnly",
    "maxAttempts",
    "duration",
    "command",
];

const ERROR_FIELDS: [&str; 5] = ["name", "kind", "message", "outcome", "venueCode"];

const MAX_TEXT: usize = 160;

#[path = "audit_ack.rs"]
mod acknowledgement;

#[cfg(test)]
#[path = "audit_ack_tests.rs"]
mod ack_tests;

pub(crate) fn algo_error(error: &str) -> Value {
    Value::Object(error_fields(&json!({"message": error})))
}

fn error_fields(detail: &Value) -> Map<String, Value> {
    let mut fields = pick(detail, &ERROR_FIELDS);
    if let Some(raw) = detail["message"].as_str() {
        // Decode the native legacy wrapper before either 160-character cap.
        // Only the closed acknowledgement projector may retain extra fields.
        let decoded = (raw.len() <= 65_536)
            .then(|| serde_json::from_str::<Value>(raw).ok())
            .flatten();
        let message = decoded
            .as_ref()
            .and_then(|value| value["message"].as_str())
            .unwrap_or_else(|| {
                if raw.trim_start().starts_with('{') {
                    "Unparseable native error detail"
                } else {
                    raw
                }
            });
        fields.insert(
            "message".into(),
            message.chars().take(MAX_TEXT).collect::<String>().into(),
        );
        if let Some(ack) = acknowledgement::project(message, decoded.as_ref()) {
            fields.insert("bybitBatchAcknowledgement".into(), ack);
        }
    }
    fields
}

fn scalar(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(text.chars().take(MAX_TEXT).collect())),
        Value::Number(_) | Value::Bool(_) => Some(value.clone()),
        _ => None,
    }
}

fn pick(source: &Value, fields: &[&str]) -> Map<String, Value> {
    fields
        .iter()
        .filter_map(|field| Some(((*field).to_owned(), scalar(source.get(*field)?)?)))
        .collect()
}

/// Refusals before handoff are journal-only and contain no client values.
pub(crate) fn pre_dispatch_refused(venue: Option<&str>, op: &str, message: &str) {
    command_refused(venue, op, message, false);
}

/// A grouped command proves only its own refusal, never the whole action.
pub(crate) fn command_refused(venue: Option<&str>, op: &str, message: &str, grouped: bool) {
    let event = if grouped {
        "order.leg_pre_dispatch_refused"
    } else {
        "order.pre_dispatch_refused"
    };
    let op = if ORDER_METHODS.contains(&op)
        || matches!(
            op,
            "order.submit"
                | "order.action"
                | "order.cancelQueued"
                | "order.actionCancelQueued"
                | "amend"
                | "cancel"
                | "cancelAll"
                | "nuke"
                | "place"
                | "cancelSymbol"
                | "setMarginType"
                | "setPositionMode"
                | "close"
                | "closeAll"
                | "stopAlgos"
                | "cancelAccountOrders"
                | "setStpMode"
                | "setAccountTier"
                | "algo"
        ) {
        op
    } else {
        "unknown"
    };
    let reason = match message {
        "not sent: invalid action binding" | "not sent: invalid order binding" => "invalid_binding",
        "not sent: unsupported action version" => "unsupported_version",
        "not sent: account not admitted for actions"
        | "not sent: account not admitted for order frames"
        | "Order submissions aren’t available on this account connection"
        | "Order changes aren’t available on this account connection" => "not_admitted",
        "not sent: invalid intent key" => "invalid_intent_key",
        "not sent: invalid key time" => "future_intent_key",
        "not sent: expired" => "expired_intent",
        "not sent: action fingerprint differs" | "not sent: intent fingerprint differs" => {
            "fingerprint_mismatch"
        }
        "unsupported action schema" => "invalid_action_schema",
        "unsupported action" => "invalid_action",
        "not sent: invalid amendment" => "invalid_amendment",
        "Invalid action queue cancellation binding" => "invalid_binding",
        "Invalid action queue cancellation" | "not sent: invalid queued cancellation" => {
            "invalid_queue_cancellation"
        }
        "not sent: invalid parameters" => "invalid_parameters",
        "not sent: client order id differs" => "client_order_id_mismatch",
        "not sent: algo account differs" => "algo_account_mismatch",
        "Algo host unavailable" | "Nuke is unavailable for this account" => "algo_unavailable",
        "not sent: invalid order frame" => "invalid_envelope",
        "not sent: action exceeds size limit"
        | "not sent: submit exceeds size limit"
        | "not sent: action queue cancellation exceeds size limit" => "frame_too_large",
        "Unauthenticated order rejected" => "unauthenticated",
        "Router lease expired; no new work was sent"
        | "The server connection expired; no new orders were sent" => "lease_fenced",
        "too many account reads in flight; retry shortly" => "capacity",
        "command IDs exhausted" => "command_ids_exhausted",
        "account is busy; command not sent" => "input_busy",
        crate::auth::TOKEN_EXPIRED => "auth_expired",
        "Action expired; no further action command was sent"
        | "This action expired before it could finish. Check your open orders and positions before trying again." => {
            "action_expired"
        }
        "Action registration retired; no further action command was sent"
        | "Action registration retired before dispatch"
        | "The account connection changed before this action could finish. Check your open orders and positions before trying again."
        | "This action couldn’t start because the account connection changed" => {
            "registration_retired"
        }
        "Algo operation cancelled-for-owner-change: authenticated connection binding changed"
        | "The automated-order request stopped because its account connection changed" => {
            "owner_changed"
        }
        crate::methods::NUKE_NOT_LOADED => "account_not_loaded",
        "restore_refused"
        | "runtime_disabled"
        | "pending_verification"
        | "account_missing"
        | "wrong_signing_mode"
        | "public_account"
        | "venue_disabled"
        | "io_fenced"
        | "holder_changed"
        | "registration_mismatch"
        | "draining" => message,
        _ => "pre_dispatch_refused",
    };
    let venue = venue.filter(|venue| safe_cex_venues::supported(venue));
    crate::state::Log::info(
        &json!({"event":event, "reason_class":reason,
        "venue":venue,"op":op})
        .to_string(),
    );
}

/// The audited fields of an engine command (as JSON), or `None` for a read.
pub fn command(command: &Value) -> Option<Map<String, Value>> {
    let op = command["op"]
        .as_str()
        .filter(|op| ENGINE_OPS.contains(op))?;
    let mut fields = pick(command, &COMMAND_FIELDS);
    fields.insert("op".into(), op.into());
    if op == "setAccountTier"
        && let Some(tier) = command["tier"]
            .as_str()
            .filter(|tier| matches!(*tier, "standard" | "premium" | "plus"))
    {
        fields.insert("tier".into(), tier.into());
    }
    if let Some(ids) = command["ids"].as_array() {
        fields.insert("ids".into(), ids.iter().filter_map(scalar).collect());
    }
    if let Some(orders) = command["orders"].as_array() {
        let orders = orders
            .iter()
            .map(|order| Value::Object(pick(order, &COMMAND_FIELDS)))
            .collect();
        fields.insert("orders".into(), Value::Array(orders));
    }
    Some(fields)
}

/// The audited fields of a hosted call refused before it became a command.
pub fn refused_call(method: &str, args: &[Value]) -> Map<String, Value> {
    let mut fields = match args.first() {
        Some(Value::Array(orders)) => [(
            "orders".to_owned(),
            orders
                .iter()
                .map(|order| Value::Object(pick(order, &COMMAND_FIELDS)))
                .collect(),
        )]
        .into_iter()
        .collect(),
        Some(first) => pick(first, &COMMAND_FIELDS),
        None => Map::new(),
    };
    fields.insert("op".into(), method.into());
    fields
}

/// The audited fields of a typed engine command an algo sent.
pub fn typed(command: &Command) -> Option<Map<String, Value>> {
    let json = match command {
        Command::PlaceOrders { orders, .. } => json!({"op": "placeOrders", "orders": orders}),
        Command::CancelOrders { ids, .. } => json!({"op": "cancelOrders", "ids": ids}),
        Command::CancelOrdersWithSymbols { orders, .. } => {
            let orders: Vec<Value> = orders
                .iter()
                .map(|order| json!({"id": order.id, "symbol": order.symbol}))
                .collect();
            json!({"op": "cancelOrdersWithSymbols", "orders": orders})
        }
        Command::AmendOrder {
            id, price, amount, ..
        } => json!({"op": "amendOrder", "id": id, "price": price, "amount": amount}),
        Command::AmendOrderWithSymbol {
            id,
            symbol,
            price,
            amount,
            ..
        } => {
            json!({"op": "amendOrderWithSymbol", "id": id, "symbol": symbol, "price": price, "amount": amount})
        }
        Command::CancelSymbolOrders { symbol, .. } => {
            json!({"op": "cancelSymbolOrders", "symbol": symbol})
        }
        Command::SetLeverage {
            symbol,
            leverage,
            side,
            ..
        } => json!({"op": "setLeverage", "symbol": symbol, "leverage": leverage, "side": side}),
        Command::SetMarginType {
            symbol, isolated, ..
        } => json!({"op": "setMarginType", "symbol": symbol, "isolated": isolated}),
        Command::SetPositionMode { hedged, symbol, .. } => {
            json!({"op": "setPositionMode", "hedged": hedged, "symbol": symbol})
        }
        Command::SetStpMode { mode, .. } => json!({"op": "setStpMode", "mode": mode}),
        _ => return None,
    };
    self::command(&json)
}

/// An algo payload's audited fields, including a TWAP's nested `options` and market symbol.
fn algo_fields(value: &Value) -> Map<String, Value> {
    let mut fields = pick(value, &ALGO_FIELDS);
    for nested in [&value["options"], &value["options"]["activeMarket"]] {
        for (key, field) in pick(nested, &ALGO_FIELDS) {
            fields.entry(key).or_insert(field);
        }
    }
    fields
}

/// The audited fields of an algo op (`chaser:start`, `twap:stop`, `exec`, ...).
/// `twap:state` restores TWAPs that then place orders, so each entry is listed.
pub fn algo_op(op: &str, data: &str) -> Map<String, Value> {
    let data: Value = serde_json::from_str(data).unwrap_or(Value::Null);
    let mut fields = match &data {
        Value::String(_) => [("id".to_owned(), scalar(&data).unwrap_or_default())]
            .into_iter()
            .collect(),
        Value::Array(entries) => [(
            "twaps".to_owned(),
            entries
                .iter()
                .map(|entry| Value::Object(algo_fields(entry)))
                .collect(),
        )]
        .into_iter()
        .collect(),
        _ => algo_fields(&data),
    };
    fields.insert("op".into(), op.into());
    fields
}

/// Closed native completion shape only; never retain values from the reply.
pub fn result_shape(result: &Value) -> &'static str {
    match result {
        Value::Array(ids) if ids.is_empty() => "empty_array",
        Value::Array(ids) if ids.len() == 1 && ids[0].is_string() => "single_id_array",
        Value::Array(_) => "array",
        Value::Object(fields) if fields.get("orderIds").is_some_and(Value::is_array) => {
            "order_ids_object"
        }
        Value::Object(_) => "object",
        Value::Null => "null",
        Value::String(_) => "string",
        Value::Bool(_) | Value::Number(_) => "scalar",
    }
}

/// `ok`, or the error's kind, message and outcome from a hosted failure detail.
pub fn result(outcome: &Result<Value, Value>) -> Value {
    match outcome {
        Ok(_) => "ok".into(),
        Err(detail) => Value::Object(error_fields(detail)),
    }
}

pub fn venue_result<T>(outcome: &Result<T, VenueError>) -> Value {
    match outcome {
        Ok(_) => "ok".into(),
        Err(error) => {
            let detail = serde_json::to_value(error).unwrap_or_default();
            Value::Object(error_fields(&detail))
        }
    }
}

/// The full line: who, where it came from (`socket:N/client:M`, `algo`, ...), what, and the result.
pub fn line(who: &Who, origin: &str, fields: &Map<String, Value>, result: Value) -> Value {
    let mut line = json!({
        "level": "info",
        "service": "exchange-server",
        "msg": "order audit",
        "user": who.user,
        "account": who.account,
        "venue": who.venue,
        "origin": origin,
    });
    for (key, value) in fields {
        line[key] = value.clone();
    }
    line["result"] = result;
    line
}

pub fn log(who: &Who, origin: &str, fields: &Map<String, Value>, result: Value) {
    let line = line(who, origin, fields, result).to_string();
    #[cfg(test)]
    CAPTURED.with(|lines| {
        if let Some(lines) = lines.borrow_mut().as_mut() {
            lines.push(line.clone());
        }
    });
    println!("{line}");
}

// Separate from `log_capture`, whose tests count their own lines exactly.
#[cfg(test)]
thread_local! {
    pub(crate) static CAPTURED: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Stage names in pipeline order, as they appear under `stages_us`.
pub const STAGES: [&str; 6] = [
    "received",
    "admitted",
    "queued",
    "sent",
    "responded",
    "replied",
];

/// Monotonic stamps for one audited command, in `STAGES` order.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stages {
    pub received: Option<Instant>,
    pub admitted: Option<Instant>,
    pub queued: Option<Instant>,
    pub sent: Option<Instant>,
    pub responded: Option<Instant>,
    pub replied: Option<Instant>,
}

impl Stages {
    /// Integer microseconds from the earliest stamp, keyed by stage name;
    /// a stage never observed is omitted rather than estimated.
    pub fn offsets(&self) -> Option<Value> {
        let stamps = [
            self.received,
            self.admitted,
            self.queued,
            self.sent,
            self.responded,
            self.replied,
        ];
        let origin = stamps.iter().flatten().min()?;
        let offsets = STAGES
            .iter()
            .zip(stamps)
            .filter_map(|(stage, stamp)| {
                let micros = stamp?.saturating_duration_since(*origin).as_micros();
                Some((
                    (*stage).to_owned(),
                    u64::try_from(micros).unwrap_or(u64::MAX).into(),
                ))
            })
            .collect();
        Some(Value::Object(offsets))
    }
}

static TYPED_SETTLED: AtomicU64 = AtomicU64::new(0);
static TYPED_UNKNOWN: AtomicU64 = AtomicU64::new(0);

/// Counts one typed `order.outcome`/`order.actionOutcome` as it is sent.
pub(crate) fn note_typed_outcome(outcome: &Value) {
    TYPED_SETTLED.fetch_add(1, Ordering::Relaxed);
    if outcome == "unknown" {
        TYPED_UNKNOWN.fetch_add(1, Ordering::Relaxed);
    }
}

/// Process-lifetime typed receipt counts for `/health`.
pub fn typed_outcomes() -> Value {
    json!({
        "settled": TYPED_SETTLED.load(Ordering::Relaxed),
        "unknown": TYPED_UNKNOWN.load(Ordering::Relaxed),
    })
}

// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
#[cfg(test)]
mod tests {
    #[test]
    fn pre_dispatch_refusal_audit_drops_all_untrusted_text_and_is_bounded() {
        let canary = "key-body-account-canary".repeat(1000);
        let capture = crate::state::log_capture::capture();
        super::pre_dispatch_refused(Some(&canary), &canary, &canary);
        super::pre_dispatch_refused(
            Some("bybit"),
            "amend",
            "not sent: action fingerprint differs",
        );
        let lines = capture.finish();
        assert_eq!(lines.len(), 2);
        for line in &lines {
            assert!(line.len() < 512);
            assert!(!line.contains("canary"));
        }
        let outer: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(outer["level"], "info");
        let fields: serde_json::Value =
            serde_json::from_str(outer["msg"].as_str().unwrap()).unwrap();
        assert_eq!(
            fields,
            json!({"event":"order.pre_dispatch_refused","venue":null,"op":"unknown","reason_class":"pre_dispatch_refused"})
        );
        let outer: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        let fields: serde_json::Value =
            serde_json::from_str(outer["msg"].as_str().unwrap()).unwrap();
        assert_eq!(
            fields,
            json!({"event":"order.pre_dispatch_refused","venue":"bybit","op":"amend","reason_class":"fingerprint_mismatch"})
        );
    }

    #[test]
    fn grouped_refusal_audit_is_closed_and_excludes_all_caller_values() {
        let canary = "private-id-body-canary".repeat(1000);
        let capture = crate::state::log_capture::capture();
        super::command_refused(Some(&canary), &canary, &canary, true);
        let lines = capture.finish();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].len() < 512 && !lines[0].contains("canary"));
        let outer: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(outer["level"], "info");
        let fields: serde_json::Value =
            serde_json::from_str(outer["msg"].as_str().unwrap()).unwrap();
        assert_eq!(
            fields,
            json!({"event":"order.leg_pre_dispatch_refused","venue":null,"op":"unknown","reason_class":"pre_dispatch_refused"})
        );
    }

    #[test]
    fn native_result_shape_is_closed_and_does_not_retain_reply_values() {
        for (value, expected) in [
            (json!([]), "empty_array"),
            (json!(["synthetic-private-order"]), "single_id_array"),
            (json!(["first", "second"]), "array"),
            (
                json!({"orderIds":["private"],"secret":"never retain"}),
                "order_ids_object",
            ),
            (json!({"secret":"never retain"}), "object"),
            (Value::Null, "null"),
            (json!("private"), "string"),
            (json!(true), "scalar"),
            (json!(5), "scalar"),
        ] {
            assert_eq!(super::result_shape(&value), expected);
        }
    }

    use super::*;

    #[test]
    fn delegated_native_writes_keep_allowlisted_audit_and_refusal_coverage() {
        let cancel =
            command(&json!({"op":"cancelAccountOrders", "credential":"not-for-the-journal"}))
                .unwrap();
        assert_eq!(Value::Object(cancel), json!({"op":"cancelAccountOrders"}));
        let tier = command(
            &json!({"op":"setAccountTier", "tier":"premium", "credential":"not-for-the-journal"}),
        )
        .unwrap();
        assert_eq!(
            Value::Object(tier),
            json!({"op":"setAccountTier", "tier":"premium"})
        );
        let malformed =
            command(&json!({"op":"setAccountTier", "tier":"unvalidated-text"})).unwrap();
        assert!(!malformed.contains_key("tier"));
        for method in [
            "delegatedCancelAccountOrders",
            "delegatedSetStpMode",
            "delegatedSetAccountTier",
        ] {
            assert!(ORDER_METHODS.contains(&method));
        }
    }

    #[test]
    fn a_dead_man_switch_is_audited_with_its_time() {
        let fields =
            command(&json!({"op":"scheduleCancel","command":3,"time":1_800_000_000_000_i64}))
                .expect("scheduleCancel cancels orders, so it is audited");
        assert_eq!(fields["op"], "scheduleCancel");
        assert_eq!(fields["time"], 1_800_000_000_000_i64);
        assert!(
            command(&json!({"op":"scheduleCancel","command":4})).is_some(),
            "a clear too"
        );
        assert!(ORDER_METHODS.contains(&"scheduleCancel"));
    }

    #[test]
    fn stage_offsets_are_integer_microseconds_from_the_first_stamp() {
        let origin = Instant::now();
        let at = |us: u64| Some(origin + std::time::Duration::from_micros(us));
        let stages = Stages {
            received: at(0),
            admitted: at(120),
            queued: at(135),
            sent: at(900),
            responded: at(45_000),
            replied: at(45_100),
        };
        let offsets = stages.offsets().expect("a stamped command has offsets");
        assert_eq!(
            offsets,
            json!({"received":0,"admitted":120,"queued":135,"sent":900,"responded":45000,"replied":45100})
        );
    }

    #[test]
    fn unobserved_stages_are_omitted_and_the_origin_falls_back_to_admission() {
        let origin = Instant::now();
        let at = |us: u64| Some(origin + std::time::Duration::from_micros(us));
        // No socket frame (received) and no native write identity (sent).
        let stages = Stages {
            admitted: at(0),
            queued: at(40),
            responded: at(3_000),
            replied: at(3_050),
            ..Stages::default()
        };
        assert_eq!(
            stages.offsets().unwrap(),
            json!({"admitted":0,"queued":40,"responded":3000,"replied":3050})
        );
        assert!(Stages::default().offsets().is_none());
    }

    #[test]
    fn a_stamped_line_stays_bounded_and_carries_only_integer_offsets() {
        let origin = Instant::now();
        let late = Some(origin + std::time::Duration::from_secs(86_400 * 365));
        let stages = Stages {
            received: Some(origin),
            admitted: late,
            queued: late,
            sent: late,
            responded: late,
            replied: late,
        };
        let mut fields =
            command(&json!({"op":"cancelOrders","command":3,"ids":["o-1"],"secret":"s"})).unwrap();
        fields.insert("stages_us".into(), stages.offsets().unwrap());
        let stamped_line = line(&who(), "socket:1/client:2", &fields, "ok".into());
        let stamped = stamped_line.to_string();
        let offsets = stamped_line["stages_us"].as_object().unwrap();
        assert_eq!(offsets.len(), STAGES.len());
        assert!(offsets.values().all(Value::is_u64));
        fields.remove("stages_us");
        let bare = line(&who(), "socket:1/client:2", &fields, "ok".into()).to_string();
        assert!(stamped.len() - bare.len() < 160, "stage fields stay small");
        assert!(!stamped.contains("secret"));
    }

    #[test]
    fn a_stamped_line_keeps_only_allowlisted_fields() {
        let mut fields = command(&json!({"op":"placeOrders","command":5,
            "orders":[{"symbol":"BTCUSDT","side":"buy","amount":1,"intentId":"i-1","signature":"sig"}],
            "stages_us":{"received":999},"intentId":"i-1","requestId":"r-1","key":"pub-key"}))
        .unwrap();
        assert!(
            !fields.contains_key("stages_us"),
            "a caller cannot supply stages"
        );
        let stages = Stages {
            received: Some(Instant::now()),
            ..Stages::default()
        };
        fields.insert("stages_us".into(), stages.offsets().unwrap());
        let stamped = line(&who(), "socket:1/client:2", &fields, "ok".into());
        let mut keys: Vec<&str> = stamped
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "account",
                "level",
                "msg",
                "op",
                "orders",
                "origin",
                "result",
                "service",
                "stages_us",
                "user",
                "venue",
            ]
        );
        assert_eq!(stamped["stages_us"], json!({"received":0}));
        let mut order: Vec<&str> = stamped["orders"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        order.sort_unstable();
        assert_eq!(order, ["amount", "side", "symbol"]);
    }

    #[test]
    fn typed_outcomes_count_settled_and_unknown_receipts() {
        let before = typed_outcomes();
        for outcome in ["placed", "applied", "refused", "unknown", "unknown"] {
            note_typed_outcome(&json!(outcome));
        }
        let after = typed_outcomes();
        let delta = |key: &str| after[key].as_u64().unwrap() - before[key].as_u64().unwrap();
        // Other tests settle receipts concurrently; this test's own five are a floor.
        assert!(delta("settled") >= 5);
        assert!(delta("unknown") >= 2);
    }

    fn who() -> Who {
        Who {
            user: "user-1".into(),
            account: "bear5".into(),
            venue: "bybit".into(),
        }
    }

    #[test]
    fn a_placement_line_carries_who_what_where_and_the_result() {
        let command = json!({"op": "placeOrders", "command": 7, "orders": [{
            "symbol": "BTCUSDT", "side": "buy", "type": "limit", "amount": 0.01,
            "price": 60000.5, "clientOrderId": "ts-abc", "key": "pub-key", "secret": "s"
        }]});
        let fields = super::command(&command).unwrap();
        let line = line(
            &who(),
            "socket:3/client:9",
            &fields,
            result(&Ok(Value::Null)),
        );
        assert_eq!(line["msg"], "order audit");
        assert_eq!(line["level"], "info");
        assert_eq!(line["user"], "user-1");
        assert_eq!(line["account"], "bear5");
        assert_eq!(line["venue"], "bybit");
        assert_eq!(line["origin"], "socket:3/client:9");
        assert_eq!(line["op"], "placeOrders");
        assert_eq!(line["result"], "ok");
        let order = &line["orders"][0];
        assert_eq!(order["symbol"], "BTCUSDT");
        assert_eq!(order["side"], "buy");
        assert_eq!(order["amount"], 0.01);
        assert_eq!(order["price"], 60000.5);
        assert_eq!(order["clientOrderId"], "ts-abc");
        let text = line.to_string();
        assert!(!text.contains("pub-key") && !text.contains("secret"));
    }

    #[test]
    fn a_failure_carries_its_kind_and_outcome_but_reads_are_not_audited() {
        let fields =
            super::command(&json!({"op": "setLeverage", "symbol": "ETHUSDT", "leverage": 5.0}))
                .unwrap();
        let failed = Err(
            json!({"name": "ExchangeError", "message": "leverage not modified", "outcome": "unknown", "signature": "sig"}),
        );
        let line = line(&who(), "socket:1/client:2", &fields, result(&failed));
        assert_eq!(line["leverage"], 5.0);
        assert_eq!(line["result"]["name"], "ExchangeError");
        assert_eq!(line["result"]["outcome"], "unknown");
        assert!(line["result"].get("signature").is_none());
        assert!(super::command(&json!({"op": "fetchAccount", "command": 1})).is_none());
    }

    #[test]
    fn a_refused_call_still_logs_what_was_asked() {
        let order = json!({"symbol": "BTCUSDT", "side": "buy", "amount": 0.001, "price": 60000, "passphrase": "p"});
        let fields = refused_call("placeOrderWithClientOrderId", &[order]);
        assert_eq!(fields["op"], "placeOrderWithClientOrderId");
        assert_eq!(fields["symbol"], "BTCUSDT");
        assert!(fields.get("passphrase").is_none());
    }

    #[test]
    fn algo_commands_and_ops_are_audited() {
        let place: Command = safe_cex_engine::engine::Command::CancelOrders {
            command: 4,
            ids: vec!["o-1".into()],
        };
        let fields = typed(&place).unwrap();
        assert_eq!(fields["op"], "cancelOrders");
        assert_eq!(fields["ids"], json!(["o-1"]));
        let start = algo_op(
            "chaser:start",
            r#"{"symbol":"BTCUSDT","size":0.5,"side":"sell","maxAttempts":10,"clientId":"c-1","accountName":"x"}"#,
        );
        assert_eq!(start["op"], "chaser:start");
        assert_eq!(start["symbol"], "BTCUSDT");
        assert_eq!(start["size"], 0.5);
        assert_eq!(start["clientId"], "c-1");
        assert!(start.get("accountName").is_none());
        assert_eq!(algo_op("chaser:terminate", r#""chase-7""#)["id"], "chase-7");
    }

    #[test]
    fn a_twap_state_restore_lists_each_restored_twap() {
        let state = r#"[{"id":"twap-1","options":{"accountName":"bear5","activeMarket":{"symbol":"BTCUSDT"},"size":0.3,"duration":30,"ordersCount":10,"side":"buy"},"state":{}}]"#;
        let fields = algo_op("twap:state", state);
        assert_eq!(fields["op"], "twap:state");
        let twap = &fields["twaps"][0];
        assert_eq!(twap["id"], "twap-1");
        assert_eq!(twap["symbol"], "BTCUSDT");
        assert_eq!(twap["side"], "buy");
        assert_eq!(twap["size"], 0.3);
        assert!(twap.get("accountName").is_none());
    }
}
