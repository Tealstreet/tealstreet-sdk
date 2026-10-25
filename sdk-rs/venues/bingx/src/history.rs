//! Rare reads return Binance-compatible JSON without touching the store.
use crate::sign::{Credentials, Params};
use crate::{Pending, wire};
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::de::optional_number;
use safe_cex_engine::history::{ReadCompletion, ReadHistoryRows};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::request_context::{Context, Error, Operation, Owner, Policy, RetryPolicy};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::time;
use safe_cex_engine::{CommandWait, Cx, Level, Method, Query, Response, TimerId, VenueError};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

const WEEK: f64 = 7.0 * 24.0 * 3_600_000.0;
const PAGE_DELAY: f64 = 200.0;
const MAX_FILL_PAGES: u32 = 1000;

pub struct Request {
    page: Page,
    leg: Leg,
    rows: Vec<Row>,
    error: Option<String>,
}

struct Page {
    command: u32,
    window: (f64, f64),
    path: &'static str,
    params: Params,
    at: ArrayAt,
    resent: bool,
}

enum Leg {
    Signed(ReadLeg),
    Public(Context),
}
impl Leg {
    fn context(&self) -> &Context {
        match self {
            Self::Signed(leg) => leg.context(),
            Self::Public(context) => context,
        }
    }
}

