use safe_cex_engine::Engine;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Notice, PublicFeedPolicy, Response, SocketId, TimerId,
    Venue,
};
use safe_cex_venue_polymarket::{Options, Polymarket};

// Gamma encoded arrays and raw CLOB events: official Gamma + realtime-data docs.
const MARKETS: &str = r#"[{"conditionId":"0x123","slug":"election","question":"Who wins?","description":"Test outcome","endDate":"2026-12-31","clobTokenIds":"[\"107505882767731489358349912513945399560393482969656700824895970500493757150417\",\"2\"]","outcomes":"[\"Yes\",\"No\"]","orderPriceMinTickSize":0.01,"orderMinSize":5,"active":true,"closed":false,"negRisk":true}]"#;
const TOKEN: &str =
    "107505882767731489358349912513945399560393482969656700824895970500493757150417";
struct H {
    e: Engine<Polymarket>,
    now: f64,
    http: Vec<HttpRequest>,
    socket: Option<SocketId>,
    sends: Vec<String>,
    timers: Vec<(TimerId, f64)>,
    closed: Vec<SocketId>,
    done: Vec<Result<String, String>>,
    logs: Vec<String>,
    connects: usize,
    fired: usize,
}
/// A jittered retry delay lands in `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
impl H {
    fn new() -> Self {
        Self::with_policy(PublicFeedPolicy::Direct)
    }
    fn with_policy(policy: PublicFeedPolicy) -> Self {
        let mut h = Self {
            e: safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
                public_data_only: true,
                ..Default::default()
            })),
            now: 1_700_000_000_000.0,
            http: vec![],
            socket: None,
            sends: vec![],
            timers: vec![],
            closed: vec![],
            done: vec![],
            logs: vec![],
            connects: 0,
            fired: 0,
        };
        h.e.set_public_feed_policy(policy).unwrap();
        h.e.start(h.now);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            match a {
                Action::Http(r) => self.http.push(r),
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    assert_eq!(channel, Some(safe_cex_engine::SocketChannel::Public));
                    self.socket = Some(socket);
                    self.connects += 1;
                }
                Action::WsSend { text, .. } => self.sends.push(text),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::Notice(Notice::CommandDone { result, .. }) => self.done.push(Ok(result)),
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    self.done.push(Err(error.legacy_text()))
                }
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => self.logs.push(message),
                _ => {}
            }
        }
    }
    /// Finds the retry timer jittered from `nominal`, then advances the clock
    /// to the nominal delay, which is never before the jittered deadline.
    fn retry_after(&mut self, nominal: f64) -> TimerId {
        let now = self.now;
        let timer = self
            .timers
            .iter()
            .find(|(_, at)| jittered(at - now, nominal))
            .unwrap()
            .0;
        self.now += nominal;
        timer
    }
    fn event(&mut self, e: Event<'_>) {
        self.e.handle(self.now, e);
        self.collect();
    }
    fn response(&mut self, path: &str, status: u16, body: &str) {
        let wrapped = (path.contains("/markets") && body.trim_start().starts_with('['))
            .then(|| format!(r#"{{"markets":{body}}}"#));
        let body = wrapped.as_deref().unwrap_or(body);
        let i = self
            .http
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing{path}: {:?}", self.http));
        let r = self.http.remove(i);
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn load(&mut self) {
        self.response("/markets", 200, MARKETS);
        self.response(
            "/midpoints",
            200,
            &format!(r#"{{"{TOKEN}":"0.5","2":"0.5"}}"#),
        );
    }
    fn subscribe(&mut self) {
        self.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "election-Yes".into(),
        }));
        self.event(Event::WsOpened {
            socket: self.socket.unwrap(),
        });
    }
    fn message(&mut self, s: &str) {
        self.event(Event::WsMessage {
            socket: self.socket.unwrap(),
            text: s,
        });
    }
}
#[test]
fn encoded_outcomes_keep_full_uint256_identity_and_prediction_metadata() {
    let mut h = H::new();
    assert!(h.socket.is_none());
    h.load();
    assert!(h.socket.is_none());
    let m = &h.e.store().markets()[0];
    assert_eq!(m.id, TOKEN);
    assert_eq!(m.quote, "pUSD");
    assert_eq!(m.symbol, "election-Yes");
    assert_eq!(m.limits.leverage.max, 1.0);
    let meta = &h.e.store().prediction_markets().unwrap()[0];
    assert_eq!(meta.outcome, "Yes");
    assert_eq!(meta.question, "Who wins?");
    assert!(meta.neg_risk);
    assert_eq!(h.e.store().tickers()[0].mark, 0.5);
    assert!(h.e.store().loaded().balance);
}
#[test]
fn malformed_encoded_arrays_are_skipped_and_incomplete_page_is_atomic() {
    let mut h = H::new();
    h.response("/markets",200,r#"[{"conditionId":"x","slug":"bad","clobTokenIds":"[\"1\",\"2\"]","outcomes":"[\"Yes\"]"}]"#);
    assert!(h.e.store().markets().is_empty());
    // An entirely unusable catalogue retries instead of committing an empty one.
    assert!(!h.e.store().loaded().markets);
}
#[test]
fn malformed_gamma_rows_are_skipped_and_null_optional_fields_default() {
    let mut h = H::new();
    let nulls = MARKETS
        .replace(r#""question":"Who wins?""#, r#""question":null"#)
        .replace(r#""description":"Test outcome""#, r#""description":null"#)
        .replace(r#""endDate":"2026-12-31""#, r#""endDate":null"#)
        .replace(r#""closed":false"#, r#""closed":null"#)
        .replace(r#""orderMinSize":5"#, r#""orderMinSize":null"#);
    let body = format!(
        r#"[{{"slug":"missing condition id"}},null,{{"conditionId":"0x9","slug":"bad-tick","clobTokenIds":"[\"9\"]","outcomes":"[\"Yes\"]","orderPriceMinTickSize":"bogus"}},{}]"#,
        &nulls[1..nulls.len() - 1]
    );
    h.response("/markets", 200, &body);
    assert!(h.e.store().loaded().markets);
    assert_eq!(h.e.store().markets().len(), 2);
    let m = h.e.store().market("election-Yes").unwrap();
    assert!(m.active);
    assert_eq!(m.limits.amount.min, 0.0);
    let meta = &h.e.store().prediction_markets().unwrap()[0];
    assert_eq!(meta.question, "");
    assert_eq!(meta.end_date, "");
    assert!(h.logs.iter().any(|l| l
        == "Skipped 3 malformed Polymarket market rows: missing condition id, unknown, bad-tick"));
}
#[test]
fn failed_market_reads_keep_retrying_until_the_venue_loads() {
    let mut h = H::new();
    for failure in [
        (500, "unavailable"),
        (200, "not json"),
        (200, r#"{"markets":[],"next_cursor":""}"#),
        (503, "busy"),
        (200, "{}"),
    ] {
        h.response("/markets", failure.0, failure.1);
        assert!(!h.e.store().loaded().markets);
        assert!(h.http.is_empty());
        let (timer, at_ms) = h.timers.pop().expect("market retry scheduled");
        h.now = at_ms;
        h.event(Event::TimerFired { timer });
    }
    h.load();
    assert!(h.e.store().loaded().markets);
    assert_eq!(h.e.store().tickers()[0].mark, 0.5);
}
#[test]
fn null_active_flag_loads_the_market_as_inactive() {
    let mut h = H::new();
    h.response(
        "/markets",
        200,
        &MARKETS.replace(r#""active":true"#, r#""active":null"#),
    );
    let m = h.e.store().market("election-Yes").unwrap();
    assert!(!m.active);
}
#[test]
fn failed_midpoint_read_retries_the_same_batch_until_tickers_seed() {
    let mut h = H::new();
    h.response("/markets", 200, MARKETS);
    for failure in [(500, "unavailable"), (200, "not a price map")] {
        h.response("/midpoints", failure.0, failure.1);
        assert!(h.e.store().tickers().is_empty());
        assert!(h.http.is_empty());
        let (timer, at_ms) = h.timers.pop().expect("midpoint retry scheduled");
        h.now = at_ms;
        h.event(Event::TimerFired { timer });
        assert!(h.http.iter().all(|r| r.url.contains("/midpoints")));
    }
    h.response(
        "/midpoints",
        200,
        &format!(r#"{{"{TOKEN}":"0.4","2":"0.6"}}"#),
    );
    assert_eq!(h.e.store().ticker("election-Yes").unwrap().mark, 0.4);
    assert_eq!(h.e.store().markets().len(), 2);
}
#[test]
fn market_retry_backoff_doubles_and_caps_at_thirty_seconds() {
    let mut h = H::new();
    let mut delays = vec![];
    for _ in 0..33 {
        h.response("/markets", 503, "busy");
        let (timer, at_ms) = h.timers.pop().expect("market retry scheduled");
        delays.push(at_ms - h.now);
        h.now = at_ms;
        h.event(Event::TimerFired { timer });
    }
    // Shared engine discovery backoff: doubling from one second, capped at thirty.
    let nominal = [1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 30000.0];
    for (delay, nominal) in delays.iter().zip(nominal) {
        assert!(
            jittered(*delay, nominal),
            "{delay} not jittered from {nominal}"
        );
    }
    assert!(delays[5..].iter().all(|d| jittered(*d, 30000.0)));
    assert_eq!(h.http.len(), 1);
}
#[test]
fn full_books_sort_snapshots_and_apply_every_absolute_change_without_truncation() {
    let mut h = H::new();
    h.load();
    h.subscribe();
    h.message(&format!(r#"[{{"event_type":"book","asset_id":"{TOKEN}","timestamp":"100","bids":[{{"price":"0.3","size":"3"}},{{"price":"0.4","size":"4"}}],"asks":[{{"price":"0.7","size":"7"}},{{"price":"0.6","size":"6"}}]}}]"#));
    let book = h.e.venue().book("election-Yes").unwrap();
    assert_eq!(book.bids(), [(0.4, 4.0), (0.3, 3.0)]);
    assert_eq!(book.asks(), [(0.6, 6.0), (0.7, 7.0)]);
    for n in 0..1200 {
        h.message(&format!(r#"{{"event_type":"price_change","timestamp":"101","price_changes":[{{"asset_id":"{TOKEN}","price":"{}","size":"1","side":"BUY","best_bid":"0.4","best_ask":"0.6"}}]}}"#,0.1+n as f64/10000.0));
    }
    assert_eq!(h.e.venue().book("election-Yes").unwrap().bids().len(), 1202);
    h.message(&format!(r#"{{"event_type":"price_change","timestamp":"102","price_changes":[{{"asset_id":"{TOKEN}","price":"0.4","size":"0","side":"BUY","best_bid":"0.3","best_ask":"0.6"}}]}}"#));
    assert_eq!(
        h.e.venue().book("election-Yes").unwrap().best_bid(),
        Some(0.3)
    );
    assert_eq!(
        h.e.store().ticker("election-Yes").unwrap().mark,
        0.44999999999999996
    );
}
#[test]
fn reconnect_discards_book_and_ignores_diffs_until_fresh_snapshot() {
    let mut h = H::new();
    h.load();
    h.subscribe();
    h.message(&format!(r#"{{"event_type":"book","asset_id":"{TOKEN}","bids":[{{"price":"0.4","size":"1"}}],"asks":[]}}"#));
    h.event(Event::WsClosed {
        socket: h.socket.unwrap(),
        code: 1006,
    });
    assert!(h.e.venue().book("election-Yes").is_none());
    let t = h
        .timers
        .iter()
        .find(|(_, at)| (750. ..=1000.).contains(&(*at - h.now)))
        .unwrap()
        .0;
    h.now += 1000.0;
    h.event(Event::TimerFired { timer: t });
    h.event(Event::WsOpened {
        socket: h.socket.unwrap(),
    });
    h.message(&format!(r#"{{"event_type":"price_change","price_changes":[{{"asset_id":"{TOKEN}","price":"0.4","size":"2","side":"BUY"}}]}}"#));
    assert!(h.e.venue().book("election-Yes").is_none());
}
#[test]
fn pong_watchdog_closes_stalled_sockets_and_tick_changes_reach_store() {
    let mut h = H::new();
    h.load();
    h.subscribe();
    assert!(h.sends.contains(&"PING".into()));
    h.message("PONG");
    h.message(&format!(
        r#"{{"event_type":"tick_size_change","asset_id":"{TOKEN}","new_tick_size":"0.001"}}"#
    ));
    assert_eq!(
        h.e.store().market("election-Yes").unwrap().precision.price,
        3.0
    );
    assert_eq!(
        h.e.store().prediction_markets().unwrap()[0].tick_size,
        0.001
    );
    let t = h.timers[0].0;
    h.now += 10000.0;
    h.event(Event::TimerFired { timer: t });
    assert!(h.closed.is_empty());
    let t = h.timers.last().unwrap().0;
    h.now += 10000.0;
    h.event(Event::TimerFired { timer: t });
    assert_eq!(h.closed.len(), 1);
}
#[test]
fn price_history_samples_are_bounded_and_subscriptions_dispose() {
    let mut h = H::new();
    h.load();
    h.event(Event::Command(Command::FetchCandles {
        command: 7,
        symbol: "election-Yes".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: Some(1),
        from_ms: None,
        to_ms: None,
    }));
    h.response(
        "/prices-history",
        200,
        r#"{"history":[{"t":100,"p":0.3},{"t":101,"p":0.4}]}"#,
    );
    assert_eq!(
        h.done.last().unwrap().as_ref().unwrap(),
        r#"[{"timestamp":101000.0,"open":0.4,"high":0.4,"low":0.4,"close":0.4,"volume":0.0}]"#
    );
    h.event(Event::Command(Command::FetchCandles {
        command: 8,
        symbol: "election-Yes".into(),
        interval: safe_cex_engine::model::Interval::S1,
        limit: None,
        from_ms: None,
        to_ms: None,
    }));
    assert!(h.done.last().unwrap().is_err());
    assert!(h.http.is_empty());
}

#[test]
fn stale_public_quotes_trades_ticks_and_malformed_snapshots_cannot_replace_newer_state() {
    let mut h = H::new();
    h.load();
    h.subscribe();
    h.message(&format!(r#"{{"event_type":"book","asset_id":"{TOKEN}","timestamp":"200","bids":[{{"price":"0.4","size":"1"}}],"asks":[]}}"#));
    h.message(&format!(
        r#"{{"event_type":"book","asset_id":"{TOKEN}","timestamp":"201"}}"#
    ));
    assert_eq!(
        h.e.venue().book("election-Yes").unwrap().bids(),
        [(0.4, 1.0)]
    );
    for timestamp in [202, 201] {
        let price = if timestamp == 202 { "0.6" } else { "0.1" };
        h.message(&format!(r#"{{"event_type":"price_change","timestamp":"{timestamp}","price_changes":[{{"asset_id":"{TOKEN}","price":"{price}","size":"1","side":"BUY","best_bid":"{price}","best_ask":"0.8"}}]}}"#));
        h.message(&format!(r#"{{"event_type":"last_trade_price","timestamp":"{timestamp}","asset_id":"{TOKEN}","price":"{price}"}}"#));
        h.message(&format!(r#"{{"event_type":"tick_size_change","timestamp":"{timestamp}","asset_id":"{TOKEN}","new_tick_size":"{}"}}"#,if timestamp==202{"0.001"}else{"0.01"}));
    }
    let t = h.e.store().ticker("election-Yes").unwrap();
    assert_eq!(t.bid, 0.6);
    assert_eq!(t.last, 0.6);
    assert_eq!(
        h.e.store().prediction_markets().unwrap()[0].tick_size,
        0.001
    );
}
#[test]
fn returned_candle_points_respect_subsecond_inclusive_bounds() {
    let mut h = H::new();
    h.load();
    h.event(Event::Command(Command::FetchCandles {
        command: 7,
        symbol: "election-Yes".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: None,
        from_ms: Some(100_500.0),
        to_ms: Some(102_000.0),
    }));
    h.response(
        "/prices-history",
        200,
        r#"{"history":[{"t":100,"p":0.3},{"t":101,"p":0.4},{"t":102,"p":0.5},{"t":103,"p":0.6}]}"#,
    );
    let result = h.done.last().unwrap().as_ref().unwrap();
    assert!(!result.contains("100000"));
    assert!(result.contains("101000"));
    assert!(result.contains("102000"));
    assert!(!result.contains("103000"));
}

#[test]
fn price_history_requests_supply_a_valid_time_selector_and_reject_upper_only_bounds() {
    // Official /prices-history rejects a missing time selector and endTs alone.
    for (from, to, selector, expected) in [
        (None, None, "&interval=max", vec![102000.0, 103000.0]),
        (Some(1000.0), None, "&startTs=1", vec![102000.0, 103000.0]),
        (
            Some(100500.0),
            None,
            "&startTs=100",
            vec![102000.0, 103000.0],
        ),
        (
            Some(100500.0),
            Some(102000.0),
            "&startTs=100&endTs=102",
            vec![101000.0, 102000.0],
        ),
    ] {
        let mut h = H::new();
        h.load();
        h.event(Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "election-Yes".into(),
            interval: safe_cex_engine::model::Interval::M5,
            limit: Some(2),
            from_ms: from,
            to_ms: to,
        }));
        let url = &h.http[0].url;
        let valid = url.contains("&startTs=")
            || (url.contains("&interval=max") && !url.contains("&endTs="));
        let expected_url = format!(
            "https://clob.polymarket.com/prices-history?market={TOKEN}{selector}&fidelity=5"
        );
        let actual_url = url.clone();
        h.response(
            "/prices-history",
            if valid { 200 } else { 400 },
            if valid {
                r#"{"history":[{"t":100,"p":0.1},{"t":101,"p":0.2},{"t":102,"p":0.3},{"t":103,"p":0.4}]}"#
            } else {
                r#"{"error":"invalid filters: the time component is mandatory"}"#
            },
        );
        let candles: Vec<serde_json::Value> =
            serde_json::from_str(h.done.last().unwrap().as_ref().unwrap()).unwrap();
        assert_eq!(actual_url, expected_url);
        assert_eq!(
            candles
                .iter()
                .map(|c| c["timestamp"].as_f64().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }
    let mut h = H::new();
    h.load();
    h.event(Event::Command(Command::FetchCandles {
        command: 8,
        symbol: "election-Yes".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: None,
        from_ms: None,
        to_ms: Some(102000.0),
    }));
    assert!(h.http.is_empty());
    assert_eq!(
        h.done.last().unwrap(),
        &Err("Polymarket upper-only candle bounds require from".into())
    );
    for from in [0.0, 999.999] {
        for to in [None, Some(102000.0)] {
            h.event(Event::Command(Command::FetchCandles {
                command: 9,
                symbol: "election-Yes".into(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: None,
                from_ms: Some(from),
                to_ms: to,
            }));
            assert!(h.http.is_empty());
            assert_eq!(
                h.done.last().unwrap(),
                &Err("Polymarket candle from must be at least 1000 milliseconds".into())
            );
        }
    }
}

#[test]
fn keyset_discovery_walks_opaque_cursors_atomically_and_rejects_cycles() {
    let mut h = H::new();
    let first = format!(r#"{{"markets":{MARKETS},"next_cursor":"a/b+=opaque"}}"#);
    h.response("/markets/keyset", 200, &first);
    assert!(!h.e.store().loaded().markets);
    assert_eq!(h.http.len(), 1);
    assert!(h.http[0].url.ends_with("after_cursor=a%2Fb%2B%3Dopaque"));
    assert!(!h.http[0].url.contains("offset="));
    let second = MARKETS
        .replace("election", "sports")
        .replace(TOKEN, "3")
        .replace("\\\"2\\\"", "\\\"4\\\"");
    h.response(
        "/markets/keyset",
        200,
        &format!(r#"{{"markets":{second}}}"#),
    );
    assert_eq!(h.e.store().markets().len(), 4);
    assert!(h.e.store().market("sports-Yes").is_some());
    let mut h = H::new();
    h.response("/markets/keyset", 200, &first);
    h.response("/markets/keyset", 200, &first);
    assert!(!h.e.store().loaded().markets);
    assert!(h.e.store().markets().is_empty());
    assert!(h.http.is_empty());
}
#[test]
fn keyset_missing_cursor_on_full_page_and_failed_later_page_do_not_commit_prefix() {
    let mut h = H::new();
    let row = &MARKETS[1..MARKETS.len() - 1];
    let full = format!(r#"{{"markets":[{}]}}"#, vec![row; 100].join(","));
    h.response("/markets/keyset", 200, &full);
    assert!(!h.e.store().loaded().markets);
    let mut h = H::new();
    h.response(
        "/markets/keyset",
        200,
        &format!(r#"{{"markets":{MARKETS},"next_cursor":"later"}}"#),
    );
    h.response("/markets/keyset", 500, "unavailable");
    assert!(h.e.store().markets().is_empty());
    assert!(!h.e.store().loaded().markets);
}
#[test]
fn price_history_duplicate_timestamp_retains_last_sample() {
    let mut h = H::new();
    h.load();
    h.event(Event::Command(Command::FetchCandles {
        command: 7,
        symbol: "election-Yes".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: None,
        from_ms: None,
        to_ms: None,
    }));
    h.response(
        "/prices-history",
        200,
        r#"{"history":[{"t":100,"p":0.3},{"t":100,"p":0.6}]}"#,
    );
    assert_eq!(
        h.done.last().unwrap().as_ref().unwrap(),
        r#"[{"timestamp":100000.0,"open":0.6,"high":0.6,"low":0.6,"close":0.6,"volume":0.0}]"#
    );
}

#[test]
fn selected_market_filters_use_exact_repeated_slugs_and_do_not_create_private_intent() {
    let mut h = H::new();
    h.e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
        polymarket: Some(safe_cex_venue_polymarket::PolymarketOptions {
            market_slugs: Some(vec!["one/a".into(), "two b".into()]),
            ..Default::default()
        }),
        ..Default::default()
    }));
    h.http.clear();
    h.e.start(h.now);
    h.collect();
    assert_eq!(
        h.http[0].url,
        "https://gamma-api.polymarket.com/markets/keyset?active=true&closed=false&limit=100&slug=one%2Fa&slug=two%20b"
    );
    h.load();
    assert!(h.e.store().loaded().orders);
    assert!(h.http.is_empty());
    assert!(h.socket.is_none());
    for slugs in [vec![], vec!["".into()], vec!["one".into(), "one".into()]] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
            polymarket: Some(safe_cex_venue_polymarket::PolymarketOptions {
                market_slugs: Some(slugs),
                ..Default::default()
            }),
            ..Default::default()
        }));
        e.start(0.0);
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn rejected_midpoints_keep_prior_quotes_retry_and_allow_valid_peers() {
    let mut h = H::new();
    h.response("/markets", 200, MARKETS);
    h.e.store_mut()
        .set_tickers(vec![safe_cex_engine::model::Ticker {
            id: TOKEN.into(),
            symbol: "election-Yes".into(),
            last: 0.4,
            mark: 0.4,
            ..Default::default()
        }]);
    h.response(
        "/midpoints",
        200,
        &format!(r#"{{"{TOKEN}":"bad","2":"0.6"}}"#),
    );
    assert_eq!(h.e.store().ticker("election-Yes").unwrap().last, 0.4);
    assert_eq!(h.e.store().ticker("election-No").unwrap().last, 0.6);
    let timer = h.retry_after(1000.0);
    h.event(Event::TimerFired { timer });
    h.response(
        "/midpoints",
        200,
        &format!(r#"{{"{TOKEN}":"still bad","2":"also bad"}}"#),
    );
    assert_eq!(h.e.store().ticker("election-Yes").unwrap().last, 0.4);
    assert_eq!(h.e.store().ticker("election-No").unwrap().last, 0.6);
    let timer = h.retry_after(2000.0);
    h.event(Event::TimerFired { timer });
    h.response(
        "/midpoints",
        200,
        &format!(r#"{{"{TOKEN}":"0.7","2":"0.3"}}"#),
    );
    assert_eq!(h.e.store().ticker("election-Yes").unwrap().last, 0.7);
    assert_eq!(h.e.store().ticker("election-No").unwrap().last, 0.3);
}

#[test]
fn late_midpoint_seeds_preserve_only_supplied_live_quote_fields() {
    let mut h = H::new();
    h.response("/markets", 200, MARKETS);
    h.subscribe();
    h.message(&format!(r#"{{"event_type":"price_change","timestamp":"101","price_changes":[{{"asset_id":"{TOKEN}","price":"0.4","size":"1","side":"BUY","best_bid":"0.4","best_ask":"0.8"}}]}}"#));
    h.message(
        r#"{"event_type":"last_trade_price","asset_id":"2","price":"0.9","timestamp":"102"}"#,
    );
    h.response(
        "/midpoints",
        200,
        &format!(r#"{{"{TOKEN}":"0.5","2":"bad"}}"#),
    );
    let yes = h.e.store().ticker("election-Yes").unwrap();
    assert_eq!((yes.bid, yes.ask, yes.last), (0.4, 0.8, 0.5));
    assert!((yes.mark - 0.6).abs() < 1e-12);
    assert_eq!(h.e.store().ticker("election-No").unwrap().last, 0.9);
    let timer = h.retry_after(1000.);
    h.event(Event::TimerFired { timer });
    h.message(
        r#"{"event_type":"last_trade_price","asset_id":"2","price":"0.8","timestamp":"103"}"#,
    );
    h.response(
        "/midpoints",
        200,
        &format!(r#"{{"{TOKEN}":"0.3","2":"0.7"}}"#),
    );
    let yes = h.e.store().ticker("election-Yes").unwrap();
    assert_eq!((yes.bid, yes.ask, yes.last), (0.4, 0.8, 0.3));
    assert!((yes.mark - 0.6).abs() < 1e-12);
    let no = h.e.store().ticker("election-No").unwrap();
    assert_eq!((no.last, no.mark), (0.8, 0.7));
}

#[test]
fn failed_midpoint_batch_retries_without_blocking_or_repeating_later_batches() {
    let mut h = H::new();
    // One condition with 101 outcomes exercises the 100-token CLOB batch seam
    // without introducing an unrelated Gamma pagination cursor.
    let tokens: Vec<_> = (1..=101).map(|n| n.to_string()).collect();
    let outcomes: Vec<_> = (1..=101).map(|n| format!("Outcome{n}")).collect();
    let markets = serde_json::json!([{
        "conditionId":"0xbatch", "slug":"batch", "question":"Batch?",
        "clobTokenIds": serde_json::to_string(&tokens).unwrap(),
        "outcomes": serde_json::to_string(&outcomes).unwrap(),
        "orderPriceMinTickSize":0.01, "orderMinSize":5,
        "active":true,"closed":false
    }])
    .to_string();
    h.response("/markets", 200, &markets);
    assert_eq!(h.e.store().markets().len(), 101);
    let first = h
        .http
        .iter()
        .find(|r| r.url.contains("/midpoints"))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(first.body.as_ref().unwrap())
            .unwrap()
            .len(),
        100
    );
    h.response("/midpoints", 200, r#"{"1":"0.5","2":"bad","101":"0.99"}"#);
    assert_eq!(h.e.store().ticker("batch-Outcome1").unwrap().last, 0.5);
    assert!(h.e.store().ticker("batch-Outcome101").is_none());
    let last = h
        .http
        .iter()
        .find(|r| r.url.contains("/midpoints"))
        .unwrap();
    assert_eq!(last.body.as_deref(), Some(r#"[{"token_id":"101"}]"#));
    h.response("/midpoints", 200, r#"{"101":"0.7"}"#);
    assert_eq!(h.e.store().ticker("batch-Outcome101").unwrap().last, 0.7);
    let timer = h.retry_after(1000.);
    h.event(Event::TimerFired { timer });
    let retry = h
        .http
        .iter()
        .find(|r| r.url.contains("/midpoints"))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(retry.body.as_ref().unwrap())
            .unwrap()
            .len(),
        100
    );
    let recovered: serde_json::Map<_, _> = tokens[..100]
        .iter()
        .map(|id| (id.clone(), serde_json::json!("0.4")))
        .collect();
    h.response(
        "/midpoints",
        200,
        &serde_json::to_string(&recovered).unwrap(),
    );
    assert!(!h.http.iter().any(|r| r.url.contains("/midpoints")));
    assert_eq!(h.e.store().tickers().len(), 101);
    assert_eq!(h.e.store().ticker("batch-Outcome1").unwrap().last, 0.4);
    assert_eq!(h.e.store().ticker("batch-Outcome101").unwrap().last, 0.7);
}

#[test]
fn omitted_midpoint_is_an_unseeded_token_not_a_failed_batch() {
    let mut h = H::new();
    h.response("/markets", 200, MARKETS);
    // CLOB omits tokens without a book; the batch is still complete.
    h.response("/midpoints", 200, &format!(r#"{{"{TOKEN}":"0.5"}}"#));
    assert_eq!(h.e.store().ticker("election-Yes").unwrap().last, 0.5);
    assert!(h.e.store().ticker("election-No").is_none());
    assert!(h.e.store().loaded().tickers);
    h.now += 60_000.;
    let due: Vec<_> = h
        .timers
        .iter()
        .filter(|(_, at)| *at <= h.now)
        .map(|(timer, _)| *timer)
        .collect();
    for timer in due {
        h.event(Event::TimerFired { timer });
    }
    assert!(!h.http.iter().any(|r| r.url.contains("/midpoints")));
}

fn no_midpoint_retry_after(h: &mut H, ms: f64) {
    h.now += ms;
    let due: Vec<_> = h
        .timers
        .iter()
        .filter(|(_, at)| *at <= h.now)
        .map(|(timer, _)| *timer)
        .collect();
    for timer in due {
        h.event(Event::TimerFired { timer });
    }
    assert!(!h.http.iter().any(|r| r.url.contains("/midpoints")));
}

#[test]
fn well_formed_empty_midpoint_batch_is_complete_not_retried() {
    let mut h = H::new();
    h.response("/markets", 200, MARKETS);
    // Every token is book-less: a well-formed `{}` rejects nothing.
    h.response("/midpoints", 200, "{}");
    assert!(
        h.e.store()
            .ticker("election-Yes")
            .is_none_or(|t| t.last == 0.0)
    );
    no_midpoint_retry_after(&mut h, 60_000.);
}

#[test]
fn trailing_batch_of_bookless_tokens_finishes_startup_without_retrying() {
    let mut h = H::new();
    let tokens: Vec<_> = (1..=101).map(|n| n.to_string()).collect();
    let outcomes: Vec<_> = (1..=101).map(|n| format!("Outcome{n}")).collect();
    let markets = serde_json::json!([{
        "conditionId":"0xbatch", "slug":"batch", "question":"Batch?",
        "clobTokenIds": serde_json::to_string(&tokens).unwrap(),
        "outcomes": serde_json::to_string(&outcomes).unwrap(),
        "orderPriceMinTickSize":0.01, "orderMinSize":5,
        "active":true,"closed":false
    }])
    .to_string();
    h.response("/markets", 200, &markets);
    h.response("/midpoints", 200, r#"{"1":"0.5"}"#);
    h.response("/midpoints", 200, "{}");
    assert_eq!(h.e.store().ticker("batch-Outcome1").unwrap().last, 0.5);
    assert!(h.e.store().loaded().tickers);
    no_midpoint_retry_after(&mut h, 60_000.);
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn public_supervisor_bounds_handshake_and_backs_off_flapping_connections() {
    let mut h = H::new();
    h.load();
    h.subscribe();
    socket_checks::flaps(h.e, h.socket.unwrap(), h.now);
}

#[test]
fn new_market_interest_during_backoff_waits_and_restores_both_outcomes() {
    let mut h = H::new();
    h.load();
    h.subscribe();
    let old = h.socket.unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let (timer, at) = *h
        .timers
        .iter()
        .find(|(_, at)| (750. ..=1000.).contains(&(*at - h.now)))
        .unwrap();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "election-No".into(),
    }));
    assert_eq!(
        h.socket,
        Some(old),
        "new interest cannot bypass pending backoff"
    );
    h.now = at;
    h.event(Event::TimerFired { timer });
    assert_ne!(h.socket, Some(old));
    h.event(Event::WsOpened {
        socket: h.socket.unwrap(),
    });
    assert!(
        h.sends
            .iter()
            .any(|text| text.contains(TOKEN) && text.contains("\"2\""))
    );
}
#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = H::with_policy(PublicFeedPolicy::Shared);
    h.load();
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "election-Yes".into(),
    }));
    let until = h.now + 600_000.0;
    while let Some(&(timer, at)) = h
        .timers
        .iter()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        h.timers.retain(|(t, _)| *t != timer);
        h.now = at;
        h.event(Event::TimerFired { timer });
        h.fired += 1;
    }
    assert_eq!(h.connects, 0, "no public connects");
    assert!(h.closed.is_empty(), "no public closes: {:?}", h.closed);
    assert_eq!(h.fired, 0, "no supervision timers fire");
}

#[test]
fn a_market_without_a_positive_tick_is_skipped_as_malformed() {
    for tick in [r#""0""#, r#""-0.01""#, r#""""#] {
        let mut h = H::new();
        let body = format!(
            r#"[{{"conditionId":"0x9","slug":"zero-tick","clobTokenIds":"[\"9\"]","outcomes":"[\"Yes\"]","orderPriceMinTickSize":{tick}}},{}]"#,
            &MARKETS[1..MARKETS.len() - 1]
        );
        h.response("/markets", 200, &body);
        assert!(h.e.store().market("election-Yes").is_some());
        assert!(h.e.store().market("zero-tick-Yes").is_none(), "{tick}");
        assert!(
            h.logs
                .iter()
                .any(|l| l.contains("malformed Polymarket market row") && l.contains("zero-tick")),
            "{:?}",
            h.logs
        );
    }
}
