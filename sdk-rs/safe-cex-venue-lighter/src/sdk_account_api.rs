//! Account API values and eager asynchronous orchestration at native host boundaries.
use crate::{
    http_models::Utf16Record,
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{HostThrownValue, SigningResult},
};
use num_bigint::BigInt;
use std::rc::Rc;
// ts: AccountInfo
#[derive(Debug, Clone, PartialEq)]
pub struct AccountInfo {
    pub account_index: Vec<u16>,
    pub l1_address: Vec<u16>,
    pub tier: Option<Vec<u16>>,
    pub status: Vec<u16>,
    pub created_at: f64,
    pub last_activity_at: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AccountRateLimits {
    pub requests_per_second: f64,
    pub orders_per_second: f64,
    pub orders_per_day: f64,
}
// ts: AccountLimits
#[derive(Debug, Clone, PartialEq)]
pub struct AccountLimits {
    pub account_index: Vec<u16>,
    pub max_open_orders: f64,
    pub max_order_size: Vec<u16>,
    pub max_position_size: Vec<u16>,
    pub daily_withdrawal_limit: Vec<u16>,
    pub rate_limits: AccountRateLimits,
}
// ts: AccountMetadata
#[derive(Debug, Clone, PartialEq)]
pub struct AccountMetadata<V> {
    pub account_index: Vec<u16>,
    pub alias: Option<Vec<u16>>,
    pub labels: Option<Vec<Vec<u16>>>,
    pub metadata: Option<Utf16Record<V>>,
    pub updated_at: f64,
}
// ts: SubAccount
#[derive(Debug, Clone, PartialEq)]
pub struct SubAccount {
    pub account_index: Vec<u16>,
    pub alias: Option<Vec<u16>>,
    pub status: Vec<u16>,
    pub created_at: f64,
}
// ts: AccountsByL1Address
#[derive(Debug, Clone, PartialEq)]
pub struct AccountsByL1Address {
    pub l1_address: Vec<u16>,
    pub accounts: Vec<SubAccount>,
    pub total: f64,
}
// ts: ApiKeyInfo
#[derive(Debug, Clone, PartialEq)]
pub struct ApiKeyInfo {
    pub index: f64,
    pub public_key: Vec<u16>,
    pub permissions: f64,
    pub label: Option<Vec<u16>>,
    pub is_active: bool,
    pub created_at: f64,
    pub last_used_at: Option<f64>,
}
// ts: AccountApiKeys
#[derive(Debug, Clone, PartialEq)]
pub struct AccountApiKeys {
    pub account_index: Vec<u16>,
    pub api_keys: Vec<ApiKeyInfo>,
}
// ts: Liquidation
#[derive(Debug, Clone, PartialEq)]
pub struct Liquidation {
    pub liquidation_id: Vec<u16>,
    pub account_index: Vec<u16>,
    pub market_id: Option<f64>,
    pub amount: Vec<u16>,
    pub price: Vec<u16>,
    pub timestamp: f64,
    pub tx_hash: Option<Vec<u16>>,
}
// ts: Liquidations
#[derive(Debug, Clone, PartialEq)]
pub struct Liquidations {
    pub liquidations: Vec<Liquidation>,
    pub total: f64,
    pub has_more: bool,
}
// ts: PnLDataPoint
#[derive(Debug, Clone, PartialEq)]
pub struct PnLDataPoint {
    pub timestamp: f64,
    pub realized_pnl: Vec<u16>,
    pub unrealized_pnl: Vec<u16>,
    pub total_pnl: Vec<u16>,
    pub roi_percent: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PnLSummary {
    pub total_realized_pnl: Vec<u16>,
    pub total_unrealized_pnl: Vec<u16>,
    pub total_pnl: Vec<u16>,
    pub win_rate: Option<f64>,
}
// ts: PnL
#[derive(Debug, Clone, PartialEq)]
pub struct PnL {
    pub account_index: Vec<u16>,
    pub data: Vec<PnLDataPoint>,
    pub summary: Option<PnLSummary>,
}
// ts: GetLiquidationsOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetLiquidationsOptions {
    pub limit: Option<f64>,
    pub offset: Option<f64>,
    pub market_id: Option<f64>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
}
// ts: GetPnlOptions
#[derive(Debug, Clone, PartialEq)]
pub struct GetPnlOptions {
    pub period: Option<Vec<u16>>,
    pub market_id: Option<f64>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
}

/// Values crossing the declared response/options boundary. Objects and opaque
/// primitives retain identity. The host implements JS property access/coercion.
#[derive(Clone)]
pub enum AccountValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    BigInt(BigInt),
    String(Vec<u16>),
    Symbol(HostThrownValue),
    Function(HostThrownValue),
    Object(Rc<dyn AccountObject>),
    HtmlDda,
}
impl AccountValue {
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null | Self::HtmlDda => false,
            Self::Boolean(v) => *v,
            Self::Number(v) => *v != 0.0 && !v.is_nan(),
            Self::BigInt(v) => v != &BigInt::from(0),
            Self::String(v) => !v.is_empty(),
            _ => true,
        }
    }
    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Null | Self::Undefined)
    }
}
pub trait AccountObject {
    fn get(&self, key: &str) -> SigningResult<AccountValue>;
}
/// Get on native primitives is also explicit: no JSON approximation or
/// fabricated prototype behavior in the account method body.
pub trait AccountSemantics {
    fn get_property(&self, value: &AccountValue, key: &str) -> SigningResult<AccountValue>;
    fn error_message(&self, value: &AccountValue) -> SigningResult<Vec<u16>>;
    /// Apply ECMAScript Promise Resolve to the async return value, including
    /// Get(then), thenable call scheduling, recursive adoption, and rejection of
    /// getter/call/coercion exceptions with original identity. Ordinary values
    /// settle immediately; callable then jobs enter the supplied FIFO queue.
    fn resolve_return(
        self: Rc<Self>,
        value: AccountValue,
        resolver: PromiseResolver<AccountValue>,
        runtime: Rc<dyn TransactionRuntime>,
    );
}
#[derive(Clone)]
pub struct AccountRequestConfig {
    pub params: Utf16Record<AccountValue>,
}
pub trait AccountGet {
    /// Receiver binding belongs to this callable, extracted from the client.
    fn call(
        &self,
        url: Vec<u16>,
        config: Option<AccountRequestConfig>,
    ) -> SigningResult<NativePromise<AccountValue>>;
}
pub trait AccountHttpClient {
    fn get_callback(&self) -> SigningResult<Rc<dyn AccountGet>>;
}
// ts: AccountApi
pub struct AccountApi {
    client: Rc<dyn AccountHttpClient>,
    runtime: Rc<dyn TransactionRuntime>,
    semantics: Rc<dyn AccountSemantics>,
}
impl AccountApi {
    // ts: AccountApi.constructor
    pub fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
    ) -> Self {
        Self {
            client,
            runtime,
            semantics,
        }
    }
    fn request(
        &self,
        prepare: impl FnOnce() -> SigningResult<(Vec<u16>, Option<AccountRequestConfig>)>,
        fallback: &'static str,
    ) -> NativePromise<AccountValue> {
        self.request_projected(prepare, fallback, |_, value| Ok(value))
    }
    /// Shared source response guard; projection executes before async return resolution.
    pub(crate) fn request_projected(
        &self,
        prepare: impl FnOnce() -> SigningResult<(Vec<u16>, Option<AccountRequestConfig>)>,
        fallback: &'static str,
        project: impl FnOnce(&dyn AccountSemantics, AccountValue) -> SigningResult<AccountValue>
        + 'static,
    ) -> NativePromise<AccountValue> {
        self.request_handled(prepare, move |semantics, response| {
            let success = semantics.get_property(response, "success")?;
            if !success.truthy() || !semantics.get_property(response, "data")?.truthy() {
                let error = semantics.get_property(response, "error")?;
                let message = if error.is_nullish() {
                    AccountValue::Undefined
                } else {
                    semantics.get_property(&error, "message")?
                };
                let message = if message.truthy() {
                    semantics.error_message(&message)?
                } else {
                    fallback.encode_utf16().collect()
                };
                return Err(TransactionBuildError {
                    name: "Error",
                    message,
                }
                .into());
            }
            // Re-read data for return, matching accessor side effects.
            project(semantics, semantics.get_property(response, "data")?)
        })
    }
    /// Run the source response body in the original await continuation, before
    /// Promise Resolve. Used by clients whose failure constructs LighterApiError.
    pub(crate) fn request_handled(
        &self,
        prepare: impl FnOnce() -> SigningResult<(Vec<u16>, Option<AccountRequestConfig>)>,
        handle: impl FnOnce(&dyn AccountSemantics, &AccountValue) -> SigningResult<AccountValue>
        + 'static,
    ) -> NativePromise<AccountValue> {
        let (promise, resolver) = NativePromise::pending(self.runtime.clone());
        // The source evaluates the callee before URL/argument expressions.
        let next = (|| {
            let call = self.client.get_callback()?;
            let (url, config) = prepare()?;
            call.call(url, config)
        })();
        match next {
            Err(error) => resolver.settle(Err(error)),
            Ok(next) => {
                let semantics = self.semantics.clone();
                let runtime = self.runtime.clone();
                next.subscribe(move |result| {
                    let result = result.and_then(|response| handle(semantics.as_ref(), &response));
                    match result {
                        Ok(value) => semantics.resolve_return(value, resolver, runtime),
                        Err(error) => resolver.settle(Err(error)),
                    }
                });
            }
        }
        promise
    }
    fn account_path(index: &BigInt, suffix: &str) -> Vec<u16> {
        format!("/api/v1/accounts/{index}{suffix}")
            .encode_utf16()
            .collect()
    }
    // ts: AccountApi.getAccount
    pub fn get_account(&self, index: &BigInt) -> NativePromise<AccountValue> {
        self.request(
            || Ok((Self::account_path(index, ""), None)),
            "Failed to get account information",
        )
    }
    // ts: AccountApi.getAccountLimits
    pub fn get_account_limits(&self, index: &BigInt) -> NativePromise<AccountValue> {
        self.request(
            || Ok((Self::account_path(index, "/limits"), None)),
            "Failed to get account limits",
        )
    }
    // ts: AccountApi.getAccountMetadata
    pub fn get_account_metadata(&self, index: &BigInt) -> NativePromise<AccountValue> {
        self.request(
            || Ok((Self::account_path(index, "/metadata"), None)),
            "Failed to get account metadata",
        )
    }
    // ts: AccountApi.getAccountsByL1Address
    pub fn get_accounts_by_l1_address(&self, address: Vec<u16>) -> NativePromise<AccountValue> {
        self.request(
            || {
                Ok((
                    "/api/v1/accounts".encode_utf16().collect(),
                    Some(AccountRequestConfig {
                        params: [(
                            "l1Address".encode_utf16().collect(),
                            AccountValue::String(address),
                        )]
                        .into(),
                    }),
                ))
            },
            "Failed to get accounts by L1 address",
        )
    }
    // ts: AccountApi.getApiKeys
    pub fn get_api_keys(&self, index: &BigInt) -> NativePromise<AccountValue> {
        self.request(
            || Ok((Self::account_path(index, "/api-keys"), None)),
            "Failed to get API keys",
        )
    }
    fn params(
        &self,
        options: Option<&AccountValue>,
        fields: &[&str],
    ) -> SigningResult<AccountRequestConfig> {
        let mut params = Utf16Record::new();
        for field in fields {
            let first = match options {
                Some(value) if !value.is_nullish() => self.semantics.get_property(value, field)?,
                _ => AccountValue::Undefined,
            };
            if !matches!(first, AccountValue::Undefined) {
                params.insert(
                    field.encode_utf16().collect(),
                    match options {
                        Some(value) => self.semantics.get_property(value, field)?,
                        None => AccountValue::Undefined,
                    },
                );
            }
        }
        Ok(AccountRequestConfig { params })
    }
    // ts: AccountApi.getLiquidations
    pub fn get_liquidations(
        &self,
        index: &BigInt,
        options: Option<&AccountValue>,
    ) -> NativePromise<AccountValue> {
        // Query getters run before resolving client.get in the source.
        match self.params(
            options,
            &["limit", "offset", "marketId", "startTime", "endTime"],
        ) {
            Ok(config) => self.request(
                || Ok((Self::account_path(index, "/liquidations"), Some(config))),
                "Failed to get liquidations",
            ),
            Err(error) => NativePromise::settled(self.runtime.clone(), Err(error)),
        }
    }
    // ts: AccountApi.getPnl
    pub fn get_pnl(
        &self,
        index: &BigInt,
        options: Option<&AccountValue>,
    ) -> NativePromise<AccountValue> {
        match self.params(options, &["period", "marketId", "startTime", "endTime"]) {
            Ok(config) => self.request(
                || Ok((Self::account_path(index, "/pnl"), Some(config))),
                "Failed to get PnL data",
            ),
            Err(error) => NativePromise::settled(self.runtime.clone(), Err(error)),
        }
    }
}
