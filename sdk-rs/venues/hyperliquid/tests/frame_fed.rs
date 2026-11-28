//! Frame-fed Hyperliquid: the page engine loads only public instrument
//! metadata, takes its account state and builder approval from a data
//! source's frames, and sends each write as exactly one venue request.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use serde_json::Value;
use support::*;

const ACCOUNT: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";

fn page_engine() -> (Engine<Hyperliquid>, Vec<String>) {
    page_engine_for(Some(ACCOUNT.into()))
}

fn page_engine_for(application_id: Option<String>) -> (Engine<Hyperliquid>, Vec<String>) {
    let mut e = private_engine(
        Options {
            key: Some(ACCOUNT.into()),
            application_id,
            ..Options::default()
        },
        TEST_SECRET.into(),
        false,
    );
    e.set_frame_fed().unwrap();
    e.start(1000.0);
    let mut reads = Vec::new();
    for _ in 0..20 {
        let actions = prepared_now(&mut e);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    let read = kind(&r);
                    let body = match read.as_str() {
                        "metaAndAssetCtxs" => META.to_owned(),
                        "perpDexs" => "[null]".into(),
                        other => panic!("frame-fed start read {other}"),
                    };
                    reads.push(read);
                    reply(&mut e, &r, 200, &body);
                }
                Action::WsConnect { .. } => panic!("frame-fed start opened a socket"),
                _ => {}
            }
        }
    }
    (e, reads)
}

/// Drives `e` (timers included, ten seconds on) answering nothing, and returns every HTTP request.
fn requests(e: &mut Engine<Hyperliquid>) -> Vec<HttpRequest> {
    let until = safe_cex_engine::test_support::dispatch_now(e) + 10_000.0;
    let mut sent = Vec::new();
    for _ in 0..20 {
        let actions = prepared_now(e);
        if actions.is_empty() {
            break;
        }
        let actions = advance_packet_timers(e, actions, until);
        sent.extend(actions.into_iter().filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        }));
    }
    sent
}

fn place(e: &mut Engine<Hyperliquid>) -> Vec<HttpRequest> {
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
    )
    .unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![order],
        }),
    );
    requests(e)
}

#[test]
fn frame_fed_start_reads_only_public_metadata_and_opens_no_socket() {
    let (e, reads) = page_engine();
    assert_eq!(
        reads,
        ["metaAndAssetCtxs", "perpDexs"].map(String::from).to_vec()
    );
    assert!(e.store().loaded().markets);
}

#[test]
fn a_source_snapshot_carries_builder_approval_so_a_placement_is_one_request() {
    let (mut source, _) = ready("disabled");
    let frame = source.snapshot_frame(1500.0).to_vec();
    let (mut e, _) = page_engine();
    e.apply_frame(1500.0, &frame, true).unwrap();

    let sent = place(&mut e);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].url.ends_with("/exchange"));
    let body: Value = serde_json::from_str(sent[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "order");
    assert!(body["action"]["builder"].is_object(), "{body}");
}

#[test]
fn a_cancel_of_a_source_order_is_one_request_and_no_read() {
    let (mut source, socket) = ready("disabled");
    message(
        &mut source,
        socket,
        1200.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":42,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
    );
    assert!(source.store().order("42").is_some());
    let frame = source.snapshot_frame(1500.0).to_vec();
    let (mut e, _) = page_engine();
    e.apply_frame(1500.0, &frame, true).unwrap();
    assert!(e.store().order("42").is_some());

    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["42".into()],
        }),
    );
    let sent = requests(&mut e);
    assert_eq!(sent.len(), 1, "{sent:?}");
    let body: Value = serde_json::from_str(sent[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "cancel");
    assert_eq!(body["action"]["cancels"][0]["o"], 42);
}

