//! BingX stablecoin-margined perpetual swaps on the sans-I/O engine.
pub mod account;
pub mod account_wire;
mod history;
pub mod paths;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use serde::Deserialize;
use sign::{Credentials, Params};
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}

pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Markets {
        rows: Vec<Market>,
        skipped: Vec<String>,
    },
    Daily {
        rows: Vec<Ticker>,
        skipped: Vec<String>,
    },
    Premium {
        skipped: Vec<String>,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        valid: bool,
    },
    Signed(Box<Signed>),
}

/// A signed request kept re-signable, so a signature rejection is resent once.
pub struct Signed {
    inner: Pending,
    method: Method,
    path: &'static str,
    params: Params,
    at: Option<ArrayAt>,
    resent: bool,
}

/// Signs and sends a request; signed requests can be re-signed after a 100001.
pub(crate) fn issue(
    cx: &mut Cx<'_, Pending>,
    credentials: Option<&Credentials>,
    method: Method,
    path: &'static str,
    params: Params,
    at: Option<ArrayAt>,
    pending: Pending,
) {
    let (url, headers, body) = params
        .clone()
        .into_request(method, path, credentials, cx.now);
    let pending = if credentials.is_some() && sign::signs(path) {
        Pending::Signed(Box::new(Signed {
            inner: pending,
            method,
            path,
            params,
            at,
            resent: false,
        }))
    } else {
        pending
    };
    match at {
        Some(at) => cx.request_streamed_body(method, url, headers, body, at, pending),
        None => cx.request(method, url, headers, body, pending),
    };
}

fn signature_rejected(body: &str) -> bool {
    serde_json::from_str::<Rejection<'_>>(body).is_ok_and(|r| {
        r.code == Some(100_001)
            || r.msg
                .to_ascii_lowercase()
                .contains("signature verification failed")
    })
}

/// Logs a read's skipped rows once, by identity only.
pub(crate) fn log_skipped(cx: &mut Cx<'_, Pending>, kind: &str, ids: &[String]) {
    log_skipped_at(cx, LogLevel::Warning, kind, ids, "");
}
pub(crate) fn log_skipped_at(
    cx: &mut Cx<'_, Pending>,
    level: LogLevel,
    kind: &str,
    ids: &[String],
    note: &str,
) {
    if ids.is_empty() {
        return;
    }
    let shown = ids
        .iter()
        .take(10)
        .map(|id| if id.is_empty() { "unidentified" } else { id })
        .collect::<Vec<_>>();
    let more = ids.len().saturating_sub(shown.len());
    cx.log(
        level,
        format!(
            "BingX skipped {} malformed {kind} row(s): {}{}{note}",
            ids.len(),
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ),
    );
}

#[derive(Clone, Copy)]
enum List {
    Markets,
    Daily,
    Premium,
}

struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(5_000.0),
                backoff_base_ms: 1_000.0,
                ..Config::default()
            }),
        }
    }
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    book: Book,
    ready: bool,
}

pub struct Bingx {
    config_error: Option<&'static str>,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
    credentials: Option<Credentials>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    seed: Vec<Ticker>,
    seed_index: Vec<Option<usize>>,
    seed_reported: Vec<bool>,
    sequence: u64,
    retries: HashMap<TimerId, List>,
    failures: [u32; 3],
    // Funding failures commit tickers without it and retry; private start never waits.
    tickers_committed: bool,
}

