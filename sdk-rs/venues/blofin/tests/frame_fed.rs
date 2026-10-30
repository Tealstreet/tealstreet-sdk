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
