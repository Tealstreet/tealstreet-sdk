//! Read covers on a real Bitget source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! venue pair 2): the open-order census (normal, protection and trailing pages,
//! each its own read) commits its rows with the cover of its earliest page.
mod support;
use safe_cex_engine::Command;
use safe_cex_engine::wire::section;
use safe_cex_venue_bitget::Product;
use support::{ACCOUNT, FixtureOptions, Harness, OPEN_ORDER, POSITION};

const EMPTY_PLANS: &str = r#"{"code":"00000","data":{"entrustedList":[]}}"#;
const EMPTY_TRAILING: &str = r#"{"code":"00000","data":[]}"#;

/// A classic account at startup with its first census's normal page still out.
fn census_out() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.login();
    assert!(h.requests.iter().any(|r| r.url.contains("/orders-pending")));
    h
}

fn account_cover(h: &mut Harness) -> (Vec<u16>, Option<f64>) {
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

/// Each page answers 1 s after the last: the cover is the first page's send.
#[test]
fn a_census_cover_is_its_first_pages_send_beside_orders() {
    let mut h = census_out();
    let first = h.now;
    h.now += 1_000.0;
    h.respond("/orders-pending", 200, OPEN_ORDER);
    h.now += 1_000.0;
    h.respond("planType=profit_loss", 200, EMPTY_PLANS);
    h.now += 1_000.0;
    h.respond("planType=track_plan", 200, EMPTY_TRAILING);
    let (kinds, cover) = account_cover(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    assert_eq!(cover, Some(first), "the earliest page, not the last");
}

/// H10: the stream opens an order after the census went out; the census lacks
/// it, keeps the row, and proves no cover.
#[test]
fn a_stream_order_after_the_census_went_out_leaves_no_cover() {
    let mut h = census_out();
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"88","side":"buy","orderType":"limit","status":"live"}]}"#);
    h.orders(OPEN_ORDER);
    assert!(
        h.engine.store().order("7").is_some(),
        "the stream row stays"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

/// A row the census cannot map proves no account cover.
#[test]
fn an_unmappable_census_row_proves_no_cover() {
    let mut h = census_out();
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"orderId":"9","symbol":"NOTAMARKET","size":"1","price":"1","side":"buy","orderType":"limit","status":"live"}],"endId":""}}"#);
    assert!(
        h.engine.store().order("9").is_none(),
        "the row was not mapped"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

/// The follow-up a refused census owes waits at least 2 s from that census's
/// start, so a busy symbol cannot drive back-to-back census reads.
#[test]
fn the_follow_up_to_a_refused_census_is_rate_bounded() {
    let mut h = census_out();
    let started = h.now;
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"88","side":"buy","orderType":"limit","status":"live"}]}"#);
    let before: Vec<_> = h.timers.iter().map(|(timer, _)| *timer).collect();
    h.orders(OPEN_ORDER);
    assert!(
        !h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "no census straight after the refused one"
    );
    let armed: Vec<f64> = h
        .timers
        .iter()
        .filter(|(timer, _)| !before.contains(timer))
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(armed.len(), 1, "one follow-up: {armed:?}");
    assert!(
        armed[0] >= started + 2_000.0 - 0.01,
        "no sooner than 2 s: {armed:?}"
    );
}

#[test]
fn successive_refused_reads_wait_for_real_bound() {
    let mut h = census_out();
    for round in 0..3 {
        let started = h.now;
        h.private_message(&format!(r#"{{"arg":{{"channel":"orders"}},"data":[{{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"{}","side":"buy","orderType":"limit","status":"live"}}]}}"#,88+round));
        let timers: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
        h.orders(OPEN_ORDER);
        assert!(!h.requests.iter().any(|r| r.url.contains("/orders-pending")));
        let (timer, at) = *h
            .timers
            .iter()
            .find(|(id, _)| !timers.contains(id))
            .unwrap();
        assert!(at >= started + 2000.);
        h.timers.retain(|(id, _)| *id != timer);
        h.now = at;
        h.event(safe_cex_engine::Event::TimerFired { timer });
        assert_eq!(
            h.requests
                .iter()
                .filter(|r| r.url.contains("/orders-pending"))
                .count(),
            1
        );
        h.event(safe_cex_engine::Event::TimerFired { timer });
        assert_eq!(
            h.requests
                .iter()
                .filter(|r| r.url.contains("/orders-pending"))
                .count(),
            1
        );
    }
}
#[test]
fn known_market_unsupported_order_blocks_cover() {
    let mut h = census_out();
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"orderId":"9","symbol":"BTCUSDT","size":"1","price":"1","side":"buy","orderType":"future_type","status":"live"}],"endId":""}}"#);
    assert!(h.engine.store().order("9").is_none());
    assert_eq!(account_cover(&mut h).1, None);
}

#[test]
fn same_clock_stream_metadata_survives_refused_read() {
    let mut h = census_out();
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"88","triggerPrice":"88","side":"sell","orderType":"market","planType":"track_plan","callbackRatio":"0.01","status":"live"}]}"#);
    let row = h.engine.store().order("7").unwrap().clone();
    let metadata = |h: &mut Harness| {
        let f = h.engine.snapshot_frame(h.now).to_vec();
        safe_cex_engine::wire::sections(&f)
            .find(|(k, _)| *k == section::VENUE_STATE)
            .unwrap()
            .1
            .to_vec()
    };
    let before = metadata(&mut h);
    h.respond("/orders-pending", 200, EMPTY_PLANS);
    h.respond("planType=profit_loss", 200, EMPTY_PLANS);
    h.respond("planType=track_plan",200,r#"{"code":"00000","data":[{"orderId":"7","symbol":"BTCUSDT","size":"1","price":"1","triggerPrice":"1","side":"sell","orderType":"market","planType":"track_plan","callbackRatio":"0.2","status":"live"}]}"#);
    assert_eq!(h.engine.store().order("7"), Some(&row));
    assert_eq!(metadata(&mut h), before);
    assert_eq!(account_cover(&mut h).1, None);
}

