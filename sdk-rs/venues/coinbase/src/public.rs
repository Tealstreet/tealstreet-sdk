use crate::{Coinbase, Pending, ProductKind, wire::*};
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use safe_cex_engine::time;
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId,
};
use std::collections::{BTreeMap, HashMap};
const INTX: &str = "https://api.international.coinbase.com/api/v1/instruments/";
const WS: &str = "wss://advanced-trade-ws.coinbase.com";

#[cfg(test)]
#[path = "../tests/support/deferred_generation.rs"]
mod deferred_generation_tests;

pub enum Request {
    Instruments {
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        invalid: bool,
        initial: bool,
    },
    Products {
        kind: ProductKind,
        generation: u64,
        cursors: Vec<String>,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        invalid: bool,
    },
    Daily {
        symbol: String,
        generation: u64,
        day: f64,
        rows: Vec<Candle>,
        invalid: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
    },
}
#[derive(Default)]
pub(crate) struct Depth {
    pub book: Book,
    pub live: bool,
    subscribers: u32,
    waiting_since: f64,
}
#[derive(Default)]
struct Daily {
    generation: u64,
    day: f64,
    rows: BTreeMap<u64, Candle>,
    seeded: bool,
    refresh: bool,
    retry: Option<TimerId>,
}
#[derive(Default)]
pub(crate) struct Public {
    socket: Option<SocketId>,
    open: bool,
    recover: bool,
    supervisor: Supervisor,
    sequence: Option<u64>,
    poll: Option<TimerId>,
    // Instruments are `None`; concurrent CFM and Spot product reads each own a key.
    retry: safe_cex_engine::startup::Backoff<Option<ProductKind>>,
    kind: Option<ProductKind>,
    pub books: HashMap<String, Depth>,
    candles: HashMap<(String, Interval), (u32, Option<Candle>)>,
    products: HashMap<String, (f64, f64)>,
    daily: HashMap<String, Daily>,
    daily_generation: u64,
    generation: u64,
    seeding: bool,
    seeds_left: usize,
    seed_markets: Vec<Market>,
    seed_tickers: Vec<Ticker>,
    deferred: Vec<Command>,
}
impl Public {
    pub(crate) fn new() -> Self {
        Self {
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: 20_000.,
                ..Config::default()
            }),
            ..Self::default()
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn market(
    symbol: String,
    base: String,
    quote: String,
    precision: Precision,
    min: f64,
    max: f64,
    contract: f64,
    spot: Option<bool>,
    expiring: Option<bool>,
) -> Market {
    Market {
        id: symbol.clone(),
        symbol,
        pretty_symbol: None,
        base,
        quote,
        active: true,
        precision,
        limits: Limits {
            amount: MinMax { min, max },
            leverage: MinMax {
                min: 1.,
                max: if spot == Some(true) { 1. } else { 10. },
            },
            price: None,
        },
        contract_size: Some(contract),
        is_expiring: expiring,
        is_spot: spot,
        is_perpetual: None,
        is_inverse: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
pub(crate) fn ws_symbol(symbol: &str) -> String {
    if symbol != "USDT-USDC" && symbol != "EURC-USDC" {
        symbol
            .strip_suffix("-USDC")
            .map_or_else(|| symbol.into(), |base| format!("{base}-USD"))
    } else {
        symbol.into()
    }
}
/// Coinbase REST granularity and its seconds.
fn interval(i: Interval) -> Option<(&'static str, f64)> {
    use Interval::*;
    let granularity = match i {
        M1 => "ONE_MINUTE",
        M5 => "FIVE_MINUTE",
        M15 => "FIFTEEN_MINUTE",
        M30 => "THIRTY_MINUTE",
        H1 => "ONE_HOUR",
        H2 => "TWO_HOUR",
        H6 => "SIX_HOUR",
        D1 => "ONE_DAY",
        _ => return None,
    };
    Some((granularity, i.ms() / 1000.))
}
fn candle(row: CandleRow) -> Option<Candle> {
    (row.start >= 0.
        && row.start.fract() == 0.
        && row.open >= 0.
        && row.high >= row.low
        && row.low >= 0.
        && row.close >= 0.
        && row.volume >= 0.)
        .then_some(Candle {
            timestamp: row.start,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
        })
}
pub(crate) fn timestamp(text: &str) -> Option<f64> {
    let part = |a, b| text.get(a..b)?.parse::<i64>().ok();
    let (year, month, day, hour, min) = (
        part(0, 4)?,
        part(5, 7)?,
        part(8, 10)?,
        part(11, 13)?,
        part(14, 16)?,
    );
    if !time::days_in_month(year, month).is_some_and(|last| (1..=last).contains(&day))
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&min)
    {
        return None;
    }
    if text.as_bytes().get(4) != Some(&b'-')
        || text.as_bytes().get(7) != Some(&b'-')
        || text.as_bytes().get(10) != Some(&b'T')
        || text.as_bytes().get(13) != Some(&b':')
        || text.as_bytes().get(16) != Some(&b':')
    {
        return None;
    }
    let sec = text.get(17..)?.strip_suffix('Z')?.parse::<f64>().ok()?;
    if !(0. ..60.).contains(&sec) {
        return None;
    }
    let days = time::days_from_civil(year, month, day);
    Some((days * 86400 + hour * 3600 + min * 60) as f64 + sec)
}
impl Coinbase {
    fn public_poll_interval(&self) -> f64 {
        if self.public.kind == Some(ProductKind::Cfm) {
            5000.0
        } else {
            1000.0
        }
    }
    pub(crate) fn public_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.options.spot_only {
            self.private_ready(ProductKind::Spot, cx);
        } else {
            self.public.kind = Some(ProductKind::Intx);
            self.instruments(cx, true);
        }
    }
    pub(crate) fn private_ready(&mut self, kind: ProductKind, cx: &mut Cx<'_, Pending>) {
        self.public.kind = Some(kind);
        self.public.generation = self.public.generation.wrapping_add(1);
        self.public.seeding = true;
        self.public.seeds_left = if kind == ProductKind::Cfm { 2 } else { 1 };
        self.public.seed_markets.clear();
        self.public.seed_tickers.clear();
        self.request_products(cx, kind);
        if kind == ProductKind::Cfm {
            self.request_products(cx, ProductKind::Spot);
        }
    }
    fn instruments(&mut self, cx: &mut Cx<'_, Pending>, initial: bool) {
        let pending = Request::Instruments {
            markets: vec![],
            tickers: vec![],
            invalid: false,
            initial,
        };
        if let Err(error) = self.request(
            cx,
            Method::Get,
            INTX.into(),
            None,
            Some(ArrayAt::Root),
            Pending::Public(pending),
        ) {
            self.public_failed(
                cx,
                Request::Instruments {
                    markets: vec![],
                    tickers: vec![],
                    invalid: true,
                    initial,
                },
                &error,
            );
        }
    }
    fn request_products(&mut self, cx: &mut Cx<'_, Pending>, kind: ProductKind) {
        let pending = Request::Products {
            kind,
            generation: self.public.generation,
            cursors: vec![],
            markets: vec![],
            tickers: vec![],
            invalid: false,
        };
        self.products_page(cx, pending, None);
    }
    fn products_page(&mut self, cx: &mut Cx<'_, Pending>, pending: Request, cursor: Option<&str>) {
        let Request::Products {
            kind, generation, ..
        } = &pending
        else {
            return;
        };
        let kind = *kind;
        let generation = *generation;
        let public = self.credentials.is_none() && self.options.spot_only;
        let mut path = if public {
            "/api/v3/brokerage/market/products?limit=1000"
        } else {
            "/api/v3/brokerage/products?limit=1000"
        }
        .to_owned();
        match kind {
            ProductKind::Cfm => path.push_str("&product_type=FUTURE"),
            ProductKind::Intx => {
                path.push_str("&product_type=FUTURE&contract_expiry_type=PERPETUAL")
            }
            ProductKind::Spot => {}
        }
        if let Some(cursor) = cursor {
            path.push_str("&cursor=");
            safe_cex_engine::url::encode_into(&mut path, cursor);
        }
        if let Err(error) = self.request(
            cx,
            Method::Get,
            path,
            None,
            Some(ArrayAt::Field("products")),
            Pending::Public(pending),
        ) {
            self.public_failed(
                cx,
                Request::Products {
                    kind,
                    generation,
                    cursors: vec![],
                    markets: vec![],
                    tickers: vec![],
                    invalid: true,
                },
                &error,
            );
        }
    }
    fn public_pending(&mut self, cx: &mut Cx<'_, Pending>) {
        for command in std::mem::take(&mut self.public.deferred) {
            self.public_command(cx, command);
        }
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.public.socket.is_some() || self.public.supervisor.is_waiting() {
            return;
        }
        self.public.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(WS)
                    .channel(SocketChannel::Public)
                    .feed("market", self.public.recover),
            ),
        );
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
    }
    fn subscription(
        &self,
        cx: &mut Cx<'_, Pending>,
        channel: &str,
        symbols: Vec<String>,
        add: bool,
    ) {
        if !self.public.open {
            return;
        }
        #[derive(serde::Serialize)]
        struct Sub<'a> {
            r#type: &'a str,
            product_ids: Vec<String>,
            channel: &'a str,
        }
        cx.send(
            self.public.socket.unwrap(),
            serde_json::to_string(&Sub {
                r#type: if add { "subscribe" } else { "unsubscribe" },
                product_ids: symbols,
                channel,
            })
            .unwrap(),
        );
    }
    pub(crate) fn public_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        self.public.sequence = None;
        for state in self.public.books.values_mut() {
            state.waiting_since = cx.now;
        }
        self.subscription(
            cx,
            "heartbeats",
            cx.store
                .markets()
                .iter()
                .map(|m| ws_symbol(&m.symbol))
                .collect(),
            true,
        );
        let symbols: Vec<_> = self.public.books.keys().map(|s| ws_symbol(s)).collect();
        if !symbols.is_empty() {
            self.subscription(cx, "level2", symbols, true);
        }
        let mut symbols: Vec<_> = self
            .public
            .candles
            .keys()
            .map(|(s, _)| ws_symbol(s))
            .collect();
        symbols.sort();
        symbols.dedup();
        if !symbols.is_empty() {
            self.subscription(cx, "candles", symbols.clone(), true);
            self.subscription(cx, "market_trades", symbols, true);
        }
        let refresh: Vec<_> = self
            .public
            .daily
            .iter()
            .filter_map(|(symbol, state)| state.refresh.then_some(symbol.clone()))
            .collect();
        let day = (cx.now / 86400000.).floor() * 86400.;
        for symbol in refresh {
            let state = self.public.daily.get_mut(&symbol).unwrap();
            state.refresh = false;
            if state.day != day {
                state.day = day;
                state.rows.clear();
            }
            self.daily_seed(cx, &symbol, day);
        }
        true
    }
    fn invalidate(&mut self, cx: &mut Cx<'_, Pending>) {
        for (symbol, book) in &mut self.public.books {
            book.live = false;
            book.book.clear();
            cx.book_changed(symbol);
        }
    }
    pub(crate) fn public_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.socket = None;
        self.public.open = false;
        self.public.sequence = None;
        self.invalidate(cx);
        // A reconnect only supplies the current five-minute bucket.
        self.public.daily_generation = self.public.daily_generation.wrapping_add(1);
        for state in self.public.daily.values_mut() {
            state.generation = self.public.daily_generation;
            state.seeded = false;
            state.refresh = true;
            if let Some(timer) = state.retry.take() {
                cx.clear_timer(timer);
            }
        }
        self.public.supervisor.retry(cx);
        true
    }
    fn reconnect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.recover = true;
        if let Some(socket) = self.public.socket {
            cx.close(socket);
            self.public_close(cx, socket);
        }
    }
    fn daily_seed(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, day: f64) {
        let Some(state) = self.public.daily.get(symbol) else {
            return;
        };
        let generation = state.generation;
        let end = (cx.now / 1000.).min(day + 86400.).max(day + 1.);
        let path = format!(
            "/api/v3/brokerage/market/products/{}/candles?granularity=FIVE_MINUTE&start={day:.0}&end={end:.0}&limit=300",
            ws_symbol(symbol)
        );
        let pending = Request::Daily {
            symbol: symbol.into(),
            generation,
            day,
            rows: vec![],
            invalid: false,
        };
        if let Err(error) = self.request(
            cx,
            Method::Get,
            path,
            None,
            Some(ArrayAt::Field("candles")),
            Pending::Public(pending),
        ) {
            self.public_failed(
                cx,
                Request::Daily {
                    symbol: symbol.into(),
                    generation,
                    day,
                    rows: vec![],
                    invalid: true,
                },
                &error,
            );
        }
    }
    fn emit_daily(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.public.daily.get(symbol).filter(|s| s.seeded) else {
            return;
        };
        let Some(first) = state.rows.values().next() else {
            return;
        };
        let mut total = *first;
        total.timestamp = state.day;
        total.volume = 0.;
        for row in state.rows.values() {
            total.high = total.high.max(row.high);
            total.low = total.low.min(row.low);
            total.close = row.close;
            total.volume += row.volume;
        }
        // Longer chart intervals retain TS's daily source-candle contract.
        for ((market, interval), (_, last)) in &mut self.public.candles {
            if market == symbol && interval_seconds(*interval) >= 86400. {
                *last = Some(total);
                cx.candle(symbol, *interval, total);
            }
        }
    }
    fn daily_update(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, row: Candle) {
        let day = (row.timestamp / 86400.).floor() * 86400.;
        if self.public.daily.get(symbol).is_some_and(|s| s.day > day) {
            return;
        }
        let new = self.public.daily.get(symbol).is_none_or(|s| s.day != day);
        if new {
            self.public.daily_generation = self.public.daily_generation.wrapping_add(1);
            self.public.daily.insert(
                symbol.into(),
                Daily {
                    generation: self.public.daily_generation,
                    day,
                    ..Daily::default()
                },
            );
        }
        if row.timestamp.fract() != 0. || row.timestamp % 300. != 0. {
            return;
        }
        let entry = self
            .public
            .daily
            .get_mut(symbol)
            .unwrap()
            .rows
            .entry(row.timestamp as u64)
            .or_insert(row);
        if row.volume >= entry.volume {
            *entry = row;
        }
        if new {
            self.daily_seed(cx, symbol, day);
        }
        self.emit_daily(cx, symbol);
    }
    pub(crate) fn public_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &mut Request,
        item: &str,
    ) {
        match pending {
            Request::Instruments {
                markets,
                tickers,
                invalid,
                ..
            } => match serde_json::from_str::<Instrument>(item) {
                Ok(row) => {
                    if row.kind != "PERP" || row.trading_state != "TRADING" {
                        return;
                    }
                    if row.base_increment <= 0. || row.quote_increment <= 0. {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    let symbol = format!("{}-INTX", row.symbol);
                    markets.push(market(
                        symbol.clone(),
                        row.base_asset_name,
                        row.quote_asset_name,
                        Precision {
                            amount: row.base_increment,
                            price: row.quote_increment,
                        },
                        if row.min_quantity > 0. {
                            row.min_quantity
                        } else if row.min_notional_value > 0. {
                            row.min_notional_value
                        } else {
                            0.001
                        },
                        1e9,
                        1.,
                        None,
                        None,
                    ));
                    if let Some(q) = row.quote {
                        if [
                            q.best_bid_price,
                            q.best_ask_price,
                            q.trade_price,
                            q.mark_price,
                            q.index_price,
                            row.qty_24hr,
                            row.notional_24hr,
                            row.open_interest,
                        ]
                        .iter()
                        .any(|n| *n < 0.)
                        {
                            *invalid = true;
                            safe_cex_engine::startup::skipped_row(cx);
                            return;
                        }
                        tickers.push(Ticker {
                            id: symbol.clone(),
                            symbol,
                            bid: q.best_bid_price,
                            ask: q.best_ask_price,
                            last: q.trade_price,
                            mark: q.mark_price,
                            index: q.index_price,
                            percentage: self
                                .public
                                .products
                                .get(&format!("{}-INTX", row.symbol))
                                .map_or(0., |v| v.0),
                            volume: row.qty_24hr,
                            quote_volume: row.notional_24hr,
                            open_interest: row.open_interest,
                            funding_rate: q.predicted_funding,
                            funding_time: self
                                .public
                                .products
                                .get(&format!("{}-INTX", row.symbol))
                                .map_or(f64::NAN, |v| v.1 + row.funding_interval / 1e6),
                            ..Ticker::default()
                        });
                    }
                }
                Err(_) => {
                    *invalid = true;
                    safe_cex_engine::startup::skipped_row(cx);
                }
            },
            Request::Products {
                markets,
                tickers,
                invalid,
                kind,
                ..
            } => match serde_json::from_str::<Product>(item) {
                Ok(row) => {
                    let future = row.future_product_details.is_some();
                    if (row.product_type == "FUTURE" && !future)
                        || (row.product_type == "SPOT" && future)
                    {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    let spot = row.product_type == "SPOT" && !future;
                    let expiring = row
                        .future_product_details
                        .as_ref()
                        .is_some_and(|f| f.contract_expiry_type == "EXPIRING");
                    if !matches!(row.product_type.as_str(), "SPOT" | "FUTURE") {
                        return;
                    }
                    if (*kind == ProductKind::Spot && !spot)
                        || (*kind == ProductKind::Cfm && !expiring)
                        || (*kind == ProductKind::Intx
                            && row
                                .future_product_details
                                .as_ref()
                                .is_none_or(|f| f.contract_expiry_type != "PERPETUAL"))
                    {
                        return;
                    }
                    let contract = row.future_product_details.as_ref().map_or(Some(1.), |f| {
                        if f.contract_size.is_empty() {
                            if expiring { None } else { Some(1.) }
                        } else {
                            f.contract_size.parse::<f64>().ok()
                        }
                    });
                    let Some(contract) = contract.filter(|n| n.is_finite() && *n > 0.) else {
                        if !expiring {
                            *invalid = true;
                            safe_cex_engine::startup::skipped_row(cx);
                        }
                        return;
                    };
                    if row.base_increment <= 0.
                        || row.price_increment <= 0.
                        || row.base_min_size < 0.
                        || row.base_max_size < row.base_min_size
                    {
                        *invalid = true;
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    let base = row
                        .future_product_details
                        .as_ref()
                        .filter(|f| !f.contract_root_unit.is_empty())
                        .map_or_else(
                            || row.base_currency_id.clone(),
                            |f| f.contract_root_unit.clone(),
                        );
                    let mut m = market(
                        row.product_id.clone(),
                        base,
                        row.quote_currency_id,
                        Precision {
                            amount: row.base_increment,
                            price: row.price_increment,
                        },
                        row.base_min_size,
                        row.base_max_size,
                        contract,
                        Some(spot),
                        Some(expiring),
                    );
                    m.active = matches!(row.status.as_str(), "TRADING" | "online")
                        && !row.is_disabled
                        && !row.trading_disabled;
                    markets.push(m);
                    let price = safe_cex_engine::num::parse(&row.price);
                    let volume = safe_cex_engine::num::parse(&row.volume_24h);
                    let percentage = safe_cex_engine::num::parse(
                        row.price_percentage_change_24h
                            .strip_suffix('%')
                            .unwrap_or(&row.price_percentage_change_24h),
                    );
                    let p = row
                        .future_product_details
                        .as_ref()
                        .and_then(|f| f.perpetual_details.as_ref());
                    tickers.push(Ticker {
                        id: row.product_id.clone(),
                        symbol: row.product_id,
                        bid: price,
                        ask: price,
                        last: price,
                        mark: price,
                        index: price,
                        percentage,
                        volume,
                        quote_volume: display_round(price * volume * contract),
                        funding_time: if *kind == ProductKind::Intx {
                            p.and_then(|p| timestamp(&p.funding_time))
                                .map_or(f64::NAN, |n| n * 1000.)
                        } else {
                            0.
                        },
                        open_interest: p.map_or(f64::NAN, |p| {
                            safe_cex_engine::num::parse(&p.open_interest) * contract
                        }),
                        ..Ticker::default()
                    });
                }
                Err(_) => {
                    *invalid = true;
                    safe_cex_engine::startup::skipped_row(cx);
                }
            },
            Request::Candles { rows, invalid, .. } | Request::Daily { rows, invalid, .. } => {
                match serde_json::from_str::<CandleRow>(item)
                    .ok()
                    .and_then(candle)
                {
                    Some(row) if rows.len() < 300 => rows.push(row),
                    _ => *invalid = true,
                }
            }
        }
    }
    pub(crate) fn public_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            // INTX can return a valid root array with an error status.
            if matches!(pending, Request::Instruments { .. })
                && response.body.trim_start().starts_with('[')
            {
                let mut pending = pending;
                let parsed = safe_cex_engine::json::each::<&serde_json::value::RawValue>(
                    response.body,
                    |r| self.public_item(cx, &mut pending, r.get()),
                );
                if parsed.is_ok() {
                    self.public_response(
                        cx,
                        pending,
                        Response {
                            status: 200,
                            headers: "",
                            body: "[]",
                        },
                    );
                    return;
                }
                self.public_failed(cx, pending, "Invalid INTX response");
                return;
            }
            self.public_failed(cx, pending, &format!("Coinbase HTTP {}", response.status));
            return;
        }
        match pending {
            Request::Instruments {
                markets,
                tickers,
                invalid,
                initial,
            } => {
                if markets.is_empty() && (initial || invalid) {
                    self.public_failed(
                        cx,
                        Request::Instruments {
                            markets: vec![],
                            tickers: vec![],
                            invalid: true,
                            initial,
                        },
                        "Malformed Coinbase instruments",
                    );
                    return;
                }
                let missing_quotes = invalid && tickers.is_empty();
                if !missing_quotes {
                    self.public.retry.succeeded(cx, &None);
                }
                if initial {
                    cx.store.set_markets(markets);
                    if !missing_quotes {
                        cx.store.set_tickers(tickers);
                    }
                    self.public_pending(cx);
                    self.connect_public(cx);
                    if self.options.public_data_only || self.options.key.is_empty() {
                        cx.store.set_orders(vec![]);
                        cx.store.set_positions(vec![]);
                        cx.store.set_balance(Default::default());
                    }
                } else if !cx.store.loaded().tickers && !tickers.is_empty() {
                    cx.store.set_tickers(tickers);
                } else {
                    for ticker in tickers {
                        cx.store
                            .update_ticker(&ticker.symbol.clone(), |old| *old = ticker);
                    }
                }
                if missing_quotes {
                    self.public_failed(
                        cx,
                        Request::Instruments {
                            markets: vec![],
                            tickers: vec![],
                            invalid: true,
                            initial: false,
                        },
                        "No valid Coinbase instrument quote rows",
                    );
                    return;
                }
                if self.public.poll.is_none() {
                    self.public.poll = Some(cx.set_timer(1000.));
                }
            }
            Request::Products {
                kind,
                generation,
                mut cursors,
                markets,
                tickers,
                invalid,
            } => {
                if generation != self.public.generation {
                    return;
                }
                #[derive(serde::Deserialize)]
                struct Envelope {
                    pagination: Option<Pagination>,
                }
                #[derive(serde::Deserialize)]
                struct Pagination {
                    has_next: bool,
                    next_cursor: Option<String>,
                }
                let envelope = match serde_json::from_str::<Envelope>(response.body) {
                    Ok(e) => e,
                    Err(_) => {
                        self.public_failed(
                            cx,
                            Request::Products {
                                kind,
                                generation,
                                cursors,
                                markets,
                                tickers,
                                invalid: true,
                            },
                            "Invalid Coinbase products pagination",
                        );
                        return;
                    }
                };
                if let Some(page) = envelope.pagination.filter(|p| p.has_next) {
                    let next = page
                        .next_cursor
                        .filter(|c| !c.is_empty() && c.len() <= 2048);
                    let Some(next) = next.filter(|c| !cursors.contains(c) && cursors.len() < 50)
                    else {
                        self.public_failed(
                            cx,
                            Request::Products {
                                kind,
                                generation,
                                cursors,
                                markets,
                                tickers,
                                invalid: true,
                            },
                            "Invalid Coinbase products cursor",
                        );
                        return;
                    };
                    cursors.push(next.clone());
                    self.products_page(
                        cx,
                        Request::Products {
                            kind,
                            generation,
                            cursors,
                            markets,
                            tickers,
                            invalid,
                        },
                        Some(&next),
                    );
                    return;
                }
                if invalid && markets.is_empty() {
                    self.public_failed(
                        cx,
                        Request::Products {
                            kind,
                            generation,
                            cursors: vec![],
                            markets: vec![],
                            tickers: vec![],
                            invalid: true,
                        },
                        "Malformed Coinbase products",
                    );
                    return;
                }
                self.public.retry.succeeded(cx, &Some(kind));
                for ticker in &tickers {
                    self.public.products.insert(
                        ticker.symbol.clone(),
                        (ticker.percentage, ticker.funding_time),
                    );
                }
                if self.public.seeding {
                    self.public.seed_markets.extend(markets);
                    self.public.seed_tickers.extend(tickers);
                    self.public.seeds_left = self.public.seeds_left.saturating_sub(1);
                    if self.public.seeds_left == 0 {
                        if self.public.seed_markets.is_empty() {
                            self.public_failed(
                                cx,
                                Request::Products {
                                    kind,
                                    generation,
                                    cursors: vec![],
                                    markets: vec![],
                                    tickers: vec![],
                                    invalid: true,
                                },
                                "Coinbase catalogue contains no supported markets",
                            );
                            return;
                        }
                        self.public.seeding = false;
                        cx.store
                            .set_markets(std::mem::take(&mut self.public.seed_markets));
                        cx.store
                            .set_tickers(std::mem::take(&mut self.public.seed_tickers));
                        self.public_pending(cx);
                        self.connect_public(cx);
                        if self.options.public_data_only || self.options.key.is_empty() {
                            cx.store.set_orders(vec![]);
                            cx.store.set_positions(vec![]);
                            cx.store.set_balance(Default::default());
                        }
                        self.private_after_markets(cx);
                        if self.public.kind == Some(ProductKind::Intx) {
                            self.instruments(cx, false);
                        }
                    }
                } else {
                    for ticker in tickers {
                        cx.store
                            .update_ticker(&ticker.symbol.clone(), |old| *old = ticker);
                    }
                }
                if !self.public.seeding && self.public.poll.is_none() {
                    self.public.poll = Some(cx.set_timer(self.public_poll_interval()));
                }
            }
            Request::Daily {
                symbol,
                generation,
                day,
                rows,
                invalid,
            } => {
                if invalid {
                    self.public_failed(
                        cx,
                        Request::Daily {
                            symbol,
                            generation,
                            day,
                            rows: vec![],
                            invalid: true,
                        },
                        "Malformed Coinbase daily candle seed",
                    );
                    return;
                }
                let Some(state) = self
                    .public
                    .daily
                    .get_mut(&symbol)
                    .filter(|s| s.generation == generation && s.day == day)
                else {
                    return;
                };
                for row in rows {
                    if row.timestamp < day
                        || row.timestamp >= day + 86400.
                        || row.timestamp % 300. != 0.
                    {
                        continue;
                    }
                    // Seed and websocket overlap; cumulative volume orders their revisions.
                    let entry = state.rows.entry(row.timestamp as u64).or_insert(row);
                    if row.volume >= entry.volume {
                        *entry = row;
                    }
                }
                state.seeded = true;
                if let Some(timer) = state.retry.take() {
                    cx.clear_timer(timer);
                }
                self.emit_daily(cx, &symbol);
            }
            Request::Candles {
                command,
                mut rows,
                invalid,
            } => {
                if invalid {
                    cx.fail(command, "Malformed Coinbase candles");
                } else {
                    rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                    match serde_json::to_string(&rows).map_err(|e| e.to_string()) {
                        Ok(data) => cx.done(command, data),
                        Err(error) => cx.fail(command, error),
                    };
                }
            }
        }
    }
    pub(crate) fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        error: &str,
    ) {
        if let Request::Products { generation, .. } = &pending
            && *generation != self.public.generation
        {
            return;
        }
        let mut failures = vec![];
        match pending {
            Request::Candles { command, .. } => {
                failures.push((command, safe_cex_engine::VenueError::new(error)));
            }
            Request::Daily {
                symbol, generation, ..
            } => {
                cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                if let Some(state) = self
                    .public
                    .daily
                    .get_mut(&symbol)
                    .filter(|s| s.generation == generation && s.retry.is_none())
                {
                    state.retry = Some(cx.set_timer(2000.));
                }
            }
            pending => {
                for deferred in std::mem::take(&mut self.public.deferred) {
                    if let Command::FetchCandles { command, .. } = deferred {
                        failures.push((
                            command,
                            safe_cex_engine::VenueError::new(
                                "Coinbase public catalogue unavailable",
                            )
                            .kind(safe_cex_engine::ErrorKind::Internal)
                            .venue_message(error),
                        ));
                    } else {
                        self.public.deferred.push(deferred);
                    }
                }
                let key = match pending {
                    Request::Products { kind, .. } => Some(kind),
                    _ => None,
                };
                cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                if let Some(timer) = self.public.poll.take() {
                    cx.clear_timer(timer);
                }
                self.public.retry.failed(cx, key);
            }
        }
        for (command, error) in failures {
            cx.fail(command, error);
        }
    }
    pub(crate) fn public_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(symbol) = self
            .public
            .daily
            .iter()
            .find_map(|(s, d)| (d.retry == Some(timer)).then(|| s.clone()))
        {
            let state = self.public.daily.get_mut(&symbol).unwrap();
            state.retry = None;
            let day = state.day;
            self.daily_seed(cx, &symbol, day);
            return true;
        }
        if self.public.supervisor.owns_timer(timer) {
            match self.public.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.connect_public(cx),
                SocketTick::TimedOut(_) => self.reconnect_public(cx),
                SocketTick::Handled
                    if self.public.open
                        && self
                            .public
                            .books
                            .values()
                            .any(|b| !b.live && cx.now - b.waiting_since >= 10_000.) =>
                {
                    self.reconnect_public(cx)
                }
                _ => {}
            }
            return true;
        }
        let retry = self.public.retry.on_timer(timer).is_some();
        if self.public.poll == Some(timer) || retry {
            if self.public.poll == Some(timer) {
                self.public.poll = None;
                // Initial private reconciliation scans every working status and
                // account family through the same paced REST queue.
                if self.credentials.is_some() && !cx.store.loaded().orders {
                    self.public.poll = Some(cx.set_timer(self.public_poll_interval()));
                    return true;
                }
            }
            match self.public.kind.unwrap_or(ProductKind::Intx) {
                ProductKind::Intx => {
                    if self.public.seeding {
                        self.private_ready(ProductKind::Intx, cx);
                    } else {
                        self.instruments(cx, cx.store.markets().is_empty());
                    }
                }
                kind => {
                    if self.public.seeding {
                        self.private_ready(kind, cx);
                    } else {
                        self.request_products(cx, kind);
                        if kind == ProductKind::Cfm {
                            self.request_products(cx, ProductKind::Spot);
                        }
                    }
                }
            }
            return true;
        }
        false
    }
    pub(crate) fn public_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        let Ok(envelope) = serde_json::from_str::<Envelope<'_>>(text) else {
            return true;
        };
        if envelope.kind == "error" {
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                envelope.message.as_ref(),
            );
            self.reconnect_public(cx);
            return true;
        }
        if matches!(
            envelope.channel.as_ref(),
            "subscriptions" | "heartbeats" | "l2_data" | "candles" | "market_trades"
        ) {
            if let Some(seq) = envelope.sequence_num {
                if let Some(last) = self.public.sequence {
                    if seq <= last {
                        return true;
                    }
                    if seq != last.saturating_add(1) {
                        self.reconnect_public(cx);
                        return true;
                    }
                }
                self.public.sequence = Some(seq);
            } else if envelope.channel == "l2_data" {
                self.reconnect_public(cx);
                return true;
            }
        }
        match envelope.channel.as_ref() {
            "l2_data" => {
                let Some(events) = envelope.events else {
                    self.reconnect_public(cx);
                    return true;
                };
                let Ok(events) = serde_json::from_str::<Vec<DepthEvent<'_>>>(events.get()) else {
                    self.reconnect_public(cx);
                    return true;
                };
                if events.iter().any(|e| {
                    !matches!(e.kind.as_ref(), "snapshot" | "update")
                        || e.updates.iter().any(|l| {
                            !matches!(l.side.as_ref(), "bid" | "offer")
                                || l.price_level <= 0.
                                || l.new_quantity < 0.
                        })
                }) {
                    self.reconnect_public(cx);
                    return true;
                }
                self.public.supervisor.received(cx.now);
                for event in events {
                    for (symbol, state) in &mut self.public.books {
                        if ws_symbol(symbol) != event.product_id {
                            continue;
                        }
                        if event.kind == "snapshot" {
                            state.book.clear();
                            state.live = true;
                        }
                        if !state.live {
                            continue;
                        }
                        for update in &event.updates {
                            state.book.apply(
                                if update.side == "bid" {
                                    BookSide::Bids
                                } else {
                                    BookSide::Asks
                                },
                                update.price_level,
                                update.new_quantity,
                            );
                        }
                        cx.book_changed(symbol);
                    }
                }
            }
            "candles" => {
                let Some(events) = envelope.events else {
                    return true;
                };
                let Ok(events) = serde_json::from_str::<Vec<CandleEvent>>(events.get()) else {
                    return true;
                };
                self.public.supervisor.received(cx.now);
                for event in events {
                    if event.kind != "update" {
                        continue;
                    }
                    for row in event.candles {
                        let product = row.product_id.clone();
                        let Some(c) = candle(row) else {
                            continue;
                        };
                        let keys: Vec<_> = self
                            .public
                            .candles
                            .keys()
                            .filter(|(s, _)| ws_symbol(s) == ws_symbol(&product))
                            .cloned()
                            .collect();
                        for (symbol, interval) in keys {
                            let seconds = interval_seconds(interval);
                            if seconds < 300. {
                                continue;
                            }
                            if seconds >= 86400. {
                                self.daily_update(cx, &symbol, c);
                            } else {
                                self.public
                                    .candles
                                    .get_mut(&(symbol.clone(), interval))
                                    .unwrap()
                                    .1 = Some(c);
                                cx.candle(&symbol, interval, c);
                            }
                        }
                    }
                }
            }
            "market_trades" => {
                let Some(events) = envelope.events else {
                    return true;
                };
                let Ok(events) = serde_json::from_str::<Vec<TradeEvent>>(events.get()) else {
                    return true;
                };
                self.public.supervisor.received(cx.now);
                for event in events {
                    if event.kind != "update" {
                        continue;
                    }
                    for trade in event.trades {
                        let Some(time) = timestamp(&trade.time) else {
                            continue;
                        };
                        if trade.price <= 0. || trade.size < 0. {
                            continue;
                        }
                        for ((symbol, interval), (count, last)) in &mut self.public.candles {
                            let seconds = interval_seconds(*interval);
                            if *count == 0
                                || seconds == 0.
                                || seconds >= 86400.
                                || ws_symbol(symbol) != ws_symbol(&trade.product_id)
                            {
                                continue;
                            }
                            if seconds >= 300. {
                                let Some(old) = *last else {
                                    continue;
                                };
                                if time < old.timestamp {
                                    continue;
                                }
                                let next = Candle {
                                    high: old.high.max(trade.price),
                                    low: old.low.min(trade.price),
                                    close: trade.price,
                                    ..old
                                };
                                *last = Some(next);
                                cx.candle(symbol, *interval, next);
                                continue;
                            }
                            let bucket = (time / seconds).floor() * seconds;
                            let next = match *last {
                                Some(old) if old.timestamp == bucket => Candle {
                                    high: old.high.max(trade.price),
                                    low: old.low.min(trade.price),
                                    close: trade.price,
                                    volume: old.volume + trade.size,
                                    ..old
                                },
                                Some(old) if old.timestamp > bucket => continue,
                                _ => Candle {
                                    timestamp: bucket,
                                    open: trade.price,
                                    high: trade.price,
                                    low: trade.price,
                                    close: trade.price,
                                    volume: trade.size,
                                },
                            };
                            *last = Some(next);
                            cx.candle(symbol, *interval, next);
                        }
                    }
                }
            }
            "heartbeats" => {
                #[derive(serde::Deserialize)]
                struct Heartbeat<'a> {
                    #[serde(borrow)]
                    heartbeat_counter: std::borrow::Cow<'a, str>,
                }
                if envelope
                    .events
                    .and_then(|e| serde_json::from_str::<Vec<Heartbeat<'_>>>(e.get()).ok())
                    .is_some_and(|events| {
                        !events.is_empty() && events.iter().all(|e| !e.heartbeat_counter.is_empty())
                    })
                {
                    self.public.supervisor.received(cx.now);
                }
            }
            _ => {}
        }
        true
    }
    pub(crate) fn public_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        if !cx.store.loaded().markets
            && matches!(
                &command,
                Command::SubscribeOrderBook { .. }
                    | Command::UnsubscribeOrderBook { .. }
                    | Command::SubscribeCandles { .. }
                    | Command::UnsubscribeCandles { .. }
                    | Command::FetchCandles { .. }
            )
        {
            if self.public.deferred.len() >= 256 {
                if let Command::FetchCandles { .. } = command {
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
                return None;
            }
            self.public.deferred.push(command);
            return None;
        }
        match command {
            Command::SubscribeOrderBook { symbol } => {
                cx.store.market(&symbol)?;
                let first = !self
                    .public
                    .books
                    .keys()
                    .any(|s| ws_symbol(s) == ws_symbol(&symbol));
                let native = self
                    .public
                    .books
                    .iter()
                    .find(|(s, b)| b.live && ws_symbol(s) == ws_symbol(&symbol))
                    .map(|(_, b)| b.book.clone());
                let state = self.public.books.entry(symbol.clone()).or_default();
                if !state.live
                    && let Some(book) = native
                {
                    state.book = book;
                    state.live = true;
                    cx.book_changed(&symbol);
                }
                state.subscribers += 1;
                state.waiting_since = cx.now;
                if first {
                    self.subscription(cx, "level2", vec![ws_symbol(&symbol)], true);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.public.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.public.books.remove(&symbol);
                        if !self
                            .public
                            .books
                            .keys()
                            .any(|s| ws_symbol(s) == ws_symbol(&symbol))
                        {
                            self.subscription(cx, "level2", vec![ws_symbol(&symbol)], false);
                        }
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                cx.store.market(&symbol)?;
                let first = !self
                    .public
                    .candles
                    .keys()
                    .any(|(s, _)| ws_symbol(s) == ws_symbol(&symbol));
                let daily = interval_seconds(interval) >= 86400.;
                self.public
                    .candles
                    .entry((symbol.clone(), interval))
                    .or_insert((0, None))
                    .0 += 1;
                if daily && !self.public.daily.contains_key(&symbol) {
                    self.public.daily_generation = self.public.daily_generation.wrapping_add(1);
                    let day = (cx.now / 86400000.).floor() * 86400.;
                    self.public.daily.insert(
                        symbol.clone(),
                        Daily {
                            generation: self.public.daily_generation,
                            day,
                            refresh: !self.public.open,
                            ..Daily::default()
                        },
                    );
                    if self.public.open {
                        self.daily_seed(cx, &symbol, day);
                    }
                }
                if first {
                    self.subscription(cx, "candles", vec![ws_symbol(&symbol)], true);
                    self.subscription(cx, "market_trades", vec![ws_symbol(&symbol)], true);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval);
                if let Some((count, _)) = self.public.candles.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.public.candles.remove(&key);
                        if interval_seconds(key.1) >= 86400.
                            && !self.public.candles.keys().any(|(market, interval)| {
                                market == &symbol && interval_seconds(*interval) >= 86400.
                            })
                            && let Some(state) = self.public.daily.remove(&symbol)
                            && let Some(timer) = state.retry
                        {
                            cx.clear_timer(timer);
                        }
                    }
                }
                if !self
                    .public
                    .candles
                    .keys()
                    .any(|(s, _)| ws_symbol(s) == ws_symbol(&symbol))
                {
                    self.subscription(cx, "candles", vec![ws_symbol(&symbol)], false);
                    self.subscription(cx, "market_trades", vec![ws_symbol(&symbol)], false);
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: timeframe,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some((granularity, seconds)) = interval(timeframe) else {
                    cx.fail(command, timeframe.unsupported("Coinbase"));
                    return None;
                };
                if cx.store.market(&symbol).is_none() {
                    cx.fail(command, "Unknown Coinbase market");
                    return None;
                }
                let end = to_ms.unwrap_or(cx.now) / 1000.;
                let start = from_ms
                    .map(|n| n / 1000.)
                    .unwrap_or(end - seconds * f64::from(limit.unwrap_or(300).clamp(1, 300)));
                if !start.is_finite() || !end.is_finite() || start < 0. || end <= start {
                    cx.fail(command, "Invalid Coinbase candle range");
                    return None;
                }
                let path = format!(
                    "/api/v3/brokerage/market/products/{}/candles?granularity={granularity}&start={:.0}&end={:.0}&limit={}",
                    ws_symbol(&symbol),
                    start,
                    end,
                    limit.unwrap_or(300).clamp(1, 300)
                );
                if let Err(error) = self.request(
                    cx,
                    Method::Get,
                    path,
                    None,
                    Some(ArrayAt::Field("candles")),
                    Pending::Public(Request::Candles {
                        command,
                        rows: vec![],
                        invalid: false,
                    }),
                ) {
                    cx.fail(command, error);
                }
            }
            Command::SetTickerSymbols { .. } => {}
            other => return Some(other),
        }
        None
    }
}
// Live candles are built from the ticker, so every interval is served.
fn interval_seconds(i: Interval) -> f64 {
    i.ms() / 1000.
}

fn display_round(value: f64) -> f64 {
    let abs = value.abs();
    let digits = if abs == 0. {
        1
    } else if abs >= 1000. {
        2
    } else if abs >= 10. {
        3
    } else if abs >= 1. {
        4
    } else if abs >= 0.0001 {
        5
    } else if abs >= 0.00001 {
        6
    } else if abs >= 0.000001 {
        7
    } else {
        8
    };
    let scale = 10f64.powi(digits);
    (value * scale).round() / scale
}
