//! Coinbase ticker and position read bodies on typed Market/Position fields and plain records.
use crate::{
    portfolios_balances::{Portfolio, PortfolioFailure, PortfolioThrown},
    rest_reads::format_spot_cfm_tickers,
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    utils::normalize_cfm_contract_size,
};
use safe_cex_num::{js_date_parse, js_time_clip, js_truthy, multiply};
use safe_cex_num::{js_retained_number, js_retained_string};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    MarginType, Market, Position, PositionExtra, PositionMode, PositionSide, Ticker,
    model_utils::create_position_id, object_map::JsObjectMap,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Retained inert response-item root: info points at the original item, never a re-encoding.
pub type ReadItem = Rc<RefCell<Input>>;
/// Original response array; CFM for-of iteration reads its live length.
pub type ReadItems = Rc<RefCell<Vec<ReadItem>>>;
pub type ReadMarket = Rc<RefCell<Market>>;
pub type ReadMarkets = Rc<RefCell<Vec<ReadMarket>>>;
/// Native typed scalar fields and the source's shared info handle. fields.info stays absent.
#[derive(Debug)]
pub struct ReadTicker {
    pub fields: Ticker,
    pub info: Option<ReadItem>,
}
#[derive(Debug)]
pub struct ReadPosition {
    pub fields: Position,
    pub info: Option<ReadItem>,
}
pub type ReadTickers = Rc<RefCell<Vec<Rc<ReadTicker>>>>;
pub type ReadPositions = Rc<RefCell<Vec<Rc<ReadPosition>>>>;
/// Closed response projection: items are the declared products/positions/instruments array.
/// Host parsing supplies the original retained item handles and valid declared DTO fields.
#[derive(Clone)]
pub struct ReadReply {
    pub items: ReadItems,
}
pub trait MarketPositionHost {
    fn get(
        &self,
        endpoint: &'static str,
        params: Option<Value>,
    ) -> Promise<Result<ReadReply, PortfolioFailure>>;
    /// Original response.data items carried by a rejected typed HTTP response.
    fn error_response_items(&self, err: &PortfolioFailure) -> Option<ReadItems>;
    fn get_supported_types(&self) -> Promise<Result<Vec<String>, PortfolioFailure>>;
    fn markets(&self) -> Result<ReadMarkets, PortfolioFailure>;
    fn market_by_id(&self, id: &str) -> Result<Option<ReadMarket>, PortfolioFailure>;
    fn cached_tickers(&self) -> Result<ReadTickers, PortfolioFailure>;
    fn cached_positions(&self) -> Result<ReadPositions, PortfolioFailure>;
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure>;
    /// Pure host timezone/DST conversion used by the native Date parser.
    fn local_wall_time_to_utc(&self, millis: f64) -> f64;
}
pub struct MarketPositionReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
    pub selected: Rc<RefCell<Option<Portfolio>>>,
}
const PRODUCTS: &str = "/api/v3/brokerage/products";
const INSTRUMENTS: &str = "https://api.international.coinbase.com/api/v1/instruments/";
const CFM_POSITIONS: &str = "/api/v3/brokerage/cfm/positions";
const INTX_POSITIONS: &str = "/api/v3/brokerage/intx/positions/{portfolio_uuid}";
fn error(name: &str, message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":name,"message":message}),
    )))
}
fn detail(err: &PortfolioFailure) -> Option<Value> {
    let remote = err
        .value
        .as_ref()
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("message"));
    if js_truthy(remote) {
        remote.cloned()
    } else {
        err.value.as_ref().and_then(|v| v.get("message")).cloned()
    }
}
fn prop<'a>(value: Option<&'a Input>, key: &str) -> Result<Option<&'a Input>, PortfolioFailure> {
    match value {
        None | Some(Input::Null | Input::Undefined) => Err(error(
            "TypeError",
            &format!(
                "Cannot read properties of {} (reading '{key}')",
                if value.is_none() { "undefined" } else { "null" }
            ),
        )),
        Some(value) => Ok(value.get(key)),
    }
}
fn has_length(value: Option<&Input>) -> bool {
    match value {
        Some(Input::Array(v)) => !v.is_empty(),
        Some(Input::Text(v)) => !v.is_empty(),
        Some(Input::Object(_)) => value
            .and_then(|v| v.get("length"))
            .is_some_and(Input::is_truthy),
        _ => false,
    }
}
fn float(value: Option<&Input>) -> Result<f64, PortfolioFailure> {
    safe_cex_num::js_retained_parse_float(value).map_err(|e| error(e.name, e.message))
}
fn text(value: Option<&Input>) -> Result<String, PortfolioFailure> {
    js_retained_string(value).map_err(|e| error(&e.name, &e.message))
}
fn or<'a>(value: Option<&'a Input>, fallback: &'a Input) -> Option<&'a Input> {
    if value.is_some_and(Input::is_truthy) {
        value
    } else {
        Some(fallback)
    }
}
fn date<H: MarketPositionHost>(host: &H, value: Option<&Input>) -> Result<f64, PortfolioFailure> {
    match value {
        Some(Input::Text(_) | Input::Array(_) | Input::Object(_)) => js_retained_string(value)
            .map(|v| js_date_parse(&v, |ms| host.local_wall_time_to_utc(ms)))
            .map_err(|e| error(&e.name, &e.message)),
        _ => js_retained_number(value)
            .map(js_time_clip)
            .map_err(|e| error(&e.name, &e.message)),
    }
}
fn ticker(id: String) -> Ticker {
    Ticker {
        id: id.clone(),
        symbol: id,
        bid: 0.0,
        ask: 0.0,
        last: 0.0,
        mark: 0.0,
        index: 0.0,
        mid_price: None,
        percentage: 0.0,
        open_interest: 0.0,
        funding_rate: 0.0,
        funding_time: 0.0,
        volume: 0.0,
        quote_volume: 0.0,
        timestamp: None,
        info: None,
    }
}
fn position(symbol: String) -> Position {
    Position {
        id: create_position_id(&symbol, None, None),
        symbol,
        side: safe_cex_types::ModelField::Value(PositionSide::Long),
        entry_price: 0.0,
        notional: 0.0,
        leverage: 10.0,
        margin_type: (Some(MarginType::Cross)).into(),
        position_mode: (Some(PositionMode::OneWay)).into(),
        unrealized_pnl: 0.0,
        realized_pnl: (None).into(),
        contracts: 0.0,
        liquidation_price: 0.0,
        margin_size: (None).into(),
        info: (None).into(),
        extra: (None).into(),
        last_update: (None).into(),
        option: (None).into(),
    }
}
fn all<T: Clone + 'static>(
    promises: Vec<Promise<Result<T, PortfolioFailure>>>,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> Promise<Result<Vec<T>, PortfolioFailure>> {
    let (promise, resolve) = Promise::pending();
    if promises.is_empty() {
        resolve.resolve(Ok(vec![]));
        return promise;
    }
    let count = Rc::new(Cell::new(promises.len()));
    let values = Rc::new(RefCell::new(vec![None; promises.len()]));
    for (i, child) in promises.into_iter().enumerate() {
        let (resolve, count, values) = (resolve.clone(), count.clone(), values.clone());
        child.observe(spawn, move |result| match result {
            Err(err) => resolve.resolve(Err(err)),
            Ok(value) => {
                values.borrow_mut()[i] = Some(value);
                count.set(count.get() - 1);
                if count.get() == 0 {
                    resolve.resolve(Ok(values
                        .borrow()
                        .iter()
                        .map(|v| v.clone().expect("settled child"))
                        .collect()));
                }
            }
        });
    }
    promise
}
fn collect_tickers(values: Vec<ReadTickers>) -> ReadTickers {
    Rc::new(RefCell::new(
        values.iter().flat_map(|v| v.borrow().clone()).collect(),
    ))
}
fn collect_positions(values: Vec<ReadPositions>) -> ReadPositions {
    Rc::new(RefCell::new(
        values.iter().flat_map(|v| v.borrow().clone()).collect(),
    ))
}
pub fn fetch_tickers<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadTickers, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let supported = r.host.get_supported_types().await?;
            let requests = supported
                .iter()
                .map(|kind| match kind.as_str() {
                    "spot" => fetch_tickers_spot(r.clone()),
                    "cfm" => fetch_tickers_cfm(r.clone()),
                    _ => fetch_tickers_intx(r.clone()),
                })
                .collect();
            Ok(collect_tickers(all(requests, &r.spawn).await?))
        }),
        &spawn,
    )
}
fn spot_cfm<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
    params: Option<Value>,
) -> Promise<Result<ReadTickers, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let result = async {
                let reply = r.host.get(PRODUCTS, params).await?;
                let products: Vec<Input> = reply
                    .items
                    .borrow()
                    .iter()
                    .map(|p| p.borrow().clone())
                    .collect();
                let fields = format_spot_cfm_tickers(&Input::Object(vec![(
                    "products".encode_utf16().collect(),
                    Input::Array(products),
                )]))
                .map_err(|e| error("Error", &e))?;
                Ok(Rc::new(RefCell::new(
                    fields
                        .into_iter()
                        .map(|fields| Rc::new(ReadTicker { fields, info: None }))
                        .collect(),
                )))
            }
            .await;
            match result {
                Ok(v) => Ok(v),
                Err(err) => {
                    r.host.emit_and_log_error(&[detail(&err)])?;
                    Ok(r.host.cached_tickers()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_tickers_spot<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadTickers, PortfolioFailure>> {
    spot_cfm(r, None)
}
pub fn fetch_tickers_cfm<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadTickers, PortfolioFailure>> {
    spot_cfm(r, Some(json!({"product_type":"FUTURE"})))
}
pub fn fetch_tickers_intx<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadTickers, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let result = async {
                let items = match r.host.get(INSTRUMENTS, None).await {
                    Ok(reply) => Ok(reply.items),
                    Err(err) => {
                        let rejection = err.value.clone().map(Input::from);
                        let response = prop(rejection.as_ref(), "response")?;
                        let data = response.and_then(|v| v.get("data"));
                        if data.is_some_and(|value| value.as_array().is_some()) {
                            let items = r
                                .host
                                .error_response_items(&err)
                                .expect("typed rejected response retains original items");
                            if items.borrow().is_empty() {
                                return Err(err);
                            }
                            Ok(items)
                        } else if has_length(data) {
                            // Source adopts this body before evaluating .filter.
                            Err(error("TypeError", "data.filter is not a function"))
                        } else {
                            return Err(err);
                        }
                    }
                };
                let mut markets = JsObjectMap::new();
                for market in r.host.markets()?.borrow().iter() {
                    let symbol = market.borrow().symbol.clone();
                    markets.insert(symbol, market.clone());
                }
                let mut tickers = vec![];
                for item in items?.borrow().clone() {
                    let m = item.borrow();
                    if !m
                        .get("trading_state")
                        .is_some_and(|v| v.text_equals("TRADING"))
                        || !m.get("type").is_some_and(|v| v.text_equals("PERP"))
                    {
                        continue;
                    }
                    let symbol = format!("{}-INTX", text(m.get("symbol"))?);
                    let raw = markets.get(&symbol).and_then(|m| m.borrow().info.clone());
                    let zero = Input::from(json!("0"));
                    let interval = float(or(m.get("funding_interval"), &zero))? / 1000.0 / 1000.0;
                    let quote = m.get("quote");
                    let mut fields = ticker(symbol);
                    fields.bid = float(prop(quote, "best_bid_price")?)?;
                    fields.ask = float(prop(quote, "best_ask_price")?)?;
                    fields.last = float(prop(quote, "trade_price")?)?;
                    fields.mark = float(prop(quote, "mark_price")?)?;
                    fields.index = float(prop(quote, "index_price")?)?;
                    fields.percentage = float(or(
                        raw.as_ref()
                            .and_then(|v| v.get("price_percentage_change_24h")),
                        &zero,
                    ))?;
                    fields.funding_rate = float(prop(quote, "predicted_funding")?)?;
                    fields.funding_time = date(
                        &r.host,
                        raw.as_ref()
                            .and_then(|v| v.get("future_product_details"))
                            .and_then(|v| v.get("perpetual_details"))
                            .and_then(|v| v.get("funding_time")),
                    )? + interval;
                    fields.volume = float(m.get("qty_24hr"))?;
                    fields.quote_volume = float(m.get("notional_24hr"))?;
                    fields.open_interest = float(m.get("open_interest"))?;
                    tickers.push(Rc::new(ReadTicker {
                        fields,
                        info: Some(item.clone()),
                    }));
                }
                Ok(Rc::new(RefCell::new(tickers)))
            }
            .await;
            match result {
                Ok(v) => Ok(v),
                Err(err) => {
                    r.host.emit_and_log_error(&[detail(&err)])?;
                    Ok(r.host.cached_tickers()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_tickers_v3<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadTickers, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let result = (|| {
                let mut tickers = vec![];
                let zero = Input::from(json!("0"));
                for market in r.host.markets()?.borrow().iter() {
                    let m = market.borrow();
                    let info = m.info.as_ref().ok_or_else(|| {
                        error(
                            "TypeError",
                            "Cannot read properties of undefined (reading 'price')",
                        )
                    })?;
                    if info.is_nullish() {
                        return Err(error(
                            "TypeError",
                            "Cannot read properties of null (reading 'price')",
                        ));
                    }
                    let details = info
                        .get("future_product_details")
                        .and_then(|v| v.get("perpetual_details"));
                    let mut fields = ticker(m.symbol.clone());
                    fields.id = m.id.clone();
                    fields.bid = float(info.get("price"))?;
                    fields.ask = float(info.get("price"))?;
                    fields.last = float(info.get("price"))?;
                    fields.mark = float(info.get("price"))?;
                    fields.index = float(info.get("price"))?;
                    fields.percentage = float(info.get("price_percentage_change_24h"))?;
                    fields.funding_rate =
                        float(or(details.and_then(|v| v.get("funding_rate")), &zero))?;
                    fields.funding_time =
                        float(or(details.and_then(|v| v.get("funding_time")), &zero))?;
                    fields.volume = float(info.get("volume_24h"))?;
                    fields.open_interest =
                        float(or(details.and_then(|v| v.get("open_interest")), &zero))?;
                    tickers.push(Rc::new(ReadTicker { fields, info: None }));
                }
                Ok(Rc::new(RefCell::new(tickers)))
            })();
            match result {
                Ok(v) => Ok(v),
                Err(err) => {
                    r.host.emit_and_log_error(&[detail(&err)])?;
                    Ok(r.host.cached_tickers()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_open_positions<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadPositions, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let supported = r.host.get_supported_types().await?;
            let requests = supported
                .iter()
                .map(|kind| match kind.as_str() {
                    "spot" => Promise::resolved(Ok(Rc::new(RefCell::new(vec![])))),
                    "cfm" => fetch_open_positions_cfm(r.clone()),
                    _ => fetch_open_positions_intx(r.clone()),
                })
                .collect();
            Ok(collect_positions(all(requests, &r.spawn).await?))
        }),
        &spawn,
    )
}
pub fn fetch_positions<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadPositions, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let open = fetch_open_positions(r.clone()).await?;
            let markets = r.host.markets()?;
            let missing: Vec<_> = markets
                .borrow()
                .iter()
                .filter(|m| {
                    !open
                        .borrow()
                        .iter()
                        .any(|p| p.fields.symbol == m.borrow().symbol)
                })
                .cloned()
                .collect();
            let mut positions = open.borrow().clone();
            for market in missing {
                let fields = position(market.borrow().symbol.clone());
                positions.push(Rc::new(ReadPosition {
                    fields,
                    info: Some(Rc::new(RefCell::new(Input::Object(vec![])))),
                }));
            }
            Ok(Rc::new(RefCell::new(positions)))
        }),
        &spawn,
    )
}
pub fn fetch_open_positions_cfm<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadPositions, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let result: Result<ReadPositions, PortfolioFailure> = async {
            let reply = r.host.get(CFM_POSITIONS, None).await?;
            let mut positions = vec![];
            let mut index=0;
            loop {
                let next=reply.items.borrow().get(index).cloned();
                let Some(item)=next else {break;};
                index+=1;
                let lookup_id = text(item.borrow().get("product_id"))?;
                let market = r.host.market_by_id(&lookup_id)?;
                let contract = market.and_then(|m| {
                    let m = m.borrow();
                    if m.is_expiring == Some(true) {m.contract_size.and_then(|size| normalize_cfm_contract_size(&Input::Float(size)))} else {None}
                });
                let Some(contract) = contract else {
                    let symbol = text(item.borrow().get("product_id"))?;
                    let message = format!("Coinbase CFM position skipped: missing or invalid contract size for {symbol}");
                    r.host.emit_and_log_error(&[Some(Value::String(message))])?;
                    continue;
                };
                let p = item.borrow();
                let mut fields = position(text(p.get("product_id"))?);
                fields.side = if p.get("side").is_some_and(|v| v.text_equals("LONG")) {safe_cex_types::ModelField::Value(PositionSide::Long)} else {safe_cex_types::ModelField::Value(PositionSide::Short)};
                fields.entry_price = float(p.get("avg_entry_price"))?;
                let mark = float(p.get("current_price"))?;
                let size = float(p.get("number_of_contracts"))?.abs();
                fields.unrealized_pnl = float(p.get("unrealized_pnl"))?;
                fields.notional = size * mark * contract;
                fields.contracts = multiply(size, contract);
                fields.extra = (Some(PositionExtra {contracts_native: Some(size), unrealized_pnl_native: None})).into();
                positions.push(Rc::new(ReadPosition {fields, info: Some(item.clone())}));
            }
            Ok(Rc::new(RefCell::new(positions)))
        }.await;
            match result {
                Ok(v) => Ok(v),
                Err(err) => {
                    r.host
                        .emit_and_log_error(&[err.value.clone(), detail(&err)])?;
                    Ok(r.host.cached_positions()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_open_positions_intx<H: MarketPositionHost + 'static>(
    r: Rc<MarketPositionReceiver<H>>,
) -> Promise<Result<ReadPositions, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let selected = r.selected.borrow().clone();
            let Some(selected) = selected.filter(|p| p.borrow().r#type == "INTX") else {
                r.host
                    .emit_and_log_error(&[Some(json!("No INTX portfolio found"))])?;
                return r.host.cached_positions();
            };
            let uuid = selected.borrow().uuid.clone();
            let result: Result<ReadPositions, PortfolioFailure> = async {
                let reply = r
                    .host
                    .get(INTX_POSITIONS, Some(json!({"portfolio_uuid":uuid})))
                    .await?;
                let mut positions = vec![];
                let ten = Input::from(json!("10"));
                for item in reply.items.borrow().clone() {
                    let p = item.borrow();
                    let mut fields = position(text(p.get("symbol"))?);
                    fields.side = if p
                        .get("position_side")
                        .is_some_and(|v| v.text_equals("POSITION_SIDE_LONG"))
                    {
                        safe_cex_types::ModelField::Value(PositionSide::Long)
                    } else {
                        safe_cex_types::ModelField::Value(PositionSide::Short)
                    };
                    fields.entry_price = float(prop(p.get("entry_vwap"), "value")?)?;
                    let mark = float(prop(p.get("mark_price"), "value")?)?;
                    let size = float(p.get("net_size"))?.abs();
                    fields.unrealized_pnl = (if fields.side == PositionSide::Long {
                        1.0
                    } else {
                        -1.0
                    }) * size
                        * (mark - fields.entry_price);
                    fields.notional = size * mark;
                    fields.contracts = size;
                    fields.leverage = float(or(p.get("leverage"), &ten))?;
                    fields.liquidation_price = float(prop(p.get("liquidation_price"), "value")?)?;
                    positions.push(Rc::new(ReadPosition {
                        fields,
                        info: Some(item.clone()),
                    }));
                }
                Ok(Rc::new(RefCell::new(positions)))
            }
            .await;
            match result {
                Ok(v) => Ok(v),
                Err(err) => {
                    r.host
                        .emit_and_log_error(&[err.value.clone(), detail(&err)])?;
                    Ok(r.host.cached_positions()?)
                }
            }
        }),
        &spawn,
    )
}
