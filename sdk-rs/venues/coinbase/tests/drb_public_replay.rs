//! Offline acceptance gate replaying unmodified LIVE public drb response bodies.
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, SocketId, TimerId, Venue,
};
use safe_cex_venue_coinbase::{Coinbase, Options};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const SYMBOL: &str = "BTC_USDC-PERPETUAL";
fn tape() -> Vec<Value> {
    include_str!("fixtures/drb-public.jsonl")
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
struct Replay {
    engine: Engine<Coinbase>,
    records: Vec<Value>,
    epoch: f64,
    now: f64,
    timers: Vec<(f64, TimerId)>,
    requests: Vec<HttpRequest>,
    socket: Option<SocketId>,
    sends: Vec<Value>,
    results: BTreeMap<u32, Value>,
    calls: Vec<String>,
}
impl Replay {
    fn new() -> Self {
        let records = tape();
        let epoch = records[0]["epoch"].as_f64().unwrap();
        let mut engine = safe_cex_engine::test_support::seeded_engine(
            Coinbase::new(Options {
                public_data_only: true,
                ..Default::default()
            })
            .unwrap(),
        );
        engine.start(epoch);
        Self {
            engine,
            records,
            epoch,
            now: epoch,
            timers: vec![],
            requests: vec![],
            socket: None,
            sends: vec![],
            results: BTreeMap::new(),
            calls: vec![],
        }
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::SetTimer { timer, at_ms } => self.timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => self.timers.retain(|(_, t)| *t != timer),
                Action::WsConnect { socket, url, .. } => {
                    assert_eq!(url, "wss://streams.drb.coinbase.com/ws/api/v2");
                    self.socket = Some(socket);
                }
                Action::WsSend { text, .. } => {
                    self.sends.push(serde_json::from_str(&text).unwrap())
                }
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.results
                        .insert(command, serde_json::from_str(&result).unwrap());
                }
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    panic!("{}", error.legacy_text())
                }
                Action::Notice(Notice::StartupFailed { .. }) => panic!("startup failed"),
                _ => {}
            }
        }
    }
    fn next(&mut self) -> HttpRequest {
        for _ in 0..100 {
            self.collect();
            if !self.requests.is_empty() {
                return self.requests.remove(0);
            }
            self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (at, timer) = self.timers.remove(0);
            self.now = self.now.max(at);
            assert!(
                self.now - self.epoch < 30_000.,
                "startup/request deadline exceeded"
            );
            self.engine.handle(self.now, Event::TimerFired { timer });
        }
        panic!("no request within replay budget")
    }
    fn reply(&mut self, request: HttpRequest) {
        assert!(
            request.url.starts_with("https://drb.coinbase.com/api/v2"),
            "retired public URL: {}",
            request.url
        );
        assert!(
            request
                .headers
                .iter()
                .all(|(k, _)| !k.to_ascii_lowercase().contains("authorization"))
        );
        let chart_get = request.method == safe_cex_engine::Method::Get;
        let body: Value = if chart_get {
            assert!(request.body.is_none());
            assert!(
                request.url.starts_with(
                    "https://drb.coinbase.com/api/v2/public/get_tradingview_chart_data?"
                )
            );
            let mut params = serde_json::Map::new();
            for pair in request.url.split_once('?').unwrap().1.split('&') {
                let (key, value) = pair.split_once('=').unwrap();
                params.insert(
                    key.into(),
                    if key.ends_with("_timestamp") {
                        json!(value.parse::<u64>().unwrap())
                    } else {
                        json!(value)
                    },
                );
            }
            json!({"jsonrpc":"2.0", "method":"public/get_tradingview_chart_data", "params":params})
        } else {
            assert_eq!(request.method, safe_cex_engine::Method::Post);
            serde_json::from_str(request.body.as_deref().expect("JSON-RPC request body")).unwrap()
        };
        assert_eq!(body["jsonrpc"], "2.0");
        let method = body["method"].as_str().unwrap();
        assert!(method.starts_with("public/") && method != "public/auth");
        let record = self
            .records
            .iter()
            .find(|r| r["type"] == "http" && r["method"] == method)
            .unwrap_or_else(|| panic!("missing live capture for {method}"));
        assert_eq!(record["httpMethod"], if chart_get { "GET" } else { "POST" });
        // Startup calls and bounded query parameters are part of the protocol gate.
        if matches!(
            method,
            "public/get_instruments" | "public/get_book_summary_by_currency"
        ) {
            assert_eq!(body["params"], record["params"]);
        } else {
            assert_eq!(body["params"]["instrument_name"], SYMBOL);
        }
        if method == "public/get_tradingview_chart_data" {
            assert_eq!(body["params"]["resolution"], "1");
            assert_eq!(body["params"]["start_timestamp"], self.records[0]["start"]);
            assert_eq!(body["params"]["end_timestamp"], self.records[0]["end"]);
        }
        let mut response: Value = serde_json::from_str(record["body"].as_str().unwrap()).unwrap();
        if !chart_get {
            response["id"] = body["id"].clone();
        }
        let response = response.to_string();
        self.calls.push(method.into());
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &response,
                },
            },
        );
    }
    fn ready() -> Self {
        let mut s = Self::new();
        for _ in 0..5 {
            if s.engine.store().loaded().markets && s.engine.store().loaded().tickers {
                break;
            }
            let r = s.next();
            s.reply(r);
        }
        assert!(s.engine.store().loaded().markets && s.engine.store().loaded().tickers);
        assert!(s.now - s.epoch < 30_000.);
        s.collect();
        s.engine.handle(
            s.now,
            Event::WsOpened {
                socket: s.socket.expect("public stream"),
            },
        );
        s.collect();
        s
    }
    fn command(&mut self, command: Command) {
        self.engine.handle(self.now, Event::Command(command));
        self.collect();
    }
    fn message(&mut self, text: &str) {
        self.engine.handle(
            self.now,
            Event::WsMessage {
                socket: self.socket.unwrap(),
                text,
            },
        );
    }
    fn frames(&self, prefix: &str) -> Vec<Value> {
        self.records
            .iter()
            .filter(|r| r["type"] == "ws")
            .filter_map(|r| {
                let v: Value = serde_json::from_str(r["text"].as_str().unwrap()).unwrap();
                v["params"]["channel"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(prefix))
                    .then_some(v)
            })
            .collect()
    }
}

