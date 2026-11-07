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

const SPOT_META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},[{"coin":"PURR/USDC","midPx":"0.2","markPx":"0.2","prevDayPx":"0.2","dayNtlVlm":"1"}]]"#;
const SPOT_STATE: &str = r#"{"balances":[{"coin":"USDC","token":0,"hold":"0","total":"10"}]}"#;

/// A started, loaded Hyperliquid spot source; its startup reads answered.
fn spot_source() -> Engine<Hyperliquid> {
    let mut e = support::private_engine(
        safe_cex_venue_hyperliquid::Options {
            key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
            application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
            ..Default::default()
        },
        support::TEST_SECRET.into(),
        true,
    );
    e.start(1000.0);
    for _ in 0..10 {
        let mut any = false;
        for action in support::prepared_now(&mut e) {
            match action {
                Action::Http(r) => {
                    any = true;
                    let body = match support::kind(&r).as_str() {
                        "spotMetaAndAssetCtxs" => SPOT_META,
                        "frontendOpenOrders" => "[]",
                        "spotClearinghouseState" => SPOT_STATE,
                        other => panic!("unexpected read {other}"),
                    };
                    support::reply(&mut e, &r, 200, body);
                }
                Action::WsConnect { socket, .. } => {
                    e.handle(1000.0, Event::WsOpened { socket });
                }
                _ => {}
            }
        }
        if !any {
            break;
        }
    }
    e
}

fn refresh(e: &mut Engine<Hyperliquid>, now: f64, command: u32, symbols: Option<Vec<String>>) {
    e.handle(
        now,
        Event::Command(safe_cex_engine::Command::Venue {
            command,
            action: safe_cex_engine::VenueAction::RefreshOrders { symbols },
        }),
    );
}

/// The open-orders reads now out, with any balance read answered.
fn orders_reads(e: &mut Engine<Hyperliquid>) -> Vec<safe_cex_engine::HttpRequest> {
    let mut reads = Vec::new();
    for action in support::prepared_now(e) {
        match action {
            Action::Http(r) if support::kind(&r) == "frontendOpenOrders" => reads.push(r),
            Action::Http(r) if support::kind(&r) == "spotClearinghouseState" => {
                support::reply(e, &r, 200, SPOT_STATE);
            }
            _ => {}
        }
    }
    reads
}

fn advertises_read_covers(e: &mut Engine<Hyperliquid>) -> bool {
    let frame = e.snapshot_frame(1_000.0).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT != 0
}

/// Spot reads one scope, so its refresh lands; perp withholds covers: a
/// symbol-scoped refresh reads only that symbol's dex, and the account cover
/// needs every HIP-3 dex (H9), so the ask would never land.
#[test]
fn spot_and_perp_advertise_read_covers() {
    assert!(advertises_read_covers(&mut spot_source()));
    let (mut perp, _) = support::ready_with_dex("disabled", true);
    assert!(advertises_read_covers(&mut perp));
}

/// The ask reads spot's open orders; its answer publishes an account cover
/// at the ask's send time, not the landing time.
#[test]
fn a_spot_refresh_ask_reads_orders_and_publishes_its_send_time() {
    for symbols in [None, Some(vec!["PURR/USDC".to_owned()])] {
        let mut e = spot_source();
        assert_eq!(account_cover(&mut e, 1_000.0), Some(1_000.0), "startup");
        let asked = 6_000.0;
        refresh(&mut e, asked, 60, symbols.clone());
        let reads = orders_reads(&mut e);
        assert_eq!(reads.len(), 1, "{symbols:?}");
        e.handle(
            asked + 1_000.0,
            Event::HttpResponse {
                request: reads[0].id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: "[]",
                },
            },
        );
        assert_eq!(
            account_cover(&mut e, asked + 1_000.0),
            Some(asked),
            "{symbols:?}"
        );
    }
}

/// H7/H3: asks while the read is out start nothing; exactly one follow-up
/// read starts after it lands, and nothing after that one.
#[test]
fn spot_asks_while_a_read_is_out_get_one_follow_up() {
    let mut e = spot_source();
    let asked = 6_000.0;
    refresh(&mut e, asked, 60, Some(vec!["PURR/USDC".to_owned()]));
    let first = orders_reads(&mut e);
    assert_eq!(first.len(), 1);
    for command in 61..161 {
        refresh(&mut e, asked + 10.0, command, None);
    }
    assert!(orders_reads(&mut e).is_empty(), "H7");
    support::reply(&mut e, &first[0], 200, "[]");
    let follow_up = orders_reads(&mut e);
    assert_eq!(follow_up.len(), 1, "one follow-up read");
    support::reply(&mut e, &follow_up[0], 200, "[]");
    assert!(orders_reads(&mut e).is_empty());
}

/// §5.3: the account cover needs every dex, so a perp refresh ask that names a
/// core symbol reads core and every HIP-3 dex, and lands with the account cover.
#[test]
fn a_perp_refresh_ask_naming_a_symbol_reads_every_dex_and_covers_the_account() {
    let (mut e, _, held) = support::ready_holding_dex_orders("no-such-dex");
    assert!(held.is_empty());
    let read = |e: &mut Engine<Hyperliquid>, now, command, symbols| {
        refresh(e, now, command, symbols);
        let mut dexes = drive(e, now, now);
        dexes.sort();
        dexes
    };
    let dexes = read(&mut e, 5_000.0, 70, Some(vec!["BTC-PERP".into()]));
    assert_eq!(dexes, [None, Some("xyz".to_owned())], "core and every dex");
    assert_eq!(account_cover(&mut e, 5_000.0), Some(5_000.0));
    let again = read(&mut e, 6_000.0, 71, None);
    assert_eq!(
        again,
        [None, Some("xyz".to_owned())],
        "the slot is free again"
    );
    assert_eq!(account_cover(&mut e, 6_000.0), Some(6_000.0));
}
