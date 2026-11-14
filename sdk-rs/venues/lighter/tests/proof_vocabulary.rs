//! Documented native spellings must preserve the guarded amendment contract.
mod support;

use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

const CANCEL_REASONS: &[&str] = &[
    "canceled-post-only",
    "canceled-reduce-only",
    "canceled-position-not-allowed",
    "canceled-margin-not-allowed",
    "canceled-too-much-slippage",
    "canceled-not-enough-liquidity",
    "canceled-self-trade",
    "canceled-expired",
    "canceled-oco",
    "canceled-child",
    "canceled-liquidation",
    "canceled-invalid-balance",
];

fn original(kind: u8) -> Value {
    let trigger = if kind == 0 {
        0
    } else if kind < 4 {
        80
    } else {
        120
    };
    let price = if kind == 0 { 90 } else { trigger - 1 };
    json!({"order_index":"42","client_order_index":"123","market_id":1,
        "status":"open","type":kind,"time_in_force":if matches!(kind, 2 | 4) {0} else {1},
        "is_ask":true,"reduce_only":true,"initial_base_amount":"2",
        "filled_base_amount":"0","filled_quote_amount":"0","remaining_base_amount":"2",
        "price":price.to_string(),"trigger_price":trigger.to_string(),
        "order_expiry":NOW as u64+1_000_000,"trigger_status":"untriggered",
        "parent_order_index":0,"parent_order_id":"0","to_trigger_order_id_0":"0",
        "to_trigger_order_id_1":"0","to_cancel_order_id_0":"0","order_flags":0})
}

fn boot(row: &Value, robinhood: bool) -> (Engine<Lighter>, Timers) {
    let (mut engine, feeds, timers) = ready_feeds(
        Options {
            testnet: true,
            robinhood,
            ..options()
        },
        true,
    );
    engine.store_mut().set_orders(vec![]);
    message(
        &mut engine,
        feeds.private,
        NOW,
        &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
    );
    message(
        &mut engine,
        feeds.public,
        NOW,
        r#"{"channel":"market_stats/all","market_stats":[{"market_id":1,"mark_price":"100"}]}"#,
    );
    drain_at(&mut engine, &mut vec![], NOW);
    (engine, timers)
}

fn amend(engine: &mut Engine<Lighter>, row: &Value) {
    let trigger: f64 = row["trigger_price"].as_str().unwrap().parse().unwrap();
    engine.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(if trigger == 0.0 { 95.0 } else { trigger + 5.0 }),
            amount: Some(1.0),
        }),
    );
}

fn request(engine: &mut Engine<Lighter>, now: f64, path: &str) -> HttpRequest {
    let actions = safe_cex_engine::test_support::prepared_actions(engine, now);
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains(path) => Some(request.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}: {actions:?}"))
}

fn start_write(engine: &mut Engine<Lighter>, row: &Value) -> HttpRequest {
    amend(engine, row);
    let nonce = request(engine, NOW, "nextNonce");
    reply(engine, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    request(engine, NOW, "sendTx")
}

fn acknowledge(engine: &mut Engine<Lighter>, request: &HttpRequest, now: f64) {
    reply_at(
        engine,
        now,
        request,
        200,
        r#"{"code":200,"tx_hash":"accepted"}"#,
    );
}

fn proof_request(engine: &mut Engine<Lighter>, timers: &mut Timers, now: f64) -> HttpRequest {
    let actions = advance(engine, timers, now);
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("accountOrders") => Some(request.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing proof read: {actions:?}"))
}

fn answer(
    engine: &mut Engine<Lighter>,
    request: &HttpRequest,
    now: f64,
    rows: Vec<Value>,
) -> Vec<Action> {
    reply_at(
        engine,
        now,
        request,
        200,
        &json!({"code":200,"orders":rows}).to_string(),
    );
    safe_cex_engine::test_support::prepared_actions(engine, now)
}

fn has_write(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|action| matches!(action, Action::Http(r) if r.url.contains("sendTx")))
}

fn cancel_actions(rows: Vec<Value>) -> Vec<Action> {
    let row = original(2);
    let (mut engine, mut timers) = boot(&row, false);
    let cancel = start_write(&mut engine, &row);
    acknowledge(&mut engine, &cancel, NOW);
    let proof = proof_request(&mut engine, &mut timers, NOW + 100.0);
    assert!(proof.url.contains("client_order_indexes=123"));
    answer(&mut engine, &proof, NOW + 100.0, rows)
}

fn tx(request: &HttpRequest) -> Value {
    let raw = request
        .body
        .as_ref()
        .unwrap()
        .split('&')
        .find_map(|part| part.strip_prefix("tx_info="))
        .unwrap();
    let mut decoded = Vec::new();
    let mut bytes = raw.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let pair = [bytes.next().unwrap(), bytes.next().unwrap()];
            decoded.push(u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap());
        } else {
            decoded.push(byte);
        }
    }
    serde_json::from_slice(&decoded).unwrap()
}