enum Root {
    Signed(ReadRoot),
    Public(Context),
}
impl Root {
    fn context(&self) -> &Context {
        match self {
            Self::Signed(root) => root.context(),
            Self::Public(context) => context,
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Fills,
    Orders,
    Income,
    Trades,
}

struct QueryState {
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    income_type: Option<String>,
    limit: u32,
    from: Option<f64>,
    to: Option<f64>,
    walk: Option<RangeWalk>,
    fills: Option<FillPages>,
}

struct Read {
    native: QueryState,
    root: Root,
    output: ReadHistoryRows<Row>,
    timer: Option<TimerId>,
}

struct FillPages {
    from: f64,
    to: f64,
    end: f64,
    page: u32,
    seen: HashSet<String>,
}

#[derive(Default)]
pub struct History {
    reads: HashMap<u32, Read>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(Trade),
}

impl Row {
    fn timestamp(&self) -> f64 {
        match self {
            Self::Fill(row) => row.raw_time,
            Self::Order(row) => row.created_time,
            Self::Income(row) => row.timestamp,
            Self::Trade(row) => row.timestamp,
        }
    }

    fn identity(&self) -> String {
        match self {
            Self::Fill(row) => row.id.clone(),
            Self::Order(row) => row.id.clone(),
            Self::Income(row) => format!(
                "{}|{}|{}|{}",
                row.tran_id, row.income_type, row.income, row.timestamp
            ),
            Self::Trade(row) => row.id.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    #[serde(skip)]
    raw_time: f64,
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: &'static str,
    commission: f64,
    commission_asset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maker: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: OrderType,
    status: OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
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
    side: &'static str,
}

#[derive(Deserialize)]
struct Envelope<T> {
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<T>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FillData {
    Direct(Vec<FillRow>),
    History { fill_history_orders: Vec<FillRow> },
    Orders { fill_orders: Vec<FillRow> },
}

impl FillData {
    fn is_empty(&self) -> bool {
        match self {
            Self::Direct(rows)
            | Self::History {
                fill_history_orders: rows,
            }
            | Self::Orders { fill_orders: rows } => rows.is_empty(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillRow {
    #[serde(
        default,
        alias = "fillId",
        alias = "id",
        deserialize_with = "identifier"
    )]
    trade_id: String,
    #[serde(alias = "order_id", deserialize_with = "identifier")]
    order_id: String,
    #[serde(default)]
    symbol: String,
    side: String,
    #[serde(alias = "filledPrice", deserialize_with = "wire::number")]
    price: f64,
    #[serde(
        alias = "volume",
        alias = "filledQty",
        deserialize_with = "wire::number"
    )]
    qty: f64,
    #[serde(alias = "filledTime", alias = "filledTm", deserialize_with = "text")]
    time: String,
    #[serde(default, alias = "fee", deserialize_with = "wire::number")]
    commission: f64,
    #[serde(default, alias = "currency")]
    commission_asset: Option<String>,
    #[serde(default)]
    position_side: Option<String>,
    #[serde(default, alias = "realizedPNL", deserialize_with = "optional_number")]
    realized_pnl: Option<f64>,
    #[serde(default)]
    maker: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderRow {
    #[serde(deserialize_with = "identifier")]
    order_id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    #[serde(deserialize_with = "wire::number")]
    price: f64,
    #[serde(default, deserialize_with = "wire::number")]
    avg_price: f64,
    #[serde(deserialize_with = "wire::number")]
    orig_qty: f64,
    #[serde(deserialize_with = "wire::number")]
    executed_qty: f64,
    #[serde(default, deserialize_with = "crate::account_wire::boolean")]
    reduce_only: bool,
    #[serde(default)]
    position_side: Option<String>,
    #[serde(deserialize_with = "wire::number")]
    time: f64,
    #[serde(default, deserialize_with = "wire::number")]
    update_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeRow {
    #[serde(default)]
    symbol: String,
    income_type: String,
    #[serde(deserialize_with = "wire::number")]
    income: f64,
    asset: String,
    #[serde(deserialize_with = "wire::number")]
    time: f64,
    #[serde(deserialize_with = "identifier")]
    tran_id: String,
    #[serde(default, deserialize_with = "optional_text")]
    trade_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeRow {
    #[serde(deserialize_with = "identifier")]
    id: String,
    #[serde(deserialize_with = "wire::number")]
    price: f64,
    #[serde(deserialize_with = "wire::number")]
    qty: f64,
    #[serde(deserialize_with = "wire::number")]
    quote_qty: f64,
    #[serde(deserialize_with = "wire::number")]
    time: f64,
    buyer_maker: bool,
}

fn local_result(cx: &mut Cx<'_, Pending>, result: Result<&str, &str>) {
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    match result {
        Ok(value) => {
            let _ = cx.done_context(&context, value.into());
        }
        Err(error) => {
            let _ = cx.fail_context(&context, error);
        }
    }
}

impl History {
    pub fn query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        command: u32,
        query: Query,
    ) {
        if credentials.is_none()
            && !matches!(
                query,
                Query::PublicTrades { .. } | Query::OrderByClientId { .. }
            )
        {
            local_result(cx, Ok("[]"));
            return;
        }
        let mut read = QueryState {
            kind: Kind::Trades,
            symbol: None,
            order_id: None,
            income_type: None,
            limit: 1000,
            from: None,
            to: None,
            walk: None,
            fills: None,
        };
        match query {
            Query::OrderByClientId { .. } => {
                local_result(
                    cx,
                    Err("BingX client order lookup is not supported by the Rust engine"),
                );
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let Some(symbol) = symbol else {
                    local_result(cx, Ok("[]"));
                    return;
                };
                read.kind = Kind::Fills;
                read.symbol = Some(symbol);
                read.order_id = order_id;
                read.limit = limit.unwrap_or(100).clamp(1, 1000);
                // This endpoint requires both timestamps, unlike other rare reads.
                read.to = Some(to_ms.unwrap_or(cx.now));
                read.from = Some(from_ms.unwrap_or(read.to.unwrap_or(cx.now) - WEEK));
            }
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                read.kind = Kind::Orders;
                read.symbol = symbol;
                read.from = from_ms;
                read.to = to_ms;
                read.limit = limit.unwrap_or(500).clamp(1, 1000);
            }
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                read.kind = Kind::Income;
                read.symbol = symbol;
                read.income_type = kind;
                read.from = from_ms;
                read.to = to_ms;
                read.limit = limit.unwrap_or(100).clamp(1, 1000);
            }
            Query::PublicTrades { symbol, limit } => {
                read.symbol = Some(symbol);
                read.limit = limit.unwrap_or(500).clamp(1, 1000);
            }
        }
        if !matches!(read.kind, Kind::Trades) {
            if read.from.is_some() && read.to.is_none() {
                read.to = Some(cx.now);
            }
            if read.to.is_some() && read.from.is_none() {
                read.from = read.to.map(|to| to - WEEK);
            }
        }
        if let (Some(from), Some(to)) = (read.from, read.to) {
            if !from.is_finite() || !to.is_finite() || from > to {
                local_result(cx, Err("invalid history time range"));
                return;
            }
            if matches!(read.kind, Kind::Fills) {
                read.fills = Some(FillPages {
                    from,
                    to: (from + WEEK).min(to),
                    end: to,
                    page: 1,
                    seen: HashSet::new(),
                });
            } else if from < to {
                read.walk = Some(RangeWalk::new(from, to, WEEK, read.limit as usize));
            }
        }
        match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return;
            }
            Some(CommandWait::Remaining(_)) => {}
        }
        let root = if matches!(read.kind, Kind::Trades) {
            let Ok(context) = cx.admit_caller_read_root(None, None) else {
                return;
            };
            Root::Public(context)
        } else {
            let Some(credentials) = credentials else {
                return;
            };
            let slots = Slots::new(
                Some(credentials.primary.clone()),
                credentials.read.clone(),
                SlotPolicy::default(),
            );
            let selected = SelectedRead::select(&slots, ReadRole::Read { scope: None });
            let Ok(root) = SelectedRead::command_root(
                cx,
                selected,
                LocalRefusal {
                    diagnostic: None,
                    error: "BingX history requires credentials".into(),
                },
            ) else {
                return;
            };
            Root::Signed(root)
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        self.reads.insert(
            command,
            Read {
                native: read,
                root,
                output: ReadHistoryRows::new(completion, encode_rows),
                timer: None,
            },
        );
        self.next(cx, command);
    }

    fn next(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        let Some(read) = self.reads.get_mut(&command) else {
            return;
        };
        if let Err(error) = cx.validate_read_root_context(read.root.context()) {
            self.admission_failed(cx, command, error);
            return;
        }
        let (from, to) = if let Some(fills) = &read.native.fills {
            (Some(fills.from), Some(fills.to))
        } else if let Some(walk) = &mut read.native.walk {
            loop {
                match walk.step() {
                    Step::Fetch { from, to } => break (Some(from), Some(to)),
                    Step::ChunkDone => continue,
                    Step::Finished => {
                        let truncated = walk.truncated;
                        self.finish(cx, command, truncated);
                        return;
                    }
                }
            }
        } else {
            (read.native.from, read.native.to)
        };
        let mut params = Params::new();
        if let Some(symbol) = &read.native.symbol {
            params = params.str("symbol", &native_symbol(symbol));
        }
        if let Some(order) = &read.native.order_id {
            params = params.str("orderId", order);
        }
        if let Some(kind) = &read.native.income_type {
            params = params.str("incomeType", kind);
        }
        let (path, start, end, limit) = match read.native.kind {
            Kind::Fills => (
                "/openApi/swap/v2/trade/fillHistory",
                "startTs",
                "endTs",
                "pageSize",
            ),
            Kind::Orders => (
                "/openApi/swap/v2/trade/allOrders",
                "startTime",
                "endTime",
                "limit",
            ),
            Kind::Income => (
                "/openApi/swap/v2/user/income",
                "startTime",
                "endTime",
                "limit",
            ),
            Kind::Trades => (
                "/openApi/swap/v2/quote/trades",
                "startTime",
                "endTime",
                "limit",
            ),
        };
        params = params.num(limit, f64::from(read.native.limit));
        if let Some(fills) = &read.native.fills {
            params = params.num("pageIndex", f64::from(fills.page));
        }
        if let Some(from) = from {
            params = params.num(start, from);
        }
        if let Some(to) = to {
            params = params.num(end, to);
        }
        let at = if matches!(read.native.kind, Kind::Fills) {
            ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"])
        } else {
            ArrayAt::EnvelopeField("data")
        };
        self.send_page(
            cx,
            Page {
                command,
                window: (from.unwrap_or(0.0), to.unwrap_or(0.0)),
                path,
                params,
                at,
                resent: false,
            },
        );
    }

    fn send_page(&mut self, cx: &mut Cx<'_, Pending>, page: Page) {
        let command = page.command;
        let Some(read) = self.reads.get(&command) else {
            return;
        };
        let (url, _, body) = page.params.clone().render(
            Method::Get,
            page.path,
            vec![("X-SOURCE-KEY", "Tealstreet".into())],
        );
        let queued = match &read.root {
            Root::Signed(root) => {
                let leg = match root.leg(
                    cx,
                    ReadLegPolicy {
                        timeout_ms: 20_000,
                        pacing: None,
                    },
                ) {
                    Ok(leg) => leg,
                    Err(error) => {
                        self.admission_failed(cx, command, error);
                        return;
                    }
                };
                let path = page.path;
                let params = page.params.clone();
                leg.queue(
                    cx,
                    ReadRequest {
                        method: Method::Get,
                        url,
                        body,
                        stream: Some(page.at),
                    },
                    move |identity| {
                        Box::new(crate::sign::dispatch::Http::new(
                            identity.clone(),
                            Method::Get,
                            path,
                            params,
                        ))
                    },
                    move |leg| {
                        Pending::History(Request {
                            page,
                            leg: Leg::Signed(leg),
                            rows: Vec::new(),
                            error: None,
                        })
                    },
                )
            }
            Root::Public(root) => {
                let context = match cx.http_leg_context(
                    root,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                ) {
                    Ok(context) => context,
                    Err(error) => {
                        self.admission_failed(cx, command, error);
                        return;
                    }
                };
                let at = page.at;
                let pending = Pending::History(Request {
                    page,
                    leg: Leg::Public(context.clone()),
                    rows: Vec::new(),
                    error: None,
                });
                let queued = cx.request_managed_with_headers(
                    context,
                    None,
                    Method::Get,
                    url,
                    crate::sign::unsigned_headers(Method::Get),
                    body,
                    pending,
                );
                if let Ok(id) = queued {
                    cx.stream_response(id, at);
                }
                queued
            }
        };
        if let Err((Pending::History(request), error)) = queued {
            let _ = cx.finish_http_context(request.leg.context());
            self.admission_failed(cx, command, error);
        }
    }

    fn delivery_current(&self, cx: &Cx<'_, Pending>, request: &Request) -> bool {
        let Some(read) = self.reads.get(&request.page.command) else {
            return false;
        };
        let selected = match (&read.root, &request.leg) {
            (Root::Signed(root), Leg::Signed(leg)) => root.validate_delivery(cx, leg),
            (Root::Public(root), Leg::Public(leg)) => cx.validate_read_http_delivery(root, leg),
            _ => return false,
        };
        selected.is_ok()
            && read
                .output
                .validate_delivery(cx, request.leg.context())
                .is_ok()
    }

    fn owner_current(&self, cx: &Cx<'_, Pending>, request: &Request) -> bool {
        self.reads.get(&request.page.command).is_some_and(|read| {
            read.root.context().owner() == request.leg.context().owner()
                && cx.validate_read_root_context(read.root.context()).is_ok()
        })
    }

    fn admission_failed(&mut self, cx: &mut Cx<'_, Pending>, command: u32, error: Error) {
        let Some(read) = self.reads.get(&command) else {
            return;
        };
        if error == Error::Revoked
            && cx.validate_read_root_context(read.root.context()) == Err(Error::Revoked)
        {
            if let Some(read) = self.reads.remove(&command)
                && let Some(timer) = read.timer
            {
                cx.clear_timer(timer);
            }
            return;
        }
        self.reject(cx, command, error.read_venue_error());
    }

    fn reject(&mut self, cx: &mut Cx<'_, Pending>, command: u32, error: VenueError) {
        if let Some(read) = self.reads.remove(&command) {
            if let Some(timer) = read.timer {
                cx.clear_timer(timer);
            }
            let _ = read.output.reject(cx, error);
        }
    }

    pub fn retire(&mut self, owner: Owner) {
        self.reads
            .retain(|_, read| read.root.context().owner() != owner);
    }

    fn resign(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) -> Result<Request, ()> {
        let signed = match &request.leg {
            Leg::Signed(_) => true,
            Leg::Public(_) => false,
        };
        if signed && !request.page.resent && crate::signature_rejected(response.body) {
            cx.notice(
                Level::Warn,
                safe_cex_engine::log::code::REQUEST_FAILED,
                "BingX rejected a request signature; re-signing it once",
            );
            let _ = cx.finish_http_context(request.leg.context());
            request.page.resent = true;
            self.send_page(cx, request.page);
            return Err(());
        }
        Ok(request)
    }

    pub fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !self.delivery_current(cx, &request) {
            let _ = cx.finish_http_context(request.leg.context());
            return;
        }
        if let Ok(request) = self.resign(cx, request, response) {
            self.on_failed(
                cx,
                request,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            );
        }
    }

    pub fn on_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        if !self.delivery_current(cx, request) {
            return;
        }
        let Some(read) = self.reads.get(&request.page.command) else {
            return;
        };
        let row = match read.native.kind {
            Kind::Orders => serde_json::from_str::<OrderRow>(item)
                .map_err(|e| e.to_string())
                .and_then(map_order),
            Kind::Income => serde_json::from_str::<IncomeRow>(item)
                .map(|r| {
                    Row::Income(Income {
                        symbol: symbol(&r.symbol),
                        category: income_category(&r.income_type),
                        income_type: r.income_type,
                        income: r.income,
                        asset: r.asset,
                        timestamp: r.time,
                        tran_id: r.tran_id,
                        trade_id: r.trade_id.filter(|s| !s.is_empty()),
                    })
                })
                .map_err(|e| e.to_string()),
            Kind::Trades => serde_json::from_str::<TradeRow>(item)
                .map(|r| {
                    Row::Trade(Trade {
                        id: r.id,
                        symbol: symbol(read.native.symbol.as_deref().unwrap_or_default()),
                        price: r.price,
                        qty: r.qty,
                        quote_qty: r.quote_qty,
                        timestamp: r.time,
                        side: if r.buyer_maker { "sell" } else { "buy" },
                    })
                })
                .map_err(|e| e.to_string()),
            Kind::Fills => serde_json::from_str::<FillRow>(item)
                .map_err(|e| e.to_string())
                .and_then(|row| map_fill(row, read.native.symbol.as_deref().unwrap_or_default())),
        };
        match row {
            Ok(row) => request.rows.push(row),
            Err(error) => request.error = Some(error),
        }
    }

    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let context = request.leg.context().clone();
        if !self.delivery_current(cx, &request) {
            let _ = cx.finish_http_context(&context);
            return;
        }
        let Ok(request) = self.resign(cx, request, response) else {
            return;
        };
        self.response_data(cx, request, response);
        let _ = cx.finish_http_context(&context);
    }

    fn response_data(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.on_failed(
                cx,
                request,
                &format!("history HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        let Some(read) = self.reads.get_mut(&request.page.command) else {
            return;
        };
        if matches!(read.native.kind, Kind::Fills) {
            let result = serde_json::from_str::<Envelope<FillData>>(response.body)
                .map_err(|e| e.to_string())
                .and_then(|response| {
                    if response.code != 0 {
                        return Err(format!("{}: {}", response.code, response.msg));
                    }
                    response.data.ok_or_else(|| "missing fill data".to_owned())
                })
                .and_then(|data| {
                    if data.is_empty() {
                        Ok(())
                    } else {
                        Err("streamed fill metadata retained unexpected rows".into())
                    }
                });
            if let Err(error) = result {
                request.error = Some(error);
            }
        } else {
            let result = serde_json::from_str::<Envelope<de::IgnoredAny>>(response.body)
                .map_err(|e| e.to_string())
                .and_then(|response| {
                    if response.code != 0 {
                        return Err(format!("{}: {}", response.code, response.msg));
                    }
                    response
                        .data
                        .ok_or_else(|| "missing history data".to_owned())
                });
            if let Err(error) = result {
                request.error = Some(error);
            }
        }
        if let Some(error) = request.error.take() {
            self.on_failed(cx, request, &error);
            return;
        }
        let command = request.page.command;
        match Self::accept_page(cx, read, &mut request) {
            Ok(Some(truncated)) => self.finish(cx, command, truncated),
            Ok(None) => read.timer = Some(cx.set_timer(PAGE_DELAY)),
            Err(error) => self.on_http_failed(cx, request, error),
        }
    }

    fn accept_page(
        cx: &Cx<'_, Pending>,
        read: &mut Read,
        request: &mut Request,
    ) -> Result<Option<bool>, VenueError> {
        let mut admission = read
            .output
            .admit(cx, request.leg.context())
            .map_err(Error::read_venue_error)?;
        let result = if let Some(fills) = &mut read.native.fills {
            let full = request.rows.len() >= read.native.limit as usize;
            if request
                .rows
                .iter()
                .any(|row| row.timestamp() < fills.from || row.timestamp() > fills.to)
            {
                return Err("fill history returned executions outside its requested window".into());
            }
            let before = fills.seen.len();
            for row in request.rows.drain(..) {
                if fills.seen.insert(row.identity()) {
                    admission.stage(row);
                }
            }
            if full {
                if fills.seen.len() == before || fills.page >= MAX_FILL_PAGES {
                    return Err("fill history is incomplete: native pagination made no progress or reached its page budget".into());
                }
                fills.page += 1;
                None
            } else if fills.to < fills.end {
                let next = fills.to + 1.0;
                if next <= fills.from {
                    return Err("fill history time range cannot advance".into());
                }
                fills.from = next;
                fills.to = (next + WEEK).min(fills.end);
                fills.page = 1;
                None
            } else {
                Some(false)
            }
        } else if let Some(walk) = &mut read.native.walk {
            let page: Vec<PageRow> = request
                .rows
                .iter()
                .map(|row| PageRow {
                    timestamp: row.timestamp(),
                    identity: row.identity(),
                })
                .collect();
            let fresh = walk.page(request.page.window.0, request.page.window.1, &page);
            let mut rows: Vec<_> = request.rows.drain(..).map(Some).collect();
            for index in fresh {
                if let Some(row) = rows[index].take() {
                    admission.stage(row);
                }
            }
            None
        } else {
            let saturated = read.native.from.is_some()
                && read.native.from == read.native.to
                && request.rows.len() >= read.native.limit as usize;
            for row in request.rows.drain(..) {
                admission.stage(row);
            }
            Some(saturated)
        };
        admission.accept_page();
        Ok(result)
    }

    fn finish(&mut self, cx: &mut Cx<'_, Pending>, command: u32, truncated: bool) {
        let Some(read) = self.reads.remove(&command) else {
            return;
        };
        if let Some(timer) = read.timer {
            cx.clear_timer(timer);
        }
        if truncated {
            let _ = read.output.reject(
                cx,
                "history is incomplete: time window could not be fully paged".into(),
            );
            return;
        }
        let kind = read.native.kind;
        let _ = read.output.finish_with(cx, Ok(()), move |mut rows| {
            if !matches!(kind, Kind::Trades) {
                rows.sort_by(|a, b| b.timestamp().total_cmp(&a.timestamp()));
            }
            encode_rows(&rows)
        });
    }

    pub fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let command = self.reads.iter_mut().find_map(|(command, read)| {
            if read.timer == Some(timer) {
                read.timer = None;
                Some(*command)
            } else {
                None
            }
        });
        if let Some(command) = command {
            self.next(cx, command);
        }
    }

    pub fn on_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.on_http_failed(cx, request, error);
    }