#[test]
fn live_catalogue_and_tickers_load_before_startup_deadline_without_a_key() {
    let s = Replay::ready();
    assert_eq!(
        s.calls,
        [
            "public/get_instruments",
            "public/get_book_summary_by_currency"
        ]
    );
    let markets = s.engine.store().markets();
    assert!(markets.len() >= 125);
    assert!(markets.iter().all(|m| m.id == m.symbol
        && m.symbol.ends_with("_USDC-PERPETUAL")
        && m.is_perpetual == Some(true)
        && m.is_inverse == Some(false)));
    let btc = s.engine.store().market(SYMBOL).unwrap();
    assert_eq!(btc.base, "BTC");
    assert_eq!(btc.quote, "USDC");
    assert_eq!(btc.precision.amount, 0.0001);
    assert!(s.engine.store().tickers().len() >= 125);
    assert!(s.sends.iter().any(|v| {
        v["method"] == "public/subscribe"
            && v["params"]["channels"]
                .as_array()
                .unwrap()
                .contains(&json!(format!("ticker.{SYMBOL}.100ms")))
    }));
}

#[test]
fn latest_candles_accept_fractional_host_clock() {
    for explicit_start in [false, true] {
        for future_end in [false, true] {
            let mut s = Replay::ready();
            s.now += 0.5;
            let end = s.now.floor();
            s.command(Command::FetchCandles {
                command: 91,
                symbol: SYMBOL.into(),
                interval: Interval::M1,
                limit: Some(10),
                from_ms: explicit_start.then_some(end - 600_000.),
                to_ms: future_end.then_some(end + 60_000.),
            });
            let request = s.requests.pop().expect("latest candles must dispatch HTTP");
            assert_eq!(request.method, safe_cex_engine::Method::Get);
            assert!(
                request
                    .url
                    .contains(&format!("end_timestamp={}", end as u64))
            );
            assert!(
                request
                    .url
                    .contains(&format!("start_timestamp={}", (end - 600_000.) as u64))
            );
        }
    }
}

