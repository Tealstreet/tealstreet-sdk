//! REST reads, account writes and the start/tick session, bound to the ported bodies.
use crate::{
    adapter::{
        BitunixVenue, Fault, coercion, from_value, json as adapter_json, reason, to_value, units,
        utf8,
    },
    sockets::Socket,
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{JsJsonValue, js_string};
use safe_cex_store::{StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    AccountValidationError, Balance, Candle, Execution, MarginType, Market, OHLCVOptions, Order,
    Position, StoreDataLoaded, Ticker, ValidateAccountResult,
};
use serde_json::Value;
use std::{cell::OnceCell, rc::Rc};

fn store_err(e: safe_cex_store::StoreMutationError) -> ExchangeError {
    e.into()
}
impl BitunixVenue {
    fn trace(&self, e: &ExchangeError) {
        self.runtime.console("trace", &e.js_error_string());
    }
    /// GET returning axios `response.data`, as the rest_reads bodies consume it.
    fn reader(
        self: &Rc<Self>,
    ) -> impl FnOnce(
        &'static str,
        Value,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<Value>, Fault>>>,
    > {
        let xhr = self.xhr();
        move |endpoint, config| {
            let f = xhr.get(endpoint, JsJsonValue::from(config));
            Box::pin(async move { f.await.map(Some).map_err(Fault) })
        }
    }
    pub(crate) fn fetch_markets(self: &Rc<Self>) -> ExchangeFuture<'static, Vec<Market>> {
        let me = self.clone();
        Box::pin(async move {
            let read = me.reader();
            Ok(crate::rest_reads::fetch_markets(
                || me.base.store.markets().to_vec(),
                read,
                |e: &Fault| me.runtime.console("error", &e.0.js_error_string()),
                |message| me.runtime.console("warning", message),
            )
            .await)
        })
    }
    pub(crate) fn fetch_tickers(self: &Rc<Self>) -> ExchangeFuture<'static, Vec<Ticker>> {
        let me = self.clone();
        Box::pin(async move {
            let read = me.reader();
            let markets: OnceCell<Vec<Market>> = OnceCell::new();
            Ok(crate::rest_reads::fetch_tickers(
                || me.base.store.tickers().to_vec(),
                |symbol| {
                    markets
                        .get_or_init(|| me.base.store.markets().to_vec())
                        .iter()
                        .find(|m| m.symbol == symbol)
                },
                read,
                |e: &Fault| me.trace(&e.0),
            )
            .await)
        })
    }
    pub(crate) fn fetch_balance(
        self: &Rc<Self>,
    ) -> ExchangeFuture<'static, (Balance, Vec<Position>)> {
        let me = self.clone();
        Box::pin(async move {
            let read = me.reader();
            let positions: OnceCell<Vec<Position>> = OnceCell::new();
            let mut mode = me.account_mode();
            let (balance, rows) = crate::rest_reads::fetch_balance(
                || me.base.store.balance().clone(),
                || positions.get_or_init(|| me.base.store.positions().to_vec()),
                &mut mode,
                read,
                |e: &Fault| {
                    me.trace(&e.0);
                    if let Err(report) = me.emit_error(&reason(&e.0)) {
                        me.trace(&report);
                    }
                },
            )
            .await;
            let rows = rows.to_vec();
            me.set_account_mode(mode);
            Ok((balance, rows))
        })
    }
    pub(crate) fn fetch_position_mode(self: &Rc<Self>) -> ExchangeFuture<'static, bool> {
        let me = self.clone();
        Box::pin(async move {
            crate::rest_reads::fetch_position_mode::<Fault, _>(me.reader())
                .await
                .map_err(|e| e.0)
        })
    }
    pub(crate) fn fetch_positions(self: &Rc<Self>) -> ExchangeFuture<'static, Vec<Position>> {
        let me = self.clone();
        Box::pin(async move {
            let read = me.reader();
            Ok(crate::position_reads::fetch_positions(
                read,
                |symbol| me.symbol_setting(symbol),
                || me.account_mode(),
                || {
                    me.base
                        .store
                        .markets()
                        .iter()
                        .map(|m| m.symbol.clone())
                        .collect()
                },
                || me.base.store.positions().to_vec(),
                |e: &Fault| {
                    me.runtime.console(
                        "error",
                        &format!("[Bitunix] fetchPositions failed: {}", e.0.message),
                    )
                },
            )
            .await)
        })
    }
    /// symbolSettings.get(symbol) as (marginType, leverage)
    pub(crate) fn symbol_setting(&self, symbol: &str) -> (Option<MarginType>, Option<f64>) {
        let state = self.state.borrow();
        let Some(JsJsonValue::Object(fields)) = state.symbol_settings.get(symbol) else {
            return (None, None);
        };
        let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
        let margin = match get("marginType") {
            Some(JsJsonValue::String(s)) if s == "isolated" => Some(MarginType::Isolated),
            Some(JsJsonValue::String(s)) if s == "cross" => Some(MarginType::Cross),
            _ => None,
        };
        let leverage = match get("leverage") {
            Some(JsJsonValue::Number(n)) => Some(n),
            Some(JsJsonValue::String(s)) => Some(safe_cex_num::parse_float(&s)),
            _ => None,
        };
        (margin, leverage)
    }
    pub(crate) fn fetch_orders(self: &Rc<Self>) -> ExchangeFuture<'static, Vec<Order>> {
        let me = self.clone();
        Box::pin(async move {
            let regular = me.clone();
            let tpsl = me.clone();
            let (rows, regular_error, tpsl_error) = crate::position_reads::fetch_orders(
                move || async move {
                    crate::order_models::fetch_regular_orders(&mut regular.order_frame()).await
                },
                move || async move {
                    crate::order_models::fetch_tpsl_orders(&mut tpsl.order_frame()).await
                },
            )
            .await;
            // Promise.allSettled discards both rejections; keep a host log line.
            for (label, error) in [
                ("fetchRegularOrders", regular_error),
                ("fetchTpslOrders", tpsl_error),
            ] {
                if let Some(Fault(e)) = error {
                    me.runtime.console(
                        "warn",
                        &format!("[Bitunix] {label} rejected: {}", e.message),
                    );
                }
            }
            me.orders_from(rows)
        })
    }
    /// JS order objects to shared Orders; an unrepresentable row is logged, never dropped silently.
    pub(crate) fn orders_from(
        &self,
        rows: Vec<safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<Vec<Order>, ExchangeError> {
        let mut orders = Vec::new();
        for row in rows {
            let value = row;
            match crate::adapter::from_input::<Order>(value.clone()) {
                Ok(order) => orders.push(order),
                Err(e) => self.log(
                    &format!(
                        "Bitunix order row cannot be represented ({e}): {}",
                        safe_cex_num::js_retained_json_stringify(&value)
                            .map_err(coercion)?
                            .unwrap_or("undefined".into())
                    ),
                    "warning",
                )?,
            }
        }
        Ok(orders)
    }
    pub(crate) fn change_position_mode(
        self: &Rc<Self>,
        hedged: bool,
    ) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move {
            let contracts: Vec<f64> = me
                .base
                .store
                .positions()
                .iter()
                .map(|p| p.contracts)
                .collect();
            let xhr = me.xhr();
            crate::rest_writes::change_position_mode(
                hedged,
                contracts.into_iter().map(Ok),
                |endpoint, body| xhr.post(endpoint, body),
                |key, value| me.base.store.set_setting(key, value).map_err(store_err),
                |literal| me.emit_error(literal),
                |e| me.emit_error(&reason(e)),
            )
            .await
        })
    }
    pub(crate) fn change_margin_type(
        self: &Rc<Self>,
        isolated: bool,
        symbol: Option<String>,
    ) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move {
            let xhr = me.xhr();
            let symbol = symbol.map(|s| units(&s));
            crate::rest_writes::change_margin_type(
                isolated,
                symbol.as_deref(),
                |endpoint, symbol, mode, coin| {
                    let symbol = utf8(symbol);
                    let xhr = xhr.clone();
                    async move {
                        xhr.post(
                            endpoint,
                            JsJsonValue::Object(vec![
                                ("symbol".into(), JsJsonValue::String(symbol?)),
                                ("marginMode".into(), JsJsonValue::String(mode.into())),
                                ("marginCoin".into(), JsJsonValue::String(coin.into())),
                            ]),
                        )
                        .await
                    }
                },
                |message| {
                    me.emit_error(message)?;
                    Ok(ExchangeError::new(message))
                },
                |e| {
                    let message = reason(e);
                    me.emit_error(&message)?;
                    Ok(ExchangeError::new(message))
                },
            )
            .await
        })
    }
    pub(crate) fn set_leverage(
        self: &Rc<Self>,
        symbol: String,
        leverage: f64,
    ) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move {
            let markets = me.base.store.markets().to_vec();
            let positions = me.base.store.positions().to_vec();
            let xhr = me.xhr();
            crate::rest_writes::set_leverage::<Fault, _, _>(
                &symbol,
                leverage,
                (
                    |s| Ok(markets.iter().find(|m| m.symbol == s)),
                    |s| Ok(positions.iter().find(|p| p.symbol == s)),
                ),
                |endpoint, body| {
                    let f = xhr.post(endpoint, body);
                    async move { f.await.map_err(Fault) }
                },
                |updates| {
                    let rows: Vec<(Option<String>, ModelPatch)> = updates
                        .into_iter()
                        .map(|(id, leverage)| {
                            let mut patch = ModelPatch::default();
                            patch.set_number("leverage", leverage);
                            (Some(id), patch)
                        })
                        .collect();
                    me.base
                        .store
                        .update_positions(&rows)
                        .map_err(|e| Fault(store_err(e)))
                },
                |e| me.emit_error(&reason(&e.0)).map_err(Fault),
            )
            .await
            .map_err(|e| e.0)
        })
    }
    pub(crate) fn validate_account(
        self: &Rc<Self>,
    ) -> ExchangeFuture<'static, ValidateAccountResult> {
        let me = self.clone();
        Box::pin(async move {
            let xhr = me.xhr();
            let balance = me.clone();
            let outcome = crate::history_rest::validate_account(
                |endpoint, config| xhr.get(endpoint, config),
                |data| {
                    crate::row_extraction::extract_account_row(Some(data), "USDT")
                        .map(drop)
                        .map_err(coercion)
                },
                |message| {
                    let mut e = ExchangeError::new(message);
                    e.name = "TypeError".into();
                    e
                },
                |e| Ok(JsJsonValue::String(reason(e))),
                |message| {
                    me.emit_error(&js_string(Some(&adapter_json(message)?)).map_err(coercion)?)
                },
                move || async move { balance.fetch_balance().await.map(|(b, _)| b) },
            )
            .await?;
            Ok(ValidateAccountResult {
                error: from_value::<AccountValidationError>(adapter_json(&outcome.error)?)?,
                balance: outcome.balance,
            })
        })
    }
    pub(crate) fn fetch_ohlcv(
        self: &Rc<Self>,
        o: OHLCVOptions,
    ) -> ExchangeFuture<'static, Vec<Candle>> {
        let xhr = self.xhr();
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let interval = to_value(&o.interval)?
                .as_str()
                .ok_or_else(|| ExchangeError::new("Timeframe must serialize as a string"))?
                .to_owned();
            crate::history_rest::fetch_ohlcv(
                crate::history_rest::OhlcvReadOptions {
                    symbol: o.symbol,
                    interval,
                    limit: o.limit,
                    from: o.from,
                    to: o.to,
                },
                |at| Ok(at.unwrap_or_else(|| runtime.now_ms())),
                dayjs_subtract,
                |at| Ok(*at),
                |endpoint, config| {
                    let f = xhr.get(endpoint, config);
                    async move {
                        let body = f.await.map_err(|e| e.message)?;
                        Ok(body.get("data").cloned().unwrap_or(Value::Null))
                    }
                },
            )
            .await
            .map_err(ExchangeError::new)
        })
    }
    pub(crate) fn update_symbol_metadata(
        self: &Rc<Self>,
        symbol: String,
    ) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move {
            let xhr = me.xhr();
            crate::history_rest::update_symbol_metadata(
                &symbol,
                |endpoint, config| {
                    let f = xhr.get(endpoint, config);
                    async move { Ok(f.await?.get("data").cloned().unwrap_or(Value::Null)) }
                },
                |s, margin, leverage| {
                    let leverage = leverage.map_or(JsJsonValue::Undefined, JsJsonValue::from);
                    me.state.borrow_mut().symbol_settings.insert(
                        s.into(),
                        JsJsonValue::Object(vec![
                            ("marginType".into(), JsJsonValue::from(to_value(&margin)?)),
                            ("leverage".into(), leverage),
                        ]),
                    );
                    Ok(())
                },
                || Ok(me.base.store.positions().to_vec()),
                |p: &Position, s| Ok(p.symbol == s),
                |p, leverage, margin| {
                    let mut value = crate::adapter::to_input(&p)?;
                    *value.get_mut("leverage").expect("native Position leverage") =
                        leverage.unwrap_or(Value::Null).into();
                    let fields = value.as_object_mut().expect("native Position object");
                    let key: Vec<u16> = "marginType".encode_utf16().collect();
                    fields.retain(|(name, _)| name != &key);
                    fields.push((key, crate::adapter::to_input(&margin)?));
                    crate::adapter::from_input(value)
                },
                |positions| {
                    me.base
                        .store
                        .update(
                            StoreUpdate {
                                positions: Some(positions),
                                ..Default::default()
                            },
                            false,
                        )
                        .map_err(store_err)
                },
            )
            .await
        })
    }
    pub(crate) fn fetch_historical_fills(
        self: &Rc<Self>,
        options: safe_cex_types::ExecutionsOptions,
    ) -> ExchangeFuture<'static, Vec<Execution>> {
        let me = self.clone();
        Box::pin(async move {
            let options = crate::adapter::to_input(&options)?;
            let rows = crate::event_models::fetch_historical_fills(&mut me.fill_frame(), &options)
                .await
                .map_err(|e| e.0)?;
            rows.into_iter().map(crate::adapter::from_input).collect()
        })
    }
    fn fill_frame(self: &Rc<Self>) -> FillFrame {
        FillFrame {
            venue: self.clone(),
        }
    }
    pub(crate) fn start(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move { crate::session_control::start(&mut me.session()).await })
    }
    pub(crate) fn tick(self: &Rc<Self>, count: f64) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move { crate::session_control::tick(&mut me.session(), count).await })
    }
    fn session(self: &Rc<Self>) -> Session {
        Session {
            venue: self.clone(),
        }
    }
}

