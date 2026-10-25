//! Reported account ledger amounts, never a fold of executions or balances.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::history::{ReadCompletion, ReadRangeHistory};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::request_context::Owner;
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, VenueError, json::ArrayAt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const DAY: f64 = 86_400_000.;
const CURRENCIES: [&str; 2] = ["USDT", "USDC"];
const PAGE: usize = 200;

#[derive(Default)]
pub struct State {
    active: HashSet<CommandId>,
    deferred: Vec<(TimerId, Request)>,
    owners: HashMap<CommandId, Owner>,
}
pub struct Request {
    root: ReadRoot,
    leg: Option<ReadLeg>,
    owner: Box<ReadRangeHistory<Income>>,
    start: f64,
    end: f64,
    currency: usize,
    kind: Option<String>,
    limit: Option<usize>,
    from: f64,
    to: f64,
    page_rows: usize,
    occurrences: HashMap<String, u32>,
    invalid: bool,
}
impl Request {
    pub(crate) fn validate_delivery(
        &self,
        cx: &Cx<'_, Pending>,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        self.root.validate_delivery(
            cx,
            self.leg
                .as_ref()
                .ok_or(safe_cex_engine::request_context::Error::Revoked)?,
        )
    }
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
            Self::finish_local_history(cx, Err("Phemex private history requires credentials"));
            return;
        }
        // This route is an account ledger; its documented rows have no symbol.
        if symbol.is_some() {
            Self::finish_local_history(
                cx,
                Err("Phemex account income cannot be filtered by symbol"),
            );
            return;
        }
        let end = to_ms.unwrap_or(cx.now.floor());
        let start = from_ms.unwrap_or((end - 30. * DAY).max(0.));
        if !valid_time(start) || !valid_time(end) || start > end || limit == Some(0) {
            Self::finish_local_history(cx, Err("Invalid Phemex income range or limit"));
            return;
        }
        if self.income.active.len() >= 16 || self.income.active.contains(&command) {
            Self::finish_local_history(cx, Err("Too many active Phemex income queries"));
            return;
        }
        let Ok(root) = SelectedRead::command_root(
            cx,
            self.selected_read(ReadRole::Read { scope: None }),
            LocalRefusal {
                diagnostic: None,
                error: VenueError::new("Phemex private history requires credentials"),
            },
        ) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        self.income.owners.insert(command, root.context().owner());
        self.income.active.insert(command);
        let request = Request {
            root,
            leg: None,
            owner: Box::new(ReadRangeHistory::new(
                completion,
                RangeWalk::new(start, end.max(start + 1.), 7. * DAY, PAGE),
                |_| "Phemex income range is incomplete".into(),
                |rows| {
                    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
                },
            )),
            start,
            end,
            currency: 0,
            kind,
            limit: limit.map(|n| n as usize),
            from: start,
            to: end,
            page_rows: 0,
            occurrences: HashMap::new(),
            invalid: false,
        };
        self.income_next(cx, request);
    }
    fn income_next(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if cx
            .validate_read_root_context(request.root.context())
            .is_err()
        {
            return;
        }
        loop {
            match request.owner.step() {
                Step::Fetch { from, to } => {
                    request.from = from;
                    request.to = to.min(request.end);
                    if !request
                        .owner
                        .restrict_active_range(request.from, request.to)
                    {
                        self.retire_income(request.root.context().owner());
                        let _ = request
                            .owner
                            .reject(cx, VenueError::new("Invalid Phemex income request bounds"));
                        return;
                    }
                    request.page_rows = 0;
                    request.occurrences.clear();
                    self.income_send(cx, request);
                    return;
                }
                Step::ChunkDone => {}
                Step::Finished => {
                    request.currency += 1;
                    if request.currency < CURRENCIES.len() {
                        if !request.owner.restart_stage(RangeWalk::new(
                            request.start,
                            request.end.max(request.start + 1.),
                            7. * DAY,
                            PAGE,
                        )) {
                            self.retire_income(request.root.context().owner());
                            let _ = request.owner.reject(
                                cx,
                                VenueError::new("Phemex income stage did not complete"),
                            );
                            return;
                        }
                        continue;
                    }
                    self.income_finish(cx, request, Ok(()));
                    return;
                }
            }
        }
    }
    fn income_send(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if cx
            .validate_read_root_context(request.root.context())
            .is_err()
        {
            return;
        }
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.income.deferred.push((timer, request));
            return;
        }
        let currency = CURRENCIES[request.currency];
        let from = request.from;
        let to = request.to;
        let query =
            format!("currency={currency}&start={from:.0}&end={to:.0}&offset=0&limit={PAGE}");
        let leg = match request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                self.income_failed(cx, request, error.read_venue_error());
                return;
            }
        };
        let path = "/api-data/futures/v2/tradeAccountDetail";
        let testnet = self.options.testnet;
        let url = format!("{}{path}?{query}", self.rest());
        if let Err((Pending::Income(request), error)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(ArrayAt::Root),
            },
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    testnet,
                    Method::Get,
                    path.into(),
                    query,
                    None,
                ))
            },
            |leg| {
                request.leg = Some(leg);
                Pending::Income(request)
            },
        ) {
            self.income_failed(cx, request, error.read_venue_error());
        }
    }
    pub(crate) fn income_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if request.invalid || request.validate_delivery(cx).is_err() {
            return;
        }
        let Ok(row) = serde_json::from_str::<Row<'_>>(item) else {
            request.invalid = true;
            return;
        };
        let amount = serde_json::from_str::<crate::wire::N>(row.amount_rv.get()).map(|n| n.0);
        let balance = serde_json::from_str::<crate::wire::N>(row.balance_rv.get());
        if request.page_rows >= PAGE
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
        request.page_rows += 1;
        let row = Income {
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
        };
        let facts = PageRow {
            timestamp: row.timestamp,
            identity: row.tran_id.clone(),
        };
        if let Ok(mut page) = request
            .owner
            .admit(cx, request.leg.as_ref().unwrap().context())
        {
            page.stage(row, facts);
        }
    }
    pub(crate) fn income_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if request.validate_delivery(cx).is_err() {
            return;
        }
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
        let start = request.start;
        let end = request.end;
        let kind = request.kind.as_deref();
        let Ok(mut page) = request
            .owner
            .admit(cx, request.leg.as_ref().unwrap().context())
        else {
            return;
        };
        page.accept_page_where(|row| {
            row.timestamp >= start
                && row.timestamp <= end
                && kind.is_none_or(|kind| kind == row.income_type)
        });
        if request.owner.accepted_len() > 100_000 {
            self.retire_income(request.root.context().owner());
            let _ = request.owner.reject(
                cx,
                VenueError::new("Phemex income result is too large; narrow the range"),
            );
            return;
        }
        if let Some(leg) = request.leg.take()
            && cx.finish_http_context(leg.context()).is_err()
        {
            return;
        }
        self.income_next(cx, request);
    }
    pub(crate) fn income_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: impl Into<VenueError>,
    ) {
        self.income_finish(cx, request, Err(error.into()));
    }
    pub(crate) fn retire_income(&mut self, owner: Owner) {
        self.income
            .deferred
            .retain(|(_, read)| read.root.context().owner() != owner);
        let ids: Vec<_> = self
            .income
            .owners
            .iter()
            .filter_map(|(id, current)| (*current == owner).then_some(*id))
            .collect();
        for id in ids {
            self.income.owners.remove(&id);
            self.income.active.remove(&id);
        }
    }
    fn income_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        result: Result<(), VenueError>,
    ) {
        if let Some(leg) = &request.leg {
            let _ = cx.finish_http_context(leg.context());
        }
        if cx
            .validate_read_root_context(request.root.context())
            .is_err()
        {
            return;
        }
        self.retire_income(request.root.context().owner());
        let limit = request.limit;
        let _ = request.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| {
                b.timestamp
                    .total_cmp(&a.timestamp)
                    .then_with(|| a.tran_id.cmp(&b.tran_id))
            });
            if let Some(limit) = limit {
                rows.truncate(limit);
            }
            serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
        });
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
