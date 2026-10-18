use super::{
    error::{failure, from_thrown},
    rt::Host,
};
use crate::{
    market_position_reads::*, portfolios_balances::*, public_data::PublicData,
    public_market_reads::*, starbase_auth::CoinbaseStarbasePromise as Promise,
};
use safe_cex_exchange::{
    js_runtime::value::{Js, from_json},
    runtime::ExchangeError,
};
use serde_json::{Value, json};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::{cell::RefCell, rc::Rc};

pub(super) fn items(data: &Input, field: &str) -> Result<ReadItems, PortfolioFailure> {
    let value = if field.is_empty() {
        data
    } else {
        data.get(field).ok_or_else(|| {
            Host::json_error(
                "TypeError",
                &format!("Missing Coinbase response array {field}"),
            )
        })?
    };
    let values = value
        .as_array()
        .ok_or_else(|| Host::json_error("TypeError", "Coinbase response items are not an array"))?;
    Ok(Rc::new(RefCell::new(
        values
            .iter()
            .map(|value| Rc::new(RefCell::new(value.clone())))
            .collect(),
    )))
}
fn response_field(endpoint: &str) -> &'static str {
    if endpoint.ends_with("/instruments/") {
        ""
    } else if endpoint.ends_with("/candles") {
        "candles"
    } else if endpoint.contains("/positions") {
        "positions"
    } else {
        "products"
    }
}
impl Host {
    pub(super) fn read_get(
        &self,
        endpoint: &'static str,
        params: Option<Value>,
    ) -> Promise<Result<ReadReply, PortfolioFailure>> {
        let result = self.request("get", endpoint, params);
        self.promise(async move {
            let data = result.await?;
            Ok(ReadReply {
                items: items(&data, response_field(endpoint))?,
            })
        })
    }
    pub(super) fn read_markets(&self) -> ReadMarkets {
        Rc::new(RefCell::new(
            self.shared
                .store
                .markets()
                .iter()
                .cloned()
                .map(|market| Rc::new(RefCell::new(market)))
                .collect(),
        ))
    }
    pub(super) fn read_market(&self, id: &str) -> Option<ReadMarket> {
        self.shared
            .store
            .markets_by_id()
            .get(id)
            .cloned()
            .map(|market| Rc::new(RefCell::new(market)))
    }
    pub(super) fn public_read_receiver(&self) -> Rc<PublicMarketReadReceiver<Self>> {
        Rc::new(PublicMarketReadReceiver {
            host: self.clone(),
            spawn: self.shared.extra.spawn.clone(),
        })
    }
    pub(super) fn emit_args(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        self.receiver()
            .call_method(
                "emitAndLogError",
                args.iter()
                    .map(|value| value.as_ref().map(from_json).unwrap_or(Js::Undefined))
                    .collect(),
            )
            .map_err(from_thrown)?;
        Ok(())
    }
    pub(super) fn cached_balance(&self) -> Balance {
        cached_portfolio_balance(&self.shared.store.balance())
    }

