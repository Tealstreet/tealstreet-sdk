//! Position-risk and Futures account snapshot bodies over current configured HTTP,
//! Store, ID and emission/logging operations. Dense own-data arrays/String symbols;
//! immutable Error failures. Response getters, concrete class/Store/socket wiring,
//! sparse/executable properties and detached async ownership remain caller boundaries.
use crate::{
    private_event_value::*,
    private_rest::{
        SnapshotClient, SnapshotRequestFuture, error_message, keyed, map, pair, pm_rows,
    },
    request_weight::{Failure, type_error},
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_types::{MarginType, PositionMode, PositionSide};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};
pub type Emit = Rc<dyn Fn(EventValue) -> Result<(), Failure>>;
pub type Log = Rc<dyn Fn(String, LogSeverity) -> Result<(), Failure>>;
pub trait PositionSnapshotBindings {
    fn get(
        &self,
        client: SnapshotClient,
        endpoint: &'static str,
    ) -> Result<SnapshotRequestFuture, Failure>;
    fn positions(&self) -> Result<EventValue, Failure>;
    fn balance(&self) -> Result<EventValue, Failure>;
    fn markets(&self) -> Result<EventValue, Failure>;
    fn tickers(&self) -> Result<EventValue, Failure>;
    fn hedged(&self) -> Result<bool, Failure>;
    /// Actual createPositionId adapter; String symbols produce String IDs.
    fn id(&self, symbol: String, side: EventValue, mode: PositionMode) -> Result<String, Failure>;
    fn capture_emit(&self) -> Result<Emit, Failure>;
    fn capture_log(&self) -> Result<Log, Failure>;
}
fn endpoint(pm: bool, name: &str) -> &'static str {
    let entries = if pm {
        crate::binancepm_types::ENDPOINTS
    } else {
        crate::binance_types::ENDPOINTS
    };
    entries
        .iter()
        .find(|(k, _)| *k == name)
        .expect("generated endpoint")
        .1
}
fn array(v: Vec<EventValue>) -> EventValue {
    EventValue::array(v)
}
fn opt(v: EventValue, k: &str) -> Result<EventValue, Failure> {
    if v.nullish() {
        Ok(EventValue::Undefined)
    } else {
        v.read(k)
    }
}
fn sides(p: &EventValue, pm: bool) -> Result<(f64, EventValue, PositionMode), Failure> {
    let raw = p.read("positionSide")?;
    let contracts = p.read("positionAmt")?.float()?;
    let entries = if pm {
        crate::binancepm_types::POSITION_SIDE
    } else {
        crate::binance_types::POSITION_SIDE
    };
    let side = fallback(
        table(entries, raw, PrivateEnum::PositionSide)?,
        EventValue::enum_value(PrivateEnum::PositionSide(if contracts > 0.0 {
            PositionSide::Long
        } else {
            PositionSide::Short
        })),
    );
    let mode = if p.read("positionSide")?.same(&string("BOTH")) {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    };
    Ok((contracts, side, mode))
}
fn leverage(config: EventValue) -> Result<f64, Failure> {
    fallback(opt(config, "leverage")?, string("0")).float()
}
fn margin(config: EventValue) -> Result<EventValue, Failure> {
    Ok(EventValue::enum_value(PrivateEnum::Margin(
        if opt(config, "marginType")?.same(&string("ISOLATED")) {
            MarginType::Isolated
        } else {
            MarginType::Cross
        },
    )))
}
fn dense(v: &EventValue, method: &str, expr: &str) -> Result<usize, Failure> {
    if let EventValue::Array(a) = v {
        Ok(a.borrow().len())
    } else if v.nullish() {
        Err(type_error(format!(
            "Cannot read properties of {} (reading '{method}')",
            v.text()?
        )))
    } else {
        Err(type_error(format!("{expr}.{method} is not a function")))
    }
}
fn unique(rows: Vec<EventValue>) -> Result<EventValue, Failure> {
    let mut seen = HashSet::new();
    let mut result = vec![];
    for row in rows {
        if seen.insert(row.read("id")?.text()?) {
            result.push(row)
        }
    }
    Ok(array(result))
}
fn emit(e: &Failure, b: &dyn PositionSnapshotBindings) -> Result<(), Failure> {
    let call = b.capture_emit()?;
    call(error_message(e))
}
// ts: BinanceExchange.fetchPositions
pub async fn fetch_futures_positions(
    b: Rc<dyn PositionSnapshotBindings>,
) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let a = b.get(SnapshotClient::Private, endpoint(false, "POSITION_RISK"))?;
        let c = b.get(SnapshotClient::Private, endpoint(false, "SYMBOL_CONFIG"))?;
        let (data, configs) = pair(a, c).await?;
        let by_symbol = EventValue::record_object(Default::default());
        map(configs, "symbolConfigs", |c| {
            by_symbol.write(&c.read("symbol")?.text()?, c);
            Ok(EventValue::Undefined)
        })?;
        let formatted = map(data, "data", |p| {
            let (contracts, side, mode) = sides(&p, false)?;
            let config = by_symbol.get(&p.read("symbol")?.text()?)?;
            let margin_type = margin(config.clone())?;
            let notional = p.read("notional")?.float()?.abs();
            let lev = leverage(config)?;
            let isolated = fallback(keyed(&p, "isolatedMargin")?, string("0")).float()?;
            let size = if isolated > 0.0 {
                number(isolated)
            } else if lev > 0.0 {
                number(notional.abs() / lev)
            } else {
                EventValue::Undefined
            };
            let id = b.id(p.read("symbol")?.text()?, side.clone(), mode)?;
            Ok(record([
                ("id", string(id)),
                ("symbol", p.read("symbol")?),
                ("side", side),
                ("entryPrice", number(p.read("entryPrice")?.float()?)),
                ("notional", number(notional)),
                ("leverage", number(lev)),
                (
                    "unrealizedPnl",
                    number(p.read("unRealizedProfit")?.float()?),
                ),
                ("contracts", number(contracts.abs())),
                (
                    "liquidationPrice",
                    number(p.read("liquidationPrice")?.float()?),
                ),
                ("marginSize", size),
                ("marginType", margin_type),
                (
                    "positionMode",
                    EventValue::enum_value(PrivateEnum::PositionMode(mode)),
                ),
                ("info", p),
            ]))
        })?;
        let mode = if b.hedged()? {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        };
        let markets = b.markets()?;
        let n = dense(&markets, "flatMap", "this.store.markets")?;
        let EventValue::Array(formatted) = formatted else {
            unreachable!()
        };
        let mut rows = array_values(&formatted);
        for i in 0..n {
            let market = markets.get(&i.to_string())?;
            for side in [PositionSide::Long, PositionSide::Short] {
                let config = by_symbol.get(&market.read("symbol")?.text()?)?;
                let lev = leverage(config.clone())?;
                let side = EventValue::enum_value(PrivateEnum::PositionSide(side));
                let id = b.id(market.read("symbol")?.text()?, side.clone(), mode)?;
                rows.push(record([
                    ("id", string(id)),
                    ("symbol", market.read("symbol")?),
                    ("side", side),
                    ("entryPrice", number(0.0)),
                    ("notional", number(0.0)),
                    ("leverage", number(lev)),
                    ("unrealizedPnl", number(0.0)),
                    ("contracts", number(0.0)),
                    ("liquidationPrice", number(0.0)),
                    ("marginSize", number(0.0)),
                    ("marginType", margin(config.clone())?),
                    (
                        "positionMode",
                        EventValue::enum_value(PrivateEnum::PositionMode(mode)),
                    ),
                    (
                        "info",
                        record([
                            ("synthetic", EventValue::Bool(true)),
                            ("symbolConfig", config),
                        ]),
                    ),
                ]));
            }
        }
        unique(rows)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(&e, b.as_ref())?;
            b.positions()
        }
    }
}
// ts: BinancePMExchange.fetchPositions
pub async fn fetch_pm_positions(
    b: Rc<dyn PositionSnapshotBindings>,
) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let data = b
            .get(SnapshotClient::Papi, endpoint(true, "POSITION_RISK"))?
            .await
            .result?;
        let rows = pm_rows(data, endpoint(true, "POSITION_RISK"))?;
        let formatted = map(rows, "rows", |p| {
            let (contracts, side, mode) = sides(&p, true)?;
            let notional = p.read("notional")?.float()?.abs();
            let lev = p.read("leverage")?.float()?;
            let id = b.id(p.read("symbol")?.text()?, side.clone(), mode)?;
            Ok(record([
                ("id", string(id)),
                ("symbol", p.read("symbol")?),
                ("side", side),
                ("entryPrice", number(p.read("entryPrice")?.float()?)),
                ("notional", number(notional)),
                ("leverage", number(lev)),
                (
                    "unrealizedPnl",
                    number(p.read("unRealizedProfit")?.float()?),
                ),
                ("contracts", number(contracts.abs())),
                (
                    "liquidationPrice",
                    number(p.read("liquidationPrice")?.float()?),
                ),
                (
                    "marginSize",
                    if lev > 0.0 {
                        number(notional / lev)
                    } else {
                        EventValue::Undefined
                    },
                ),
                (
                    "marginType",
                    EventValue::enum_value(PrivateEnum::Margin(MarginType::Cross)),
                ),
                (
                    "positionMode",
                    EventValue::enum_value(PrivateEnum::PositionMode(mode)),
                ),
                ("info", p),
            ]))
        })?;
        let mode = if b.hedged()? {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        };
        let markets = b.markets()?;
        let n = dense(&markets, "reduce", "this.store.markets")?;
        let EventValue::Array(formatted) = formatted else {
            unreachable!()
        };
        let mut rows = array_values(&formatted);
        for i in 0..n {
            let market = markets.get(&i.to_string())?;
            for side in [PositionSide::Long, PositionSide::Short] {
                let side = EventValue::enum_value(PrivateEnum::PositionSide(side));
                let id = b.id(market.read("symbol")?.text()?, side.clone(), mode)?;
                rows.push(record([
                    ("id", string(id)),
                    ("symbol", market.read("symbol")?),
                    ("side", side),
                    ("entryPrice", number(0.0)),
                    ("notional", number(0.0)),
                    ("unrealizedPnl", number(0.0)),
                    ("contracts", number(0.0)),
                    ("leverage", number(10.0)),
                    ("liquidationPrice", number(0.0)),
                    (
                        "positionMode",
                        EventValue::enum_value(PrivateEnum::PositionMode(mode)),
                    ),
                    (
                        "marginType",
                        EventValue::enum_value(PrivateEnum::Margin(MarginType::Cross)),
                    ),
                ]));
            }
        }
        unique(rows)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(&e, b.as_ref())?;
            b.positions()
        }
    }
}
const STABLE: &[&str] = &["USDT", "USDC", "BUSD", "FDUSD", "BFUSD", "LDUSDT", "BNFCR"];
fn stable(v: &EventValue) -> bool {
    STABLE.iter().any(|s| v.same(&string(*s)))
}
struct Detail {
    asset: EventValue,
    wallet: f64,
    usd: f64,
    price: Option<EventValue>,
}
fn positive(wallet: f64, available: f64, initial: f64, upnl: f64) -> bool {
    wallet > 0.0 || available > 0.0 || initial > 0.0 || upnl.abs() > 0.0
}
fn find_market(
    b: &dyn PositionSnapshotBindings,
    asset: &EventValue,
    btc: bool,
) -> Result<EventValue, Failure> {
    let markets = b.markets()?;
    let n = dense(&markets, "find", "this.store.markets")?;
    for i in 0..n {
        let m = markets.get(&i.to_string())?;
        if m.read("base")?.same(&asset.read("asset")?) {
            let q = m.read("quote")?;
            if if btc {
                q.same(&string("BTC"))
            } else {
                ["USDT", "USDC", "BUSD"].iter().any(|s| q.same(&string(*s)))
            } {
                return Ok(m);
            }
        }
    }
    Ok(EventValue::Undefined)
}
fn fixed(v: f64) -> String {
    safe_cex_num::js_to_fixed(v, 2.0).expect("constant toFixed digits")
}
fn price_fixed(v: Option<&EventValue>) -> Result<String, Failure> {
    match v {
        None | Some(EventValue::Undefined | EventValue::Null) => Ok("undefined".into()),
        Some(EventValue::Number(n)) => Ok(fixed(*n)),
        _ => Err(type_error("d.price?.toFixed is not a function")),
    }
}
fn error_text(e: &Failure) -> String {
    if e.name.is_empty() {
        e.message.clone()
    } else if e.message.is_empty() {
        e.name.clone()
    } else {
        format!("{}: {}", e.name, e.message)
    }
}
// ts: BinanceExchange.fetchPositionsAndBalance
pub async fn fetch_futures_positions_and_balance(
    has_logged: &Cell<bool>,
    b: Rc<dyn PositionSnapshotBindings>,
) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let data = b
            .get(SnapshotClient::Private, endpoint(false, "ACCOUNT"))?
            .await
            .result?;
        let (mut total, mut free, mut upnl, mut used) = (0.0, 0.0, 0.0, 0.0);
        let mut details = vec![];
        let aggregation: Result<(), Failure> = (|| {
            let assets = fallback(opt(data.clone(), "assets")?, array(vec![]));
            let a = match assets {
                EventValue::Array(a)=>a,
                EventValue::String(s)=>Rc::new(RefCell::new(EventValue::String(s.clone()).text()?.chars().map(|c|Some(string(c.to_string()))).collect())),
                v=>return Err(type_error(match &v {
                    EventValue::Bool(x)=>format!("boolean {x} is not iterable (cannot read property Symbol(Symbol.iterator))"),
                    EventValue::Number(x)=>format!("number {} is not iterable (cannot read property Symbol(Symbol.iterator))",safe_cex_num::js_number_string(*x)),
                    _=>"object is not iterable (cannot read property Symbol(Symbol.iterator))".into(),
                })),
            };
            let mut i = 0;
            while i < a.borrow().len() {
                let asset = a.borrow()[i].clone().unwrap_or(EventValue::Undefined);
                i += 1;
                let wallet = fallback(asset.read("walletBalance")?, string("0")).float()?;
                let available = fallback(asset.read("availableBalance")?, string("0")).float()?;
                let initial = fallback(asset.read("initialMargin")?, string("0")).float()?;
                let unrealized = fallback(asset.read("unrealizedProfit")?, string("0")).float()?;
                if wallet == 0.0 && available == 0.0 && initial == 0.0 && unrealized.abs() == 0.0 {
                    continue;
                }
                if stable(&asset.read("asset")?) {
                    total += wallet;
                    free += available;
                    used += initial;
                    upnl += unrealized;
                    details.push(Detail {
                        asset: asset.read("asset")?,
                        wallet,
                        usd: wallet,
                        price: None,
                    });
                } else {
                    let mut price = number(0.0);
                    if positive(wallet, available, initial, unrealized) {
                        let direct = find_market(b.as_ref(), &asset, false)?;
                        if direct.truthy() {
                            let symbol = direct.read("symbol")?;
                            let ticker = b.tickers()?.get(&symbol.text()?)?;
                            if ticker.truthy() && ticker.read("last")?.number()? > 0.0 {
                                price = ticker.read("last")?;
                            }
                        }
                        if price.same(&number(0.0)) && !asset.read("asset")?.same(&string("BTC")) {
                            let market = find_market(b.as_ref(), &asset, true)?;
                            if market.truthy() {
                                let btc = b.tickers()?.get(&market.read("symbol")?.text()?)?;
                                let tickers = b.tickers()?;
                                let first = tickers.read("BTCUSDT")?;
                                let quote = if first.truthy() {
                                    first
                                } else {
                                    b.tickers()?.read("BTCUSDC")?
                                };
                                if btc.truthy()
                                    && btc.read("last")?.number()? > 0.0
                                    && quote.truthy()
                                    && quote.read("last")?.number()? > 0.0
                                {
                                    price = number(btc.read("last")?.number()? * quote.read("last")?.number()?);
                                    let _symbol = format!(
                                        "{} -> {}",
                                        market.read("symbol")?.text()?,
                                        quote.read("symbol")?.text()?
                                    );
                                }
                            }
                        }
                    }
                    let numeric_price=price.number()?;
                    if numeric_price > 0.0 {
                        let usd = wallet * numeric_price;
                        total += usd;
                        free += available * numeric_price;
                        used += initial * numeric_price;
                        upnl += unrealized * numeric_price;
                        details.push(Detail {
                            asset: asset.read("asset")?,
                            wallet,
                            usd,
                            price: Some(price),
                        });
                    } else if positive(wallet, available, initial, unrealized) {
                        details.push(Detail {
                            asset: asset.read("asset")?,
                            wallet,
                            usd: 0.0,
                            price: Some(number(0.0)),
                        });
                    }
                }
            }
            if !has_logged.get() && !details.is_empty() {
                has_logged.set(true);
                let mut message = String::from("Balance aggregation:\n");
                let stable_rows = details
                    .iter()
                    .filter(|d| stable(&d.asset) && d.wallet > 0.0)
                    .collect::<Vec<_>>();
                let other_rows = details
                    .iter()
                    .filter(|d| !stable(&d.asset) && d.wallet > 0.0)
                    .collect::<Vec<_>>();
                if !stable_rows.is_empty() {
                    message += &format!(
                        "  Stablecoins: {}\n",
                        stable_rows
                            .iter()
                            .map(|d| Ok(format!(
                                "{}({})",
                                d.asset.text()?,
                                safe_cex_num::js_number_string(d.wallet)
                            )))
                            .collect::<Result<Vec<_>, Failure>>()?
                            .join(", ")
                    );
                }
                if !other_rows.is_empty() {
                    message += &format!(
                        "  Non-stablecoins: {}\n",
                        other_rows
                            .iter()
                            .map(|d| Ok(format!(
                                "{}({} @ ${} = ${})",
                                d.asset.text()?,
                                safe_cex_num::js_number_string(d.wallet),
                                price_fixed(d.price.as_ref())?,
                                fixed(d.usd)
                            )))
                            .collect::<Result<Vec<_>, Failure>>()?
                            .join(", ")
                    );
                }
                message += &format!(
                    "  Total: ${}, Free: ${}, Used: ${}, UPnL: ${}",
                    fixed(total),
                    fixed(free),
                    fixed(used),
                    fixed(upnl)
                );
                b.capture_log()?(message, LogSeverity::Info)?;
            }
            Ok(())
        })();
        if let Err(e) = aggregation {
            let log = b.capture_log()?;
            log(
                format!("Error aggregating balances: {}", error_text(&e)),
                LogSeverity::Error,
            )?;
            total = fallback(data.read("totalWalletBalance")?, string("0")).float()?;
            free = fallback(data.read("availableBalance")?, string("0")).float()?;
            upnl = fallback(data.read("totalUnrealizedProfit")?, string("0")).float()?;
            used = fallback(data.read("totalInitialMargin")?, string("0")).float()?;
        }
        if data.nullish() {
            return Err(type_error(format!(
                "Cannot destructure property 'positions' of 'data' as it is {}.",
                data.text()?
            )));
        }
        let _positions = data.read("positions")?;
        let mut rest = data.spread();
        rest.remove("positions");
        let balance = record([
            ("total", number(total)),
            ("free", number(free)),
            ("used", number(used)),
            ("upnl", number(upnl)),
            ("info", EventValue::record_object(rest)),
        ]);
        Ok(record([
            ("balance", balance),
            ("positions", b.positions()?),
        ]))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(&e, b.as_ref())?;
            Ok(record([
                ("balance", b.balance()?),
                ("positions", b.positions()?),
            ]))
        }
    }
}
