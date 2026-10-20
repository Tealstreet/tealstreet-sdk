//! Venue-reported closed positions: raw `/v5/position/closed-pnl` rows, paged as TS pages them.
mod support;
use safe_cex_engine::{Command, VenueAction};
use safe_cex_venue_bybit::{Endpoints, Options, Variant};
use support::Harness;

const DAY: f64 = 24.0 * 3_600_000.0;
const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;

fn startup(variant: Variant, credentials: bool) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: credentials.then(|| "key".into()),
        secret: credentials.then(|| "secret".into()),
        public_data_only: !credentials,
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    });
    for (path, body) in [
        ("/v5/account/fee-rate", support::FEE_RATES),
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("status=PreLaunch", EMPTY),
    ] {
        if h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.requests.clear();
    h
}

fn fetch(h: &mut Harness, command: u32, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    h.command(Command::Venue {
        command,
        action: VenueAction::FetchClosedPositions {
            symbol: symbol.map(Into::into),
            from_ms: from,
            to_ms: to,
        },
    });
}

fn page(rows: &[&str], cursor: &str) -> String {
    format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"category":"linear","list":[{}],"nextPageCursor":"{cursor}"}}}}"#,
        rows.join(",")
    )
}

const ROW_A: &str = r#"{"symbol":"BTCUSDT","orderId":"a","side":"Sell","qty":"0.01","orderPrice":"0","orderType":"Market","execType":"Trade","closedSize":"0.01","cumEntryValue":"950","avgEntryPrice":"95000","cumExitValue":"960","avgExitPrice":"96000","closedPnl":"9.5","fillCount":"1","leverage":"10","openFee":"0.1","closeFee":"0.1","createdTime":"1700000000000","updatedTime":"1700000000001"}"#;
const ROW_B: &str = r#"{"symbol":"ETHUSDT","orderId":"b","side":"Buy","closedSize":"1","avgEntryPrice":"2000","avgExitPrice":"1990","closedPnl":"9","leverage":"5","updatedTime":"1699000000000","unknownField":{"kept":[1,2]}}"#;

#[test]
fn account_wide_rows_walk_newest_window_first_and_pass_through_verbatim() {
    let mut h = startup(Variant::Linear, true);
    let to = 1_700_000_000_000.0;
    let from = to - 10.0 * DAY;
    fetch(&mut h, 1, None, Some(from), Some(to));
    let first = h.take("/v5/position/closed-pnl");
    assert!(first.url.contains("category=linear"), "{}", first.url);
    assert!(first.url.contains("limit=100"), "{}", first.url);
    assert!(!first.url.contains("symbol="), "{}", first.url);
    let newest_start = to - (7.0 * DAY - 1.0);
    assert!(
        first.url.contains(&format!("endTime={to}")),
        "{}",
        first.url
    );
    assert!(
        first.url.contains(&format!("startTime={newest_start}")),
        "{}",
        first.url
    );
    assert!(Harness::header(&first, "X-BAPI-SIGN").is_some());
    h.reply(first.id, 200, &page(&[ROW_A], "next%3D1"));
    let second = h.take("/v5/position/closed-pnl");
    assert!(second.url.contains("cursor=next%253D1"), "{}", second.url);
    h.reply(second.id, 200, &page(&[ROW_B], ""));
    let older = h.take("/v5/position/closed-pnl");
    assert!(
        older.url.contains(&format!("startTime={from}")),
        "{}",
        older.url
    );
    assert!(
        older
            .url
            .contains(&format!("endTime={}", newest_start - 1.0)),
        "{}",
        older.url
    );
    assert!(!older.url.contains("cursor="), "{}", older.url);
    h.reply(older.id, 200, &page(&[], ""));
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(1).unwrap().as_ref().unwrap()).unwrap();
    let expected: Vec<serde_json::Value> = [ROW_A, ROW_B]
        .iter()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows, expected);
}

#[test]
fn omitted_bounds_read_the_last_seven_days_like_ts() {
    let mut h = startup(Variant::Linear, true);
    fetch(&mut h, 2, None, None, None);
    let now = h.now;
    // Seven days is one millisecond wider than a Bybit window, so TS splits it too.
    let request = h.take("/v5/position/closed-pnl");
    assert!(
        request.url.contains(&format!("endTime={now}")),
        "{}",
        request.url
    );
    let split = now - (7.0 * DAY - 1.0);
    assert!(
        request.url.contains(&format!("startTime={split}")),
        "{}",
        request.url
    );
    h.reply(request.id, 200, EMPTY);
    let request = h.take("/v5/position/closed-pnl");
    let oldest = now - 7.0 * DAY;
    assert!(
        request
            .url
            .contains(&format!("startTime={oldest}&endTime={oldest}")),
        "{}",
        request.url
    );
    h.reply(request.id, 200, EMPTY);
    assert_eq!(h.result(2), Some(&Ok("[]".to_owned())));
}

#[test]
fn symbol_selects_its_category_and_options_are_refused() {
    let mut h = startup(Variant::Linear, true);
    fetch(&mut h, 3, Some("BTCUSD"), Some(1.0), Some(2.0));
    let request = h.take("/v5/position/closed-pnl");
    assert!(request.url.contains("category=inverse"), "{}", request.url);
    assert!(request.url.contains("symbol=BTCUSD&"), "{}", request.url);
    fetch(&mut h, 4, Some("BTC-27DEC24-100000-C"), None, None);
    assert_eq!(
        h.result(4),
        Some(&Err(
            "Bybit has no closed PnL for options (BTC-27DEC24-100000-C)".to_owned()
        ))
    );
}

#[test]
fn a_venue_rejection_fails_the_whole_read_instead_of_returning_a_partial_history() {
    let mut h = startup(Variant::Linear, true);
    fetch(&mut h, 5, None, Some(1.0), Some(2.0));
    h.respond("/v5/position/closed-pnl", 200, &page(&[ROW_A], "more"));
    h.respond(
        "/v5/position/closed-pnl",
        200,
        r#"{"retCode":10002,"retMsg":"invalid request","result":{}}"#,
    );
    assert_eq!(
        h.result(5),
        Some(&Err(
            "bybit closed-PnL fetch failed: invalid request".to_owned()
        ))
    );
}

#[test]
fn the_page_cap_keeps_rows_and_warns() {
    let mut h = startup(Variant::Linear, true);
    fetch(&mut h, 6, None, Some(1.0), Some(2.0));
    for i in 0..20 {
        h.respond(
            "/v5/position/closed-pnl",
            200,
            &page(&[ROW_A], &format!("c{i}")),
        );
    }
    assert!(!h.has_request("/v5/position/closed-pnl"));
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(6).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 20);
    assert!(h.logs.iter().any(|message| message.contains("20-page cap")));
}

#[test]
fn inverse_reads_inverse_spot_refuses_and_keyless_is_empty() {
    let mut h = startup(Variant::Inverse, true);
    fetch(&mut h, 7, None, Some(1.0), Some(2.0));
    let request = h.take("/v5/position/closed-pnl");
    assert!(request.url.contains("category=inverse"), "{}", request.url);

    let mut h = startup(Variant::Spot, true);
    fetch(&mut h, 8, None, None, None);
    assert_eq!(
        h.result(8),
        Some(&Err("Bybit spot has no closed positions".to_owned()))
    );

    let mut h = startup(Variant::Linear, false);
    fetch(&mut h, 9, None, None, None);
    assert_eq!(h.result(9), Some(&Ok("[]".to_owned())));
    assert!(!h.has_request("/v5/position/closed-pnl"));
}
