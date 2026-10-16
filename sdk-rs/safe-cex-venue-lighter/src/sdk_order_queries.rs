//! Order/trade values and seven ordinary query bodies. Raw order-book-orders
//! response/config spread handling and complete OrderApi coverage remain pending.
use crate::{
    http_models::Utf16Record,
    sdk_account_api::*,
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::SigningResult,
};
use num_bigint::BigInt;
use safe_cex_num::js_number_string;
use std::rc::Rc;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSide {
    Buy,
    Sell,
}
impl TradeSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}
// ts: ApiTrade
#[derive(Debug, Clone, PartialEq)]
pub struct ApiTrade {
    pub trade_id: Vec<u16>,
    pub market_id: f64,
    pub symbol: Vec<u16>,
    pub price: Vec<u16>,
    pub quantity: Vec<u16>,
    pub quote_quantity: Vec<u16>,
    pub side: TradeSide,
    pub is_buyer_maker: bool,
    pub timestamp: f64,
    pub maker_account_index: Option<f64>,
    pub taker_account_index: Option<f64>,
}
// ts: TradesResponse
#[derive(Debug, Clone, PartialEq)]
pub struct TradesResponse {
    pub trades: Vec<ApiTrade>,
    pub total: Option<f64>,
    pub next_cursor: Option<Vec<u16>>,
}
// ts: RecentTradesResponse
#[derive(Debug, Clone, PartialEq)]
pub struct RecentTradesResponse {
    pub market_id: f64,
    pub trades: Vec<ApiTrade>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MarketStats {
    pub market_id: f64,
    pub symbol: Vec<u16>,
    pub volume24h: Vec<u16>,
    pub trades24h: f64,
    pub price_change24h: Vec<u16>,
    pub price_change_percent24h: Vec<u16>,
    pub high24h: Vec<u16>,
    pub low24h: Vec<u16>,
    pub last_price: Vec<u16>,
}
// ts: ExchangeStats
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeStats {
    pub volume24h: Vec<u16>,
    pub trades24h: f64,
    pub total_markets: f64,
    pub active_orders: f64,
    pub total_accounts: f64,
    pub tvl: Vec<u16>,
    pub market_stats: Vec<MarketStats>,
    pub timestamp: f64,
}
// ts: ExchangeStatsResponse
#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeStatsResponse {
    pub stats: ExchangeStats,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InactiveSortBy {
    CreatedAt,
    UpdatedAt,
    Price,
}
impl InactiveSortBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreatedAt => "createdAt",
            Self::UpdatedAt => "updatedAt",
            Self::Price => "price",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradesSortBy {
    Timestamp,
    Price,
}
impl TradesSortBy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timestamp => "timestamp",
            Self::Price => "price",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}
