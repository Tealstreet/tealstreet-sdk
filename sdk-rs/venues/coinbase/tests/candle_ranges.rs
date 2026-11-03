use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response, TimerId};
use safe_cex_venue_coinbase::{Coinbase, Options};

const NOW: f64 = 1_700_100_000_000.;
const START: u64 = 1_700_000_040;
struct Session {
    engine: Engine<Coinbase>,
    now: f64,
    requests: Vec<HttpRequest>,
    timers: Vec<(f64, TimerId)>,
    result: Option<Result<String, String>>,
}
impl Session {
    fn new() -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(
            Coinbase::new(Options {
                spot_only: true,
                public_data_only: true,
                ..Default::default()
            })
            .unwrap(),
        );
        engine.start(NOW);
        let mut s = Self {
            engine,
            now: NOW,
            requests: vec![],
            timers: vec![],
            result: None,
        };
        let catalogue = s.next();
        s.reply(&catalogue, 200, r#"{"products":[{"product_id":"ARB-USDC","product_type":"SPOT","base_currency_id":"ARB","quote_currency_id":"USDC","status":"TRADING","base_increment":"0.01","price_increment":"0.001","base_min_size":"0.01","base_max_size":"1000000","price":"1"}]}"#);
        assert!(s.engine.store().loaded().markets);
        s
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::SetTimer { timer, at_ms } => self.timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => self.timers.retain(|(_, t)| *t != timer),
                Action::Notice(Notice::CommandDone {
                    command: 71,
                    result,
                }) => self.result = Some(Ok(result)),
                Action::Notice(Notice::CommandFailed { command: 71, error }) => {
                    self.result = Some(Err(error.legacy_text()))
                }
                _ => {}
            }
        }
    }
    fn next(&mut self) -> HttpRequest {
        for _ in 0..20 {
            self.collect();
            if !self.requests.is_empty() {
                return self.requests.remove(0);
            }
            assert!(
                self.result.is_none(),
                "command ended before next candle page: {:?}",
                self.result
            );
            self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (at, timer) = self.timers.remove(0);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
        }
        panic!("no candle request");
    }
    fn fetch(
        &mut self,
        interval: Interval,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        self.engine.handle(
            self.now,
            Event::Command(Command::FetchCandles {
                command: 71,
                symbol: "ARB-USDC".into(),
                interval,
                limit,
                from_ms,
                to_ms,
            }),
        );
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
}
fn parameter<'a>(request: &'a HttpRequest, key: &str) -> &'a str {
    request
        .url
        .split('?')
        .nth(1)
        .unwrap()
        .split('&')
        .filter_map(|part| part.split_once('='))
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
}
fn candles(times: impl IntoIterator<Item = u64>) -> String {
    serde_json::json!({"candles": times.into_iter().map(|start| serde_json::json!({
        "start":start.to_string(),"open":"1","high":"3","low":"0.5","close":"2","volume":"4"
    })).collect::<Vec<_>>()})
    .to_string()
}

#[test]
fn thousand_minute_spot_range_uses_four_bounded_pages_and_returns_contiguous_bars() {
    let mut s = Session::new();
    s.fetch(
        Interval::M1,
        Some(1000),
        Some(START as f64 * 1000.),
        Some((START + 1000 * 60) as f64 * 1000.),
    );
    let mut expected_start = START;
    for count in [300, 300, 300, 100] {
        let request = s.next();
        assert!(request.url.contains("/market/products/ARB-USD/candles?"));
        assert!(request.headers.is_empty());
        assert_eq!(parameter(&request, "granularity"), "ONE_MINUTE");
        let start = parameter(&request, "start").parse::<u64>().unwrap();
        let end = parameter(&request, "end").parse::<u64>().unwrap();
        assert_eq!(start, expected_start);
        assert_eq!(
            end - start,
            count * 60,
            "each physical range must cover at most300 candles"
        );
        assert!(parameter(&request, "limit").parse::<u64>().unwrap() <= 300);
        // Descending response order and a repeated first row exercise canonical merging.
        let mut times = (0..count).map(|i| start + i * 60).rev().collect::<Vec<_>>();
        if count < 300 {
            times.push(start);
        }
        s.reply(&request, 200, &candles(times));
        s.collect();
        if count == 300 {
            assert!(s.result.is_none(), "no partial command result");
        }
        expected_start = end;
    }
    let rows: Vec<serde_json::Value> = serde_json::from_str(&s.result.unwrap().unwrap()).unwrap();
    assert_eq!(rows.len(), 1000);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            row["timestamp"].as_f64().unwrap(),
            (START + i as u64 * 60) as f64
        );
        assert_eq!(row["close"], 2.);
    }
}

#[test]
fn implicit_range_honors_a_thousand_bar_limit_instead_of_clamping_to_one_page() {
    let mut s = Session::new();
    s.fetch(
        Interval::M1,
        Some(1000),
        None,
        Some((START + 60000) as f64 * 1000.),
    );
    let request = s.next();
    assert_eq!(parameter(&request, "start").parse::<u64>().unwrap(), START);
    assert_eq!(
        parameter(&request, "end").parse::<u64>().unwrap(),
        START + 18000
    );
}

#[test]
fn negative_start_is_clamped_and_empty_ranges_complete_without_http() {
    let mut s = Session::new();
    s.fetch(Interval::M1, Some(300), Some(-60_000.), Some(120_000.));
    let request = s.next();
    assert_eq!(parameter(&request, "start"), "0");
    assert_eq!(parameter(&request, "end"), "120");
    s.reply(&request, 200, &candles([0, 60]));
    s.collect();
    assert!(s.result.take().unwrap().is_ok());
    s.fetch(Interval::M1, None, Some(120_000.), Some(120_000.));
    s.collect();
    assert!(s.requests.is_empty());
    assert_eq!(s.result, Some(Ok("[]".into())));
}
