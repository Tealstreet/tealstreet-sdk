//! A BingX 100410 on chart history carries only the wait BingX names: the chart's
//! own backoff applies when the venue says only "please try again later".
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Response, VenueError,
    model::{Interval, Limits, Market, MinMax, Precision},
};
use safe_cex_venue_bingx::Bingx;
mod support;

const NOW: f64 = 1_700_000_000_000.;

fn engine() -> Engine<Bingx> {
    let mut engine = support::engine(support::Options {
        public_data_only: true,
        ..Default::default()
    });
    engine.store_mut().set_markets(vec![Market {
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
    }]);
    engine
}

/// The candle command's failure after BingX answers its one request with `headers` and `body`.
fn refused(headers: &str, body: &str) -> VenueError {
    let mut e = engine();
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: Interval::M5,
            limit: Some(10),
            from_ms: None,
            to_ms: None,
        }),
    );
    let requests: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].url.contains("/openApi/swap/v3/quote/klines"));
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: requests[0].id,
            response: Response {
                status: 200,
                headers,
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.)
        .into_iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .expect("the candle command fails")
}

#[test]
fn an_unnamed_100410_on_chart_history_sets_no_retry_after() {
    let error = refused("", r#"{"code":100410,"msg":"请稍后重试","data":{}}"#);
    assert_eq!(error.kind, Some(ErrorKind::RateLimited));
    assert_eq!(error.venue_code.as_deref(), Some("100410"));
    assert_eq!(
        error.retry_after_ms, None,
        "the chart's ladder applies, not a 5 min floor"
    );
}

#[test]
fn a_named_unblock_wait_is_still_carried() {
    let error = refused(
        "Retry-After: 30",
        r#"{"code":100410,"msg":"请稍后重试","data":{}}"#,
    );
    assert_eq!(error.kind, Some(ErrorKind::RateLimited));
    assert_eq!(error.retry_after_ms, Some(30_000.));
}
