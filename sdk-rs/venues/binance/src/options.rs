//! European Options codecs. Native quantities remain contracts, never futures lots.
use crate::{BinanceFutures, Channel, Pending, Query, Variant, reads};
use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason};
use safe_cex_engine::model::{
    Balance, CurrencyBalance, DerivativeProduct, Limits, Market, MinMax, Order, OrderExtra,
    OrderSide, OrderStatus, OrderType, Position, PositionSide, Precision, ProductKind, Ticker,
    TimeInForce,
};
use safe_cex_engine::request_context::{Context, Scope};
use safe_cex_engine::store::OrderSnapshotToken;
use safe_cex_engine::{Cx, ErrorKind, Level, Response, TimerId, VenueError};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Markets,
    Tickers,
    Marks,
    Balance,
    Positions,
    Orders,
    Fills,
    History,
}

pub struct Request {
    context: Context,
    kind: Kind,
    token: Option<OrderSnapshotToken>,
    read_epoch: u64,
    query: Option<safe_cex_engine::Query>,
}

#[derive(Default)]
pub(crate) struct State {
    busy: HashSet<Kind>,
    poll: Option<TimerId>,
    started: bool,
    orders_owed: bool,
    marks: HashMap<String, f64>,
}

fn error(message: impl Into<String>) -> VenueError {
    VenueError::new(message).kind(ErrorKind::InvalidInput)
}
fn finite(text: &str) -> Result<f64, VenueError> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| error("Malformed Binance option number"))
}
fn positive(text: &str) -> Result<f64, VenueError> {
    finite(text).and_then(|v| {
        if v > 0. {
            Ok(v)
        } else {
            Err(error("Binance option number must be positive"))
        }
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalogue {
    option_contracts: Vec<Contract>,
    option_symbols: Vec<Symbol>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    underlying: String,
    base_asset: String,
    settle_asset: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Symbol {
    symbol: String,
    underlying: String,
    quote_asset: String,
    status: String,
    side: String,
    strike_price: String,
    expiry_date: f64,
    filters: Vec<Filter>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Filter {
    filter_type: String,
    #[serde(default)]
    tick_size: String,
    #[serde(default)]
    step_size: String,
    #[serde(default)]
    min_qty: String,
    #[serde(default)]
    max_qty: String,
    #[serde(default)]
    min_price: Option<String>,
}

fn catalogue(body: &str) -> Result<(Vec<Market>, Vec<DerivativeProduct>), VenueError> {
    let raw: Catalogue =
        serde_json::from_str(body).map_err(|_| error("Malformed Binance option catalogue"))?;
    let mut markets = Vec::new();
    let mut products = Vec::new();
    let mut seen = HashSet::new();
    for row in raw.option_symbols {
        if row.status != "TRADING" {
            continue;
        }
        let contract = raw
            .option_contracts
            .iter()
            .find(|c| c.underlying == row.underlying)
            .ok_or_else(|| error("Unknown option underlying"))?;
        let kind = match row.side.as_str() {
            "CALL" => ProductKind::Call,
            "PUT" => ProductKind::Put,
            _ => return Err(error("Unknown option right")),
        };
        let lot = row
            .filters
            .iter()
            .find(|f| f.filter_type == "LOT_SIZE")
            .ok_or_else(|| error("Missing option lot size"))?;
        let price = row
            .filters
            .iter()
            .find(|f| f.filter_type == "PRICE_FILTER")
            .ok_or_else(|| error("Missing option price filter"))?;
        if row.symbol.is_empty()
            || !seen.insert(row.symbol.clone())
            || contract.base_asset.is_empty()
            || contract.settle_asset.is_empty()
            || row.quote_asset.is_empty()
            || !row.expiry_date.is_finite()
            || row.expiry_date <= 0.
        {
            return Err(error("Invalid option market identity"));
        }
        let amount = positive(&lot.step_size)?;
        let minimum = positive(&lot.min_qty)?;
        let maximum = positive(&lot.max_qty)?;
        if minimum > maximum {
            return Err(error("Invalid option quantity bounds"));
        }
        products.push(DerivativeProduct {
            market: markets.len() as u32,
            kind,
            underlying: contract.base_asset.clone(),
            expiry: Some(row.expiry_date),
            strike: Some(positive(&row.strike_price)?),
            settlement_asset: Some(contract.settle_asset.clone()),
            premium_asset: Some(row.quote_asset.clone()),
            style: Some("european".into()),
            settlement: Some("cash".into()),
        });
        markets.push(Market {
            id: row.symbol.clone(),
            symbol: row.symbol,
            pretty_symbol: None,
            base: contract.base_asset.clone(),
            quote: row.quote_asset,
            active: true,
            precision: Precision {
                amount,
                price: positive(&price.tick_size)?,
            },
            limits: Limits {
                amount: MinMax {
                    min: minimum,
                    max: maximum,
                },
                leverage: MinMax { min: 1., max: 1. },
                price: price
                    .min_price
                    .as_deref()
                    .map(finite)
                    .transpose()?
                    .map(|min| safe_cex_engine::model::Min { min }),
            },
            contract_size: Some(1.),
            is_expiring: Some(true),
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        });
    }
    Ok((markets, products))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Daily {
    symbol: String,
    last_price: String,
    price_change_percent: String,
    volume: String,
    amount: String,
    bid_price: String,
    ask_price: String,
    exercise_price: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mark {
    symbol: String,
    mark_price: String,
}
#[derive(Deserialize)]
struct Margin {
    asset: Vec<Asset>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Asset {
    asset: String,
    equity: String,
    available: String,
    initial_margin: String,
    #[serde(rename = "unrealizedPNL")]
    upnl: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionRow {
    symbol: String,
    side: String,
    quantity: String,
    entry_price: String,
    mark_value: String,
    #[serde(rename = "unrealizedPNL")]
    upnl: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderRow {
    #[serde(deserialize_with = "native_id")]
    pub order_id: u64,
    pub symbol: String,
    pub side: String,
    pub status: String,
    pub quantity: String,
    pub executed_qty: String,
    pub price: String,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(default)]
    pub post_only: bool,
    #[serde(default)]
    pub client_order_id: Option<String>,
    #[serde(default)]
    pub time_in_force: Option<String>,
}
impl OrderRow {
    fn order(self, now: f64) -> Result<Order, VenueError> {
        let side = match self.side.as_str() {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => return Err(error("Invalid option order side")),
        };
        let status = match self.status.as_str() {
            "NEW" | "ACCEPTED" | "PARTIALLY_FILLED" => OrderStatus::Open,
            "FILLED" => OrderStatus::Closed,
            "CANCELLED" | "CANCELED" | "REJECTED" | "EXPIRED" => OrderStatus::Canceled,
            _ => return Err(error("Unknown option order status")),
        };
        let amount = positive(&self.quantity)?;
        let filled = finite(&self.executed_qty)?;
        if self.order_id == 0 || self.symbol.is_empty() || filled < 0. || filled > amount {
            return Err(error("Invalid option order quantities"));
        }
        let time_in_force = if self.post_only {
            Some(TimeInForce::PostOnly)
        } else {
            match self.time_in_force.as_deref() {
                Some("GTC") => Some(TimeInForce::GoodTillCancel),
                Some("IOC") => Some(TimeInForce::ImmediateOrCancel),
                Some("FOK") => Some(TimeInForce::FillOrKill),
                None => None,
                _ => return Err(error("Invalid option time in force")),
            }
        };
        Ok(Order {
            id: self.order_id.to_string(),
            parent_id: None,
            symbol: self.symbol,
            status,
            kind: OrderType::Limit,
            side,
            price: positive(&self.price)?,
            amount,
            filled,
            remaining: amount - filled,
            reduce_only: self.reduce_only,
            close: None,
            client_order_id: self.client_order_id,
            time_in_force,
            last_update: now,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra::default()),
        })
    }
}

impl BinanceFutures {
    pub(crate) fn is_options(&self) -> bool {
        self.options.variant == Variant::Options
    }

    pub(crate) fn options_read(&mut self, cx: &mut Cx<'_, Pending>, kind: Kind) {
        if self.option_state.busy.contains(&kind) {
            return;
        }
        let private = matches!(kind, Kind::Balance | Kind::Positions | Kind::Orders);
        let selected = if private {
            let role = if kind == Kind::Orders {
                CredentialRole::PrimaryRead {
                    reason: PrimaryReason::PostUnknownOrderRead,
                    scope: None,
                }
            } else {
                CredentialRole::Read { scope: None }
            };
            match self.credentials.select(role) {
                Ok((selection, id)) => {
                    Some((selection.credential(id.credential.clone()), id.clone()))
                }
                Err(_) => return,
            }
        } else {
            None
        };
        let token = if kind == Kind::Orders {
            match cx.store.begin_orders_snapshot() {
                Some(token) => Some(token),
                None => {
                    cx.notice(
                        Level::Error,
                        "SNAPSHOT_EXHAUSTED",
                        "Binance Options order snapshot stamps exhausted",
                    );
                    return;
                }
            }
        } else {
            None
        };
        let Ok(context) = cx.http_context(
            Scope::Background,
            reads::policy(selected.as_ref().map(|(c, _)| c.clone())),
        ) else {
            return;
        };
        let path = match kind {
            Kind::Markets => self.paths.exchange_info,
            Kind::Tickers => self.paths.daily,
            Kind::Marks => self.paths.premium_index,
            Kind::Balance => self.paths.balance,
            Kind::Positions => self.paths.position_risk,
            Kind::Orders => self.paths.open_orders,
            Kind::Fills | Kind::History => unreachable!(),
        };
        self.option_state.busy.insert(kind);
        if kind == Kind::Orders {
            self.option_state.orders_owed = false;
        }
        let request = Request {
            context: context.clone(),
            kind,
            token,
            read_epoch: cx.order_read_epoch(),
            query: None,
        };
        self.managed_read(
            cx,
            context,
            reads::Read {
                url: format!("{}{path}", self.urls.rest),
                credentials: selected.map(|(_, id)| id),
                query: Query::new(),
                stream: None,
            },
            Pending::Options(Box::new(request)),
        );
    }

    pub(crate) fn start_options(&mut self, cx: &mut Cx<'_, Pending>) {
        self.options_read(cx, Kind::Markets);
        self.option_state.poll = Some(cx.set_poll_timer(5_000.));
    }

    pub(crate) fn options_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.option_state.busy.contains(&Kind::Orders) {
            self.option_state.orders_owed |= !cx.reconcile_reask();
        } else {
            self.options_read(cx, Kind::Orders);
        }
    }

    pub(crate) fn options_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.option_state.poll != Some(timer) {
            return;
        }
        self.option_state.poll = None;
        if cx.now.is_finite() {
            if !cx.store.loaded().markets {
                self.options_read(cx, Kind::Markets);
            } else {
                self.options_refresh(cx);
            }
            self.option_state.poll = Some(cx.set_poll_timer(5_000.));
        }
    }

    pub(crate) fn options_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        for kind in [Kind::Tickers, Kind::Marks] {
            self.options_read(cx, kind);
        }
        if self.credentials.has_primary() {
            for kind in [Kind::Balance, Kind::Positions, Kind::Orders] {
                self.options_read(cx, kind);
            }
        }
    }

    pub(crate) fn options_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if request.query.is_some() {
            if cx
                .validate_read_http_delivery(&request.context, &request.context)
                .is_err()
            {
                let _ = cx.finish_http_context(&request.context);
                return;
            }
            let result = crate::account::venue_error(&response).map_or_else(
                || self.options_query_result(&request, response.body),
                |(_, message)| Err(error(message)),
            );
            match result {
                Ok(result) => {
                    let _ = cx.done_context(&request.context, result);
                }
                Err(error) => {
                    let _ = cx.fail_context(&request.context, error);
                }
            }
            return;
        }
        if cx
            .validate_background_read_http_delivery(&request.context)
            .is_err()
        {
            self.options_failed(cx, request, "Expired option read");
            return;
        }
        let result = crate::account::venue_error(&response).map_or_else(
            || self.publish_options(cx, &mut request, response.body),
            |(_, message)| Err(error(message)),
        );
        self.option_state.busy.remove(&request.kind);
        let _ = cx.finish_http_context(&request.context);
        match result {
            Err(error) => cx.notice(Level::Warn, "REQUEST_FAILED", error.message),
            Ok(()) if request.kind == Kind::Markets => {
                if !self.option_state.started {
                    self.option_state.started = true;
                    if self.credentials.has_primary() {
                        self.request_listen_key(cx);
                    }
                }
                self.options_refresh(cx);
                if self.public.idle() {
                    self.open_feed(cx, Channel::Public);
                }
            }
            _ => {}
        }
        if self.option_state.orders_owed {
            self.options_read(cx, Kind::Orders);
        }
    }

    pub(crate) fn options_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        message: &str,
    ) {
        if request.query.is_some() {
            let _ = cx.fail_context(
                &request.context,
                VenueError::new(message).kind(ErrorKind::Network),
            );
            return;
        }
        self.option_state.busy.remove(&request.kind);
        let _ = cx.finish_http_context(&request.context);
        if cx.now.is_finite() {
            cx.notice(Level::Warn, "REQUEST_FAILED", message);
        }
    }

    fn publish_options(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        body: &str,
    ) -> Result<(), VenueError> {
        match request.kind {
            Kind::Fills | Kind::History => {
                return Err(error("History delivered to a Background read"));
            }
            Kind::Markets => {
                let (markets, products) = catalogue(body)?;
                cx.store.set_markets(markets);
                cx.store.set_derivative_products(products);
                if !self.credentials.has_primary() {
                    cx.store.set_orders(Vec::new());
                    cx.store.set_positions(Vec::new());
                    cx.store.set_balance(Balance::default());
                }
            }
            Kind::Tickers => {
                let rows: Vec<Daily> =
                    serde_json::from_str(body).map_err(|_| error("Malformed option tickers"))?;
                let tickers = rows
                    .into_iter()
                    .filter(|r| cx.store.market(&r.symbol).is_some())
                    .map(|r| {
                        let last = finite(&r.last_price)?;
                        Ok(Ticker {
                            id: r.symbol.clone(),
                            symbol: r.symbol.clone(),
                            last,
                            bid: finite(&r.bid_price)?,
                            ask: finite(&r.ask_price)?,
                            mark: self
                                .option_state
                                .marks
                                .get(&r.symbol)
                                .copied()
                                .unwrap_or(last),
                            index: finite(&r.exercise_price)?,
                            percentage: finite(&r.price_change_percent)? * 100.,
                            volume: finite(&r.volume)?,
                            quote_volume: finite(&r.amount)?,
                            ..Ticker::default()
                        })
                    })
                    .collect::<Result<Vec<_>, VenueError>>()?;
                cx.store.set_tickers(tickers);
            }
            Kind::Marks => {
                let rows: Vec<Mark> =
                    serde_json::from_str(body).map_err(|_| error("Malformed option marks"))?;
                let marks = rows
                    .into_iter()
                    .map(|r| Ok((r.symbol, finite(&r.mark_price)?)))
                    .collect::<Result<HashMap<_, _>, VenueError>>()?;
                for (symbol, mark) in &marks {
                    cx.store.update_ticker(symbol, |ticker| ticker.mark = *mark);
                }
                self.option_state.marks = marks;
            }
            Kind::Balance => {
                let raw: Margin =
                    serde_json::from_str(body).map_err(|_| error("Malformed option account"))?;
                let mut total = Balance::default();
                let mut currencies = Vec::new();
                let mut seen = HashSet::new();
                for asset in raw.asset {
                    if asset.asset.is_empty() || !seen.insert(asset.asset.clone()) {
                        return Err(error("Invalid option balance asset"));
                    }
                    let balance = Balance {
                        total: finite(&asset.equity)?,
                        free: finite(&asset.available)?,
                        used: finite(&asset.initial_margin)?,
                        upnl: finite(&asset.upnl)?,
                    };
                    total.total += balance.total;
                    total.free += balance.free;
                    total.used += balance.used;
                    total.upnl += balance.upnl;
                    currencies.push(CurrencyBalance {
                        currency: asset.asset,
                        balance,
                    });
                }
                if ![total.total, total.free, total.used, total.upnl]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    return Err(error("Option balance overflow"));
                }
                cx.store.set_balance(total);
                cx.store.set_currency_balances(currencies);
            }
            Kind::Positions => {
                let rows: Vec<PositionRow> =
                    serde_json::from_str(body).map_err(|_| error("Malformed option positions"))?;
                let mut positions = Vec::new();
                for row in rows {
                    let quantity = finite(&row.quantity)?;
                    if quantity == 0. {
                        continue;
                    }
                    if cx.store.market(&row.symbol).is_none() {
                        return Err(error("Unknown option position symbol"));
                    }
                    let side = match row.side.as_str() {
                        "SHORT" => PositionSide::Short,
                        "LONG" => {
                            if quantity < 0. {
                                PositionSide::Short
                            } else {
                                PositionSide::Long
                            }
                        }
                        _ => return Err(error("Invalid option position side")),
                    };
                    positions.push(Position {
                        id: format!(
                            "{}-{}",
                            row.symbol,
                            if side == PositionSide::Short {
                                "short"
                            } else {
                                "long"
                            }
                        ),
                        symbol: row.symbol,
                        side,
                        entry_price: finite(&row.entry_price)?,
                        notional: finite(&row.mark_value)?.abs(),
                        leverage: 1.,
                        margin_type: None,
                        position_mode: None,
                        unrealized_pnl: finite(&row.upnl)?,
                        realized_pnl: None,
                        contracts: quantity.abs(),
                        liquidation_price: 0.,
                        margin_size: None,
                        last_update: Some(cx.now),
                    });
                }
                cx.store.set_positions(positions);
            }
            Kind::Orders => {
                let rows: Vec<OrderRow> =
                    serde_json::from_str(body).map_err(|_| error("Malformed option orders"))?;
                let orders = rows
                    .into_iter()
                    .map(|r| r.order(cx.now))
                    .collect::<Result<Vec<_>, _>>()?;
                if orders.iter().any(|o| cx.store.market(&o.symbol).is_none()) {
                    return Err(error("Unknown option order symbol"));
                }
                let token = request
                    .token
                    .take()
                    .ok_or_else(|| error("Missing order snapshot token"))?;
                // The token spans the entire response; no older read can clear a new mark.
                if cx.store.orders_snapshot_current(&token) && !self.option_state.orders_owed {
                    if cx.store.commit_orders_snapshot([token], |_| orders) {
                        cx.orders_reconciled_since(None, request.read_epoch);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct StreamTicker {
    s: String,
    c: String,
    #[serde(rename = "P")]
    percentage: String,
    v: String,
    q: String,
}

impl BinanceFutures {
    pub(crate) fn on_options_frame(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: Channel,
        text: &str,
    ) {
        if channel == Channel::Private {
            // Stream facts request a complete fresh account observation; they cannot
            // turn a partial position/balance event into a full snapshot.
            #[derive(Deserialize)]
            struct Header {
                e: String,
            }
            if let Ok(header) = serde_json::from_str::<Header>(text) {
                match header.e.as_str() {
                    "listenKeyExpired" => self.retire(
                        cx,
                        channel,
                        safe_cex_engine::socket_lifecycle::RetireReason::Closed,
                    ),
                    "ORDER_TRADE_UPDATE" | "ACCOUNT_UPDATE" => {
                        cx.store.invalidate_order_snapshots();
                        self.options_refresh(cx);
                    }
                    _ => {}
                }
            }
            return;
        }
        #[derive(Deserialize)]
        struct Header {
            e: String,
        }
        let Ok(header) = serde_json::from_str::<Header>(text) else {
            return;
        };
        match header.e.as_str() {
            "24hrTicker" => {
                let Ok(row) = serde_json::from_str::<StreamTicker>(text) else {
                    return;
                };
                let Ok(values) = [&row.c, &row.percentage, &row.v, &row.q]
                    .map(|s| finite(s))
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                else {
                    return;
                };
                cx.store.update_ticker(&row.s, |ticker| {
                    ticker.last = values[0];
                    ticker.percentage = values[1] * 100.;
                    ticker.volume = values[2];
                    ticker.quote_volume = values[3];
                });
            }
            "depth" | "depthUpdate" => {
                let Ok(row) = serde_json::from_str::<crate::wire::DepthEvent>(text) else {
                    return;
                };
                let parse = |rows: Vec<[std::borrow::Cow<'_, str>; 2]>| {
                    rows.into_iter()
                        .map(|[p, q]| {
                            let price = positive(&p)?;
                            let quantity = finite(&q)?;
                            if quantity < 0. {
                                return Err(error("Negative option book quantity"));
                            }
                            Ok((price, quantity))
                        })
                        .collect::<Result<Vec<_>, VenueError>>()
                };
                let (Ok(bids), Ok(asks)) = (parse(row.b), parse(row.a)) else {
                    return;
                };
                let Some(state) = self.books.get_mut(row.s.as_ref()) else {
                    return;
                };
                if matches!(state.sync, crate::Sync::Synced { last_applied, .. } if last_applied >= row.final_update_id)
                {
                    return;
                }
                state.book.clear();
                for (p, q) in bids {
                    state
                        .book
                        .apply(safe_cex_engine::book::BookSide::Bids, p, q);
                }
                for (p, q) in asks {
                    state
                        .book
                        .apply(safe_cex_engine::book::BookSide::Asks, p, q);
                }
                state.sync = crate::Sync::Synced {
                    last_applied: row.final_update_id,
                    awaiting_first: false,
                };
                cx.book_changed(row.s.as_ref());
            }
            "kline" => self.on_market_frame(cx, text),
            _ => {}
        }
    }
}

pub(crate) fn native_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Number(u64),
        Text(String),
    }
    match Id::deserialize(d)? {
        Id::Number(id) => Ok(id),
        Id::Text(id) => id.parse().map_err(serde::de::Error::custom),
    }
}
pub(crate) fn placed_id(body: &str) -> Option<u64> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Proof {
        #[serde(deserialize_with = "native_id")]
        order_id: u64,
        code: Option<i64>,
    }
    if !body.trim_start().starts_with('{') {
        return None;
    }
    serde_json::from_str::<Proof>(body)
        .ok()
        .filter(|p| p.code.is_none() && p.order_id > 0)
        .map(|p| p.order_id)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalRow {
    #[serde(flatten)]
    order: OrderRow,
    #[serde(default, alias = "createDate")]
    create_time: f64,
    #[serde(default)]
    update_time: f64,
    #[serde(default)]
    avg_price: Option<String>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: OrderSide,
    #[serde(rename = "type")]
    kind: OrderType,
    status: OrderStatus,
    price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    updated_time: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillRow {
    #[serde(deserialize_with = "native_id", alias = "id")]
    trade_id: u64,
    #[serde(deserialize_with = "native_id")]
    order_id: u64,
    symbol: String,
    side: String,
    price: String,
    quantity: String,
    fee: String,
    quote_asset: String,
    liquidity: String,
    time: f64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Fill {
    id: String,
    order_id: String,
    symbol: String,
    side: OrderSide,
    price: f64,
    qty: f64,
    timestamp: f64,
    commission: f64,
    commission_asset: String,
    maker: bool,
}

impl BinanceFutures {
    pub(crate) fn options_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        query: safe_cex_engine::Query,
    ) {
        use safe_cex_engine::Query as Q;
        let (symbol, from, to, limit, kind) = match &query {
            Q::Fills {
                symbol: Some(symbol),
                from_ms,
                to_ms,
                limit,
                ..
            } => (symbol, *from_ms, *to_ms, *limit, Kind::Fills),
            Q::OrderHistory {
                symbol: Some(symbol),
                from_ms,
                to_ms,
                limit,
            } => (symbol, *from_ms, *to_ms, *limit, Kind::History),
            _ => {
                cx.refuse_command(safe_cex_engine::request_context::Operation::Read, VenueError::new("Binance Options history requires a symbol; income, public trades and client-ID lookup are unsupported").kind(ErrorKind::Unsupported));
                return;
            }
        };
        if symbol.is_empty()
            || [from, to]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v < 0.)
            || matches!((from,to),(Some(a),Some(b)) if a>b)
            || limit.is_some_and(|n| n == 0 || n > 1000)
        {
            cx.refuse_command(
                safe_cex_engine::request_context::Operation::Read,
                error("Invalid Binance Options history bounds"),
            );
            return;
        }
        let Ok((selection, id)) = self
            .credentials
            .select(CredentialRole::Read { scope: None })
        else {
            cx.refuse_command(
                safe_cex_engine::request_context::Operation::Read,
                VenueError::new("Binance Options history requires credentials")
                    .kind(ErrorKind::AuthInvalid),
            );
            return;
        };
        let credential = selection.credential(id.credential.clone());
        let id = id.clone();
        let Ok(context) = cx.admit_http_command(reads::policy(Some(credential))) else {
            return;
        };
        let mut params = Query::new().str("symbol", symbol);
        if let Some(from) = from {
            params = params.num("startTime", from);
        }
        if let Some(to) = to {
            params = params.num("endTime", to);
        }
        if let Some(limit) = limit {
            params = params.num("limit", f64::from(limit));
        }
        let path = if kind == Kind::Fills {
            self.paths.user_trades
        } else {
            self.paths.all_orders
        };
        let request = Request {
            context: context.clone(),
            kind,
            token: None,
            read_epoch: 0,
            query: Some(query),
        };
        self.managed_read(
            cx,
            context,
            reads::Read {
                url: format!("{}{path}", self.urls.rest),
                credentials: Some(id),
                query: params,
                stream: None,
            },
            Pending::Options(Box::new(request)),
        );
    }

    fn options_query_result(&self, request: &Request, body: &str) -> Result<String, VenueError> {
        let malformed = || error("Malformed Binance Options history response");
        match request.query.as_ref() {
            Some(safe_cex_engine::Query::Fills {
                symbol, order_id, ..
            }) => {
                let rows: Vec<FillRow> = serde_json::from_str(body).map_err(|_| malformed())?;
                let mut fills = Vec::new();
                let mut seen = HashSet::new();
                for row in rows {
                    if symbol.as_deref() != Some(row.symbol.as_str())
                        || row.trade_id == 0
                        || row.order_id == 0
                        || !row.time.is_finite()
                        || row.time < 0.
                    {
                        return Err(malformed());
                    }
                    if order_id
                        .as_ref()
                        .is_some_and(|id| *id != row.order_id.to_string())
                    {
                        continue;
                    }
                    if !seen.insert(row.trade_id) {
                        return Err(malformed());
                    }
                    let side = match row.side.as_str() {
                        "BUY" => OrderSide::Buy,
                        "SELL" => OrderSide::Sell,
                        _ => return Err(malformed()),
                    };
                    if !matches!(row.liquidity.as_str(), "MAKER" | "TAKER") {
                        return Err(malformed());
                    }
                    fills.push(Fill {
                        id: row.trade_id.to_string(),
                        order_id: row.order_id.to_string(),
                        symbol: row.symbol,
                        side,
                        price: positive(&row.price)?,
                        qty: positive(&row.quantity)?,
                        timestamp: row.time / 1000.,
                        commission: -finite(&row.fee)?,
                        commission_asset: row.quote_asset,
                        maker: row.liquidity == "MAKER",
                    });
                }
                serde_json::to_string(&fills).map_err(|_| malformed())
            }
            Some(safe_cex_engine::Query::OrderHistory { symbol, .. }) => {
                let rows: Vec<HistoricalRow> =
                    serde_json::from_str(body).map_err(|_| malformed())?;
                let mut history = Vec::new();
                let mut seen = HashSet::new();
                for row in rows {
                    let order = row.order.order(0.)?;
                    if symbol.as_deref() != Some(order.symbol.as_str())
                        || !seen.insert(order.id.clone())
                        || !row.create_time.is_finite()
                        || !row.update_time.is_finite()
                    {
                        return Err(malformed());
                    }
                    history.push(HistoricalOrder {
                        id: order.id,
                        symbol: order.symbol,
                        side: order.side,
                        kind: order.kind,
                        status: order.status,
                        price: order.price,
                        average_price: row
                            .avg_price
                            .as_deref()
                            .map(finite)
                            .transpose()?
                            .filter(|v| *v > 0.),
                        amount: order.amount,
                        filled: order.filled,
                        reduce_only: order.reduce_only,
                        created_time: row.create_time,
                        updated_time: row.update_time,
                    });
                }
                serde_json::to_string(&history).map_err(|_| malformed())
            }
            _ => Err(malformed()),
        }
    }
}
