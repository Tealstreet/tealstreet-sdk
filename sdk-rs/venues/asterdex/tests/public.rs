//! Shapes and depth values from the official Aster V3 API's market-data examples.
//! github.com/asterdex/api-docs: V3(Recommended)/EN/aster-finance-futures-api-v3.md
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, PublicFeedPolicy, Response, SocketChannel,
    SocketId, TimerId,
};
mod support;
use safe_cex_venue_asterdex::{AsterDex, sign::Version};
use support::Options;

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"},{"filterType":"MAX_NUM_ALGO_ORDERS","limit":100}]},{"symbol":"OLDUSDT","status":"SETTLING","baseAsset":"OLD","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3}]}"#;
const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"4.00000200","priceChangePercent":"-95.960","volume":"8913.30000000","quoteVolume":"15.30000000"}]"#;
const DOCUMENTED_DEPTH: &str = r#"{"stream":"btcusdt@depth20@100ms","data":{"e":"depthUpdate","E":1571889248277,"T":1571889248276,"s":"BTCUSDT","U":390497796,"u":390497878,"pu":390497794,"b":[["7403.89","0.002"],["7403.90","3.906"],["7404.00","1.428"],["7404.85","5.239"],["7405.43","2.562"]],"a":[["7405.96","3.340"],["7406.63","4.525"],["7407.08","2.475"],["7407.15","4.800"],["7407.20","0.175"]]}}"#;

struct Harness {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<(SocketId, String, Option<SocketChannel>)>,
    sent: Vec<String>,
    timers: Vec<(TimerId, f64)>,
    closed: Vec<SocketId>,
    done: Vec<(u32, Result<String, String>)>,
    fired: usize,
}

impl Harness {
    fn start(options: Options) -> Self {
        Self::start_with_policy(options, PublicFeedPolicy::Direct)
    }

    fn start_with_policy(options: Options, policy: PublicFeedPolicy) -> Self {
        let mut harness = Self {
            engine: support::engine(options),
            now: 1_700_000_000_000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            sent: Vec::new(),
            timers: Vec::new(),
            closed: Vec::new(),
            done: Vec::new(),
            fired: 0,
        };
        harness.engine.set_public_feed_policy(policy).unwrap();
        harness.engine.start(harness.now);
        harness.collect();
        harness
    }

    fn ready() -> Self {
        let mut harness = Self::start(Options {
            public_data_only: true,
            ..Options::default()
        });
        harness.load();
        harness
    }

    fn load(&mut self) {
        self.bootstrap();
        let socket = self.socket();
        self.event(Event::WsOpened { socket });
    }

    fn bootstrap(&mut self) {
        if self
            .requests
            .iter()
            .any(|request| request.url.ends_with("/time"))
        {
            self.reply("/time", 200, r#"{"serverTime":1700000000000}"#);
        }
        self.reply("/exchangeInfo", 200, MARKETS);
        self.reply("/leverageBracket", 200, "[]");
        self.reply("/ticker/24hr", 200, DAILY);
        self.reply(
            "/openInterest",
            200,
            r#"[{"symbol":"BTCUSDT","openInterest":"123.45"}]"#,
        );
    }

    fn socket(&self) -> SocketId {
        self.sockets.last().unwrap().0
    }
    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now)
            .into_iter()
            .collect::<Vec<_>>()
        {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect {
                    socket,
                    url,
                    channel,
                    ..
                } => self.sockets.push((socket, url, channel)),
                Action::WsSend { text, .. } => self.sent.push(text),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(value, _)| *value != timer),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())))
                }
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }
    fn reply(&mut self, suffix: &str, status: u16, body: &str) {
        let position = self
            .requests
            .iter()
            .position(|request| request.url.split('?').next().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix}"));
        let request = self.requests.remove(position);
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn message(&mut self, text: &str) {
        self.event(Event::WsMessage {
            socket: self.socket(),
            text,
        });
    }
    fn subscribe(&mut self) {
        self.command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        });
    }
    fn advance(&mut self, ms: f64) {
        let until = self.now + ms;
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first() else {
                break;
            };
            if at > until {
                break;
            }
            self.timers.remove(0);
            self.now = at;
            self.event(Event::TimerFired { timer });
            self.fired += 1;
        }
        self.now = until;
    }
}