    pub fn on_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        self.on_http_failed(cx, request, error.into());
    }

    pub fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        let current = self.owner_current(cx, &request);
        let _ = cx.finish_http_context(request.leg.context());
        if current {
            self.reject(cx, request.page.command, error);
        }
    }
}

fn encode_rows(rows: &[Row]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| error.to_string().into())
}

fn map_fill(row: FillRow, fallback_symbol: &str) -> Result<Row, String> {
    if row.trade_id.trim().is_empty() {
        return Err("missing native fill identifier".into());
    }
    let timestamp = parse_time(&row.time).ok_or_else(|| "invalid fill timestamp".to_owned())?;
    Ok(Row::Fill(Execution {
        raw_time: timestamp,
        id: row.trade_id,
        timestamp: (timestamp / 1000.0).floor(),
        symbol: if row.symbol.is_empty() {
            fallback_symbol.to_owned()
        } else {
            symbol(&row.symbol)
        },
        order_id: row.order_id,
        price: row.price,
        qty: row.qty,
        side: side(&row.side)?,
        commission: row.commission.abs(),
        commission_asset: row.commission_asset.unwrap_or_else(|| "USDT".into()),
        position_side: hedge_side(row.position_side.as_deref()),
        realized_pnl: row.realized_pnl,
        maker: row.maker,
    }))
}

