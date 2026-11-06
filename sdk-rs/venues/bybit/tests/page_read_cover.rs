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
    let mut page = page(source, bit);
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

fn page(source: &mut Harness, bit: bool) -> Harness {
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
    page
}

#[test]
fn an_unknown_page_cancel_owes_one_successor_behind_a_pre_cancel_source_read() {
    let mut source = source();
    let old: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert!(!old.is_empty());
    let mut page = page(&mut source, true);
    source.advance(100.);
    page.advance(100.);
    page.command(Command::CancelOrdersWithSymbols {
        command: 7,
        orders: vec![safe_cex_engine::model::CancelOrderRef {
            id: "native-stop".into(),
            symbol: "BTCUSDT".into(),
        }],
    });
    let cancel = page.take("/v5/order/cancel");
    page.event(Event::HttpFailed {
        request: cancel.id,
        error: "cancel response lost",
    });
    assert_eq!(page.unknown, [7]);
    let asks: Vec<_> = page
        .notices
        .iter()
        .filter_map(|n| match n {
            safe_cex_engine::Notice::Event(e) if e.code == "orders.refresh" => Some(e),
            _ => None,
        })
        .collect();
    assert_eq!(asks.len(), 1);
    assert!(asks[0].fields.iter().any(
        |(k, v)| *k == "symbols" && matches!(v, safe_cex_engine::Value::Str(s) if s.is_empty())
    ));
    source.command(Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
    });
    let out: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert_eq!(out, old, "the ask joins the old read without replacing it");
    for id in &old {
        source.requests.retain(|r| r.id != *id);
        source.reply(*id, 200, EMPTY_LIST);
    }
    let fresh: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert_eq!(fresh.len(), old.len(), "one owed successor batch");
    assert!(fresh.iter().all(|id| !old.contains(id)));
    answer_account(&mut source);
    assert!(
        !source.has_request("/v5/order/realtime"),
        "clean successor ends the demand"
    );
    page.engine
        .apply_frame(page.now, &snapshot(&mut source, true), false)
        .unwrap();
    page.collect();
    assert_eq!(
        page.unknown,
        [7],
        "a read never rewrites the original cancel result"
    );
    assert!(
        page.requests.is_empty(),
        "no cancel replay or page-side read"
    );
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

/// The page's `orders.held` asks since `from`: (symbols, liftAfterMs).
fn held(page: &Harness, from: usize) -> Vec<(String, f64)> {
    page.notices[from..]
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::Event(event)
                if event.code == safe_cex_engine::log::code::ORDERS_HELD =>
            {
                let field = |name| event.fields.iter().find(|(key, _)| *key == name);
                match (field("symbols"), field("liftAfterMs")) {
                    (
                        Some((_, safe_cex_engine::Value::Str(symbols))),
                        Some((_, safe_cex_engine::Value::Num(after))),
                    ) => Some((symbols.clone(), *after)),
                    _ => panic!("{event:?}"),
                }
            }
            _ => None,
        })
        .collect()
}

/// H1: the page names its fence in `orders.held`; the host relays it after
/// `liftAfterMs`, the source reads after the ask, and that read's frame lifts
/// the fence. A second ask while the read is out starts nothing (H7).
#[test]
fn h1_the_pages_ask_brings_the_read_that_lifts_its_fence() {
    let mut source = source();
    answer_account(&mut source);
    let mut page = fenced_page(&mut source, true);
    let asks = held(&page, 0);
    assert_eq!(asks, [("BTCUSDT".to_owned(), 3_000.0)], "{asks:?}");
    source.advance(asks[0].1 + 500.0);
    page.advance(asks[0].1 + 500.0);
    answer_account(&mut source);
    let orders_reads = |source: &Harness| {
        source
            .paths()
            .iter()
            .filter(|path| path.contains("/v5/order/realtime"))
            .count()
    };
    source.command(Command::Venue {
        command: 50,
        action: safe_cex_engine::VenueAction::RefreshOrders {
            symbols: Some(vec![asks[0].0.clone()]),
        },
    });
    let asked = orders_reads(&source);
    assert!(asked > 0, "the ask reads: {:?}", source.paths());
    source.command(Command::Venue {
        command: 51,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
    });
    assert_eq!(
        orders_reads(&source),
        asked,
        "H7: nothing more while it is out"
    );
    answer_account(&mut source);
    assert!(placeable(&mut page, &mut source, true, 3));
}

/// Bybit advertises read covers in its own snapshot, without forcing.
#[test]
fn bybit_advertises_read_covers() {
    let mut source = source();
    let frame = source.engine.snapshot_frame(source.now).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    assert_ne!(
        flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT,
        0
    );
}

#[test]
fn one_hundred_asks_start_only_one_follow_up() {
    let mut source = source();
    answer_account(&mut source);
    source.advance(3500.);
    let ask = |command| Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::RefreshOrders {
            symbols: Some(vec!["BTCUSDT".into()]),
        },
    };
    source.command(ask(90));
    let first: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert!(!first.is_empty());
    for command in 91..190 {
        source.command(ask(command));
    }
    let after: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert_eq!(after, first, "H7: old exact physical reads stay out");
    for id in first {
        source.requests.retain(|r| r.id != id);
        source.reply(id, 200, EMPTY_LIST);
    }
    let follow: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert_eq!(follow.len(), after.len(), "one follow-up batch");
    answer_account(&mut source);
    assert!(!source.has_request("/v5/order/realtime"));
}