fn restoration_actions(row: Value, mutate: impl FnOnce(&mut Value)) -> Vec<Action> {
    let (mut engine, mut timers) = boot(&row, false);
    let cancel = start_write(&mut engine, &row);
    acknowledge(&mut engine, &cancel, NOW);
    let proof = proof_request(&mut engine, &mut timers, NOW + 100.0);
    let mut terminal = row.clone();
    terminal["status"] = json!("canceled");
    reply_at(
        &mut engine,
        NOW + 100.0,
        &proof,
        200,
        &json!({"code":200,"orders":[terminal]}).to_string(),
    );
    let replacement = request(&mut engine, NOW + 100.0, "sendTx");
    reply_at(
        &mut engine,
        NOW + 100.0,
        &replacement,
        200,
        r#"{"code":21702,"message":"replacement refused"}"#,
    );
    let nonce = request(&mut engine, NOW + 100.0, "nextNonce");
    reply_at(
        &mut engine,
        NOW + 100.0,
        &nonce,
        200,
        r#"{"code":200,"nonce":20}"#,
    );
    let putback = request(&mut engine, NOW + 100.0, "sendTx");
    let wire = tx(&putback);
    assert_eq!(wire["BaseAmount"], 2000);
    assert_eq!(wire["Type"], row["type"]);
    assert_eq!(wire["TimeInForce"], row["time_in_force"]);
    assert_eq!(wire["OrderExpiry"], row["order_expiry"]);
    acknowledge(&mut engine, &putback, NOW + 100.0);
    let proof = proof_request(&mut engine, &mut timers, NOW + 200.0);
    let mut restored = row;
    restored["order_index"] = json!("142");
    restored["client_order_index"] = wire["ClientOrderIndex"].clone();
    mutate(&mut restored);
    answer(&mut engine, &proof, NOW + 200.0, vec![restored])
}

fn is_restored(actions: &[Action]) -> bool {
    actions.iter().any(|action| {
        matches!(action,
        Action::Notice(Notice::CommandFailed { command: 8, error })
        if error.protection == Some(safe_cex_engine::error::Protection::Restored))
    })
}

#[test]
fn documented_hyphenated_sources_modify_the_exact_native_stop_in_both_venues() {
    for robinhood in [false, true] {
        for (kind, text) in [
            (2, "stop-loss"),
            (3, "stop-loss-limit"),
            (4, "take-profit"),
            (5, "take-profit-limit"),
        ] {
            let mut row = original(kind);
            row["type"] = json!(text);
            row["trigger_status"] = json!("mark-price");
            let (execution_price, modified_price) = match kind {
                2 => ("79.2", 8415),
                3 => ("79", 7900),
                4 => ("118.8", 12375),
                5 => ("119", 11900),
                _ => unreachable!(),
            };
            row["price"] = json!(execution_price);
            let (mut engine, mut timers) = boot(&row, robinhood);
            let modify = start_write(&mut engine, &row);
            assert!(
                modify
                    .body
                    .as_ref()
                    .unwrap()
                    .split('&')
                    .any(|part| part == "tx_type=17")
            );
            assert_eq!(tx(&modify)["Index"], 42);
            assert_eq!(tx(&modify)["BaseAmount"], 1000);
            assert_eq!(tx(&modify)["Price"], modified_price);
            acknowledge(&mut engine, &modify, NOW);
            let actions = advance(&mut engine, &mut timers, NOW + 1000.0);
            assert!(!has_write(&actions), "{text}: {actions:?}");
            assert!(
                actions.iter().any(|action| matches!(
                    action,
                    Action::Notice(Notice::CommandDone { command: 8, .. })
                )),
                "{text}: {actions:?}"
            );
        }
    }
}

#[test]
fn documented_symbolic_tif_sources_allow_protected_closing_limit_edits() {
    for text in ["immediate-or-cancel", "good-till-time", "post-only"] {
        let mut row = original(0);
        row["time_in_force"] = json!(text);
        let (mut engine, _) = boot(&row, false);
        let cancel = start_write(&mut engine, &row);
        assert!(cancel.url.contains("sendTx"), "{text}");
    }
}

#[test]
fn every_documented_reason_qualified_cancel_proves_the_exact_zero_fill_target() {
    for status in CANCEL_REASONS {
        let mut row = original(2);
        row["status"] = json!(status);
        let actions = cancel_actions(vec![row]);
        assert!(has_write(&actions), "{status}: {actions:?}");
    }
}

