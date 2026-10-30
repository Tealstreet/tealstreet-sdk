//! Frame-fed Hyperliquid: the page engine loads only public instrument
//! metadata, takes its account state and builder approval from a data
//! source's frames, and sends each write as exactly one venue request.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use serde_json::Value;
use support::*;

const ACCOUNT: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";

fn page_engine() -> (Engine<Hyperliquid>, Vec<String>) {
    let mut e = private_engine(
        Options {
            key: Some(ACCOUNT.into()),
            application_id: Some(ACCOUNT.into()),
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
