//! Position ordering uses local receive/release cuts, never cross-stream native time.
mod support;
use safe_cex_engine::Event;
use serde_json::{Value, json};
use support::*;
fn ready() -> Harness {
    let mut h = Harness::classic();
    while h.requests.iter().any(|r| r.url.contains("/all-position")) {
        h.respond("/all-position", 200, POSITION);
    }
    h
}
fn row(size: u32, time: &str) -> Value {
    json!({"symbol":"BTCUSDT","holdSide":"long","total":size.to_string(),"openPriceAvg":"90","markPrice":"100","leverage":"20","unrealizedPL":"10","liquidationPrice":"50","marginSize":"50","marginMode":"crossed","posMode":"one_way_mode","uTime":time})
}
fn ws(h: &mut Harness, size: u32, time: &str) {
    h.private_message(
        &json!({"arg":{"channel":"positions"},"action":"snapshot","data":[row(size,time)]})
            .to_string(),
    );
}
fn repair_at(h: &mut Harness, delay: f64) {
    let (timer, _) = h
        .timers
        .iter()
        .find(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
        .copied()
        .unwrap();
    h.timers.retain(|(t, _)| *t != timer);
    h.now += delay;
    h.event(Event::TimerFired { timer });
}
fn rest(h: &mut Harness, rows: Value) {
    h.respond(
        "/all-position",
        200,
        &json!({"code":"00000","data":rows}).to_string(),
    );
}
fn size(h: &Harness) -> f64 {
    h.engine
        .store()
        .positions()
        .iter()
        .map(|p| p.contracts)
        .sum()
}
#[test]
fn a_recent_pre_send_open_beats_stale_rest_even_with_a_later_native_time() {
    let mut h = ready();
    ws(&mut h, 2, "100");
    repair_at(&mut h, 1000.0);
    rest(&mut h, json!([row(1, "999999")]));
    assert_eq!(
        size(&h),
        2.0,
        "server uTime is not cross-stream causal evidence"
    );
    assert!(
        h.timers
            .iter()
            .any(|(_, at)| (*at - h.now - 500.0).abs() < 0.01),
        "conflicting REST owes a bounded re-read"
    );
}
#[test]
fn a_recent_pre_send_zero_cannot_be_resurrected_by_stale_rest() {
    let mut h = ready();
    ws(&mut h, 0, "100");
    repair_at(&mut h, 1000.0);
    rest(&mut h, json!([row(1, "999999")]));
    assert_eq!(
        size(&h),
        0.0,
        "zero is a per-leg observation, not an old server timestamp"
    );
    assert!(
        h.timers
            .iter()
            .any(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
    );
}
#[test]
fn a_recent_pre_send_open_survives_omission_and_owes_a_re_read() {
    let mut h = ready();
    ws(&mut h, 2, "100");
    repair_at(&mut h, 1000.0);
    rest(&mut h, json!([]));
    assert_eq!(size(&h), 2.0);
    assert!(
        h.timers
            .iter()
            .any(|(_, at)| (*at - h.now - 500.0).abs() < 0.01),
        "stale omission must schedule a re-read"
    );
}
#[test]
fn same_connection_receive_order_and_a_post_send_observation_are_preserved() {
    let mut h = ready();
    ws(&mut h, 3, "9000");
    ws(&mut h, 2, "100");
    assert_eq!(size(&h), 2.0);
    repair_at(&mut h, 500.0);
    h.now += 1.0;
    ws(&mut h, 4, "1");
    rest(&mut h, json!([]));
    assert_eq!(size(&h), 4.0);
}

#[test]
fn one_cut_owes_one_coalesced_repair_without_a_500ms_feedback_loop() {
    let mut h = ready();
    h.now += 1.0;
    ws(&mut h, 2, "100");
    repair_at(&mut h, 1000.0);
    rest(&mut h, json!([row(1, "999999")]));
    assert_eq!(size(&h), 2.0);
    repair_at(&mut h, 500.0);
    rest(&mut h, json!([row(1, "999999")]));
    assert_eq!(size(&h), 2.0);
    assert!(
        !h.timers
            .iter()
            .any(|(_, at)| (*at - h.now - 500.0).abs() < 0.01),
        "a repair never recursively owes another repair"
    );
    h.now += 5001.0;
    h.command(safe_cex_engine::Command::Venue {
        command: 88,
        action: safe_cex_engine::VenueAction::ReadPositions,
    });
    rest(&mut h, json!([row(1, "0")]));
    assert_eq!(
        size(&h),
        1.0,
        "expired local grace permits healing, regardless of native time"
    );
}

#[test]
fn a_read_waiting_for_preparation_uses_its_physical_release_not_logical_start() {
    use safe_cex_engine::{
        Action,
        request_context::{HttpAction, Prepared},
    };
    let mut h = ready();
    ws(&mut h, 3, "100");
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .find(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
        .unwrap();
    h.timers.retain(|(id, _)| *id != timer);
    h.now = at;
    h.engine.handle(h.now, Event::TimerFired { timer });
    let mut held = None;
    for action in h.engine.drain_actions().collect::<Vec<_>>() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains("/all-position") => held = Some(preparation),
                _ => panic!("unexpected managed preparation"),
            },
            other => h.absorb(vec![other]),
        }
    }
    let preparation = held.expect("real native positions preparation");
    h.now += 1.0;
    ws(&mut h, 2, "900000");
    h.now += 6000.0;
    h.engine.http_prepared(
        h.now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    h.engine.prepare_dispatch(h.now);
    for action in h.engine.drain_actions().collect::<Vec<_>>() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(
                        h.engine.claim_http(h.now, permit),
                        Ok(safe_cex_engine::request_context::Claim::Released(_))
                    ));
                    h.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
                _ => panic!("expected the held preparation's exact Bind"),
            },
            other => h.absorb(vec![other]),
        }
    }
    h.collect();
    rest(&mut h, json!([row(1, "0")]));
    assert_eq!(
        size(&h),
        1.0,
        "the pre-release observation is outside its bounded receipt window"
    );
}

