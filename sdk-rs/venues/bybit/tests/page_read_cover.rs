//! The page lift rule on a real Bybit source feeding a real frame-fed page
//! (docs/plans/RUST_FRAME_FED_READ_COVER.md §9.5). Step 2 ships dormant, so
//! the source's `order_read_covers` bit is forced on in its frames here.
mod support;
use safe_cex_engine::wire::section;
use safe_cex_engine::{Command, Event};
use support::{Harness, Options};

const EMPTY_LIST: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":"","category":"linear"},"retExtInfo":{},"time":1684765770483}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;
/// A flat one-way BTCUSDT row: the source learns its position mode.
const FLAT_BTC: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"side":"","size":"0","avgPrice":"0","markPrice":"51000","positionValue":"0","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}],"nextPageCursor":"","category":"linear"}}"#;
const STREAMED_ORDER: &str = r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"streamed","side":"Buy","orderType":"Limit","orderStatus":"New","price":"50000","qty":"0.01","leavesQty":"0.01","cumExecQty":"0","updatedTime":"1672364262457"}]}"#;

/// A started source with its first orders read out.
fn source() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.load_public();
    assert!(h.has_request("/v5/order/realtime"), "{:?}", h.paths());
    // Its position mode reaches the page; the orders read stays out.
    for (path, body) in [
        ("/v5/position/list", FLAT_BTC),
        ("/v5/account/wallet-balance", WALLET),
    ] {
        while h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h
}

fn answer_account(h: &mut Harness) {
    for (path, body) in [
        ("/v5/order/realtime", EMPTY_LIST),
        ("/v5/position/list", EMPTY_LIST),
        ("/v5/account/wallet-balance", WALLET),
    ] {
        while h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
}

/// The source's snapshot, its CAPABILITIES bit forced to `bit`.
fn snapshot(source: &mut Harness, bit: bool) -> Vec<u8> {
    let mut frame = source.engine.snapshot_frame(source.now).to_vec();
    let mut at = 16;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let length = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == section::CAPABILITIES {
            let flags = u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
            let bit30 = 1u32 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT;
            let flags = if bit { flags | bit30 } else { flags & !bit30 };
            frame[at + 8..at + 12].copy_from_slice(&flags.to_le_bytes());
        }
        at += 8 + length;
    }
    frame
}

/// A loaded frame-fed page on the source's clock (offset measured as zero),
/// with one lost BTCUSDT placement fencing it.
fn fenced_page(source: &mut Harness, bit: bool) -> Harness {
    let mut engine = support::engine(Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    });
    engine.set_frame_fed().unwrap();
    let mut page = Harness::started_engine(engine);
    page.respond("status=PreLaunch", 200, support::PRELAUNCH);
    page.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    page.engine.set_source_clock_offset(Some(0.0));
    let first = snapshot(source, bit);
    page.engine.apply_frame(page.now, &first, true).unwrap();
    page.collect();
    page.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    let request = page.take("/v5/order/create");
    page.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert_eq!(page.unknown, vec![1]);
    page
}

/// Ingest the source's snapshot, then try a placement: true when it went out.
fn placeable(page: &mut Harness, source: &mut Harness, bit: bool, command: u32) -> bool {
    let frame = snapshot(source, bit);
    page.engine.apply_frame(page.now, &frame, true).unwrap();
    page.collect();
    page.command(Command::PlaceOrders {
        command,
        orders: vec![support::order()],
    });
    let sent = !page.requests.is_empty();
    page.requests.clear();
    sent
}

/// H2: the source's read went out before the lost write and lands 4 s later.
/// With the bit, its frame lifts nothing; without it, master's stamp rule
/// lifts (H2 reproduces there, as on master).
#[test]
fn h2_a_read_sent_before_the_lost_write_lifts_nothing_with_the_bit() {
    for bit in [true, false] {
        let mut source = source();
        let mut page = fenced_page(&mut source, bit);
        source.advance(4_000.0);
        page.advance(4_000.0);
        answer_account(&mut source);
        assert_eq!(placeable(&mut page, &mut source, bit, 2), !bit, "bit {bit}");
    }
}

/// A read the source sends well after the mark lifts the fence.
#[test]
fn a_read_sent_well_after_the_mark_lifts_with_the_bit() {
    let mut source = source();
    let mut page = fenced_page(&mut source, true);
    answer_account(&mut source);
    source.advance(4_000.0);
    page.advance(4_000.0);
    // A subscription acknowledgement releases a fresh account read.
    let socket = source.socket("/v5/private").expect("private socket");
    source.event(Event::WsOpened { socket });
    source.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth"}"#,
    );
    source.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    assert!(
        !placeable(&mut page, &mut source, true, 2),
        "not yet answered"
    );
    answer_account(&mut source);
    assert!(placeable(&mut page, &mut source, true, 3));
}

/// H10 at page level: the stream opens an order after the source's read went
/// out; the source refuses that read, publishes no newer cover, and the
/// page's fence stands however late the frame.
#[test]
fn h10_a_refused_source_read_lifts_nothing_with_the_bit() {
    for bit in [true, false] {
        let mut source = source();
        let mut page = fenced_page(&mut source, bit);
        source.advance(4_000.0);
        page.advance(4_000.0);
        let socket = source.socket("/v5/private").expect("private socket");
        source.event(Event::WsOpened { socket });
        source.message(
            "/v5/private",
            r#"{"success":true,"ret_msg":"","op":"auth"}"#,
        );
        source.message("/v5/private", STREAMED_ORDER);
        answer_account(&mut source);
        assert_eq!(placeable(&mut page, &mut source, bit, 2), !bit, "bit {bit}");
    }
}