#[test]
fn an_unknown_placement_outcome_is_never_re_sent_or_read_back() {
    let (mut source, _) = ready("disabled");
    let frame = source.snapshot_frame(1500.0).to_vec();
    let (mut e, _) = page_engine();
    e.apply_frame(1500.0, &frame, true).unwrap();
    let sent = place(&mut e);
    assert_eq!(sent.len(), 1);
    reply(&mut e, &sent[0], 502, "bad gateway");
    let later = requests(&mut e);
    assert!(later.is_empty(), "{later:?}");
}

#[test]
fn confirmed_fills_leave_frame_fed_positions_and_refresh_timers_to_the_provider() {
    let (mut source, socket) = ready("disabled");
    let snapshot = source.snapshot_frame(1500.).to_vec();
    let (mut page, _) = page_engine();
    page.apply_frame(1500., &snapshot, true).unwrap();
    let before = page.store().positions().to_vec();
    let mut local_refreshes = 0;
    let mut recurring_refreshes = 0;

    for (command, now) in [(93, 2000.), (94, 4000.)] {
        let order: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTC-PERP","type":"market","side":"buy","amount":1}"#,
        )
        .unwrap();
        page.handle(
            now,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order],
            }),
        );
        let sent = prepared_now(&mut page);
        let writes: Vec<_> = sent
            .iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(
            writes.len(),
            1,
            "a provider-fed write must actually dispatch"
        );
        assert!(writes[0].url.ends_with("/exchange"));
        reply(
            &mut page,
            writes[0],
            200,
            &format!(
                r#"{{"status":"ok","response":{{"type":"order","data":{{"statuses":[{{"filled":{{"oid":{command},"totalSz":"1","avgPx":"100"}}}}]}}}}}}"#
            ),
        );
        let acknowledged = prepared_now(&mut page);
        assert!(acknowledged.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandDone { command: completed, .. }) if *completed == command
        )), "the fixture must confirm both fills: {acknowledged:?}");
        assert!(
            !acknowledged
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );

        for action in acknowledged {
            if let Action::SetTimer { timer, at_ms } = action
                && at_ms == now + 350.
            {
                local_refreshes += 1;
                page.handle(at_ms, Event::TimerFired { timer });
                let after_tick = prepared_now(&mut page);
                assert!(
                    !after_tick
                        .iter()
                        .any(|action| matches!(action, Action::Http(_))),
                    "a frame-fed engine must neither read account state nor resend"
                );
                recurring_refreshes += after_tick
                    .iter()
                    .filter(|action| {
                        matches!(action,
                            Action::SetTimer { at_ms: next, .. } if *next == at_ms + 350.
                        )
                    })
                    .count();
            }
        }
        assert_eq!(
            page.store().positions(),
            before,
            "ACKs are not provider snapshots"
        );
    }

    assert_eq!(
        recurring_refreshes, 0,
        "a discarded frame-fed account read must not create a recurring 350-ms loop after the next fill"
    );
    assert_eq!(
        local_refreshes, 0,
        "frame-fed fills must not arm local account refreshes"
    );

    let mut updated: Value = serde_json::from_str(STATE).unwrap();
    updated["assetPositions"][0]["position"]["szi"] = "4".into();
    updated["time"] = 5000.into();
    message(
        &mut source,
        socket,
        5000.,
        &serde_json::json!({
            "channel":"clearinghouseState", "data":{"dex":"", "clearinghouseState":updated}
        })
        .to_string(),
    );
    page.apply_frame(5001., source.snapshot_frame(5001.), true)
        .unwrap();
    assert_eq!(page.store().positions()[0].contracts, 4.);
    assert!(
        requests(&mut page).is_empty(),
        "the provider remains the account authority"
    );
}

