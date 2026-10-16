//! Rust-owned LighterExchange class composition and direct native body dispatch.
//! Realm operations cover JavaScript construction, descriptors, traced lexical
//! environments and callable/Promise representation; no exchange body is delegated.
use crate::exchange_account_access::{self, ExchangeAccountAccessHost};
use crate::exchange_account_cache::{self, AccountCacheHost};
use crate::exchange_account_lookup::{self, AccountLookupHost};
use crate::exchange_batch_placement::{self, BatchPlacementHost};
use crate::exchange_error_recovery::{self, ExchangeErrorRecoveryHost};
use crate::exchange_historical_fills::{self, HistoricalFillsHost};
use crate::exchange_live_helpers::{self, LiveHelperHost};
use crate::exchange_market_reads::{self, MarketReadHost};
use crate::exchange_ohlcv::{self, OhlcvHost};
use crate::exchange_order_actions::{self, OrderActionHost};
use crate::exchange_order_mapping::{self, OrderMappingHost};
use crate::exchange_order_snapshot::{self, OrderSnapshotHost};
use crate::exchange_positions::{self, PositionReadHost};
use crate::exchange_reconciliation::{self, ReconciliationHost};
use crate::exchange_runtime_initialization::{self, RuntimeInitializationHost};
use crate::exchange_single_placement::{self, SinglePlacementHost};
use crate::exchange_trading_preflight::{self, TradingHost};
use crate::{
    sdk_account_api::AccountValue, transaction_promise::NativePromise,
    transaction_signing::SigningResult,
};
use std::rc::Rc;

