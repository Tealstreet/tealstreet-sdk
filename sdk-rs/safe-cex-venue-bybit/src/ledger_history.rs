//! Primary closed/order/income history bodies over the venue's value/Promise capabilities.
use crate::core_snapshot_readers::{Operation, SnapshotHost};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomeCategory {
    Linear,
    Inverse,
}
impl IncomeCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Inverse => "inverse",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomeAccountType {
    Unified,
    Contract,
}
impl IncomeAccountType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unified => "UNIFIED",
            Self::Contract => "CONTRACT",
        }
    }
}
/// BybitIncomeContext: the original XHR and bound logging handle, and the two source unions.
#[derive(Clone, Debug)]
pub struct BybitIncomeContext<X, L> {
    pub xhr: X,
    pub log: L,
    pub category: IncomeCategory,
    pub account_type: IncomeAccountType,
}
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let f = $h.select($owner, $key)?;
        $h.apply(f, $args)?
    }};
}
fn or<H: SnapshotHost>(h: &H, v: H::Value, fallback: impl FnOnce() -> H::Value) -> H::Value {
    if h.truthy(&v) { v } else { fallback() }
}
fn defined<H: SnapshotHost>(h: &H, v: &H::Value) -> bool {
    !h.equal(v, &h.undefined())
}
fn nullish<H: SnapshotHost>(
    h: &H,
    v: H::Value,
    fallback: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.equal(&v, &h.undefined()) || h.equal(&v, &h.intrinsic("null", vec![])?) {
        fallback()
    } else {
        Ok(v)
    }
}
fn template<H: SnapshotHost>(h: &H, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    h.intrinsic("template", args)
}
fn omitted<H: SnapshotHost>(
    h: &H,
    fields: Vec<(&'static str, H::Value)>,
) -> Result<H::Value, H::Error> {
    h.intrinsic("omitUndefined", vec![h.object(fields)])
}
fn list<H: SnapshotHost>(h: &H, data: &H::Value) -> Result<H::Value, H::Error> {
    Ok(or(h, h.optional(&h.get(data, "result")?, "list")?, || {
        h.array(vec![])
    }))
}
fn cursor<H: SnapshotHost>(h: &H, data: &H::Value) -> Result<H::Value, H::Error> {
    Ok(or(
        h,
        h.optional(&h.get(data, "result")?, "nextPageCursor")?,
        || h.undefined(),
    ))
}
fn rejected<H: SnapshotHost>(h: &H, data: &H::Value) -> Result<bool, H::Error> {
    Ok(!h.equal(&h.get(data, "retMsg")?, &h.literal("OK"))
        && h.numeric(&h.get(data, "retCode")?)? != 0.)
}
fn emit<H: SnapshotHost>(h: &H, owner: &H::Value, e: H::Error) -> Result<(), H::Error> {
    let f = h.select(owner.clone(), "emitAndLogError")?;
    let e = h.error_value(e);
    let remote = h.optional(&h.optional(&h.optional(&e, "response")?, "data")?, "retMsg")?;
    let message = if h.truthy(&remote) {
        remote
    } else {
        h.optional(&e, "message")?
    };
    h.apply(f, vec![message])?;
    Ok(())
}
// Iterator primitives implement the standard live iterator protocol and IteratorClose.
// A body throw preserves its original completion even when closing throws.
async fn each<H: SnapshotHost, F>(h: Rc<H>, values: H::Value, mut body: F) -> Result<(), H::Error>
where
    F: FnMut(H::Value) -> Operation<bool, H::Error>,
{
    let iterator = h.intrinsic("iterator", vec![values])?;
    loop {
        let step = h.intrinsic("iterator.next", vec![iterator.clone()])?;
        if h.truthy(&h.get(&step, "done")?) {
            return Ok(());
        }
        let value = h.get(&step, "value")?;
        match body(value).await {
            Ok(true) => {}
            Ok(false) => {
                h.intrinsic("iterator.close", vec![iterator])?;
                return Ok(());
            }
            Err(e) => {
                let _ = h.intrinsic("iterator.close", vec![iterator]);
                return Err(e);
            }
        }
    }
}
fn closed_bounds<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
    opts: &H::Value,
) -> Result<(H::Value, H::Value, H::Value), H::Error> {
    let category = call!(
        h,
        owner.clone(),
        "getCategoryForSymbol",
        vec![h.get(opts, "symbol")?]
    );
    let to = nullish(h, h.get(opts, "to")?, || {
        h.intrinsic("virtualClock.getCurrentTimeMs", vec![])
    })?;
    let from = nullish(h, h.get(opts, "from")?, || {
        Ok(h.number(h.numeric(&h.intrinsic(
            "subtract",
            vec![to.clone(), h.get(owner, "DEFAULT_EXECUTIONS_LOOKBACK_MS")?],
        )?)?))
    })?;
    Ok((category, from, to))
}