#[test]
fn reconnect_invalidates_the_old_cut_and_a_genuine_new_census_can_heal() {
    let mut h = ready();
    ws(&mut h, 2, "900000");
    repair_at(&mut h, 500.0);
    let old = h.take("/all-position");
    let socket = h
        .sockets
        .iter()
        .rev()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0;
    let timers: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
    h.event(Event::WsClosed { socket, code: 1006 });
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .filter(|(id, _)| !timers.contains(id))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("native private reconnect timer");
    h.timers.retain(|(id, _)| *id != timer);
    h.now = at;
    h.event(Event::TimerFired { timer });
    let next = h
        .sockets
        .iter()
        .rev()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0;
    assert_ne!(socket, next);
    h.event(Event::WsOpened { socket: next });
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    h.reply(old.id, 200, r#"{"code":"00000","data":[]}"#);
    assert_eq!(
        size(&h),
        2.0,
        "old connection census cannot pay the reconnect debt"
    );
    repair_at(&mut h, 500.0);
    rest(&mut h, json!([]));
    assert_eq!(
        size(&h),
        0.0,
        "post-reconnect REST heals without old connection marks"
    );
}

fn read_positions(h: &mut Harness, command: u32) {
    h.command(safe_cex_engine::Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::ReadPositions,
    });
}

fn hedge_row(side: &str, size: u32) -> Value {
    let mut p = row(size, "999999");
    p["holdSide"] = json!(side);
    p["posMode"] = json!("hedge_mode");
    p
}

#[test]
fn a_protected_hedge_leg_does_not_block_rest_healing_its_unobserved_peer() {
    use safe_cex_engine::model::PositionSide;
    let mut h = ready();
    read_positions(&mut h, 90);
    rest(&mut h, json!([hedge_row("long", 1), hedge_row("short", 1)]));
    assert!(h.engine.store().hedged());
    h.private_message(
        &json!({"arg":{"channel":"positions"},"action":"update","data":[hedge_row("long",2)]})
            .to_string(),
    );
    repair_at(&mut h, 1000.0);
    rest(&mut h, json!([hedge_row("long", 1), hedge_row("short", 4)]));
    let rows = h.engine.store().positions();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|p| p.side == PositionSide::Long)
            .unwrap()
            .contracts,
        2.0
    );
    assert_eq!(
        rows.iter()
            .find(|p| p.side == PositionSide::Short)
            .unwrap()
            .contracts,
        4.0
    );

    h.private_message(
        &json!({"arg":{"channel":"positions"},"action":"update","data":[hedge_row("long",0)]})
            .to_string(),
    );
    read_positions(&mut h, 91);
    rest(&mut h, json!([hedge_row("long", 1), hedge_row("short", 6)]));
    let rows = h.engine.store().positions();
    assert_eq!(
        rows.len(),
        1,
        "the long zero tombstone is independent of the short leg"
    );
    assert_eq!(
        (rows[0].side, rows[0].contracts),
        (PositionSide::Short, 6.0)
    );
}