    pub(super) fn dayjs(&self, value: Option<Value>) -> Result<Js, PortfolioFailure> {
        self.global("dayjs", value.iter().map(from_json).collect())
    }
    pub(super) fn daily(&self, symbol: &str) -> Option<PublicData> {
        self.shared
            .extra
            .daily_candles
            .borrow()
            .get(symbol)
            .cloned()
            .or_else(|| {
                // Source ordinary object lookup observes inherited functions as truthy values.
                Js::object(vec![])
                    .get(symbol)
                    .ok()
                    .filter(|value| !value.is_undefined())
                    .map(|_| PublicData::Generated(Rc::default()))
            })
    }
}
impl PortfolioHost for Host {
    fn get(
        &self,
        endpoint: &'static str,
        portfolio_uuid: Option<String>,
    ) -> Promise<Result<PortfolioReply, PortfolioFailure>> {
        let result = self.request(
            "get",
            endpoint,
            portfolio_uuid.map(|id| json!({"portfolio_uuid":id})),
        );
        self.promise(async move {
            let data = result.await?;
            let portfolios = if endpoint.ends_with("/portfolios") {
                let array = items(&data, "portfolios")?;
                let mut portfolios = Vec::new();
                for item in array.borrow().iter() {
                    portfolios.push(Rc::new(RefCell::new(
                        serde_json::from_value(item.borrow().clone())
                            .map_err(|error| failure(ExchangeError::new(error.to_string())))?,
                    )));
                }
                Some(Rc::new(RefCell::new(portfolios)))
            } else {
                None
            };
            Ok(PortfolioReply { data, portfolios })
        })
    }
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        self.emit_args(args)
    }
    fn cached_balance(&self) -> Balance {
        self.cached_balance()
    }
    fn position_upnls(&self) -> Vec<f64> {
        self.shared
            .store
            .positions()
            .iter()
            .map(|position| position.unrealized_pnl)
            .collect()
    }
}
impl MarketPositionHost for Host {
    fn get(
        &self,
        endpoint: &'static str,
        params: Option<Value>,
    ) -> Promise<Result<ReadReply, PortfolioFailure>> {
        self.read_get(endpoint, params)
    }
    fn error_response_items(&self, err: &PortfolioFailure) -> Option<ReadItems> {
        let data = err.value.as_ref()?.get("response")?.get("data")?;
        items(
            data,
            if data.is_array() {
                ""
            } else if data.get("products").is_some() {
                "products"
            } else {
                "positions"
            },
        )
        .ok()
    }
    fn get_supported_types(&self) -> Promise<Result<Vec<String>, PortfolioFailure>> {
        let result = self.class().map(|class| class.supported_types());
        self.promise(async move { Ok(result?.await?.borrow().clone()) })
    }
    fn markets(&self) -> ReadMarkets {
        self.read_markets()
    }
    fn market_by_id(&self, id: &str) -> Option<ReadMarket> {
        self.read_market(id)
    }
    fn cached_tickers(&self) -> ReadTickers {
        Rc::new(RefCell::new(
            self.shared
                .store
                .tickers()
                .iter()
                .cloned()
                .map(|mut fields| {
                    let info = fields.info.take().map(|value| Rc::new(RefCell::new(value)));
                    Rc::new(ReadTicker { fields, info })
                })
                .collect(),
        ))
    }
    fn cached_positions(&self) -> ReadPositions {
        Rc::new(RefCell::new(
            self.shared
                .store
                .positions()
                .iter()
                .cloned()
                .map(|mut fields| {
                    let info = fields.info.take().map(|value| Rc::new(RefCell::new(value)));
                    Rc::new(ReadPosition { fields, info })
                })
                .collect(),
        ))
    }
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        self.emit_args(args)
    }
    fn local_wall_time_to_utc(&self, millis: f64) -> f64 {
        (self.shared.extra.local_wall_time_to_utc)(millis)
    }
}
impl PublicMarketReadHost for Host {
    fn public_only(&self) -> bool {
        self.public_only()
    }
    fn get_supported_types(&self) -> PublicReadAsync<CatalogSupportedTypes> {
        Ok(self.class()?.supported_types())
    }
    fn get(
        &self,
        endpoint: &'static str,
        params: Option<PublicMarketQuery>,
    ) -> PublicReadAsync<ReadReply> {
        let params = params.map(|query| match query {
            PublicMarketQuery::Catalog(value) => value,
            PublicMarketQuery::Candles(query) => json!({"product_id":query.product_id,"granularity":query.granularity,"start":query.start,"end":query.end}),
        });
        Ok(self.read_get(endpoint, params))
    }
    fn error_response_items(&self, err: &PortfolioFailure) -> Option<ReadItems> {
        MarketPositionHost::error_response_items(self, err)
    }
    fn cached_markets(&self) -> CatalogMarkets {
        Rc::new(RefCell::new(
            self.shared
                .store
                .markets()
                .iter()
                .cloned()
                .map(|mut fields| {
                    let info = Rc::new(RefCell::new(fields.info.take().unwrap_or(Value::Null)));
                    Rc::new(RefCell::new(CatalogMarket { fields, info }))
                })
                .collect(),
        ))
    }
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure> {
        self.emit(message)
    }
    fn dayjs_value_of(&self, value: Option<Value>) -> Result<f64, PortfolioFailure> {
        self.dayjs(value)?
            .call_method("valueOf", vec![])
            .and_then(|v| v.to_number())
            .map_err(from_thrown)
    }
    fn dayjs_subtract(
        &self,
        end_ms: f64,
        amount: f64,
        unit: &str,
    ) -> Result<f64, PortfolioFailure> {
        self.global("dayjs", vec![Js::Number(end_ms)])?
            .call_method("subtract", vec![Js::Number(amount), Js::str(unit)])
            .and_then(|v| v.call_method("valueOf", vec![]))
            .and_then(|v| v.to_number())
            .map_err(from_thrown)
    }
    fn daily_candle(&self, symbol: &str) -> Option<PublicData> {
        self.daily(symbol)
    }
    fn set_daily_candle(&self, symbol: String, candle: PublicData) -> Result<(), PortfolioFailure> {
        self.shared
            .extra
            .daily_candles
            .borrow_mut()
            .insert(symbol, candle);
        Ok(())
    }
}