fn map_order(row: OrderRow) -> Result<Row, String> {
    let kind = match row.kind.as_str() {
        "MARKET" => OrderType::Market,
        "LIMIT" => OrderType::Limit,
        "STOP" | "TRIGGER_LIMIT" | "STOP_LIMIT" | "TAKE_PROFIT" | "TAKE_PROFIT_LIMIT" => {
            OrderType::Conditional
        }
        "STOP_MARKET" | "TRIGGER_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        "TRAILING_STOP_MARKET" | "TRAILING_TP_SL" => OrderType::TrailingStopLoss,
        _ => return Err(format!("unknown order type {}", row.kind)),
    };
    let status = match row.status.as_str() {
        "NEW" | "OPEN" | "WORKING" | "PENDING" | "PARTIALLY_FILLED" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        _ => return Err(format!("unknown order status {}", row.status)),
    };
    Ok(Row::Order(HistoricalOrder {
        id: row.order_id,
        symbol: symbol(&row.symbol),
        side: side(&row.side)?,
        kind,
        status,
        price: (row.price != 0.0).then_some(row.price),
        average_price: (row.avg_price != 0.0).then_some(row.avg_price),
        amount: row.orig_qty,
        filled: row.executed_qty,
        reduce_only: row.reduce_only,
        position_side: hedge_side(row.position_side.as_deref()),
        created_time: row.time,
        updated_time: (row.update_time != 0.0).then_some(row.update_time),
    }))
}