impl SortOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}
// ts: GetInactiveOrdersOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetInactiveOrdersOptions {
    pub market_id: Option<f64>,
    pub limit: Option<f64>,
    pub ask_filter: Option<bool>,
    pub between_timestamps: Option<[f64; 2]>,
    pub cursor: Option<Vec<u16>>,
    pub sort_by: Option<InactiveSortBy>,
    pub sort_order: Option<SortOrder>,
}
// ts: GetTradesOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetTradesOptions {
    pub market_id: Option<f64>,
    pub account_index: Option<BigInt>,
    pub limit: Option<f64>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub cursor: Option<Vec<u16>>,
    pub sort_by: Option<TradesSortBy>,
    pub sort_order: Option<SortOrder>,
}
/// Get toString and call with the original value as receiver, preserving the
/// exact native return (even a non-string override) and original exceptions.
pub trait OrderQuerySemantics {
    fn to_string_method(&self, value: &AccountValue) -> SigningResult<AccountValue>;
}
/// Native class; getOrderBookOrders lives in safe-cex-lighter-sdk/src/api_reads.rs.
pub struct OrderApi {
    host: AccountApi,
    semantics: Rc<dyn AccountSemantics>,
    query: Rc<dyn OrderQuerySemantics>,
    runtime: Rc<dyn TransactionRuntime>,
}
impl OrderApi {
    // ts: OrderApi.constructor
    pub fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
        query: Rc<dyn OrderQuerySemantics>,
    ) -> Self {
        Self {
            host: AccountApi::new(client, runtime.clone(), semantics.clone()),
            semantics,
            query,
            runtime,
        }
    }
    fn option(&self, options: Option<&AccountValue>, key: &str) -> SigningResult<AccountValue> {
        match options {
            Some(value) if !value.is_nullish() => self.semantics.get_property(value, key),
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn default_limit(&self, limit: AccountValue) -> SigningResult<AccountValue> {
        self.query.to_string_method(&if limit.truthy() {
            limit
        } else {
            AccountValue::Number(50.0)
        })
    }
    fn request(
        &self,
        url: &'static str,
        params: SigningResult<Utf16Record<AccountValue>>,
        fallback: &'static str,
    ) -> NativePromise<AccountValue> {
        match params {
            Ok(params) => self.host.request_projected(
                || {
                    Ok((
                        url.encode_utf16().collect(),
                        Some(AccountRequestConfig { params }),
                    ))
                },
                fallback,
                |_, data| Ok(data),
            ),
            Err(error) => NativePromise::settled(self.runtime.clone(), Err(error)),
        }
    }
    fn defined_string(
        &self,
        params: &mut Utf16Record<AccountValue>,
        options: Option<&AccountValue>,
        key: &str,
        out: &str,
    ) -> SigningResult<()> {
        if !matches!(self.option(options, key)?, AccountValue::Undefined) {
            params.insert(
                out.encode_utf16().collect(),
                self.query.to_string_method(&self.option(options, key)?)?,
            );
        }
        Ok(())
    }
    fn truthy_raw(
        &self,
        params: &mut Utf16Record<AccountValue>,
        options: Option<&AccountValue>,
        key: &str,
        out: &str,
    ) -> SigningResult<()> {
        if self.option(options, key)?.truthy() {
            params.insert(out.encode_utf16().collect(), self.option(options, key)?);
        }
        Ok(())
    }
    fn optional_market(&self, market_id: Option<f64>) -> SigningResult<Utf16Record<AccountValue>> {
        let mut params = Utf16Record::new();
        if let Some(value) = market_id {
            params.insert(
                "market_id".encode_utf16().collect(),
                self.query.to_string_method(&AccountValue::Number(value))?,
            );
        }
        Ok(params)
    }
    // ts: OrderApi.getAccountActiveOrders
    pub fn get_account_active_orders(
        &self,
        index: &BigInt,
        market_id: Option<f64>,
    ) -> NativePromise<AccountValue> {
        let params = (|| {
            let mut params = Utf16Record::new();
            params.insert(
                "account_index".encode_utf16().collect(),
                self.query
                    .to_string_method(&AccountValue::BigInt(index.clone()))?,
            );
            if let Some(value) = market_id {
                params.insert(
                    "market_id".encode_utf16().collect(),
                    self.query.to_string_method(&AccountValue::Number(value))?,
                );
            }
            Ok(params)
        })();
        self.request(
            "/api/v1/accountActiveOrders",
            params,
            "Failed to get active orders",
        )
    }
    // ts: OrderApi.getAccountInactiveOrders
    pub fn get_account_inactive_orders(
        &self,
        index: &BigInt,
        options: Option<&AccountValue>,
    ) -> NativePromise<AccountValue> {
        let params = (|| {
            let mut params = Utf16Record::new();
            params.insert(
                "account_index".encode_utf16().collect(),
                self.query
                    .to_string_method(&AccountValue::BigInt(index.clone()))?,
            );
            params.insert(
                "limit".encode_utf16().collect(),
                self.default_limit(self.option(options, "limit")?)?,
            );
            self.defined_string(&mut params, options, "marketId", "market_id")?;
            self.defined_string(&mut params, options, "askFilter", "ask_filter")?;
            if self.option(options, "betweenTimestamps")?.truthy() {
                let first = self.option(options, "betweenTimestamps")?;
                let first = self.semantics.get_property(&first, "0")?;
                params.insert(
                    "start_time".encode_utf16().collect(),
                    self.query.to_string_method(&first)?,
                );
                let last = self.option(options, "betweenTimestamps")?;
                let last = self.semantics.get_property(&last, "1")?;
                params.insert(
                    "end_time".encode_utf16().collect(),
                    self.query.to_string_method(&last)?,
                );
            }
            self.truthy_raw(&mut params, options, "cursor", "cursor")?;
            self.truthy_raw(&mut params, options, "sortBy", "sort_by")?;
            self.truthy_raw(&mut params, options, "sortOrder", "sort_order")?;
            Ok(params)
        })();
        self.request(
            "/api/v1/accountInactiveOrders",
            params,
            "Failed to get inactive orders",
        )
    }
    // ts: OrderApi.getOrderBooks
    pub fn get_order_books(&self, market_id: Option<f64>) -> NativePromise<AccountValue> {
        self.request(
            "/api/v1/order-books",
            self.optional_market(market_id),
            "Failed to get order books",
        )
    }
    // ts: OrderApi.getOrderBookDetails
    pub fn get_order_book_details(&self, market_id: Option<f64>) -> NativePromise<AccountValue> {
        self.request(
            "/api/v1/order-books/details",
            self.optional_market(market_id),
            "Failed to get order book details",
        )
    }
    // ts: OrderApi.getRecentTrades
    pub fn get_recent_trades(
        &self,
        market_id: f64,
        limit: Option<f64>,
    ) -> NativePromise<AccountValue> {
        match self.default_limit(
            limit
                .map(AccountValue::Number)
                .unwrap_or(AccountValue::Undefined),
        ) {
            Ok(limit) => self.host.request_projected(
                || {
                    Ok((
                        format!("/api/v1/trades/recent/{}", js_number_string(market_id))
                            .encode_utf16()
                            .collect(),
                        Some(AccountRequestConfig {
                            params: [("limit".encode_utf16().collect(), limit)].into(),
                        }),
                    ))
                },
                "Failed to get recent trades",
                |_, data| Ok(data),
            ),
            Err(error) => NativePromise::settled(self.runtime.clone(), Err(error)),
        }
    }
    // ts: OrderApi.getTrades
    pub fn get_trades(&self, options: Option<&AccountValue>) -> NativePromise<AccountValue> {
        let params = (|| {
            let mut params = Utf16Record::new();
            params.insert(
                "limit".encode_utf16().collect(),
                self.default_limit(self.option(options, "limit")?)?,
            );
            for (key, out) in [
                ("marketId", "market_id"),
                ("accountIndex", "account_index"),
                ("startTime", "start_time"),
                ("endTime", "end_time"),
            ] {
                self.defined_string(&mut params, options, key, out)?;
            }
            for (key, out) in [
                ("cursor", "cursor"),
                ("sortBy", "sort_by"),
                ("sortOrder", "sort_order"),
            ] {
                self.truthy_raw(&mut params, options, key, out)?;
            }
            Ok(params)
        })();
        self.request("/api/v1/trades", params, "Failed to get trades")
    }
    // ts: OrderApi.getExchangeStats
    pub fn get_exchange_stats(&self) -> NativePromise<AccountValue> {
        self.host.request_projected(
            || Ok(("/api/v1/stats".encode_utf16().collect(), None)),
            "Failed to get exchange stats",
            |_, data| Ok(data),
        )
    }
}