#[test]
fn a_leverage_write_before_the_source_settings_refuses_visibly_then_uses_them() {
    let (mut e, _) = page_engine();
    let set = |e: &mut Engine<Hyperliquid>, command| {
        e.handle(
            2000.0,
            Event::Command(Command::SetLeverage {
                command,
                symbol: "BTC-PERP".into(),
                leverage: 5.0,
                side: None,
            }),
        );
    };
    set(&mut e, 9);
    let refused = e.drain_plain().collect::<Vec<_>>();
    assert!(
        refused
            .iter()
            .any(|a| format!("{a:?}").contains("settings have not arrived")),
        "{refused:?}"
    );
    assert!(requests(&mut e).is_empty());

    let (mut source, _) = ready("disabled");
    let frame = source.snapshot_frame(1500.0).to_vec();
    e.apply_frame(1500.0, &frame, true).unwrap();
    set(&mut e, 10);
    let sent = requests(&mut e);
    assert_eq!(sent.len(), 1, "{sent:?}");
    let body: Value = serde_json::from_str(sent[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "updateLeverage");
    assert_eq!(body["action"]["isCross"], true);
    assert_eq!(body["action"]["leverage"], 5);
}

fn margin(e: &mut Engine<Hyperliquid>, command: u32, isolated: bool) {
    e.handle(
        2000.0,
        Event::Command(Command::SetMarginType {
            command,
            symbol: "BTC-PERP".into(),
            isolated,
        }),
    );
}

fn leverage(e: &mut Engine<Hyperliquid>, command: u32, leverage: f64) {
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command,
            symbol: "BTC-PERP".into(),
            leverage,
            side: None,
        }),
    );
}

/// The one leverage write sent, answered ok; returns its isCross.
fn confirm_one(e: &mut Engine<Hyperliquid>) -> bool {
    let sent = requests(e);
    assert_eq!(sent.len(), 1, "{sent:?}");
    let body: Value = serde_json::from_str(sent[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "updateLeverage");
    reply(
        e,
        &sent[0],
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    body["action"]["isCross"].as_bool().unwrap()
}

#[test]
fn a_confirmed_margin_change_outlasts_a_stale_source_frame() {
    let (mut source, _) = ready("disabled");
    let stale = source.snapshot_frame(1500.0).to_vec();
    let (mut e, _) = page_engine();
    e.apply_frame(1500.0, &stale, true).unwrap();
    margin(&mut e, 20, true);
    assert!(!confirm_one(&mut e));

    // The source has not re-read: its frame still says cross.
    e.apply_frame(2100.0, &stale, true).unwrap();
    leverage(&mut e, 21, 7.0);
    assert!(!confirm_one(&mut e), "the held isolated margin wins");
}

#[test]
fn a_source_frame_that_agrees_releases_the_hold() {
    let (mut source, _) = ready("disabled");
    let stale = source.snapshot_frame(1500.0).to_vec();
    margin(&mut source, 30, true);
    assert!(!confirm_one(&mut source));
    let agreeing = source.snapshot_frame(2050.0).to_vec();

    let (mut e, _) = page_engine();
    e.apply_frame(1500.0, &stale, true).unwrap();
    margin(&mut e, 31, true);
    assert!(!confirm_one(&mut e));
    e.apply_frame(2100.0, &agreeing, true).unwrap();
    // Released: the source owns the setting again, even back to cross.
    e.apply_frame(2200.0, &stale, true).unwrap();
    leverage(&mut e, 32, 7.0);
    assert!(
        confirm_one(&mut e),
        "after release the source's cross applies"
    );
}

#[test]
fn refresh_settings_on_a_source_only_re_reads() {
    let (mut source, _) = ready("disabled");
    source.handle(
        2000.0,
        Event::Command(Command::Venue {
            command: 40,
            action: safe_cex_engine::VenueAction::RefreshSettings {
                symbol: Some("BTC-PERP".into()),
            },
        }),
    );
    let sent = requests(&mut source);
    assert!(!sent.is_empty());
    assert!(sent.iter().all(|r| r.url.ends_with("/info")), "{sent:?}");
    let reads = sent.iter().map(kind).collect::<Vec<_>>();
    assert!(reads.contains(&"activeAssetData".to_string()), "{reads:?}");
    assert!(reads.contains(&"maxBuilderFee".to_string()), "{reads:?}");
}

#[test]
fn the_page_engine_s_own_builder_approval_outlasts_a_source_that_has_not_read_it() {
    let (mut e, _) = page_engine();
    // No source state yet: the first placement reads the approval once.
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
    )
    .unwrap();
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 50,
            orders: vec![order.clone()],
        }),
    );
    // Answered within the builder wait: collect without advancing timers.
    let read = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(read.len(), 1, "only the approval read before the write");
    let builder = read
        .iter()
        .find(|r| kind(r) == "maxBuilderFee")
        .expect("one approval read");
    reply(&mut e, builder, 200, "30");
    let write = requests(&mut e);
    assert_eq!(write.len(), 1);
    assert!(write[0].url.ends_with("/exchange"));
    let body: Value = serde_json::from_str(write[0].body.as_deref().unwrap()).unwrap();
    assert!(body["action"]["builder"].is_object(), "{body}");
    reply(
        &mut e,
        &write[0],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":1}}]}}}"#,
    );

    // A source that saw a refusal (VENUE_STATE builder = 1) does not undo it.
    let (mut source, _) = ready_custom("disabled", false, None, "0");
    let refused = source.snapshot_frame(2100.0).to_vec();
    e.apply_frame(2100.0, &refused, true).unwrap();
    e.handle(
        2200.0,
        Event::Command(Command::PlaceOrders {
            command: 51,
            orders: vec![order],
        }),
    );
    let sent = requests(&mut e);
    assert_eq!(sent.len(), 1, "{sent:?}");
    let body: Value = serde_json::from_str(sent[0].body.as_deref().unwrap()).unwrap();
    assert!(body["action"]["builder"].is_object(), "{body}");
}