impl Bingx {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        let config_error = options
            .read_credentials
            .as_ref()
            .and_then(|read| read.validate().err());
        let credentials = if options.public_data_only {
            None
        } else {
            options
                .key
                .as_ref()
                .zip(options.secret.as_ref())
                .filter(|(k, s)| !k.is_empty() && !s.is_empty())
                .map(|(key, secret)| Credentials {
                    key: key.clone(),
                    secret: secret.clone(),
                    read: options.read_credentials.clone(),
                })
        };
        Self {
            config_error,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            history: history::History::default(),
            credentials,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            seed: Vec::new(),
            seed_index: Vec::new(),
            seed_reported: Vec::new(),
            sequence: 0,
            retries: HashMap::new(),
            failures: [0; 3],
            tickers_committed: false,
        }
    }

    fn observe_orders(&mut self, cx: &Cx<'_, Pending>) {
        for (order, kind, side, attached, native_status) in self.account.drain_order_meta() {
            self.trading
                .observe_order(&order, &kind, &side, attached, &native_status);
        }
        self.trading.retain_open_orders(cx.store);
    }

    fn retry_list(&mut self, cx: &mut Cx<'_, Pending>, kind: List) {
        let slot = match kind {
            List::Markets => 0,
            List::Daily => 1,
            List::Premium => 2,
        };
        let delay = (1000. * 2f64.powi(self.failures[slot].min(5) as i32)).min(30_000.);
        self.failures[slot] += 1;
        self.retries.insert(cx.set_timer(delay), kind);
    }

    fn read(&self, cx: &mut Cx<'_, Pending>, path: &'static str, pending: Pending) {
        issue(
            cx,
            self.credentials.as_ref(),
            Method::Get,
            path,
            Params::new(),
            Some(ArrayAt::EnvelopeField("data")),
            pending,
        );
    }

    fn resend(&self, cx: &mut Cx<'_, Pending>, signed: Signed) {
        cx.log(
            LogLevel::Warning,
            "BingX rejected a request signature; re-signing it once",
        );
        let Signed {
            inner,
            method,
            path,
            params,
            at,
            ..
        } = signed;
        let (url, headers, body) =
            params
                .clone()
                .into_request(method, path, self.credentials.as_ref(), cx.now);
        let pending = Pending::Signed(Box::new(Signed {
            inner,
            method,
            path,
            params,
            at,
            resent: true,
        }));
        match at {
            Some(at) => cx.request_streamed_body(method, url, headers, body, at, pending),
            None => cx.request(method, url, headers, body, pending),
        };
    }

    /// The seed's ticker slot for a native symbol; after commit, the store's.
    fn ticker_slot(&self, cx: &Cx<'_, Pending>, id: &str) -> Option<usize> {
        let market = cx
            .store
            .market_by_id(id)
            .and_then(|m| cx.store.market_index(&m.symbol))?;
        self.seed_index.get(market as usize).copied().flatten()
    }

    /// Publishes the seeded tickers and starts the private side. Without a
    /// funding read every daily ticker is kept; with one, only reported ones.
    fn commit_tickers(&mut self, cx: &mut Cx<'_, Pending>, keep_unreported: bool) {
        let reported = std::mem::take(&mut self.seed_reported);
        if !keep_unreported {
            let mut index = 0;
            self.seed.retain(|_| {
                let keep = reported[index];
                index += 1;
                keep
            });
        }
        let rows = std::mem::take(&mut self.seed);
        self.seed_index.fill(None);
        for (i, ticker) in rows.iter().enumerate() {
            if let Some(market) = cx.store.market_index(&ticker.symbol) {
                self.seed_index[market as usize] = Some(i);
            }
        }
        cx.store.set_tickers(rows);
        self.tickers_committed = true;
        if let Some(c) = &self.credentials {
            self.account.start(cx, c);
        }
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket = Some(cx.connect_channel(
            paths::PUBLIC_WS.to_owned(),
            safe_cex_engine::SocketChannel::Public,
        ));
        self.public.supervisor.connecting(cx);
        self.public.open = false;
    }

    fn send_topic(&mut self, cx: &mut Cx<'_, Pending>, topic: &str, subscribe: bool) {
        if !self.public.open {
            return;
        }
        if let Some(socket) = self.public.socket {
            self.sequence += 1;
            cx.send(
                socket,
                format!(
                    r#"{{"id":"{}","reqType":"{}","dataType":"{}"}}"#,
                    self.sequence,
                    if subscribe { "sub" } else { "unsub" },
                    topic
                ),
            );
        }
    }

    fn symbol_id(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        cx.store.market(symbol).map(|m| m.id.clone())
    }

    fn topic_book(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        Some(format!(
            "market.depth.{}.step0.level100",
            self.symbol_id(cx, symbol)?
        ))
    }
    fn topic_candle(&self, cx: &Cx<'_, Pending>, symbol: &str, interval: &str) -> Option<String> {
        Some(format!(
            "market.kline.{}.{}.trade.utc+8",
            self.symbol_id(cx, symbol)?,
            stream_interval(interval)?
        ))
    }

    fn reconnect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.public.socket.take() {
            cx.close(socket)
        }
        self.public.open = false;
        for state in self.books.values_mut() {
            state.ready = false;
            state.book.clear()
        }
        self.public.supervisor.retry(cx);
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Some(socket) = self.public.socket else {
            return;
        };
        self.public.supervisor.received(cx.now);
        if text == "Ping" {
            cx.send(socket, "Pong");
            return;
        }
        if text == "Pong" {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if message.ping.is_some() {
            cx.send(socket, "Pong");
            return;
        }
        if message.pong.is_some() {
            return;
        }
        let Some(data) = message.data else { return };
        if message.data_type == "market.contracts" {
            let result = safe_cex_engine::json::each_in_field::<LiveTicker<'_>>(
                data.get(),
                "contracts",
                |row| {
                    let Some(market) = cx.store.market_index_by_id(&row.symbol) else {
                        return;
                    };
                    let Some(ticker) = self.seed_index.get(market as usize).copied().flatten()
                    else {
                        return;
                    };
                    let step = cx.store.markets()[market as usize].precision.price;
                    cx.store.update_ticker_at(ticker as u32, |t| {
                        if row.trade_price > 0. {
                            t.last = row.trade_price;
                            (t.bid, t.ask) = touch(row.trade_price, step)
                        }
                        t.mark = row.fair_price;
                        t.index = row.index_price;
                        t.funding_rate = row.funding_rate / 100.;
                        t.percentage = row.change_percentage.unwrap_or_default();
                        t.volume = row.volume.unwrap_or_default();
                        t.quote_volume = row.value.unwrap_or_default();
                        t.funding_time = 0.;
                    });
                },
            );
            if result.is_err() {
                cx.log(LogLevel::Warning, "invalid BingX ticker frame")
            }
            return;
        }
        if let Some(rest) = message.data_type.strip_prefix("market.depth.") {
            let Some(id) = rest.strip_suffix(".step0.level100") else {
                return;
            };
            let Some(m) = cx.store.market_by_id(id) else {
                return;
            };
            let symbol = m.symbol.clone();
            let Ok(Depth { mut bids, mut asks }) = serde_json::from_str::<Depth>(data.get()) else {
                return;
            };
            if let Some(state) = self.books.get_mut(&symbol) {
                bids.sort_unstable_by(|a, b| b.price.total_cmp(&a.price));
                asks.sort_unstable_by(|a, b| a.price.total_cmp(&b.price));
                state.book.load_sorted(
                    BookSide::Bids,
                    bids.into_iter().map(|l| (l.price, l.volume)),
                );
                state.book.load_sorted(
                    BookSide::Asks,
                    asks.into_iter().map(|l| (l.price, l.volume)),
                );
                state.ready = true;
                cx.book_changed(&symbol);
            }
        } else if let Some(rest) = message.data_type.strip_prefix("market.kline.") {
            let mut split = rest.split('.');
            let (Some(id), Some(native_interval)) = (split.next(), split.next()) else {
                return;
            };
            let Some(m) = cx.store.market_by_id(id) else {
                return;
            };
            let symbol = m.symbol.clone();
            let matches: Vec<_> = self
                .candles
                .keys()
                .filter(|(s, i)| s == &symbol && stream_interval(i) == Some(native_interval))
                .cloned()
                .collect();
            let Ok(data) = serde_json::from_str::<CandleData>(data.get()) else {
                return;
            };
            if let Some(row) = data.kline_infos_vo.into_iter().next() {
                let candle = Candle::from(row);
                for (symbol, interval) in matches {
                    cx.candle(&symbol, &interval, candle)
                }
            }
        }
    }
}

