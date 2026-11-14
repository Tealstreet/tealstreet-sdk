//! Public USDC perpetual data over the shared S0 JSON-RPC transport.
use super::*;
use crate::drb::RpcId;
use safe_cex_engine::request_context::{Context, Policy, RetryPolicy, Scope};
use safe_cex_engine::{ErrorKind, Query};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct Public {
    pub active: bool,
    catalogue_ready: bool,
    ready: bool,
    sequence: u64,
    pending: HashMap<u64, (RpcId, Context, Operation)>,
    deferred_reads: HashMap<CommandId, Context>,
    acknowledgements: HashMap<RpcId, f64>,
    trade_sequence: HashMap<String, u64>,
    ticker_time: HashMap<String, f64>,
}
impl Public {
    pub fn closed(&mut self) {
        self.acknowledgements.clear();
    }
}
enum Operation {
    Instruments,
    Summaries,
    Chart(Box<Chart>),
    Trades {
        context: Context,
        symbol: String,
        limit: u32,
    },
}
struct Chart {
    context: Context,
    symbol: String,
    resolution: &'static str,
    seconds: f64,
    cursor: u64,
    end: u64,
    page_end: u64,
    rows: BTreeMap<u64, Candle>,
}
#[derive(Deserialize)]
struct Instrument {
    instrument_name: String,
    kind: String,
    instrument_type: String,
    settlement_period: String,
    settlement_currency: String,
    base_currency: String,
    quote_currency: String,
    is_active: bool,
    tick_size: f64,
    contract_size: f64,
    min_trade_amount: f64,
    max_leverage: f64,
    maker_commission: Option<f64>,
    taker_commission: Option<f64>,
}
fn number(v: &Value, key: &str) -> Option<f64> {
    v.get(key)?.as_f64().filter(|n| n.is_finite())
}
fn nonnegative(v: &Value, key: &str) -> Option<f64> {
    match v.get(key) {
        None | Some(Value::Null) => Some(0.),
        Some(_) => number(v, key).filter(|n| *n >= 0.),
    }
}
fn resolution(i: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match i {
        M1 => "1",
        M3 => "3",
        M5 => "5",
        M15 => "15",
        M30 => "30",
        H1 => "60",
        H2 => "120",
        H6 => "360",
        H12 => "720",
        D1 => "1D",
        _ => return None,
    })
}
fn chart_candle(row: &Value) -> Option<Candle> {
    let timestamp = number(row, "tick")? / 1000.;
    let c = Candle {
        timestamp,
        open: number(row, "open")?,
        high: number(row, "high")?,
        low: number(row, "low")?,
        close: number(row, "close")?,
        volume: number(row, "volume")?,
    };
    (timestamp >= 0.
        && timestamp.fract() == 0.
        && c.low >= 0.
        && c.high >= c.open.max(c.close).max(c.low)
        && c.low <= c.open.min(c.close)
        && c.volume >= 0.)
        .then_some(c)
}
fn ticker(row: &Value, summary: bool, now: f64) -> Option<Ticker> {
    let symbol = row.get("instrument_name")?.as_str()?.to_owned();
    let stats = if summary { row } else { row.get("stats")? };
    let timestamp = number(row, "timestamp")
        .or_else(|| number(row, "creation_timestamp"))
        .unwrap_or(now);
    let rate = number(row, "funding_8h")?;
    Some(Ticker {
        id: symbol.clone(),
        symbol,
        bid: nonnegative(
            row,
            if summary {
                "bid_price"
            } else {
                "best_bid_price"
            },
        )?,
        ask: nonnegative(
            row,
            if summary {
                "ask_price"
            } else {
                "best_ask_price"
            },
        )?,
        last: nonnegative(row, if summary { "last" } else { "last_price" })?,
        mark: nonnegative(row, "mark_price")?,
        index: nonnegative(
            row,
            if summary {
                "estimated_delivery_price"
            } else {
                "index_price"
            },
        )?,
        percentage: number(stats, "price_change").unwrap_or(0.),
        volume: nonnegative(stats, "volume")?,
        quote_volume: nonnegative(stats, "volume_notional")?,
        open_interest: nonnegative(row, "open_interest")?,
        funding_rate: rate,
        // Deribit publishes an eight-hour rate, settled at 00:00/08:00/16:00 UTC.
        funding_time: ((timestamp / 28_800_000.).floor() + 1.) * 28_800_000.,
        ..Ticker::default()
    })
}