#[test]
fn a_refused_refresh_keeps_bounded_retry_demand() {
    let mut source = source();
    answer_account(&mut source);
    let socket = source.socket("/v5/private").unwrap();
    source.event(Event::WsOpened { socket });
    source.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth"}"#,
    );
    source.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    answer_account(&mut source);
    let mut page = fenced_page(&mut source, true);
    source.advance(3500.);
    page.advance(3500.);
    source.command(Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::RefreshOrders {
            symbols: Some(vec!["BTCUSDT".into()]),
        },
    });
    assert!(source.has_request("/v5/order/realtime"));
    source.message("/v5/private", STREAMED_ORDER);
    answer_account(&mut source);
    assert!(
        !placeable(&mut page, &mut source, true, 2),
        "refused census does not lift"
    );
    for _ in 0..31 {
        source.advance(1000.);
        page.advance(1000.);
        source.message("/v5/private", r#"{"op":"pong"}"#);
    }
    assert!(
        source.has_request("/v5/order/realtime"),
        "a refused refresh cannot erase all retry demand and leave the page fenced"
    );
    answer_account(&mut source);
    assert!(
        placeable(&mut page, &mut source, true, 3),
        "fresh retry lifts"
    );
}

#[test]
fn a_read_before_a_later_lost_eth_write_cannot_lift_eth() {
    let mut source = source();
    answer_account(&mut source);
    source.command(Command::FetchLeverage {
        command: 88,
        symbol: "ETHUSDT".into(),
    });
    source.respond(
        "/v5/position/list",
        200,
        &FLAT_BTC.replace("BTCUSDT", "ETHUSDT"),
    );
    let mut page = fenced_page(&mut source, true);
    source.advance(3500.);
    page.advance(3500.);
    source.command(Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
    });
    let old: Vec<_> = source
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .map(|r| r.id)
        .collect();
    assert!(!old.is_empty());
    source.advance(100.);
    page.advance(100.);
    let mut order = support::order();
    order.symbol = "ETHUSDT".into();
    order.price = Some(3000.);
    order.amount = 0.01;
    page.command(Command::PlaceOrders {
        command: 2,
        orders: vec![order.clone()],
    });
    if page.has_request("/v5/position/list") {
        page.respond(
            "/v5/position/list",
            200,
            &FLAT_BTC.replace("BTCUSDT", "ETHUSDT"),
        );
    }
    let sent = page.take("/v5/order/create");
    page.event(Event::HttpFailed {
        request: sent.id,
        error: "lost ETH",
    });
    assert!(page.unknown.contains(&2));
    source.advance(3500.);
    page.advance(3500.);
    source.command(Command::Venue {
        command: 91,
        action: safe_cex_engine::VenueAction::RefreshOrders {
            symbols: Some(vec!["ETHUSDT".into()]),
        },
    });
    for id in old {
        source.requests.retain(|r| r.id != id);
        source.reply(id, 200, EMPTY_LIST);
    }
    let frame = snapshot(&mut source, true);
    page.engine.apply_frame(page.now, &frame, true).unwrap();
    page.collect();
    page.command(Command::PlaceOrders {
        command: 3,
        orders: vec![order.clone()],
    });
    assert!(
        !page.has_request("/v5/order/create"),
        "H3: old ask cannot clear later ETH"
    );
    assert!(
        source.has_request("/v5/order/realtime"),
        "new epoch follow-up"
    );
    answer_account(&mut source);
    let frame = snapshot(&mut source, true);
    page.engine.apply_frame(page.now, &frame, true).unwrap();
    page.collect();
    page.command(Command::PlaceOrders {
        command: 4,
        orders: vec![order],
    });
    if page.has_request("/v5/position/list") {
        page.respond(
            "/v5/position/list",
            200,
            &FLAT_BTC.replace("BTCUSDT", "ETHUSDT"),
        );
    }
    assert!(
        page.has_request("/v5/order/create"),
        "working positive after real newer read"
    );
}

#[test]
fn a_new_ask_after_a_refused_read_does_recover() {
    let mut source = source();
    answer_account(&mut source);
    let socket = source.socket("/v5/private").unwrap();
    source.event(Event::WsOpened { socket });
    source.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"auth"}"#,
    );
    source.message(
        "/v5/private",
        r#"{"success":true,"ret_msg":"","op":"subscribe"}"#,
    );
    answer_account(&mut source);
    let mut page = fenced_page(&mut source, true);
    source.advance(3500.);
    page.advance(3500.);
    source.command(Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::RefreshOrders {
            symbols: Some(vec!["BTCUSDT".into()]),
        },
    });
    assert!(source.has_request("/v5/order/realtime"));
    source.message("/v5/private", STREAMED_ORDER);
    answer_account(&mut source);
    assert!(
        !placeable(&mut page, &mut source, true, 2),
        "refused census does not lift"
    );
    source.advance(1000.);
    page.advance(1000.);
    source.command(Command::Venue {
        command: 91,
        action: safe_cex_engine::VenueAction::RefreshOrders {
            symbols: Some(vec!["BTCUSDT".into()]),
        },
    });
    assert!(source.has_request("/v5/order/realtime"));
    answer_account(&mut source);
    assert!(
        placeable(&mut page, &mut source, true, 3),
        "fresh retry lifts"
    );
}