/// The ORDER_READ_COVERS bit of a source's snapshot CAPABILITIES section.
fn advertises_covers(h: &mut Harness) -> bool {
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT != 0
}

/// Every Bitget product advertises read covers in its own snapshot.
#[test]
fn bitget_advertises_read_covers() {
    for product in [Product::Usdt, Product::Usdc, Product::Inverse] {
        let mut h = Harness::started(FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            product,
            ..FixtureOptions::default()
        });
        assert!(advertises_covers(&mut h), "{product:?}");
    }
}

/// A UTA account loaded with its first census answered.
fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#,
    );
    answer_uta(&mut h);
    h.login();
    while reads(&h, "/unfilled-orders") > 0 {
        answer_uta(&mut h);
    }
    h
}

fn answer_uta(h: &mut Harness) {
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
}

/// Each account mode's loaded source, its normal-orders path, and how a
/// whole census is answered.
type Mode = (fn() -> Harness, &'static str, fn(&mut Harness));
const MODES: [Mode; 2] = [
    (classic, "/orders-pending", |h| h.orders(OPEN_ORDER)),
    (uta, "/unfilled-orders", answer_uta),
];

/// A classic account with every census its login started answered.
fn classic() -> Harness {
    let mut h = Harness::classic();
    while reads(&h, "/orders-pending") > 0 {
        h.orders(OPEN_ORDER);
    }
    h
}

fn reads(h: &Harness, path: &str) -> usize {
    h.requests.iter().filter(|r| r.url.contains(path)).count()
}

fn ask(h: &mut Harness, command: u32, symbols: Option<Vec<String>>) {
    h.command(Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols },
    });
}

/// A refresh ask reads the account's orders (every family), and the census
/// publishes an account cover at the ask's send time.
#[test]
fn a_refresh_ask_reads_orders_and_publishes_its_send_time() {
    for (source, path, answer) in MODES {
        for symbols in [None, Some(vec!["BTCUSDT".to_owned()])] {
            let mut h = source();
            assert_eq!(reads(&h, path), 0, "{path}: no census out");
            h.now += 5_000.0;
            let sent = h.now;
            ask(&mut h, 50, symbols.clone());
            assert_eq!(reads(&h, path), 1, "{path} {symbols:?}: the ask reads");
            h.now += 1_000.0;
            answer(&mut h);
            assert_eq!(account_cover(&mut h).1, Some(sent), "{path} {symbols:?}");
            assert_eq!(reads(&h, path), 0, "{path} {symbols:?}: nothing more");
        }
    }
}

/// H7/H3: asks while the read is out start nothing; exactly one follow-up
/// census starts after it lands, and none after that.
#[test]
fn asks_while_a_read_is_out_get_one_follow_up() {
    for (source, path, answer) in MODES {
        let mut h = source();
        h.now += 5_000.0;
        ask(&mut h, 50, None);
        assert_eq!(reads(&h, path), 1, "{path}");
        for command in 51..150 {
            h.now += 1.0;
            ask(&mut h, command, Some(vec!["BTCUSDT".to_owned()]));
        }
        assert_eq!(reads(&h, path), 1, "{path}: H7");
        h.now += 1_000.0;
        let follow_up = h.now;
        answer(&mut h);
        assert_eq!(reads(&h, path), 1, "{path}: one follow-up");
        h.now += 1_000.0;
        answer(&mut h);
        assert_eq!(account_cover(&mut h).1, Some(follow_up), "{path}");
        assert_eq!(reads(&h, path), 0, "{path}: none after it");
    }
}

/// Fires the earliest timer, answering nothing.
fn fire_next(h: &mut Harness) {
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a timer");
    h.timers.retain(|(t, _)| *t != timer);
    h.now = h.now.max(at);
    h.event(safe_cex_engine::Event::TimerFired { timer });
}

/// codex: an ask during a REST pause owes a full census. The stream-live 60 s
/// safety read (normal orders only, proving no cover) must not stand in for it.
#[test]
fn an_ask_during_a_rest_pause_gets_a_full_census_not_the_safety_read() {
    let mut h = classic();
    // A rate-limited account read pauses REST.
    while !h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/accounts"))
    {
        fire_next(&mut h);
    }
    // Longer than the engine's 30 s refresh deadline, so the safety tick can
    // come due before the ask's own retry.
    let accounts = h.take("/account/accounts");
    h.event(safe_cex_engine::Event::HttpResponse {
        request: accounts.id,
        response: safe_cex_engine::Response {
            status: 429,
            headers: "retry-after: 50",
            body: r#"{"code":"429","msg":"Too Many Requests"}"#,
        },
    });
    let paused_at = h.now;
    ask(&mut h, 60, None);
    assert_eq!(reads(&h, "/orders-pending"), 0, "paused: the ask waits");
    // Past the pause and the safety interval, the next order read goes out.
    while reads(&h, "/orders-pending") == 0 {
        fire_next(&mut h);
        assert!(
            h.now < paused_at + 200_000.0,
            "no order read after the pause"
        );
    }
    h.respond("/orders-pending", 200, OPEN_ORDER);
    assert!(
        reads(&h, "planType=profit_loss") > 0,
        "the read the ask owed is a full census: {:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    h.respond("planType=profit_loss", 200, EMPTY_PLANS);
    h.respond("planType=track_plan", 200, EMPTY_TRAILING);
    assert!(
        account_cover(&mut h)
            .1
            .is_some_and(|cover| cover > paused_at)
    );
}