impl Coinbase {
    fn drb_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: &str,
        params: Value,
        op: Operation,
    ) {
        self.public.drb.sequence += 1;
        let sequence = self.public.drb.sequence;
        let startup = matches!(op, Operation::Instruments | Operation::Summaries);
        let context = match &op {
            Operation::Chart(chart) => chart.context.clone(),
            Operation::Trades { context, .. } => context.clone(),
            _ => {
                let Ok(context) = cx.http_context(Scope::Background, drb_read_policy()) else {
                    self.drb_operation_failed(
                        cx,
                        op,
                        "Coinbase drb public context was not admitted",
                    );
                    return;
                };
                context
            }
        };
        match self.drb.http(
            cx,
            &context,
            method,
            params,
            None,
            Pending::Public(Request::Drb { sequence, startup }),
        ) {
            Ok((id, _)) => {
                self.public.drb.pending.insert(sequence, (id, context, op));
            }
            Err(_) => {
                if startup {
                    let _ = cx.finish_http_context(&context);
                }
                self.drb_operation_failed(cx, op, "Coinbase drb public request was not admitted")
            }
        }
    }
    pub(crate) fn drb_start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.drb.active = true;
        self.public.kind = Some(ProductKind::Intx);
        if self.public.drb.catalogue_ready {
            self.drb_summaries(cx);
        } else {
            self.drb_request(
                cx,
                "public/get_instruments",
                json!({"currency":"USDC", "kind":"future", "expired":false}),
                Operation::Instruments,
            );
        }
    }
    fn drb_summaries(&mut self, cx: &mut Cx<'_, Pending>) {
        self.drb_request(
            cx,
            "public/get_book_summary_by_currency",
            json!({"currency":"USDC", "kind":"future"}),
            Operation::Summaries,
        );
    }
    fn drb_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.connect_public(cx);
    }
    fn drb_subscribe(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channels: Vec<String>,
        add: bool,
    ) -> Result<(), String> {
        let Some(socket) = self.public.socket.filter(|_| self.public.open) else {
            return Ok(());
        };
        if channels.is_empty() {
            return Ok(());
        }
        let id = self.drb.websocket(
            cx,
            socket,
            if add {
                crate::drb::ControlMethod::PublicSubscribe
            } else {
                crate::drb::ControlMethod::PublicUnsubscribe
            },
            json!({"channels": channels}),
        )?;
        self.public.drb.acknowledgements.insert(id, cx.now);
        Ok(())
    }
    fn drb_subscribe_or_reconnect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channels: Vec<String>,
        add: bool,
    ) {
        if self.drb_subscribe(cx, channels, add).is_err() {
            self.reconnect_public(cx);
        }
    }
    pub(super) fn drb_open(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.open = true;
        self.public.supervisor.opened(cx);
        let mut channels: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| format!("ticker.{}.100ms", m.id))
            .collect();
        for (symbol, depth) in &mut self.public.books {
            depth.waiting_since = cx.now;
            depth.change_id = None;
            channels.push(format!("book.{symbol}.100ms"));
        }
        for (symbol, interval) in self.public.candles.keys() {
            channels.push(candle_channel(symbol, *interval).unwrap());
        }
        channels.sort();
        channels.dedup();
        self.drb_subscribe_or_reconnect(cx, channels, true);
    }
    pub(super) fn drb_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        sequence: u64,
        response: Response<'_>,
    ) {
        let Some((id, context, operation)) = self.public.drb.pending.remove(&sequence) else {
            return;
        };
        if cx.validate_http_context(&context).is_err() {
            self.drb.failed(id, ErrorKind::Network);
            let _ = cx.finish_http_context(&context);
            self.drb_operation_failed(cx, operation, "Coinbase drb public context expired");
            return;
        }
        let background = matches!(operation, Operation::Instruments | Operation::Summaries);
        let result = match self
            .drb
            .response_at(id, response.status, response.body, cx.now)
        {
            Ok(result) => result,
            Err(_) => {
                if background {
                    let _ = cx.finish_http_context(&context);
                }
                self.drb_operation_failed(cx, operation, "Coinbase drb public RPC failed");
                return;
            }
        };
        self.drb_result(cx, operation, result);
        if background {
            let _ = cx.finish_http_context(&context);
        }
    }
    fn drb_result(&mut self, cx: &mut Cx<'_, Pending>, op: Operation, result: Value) {
        match op {
            Operation::Instruments => {
                let mut markets = vec![];
                if let Some(rows) = result.as_array() {
                    for row in rows {
                        let Ok(row) = serde_json::from_value::<Instrument>(row.clone()) else {
                            safe_cex_engine::startup::skipped_row(cx);
                            continue;
                        };
                        if !row.is_active
                            || row.kind != "future"
                            || row.instrument_type != "linear"
                            || row.settlement_period != "perpetual"
                            || row.settlement_currency != "USDC"
                            || row.quote_currency != "USDC"
                            || row.instrument_name
                                != format!("{}_USDC-PERPETUAL", row.base_currency)
                        {
                            continue;
                        }
                        if [
                            row.tick_size,
                            row.contract_size,
                            row.min_trade_amount,
                            row.max_leverage,
                        ]
                        .iter()
                        .any(|n| !n.is_finite() || *n <= 0.)
                        {
                            safe_cex_engine::startup::skipped_row(cx);
                            continue;
                        }
                        let mut m = market(
                            row.instrument_name,
                            row.base_currency,
                            row.quote_currency,
                            Precision {
                                price: row.tick_size,
                                amount: row.contract_size,
                            },
                            row.min_trade_amount,
                            1e9,
                            row.contract_size,
                            Some(false),
                            Some(false),
                        );
                        m.is_perpetual = Some(true);
                        m.is_inverse = Some(false);
                        m.limits.leverage.max = row.max_leverage;
                        m.maker_fee = row.maker_commission;
                        m.taker_fee = row.taker_commission;
                        if !markets.iter().any(|old: &Market| old.id == m.id) {
                            markets.push(m);
                        }
                    }
                }
                if markets.is_empty() {
                    self.drb_operation_failed(
                        cx,
                        Operation::Instruments,
                        "Coinbase drb catalogue contains no supported perpetuals",
                    );
                    return;
                }
                cx.store.set_markets(markets);
                self.public.drb.catalogue_ready = true;
                self.public.retry.succeeded(cx, &None);
                self.drb_summaries(cx);
            }
            Operation::Summaries => {
                let tickers: Vec<_> = result
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|row| {
                        ticker(row, true, cx.now)
                            .map(|t| (t, number(row, "creation_timestamp").unwrap_or(cx.now)))
                    })
                    .filter(|(t, _)| cx.store.market(&t.symbol).is_some())
                    .collect();
                if tickers.is_empty() {
                    self.drb_operation_failed(
                        cx,
                        Operation::Summaries,
                        "Coinbase drb ticker catalogue is empty",
                    );
                    return;
                }
                if !self.public.drb.ready {
                    self.public.drb.ready = true;
                    for (t, timestamp) in &tickers {
                        self.public
                            .drb
                            .ticker_time
                            .insert(t.symbol.clone(), *timestamp);
                    }
                    cx.store
                        .set_tickers(tickers.into_iter().map(|(t, _)| t).collect());
                    if !self.signers.has_primary() {
                        cx.store.set_orders(vec![]);
                        cx.store.set_positions(vec![]);
                        cx.store.set_balance(Default::default());
                    }
                    self.public_pending(cx);
                    self.drb_connect(cx);
                    self.private_after_markets(cx);
                } else {
                    for (t, timestamp) in tickers {
                        if self
                            .public
                            .drb
                            .ticker_time
                            .get(&t.symbol)
                            .is_some_and(|old| *old > timestamp)
                        {
                            continue;
                        }
                        self.public
                            .drb
                            .ticker_time
                            .insert(t.symbol.clone(), timestamp);
                        cx.store.ensure_ticker(&t.symbol);
                        cx.store.update_ticker(&t.symbol.clone(), |old| *old = t);
                    }
                }
                self.public.retry.succeeded(cx, &None);
                if self.public.poll.is_none() {
                    self.public.poll = Some(cx.set_timer(5000.));
                }
            }
            Operation::Chart(mut chart) => {
                if result["status"] != "no_data" {
                    let Some(ticks) = result["ticks"].as_array() else {
                        let _ =
                            cx.fail_context(&chart.context, "Malformed Coinbase drb chart ticks");
                        return;
                    };
                    let fields = ["open", "high", "low", "close", "volume"];
                    if fields
                        .iter()
                        .any(|k| result[*k].as_array().is_none_or(|v| v.len() != ticks.len()))
                        || ticks.len() > 5001
                        || result["status"] != "ok"
                    {
                        let _ =
                            cx.fail_context(&chart.context, "Malformed Coinbase drb chart arrays");
                        return;
                    }
                    for (i, tick) in ticks.iter().enumerate() {
                        let Some(c) = chart_candle(
                            &json!({"tick": tick, "open":result["open"][i], "high":result["high"][i], "low":result["low"][i], "close":result["close"][i], "volume":result["volume"][i]}),
                        ) else {
                            let _ = cx.fail_context(&chart.context, "Invalid Coinbase drb candle");
                            return;
                        };
                        let ms = c.timestamp * 1000.;
                        if ms >= chart.cursor as f64 && ms < chart.page_end as f64 {
                            chart.rows.insert(ms as u64, c);
                        }
                    }
                }
                chart.cursor = chart.page_end;
                if chart.cursor < chart.end {
                    self.drb_chart_page(cx, chart);
                } else {
                    let _ = cx.done_context(
                        &chart.context,
                        serde_json::to_string(&chart.rows.into_values().collect::<Vec<_>>())
                            .unwrap(),
                    );
                }
            }
            Operation::Trades {
                context,
                symbol,
                limit,
            } => {
                let Some(trades) = result["trades"]
                    .as_array()
                    .filter(|t| t.len() <= limit as usize)
                else {
                    let _ = cx.fail_context(&context, "Malformed Coinbase drb public trades");
                    return;
                };
                let mut rows = vec![];
                for trade in trades {
                    let Some(row) = trade_result(trade, &symbol) else {
                        let _ = cx.fail_context(&context, "Invalid Coinbase drb public trade");
                        return;
                    };
                    rows.push(row);
                }
                let _ = cx.done_context(&context, serde_json::to_string(&rows).unwrap());
            }
        }
    }
    pub(super) fn drb_failed(&mut self, cx: &mut Cx<'_, Pending>, sequence: u64, error: &str) {
        if let Some((id, context, op)) = self.public.drb.pending.remove(&sequence) {
            self.drb.failed(id, ErrorKind::Network);
            if matches!(op, Operation::Instruments | Operation::Summaries) {
                let _ = cx.finish_http_context(&context);
            }
            self.drb_operation_failed(cx, op, error);
        }
    }
    fn drb_operation_failed(&mut self, cx: &mut Cx<'_, Pending>, op: Operation, error: &str) {
        match op {
            Operation::Chart(chart) => {
                let _ = cx.fail_context(&chart.context, error);
            }
            Operation::Trades { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            _ => {
                for command in std::mem::take(&mut self.public.deferred) {
                    match command {
                        Command::FetchCandles { command, .. }
                        | Command::Query {
                            command,
                            query: Query::PublicTrades { .. },
                        } => {
                            let error = safe_cex_engine::VenueError::new(
                                "Coinbase public catalogue unavailable",
                            )
                            .kind(ErrorKind::Internal)
                            .venue_message(error);
                            if let Some(context) = self.public.drb.deferred_reads.remove(&command) {
                                let _ = cx.settle_caller_read_root(&context, Err(error));
                            }
                        }
                        other => self.public.deferred.push(other),
                    }
                }
                cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                if let Some(timer) = self.public.poll.take() {
                    cx.clear_timer(timer);
                }
                self.public.retry.failed(cx, None);
            }
        }
    }
    pub(super) fn drb_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.public.supervisor.owns_timer(timer) {
            let expired: Vec<_> = self
                .public
                .drb
                .acknowledgements
                .iter()
                .filter_map(|(id, start)| (cx.now - start >= 10_000.).then_some(*id))
                .collect();
            for id in &expired {
                self.drb.failed(*id, ErrorKind::Network);
                self.public.drb.acknowledgements.remove(id);
            }
            match self.public.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.drb_connect(cx),
                SocketTick::TimedOut(_) => self.reconnect_public(cx),
                _ if !expired.is_empty()
                    || (self.public.open
                        && self
                            .public
                            .books
                            .values()
                            .any(|b| !b.live && cx.now - b.waiting_since >= 10_000.)) =>
                {
                    self.reconnect_public(cx)
                }
                _ => {}
            }
            return true;
        }
        if self.public.poll == Some(timer) || self.public.retry.on_timer(timer).is_some() {
            self.public.poll = None;
            self.drb_start(cx);
            return true;
        }
        false
    }
    pub(super) fn drb_repair(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        topic: &SubscriptionTopic,
    ) -> Result<(), String> {
        if !self.public.open {
            return Err("Coinbase drb public socket is not open".into());
        }
        let channel = match topic {
            SubscriptionTopic::OrderBook { symbol } => {
                let Some(depth) = self.public.books.get_mut(symbol) else {
                    return Err(format!("{topic} is not subscribed"));
                };
                depth.live = false;
                depth.book.clear();
                depth.change_id = None;
                depth.waiting_since = cx.now;
                cx.book_changed(symbol);
                format!("book.{symbol}.100ms")
            }
            SubscriptionTopic::Candles { symbol, interval } => {
                if !self
                    .public
                    .candles
                    .contains_key(&(symbol.clone(), *interval))
                {
                    return Err(format!("{topic} is not subscribed"));
                }
                candle_channel(symbol, *interval)
                    .ok_or_else(|| interval.unsupported("Coinbase drb"))?
            }
        };
        self.drb_subscribe(cx, vec![channel.clone()], false)?;
        self.drb_subscribe(cx, vec![channel], true)
    }
    pub(super) fn drb_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(envelope) = serde_json::from_str::<Value>(text) else {
            return;
        };
        if let Some(id) = envelope["id"].as_u64() {
            if self.public.drb.acknowledgements.remove(&id).is_some() {
                if self.drb.response_at(id, 200, text, cx.now).is_err() {
                    self.reconnect_public(cx);
                } else {
                    self.public.supervisor.received(cx.now);
                }
            }
            return;
        }
        if envelope["jsonrpc"] != "2.0" || envelope["method"] != "subscription" {
            return;
        }
        let Some(channel) = envelope["params"]["channel"].as_str() else {
            return;
        };
        let data = &envelope["params"]["data"];
        if channel.starts_with("book.") {
            if !self
                .public
                .books
                .keys()
                .any(|s| channel == format!("book.{s}.100ms"))
            {
                return;
            }
            if !self.drb_book(cx, channel, data) {
                self.reconnect_public(cx);
                return;
            }
        } else if channel.starts_with("ticker.") {
            if let Some(t) = ticker(data, false, cx.now).filter(|t| {
                channel == format!("ticker.{}.100ms", t.symbol)
                    && cx.store.market(&t.symbol).is_some()
            }) {
                let timestamp = number(data, "timestamp").unwrap_or(cx.now);
                if self
                    .public
                    .drb
                    .ticker_time
                    .get(&t.symbol)
                    .is_some_and(|old| *old > timestamp)
                {
                    return;
                }
                self.public
                    .drb
                    .ticker_time
                    .insert(t.symbol.clone(), timestamp);
                cx.store.ensure_ticker(&t.symbol);
                cx.store.update_ticker(&t.symbol.clone(), |old| *old = t);
            } else {
                return;
            }
        } else if channel.starts_with("chart.trades.") {
            if !self
                .public
                .candles
                .keys()
                .any(|(s, i)| candle_channel(s, *i).as_deref() == Some(channel))
            {
                return;
            }
            let Some(c) = chart_candle(data) else {
                return;
            };
            for ((symbol, interval), (_, last)) in &mut self.public.candles {
                if candle_channel(symbol, *interval).as_deref() == Some(channel)
                    && last.is_none_or(|old| {
                        old.timestamp < c.timestamp
                            || (old.timestamp == c.timestamp && old.volume <= c.volume)
                    })
                {
                    *last = Some(c);
                    cx.candle(symbol, *interval, c);
                }
            }
        } else if channel.starts_with("trades.") {
            if !self
                .public
                .candles
                .keys()
                .any(|(s, i)| i.ms() < 60_000. && candle_channel(s, *i).as_deref() == Some(channel))
            {
                return;
            }
            let Some(trades) = data.as_array() else {
                return;
            };
            for trade in trades {
                self.drb_trade(cx, channel, trade);
            }
        } else {
            return;
        }
        self.public.supervisor.received(cx.now);
    }
    fn drb_book(&mut self, cx: &mut Cx<'_, Pending>, channel: &str, row: &Value) -> bool {
        let Some(symbol) = row["instrument_name"].as_str() else {
            return false;
        };
        if channel != format!("book.{symbol}.100ms") {
            return false;
        }
        let Some(depth) = self.public.books.get_mut(symbol) else {
            return true;
        };
        let Some(id) = row["change_id"].as_u64() else {
            return false;
        };
        if depth.change_id.is_some_and(|last| id <= last) {
            return true;
        }
        // An old subscription can still deliver changes while its replacement
        // snapshot is pending. They cannot establish or invalidate that book.
        if !depth.live && row["type"] == "change" {
            return true;
        }
        let snapshot = row["type"] == "snapshot";
        if !snapshot
            && (row["type"] != "change"
                || !depth.live
                || row["prev_change_id"].as_u64() != depth.change_id)
        {
            return false;
        }
        let mut updates = vec![];
        for (name, side) in [("bids", BookSide::Bids), ("asks", BookSide::Asks)] {
            let Some(levels) = row[name].as_array() else {
                return false;
            };
            for level in levels {
                let Some(level) = level.as_array().filter(|l| l.len() == 3) else {
                    return false;
                };
                let Some(action) = level[0].as_str() else {
                    return false;
                };
                let Some(price) = level[1].as_f64().filter(|n| n.is_finite() && *n > 0.) else {
                    return false;
                };
                let Some(amount) = level[2].as_f64().filter(|n| n.is_finite() && *n >= 0.) else {
                    return false;
                };
                if !matches!(action, "new" | "change" | "delete")
                    || (action == "delete" && amount != 0.)
                {
                    return false;
                }
                updates.push((side, price, amount));
            }
        }
        if snapshot {
            depth.book.clear();
        }
        for (side, price, amount) in updates {
            depth.book.apply(side, price, amount);
        }
        depth.live = true;
        depth.change_id = Some(id);
        cx.book_changed(symbol);
        true
    }
    fn drb_trade(&mut self, cx: &mut Cx<'_, Pending>, channel: &str, trade: &Value) {
        let Some(symbol) = trade["instrument_name"].as_str() else {
            return;
        };
        if channel != format!("trades.{symbol}.100ms")
            || cx.store.market(symbol).is_none()
            || trade_result(trade, symbol).is_none()
        {
            return;
        }
        let Some(sequence) = trade["trade_seq"].as_u64() else {
            return;
        };
        if self
            .public
            .drb
            .trade_sequence
            .get(symbol)
            .is_some_and(|last| sequence <= *last)
        {
            return;
        }
        self.public
            .drb
            .trade_sequence
            .insert(symbol.into(), sequence);
        let time = number(trade, "timestamp").unwrap() / 1000.;
        let price = number(trade, "price").unwrap();
        let amount = number(trade, "amount").unwrap();
        for ((market, interval), (_, last)) in &mut self.public.candles {
            if market != symbol || interval.ms() >= 60_000. {
                continue;
            }
            let bucket = (time / interval_seconds(*interval)).floor() * interval_seconds(*interval);
            let c = match *last {
                Some(old) if old.timestamp == bucket => Candle {
                    high: old.high.max(price),
                    low: old.low.min(price),
                    close: price,
                    volume: old.volume + amount,
                    ..old
                },
                Some(old) if old.timestamp > bucket => continue,
                _ => Candle {
                    timestamp: bucket,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume: amount,
                },
            };
            *last = Some(c);
            cx.candle(market, *interval, c);
        }
    }
    pub(super) fn drb_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        if !self.public.drb.ready
            && matches!(
                &command,
                Command::SubscribeOrderBook { .. }
                    | Command::UnsubscribeOrderBook { .. }
                    | Command::SubscribeCandles { .. }
                    | Command::UnsubscribeCandles { .. }
                    | Command::FetchCandles { .. }
                    | Command::Query {
                        query: Query::PublicTrades { .. },
                        ..
                    }
            )
        {
            if self.public.deferred.len() < 256 {
                if let Some(command) = command.completion_id() {
                    let Ok(context) = cx.admit_caller_read_root(None, None) else {
                        return None;
                    };
                    self.public.drb.deferred_reads.insert(command, context);
                }
                self.public.deferred.push(command);
            } else {
                if matches!(
                    command,
                    Command::FetchCandles { .. } | Command::Query { .. }
                ) {
                    crate::local_context::complete(
                        cx,
                        safe_cex_engine::request_context::Operation::Read,
                        Err("Coinbase public startup command queue is full".into()),
                    );
                }
                cx.notice(
                    Level::Warn,
                    code::SUBSCRIPTION_REJECTED,
                    "Coinbase public startup command queue is full",
                );
            }
            return None;
        }
        match command {
            Command::SubscribeOrderBook { symbol } => {
                cx.store.market(&symbol)?;
                let depth = self.public.books.entry(symbol.clone()).or_default();
                depth.subscribers += 1;
                if depth.subscribers == 1 {
                    depth.waiting_since = cx.now;
                    self.drb_subscribe_or_reconnect(cx, vec![format!("book.{symbol}.100ms")], true);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(depth) = self.public.books.get_mut(&symbol) {
                    depth.subscribers = depth.subscribers.saturating_sub(1);
                    if depth.subscribers == 0 {
                        self.public.books.remove(&symbol);
                        self.drb_subscribe_or_reconnect(
                            cx,
                            vec![format!("book.{symbol}.100ms")],
                            false,
                        );
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                cx.store.market(&symbol)?;
                let Some(channel) = candle_channel(&symbol, interval) else {
                    cx.notice(
                        Level::Warn,
                        code::SUBSCRIPTION_REJECTED,
                        interval.unsupported("Coinbase drb"),
                    );
                    return None;
                };
                let first = !self
                    .public
                    .candles
                    .keys()
                    .any(|(s, i)| candle_channel(s, *i).as_deref() == Some(&channel));
                self.public
                    .candles
                    .entry((symbol, interval))
                    .or_insert((0, None))
                    .0 += 1;
                if first {
                    self.drb_subscribe_or_reconnect(cx, vec![channel], true);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval);
                if let Some((count, _)) = self.public.candles.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.public.candles.remove(&key);
                        if let Some(channel) = candle_channel(&symbol, interval)
                            && !self
                                .public
                                .candles
                                .keys()
                                .any(|(s, i)| candle_channel(s, *i).as_deref() == Some(&channel))
                        {
                            self.drb_subscribe_or_reconnect(cx, vec![channel], false);
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some(resolution) = resolution(interval) else {
                    self.drb_local_result(
                        cx,
                        command,
                        Err(interval.unsupported("Coinbase drb").into()),
                    );
                    return None;
                };
                if cx.store.market(&symbol).is_none() {
                    self.drb_local_result(cx, command, Err("Unknown Coinbase drb market".into()));
                    return None;
                }
                if from_ms
                    .into_iter()
                    .chain(to_ms)
                    .any(|n| !n.is_finite() || n.fract() != 0.)
                {
                    self.drb_local_result(
                        cx,
                        command,
                        Err("Invalid Coinbase drb candle range".into()),
                    );
                    return None;
                }
                let utc_now = cx.protocol_now();
                let end = to_ms.unwrap_or(utc_now).min(utc_now).floor();
                let start = from_ms
                    .unwrap_or(end - interval.ms() * f64::from(limit.unwrap_or(300).clamp(1, 1000)))
                    .max(0.);
                if ![start, end]
                    .iter()
                    .all(|n| n.is_finite() && *n >= 0. && n.fract() == 0. && *n <= 253402300799999.)
                    || (end - start) / interval.ms() > 100_000.
                {
                    self.drb_local_result(
                        cx,
                        command,
                        Err("Invalid Coinbase drb candle range".into()),
                    );
                    return None;
                }
                if end <= start {
                    self.drb_local_result(cx, command, Ok("[]".into()));
                    return None;
                }
                let Ok(context) = self
                    .public
                    .drb
                    .deferred_reads
                    .remove(&command)
                    .map(Ok)
                    .unwrap_or_else(|| cx.admit_http_command(drb_read_policy()))
                else {
                    return None;
                };
                self.drb_chart_page(
                    cx,
                    Box::new(Chart {
                        context,
                        symbol,
                        resolution,
                        seconds: interval_seconds(interval),
                        cursor: start as u64,
                        end: end as u64,
                        page_end: start as u64,
                        rows: BTreeMap::new(),
                    }),
                );
            }
            Command::Query {
                command,
                query: Query::PublicTrades { symbol, limit },
            } => {
                let limit = limit.unwrap_or(100);
                if cx.store.market(&symbol).is_none() || !(1..=1000).contains(&limit) {
                    self.drb_local_result(
                        cx,
                        command,
                        Err("Invalid Coinbase drb public trade request".into()),
                    );
                    return None;
                }
                let Ok(context) = self
                    .public
                    .drb
                    .deferred_reads
                    .remove(&command)
                    .map(Ok)
                    .unwrap_or_else(|| cx.admit_http_command(drb_read_policy()))
                else {
                    return None;
                };
                self.drb_request(
                    cx,
                    "public/get_last_trades_by_instrument",
                    json!({"instrument_name":symbol, "count":limit, "sorting":"asc"}),
                    Operation::Trades {
                        context,
                        symbol,
                        limit,
                    },
                );
            }
            Command::SetTickerSymbols { .. } => {}
            other => return Some(other),
        }
        None
    }
    fn drb_chart_page(&mut self, cx: &mut Cx<'_, Pending>, mut chart: Box<Chart>) {
        chart.page_end = (chart.cursor + (chart.seconds * 1000. * 1000.) as u64).min(chart.end);
        self.drb_request(cx, "public/get_tradingview_chart_data", json!({"instrument_name":chart.symbol, "resolution":chart.resolution, "start_timestamp":chart.cursor, "end_timestamp":chart.page_end}), Operation::Chart(chart));
    }
    fn drb_local_result(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        result: Result<String, safe_cex_engine::VenueError>,
    ) {
        if let Some(context) = self.public.drb.deferred_reads.remove(&command) {
            let _ = cx.settle_caller_read_root(&context, result);
        } else {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                result,
            );
        }
    }
}
fn drb_read_policy() -> Policy {
    Policy {
        operation: safe_cex_engine::request_context::Operation::Read,
        credential: None,
        timeout_ms: crate::drb::REQUEST_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn candle_channel(symbol: &str, interval: Interval) -> Option<String> {
    if interval.ms() > 0. && interval.ms() < 60_000. {
        Some(format!("trades.{symbol}.100ms"))
    } else {
        resolution(interval).map(|r| format!("chart.trades.{symbol}.{r}"))
    }
}
fn trade_result(trade: &Value, symbol: &str) -> Option<Value> {
    if trade["instrument_name"].as_str()? != symbol {
        return None;
    }
    let id = trade["trade_id"].as_str().filter(|s| !s.is_empty())?;
    let side = trade["direction"]
        .as_str()
        .filter(|s| matches!(*s, "buy" | "sell"))?;
    let price = number(trade, "price").filter(|n| *n > 0.)?;
    let amount = number(trade, "amount").filter(|n| *n > 0.)?;
    let timestamp = number(trade, "timestamp")
        .filter(|n| *n >= 0. && n.fract() == 0. && *n <= 253402300799999.)?
        as u64;
    let quote = price * amount;
    quote.is_finite().then(|| json!({"id":id, "symbol":symbol, "side":side, "price":price, "qty":amount, "quoteQty":quote, "timestamp":timestamp}))
}
