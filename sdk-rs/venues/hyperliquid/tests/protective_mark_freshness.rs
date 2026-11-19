//! AGT-47: a live symbol's quiet mark must not refuse a protective edit.
//! `fastAssetCtxs` diffs omit an unchanged mark; a stale mark is re-read once.
mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::model::StopTriggerType::MarkPrice;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, SocketId};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::{Value, json};
use support::*;

const STALE: &str =
    "Hyperliquid requires a fresh mark price before editing protection; order unchanged";

fn take_profit() -> Value {
    json!({"coin":"BTC","oid":9,"side":"A","limitPx":"70","triggerPx":"120",
        "origSz":"1","sz":"1","reduceOnly":true,"orderType":"Take Profit Market",
        "isTrigger":true,"isPositionTpsl":false,"cloid":"0x11111111111111111111111111111111"})
}
fn setup() -> (Engine<Hyperliquid>, SocketId, Value) {
    let (mut e, s) = ready("disabled");
    let row = take_profit();
    let frame =
        json!({"channel":"orderUpdates","data":[{"order":row,"status":"open"}]}).to_string();
    message(&mut e, s, 1500., &frame);
    prepared_now(&mut e);
    (e, s, row)
}
fn contexts(e: &mut Engine<Hyperliquid>, socket: SocketId, at: f64, row: Value) {
    use base64::Engine as _;
    let zipped =
        miniz_oxide::deflate::compress_to_vec(json!({"BTC": row}).to_string().as_bytes(), 6);
    let data = base64::engine::general_purpose::STANDARD.encode(zipped);
    let text = json!({"channel":"fastAssetCtxs","data":data}).to_string();
    message(e, socket, at, &text);
}
fn info(actions: &[Action], kind_name: &str) -> Option<HttpRequest> {
    actions.iter().find_map(|a| match a {
        Action::Http(r) if r.url.ends_with("/info") && kind(r) == kind_name => Some(r.clone()),
        _ => None,
    })
}
fn sends(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
}
/// Amend the TP at `at` and answer its sizing read; the actions that follow.
fn amend_and_size(e: &mut Engine<Hyperliquid>, at: f64, row: &Value) -> Vec<Action> {
    e.handle(
        at,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: "9".into(),
            price: Some(125.),
            amount: None,
        }),
    );
    let read = info(&prepared_now(e), "orderStatus").expect("fresh sizing read");
    let text = json!({"status":"order","order":{"order":row,"status":"open"}}).to_string();
    reply(e, &read, 200, &text);
    prepared_now(e)
}

#[test]
fn quiet_mark_on_a_streaming_symbol_never_refuses_a_partial_tp_edit() {
    let (mut e, s, row) = setup();
    contexts(&mut e, s, 2_000., json!({"markPx":"100","midPx":"100"}));
    // The mark holds while mids move: every later diff omits it.
    for (at, mid) in [(4_000., "100.5"), (6_000., "101"), (8_000., "100.7")] {
        contexts(&mut e, s, at, json!({"midPx": mid}));
    }
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(8_000.)
    );
    let actions = amend_and_size(&mut e, 8_500., &row);
    assert!(
        info(&actions, "activeAssetData").is_none(),
        "a confirmed mark needs no read"
    );
    assert!(
        sends(&actions),
        "the guarded cancel is released, not refused"
    );
}

#[test]
fn omission_confirms_only_this_sockets_own_unchanged_mark() {
    // A mark moved by another source is not what the diff stream last sent.
    let (mut e, s, _) = setup();
    contexts(&mut e, s, 2_000., json!({"markPx":"100"}));
    e.store_mut().update_ticker("BTC-PERP", |t| t.mark = 99.);
    contexts(&mut e, s, 3_000., json!({"midPx":"100"}));
    assert!(e.store().reference_seen_at("BTC-PERP", MarkPrice) < Some(3_000.));

    // An unreadable diff may have carried a change.
    let (mut e, s, _) = setup();
    contexts(&mut e, s, 2_000., json!({"markPx":"100"}));
    message(
        &mut e,
        s,
        2_500.,
        r#"{"channel":"fastAssetCtxs","data":"not-base64"}"#,
    );
    contexts(&mut e, s, 3_000., json!({"midPx":"100"}));
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );

    // An explicit null is not an omission.
    let (mut e, s, _) = setup();
    contexts(&mut e, s, 2_000., json!({"markPx":"100"}));
    contexts(&mut e, s, 2_500., json!({"markPx":null}));
    contexts(&mut e, s, 3_000., json!({"midPx":"100"}));
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );

    // A new socket's diffs cannot inherit the old socket's lineage.
    let (mut e, s, _) = setup();
    contexts(&mut e, s, 2_000., json!({"markPx":"100"}));
    let (fresh, at) = reconnect(&mut e, s, 2_500.);
    contexts(&mut e, fresh, at + 1., json!({"midPx":"100"}));
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );
    contexts(&mut e, fresh, at + 2., json!({"markPx":"100"}));
    contexts(&mut e, fresh, at + 3., json!({"midPx":"100"}));
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(at + 3.)
    );
}

#[test]
fn genuinely_stale_mark_is_read_on_demand_before_the_cancel() {
    let (mut e, s, row) = setup();
    contexts(&mut e, s, 2_000., json!({"markPx":"100"}));
    let actions = amend_and_size(&mut e, 9_000., &row);
    assert!(
        !sends(&actions),
        "nothing is canceled before the mark is fresh"
    );
    let read = info(&actions, "activeAssetData").expect("one bounded mark read");
    let body: Value = serde_json::from_str(read.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["coin"], "BTC");
    reply(
        &mut e,
        &read,
        200,
        r#"{"user":"0x0","coin":"BTC","leverage":{"type":"cross","value":20},"markPx":"101.5"}"#,
    );
    let actions = prepared_now(&mut e);
    assert_eq!(e.store().ticker("BTC-PERP").unwrap().mark, 101.5);
    assert!(
        sends(&actions),
        "the fresh mark releases the guarded cancel"
    );
}

#[test]
fn an_unanswered_or_unreadable_mark_read_refuses_with_the_order_unchanged() {
    for answer in [
        Some((200, r#"{"coin":"ETH","markPx":"1"}"#)),
        Some((500, "")),
        None,
    ] {
        let (mut e, s, row) = setup();
        contexts(&mut e, s, 2_000., json!({"markPx":"100"}));
        let actions = amend_and_size(&mut e, 9_000., &row);
        let read = info(&actions, "activeAssetData").expect("one bounded mark read");
        let actions = match answer {
            Some((status, body)) => {
                reply(&mut e, &read, status, body);
                prepared_now(&mut e)
            }
            // The amend's own bound ends the wait.
            None => advance_packet_timers(&mut e, actions, 9_000. + 10_000.),
        };
        assert!(!sends(&actions));
        let refused = actions.iter().any(|a| {
            matches!(a, Action::Notice(Notice::CommandFailed { command: 90, error })
                if error.message.contains(STALE))
        });
        assert!(refused, "{answer:?}: {actions:?}");
        assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
    }
}
