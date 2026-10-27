//! Account-fraction and cancel/recreate sizing regressions from the port review.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn ready_unattributed() -> (Engine<Lighter>, safe_cex_engine::SocketId) {
    ready_with(unattributed())
}
/// The private socket's first open is the test's own resync.
fn unopened_unattributed() -> (Engine<Lighter>, safe_cex_engine::SocketId) {
    ready_unopened_meta(unattributed(), META)
}
fn unattributed() -> Options {
    let mut options = options();
    options.testnet = true;
    options
}
fn http(e: &mut Engine<Lighter>, path: &str) -> HttpRequest {
    let actions = safe_cex_engine::test_support::prepared_actions(e, NOW);
    let debug = format!("{actions:?}");
    actions
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains(path) => Some(request),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing request {path}: {debug}"))
}
fn wire(request: &HttpRequest) -> Value {
    let encoded = request
        .body
        .as_deref()
        .unwrap()
        .split('&')
        .find_map(|part| part.strip_prefix("tx_info="))
        .unwrap();
    let mut bytes = encoded.bytes();
    let mut decoded = Vec::new();
    while let Some(byte) = bytes.next() {
        decoded.push(if byte == b'%' {
            let pair = [bytes.next().unwrap(), bytes.next().unwrap()];
            u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap()
        } else if byte == b'+' {
            b' '
        } else {
            byte
        });
    }
    serde_json::from_slice(&decoded).unwrap()
}
fn nonce(e: &mut Engine<Lighter>) -> HttpRequest {
    let nonce = http(e, "nextNonce");
    reply(e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    http(e, "sendTx")
}
fn ack(e: &mut Engine<Lighter>, request: &HttpRequest) {
    reply(
        e,
        request,
        200,
        r#"{"code":200,"tx_hash":"transaction-hash"}"#,
    );
}

#[test]
fn position_fraction_reads_percent_strings_and_tick_integers_on_rest_and_ws() {
    // "1.00" is 1% (100x, clamped to the 20x market max), never a 1x ratio.
    for (fraction, leverage) in [
        (json!("20.00"), 5.0),
        (json!(2000), 5.0),
        (json!("1.00"), 20.0),
        (json!("0.50"), 20.0),
        // A percent above 100 is impossible, so it is ticks; a JSON float is a percent.
        (json!("2000"), 5.0),
        (json!(20.0), 5.0),
    ] {
        for websocket in [false, true] {
            let (mut e, socket) = if websocket {
                ready_unattributed()
            } else {
                unopened_unattributed()
            };
            if websocket {
                message(
                    &mut e,
                    socket,
                    NOW + 1.0,
                    &json!({
                        "channel":"account_all:718895", "positions":{"1":{
                            "market_id":1, "position":"2", "avg_entry_price":"100",
                            "initial_margin_fraction":fraction, "margin_mode":0
                        }}
                    })
                    .to_string(),
                );
            } else {
                e.handle(NOW + 1.0, Event::WsOpened { socket });
                let reads = requests(&mut e);
                let account = reads.iter().find(|r| r.url.contains("/account?")).unwrap();
                let mut value: Value = serde_json::from_str(ACCOUNT).unwrap();
                value["accounts"][0]["positions"][0]["initial_margin_fraction"] = fraction.clone();
                reply_at(&mut e, NOW + 2.0, account, 200, &value.to_string());
            }
            assert_eq!(
                e.store().positions()[0].leverage,
                leverage,
                "{fraction} WS={websocket}"
            );
            assert_eq!(e.store().setting("ETH").unwrap().leverage, leverage);
            // Public market risk metadata still uses fixed 1/10,000 ticks.
            assert_eq!(e.store().market("ETH").unwrap().limits.leverage.max, 20.0);
        }
    }
}

#[test]
fn margin_mode_change_signs_current_five_x_from_percent_position() {
    let (mut e, socket) = ready_unattributed();
    message(
        &mut e,
        socket,
        NOW,
        r#"{"channel":"account_all:718895","positions":{"1":{"market_id":1,"position":"2","initial_margin_fraction":"20.00","margin_mode":0}}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 8,
            symbol: "ETH".into(),
            isolated: true,
        }),
    );
    let write = nonce(&mut e);
    let payload = wire(&write);
    assert_eq!(payload["InitialMarginFraction"], 2000);
    assert_eq!(payload["MarginMode"], 1);
    ack(&mut e, &write);
    assert_eq!(e.store().setting("ETH").unwrap().leverage, 5.0);
    assert_eq!(
        e.store().setting("ETH").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
}

#[test]
fn price_amend_recreates_only_unfilled_quantity_and_explicit_size_stays_requested() {
    for (amount, expected) in [(None, 1250), (Some(0.5), 500)] {
        let (mut e, _) = ready_unattributed();
        e.store_mut().update_order("42", |order| {
            order.amount = 2.0;
            order.filled = 0.75;
            order.remaining = 1.25;
        });
        e.handle(
            NOW,
            Event::Command(Command::AmendOrder {
                command: 8,
                id: "42".into(),
                price: Some(92.0),
                amount,
            }),
        );
        let cancel = nonce(&mut e);
        ack(&mut e, &cancel);
        let timer = e
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if at_ms < NOW + 500.0 => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(NOW + 100.0, Event::TimerFired { timer });
        let replacement = http(&mut e, "sendTx");
        let payload = wire(&replacement);
        assert_eq!(payload["BaseAmount"], expected);
        assert_eq!(payload["Price"], 9200);
        ack(&mut e, &replacement);
        assert!(e.store().order("42").is_none());
        assert_eq!(
            e.store()
                .orders()
                .iter()
                .find(|order| order.id != "42")
                .unwrap()
                .amount,
            expected as f64 / 1000.0
        );
    }
}

#[test]
fn invalid_leverage_bounds_reject_setting_write_and_preserve_account_state() {
    for (min, max) in [(30.0, 20.0), (f64::NAN, 20.0), (1.0, 0.0)] {
        let (mut e, socket) = unopened_unattributed();
        let before_positions = e.store().positions().to_vec();
        let before_setting = e.store().setting("ETH").unwrap();
        e.store_mut().update_markets(|market| {
            if market.symbol == "ETH" {
                market.limits.leverage.min = min;
                market.limits.leverage.max = max;
            }
        });
        // Invalid read metadata may never wipe the last valid position snapshot.
        e.handle(NOW + 1.0, Event::WsOpened { socket });
        let reads = requests(&mut e);
        let account = reads.iter().find(|r| r.url.contains("/account?")).unwrap();
        reply_at(&mut e, NOW + 2.0, account, 200, ACCOUNT);
        assert_eq!(e.store().positions(), before_positions);
        assert_eq!(e.store().setting("ETH").unwrap(), before_setting);
        // The same validation also protects a settings write before signing.
        e.drain_actions().for_each(drop);
        e.handle(
            NOW + 3.0,
            Event::Command(Command::SetLeverage {
                command: 8,
                symbol: "ETH".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        let nonce = http(&mut e, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("sendTx"))
        ));
        assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 8, error }) if error.message.contains("leverage limits"))));
        assert_eq!(e.store().positions(), before_positions);
    }
}