fn cached_portfolio_balance(native: &safe_cex_types::Balance) -> Balance {
    let currencies = native.currencies.as_ref().map(|values| {
        Rc::new(RefCell::new(PortfolioCurrencies {
            own: values
                .iter()
                .map(|(key, value)| (key.clone(), cached_currency_balance(value)))
                .collect(),
            prototype: None,
        }))
    });
    Rc::new(RefCell::new(PortfolioBalance {
        total: native.total,
        free: native.free,
        used: native.used,
        upnl: native.upnl,
        has_currencies: currencies.is_some(),
        currencies,
        native_snapshot: Some(Rc::new(native.clone().into())),
        ..Default::default()
    }))
}

fn cached_currency_balance(native: &safe_cex_types::CurrencyBalance) -> Balance {
    match native {
        safe_cex_types::CurrencyBalance::Declared(value) => cached_portfolio_balance(value),
        safe_cex_types::CurrencyBalance::AvailableLocked { .. } => {
            Rc::new(RefCell::new(PortfolioBalance {
                native_snapshot: Some(Rc::new(native.clone())),
                ..Default::default()
            }))
        }
    }
}
fn currency_fields(balance: &Balance) -> safe_cex_types::CurrencyBalance {
    if let Some(native) = &balance.borrow().native_snapshot
        && matches!(
            native.as_ref(),
            safe_cex_types::CurrencyBalance::AvailableLocked { .. }
        )
    {
        return native.as_ref().clone();
    }
    balance_fields(balance).into()
}

pub(super) fn balance_fields(balance: &Balance) -> safe_cex_types::Balance {
    let value = balance.borrow();
    safe_cex_types::Balance {
        total: value.total,
        free: value.free,
        used: value.used,
        upnl: value.upnl,
        info: value
            .extra
            .get("info")
            .map(|value| value.borrow().clone())
            .or_else(|| {
                value
                    .native_snapshot
                    .as_ref()
                    .and_then(|native| native.as_balance())
                    .and_then(|native| native.info.clone())
            }),
        coins: if let Some(coins) = value.extra.get("coins") {
            serde_json::from_value(coins.borrow().clone()).ok()
        } else {
            value
                .native_snapshot
                .as_ref()
                .and_then(|native| native.as_balance())
                .and_then(|native| native.coins.clone())
        },
        currencies: value.currencies.as_ref().map(|currencies| {
            currencies
                .borrow()
                .own
                .iter()
                .map(|(key, value)| (key.clone(), currency_fields(value)))
                .collect()
        }),
    }
}