#[test]
fn latest_candles_use_corrected_protocol_utc_after_suspend() {
    assert_corrected_candle_bounds(7_200_000., None);
}

#[test]
fn latest_candles_explicit_end_uses_corrected_protocol_utc() {
    assert_corrected_candle_bounds(7_200_000., Some(0.));
}

#[test]
fn corrected_candle_bounds_clamp_future_ends_and_preserve_historical_ends() {
    for correction in [-7_200_000., 7_200_000.] {
        for end_offset in [None, Some(60_000.), Some(-60_000.)] {
            assert_corrected_candle_bounds(correction, end_offset);
        }
    }
}

fn assert_corrected_candle_bounds(correction: f64, end_offset: Option<f64>) {
    let mut s = Replay::ready();
    s.now += 0.5;
    assert!(s.engine.sync_host_time(s.now, s.now));
    let utc = s.now + correction;
    assert!(s.engine.sync_host_time(s.now, utc));
    let explicit_end = end_offset.map(|offset| utc.floor() + offset);
    let expected_end = utc.floor() + end_offset.unwrap_or_default().min(0.);
    s.command(Command::FetchCandles {
        command: 91,
        symbol: SYMBOL.into(),
        interval: Interval::M1,
        limit: Some(10),
        from_ms: None,
        to_ms: explicit_end,
    });
    assert_eq!(
        safe_cex_engine::test_support::dispatch_now(&s.engine),
        s.now,
        "scheduling stays on raw time"
    );
    assert_eq!(s.requests.len(), 1, "one candle read: {:?}", s.requests);
    let request = s.requests.pop().unwrap();
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    let query = request.url.split_once('?').unwrap().1;
    for (name, expected) in [
        ("end_timestamp", expected_end),
        ("start_timestamp", expected_end - 600_000.),
    ] {
        let value = query
            .split('&')
            .find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == name).then_some(value)
            })
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert_eq!(
            value, expected as u64,
            "{name}: correction={correction}, end={explicit_end:?}"
        );
    }
}

#[test]
fn initially_missing_tickers_recover_from_summary_or_stream() {
    for malformed in [false, true] {
        for stream in [false, true] {
            let mut s = Replay::new();
            let instruments = s.next();
            s.reply(instruments);
            let summary = s.next();
            let index = s
                .records
                .iter()
                .position(|r| r["method"] == "public/get_book_summary_by_currency")
                .unwrap();
            let original = s.records[index]["body"].clone();
            let mut response: Value = serde_json::from_str(original.as_str().unwrap()).unwrap();
            let rows = response["result"].as_array_mut().unwrap();
            if malformed {
                let row = rows
                    .iter_mut()
                    .find(|r| r["instrument_name"] == SYMBOL)
                    .unwrap();
                row["funding_8h"] = json!("malformed");
            } else {
                rows.retain(|r| r["instrument_name"] != SYMBOL);
            }
            s.records[index]["body"] = json!(response.to_string());
            s.reply(summary);
            s.collect();
            assert!(s.engine.store().loaded().tickers);
            assert!(s.engine.store().market(SYMBOL).is_some());
            assert!(s.engine.store().ticker(SYMBOL).is_none());
            let seeded = s.engine.store().tickers().len();
            if stream {
                s.engine.handle(
                    s.now,
                    Event::WsOpened {
                        socket: s.socket.unwrap(),
                    },
                );
                s.collect();
                let update = s.frames("ticker.").pop().unwrap();
                assert_eq!(update["params"]["data"]["instrument_name"], SYMBOL);
                s.message(&update.to_string());
            } else {
                s.records[index]["body"] = original;
                let poll = s.next();
                s.reply(poll);
                s.collect();
            }
            let recovered = s
                .engine
                .store()
                .ticker(SYMBOL)
                .expect("a valid update must recover a ticker omitted from the seed");
            assert!(recovered.last > 0. && recovered.mark > 0.);
            assert_eq!(s.engine.store().tickers().len(), seeded + 1);
        }
    }
}

