//! Historical page walks and opaque record orchestration.
use std::{cmp::Ordering, collections::HashSet, future::Future};

pub struct HistoryPageRequest<S> {
    pub symbol: Option<S>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub limit: f64,
    pub skip: Option<f64>,
}

type FieldRead<'a, R, E> = dyn FnMut(&R, &'static str) -> Result<Option<Vec<u16>>, E> + 'a;

pub struct HistoryRowSelectors<R, E> {
    pub time_fields: &'static [&'static str],
    pub id_field: &'static str,
    pub time_of: for<'a> fn(&R, &[&'static str], &mut FieldRead<'a, R, E>) -> Result<f64, E>,
    pub id_of: for<'a> fn(&R, &'static str, &mut FieldRead<'a, R, E>) -> Result<Vec<u16>, E>,
}

pub fn history_row_time<R, E>(
    row: &R,
    fields: &[&'static str],
    read: &mut FieldRead<'_, R, E>,
) -> Result<f64, E> {
    for field in fields {
        if let Some(text) = read(row, field)? {
            return Ok(safe_cex_num::parse_int(
                &String::from_utf16_lossy(&text),
                Some(10),
            ));
        }
    }
    Ok(0.0)
}

pub fn history_row_id<R, E>(
    row: &R,
    field: &'static str,
    read: &mut FieldRead<'_, R, E>,
) -> Result<Vec<u16>, E> {
    Ok(read(row, field)?.unwrap_or_default())
}

fn selectors<R, E>(fields: &'static [&'static str], id: &'static str) -> HistoryRowSelectors<R, E> {
    HistoryRowSelectors {
        time_fields: fields,
        id_field: id,
        time_of: history_row_time,
        id_of: history_row_id,
    }
}

fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}
fn descending<T, E>(
    mut rows: Vec<T>,
    mut time: impl FnMut(&T) -> Result<f64, E>,
) -> Result<Vec<T>, E> {
    let mut values = Vec::with_capacity(rows.len());
    for row in rows.drain(..) {
        values.push((time(&row)?, row));
    }
    values.sort_by(|(a, _), (b, _)| b.partial_cmp(a).unwrap_or(Ordering::Equal));
    Ok(values.into_iter().map(|(_, row)| row).collect())
}