/// dayjs(end).subtract(amount, unit) over epoch milliseconds.
fn dayjs_subtract(end: &f64, amount: f64, unit: Option<&str>) -> Result<f64, String> {
    let ms = match unit {
        Some("ms") => 1.0,
        Some("s") => 1000.0,
        Some("m") => 60_000.0,
        Some("h") => 3_600_000.0,
        Some("d") => 86_400_000.0,
        Some("w") => 604_800_000.0,
        other => return Err(format!("Unsupported Bitunix OHLCV interval unit {other:?}")),
    };
    Ok(end - amount * ms)
}

struct FillFrame {
    venue: Rc<BitunixVenue>,
}
impl crate::event_models::FillHistoryHost for FillFrame {
    type Error = Fault;
    fn public_only(&mut self) -> Result<bool, Fault> {
        Ok(self.venue.public_only())
    }
    fn market(&mut self, symbol: &Input) -> Result<Option<Input>, Fault> {
        let symbol = safe_cex_num::js_retained_string(Some(symbol))?;
        let market = self
            .venue
            .base
            .store
            .markets_by_symbol()
            .get(&symbol)
            .cloned();
        Ok(market.map(|m| crate::adapter::to_input(&m)).transpose()?)
    }
    fn get(
        &mut self,
        endpoint: &'static str,
        config: Input,
    ) -> impl std::future::Future<Output = Result<Input, Fault>> {
        let client = self.venue.xhr();
        async move {
            Ok(client
                .get(endpoint, crate::adapter::request_payload(config)?)
                .await?
                .into())
        }
    }
    fn local_to_utc(&mut self, value: f64) -> f64 {
        value
    }
    fn warning(&mut self, prefix: &'static str, error: &Fault) -> Result<(), Fault> {
        Ok(self
            .venue
            .log(&format!("{prefix}{}", reason(&error.0)), "warning")?)
    }
}

