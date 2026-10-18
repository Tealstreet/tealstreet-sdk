//! Inverse balance, paged order, market and income read bodies.
use crate::{
    hyro_private_session::SessionFuture, variant_account_refresh::AccountRefreshHost as Host,
};
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn eager<'a, V: 'a, E: 'a>(mut future: SessionFuture<'a, V, E>) -> SessionFuture<'a, V, E> {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => Box::pin(async move { value }),
        std::task::Poll::Pending => future,
    }
}
fn endpoint<H: Host>(h: &H, key: &str) -> Result<H::Value, H::Error> {
    h.get(&h.intrinsic("ENDPOINTS", vec![])?, key)
}
fn value<H: Host>(h: &H, data: H::Value, key: &str) -> Result<H::Value, H::Error> {
    Ok(call!(h, h.global(), "v", vec![data, h.literal(key)]))
}
fn parse<H: Host>(h: &H, owner: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    let parse = h.select(h.global(), "parseFloat")?;
    h.apply(parse, vec![h.get(owner, key)?])
}
fn spread<H: Host>(h: &H, value: H::Value) -> Result<Vec<H::Value>, H::Error> {
    let iterator = h.intrinsic("iterator", vec![value])?;
    let step = h.select(iterator, "next")?;
    let mut out = vec![];
    loop {
        let result = h.apply(step.clone(), vec![])?;
        if h.truthy(&h.get(&result, "done")?) {
            return Ok(out);
        }
        out.push(h.get(&result, "value")?);
    }
}
/// Closed set of lexical callbacks created by this module and selected by the native host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    InversePnl,
    InverseCoin,
    InverseNullMarket,
}
impl Callback {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "inverse-pnl" => Some(Self::InversePnl),
            "inverse-coin" => Some(Self::InverseCoin),
            "inverse-null-market" => Some(Self::InverseNullMarket),
            _ => None,
        }
    }
}