#[test]
fn live_book_snapshot_and_every_change_match_independent_reconstruction() {
    let mut s = Replay::ready();
    s.command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    });
    let frames = s.frames("book.");
    assert!(frames.len() > 10);
    let mut bids = BTreeMap::new();
    let mut asks = BTreeMap::new();
    for frame in frames {
        let data = &frame["params"]["data"];
        if data["type"] == "snapshot" {
            bids.clear();
            asks.clear();
        }
        for (name, side) in [("bids", &mut bids), ("asks", &mut asks)] {
            for level in data[name].as_array().unwrap() {
                let price = level[1].as_f64().unwrap();
                let size = level[2].as_f64().unwrap();
                if size == 0. {
                    side.remove(&price.to_bits());
                } else {
                    side.insert(price.to_bits(), size);
                }
            }
        }
        s.message(&frame.to_string());
        let book = s
            .engine
            .venue()
            .book(SYMBOL)
            .expect("snapshot makes book live");
        assert_eq!(
            book.bids(),
            bids.iter()
                .rev()
                .map(|(p, q)| (f64::from_bits(*p), *q))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            book.asks(),
            asks.iter()
                .map(|(p, q)| (f64::from_bits(*p), *q))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn live_tickers_preserve_base_size_funding_and_index_units() {
    let mut s = Replay::ready();
    for frame in s.frames("ticker.") {
        s.message(&frame.to_string());
        let row = &frame["params"]["data"];
        let t = s
            .engine
            .store()
            .tickers()
            .iter()
            .find(|t| t.symbol == SYMBOL)
            .unwrap();
        assert_eq!(t.last, row["last_price"].as_f64().unwrap());
        assert_eq!(t.index, row["index_price"].as_f64().unwrap());
        assert_eq!(t.volume, row["stats"]["volume"].as_f64().unwrap());
        assert_eq!(t.open_interest, row["open_interest"].as_f64().unwrap());
        assert_eq!(t.funding_rate, row["funding_8h"].as_f64().unwrap());
        assert!(t.funding_time > row["timestamp"].as_f64().unwrap());
    }
}

#[test]
fn book_gaps_invalidate_immediately_and_duplicates_do_not_rewind() {
    let mut s = Replay::ready();
    s.command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    });
    let frames = s.frames("book.");
    s.message(&frames[0].to_string());
    s.message(&frames[1].to_string());
    let before = s.engine.venue().book(SYMBOL).unwrap().bids().to_vec();
    s.message(&frames[0].to_string());
    assert_eq!(s.engine.venue().book(SYMBOL).unwrap().bids(), before);
    let mut gap = frames[2].clone();
    gap["params"]["data"]["prev_change_id"] = json!(0);
    s.message(&gap.to_string());
    assert!(
        s.engine.venue().book(SYMBOL).is_none(),
        "a gap must never leave a stale live book"
    );
}

#[test]
fn malformed_new_snapshot_cannot_publish_a_partial_book() {
    let mut s = Replay::ready();
    s.command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    });
    let snapshot = s.frames("book.")[0].clone();
    s.message(&snapshot.to_string());
    let mut invalid = snapshot.clone();
    invalid["params"]["data"]["change_id"] =
        json!(snapshot["params"]["data"]["change_id"].as_u64().unwrap() + 1);
    invalid["params"]["data"]["asks"][0][2] = json!(-1);
    s.message(&invalid.to_string());
    assert!(s.engine.venue().book(SYMBOL).is_none());
}