pub fn fetch_closed_position_rows<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    opts: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        if h.truthy(&h.get(&owner, "isPublicDataOnly")?) {
            return Ok(h.array(vec![]));
        }
        call!(
            h,
            owner.clone(),
            "refuseOptionSymbol",
            vec![h.get(&opts, "symbol")?, h.literal("closed PnL")]
        );
        let (category, from, to) = closed_bounds(h.as_ref(), &owner, &opts)?;
        let collected = h.array(vec![]);
        let windows = call!(h, owner.clone(), "getExecutionQueryWindows", vec![from, to]);
        let hh = h.clone();
        let output = collected.clone();
        each(h.clone(), windows, move |window| {
            let h = hh.clone();
            let owner = owner.clone();
            let opts = opts.clone();
            let category = category.clone();
            let collected = output.clone();
            Box::pin(async move {
                let mut cursor = h.undefined();
                let mut page = 0;
                loop {
                    let f = h.select(h.get(&owner, "xhr")?, "get")?;
                    let params = omitted(
                        h.as_ref(),
                        vec![
                            ("category", category.clone()),
                            ("symbol", h.get(&opts, "symbol")?),
                            ("startTime", h.get(&window, "startTime")?),
                            ("endTime", h.get(&window, "endTime")?),
                            ("limit", h.number(100.)),
                            ("cursor", cursor),
                        ],
                    )?;
                    let p = h.apply(
                        f,
                        vec![
                            h.literal("/v5/position/closed-pnl"),
                            h.object(vec![("params", params)]),
                        ],
                    )?;
                    let data = h.get(&h.wait(p).await?, "data")?;
                    if rejected(h.as_ref(), &data)? {
                        let message = template(
                            h.as_ref(),
                            vec![
                                h.literal("bybit closed-PnL fetch failed: "),
                                h.get(&data, "retMsg")?,
                            ],
                        )?;
                        return Err(h.error(message)?);
                    }
                    let push = h.select(collected.clone(), "push")?;
                    let rows = list(h.as_ref(), &data)?;
                    let args = h.intrinsic("spreadArgs", vec![rows])?;
                    h.apply(push, h.values(&args)?)?;
                    cursor = crate::ledger_history::cursor(h.as_ref(), &data)?;
                    page += 1;
                    if !h.truthy(&cursor) || page >= 20 {
                        break;
                    }
                }
                Ok(true)
            })
        })
        .await?;
        Ok(collected)
    }))
}
pub fn fetch_historical_positions<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    opts: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let to = nullish(h.as_ref(), h.get(&opts, "to")?, || {
            h.intrinsic("virtualClock.getCurrentTimeMs", vec![])
        })?;
        let from = nullish(h.as_ref(), h.get(&opts, "from")?, || {
            Ok(h.number(h.numeric(&h.intrinsic(
                "subtract",
                vec![to.clone(), h.get(&owner, "DEFAULT_EXECUTIONS_LOOKBACK_MS")?],
            )?)?))
        })?;
        let p = call!(
            h,
            owner.clone(),
            "fetchClosedPositionRows",
            vec![h.object(vec![("from", from.clone()), ("to", to.clone())])]
        );
        let rows = h.wait(p).await?;
        let legs = h.intrinsic("Map", vec![])?;
        let join = async {
            let p = call!(
                h,
                owner.clone(),
                "fetchHistoricalOrders",
                vec![h.object(vec![("from", from), ("to", to)])]
            );
            let orders = h.wait(p).await?;
            let hh = h.clone();
            let ll = legs.clone();
            each(h.clone(), orders, move |order| {
                let h = hh.clone();
                let legs = ll.clone();
                Box::pin(async move {
                    if h.truthy(&h.optional(&order, "id")?)
                        && h.truthy(&h.get(&order, "positionSide")?)
                    {
                        let set = h.select(legs, "set")?;
                        let id = h.intrinsic("String", vec![h.get(&order, "id")?])?;
                        h.apply(set, vec![id, h.get(&order, "positionSide")?])?;
                    }
                    Ok(true)
                })
            })
            .await
        }
        .await;
        if join.is_err() {
            return Err(
                h.error(h.literal("bybit: could not read the order axis to resolve hedge legs"))?
            );
        }
        let aggregate = h.intrinsic("select:aggregateBybitClosedPnl", vec![])?;
        let account = h.get(&opts, "account")?;
        let quote = h.callback(owner.clone(), "history_quote", vec![])?;
        let leg = if h.truthy(&h.get(&legs, "size")?) {
            h.callback(owner, "history_leg", vec![legs])?
        } else {
            h.undefined()
        };
        h.apply(
            aggregate,
            vec![
                rows,
                h.object(vec![
                    ("account", account),
                    ("resolveSettleCurrency", quote),
                    ("resolveLeg", leg),
                ]),
            ],
        )
    }))
}
pub fn fetch_closed_pnl<H: SnapshotHost>(h: Rc<H>, owner: H::Value, opts: H::Value) -> H::Value {
    fetch_closed_pnl_for_category(h, owner, opts, false)
}
pub fn fetch_inverse_closed_pnl<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    opts: H::Value,
) -> H::Value {
    fetch_closed_pnl_for_category(h, owner, opts, true)
}
fn fetch_closed_pnl_for_category<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    opts: H::Value,
    inverse: bool,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        if h.truthy(&h.get(&owner, "isPublicDataOnly")?) {
            return Ok(h.array(vec![]));
        }
        if !inverse {
            call!(
                h,
                owner.clone(),
                "refuseOptionSymbol",
                vec![h.get(&opts, "symbol")?, h.literal("closed PnL")]
            );
        }
        let (category, from, to) = if inverse {
            let to = nullish(h.as_ref(), h.get(&opts, "to")?, || {
                h.intrinsic("virtualClock.getCurrentTimeMs", vec![])
            })?;
            let from = nullish(h.as_ref(), h.get(&opts, "from")?, || {
                h.intrinsic(
                    "subtract",
                    vec![to.clone(), h.get(&owner, "DEFAULT_EXECUTIONS_LOOKBACK_MS")?],
                )
            })?;
            (h.literal("inverse"), from, to)
        } else {
            closed_bounds(h.as_ref(), &owner, &opts)?
        };
        let entries = h.array(vec![]);
        let result = async {
            let windows = call!(h, owner.clone(), "getExecutionQueryWindows", vec![from, to]);
            let hh = h.clone();
            let oo = owner.clone();
            let pp = opts.clone();
            let ee = entries.clone();
            each(h.clone(), windows, move |window| {
                let h = hh.clone();
                let owner = oo.clone();
                let opts = pp.clone();
                let category = category.clone();
                let entries = ee.clone();
                Box::pin(async move {
                    let mut cursor = h.undefined();
                    let mut page = 0;
                    loop {
                        let f = h.select(h.get(&owner, "xhr")?, "get")?;
                        let params = omitted(
                            h.as_ref(),
                            vec![
                                ("category", category.clone()),
                                ("symbol", h.get(&opts, "symbol")?),
                                ("startTime", h.get(&window, "startTime")?),
                                ("endTime", h.get(&window, "endTime")?),
                                ("limit", h.number(100.)),
                                ("cursor", cursor),
                            ],
                        )?;
                        let p = h.apply(
                            f,
                            vec![
                                h.literal("/v5/position/closed-pnl"),
                                h.object(vec![("params", params)]),
                            ],
                        )?;
                        let data = h.get(&h.wait(p).await?, "data")?;
                        if rejected(h.as_ref(), &data)? {
                            let log = h.select(owner.clone(), "log")?;
                            let message = template(
                                h.as_ref(),
                                vec![
                                    h.literal("Failed to fetch closed PnL: "),
                                    h.get(&data, "retMsg")?,
                                ],
                            )?;
                            h.apply(log, vec![message, h.literal("error")])?;
                            break;
                        }
                        let rows = list(h.as_ref(), &data)?;
                        let hh = h.clone();
                        let pp = opts.clone();
                        let ee = entries.clone();
                        each(h.clone(), rows, move |row| {
                            let h = hh.clone();
                            let opts = pp.clone();
                            let entries = ee.clone();
                            Box::pin(async move {
                                let mapper = h.intrinsic("select:mapBybitClosedPnl", vec![])?;
                                let account =
                                    or(h.as_ref(), h.get(&opts, "accountName")?, || h.literal(""));
                                let inverse = if inverse {
                                    h.boolean(true)
                                } else {
                                    h.intrinsic(
                                        "isBybitInverseSymbol",
                                        vec![h.get(&row, "symbol")?],
                                    )?
                                };
                                let entry = h.apply(mapper, vec![row, account, inverse])?;
                                if h.truthy(&entry) {
                                    call!(h, entries, "push", vec![entry]);
                                }
                                Ok(true)
                            })
                        })
                        .await?;
                        cursor = crate::ledger_history::cursor(h.as_ref(), &data)?;
                        page += 1;
                        if h.truthy(&cursor) && page >= 20 {
                            let log = h.select(owner.clone(), "log")?;
                            let message = template(
                                h.as_ref(),
                                vec![
                                    h.literal("Closed-PnL paging hit the 20-page cap for "),
                                    h.get(&opts, "symbol")?,
                                    h.literal("; older rows in this window were not fetched"),
                                ],
                            )?;
                            h.apply(log, vec![message, h.literal("warning")])?;
                        }
                        if !h.truthy(&cursor) || page >= 20 {
                            break;
                        }
                    }
                    Ok(true)
                })
            })
            .await?;
            let sort = h.select(entries.clone(), "sort")?;
            let compare = h.callback(owner.clone(), "history_closed_sort", vec![])?;
            h.apply(sort, vec![compare])
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                emit(h.as_ref(), &owner, e)?;
                Ok(h.array(vec![]))
            }
        }
    }))
}
fn windows<H: SnapshotHost>(
    h: &H,
    opts: &H::Value,
    from: &'static str,
    to: &'static str,
) -> Result<H::Value, H::Error> {
    if defined(h, &h.optional(opts, from)?) && defined(h, &h.optional(opts, to)?) {
        let split = h.intrinsic("select:splitTimeRange", vec![])?;
        h.apply(
            split,
            vec![h.get(opts, from)?, h.get(opts, to)?, h.number(604799999.)],
        )
    } else {
        Ok(h.array(vec![h.object(vec![
            ("startTime", h.optional(opts, from)?),
            ("endTime", h.optional(opts, to)?),
        ])]))
    }
}
pub fn fetch_historical_orders<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    opts: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move{
        if h.truthy(&h.get(&owner,"isPublicDataOnly")?){return Ok(h.array(vec![]))}
        let result=async{
            let orders=h.array(vec![]);let seen=h.intrinsic("Set",vec![])?;let page=Rc::new(Cell::new(0));
            let values=windows(h.as_ref(),&opts,"from","to")?;
            // ts: a symbol queries its own category; otherwise linear, plus inverse when enabled.
            let categories=if h.truthy(&h.optional(&opts,"symbol")?){vec![call!(h,owner.clone(),"getCategoryForSymbol",vec![h.get(&opts,"symbol")?])]}else if h.truthy(&call!(h,owner.clone(),"inverseEnabled",vec![])){vec![h.literal("linear"),h.literal("inverse")]}else{vec![h.literal("linear")]};
            for category in categories{let values=values.clone();let seen=seen.clone();let hh=h.clone();let oo=owner.clone();let pp=opts.clone();let orders2=orders.clone();let page2=page.clone();
            each(h.clone(),values,move|window|{let h=hh.clone();let owner=oo.clone();let opts=pp.clone();let orders=orders2.clone();let seen=seen.clone();let page=page2.clone();let category=category.clone();Box::pin(async move{
                let mut cursor=h.undefined();page.set(0);
                while page.get()<20{
                    let params=omitted(h.as_ref(),vec![("category",category.clone()),("symbol",h.optional(&opts,"symbol")?),("startTime",h.get(&window,"startTime")?),("endTime",h.get(&window,"endTime")?),("limit",h.number(50.)),("cursor",cursor)])?;
                    let p=call!(h,h.get(&owner,"xhr")?,"get",vec![h.literal("/v5/order/history"),h.object(vec![("params",params)])]);let data=h.get(&h.wait(p).await?,"data")?;
                    if rejected(h.as_ref(),&data)?{let log=h.select(owner.clone(),"log")?;let message=template(h.as_ref(),vec![h.literal("Failed to fetch order history: "),h.get(&data,"retMsg")?])?;h.apply(log,vec![message,h.literal("error")])?;break}
                    let rows=list(h.as_ref(),&data)?;if h.equal(&h.get(&rows,"length")?,&h.number(0.)){break}
                    let hh=h.clone();let ss=seen.clone();let oo=orders.clone();
                    each(h.clone(),rows,move|row|{let h=hh.clone();let seen=ss.clone();let orders=oo.clone();Box::pin(async move{
                        let mapper=h.intrinsic("select:mapBybitHistoricalOrder",vec![])?;let order=h.apply(mapper,vec![row])?;
                        if !h.truthy(&order)||h.truthy(&call!(h,seen.clone(),"has",vec![h.get(&order,"id")?])){return Ok(true)}
                        call!(h,seen,"add",vec![h.get(&order,"id")?]);call!(h,orders,"push",vec![order]);Ok(true)
                    })}).await?;
                    cursor=crate::ledger_history::cursor(h.as_ref(),&data)?;if !h.truthy(&cursor){break}page.set(page.get()+1);
                }Ok(true)
            })}).await?;}
            if page.get()>=20{call!(h,owner.clone(),"log",vec![h.literal("fetchHistoricalOrders stopped at the 20-page cap; older orders were not fetched"),h.literal("warning")]);}
            let sort=h.select(orders,"sort")?;let compare=h.callback(owner.clone(),"history_order_sort",vec![])?;let sorted=h.apply(sort,vec![compare])?;
            let slice=h.select(sorted,"slice")?;
            let end=if h.truthy(&h.optional(&opts,"limit")?)&&h.numeric(&h.get(&opts,"limit")?)?>0.{h.get(&opts,"limit")?}else{h.undefined()};h.apply(slice,vec![h.number(0.),end])
        }.await;match result{Ok(v)=>Ok(v),Err(e)=>{emit(h.as_ref(),&owner,e)?;Ok(h.array(vec![]))}}
    }))
}
// ts: BybitExchange#refuseOptionSymbol
pub fn refuse_option_symbol<H: SnapshotHost>(
    h: &H,
    symbol: &H::Value,
    what: &H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(symbol) && h.truthy(&h.intrinsic("isBybitOptionSymbol", vec![symbol.clone()])?) {
        let message = template(
            h,
            vec![
                h.literal("Bybit has no "),
                what.clone(),
                h.literal(" for options ("),
                symbol.clone(),
                h.literal(")"),
            ],
        )?;
        return Err(h.error(message)?);
    }
    Ok(h.undefined())
}
// ts: BybitExchange#inverseEnabled
pub fn inverse_enabled<H: SnapshotHost>(h: &H, owner: &H::Value) -> Result<H::Value, H::Error> {
    let options = h.get(owner, "options")?;
    Ok(h.boolean(
        h.truthy(&h.get(&options, "enableInverseMarkets")?)
            && !h.truthy(&h.get(&options, "externalPublicData")?),
    ))
}
// ts: BybitExchange.get incomeContexts — one context per traded category.
pub fn income_contexts<H: SnapshotHost>(h: &H, owner: &H::Value) -> Result<H::Value, H::Error> {
    let categories = if h.truthy(&call!(h, owner.clone(), "inverseEnabled", vec![])) {
        vec![IncomeCategory::Linear, IncomeCategory::Inverse]
    } else {
        vec![IncomeCategory::Linear]
    };
    let mut contexts = Vec::new();
    for category in categories {
        let ctx = BybitIncomeContext {
            xhr: h.get(owner, "xhr")?,
            log: h.get(owner, "log")?,
            category,
            account_type: IncomeAccountType::Unified,
        };
        contexts.push(h.object(vec![
            ("xhr", ctx.xhr),
            ("log", ctx.log),
            ("category", h.literal(ctx.category.as_str())),
            ("accountType", h.literal(ctx.account_type.as_str())),
        ]));
    }
    Ok(h.array(contexts))
}
// mapSeries over the contexts, then .flat().
async fn each_context<H: SnapshotHost>(
    h: &Rc<H>,
    owner: &H::Value,
    select: &str,
    args: impl Fn() -> Vec<H::Value>,
) -> Result<Vec<H::Value>, H::Error> {
    let contexts = h.get(owner, "incomeContexts")?;
    let mut records = Vec::new();
    for ctx in h.values(&contexts)? {
        // p-iteration awaits each element before invoking the callback.
        let ctx = h.wait(ctx).await?;
        let f = h.intrinsic(select, vec![])?;
        let mut call_args = vec![ctx];
        call_args.extend(args());
        let rows = h.wait(h.apply(f, call_args)?).await?;
        records.extend(h.values(&rows)?);
    }
    Ok(records)
}
pub fn fetch_income_history<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    opts: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let records = each_context(&h, &owner, "select:fetchBybitIncomeHistory", || {
            vec![opts.clone()]
        })
        .await?;
        let records = h.array(records);
        if !h.truthy(&call!(h, owner.clone(), "inverseEnabled", vec![])) {
            return Ok(records);
        }
        let sort = h.select(records, "sort")?;
        let compare = h.callback(owner.clone(), "history_income_sort", vec![])?;
        let newest_first = h.apply(sort, vec![compare])?;
        if h.truthy(&h.optional(&opts, "limit")?) {
            let slice = h.select(newest_first, "slice")?;
            return h.apply(slice, vec![h.number(0.), h.get(&opts, "limit")?]);
        }
        Ok(newest_first)
    }))
}
pub fn fetch_income_history_range<H: SnapshotHost>(
    h: Rc<H>,
    owner: H::Value,
    start: H::Value,
    end: H::Value,
    income_type: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let records = each_context(&h, &owner, "select:fetchBybitIncomeHistoryRange", || {
            vec![start.clone(), end.clone(), income_type.clone()]
        })
        .await?;
        Ok(h.array(records))
    }))
}
pub fn fetch_bybit_income_history<H: SnapshotHost>(
    h: Rc<H>,
    ctx: H::Value,
    opts: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let result = async {
            let account = h.get(&ctx, "accountType")?;
            let category = h.get(&ctx, "category")?;
            let limit = or(h.as_ref(), h.optional(&opts, "limit")?, || h.number(50.));
            let cap = h.intrinsic("Math.min", vec![limit, h.number(50.)])?;
            let params = h.object(vec![
                ("accountType", account),
                ("category", category),
                ("limit", cap),
            ]);
            if h.truthy(&h.optional(&opts, "incomeType")?) {
                let type_map = h.object(vec![
                    ("REALIZED_PNL", h.literal("TRADE")),
                    ("FUNDING_FEE", h.literal("SETTLEMENT")),
                    ("COMMISSION", h.literal("TRADE")),
                ]);
                let mapped = h.intrinsic("get-key", vec![type_map, h.get(&opts, "incomeType")?])?;
                let kind = if h.truthy(&mapped) {
                    mapped
                } else {
                    h.get(&opts, "incomeType")?
                };
                h.set(&params, "type", kind)?;
            }
            let records = h.array(vec![]);
            let record_cap = or(h.as_ref(), h.optional(&opts, "limit")?, || h.number(1000.));
            let values = windows(h.as_ref(), &opts, "startTime", "endTime")?;
            let hh = h.clone();
            let ctx2 = ctx.clone();
            let pp = params.clone();
            let rr = records.clone();
            each(h.clone(), values, move |window| {
                let h = hh.clone();
                let ctx = ctx2.clone();
                let params = pp.clone();
                let records = rr.clone();
                let record_cap = record_cap.clone();
                Box::pin(async move {
                    let mut cursor = h.undefined();
                    let mut page = 0;
                    loop {
                        let spread = h.spread(params.clone(), vec![])?;
                        h.set(&spread, "startTime", h.get(&window, "startTime")?)?;
                        h.set(&spread, "endTime", h.get(&window, "endTime")?)?;
                        h.set(&spread, "cursor", cursor)?;
                        let request = h.intrinsic("omitUndefined", vec![spread])?;
                        let p = call!(
                            h,
                            h.get(&ctx, "xhr")?,
                            "get",
                            vec![
                                h.literal("/v5/account/transaction-log"),
                                h.object(vec![("params", request)])
                            ]
                        );
                        let data = h.get(&h.wait(p).await?, "data")?;
                        if rejected(h.as_ref(), &data)? {
                            let log = h.select(ctx.clone(), "log")?;
                            let message = template(
                                h.as_ref(),
                                vec![
                                    h.literal("Failed to fetch transaction log: "),
                                    h.get(&data, "retMsg")?,
                                ],
                            )?;
                            h.apply(log, vec![message, h.literal("error")])?;
                            break;
                        }
                        let rows = h.optional(&h.get(&data, "result")?, "list")?;
                        if !h.truthy(&rows) || h.equal(&h.get(&rows, "length")?, &h.number(0.)) {
                            break;
                        }
                        let hh = h.clone();
                        let rr = records.clone();
                        each(h.clone(), rows, move |row| {
                            let h = hh.clone();
                            let records = rr.clone();
                            Box::pin(async move {
                                let push = h.select(records, "push")?;
                                let mapper = h.intrinsic(
                                    "select:mapBybitTransactionToIncomeRecords",
                                    vec![],
                                )?;
                                let mapped = h.apply(mapper, vec![row])?;
                                let args = h.intrinsic("spreadArgs", vec![mapped])?;
                                h.apply(push, h.values(&args)?)?;
                                Ok(true)
                            })
                        })
                        .await?;
                        cursor = h.optional(&h.get(&data, "result")?, "nextPageCursor")?;
                        page += 1;
                        // JS `!(records.length < recordCap)`: h.numeric coerces,
                        // so NaN is reachable and must take the break branch.
                        // clippy's `>=` rewrite inverts exactly that case.
                        #[allow(clippy::neg_cmp_op_on_partial_ord)]
                        if !h.truthy(&cursor)
                            || page >= 10
                            || !(h.numeric(&h.get(&records, "length")?)?
                                < h.numeric(&record_cap)?)
                        {
                            break;
                        }
                    }
                    #[allow(clippy::neg_cmp_op_on_partial_ord)]
                    let done =
                        !(h.numeric(&h.get(&records, "length")?)? >= h.numeric(&record_cap)?);
                    Ok(done)
                })
            })
            .await?;
            Ok(records)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let log = h.select(ctx, "log")?;
                let message = template(
                    h.as_ref(),
                    vec![
                        h.literal("Error fetching income history: "),
                        h.get(&h.error_value(e), "message")?,
                    ],
                )?;
                h.apply(log, vec![message, h.literal("error")])?;
                Ok(h.array(vec![]))
            }
        }
    }))
}
pub fn fetch_bybit_income_history_range<H: SnapshotHost>(
    h: Rc<H>,
    ctx: H::Value,
    start: H::Value,
    end: H::Value,
    income_type: H::Value,
) -> H::Value {
    let launcher = h.clone();
    launcher.launch(Box::pin(async move {
        let records = h.array(vec![]);
        let mut current = start;
        while h.truthy(&h.intrinsic("less", vec![current.clone(), end.clone()])?) {
            let plus = h.intrinsic("add", vec![current.clone(), h.number(604800000.)])?;
            let current_end = h.intrinsic("Math.min", vec![plus, end.clone()])?;
            let f = h.intrinsic("select:fetchBybitIncomeHistory", vec![])?;
            let p = h.apply(
                f,
                vec![
                    ctx.clone(),
                    h.object(vec![
                        ("startTime", current),
                        ("endTime", current_end.clone()),
                        ("incomeType", income_type.clone()),
                        ("limit", h.number(1000.)),
                    ]),
                ],
            )?;
            let rows = h.wait(p).await?;
            let push = h.select(records.clone(), "push")?;
            let args = h.intrinsic("spreadArgs", vec![rows])?;
            h.apply(push, h.values(&args)?)?;
            current = h.intrinsic("add", vec![current_end, h.number(1.)])?;
            let f = h.intrinsic("select:sleep", vec![])?;
            let p = h.apply(f, vec![h.number(100.)])?;
            h.wait(p).await?;
        }
        let sort = h.select(records, "sort")?;
        let compare = h.callback(ctx, "history_income_sort", vec![])?;
        h.apply(sort, vec![compare])
    }))
}
/// Source lexical callbacks, including live quote and hedge-leg lookup.
pub fn dispatch<H: SnapshotHost>(
    h: &H,
    owner: &H::Value,
    code: &str,
    captures: &[H::Value],
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    match code {
        "history_quote" => {
            let store = h.get(owner, "store")?;
            let markets = h.optional(&store, "marketsBySymbol")?;
            let market = if h.equal(&markets, &h.undefined())
                || h.equal(&markets, &h.intrinsic("null", vec![])?)
            {
                h.undefined()
            } else {
                h.intrinsic("get-key", vec![markets, args[0].clone()])?
            };
            Ok(or(h, h.optional(&market, "quote")?, || h.literal("USDT")))
        }
        "history_leg" => Ok(call!(h, captures[0].clone(), "get", vec![args[0].clone()])),
        "history_closed_sort" | "history_order_sort" | "history_income_sort" => {
            let key = match code {
                "history_closed_sort" => "closedTime",
                "history_order_sort" => "createdTime",
                _ => "timestamp",
            };
            let rhs = h.get(&args[1], key)?;
            let lhs = h.get(&args[0], key)?;
            h.intrinsic("subtract", vec![rhs, lhs])
        }
        _ => panic!("unknown ledger-history callback"),
    }
}