pub fn dispatch<H: Host>(h: &H, code: Callback, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    match code {
        Callback::InversePnl => h.intrinsic(
            "addition",
            vec![args[0].clone(), h.get(&args[1], "unrealizedPnl")?],
        ),
        Callback::InverseCoin => {
            let prev = args[0].clone();
            let coin = &args[1];
            let key = h.intrinsic("property key", vec![h.get(coin, "coin")?])?;
            let next = h.object(vec![
                ("base", h.get(coin, "coin")?),
                ("amount", parse(h, coin, "equity")?),
                ("notional", parse(h, coin, "usdValue")?),
            ]);
            h.intrinsic("computed assignment", vec![prev.clone(), key, next])?;
            Ok(prev)
        }
        Callback::InverseNullMarket => {
            Ok(h.boolean(h.equal(&args[0], &h.intrinsic("null", vec![])?)))
        }
    }
}
pub fn cached_api_key_info<H: Host>(h: &H, owner: &H::Value) -> Result<H::Value, H::Error> {
    h.get(owner, "apiKeyInfo")
}
pub fn income_context<H: Host>(h: &H, owner: &H::Value) -> Result<H::Value, H::Error> {
    Ok(h.object(vec![
        ("xhr", h.get(owner, "xhr")?),
        ("log", h.get(owner, "log")?),
        ("category", h.literal("inverse")),
        ("accountType", h.literal("UNIFIED")),
    ]))
}
pub fn fetch_income_history<H: Host>(
    h: &H,
    owner: H::Value,
    opts: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        let fetch = h.select(h.global(), "fetchBybitIncomeHistory")?;
        let context = h.get(&owner, "incomeContext")?;
        h.apply(fetch, vec![context, opts])
    })();
    eager(Box::pin(async move { h.wait(request?).await }))
}
pub fn fetch_income_history_range<H: Host>(
    h: &H,
    owner: H::Value,
    start: H::Value,
    end: H::Value,
    kind: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    let request = (|| {
        let fetch = h.select(h.global(), "fetchBybitIncomeHistoryRange")?;
        let context = h.get(&owner, "incomeContext")?;
        h.apply(fetch, vec![context, start, end, kind])
    })();
    eager(Box::pin(async move { h.wait(request?).await }))
}
pub fn fetch_raw_balance<H: Host>(h: &H, owner: H::Value) -> SessionFuture<'_, H::Value, H::Error> {
    eager(Box::pin(async move {
        let get = h.select(h.get(&owner, "xhr")?, "get")?;
        let endpoint = endpoint(h, "BALANCE")?;
        let account = if h.truthy(&h.get(&owner, "forceUnifiedBalance")?) {
            h.literal("UNIFIED")
        } else {
            h.get(&owner, "accountType")?
        };
        let request = h.apply(
            get,
            vec![
                endpoint,
                h.object(vec![("params", h.object(vec![("accountType", account)]))]),
            ],
        )?;
        let response = h.wait(request).await?;
        let data = h.get(&response, "data")?;
        if !h.equal(&value(h, data.clone(), "retMsg")?, &h.literal("OK"))
            && h.numeric(&value(h, data.clone(), "retCode")?)? != 0.0
        {
            if h.equal(
                &value(h, data.clone(), "retMsg")?,
                &h.literal("accountType only support UNIFIED."),
            ) && !h.truthy(&h.get(&owner, "forceUnifiedBalance")?)
            {
                h.set(&owner, "forceUnifiedBalance", h.boolean(true))?;
                h.set(&owner, "unifiedMarginStatus", h.number(6.0))?;
                let retry = call!(h, owner.clone(), "fetchRawBalance", vec![]);
                return h.wait(retry).await;
            }
            let emit = h.select(owner.clone(), "emitAndLogError")?;
            h.apply(emit, vec![value(h, data, "retMsg")?])?;
            let info = h.optional(&h.get(&h.get(&owner, "store")?, "balance")?, "info")?;
            return Ok(if h.truthy(&info) {
                info
            } else {
                h.array(vec![])
            });
        }
        h.get(&h.get(&data, "result")?, "list")
    }))
}
pub fn format_balance<H: Host>(
    h: &H,
    owner: H::Value,
    raw: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&h.intrinsic("Array.isArray", vec![raw.clone()])?)
        || h.equal(&h.get(&raw, "length")?, &h.number(0.0))
    {
        let log = h.select(owner.clone(), "log")?;
        let warning = h.get(&h.intrinsic("LogSeverity", vec![])?, "Warning")?;
        h.apply(
            log,
            vec![h.literal("Invalid or empty balance data received"), warning],
        )?;
        return h.get(&h.get(&owner, "store")?, "balance");
    }
    let first = h.intrinsic("destructure first", vec![raw])?;
    let reduce = h.select(h.get(&h.get(&owner, "store")?, "positions")?, "reduce")?;
    let upnl = h.apply(
        reduce,
        vec![
            h.callback(owner.clone(), "inverse-pnl", vec![])?,
            h.number(0.0),
        ],
    )?;
    let subtract = h.select(h.global(), "subtract")?;
    let total = h.apply(
        subtract,
        vec![parse(h, &first, "totalEquity")?, upnl.clone()],
    )?;
    let used = h.intrinsic(
        "addition",
        vec![
            parse(h, &first, "totalMaintenanceMargin")?,
            parse(h, &first, "totalInitialMargin")?,
        ],
    )?;
    let parse_float = h.select(h.global(), "parseFloat")?;
    let available = h.get(&first, "totalAvailableBalance")?;
    let available = if h.equal(&available, &h.undefined())
        || h.equal(&available, &h.intrinsic("null", vec![])?)
    {
        h.get(&first, "totalMarginBalance")?
    } else {
        available
    };
    let free = h.apply(parse_float, vec![available])?;
    let reduce = h.select(h.get(&first, "coin")?, "reduce")?;
    let coins = h.apply(
        reduce,
        vec![h.callback(owner, "inverse-coin", vec![])?, h.object(vec![])],
    )?;
    Ok(h.object(vec![
        ("total", total),
        ("upnl", upnl),
        ("used", used),
        ("free", free),
        ("coins", coins),
        ("info", first),
    ]))
}
pub fn fetch_orders_helper<H: Host>(
    h: &H,
    owner: H::Value,
    category: H::Value,
    settle: H::Value,
) -> SessionFuture<'_, H::Value, H::Error> {
    eager(Box::pin(async move {
        let mut cursor = h.literal("");
        let mut orders = h.array(vec![]);
        let rows = loop {
            let reinit = h.select(owner.clone(), "reinitOnThrow")?;
            let get = h.select(h.get(&owner, "xhr")?, "get")?;
            let endpoint = endpoint(h, "UNFILLED_ORDERS")?;
            let params = h.object(vec![
                ("category", category.clone()),
                ("settleCoin", settle.clone()),
                ("limit", h.number(50.0)),
                ("cursor", cursor),
            ]);
            let request = h.apply(get, vec![endpoint, h.object(vec![("params", params)])])?;
            let request = h.apply(reinit, vec![request])?;
            let response = h.wait(request).await?;
            let data = h.get(&response, "data")?;
            if !h.equal(&value(h, data.clone(), "retMsg")?, &h.literal("OK")) {
                let emit = h.select(owner.clone(), "emitAndLogError")?;
                h.apply(emit, vec![value(h, data.clone(), "retMsg")?])?;
                h.set(&owner, "needsReinit", h.boolean(true))?;
                return Err(h.error(value(h, data, "retMsg")?)?);
            }
            let list = if h.truthy(&h.intrinsic(
                "Array.isArray",
                vec![h.optional(&h.optional(&data, "result")?, "list")?],
            )?) {
                h.get(&h.get(&data, "result")?, "list")?
            } else {
                h.array(vec![])
            };
            if h.equal(&h.get(&list, "length")?, &h.number(0.0)) {
                break orders;
            }
            let following = h.get(&h.get(&data, "result")?, "nextPageCursor")?;
            if h.truthy(&following) {
                cursor = h.get(&h.get(&data, "result")?, "nextPageCursor")?;
                let mut accumulated = spread(h, orders)?;
                accumulated.extend(spread(h, list)?);
                orders = h.array(accumulated);
                continue;
            }
            let mut accumulated = spread(h, orders)?;
            accumulated.extend(spread(h, list)?);
            break h.array(accumulated);
        };
        let flatmap = h.select(rows, "flatMap")?;
        let bind = h.select(h.get(&owner, "mapOrder")?, "bind")?;
        let bound = h.apply(bind, vec![owner])?;
        h.apply(flatmap, vec![bound])
    }))
}
pub fn fetch_markets<H: Host>(h: &H, owner: H::Value) -> SessionFuture<'_, H::Value, H::Error> {
    eager(Box::pin(async move {
        let all = h.select(h.get(&h.global(), "Promise")?, "all")?;
        let map = h.select(h.get(&owner, "supportedCategories")?, "map")?;
        let bind = h.select(h.get(&owner, "fetchMarketsHelper")?, "bind")?;
        let bound = h.apply(bind, vec![owner.clone()])?;
        let requests = h.apply(map, vec![bound])?;
        let combined = h.apply(all, vec![requests])?;
        let markets = h.wait(combined).await?;
        let flat = call!(h, markets, "flat", vec![]);
        let some = h.select(flat.clone(), "some")?;
        let callback = h.callback(owner.clone(), "inverse-null-market", vec![])?;
        if h.truthy(&h.apply(some, vec![callback])?) {
            call!(
                h,
                owner.clone(),
                "emitAndLogError",
                vec![h.literal("Failed to fetch markets")]
            );
            return h.get(&h.get(&owner, "store")?, "markets");
        }
        Ok(flat)
    }))
}