#[test]
fn bootstrap_uses_aster_precision_and_seed_sources() {
    let h = Harness::ready();
    assert_eq!(h.engine.store().markets().len(), 1);
    let market = h.engine.store().market("BTCUSDT").unwrap();
    assert_eq!(market.id, "BTCUSDT");
    assert_eq!(market.contract_size, Some(1.0));
    assert_eq!(market.precision.price, 0.01);
    assert_eq!(market.precision.amount, 0.001);
    assert_eq!(market.limits.amount.max, 1000.0);
    assert_eq!(market.limits.leverage.max, 10.0);
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(ticker.last, 4.000002);
    assert_eq!(ticker.open_interest, 123.45);
    assert_eq!(
        (ticker.bid, ticker.ask, ticker.mark, ticker.funding_rate),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert_eq!(h.sockets[0].2, Some(SocketChannel::Public));
    assert_eq!(h.sockets[0].1, "wss://fstream.asterdex.com/stream");
    assert!(
        h.sent
            .iter()
            .any(|s| s.contains("!ticker@arr") && s.contains("!markPrice@arr@1s"))
    );
    assert!(
        !h.requests
            .iter()
            .any(|request| request.url.contains("bookTicker")
                || request.url.contains("premiumIndex"))
    );
}

#[test]
fn explicit_v3_testnet_public_urls_and_clock_safe_defaults() {
    let mut h = Harness::start(Options {
        testnet: true,
        public_data_only: true,
        api_version: Some(Version::V3),
        ..Options::default()
    });
    assert!(h.requests.iter().all(|request| {
        request
            .url
            .starts_with("https://fapi.asterdex-testnet.com/fapi/v3/")
    }));
    h.load();
    assert_eq!(h.sockets[0].1, "wss://fstream5.asterdex-testnet.com/stream");
}

#[test]
fn documented_partial_snapshot_is_sorted_and_authoritative() {
    let mut h = Harness::ready();
    h.subscribe();
    h.message(DOCUMENTED_DEPTH);
    let book = h.engine.book("BTCUSDT").unwrap();
    assert_eq!(book.best_bid(), Some(7405.43));
    assert_eq!(book.best_ask(), Some(7405.96));
    assert_eq!(book.bids().len(), 5);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().bid, 7405.43);
    h.message(r#"{"stream":"btcusdt@depth20@100ms","data":{"u":390497879,"b":[["7400","2"]],"a":[["7401","3"]]}}"#);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids(), [(7400.0, 2.0)]);
    h.message(DOCUMENTED_DEPTH);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(7400.0));
}

#[test]
fn malformed_crossed_zero_numeric_and_diff_frames_preserve_good_snapshot() {
    let mut h = Harness::ready();
    h.subscribe();
    h.message(DOCUMENTED_DEPTH);
    for pair in [
        r#"[100,"1"]"#,
        r#"["100",true]"#,
        r#"["100","0"]"#,
        r#"["100junk","1"]"#,
        r#"["100","1","extra"]"#,
        r#"["NaN","1"]"#,
        r#"["","1"]"#,
        r#"["-1","1"]"#,
    ] {
        let message = format!(
            r#"{{"stream":"btcusdt@depth20@100ms","data":{{"b":[{pair}],"a":[["101","1"]]}}}}"#
        );
        h.message(&message);
        assert_eq!(
            h.engine.book("BTCUSDT").unwrap().best_bid(),
            Some(7405.43),
            "{pair}"
        );
    }
    for message in [
        r#"{"stream":"btcusdt@depth20@100ms","data":{"b":[],"a":[["101","1"]]}}"#,
        r#"{"stream":"btcusdt@depth20@100ms","data":{"b":[["102","1"]],"a":[["101","1"]]}}"#,
        r#"{"stream":"btcusdt@depth","data":{"b":[["100","1"]],"a":[["101","1"]]}}"#,
        r#"{"e":"depthUpdate","s":"BTCUSDT","b":[["100","1"]],"a":[["101","1"]]}"#,
    ] {
        h.message(message);
        assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(7405.43));
    }
}

