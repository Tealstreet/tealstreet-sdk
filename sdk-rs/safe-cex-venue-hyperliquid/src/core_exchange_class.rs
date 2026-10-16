//! Native core exchange class and constructor composition. Allocation, JS
//! primitives, imported callees and promise observation are explicit bindings;
//! every installed callable enters a mapped Rust body below.
use crate::account_position_leaves;
use crate::balance_state;
use crate::cache_orders;
use crate::cancellation;
use crate::core_account_snapshots;
use crate::core_position_order_state;
use crate::core_rest_reads;
use crate::hip3_position_refresh;
use crate::lifecycle_delegates;
use crate::limit_orders;
use crate::market_bootstrap;
use crate::market_data_selectors;
use crate::order_reads;
use crate::order_request_leaves;
use crate::order_retry;
use crate::order_update;
use crate::placement_dispatch;
use crate::spot_market_bootstrap;
use crate::startup_account_resolution;
use crate::startup_lifecycle;
use crate::startup_timeout;
use crate::{
    core_exchange_leaves::{self, CoreExchangeLeafHost},
    helper_tasks::{self, HelperObserver},
    json_boundaries::JsPayloadHost,
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
    startup_timeout::StartupTimeoutHost,
    ws_ping_lifecycle::WsPingHost,
};
pub type ExchangeValue<H> = <H as WsPingHost>::Value;
pub type ExchangeError<H> = <H as WsPingHost>::Error;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreExchangeMethod {
    FetchHistoricalPositions,
    GenerateNonce,
    WithStartupTimeout,
    GetOrderBasePrice,
    ShouldUsePositionTpsl,
    BuildMarketsLowerCaseMap,
    AddToPrePlacementCache,
    MatchPrePlacement,
    GetPendingHip3Order,
    AddPendingHip3Order,
    RemovePendingHip3Order,
    RememberLimitOrderTimeInForce,
    GetPlacedLimitOrderTimeInForce,
    ForgetLimitOrderTimeInForce,
    IsOrderCancelled,
    SuppressCancelledOrder,
    UnsuppressCancelledOrder,
    HasPrivateSessionIdentity,
    IsPublicDataOnly,
    ShouldUseSpotCollateralBalance,
    ShouldRequestPortfolioMarginSpotState,
    ShouldTrackSpotHoldings,
    ResolveUserAbstractionMode,
    GetSpotCollateralMarkPrice,
    BuildUnifiedSpotCollateralSnapshot,
    GetAuthoritativeUnifiedUsdcAvailability,
    ApplyBalanceFromCurrentMode,
    ApplyCorePerpsBalanceSnapshot,
    HandleSpotClearinghouseStateUpdate,
    RequestUnifiedSpotCollateralRefresh,
    RefreshUnifiedSpotCollateralBalance,
    ValidateAccount,
    FetchBuilderFeeApproval,
    ExtractClearinghouseBalanceSummary,
    ResolveApplicationIdFromKey,
    FetchCorePerpsState,
    GetBuilderInfo,
    GetOrderErrorTelemetry,
    WarnOnMismatchedApiWallet,
    Start,
    FetchAndSetPrivateSnapshot,
    ScheduleOrderRefresh,
    Tick,
    Dispose,
    FetchFee,
    FetchMarketsHelper,
    FetchHip3Dexs,
    FetchHip3Markets,
    FetchMarketsAndTickers,
    ResolveSpotDisplayName,
    FetchSpotMarketsAndTickers,
    FetchOHLCV,
    GetSymbolConfigurationFromAssetData,
    SyncBalanceFromLivePositions,
    PostOrderAction,
    ValidateResponseAndExtractOrderIds,
    ResponseHasDeferredOrderStatus,
    GenerateFakePositionsFromMarkets,
    GetFakePositions,
    InvalidateFakePositionsCache,
    FetchPositions,
    FetchHip3Positions,
    FetchAllHip3Positions,
    RefreshHip3PositionsAsync,
    ScheduleHip3PositionRefresh,
    FetchOrdersInternal,
    FetchOrders,
    RefreshOrders,
    RefreshHip3OrdersWithRetry,
    RunRetryChainStep,
    EndRetryChain,
    ParseOpenOrders,
    FetchIncomeHistory,
    ResolveCoinToSymbol,
    FetchHistoricalOrders,
    DeriveClosedPositions,
    FetchIncomeHistoryRange,
    FetchHistoricalFills,
    FetchFillsForOrderIds,
    ListenOHLCV,
    ListenOrderBook,
    SetLeverage,
    ChangeMarginType,
    HasFullSizedPositionOppositeSide,
    CreateLimitOrderWires,
    PlaceLimitOrder,
    ResolveReduceOnly,
    SignActionOrDefer,
    PlaceOrder,
    CancelOrders,
    ScheduleCancel,
    UpdateOrder,
    PriceToPrecision,
    PlaceOrders,
}
#[derive(Clone, Debug)]
pub struct CoreExchangeCapture<V> {
    pub class: V,
    pub receiver: Option<V>,
    pub body: CoreExchangeMethod,
}
pub trait CoreExchangeClassHost:
    CoreExchangeLeafHost
    + core_rest_reads::RestReadHost
    + core_position_order_state::PositionOrderHost
    + startup_lifecycle::StartupLifecycleHost
    + market_bootstrap::MarketBootstrapHost
    + spot_market_bootstrap::SpotMarketBootstrapHost
    + hip3_position_refresh::PositionRefreshHost
    + order_reads::OrderReadHost
    + order_retry::OrderRetryHost
    + limit_orders::LimitOrderHost
    + placement_dispatch::PlacementHost
    + cancellation::CancellationHost
    + order_update::OrderUpdateHost
    + account_position_leaves::AccountPositionHost
    + balance_state::BalanceHost
    + market_data_selectors::MarketDataHost
    + order_request_leaves::OrderRequestHost
    + lifecycle_delegates::LifecycleHost
    + cache_orders::CacheOrdersHost
    + StartupTimeoutHost<Value = ExchangeValue<Self>, Error = ExchangeError<Self>>
    + JsPayloadHost<Value = ExchangeValue<Self>, Error = ExchangeError<Self>>
    + WsPingHost<Value: 'static, Error: Clone + 'static>
{
    /// Native derived class/prototype allocation, name HyperliquidExchange,
    /// length3, constructor/prototype inheritance and strict new-only calls.
    fn allocate_exchange_class(
        &mut self,
        base: &ExchangeValue<Self>,
    ) -> Result<(ExchangeValue<Self>, ExchangeValue<Self>), ExchangeError<Self>>;
    /// Native class own field arrow or prototype method/getter allocation. An
    /// arrow captures receiver; a prototype member uses the call-time receiver.
    /// Calls enter call_core_exchange with this capture; no algorithm is bound here.
    fn core_exchange_function(
        &mut self,
        capture: CoreExchangeCapture<ExchangeValue<Self>>,
        name: &str,
        length: u8,
    ) -> Result<ExchangeValue<Self>, ExchangeError<Self>>;
    fn define_exchange_method(
        &mut self,
        prototype: &ExchangeValue<Self>,
        name: &str,
        value: &ExchangeValue<Self>,
        getter: bool,
    ) -> Result<(), ExchangeError<Self>>;
    /// Native super(...args) with the defining class's current superclass and
    /// original newTarget, including a replacement receiver from the base.
    fn exchange_super(
        &mut self,
        class: &ExchangeValue<Self>,
        args: &[ExchangeValue<Self>; 3],
        target: &ExchangeValue<Self>,
    ) -> Result<ExchangeValue<Self>, ExchangeError<Self>>;
    /// Clone one immutable lexical environment with the defining class binding.
    /// Values/runtime remain shared. Every imported/global class lookup and every
    /// retained callback/task clone must observe this binding, never this.constructor.
    fn with_exchange_class(&self, class: &ExchangeValue<Self>) -> Self;
    /// Native Promise observing an already-started retained HelperObserver.
    /// Expose its settled outcome without executing its body again or applying
    /// a second Resolve(value) to an already-assimilated result. Preserve rejection
    /// identity and the retained task independently of observer lifetime.
    fn observe_exchange_task(
        &self,
        observer: HelperObserver<ExchangeValue<Self>, ExchangeError<Self>>,
    ) -> ExchangeValue<Self>;
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> ExchangeValue<H> {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn number<H: SpotAccountHost>(h: &H, n: f64) -> ExchangeValue<H> {
    SpotAccountHost::number(h, n)
}
fn call_method<H: WsPingHost>(
    h: &mut H,
    r: &ExchangeValue<H>,
    key: &str,
    a: &[ExchangeValue<H>],
) -> Result<ExchangeValue<H>, ExchangeError<H>> {
    let f = h.get(r, key)?;
    h.call(&f, r, a)
}
/// Class evaluation installs prototype members before static initializers.
pub fn create_core_exchange_class<H: CoreExchangeClassHost>(
    h: &mut H,
    base: &ExchangeValue<H>,
) -> Result<ExchangeValue<H>, ExchangeError<H>> {
    let (class, prototype) = h.allocate_exchange_class(base)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::GenerateNonce,
        },
        "generateNonce",
        0,
    )?;
    h.define_exchange_method(&prototype, "generateNonce", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::BuildMarketsLowerCaseMap,
        },
        "buildMarketsLowerCaseMap",
        1,
    )?;
    h.define_exchange_method(&prototype, "buildMarketsLowerCaseMap", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::AddToPrePlacementCache,
        },
        "addToPrePlacementCache",
        5,
    )?;
    h.define_exchange_method(&prototype, "addToPrePlacementCache", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::MatchPrePlacement,
        },
        "matchPrePlacement",
        4,
    )?;
    h.define_exchange_method(&prototype, "matchPrePlacement", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::GetPendingHip3Order,
        },
        "getPendingHip3Order",
        1,
    )?;
    h.define_exchange_method(&prototype, "getPendingHip3Order", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::AddPendingHip3Order,
        },
        "addPendingHip3Order",
        3,
    )?;
    h.define_exchange_method(&prototype, "addPendingHip3Order", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::RemovePendingHip3Order,
        },
        "removePendingHip3Order",
        1,
    )?;
    h.define_exchange_method(&prototype, "removePendingHip3Order", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::RememberLimitOrderTimeInForce,
        },
        "rememberLimitOrderTimeInForce",
        2,
    )?;
    h.define_exchange_method(&prototype, "rememberLimitOrderTimeInForce", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::GetPlacedLimitOrderTimeInForce,
        },
        "getPlacedLimitOrderTimeInForce",
        1,
    )?;
    h.define_exchange_method(&prototype, "getPlacedLimitOrderTimeInForce", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::ForgetLimitOrderTimeInForce,
        },
        "forgetLimitOrderTimeInForce",
        1,
    )?;
    h.define_exchange_method(&prototype, "forgetLimitOrderTimeInForce", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::IsOrderCancelled,
        },
        "isOrderCancelled",
        1,
    )?;
    h.define_exchange_method(&prototype, "isOrderCancelled", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::SuppressCancelledOrder,
        },
        "suppressCancelledOrder",
        1,
    )?;
    h.define_exchange_method(&prototype, "suppressCancelledOrder", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::UnsuppressCancelledOrder,
        },
        "unsuppressCancelledOrder",
        1,
    )?;
    h.define_exchange_method(&prototype, "unsuppressCancelledOrder", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::HasPrivateSessionIdentity,
        },
        "get hasPrivateSessionIdentity",
        0,
    )?;
    h.define_exchange_method(&prototype, "hasPrivateSessionIdentity", &f, true)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::IsPublicDataOnly,
        },
        "get isPublicDataOnly",
        0,
    )?;
    h.define_exchange_method(&prototype, "isPublicDataOnly", &f, true)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::GetSymbolConfigurationFromAssetData,
        },
        "_getSymbolConfigurationFromAssetData",
        1,
    )?;
    h.define_exchange_method(
        &prototype,
        "_getSymbolConfigurationFromAssetData",
        &f,
        false,
    )?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::PostOrderAction,
        },
        "postOrderAction",
        1,
    )?;
    h.define_exchange_method(&prototype, "postOrderAction", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::ValidateResponseAndExtractOrderIds,
        },
        "validateResponseAndExtractOrderIds",
        1,
    )?;
    h.define_exchange_method(&prototype, "validateResponseAndExtractOrderIds", &f, false)?;
    let f = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: None,
            body: CoreExchangeMethod::ResponseHasDeferredOrderStatus,
        },
        "responseHasDeferredOrderStatus",
        1,
    )?;
    h.define_exchange_method(&prototype, "responseHasDeferredOrderStatus", &f, false)?;
    let value = number(h, 8000.0);
    WsPingHost::define_data(h, &class, "BUILDER_FEE_APPROVAL_TIMEOUT_MS", &value)?;
    let value = number(h, 6000.0);
    WsPingHost::define_data(h, &class, "HIP3_DEX_DISCOVERY_TIMEOUT_MS", &value)?;
    let value = number(h, 8000.0);
    WsPingHost::define_data(h, &class, "HIP3_MARKETS_LOAD_TIMEOUT_MS", &value)?;
    let value = number(h, 2000.0);
    WsPingHost::define_data(h, &class, "HIP3_PER_DEX_LOAD_BUDGET_MS", &value)?;
    let value = number(h, 15000.0);
    WsPingHost::define_data(h, &class, "HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS", &value)?;
    let value = number(h, 6000.0);
    WsPingHost::define_data(h, &class, "SPOT_MARKETS_LOAD_TIMEOUT_MS", &value)?;
    let value = number(h, 6000.0);
    WsPingHost::define_data(h, &class, "USER_ROLE_RESOLVE_TIMEOUT_MS", &value)?;
    let value = number(h, 6000.0);
    WsPingHost::define_data(h, &class, "USER_ABSTRACTION_RESOLVE_TIMEOUT_MS", &value)?;
    let value = number(h, 10000.0);
    WsPingHost::define_data(h, &class, "SPOT_COLLATERAL_REFRESH_INTERVAL_MS", &value)?;
    let value = lit(h, "USDC");
    WsPingHost::define_data(h, &class, "UNIFIED_MARGIN_CURRENCY", &value)?;
    let value = SpotAccountHost::array(
        h,
        &[
            lit(h, "vntls"),
            lit(h, "xyz"),
            lit(h, "VOLMEX"),
            lit(h, "flx"),
        ],
    )?;
    WsPingHost::define_data(h, &class, "TESTNET_HIP3_WHITELIST", &value)?;
    let value = {
        let object = WsPingHost::global(h, "Object")?;
        let freeze = WsPingHost::get(h, &object, "freeze")?;
        let config = WsPingHost::object(h)?;
        let table = SpotAccountHost::imported(h, "MarginType")?;
        let cross = WsPingHost::get(h, &table, "Cross")?;
        WsPingHost::define_data(h, &config, "marginType", &cross)?;
        let table = SpotAccountHost::imported(h, "PositionMode")?;
        let oneway = WsPingHost::get(h, &table, "OneWay")?;
        WsPingHost::define_data(h, &config, "positionMode", &oneway)?;
        WsPingHost::define_data(h, &config, "leverage", &number(h, 20.))?;
        WsPingHost::call(h, &freeze, &object, &[config])?
    };
    WsPingHost::define_data(h, &class, "DEFAULT_SYMBOL_CONFIG", &value)?;
    Ok(class)
}
/// Source constructor and every ordered instance initializer. API factory and
/// socket class values are imported native callables with their own mapped bodies.
pub fn construct_core_exchange_class<H: CoreExchangeClassHost>(
    h: &mut H,
    class: &ExchangeValue<H>,
    args: &[ExchangeValue<H>; 3],
    new_target: Option<&ExchangeValue<H>>,
) -> Result<ExchangeValue<H>, ExchangeError<H>> {
    let owner = h.exchange_super(class, args, new_target.unwrap_or(class))?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchHistoricalPositions,
        },
        "fetchHistoricalPositions",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchHistoricalPositions", &value)?;
    let value = WsPingHost::undefined(h);
    WsPingHost::define_data(h, &owner, "xhr", &value)?;
    let value = WsPingHost::undefined(h);
    WsPingHost::define_data(h, &owner, "infoXhr", &value)?;
    let value = WsPingHost::undefined(h);
    WsPingHost::define_data(h, &owner, "unlimitedXHR", &value)?;
    let value = WsPingHost::undefined(h);
    WsPingHost::define_data(h, &owner, "publicWebsocket", &value)?;
    let value = WsPingHost::undefined(h);
    WsPingHost::define_data(h, &owner, "privateWebsocket", &value)?;
    let value = SpotAccountHost::imported(h, "GRAFANA_ENDPOINTS_EVERY")?;
    WsPingHost::define_data(h, &owner, "grafanaEndpointsEvery", &value)?;
    let value = SpotAccountHost::imported(h, "GRAFANA_ENDPOINTS_PERIODIC")?;
    WsPingHost::define_data(h, &owner, "grafanaEndpointsPeriodic", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "lastNonce", &value)?;
    let value = lit(h, "standard");
    WsPingHost::define_data(h, &owner, "userAbstractionMode", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "spotCollateralRefreshInFlight", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "lastSpotCollateralRefreshAt", &value)?;
    let value = SpotAccountHost::boolean(h, false);
    WsPingHost::define_data(h, &owner, "hasSpotCollateralSnapshot", &value)?;
    let value = {
        let o = WsPingHost::object(h)?;
        let balances = SpotAccountHost::array(h, &[])?;
        WsPingHost::define_data(h, &o, "balances", &balances)?;
        o
    };
    WsPingHost::define_data(h, &owner, "cachedSpotCollateralState", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(
        h,
        &owner,
        "latestAuthoritativeUnifiedBalanceSnapshot",
        &value,
    )?;
    let value = SpotAccountHost::boolean(h, false);
    WsPingHost::define_data(h, &owner, "hasAuthoritativeUnifiedSpotWsSnapshot", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "latestCorePerpsAccountValue", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "latestCorePerpsUsedMargin", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "latestCorePerpsUpnl", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::WithStartupTimeout,
        },
        "withStartupTimeout",
        4,
    )?;
    WsPingHost::define_data(h, &owner, "withStartupTimeout", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GetOrderBasePrice,
        },
        "getOrderBasePrice",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "getOrderBasePrice", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ShouldUsePositionTpsl,
        },
        "shouldUsePositionTpsl",
        3,
    )?;
    WsPingHost::define_data(h, &owner, "shouldUsePositionTpsl", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Map")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "activeAssetData", &value)?;
    let value = SpotAccountHost::array(h, &[])?;
    WsPingHost::define_data(h, &owner, "hip3DexNames", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Map")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "marketsLowerCaseMap", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "_lastOrderRefreshTime", &value)?;
    let value = number(h, 1000.0);
    WsPingHost::define_data(h, &owner, "HIP3_ORDER_REFRESH_INTERVAL_MS", &value)?;
    let value = SpotAccountHost::boolean(h, false);
    WsPingHost::define_data(h, &owner, "_hip3OrdersNeedRefresh", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Map")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "_placedLimitOrderTif", &value)?;
    let value = number(h, 500.0);
    WsPingHost::define_data(h, &owner, "PLACED_TIF_CACHE_LIMIT", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_fakePositionsCache", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_builderFeeApproved", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_builderFeeLastCheck", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Map")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "_cancelledOrders", &value)?;
    let value = number(h, 120000.0);
    WsPingHost::define_data(h, &owner, "CANCEL_SUPPRESSION_MAX_AGE_MS", &value)?;
    let value = number(h, 30000.0);
    WsPingHost::define_data(h, &owner, "CANCEL_SUPPRESSION_TRUST_WINDOW_MS", &value)?;
    let value = {
        let math = WsPingHost::global(h, "Math")?;
        let random = call_method(h, &math, "random", &[])?;
        let encoded = call_method(h, &random, "toString", &[number(h, 36.)])?;
        call_method(h, &encoded, "substr", &[number(h, 2.), number(h, 9.)])?
    };
    WsPingHost::define_data(h, &owner, "_instanceId", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ShouldUseSpotCollateralBalance,
        },
        "shouldUseSpotCollateralBalance",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "shouldUseSpotCollateralBalance", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ShouldRequestPortfolioMarginSpotState,
        },
        "shouldRequestPortfolioMarginSpotState",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "shouldRequestPortfolioMarginSpotState", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ShouldTrackSpotHoldings,
        },
        "shouldTrackSpotHoldings",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "shouldTrackSpotHoldings", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ResolveUserAbstractionMode,
        },
        "resolveUserAbstractionMode",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "resolveUserAbstractionMode", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GetSpotCollateralMarkPrice,
        },
        "getSpotCollateralMarkPrice",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "getSpotCollateralMarkPrice", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::BuildUnifiedSpotCollateralSnapshot,
        },
        "buildUnifiedSpotCollateralSnapshot",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "buildUnifiedSpotCollateralSnapshot", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GetAuthoritativeUnifiedUsdcAvailability,
        },
        "getAuthoritativeUnifiedUsdcAvailability",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "getAuthoritativeUnifiedUsdcAvailability", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ApplyBalanceFromCurrentMode,
        },
        "applyBalanceFromCurrentMode",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "applyBalanceFromCurrentMode", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ApplyCorePerpsBalanceSnapshot,
        },
        "applyCorePerpsBalanceSnapshot",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "applyCorePerpsBalanceSnapshot", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::HandleSpotClearinghouseStateUpdate,
        },
        "handleSpotClearinghouseStateUpdate",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "handleSpotClearinghouseStateUpdate", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::RequestUnifiedSpotCollateralRefresh,
        },
        "requestUnifiedSpotCollateralRefresh",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "requestUnifiedSpotCollateralRefresh", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::RefreshUnifiedSpotCollateralBalance,
        },
        "refreshUnifiedSpotCollateralBalance",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "refreshUnifiedSpotCollateralBalance", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ValidateAccount,
        },
        "validateAccount",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "validateAccount", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchBuilderFeeApproval,
        },
        "fetchBuilderFeeApproval",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchBuilderFeeApproval", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ExtractClearinghouseBalanceSummary,
        },
        "extractClearinghouseBalanceSummary",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "extractClearinghouseBalanceSummary", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ResolveApplicationIdFromKey,
        },
        "resolveApplicationIdFromKey",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "resolveApplicationIdFromKey", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchCorePerpsState,
        },
        "fetchCorePerpsState",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchCorePerpsState", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GetBuilderInfo,
        },
        "getBuilderInfo",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "getBuilderInfo", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GetOrderErrorTelemetry,
        },
        "getOrderErrorTelemetry",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "getOrderErrorTelemetry", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::WarnOnMismatchedApiWallet,
        },
        "warnOnMismatchedApiWallet",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "warnOnMismatchedApiWallet", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::Start,
        },
        "start",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "start", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchAndSetPrivateSnapshot,
        },
        "fetchAndSetPrivateSnapshot",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchAndSetPrivateSnapshot", &value)?;
    let value = number(h, 120000.0);
    WsPingHost::define_data(h, &owner, "HIP3_POSITION_REFRESH_INTERVAL_MS", &value)?;
    let value = number(h, 0.0);
    WsPingHost::define_data(h, &owner, "_lastPositionRefreshTime", &value)?;
    let value = number(h, 350.0);
    WsPingHost::define_data(h, &owner, "HIP3_POSITION_REFRESH_DEBOUNCE_MS", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_hip3PositionRefreshDebounceTimer", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Set")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "_pendingHip3PositionRefreshDexes", &value)?;
    let value = SpotAccountHost::boolean(h, false);
    WsPingHost::define_data(h, &owner, "_orderRefreshPending", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_orderRefreshDebounceTimer", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_fetchOrdersInFlight", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ScheduleOrderRefresh,
        },
        "scheduleOrderRefresh",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "scheduleOrderRefresh", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::Tick,
        },
        "tick",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "tick", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::Dispose,
        },
        "dispose",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "dispose", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchFee,
        },
        "fetchFee",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchFee", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchMarketsHelper,
        },
        "fetchMarketsHelper",
        3,
    )?;
    WsPingHost::define_data(h, &owner, "fetchMarketsHelper", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchHip3Dexs,
        },
        "fetchHip3Dexs",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchHip3Dexs", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchHip3Markets,
        },
        "fetchHip3Markets",
        3,
    )?;
    WsPingHost::define_data(h, &owner, "fetchHip3Markets", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchMarketsAndTickers,
        },
        "fetchMarketsAndTickers",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchMarketsAndTickers", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ResolveSpotDisplayName,
        },
        "resolveSpotDisplayName",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "resolveSpotDisplayName", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchSpotMarketsAndTickers,
        },
        "fetchSpotMarketsAndTickers",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchSpotMarketsAndTickers", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchOHLCV,
        },
        "fetchOHLCV",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchOHLCV", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::SyncBalanceFromLivePositions,
        },
        "syncBalanceFromLivePositions",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "syncBalanceFromLivePositions", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GenerateFakePositionsFromMarkets,
        },
        "generateFakePositionsFromMarkets",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "generateFakePositionsFromMarkets", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::GetFakePositions,
        },
        "getFakePositions",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "getFakePositions", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::InvalidateFakePositionsCache,
        },
        "invalidateFakePositionsCache",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "invalidateFakePositionsCache", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchPositions,
        },
        "fetchPositions",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchPositions", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchHip3Positions,
        },
        "fetchHip3Positions",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchHip3Positions", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchAllHip3Positions,
        },
        "fetchAllHip3Positions",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchAllHip3Positions", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::RefreshHip3PositionsAsync,
        },
        "refreshHip3PositionsAsync",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "refreshHip3PositionsAsync", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ScheduleHip3PositionRefresh,
        },
        "scheduleHip3PositionRefresh",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "scheduleHip3PositionRefresh", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchOrdersInternal,
        },
        "fetchOrdersInternal",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "fetchOrdersInternal", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchOrders,
        },
        "fetchOrders",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchOrders", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::RefreshOrders,
        },
        "refreshOrders",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "refreshOrders", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Set")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "_pendingRetryOrderIds", &value)?;
    let value = SpotAccountHost::boolean(h, false);
    WsPingHost::define_data(h, &owner, "_retryChainActive", &value)?;
    let value = SpotAccountHost::null(h);
    WsPingHost::define_data(h, &owner, "_retryChainTimer", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::RefreshHip3OrdersWithRetry,
        },
        "refreshHip3OrdersWithRetry",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "refreshHip3OrdersWithRetry", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::RunRetryChainStep,
        },
        "runRetryChainStep",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "runRetryChainStep", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::EndRetryChain,
        },
        "endRetryChain",
        0,
    )?;
    WsPingHost::define_data(h, &owner, "endRetryChain", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ParseOpenOrders,
        },
        "parseOpenOrders",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "parseOpenOrders", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchIncomeHistory,
        },
        "fetchIncomeHistory",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchIncomeHistory", &value)?;
    let value = SpotAccountHost::boolean(h, true);
    WsPingHost::define_data(h, &owner, "closedPositionsShareLedgerSource", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ResolveCoinToSymbol,
        },
        "resolveCoinToSymbol",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "resolveCoinToSymbol", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchHistoricalOrders,
        },
        "fetchHistoricalOrders",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchHistoricalOrders", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::DeriveClosedPositions,
        },
        "deriveClosedPositions",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "deriveClosedPositions", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchIncomeHistoryRange,
        },
        "fetchIncomeHistoryRange",
        3,
    )?;
    WsPingHost::define_data(h, &owner, "fetchIncomeHistoryRange", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchHistoricalFills,
        },
        "fetchHistoricalFills",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchHistoricalFills", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::FetchFillsForOrderIds,
        },
        "fetchFillsForOrderIds",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "fetchFillsForOrderIds", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ListenOHLCV,
        },
        "listenOHLCV",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "listenOHLCV", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ListenOrderBook,
        },
        "listenOrderBook",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "listenOrderBook", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::SetLeverage,
        },
        "setLeverage",
        3,
    )?;
    WsPingHost::define_data(h, &owner, "setLeverage", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ChangeMarginType,
        },
        "changeMarginType",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "changeMarginType", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::HasFullSizedPositionOppositeSide,
        },
        "_hasFullSizedPositionOppositeSide",
        3,
    )?;
    WsPingHost::define_data(h, &owner, "_hasFullSizedPositionOppositeSide", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::CreateLimitOrderWires,
        },
        "createLimitOrderWires",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "createLimitOrderWires", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::PlaceLimitOrder,
        },
        "placeLimitOrder",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "placeLimitOrder", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ResolveReduceOnly,
        },
        "resolveReduceOnly",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "resolveReduceOnly", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::SignActionOrDefer,
        },
        "signActionOrDefer",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "signActionOrDefer", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::PlaceOrder,
        },
        "placeOrder",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "placeOrder", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::CancelOrders,
        },
        "cancelOrders",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "cancelOrders", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::ScheduleCancel,
        },
        "scheduleCancel",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "scheduleCancel", &value)?;
    let value = {
        let c = WsPingHost::global(h, "Set")?;
        SpotAccountHost::construct(h, &c, &[])?
    };
    WsPingHost::define_data(h, &owner, "_deadOrders", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::UpdateOrder,
        },
        "updateOrder",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "updateOrder", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::PriceToPrecision,
        },
        "priceToPrecision",
        2,
    )?;
    WsPingHost::define_data(h, &owner, "priceToPrecision", &value)?;
    let value = h.core_exchange_function(
        CoreExchangeCapture {
            class: class.clone(),
            receiver: Some(owner.clone()),
            body: CoreExchangeMethod::PlaceOrders,
        },
        "placeOrders",
        1,
    )?;
    WsPingHost::define_data(h, &owner, "placeOrders", &value)?;
    let rate = SpotAccountHost::imported(h, "rateLimit")?;
    let factory = SpotAccountHost::imported(h, "createExchangeAPI")?;
    let api = WsPingHost::call(
        h,
        &factory,
        &WsPingHost::undefined(h),
        &[args[1].clone(), owner.clone()],
    )?;
    let config = WsPingHost::object(h)?;
    WsPingHost::define_data(h, &config, "maxRPS", &number(h, 3.))?;
    let xhr = WsPingHost::call(h, &rate, &WsPingHost::undefined(h), &[api, config])?;
    WsPingHost::set(h, &owner, "xhr", &xhr)?;
    let factory = SpotAccountHost::imported(h, "createInfoAPI")?;
    let xhr = WsPingHost::call(
        h,
        &factory,
        &WsPingHost::undefined(h),
        &[args[1].clone(), owner.clone()],
    )?;
    WsPingHost::set(h, &owner, "infoXhr", &xhr)?;
    let factory = SpotAccountHost::imported(h, "createExchangeAPI")?;
    let xhr = WsPingHost::call(
        h,
        &factory,
        &WsPingHost::undefined(h),
        &[args[1].clone(), owner.clone()],
    )?;
    WsPingHost::set(h, &owner, "unlimitedXHR", &xhr)?;
    let public = SpotAccountHost::imported(h, "HyperliquidPublicWebSocket")?;
    let socket = SpotAccountHost::construct(h, &public, &[owner.clone()])?;
    WsPingHost::set(h, &owner, "publicWebsocket", &socket)?;
    let private = SpotAccountHost::imported(h, "HyperliquidPrivateWebSocket")?;
    let socket = SpotAccountHost::construct(h, &private, &[owner.clone()])?;
    WsPingHost::set(h, &owner, "privateWebsocket", &socket)?;
    Ok(owner)
}
/// Lexical arrows and dynamic prototype methods enter the actual native bodies.
pub fn call_core_exchange<H: CoreExchangeClassHost>(
    host: &mut H,
    capture: &CoreExchangeCapture<ExchangeValue<H>>,
    receiver: &ExchangeValue<H>,
    args: &[ExchangeValue<H>],
) -> Result<ExchangeValue<H>, ExchangeError<H>> {
    let mut scope = host.with_exchange_class(&capture.class);
    let h = &mut scope;
    let receiver = capture.receiver.as_ref().unwrap_or(receiver);
    let u = WsPingHost::undefined(h);
    let arg = |index| args.get(index).cloned().unwrap_or_else(|| u.clone());
    match capture.body {
        CoreExchangeMethod::FetchHistoricalPositions => {
            let a0 = arg(0);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start_adopted(h, async move {
                core_account_snapshots::fetch_historical_positions(
                    &mut task_host,
                    &task_receiver,
                    &a0,
                )
                .await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::GenerateNonce => core_exchange_leaves::generate_nonce(h, receiver),
        CoreExchangeMethod::WithStartupTimeout => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            let a3 = arg(3);
            let promise = startup_timeout::with_startup_timeout(
                h,
                receiver.clone(),
                a0.clone(),
                a1.clone(),
                a2.clone(),
                a3.clone(),
            );
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::GetOrderBasePrice => {
            let a0 = arg(0);
            let a1 = arg(1);
            core_exchange_leaves::get_order_base_price(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::ShouldUsePositionTpsl => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            core_exchange_leaves::should_use_position_tpsl(h, receiver, &a0, &a1, &a2)
        }
        CoreExchangeMethod::BuildMarketsLowerCaseMap => {
            let a0 = arg(0);
            market_data_selectors::build_markets_lower_case_map(h, receiver, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::AddToPrePlacementCache => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            let a3 = arg(3);
            let a4 = arg(4);
            cache_orders::exchange_add_to_pre_placement_cache(h, &a0, &a1, &a2, &a3, &a4, None)?;
            Ok(u)
        }
        CoreExchangeMethod::MatchPrePlacement => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            let a3 = arg(3);
            cache_orders::exchange_match_pre_placement(h, &a0, &a1, Some(&a2), Some(&a3))
        }
        CoreExchangeMethod::GetPendingHip3Order => {
            let a0 = arg(0);
            cache_orders::exchange_get_pending_hip3_order(h, &a0)
        }
        CoreExchangeMethod::AddPendingHip3Order => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            cache_orders::exchange_add_pending_hip3_order(h, &a0, &a1, &a2)?;
            Ok(u)
        }
        CoreExchangeMethod::RemovePendingHip3Order => {
            let a0 = arg(0);
            cache_orders::exchange_remove_pending_hip3_order(h, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::RememberLimitOrderTimeInForce => {
            let a0 = arg(0);
            let a1 = arg(1);
            core_exchange_leaves::remember_limit_order_time_in_force(h, receiver, &a0, &a1)?;
            Ok(u)
        }
        CoreExchangeMethod::GetPlacedLimitOrderTimeInForce => {
            let a0 = arg(0);
            core_exchange_leaves::get_placed_limit_order_time_in_force(h, receiver, &a0)
        }
        CoreExchangeMethod::ForgetLimitOrderTimeInForce => {
            let a0 = arg(0);
            core_exchange_leaves::forget_limit_order_time_in_force(h, receiver, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::IsOrderCancelled => {
            let a0 = arg(0);
            core_exchange_leaves::is_order_cancelled(h, receiver, &a0)
        }
        CoreExchangeMethod::SuppressCancelledOrder => {
            let a0 = arg(0);
            core_exchange_leaves::suppress_cancelled_order(h, receiver, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::UnsuppressCancelledOrder => {
            let a0 = arg(0);
            core_exchange_leaves::unsuppress_cancelled_order(h, receiver, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::HasPrivateSessionIdentity => {
            core_exchange_leaves::has_private_session_identity(h, receiver)
        }
        CoreExchangeMethod::IsPublicDataOnly => {
            core_exchange_leaves::is_public_data_only(h, receiver)
        }
        CoreExchangeMethod::ShouldUseSpotCollateralBalance => {
            let value = account_position_leaves::should_use_spot_collateral_balance(h, receiver)?;
            Ok(SpotAccountHost::boolean(h, value))
        }
        CoreExchangeMethod::ShouldRequestPortfolioMarginSpotState => {
            let value =
                account_position_leaves::should_request_portfolio_margin_spot_state(h, receiver)?;
            Ok(SpotAccountHost::boolean(h, value))
        }
        CoreExchangeMethod::ShouldTrackSpotHoldings => {
            account_position_leaves::should_track_spot_holdings(h, receiver)
        }
        CoreExchangeMethod::ResolveUserAbstractionMode => {
            let promise = startup_account_resolution::resolve_user_abstraction_mode(
                h.clone(),
                receiver.clone(),
            );
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::GetSpotCollateralMarkPrice => {
            let a0 = arg(0);
            balance_state::get_spot_collateral_mark_price(h, receiver, &a0)
        }
        CoreExchangeMethod::BuildUnifiedSpotCollateralSnapshot => {
            let a0 = arg(0);
            balance_state::build_unified_spot_collateral_snapshot(h, receiver, &a0)
        }
        CoreExchangeMethod::GetAuthoritativeUnifiedUsdcAvailability => {
            let a0 = arg(0);
            balance_state::get_authoritative_unified_usdc_availability(h, &a0)
        }
        CoreExchangeMethod::ApplyBalanceFromCurrentMode => {
            balance_state::apply_balance_from_current_mode(h, receiver)?;
            Ok(u)
        }
        CoreExchangeMethod::ApplyCorePerpsBalanceSnapshot => {
            let a0 = arg(0);
            balance_state::apply_core_perps_balance_snapshot(h, receiver, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::HandleSpotClearinghouseStateUpdate => {
            let a0 = arg(0);
            let a1 = {
                let value = arg(1);
                if SpotAccountHost::is_undefined(h, &value) {
                    lit(h, "legacy")
                } else {
                    value
                }
            };
            core_account_snapshots::handle_spot_clearinghouse_state_update(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::RequestUnifiedSpotCollateralRefresh => {
            let a0 = {
                let value = arg(0);
                if SpotAccountHost::is_undefined(h, &value) {
                    SpotAccountHost::boolean(h, false)
                } else {
                    value
                }
            };
            balance_state::request_unified_spot_collateral_refresh(h, receiver, &a0)?;
            Ok(u)
        }
        CoreExchangeMethod::RefreshUnifiedSpotCollateralBalance => {
            let a0 = {
                let value = arg(0);
                if SpotAccountHost::is_undefined(h, &value) {
                    SpotAccountHost::boolean(h, false)
                } else {
                    value
                }
            };
            let promise = startup_account_resolution::refresh_unified_spot_collateral_balance(
                h.clone(),
                receiver.clone(),
                a0.clone(),
            );
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ValidateAccount => {
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::validate_account(&mut task_host, &task_receiver).await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchBuilderFeeApproval => {
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::fetch_builder_fee_approval(&mut task_host, &task_receiver)
                    .await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ExtractClearinghouseBalanceSummary => {
            let a0 = arg(0);
            market_data_selectors::extract_clearinghouse_balance_summary(h, &a0)
        }
        CoreExchangeMethod::ResolveApplicationIdFromKey => {
            let promise = startup_account_resolution::resolve_application_id_from_key(
                h.clone(),
                receiver.clone(),
            );
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchCorePerpsState => {
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::fetch_core_perps_state(&mut task_host, &task_receiver).await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::GetBuilderInfo => {
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                order_request_leaves::get_builder_info(&mut task_host, &task_receiver).await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::GetOrderErrorTelemetry => {
            let a0 = arg(0);
            let a1 = arg(1);
            order_request_leaves::get_order_error_telemetry(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::WarnOnMismatchedApiWallet => {
            let a0 = arg(0);
            core_account_snapshots::warn_on_mismatched_api_wallet(h, receiver, &a0)
        }
        CoreExchangeMethod::Start => {
            let promise = startup_lifecycle::start(h.clone(), receiver.clone());
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchAndSetPrivateSnapshot => {
            let a0 = {
                let value = arg(0);
                if SpotAccountHost::is_undefined(h, &value) {
                    number(h, 1.0)
                } else {
                    value
                }
            };
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::fetch_and_set_private_snapshot(
                    &mut task_host,
                    &task_receiver,
                    &a0,
                )
                .await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ScheduleOrderRefresh => {
            lifecycle_delegates::schedule_order_refresh(h, receiver)?;
            Ok(u)
        }
        CoreExchangeMethod::Tick => {
            let promise = startup_lifecycle::tick(h.clone(), receiver.clone());
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::Dispose => {
            lifecycle_delegates::core_dispose(h, receiver)?;
            Ok(u)
        }
        CoreExchangeMethod::FetchFee => {
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::fetch_fee(&mut task_host, &task_receiver).await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchMarketsHelper => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            core_rest_reads::fetch_markets_helper(h, receiver, &a0, &a1, &a2)
        }
        CoreExchangeMethod::FetchHip3Dexs => {
            let promise = core_rest_reads::fetch_hip3_dexs(h, receiver);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchHip3Markets => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            let promise = core_rest_reads::fetch_hip3_markets(h, receiver, &a0, &a1, &a2);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchMarketsAndTickers => {
            let promise = market_bootstrap::fetch_markets_and_tickers(h.clone(), receiver.clone());
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ResolveSpotDisplayName => {
            let a0 = arg(0);
            let a1 = arg(1);
            market_data_selectors::resolve_spot_display_name(h, &a0, &a1)
        }
        CoreExchangeMethod::FetchSpotMarketsAndTickers => {
            let promise =
                spot_market_bootstrap::fetch_spot_markets_and_tickers(h.clone(), receiver.clone());
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchOHLCV => {
            let a0 = arg(0);
            let promise = core_rest_reads::fetch_ohlcv(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::GetSymbolConfigurationFromAssetData => {
            let a0 = arg(0);
            account_position_leaves::symbol_configuration_from_asset_data(h, receiver, &a0)
        }
        CoreExchangeMethod::SyncBalanceFromLivePositions => {
            let a0 = arg(0);
            core_position_order_state::sync_balance_from_live_positions(h, receiver, &a0)
        }
        CoreExchangeMethod::PostOrderAction => {
            let a0 = arg(0);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                order_request_leaves::post_order_action(&mut task_host, &task_receiver, &a0).await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ValidateResponseAndExtractOrderIds => {
            let a0 = arg(0);
            order_request_leaves::validate_response_and_extract_order_ids(h, receiver, &a0)
        }
        CoreExchangeMethod::ResponseHasDeferredOrderStatus => {
            let a0 = arg(0);
            market_data_selectors::response_has_deferred_order_status(h, &a0)
        }
        CoreExchangeMethod::GenerateFakePositionsFromMarkets => {
            let a0 = arg(0);
            core_position_order_state::generate_fake_positions_from_markets(h, receiver, &a0)
        }
        CoreExchangeMethod::GetFakePositions => {
            account_position_leaves::get_fake_positions(h, receiver)
        }
        CoreExchangeMethod::InvalidateFakePositionsCache => {
            account_position_leaves::invalidate_fake_positions_cache(h, receiver)?;
            Ok(u)
        }
        CoreExchangeMethod::FetchPositions => {
            let a0 = arg(0);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::fetch_positions(&mut task_host, &task_receiver, &a0).await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchHip3Positions => {
            let a0 = arg(0);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_position_order_state::fetch_hip3_positions(&mut task_host, &task_receiver, &a0)
                    .await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchAllHip3Positions => {
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                core_account_snapshots::fetch_all_hip3_positions(&mut task_host, &task_receiver)
                    .await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::RefreshHip3PositionsAsync => {
            let a0 = arg(0);
            hip3_position_refresh::refresh_hip3_positions_async(h, receiver, &a0)
        }
        CoreExchangeMethod::ScheduleHip3PositionRefresh => {
            let a0 = arg(0);
            hip3_position_refresh::schedule_hip3_position_refresh(h, receiver, &a0)
        }
        CoreExchangeMethod::FetchOrdersInternal => {
            let promise = order_reads::fetch_orders_internal(h, receiver);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchOrders => {
            let a0 = arg(0);
            let promise = order_reads::fetch_orders(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::RefreshOrders => order_reads::refresh_orders(h, receiver),
        CoreExchangeMethod::RefreshHip3OrdersWithRetry => {
            let a0 = arg(0);
            order_retry::refresh_hip3_orders_with_retry(h, receiver, &a0)
        }
        CoreExchangeMethod::RunRetryChainStep => {
            let a0 = arg(0);
            order_retry::run_retry_chain_step(h, receiver, &a0)
        }
        CoreExchangeMethod::EndRetryChain => {
            lifecycle_delegates::end_retry_chain(h, receiver)?;
            Ok(u)
        }
        CoreExchangeMethod::ParseOpenOrders => {
            let a0 = arg(0);
            core_position_order_state::parse_open_orders(h, receiver, &a0)
        }
        CoreExchangeMethod::FetchIncomeHistory => {
            let a0 = arg(0);
            let promise = core_rest_reads::fetch_income_history(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ResolveCoinToSymbol => {
            let a0 = arg(0);
            market_data_selectors::resolve_coin_to_symbol(h, receiver, &a0)
        }
        CoreExchangeMethod::FetchHistoricalOrders => {
            let a0 = arg(0);
            let promise = core_rest_reads::fetch_historical_orders(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::DeriveClosedPositions => {
            let a0 = arg(0);
            let promise = core_rest_reads::derive_closed_positions(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchIncomeHistoryRange => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            let promise = core_rest_reads::fetch_income_history_range(h, receiver, &a0, &a1, &a2);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchHistoricalFills => {
            let a0 = arg(0);
            let promise = core_rest_reads::fetch_historical_fills(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::FetchFillsForOrderIds => {
            let a0 = arg(0);
            let promise = core_rest_reads::fetch_fills_for_order_ids(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ListenOHLCV => {
            let a0 = arg(0);
            let a1 = arg(1);
            lifecycle_delegates::core_listen_ohlcv(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::ListenOrderBook => {
            let a0 = arg(0);
            let a1 = arg(1);
            lifecycle_delegates::core_listen_order_book(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::SetLeverage => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                order_request_leaves::core_set_leverage(
                    &mut task_host,
                    &task_receiver,
                    &a0,
                    &a1,
                    &a2,
                )
                .await?;
                Ok(WsPingHost::undefined(&task_host))
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ChangeMarginType => {
            let a0 = arg(0);
            let a1 = arg(1);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                order_request_leaves::core_change_margin_type(
                    &mut task_host,
                    &task_receiver,
                    &a0,
                    &a1,
                )
                .await?;
                Ok(WsPingHost::undefined(&task_host))
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::HasFullSizedPositionOppositeSide => {
            let a0 = arg(0);
            let a1 = arg(1);
            let a2 = arg(2);
            account_position_leaves::has_full_sized_position_opposite_side(
                h, receiver, &a0, &a1, &a2,
            )
        }
        CoreExchangeMethod::CreateLimitOrderWires => {
            let a0 = arg(0);
            let a1 = arg(1);
            limit_orders::create_limit_order_wires(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::PlaceLimitOrder => {
            let a0 = arg(0);
            let a1 = arg(1);
            let promise = limit_orders::place_limit_order(h, receiver, &a0, &a1);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ResolveReduceOnly => {
            let a0 = arg(0);
            let a1 = arg(1);
            account_position_leaves::resolve_reduce_only(h, &a0, &a1)
        }
        CoreExchangeMethod::SignActionOrDefer => {
            let a0 = arg(0);
            let a1 = arg(1);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                order_request_leaves::sign_action_or_defer(&mut task_host, &task_receiver, &a0, &a1)
                    .await
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::PlaceOrder => {
            let a0 = arg(0);
            let promise = placement_dispatch::place_order(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::CancelOrders => {
            let a0 = arg(0);
            let promise = cancellation::cancel_orders(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::ScheduleCancel => {
            let a0 = arg(0);
            let mut task_host = h.clone();
            let task_receiver = receiver.clone();
            let promise = helper_tasks::start(h, async move {
                order_request_leaves::schedule_cancel(&mut task_host, &task_receiver, &a0).await?;
                Ok(WsPingHost::undefined(&task_host))
            });
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::UpdateOrder => {
            let a0 = arg(0);
            let promise = order_update::update_order(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
        CoreExchangeMethod::PriceToPrecision => {
            let a0 = arg(0);
            let a1 = arg(1);
            market_data_selectors::core_price_to_precision(h, receiver, &a0, &a1)
        }
        CoreExchangeMethod::PlaceOrders => {
            let a0 = arg(0);
            let promise = placement_dispatch::place_orders(h, receiver, &a0);
            Ok(h.observe_exchange_task(promise))
        }
    }
}
