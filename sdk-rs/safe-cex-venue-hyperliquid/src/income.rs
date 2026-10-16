//! Income, funding, inclusive history pagination and derived close-fill records.
use crate::{cross_cache::CrossCacheHost, spot_json::JsString};
use std::{future::Future, pin::Pin};
pub type IncomeFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait IncomeHost: CrossCacheHost {
    fn number(&mut self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn number_value(&self, value: f64) -> Self::Value;
    fn equal_number(&self, value: &Self::Value, number: f64) -> bool;
    fn parse_float(&mut self, text: &JsString) -> Result<f64, Self::Error>;
    fn array(&mut self, values: &[Self::Value]) -> Result<Self::Value, Self::Error>;
    fn new_set(&mut self) -> Result<Self::Value, Self::Error>;
    fn math_max(&mut self, left: f64, right: f64) -> Result<f64, Self::Error>;
    fn await_value(&mut self, value: Self::Value) -> IncomeFuture<'_, Self::Value, Self::Error>;
}
fn text(v: &str) -> JsString {
    JsString(v.encode_utf16().collect())
}
fn concat(parts: &[&[u16]]) -> JsString {
    JsString(parts.iter().flat_map(|p| p.iter().copied()).collect())
}
fn field<H: IncomeHost>(h: &mut H, o: &H::Value, k: &str, v: &H::Value) -> Result<(), H::Error> {
    h.define_data(o, &text(k), v)
}
fn method<H: IncomeHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    h.call(&f, o, args)
}
fn nonnull<H: IncomeHost>(h: &H, v: H::Value, fallback: impl FnOnce() -> H::Value) -> H::Value {
    if h.is_nullish(&v) { fallback() } else { v }
}
pub fn to_number<H: IncomeHost>(h: &mut H, value: &H::Value) -> Result<f64, H::Error> {
    let value = if h.is_nullish(value) {
        h.literal(&text(""))
    } else {
        value.clone()
    };
    let value = h.to_string(&value)?;
    let parsed = h.parse_float(&value)?;
    Ok(if parsed.is_finite() { parsed } else { 0. })
}
pub fn read_funding_row<H: IncomeHost>(h: &mut H, row: &H::Value) -> Result<H::Value, H::Error> {
    let delta = h.get(row, "delta")?;
    let delta = nonnull(h, delta, || row.clone());
    let result = h.object()?;
    let coin = h.get(&delta, "coin")?;
    let coin = if h.is_nullish(&coin) {
        h.get(row, "coin")?
    } else {
        coin
    };
    let coin = nonnull(h, coin, || h.literal(&text("")));
    let coin = h.to_string(&coin)?;
    field(h, &result, "coin", &h.literal(&coin))?;
    let usdc = h.get(&delta, "usdc")?;
    let usdc = if h.is_nullish(&usdc) {
        h.get(row, "usdc")?
    } else {
        usdc
    };
    let usdc = to_number(h, &usdc)?;
    field(h, &result, "usdc", &h.number_value(usdc))?;
    let hash = h.get(row, "hash")?;
    let hash = nonnull(h, hash, || h.literal(&text("")));
    let hash = h.to_string(&hash)?;
    field(h, &result, "hash", &h.literal(&hash))?;
    let time = h.get(row, "time")?;
    let time = nonnull(h, time, || h.number_value(0.));
    let time = h.number(&time)?;
    field(h, &result, "time", &h.number_value(time))?;
    Ok(result)
}
pub fn map_hyperliquid_fill_to_income_records<H: IncomeHost>(
    h: &mut H,
    fill: &H::Value,
    resolve: &H::Value,
) -> Result<H::Value, H::Error> {
    let time = h.get(fill, "time")?;
    let time = nonnull(h, time, || h.number_value(0.));
    let time = h.number(&time)?;
    let time_value = h.number_value(time);
    let coin = h.get(fill, "coin")?;
    let coin = nonnull(h, coin, || h.literal(&text("")));
    let coin = h.to_string(&coin)?;
    let coin = h.literal(&coin);
    if !h.truthy(&time_value) || !h.truthy(&coin) {
        return h.array(&[]);
    }
    let pnl = h.get(fill, "closedPnl")?;
    let pnl = to_number(h, &pnl)?;
    let fee = h.get(fill, "fee")?;
    let fee = to_number(h, &fee)?;
    let symbol = h.call(resolve, &h.undefined(), &[coin.clone()])?;
    let symbol = if h.truthy(&symbol) { symbol } else { coin };
    let asset = h.get(fill, "feeToken")?;
    let asset = if h.truthy(&asset) {
        asset
    } else {
        h.literal(&text("USDC"))
    };
    let asset = h.to_string(&asset)?;
    let asset = h.literal(&asset);
    let tid = h.get(fill, "tid")?;
    let tid = if h.is_nullish(&tid) {
        let hash = h.get(fill, "hash")?;
        if h.is_nullish(&hash) {
            let t = h.to_string(&time_value)?;
            h.literal(&t)
        } else {
            hash
        }
    } else {
        tid
    };
    let tid = h.to_string(&tid)?;
    let records = h.array(&[])?;
    for (enabled, income_type, income, suffix, category) in [
        (pnl != 0., "REALIZED_PNL", pnl, ":pnl", "realized_pnl"),
        (
            fee != 0.,
            if fee > 0. { "COMMISSION" } else { "REBATE" },
            -fee,
            ":fee",
            if fee > 0. { "commission" } else { "rebate" },
        ),
    ] {
        if !enabled {
            continue;
        }
        let push = h.get(&records, "push")?;
        let record = h.object()?;
        field(h, &record, "symbol", &symbol)?;
        field(h, &record, "incomeType", &h.literal(&text(income_type)))?;
        field(h, &record, "income", &h.number_value(income))?;
        field(h, &record, "asset", &asset)?;
        field(h, &record, "timestamp", &time_value)?;
        field(
            h,
            &record,
            "tranId",
            &h.literal(&concat(&[&tid.0, &text(suffix).0])),
        )?;
        field(h, &record, "tradeId", &h.literal(&tid))?;
        field(h, &record, "category", &h.literal(&text(category)))?;
        h.call(&push, &records, &[record])?;
    }
    Ok(records)
}
pub fn map_hyperliquid_funding_to_income_record<H: IncomeHost>(
    h: &mut H,
    row: &H::Value,
    resolve: &H::Value,
) -> Result<H::Value, H::Error> {
    let normalized = read_funding_row(h, row)?;
    let coin = h.get(&normalized, "coin")?;
    let usdc = h.get(&normalized, "usdc")?;
    let hash = h.get(&normalized, "hash")?;
    let time = h.get(&normalized, "time")?;
    if !h.truthy(&time) || !h.truthy(&coin) || h.equal_number(&usdc, 0.) {
        return Ok(h.null());
    }
    let record = h.object()?;
    let symbol = h.call(resolve, &h.undefined(), &[coin.clone()])?;
    let symbol = if h.truthy(&symbol) {
        symbol
    } else {
        coin.clone()
    };
    field(h, &record, "symbol", &symbol)?;
    field(h, &record, "incomeType", &h.literal(&text("FUNDING_FEE")))?;
    field(h, &record, "income", &usdc)?;
    field(h, &record, "asset", &h.literal(&text("USDC")))?;
    field(h, &record, "timestamp", &time)?;
    let id = if h.truthy(&hash) { hash } else { time };
    let id = h.to_string(&id)?;
    let coin = h.to_string(&coin)?;
    field(
        h,
        &record,
        "tranId",
        &h.literal(&concat(&[
            &id.0,
            &text(":").0,
            &coin.0,
            &text(":funding").0,
        ])),
    )?;
    field(h, &record, "category", &h.literal(&text("funding")))?;
    Ok(record)
}
async fn paginate_body<H: IncomeHost>(
    h: &mut H,
    post: &H::Value,
    kind: &H::Value,
    start_time: f64,
    end_time: &H::Value,
    key_of: &H::Value,
    on_cap: &H::Value,
    max_pages: Option<f64>,
) -> Result<H::Value, H::Error> {
    let max_pages = max_pages.unwrap_or(200.);
    let collected = h.array(&[])?;
    let seen = h.new_set()?;
    let mut cursor = start_time;
    let mut page = 0.;
    while page < max_pages {
        let body = h.object()?;
        field(h, &body, "startTime", &h.number_value(cursor))?;
        field(h, &body, "endTime", end_time)?;
        let promise = h.call(post, &h.undefined(), &[kind.clone(), body])?;
        let rows = h.await_value(promise).await?;
        let length = h.get(&rows, "length")?;
        if h.equal_number(&length, 0.) {
            break;
        }
        let mut newest = cursor;
        let mut added = 0;
        let iterator = h.entries(&rows)?;
        while let Some(row) = h.iterator_step(&iterator)? {
            let body = (|| {
                let key = h.call(key_of, &h.undefined(), &[row.clone()])?;
                let exists = method(h, &seen, "has", &[key.clone()])?;
                if h.truthy(&exists) {
                    return Ok(());
                }
                method(h, &seen, "add", &[key])?;
                method(h, &collected, "push", &[row.clone()])?;
                added += 1;
                let time = h.get(&row, "time")?;
                let time = nonnull(h, time, || h.number_value(0.));
                let time = h.number(&time)?;
                newest = h.math_max(newest, time)?;
                Ok(())
            })();
            if let Err(error) = body {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
        if added == 0 {
            break;
        }
        cursor = newest;
        page += 1.;
    }
    if page >= max_pages && !h.is_nullish(on_cap) {
        h.call(
            on_cap,
            &h.undefined(),
            &[kind.clone(), h.number_value(max_pages)],
        )?;
    }
    Ok(collected)
}
pub fn derive_hyperliquid_closed_positions<H: IncomeHost>(
    h: &mut H,
    fills: &H::Value,
    resolve: &H::Value,
    account: &H::Value,
) -> Result<H::Value, H::Error> {
    let result = h.array(&[])?;
    let iterator = h.entries(fills)?;
    while let Some(fill) = h.iterator_step(&iterator)? {
        let body = (|| {
            let dir = h.get(&fill, "dir")?;
            let dir = nonnull(h, dir, || h.literal(&text("")));
            let dir = h.to_string(&dir)?;
            let dir_value = h.literal(&dir);
            let lower = method(h, &dir_value, "toLowerCase", &[])?;
            let starts = method(h, &lower, "startsWith", &[h.literal(&text("close"))])?;
            if !h.truthy(&starts) {
                return Ok(());
            }
            let time = h.get(&fill, "time")?;
            let time = nonnull(h, time, || h.number_value(0.));
            let time = h.number(&time)?;
            let time = h.number_value(time);
            let coin = h.get(&fill, "coin")?;
            let coin = nonnull(h, coin, || h.literal(&text("")));
            let coin = h.to_string(&coin)?;
            let coin = h.literal(&coin);
            if !h.truthy(&time) || !h.truthy(&coin) {
                return Ok(());
            }
            let px = h.get(&fill, "px")?;
            let px = to_number(h, &px)?;
            let size = h.get(&fill, "sz")?;
            let size = to_number(h, &size)?;
            let push = h.get(&result, "push")?;
            let row = h.object()?;
            field(h, &row, "provenance", &h.literal(&text("derived")))?;
            let symbol = h.call(resolve, &h.undefined(), &[coin.clone()])?;
            let symbol = if h.truthy(&symbol) { symbol } else { coin };
            field(h, &row, "symbol", &symbol)?;
            field(h, &row, "notional", &h.number_value(px * size))?;
            field(h, &row, "contracts", &h.number_value(size))?;
            field(h, &row, "entryPrice", &h.number_value(0.))?;
            field(h, &row, "exitPrice", &h.number_value(px))?;
            let pnl = h.get(&fill, "closedPnl")?;
            let pnl = to_number(h, &pnl)?;
            field(h, &row, "unrealizedPnl", &h.number_value(pnl))?;
            let lower = method(h, &dir_value, "toLowerCase", &[])?;
            let includes = method(h, &lower, "includes", &[h.literal(&text("long"))])?;
            let side = if h.truthy(&includes) { "long" } else { "short" };
            field(h, &row, "side", &h.literal(&text(side)))?;
            let fee = h.get(&fill, "fee")?;
            let fee = to_number(h, &fee)?;
            field(h, &row, "fees", &h.number_value(fee))?;
            field(h, &row, "time", &time)?;
            field(h, &row, "closedTime", &time)?;
            field(h, &row, "account", account)?;
            field(h, &row, "leverage", &h.number_value(0.))?;
            h.call(&push, &result, &[row])?;
            Ok(())
        })();
        if let Err(error) = body {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    Ok(result)
}

pub fn paginate_hyperliquid_by_time<H>(
    host: H,
    post: H::Value,
    kind: H::Value,
    start_time: f64,
    end_time: H::Value,
    key_of: H::Value,
    on_cap: H::Value,
    max_pages: Option<f64>,
) -> crate::helper_tasks::HelperObserver<H::Value, H::Error>
where
    H: IncomeHost + crate::helper_tasks::HelperTaskHost<H::Value, H::Error>,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut worker = host.clone();
    crate::helper_tasks::start(&host, async move {
        paginate_body(
            &mut worker,
            &post,
            &kind,
            start_time,
            &end_time,
            &key_of,
            &on_cap,
            max_pages,
        )
        .await
    })
}
