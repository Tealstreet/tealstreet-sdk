//! A frame-fed BloFin engine: public instruments only, no sockets or account
//! reads, and writes that read the store and VENUE_STATE its data source sends.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

fn place(h: &mut Harness, command: u32) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_str(
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.1,"price":60000}]"#,
    )
    .unwrap();
    h.handle(
        1.0,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
    h.collect();
}

fn failed(h: &Harness, command: u32) -> bool {
    h.actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: c, .. }) if *c == command),
    )
}

fn urls(h: &Harness) -> Vec<&str> {
    h.pending.iter().map(|r| r.url.as_str()).collect()
}

#[test]
fn a_frame_fed_start_reads_only_public_instruments_and_opens_no_socket() {
    let mut h = Harness::frame_fed();
    assert!(h.e.store().loaded().markets);
    assert!(
        h.pending.is_empty(),
        "nothing after instruments: {:?}",
        urls(&h)
    );
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    for timer in h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .collect::<Vec<_>>()
    {
        h.handle(120_000.0, Event::TimerFired { timer });
        h.collect();
    }
    assert!(h.pending.is_empty(), "no polls: {:?}", urls(&h));
}

#[test]
fn a_write_waits_for_the_source_state_then_sends_only_itself_and_never_resends() {
    let mut h = Harness::frame_fed();
    // Without the source's margin mode the write refuses visibly, unsent.
    place(&mut h, 7);
    assert!(failed(&h, 7));
    assert!(h.pending.is_empty());

    let mut source = Harness::new();
    source.seed();
    let frame = source.e.snapshot_frame(source.now).to_vec();
    h.e.apply_frame(h.now, &frame, true).unwrap();
    h.collect();
    assert!(h.pending.is_empty(), "ingest reads nothing: {:?}", urls(&h));
    assert_eq!(h.e.store().positions().len(), 1);

    place(&mut h, 8);
    assert_eq!(
        urls(&h),
        ["https://openapi.blofin.com/api/v1/trade/batch-orders"]
    );
    let write = h.take("/trade/batch-orders");
    // An unknown outcome: no resend and no reconcile read; frames settle it.
    h.handle(
        2.0,
        Event::HttpFailed {
            request: write.id,
            error: "connection reset",
        },
    );
    h.collect();
    assert!(
        h.pending.is_empty(),
        "nothing after the write: {:?}",
        urls(&h)
    );
}

/// A data source snapshot whose position mode is `mode` (`net_mode` or `long_short_mode`).
fn source_frame(mode: &str) -> Vec<u8> {
    let mut source = Harness::new();
    source.reply(
        "margin-mode",
        r#"{"code":"0","data":{"marginMode":"cross"}}"#,
    );
    source.reply(
        "position-mode",
        &format!(r#"{{"code":"0","data":{{"positionMode":"{mode}"}}}}"#),
    );
    source.e.snapshot_frame(source.now).to_vec()
}

fn hedged_engine() -> Harness {
    let mut h = Harness::frame_fed();
    h.e.apply_frame(h.now, &source_frame("net_mode"), true)
        .unwrap();
    h.collect();
    h.handle(
        1.0,
        Event::Command(Command::SetPositionMode {
            command: 5,
            hedged: true,
            symbol: None,
        }),
    );
    h.collect();
    h.reply(
        "set-position-mode",
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    assert!(h.e.store().hedged());
    h
}

#[test]
fn a_confirmed_position_mode_outlasts_a_stale_source_frame() {
    let mut h = hedged_engine();
    // A quiet frame (no STATUS) first: it must not count as the source agreeing.
    let quiet = safe_cex_engine::wire::FrameWriter::new(Vec::new(), h.now).finish();
    h.e.apply_frame(h.now, &quiet, false).unwrap();
    h.e.apply_frame(h.now, &source_frame("net_mode"), false)
        .unwrap();
    h.collect();
    assert!(
        h.e.store().hedged(),
        "the stale frame does not undo the write"
    );
    place(&mut h, 8);
    assert_eq!(
        urls(&h),
        ["https://openapi.blofin.com/api/v1/trade/batch-orders"]
    );
    let body = h.take("/trade/batch-orders").body.unwrap();
    assert!(
        body.contains(r#""positionSide":"long""#),
        "hedged buy opens long: {body}"
    );
}

#[test]
fn a_source_frame_with_the_new_mode_releases_the_hold() {
    let mut h = hedged_engine();
    h.e.apply_frame(h.now, &source_frame("long_short_mode"), false)
        .unwrap();
    h.collect();
    assert!(h.e.store().hedged());
    // Released: the source rules again, so a later one-way frame applies.
    h.e.apply_frame(h.now, &source_frame("net_mode"), false)
        .unwrap();
    h.collect();
    assert!(!h.e.store().hedged());
}

#[test]
fn refresh_settings_on_a_data_source_re_reads_its_settings_and_writes_nothing() {
    let mut h = Harness::new();
    h.seed();
    assert!(h.pending.is_empty(), "{:?}", urls(&h));
    h.handle(
        2.0,
        Event::Command(Command::Venue {
            command: 4,
            action: safe_cex_engine::VenueAction::RefreshSettings { symbol: None },
        }),
    );
    h.collect();
    let mut paths: Vec<_> = urls(&h)
        .iter()
        .map(|url| {
            url.split('?')
                .next()
                .unwrap()
                .rsplit("/v1/")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "account/margin-mode",
            "account/position-mode",
            "account/positions"
        ]
    );
    assert!(h.pending.iter().all(|r| r.body.is_none()), "reads only");
}

#[test]
fn a_confirmed_margin_mode_outlasts_a_stale_source_frame_until_it_agrees() {
    let mut h = Harness::frame_fed();
    h.e.apply_frame(h.now, &source_frame("net_mode"), true)
        .unwrap();
    h.collect();
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 5,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    h.reply(
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    assert!(h.pending.is_empty(), "no post-write read: {:?}", urls(&h));
    // The stale cross frame neither reverts the account mode nor a flat symbol's row.
    h.e.apply_frame(h.now, &source_frame("net_mode"), false)
        .unwrap();
    h.collect();
    place(&mut h, 8);
    let body = h.take("/trade/batch-orders").body.unwrap();
    assert!(body.contains(r#""marginMode":"isolated""#), "{body}");
}
