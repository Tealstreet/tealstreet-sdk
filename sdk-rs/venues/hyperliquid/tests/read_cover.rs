//! Read covers on a real Hyperliquid source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! step 1c): each dex's open-orders read commits its rows with the cover it
//! proves, and the account cover is the oldest across every dex (H9).
mod support;
use safe_cex_engine::wire::section;
use safe_cex_engine::{Action, Engine, Event};
use safe_cex_venue_hyperliquid::Hyperliquid;

/// The account cover in the source's snapshot, if any.
fn account_cover(e: &mut Engine<Hyperliquid>, now: f64) -> Option<f64> {
    let frame = e.snapshot_frame(now).to_vec();
    safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan())
}

/// Run the engine to `until`, firing due timers and answering open-orders
/// reads with no rows (returning the dexes read); other asks are ignored.
fn drive(e: &mut Engine<Hyperliquid>, mut now: f64, until: f64) -> Vec<Option<String>> {
    let mut timers: Vec<(safe_cex_engine::TimerId, f64)> = Vec::new();
    let mut read = Vec::new();
    loop {
        for action in support::prepared_now(e) {
            match action {
                Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => {
                    let body: serde_json::Value =
                        serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
                    read.push(body["dex"].as_str().map(str::to_owned));
                    support::reply(e, &r, 200, "[]");
                }
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(id, _)| *id != timer),
                _ => {}
            }
        }
        timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        match timers.first().copied() {
            Some((timer, at)) if at <= until => {
                timers.remove(0);
                now = now.max(at);
                e.handle(now, Event::TimerFired { timer });
            }
            _ => return read,
        }
    }
}

/// H9: an old HIP-3 dex read (released at startup) lands after a fresh core
/// read. The account cover is the old read's send, never the core read's.
#[test]
fn an_old_dex_read_keeps_the_account_cover_at_its_own_send() {
    let (mut e, socket, held) = support::ready_holding_dex_orders("xyz");
    assert_eq!(held.len(), 1, "the xyz open-orders read is out");
    assert_eq!(
        account_cover(&mut e, 1_000.0),
        None,
        "one dex is not the account"
    );
    // A core order event with no time in force schedules a fresh core read,
    // well after startup.
    support::message(
        &mut e,
        socket,
        5_000.0,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit"},"status":"open"}]}"#,
    );
    let now = 7_000.0;
    assert_eq!(drive(&mut e, 5_000.0, now), [None], "one fresh core read");
    assert_eq!(
        account_cover(&mut e, now),
        None,
        "still no account: xyz has not answered"
    );
    support::reply(&mut e, &held[0], 200, "[]");
    assert_eq!(
        account_cover(&mut e, now),
        Some(1_000.0),
        "the oldest dex send, not the fresher core read's"
    );
}

/// The control: with every dex answered at startup, the account cover is
/// their (shared) send.
#[test]
fn every_dex_read_at_startup_proves_the_account_cover() {
    let (mut e, _, held) = support::ready_holding_dex_orders("no-such-dex");
    assert!(held.is_empty());
    assert_eq!(account_cover(&mut e, 1_000.0), Some(1_000.0));
}
