//! Bounded reverse-cursor reads share engine-owned page admission and completion.
//! Order-filtered fill totals remain strict proofs, never partial executions.
use crate::account::decode_order;
use crate::wire::{Number, Str, Text};
use crate::{Grvt, Job, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet};

const PAGE_SIZE: usize = 1000;
const PAGE_CAP: u32 = 50;
const CREATION_SKEW_NS: u64 = 300_000_000_000;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fills,
    Orders,
    Funding,
    IncomeFills,
    Trades,
    Lookup,
}
pub(crate) struct Request {
    kind: Kind,
    symbol: Option<String>,
    instrument: Option<String>,
    base: Option<String>,
    quote: Option<String>,
    account: Option<String>,
    order_id: Option<String>,
    native_ids: HashMap<String, String>,
    target_created: Option<u64>,
    lifecycles: HashMap<String, String>,
    page_lifecycles: HashMap<String, String>,
    from: Option<u64>,
    to: u64,
    limit: usize,
    income_kind: Option<String>,
    combine_income: bool,
    history: Box<CursorHistory<Row>>,
    seen: HashSet<String>,
    page_seen: HashSet<String>,
    raw_seen: HashSet<String>,
    page_raw: HashSet<String>,
    page_count: usize,
    stage_count: usize,
    failed: Option<String>,
}
/// Request-level failures fail the read; row-level ones skip only that row.
enum Rejected {
    Request(String),
    Row(String),
}
impl From<String> for Rejected {
    fn from(error: String) -> Self {
        Self::Request(error)
    }
}
impl From<&str> for Rejected {
    fn from(error: &str) -> Self {
        Self::Request(error.into())
    }
}
impl Request {
    pub(crate) fn array_at(&self) -> Option<ArrayAt> {
        (self.kind != Kind::Lookup).then_some(ArrayAt::Envelope(&[&["r"], &["result"]]))
    }
    pub(crate) fn is_public(&self) -> bool {
        self.kind == Kind::Trades
    }
    pub(crate) fn reset(&mut self) -> bool {
        if !self.history.discard_staged_page() {
            return false;
        }
        self.page_seen.clear();
        self.page_raw.clear();
        self.page_lifecycles.clear();
        self.page_count = 0;
        self.failed = None;
        true
    }
    fn accepts(&self, symbol: &str, time: u64) -> bool {
        self.symbol.as_ref().is_none_or(|s| s == symbol)
            && self.from.is_none_or(|start| time >= start)
            && time <= self.to
    }
    fn append(&mut self, key: String, row: Row) {
        if !self.seen.contains(&key) && self.page_seen.insert(key) {
            self.history.stage(row);
        }
    }
    fn count(&self) -> usize {
        self.history.accepted_len() + self.history.staged_len()
    }
    fn complete_fills(&self) -> bool {
        self.kind == Kind::Fills && self.order_id.is_some()
    }
    fn wire_start(&self) -> Option<u64> {
        if self.complete_fills()
            && let Some(created) = self.target_created
        {
            // Earlier lifecycle rows are already excluded from filtered results.
            // Bound the native walk so a new chaser avoids lifetime account history.
            let anchor = created.saturating_sub(CREATION_SKEW_NS).min(self.to);
            Some(self.from.unwrap_or(0).max(anchor))
        } else {
            self.from
        }
    }
    fn path(&self) -> &'static str {
        match self.kind {
            Kind::Fills | Kind::IncomeFills => "/lite/v1/fill_history",
            Kind::Orders => "/lite/v1/order_history",
            Kind::Funding => "/lite/v1/funding_payment_history",
            Kind::Trades => "/lite/v1/trade_history",
            Kind::Lookup => "/lite/v1/order",
        }
    }
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
    fn time(&self) -> f64 {
        match self {
            Self::Fill(v) => v.timestamp * 1000.,
            Self::Order(v) => v.created_time,
            Self::Income(v) => v.timestamp,
            Self::Trade(v) => v.timestamp,
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
    side: &'static str,
    commission: f64,
    commission_asset: String,
    position_side: &'static str,
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
    price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    position_side: &'static str,
    created_time: f64,
    updated_time: f64,
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
    side: &'static str,
}
#[derive(Deserialize)]
struct FillWire<'a> {
    #[serde(borrow, rename = "et", alias = "event_time")]
    time: Text<'a>,
    #[serde(borrow, rename = "sa", alias = "sub_account_id", default)]
    account: Str<'a>,
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(borrow, rename = "ti", alias = "trade_id")]
    id: Text<'a>,
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    client_id: Text<'a>,
    #[serde(borrow, rename = "oi", alias = "order_id", default)]
    native_id: Text<'a>,
    #[serde(borrow, rename = "m", alias = "metadata", default)]
    metadata: Option<FillMetadata<'a>>,
    #[serde(rename = "ib", alias = "is_buyer")]
    buyer: bool,
    #[serde(rename = "it", alias = "is_taker", default)]
    taker: Option<bool>,
    #[serde(rename = "s", alias = "size")]
    size: Number,
    #[serde(rename = "p", alias = "price")]
    price: Number,
    #[serde(rename = "f", alias = "fee", default)]
    fee: Option<Number>,
    #[serde(rename = "rp", alias = "realized_pnl", default)]
    pnl: Option<Number>,
    #[serde(borrow, rename = "fc", alias = "fee_currency", default)]
    asset: Str<'a>,
}
#[derive(Deserialize)]
struct FillMetadata<'a> {
    #[serde(borrow, rename = "co", alias = "client_order_id", default)]
    client_id: Text<'a>,
}
#[derive(Deserialize)]
struct FundingWire<'a> {
    #[serde(borrow, rename = "et", alias = "event_time")]
    time: Text<'a>,
    #[serde(borrow, rename = "sa", alias = "sub_account_id", default)]
    account: Str<'a>,
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(borrow, rename = "ti", alias = "tx_id")]
    id: Text<'a>,
    #[serde(borrow, rename = "c", alias = "currency")]
    asset: Str<'a>,
    #[serde(rename = "a", alias = "amount")]
    amount: Number,
}
#[derive(Deserialize)]
struct TradeWire<'a> {
    #[serde(borrow, rename = "et", alias = "event_time")]
    time: Text<'a>,
    #[serde(borrow, rename = "i", alias = "instrument")]
    instrument: Str<'a>,
    #[serde(borrow, rename = "ti", alias = "trade_id")]
    id: Text<'a>,
    #[serde(rename = "it", alias = "is_taker_buyer")]
    buyer: bool,
    #[serde(rename = "s", alias = "size")]
    size: Number,
    #[serde(rename = "p", alias = "price")]
    price: Number,
}
#[derive(Deserialize)]
struct OrderExtras<'a> {
    #[serde(borrow, rename = "sa", alias = "sub_account_id", default)]
    account: Str<'a>,
    #[serde(rename = "s1", alias = "state")]
    state: Average,
}
#[derive(Deserialize)]
struct Average {
    #[serde(rename = "af", alias = "avg_fill_price", default)]
    prices: Vec<Number>,
}
#[derive(Deserialize)]
struct Cursor<'a> {
    #[serde(borrow, rename = "n", alias = "next")]
    next: Str<'a>,
}
#[derive(Serialize)]
struct Body<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    sa: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    i: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    b: Option<[&'a str; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    q: Option<[&'a str; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    st: Option<String>,
    et: String,
    l: usize,
    c: &'a str,
}
fn time(text: &str) -> Result<u64, String> {
    text.parse::<u64>()
        .ok()
        .filter(|t| *t > 0)
        .ok_or("Invalid GRVT history time".into())
}
fn bound(value: f64) -> Result<u64, String> {
    if !value.is_finite() || value < 0. || value >= u64::MAX as f64 / 1e6 {
        return Err("Invalid GRVT history bounds".into());
    }
    crate::sign::scaled_u64(&value.to_string(), 6)
}
fn side(buy: bool) -> &'static str {
    if buy { "buy" } else { "sell" }
}
impl Grvt {
    pub(crate) fn start_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (kind, symbol, order_id, from, to, limit, income_kind, combine_income) = match query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Fills,
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
                None,
                false,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                symbol,
                None,
                from_ms,
                to_ms,
                limit,
                None,
                false,
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                let k = kind.map(|s| s.to_ascii_uppercase());
                let selected = match k.as_deref() {
                    None | Some("FUNDING_FEE") => Kind::Funding,
                    Some("COMMISSION" | "REALIZED_PNL") => Kind::IncomeFills,
                    _ => {
                        let Ok(context) = cx.admit_http_command(Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 20_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        }) else {
                            return;
                        };
                        let _ = cx.fail_context(&context, "Unsupported GRVT income category");
                        return;
                    }
                };
                let combine = k.is_none();
                (selected, symbol, None, from_ms, to_ms, limit, k, combine)
            }
            Query::PublicTrades { symbol, limit } => (
                Kind::Trades,
                Some(symbol),
                None,
                None,
                None,
                limit,
                None,
                false,
            ),
            Query::OrderByClientId {
                symbol,
                client_order_id,
            } => (
                Kind::Lookup,
                Some(symbol),
                Some(client_order_id),
                None,
                None,
                None,
                None,
                false,
            ),
        };
        if kind != Kind::Trades && (self.options.public_data_only || self.account.session.is_none())
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
            let _ = cx.fail_context(&context, "GRVT query requires an authenticated session");
            return;
        }
        let selected = symbol.as_ref().and_then(|s| cx.store.market(s));
        if symbol.is_some() && selected.is_none() {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.fail_context(&context, "Unknown GRVT query market");
            return;
        }
        let result =
            (|| Ok::<_, String>((from.map(bound).transpose()?, bound(to.unwrap_or(cx.now))?)))();
        let (from, to) = match result {
            Ok(v) => v,
            Err(e) => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                let _ = cx.fail_context(&context, e);
                return;
            }
        };
        if from.is_some_and(|f| f > to) || order_id.as_ref().is_some_and(String::is_empty) {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.fail_context(&context, "Invalid GRVT query range or order ID");
            return;
        }
        if kind == Kind::Lookup
            && order_id.as_ref().is_none_or(|id| {
                id.parse::<u64>().ok().is_none_or(|n| n == 0) || id.starts_with('0')
            })
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
            let _ = cx.fail_context(&context, "Invalid GRVT client order ID");
            return;
        }
        let account = self.account.trading_account_id.clone();
        if kind != Kind::Trades && account.is_none() {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.fail_context(&context, "GRVT query requires a subaccount");
            return;
        }
        let target_created = order_id
            .as_deref()
            .and_then(|id| self.account.order_created(id));
        let request = Request {
            kind,
            symbol,
            instrument: selected.map(|m| m.id.clone()),
            base: selected.map(|m| m.base.clone()),
            quote: selected.map(|m| m.quote.clone()),
            account,
            order_id,
            native_ids: self.account.native_ids.clone(),
            target_created,
            lifecycles: HashMap::new(),
            page_lifecycles: HashMap::new(),
            from,
            to,
            limit: limit.unwrap_or(1000).clamp(1, 1000) as usize,
            income_kind,
            combine_income,
            history: Box::new(CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(PAGE_CAP)
                    .stall_on_empty_page()
                    .stall_on_no_new_rows(),
                |reason| match reason {
                    Stall::PageBudget => "INCOMPLETE: GRVT history page cap reached".into(),
                    _ => "INCOMPLETE: GRVT history cursor stalled or page cap reached".into(),
                },
                |rows| {
                    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
                },
            )),
            seen: HashSet::new(),
            page_seen: HashSet::new(),
            raw_seen: HashSet::new(),
            page_raw: HashSet::new(),
            page_count: 0,
            stage_count: 0,
            failed: None,
        };
        self.history_send(cx, request);
    }
    fn history_send(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let body = if request.kind == Kind::Lookup {
            #[derive(Serialize)]
            struct Lookup<'a> {
                sa: &'a str,
                co: &'a str,
            }
            serde_json::to_string(&Lookup {
                sa: request.account.as_deref().unwrap_or(""),
                co: request.order_id.as_deref().unwrap_or(""),
            })
            .expect("query")
        } else {
            let public = request.is_public();
            serde_json::to_string(&Body {
                sa: if public {
                    None
                } else {
                    request.account.as_deref()
                },
                i: if public || request.kind == Kind::Funding {
                    request.instrument.as_deref()
                } else {
                    None
                },
                b: if public {
                    None
                } else {
                    request.base.as_deref().map(|s| [s])
                },
                q: if public {
                    None
                } else {
                    request.quote.as_deref().map(|s| [s])
                },
                st: request.wire_start().map(|t| t.to_string()),
                et: request.to.to_string(),
                l: PAGE_SIZE,
                c: request.history.cursor().unwrap_or(""),
            })
            .expect("query")
        };
        self.request(cx, request.path(), body, Job::History(Box::new(request)));
    }
    pub(crate) fn history_item(&mut self, cx: &mut Cx<'_, Pending>, r: &mut Request, item: &str) {
        r.page_count += 1;
        if r.page_count > PAGE_SIZE {
            r.failed = Some("Malformed GRVT history page exceeds the native limit".into());
            return;
        }
        match self.history_decode(cx, r, item) {
            Ok(()) => {}
            Err(Rejected::Request(error)) => r.failed = Some(error),
            Err(Rejected::Row(error)) => {
                // The raw text stands in for the identity, so paging still sees a fresh row.
                r.page_raw.insert(format!("skipped:{item}"));
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!("Skipped malformed GRVT historical order: {error}"),
                );
            }
        }
    }
    fn history_decode(
        &self,
        cx: &Cx<'_, Pending>,
        r: &mut Request,
        item: &str,
    ) -> Result<(), Rejected> {
        match r.kind {
            Kind::Fills | Kind::IncomeFills => {
                let f: FillWire<'_> =
                    serde_json::from_str(item).map_err(|_| "Invalid GRVT historical fill")?;
                let t = time(&f.time)?;
                r.page_raw.insert(f.id.to_string());
                if f.id.is_empty() || f.size.0 <= 0. || f.price.0 <= 0. {
                    return Err("Invalid GRVT fill amounts or identity".into());
                }
                if !f.account.is_empty() && Some(&*f.account) != r.account.as_deref() {
                    return Err("GRVT fill belongs to another subaccount".into());
                }
                let Some(m) = cx.store.market_by_id(&f.instrument) else {
                    return Ok(());
                };
                if !r.accepts(&m.symbol, t) {
                    return Ok(());
                }
                let top = (!f.client_id.is_empty() && &*f.client_id != "0")
                    .then(|| f.client_id.to_string());
                let meta = f
                    .metadata
                    .as_ref()
                    .filter(|m| !m.client_id.is_empty() && &*m.client_id != "0")
                    .map(|m| m.client_id.to_string());
                if top.is_some() && meta.is_some() && top != meta {
                    return Err("Conflicting GRVT fill client IDs".into());
                }
                let client = top.or(meta);
                let canonical = client.clone().unwrap_or_else(|| {
                    r.native_ids
                        .iter()
                        .find(|(_, native)| native.as_str() == &*f.native_id)
                        .map_or_else(|| f.native_id.to_string(), |(id, _)| id.clone())
                });
                if canonical.is_empty() {
                    return Err("GRVT fill has no order identity".into());
                }
                if r.order_id
                    .as_ref()
                    .is_some_and(|id| id != &canonical && (client.is_some() || id != &*f.native_id))
                {
                    return Ok(());
                }
                if r.complete_fills() {
                    if r.target_created.is_some_and(|created| t < created) {
                        return Ok(());
                    }
                    if f.native_id.is_empty() || matches!(&*f.native_id, "0" | "0x0" | "0x00") {
                        return Err(
                            "INCOMPLETE: GRVT filtered fill has no native lifecycle identity"
                                .into(),
                        );
                    }
                    if let Some(native) = r.order_id.as_ref().and_then(|id| r.native_ids.get(id)) {
                        if native != &*f.native_id {
                            return Ok(());
                        }
                    } else {
                        let known = r
                            .lifecycles
                            .get(&canonical)
                            .or_else(|| r.page_lifecycles.get(&canonical));
                        if known.is_some_and(|native| native != &*f.native_id) {
                            return Err(
                                "INCOMPLETE: GRVT client ID spans ambiguous order lifecycles"
                                    .into(),
                            );
                        }
                        r.page_lifecycles
                            .insert(canonical.clone(), f.native_id.to_string());
                    }
                }
                if r.kind == Kind::Fills {
                    r.append(
                        f.id.to_string(),
                        Row::Fill(Execution {
                            id: f.id.to_string(),
                            timestamp: t as f64 / 1e9,
                            symbol: m.symbol.clone(),
                            order_id: canonical,
                            price: f.price.0,
                            qty: f.size.0,
                            side: side(f.buyer),
                            commission: f.fee.map_or(0., |n| n.0),
                            commission_asset: if f.asset.is_empty() {
                                m.quote.clone()
                            } else {
                                f.asset.to_string()
                            },
                            position_side: "both",
                            realized_pnl: f.pnl.map(|n| n.0),
                            maker: f.taker.map(|b| !b),
                        }),
                    );
                } else {
                    let asset = if f.asset.is_empty() {
                        m.quote.clone()
                    } else {
                        f.asset.to_string()
                    };
                    for (kind, value, category) in [
                        ("COMMISSION", f.fee.map(|n| -n.0), "commission"),
                        ("REALIZED_PNL", f.pnl.map(|n| n.0), "realized_pnl"),
                    ] {
                        if r.income_kind.as_ref().is_none_or(|k| k == kind)
                            && let Some(value) = value
                        {
                            r.append(
                                format!("{kind}:{}", f.id),
                                Row::Income(Income {
                                    symbol: m.symbol.clone(),
                                    income_type: kind,
                                    income: value,
                                    asset: if kind == "COMMISSION" {
                                        asset.clone()
                                    } else {
                                        m.quote.clone()
                                    },
                                    timestamp: t as f64 / 1e6,
                                    tran_id: f.id.to_string(),
                                    trade_id: Some(f.id.to_string()),
                                    category,
                                }),
                            );
                        }
                    }
                }
            }
            Kind::Funding => {
                let f: FundingWire<'_> =
                    serde_json::from_str(item).map_err(|_| "Invalid GRVT funding payment")?;
                let t = time(&f.time)?;
                r.page_raw
                    .insert(format!("{}:{}:{}:{}", f.id, f.instrument, t, f.asset));
                if f.id.is_empty() || f.asset.is_empty() {
                    return Err("Invalid GRVT funding identity".into());
                }
                if !f.account.is_empty() && Some(&*f.account) != r.account.as_deref() {
                    return Err("GRVT funding belongs to another subaccount".into());
                }
                let Some(m) = cx.store.market_by_id(&f.instrument) else {
                    return Ok(());
                };
                if r.accepts(&m.symbol, t) {
                    r.append(
                        format!("{}:{}:{}:{}", f.id, m.id, t, f.asset),
                        Row::Income(Income {
                            symbol: m.symbol.clone(),
                            income_type: "FUNDING_FEE",
                            income: -f.amount.0,
                            asset: f.asset.to_string(),
                            timestamp: t as f64 / 1e6,
                            tran_id: f.id.to_string(),
                            trade_id: None,
                            category: "funding",
                        }),
                    );
                }
            }
            Kind::Orders => {
                // Ownership is checked before decoding, so a foreign row always fails the read.
                let extra: OrderExtras<'_> = serde_json::from_str(item)
                    .map_err(|_| Rejected::Row("Invalid GRVT historical order".into()))?;
                if !extra.account.is_empty() && Some(&*extra.account) != r.account.as_deref() {
                    return Err("GRVT order belongs to another subaccount".into());
                }
                let o = decode_order(item, cx.store, cx.now).map_err(Rejected::Row)?;
                r.page_raw.insert(
                    o.native_id
                        .clone()
                        .unwrap_or_else(|| format!("{}:{}", o.order.id, o.created_ns)),
                );
                if o.created_ns == 0 {
                    return Err(Rejected::Row("Missing GRVT order creation time".into()));
                }
                if o.order.status == OrderStatus::Open || !r.accepts(&o.order.symbol, o.created_ns)
                {
                    return Ok(());
                }
                let order = o.order;
                r.append(
                    o.native_id
                        .unwrap_or_else(|| format!("{}:{}", order.id, o.created_ns)),
                    Row::Order(HistoricalOrder {
                        id: order.id,
                        symbol: order.symbol,
                        side: side(order.side == safe_cex_engine::model::OrderSide::Buy),
                        kind: order.kind,
                        status: order.status,
                        price: order.price,
                        average_price: extra.state.prices.first().map(|p| p.0),
                        amount: order.amount,
                        filled: order.filled,
                        reduce_only: order.reduce_only,
                        position_side: "both",
                        created_time: o.created_ns as f64 / 1e6,
                        updated_time: o.updated_ns as f64 / 1e6,
                    }),
                );
            }
            Kind::Trades => {
                let f: TradeWire<'_> =
                    serde_json::from_str(item).map_err(|_| "Invalid GRVT public trade")?;
                let t = time(&f.time)?;
                r.page_raw.insert(f.id.to_string());
                if f.id.is_empty()
                    || f.size.0 <= 0.
                    || f.price.0 <= 0.
                    || !(f.size.0 * f.price.0).is_finite()
                {
                    return Err("Invalid GRVT trade amounts".into());
                }
                let Some(m) = cx.store.market_by_id(&f.instrument) else {
                    return Err("Unknown GRVT public trade instrument".into());
                };
                if r.accepts(&m.symbol, t) {
                    r.append(
                        f.id.to_string(),
                        Row::Trade(Trade {
                            id: f.id.to_string(),
                            symbol: m.symbol.clone(),
                            price: f.price.0,
                            qty: f.size.0,
                            quote_qty: f.size.0 * f.price.0,
                            timestamp: t as f64 / 1e6,
                            side: side(f.buyer),
                        }),
                    );
                }
            }
            Kind::Lookup => return Err("Unexpected streamed GRVT order lookup".into()),
        }
        Ok(())
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = crate::wire::check(&response) {
            self.history_failed(cx, r, &error);
            return;
        }
        if r.kind == Kind::Lookup {
            #[derive(Deserialize)]
            struct Lookup<'a> {
                #[serde(borrow, rename = "r", alias = "result")]
                result: &'a RawValue,
            }
            #[derive(Serialize)]
            struct Found {
                found: bool,
                order: safe_cex_engine::model::Order,
            }
            let result = (|| {
                let lookup: Lookup<'_> = serde_json::from_str(response.body)
                    .map_err(|_| "Malformed GRVT order lookup")?;
                let raw = lookup.result;
                if raw.get() == "null" {
                    return Ok("{\"found\":false,\"definitive\":false}".into());
                }
                let extra: OrderExtras<'_> =
                    serde_json::from_str(raw.get()).map_err(|_| "Malformed GRVT lookup owner")?;
                if !extra.account.is_empty() && Some(&*extra.account) != r.account.as_deref() {
                    return Err("GRVT lookup belongs to another subaccount".into());
                }
                let decoded = decode_order(raw.get(), cx.store, cx.now)?;
                if decoded.order.client_order_id.as_ref() != r.order_id.as_ref()
                    || Some(&decoded.order.symbol) != r.symbol.as_ref()
                {
                    return Err("GRVT lookup identity mismatch".into());
                }
                serde_json::to_string(&Found {
                    found: true,
                    order: decoded.order,
                })
                .map_err(|e| e.to_string())
            })();
            match result {
                Ok(data) => cx.done(r.history.command(), data),
                Err(error) => cx.fail(r.history.command(), error),
            };
            return;
        }
        if let Some(error) = r.failed.take() {
            self.history_failed(cx, r, &error);
            return;
        }
        let cursor = match serde_json::from_str::<Cursor<'_>>(response.body) {
            Ok(c) => c.next.to_string(),
            Err(_) => {
                self.history_failed(cx, r, "INCOMPLETE: missing or invalid GRVT history cursor");
                return;
            }
        };
        if r.complete_fills() && r.count() > r.limit {
            self.history_failed(cx, r, "INCOMPLETE: GRVT order fills exceed requested limit");
            return;
        }
        // Raw identities commit only with their page, so a retried page is not a repeat.
        let page = CursorPage {
            next: Some(cursor),
            rows: r.page_count,
            fresh: r
                .page_raw
                .iter()
                .filter(|id| !r.raw_seen.contains(*id))
                .count(),
            want_more: r.complete_fills() || r.stage_count + r.history.staged_len() < r.limit,
        };
        let admitted = r.history.staged_len();
        let more = match r.history.advance(page) {
            CursorStep::More(_) => true,
            CursorStep::Done => false,
            CursorStep::Stalled(reason) => {
                let message = match reason {
                    Stall::PageBudget => "INCOMPLETE: GRVT history page cap reached",
                    _ => "INCOMPLETE: GRVT history cursor stalled or page cap reached",
                };
                self.history_failed(cx, r, message);
                return;
            }
        };
        r.stage_count += admitted;
        r.seen.extend(r.page_seen.drain());
        r.raw_seen.extend(r.page_raw.drain());
        r.lifecycles.extend(r.page_lifecycles.drain());
        if !more && r.combine_income && r.kind == Kind::Funding {
            r.kind = Kind::IncomeFills;
            if !r.history.restart_stage() {
                self.history_rejected(cx, r, "GRVT history stage restart failed");
                return;
            }
            r.stage_count = 0;
            r.raw_seen.clear();
            if !r.reset() {
                self.history_rejected(cx, r, "GRVT history page reset failed");
                return;
            }
            self.history_send(cx, r);
            return;
        }
        if more {
            if !r.reset() {
                self.history_rejected(cx, r, "GRVT history page reset failed");
                return;
            }
            self.history_send(cx, r);
            return;
        }
        self.history_finish(cx, r, Ok(()));
    }
    fn history_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        result: Result<(), VenueError>,
    ) {
        let limit = request.limit;
        request.history.finish_with(cx, result, move |mut rows| {
            rows.sort_by(|a, b| b.time().total_cmp(&a.time()));
            rows.truncate(limit);
            serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
        });
    }
    /// Preparation/session refusal and complete-order proofs never publish a
    /// partial total, even if an earlier history page was admitted.
    pub(crate) fn history_rejected(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        if request.kind == Kind::Lookup {
            cx.fail(request.history.command(), error.to_owned());
        } else {
            request.history.reject(cx, VenueError::new(error));
        }
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        if request.kind == Kind::Lookup || request.complete_fills() {
            self.history_rejected(cx, request, error);
        } else {
            self.history_finish(cx, request, Err(VenueError::new(error)));
        }
    }
}