#[test]
fn duplicate_subscribers_unsubscribe_independently_and_reconnect_retains_topics() {
    let mut h = Harness::ready();
    h.subscribe();
    h.subscribe();
    assert_eq!(
        h.sent
            .iter()
            .filter(|text| text.contains("depth20") && text.contains("SUBSCRIBE"))
            .count(),
        1
    );
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    assert!(!h.sent.iter().any(|text| text.contains("UNSUBSCRIBE")));
    h.message(DOCUMENTED_DEPTH);
    let old_socket = h.socket();
    h.event(Event::WsClosed {
        socket: old_socket,
        code: 1006,
    });
    h.advance(500.0);
    let new_socket = h.socket();
    assert_ne!(old_socket, new_socket);
    h.event(Event::WsOpened { socket: new_socket });
    assert!(h.sent.last().unwrap().contains("btcusdt@depth20@100ms"));
    h.message(
        r#"{"stream":"btcusdt@depth20@100ms","data":{"u":1,"b":[["100","1"]],"a":[["101","1"]]}}"#,
    );
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(100.0));
    h.event(Event::WsMessage {
        socket: old_socket,
        text: DOCUMENTED_DEPTH,
    });
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(100.0));
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    assert!(h.engine.book("BTCUSDT").is_none());
    assert!(h.sent.last().unwrap().contains("UNSUBSCRIBE"));
}

