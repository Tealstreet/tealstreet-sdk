use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId, Venue,
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
}
impl H {
    fn new() -> Self {
        let mut h = Self {
            e: Engine::new(Polymarket::new(Options {
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
        };
        h.e.start(h.now);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for a in self.e.drain_actions() {
            match a {
                Action::Http(r) => self.http.push(r),
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    assert_eq!(channel, Some(safe_cex_engine::SocketChannel::Public));
                    self.socket = Some(socket);
                }
                Action::WsSend { text, .. } => self.sends.push(text),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::Notice(Notice::CommandDone { result, .. }) => self.done.push(result),
                _ => {}
            }
        }
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
    assert!(h.e.store().loaded().markets);
    let mut h = H::new();
    h.response("/markets", 200, r#"[{"slug":"missing condition id"}]"#);
    assert!(!h.e.store().loaded().markets);
    assert!(h.e.store().markets().is_empty());
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
    h.e = Engine::new(Polymarket::new(Options {
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
        let mut e = Engine::new(Polymarket::new(Options {
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