#[test]
fn separate_books_have_independent_change_ids_and_repair_clears_only_its_book() {
    let mut s = Replay::ready();
    let eth = "ETH_USDC-PERPETUAL";
    for symbol in [SYMBOL, eth] {
        s.command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        });
    }
    let snapshot = s.frames("book.")[0].clone();
    s.message(&snapshot.to_string());
    let mut second = snapshot.clone();
    second["params"]["channel"] = json!(format!("book.{eth}.100ms"));
    second["params"]["data"]["instrument_name"] = json!(eth);
    second["params"]["data"]["change_id"] = json!(1);
    s.message(&second.to_string());
    assert!(s.engine.venue().book(eth).is_some());
    s.engine.handle(
        s.now,
        Event::Command(Command::RepairSubscription(
            safe_cex_engine::SubscriptionTopic::OrderBook {
                symbol: SYMBOL.into(),
            },
        )),
    );
    assert!(s.engine.venue().book(SYMBOL).is_none());
    assert!(s.engine.venue().book(eth).is_some());
}

#[test]
fn repair_ignores_changes_until_the_replacement_snapshot_without_resetting_other_books() {
    let mut s = Replay::ready();
    let eth = "ETH_USDC-PERPETUAL";
    for symbol in [SYMBOL, eth] {
        s.command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        });
    }
    let frames = s.frames("book.");
    s.message(&frames[0].to_string());
    s.message(&frames[1].to_string());
    let expected_bids = s.engine.venue().book(SYMBOL).unwrap().bids().to_vec();
    let expected_asks = s.engine.venue().book(SYMBOL).unwrap().asks().to_vec();
    let mut second = frames[0].clone();
    second["params"]["channel"] = json!(format!("book.{eth}.100ms"));
    second["params"]["data"]["instrument_name"] = json!(eth);
    second["params"]["data"]["change_id"] = json!(1);
    s.message(&second.to_string());
    let other_bids = s.engine.venue().book(eth).unwrap().bids().to_vec();
    let other_asks = s.engine.venue().book(eth).unwrap().asks().to_vec();

    s.command(Command::RepairSubscription(
        safe_cex_engine::SubscriptionTopic::OrderBook {
            symbol: SYMBOL.into(),
        },
    ));
    assert!(s.engine.venue().book(SYMBOL).is_none());
    assert_eq!(frames[2]["params"]["data"]["type"], "change");
    // This packet was in flight on the old subscription when repair began.
    s.message(&frames[2].to_string());
    assert!(s.engine.venue().book(SYMBOL).is_none());
    let other = s
        .engine
        .venue()
        .book(eth)
        .expect("repair must keep the other book live");
    assert_eq!(other.bids(), other_bids);
    assert_eq!(other.asks(), other_asks);
    assert!(
        !s.engine
            .drain_actions()
            .any(|action| matches!(action, Action::WsClose { .. } | Action::WsConnect { .. }))
    );

    // The ignored delta must not establish a sequence watermark. The fresh
    // snapshot establishes it, and ordinary sequenced changes apply again.
    s.message(&frames[0].to_string());
    s.message(&frames[1].to_string());
    let repaired = s.engine.venue().book(SYMBOL).unwrap();
    assert_eq!(repaired.bids(), expected_bids);
    assert_eq!(repaired.asks(), expected_asks);
}

#[test]
fn older_ticker_notifications_cannot_rewind_prices_or_funding() {
    let mut s = Replay::ready();
    let frames = s.frames("ticker.");
    let latest = frames.last().unwrap();
    s.message(&latest.to_string());
    let before = serde_json::to_value(s.engine.store().ticker(SYMBOL).unwrap()).unwrap();
    s.message(&frames[0].to_string());
    assert_eq!(
        serde_json::to_value(s.engine.store().ticker(SYMBOL).unwrap()).unwrap(),
        before
    );
}