// Live: an agent key with no applicationId read orderStatus with an empty user and got 422.
#[test]
fn an_amend_with_no_application_id_resolves_the_account_before_its_read() {
    let (mut e, _) = page_engine_for(None);
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    let order = Order {
        id: "592084223302".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTC-PERP".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 100.0,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    };
    e.store_mut().set_orders(vec![order]);
    e.handle(
        2000.0,
        Event::Command(safe_cex_engine::Command::AmendOrder {
            command: 7,
            id: "592084223302".into(),
            price: Some(99.0),
            amount: None,
        }),
    );
    let mut bodies = Vec::new();
    for _ in 0..5 {
        for action in prepared_now(&mut e) {
            if let Action::Http(r) = action {
                bodies.push(r.body.clone().unwrap_or_default());
                if kind(&r) == "userRole" {
                    reply(
                        &mut e,
                        &r,
                        200,
                        r#"{"role":"agent","data":{"user":"0x1111111111111111111111111111111111111111"}}"#,
                    );
                }
            }
        }
    }
    assert!(bodies[0].contains("userRole"), "{bodies:?}");
    let read = bodies
        .iter()
        .find(|b| b.contains("orderStatus"))
        .expect("the fresh read");
    assert!(
        read.contains("0x1111111111111111111111111111111111111111"),
        "{read}"
    );
}

#[test]
fn a_frame_fed_protective_amend_resolves_identity_and_size_before_cancellation() {
    let (mut source, socket) = ready_custom("disabled", false, None, "null");
    message(
        &mut source,
        socket,
        1200.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"120","triggerPx":"120","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Take Profit Market","isPositionTpsl":true},"status":"open"}]}"#,
    );
    let frame = source.snapshot_frame(1500.).to_vec();
    for role_ok in [false, true] {
        let (mut e, _) = page_engine_for(None);
        e.apply_frame(1500., &frame, true).unwrap();
        e.handle(
            2000.,
            Event::Command(Command::AmendOrder {
                command: 90,
                id: "9".into(),
                price: Some(75.),
                amount: None,
            }),
        );
        let actions = prepared_now(&mut e);
        let reads: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(
            reads.len(),
            1,
            "identity first, no cancel or approval query"
        );
        assert_eq!(kind(reads[0]), "userRole");
        assert_eq!(
            e.store().order("9").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        if !role_ok {
            reply(&mut e, reads[0], 500, "unavailable");
            let actions = prepared_now(&mut e);
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(actions.iter().any(|a| matches!(a,
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 90, error }) if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            )));
            assert_eq!(
                e.store().order("9").unwrap().status,
                safe_cex_engine::model::OrderStatus::Open
            );
            continue;
        }
        reply(
            &mut e,
            reads[0],
            200,
            &format!(r#"{{"role":"agent","data":{{"user":"{ACCOUNT}"}}}}"#),
        );
        let actions = prepared_now(&mut e);
        let reads: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(reads.len(), 1, "fresh sizing next, still no cancellation");
        assert_eq!(kind(reads[0]), "orderStatus");
        assert_eq!(
            serde_json::from_str::<Value>(reads[0].body.as_deref().unwrap()).unwrap()["user"],
            ACCOUNT
        );
        answer_order_status(&mut e, reads[0]);
        let actions = prepared_now(&mut e);
        let cancel = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
                _ => None,
            })
            .expect("cancel only after both prerequisite replies");
        reply(
            &mut e,
            cancel,
            200,
            r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
        );
        let actions = prepared_now(&mut e);
        assert!(
            !actions.iter().any(|a| matches!(a,
                Action::Http(r) if r.url.ends_with("/info")
            )),
            "no optional builder read after cancellation"
        );
        let actions = advance_packet_timers(&mut e, actions, 2334.);
        let writes: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(
            writes.len(),
            1,
            "replacement waits only for native 334ms pacing"
        );
        let body: Value = serde_json::from_str(writes[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["action"]["type"], "order");
        assert!(body["action"].get("builder").is_none());
    }
}

