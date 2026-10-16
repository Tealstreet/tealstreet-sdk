//! Market-data clients at the existing native HTTP/ECMAScript host boundary.
use crate::{
    sdk_account_api::{
        AccountApi, AccountHttpClient, AccountRequestConfig, AccountSemantics, AccountValue,
    },
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::{HostThrownValue, SigningResult},
};
use std::rc::Rc;

// ts: Candlestick
#[derive(Debug, Clone, PartialEq)]
pub struct Candlestick {
    pub timestamp: f64,
    pub open: Vec<u16>,
    pub high: Vec<u16>,
    pub low: Vec<u16>,
    pub close: Vec<u16>,
    pub volume: Vec<u16>,
}
// ts: CandlesticksResponse
#[derive(Debug, Clone, PartialEq)]
pub struct CandlesticksResponse {
    pub candlesticks: Vec<Candlestick>,
    pub market_id: f64,
    pub resolution: Vec<u16>,
}
// ts: Funding
#[derive(Debug, Clone, PartialEq)]
pub struct Funding {
    pub timestamp: f64,
    pub rate: Vec<u16>,
    pub mark_price: Option<Vec<u16>>,
    pub index_price: Option<Vec<u16>>,
}
// ts: FundingsResponse
#[derive(Debug, Clone, PartialEq)]
pub struct FundingsResponse {
    pub fundings: Vec<Funding>,
    pub market_id: f64,
    pub resolution: Vec<u16>,
}
// ts: FundingRate
#[derive(Debug, Clone, PartialEq)]
pub struct FundingRate {
    pub market_id: f64,
    pub symbol: Option<Vec<u16>>,
    pub rate: Vec<u16>,
    pub next_funding_time: Option<f64>,
    pub predicted_rate: Option<Vec<u16>>,
    pub mark_price: Option<Vec<u16>>,
    pub index_price: Option<Vec<u16>>,
    pub timestamp: Option<f64>,
}
// ts: FundingRatesResponse
#[derive(Debug, Clone, PartialEq)]
pub struct FundingRatesResponse {
    pub funding_rates: Vec<FundingRate>,
}
// ts: WithdrawalDelay
#[derive(Debug, Clone, PartialEq)]
pub struct WithdrawalDelay {
    pub delay_seconds: f64,
    pub delay_ms: Option<f64>,
    pub description: Option<Vec<u16>>,
}
// ts: WithdrawalDelayResponse
#[derive(Debug, Clone, PartialEq)]
pub struct WithdrawalDelayResponse {
    pub withdrawal_delay: WithdrawalDelay,
}