fn touch(last: f64, step: f64) -> (f64, f64) {
    if last <= 0. {
        return (0., 0.);
    }
    // Native ticks are powers of ten. Recover common scales without formatting;
    // above 15 digits reciprocal rounding can lose an integer tick.
    let scale = step.recip().round();
    let factor = if (1. ..=1e15).contains(&scale) {
        scale
    } else {
        10f64.powi(num::decimals(step) as i32)
    };
    (
        ((last * factor - step * factor).round() / factor).max(0.),
        (last * factor + step * factor).round() / factor,
    )
}

fn stream_interval(interval: &str) -> Option<&'static str> {
    Some(match interval {
        "1m" => "1min",
        "3m" => "3min",
        "5m" => "5min",
        "15m" => "15min",
        "30m" => "30min",
        "1h" => "1hour",
        "2h" => "2hour",
        "3h" => "3hour",
        "4h" => "4hour",
        "6h" => "6hour",
        "12h" => "12hour",
        "1d" => "1day",
        "1w" => "1week",
        "1M" => "1month",
        "3M" => "3month",
        "1Y" => "1year",
        _ => return None,
    })
}

fn interval_ms(interval: &str) -> Option<f64> {
    stream_interval(interval)?;
    let unit = interval.chars().last()?;
    let count: f64 = interval[..interval.len() - 1].parse().ok()?;
    Some(
        count
            * match unit {
                'm' => 60_000.,
                'h' => 3_600_000.,
                'd' => 86_400_000.,
                'w' => 604_800_000.,
                'M' => 2_592_000_000.,
                'Y' => 31_536_000_000.,
                _ => return None,
            },
    )
}

