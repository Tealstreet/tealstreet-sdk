//! Reported account ledger amounts, never a fold of executions or balances.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, json::ArrayAt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const DAY: f64 = 86_400_000.;
const CURRENCIES: [&str; 2] = ["USDT", "USDC"];
const PAGE: usize = 200;

#[derive(Default)]
pub struct State {
    active: HashSet<CommandId>,
    deferred: Vec<(TimerId, Request)>,
}
pub struct Request {
    command: CommandId,
    start: f64,
    end: f64,
    currency: usize,
    kind: Option<String>,
    limit: Option<usize>,
    walk: RangeWalk,
    from: f64,
    to: f64,
    page: Vec<Income>,
    occurrences: HashMap<String, u32>,
    rows: Vec<Income>,
    invalid: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row<'a> {
    type_desc: &'a str,
    #[serde(borrow)]
    amount_rv: &'a serde_json::value::RawValue,
    #[serde(borrow)]
    balance_rv: &'a serde_json::value::RawValue,
    create_time: u64,
    currency: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: &'static str,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    category: &'static str,
}
fn valid_time(value: f64) -> bool {
    value.is_finite() && value >= 0. && value.fract() == 0. && value <= 9_007_199_254_740_991.
}
impl PhemexPerpetuals {
    pub(crate) fn income_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let Query::Income {
            symbol,
            kind,
            from_ms,
            to_ms,
            limit,
        } = query
        else {
            unreachable!()
        };
        if self.options.public_data_only || self.credentials.is_none() {
            cx.done(
                command,
                Err("Phemex private history requires credentials".into()),
            );
            return;
        }
        // This route is an account ledger; its documented rows have no symbol.
        if symbol.is_some() {
            cx.done(
                command,
                Err("Phemex account income cannot be filtered by symbol".into()),
            );
            return;
        }
        let end = to_ms.unwrap_or(cx.now.floor());
        let start = from_ms.unwrap_or((end - 30. * DAY).max(0.));
        if !valid_time(start) || !valid_time(end) || start > end || limit == Some(0) {
            cx.done(command, Err("Invalid Phemex income range or limit".into()));
            return;
        }
        if self.income.active.len() >= 16 || !self.income.active.insert(command) {
            cx.done(command, Err("Too many active Phemex income queries".into()));
            return;
        }
        let request = Request {
            command,
            start,
            end,
            currency: 0,
            kind,
            limit: limit.map(|n| n as usize),
            walk: RangeWalk::new(start, end.max(start + 1.), 7. * DAY, PAGE),
            from: start,
            to: end,
            page: Vec::new(),
            occurrences: HashMap::new(),
            rows: Vec::new(),
            invalid: false,
        };
        self.income_next(cx, request);
    }
    fn income_next(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        loop {
            match request.walk.step() {
                Step::Fetch { from, to } => {
                    request.from = from;
                    request.to = to.min(request.end);
                    request.page.clear();
                    request.occurrences.clear();
                    self.income_send(cx, request);
                    return;
                }
                Step::ChunkDone => {}
                Step::Finished => {
                    if request.walk.truncated {
                        self.income_failed(cx, request, "Phemex income range is incomplete");
                        return;
                    }
                    request.currency += 1;
                    if request.currency < CURRENCIES.len() {
                        request.walk = RangeWalk::new(
                            request.start,
                            request.end.max(request.start + 1.),
                            7. * DAY,
                            PAGE,
                        );
                        continue;
                    }
                    request.rows.sort_by(|a, b| {
                        b.timestamp
                            .total_cmp(&a.timestamp)
                            .then_with(|| a.tran_id.cmp(&b.tran_id))
                    });
                    if let Some(limit) = request.limit {
                        request.rows.truncate(limit);
                    }
                    self.income.active.remove(&request.command);
                    cx.done(
                        request.command,
                        serde_json::to_string(&request.rows)
                            .map_err(|e| e.to_string())
                            .map_err(Into::into),
                    );
                    return;
                }
            }
        }
    }
    fn income_send(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.income.deferred.push((timer, request));
            return;
        }
        let query = format!(
            "currency={}&start={:.0}&end={:.0}&offset=0&limit={PAGE}",
            CURRENCIES[request.currency], request.from, request.to
        );
        self.request(
            cx,
            Method::Get,
            "/api-data/futures/v2/tradeAccountDetail",
            &query,
            None,
            Pending::Income(request),
            Some(ArrayAt::Root),
        );
    }
    pub(crate) fn income_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if request.invalid {
            return;
        }
        let Ok(row) = serde_json::from_str::<Row<'_>>(item) else {
            request.invalid = true;
            return;
        };
        let amount = serde_json::from_str::<crate::wire::N>(row.amount_rv.get()).map(|n| n.0);
        let balance = serde_json::from_str::<crate::wire::N>(row.balance_rv.get());
        if request.page.len() >= PAGE
            || row.type_desc.is_empty()
            || row.currency != CURRENCIES[request.currency]
            || row.create_time > 9_007_199_254_740_991
            || amount.is_err()
            || balance.is_err()
        {
            request.invalid = true;
            return;
        }
        // The API exposes no transaction ID. Running balance distinguishes
        // otherwise equal postings; occurrence preserves identical zero rows.
        let identity = format!(
            "{}:{}:{}:{}:{}",
            row.currency,
            row.create_time,
            row.type_desc,
            row.amount_rv.get(),
            row.balance_rv.get()
        );
        let occurrence = request.occurrences.entry(identity.clone()).or_default();
        *occurrence += 1;
        request.page.push(Income {
            symbol: "",
            income_type: row.type_desc.into(),
            income: amount.unwrap(),
            asset: row.currency.into(),
            timestamp: row.create_time as f64,
            tran_id: format!("{identity}:{occurrence}"),
            category: match row.type_desc {
                "REALIZED_PNL" => "realized_pnl",
                "FUNDING_FEE" => "funding",
                "COMMISSION" | "TRADE_FEE" => "commission",
                "TRANSFER" => "transfer",
                _ => "other",
            },
        });
    }
    pub(crate) fn income_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.income_failed(cx, request, "Phemex income HTTP failure");
            return;
        }
        let parsed =
            safe_cex_engine::json::each::<&serde_json::value::RawValue>(response.body, |row| {
                self.income_item(cx, &mut request, row.get())
            })
            .is_ok();
        if !parsed || request.invalid {
            self.income_failed(cx, request, "Malformed or oversized Phemex income response");
            return;
        }
        let page: Vec<PageRow> = request
            .page
            .iter()
            .map(|row| PageRow {
                timestamp: row.timestamp,
                identity: row.tran_id.clone(),
            })
            .collect();
        let fresh = request.walk.page(request.from, request.to, &page);
        let mut indexes = fresh.into_iter().peekable();
        for (index, row) in request.page.drain(..).enumerate() {
            if indexes.peek() == Some(&index) {
                indexes.next();
                if row.timestamp >= request.start
                    && row.timestamp <= request.end
                    && request
                        .kind
                        .as_deref()
                        .is_none_or(|kind| kind == row.income_type)
                {
                    request.rows.push(row);
                }
            }
        }
        if request.rows.len() > 100_000 {
            self.income_failed(
                cx,
                request,
                "Phemex income result is too large; narrow the range",
            );
            return;
        }
        self.income_next(cx, request);
    }
    pub(crate) fn income_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.income.active.remove(&request.command);
        cx.done(request.command, Err(error.into()));
    }
    pub(crate) fn income_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(index) = self.income.deferred.iter().position(|(id, _)| *id == timer) else {
            return false;
        };
        let (_, request) = self.income.deferred.swap_remove(index);
        self.income_send(cx, request);
        true
    }
}
