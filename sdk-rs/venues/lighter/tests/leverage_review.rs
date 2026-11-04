//! Account-fraction and cancel/recreate sizing regressions from the port review.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn ready_unattributed() -> (Engine<Lighter>, safe_cex_engine::SocketId) {
    let mut options = options();
    options.testnet = true;
    ready_with(options)
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
            let (mut e, socket) = ready_unattributed();
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
fn an_amend_recreates_only_what_is_left_of_the_total() {
    for (amount, expected) in [(None, 1250), (Some(1.25), 500)] {
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
        let (mut e, socket) = ready_unattributed();
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

/// lighter-robinhood's shape: every market's default fraction (2x) sits far
/// below its maximum (min 500 = 20x). ETH has an account row at 10x; BTC none.
const DEFAULTS_META: &str = r#"{"code":200,"order_book_details":[{"market_id":1,"symbol":"ETH","status":"active","market_type":"perp","supported_size_decimals":3,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"1","min_initial_margin_fraction":500,"default_initial_margin_fraction":666,"maintenance_margin_fraction":250},{"market_id":2,"symbol":"BTC","status":"active","market_type":"perp","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"1","min_initial_margin_fraction":500,"default_initial_margin_fraction":5000,"maintenance_margin_fraction":250}]}"#;

fn ready_with_defaults() -> (Engine<Lighter>, safe_cex_engine::SocketId) {
    let mut options = options();
    options.testnet = true;
    ready_with_meta(options, DEFAULTS_META)
}

/// daxx on preview-next: lighter-robinhood showed 20x for a market the account
/// never set, where the venue applies its default fraction, 2x.
#[test]
fn a_market_with_no_account_row_shows_the_venue_default_leverage() {
    let (e, _) = ready_with_defaults();
    let btc = e.store().market("BTC").unwrap();
    assert_eq!(
        btc.limits.leverage.max, 20.0,
        "the maximum is still the minimum fraction's"
    );
    assert_eq!(
        e.store().setting("BTC").unwrap().leverage,
        2.0,
        "the venue's default, not its maximum"
    );
    assert_eq!(
        e.store().setting("ETH").unwrap().leverage,
        10.0,
        "an account row's fraction wins"
    );
}

/// A margin-mode change sends the leverage the account holds; on an
/// unconfigured market that is the venue default, never the maximum.
#[test]
fn a_margin_mode_change_on_an_unconfigured_market_keeps_its_default_leverage() {
    let (mut e, _) = ready_with_defaults();
    e.drain_actions().for_each(drop);
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 9,
            symbol: "BTC".into(),
            isolated: true,
        }),
    );
    let write = nonce(&mut e);
    assert_eq!(
        wire(&write)["InitialMarginFraction"],
        5000,
        "2x, not 500 (20x)"
    );
}

/// A market whose metadata names no default keeps the maximum, as before.
#[test]
fn a_market_without_a_default_fraction_falls_back_to_its_minimum() {
    let (e, _) = ready_unattributed();
    assert_eq!(e.store().setting("BTC").unwrap().leverage, 20.0);
}

#[test]
fn defaults_clamp_to_limits_and_absent_or_nonpositive_defaults_keep_maximum() {
    for (value, expected) in [
        (json!(100), 20.0),
        (json!(20000), 1.0),
        (json!(5000), 2.0),
        (json!(0), 20.0),
        (json!(-1), 20.0),
        (Value::Null, 20.0),
    ] {
        let mut meta: Value = serde_json::from_str(DEFAULTS_META).unwrap();
        meta["order_book_details"][1]["default_initial_margin_fraction"] = value;
        let (e, _) = ready_with_meta(options(), &meta.to_string());
        assert_eq!(e.store().setting("BTC").unwrap().leverage, expected);
        assert_eq!(e.store().market("BTC").unwrap().limits.leverage.max, 20.0);
    }
}