pub(crate) fn success(response: Response<'_>) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Envelope {
        code: i64,
        #[serde(default)]
        msg: String,
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("BingX HTTP {}: {}", response.status, response.body));
    }
    let envelope: Envelope =
        serde_json::from_str(response.body).map_err(|e| format!("invalid BingX response: {e}"))?;
    if envelope.code == 0 {
        Ok(())
    } else {
        Err(format!("BingX {}: {}", envelope.code, envelope.msg))
    }
}

impl Venue for Bingx {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(error) = self.config_error {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.read(
            cx,
            paths::MARKETS,
            Pending::Markets {
                rows: Vec::with_capacity(1536),
                skipped: Vec::new(),
            },
        );
        if self.credentials.is_none() {
            cx.store.set_balance(Default::default());
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_open(cx, c);
            }
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        self.send_topic(cx, "market.contracts", true);
        let topics: Vec<_> = self
            .books
            .keys()
            .filter_map(|s| self.topic_book(cx, s))
            .chain(
                self.candles
                    .keys()
                    .filter_map(|(s, i)| self.topic_candle(cx, s, i)),
            )
            .collect();
        for topic in topics {
            self.send_topic(cx, &topic, true)
        }
        cx.send(socket, "Ping");
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_message(cx, text, c);
            }
            self.observe_orders(cx);
        } else if self.public.socket == Some(socket) {
            self.public_message(cx, text)
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_close(cx, c);
            }
        } else if self.public.socket == Some(socket) {
            self.reconnect(cx)
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Signed(signed) => self.on_item(cx, &mut signed.inner, item),
            Pending::Account(request) => self.account.on_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::History(request) => self.history.on_item(cx, request, item),
            Pending::Markets { rows, skipped } => {
                if rows.len() == rows.capacity() {
                    rows.reserve_exact(128)
                }
                match serde_json::from_str::<Contract<'_>>(item) {
                    Ok(m) => rows.push(Market {
                        id: m.symbol.to_string(),
                        symbol: m.symbol.replace('-', ""),
                        pretty_symbol: None,
                        base: m.asset.into_owned(),
                        quote: m.currency.into_owned(),
                        active: m.status == 1.,
                        precision: Precision {
                            amount: 10f64.powi(m.quantity_precision as i32).recip(),
                            price: 10f64.powi(m.price_precision as i32).recip(),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: m.trade_min_limit,
                                max: f64::INFINITY,
                            },
                            leverage: MinMax {
                                min: 1.,
                                max: if m.max_long_leverage > 0. {
                                    m.max_long_leverage
                                } else {
                                    100.
                                },
                            },
                            price: None,
                        },
                        contract_size: None,
                        is_expiring: None,
                        is_inverse: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    }),
                    Err(_) => skipped.push(row_symbol(item).unwrap_or_default().into_owned()),
                }
            }
            Pending::Daily { rows, skipped } => match serde_json::from_str::<Daily<'_>>(item) {
                Ok(d) => {
                    if let Some(m) = cx.store.market_by_id(&d.symbol) {
                        let (bid, ask) = touch(d.last_price, m.precision.price);
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid,
                            ask,
                            last: d.last_price,
                            percentage: d.price_change_percent,
                            volume: d.volume,
                            quote_volume: d.quote_volume,
                            ..Ticker::default()
                        })
                    }
                }
                Err(_) => skipped.push(row_symbol(item).unwrap_or_default().into_owned()),
            },
            Pending::Premium { skipped } => match serde_json::from_str::<Premium<'_>>(item) {
                Ok(p) => {
                    let Some(i) = self.ticker_slot(cx, &p.symbol) else {
                        return;
                    };
                    let apply = |t: &mut Ticker| {
                        t.mark = p.mark_price;
                        t.index = p.index_price;
                        t.funding_rate = p.last_funding_rate / 100.;
                        t.funding_time = p.next_funding_time;
                    };
                    if self.tickers_committed {
                        cx.store.update_ticker_at(i as u32, apply);
                    } else {
                        apply(&mut self.seed[i]);
                        self.seed_reported[i] = true
                    }
                }
                Err(_) => {
                    let symbol = row_symbol(item);
                    // A malformed funding row keeps its daily ticker, without funding.
                    if !self.tickers_committed
                        && let Some(i) = symbol.as_deref().and_then(|s| self.ticker_slot(cx, s))
                    {
                        self.seed_reported[i] = true
                    }
                    skipped.push(symbol.unwrap_or_default().into_owned())
                }
            },
            Pending::Candles { rows, valid, .. } => match serde_json::from_str::<Kline>(item) {
                Ok(k) => rows.push(k.into()),
                Err(_) => *valid = false,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let pending = match pending {
            Pending::Signed(signed) => {
                if !signed.resent && signature_rejected(response.body) {
                    self.resend(cx, *signed);
                } else {
                    self.on_response(cx, signed.inner, response);
                }
                return;
            }
            Pending::History(request) => {
                self.history
                    .on_response(cx, self.credentials.as_ref(), request, response);
                return;
            }
            Pending::Trade(request) => {
                self.account.bump_orders();
                self.account.bump_positions();
                let reconcile = (request.changes_settings() || request.reconciles_orders())
                    && success(response).is_ok();
                let accepted = request.accepts_placement(response);
                self.trading
                    .on_response(cx, self.credentials.as_ref(), request, response);
                if accepted {
                    self.account.accept_placed_response(cx, response.body);
                }
                if reconcile && let Some(c) = &self.credentials {
                    self.account.reconcile(cx, c);
                }
                self.observe_orders(cx);
                return;
            }
            Pending::Account(request) => {
                if let Some(c) = &self.credentials {
                    self.account.on_response(cx, request, response, c);
                }
                self.observe_orders(cx);
                return;
            }
            pending => pending,
        };
        if let Err(error) = success(response) {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Markets { rows, skipped } if rows.is_empty() && !skipped.is_empty() => {
                log_skipped(cx, "market", &skipped);
                self.on_request_failed(
                    cx,
                    Pending::Markets { rows, skipped },
                    "no valid BingX market rows",
                )
            }
            Pending::Markets { mut rows, skipped } => {
                log_skipped(cx, "market", &skipped);
                self.account
                    .set_skipped_markets(skipped.iter().map(|id| id.replace('-', "")));
                rows.shrink_to_fit();
                cx.store.set_markets(rows);
                self.connect(cx);
                self.read(
                    cx,
                    paths::DAILY,
                    Pending::Daily {
                        rows: Vec::with_capacity(cx.store.markets().len()),
                        skipped: Vec::new(),
                    },
                )
            }
            Pending::Daily { mut rows, skipped } => {
                log_skipped(cx, "ticker", &skipped);
                // A market whose daily row was malformed still gets a slot, filled
                // later by funding and the ticker stream.
                let reported = rows.len();
                for id in &skipped {
                    if let Some(m) = cx.store.market_by_id(id)
                        && !rows.iter().any(|t| t.id == m.id)
                    {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            ..Ticker::default()
                        });
                    }
                }
                self.seed_index = vec![None; cx.store.markets().len()];
                for (i, ticker) in rows.iter().enumerate() {
                    if let Some(index) = cx.store.market_index(&ticker.symbol) {
                        self.seed_index[index as usize] = Some(i)
                    }
                }
                self.seed_reported = (0..rows.len()).map(|i| i >= reported).collect();
                self.seed = rows;
                self.read(
                    cx,
                    paths::PREMIUM,
                    Pending::Premium {
                        skipped: Vec::new(),
                    },
                )
            }
            Pending::Premium { skipped } => {
                log_skipped(cx, "funding", &skipped);
                if !self.tickers_committed {
                    self.commit_tickers(cx, false);
                }
            }
            Pending::Candles {
                command,
                mut rows,
                valid: true,
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(
                    command,
                    serde_json::to_string(&rows).map_err(|e| e.to_string()),
                )
            }
            pending => self.on_request_failed(cx, pending, "malformed BingX list row"),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        match pending {
            Pending::Signed(signed) if !signed.resent && signature_rejected(response.body) => {
                self.resend(cx, *signed)
            }
            Pending::Signed(signed) => self.on_stream_error(cx, signed.inner, response),
            pending => self.on_request_failed(
                cx,
                pending,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            ),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Signed(signed) => {
                self.on_request_failed(cx, signed.inner, error);
                return;
            }
            Pending::History(request) => {
                self.history.on_failed(cx, request, error);
                return;
            }
            Pending::Trade(request) => {
                self.trading
                    .on_failed(cx, self.credentials.as_ref(), request, error);
                return;
            }
            Pending::Account(request) => {
                if let Some(c) = &self.credentials {
                    self.account.on_request_failed(cx, request, error, c);
                }
                return;
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Markets { .. } => self.retry_list(cx, List::Markets),
            Pending::Daily { .. } => self.retry_list(cx, List::Daily),
            Pending::Premium { .. } => {
                if !self.tickers_committed {
                    cx.log(
                        LogLevel::Warning,
                        "BingX funding read failed; starting without funding rates",
                    );
                    self.commit_tickers(cx, true);
                }
                self.retry_list(cx, List::Premium)
            }
        }
        cx.log(LogLevel::Error, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.history.on_timer(cx, self.credentials.as_ref(), timer);
        if self.account.handles_timer(timer) {
            if let Some(c) = &self.credentials {
                self.account.on_timer(cx, timer, c);
            }
            return;
        }
        if let Some(kind) = self.retries.remove(&timer) {
            let (path, pending) = match kind {
                List::Markets => (
                    paths::MARKETS,
                    Pending::Markets {
                        rows: Vec::new(),
                        skipped: Vec::new(),
                    },
                ),
                List::Daily => (
                    paths::DAILY,
                    Pending::Daily {
                        rows: Vec::with_capacity(cx.store.markets().len()),
                        skipped: Vec::new(),
                    },
                ),
                List::Premium => (
                    paths::PREMIUM,
                    Pending::Premium {
                        skipped: Vec::new(),
                    },
                ),
            };
            self.read(cx, path, pending);
            return;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Reconnect => self.connect(cx),
            Tick::TimedOut(_) => self.reconnect(cx),
            Tick::Ping => {
                if self.public.open
                    && let Some(socket) = self.public.socket
                {
                    cx.send(socket, "Ping");
                }
            }
            Tick::Handled | Tick::Unhandled => {}
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::Query { command, query } => {
                self.history
                    .query(cx, self.credentials.as_ref(), command, query)
            }
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1
                    && let Some(topic) = self.topic_book(cx, &symbol)
                {
                    self.send_topic(cx, &topic, true)
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        if let Some(topic) = self.topic_book(cx, &symbol) {
                            self.send_topic(cx, &topic, false)
                        }
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if stream_interval(&interval).is_none() {
                    cx.log(LogLevel::Error, "unsupported BingX candle interval");
                    return;
                }
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *count += 1;
                if *count == 1
                    && let Some(topic) = self.topic_candle(cx, &symbol, &interval)
                {
                    self.send_topic(cx, &topic, true)
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if let Some(count) = self.candles.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&key);
                        if let Some(topic) = self.topic_candle(cx, &symbol, &interval) {
                            self.send_topic(cx, &topic, false)
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
                let Some(market) = cx.store.market(&symbol) else {
                    cx.done(command, Err(format!("unknown market {symbol}")));
                    return;
                };
                let Some(step) = interval_ms(&interval) else {
                    cx.done(command, Err("invalid interval".into()));
                    return;
                };
                let end = to_ms.unwrap_or(cx.now);
                let start = if limit.is_none() {
                    from_ms.unwrap_or(end - step * 500.)
                } else {
                    end - step * f64::from(limit.unwrap_or(500).min(1500))
                };
                if !start.is_finite() || !end.is_finite() || start > end {
                    cx.done(command, Err("invalid candle time range".into()));
                    return;
                }
                let params = Params::new()
                    .str("symbol", &market.id)
                    .str("interval", &interval)
                    .num("startTime", start)
                    .num("endTime", end);
                issue(
                    cx,
                    self.credentials.as_ref(),
                    Method::Get,
                    paths::KLINES,
                    params,
                    Some(ArrayAt::EnvelopeField("data")),
                    Pending::Candles {
                        command,
                        rows: Vec::new(),
                        valid: true,
                    },
                );
            }
            other => {
                self.account.bump_orders();
                self.account.bump_positions();
                if !self.trading.command(cx, self.credentials.as_ref(), other) {
                    cx.log(LogLevel::Warning, "unsupported BingX command");
                }
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| b.ready).map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| b.ready)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_tick_scales_preserve_decimal_touch_prices() {
        for precision in 0..=18 {
            let factor = 10f64.powi(precision);
            let step = factor.recip();
            for last in [0., step, 0.000_001_234_567_89, 1.234_567_89, 90_000.123_456] {
                let expected = if last <= 0. {
                    (0., 0.)
                } else {
                    (
                        ((last * factor - step * factor).round() / factor).max(0.),
                        (last * factor + step * factor).round() / factor,
                    )
                };
                assert_eq!(
                    super::touch(last, step),
                    expected,
                    "precision={precision}, last={last}"
                );
            }
        }
    }
}
