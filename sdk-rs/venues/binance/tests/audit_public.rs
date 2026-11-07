//! Audit regressions for public range requests and book recovery.
use support::Options;
mod support;

use safe_cex_engine::{Command, Event, wire};
use safe_cex_venue_binance::Variant;
use support::Harness;

fn snapshot_books(h: &mut Harness) -> Vec<Vec<u8>> {
    wire::sections(h.engine.snapshot_frame(h.now))
        .filter(|(kind, _)| *kind == wire::section::BOOK)
        .map(|(_, payload)| payload.to_vec())
        .collect()
}

#[test]
fn missed_gap_clear_is_recoverable_from_snapshot_then_fresh_depth() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98");
    h.respond(
        "/fapi/v1/depth",
        200,
        r#"{"lastUpdateId":100,"bids":[["99","5"]],"asks":[["101","5"]]}"#,
    );
    h.diff(98, 105, 97, "99.5");
    assert_eq!(wire::book_top(&snapshot_books(&mut h)[0]).unwrap().1, 99.5);
    h.engine.take_frame(h.now); // The consumer has seen the live book.
    h.diff(120, 125, 119, "99.6");
    h.engine.take_frame(h.now); // Discard any pending incremental frame.
    assert!(h.engine.book("BTCUSDT").is_none());
    let books = snapshot_books(&mut h);
    assert_eq!(books.len(), 1, "resync identity must survive snapshots");
    assert_eq!(&books[0][4..12], &[0; 8], "zero levels clear the consumer");
    h.diff(126, 130, 125, "99.6");
    h.respond(
        "/fapi/v1/depth",
        200,
        r#"{"lastUpdateId":129,"bids":[["98","5"]],"asks":[["101","5"]]}"#,
    );
    assert_eq!(wire::book_top(&snapshot_books(&mut h)[0]).unwrap().1, 99.6);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    assert_eq!(snapshot_books(&mut h).len(), 1, "other subscriber survives");
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    assert!(snapshot_books(&mut h).is_empty());
}

fn candle_url(
    variant: Variant,
    interval: &str,
    limit: Option<u32>,
    from_ms: Option<f64>,
    to_ms: Option<f64>,
) -> String {
    let mut h = Harness::started(Options {
        variant,
        public_data_only: true,
        ..Options::default()
    });
    h.now = 1_711_888_496_789.0; // 2024-03-31 12:34:56.789 UTC
    h.event(Event::Command(Command::FetchCandles {
        command: 5,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
        limit,
        from_ms,
        to_ms,
    }));
    h.take("/klines").url
}

#[test]
fn every_profile_uses_the_trailing_range_when_limit_is_supplied() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::CoinM,
        Variant::Spot,
    ] {
        let url = candle_url(
            variant,
            "1m",
            Some(100),
            Some(1_000.0),
            Some(1_700_000_000_000.0),
        );
        assert!(url.ends_with("?symbol=BTCUSDT&interval=1m&startTime=1699994000000&endTime=1700000000000&limit=100"), "{variant:?}: {url}");
        let capped = candle_url(variant, "1h", Some(5_000), None, Some(1_700_000_000_000.0));
        assert!(
            capped.ends_with("&startTime=1694600000000&endTime=1700000000000&limit=1500"),
            "{capped}"
        );
        for limit in [None, Some(0)] {
            let from_only = candle_url(
                variant,
                "1m",
                limit,
                Some(1_000.0),
                Some(1_700_000_000_000.0),
            );
            assert!(
                from_only.ends_with("&startTime=1000&endTime=1700000000000"),
                "{from_only}"
            );
            let default = candle_url(variant, "1m", limit, None, None);
            assert!(
                default.ends_with("&startTime=1711858496789&endTime=1711888496789"),
                "{default}"
            );
            assert_eq!(
                default,
                candle_url(variant, "1m", limit, Some(0.0), Some(0.0))
            );
        }
    }
}

#[test]
fn monthly_ranges_subtract_calendar_months_and_clamp_leap_day() {
    for (end, count, start) in [
        (1_711_888_496_789.0, 1, 1_709_210_096_789_u64), // Mar31 -> Feb29, leap year
        (1_680_266_096_789.0, 1, 1_677_587_696_789),     // Mar31 -> Feb28
        (1_709_210_096_789.0, 12, 1_677_587_696_789),    // Feb29 -> Feb28 previous year
        (1_711_888_496_789.0, 3, 1_704_026_096_789),     // Mar31 -> Dec31 previous year
        (1_711_888_496_789.0, 2, 1_706_704_496_789),     // Mar31 -> Jan31 in one shift
    ] {
        let url = candle_url(Variant::UsdM, "1M", Some(count), None, Some(end));
        assert!(url.contains(&format!("&startTime={start}&")), "{url}");
    }
}