#[test]
fn a_retired_amend_role_cannot_resolve_or_release_its_successor() {
    let (mut source, socket) = ready_custom("disabled", false, None, "null");
    message(
        &mut source,
        socket,
        1200.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"80","triggerPx":"80","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Stop Market"},"status":"open"}]}"#,
    );
    let frame = source.snapshot_frame(1500.).to_vec();
    let (mut e, _) = page_engine_for(None);
    e.apply_frame(1500., &frame, true).unwrap();
    e.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: "9".into(),
            price: Some(75.),
            amount: None,
        }),
    );
    let actions = prepared_now(&mut e);
    let old = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "userRole" => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 7000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(7000., Event::TimerFired { timer });
    prepared_now(&mut e);
    use base64::Engine as _;
    // A moved mark: a snapshot re-sending the seeded 100 is not a fresh observation (AGT-55).
    let zipped = miniz_oxide::deflate::compress_to_vec(br#"{"BTC":{"markPx":"101"}}"#, 6);
    let quote = serde_json::json!({"channel":"fastAssetCtxs","data":base64::engine::general_purpose::STANDARD.encode(zipped)}).to_string();
    message(&mut source, socket, 7100., &quote);
    e.apply_frame(7100., source.snapshot_frame(7100.), true)
        .unwrap();
    e.handle(
        7100.,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "9".into(),
            price: Some(70.),
            amount: None,
        }),
    );
    let actions = prepared_now(&mut e);
    let current = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "userRole" => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    let before = e.store().order("9").unwrap().clone();
    e.handle(7200., Event::HttpResponse {request: old.id, response: safe_cex_engine::Response {status: 200, headers: "", body: r#"{"role":"agent","data":{"user":"0x9999999999999999999999999999999999999999"}}"#}});
    let actions = prepared_now(&mut e);
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "retired role must not issue a successor sizing read"
    );
    assert_eq!(
        serde_json::to_value(e.store().order("9").unwrap()).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    e.handle(
        7300.,
        Event::HttpResponse {
            request: current.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: &format!(r#"{{"role":"agent","data":{{"user":"{ACCOUNT}"}}}}"#),
            },
        },
    );
    let actions = prepared_now(&mut e);
    let sizing = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .expect("current role still resolves its own account");
    assert_eq!(
        serde_json::from_str::<Value>(sizing.body.as_deref().unwrap()).unwrap()["user"],
        ACCOUNT
    );
    answer_order_status(&mut e, sizing);
    assert!(
        prepared_now(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
}

fn telemetry_page() -> Engine<Hyperliquid> {
    let (mut source, _) = ready("disabled");
    let frame = source.snapshot_frame(1500.0).to_vec();
    let (mut engine, _) = page_engine();
    engine.apply_frame(1500.0, &frame, true).unwrap();
    engine
}

fn unknown_events(actions: &[Action]) -> Vec<&safe_cex_engine::LogEvent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == "order.outcome_unknown" =>
            {
                support::assert_unknown_projection(event);
                Some(event)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn frame_fed_engine_unknown_emits_one_hook_event() {
    for status in [Some(408), Some(429), Some(503), None] {
        let mut engine = telemetry_page();
        let sent = place(&mut engine);
        assert_eq!(sent.len(), 1);
        let request = &sent[0];
        if let Some(status) = status {
            reply(&mut engine, request, status, "unproven");
        } else {
            let now = safe_cex_engine::test_support::dispatch_now(&engine);
            engine.handle(
                now,
                Event::HttpFailed {
                    request: request.id,
                    error: "lost response",
                },
            );
        }
        let actions = prepared_now(&mut engine);
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, error }) => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "{status:?}: {actions:?}");
        assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
        let events = unknown_events(&actions);
        assert_eq!(events.len(), 1, "{status:?}: one engine-sealed diagnostic");
        let event = events[0];
        assert!(event.message.contains("Check your open orders"));
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "command")
                .unwrap()
                .1
                .as_str(),
            Some("command.7")
        );
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "request")
                .unwrap()
                .1
                .as_str(),
            Some(request.id.0.to_string().as_str())
        );
        assert_eq!(
            event
                .fields
                .iter()
                .find(|(key, _)| *key == "outcome")
                .unwrap()
                .1
                .as_str(),
            Some("unknown")
        );
        let field = |key| {
            event
                .fields
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value)
        };
        assert_eq!(
            field("placedOrderIds").and_then(|value| value.as_str()),
            Some("")
        );
        assert_eq!(
            field("unknownIndices").and_then(|value| value.as_str()),
            Some("0")
        );
        assert_eq!(
            field("status"),
            status.map(safe_cex_engine::log::Value::from).as_ref()
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "frame-fed failure cannot send reads or writes"
        );
        reply(
            &mut engine,
            request,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":99}}]}}}"#,
        );
        let now = safe_cex_engine::test_support::dispatch_now(&engine);
        engine.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "late failure",
            },
        );
        let late = prepared_now(&mut engine);
        assert!(unknown_events(&late).is_empty());
        assert!(!late.iter().any(|action| matches!(
            action,
            Action::Http(_)
                | Action::Notice(
                    safe_cex_engine::Notice::CommandFailed { command: 7, .. }
                        | safe_cex_engine::Notice::CommandDone { command: 7, .. }
                )
        )));
    }
}

