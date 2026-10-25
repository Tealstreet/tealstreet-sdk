//! Cursor-owned rare reads. Failed pages never masquerade as complete history.
use crate::{Lighter, Pending};
use safe_cex_engine::de::number;
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, Method, Outcome, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;

/// Pages per stage before a read that still has a cursor fails as incomplete.
const MAX_PAGES: u32 = 1000;

pub struct Request {
    owner: Box<CursorHistory<Row>>,
    read: Read,
    stage: Stage,
    symbol: Option<String>,
    market: Option<String>,
    account: Option<u64>,
    order: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    /// Accepted output count at the start of the native stage.
    stage_start: usize,
    count: usize,
    failed: Option<String>,
    /// Trades arrive newest first; a row older than `from` ends the window.
    past_window: bool,
}
enum Read {
    Fills,
    Orders,
    Trades,
    Income(IncomeKind),
}
#[derive(Clone, Copy, PartialEq)]
enum IncomeKind {
    All,
    Funding,
    Commission,
    Rebate,
    Realized,
}
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Trades,
    Orders,
    Funding,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Execution(Execution),
    Order(HistoricalOrder),
    Trade(Trade),
    Income(Income),
}
impl Row {
    fn time(&self) -> f64 {
        match self {
            Self::Execution(v) => v.timestamp * 1000.0,
            Self::Order(v) => v.updated_time.unwrap_or(v.created_time),
            Self::Trade(v) => v.timestamp,
            Self::Income(v) => v.timestamp,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: OrderSide,
    commission: f64,
    commission_asset: String,
    maker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
}
#[derive(Serialize)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: &'static str,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_id: Option<String>,
    category: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: OrderSide,
}

#[derive(Default, Deserialize)]
#[serde(transparent)]
struct Atom<'a>(#[serde(borrow, deserialize_with = "atom")] Cow<'a, str>);
fn atom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Cow<'de, str>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Cow<'de, str>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("string or integer")
        }
        fn visit_borrowed_str<E: serde::de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(s))
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(s.into()))
        }
        fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
        fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
    }
    d.deserialize_any(Visitor)
}
#[derive(Default, Deserialize)]
#[serde(transparent)]
struct Num(#[serde(deserialize_with = "number")] f64);
#[derive(Deserialize)]
struct TradeWire<'a> {
    #[serde(default, borrow)]
    trade_id: Atom<'a>,
    #[serde(default, borrow)]
    trade_id_str: Atom<'a>,
    #[serde(borrow)]
    market_id: Atom<'a>,
    size: Num,
    price: Num,
    #[serde(default)]
    usd_amount: Option<Num>,
    timestamp: Num,
    #[serde(default)]
    transaction_time: Option<Num>,
    #[serde(default, borrow)]
    ask_id: Atom<'a>,
    #[serde(default, borrow)]
    bid_id: Atom<'a>,
    #[serde(default, borrow)]
    ask_id_str: Atom<'a>,
    #[serde(default, borrow)]
    bid_id_str: Atom<'a>,
    #[serde(default, borrow)]
    ask_account_id: Atom<'a>,
    #[serde(default, borrow)]
    bid_account_id: Atom<'a>,
    is_maker_ask: bool,
    #[serde(default)]
    maker_fee: Option<Num>,
    #[serde(default)]
    taker_fee: Option<Num>,
    #[serde(default)]
    integrator_maker_fee: Option<Num>,
    #[serde(default)]
    integrator_taker_fee: Option<Num>,
    #[serde(default)]
    ask_account_pnl: Option<Num>,
    #[serde(default)]
    bid_account_pnl: Option<Num>,
}
#[derive(Deserialize)]
struct FundingWire<'a> {
    #[serde(borrow)]
    funding_id: Atom<'a>,
    #[serde(borrow)]
    market_id: Atom<'a>,
    timestamp: Num,
    change: Num,
}
#[derive(Deserialize)]
struct OrderTimes {
    #[serde(default)]
    created_at: Option<Num>,
    #[serde(default)]
    timestamp: Option<Num>,
    #[serde(default)]
    updated_at: Option<Num>,
    #[serde(default)]
    filled_quote_amount: Option<Num>,
}
#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    code: Atom<'a>,
    #[serde(default, borrow)]
    message: Cow<'a, str>,
    #[serde(default, borrow)]
    next_cursor: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    trades: Option<&'a RawValue>,
    #[serde(default, borrow)]
    orders: Option<&'a RawValue>,
    #[serde(default, borrow)]
    position_fundings: Option<&'a RawValue>,
}