pub async fn fetch_bitunix_history_pages<D, L, R, S: Clone, E, F: Future<Output = Result<D, E>>>(
    endpoint: &[u16],
    start: Option<f64>,
    end: Option<f64>,
    symbol: Option<S>,
    mut fetch: impl FnMut(&[u16], HistoryPageRequest<S>) -> F,
    (mut body, mut field, is_array): (
        impl FnMut(D) -> Result<D, E>,
        impl FnMut(&D, &'static str) -> Result<D, E>,
        impl Fn(&D) -> bool,
    ),
    (mut iterator, mut next, mut length): (
        impl FnMut(D) -> Result<L, E>,
        impl FnMut(&L, usize) -> Result<Option<R>, E>,
        impl FnMut(&L) -> Result<f64, E>,
    ),
    mut time_of: impl FnMut(&R) -> Result<f64, E>,
    mut id_of: impl FnMut(&R) -> Result<Vec<u16>, E>,
    mut warning: impl FnMut(Vec<u16>, &'static str) -> Result<(), E>,
    report: impl FnOnce(E) -> Result<(), E>,
) -> Result<Vec<R>, E> {
    let mut collected = Vec::new();
    let mut seen = HashSet::new();
    let result = async {
        for page in 0..100 {
            let response = fetch(
                endpoint,
                HistoryPageRequest {
                    symbol: symbol.clone(),
                    start_time: start,
                    end_time: end,
                    limit: 100.0,
                    skip: (page != 0).then_some(page as f64 * 100.0),
                },
            )
            .await?;
            let body = body(response)?;
            let rows = if is_array(&body) {
                Some(body)
            } else {
                let mut rows = None;
                for key in ["positionList", "tradeList", "orderList"] {
                    if is_array(&field(&body, key)?) {
                        rows = Some(field(&body, key)?);
                        break;
                    }
                }
                rows
            };
            let Some(rows) = rows else { break };
            let rows = iterator(rows)?;
            if length(&rows)? == 0.0 {
                break;
            }
            let mut added = 0usize;
            let mut reached_start = false;
            let mut index = 0usize;
            while let Some(row) = next(&rows, index)? {
                index += 1;
                let time = time_of(&row)?;
                if start.is_some_and(|start| truthy(start) && truthy(time) && time < start) {
                    reached_start = true;
                    continue;
                }
                if end.is_some_and(|end| truthy(end) && truthy(time) && time > end) {
                    continue;
                }
                let id = id_of(&row)?;
                if !id.is_empty() && seen.contains(&id) {
                    continue;
                }
                if !id.is_empty() {
                    seen.insert(id);
                }
                collected.push(row);
                added += 1;
            }
            if reached_start || length(&rows)? < 100.0 || added == 0 {
                break;
            }
            if page == 99 {
                let mut text = "bitunix history paging hit the 100-page cap on "
                    .encode_utf16()
                    .collect::<Vec<_>>();
                text.extend_from_slice(endpoint);
                text.extend("; older rows were not fetched".encode_utf16());
                warning(text, "warning")?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        report(error)?;
    }
    Ok(collected)
}

pub async fn fetch_closed_position_rows<R, S, E, F: Future<Output = Result<Vec<R>, E>>>(
    is_public: impl FnOnce() -> Result<bool, E>,
    options: impl FnOnce() -> Result<(Option<f64>, Option<f64>, Option<S>), E>,
    fetch: impl FnOnce(
        &'static str,
        Option<f64>,
        Option<f64>,
        HistoryRowSelectors<R, E>,
        Option<S>,
    ) -> F,
) -> Result<Vec<R>, E> {
    if is_public()? {
        return Ok(vec![]);
    }
    let (from, to, symbol) = options()?;
    fetch(
        endpoint("HISTORY_POSITIONS"),
        from,
        to,
        selectors(&["mtime", "ctime"], "positionId"),
        symbol,
    )
    .await
}

pub async fn fetch_income_history<R, O, S, E, F: Future<Output = Result<Vec<R>, E>>>(
    is_public: impl FnOnce() -> Result<bool, E>,
    options: impl FnOnce() -> Result<(Option<f64>, Option<f64>, Option<S>), E>,
    fetch: impl FnOnce(
        &'static str,
        Option<f64>,
        Option<f64>,
        HistoryRowSelectors<R, E>,
        Option<S>,
    ) -> F,
    mut map: impl FnMut(R) -> Result<Vec<O>, E>,
    time: impl FnMut(&O) -> Result<f64, E>,
) -> Result<Vec<O>, E> {
    if is_public()? {
        return Ok(vec![]);
    }
    let (from, to, symbol) = options()?;
    let rows = fetch(
        endpoint("HISTORY_TRADES"),
        from,
        to,
        selectors(&["ctime"], "tradeId"),
        symbol,
    )
    .await?;
    let mut records = Vec::new();
    for row in rows {
        records.extend(map(row)?);
    }
    descending(records, time)
}

pub async fn fetch_closed_pnl<R, O, A, S, E, F: Future<Output = Result<Vec<R>, E>>>(
    is_public: impl FnOnce() -> Result<bool, E>,
    options: impl FnOnce() -> Result<(Option<f64>, Option<f64>, Option<S>), E>,
    fetch: impl FnOnce(
        &'static str,
        Option<f64>,
        Option<f64>,
        HistoryRowSelectors<R, E>,
        Option<S>,
    ) -> F,
    mut account: impl FnMut() -> Result<A, E>,
    mut map: impl FnMut(R, A) -> Result<Option<O>, E>,
    time: impl FnMut(&O) -> Result<f64, E>,
) -> Result<Vec<O>, E> {
    if is_public()? {
        return Ok(vec![]);
    }
    let (from, to, symbol) = options()?;
    let rows = fetch(
        endpoint("HISTORY_POSITIONS"),
        from,
        to,
        selectors(&["mtime", "ctime"], "positionId"),
        symbol,
    )
    .await?;
    let mut records = Vec::new();
    for row in rows {
        if let Some(record) = map(row, account()?)? {
            records.push(record);
        }
    }
    descending(records, time)
}

pub async fn fetch_historical_positions<R, O, A, S, E, F: Future<Output = Result<Vec<R>, E>>>(
    options: impl FnOnce() -> Result<(Option<f64>, Option<f64>), E>,
    fetch: impl FnOnce(Option<f64>, Option<f64>) -> F,
    mut account: impl FnMut() -> Result<A, E>,
    mut quote: impl FnMut(S) -> Result<Option<Vec<u16>>, E>,
    mut map: impl FnMut(R, A, &mut dyn FnMut(S) -> Result<Vec<u16>, E>) -> Result<Option<O>, E>,
) -> Result<Vec<O>, E> {
    let (from, to) = options()?;
    let rows = fetch(from, to).await?;
    let mut records = Vec::new();
    for row in rows {
        let account = account()?;
        let mut resolve = |symbol| {
            Ok(quote(symbol)?
                .filter(|q| !q.is_empty())
                .unwrap_or_else(|| "USDT".encode_utf16().collect()))
        };
        if let Some(record) = map(row, account, &mut resolve)? {
            records.push(record);
        }
    }
    Ok(records)
}

pub async fn fetch_historical_orders<D, L, R, O, S: Clone, E, F: Future<Output = Result<D, E>>>(
    is_public: impl FnOnce() -> Result<bool, E>,
    mut options: impl FnMut(&'static str) -> Result<Option<f64>, E>,
    mut symbol: impl FnMut() -> Result<Option<S>, E>,
    mut fetch: impl FnMut(&'static str, HistoryPageRequest<S>) -> F,
    (mut rows, mut next, mut length): (
        impl FnMut(D) -> Result<L, E>,
        impl FnMut(&L, usize) -> Result<Option<R>, E>,
        impl FnMut(&L) -> Result<f64, E>,
    ),
    mut text: impl FnMut(&R, &'static str) -> Result<Option<Vec<u16>>, E>,
    mut map: impl FnMut(R) -> Result<Option<O>, E>,
    time: impl FnMut(&O) -> Result<f64, E>,
    warning: impl FnOnce(&'static str, &'static str) -> Result<(), E>,
    report: impl FnOnce(E) -> Result<(), E>,
) -> Result<Vec<O>, E> {
    if is_public()? {
        return Ok(vec![]);
    }
    let result = async {
        let mut orders = Vec::new();
        let mut seen = HashSet::new();
        let mut end = options("to")?;
        let mut page = 0usize;
        while page < 50 {
            let request = HistoryPageRequest {
                symbol: symbol()?,
                start_time: options("from")?,
                end_time: end,
                limit: 100.0,
                skip: None,
            };
            let rows = rows(fetch(endpoint("HISTORY_ORDERS"), request).await?)?;
            if length(&rows)? == 0.0 {
                break;
            }
            let mut oldest = f64::INFINITY;
            let mut fresh = 0usize;
            let mut index = 0usize;
            while let Some(row) = next(&rows, index)? {
                index += 1;
                let created = safe_cex_num::parse_int(
                    &String::from_utf16_lossy(&text(&row, "ctime")?.unwrap_or_else(|| vec![48])),
                    Some(10),
                );
                if truthy(created) {
                    oldest = oldest.min(created);
                }
                let id = text(&row, "orderId")?.unwrap_or_default();
                if id.is_empty() || seen.contains(&id) {
                    continue;
                }
                seen.insert(id);
                fresh += 1;
                if let Some(order) = map(row)? {
                    orders.push(order);
                }
            }
            if length(&rows)? < 100.0 || fresh == 0 || !oldest.is_finite() {
                break;
            }
            if options("from")?.is_some_and(truthy)
                && oldest <= options("from")?.unwrap_or(f64::NAN)
            {
                break;
            }
            end = Some(oldest);
            page += 1;
        }
        if page >= 50 {
            warning(
                "fetchHistoricalOrders stopped at the 50-page cap; older orders were not fetched",
                "warning",
            )?;
        }
        let sorted = descending(orders, time)?;
        if options("limit")?.is_some_and(truthy) && options("limit")?.is_some_and(|v| v > 0.0) {
            let limit = options("limit")?.unwrap_or(f64::NAN);
            let size = if limit.is_nan() {
                0
            } else if limit.is_sign_positive() {
                limit.trunc().min(sorted.len() as f64) as usize
            } else {
                0
            };
            Ok(sorted.into_iter().take(size).collect())
        } else {
            Ok(sorted)
        }
    }
    .await;
    match result {
        Ok(rows) => Ok(rows),
        Err(error) => {
            report(error)?;
            Ok(vec![])
        }
    }
}