#[test]
fn frame_fed_definite_refusal_emits_no_unknown_event() {
    let mut engine = telemetry_page();
    let sent = place(&mut engine);
    assert_eq!(sent.len(), 1);
    reply(
        &mut engine,
        &sent[0],
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order must have minimum value of $10."}]}}}"#,
    );
    let actions = prepared_now(&mut engine);
    assert!(unknown_events(&actions).is_empty());
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, error }) if error.outcome != Some(safe_cex_engine::Outcome::Unknown))));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn frame_fed_venue_decoded_unknown_still_emits_once() {
    let mut engine = telemetry_page();
    let sent = place(&mut engine);
    assert_eq!(sent.len(), 1);
    reply(&mut engine, &sent[0], 200, "{}");
    let actions = prepared_now(&mut engine);
    assert_eq!(unknown_events(&actions).len(), 1);
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, error }) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    reply(&mut engine, &sent[0], 200, "{}");
    assert!(unknown_events(&prepared_now(&mut engine)).is_empty());
}

#[test]
fn frame_fed_unknown_telemetry_caps_a_batched_unknown_journal() {
    for status in [200, 503] {
        let mut engine = telemetry_page();
        let order: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
        )
        .unwrap();
        engine.handle(
            2000.,
            Event::Command(Command::PlaceOrders {
                command: 7,
                orders: vec![order; 17],
            }),
        );
        let sent = requests(&mut engine);
        assert_eq!(sent.len(), 1);
        reply(&mut engine, &sent[0], status, "{}");
        let actions = prepared_now(&mut engine);
        let events = unknown_events(&actions);
        assert_eq!(events.len(), 1);
        let indices = events[0]
            .fields
            .iter()
            .find(|(key, _)| *key == "unknownIndices")
            .unwrap()
            .1
            .as_str()
            .unwrap();
        // The 17 plain limits share one batched `order` action, so the unproven
        // reply leaves every row Unknown; telemetry caps the list at 16 entries.
        assert_eq!(
            indices,
            (0..16).map(|i| i.to_string()).collect::<Vec<_>>().join(",")
        );
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, error }) => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        let journal = failures[0].elements.as_ref().unwrap();
        assert_eq!(journal.len(), 17);
        assert_eq!(journal[16].element.index, 16);
        assert!(journal.iter().all(|row| matches!(
            row.result,
            safe_cex_engine::write_plan::ElementResult::Unknown { .. }
        )));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}

