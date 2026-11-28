//! Chart history honours the IP ban a 418 or IP-weight 429 announced.
use support::Options;
mod support;

use safe_cex_engine::{Command, ErrorKind, Event, Outcome};
use support::Harness;

const NOW: f64 = 1_795_867_200_000.0;
const BANNED_UNTIL: f64 = 1_795_867_733_681.0;

fn public() -> Harness {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.now = NOW;
    h
}

fn fetch(h: &mut Harness, command: u32) {
    h.event(Event::Command(Command::FetchCandles {
        command,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::parse("1m").unwrap(),
        limit: Some(10),
        from_ms: None,
        to_ms: None,
    }));
}

fn answer_klines(h: &mut Harness, status: u16, headers: &str, body: &str) {
    let request = h.take("/klines").id;
    h.reply(request, status, headers, body);
}

#[test]
fn a_banned_ip_stops_chart_history_until_the_ban_ends() {
    let mut h = public();
    fetch(&mut h, 1);
    answer_klines(
        &mut h,
        418,
        "retry-after: 399",
        &format!(
            r#"{{"code":-1003,"msg":"Way too many requests; IP(108.61.162.122) banned until {BANNED_UNTIL:.0}. Please use the websocket for live updates to avoid bans."}}"#
        ),
    );
    h.failures.clear();

    h.now = NOW + 60_000.0;
    fetch(&mut h, 2);
    assert!(!h.has_request("/klines"), "{:?}", h.paths());
    let [(2, error)] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(
        (error.kind, error.outcome, error.retry_after_ms),
        (
            Some(ErrorKind::RateLimited),
            Some(Outcome::NotSent),
            Some(BANNED_UNTIL - h.now)
        )
    );

    h.now = BANNED_UNTIL + 1.0;
    fetch(&mut h, 3);
    assert!(h.has_request("/klines"), "{:?}", h.failures);
}

#[test]
fn without_a_ban_time_the_pause_follows_retry_after() {
    let mut h = public();
    fetch(&mut h, 1);
    answer_klines(
        &mut h,
        429,
        "retry-after: 30",
        r#"{"code":-1003,"msg":"Too many requests; current limit of IP(108.61.162.122) is 2400 requests per minute."}"#,
    );
    h.failures.clear();

    h.now = NOW + 10_000.0;
    fetch(&mut h, 2);
    assert!(!h.has_request("/klines"));
    assert_eq!(h.failures[0].1.retry_after_ms, Some(20_000.0));

    h.now = NOW + 30_001.0;
    fetch(&mut h, 3);
    assert!(h.has_request("/klines"));
}

#[test]
fn an_order_rate_429_or_heavy_weight_leaves_chart_history_alone() {
    let mut h = public();
    fetch(&mut h, 1);
    answer_klines(
        &mut h,
        429,
        "retry-after: 10",
        r#"{"code":-1015,"msg":"Too many new orders; current limit is 300 orders per TEN_SECONDS."}"#,
    );
    fetch(&mut h, 2);
    answer_klines(&mut h, 200, "x-mbx-used-weight-1m: 2000", "[]");
    h.failures.clear();

    fetch(&mut h, 3);
    assert!(h.has_request("/klines"), "{:?}", h.failures);
}