fn side(value: &str) -> Result<&'static str, String> {
    match value.to_ascii_uppercase().as_str() {
        "BUY" => Ok("buy"),
        "SELL" => Ok("sell"),
        _ => Err(format!("invalid side {value}")),
    }
}

fn hedge_side(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "LONG" | "long" => Some("long"),
        "SHORT" | "short" => Some("short"),
        "BOTH" | "both" => Some("both"),
        _ => None,
    }
}

fn income_category(value: &str) -> &'static str {
    match value {
        "REALIZED_PNL" => "realized_pnl",
        "FUNDING_FEE" => "funding",
        "TRADING_FEE" | "COMMISSION" => "commission",
        "TRANSFER" => "transfer",
        "INSURANCE_CLEAR" | "ADL" => "liquidation",
        _ => "other",
    }
}

fn symbol(value: &str) -> String {
    value.replace('-', "")
}

fn native_symbol(value: &str) -> String {
    if value.contains('-') {
        return value.to_owned();
    }
    for quote in ["USDT", "USDC"] {
        if let Some(base) = value.strip_suffix(quote) {
            return format!("{base}-{quote}");
        }
    }
    value.to_owned()
}

fn text<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    struct Text;
    impl Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string or integer")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
            Ok(value.to_owned())
        }
        fn visit_u64<E: de::Error>(self, value: u64) -> Result<String, E> {
            Ok(value.to_string())
        }
        fn visit_i64<E: de::Error>(self, value: i64) -> Result<String, E> {
            Ok(value.to_string())
        }
    }
    de.deserialize_any(Text)
}