#[test]
fn a_delayed_summary_poll_cannot_overwrite_a_newer_stream_ticker() {
    let mut s = Replay::ready();
    let latest = s.frames("ticker.").pop().unwrap();
    s.message(&latest.to_string());
    let before = serde_json::to_value(s.engine.store().ticker(SYMBOL).unwrap()).unwrap();
    let poll = s.next();
    s.reply(poll);
    assert_eq!(
        serde_json::to_value(s.engine.store().ticker(SYMBOL).unwrap()).unwrap(),
        before
    );
}

#[test]
fn empty_ticker_catalogue_retries_without_discarding_markets() {
    let mut s = Replay::new();
    let request = s.next();
    s.reply(request);
    let summary = s.next();
    let index = s
        .records
        .iter()
        .position(|r| r["method"] == "public/get_book_summary_by_currency")
        .unwrap();
    let good = s.records[index]["body"].clone();
    s.records[index]["body"] = json!(r#"{"jsonrpc":"2.0","result":[]}"#);
    s.reply(summary);
    assert!(s.engine.store().loaded().markets);
    assert!(!s.engine.store().loaded().tickers);
    s.records[index]["body"] = good;
    let retry = s.next();
    s.reply(retry);
    s.collect();
    assert!(s.engine.store().loaded().tickers);
    assert!(s.socket.is_some());
}

#[test]
fn a_cached_store_still_opens_its_public_stream_after_refresh() {
    let prior = Replay::ready();
    let mut s = Replay::new();
    s.engine
        .store_mut()
        .set_markets(prior.engine.store().markets().to_vec());
    s.engine
        .store_mut()
        .set_tickers(prior.engine.store().tickers().to_vec());
    let request = s.next();
    s.reply(request);
    let request = s.next();
    s.reply(request);
    s.collect();
    assert!(s.socket.is_some());
}

fn candle_frame(s: &mut Replay) -> Vec<[f64; 6]> {
    let Some(frame) = s.engine.take_frame(s.now) else {
        return vec![];
    };
    let mut rows = vec![];
    for (kind, payload) in safe_cex_engine::wire::sections(frame) {
        if kind != 6 {
            continue;
        }
        let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
        let start = (4 + count * 8 + 7) & !7;
        for row in payload[start..start + count * 48].as_chunks::<48>().0 {
            rows.push(std::array::from_fn(|i| {
                f64::from_le_bytes(row[i * 8..i * 8 + 8].try_into().unwrap())
            }));
        }
    }
    rows
}

#[test]
fn live_chart_stream_uses_its_own_interval_and_cumulative_volume() {
    let mut s = Replay::ready();
    s.engine.take_frame(s.now);
    s.command(Command::SubscribeCandles {
        symbol: SYMBOL.into(),
        interval: Interval::M1,
    });
    assert!(s.sends.iter().any(|v| {
        v["params"]["channels"]
            .as_array()
            .is_some_and(|a| a.contains(&json!(format!("chart.trades.{SYMBOL}.1"))))
    }));
    for frame in s.frames("chart.") {
        s.message(&frame.to_string());
        let row = &frame["params"]["data"];
        assert_eq!(
            candle_frame(&mut s),
            vec![[
                row["tick"].as_f64().unwrap() / 1000.,
                row["open"].as_f64().unwrap(),
                row["high"].as_f64().unwrap(),
                row["low"].as_f64().unwrap(),
                row["close"].as_f64().unwrap(),
                row["volume"].as_f64().unwrap()
            ]]
        );
    }
}

#[test]
fn live_trade_stream_builds_second_candles_once_per_trade() {
    let mut s = Replay::ready();
    s.engine.take_frame(s.now);
    s.command(Command::SubscribeCandles {
        symbol: SYMBOL.into(),
        interval: Interval::S1,
    });
    let frame = s.frames("trades.")[0].clone();
    s.message(&frame.to_string());
    let trades = frame["params"]["data"].as_array().unwrap();
    let mut volumes = BTreeMap::new();
    for trade in trades {
        let second = trade["timestamp"].as_u64().unwrap() / 1000;
        *volumes.entry(second).or_insert(0.) += trade["amount"].as_f64().unwrap();
    }
    let rows = candle_frame(&mut s);
    assert_eq!(
        rows.last().unwrap()[5],
        *volumes.last_key_value().unwrap().1
    );
    s.message(&frame.to_string());
    assert!(
        candle_frame(&mut s).is_empty(),
        "replayed trades must not count volume twice"
    );
}

#[test]
fn reads_deferred_during_startup_keep_their_caller_contexts() {
    let mut s = Replay::new();
    s.command(Command::FetchCandles {
        command: 81,
        symbol: SYMBOL.into(),
        interval: Interval::M1,
        limit: Some(60),
        from_ms: s.records[0]["start"].as_f64(),
        to_ms: s.records[0]["end"].as_f64(),
    });
    s.command(Command::Query {
        command: 82,
        query: Query::PublicTrades {
            symbol: SYMBOL.into(),
            limit: Some(10),
        },
    });
    for _ in 0..4 {
        let request = s.next();
        s.reply(request);
    }
    s.collect();
    assert_eq!(s.results[&81].as_array().unwrap().len(), 60);
    assert_eq!(s.results[&82].as_array().unwrap().len(), 10);
}

#[test]
fn live_chart_history_uses_milliseconds_on_wire_and_seconds_in_candles() {
    let mut s = Replay::ready();
    s.command(Command::FetchCandles {
        command: 71,
        symbol: SYMBOL.into(),
        interval: Interval::M1,
        limit: Some(60),
        from_ms: s.records[0]["start"].as_f64(),
        to_ms: s.records[0]["end"].as_f64(),
    });
    let r = s.next();
    s.reply(r);
    s.collect();
    let rows = s.results.get(&71).unwrap().as_array().unwrap();
    let record = s
        .records
        .iter()
        .find(|r| r["method"] == "public/get_tradingview_chart_data")
        .unwrap();
    let body: Value = serde_json::from_str(record["body"].as_str().unwrap()).unwrap();
    let data = &body["result"];
    // The gateway includes its upper endpoint; the SDK range is half-open.
    assert_eq!(rows.len(), 60);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            row["timestamp"].as_f64().unwrap(),
            data["ticks"][i].as_f64().unwrap() / 1000.
        );
        for name in ["open", "high", "low", "close", "volume"] {
            assert_eq!(row[name], data[name][i]);
        }
    }
}

#[test]
fn live_public_trades_keep_taker_side_and_base_amount() {
    let mut s = Replay::ready();
    s.command(Command::Query {
        command: 72,
        query: Query::PublicTrades {
            symbol: SYMBOL.into(),
            limit: Some(10),
        },
    });
    let r = s.next();
    s.reply(r);
    s.collect();
    let rows = s.results.get(&72).unwrap().as_array().unwrap();
    let record = s
        .records
        .iter()
        .find(|r| r["method"] == "public/get_last_trades_by_instrument")
        .unwrap();
    let body: Value = serde_json::from_str(record["body"].as_str().unwrap()).unwrap();
    let trades = body["result"]["trades"].as_array().unwrap();
    assert_eq!(rows.len(), trades.len());
    for (row, trade) in rows.iter().zip(trades) {
        assert_eq!(row["id"], trade["trade_id"]);
        assert_eq!(row["side"], trade["direction"]);
        assert_eq!(row["qty"], trade["amount"]);
        assert_eq!(row["timestamp"], trade["timestamp"]);
    }
}