#[test]
fn hyphenated_putback_types_prove_the_original_market_stop_parameters() {
    for (kind, text) in [(2, "stop-loss"), (4, "take-profit")] {
        let actions = restoration_actions(original(kind), |row| row["type"] = json!(text));
        assert!(is_restored(&actions), "{text}: {actions:?}");
        assert!(!has_write(&actions));
    }
}

#[test]
fn symbolic_putback_tif_proves_each_exact_original_numeric_tif() {
    for (code, text) in [
        (0, "immediate-or-cancel"),
        (1, "good-till-time"),
        (2, "post-only"),
    ] {
        let mut row = original(0);
        row["time_in_force"] = json!(code);
        let actions = restoration_actions(row, |row| row["time_in_force"] = json!(text));
        assert!(is_restored(&actions), "{text}: {actions:?}");
        assert!(!has_write(&actions));
    }
}

#[test]
fn unknown_or_nonterminal_cancel_spellings_never_authorize_replacement() {
    for status in [
        "canceled-unknown",
        "canceled-",
        "canceled_post_only",
        "cancelled-post-only",
        "CANCELED-POST-ONLY",
        "expired",
        "rejected",
        "filled",
        "open",
        "pending",
        "in-progress",
        "",
    ] {
        let mut row = original(2);
        row["status"] = json!(status);
        let actions = cancel_actions(vec![row]);
        assert!(!has_write(&actions), "{status}: {actions:?}");
    }
}

#[test]
fn documented_cancel_reason_does_not_relax_identity_fill_or_cardinality() {
    for (field, value) in [
        ("order_index", json!("43")),
        ("market_id", json!(2)),
        ("client_order_index", json!("124")),
        ("initial_base_amount", json!("3")),
        ("filled_base_amount", json!("0.1")),
        ("filled_quote_amount", json!("1")),
    ] {
        let mut row = original(2);
        row["status"] = json!("canceled-post-only");
        row[field] = value;
        let actions = cancel_actions(vec![row]);
        assert!(!has_write(&actions), "{field}: {actions:?}");
    }
    let mut row = original(2);
    row["status"] = json!("canceled-post-only");
    assert!(!has_write(&cancel_actions(vec![])));
    assert!(!has_write(&cancel_actions(vec![row.clone(), row])));
}

#[test]
fn undocumented_source_types_and_tif_still_refuse_before_any_write() {
    for (field, value) in [
        ("type", json!("stop-loss-extra")),
        ("type", json!("twap")),
        ("type", json!("market")),
        ("time_in_force", json!("Unknown")),
        ("time_in_force", json!("post_only")),
        ("time_in_force", json!("GTC")),
        ("time_in_force", json!(3)),
        ("time_in_force", json!(null)),
    ] {
        let mut row = original(2);
        row[field] = value;
        let (mut engine, _) = boot(&row, false);
        amend(&mut engine, &row);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "{field}: {actions:?}"
        );
    }
}

#[test]
fn putback_aliases_do_not_relax_native_kind_side_trigger_amount_or_standalone_proof() {
    for (field, value) in [
        ("type", json!("take-profit")),
        ("type", json!("stop-loss-limit")),
        ("type", json!("stop-loss-extra")),
        ("is_ask", json!(false)),
        ("reduce_only", json!(false)),
        ("market_id", json!(2)),
        ("trigger_price", json!("81")),
        ("price", json!("78")),
        ("initial_base_amount", json!("3")),
        ("remaining_base_amount", json!("1")),
        ("filled_base_amount", json!("0.1")),
        ("filled_quote_amount", json!("1")),
        ("time_in_force", json!("good-till-time")),
        ("time_in_force", json!("Unknown")),
        ("order_expiry", json!(0)),
        ("order_index", json!(0)),
        ("client_order_index", json!(1)),
        ("trigger_status", json!("parent-order")),
        ("parent_order_index", json!(1)),
        ("order_flags", json!(1)),
    ] {
        let actions = restoration_actions(original(2), |row| {
            row["type"] = json!("stop-loss");
            row["time_in_force"] = json!("immediate-or-cancel");
            row[field] = value;
        });
        assert!(!is_restored(&actions), "{field}: {actions:?}");
        assert!(!has_write(&actions), "{field}: {actions:?}");
    }
}

#[test]
fn existing_numeric_and_underscore_vocabulary_remains_a_working_control() {
    for kind in [json!(2), json!("2"), json!("stop_loss")] {
        let mut row = original(2);
        row["type"] = kind;
        row["trigger_status"] = json!("mark-price");
        let (mut engine, _) = boot(&row, false);
        assert!(start_write(&mut engine, &row).url.contains("sendTx"));
    }
    for status in ["canceled", "cancelled"] {
        let mut row = original(2);
        row["status"] = json!(status);
        assert!(has_write(&cancel_actions(vec![row])));
    }
    assert!(is_restored(&restoration_actions(original(2), |_| {})));
}