fn identifier<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    let value = text(de)?;
    if value.trim().is_empty() {
        return Err(de::Error::custom("empty native identifier"));
    }
    Ok(value)
}

fn optional_text<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Text(#[serde(deserialize_with = "text")] String);
    Option::<Text>::deserialize(de).map(|value| value.map(|value| value.0))
}

/// RFC3339 offsets are part of the instant; never consult the host timezone.
fn parse_time(value: &str) -> Option<f64> {
    if !value.contains('T') {
        return value.parse::<f64>().ok().filter(|v| v.is_finite());
    }
    let (date, time) = value.split_once('T')?;
    let mut date = date.split('-');
    let year = date.next()?.parse::<i32>().ok()?;
    if !(0..=9999).contains(&year) {
        return None;
    }
    let month = date.next()?.parse::<u32>().ok()?;
    let day = date.next()?.parse::<u32>().ok()?;
    if date.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let last = time::days_in_month(i64::from(year), i64::from(month))?;
    if day == 0 || i64::from(day) > last {
        return None;
    }
    let (clock, offset) = if let Some(clock) = time.strip_suffix('Z') {
        (clock, 0.0)
    } else {
        let at = time.find(['+', '-'])?;
        let (clock, offset) = time.split_at(at);
        let mut parts = offset[1..].split(':');
        let hours = parts.next()?.parse::<u32>().ok()?;
        let minutes = parts.next()?.parse::<u32>().ok()?;
        if parts.next().is_some() || hours > 23 || minutes > 59 {
            return None;
        }
        let seconds = f64::from(hours * 3600 + minutes * 60);
        (
            clock,
            if offset.starts_with('-') {
                -seconds
            } else {
                seconds
            },
        )
    };
    let mut clock = clock.split(':');
    let hour = clock.next()?.parse::<u32>().ok()?;
    let minute = clock.next()?.parse::<u32>().ok()?;
    let second = clock.next()?.parse::<f64>().ok()?;
    if clock.next().is_some() || hour > 23 || minute > 59 || !(0.0..60.0).contains(&second) {
        return None;
    }
    let days = time::days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    Some((days as f64 * 86400.0 + f64::from(hour * 3600 + minute * 60) + second - offset) * 1000.0)
}