#[test]
fn managed_unknown_paths_emit_once_and_refusal_emits_none() {
    for case in ["http", "transport", "decoded", "refused"] {
        let (mut engine, _) = ready("disabled");
        let sent = place(&mut engine);
        let writes: Vec<_> = sent
            .iter()
            .filter(|request| request.url.ends_with("/exchange"))
            .collect();
        assert_eq!(writes.len(), 1);
        let request = writes[0];
        match case {
            "http" => reply(&mut engine, request, 503, "unproven"),
            "transport" => {
                let now = safe_cex_engine::test_support::dispatch_now(&engine);
                engine.handle(
                    now,
                    Event::HttpFailed {
                        request: request.id,
                        error: "lost response",
                    },
                );
            }
            "decoded" => reply(&mut engine, request, 200, "{}"),
            "refused" => reply(
                &mut engine,
                request,
                200,
                r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order must have minimum value of $10."}]}}}"#,
            ),
            _ => unreachable!(),
        }
        let actions = prepared_now(&mut engine);
        assert_eq!(
            unknown_events(&actions).len(),
            usize::from(case != "refused"),
            "{case}: {actions:?}"
        );
        assert!(!actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.ends_with("/exchange"))
        ));
    }
}

#[test]
fn frame_fed_cancel_and_settings_use_the_engine_reporter_once() {
    for operation in ["cancel", "leverage"] {
        for status in [Some(503), None] {
            let (mut source, socket) = ready("disabled");
            message(
                &mut source,
                socket,
                1200.,
                r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":42,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
            );
            let frame = source.snapshot_frame(1500.).to_vec();
            let (mut engine, _) = page_engine();
            engine.apply_frame(1500., &frame, true).unwrap();
            let command = if operation == "cancel" {
                Command::CancelOrders {
                    command: 7,
                    ids: vec!["42".into()],
                }
            } else {
                Command::SetLeverage {
                    command: 7,
                    symbol: "BTC-PERP".into(),
                    leverage: 5.,
                    side: None,
                }
            };
            engine.handle(2000., Event::Command(command));
            let sent = requests(&mut engine);
            assert_eq!(sent.len(), 1, "{operation}");
            if let Some(status) = status {
                reply(&mut engine, &sent[0], status, "unproven");
            } else {
                let now = safe_cex_engine::test_support::dispatch_now(&engine);
                engine.handle(
                    now,
                    Event::HttpFailed {
                        request: sent[0].id,
                        error: "lost response",
                    },
                );
            }
            let actions = prepared_now(&mut engine);
            assert_eq!(
                unknown_events(&actions).len(),
                1,
                "{operation} {status:?}: {actions:?}"
            );
            assert!(actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, error }) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_)))
            );
        }
    }
}