/// Contexts bind live operations to the invocation's actual owner and class realm.
pub trait ExchangeClassContexts {
    fn access(&self, owner: AccountValue) -> Rc<dyn ExchangeAccountAccessHost>;
    fn lookup(&self, owner: AccountValue) -> Rc<dyn AccountLookupHost>;
    fn cache(&self, owner: AccountValue) -> Rc<dyn AccountCacheHost>;
    fn trading(&self, owner: AccountValue) -> Rc<dyn TradingHost>;
    fn errors(&self, owner: AccountValue) -> Rc<dyn ExchangeErrorRecoveryHost>;
    fn initialization(&self, owner: AccountValue) -> Rc<dyn RuntimeInitializationHost>;
    fn reconciliation(&self, owner: AccountValue) -> Rc<dyn ReconciliationHost>;
    fn markets(&self, owner: AccountValue) -> Rc<dyn MarketReadHost>;
    fn positions(&self, owner: AccountValue) -> Rc<dyn PositionReadHost>;
    fn order_snapshot(&self, owner: AccountValue) -> Rc<dyn OrderSnapshotHost>;
    fn fills(&self, owner: AccountValue) -> Rc<dyn HistoricalFillsHost>;
    fn ohlcv(&self, owner: AccountValue) -> Rc<dyn OhlcvHost>;
    fn order_actions(&self, owner: AccountValue) -> Rc<dyn OrderActionHost>;
    fn single_placement(&self, owner: AccountValue) -> Rc<dyn SinglePlacementHost>;
    fn batch_placement(&self, owner: AccountValue) -> Rc<dyn BatchPlacementHost>;
    fn order_mapping(&self, owner: AccountValue) -> Rc<dyn OrderMappingHost>;
    fn live_helpers(&self, owner: AccountValue) -> Rc<dyn LiveHelperHost>;
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodMetadata {
    pub name: &'static str,
    pub length: usize,
    pub asynchronous: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExchangeMethod {
    ParseAccountIndex,
    RememberPlacedOrderHint,
    ReadPlacedOrderHint,
    InferStopLossTypeFromReduceOnlyLimit,
    ExtractAccountFromResponse,
    FetchParsedAccountLookup,
    FetchAccountFromL1AddressLookup,
    EnsureAccountIndex,
    GetValidatedApiKeyIndex,
    ExtractL1AddressCandidate,
    FetchAccountInfo,
    FetchAccountSnapshot,
    RememberRecentAccountSnapshot,
    InvalidateAccountSnapshotCache,
    FetchAccountLimits,
    GetCachedAccountTier,
    EnsurePartnerAttributionTierEligible,
    EnsurePartnerAttributionApproved,
    GetAccountRateLimitGuidanceMessage,
    ChangeAccountTier,
    WarmupSignerRuntime,
    NormalizeSignerErrorMessage,
    IsWasmRuntimeIssueMessage,
    ShouldShortCircuitSignerRetry,
    GetSignerRetryBlockMessage,
    WarnSignerUnavailable,
    EnsureSignerAdapter,
    Dispose,
    GetAccount,
    ValidateAccount,
    Start,
    GetInitialTickDelay,
    FetchAndSetPrivateSnapshot,
    FetchAndSetPrivateSnapshotHelper,
    Tick,
    FetchBalance,
    FetchAndSetBalance,
    FetchRawBalance,
    ParseMarginMode,
    MarginModeToMarginType,
    MarginTypeToMarginMode,
    GetMarketConfiguredMarginMode,
    GetCurrentMarginModeForSymbol,
    GetCurrentLeverageForSymbol,
    FetchPositions,
    ShouldSkipOrderReconciliationFetch,
    FetchOrders,
    FetchOrdersOnce,
    FetchHistoricalFills,
    FetchMarkets,
    FetchLighterMarketRows,
    FetchMarketsHelper,
    FetchOHLCV,
    GetOhlcvFallbackKey,
    FetchLatestOhlcvWithSharedFallback,
    ListenOHLCV,
    ListenOrderBook,
    GetPrivateWsAuthToken,
    GetPrivateWsAccountIndex,
    GetPrivateWsApiKeyIndex,
    GetReduceOnlyClosingPositionSide,
    HasOpenReduceOnlyTargetPosition,
    BuildNoPositionReduceOnlyMessage,
    IsTransientLighterRestTransportError,
    HandlePrivateSnapshotFetchError,
    ExtractOrderErrorMessage,
    IsRateLimitOrderError,
    IsInvalidMarginModeOrderError,
    IsPartnerAttributionTierOrderError,
    IsPartnerAttributionApprovalMissingOrderError,
    BuildInvalidMarginModeOrderMessage,
    RefreshMarginModeStateAfterInvalidOrder,
    NormalizePlaceOrderErrorMessage,
    AssertReduceOnlyLimitHasPosition,
    GetOrderMinimumViolationReasons,
    GetPositivePriceCandidate,
    ResolveMarketOrderReferencePrice,
    ResolveStopSlippagePct,
    ResolveConditionalExecutionPrice,
    PlaceOrder,
    ShouldUseBatchedCreateOrders,
    PlaceOrdersSequentially,
    PlaceOrders,
    UpdateOrder,
    CancelOrders,
    CancelSymbolOrders,
    SnapToLeverageTier,
    SetLeverage,
    ChangeMarginType,
    SyncLeverageFromExchange,
    ResolvePositionLeverage,
    ResolvePositionLiquidationPrice,
    MapPosition,
    MapOrder,
}
impl ExchangeMethod {
    pub fn metadata(self) -> MethodMetadata {
        match self {
            Self::ParseAccountIndex => MethodMetadata {
                name: "parseAccountIndex",
                length: 1,
                asynchronous: false,
            },
            Self::RememberPlacedOrderHint => MethodMetadata {
                name: "rememberPlacedOrderHint",
                length: 2,
                asynchronous: false,
            },
            Self::ReadPlacedOrderHint => MethodMetadata {
                name: "readPlacedOrderHint",
                length: 1,
                asynchronous: false,
            },
            Self::InferStopLossTypeFromReduceOnlyLimit => MethodMetadata {
                name: "inferStopLossTypeFromReduceOnlyLimit",
                length: 1,
                asynchronous: false,
            },
            Self::ExtractAccountFromResponse => MethodMetadata {
                name: "extractAccountFromResponse",
                length: 1,
                asynchronous: false,
            },
            Self::FetchParsedAccountLookup => MethodMetadata {
                name: "fetchParsedAccountLookup",
                length: 1,
                asynchronous: true,
            },
            Self::FetchAccountFromL1AddressLookup => MethodMetadata {
                name: "fetchAccountFromL1AddressLookup",
                length: 1,
                asynchronous: true,
            },
            Self::EnsureAccountIndex => MethodMetadata {
                name: "ensureAccountIndex",
                length: 0,
                asynchronous: true,
            },
            Self::GetValidatedApiKeyIndex => MethodMetadata {
                name: "getValidatedApiKeyIndex",
                length: 0,
                asynchronous: false,
            },
            Self::ExtractL1AddressCandidate => MethodMetadata {
                name: "extractL1AddressCandidate",
                length: 1,
                asynchronous: false,
            },
            Self::FetchAccountInfo => MethodMetadata {
                name: "fetchAccountInfo",
                length: 0,
                asynchronous: true,
            },
            Self::FetchAccountSnapshot => MethodMetadata {
                name: "fetchAccountSnapshot",
                length: 0,
                asynchronous: true,
            },
            Self::RememberRecentAccountSnapshot => MethodMetadata {
                name: "rememberRecentAccountSnapshot",
                length: 2,
                asynchronous: false,
            },
            Self::InvalidateAccountSnapshotCache => MethodMetadata {
                name: "invalidateAccountSnapshotCache",
                length: 0,
                asynchronous: false,
            },
            Self::FetchAccountLimits => MethodMetadata {
                name: "fetchAccountLimits",
                length: 0,
                asynchronous: true,
            },
            Self::GetCachedAccountTier => MethodMetadata {
                name: "getCachedAccountTier",
                length: 0,
                asynchronous: false,
            },
            Self::EnsurePartnerAttributionTierEligible => MethodMetadata {
                name: "ensurePartnerAttributionTierEligible",
                length: 1,
                asynchronous: true,
            },
            Self::EnsurePartnerAttributionApproved => MethodMetadata {
                name: "ensurePartnerAttributionApproved",
                length: 1,
                asynchronous: true,
            },
            Self::GetAccountRateLimitGuidanceMessage => MethodMetadata {
                name: "getAccountRateLimitGuidanceMessage",
                length: 0,
                asynchronous: false,
            },
            Self::ChangeAccountTier => MethodMetadata {
                name: "changeAccountTier",
                length: 1,
                asynchronous: true,
            },
            Self::WarmupSignerRuntime => MethodMetadata {
                name: "warmupSignerRuntime",
                length: 0,
                asynchronous: true,
            },
            Self::NormalizeSignerErrorMessage => MethodMetadata {
                name: "normalizeSignerErrorMessage",
                length: 1,
                asynchronous: false,
            },
            Self::IsWasmRuntimeIssueMessage => MethodMetadata {
                name: "isWasmRuntimeIssueMessage",
                length: 1,
                asynchronous: false,
            },
            Self::ShouldShortCircuitSignerRetry => MethodMetadata {
                name: "shouldShortCircuitSignerRetry",
                length: 0,
                asynchronous: false,
            },
            Self::GetSignerRetryBlockMessage => MethodMetadata {
                name: "getSignerRetryBlockMessage",
                length: 0,
                asynchronous: false,
            },
            Self::WarnSignerUnavailable => MethodMetadata {
                name: "warnSignerUnavailable",
                length: 1,
                asynchronous: false,
            },
            Self::EnsureSignerAdapter => MethodMetadata {
                name: "ensureSignerAdapter",
                length: 0,
                asynchronous: true,
            },
            Self::Dispose => MethodMetadata {
                name: "dispose",
                length: 0,
                asynchronous: false,
            },
            Self::GetAccount => MethodMetadata {
                name: "getAccount",
                length: 0,
                asynchronous: true,
            },
            Self::ValidateAccount => MethodMetadata {
                name: "validateAccount",
                length: 0,
                asynchronous: true,
            },
            Self::Start => MethodMetadata {
                name: "start",
                length: 0,
                asynchronous: true,
            },
            Self::GetInitialTickDelay => MethodMetadata {
                name: "getInitialTickDelay",
                length: 0,
                asynchronous: false,
            },
            Self::FetchAndSetPrivateSnapshot => MethodMetadata {
                name: "fetchAndSetPrivateSnapshot",
                length: 0,
                asynchronous: true,
            },
            Self::FetchAndSetPrivateSnapshotHelper => MethodMetadata {
                name: "fetchAndSetPrivateSnapshotHelper",
                length: 0,
                asynchronous: true,
            },
            Self::Tick => MethodMetadata {
                name: "tick",
                length: 0,
                asynchronous: true,
            },
            Self::FetchBalance => MethodMetadata {
                name: "fetchBalance",
                length: 0,
                asynchronous: true,
            },
            Self::FetchAndSetBalance => MethodMetadata {
                name: "fetchAndSetBalance",
                length: 0,
                asynchronous: true,
            },
            Self::FetchRawBalance => MethodMetadata {
                name: "fetchRawBalance",
                length: 0,
                asynchronous: true,
            },
            Self::ParseMarginMode => MethodMetadata {
                name: "parseMarginMode",
                length: 1,
                asynchronous: false,
            },
            Self::MarginModeToMarginType => MethodMetadata {
                name: "marginModeToMarginType",
                length: 1,
                asynchronous: false,
            },
            Self::MarginTypeToMarginMode => MethodMetadata {
                name: "marginTypeToMarginMode",
                length: 1,
                asynchronous: false,
            },
            Self::GetMarketConfiguredMarginMode => MethodMetadata {
                name: "getMarketConfiguredMarginMode",
                length: 1,
                asynchronous: false,
            },
            Self::GetCurrentMarginModeForSymbol => MethodMetadata {
                name: "getCurrentMarginModeForSymbol",
                length: 1,
                asynchronous: false,
            },
            Self::GetCurrentLeverageForSymbol => MethodMetadata {
                name: "getCurrentLeverageForSymbol",
                length: 2,
                asynchronous: false,
            },
            Self::FetchPositions => MethodMetadata {
                name: "fetchPositions",
                length: 1,
                asynchronous: true,
            },
            Self::ShouldSkipOrderReconciliationFetch => MethodMetadata {
                name: "shouldSkipOrderReconciliationFetch",
                length: 0,
                asynchronous: false,
            },
            Self::FetchOrders => MethodMetadata {
                name: "fetchOrders",
                length: 0,
                asynchronous: true,
            },
            Self::FetchOrdersOnce => MethodMetadata {
                name: "fetchOrdersOnce",
                length: 0,
                asynchronous: true,
            },
            Self::FetchHistoricalFills => MethodMetadata {
                name: "fetchHistoricalFills",
                length: 1,
                asynchronous: true,
            },
            Self::FetchMarkets => MethodMetadata {
                name: "fetchMarkets",
                length: 0,
                asynchronous: true,
            },
            Self::FetchLighterMarketRows => MethodMetadata {
                name: "fetchLighterMarketRows",
                length: 0,
                asynchronous: true,
            },
            Self::FetchMarketsHelper => MethodMetadata {
                name: "fetchMarketsHelper",
                length: 0,
                asynchronous: true,
            },
            Self::FetchOHLCV => MethodMetadata {
                name: "fetchOHLCV",
                length: 1,
                asynchronous: true,
            },
            Self::GetOhlcvFallbackKey => MethodMetadata {
                name: "getOhlcvFallbackKey",
                length: 1,
                asynchronous: false,
            },
            Self::FetchLatestOhlcvWithSharedFallback => MethodMetadata {
                name: "fetchLatestOhlcvWithSharedFallback",
                length: 2,
                asynchronous: true,
            },
            Self::ListenOHLCV => MethodMetadata {
                name: "listenOHLCV",
                length: 2,
                asynchronous: false,
            },
            Self::ListenOrderBook => MethodMetadata {
                name: "listenOrderBook",
                length: 2,
                asynchronous: false,
            },
            Self::GetPrivateWsAuthToken => MethodMetadata {
                name: "getPrivateWsAuthToken",
                length: 0,
                asynchronous: true,
            },
            Self::GetPrivateWsAccountIndex => MethodMetadata {
                name: "getPrivateWsAccountIndex",
                length: 0,
                asynchronous: false,
            },
            Self::GetPrivateWsApiKeyIndex => MethodMetadata {
                name: "getPrivateWsApiKeyIndex",
                length: 0,
                asynchronous: false,
            },
            Self::GetReduceOnlyClosingPositionSide => MethodMetadata {
                name: "getReduceOnlyClosingPositionSide",
                length: 1,
                asynchronous: false,
            },
            Self::HasOpenReduceOnlyTargetPosition => MethodMetadata {
                name: "hasOpenReduceOnlyTargetPosition",
                length: 2,
                asynchronous: false,
            },
            Self::BuildNoPositionReduceOnlyMessage => MethodMetadata {
                name: "buildNoPositionReduceOnlyMessage",
                length: 2,
                asynchronous: false,
            },
            Self::IsTransientLighterRestTransportError => MethodMetadata {
                name: "isTransientLighterRestTransportError",
                length: 1,
                asynchronous: false,
            },
            Self::HandlePrivateSnapshotFetchError => MethodMetadata {
                name: "handlePrivateSnapshotFetchError",
                length: 1,
                asynchronous: false,
            },
            Self::ExtractOrderErrorMessage => MethodMetadata {
                name: "extractOrderErrorMessage",
                length: 1,
                asynchronous: false,
            },
            Self::IsRateLimitOrderError => MethodMetadata {
                name: "isRateLimitOrderError",
                length: 2,
                asynchronous: false,
            },
            Self::IsInvalidMarginModeOrderError => MethodMetadata {
                name: "isInvalidMarginModeOrderError",
                length: 2,
                asynchronous: false,
            },
            Self::IsPartnerAttributionTierOrderError => MethodMetadata {
                name: "isPartnerAttributionTierOrderError",
                length: 2,
                asynchronous: false,
            },
            Self::IsPartnerAttributionApprovalMissingOrderError => MethodMetadata {
                name: "isPartnerAttributionApprovalMissingOrderError",
                length: 2,
                asynchronous: false,
            },
            Self::BuildInvalidMarginModeOrderMessage => MethodMetadata {
                name: "buildInvalidMarginModeOrderMessage",
                length: 1,
                asynchronous: false,
            },
            Self::RefreshMarginModeStateAfterInvalidOrder => MethodMetadata {
                name: "refreshMarginModeStateAfterInvalidOrder",
                length: 1,
                asynchronous: true,
            },
            Self::NormalizePlaceOrderErrorMessage => MethodMetadata {
                name: "normalizePlaceOrderErrorMessage",
                length: 2,
                asynchronous: false,
            },
            Self::AssertReduceOnlyLimitHasPosition => MethodMetadata {
                name: "assertReduceOnlyLimitHasPosition",
                length: 1,
                asynchronous: true,
            },
            Self::GetOrderMinimumViolationReasons => MethodMetadata {
                name: "getOrderMinimumViolationReasons",
                length: 4,
                asynchronous: false,
            },
            Self::GetPositivePriceCandidate => MethodMetadata {
                name: "getPositivePriceCandidate",
                length: 0,
                asynchronous: false,
            },
            Self::ResolveMarketOrderReferencePrice => MethodMetadata {
                name: "resolveMarketOrderReferencePrice",
                length: 2,
                asynchronous: false,
            },
            Self::ResolveStopSlippagePct => MethodMetadata {
                name: "resolveStopSlippagePct",
                length: 1,
                asynchronous: false,
            },
            Self::ResolveConditionalExecutionPrice => MethodMetadata {
                name: "resolveConditionalExecutionPrice",
                length: 3,
                asynchronous: false,
            },
            Self::PlaceOrder => MethodMetadata {
                name: "placeOrder",
                length: 1,
                asynchronous: true,
            },
            Self::ShouldUseBatchedCreateOrders => MethodMetadata {
                name: "shouldUseBatchedCreateOrders",
                length: 1,
                asynchronous: false,
            },
            Self::PlaceOrdersSequentially => MethodMetadata {
                name: "placeOrdersSequentially",
                length: 1,
                asynchronous: true,
            },
            Self::PlaceOrders => MethodMetadata {
                name: "placeOrders",
                length: 1,
                asynchronous: true,
            },
            Self::UpdateOrder => MethodMetadata {
                name: "updateOrder",
                length: 1,
                asynchronous: true,
            },
            Self::CancelOrders => MethodMetadata {
                name: "cancelOrders",
                length: 1,
                asynchronous: true,
            },
            Self::CancelSymbolOrders => MethodMetadata {
                name: "cancelSymbolOrders",
                length: 1,
                asynchronous: true,
            },
            Self::SnapToLeverageTier => MethodMetadata {
                name: "snapToLeverageTier",
                length: 1,
                asynchronous: false,
            },
            Self::SetLeverage => MethodMetadata {
                name: "setLeverage",
                length: 3,
                asynchronous: true,
            },
            Self::ChangeMarginType => MethodMetadata {
                name: "changeMarginType",
                length: 2,
                asynchronous: true,
            },
            Self::SyncLeverageFromExchange => MethodMetadata {
                name: "syncLeverageFromExchange",
                length: 1,
                asynchronous: true,
            },
            Self::ResolvePositionLeverage => MethodMetadata {
                name: "resolvePositionLeverage",
                length: 2,
                asynchronous: false,
            },
            Self::ResolvePositionLiquidationPrice => MethodMetadata {
                name: "resolvePositionLiquidationPrice",
                length: 7,
                asynchronous: false,
            },
            Self::MapPosition => MethodMetadata {
                name: "mapPosition",
                length: 1,
                asynchronous: false,
            },
            Self::MapOrder => MethodMetadata {
                name: "mapOrder",
                length: 1,
                asynchronous: false,
            },
        }
    }
    /// Called by the realm with traced lexical owner for arrows or actual this
    /// for prototype methods. The sealed match invokes Rust bodies directly.
    pub fn invoke(
        self,
        contexts: &dyn ExchangeClassContexts,
        owner: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        match self {
            Self::ParseAccountIndex => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::parse_account_index(h, args)
            }
            Self::RememberPlacedOrderHint => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::remember_placed_order_hint(h, args)
            }
            Self::ReadPlacedOrderHint => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::read_placed_order_hint(h, args)
            }
            Self::InferStopLossTypeFromReduceOnlyLimit => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::infer_stop_loss_type_from_reduce_only_limit(h, args)
            }
            Self::ExtractAccountFromResponse => {
                let h = contexts.access(owner);
                exchange_account_access::extract_account_from_response(h.as_ref(), &arg(args, 0))
            }
            Self::FetchParsedAccountLookup => {
                let h = contexts.access(owner);
                Ok(
                    contexts.promise_value(exchange_account_access::fetch_parsed_account_lookup(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::FetchAccountFromL1AddressLookup => {
                let h = contexts.lookup(owner);
                Ok(contexts.promise_value(
                    exchange_account_lookup::fetch_account_from_l1_address_lookup(h, arg(args, 0)),
                ))
            }
            Self::EnsureAccountIndex => {
                let h = contexts.access(owner);
                Ok(contexts.promise_value(exchange_account_access::ensure_account_index(h)))
            }
            Self::GetValidatedApiKeyIndex => {
                let h = contexts.access(owner);
                exchange_account_access::get_validated_api_key_index(h.as_ref())
            }
            Self::ExtractL1AddressCandidate => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::extract_l1_address_candidate(h, args)
            }
            Self::FetchAccountInfo => {
                let h = contexts.lookup(owner);
                Ok(contexts.promise_value(exchange_account_lookup::fetch_account_info(h)))
            }
            Self::FetchAccountSnapshot => {
                let h = contexts.cache(owner);
                Ok(
                    contexts.promise_value(exchange_account_cache::fetch_account_snapshot(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::RememberRecentAccountSnapshot => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::remember_recent_account_snapshot(h, args)
            }
            Self::InvalidateAccountSnapshotCache => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::invalidate_account_snapshot_cache(h, args)
            }
            Self::FetchAccountLimits => {
                let h = contexts.cache(owner);
                Ok(contexts.promise_value(exchange_account_cache::fetch_account_limits(h)))
            }
            Self::GetCachedAccountTier => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_cached_account_tier(h, args)
            }
            Self::EnsurePartnerAttributionTierEligible => {
                let h = contexts.trading(owner);
                Ok(contexts.promise_value(
                    exchange_trading_preflight::ensure_partner_attribution_tier_eligible(
                        h,
                        arg(args, 0),
                    ),
                ))
            }
            Self::EnsurePartnerAttributionApproved => {
                let h = contexts.trading(owner);
                Ok(contexts.promise_value(
                    exchange_trading_preflight::ensure_partner_attribution_approved(
                        h,
                        arg(args, 0),
                    ),
                ))
            }
            Self::GetAccountRateLimitGuidanceMessage => {
                let h = contexts.errors(owner);
                exchange_error_recovery::get_account_rate_limit_guidance_message(h.as_ref())
            }
            Self::ChangeAccountTier => {
                let h = contexts.initialization(owner);
                Ok(
                    contexts.promise_value(exchange_runtime_initialization::change_account_tier(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::WarmupSignerRuntime => {
                let h = contexts.access(owner);
                Ok(contexts.promise_value(exchange_account_access::warmup_signer_runtime(h)))
            }
            Self::NormalizeSignerErrorMessage => {
                let h = contexts.errors(owner);
                exchange_error_recovery::normalize_signer_error_message(h.as_ref(), &arg(args, 0))
            }
            Self::IsWasmRuntimeIssueMessage => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::is_wasm_runtime_issue_message(h, args)
            }
            Self::ShouldShortCircuitSignerRetry => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::should_short_circuit_signer_retry(h, args)
            }
            Self::GetSignerRetryBlockMessage => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_signer_retry_block_message(h, args)
            }
            Self::WarnSignerUnavailable => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::warn_signer_unavailable(h, args)
            }
            Self::EnsureSignerAdapter => {
                let h = contexts.initialization(owner);
                Ok(contexts
                    .promise_value(exchange_runtime_initialization::ensure_signer_adapter(h)))
            }
            Self::Dispose => {
                let h = contexts.reconciliation(owner);
                exchange_reconciliation::dispose(h.as_ref())
            }
            Self::GetAccount => {
                let h = contexts.access(owner);
                Ok(contexts.promise_value(exchange_account_access::get_account(h)))
            }
            Self::ValidateAccount => {
                let h = contexts.initialization(owner);
                Ok(contexts.promise_value(exchange_runtime_initialization::validate_account(h)))
            }
            Self::Start => {
                let h = contexts.initialization(owner);
                Ok(contexts.promise_value(exchange_runtime_initialization::start_exchange(h)))
            }
            Self::GetInitialTickDelay => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_initial_tick_delay(h, args)
            }
            Self::FetchAndSetPrivateSnapshot => {
                let h = contexts.reconciliation(owner);
                Ok(contexts
                    .promise_value(exchange_reconciliation::fetch_and_set_private_snapshot(h)))
            }
            Self::FetchAndSetPrivateSnapshotHelper => {
                let h = contexts.reconciliation(owner);
                Ok(contexts.promise_value(
                    exchange_reconciliation::fetch_and_set_private_snapshot_helper(h),
                ))
            }
            Self::Tick => {
                let h = contexts.reconciliation(owner);
                Ok(contexts.promise_value(exchange_reconciliation::tick(h)))
            }
            Self::FetchBalance => {
                let h = contexts.reconciliation(owner);
                Ok(contexts.promise_value(exchange_reconciliation::fetch_balance(h, arg(args, 0))))
            }
            Self::FetchAndSetBalance => {
                let h = contexts.reconciliation(owner);
                Ok(
                    contexts.promise_value(exchange_reconciliation::fetch_and_set_balance(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::FetchRawBalance => {
                let h = contexts.markets(owner);
                Ok(contexts
                    .promise_value(exchange_market_reads::fetch_raw_balance(h, arg(args, 0))))
            }
            Self::ParseMarginMode => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::parse_margin_mode(h, args)
            }
            Self::MarginModeToMarginType => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::margin_mode_to_margin_type(h, args)
            }
            Self::MarginTypeToMarginMode => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::margin_type_to_margin_mode(h, args)
            }
            Self::GetMarketConfiguredMarginMode => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_market_configured_margin_mode(h, args)
            }
            Self::GetCurrentMarginModeForSymbol => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_current_margin_mode_for_symbol(h, args)
            }
            Self::GetCurrentLeverageForSymbol => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_current_leverage_for_symbol(h, args)
            }
            Self::FetchPositions => {
                let h = contexts.positions(owner);
                Ok(contexts.promise_value(exchange_positions::fetch_positions(
                    h,
                    arg(args, 0),
                    arg(args, 1),
                )))
            }
            Self::ShouldSkipOrderReconciliationFetch => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::should_skip_order_reconciliation_fetch(h, args)
            }
            Self::FetchOrders => {
                let h = contexts.markets(owner);
                Ok(contexts.promise_value(exchange_market_reads::fetch_orders(h)))
            }
            Self::FetchOrdersOnce => {
                let h = contexts.order_snapshot(owner);
                Ok(contexts.promise_value(exchange_order_snapshot::fetch_orders_once(h)))
            }
            Self::FetchHistoricalFills => {
                let h = contexts.fills(owner);
                Ok(
                    contexts.promise_value(exchange_historical_fills::fetch_historical_fills(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::FetchMarkets => {
                let h = contexts.markets(owner);
                Ok(contexts.promise_value(exchange_market_reads::fetch_markets(h)))
            }
            Self::FetchLighterMarketRows => {
                let h = contexts.markets(owner);
                Ok(contexts.promise_value(exchange_market_reads::fetch_lighter_market_rows(h)))
            }
            Self::FetchMarketsHelper => {
                let h = contexts.markets(owner);
                Ok(contexts.promise_value(exchange_market_reads::fetch_markets_helper(h)))
            }
            Self::FetchOHLCV => {
                let h = contexts.ohlcv(owner);
                Ok(contexts.promise_value(exchange_ohlcv::fetch_ohlcv(h, arg(args, 0))))
            }
            Self::GetOhlcvFallbackKey => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_ohlcv_fallback_key(h, args)
            }
            Self::FetchLatestOhlcvWithSharedFallback => {
                let h = contexts.ohlcv(owner);
                Ok(
                    contexts.promise_value(
                        exchange_ohlcv::fetch_latest_ohlcv_with_shared_fallback(
                            h,
                            arg(args, 0),
                            arg(args, 1),
                        ),
                    ),
                )
            }
            Self::ListenOHLCV => {
                let h = contexts.ohlcv(owner);
                exchange_ohlcv::listen_ohlcv(h, arg(args, 0), arg(args, 1))
            }
            Self::ListenOrderBook => {
                let h = contexts.ohlcv(owner);
                exchange_ohlcv::listen_order_book(h.as_ref(), arg(args, 0), arg(args, 1))
            }
            Self::GetPrivateWsAuthToken => {
                let h = contexts.reconciliation(owner);
                Ok(
                    contexts.promise_value(exchange_reconciliation::get_private_ws_auth_token(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::GetPrivateWsAccountIndex => {
                let h = contexts.access(owner);
                exchange_account_access::get_private_ws_account_index(h.as_ref())
            }
            Self::GetPrivateWsApiKeyIndex => {
                let h = contexts.access(owner);
                exchange_account_access::get_private_ws_api_key_index(h.as_ref())
            }
            Self::GetReduceOnlyClosingPositionSide => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_reduce_only_closing_position_side(h, args)
            }
            Self::HasOpenReduceOnlyTargetPosition => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::has_open_reduce_only_target_position(h, args)
            }
            Self::BuildNoPositionReduceOnlyMessage => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::build_no_position_reduce_only_message(h, args)
            }
            Self::IsTransientLighterRestTransportError => {
                let h = contexts.errors(owner);
                exchange_error_recovery::is_transient_lighter_rest_transport_error(
                    h.as_ref(),
                    &arg(args, 0),
                )
                .map(AccountValue::Boolean)
            }
            Self::HandlePrivateSnapshotFetchError => {
                let h = contexts.errors(owner);
                exchange_error_recovery::handle_private_snapshot_fetch_error(
                    h.as_ref(),
                    &arg(args, 0),
                )
                .map(|()| AccountValue::Undefined)
            }
            Self::ExtractOrderErrorMessage => {
                let h = contexts.errors(owner);
                exchange_error_recovery::extract_order_error_message(h.as_ref(), &arg(args, 0))
            }
            Self::IsRateLimitOrderError => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::is_rate_limit_order_error(h, args)
            }
            Self::IsInvalidMarginModeOrderError => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::is_invalid_margin_mode_order_error(h, args)
            }
            Self::IsPartnerAttributionTierOrderError => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::is_partner_attribution_tier_order_error(h, args)
            }
            Self::IsPartnerAttributionApprovalMissingOrderError => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::is_partner_attribution_approval_missing_order_error(h, args)
            }
            Self::BuildInvalidMarginModeOrderMessage => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::build_invalid_margin_mode_order_message(h, args)
            }
            Self::RefreshMarginModeStateAfterInvalidOrder => {
                let h = contexts.trading(owner);
                Ok(contexts.promise_value(
                    exchange_trading_preflight::refresh_margin_mode_state_after_invalid_order(
                        h,
                        arg(args, 0),
                    ),
                ))
            }
            Self::NormalizePlaceOrderErrorMessage => {
                let h = contexts.order_actions(owner);
                exchange_order_actions::normalize_place_order_error_message(
                    h.as_ref(),
                    arg(args, 0),
                    arg(args, 1),
                )
            }
            Self::AssertReduceOnlyLimitHasPosition => {
                let h = contexts.trading(owner);
                Ok(contexts.promise_value(
                    exchange_trading_preflight::assert_reduce_only_limit_has_position(
                        h,
                        arg(args, 0),
                    ),
                ))
            }
            Self::GetOrderMinimumViolationReasons => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_order_minimum_violation_reasons(h, args)
            }
            Self::GetPositivePriceCandidate => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::get_positive_price_candidate(h, args)
            }
            Self::ResolveMarketOrderReferencePrice => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::resolve_market_order_reference_price(h, args)
            }
            Self::ResolveStopSlippagePct => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::resolve_stop_slippage_pct(h, args)
            }
            Self::ResolveConditionalExecutionPrice => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::resolve_conditional_execution_price(h, args)
            }
            Self::PlaceOrder => {
                let h = contexts.single_placement(owner);
                Ok(contexts.promise_value(exchange_single_placement::place_order(h, arg(args, 0))))
            }
            Self::ShouldUseBatchedCreateOrders => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::should_use_batched_create_orders(h, args)
            }
            Self::PlaceOrdersSequentially => {
                let h = contexts.order_actions(owner);
                Ok(
                    contexts.promise_value(exchange_order_actions::place_orders_sequentially(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::PlaceOrders => {
                let h = contexts.batch_placement(owner);
                Ok(contexts.promise_value(exchange_batch_placement::place_orders(h, arg(args, 0))))
            }
            Self::UpdateOrder => {
                let h = contexts.order_actions(owner);
                Ok(contexts.promise_value(exchange_order_actions::update_order(h, arg(args, 0))))
            }
            Self::CancelOrders => {
                let h = contexts.order_actions(owner);
                Ok(contexts.promise_value(exchange_order_actions::cancel_orders(h, arg(args, 0))))
            }
            Self::CancelSymbolOrders => {
                let h = contexts.trading(owner);
                Ok(
                    contexts.promise_value(exchange_trading_preflight::cancel_symbol_orders(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::SnapToLeverageTier => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::snap_to_leverage_tier(h, args)
            }
            Self::SetLeverage => {
                let h = contexts.trading(owner);
                Ok(
                    contexts.promise_value(exchange_trading_preflight::set_leverage_value(
                        h,
                        arg(args, 0),
                        arg(args, 1),
                    )),
                )
            }
            Self::ChangeMarginType => {
                let h = contexts.trading(owner);
                Ok(
                    contexts.promise_value(exchange_trading_preflight::change_margin_type(
                        h,
                        arg(args, 0),
                        arg(args, 1),
                    )),
                )
            }
            Self::SyncLeverageFromExchange => {
                let h = contexts.reconciliation(owner);
                Ok(
                    contexts.promise_value(exchange_reconciliation::sync_leverage_from_exchange(
                        h,
                        arg(args, 0),
                    )),
                )
            }
            Self::ResolvePositionLeverage => {
                let h = contexts.markets(owner);
                exchange_market_reads::resolve_position_leverage(h, arg(args, 0), arg(args, 1))
            }
            Self::ResolvePositionLiquidationPrice => {
                let h = contexts.live_helpers(owner);
                exchange_live_helpers::resolve_position_liquidation_price(h, args)
            }
            Self::MapPosition => {
                let h = contexts.markets(owner);
                exchange_market_reads::map_position(h.as_ref(), arg(args, 0))
            }
            Self::MapOrder => {
                let h = contexts.order_mapping(owner);
                exchange_order_mapping::map_order(h, arg(args, 0))
            }
        }
    }
}
fn arg(args: &[AccountValue], index: usize) -> AccountValue {
    args.get(index).cloned().unwrap_or(AccountValue::Undefined)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleHelper {
    ParseMarketSymbol,
    IsPerpMarket,
    MarketLeverageMax,
    MarginFractionRatio,
    StopSlippagePct,
}
impl ModuleHelper {
    pub fn invoke(
        self,
        h: Rc<dyn LiveHelperHost>,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        match self {
            Self::ParseMarketSymbol => exchange_live_helpers::parse_lighter_market_symbol(h, args),
            Self::IsPerpMarket => exchange_live_helpers::is_lighter_perp_market(h, args),
            Self::MarketLeverageMax => {
                exchange_live_helpers::get_lighter_market_leverage_max(h, args)
            }
            Self::MarginFractionRatio => {
                exchange_live_helpers::parse_lighter_margin_fraction_ratio(h, args)
            }
            Self::StopSlippagePct => {
                exchange_live_helpers::normalize_lighter_stop_slippage_pct(h, args)
            }
        }
    }
}

pub type ClassConstructor = fn(
    &dyn ExchangeClassRealm,
    AccountValue,
    AccountValue,
    &[AccountValue],
) -> SigningResult<AccountValue>;
/// Required language operations, including exact BaseExchange super/newTarget.
pub trait ExchangeClassRealm: ExchangeClassContexts {
    fn install_module_helper(
        &self,
        name: &'static str,
        length: usize,
        helper: ModuleHelper,
    ) -> SigningResult<()>;
    /// Establish the derived constructor, prototype chain and lexical class TDZ.
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        body: ClassConstructor,
    ) -> SigningResult<AccountValue>;
    /// Non-enumerable writable configurable prototype method, dynamic this.
    fn define_prototype_method(
        &self,
        class: &AccountValue,
        body: ExchangeMethod,
    ) -> SigningResult<()>;
    /// Trace the lexical owner, retain detached calls, and collect realm cycles.
    /// Async callables return the body's existing promise directly, without
    /// adding an async wrapper or another adoption step.
    fn arrow(&self, owner: &AccountValue, body: ExchangeMethod) -> SigningResult<AccountValue>;
    /// DefineField/static public data descriptor: writable/enumerable/configurable.
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()>;
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue>;
    fn array_literal(&self, values: &[AccountValue]) -> SigningResult<AccountValue>;
    fn finish_class_binding(&self, class: &AccountValue) -> SigningResult<()>;
    /// Exact source BaseExchange constructor and super return validation; no
    /// derived fields run here. Retain original receiver/newTarget and throws.
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
}
// ts: LighterExchange
pub struct LighterExchange;
impl LighterExchange {
    pub fn define(realm: &dyn ExchangeClassRealm) -> SigningResult<AccountValue> {
        for (name, length, body) in [
            (
                "parseLighterMarketSymbol",
                1,
                ModuleHelper::ParseMarketSymbol,
            ),
            ("isLighterPerpMarket", 1, ModuleHelper::IsPerpMarket),
            (
                "getLighterMarketLeverageMax",
                1,
                ModuleHelper::MarketLeverageMax,
            ),
            (
                "parseLighterMarginFractionRatio",
                1,
                ModuleHelper::MarginFractionRatio,
            ),
            (
                "normalizeLighterStopSlippagePct",
                1,
                ModuleHelper::StopSlippagePct,
            ),
        ] {
            realm.install_module_helper(name, length, body)?;
        }
        let base = realm.binding_value("BaseExchange")?;
        let class = realm.create_derived_class(base, "LighterExchange", 3, Self::construct)?;
        realm.define_prototype_method(&class, ExchangeMethod::MapPosition)?;
        realm.define_prototype_method(&class, ExchangeMethod::MapOrder)?;
        realm.define_data(
            &class,
            "SIGNER_RETRY_COOLDOWN_MS",
            AccountValue::Number(30000.0),
        )?;
        realm.define_data(
            &class,
            "SIGNER_WARNING_LOG_COOLDOWN_MS",
            AccountValue::Number(10000.0),
        )?;
        let tiers = realm.array_literal(
            &[1.0, 2.0, 3.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0].map(AccountValue::Number),
        )?;
        realm.define_data(&class, "VALID_LEVERAGE_TIERS", tiers)?;
        realm.finish_class_binding(&class)?;
        Ok(class)
    }
    pub fn construct(
        realm: &dyn ExchangeClassRealm,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let exchange_name = arg(args, 0);
        let options = arg(args, 1);
        let store = arg(args, 2);
        let owner = realm.construct_base(
            class,
            new_target,
            &[exchange_name.clone(), options.clone(), store],
        )?;
        Self::initialize_fields(realm, &owner)?;
        let h = realm.access(owner.clone());
        crate::exchange_constructor::initialize_exchange_clients(
            h.as_ref(),
            owner,
            exchange_name,
            options,
        )
    }
    pub fn initialize_fields(
        realm: &dyn ExchangeClassRealm,
        owner: &AccountValue,
    ) -> SigningResult<()> {
        let value = AccountValue::Undefined;
        realm.define_data(owner, "lighterApiClient", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "signerAdapter", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "lighterDeploymentProfile", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "orderApi", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "candlestickApi", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "publicWebsocket", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "privateWebsocket", value)?;
        let value = realm.binding_value("GRAFANA_ENDPOINTS_EVERY")?;
        realm.define_data(owner, "grafanaEndpointsEvery", value)?;
        let value = realm.binding_value("GRAFANA_ENDPOINTS_PERIODIC")?;
        realm.define_data(owner, "grafanaEndpointsPeriodic", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "lastWebsocketPositionsUpdate", value)?;
        let value = realm.access(owner.clone()).global("Map")?.construct(&[])?;
        realm.define_data(owner, "recentlyCancelledOrders", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "accountIndex", value)?;
        let value = AccountValue::Boolean(false);
        realm.define_data(owner, "hasWarnedMissingOrderAuth", value)?;
        let value = AccountValue::Boolean(false);
        realm.define_data(owner, "hasWarnedMissingOrderAccountIndex", value)?;
        let value = AccountValue::Boolean(false);
        realm.define_data(owner, "hasWarnedInvalidOrderAuthAccount", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "signerInitErrorMessage", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "signerAdapterInitPromise", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "signerRetryBlockedUntil", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "lastSignerWarningAt", value)?;
        let value = realm.access(owner.clone()).global("Map")?.construct(&[])?;
        realm.define_data(owner, "placedOrderHints", value)?;
        let value = realm.access(owner.clone()).global("Map")?.construct(&[])?;
        realm.define_data(owner, "ohlcvFallbackFetchByKey", value)?;
        let value = realm.access(owner.clone()).global("Map")?.construct(&[])?;
        realm.define_data(owner, "ohlcvFallbackCooldownUntilByKey", value)?;
        let value = realm.access(owner.clone()).global("Map")?.construct(&[])?;
        realm.define_data(owner, "marginModeBySymbol", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "accountSnapshotFetchPromise", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "accountSnapshotFetchVersion", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "accountSnapshotCacheVersion", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "recentAccountSnapshot", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "ordersFetchPromise", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "lastOrdersFetchAt", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "accountLimits", value)?;
        let value = AccountValue::Undefined;
        realm.define_data(owner, "accountLimitsFetchPromise", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "accountLimitsNextFetchAt", value)?;
        let value = AccountValue::Number(0.0);
        realm.define_data(owner, "lastTransientSnapshotWarningAt", value)?;
        let value = AccountValue::Boolean(false);
        realm.define_data(owner, "hasWarnedAccountLimitsLookup", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ParseAccountIndex)?;
        realm.define_data(owner, "parseAccountIndex", value)?;
        let value = realm.arrow(owner, ExchangeMethod::RememberPlacedOrderHint)?;
        realm.define_data(owner, "rememberPlacedOrderHint", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ReadPlacedOrderHint)?;
        realm.define_data(owner, "readPlacedOrderHint", value)?;
        let value = realm.arrow(owner, ExchangeMethod::InferStopLossTypeFromReduceOnlyLimit)?;
        realm.define_data(owner, "inferStopLossTypeFromReduceOnlyLimit", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ExtractAccountFromResponse)?;
        realm.define_data(owner, "extractAccountFromResponse", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchParsedAccountLookup)?;
        realm.define_data(owner, "fetchParsedAccountLookup", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAccountFromL1AddressLookup)?;
        realm.define_data(owner, "fetchAccountFromL1AddressLookup", value)?;
        let value = realm.arrow(owner, ExchangeMethod::EnsureAccountIndex)?;
        realm.define_data(owner, "ensureAccountIndex", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetValidatedApiKeyIndex)?;
        realm.define_data(owner, "getValidatedApiKeyIndex", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ExtractL1AddressCandidate)?;
        realm.define_data(owner, "extractL1AddressCandidate", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAccountInfo)?;
        realm.define_data(owner, "fetchAccountInfo", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAccountSnapshot)?;
        realm.define_data(owner, "fetchAccountSnapshot", value)?;
        let value = realm.arrow(owner, ExchangeMethod::RememberRecentAccountSnapshot)?;
        realm.define_data(owner, "rememberRecentAccountSnapshot", value)?;
        let value = realm.arrow(owner, ExchangeMethod::InvalidateAccountSnapshotCache)?;
        realm.define_data(owner, "invalidateAccountSnapshotCache", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAccountLimits)?;
        realm.define_data(owner, "fetchAccountLimits", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetCachedAccountTier)?;
        realm.define_data(owner, "getCachedAccountTier", value)?;
        let value = realm.arrow(owner, ExchangeMethod::EnsurePartnerAttributionTierEligible)?;
        realm.define_data(owner, "ensurePartnerAttributionTierEligible", value)?;
        let value = realm.arrow(owner, ExchangeMethod::EnsurePartnerAttributionApproved)?;
        realm.define_data(owner, "ensurePartnerAttributionApproved", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetAccountRateLimitGuidanceMessage)?;
        realm.define_data(owner, "getAccountRateLimitGuidanceMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ChangeAccountTier)?;
        realm.define_data(owner, "changeAccountTier", value)?;
        let value = realm.arrow(owner, ExchangeMethod::WarmupSignerRuntime)?;
        realm.define_data(owner, "warmupSignerRuntime", value)?;
        let value = realm.arrow(owner, ExchangeMethod::NormalizeSignerErrorMessage)?;
        realm.define_data(owner, "normalizeSignerErrorMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::IsWasmRuntimeIssueMessage)?;
        realm.define_data(owner, "isWasmRuntimeIssueMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ShouldShortCircuitSignerRetry)?;
        realm.define_data(owner, "shouldShortCircuitSignerRetry", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetSignerRetryBlockMessage)?;
        realm.define_data(owner, "getSignerRetryBlockMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::WarnSignerUnavailable)?;
        realm.define_data(owner, "warnSignerUnavailable", value)?;
        let value = realm.arrow(owner, ExchangeMethod::EnsureSignerAdapter)?;
        realm.define_data(owner, "ensureSignerAdapter", value)?;
        let value = realm.arrow(owner, ExchangeMethod::Dispose)?;
        realm.define_data(owner, "dispose", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetAccount)?;
        realm.define_data(owner, "getAccount", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ValidateAccount)?;
        realm.define_data(owner, "validateAccount", value)?;
        let value = realm.arrow(owner, ExchangeMethod::Start)?;
        realm.define_data(owner, "start", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetInitialTickDelay)?;
        realm.define_data(owner, "getInitialTickDelay", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAndSetPrivateSnapshot)?;
        realm.define_data(owner, "fetchAndSetPrivateSnapshot", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAndSetPrivateSnapshotHelper)?;
        realm.define_data(owner, "fetchAndSetPrivateSnapshotHelper", value)?;
        let value = realm.arrow(owner, ExchangeMethod::Tick)?;
        realm.define_data(owner, "tick", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchBalance)?;
        realm.define_data(owner, "fetchBalance", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchAndSetBalance)?;
        realm.define_data(owner, "fetchAndSetBalance", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchRawBalance)?;
        realm.define_data(owner, "fetchRawBalance", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ParseMarginMode)?;
        realm.define_data(owner, "parseMarginMode", value)?;
        let value = realm.arrow(owner, ExchangeMethod::MarginModeToMarginType)?;
        realm.define_data(owner, "marginModeToMarginType", value)?;
        let value = realm.arrow(owner, ExchangeMethod::MarginTypeToMarginMode)?;
        realm.define_data(owner, "marginTypeToMarginMode", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetMarketConfiguredMarginMode)?;
        realm.define_data(owner, "getMarketConfiguredMarginMode", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetCurrentMarginModeForSymbol)?;
        realm.define_data(owner, "getCurrentMarginModeForSymbol", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetCurrentLeverageForSymbol)?;
        realm.define_data(owner, "getCurrentLeverageForSymbol", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchPositions)?;
        realm.define_data(owner, "fetchPositions", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ShouldSkipOrderReconciliationFetch)?;
        realm.define_data(owner, "shouldSkipOrderReconciliationFetch", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchOrders)?;
        realm.define_data(owner, "fetchOrders", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchOrdersOnce)?;
        realm.define_data(owner, "fetchOrdersOnce", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchHistoricalFills)?;
        realm.define_data(owner, "fetchHistoricalFills", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchMarkets)?;
        realm.define_data(owner, "fetchMarkets", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchLighterMarketRows)?;
        realm.define_data(owner, "fetchLighterMarketRows", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchMarketsHelper)?;
        realm.define_data(owner, "fetchMarketsHelper", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchOHLCV)?;
        realm.define_data(owner, "fetchOHLCV", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetOhlcvFallbackKey)?;
        realm.define_data(owner, "getOhlcvFallbackKey", value)?;
        let value = realm.arrow(owner, ExchangeMethod::FetchLatestOhlcvWithSharedFallback)?;
        realm.define_data(owner, "fetchLatestOhlcvWithSharedFallback", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ListenOHLCV)?;
        realm.define_data(owner, "listenOHLCV", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ListenOrderBook)?;
        realm.define_data(owner, "listenOrderBook", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetPrivateWsAuthToken)?;
        realm.define_data(owner, "getPrivateWsAuthToken", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetPrivateWsAccountIndex)?;
        realm.define_data(owner, "getPrivateWsAccountIndex", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetPrivateWsApiKeyIndex)?;
        realm.define_data(owner, "getPrivateWsApiKeyIndex", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetReduceOnlyClosingPositionSide)?;
        realm.define_data(owner, "getReduceOnlyClosingPositionSide", value)?;
        let value = realm.arrow(owner, ExchangeMethod::HasOpenReduceOnlyTargetPosition)?;
        realm.define_data(owner, "hasOpenReduceOnlyTargetPosition", value)?;
        let value = realm.arrow(owner, ExchangeMethod::BuildNoPositionReduceOnlyMessage)?;
        realm.define_data(owner, "buildNoPositionReduceOnlyMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::IsTransientLighterRestTransportError)?;
        realm.define_data(owner, "isTransientLighterRestTransportError", value)?;
        let value = realm.arrow(owner, ExchangeMethod::HandlePrivateSnapshotFetchError)?;
        realm.define_data(owner, "handlePrivateSnapshotFetchError", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ExtractOrderErrorMessage)?;
        realm.define_data(owner, "extractOrderErrorMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::IsRateLimitOrderError)?;
        realm.define_data(owner, "isRateLimitOrderError", value)?;
        let value = realm.arrow(owner, ExchangeMethod::IsInvalidMarginModeOrderError)?;
        realm.define_data(owner, "isInvalidMarginModeOrderError", value)?;
        let value = realm.arrow(owner, ExchangeMethod::IsPartnerAttributionTierOrderError)?;
        realm.define_data(owner, "isPartnerAttributionTierOrderError", value)?;
        let value = realm.arrow(
            owner,
            ExchangeMethod::IsPartnerAttributionApprovalMissingOrderError,
        )?;
        realm.define_data(
            owner,
            "isPartnerAttributionApprovalMissingOrderError",
            value,
        )?;
        let value = realm.arrow(owner, ExchangeMethod::BuildInvalidMarginModeOrderMessage)?;
        realm.define_data(owner, "buildInvalidMarginModeOrderMessage", value)?;
        let value = realm.arrow(
            owner,
            ExchangeMethod::RefreshMarginModeStateAfterInvalidOrder,
        )?;
        realm.define_data(owner, "refreshMarginModeStateAfterInvalidOrder", value)?;
        let value = realm.arrow(owner, ExchangeMethod::NormalizePlaceOrderErrorMessage)?;
        realm.define_data(owner, "normalizePlaceOrderErrorMessage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::AssertReduceOnlyLimitHasPosition)?;
        realm.define_data(owner, "assertReduceOnlyLimitHasPosition", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetOrderMinimumViolationReasons)?;
        realm.define_data(owner, "getOrderMinimumViolationReasons", value)?;
        let value = realm.arrow(owner, ExchangeMethod::GetPositivePriceCandidate)?;
        realm.define_data(owner, "getPositivePriceCandidate", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ResolveMarketOrderReferencePrice)?;
        realm.define_data(owner, "resolveMarketOrderReferencePrice", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ResolveStopSlippagePct)?;
        realm.define_data(owner, "resolveStopSlippagePct", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ResolveConditionalExecutionPrice)?;
        realm.define_data(owner, "resolveConditionalExecutionPrice", value)?;
        let value = realm.arrow(owner, ExchangeMethod::PlaceOrder)?;
        realm.define_data(owner, "placeOrder", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ShouldUseBatchedCreateOrders)?;
        realm.define_data(owner, "shouldUseBatchedCreateOrders", value)?;
        let value = realm.arrow(owner, ExchangeMethod::PlaceOrdersSequentially)?;
        realm.define_data(owner, "placeOrdersSequentially", value)?;
        let value = realm.arrow(owner, ExchangeMethod::PlaceOrders)?;
        realm.define_data(owner, "placeOrders", value)?;
        let value = realm.arrow(owner, ExchangeMethod::UpdateOrder)?;
        realm.define_data(owner, "updateOrder", value)?;
        let value = realm.arrow(owner, ExchangeMethod::CancelOrders)?;
        realm.define_data(owner, "cancelOrders", value)?;
        let value = realm.arrow(owner, ExchangeMethod::CancelSymbolOrders)?;
        realm.define_data(owner, "cancelSymbolOrders", value)?;
        let value = realm.arrow(owner, ExchangeMethod::SnapToLeverageTier)?;
        realm.define_data(owner, "snapToLeverageTier", value)?;
        let value = realm.arrow(owner, ExchangeMethod::SetLeverage)?;
        realm.define_data(owner, "setLeverage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ChangeMarginType)?;
        realm.define_data(owner, "changeMarginType", value)?;
        let value = realm.arrow(owner, ExchangeMethod::SyncLeverageFromExchange)?;
        realm.define_data(owner, "syncLeverageFromExchange", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ResolvePositionLeverage)?;
        realm.define_data(owner, "resolvePositionLeverage", value)?;
        let value = realm.arrow(owner, ExchangeMethod::ResolvePositionLiquidationPrice)?;
        realm.define_data(owner, "resolvePositionLiquidationPrice", value)?;
        Ok(())
    }
}
