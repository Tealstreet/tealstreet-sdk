//! Current positions/balance and historical income/closed axes on configured REST.
use crate::{
    income::{IncomeInput, map_blofin_closed_position, map_blofin_fill_to_income_records},
    modes::AccountModes,
    order_reads::error_value,
    order_services::{
        ServiceError, ServiceHost, emit, field, get_or_fetch_margin_type,
        get_or_fetch_position_mode, request,
    },
    position_adapter::{BlofinPositionAdapterDeps, map_blofin_position_history},
    positions::{PositionMapError, PositionsInput, map_positions},
    symbols::{market_id_to_symbol, symbol_to_market_id},
    utility::UtilityInput,
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_num::{
    add, js_string, js_truthy, oldest_cursor_id, parse_float, parse_int, round_usd, subtract,
};
use safe_cex_store::Store;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest};
use safe_cex_types::{
    Balance, ClosedPnlEntry, HistoricalTradeOptions, IncomeHistoryOptions, IncomeRecord,
    MarginType, Position, PositionMode, PositionRecord, PositionSide,
    model_utils::create_position_id,
};
use serde_json::{Map, Value, json};
use std::{
    cell::Cell,
    collections::HashSet,
    future::{Future, poll_fn},
    task::Poll,
};
type DetailedEmitter<'a, E> = Box<dyn Fn(&ServiceError<E>, Option<&Value>) -> Result<(), E> + 'a>;
type ConsoleWarning<'a, E> = Box<dyn Fn(&str) -> Result<(), E> + 'a>;
pub struct PositionReadHost<'a, E> {
    pub service: &'a ServiceHost<'a, E>,
    pub modes: &'a AccountModes,
    pub auth_failure_streak: &'a Cell<f64>,
    pub disposed: &'a Cell<bool>,
    pub console_warn: ConsoleWarning<'a, E>,
    pub emit_error_with_detail: DetailedEmitter<'a, E>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PositionMetadata {
    pub symbol: String,
    pub leverage: f64,
    pub margin_type: MarginType,
    pub position_mode: PositionMode,
    pub side: PositionSide,
}
pub struct BalanceAndPositions {
    pub balance: Balance,
    pub positions: Vec<Position>,
}
pub struct HistoricalPositionsOptions {
    pub account: String,
    pub from: Option<f64>,
    pub to: Option<f64>,
}
struct YieldAdapter<'a>(&'a dyn RestAdapter);
impl RestAdapter for YieldAdapter<'_> {
    fn query_in_url(&self) -> bool {
        self.0.query_in_url()
    }
    fn execute(&self, req: RestRequest) -> RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(req)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        req: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            let response = self.0.execute_with_receipt(req).await;
            let mut yielded = false;
            poll_fn(|cx| {
                if !yielded {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            response
        })
    }
}
async fn get<E>(
    xhr: &dyn RestAdapter,
    endpoint: &str,
    params: Value,
) -> Result<UtilityInput, ServiceError<E>> {
    request(&YieldAdapter(xhr), "GET", endpoint, Some(params), None).await
}
async fn pair<A, B, E>(
    a: impl Future<Output = Result<A, ServiceError<E>>>,
    b: impl Future<Output = Result<B, ServiceError<E>>>,
) -> Result<(A, B), ServiceError<E>> {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let mut left = None;
    let mut right = None;
    poll_fn(|cx| {
        let mut failure = None;
        if left.is_none() {
            match a.as_mut().poll(cx) {
                Poll::Ready(Ok(v)) => left = Some(v),
                Poll::Ready(Err(e)) => failure = Some(e),
                Poll::Pending => (),
            }
        }
        if right.is_none() {
            match b.as_mut().poll(cx) {
                Poll::Ready(Ok(v)) => right = Some(v),
                Poll::Ready(Err(e)) => {
                    if failure.is_none() {
                        failure = Some(e)
                    }
                }
                Poll::Pending => (),
            }
        }
        if let Some(e) = failure {
            return Poll::Ready(Err(e));
        }
        if left.is_some() && right.is_some() {
            Poll::Ready(Ok((
                left.take().expect("left"),
                right.take().expect("right"),
            )))
        } else {
            Poll::Pending
        }
    })
    .await
}
fn input<E>(text: impl Into<String>) -> ServiceError<E> {
    ServiceError::Input(text.into())
}
fn parse<E>(v: Option<&Value>) -> Result<f64, ServiceError<E>> {
    Ok(parse_float(&js_string(v).map_err(ServiceError::Coercion)?))
}
fn selected(store: &dyn Store, symbol: Option<&str>) -> Vec<Position> {
    store
        .positions()
        .iter()
        .filter(|p| symbol.is_none_or(|s| p.symbol == s))
        .cloned()
        .collect()
}
pub async fn fetch_positions_helper<E: std::fmt::Display>(
    symbol: Option<&str>,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<Vec<Position>, ServiceError<E>> {
    let symbol = symbol.filter(|s| !s.is_empty());
    let outcome = async {
        let params = match symbol {
            Some(s) => {
                symbol_to_market_id(s, store).map_or_else(|| json!({}), |id| json!({"instId":id}))
            }
            None => json!({}),
        };
        let response = get(xhr, "POSITIONS", params).await?;
        let data = field(response.json(), "data")?;
        if !js_truthy(data) {
            let message = response
                .json()
                .get("msg")
                .filter(|v| js_truthy(Some(v)))
                .cloned()
                .unwrap_or_else(|| json!("No data found"));
            emit(host.service, Some(&message))?;
            return Ok(selected(store, symbol));
        }
        map_positions(
            &PositionsInput::from_retained(data.expect("truthy").clone()),
            store,
            |text| emit(host.service, Some(&json!(text))),
        )
        .map_err(|e| match e {
            PositionMapError::Coercion(e) => ServiceError::Coercion(e),
            PositionMapError::Emitter(e) => e,
        })
    }
    .await;
    match outcome {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host.service, Some(&error_value(&e, false, None)))?;
            Ok(selected(store, symbol))
        }
    }
}
pub async fn fetch_positions<E: std::fmt::Display>(
    symbols: Option<&[String]>,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<Vec<Position>, ServiceError<E>> {
    let open = fetch_positions_helper(None, store, xhr, host).await?;
    if host.auth_failure_streak.get() > 0.0 {
        return Ok(match symbols {
            Some(symbols) => store
                .positions()
                .iter()
                .filter(|p| symbols.contains(&p.symbol))
                .cloned()
                .collect(),
            None => open,
        });
    }
    let Some(symbols) = symbols else {
        return Ok(open);
    };
    let mut out: Vec<_> = open
        .into_iter()
        .filter(|p| symbols.contains(&p.symbol))
        .collect();
    let remaining: Vec<_> = symbols
        .iter()
        .filter(|s| !out.iter().any(|p| p.symbol == **s))
        .cloned()
        .collect();
    if !remaining.is_empty() {
        let adapter = YieldAdapter(xhr);
        let (margin, mode) = pair(
            get_or_fetch_margin_type(None, true, host.modes, store, &adapter, host.service),
            get_or_fetch_position_mode(None, true, host.modes, store, &adapter, host.service),
        )
        .await?;
        for metadata in fetch_position_metadata(&remaining, store, xhr, host).await? {
            out.push(Position {
                id: create_position_id(
                    &metadata.symbol,
                    Some(metadata.side),
                    Some(metadata.position_mode),
                ),
                symbol: metadata.symbol,
                side: (metadata.side).into(),
                entry_price: 0.0,
                notional: 0.0,
                leverage: metadata.leverage,
                margin_type: (Some(metadata.margin_type)).into(),
                position_mode: (Some(metadata.position_mode)).into(),
                unrealized_pnl: 0.0,
                contracts: 0.0,
                liquidation_price: 0.0,
                info: (Some(json!({}).into())).into(),
                extra: (None).into(),
                realized_pnl: (None).into(),
                margin_size: (None).into(),
                last_update: (None).into(),
                option: (None).into(),
            });
        }
        // Metadata carries both fields and source spread overrides the outer defaults.
        let _ = (margin, mode);
    }
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.id.clone()));
    Ok(out)
}
pub async fn fetch_position_metadata<E: std::fmt::Display>(
    symbols: &[String],
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<Vec<PositionMetadata>, ServiceError<E>> {
    let outcome=async {
        let adapter=YieldAdapter(xhr);
        let (margin,mode)=pair(get_or_fetch_margin_type(None,false,host.modes,store,&adapter,host.service),get_or_fetch_position_mode(None,false,host.modes,store,&adapter,host.service)).await?;
        let mut out=Vec::new();
        for batch in symbols.chunks(20){
            if host.disposed.get(){continue;}
            let ids:Vec<_>=batch.iter().map(|s|symbol_to_market_id(s,store).unwrap_or_default()).collect();
            let response=get(xhr,"LEVERAGE",json!({"instId":ids.join(","),"marginMode":if margin==MarginType::Isolated{"isolated"}else{"cross"}})).await?;
            let response=response.json();
            if field(response,"code")?.and_then(Value::as_str)!=Some("0"){emit(host.service,response.get("msg"))?;}else if !js_truthy(response.get("data")){(host.console_warn)("No leverage data found").map_err(ServiceError::Host)?;}
            let Some(rows)=response.get("data").filter(|v|js_truthy(Some(v))) else{continue};
            let rows=rows.as_array().ok_or_else(||input("Leverage data is outside the native array model"))?;
            for row in rows {
                let id=js_string(field(row,"instId")?).map_err(ServiceError::Coercion)?;
                let symbol=market_id_to_symbol(&id,store).ok_or_else(||input("Metadata symbol is outside the native string model"))?;
                if field(row,"instId")?.is_none_or(|v|!v.is_string()) && !store.markets_by_id().contains_key(&id){return Err(input("Metadata symbol is outside the native string model"));}
                let side=field(row,"positionSide")?.and_then(Value::as_str);
                out.push(PositionMetadata {symbol,leverage:parse(field(row,"leverage")?)?,margin_type:if field(row,"marginMode")?.and_then(Value::as_str)==Some("cross"){MarginType::Cross}else{MarginType::Isolated},position_mode:if side==Some("net"){PositionMode::OneWay}else{PositionMode::Hedged},side:if matches!(side,Some("net"|"long")){PositionSide::Long}else{PositionSide::Short}});
            }
        }
        out.retain(|p|p.position_mode==mode);Ok(out)
    }.await;
    match outcome {
        Ok(v) => Ok(v),
        Err(e) => {
            let detail = error_value(&e, false, None);
            (host.emit_error_with_detail)(&e, Some(&detail)).map_err(ServiceError::Host)?;
            Ok(Vec::new())
        }
    }
}
pub async fn fetch_balance_and_positions<E: std::fmt::Display>(
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<BalanceAndPositions, ServiceError<E>> {
    let outcome = async {
        let positions = fetch_positions(None, store, xhr, host).await?;
        let response = get(xhr, "BALANCE", json!({"accountType":"futures"})).await?;
        let bal = field(response.json(), "data")?;
        if bal.is_some_and(|v| !v.is_array() && v.get("length").is_some_and(|n| js_truthy(Some(n))))
        {
            return Err(input(
                "Array-like balance rows are outside the native array model",
            ));
        }
        let mut available = None;
        let mut frozen = None;
        // Lodash sums are ordinary JS +, not decimal-safe add at each step.
        for row in bal.and_then(Value::as_array).into_iter().flatten() {
            let value = parse(field(row, "available")?)?;
            available = Some(available.map_or(value, |sum| sum + value));
            let value = parse(field(row, "frozen")?)?;
            frozen = Some(frozen.map_or(value, |sum| sum + value));
        }
        let total_collateral = add(
            round_usd(available.unwrap_or(0.0)),
            round_usd(frozen.unwrap_or(0.0)),
        );
        let mut maintenance = None;
        let mut pnl = None;
        for p in &positions {
            let info = p.info.as_ref().ok_or_else(|| {
                input("Cannot read properties of undefined (reading 'maintenanceMargin')")
            })?;
            if info.is_nullish() {
                return Err(input(if info.is_null() {
                    "Cannot read properties of null (reading 'maintenanceMargin')"
                } else {
                    "Cannot read properties of undefined (reading 'maintenanceMargin')"
                }));
            }
            let value = safe_cex_num::js_retained_parse_float(info.get("maintenanceMargin"))
                .map_err(ServiceError::Coercion)?;
            maintenance = Some(maintenance.map_or(value, |sum| sum + value));
            pnl = Some(pnl.map_or(p.unrealized_pnl, |sum| sum + p.unrealized_pnl));
        }
        let used = round_usd(maintenance.unwrap_or(0.0));
        let upnl = round_usd(pnl.unwrap_or(0.0));
        let balance = Balance {
            used,
            free: subtract(total_collateral, used),
            total: subtract(total_collateral, upnl),
            upnl,
            info: bal.cloned().map(Into::into),
            currencies: None,
            coins: None,
        };
        Ok(BalanceAndPositions { balance, positions })
    }
    .await;
    match outcome {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host.service, Some(&error_value(&e, false, None)))?;
            Ok(BalanceAndPositions {
                balance: store.balance().clone(),
                positions: store.positions().to_vec(),
            })
        }
    }
}
fn selected_market_id(store: &dyn Store, symbol: Option<&str>) -> Option<Option<String>> {
    let Some(symbol) = symbol.filter(|s| !s.is_empty()) else {
        return Some(None);
    };
    store
        .markets_by_symbol()
        .get(symbol)
        .map(|m| Some(m.id.clone()))
}
fn query<E>(
    market_id: Option<&str>,
    from: Option<f64>,
    to: Option<f64>,
    after: Option<&str>,
) -> Result<Value, ServiceError<E>> {
    let mut params = Map::new();
    if let Some(id) = market_id {
        params.insert("instId".into(), json!(id));
    }
    for (key, value) in [("begin", from), ("end", to)] {
        if let Some(value) = value {
            if !value.is_finite() {
                return Err(input("Nonfinite history query time is outside native JSON"));
            }
            params.insert(key.into(), json!(value));
        }
    }
    params.insert("limit".into(), json!(100));
    if let Some(after) = after {
        params.insert("after".into(), json!(after));
    }
    Ok(Value::Object(params))
}
fn cursor<E>(rows: &[Value], key: &str) -> Result<Option<String>, ServiceError<E>> {
    let mut ids = Vec::new();
    for row in rows {
        if let Some(v) = row.get(key).filter(|v| !v.is_null()) {
            ids.push(js_string(Some(v)).map_err(ServiceError::Coercion)?);
        }
    }
    Ok(oldest_cursor_id(&ids))
}
fn row_id<E>(row: &Value, key: &str) -> Result<String, ServiceError<E>> {
    match field(row, key)?.filter(|v| !v.is_null()) {
        Some(v) => js_string(Some(v)).map_err(ServiceError::Coercion),
        None => Ok(String::new()),
    }
}
fn sort_desc<T>(items: &mut [T], time: impl Fn(&T) -> f64) {
    // Stable insertion sort: JS comparator NaN is equal and must not cross barriers.
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && time(&items[j]) - time(&items[j - 1]) > 0.0 {
            items.swap(j, j - 1);
            j -= 1;
        }
    }
}
pub async fn fetch_income_history<E: std::fmt::Display>(
    opts: Option<&IncomeHistoryOptions>,
    public_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<Vec<IncomeRecord>, ServiceError<E>> {
    if public_only {
        return Ok(Vec::new());
    }
    let symbol = opts.and_then(|o| o.symbol.as_deref());
    let Some(market_id) = selected_market_id(store, symbol) else {
        return Ok(Vec::new());
    };
    let start = opts.and_then(|o| o.start_time);
    let end = opts.and_then(|o| o.end_time);
    let outcome = async {
        let mut records = Vec::new();
        let mut seen = HashSet::new();
        let mut after = None;
        let mut page = 0;
        while page < 100 {
            let params = query(market_id.as_deref(), start, end, after.as_deref())?;
            let response = get(xhr, "FILLS", params).await?;
            let rows = response
                .json()
                .get("data")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            if rows.is_empty() {
                break;
            }
            let mut added = 0;
            let mut reached_start = false;
            for row in rows {
                let ts = parse_int(&row_id(row, "ts")?, Some(10));
                if start
                    .is_some_and(|s| s != 0.0 && !s.is_nan() && ts != 0.0 && !ts.is_nan() && ts < s)
                {
                    reached_start = true;
                    continue;
                }
                if end
                    .is_some_and(|e| e != 0.0 && !e.is_nan() && ts != 0.0 && !ts.is_nan() && ts > e)
                {
                    continue;
                }
                let id = row_id(row, "tradeId")?;
                if !id.is_empty() && !seen.insert(id) {
                    continue;
                }
                added += 1;
                records.extend(
                    map_blofin_fill_to_income_records(
                        &IncomeInput::from_retained(row.clone()),
                        |id| store.markets_by_id().get(id).map(|m| m.symbol.clone()),
                    )
                    .map_err(ServiceError::Coercion)?,
                );
            }
            if reached_start || rows.len() < 100 || added == 0 {
                break;
            }
            let next = cursor(rows, "tradeId")?.unwrap_or_default();
            if next.is_empty() || after.as_deref() == Some(next.as_str()) {
                break;
            }
            after = Some(next);
            page += 1;
        }
        if page >= 100 {
            (host.service.log)(
                "fetchIncomeHistory stopped at the 100-page cap; older fills were not fetched",
                LogSeverity::Warning,
            )
            .map_err(ServiceError::Host)?;
        }
        sort_desc(&mut records, |r| r.timestamp);
        Ok(records)
    }
    .await;
    match outcome {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host.service, Some(&error_value(&e, false, None)))?;
            Ok(Vec::new())
        }
    }
}
pub async fn fetch_income_history_range<E: std::fmt::Display>(
    start_time: f64,
    end_time: f64,
    income_type: Option<&str>,
    public_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<Vec<IncomeRecord>, ServiceError<E>> {
    let opts = IncomeHistoryOptions {
        symbol: None,
        income_type: income_type.map(str::to_owned),
        start_time: Some(start_time),
        end_time: Some(end_time),
        limit: None,
    };
    fetch_income_history(Some(&opts), public_only, store, xhr, host).await
}
pub async fn fetch_closed_position_rows<E>(
    opts: &HistoricalTradeOptions,
    public_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
) -> Result<Vec<UtilityInput>, ServiceError<E>> {
    if public_only {
        return Ok(Vec::new());
    }
    let Some(market_id) = selected_market_id(store, Some(&opts.symbol)) else {
        return Ok(Vec::new());
    };
    let mut collected = Vec::new();
    let mut seen = HashSet::new();
    let mut after = None;
    for _ in 0..100 {
        let params = query(market_id.as_deref(), opts.from, opts.to, after.as_deref())?;
        let response = get(xhr, "POSITIONS_HISTORY", params).await?;
        let rows = response
            .json()
            .get("data")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        if rows.is_empty() {
            break;
        }
        let mut added = 0;
        for row in rows {
            let id = row_id(row, "historyId")?;
            if !id.is_empty() && !seen.insert(id) {
                continue;
            }
            added += 1;
            collected.push(UtilityInput::from_retained((row.clone()).into()));
        }
        if added == 0 || rows.len() < 100 {
            break;
        }
        let next = cursor(rows, "historyId")?.unwrap_or_default();
        if next.is_empty() || after.as_deref() == Some(next.as_str()) {
            break;
        }
        after = Some(next);
    }
    Ok(collected)
}
pub async fn fetch_historical_positions<E>(
    opts: &HistoricalPositionsOptions,
    public_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
) -> Result<Vec<PositionRecord>, ServiceError<E>> {
    let query_opts = HistoricalTradeOptions {
        symbol: String::new(),
        order_id: None,
        from: opts.from,
        to: opts.to,
        limit: None,
        from_id: None,
        account_name: None,
    };
    let rows = fetch_closed_position_rows(&query_opts, public_only, store, xhr).await?;
    let mut records = Vec::new();
    for row in rows {
        let mut symbol = |id: &str| store.markets_by_id().get(id).map(|m| m.symbol.clone());
        let mut settle = |id: &str| store.markets_by_id().get(id).map(|m| m.quote.clone());
        let mut deps = BlofinPositionAdapterDeps {
            account: &opts.account,
            resolve_symbol: &mut symbol,
            resolve_settle_currency: &mut settle,
            contract_size: None,
        };
        if let Some(record) =
            map_blofin_position_history(row.json(), &mut deps).map_err(ServiceError::Input)?
        {
            records.push(record)
        }
    }
    Ok(records)
}
pub async fn fetch_closed_pnl<E: std::fmt::Display>(
    opts: &HistoricalTradeOptions,
    public_only: bool,
    store: &dyn Store,
    xhr: &dyn RestAdapter,
    host: &PositionReadHost<'_, E>,
) -> Result<Vec<ClosedPnlEntry>, ServiceError<E>> {
    if public_only {
        return Ok(Vec::new());
    }
    let Some(market_id) = selected_market_id(store, Some(&opts.symbol)) else {
        return Ok(Vec::new());
    };
    let outcome = async {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        let mut after = None;
        let mut page = 0;
        while page < 100 {
            let params = query(market_id.as_deref(), opts.from, opts.to, after.as_deref())?;
            let response = get(xhr, "POSITIONS_HISTORY", params).await?;
            let rows = response
                .json()
                .get("data")
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            if rows.is_empty() {
                break;
            }
            let mut added = 0;
            for row in rows {
                let id = row_id(row, "historyId")?;
                if !id.is_empty() && !seen.insert(id) {
                    continue;
                }
                added += 1;
                if let Some(entry) = map_blofin_closed_position(
                    &IncomeInput::from_retained(row.clone()),
                    |id| store.markets_by_id().get(id).map(|m| m.symbol.clone()),
                    opts.account_name.as_deref().unwrap_or(""),
                )
                .map_err(ServiceError::Coercion)?
                {
                    entries.push(entry)
                }
            }
            if added == 0 || rows.len() < 100 {
                break;
            }
            let next = cursor(rows, "historyId")?.unwrap_or_default();
            if next.is_empty() || after.as_deref() == Some(next.as_str()) {
                break;
            }
            after = Some(next);
            page += 1;
        }
        if page >= 100 {
            (host.service.log)(
                "fetchClosedPnl stopped at the 100-page cap; older positions were not fetched",
                LogSeverity::Warning,
            )
            .map_err(ServiceError::Host)?;
        }
        sort_desc(&mut entries, |r| r.closed_time);
        Ok(entries)
    }
    .await;
    match outcome {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(host.service, Some(&error_value(&e, false, None)))?;
            Ok(Vec::new())
        }
    }
}
