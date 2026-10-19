mod support;
use safe_cex_engine::{Action, Event, Notice};
use support::*;

#[test]
fn private_auth_is_delayed_and_subscriptions_wait_for_ack() {
    let mut h = Harness::new();
    let socket = h.socket(true);
    h.event(Event::WsOpened { socket });
    assert!(!h.actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 1000.0;
    h.event(Event::TimerFired { timer });
    assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("public/auth") && text.contains("sig"))));
    h.event(Event::WsMessage {
        socket,
        text: r#"{"method":"public/auth","code":0}"#,
    });
    assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("user.advance.order") && text.contains("user.trade") && text.contains("user.balance"))));
}
#[test]
fn position_snapshot_cannot_overwrite_newer_stream_exposure() {
    let mut h = Harness::new();
    let old = h.http("get-positions");
    h.ws("user.positions", r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","open_position_pnl":"3","leverage":"10","liquidation_price":"120","update_timestamp_ms":5}"#);
    h.respond(&old, r#"{"code":0,"result":{"data":[{"instrument_name":"BTCUSD-PERP","quantity":"1","cost":"99","update_timestamp_ms":1}]}}"#);
    let p = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|p| p.side == safe_cex_engine::model::PositionSide::Short)
        .unwrap();
    assert_eq!((p.contracts, p.leverage, p.entry_price), (2.0, 10.0, 100.0));
    assert!(h.engine.store().loaded().positions);
}
#[test]
fn terminal_events_fence_both_snapshot_and_out_of_order_frames() {
    let mut h = Harness::new();
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!("{{\"code\":0,\"result\":{{\"data\":[{ORDER}]}}}}"),
    );
    h.ws(
        "user.order",
        &ORDER
            .replace("ACTIVE", "FILLED")
            .replace("\"update_time\":1", "\"update_time\":2"),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    h.ws("user.order", ORDER);
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(
        h.engine.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    assert_eq!(
        h.engine.store().orders()[0].client_order_id.as_deref(),
        Some("caller")
    );
}
#[test]
fn failed_second_order_snapshot_keeps_the_first_good_state() {
    let mut h = Harness::new();
    h.ws("user.order", ORDER);
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    let r = h.http("advanced/get-open-orders");
    h.event(Event::HttpResponse {
        request: r.id,
        response: safe_cex_engine::Response {
            status: 403,
            headers: "",
            body: "denied",
        },
    });
    assert_eq!(h.engine.store().orders().len(), 1);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Log { .. })))
    );
}
#[test]
fn each_execution_emits_once_with_large_ids_intact() {
    let mut h = Harness::new();
    h.load();
    h.engine.take_frame(h.now);
    let row = r#"{"trade_id":9007199254740997,"order_id":9007199254740993,"instrument_name":"BTCUSD-PERP","side":"BUY","traded_price":"100","traded_quantity":"0.1"}"#;
    h.ws("user.trade", row);
    h.ws("user.trade", row);
    let bytes = h.engine.take_frame(h.now).unwrap();
    let mut at = 16;
    while at < bytes.len() {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            assert_eq!(
                u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()),
                1
            );
            return;
        }
        at += 8 + len;
    }
    panic!("missing fills section");
}

#[test]
fn closure_timestamps_fence_late_nonzero_and_zero_frames() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    let zero =
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"0","cost":"0","update_timestamp_ms":20}"#;
    h.ws("user.positions", short);
    assert_eq!(h.engine.store().positions()[0].id, "BTCUSD-PERP:short");
    h.ws("user.positions", &zero.replace(":20", ":5"));
    assert_eq!(h.engine.store().positions().len(), 1);
    h.ws("user.positions", zero);
    h.ws("user.positions", short);
    assert!(h.engine.store().positions().is_empty());
}
#[test]
fn live_long_does_not_drop_untouched_short_in_initial_snapshot() {
    let mut h = Harness::new();
    let old = h.http("get-positions");
    let long =
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"2","cost":"200","update_timestamp_ms":10}"#;
    let short =
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"-1","cost":"100","update_timestamp_ms":1}"#;
    h.ws("user.positions", long);
    h.respond(
        &old,
        &format!(
            "{{\"code\":0,\"result\":{{\"data\":[{},{}]}}}}",
            long.replace(":10", ":1").replace("\"2\"", "\"1\""),
            short
        ),
    );
    let positions = h.engine.store().positions();
    assert_eq!(positions.len(), 2);
    assert!(
        positions
            .iter()
            .any(|p| p.id == "BTCUSD-PERP:long" && p.contracts == 2.0)
    );
    assert!(
        positions
            .iter()
            .any(|p| p.id == "BTCUSD-PERP:short" && p.contracts == 1.0)
    );
}
#[test]
fn signature_rejections_retry_once_even_with_http_error_status() {
    let mut h = Harness::new();
    for attempt in 0..2 {
        let r = h.http("get-positions");
        h.event(Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 401,
                headers: "",
                body: r#"{"code":40101,"message":"bad signature"}"#,
            },
        });
        if attempt == 0 {
            assert!(h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn closed_exposure_stays_closed_against_cached_rest_and_equal_time_frames() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    h.ws("user.positions", short);
    h.ws(
        "user.positions",
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"0","cost":"0","update_timestamp_ms":20}"#,
    );
    h.ws("user.positions", &short.replace(":10", ":20"));
    assert!(h.engine.store().positions().is_empty());
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    assert!(h.engine.store().positions().is_empty());
}
#[test]
fn rest_position_timestamps_fence_older_stream_frames() {
    let mut h = Harness::new();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":30}"#;
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    h.ws(
        "user.positions",
        &short.replace(":30", ":10").replace(r#""-2""#, r#""-1""#),
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}

#[test]
fn cached_rest_order_cannot_rewind_stream_update_before_request_started() {
    let mut h = Harness::new();
    let live = ORDER
        .replace("\"update_time\":1", "\"update_time\":30")
        .replace(
            "\"cumulative_quantity\":\"0\"",
            "\"cumulative_quantity\":\"0.5\"",
        );
    h.ws("user.order", &live);
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    let row = &h.engine.store().orders()[0];
    assert_eq!(row.last_update, 30.0);
    assert_eq!(row.filled, 0.5);
}

#[test]
fn authoritative_empty_snapshot_fences_duplicate_live_position() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    h.ws("user.positions", short);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    h.reply("get-positions", EMPTY);
    assert!(h.engine.store().positions().is_empty());
    h.ws("user.positions", short);
    assert!(
        h.engine.store().positions().is_empty(),
        "duplicate older exposure must not resurrect after authoritative closure"
    );
    h.reply("user-balance", BALANCE);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    assert!(h.engine.store().positions().is_empty());
    h.ws("user.positions", &short.replace(":10", ":11"));
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}
