//! BingX answers 109400 ("The request you constructed does not meet the
//! requirements") to a fractional millisecond time field. Calibrated protocol
//! time carries a fraction, so every outbound time field must be an integer.
use safe_cex_engine::{
    Action, Command, Engine, Event, Query,
    model::{Interval, Limits, Market, MinMax, Precision},
};
use safe_cex_venue_bingx::Bingx;
mod support;

const FRACTIONAL_NOW: f64 = 1_795_795_470_123.75;

fn market() -> Market {
    Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 10.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn engine(private: bool) -> Engine<Bingx> {
    let mut engine = support::engine(support::Options {
        public_data_only: !private,
        key: private.then(|| "key".into()),
        secret: private.then(|| "secret".into()),
        ..Default::default()
    });
    engine.store_mut().set_markets(vec![market()]);
    engine
}

fn sent_urls(engine: &mut Engine<Bingx>, command: Command) -> Vec<String> {
    engine.handle(FRACTIONAL_NOW, Event::Command(command));
    safe_cex_engine::test_support::prepared_actions(engine, FRACTIONAL_NOW)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request.url),
            _ => None,
        })
        .collect()
}

fn assert_integer_times(url: &str, names: &[&str]) {
    let query = url.split_once('?').map_or("", |(_, query)| query);
    for name in names {
        let value = query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("{name} missing from {url}"));
        assert!(
            !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
            "{name}={value} must be integer milliseconds: {url}"
        );
    }
}

#[test]
fn candle_reads_send_integer_times_from_calibrated_now() {
    for private in [false, true] {
        let mut engine = engine(private);
        let urls = sent_urls(
            &mut engine,
            Command::FetchCandles {
                command: 7,
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
                limit: Some(300),
                from_ms: None,
                to_ms: None,
            },
        );
        let url = urls
            .iter()
            .find(|url| url.contains("/openApi/swap/v3/quote/klines"))
            .unwrap_or_else(|| panic!("no klines request: {urls:?}"));
        assert_integer_times(url, &["startTime", "endTime"]);
    }
}

#[test]
fn candle_reads_floor_fractional_caller_times() {
    let mut engine = engine(false);
    let urls = sent_urls(
        &mut engine,
        Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: Interval::H1,
            limit: None,
            from_ms: Some(1_795_700_000_000.5),
            to_ms: Some(1_795_790_000_000.9),
        },
    );
    let url = urls
        .iter()
        .find(|url| url.contains("/openApi/swap/v3/quote/klines"))
        .unwrap_or_else(|| panic!("no klines request: {urls:?}"));
    assert!(url.contains("startTime=1795700000000&"), "{url}");
    assert!(url.contains("endTime=1795790000000"), "{url}");
    assert_integer_times(url, &["startTime", "endTime"]);
}

#[test]
fn history_reads_send_integer_times_from_calibrated_now() {
    let cases = [
        (
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(FRACTIONAL_NOW - 3_600_000.),
                to_ms: None,
                limit: None,
            },
            "/openApi/swap/v2/trade/fillHistory",
            ["startTs", "endTs"],
        ),
        (
            Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(1_795_700_000_000.),
                to_ms: None,
                limit: None,
            },
            "/openApi/swap/v2/trade/allOrders",
            ["startTime", "endTime"],
        ),
        (
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: None,
                to_ms: Some(FRACTIONAL_NOW),
                limit: None,
            },
            "/openApi/swap/v2/user/income",
            ["startTime", "endTime"],
        ),
    ];
    for (query, path, names) in cases {
        let mut engine = engine(true);
        let urls = sent_urls(&mut engine, Command::Query { command: 42, query });
        let url = urls
            .iter()
            .find(|url| url.contains(path))
            .unwrap_or_else(|| panic!("no {path} request: {urls:?}"));
        assert_integer_times(url, &names);
    }
}