impl Lighter {
    pub(crate) fn history_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (read, stage, symbol, order, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                cx.refuse_command(
                    Operation::Read,
                    "Lighter does not support caller client order IDs",
                );
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Read::Fills,
                Stage::Trades,
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Read::Orders,
                Stage::Orders,
                symbol,
                None,
                from_ms,
                to_ms,
                limit,
            ),
            Query::PublicTrades { symbol, limit } => (
                Read::Trades,
                Stage::Trades,
                Some(symbol),
                None,
                None,
                None,
                limit,
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                let kind = match kind.as_deref().map(str::to_ascii_uppercase).as_deref() {
                    None | Some("ALL") => IncomeKind::All,
                    Some("FUNDING" | "FUNDING_FEE") => IncomeKind::Funding,
                    Some("COMMISSION") => IncomeKind::Commission,
                    Some("COMMISSION_REBATE") => IncomeKind::Rebate,
                    Some("REALIZED_PNL") => IncomeKind::Realized,
                    _ => {
                        cx.refuse_command(
                            Operation::Read,
                            "Lighter income supports funding, commission and realized PnL",
                        );
                        return;
                    }
                };
                (
                    Read::Income(kind),
                    if kind == IncomeKind::Funding {
                        Stage::Funding
                    } else {
                        Stage::Trades
                    },
                    symbol,
                    None,
                    from_ms,
                    to_ms,
                    limit,
                )
            }
        };
        if from
            .into_iter()
            .chain(to)
            .any(|n| !n.is_finite() || n < 0.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
        {
            cx.refuse_command(Operation::Read, "Invalid Lighter history time range");
            return;
        }
        if order.as_ref().is_some_and(|s| s.parse::<u64>().is_err()) {
            cx.refuse_command(Operation::Read, "Invalid Lighter native order index");
            return;
        }
        let market = match symbol.as_ref() {
            Some(symbol) => match cx.store.market(symbol) {
                Some(m) => Some(m.id.clone()),
                None => {
                    cx.refuse_command(Operation::Read, "Lighter market not found");
                    return;
                }
            },
            None => None,
        };
        let account = if matches!(read, Read::Trades) {
            None
        } else {
            match self.account.account_index {
                Some(a) if self.account.signer.is_some() => Some(a),
                _ => {
                    if self.options.public_data_only
                        || self.options.secret.as_deref().is_none_or(str::is_empty)
                        || self
                            .options
                            .api_key_index
                            .as_deref()
                            .is_none_or(str::is_empty)
                        || self.account.account_index.is_some()
                    {
                        let Ok(context) = cx.admit_http_command(Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 20_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        }) else {
                            return;
                        };
                        let _ = cx.done_context(&context, "[]".into());
                    } else {
                        cx.refuse_command(
                            Operation::Read,
                            "Lighter private account identity is unavailable",
                        );
                    }
                    return;
                }
            }
        };
        let request = Request {
            owner: Box::new(CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(MAX_PAGES),
                |_| "Lighter history cursor did not make progress".into(),
                |rows| serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string())),
            )),
            read,
            stage,
            symbol,
            market,
            account,
            order,
            from,
            to,
            limit: limit.unwrap_or(100).clamp(1, 200) as usize,
            stage_start: 0,
            count: 0,
            failed: None,
            past_window: false,
        };
        self.history_request(cx, request);
    }

    fn history_request(&self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.count = 0;
        let path = match request.stage {
            Stage::Trades => "trades",
            Stage::Orders => "accountInactiveOrders",
            Stage::Funding => "positionFunding",
        };
        let mut url = format!("{}/api/v1/{path}?limit=100", self.host());
        if let Some(account) = request.account {
            url.push_str(&format!("&account_index={account}"));
        }
        if request.stage == Stage::Funding {
            if let Some(market) = &request.market {
                url.push_str(&format!("&market_ids={market}"));
            }
            if let Some(from) = request.from {
                url.push_str(&format!("&start_timestamp={:.0}", (from / 1000.0).floor()));
            }
            if let Some(to) = request.to {
                url.push_str(&format!("&end_timestamp={:.0}", (to / 1000.0).ceil()));
            }
        } else {
            url.push_str("&market_type=perp");
            if let Some(market) = &request.market {
                url.push_str(&format!("&market_id={market}"));
            }
            if request.stage == Stage::Trades {
                url.push_str("&sort_by=timestamp&sort_dir=desc&aggregate=false");
                if let Some(order) = &request.order {
                    url.push_str(&format!("&order_index_str={}", encode(order)));
                }
            }
        }
        if let Some(cursor) = request.owner.cursor() {
            url.push_str(&format!("&cursor={}", encode(cursor)));
        }
        let mut headers = Vec::new();
        if request.account.is_some() {
            match self.account_auth(cx) {
                Ok(auth) => headers.push(("authorization", auth)),
                Err(error) => {
                    request
                        .owner
                        .reject(cx, VenueError::new(error).outcome(Outcome::NotSent));
                    return;
                }
            }
        }
        let at = match request.stage {
            Stage::Trades => ArrayAt::Field("trades"),
            Stage::Orders => ArrayAt::Field("orders"),
            Stage::Funding => ArrayAt::Field("position_fundings"),
        };
        cx.request_streamed(Method::Get, url, headers, at, Pending::History(request));
    }

    pub(crate) fn history_item(&self, cx: &Cx<'_, Pending>, request: &mut Request, text: &str) {
        request.count += 1;
        if request.failed.is_some() {
            return;
        }
        let result = match request.stage {
            Stage::Trades => serde_json::from_str::<TradeWire<'_>>(text)
                .map_err(|e| e.to_string())
                .and_then(|t| self.history_trade(cx, request, t)),
            Stage::Funding => serde_json::from_str::<FundingWire<'_>>(text)
                .map_err(|e| e.to_string())
                .and_then(|f| self.history_funding(cx, request, f)),
            Stage::Orders => self.history_order(cx, request, text),
        };
        if let Err(error) = result {
            request.failed = Some(error);
        }
    }

    fn history_trade(
        &self,
        cx: &Cx<'_, Pending>,
        request: &mut Request,
        t: TradeWire<'_>,
    ) -> Result<(), String> {
        let id = if !t.trade_id_str.0.is_empty() {
            t.trade_id_str.0.as_ref()
        } else {
            t.trade_id.0.as_ref()
        };
        if id.is_empty() || t.price.0 <= 0.0 || t.size.0 <= 0.0 {
            return Err("Invalid Lighter execution row".into());
        }
        let Some(symbol) = self.symbol_for_id(&t.market_id.0) else {
            return Ok(());
        };
        if request.symbol.as_ref().is_some_and(|s| s != symbol) {
            return Ok(());
        }
        let timestamp = millis(t.transaction_time.as_ref().map_or(t.timestamp.0, |n| n.0));
        if timestamp <= 0.0 {
            return Err("Invalid Lighter execution timestamp".into());
        }
        if request.from.is_some_and(|from| timestamp < from) {
            request.past_window = true;
        }
        if !within(request, timestamp) {
            return Ok(());
        }
        if !request.owner.fresh(format!("trade:{}:{id}", t.market_id.0)) {
            return Ok(());
        }
        let quote = t.usd_amount.as_ref().map_or(t.price.0 * t.size.0, |n| n.0);
        if !quote.is_finite() || quote < 0.0 {
            return Err("Invalid Lighter execution notional".into());
        }
        if matches!(request.read, Read::Trades) {
            request.owner.stage(Row::Trade(Trade {
                id: id.into(),
                symbol: symbol.into(),
                price: t.price.0,
                qty: t.size.0,
                quote_qty: quote,
                timestamp,
                side: if t.is_maker_ask {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                },
            }));
            return Ok(());
        }
        let account = request.account.unwrap().to_string();
        let side = if t.ask_account_id.0 == account {
            OrderSide::Sell
        } else if t.bid_account_id.0 == account {
            OrderSide::Buy
        } else {
            return Err("Lighter trade does not belong to the requested account".into());
        };
        let order = if side == OrderSide::Sell {
            if t.ask_id_str.0.is_empty() {
                t.ask_id.0.as_ref()
            } else {
                t.ask_id_str.0.as_ref()
            }
        } else if t.bid_id_str.0.is_empty() {
            t.bid_id.0.as_ref()
        } else {
            t.bid_id_str.0.as_ref()
        };
        if order.is_empty() {
            return Err("Lighter execution has no native order index".into());
        }
        if request.order.as_ref().is_some_and(|id| id != order) {
            return Ok(());
        }
        let maker = (side == OrderSide::Sell) == t.is_maker_ask;
        let (rate, integrator) = if maker {
            (t.maker_fee.as_ref(), t.integrator_maker_fee.as_ref())
        } else {
            (t.taker_fee.as_ref(), t.integrator_taker_fee.as_ref())
        };
        // lighter-go FeeTick is 1_000_000; these integers are rates, not USDC amounts.
        let fee =
            quote * (rate.map_or(0.0, |n| n.0) + integrator.map_or(0.0, |n| n.0)) / 1_000_000.0;
        let pnl = if side == OrderSide::Sell {
            t.ask_account_pnl.as_ref()
        } else {
            t.bid_account_pnl.as_ref()
        }
        .map(|n| n.0);
        let asset = cx.store.market(symbol).unwrap().quote.clone();
        match request.read {
            Read::Fills => request.owner.stage(Row::Execution(Execution {
                id: id.into(),
                timestamp: timestamp / 1000.0,
                symbol: symbol.into(),
                order_id: order.into(),
                price: t.price.0,
                qty: t.size.0,
                side,
                commission: fee,
                commission_asset: asset,
                maker,
                realized_pnl: pnl,
            })),
            Read::Income(kind) => {
                if (kind == IncomeKind::All
                    || (kind == IncomeKind::Commission && fee > 0.0)
                    || (kind == IncomeKind::Rebate && fee < 0.0))
                    && fee != 0.0
                {
                    request.owner.stage(Row::Income(Income {
                        symbol: symbol.into(),
                        income_type: if fee < 0.0 {
                            "COMMISSION_REBATE"
                        } else {
                            "COMMISSION"
                        },
                        income: -fee,
                        asset: asset.clone(),
                        timestamp,
                        tran_id: format!("fee:{id}"),
                        trade_id: Some(id.into()),
                        category: if fee < 0.0 { "rebate" } else { "commission" },
                    }));
                }
                if matches!(kind, IncomeKind::All | IncomeKind::Realized)
                    && let Some(pnl) = pnl.filter(|n| *n != 0.0)
                {
                    request.owner.stage(Row::Income(Income {
                        symbol: symbol.into(),
                        income_type: "REALIZED_PNL",
                        income: pnl,
                        asset,
                        timestamp,
                        tran_id: format!("pnl:{id}"),
                        trade_id: Some(id.into()),
                        category: "realized_pnl",
                    }));
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn history_funding(
        &self,
        cx: &Cx<'_, Pending>,
        request: &mut Request,
        f: FundingWire<'_>,
    ) -> Result<(), String> {
        if f.funding_id.0.is_empty() {
            return Err("Lighter funding row has no funding ID".into());
        }
        let Some(symbol) = self.symbol_for_id(&f.market_id.0) else {
            return Ok(());
        };
        if request.symbol.as_ref().is_some_and(|s| s != symbol) {
            return Ok(());
        }
        let timestamp = millis(f.timestamp.0);
        if timestamp <= 0.0 {
            return Err("Invalid Lighter funding timestamp".into());
        }
        if within(request, timestamp)
            && request
                .owner
                .fresh(format!("funding:{}:{}", f.market_id.0, f.funding_id.0))
        {
            request.owner.stage(Row::Income(Income {
                symbol: symbol.into(),
                income_type: "FUNDING_FEE",
                income: f.change.0,
                asset: cx.store.market(symbol).unwrap().quote.clone(),
                timestamp,
                tran_id: f.funding_id.0.into_owned(),
                trade_id: None,
                category: "funding",
            }));
        }
        Ok(())
    }

    fn history_order(
        &self,
        cx: &Cx<'_, Pending>,
        request: &mut Request,
        text: &str,
    ) -> Result<(), String> {
        let raw: &RawValue = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let Some(order) = self.account_map_order(cx, raw, None)? else {
            return Ok(());
        };
        if request.symbol.as_ref().is_some_and(|s| s != &order.symbol) {
            return Ok(());
        }
        let times: OrderTimes = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let created = times
            .created_at
            .filter(|n| n.0 > 0.0)
            .or(times.timestamp.filter(|n| n.0 > 0.0))
            .map(|n| millis(n.0));
        let updated = times.updated_at.filter(|n| n.0 > 0.0).map(|n| millis(n.0));
        let timestamp = updated
            .or(created)
            .ok_or("Lighter historical order has no timestamp")?;
        if !within(request, timestamp)
            || !request
                .owner
                .fresh(format!("order:{}:{}", order.symbol, order.id))
        {
            return Ok(());
        }
        let average = times
            .filled_quote_amount
            .filter(|_| order.filled > 0.0)
            .map(|n| n.0 / order.filled);
        request.owner.stage(Row::Order(HistoricalOrder {
            id: order.id,
            symbol: order.symbol,
            side: order.side,
            kind: order.kind,
            status: order.status,
            price: order.price,
            average_price: average,
            amount: order.amount,
            filled: order.filled,
            reduce_only: order.reduce_only,
            created_time: created.unwrap_or(timestamp),
            updated_time: updated,
        }));
        Ok(())
    }

    pub(crate) fn history_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        let envelope = serde_json::from_str::<Envelope<'_>>(response.body);
        let envelope = match envelope {
            Ok(e) if response.status < 300 && e.code.0 == "200" => e,
            Ok(e) => {
                self.history_failed(cx, request, &format!("{} {}", response.status, e.message));
                return;
            }
            Err(e) => {
                finish(
                    cx,
                    request,
                    Err(VenueError::new(format!(
                        "Invalid Lighter history envelope: {e}"
                    ))),
                );
                return;
            }
        };
        let list = match request.stage {
            Stage::Trades => envelope.trades,
            Stage::Orders => envelope.orders,
            Stage::Funding => envelope.position_fundings,
        };
        if !list.is_some_and(|list| list.get().trim_start().starts_with('[')) {
            self.history_failed(cx, request, "Lighter history response has no row list");
            return;
        }
        if let Some(error) = request.failed.take() {
            finish(cx, request, Err(VenueError::new(error)));
            return;
        }
        // One trade may produce multiple ledger rows. Count shared output rows
        // relative to this stage, not raw rows or previous-stage income.
        let enough = request.owner.accepted_len() + request.owner.staged_len()
            - request.stage_start
            >= request.limit
            || request.past_window;
        let page = CursorPage {
            next: envelope
                .next_cursor
                .filter(|c| !c.is_empty())
                .map(Cow::into_owned),
            rows: request.count,
            fresh: request.count,
            want_more: !enough,
        };
        match request.owner.advance(page) {
            CursorStep::More(_) => {
                self.history_request(cx, request);
                return;
            }
            CursorStep::Stalled(_) => {
                // Native proof policy stays strict even with accepted rows.
                request.owner.reject(
                    cx,
                    VenueError::new("Lighter history cursor did not make progress"),
                );
                return;
            }
            CursorStep::Done => {}
        }
        if !enough && request.count >= 100 {
            request.owner.reject(
                cx,
                VenueError::new("Lighter history page saturated without a continuation cursor"),
            );
            return;
        }
        if request.stage == Stage::Trades && matches!(request.read, Read::Income(IncomeKind::All)) {
            request.stage = Stage::Funding;
            request.stage_start = request.owner.accepted_len();
            if !request.owner.restart_stage() {
                request.owner.reject(
                    cx,
                    VenueError::new("Lighter history stage did not complete"),
                );
                return;
            }
            request.past_window = false;
            self.history_request(cx, request);
            return;
        }
        finish(cx, request, Ok(()));
    }
    pub(crate) fn history_failed(&self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        finish(
            cx,
            request,
            Err(VenueError::new(format!("Lighter history failed: {error}"))),
        );
    }
}
fn millis(time: f64) -> f64 {
    if time >= 1e17 {
        time / 1e6
    } else if time >= 1e14 {
        time / 1000.0
    } else if time < 1e12 {
        time * 1000.0
    } else {
        time
    }
}
fn within(request: &Request, time: f64) -> bool {
    request.from.is_none_or(|from| time >= from) && request.to.is_none_or(|to| time <= to)
}

fn finish(cx: &mut Cx<'_, Pending>, request: Request, result: Result<(), VenueError>) {
    let limit = request.limit;
    request.owner.finish_with(cx, result, |mut rows| {
        rows.sort_by(|a, b| b.time().total_cmp(&a.time()));
        rows.truncate(limit);
        serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
    });
}