/// Construct the actual source LighterApiError with two evaluated arguments.
/// Must implement its Error(message) conversion, default code semantics, brand,
/// details/requestId/statusCode assignments and optional captureStackTrace call;
/// constructor/coercion exceptions propagate with their original identity.
/// The method body deliberately does not pre-coerce either argument: the code
/// getter runs before Error(message) coercion, and code may be any host value.
pub trait MarketApiErrors {
    fn construct(
        &self,
        message: AccountValue,
        code: AccountValue,
    ) -> SigningResult<HostThrownValue>;
}
struct MarketRequest {
    api: AccountApi,
    errors: Rc<dyn MarketApiErrors>,
}
impl MarketRequest {
    fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
        errors: Rc<dyn MarketApiErrors>,
    ) -> Self {
        Self {
            api: AccountApi::new(client, runtime, semantics),
            errors,
        }
    }
    fn request(
        &self,
        path: &'static str,
        config: Option<AccountRequestConfig>,
        message: &'static str,
        code: &'static str,
    ) -> NativePromise<AccountValue> {
        let errors = self.errors.clone();
        self.api.request_handled(
            || Ok((path.encode_utf16().collect(), config)),
            move |semantics, response| {
                let success = semantics.get_property(response, "success")?;
                if !success.truthy() || !semantics.get_property(response, "data")?.truthy() {
                    let error = semantics.get_property(response, "error")?;
                    let raw_message = if error.is_nullish() {
                        AccountValue::Undefined
                    } else {
                        semantics.get_property(&error, "message")?
                    };
                    let message = if raw_message.truthy() {
                        raw_message
                    } else {
                        AccountValue::String(message.encode_utf16().collect())
                    };
                    // Separate response.error access for the second argument, even
                    // if the first access produced null or a different object.
                    let error = semantics.get_property(response, "error")?;
                    let raw_code = if error.is_nullish() {
                        AccountValue::Undefined
                    } else {
                        semantics.get_property(&error, "code")?
                    };
                    let code = if raw_code.truthy() {
                        raw_code
                    } else {
                        AccountValue::String(code.encode_utf16().collect())
                    };
                    return Err(errors.construct(message, code)?.into());
                }
                semantics.get_property(response, "data")
            },
        )
    }
}
// ts: CandlestickApi
pub struct CandlestickApi {
    request: MarketRequest,
}
impl CandlestickApi {
    // ts: CandlestickApi.constructor
    pub fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
        errors: Rc<dyn MarketApiErrors>,
    ) -> Self {
        Self {
            request: MarketRequest::new(client, runtime, semantics, errors),
        }
    }
    fn config(
        market_id: f64,
        resolution: Vec<u16>,
        start_ts: f64,
        end_ts: f64,
        count_back: f64,
    ) -> AccountRequestConfig {
        AccountRequestConfig {
            params: [
                ("marketId", AccountValue::Number(market_id)),
                ("resolution", AccountValue::String(resolution)),
                ("startTimestamp", AccountValue::Number(start_ts)),
                ("endTimestamp", AccountValue::Number(end_ts)),
                ("countBack", AccountValue::Number(count_back)),
            ]
            .into_iter()
            .map(|(key, value)| (key.encode_utf16().collect(), value))
            .collect(),
        }
    }
    // ts: CandlestickApi.getCandlesticks
    pub fn get_candlesticks(
        &self,
        market_id: f64,
        resolution: Vec<u16>,
        start_ts: f64,
        end_ts: f64,
        count_back: f64,
    ) -> NativePromise<AccountValue> {
        self.request.request(
            "/api/v1/candlesticks",
            Some(Self::config(
                market_id, resolution, start_ts, end_ts, count_back,
            )),
            "Failed to get candlesticks",
            "GET_CANDLESTICKS_FAILED",
        )
    }
    // ts: CandlestickApi.getFundings
    pub fn get_fundings(
        &self,
        market_id: f64,
        resolution: Vec<u16>,
        start_ts: f64,
        end_ts: f64,
        count_back: f64,
    ) -> NativePromise<AccountValue> {
        self.request.request(
            "/api/v1/fundings",
            Some(Self::config(
                market_id, resolution, start_ts, end_ts, count_back,
            )),
            "Failed to get fundings",
            "GET_FUNDINGS_FAILED",
        )
    }
}
// ts: FundingApi
pub struct FundingApi {
    request: MarketRequest,
}
impl FundingApi {
    // ts: FundingApi.constructor
    pub fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
        errors: Rc<dyn MarketApiErrors>,
    ) -> Self {
        Self {
            request: MarketRequest::new(client, runtime, semantics, errors),
        }
    }
    // ts: FundingApi.getFundingRates
    pub fn get_funding_rates(&self) -> NativePromise<AccountValue> {
        self.request.request(
            "/api/v1/funding-rates",
            None,
            "Failed to get funding rates",
            "GET_FUNDING_RATES_FAILED",
        )
    }
}
// ts: InfoApi
pub struct InfoApi {
    request: MarketRequest,
}
impl InfoApi {
    // ts: InfoApi.constructor
    pub fn new(
        client: Rc<dyn AccountHttpClient>,
        runtime: Rc<dyn TransactionRuntime>,
        semantics: Rc<dyn AccountSemantics>,
        errors: Rc<dyn MarketApiErrors>,
    ) -> Self {
        Self {
            request: MarketRequest::new(client, runtime, semantics, errors),
        }
    }
    // ts: InfoApi.getWithdrawalDelay
    pub fn get_withdrawal_delay(&self) -> NativePromise<AccountValue> {
        self.request.request(
            "/api/v1/withdrawalDelay",
            None,
            "Failed to get withdrawal delay",
            "GET_WITHDRAWAL_DELAY_FAILED",
        )
    }
}