#[test]
fn explicit_account_imf_overrides_nonmax_default_on_rest_and_stream() {
    for streamed in [false, true] {
        let (mut e, socket) = ready_with_defaults();
        if streamed {
            message(
                &mut e,
                socket,
                NOW,
                r#"{"channel":"account_all:718895","positions":{"2":{"market_id":2,"position":"0","initial_margin_fraction":2000,"margin_mode":0}}}"#,
            );
        } else {
            e.handle(NOW, Event::WsOpened { socket });
            let reads = requests(&mut e);
            let account = reads.iter().find(|r| r.url.contains("/account?")).unwrap();
            let mut body: Value = serde_json::from_str(ACCOUNT).unwrap();
            body["accounts"][0]["positions"] = json!([{"market_id":2,"position":"0","initial_margin_fraction":2000,"margin_mode":0}]);
            reply(&mut e, account, 200, &body.to_string());
        }
        assert_eq!(e.store().setting("BTC").unwrap().leverage, 5.0);
        e.drain_actions().for_each(drop);
        e.handle(
            NOW,
            Event::Command(Command::SetMarginType {
                command: 101,
                symbol: "BTC".into(),
                isolated: true,
            }),
        );
        let write = nonce(&mut e);
        assert_eq!(
            wire(&write)["InitialMarginFraction"],
            2000,
            "streamed={streamed}"
        );
    }
}

#[test]
fn queued_margin_change_keeps_accepted_user_leverage_over_catalogue_default() {
    let (mut e, _) = ready_with_defaults();
    e.handle(
        NOW,
        Event::Command(Command::SetLeverage {
            command: 100,
            symbol: "BTC".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let request = http(&mut e, "nextNonce");
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 101,
            symbol: "BTC".into(),
            isolated: true,
        }),
    );
    reply(&mut e, &request, 200, r#"{"code":200,"nonce":10}"#);
    let leverage = http(&mut e, "sendTx");
    assert_eq!(wire(&leverage)["InitialMarginFraction"], 2000);
    ack(&mut e, &leverage);
    let margin = http(&mut e, "sendTx");
    assert_eq!(wire(&margin)["InitialMarginFraction"], 2000);
    assert_eq!(wire(&margin)["MarginMode"], 1);
    ack(&mut e, &margin);
    assert_eq!(e.store().setting("BTC").unwrap().leverage, 5.0);
}

#[test]
fn position_without_fraction_uses_default_and_no_row_refresh_keeps_explicit_setting() {
    let (mut e, socket) = ready_with_defaults();
    message(
        &mut e,
        socket,
        NOW,
        r#"{"channel":"account_all:718895","positions":{"2":{"market_id":2,"position":"1","avg_entry_price":"200","margin_mode":0}}}"#,
    );
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == "BTC")
            .unwrap()
            .leverage,
        2.0
    );
    message(
        &mut e,
        socket,
        NOW,
        r#"{"channel":"account_all:718895","positions":{"2":{"market_id":2,"position":"0","initial_margin_fraction":2000,"margin_mode":0}}}"#,
    );
    assert_eq!(e.store().setting("BTC").unwrap().leverage, 5.0);
    e.handle(NOW, Event::WsOpened { socket });
    let reads = requests(&mut e);
    let account = reads.iter().find(|r| r.url.contains("/account?")).unwrap();
    reply(&mut e, account, 200, ACCOUNT);
    assert_eq!(e.store().setting("BTC").unwrap().leverage, 5.0);
}

#[test]
fn invalid_catalogue_bounds_do_not_panic_when_a_default_is_present() {
    for has_default in [false, true] {
        let mut meta: Value = serde_json::from_str(DEFAULTS_META).unwrap();
        meta["order_book_details"][1]["min_initial_margin_fraction"] = json!(20000);
        if !has_default {
            meta["order_book_details"][1]
                .as_object_mut()
                .unwrap()
                .remove("default_initial_margin_fraction");
        }
        let result = std::panic::catch_unwind(|| ready_with_meta(options(), &meta.to_string()));
        assert!(
            result.is_ok(),
            "invalid venue leverage bounds must be refused without panicking; default={has_default}"
        );
    }
}