#[derive(Clone)]
pub(crate) enum Data {
    Markets(Vec<Market>),
    Tickers(Vec<Ticker>),
    Positions(Vec<Position>),
    Orders(Vec<Order>),
    Balance(Balance),
    Flag(bool),
    Loaded(StoreDataLoaded),
}
struct Session {
    venue: Rc<BitunixVenue>,
}
impl crate::session_control::ExchangeSessionHost for Session {
    type Value = Data;
    type Socket = Rc<Socket>;
    type Error = ExchangeError;
    fn external_public(
        &mut self,
    ) -> impl std::future::Future<Output = Result<bool, ExchangeError>> {
        let venue = self.venue.clone();
        async move {
            let factory = venue.external.clone();
            let owner = venue.base.clone();
            venue
                .base
                .init_external_public_data(|c| factory(c, owner))
                .await
        }
    }
    fn fetch(
        &mut self,
        member: &'static str,
    ) -> impl std::future::Future<Output = Result<Data, ExchangeError>> {
        let venue = self.venue.clone();
        async move {
            Ok(match member {
                "fetchMarkets" => Data::Markets(venue.fetch_markets().await?),
                "fetchTickers" => Data::Tickers(venue.fetch_tickers().await?),
                "fetchPositionMode" => Data::Flag(venue.fetch_position_mode().await?),
                "fetchPositions" => Data::Positions(venue.fetch_positions().await?),
                "fetchOrders" => Data::Orders(venue.fetch_orders().await?),
                other => return Err(ExchangeError::new(format!("Unknown Bitunix read {other}"))),
            })
        }
    }
    fn fetch_balance(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(Data, Data), ExchangeError>> {
        let f = self.venue.fetch_balance();
        async move {
            let (balance, positions) = f.await?;
            Ok((Data::Balance(balance), Data::Positions(positions)))
        }
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.venue.base.is_disposed())
    }
    fn public_only(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.venue.public_only())
    }
    fn loaded(&mut self) -> Result<Data, ExchangeError> {
        Ok(Data::Loaded(self.venue.base.store.loaded().clone()))
    }
    fn update(
        &mut self,
        fields: Vec<(&'static str, &Data)>,
        loaded: Option<(Data, Vec<&'static str>)>,
    ) -> Result<(), ExchangeError> {
        let mut update = StoreUpdate::default();
        for (key, value) in fields {
            match (key, value.clone()) {
                ("markets", Data::Markets(v)) => update.markets = Some(v),
                ("tickers", Data::Tickers(v)) => update.tickers = Some(v),
                ("positions", Data::Positions(v)) => update.positions = Some(v),
                ("orders", Data::Orders(v)) => update.orders = Some(v),
                ("balance", Data::Balance(v)) => update.balance = Some(v),
                _ => {
                    return Err(ExchangeError::new(format!(
                        "Bitunix store field {key} kind mismatch"
                    )));
                }
            }
        }
        if let Some((Data::Loaded(mut current), keys)) = loaded {
            for key in keys {
                match key {
                    "markets" => current.markets = true,
                    "tickers" => current.tickers = true,
                    "positions" => current.positions = true,
                    "orders" => current.orders = true,
                    "balance" => current.balance = true,
                    other => {
                        return Err(ExchangeError::new(format!("Unknown loaded flag {other}")));
                    }
                }
            }
            update.loaded = Some(current);
        }
        self.venue
            .base
            .store
            .update(update, false)
            .map_err(store_err)
    }
    fn connect(&mut self, socket: &'static str) -> Result<(), ExchangeError> {
        match socket {
            "futuresWebsocket" => self.venue.socket("futures").connect_and_subscribe(),
            "privateWebsocket" => self.venue.socket("private").connect_and_subscribe(),
            other => Err(ExchangeError::new(format!(
                "Unknown Bitunix socket {other}"
            ))),
        }
    }
    fn configure_shards(&mut self, markets: &Data) -> Result<Vec<Rc<Socket>>, ExchangeError> {
        let Data::Markets(markets) = markets else {
            return Err(ExchangeError::new("Bitunix shards need markets"));
        };
        self.venue
            .configure_public_websocket_shards(markets.clone())
    }
    fn connect_shard(&mut self, socket: Rc<Socket>) -> Result<(), ExchangeError> {
        socket.connect_and_subscribe()
    }
    fn length(&mut self, rows: &Data) -> Result<usize, ExchangeError> {
        match rows {
            Data::Markets(v) => Ok(v.len()),
            Data::Tickers(v) => Ok(v.len()),
            Data::Positions(v) => Ok(v.len()),
            Data::Orders(v) => Ok(v.len()),
            _ => Err(ExchangeError::new("Value has no array length")),
        }
    }
    fn log(&mut self, text: Vec<u16>) -> Result<(), ExchangeError> {
        self.venue.log(&utf8(&text)?, "info")
    }
    fn set_hedged(&mut self, mode: Data) -> Result<(), ExchangeError> {
        let Data::Flag(hedged) = mode else {
            return Err(ExchangeError::new("isHedged must be a boolean"));
        };
        self.venue
            .base
            .store
            .set_setting("isHedged", hedged)
            .map_err(store_err)
    }
    fn initial_tick(&mut self) -> impl std::future::Future<Output = Result<(), ExchangeError>> {
        self.venue.tick(0.0)
    }
    fn position_modes_equal(&mut self) -> Result<bool, ExchangeError> {
        let state = self.venue.state.borrow();
        Ok(state.last_tick_position_mode == state.account_position_mode)
    }
    fn copy_position_mode(&mut self) -> Result<(), ExchangeError> {
        let mut state = self.venue.state.borrow_mut();
        state.last_tick_position_mode = state.account_position_mode;
        Ok(())
    }
    fn report_message(&mut self, error: ExchangeError) -> Result<(), ExchangeError> {
        self.venue.emit_error(&error.message)
    }
    fn tick_interval(&mut self) -> Result<f64, ExchangeError> {
        Ok(self.venue.base.get_tick_interval().unwrap_or(f64::NAN))
    }
    fn schedule_tick(&mut self, next_count: f64, interval: f64) -> Result<(), ExchangeError> {
        // loop(fn, minInterval): a missing interval falls back to 5000ms with a warning.
        let interval = if interval > 0.0 {
            interval
        } else {
            self.venue.runtime.console(
                "warn",
                "[safe-cex] loop() called without a minInterval — defaulting to 5000ms.",
            );
            5000.0
        };
        let sleep = self.venue.runtime.sleep(interval as u64);
        let venue = self.venue.clone();
        self.venue.spawn(
            "Bitunix tick",
            Box::pin(async move {
                sleep.await?;
                venue.tick(next_count).await
            }),
        );
        Ok(())
    }
}

impl BitunixVenue {
    pub(crate) fn configure_public_websocket_shards(
        self: &Rc<Self>,
        markets: Vec<Market>,
    ) -> Result<Vec<Rc<Socket>>, ExchangeError> {
        let old = self.state.borrow().public_shards.clone();
        let fresh: std::cell::RefCell<Vec<Rc<Socket>>> = std::cell::RefCell::new(vec![]);
        let result = crate::exchange_lifecycle::configure_public_websocket_shards(
            || Ok((old.len(), |i: usize| Ok(old.get(i).cloned()))),
            |shard: Rc<Socket>| shard.dispose(),
            || {
                self.state.borrow_mut().public_shards.clear();
                Ok(())
            },
            (|| Ok(markets.clone()), |m: Market| Ok(m.symbol)),
            (
                || Ok(self.socket("public")),
                || Ok(self.socket("secondary")),
            ),
            |socket, symbols| socket.set_market_symbols(Some(symbols.borrow().clone())),
            |secondary, symbols| {
                Socket::new(
                    Rc::new(std::cell::RefCell::new(Rc::downgrade(self))),
                    &self.base,
                    self.runtime.clone(),
                    "public",
                    Some(secondary),
                    Some(symbols.borrow().clone()),
                )
            },
            |a, b| {
                fresh.borrow_mut().extend([a, b]);
                Ok(())
            },
            || Ok(fresh.borrow().clone()),
        )?;
        self.state.borrow_mut().public_shards = fresh.into_inner();
        Ok(result)
    }
    pub(crate) fn position_update(&self, positions: Vec<Position>) -> Result<(), ExchangeError> {
        self.base
            .store
            .update(
                StoreUpdate {
                    positions: Some(positions),
                    ..Default::default()
                },
                false,
            )
            .map_err(store_err)
    }
    pub(crate) fn order_update(&self, orders: Vec<Order>) -> Result<(), ExchangeError> {
        self.base
            .store
            .update(
                StoreUpdate {
                    orders: Some(orders),
                    ..Default::default()
                },
                false,
            )
            .map_err(store_err)
    }
}