#[test]
fn aggregate_and_raw_events_preserve_partial_fields_and_funding_zero() {
    let mut h = Harness::ready();
    h.message(
        r#"{"stream":"!ticker@arr","data":[{"s":"BTCUSDT","c":"100","P":"2","v":"3","q":"4"}]}"#,
    );
    h.message(r#"[{"e":"24hrTicker","s":"BTCUSDT","c":"101"},{"e":"markPriceUpdate","s":"BTCUSDT","p":"100.5","i":"100.4","r":"0.0001","T":1700000100000}]"#);
    h.message(r#"{"stream":"!markPrice@arr@1s","data":[{"s":"BTCUSDT","r":"0"}]}"#);
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (
            ticker.last,
            ticker.percentage,
            ticker.volume,
            ticker.quote_volume
        ),
        (101.0, 2.0, 3.0, 4.0)
    );
    assert_eq!(
        (ticker.mark, ticker.index, ticker.funding_rate),
        (100.5, 100.4, 0.0)
    );
    assert_eq!(ticker.funding_time, 1700000100000.0);
}

#[test]
fn single_and_wrapped_interest_payloads_and_failure_retain_values() {
    let mut h = Harness::ready();
    h.advance(15_000.0);
    h.reply(
        "/openInterest",
        200,
        r#"{"data":{"symbol":"BTCUSDT","openInterest":77.5}}"#,
    );
    assert_eq!(
        h.engine.store().ticker("BTCUSDT").unwrap().open_interest,
        77.5
    );
    h.advance(15_000.0);
    for delay in [1000.0, 2000.0, 3000.0] {
        h.reply(
            "/openInterest",
            500,
            r#"[{"symbol":"BTCUSDT","openInterest":"0"}]"#,
        );
        assert_eq!(
            h.engine.store().ticker("BTCUSDT").unwrap().open_interest,
            77.5
        );
        assert!(
            !h.requests
                .iter()
                .any(|request| request.url.ends_with("openInterest?symbol=BTCUSDT"))
        );
        h.advance(delay);
    }
    h.reply(
        "/openInterest",
        500,
        r#"[{"symbol":"BTCUSDT","openInterest":"0"}]"#,
    );
    assert!(
        h.requests
            .iter()
            .any(|request| request.url.ends_with("openInterest?symbol=BTCUSDT"))
    );
    h.reply(
        "/openInterest",
        200,
        r#"{"symbol":"BTCUSDT","openInterest":"88.5"}"#,
    );
    assert_eq!(
        h.engine.store().ticker("BTCUSDT").unwrap().open_interest,
        88.5
    );
}

#[test]
fn silent_socket_reconnects_with_protocol_ping() {
    let mut h = Harness::ready();
    assert!(
        h.sent
            .iter()
            .any(|text| text == r#"{"id":-1,"method":"LIST_SUBSCRIPTIONS"}"#)
    );
    let socket = h.socket();
    h.advance(25_000.0);
    assert!(h.closed.contains(&socket));
    h.advance(500.0);
    assert_ne!(h.socket(), socket);
}

#[test]
fn streamed_klines_decode_and_malformed_rows_fail_atomically() {
    let mut h = Harness::ready();
    h.command(Command::FetchCandles {
        command: 12,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: Some(2),
        from_ms: Some(1499040000000.0),
        to_ms: None,
    });
    let index = h
        .requests
        .iter()
        .position(|request| request.url.contains("/klines?"))
        .unwrap();
    let request = h.requests.remove(index);
    assert!(request.url.contains("startTime=1499040000000"));
    h.event(Event::HttpStart {
        request: request.id,
        status: 200,
        headers: "",
    });
    for data in br#"[[1499040000000,"0.01634790","0.80000000","0.01575800","0.01577100","148976.11427815",1499644799999,"2434.19055334",308,"1756.87402397","28.46694368","0"]]"#.chunks(13) {
        h.event(Event::HttpChunk { request: request.id, data });
    }
    h.event(Event::HttpEnd {
        request: request.id,
    });
    let result = h
        .done
        .iter()
        .find(|(command, _)| *command == 12)
        .unwrap()
        .1
        .as_ref()
        .unwrap();
    assert!(result.contains("1499040000.0"));
    assert!(result.contains("0.0163479"));
    h.command(Command::FetchCandles {
        command: 13,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: None,
        from_ms: None,
        to_ms: None,
    });
    h.reply(
        "/klines",
        200,
        r#"[[1499040000000,"1","2","0.5","1.5","2",0,"0",1,"0","0","0"],["bad"]]"#,
    );
    let error = h
        .done
        .iter()
        .find(|(command, _)| *command == 13)
        .unwrap()
        .1
        .clone()
        .unwrap_err();
    assert!(
        error.starts_with("Aster klines HTTP 200 or malformed payload: row 1: "),
        "{error}"
    );
}

#[test]
fn failed_optional_interest_does_not_block_public_start() {
    let mut h = Harness::start(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.reply("/time", 200, r#"{"serverTime":1700000000000}"#);
    h.reply("/exchangeInfo", 200, MARKETS);
    h.reply("/leverageBracket", 200, "[]");
    h.reply("/ticker/24hr", 200, DAILY);
    h.reply(
        "/openInterest",
        400,
        r#"{"code":-1102,"msg":"symbol required"}"#,
    );
    assert_eq!(h.sockets.len(), 1);
    assert_eq!(
        h.engine.store().ticker("BTCUSDT").unwrap().open_interest,
        0.0
    );
    h.advance(250.0);
    h.reply(
        "/openInterest",
        400,
        r#"{"code":-1102,"msg":"symbol required"}"#,
    );
    assert!(
        h.requests
            .iter()
            .any(|request| request.url.ends_with("openInterest?symbol=BTCUSDT"))
    );
}

#[test]
fn one_off_rest_depth_sorts_levels_and_preserves_quote_totals() {
    let mut h = Harness::ready();
    h.command(Command::FetchOrderBook {
        command: 50,
        symbol: "BTCUSDT".into(),
        limit: Some(100),
    });
    h.reply("/depth",200,r#"{"lastUpdateId":1027024,"bids":[["100","2"],["101","1"]],"asks":[["103","2"],["102","1"]]}"#);
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == 50)
        .unwrap()
        .1
        .as_ref()
        .unwrap();
    let result: serde_json::Value = serde_json::from_str(result).unwrap();
    assert_eq!(
        result,
        serde_json::json!({"bids":[{"price":101.0,"amount":1.0,"total":101.0},{"price":100.0,"amount":2.0,"total":301.0}],"asks":[{"price":102.0,"amount":1.0,"total":102.0},{"price":103.0,"amount":2.0,"total":308.0}]})
    );
}
#[test]
fn ticker_seed_retains_native_rows_without_active_market_metadata() {
    let mut h = Harness::start(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.reply("/time", 200, r#"{"serverTime":1700000000000}"#);
    h.reply("/exchangeInfo", 200, MARKETS);
    h.reply("/leverageBracket", 200, "[]");
    h.reply("/ticker/24hr",200,r#"[{"symbol":"OLDUSDT","lastPrice":"1","priceChangePercent":"0","volume":"2","quoteVolume":"2"}]"#);
    h.reply("/openInterest", 200, "[]");
    assert_eq!(h.engine.store().markets().len(), 1);
    assert_eq!(h.engine.store().tickers().len(), 1);
    assert_eq!(h.engine.store().tickers()[0].symbol, "OLDUSDT");
}

#[test]
fn catalogue_skips_bad_neighbor_and_account_starts_before_ticker_response() {
    let mut h = Harness::start(Options {
        key: Some("synthetic-key".into()),
        secret: Some("synthetic-secret".into()),
        ..Options::default()
    });
    h.reply("/time", 200, r#"{"serverTime":1700000000000}"#);
    h.reply("/exchangeInfo", 200, &MARKETS.replacen("[", "[{},", 1));
    assert_eq!(h.engine.store().markets().len(), 1);
    assert!(h.requests.iter().any(|r| r.url.contains("/account")));
    assert!(!h.engine.store().loaded().tickers);
    h.reply("/leverageBracket", 200, "[]");
    h.reply("/ticker/24hr", 503, "unavailable");
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .find(|(_, at)| jittered(*at - h.now, 1000.))
        .unwrap();
    h.now = at;
    h.event(Event::TimerFired { timer });
    h.reply("/ticker/24hr", 200, &DAILY.replacen("[", "[{},", 1));
    assert_eq!(h.engine.store().tickers().len(), 1);
    assert_eq!(h.engine.store().tickers()[0].last, 4.000002);
}

#[test]
fn all_malformed_discovery_rows_retry_and_preserve_existing_catalogue_and_quotes() {
    let mut h = Harness::start(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.reply("/time", 200, r#"{"serverTime":1700000000000}"#);
    h.reply("/exchangeInfo", 200, r#"{"symbols":[{}]}"#);
    assert!(!h.engine.store().loaded().markets);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/leverageBracket"))
    );
    h.advance(1000.0);
    h.reply("/exchangeInfo", 200, MARKETS);
    let markets = h.engine.store().markets().to_vec();
    h.reply("/leverageBracket", 200, "[]");
    let prior = safe_cex_engine::model::Ticker {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        last: 123.0,
        ..Default::default()
    };
    h.engine.store_mut().set_tickers(vec![prior.clone()]);
    h.reply("/ticker/24hr", 200,
        r#"[{},{"symbol":"","lastPrice":"1","priceChangePercent":"0","volume":"1","quoteVolume":"1"}]"#);
    assert_eq!(h.engine.store().markets(), markets);
    assert_eq!(h.engine.store().ticker("BTCUSDT"), Some(&prior));
    h.advance(1000.0);
    h.reply("/ticker/24hr", 200, DAILY);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 4.000002);
}

#[test]
fn disconnect_clears_visible_book_until_a_new_partial_snapshot() {
    let mut h = Harness::ready();
    h.subscribe();
    h.message(DOCUMENTED_DEPTH);
    assert!(h.engine.book("BTCUSDT").is_some());
    h.engine.take_frame(h.now);
    let old_socket = h.socket();
    h.event(Event::WsClosed {
        socket: old_socket,
        code: 1006,
    });
    assert!(h.engine.book("BTCUSDT").is_none());
    let frame = h.engine.take_frame(h.now).unwrap();
    assert!(
        safe_cex_engine::wire::sections(frame).any(|(kind, payload)| {
            kind == safe_cex_engine::wire::section::BOOK && payload[4..12] == [0; 8]
        })
    );
    h.event(Event::WsMessage {
        socket: old_socket,
        text: DOCUMENTED_DEPTH,
    });
    assert!(h.engine.book("BTCUSDT").is_none());
    h.advance(500.0);
    let socket = h.socket();
    assert_ne!(old_socket, socket);
    h.event(Event::WsOpened { socket });
    assert!(h.engine.book("BTCUSDT").is_none());
    h.message(DOCUMENTED_DEPTH);
    assert!(h.engine.book("BTCUSDT").is_some());
}

#[test]
fn malformed_market_and_ticker_rows_are_skipped_without_halting_startup() {
    let mut h = Harness::start(Options {
        public_data_only: true,
        ..Options::default()
    });
    if h.requests.iter().any(|r| r.url.ends_with("/time")) {
        h.reply("/time", 200, r#"{"serverTime":1700000000000}"#);
    }
    h.reply(
        "/exchangeInfo",
        200,
        &MARKETS.replace(
            r#"{"symbol":"OLDUSDT""#,
            r#"{"symbol":"BADUSDT","status":"TRADING","pricePrecision":"x"},{"symbol":"OLDUSDT""#,
        ),
    );
    h.reply("/leverageBracket", 200, "[]");
    h.reply(
        "/ticker/24hr",
        200,
        &DAILY.replace('[', r#"[{"symbol":"BADUSDT","lastPrice":1},"#),
    );
    assert!(h.engine.store().market("BTCUSDT").is_some());
    assert!(h.engine.store().market("BADUSDT").is_none());
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 4.000002);
    assert!(h.requests.iter().any(|r| r.url.contains("/openInterest")));
}

#[test]
fn supervisor_backs_off_open_flaps_and_retires_a_connection_that_never_opens() {
    let mut h = Harness::ready();
    let mut previous = 0.0;
    for nominal in [500.0, 1000.0, 2000.0] {
        let socket = h.socket();
        h.event(Event::WsClosed { socket, code: 1006 });
        let (timer, at) = *h.timers.last().unwrap();
        let delay = at - h.now;
        assert!((nominal * 0.75..=nominal).contains(&delay));
        assert!(delay > previous);
        previous = delay;
        h.now = at;
        h.event(Event::TimerFired { timer });
        assert_ne!(h.socket(), socket);
        if nominal < 2000.0 {
            h.event(Event::WsOpened { socket: h.socket() });
        }
    }
    let stalled = h.socket();
    h.advance(10_000.0);
    assert!(h.closed.contains(&stalled));
    let count = h.sockets.len();
    h.event(Event::WsOpened { socket: stalled });
    h.event(Event::WsClosed {
        socket: stalled,
        code: 1006,
    });
    assert_eq!(h.sockets.len(), count);
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::start_with_policy(
        Options {
            public_data_only: true,
            ..Options::default()
        },
        PublicFeedPolicy::Shared,
    );
    h.bootstrap();
    h.subscribe();
    h.advance(600_000.0);
    assert!(h.sockets.is_empty(), "no public connects: {:?}", h.sockets);
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    // Unanswered reads now expire at their bound, so the interest and clock
    // refreshes keep cycling; nothing else is issued.
    assert!(h.fired >= 2);
    assert!(
        h.requests.iter().all(|request| {
            let path = request.url.split('?').next().unwrap();
            path.ends_with("/openInterest") || path.ends_with("/time")
        }),
        "only interest and clock refreshes: {:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
}

// AsterDEX used to serve unsupported intervals as 1h candles under the
// requested interval; they are now rejected and nothing is requested.
#[test]
fn unsupported_candle_intervals_are_rejected_not_served_as_one_hour() {
    let mut h = Harness::ready();
    let sent = h.sent.len();
    h.command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::H8,
    });
    assert_eq!(h.sent.len(), sent, "no kline subscription is sent");
    let requests = h.requests.len();
    h.command(Command::FetchCandles {
        command: 14,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::H8,
        limit: None,
        from_ms: None,
        to_ms: None,
    });
    assert_eq!(h.requests.len(), requests);
    let (_, result) = h.done.iter().find(|(command, _)| *command == 14).unwrap();
    assert_eq!(
        result.as_ref().unwrap_err(),
        "AsterDEX does not support 8h candles"
    );
}