#[test]
fn an_agreeing_read_settles_without_waiting_for_the_receipt_grace() {
    let mut h = ready();
    h.now += 1.0;
    ws(&mut h, 2, "100");
    repair_at(&mut h, 500.0);
    rest(&mut h, json!([row(2, "1")]));
    assert_eq!(size(&h), 2.0);
    assert!(
        !h.timers
            .iter()
            .any(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
    );
    read_positions(&mut h, 92);
    rest(&mut h, json!([row(2, "2")]));
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 92 && result.is_ok())
    );
}

#[test]
fn agreement_cannot_certify_a_read_the_ws_observed_after_its_release() {
    let mut h = ready();
    read_positions(&mut h, 94);
    h.now += 1.0;
    ws(&mut h, 2, "100");
    rest(&mut h, json!([row(2, "999999")]));
    assert_eq!(size(&h), 2.0);
    assert!(
        !h.done.iter().any(|(id, _)| *id == 94),
        "agreement is not a cross-stream barrier for a post-send WS observation"
    );
    repair_at(&mut h, 500.0);
    rest(&mut h, json!([row(2, "0")]));
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 94 && result.is_ok()),
        "a genuine later census can settle"
    );
}

#[test]
fn a_recent_netting_side_cannot_be_replaced_by_the_opposite_rest_side() {
    use safe_cex_engine::model::PositionSide;
    let mut h = ready();
    ws(&mut h, 2, "100");
    repair_at(&mut h, 1000.0);
    let mut opposite = row(1, "999999");
    opposite["holdSide"] = json!("short");
    rest(&mut h, json!([opposite]));
    let rows = h.engine.store().positions();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].side, rows[0].contracts), (PositionSide::Long, 2.0));
    assert!(
        h.timers
            .iter()
            .any(|(_, at)| (*at - h.now - 500.0).abs() < 0.01)
    );
}

#[test]
fn uta_and_classic_demo_keep_recent_opens_and_zero_tombstones() {
    for uta in [true, false] {
        let mut h = Harness::started(support::FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            testnet: !uta,
            ..Default::default()
        });
        if uta {
            h.load_public();
        } else {
            h.respond("/contracts", 200, &CONTRACTS.replace("USDT", "SUSDT"));
            h.respond("/tickers", 200, &TICKERS.replace("USDT", "SUSDT"));
            for (socket, _) in h.sockets.clone() {
                h.event(Event::WsOpened { socket });
            }
        }
        h.respond(
            "/account/settings",
            200,
            if uta {
                r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#
            } else {
                r#"{"code":"40084","data":null}"#
            },
        );
        let endpoint = if uta {
            "/current-position"
        } else {
            "/all-position"
        };
        let response = |rows: Value| {
            if uta {
                json!({"code":"00000","data":{"list":rows}}).to_string()
            } else {
                json!({"code":"00000","data":rows}).to_string()
            }
        };
        h.respond(endpoint, 200, &response(json!([])));
        h.login();
        // Login can owe a genuine successor to the startup census.
        while h.requests.iter().any(|r| r.url.contains(endpoint)) {
            h.respond(endpoint, 200, &response(json!([])));
        }
        let native_row = |size, time| {
            let mut p = row(size, time);
            if !uta {
                p["symbol"] = json!("BTCSUSDT");
            }
            p
        };
        h.private_message(&json!({"arg":{"channel":"positions"},"action":"snapshot","data":[native_row(2,"100")]}).to_string());
        repair_at(&mut h, 1000.0);
        h.respond(endpoint, 200, &response(json!([])));
        assert_eq!(size(&h), 2.0, "uta={uta}");
        h.private_message(
            &json!({"arg":{"channel":"positions"},"action":"update","data":[native_row(0,"1")]})
                .to_string(),
        );
        read_positions(&mut h, 93);
        h.respond(endpoint, 200, &response(json!([native_row(1, "999999")])));
        assert_eq!(size(&h), 0.0, "uta={uta}");
    }
}
