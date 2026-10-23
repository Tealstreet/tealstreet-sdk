import {
  privateProxyIpsSingapore,
  privateProxyIpsTokyo,
  privateProxyIpsTokyoSmallPP,
  tokyoK8sNatGatewayIp,
} from '@tealstreet/proxy-ips-lists/src';
import { ExchangeName, MarginType, OrderStopTriggerType, PositionMode } from '@tealstreet/safe-cex/src/types';

/** Features supported by an exchange - used to skip health checks for unsupported features */
export type ExchangeFeatures = {
  /** Whether the exchange supports kline/candle data */
  kline: boolean;
  /** Whether the exchange supports orderbook data */
  orderbook: boolean;
  /** Whether the exchange supports order management */
  orders: boolean;
  /** Whether the exchange supports position tracking */
  positions: boolean;
  /** Whether the exchange supports balance queries */
  balance: boolean;
};

export type ExchangeMetadata = {
  /** Unique identifier for the exchange (e.g., 'bybit', 'binance') */
  exchangeName: ExchangeName;
  /** Path to the main exchange logo SVG/image */
  logo: string;
  /** Path to the light theme version of the exchange logo */
  lightLogo: string;
  /** Path to the dark theme version of the exchange logo */
  darkLogo: string;
  /** Path to the referral link image/banner */
  refImage: string;
  /** Whether the exchange API requires a passphrase for authentication */
  requiresPassphrase: boolean;
  /** Whether the exchange requires an application ID for API access */
  requiresApplicationId: boolean;
  /** Whether the exchange requires an API key index for API access */
  requiresApiKeyIndex: boolean;
  /** Whether the exchange doesn't require an API secret (public key only) */
  requiresNoSecret: boolean;
  /** Optional referral/affiliate link for the exchange */
  refLink?: string;
  /** Whether this exchange is enabled in production */
  enabled: boolean;
  /** Whether this exchange is enabled in development environment */
  enabledInDev: boolean;
  /** Whether this exchange is enabled in preview/staging environment */
  enableInPreview?: boolean;
  /**
   * Hide this exchange from the "add API key" picker while keeping it fully
   * runnable for accounts already saved against it.
   *
   * This is deliberately NOT `enabled: false`. Availability gates
   * (`isExchangeAvailableInCurrentEnv`) also decide whether a *saved* account
   * is allowed to register at startup, so disabling a superseded venue would
   * silently break every existing user account on it. Use this flag when a
   * venue has been folded into another one and should no longer be offered
   * for new connections.
   */
  hiddenFromNewAccountPicker?: boolean;
  /**
   * Whether this exchange is allowed to run in the React Native mobile app.
   *
   * Default-off. Mobile uses this as a saved-account availability gate, so
   * exchanges can remain enabled on web while imported/synced mobile accounts
   * for unsupported venues are kept inactive.
   */
  supportedOnMobile: boolean;
  /**
   * Whether this exchange serves spot markets alongside derivatives from one
   * connection, and therefore should offer a per-account "also trade spot"
   * toggle in the add-account form (`ExchangeAccountConfig.enableSpotMarkets`).
   */
  supportsSpotToggle?: boolean;
  /**
   * Whether this exchange trades options in the same account as its derivatives, and
   * therefore offers a per-account "also trade options" toggle
   * (`ExchangeAccountConfig.enableOptionMarkets`).
   */
  supportsOptionsToggle?: boolean;
  /** Whether every market on this exchange is an option, so options are on without a toggle (`binanceoptions`). */
  optionsOnly?: boolean;
  /**
   * Whether this exchange trades inverse contracts in the same account as its linear derivatives, and therefore
   * offers a per-account "also trade inverse" toggle (`ExchangeAccountConfig.enableInverseMarkets`).
   */
  supportsInverseToggle?: boolean;
  /** Whether the exchange supports testnet/paper trading */
  supportsTestnet: boolean;
  /** Predefined leverage values for quick selection in the UI */
  leverageMagnets: number[];
  /** Delay in milliseconds between orderbook data updates */
  orderbookTickDelay: number;
  /** Human-readable display name for the exchange */
  prettyName: string;
  /** TradingView chart intraday time intervals supported (e.g., '1', '5', '15') */
  tvIntraDayMultipliers: string[];
  /** Whether TradingView should enable daily candles (1D) */
  tvHasDaily?: boolean;
  /** Whether TradingView should enable weekly and monthly candles (1W, 1M) */
  tvHasWeeklyAndMonthly?: boolean;
  /** Maximum number of candle/bar fetch requests allowed for TradingView charts */
  tvMaxGetBarFetches: number;
  /** Maximum number of candles that can be fetched in a single API call */
  candleApiCountLimit: number;
  /** Default quote currency for the exchange (e.g., 'USDT', 'USD', 'USDC') */
  defaultQuote: string;
  /** Supported position modes (OneWay, Hedged) */
  positionModes: PositionMode[];
  /** Supported margin types (Cross, Isolated) */
  marginTypes: MarginType[];
  /** Whether the leverage slider should be disabled in the UI */
  leverageSliderDisabled: boolean;
  /**
   * Whether hedged positions can have independent (per-side) leverage in
   * isolated margin mode. Most exchanges that support per-side hedged
   * leverage at all support it here.
   */
  hasIndependentHedgedLeverageInIsolatedMargin: boolean;
  /**
   * Whether hedged positions can have independent (per-side) leverage in
   * cross margin mode. Some exchanges (e.g. Bitget) couple long/short
   * leverage in cross because the margin pool is shared per symbol —
   * those leave this flag false.
   */
  hasIndependentHedgedLeverageInCrossMargin: boolean;
  /** Whether the exchange supports trailing stop loss orders */
  supportsTrailingStopLoss: boolean;
  /**
   * Whether the exchange supports non-reduce-only trigger entries, surfaced in
   * the UI as Conditional orders. This is intentionally narrower than generic
   * exchange "conditional order" support: TP/SL and trailing stops are covered
   * by the stop-specific capability flags below.
   */
  supportsOpeningTriggerOrders: boolean;
  /** Supported trigger types for take profit/stop loss orders */
  supportedTPSLTriggerTypes: OrderStopTriggerType[];
  /** List of proxy IP addresses for this exchange */
  privateProxyIps: string[];
  /** Default trigger type for take profit orders */
  defaultTPTriggerType: OrderStopTriggerType | undefined;
  /** Default trigger type for stop loss orders */
  defaultSLTriggerType: OrderStopTriggerType | undefined;
  /** Delay in milliseconds before updating pending order status */
  orderPendingUpdateDelay: number;
  /** Delay in milliseconds before updating pending stop order status */
  stopOrderPendingUpdateDelay: number;
  /** Whether CORS is required for API calls to this exchange */
  requiresCors: boolean;
  /** Whether CORS should be explicitly forbidden for this exchange */
  forbidCors: boolean;
  /**
   * Interval in milliseconds for periodic REST data updates (balance, positions).
   * Required: each exchange must set its own cadence. Read by exchanges via
   * `BaseExchange.getTickInterval()`; `options.extra.tickInterval` is the
   * explicit override (web wrapper's browser slowdown, dev knobs).
   */
  tickInterval: number;
  /** Whether the exchange returns empty position objects (vs null/undefined) */
  hasEmptyPositions: boolean;
  /** Whether position mode setting applies globally across all symbols */
  positionModeIsGlobal: boolean;
  /** Whether margin type setting applies globally across all symbols */
  marginTypeIsGlobal: boolean;
  /** Whether leverage setting applies globally across all symbols */
  leverageIsGlobal: boolean;
  /** Threshold in milliseconds before showing a disconnect warning */
  disconnectWarningThreshold: number;
  /** Whether the exchange API provides fee rate information */
  supportsFeeRates: boolean;
  /** Delay in seconds after placing a market order before allowing another */
  marketOrderDelayInSeconds: number;
  /** Mapping of region codes to proxy IP addresses */
  privateProxyRegions: { [key: string]: string[] };
  /** Whether to automatically reduce order size if it exceeds position size */
  autoDownSizeReduceOrders: boolean;
  /** Whether the exchange supports chaser orders (orders that follow price) */
  supportsChaser: boolean;
  /** Whether the exchange exposes a per-position margin amount (Position.marginSize) */
  supportsPositionMargin: boolean;
  /** Whether the exchange has an endpoint for closed/realized PnL data */
  supportsClosedPnLEndpoint: boolean;
  /** Whether the exchange supports partial stop orders (stop loss on part of position) */
  supportsPartialStops: boolean;
  /** Whether the exchange supports infinity/full position stop orders */
  supportsInfinityStops: boolean;
  /** Whether the exchange supports maximum cross leverage setting */
  supportsMaxCrossLeverage: boolean;
  /** Whether the exchange supports tunnel/proxy connections */
  supportsTunnel?: boolean;
  /** Whether the exchange supports WebSocket routing */
  supportsWsRoute?: boolean;
  /** Whether the exchange supports vault/treasury functionality */
  supportsVault?: boolean;
  /** Whether the exchange requires an OmniKey seed for authentication */
  requiresOmniKeySeed?: boolean;
  /** Whether the exchange requires Paradex address and Ethereum address (Paradex) */
  requiresParadexAddress?: boolean;
  /** Whether the exchange supports subaccounts */
  supportsSubaccount?: boolean;
  /** Whether the exchange supports fetching maximum leverage per symbol */
  supportsFetchSymbolMaxLeverage?: boolean;
  /** Whether the exchange uses unified margin (false if separate wallets per currency) */
  unifiedMargin?: boolean;
  /** Whether the exchange supports second-resolution kline/candle data */
  supportsKlineSeconds?: boolean;
  /** Default trading symbol to display when first loading the exchange */
  defaultSymbol?: string;
  /** Default symbol to use specifically for TradingView chart widget (overrides global BTCUSDT default) */
  defaultTradingViewSymbol?: string;
  /** Whether the exchange supports read-only API keys */
  supportsReadOnlyKey: boolean;
  /** Tick interval in milliseconds for secondary API key operations */
  secondaryApiKeyTickInterval?: number;
  /** Whether to always calculate unrealized PnL from last price (vs mark price) */
  alwaysCalculateUnrealizedPnlFromLastPrice?: boolean;
  /** Whether position mode changes must be done on the exchange website (not via API) */
  forceAccountPositionModeChangeOnExchangeItself?: boolean;
  /** Whether to hide the margin type selector in the UI */
  hideMarginTypeSelector?: boolean;
  /** Whether to hide the position mode selector in the UI */
  hidePositionModeSelector?: boolean;
  /** Whether to hide lightweight chart popovers/tooltips */
  hideLightweightChartsPopovers?: boolean;
  /** Whether TP/SL orders support the 'close' attribute for position closing */
  supportsTPSLCloseAttribute?: boolean;
  /** Number of seconds to wait before resetting chart when no kline callbacks detected */
  klineCallbackResetThreshold?: number;
  /** Whether to fetch leverage when the active margin symbol changes */
  fetchLeverageOnMarginSymbolChange?: boolean;
  /** Whether to enforce WebSocket tunnel for this exchange */
  enforceWsTunnel?: boolean;
  /** Whether open interest data is not available from this exchange */
  doesNotSupportOpenInterest: boolean;
  /** Domain to use for rpc.latency measurement (must be in tradetunnel ALLOWED_URLS) */
  latencyDomain?: string;
  /** Whether the exchange requires tickers to be loaded before markets are considered loaded */
  requiresTickersForMarketsLoaded?: boolean;
  /** Features supported by this exchange - used to skip health checks for unsupported features */
  features: ExchangeFeatures;
  /**
   * Whether fetchOHLCV must run in the worker/server (true) or can run directly
   * from the browser (false, default). Set to true for exchanges whose listenOHLCV
   * calls fetchOHLCV internally (e.g., BitMEX polling, Coinbase REST from WS,
   * Bitrue WS klines, Lighter WS OHLCV, Paradex REST polling).
   */
  fetchOHLCVInWorker?: boolean;
  /** Whether this exchange has been verified to work in public-only mode (no API keys).
   *  When true, createPublicExchange() is guaranteed to work correctly.
   *  Exchanges must be explicitly verified and opted in. */
  supportsPublicOnly: boolean;
};

const defaultMetaData: Omit<ExchangeMetadata, 'exchangeName' | 'features'> = {
  logo: '',
  lightLogo: '',
  darkLogo: '',
  refImage: '',
  requiresPassphrase: false,
  requiresApplicationId: false,
  requiresApiKeyIndex: false,
  enabled: false,
  enabledInDev: true,
  enableInPreview: true,
  supportedOnMobile: false,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: '',
  leverageMagnets: [1, 10, 20, 50, 100, 125, 150],
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60', '240'],
  tvMaxGetBarFetches: 2,
  candleApiCountLimit: 500,
  defaultQuote: 'USDT',
  positionModes: [PositionMode.OneWay, PositionMode.Hedged],
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  hasIndependentHedgedLeverageInCrossMargin: false,
  supportsTrailingStopLoss: false,
  supportsOpeningTriggerOrders: false,
  privateProxyIps: privateProxyIpsSingapore,
  supportedTPSLTriggerTypes: [],
  defaultTPTriggerType: undefined,
  defaultSLTriggerType: undefined,
  orderPendingUpdateDelay: 250,
  stopOrderPendingUpdateDelay: 250,
  requiresCors: false,
  hasEmptyPositions: true,
  klineCallbackResetThreshold: 20,
  positionModeIsGlobal: false,
  marginTypeIsGlobal: false,
  requiresNoSecret: false,
  supportsVault: false,
  leverageIsGlobal: false,
  forbidCors: false,
  disconnectWarningThreshold: 20 * 1000,
  supportsFeeRates: false,
  marketOrderDelayInSeconds: 2,
  privateProxyRegions: {},
  autoDownSizeReduceOrders: false,
  // Safe default. Most exchanges override (BitMEX = 20_000, kraken = 500, etc.).
  tickInterval: 2_000,
  supportsChaser: false,
  supportsPositionMargin: false,
  supportsClosedPnLEndpoint: false,
  supportsPartialStops: true,
  supportsInfinityStops: true,
  supportsMaxCrossLeverage: true,
  supportsTunnel: false,
  supportsWsRoute: false,
  requiresOmniKeySeed: false,
  leverageSliderDisabled: false,
  supportsFetchSymbolMaxLeverage: false,
  unifiedMargin: true,
  supportsReadOnlyKey: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: false,
  forceAccountPositionModeChangeOnExchangeItself: false,
  hideMarginTypeSelector: false,
  hidePositionModeSelector: false,
  hideLightweightChartsPopovers: false,
  supportsTPSLCloseAttribute: false,
  fetchLeverageOnMarginSymbolChange: false,
  enforceWsTunnel: false,
  doesNotSupportOpenInterest: false,
  supportsKlineSeconds: true,
  fetchOHLCVInWorker: false,
  supportsPublicOnly: false,
};

const BybitMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'bybit' as ExchangeName,
  supportsOptionsToggle: true,
  supportsInverseToggle: true,
  logo: '/static/exchanges/logos/BYBIT/logo.svg',
  lightLogo: '/static/exchanges/logos/BYBIT/light.png',
  darkLogo: '/static/exchanges/logos/BYBIT/dark.png',
  refLink: 'https://teal.link/bybit1',
  refImage: '/static/exchanges/reflinks/bybit/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Bybit',
  supportsTrailingStopLoss: true,
  supportsOpeningTriggerOrders: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  hasEmptyPositions: false,
  // sort of global, we dont pass symbol so it tries to set as many as possible so we need to do full fetches on update
  positionModeIsGlobal: true,
  // UTA margin mode is account-wide via /v5/account/set-margin-mode.
  marginTypeIsGlobal: true,
  tickInterval: 500,
  supportsFeeRates: true,
  privateProxyIps: privateProxyIpsSingapore,
  privateProxyRegions: {
    SG: privateProxyIpsSingapore,
    JA: privateProxyIpsTokyo,
  },
  supportsChaser: true,
  supportsPositionMargin: true,
  supportsTunnel: true,
  tvMaxGetBarFetches: 20,
  supportsClosedPnLEndpoint: true,
  supportsKlineSeconds: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  supportsTPSLCloseAttribute: false,
  latencyDomain: 'api.bybit.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BybitDemoMetadata: ExchangeMetadata = {
  ...BybitMetadata,
  exchangeName: 'bybitdemo' as ExchangeName,
  // Not confirmed that the demo environment lists options.
  supportsOptionsToggle: false,
  supportsInverseToggle: false,
  prettyName: 'Bybit Demo',
  supportedOnMobile: true,
  supportsTestnet: false,
  forbidCors: true,
  enforceWsTunnel: true,
};

const HyrotraderMetadata: ExchangeMetadata = {
  ...BybitDemoMetadata,
  supportsKlineSeconds: false,
  positionModes: [PositionMode.OneWay],
  hidePositionModeSelector: true,
  marginTypes: [MarginType.Cross],
  hideMarginTypeSelector: true,
  supportsFeeRates: false,
  supportsPositionMargin: false,
  exchangeName: 'hyrotrader' as ExchangeName,
  // Prop-firm white-label of Bybit, not certified on mobile yet: the spread
  // above would otherwise inherit Bybit's `true`.
  supportedOnMobile: false,
  prettyName: 'HyroTrader',
  logo: '/static/exchanges/logos/HYROTRADER/logo.svg',
  lightLogo: '/static/exchanges/logos/HYROTRADER/logo.svg',
  darkLogo: '/static/exchanges/logos/HYROTRADER/logo.svg',
  refLink: undefined,
  refImage: '/static/exchanges/logos/HYROTRADER/full-white.svg',
  enabled: false,
  enabledInDev: true,
  // Keep preview/prod dark until the private REST/WS path has either Hyro HTTPS
  // origins or a Tealstreet relay. Local dev can still test the simulator
  // directly over plain HTTP.
  enableInPreview: false,
  forbidCors: false,
  enforceWsTunnel: false,
  supportsTunnel: false,
  privateProxyIps: [],
  privateProxyRegions: {},
  latencyDomain: 'hyrotrader-simulator',
};

const BybitSpotMetadata: ExchangeMetadata = {
  ...BybitMetadata,
  // Options trade on the `bybit` connection.
  supportsOptionsToggle: false,
  supportsInverseToggle: false,
  exchangeName: 'bybitspot' as ExchangeName,
  prettyName: 'Bybit Spot',
  supportedOnMobile: true,
  supportsTestnet: false,
  // Spot has no position-based closed PnL. This flag is a SOURCE SWITCH for the
  // Closed Positions tab, not a generic capability advertisement.
  supportsClosedPnLEndpoint: false,
  supportsOpeningTriggerOrders: false,
  forbidCors: true,
  enabled: true,
  enabledInDev: true,
  marginTypes: [MarginType.Isolated],
  positionModes: [PositionMode.OneWay],
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
};

const BybitUnifiedMetadata: ExchangeMetadata = {
  ...BybitMetadata,
  exchangeName: 'bybit' as ExchangeName,
  marginTypes: [MarginType.Cross],
};

const BybitInverseMetadata: ExchangeMetadata = {
  ...BybitMetadata,
  // Options trade on the `bybit` connection.
  supportsOptionsToggle: false,
  supportsInverseToggle: false,
  exchangeName: 'bybitinverse' as ExchangeName,
  prettyName: 'Bybit Inverse',
  supportedOnMobile: true,
  logo: '/static/exchanges/logos/BYBITINVERSE/logo.svg',
  lightLogo: '/static/exchanges/logos/BYBITINVERSE/light.png',
  darkLogo: '/static/exchanges/logos/BYBITINVERSE/dark.png',
  refImage: '/static/exchanges/reflinks/bybitinverse/ref.png',
  requiresTickersForMarketsLoaded: true,
  supportsOpeningTriggerOrders: true,
};

const BinanceMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'binance' as ExchangeName,
  logo: '/static/exchanges/logos/BINANCE/logo.svg',
  lightLogo: '/static/exchanges/logos/BINANCE/light.png',
  darkLogo: '/static/exchanges/logos/BINANCE/dark.png',
  refLink: 'https://teal.link/Binance1',
  refImage: '/static/exchanges/reflinks/binance/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: true,
  orderbookTickDelay: 60,
  prettyName: 'Binance',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: true,
  supportsPositionMargin: true,
  tickInterval: 500,
  supportsTunnel: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  supportsTPSLCloseAttribute: true,
  doesNotSupportOpenInterest: true,
  latencyDomain: 'fapi.binance.com',
  // supportsReadOnlyKey: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BinanceInverseMetadata: ExchangeMetadata = {
  ...BinanceMetadata,
  exchangeName: 'binanceinverse' as ExchangeName,
  prettyName: 'Binance Inverse',
  defaultQuote: 'USD',
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportedOnMobile: true,
  latencyDomain: 'dapi.binance.com',
  requiresCors: true,
  privateProxyRegions: {
    JA: privateProxyIpsTokyo,
  },
  supportsTestnet: false,
  requiresTickersForMarketsLoaded: true,
};

const BinancePMMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'binancepm' as ExchangeName,
  logo: '/static/exchanges/logos/BINANCE/logo.svg',
  lightLogo: '/static/exchanges/logos/BINANCE/light.png',
  darkLogo: '/static/exchanges/logos/BINANCE/dark.png',
  refLink: 'https://teal.link/Binance1',
  refImage: '/static/exchanges/reflinks/binance/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: true,
  // No fetchHistoricalFills/fetchHistoricalTrades implementation in the PM adapter.
  orderbookTickDelay: 60,
  prettyName: 'Binance Portfolio Margin',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: true,
  supportsPositionMargin: false,
  tickInterval: 500,
  supportsTunnel: true,
  marginTypes: [MarginType.Cross],
  // supportsReadOnlyKey: true,
  hideMarginTypeSelector: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  doesNotSupportOpenInterest: true,
  latencyDomain: 'papi.binance.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

// REST only and not offered to any app: no websockets, and no private call has run against a live key.
const BinanceOptionsMetadata: ExchangeMetadata = {
  ...BinanceMetadata,
  supportsPublicOnly: false,
  exchangeName: 'binanceoptions' as ExchangeName,
  prettyName: 'Binance Options',
  enabled: false,
  enabledInDev: false,
  enableInPreview: false,
  supportedOnMobile: false,
  supportsTestnet: true,
  optionsOnly: true,
  supportsOptionsToggle: false,
  latencyDomain: 'eapi.binance.com',
  // Polls everything: /eapi/v1/ticker alone weighs 40 of the 400-a-minute budget.
  tickInterval: 10_000,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Cross],
  leverageMagnets: [1],
  leverageSliderDisabled: true,
  hidePositionModeSelector: true,
  hideMarginTypeSelector: true,
  supportsTrailingStopLoss: false,
  supportedTPSLTriggerTypes: [],
  defaultTPTriggerType: undefined,
  defaultSLTriggerType: undefined,
  supportsChaser: false,
  supportsPositionMargin: false,
  supportsPartialStops: false,
  supportsInfinityStops: false,
  supportsTunnel: false,
  supportsTPSLCloseAttribute: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: false,
  features: {
    kline: true,
    orderbook: false,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BitunixMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'bitunix' as ExchangeName,
  logo: '/static/exchanges/logos/BITUNIX/logo.svg',
  lightLogo: '/static/exchanges/logos/BITUNIX/light.png',
  darkLogo: '/static/exchanges/logos/BITUNIX/dark.png',
  refLink: 'https://teal.link/bitunix',
  refImage: '/static/exchanges/reflinks/bitunix/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Bitunix',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: true,
  supportsPositionMargin: false,
  tickInterval: 500,
  tvMaxGetBarFetches: 10,
  supportsReadOnlyKey: false,
  supportsWsRoute: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  forceAccountPositionModeChangeOnExchangeItself: true,
  hideLightweightChartsPopovers: true,
  enforceWsTunnel: true,
  autoDownSizeReduceOrders: true,
  latencyDomain: 'fapi.bitunix.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const WeexMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'weex' as ExchangeName,
  logo: '/static/exchanges/logos/WEEX/logo.svg',
  lightLogo: '/static/exchanges/logos/WEEX/light.svg',
  darkLogo: '/static/exchanges/logos/WEEX/dark.svg',
  refLink: 'https://www.weex.com',
  refImage: '',
  requiresPassphrase: true,
  requiresApplicationId: false,
  enabled: false,
  enabledInDev: true,
  enableInPreview: false,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'WEEX',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: false,
  supportsOpeningTriggerOrders: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: true,
  supportsPositionMargin: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  hasIndependentHedgedLeverageInCrossMargin: true,
  autoDownSizeReduceOrders: true,
  tickInterval: 2_000,
  tvMaxGetBarFetches: 2,
  candleApiCountLimit: 1000,
  leverageMagnets: [1, 5, 10, 20, 50, 100, 200, 400],
  positionModeIsGlobal: false,
  marginTypeIsGlobal: false,
  leverageIsGlobal: false,
  supportsFeeRates: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  doesNotSupportOpenInterest: true,
  latencyDomain: 'api-contract.weex.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const MexcMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'mexc' as ExchangeName,
  logo: '/static/exchanges/logos/MEXC/logo.svg',
  lightLogo: '/static/exchanges/logos/MEXC/logo.svg',
  darkLogo: '/static/exchanges/logos/MEXC/logo.svg',
  refLink: 'https://teal.link/mexc',
  refImage: '',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'MEXC',
  requiresCors: true,
  privateProxyIps: privateProxyIpsTokyo,
  privateProxyRegions: {
    JA: privateProxyIpsTokyo,
  },
  supportsTrailingStopLoss: false,
  supportsPartialStops: true,
  supportsInfinityStops: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: true,
  supportsPositionMargin: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  hasIndependentHedgedLeverageInCrossMargin: true,
  autoDownSizeReduceOrders: true,
  tickInterval: 2_000,
  tvMaxGetBarFetches: 2,
  candleApiCountLimit: 2000,
  leverageMagnets: [1, 5, 10, 20, 50, 100, 200, 500],
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  leverageIsGlobal: false,
  supportsFeeRates: true,
  fetchLeverageOnMarginSymbolChange: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  doesNotSupportOpenInterest: false,
  latencyDomain: 'api.mexc.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BinanceSpotMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'binancespot' as ExchangeName,
  logo: '/static/exchanges/logos/BINANCE/logo.svg',
  lightLogo: '/static/exchanges/logos/BINANCE/light.png',
  darkLogo: '/static/exchanges/logos/BINANCE/dark.png',
  refLink: 'https://teal.link/Binance1',
  refImage: '/static/exchanges/reflinks/binance/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: true,
  // No fetchHistoricalFills/fetchHistoricalTrades implementation in the Spot adapter.
  orderbookTickDelay: 60,
  privateProxyIps: privateProxyIpsTokyo,
  supportsChaser: true,
  supportsPositionMargin: false,
  supportsInfinityStops: false,
  prettyName: 'Binance Spot',
  doesNotSupportOpenInterest: true,
  latencyDomain: 'api.binance.com',
  tickInterval: 500,
  // supportsReadOnlyKey: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BackpackMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'backpack' as ExchangeName,
  logo: '/static/exchanges/logos/BACKPACK/logo.svg',
  lightLogo: '/static/exchanges/logos/BACKPACK/light.png',
  darkLogo: '/static/exchanges/logos/BACKPACK/dark.png',
  refLink: 'https://teal.link/backpack',
  refImage: '/static/exchanges/reflinks/backpack/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  requiresNoSecret: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,
  orderbookTickDelay: 100,
  prettyName: 'Backpack',
  defaultQuote: 'USDC',
  defaultSymbol: 'BTC_USDC_PERP',
  positionModes: [PositionMode.Hedged],
  marginTypes: [MarginType.Cross],
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  leverageMagnets: [1, 2, 3, 5, 10, 15, 20, 25, 50, 100],
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60'],
  tvMaxGetBarFetches: 10,
  candleApiCountLimit: 500,
  privateProxyIps: privateProxyIpsTokyo,
  tickInterval: 2000,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  requiresCors: true,
  forbidCors: false,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  leverageIsGlobal: true, // Backpack sets leverage at account level via leverageLimit
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  supportsTrailingStopLoss: false,
  supportsChaser: false,
  supportsPositionMargin: false,
  supportsClosedPnLEndpoint: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  supportsMaxCrossLeverage: true,
  supportsTunnel: false,
  supportsWsRoute: false,
  leverageSliderDisabled: false,
  supportsFeeRates: false,
  supportsReadOnlyKey: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: false,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BitgetMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'bitget' as ExchangeName,
  logo: '/static/exchanges/logos/BITGET/logo.svg',
  lightLogo: '/static/exchanges/logos/BITGET/light.png',
  darkLogo: '/static/exchanges/logos/BITGET/dark.png',
  refLink: 'https://teal.link/Bitget',
  refImage: '/static/exchanges/reflinks/bitget/ref.png',
  requiresPassphrase: true,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: false,
  supportsTrailingStopLoss: false,
  orderbookTickDelay: 60,
  prettyName: 'Bitget',
  requiresCors: true,
  privateProxyIps: privateProxyIpsTokyo,
  hasEmptyPositions: false,
  marginTypeIsGlobal: false,
  tickInterval: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  // sort of. it is per contract type, so we say global to force refetch of all positions
  positionModeIsGlobal: true,
  tvMaxGetBarFetches: 5,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  autoDownSizeReduceOrders: true,
  supportsChaser: true,
  supportsPositionMargin: true,
  supportsTunnel: true,
  // No `supportsWsRoute`: bitget never reads `tealstreetWsRoute`. Its public
  // routing is `fallbackOnly` (the two public ws classes flip `useFallbackUrl`
  // themselves) and its private routing follows `tealstreetProxy`
  // (`proxyConditional` + relay). Advertising it rendered a checkbox that saved
  // a value nothing consumed. Contrast bitunix,
  // which initialises `useFallbackUrl` from the flag, and lighter, whose
  // routing is `wsRouteCandidates` — the flag is real for both.
  supportedTPSLTriggerTypes: [OrderStopTriggerType.MarkPrice],
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  latencyDomain: 'api.bitget.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BitgetUSDCMetadata: ExchangeMetadata = {
  ...BitgetMetadata,
  supportsPublicOnly: false,
  exchangeName: 'bitgetusdc' as ExchangeName,
  // Hidden from the picker and not certified on mobile; the Bitget spread
  // above would otherwise inherit `true`.
  supportedOnMobile: false,
  logo: '/static/exchanges/logos/BITGETUSDC/logo.svg',
  lightLogo: '/static/exchanges/logos/BITGETUSDC/light.png',
  darkLogo: '/static/exchanges/logos/BITGETUSDC/dark.png',
  refImage: '/static/exchanges/reflinks/bitgetusdc/ref.png',
  prettyName: 'Bitget USDC',
  defaultQuote: 'USDC',
};

const BitgetInverseMetadata: ExchangeMetadata = {
  ...BitgetMetadata,
  supportsPublicOnly: false,
  exchangeName: 'bitgetinverse' as ExchangeName,
  // Hidden from the picker and not certified on mobile; the Bitget spread
  // above would otherwise inherit `true`.
  supportedOnMobile: false,
  logo: '/static/exchanges/logos/BITGET/logo.svg',
  lightLogo: '/static/exchanges/logos/BITGET/light.png',
  darkLogo: '/static/exchanges/logos/BITGET/dark.png',
  refImage: '/static/exchanges/reflinks/bitget/ref.png',
  prettyName: 'Bitget Inverse',
  enabled: false,
  enableInPreview: false,
  requiresTickersForMarketsLoaded: true,
};

const CryptocomMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'cryptocom' as ExchangeName,
  logo: '/static/exchanges/logos/CRYPTOCOM/logo.svg',
  lightLogo: '/static/exchanges/logos/CRYPTOCOM/light.png',
  darkLogo: '/static/exchanges/logos/CRYPTOCOM/dark.png',
  refLink: 'https://teal.link/cryptocom',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: true,
  orderbookTickDelay: 100,
  prettyName: 'Crypto.com',
  defaultQuote: 'USD',
  positionModes: [PositionMode.Hedged],
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  leverageMagnets: [1, 2, 5, 10, 20, 50, 100, 125, 150],
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60', '240'],
  tvMaxGetBarFetches: 10,
  candleApiCountLimit: 500,
  privateProxyIps: privateProxyIpsTokyo,
  privateProxyRegions: {
    JA: privateProxyIpsTokyo,
    SG: privateProxyIpsSingapore,
  },
  tickInterval: 2000,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  requiresCors: true,
  forbidCors: false,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  leverageIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  supportsTrailingStopLoss: false,
  supportsChaser: false,
  supportsPositionMargin: false,
  supportsClosedPnLEndpoint: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  supportsMaxCrossLeverage: false,
  supportsTunnel: false,
  supportsWsRoute: false,
  leverageSliderDisabled: false,
  supportsFeeRates: false,
  supportsReadOnlyKey: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: false,
  // Isolated positions require per-order isolation_id/ISOLATED_MARGIN wiring;
  // the legacy change-margin-type endpoint no longer exists in Exchange v1.
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BingxMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'bingx' as ExchangeName,
  logo: '/static/exchanges/logos/BINGX/logo.svg',
  lightLogo: '/static/exchanges/logos/BINGX/light.png',
  darkLogo: '/static/exchanges/logos/BINGX/dark.png',
  klineCallbackResetThreshold: 60,
  refLink: 'https://teal.link/bingx20',
  refImage: '/static/exchanges/reflinks/bingx/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'BingX',
  privateProxyIps: privateProxyIpsSingapore,
  requiresCors: true,
  orderPendingUpdateDelay: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  tickInterval: 2_000,
  tvMaxGetBarFetches: 20,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  hasIndependentHedgedLeverageInCrossMargin: true,
  supportsTunnel: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: false,
  supportsPositionMargin: true,
  supportsPartialStops: true,
  supportsTrailingStopLoss: true,
  supportsInfinityStops: true,
  enforceWsTunnel: true,
  latencyDomain: 'open-api-swap.bingx.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BitrueMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'bitrue' as ExchangeName,
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60'],
  // TODO: audit below....
  logo: '/static/exchanges/logos/BITRUE/logo.svg',
  lightLogo: '/static/exchanges/logos/BITRUE/light.png',
  darkLogo: '/static/exchanges/logos/BITRUE/dark.png',
  refLink: 'https://teal.link/bitrue',
  refImage: '/static/exchanges/reflinks/bitrue/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  // Removed from supported exchanges (hidden from add-account everywhere). Code
  // is kept in place; flip these back to re-enable. Same soft-disable pattern as
  // dydx/arkham/salsadex — gated by isExchangeAvailableInCurrentEnv.
  enabled: false,
  enabledInDev: false,
  enableInPreview: false,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Bitrue',
  privateProxyIps: privateProxyIpsTokyo,
  requiresCors: true,
  orderPendingUpdateDelay: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  tickInterval: 2_000,
  positionModes: [PositionMode.OneWay],
  // sort of, we fake it all client side but it acts as if it does
  hasEmptyPositions: false,
  marginTypeIsGlobal: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: false,
  supportsPositionMargin: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  // Bitrue does not auto-cap reduce-only orders to the open position server-side,
  // so downsize them client-side via the shared `downSizeReduceOrder` util.
  autoDownSizeReduceOrders: true,
  supportsFetchSymbolMaxLeverage: true,
  fetchLeverageOnMarginSymbolChange: true,
  enforceWsTunnel: true,
  latencyDomain: 'fapi.bitrue.com',
  fetchOHLCVInWorker: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BreakoutpropMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60'],
  // TODO: audit below....
  exchangeName: 'breakoutprop' as ExchangeName,
  logo: '/static/exchanges/logos/BREAKOUT_PROP/logo.svg',
  lightLogo: '/static/exchanges/logos/BREAKOUT_PROP/light.png',
  darkLogo: '/static/exchanges/logos/BREAKOUT_PROP/dark.png',
  refLink: 'https://teal.link/breakout',
  refImage: '/static/exchanges/reflinks/breakoutprop/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: false,
  enableInPreview: false,
  enabledInDev: true,
  supportsTestnet: false,
  orderbookTickDelay: 100,
  prettyName: 'Breakout',
  privateProxyIps: privateProxyIpsSingapore,
  orderPendingUpdateDelay: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  tickInterval: 2_000,
  tvMaxGetBarFetches: 20,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  hasIndependentHedgedLeverageInCrossMargin: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.LastPrice,
  supportsChaser: false,
  supportsPositionMargin: false,
  requiresCors: false,
  supportsMaxCrossLeverage: false,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Cross],
  supportsSubaccount: true,
  enforceWsTunnel: true,
  latencyDomain: 'app.breakoutprop.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const BlofinMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  supportsOpeningTriggerOrders: true,
  exchangeName: 'blofin' as ExchangeName,
  logo: '/static/exchanges/logos/BLOFIN/logo-light.svg',
  lightLogo: '/static/exchanges/logos/BLOFIN/light.png',
  darkLogo: '/static/exchanges/logos/BLOFIN/dark.png',
  refLink: 'https://teal.link/blofin',
  refImage: '/static/exchanges/reflinks/blofin/ref.svg',
  requiresPassphrase: true,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: false,
  orderbookTickDelay: 250,
  prettyName: 'BloFin',
  privateProxyIps: privateProxyIpsSingapore,
  requiresCors: true,
  tickInterval: 10_000,
  hasEmptyPositions: false,
  supportsTunnel: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: true,
  tvMaxGetBarFetches: 20,
  supportsChaser: true,
  supportsPositionMargin: false,
  supportsPartialStops: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  fetchLeverageOnMarginSymbolChange: true,
  latencyDomain: 'openapi.blofin.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const OkxMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  marginTypes: [MarginType.Cross],
  exchangeName: 'okx' as ExchangeName,
  logo: '/static/exchanges/logos/OKEX/logo.svg',
  lightLogo: '/static/exchanges/logos/OKEX/light.png',
  klineCallbackResetThreshold: 60,
  darkLogo: '/static/exchanges/logos/OKEX/dark.png',
  refLink: 'https://teal.link/OKX',
  refImage: '/static/exchanges/reflinks/okx/ref.png',
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  requiresPassphrase: true,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: false,
  orderbookTickDelay: 250,
  prettyName: 'OKX',
  tvMaxGetBarFetches: 5,
  tickInterval: 2_000,
  privateProxyIps: privateProxyIpsTokyo,
  supportsChaser: true,
  supportsPositionMargin: false,
  supportsTrailingStopLoss: false,
  supportsOpeningTriggerOrders: true,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  unifiedMargin: false,
  latencyDomain: 'www.okx.com',
  requiresTickersForMarketsLoaded: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const OkxEUMetadata: ExchangeMetadata = {
  ...OkxMetadata,
  exchangeName: 'okxeu' as ExchangeName,
  prettyName: 'OKX EU',
  supportedOnMobile: false,
  requiresCors: true,
  latencyDomain: 'eea.okx.com',
};

const OkxDemoMetadata: ExchangeMetadata = {
  ...OkxMetadata,
  exchangeName: 'okxdemo' as ExchangeName,
  prettyName: 'OKX Demo',
  supportedOnMobile: false,
  supportsTestnet: false,
  // OKX Demo REST keys are third-party linked through Tealstreet routing; keep
  // REST on the existing OKX proxy path while leaving demo WebSockets direct.
  requiresCors: true,
  forbidCors: false,
  latencyDomain: 'openapi.okx.com',
  // OKX Demo uses direct `wspap.okx.com` sockets and authenticates with a
  // post-connect login frame, so it must not inherit Bybit Demo's WS tunnel flag.
  enforceWsTunnel: false,
};

const OkxEUDemoMetadata: ExchangeMetadata = {
  ...OkxEUMetadata,
  exchangeName: 'okxeudemo' as ExchangeName,
  prettyName: 'OKX EU Demo',
  supportedOnMobile: false,
  supportsTestnet: false,
  // EEA demo REST uses the live EEA host with `x-simulated-trading: 1`, and
  // browser REST still needs the existing OKX EU proxy path.
  requiresCors: true,
  forbidCors: false,
  latencyDomain: 'eea.okx.com',
  // EEA demo WebSockets authenticate with the standard OKX login frame against
  // direct `wseeapap.okx.com` sockets, not the Tealstreet WS proxy.
  enforceWsTunnel: false,
};

const WooMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'woo' as ExchangeName,
  logo: '/static/exchanges/logos/WOO/logo.svg',
  lightLogo: '/static/exchanges/logos/WOO/light.png',
  darkLogo: '/static/exchanges/logos/WOO/dark.png',
  refLink: 'https://teal.link/woo',
  refImage: '/static/exchanges/reflinks/woo/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50],
  orderbookTickDelay: 60,
  prettyName: 'WOO X',
  marginTypes: [MarginType.Cross],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: true,
  supportsTunnel: true,
  tickInterval: 2_000,
  leverageIsGlobal: false,
  supportsChaser: true,
  supportsPositionMargin: false,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  supportsPartialStops: true,
  // WOO does not auto-cap reduce-only orders to the open position server-side,
  // so downsize them client-side via the shared `downSizeReduceOrder` util.
  autoDownSizeReduceOrders: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  // supportsReadOnlyKey: true,
  secondaryApiKeyTickInterval: 1000,
  supportsKlineSeconds: true,
  tvMaxGetBarFetches: 10,
  latencyDomain: 'api.woo.network',
  requiresTickersForMarketsLoaded: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const WooFiMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'woofi' as ExchangeName,
  logo: '/static/exchanges/logos/WOOFI/logo.svg',
  lightLogo: '/static/exchanges/logos/WOOFI/light.png',
  darkLogo: '/static/exchanges/logos/WOOFI/dark.png',
  refLink: 'https://teal.link/WOOFi',
  refImage: '/static/exchanges/reflinks/woofi/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50, 100],
  orderbookTickDelay: 60,
  prettyName: 'WOOFi',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: true,
  supportsFetchSymbolMaxLeverage: true,
  hideMarginTypeSelector: false,
  hidePositionModeSelector: true,
  latencyDomain: 'api-evm.orderly.org',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const KodiakFIMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'kodiakfi' as ExchangeName,
  logo: '/static/exchanges/logos/KODIAKFI/logo.svg',
  lightLogo: '/static/exchanges/logos/KODIAKFI/logo.webp',
  darkLogo: '/static/exchanges/logos/KODIAKFI/logo.webp',
  refLink: 'https://teal.link/kodiak',
  refImage: '/static/exchanges/reflinks/kodiakfi/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50, 100],
  orderbookTickDelay: 60,
  prettyName: 'Kodiak',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: true,
  supportsFetchSymbolMaxLeverage: true,
  hideMarginTypeSelector: false,
  hidePositionModeSelector: true,
  latencyDomain: 'api-evm.orderly.org',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const MODEMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'mode' as ExchangeName,
  logo: '/static/exchanges/logos/MODE/logo.svg',
  lightLogo: '/static/exchanges/logos/MODE/light.png',
  darkLogo: '/static/exchanges/logos/MODE/dark.png',
  refLink: 'https://teal.link/mode',
  refImage: '/static/exchanges/reflinks/mode/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50, 100],
  orderbookTickDelay: 60,
  prettyName: 'Mode',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: true,
  supportsFetchSymbolMaxLeverage: true,
  hideMarginTypeSelector: false,
  hidePositionModeSelector: true,
  latencyDomain: 'api-evm.orderly.org',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const SalsaDEXMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'salsadex' as ExchangeName,
  logo: '/static/exchanges/logos/SALSADEX/logo.webp',
  lightLogo: '/static/exchanges/logos/SALSADEX/logo-big.webp',
  darkLogo: '/static/exchanges/logos/SALSADEX/logo-big.webp',
  refLink: 'https://teal.link/salsadex',
  refImage: '/static/exchanges/reflinks/salsadex/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  // Removed from supported exchanges (hidden from add-account everywhere). Code
  // is kept in place; flip these back to re-enable. Same soft-disable pattern as
  // dydx/arkham — gated by isExchangeAvailableInCurrentEnv.
  enabled: false,
  enabledInDev: false,
  enableInPreview: false,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50, 100],
  orderbookTickDelay: 60,
  prettyName: 'SalsaDEX',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: true,
  supportsFetchSymbolMaxLeverage: true,
  hideMarginTypeSelector: false,
  hidePositionModeSelector: true,
  latencyDomain: 'api-evm.orderly.org',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const WhatMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'what' as ExchangeName,
  logo: '/static/exchanges/logos/WHAT/logo.svg',
  lightLogo: '/static/exchanges/logos/WHAT/light.png',
  darkLogo: '/static/exchanges/logos/WHAT/dark.png',
  refLink: 'https://teal.link/what',
  refImage: '/static/exchanges/reflinks/what/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50, 100],
  orderbookTickDelay: 60,
  prettyName: 'What',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: true,
  supportsFetchSymbolMaxLeverage: true,
  hideMarginTypeSelector: false,
  hidePositionModeSelector: true,
  latencyDomain: 'api-evm.orderly.org',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const TealstreetDEXMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'tealstreet-dex' as ExchangeName,
  // No longer offered for new connections. Hidden rather than disabled: the
  // availability flags also gate whether a *saved* account may register at
  // startup, so `enabled: false` would stop any existing TSDEX account from
  // loading — including the owner's ability to open it and export or delete it.
  hiddenFromNewAccountPicker: true,
  logo: '/static/exchanges/logos/TSDEX/logo.svg',
  lightLogo: '/static/exchanges/logos/TSDEX/light.png',
  darkLogo: '/static/exchanges/logos/TSDEX/dark.png',
  refLink: 'https://teal.link/tsdex',
  refImage: '/static/exchanges/reflinks/tsdex/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50, 100],
  orderbookTickDelay: 60,
  prettyName: 'TSDEX',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: true,
  supportsFetchSymbolMaxLeverage: true,
  latencyDomain: 'api-evm.orderly.org',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const HyperliquidMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'hyperliquid' as ExchangeName,
  // One connection serves perps + spot; spot is opt-in per account.
  supportsSpotToggle: true,
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  positionModes: [PositionMode.OneWay],
  hidePositionModeSelector: true,
  logo: '/static/exchanges/logos/HYPERLIQUID/logo.svg',
  lightLogo: '/static/exchanges/logos/HYPERLIQUID/light.png',
  klineCallbackResetThreshold: 60,
  darkLogo: '/static/exchanges/logos/HYPERLIQUID/dark.png',
  refLink: 'https://teal.link/hyperliquid',
  refImage: '/static/exchanges/reflinks/hyperliquid/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  defaultSymbol: 'BTC-PERP',
  enabledInDev: true,
  supportedOnMobile: true,
  supportsTestnet: true,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50],
  orderbookTickDelay: 60,
  prettyName: 'Hyperliquid',
  requiresNoSecret: false,
  defaultQuote: 'USDC',
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: true,
  // Hyperliquid rests a reduce-only order at whatever size it was sent, so the
  // client has to cap it to the open position itself.
  autoDownSizeReduceOrders: true,
  forbidCors: true,
  supportsVault: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  enforceWsTunnel: true,
  latencyDomain: 'api.hyperliquid.xyz',
  fetchLeverageOnMarginSymbolChange: false, // Using bulk activeAssetData subscriptions instead
  requiresTickersForMarketsLoaded: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const HyperliquidSpotMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'hyperliquidspot' as ExchangeName,
  // Superseded by the `hyperliquid` connection's spot toggle. Kept enabled so
  // existing saved accounts keep loading; hidden from the picker so nobody
  // creates a new split account.
  hiddenFromNewAccountPicker: true,
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  logo: '/static/exchanges/logos/HYPERLIQUIDSPOT/logo.svg',
  lightLogo: '/static/exchanges/logos/HYPERLIQUIDSPOT/light.png',
  darkLogo: '/static/exchanges/logos/HYPERLIQUIDSPOT/dark.png',
  refLink: 'https://teal.link/hyperliquid',
  refImage: '/static/exchanges/reflinks/hyperliquidspot/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50],
  orderbookTickDelay: 60,
  prettyName: 'Hyperliquid Spot',
  requiresNoSecret: false,
  defaultQuote: 'USDC',
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  tickInterval: 2_000,
  supportsChaser: false,
  supportsPositionMargin: false,
  forbidCors: true,
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  enforceWsTunnel: true,
  latencyDomain: 'api.hyperliquid.xyz',
  requiresTickersForMarketsLoaded: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const GrvtMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'grvt' as ExchangeName,
  autoDownSizeReduceOrders: true,
  logo: '/static/exchanges/logos/GRVT/logo.svg',
  lightLogo: '/static/exchanges/logos/GRVT/logo-wide.svg',
  darkLogo: '/static/exchanges/logos/GRVT/logo-wide.svg',
  refLink: 'https://teal.link/grvt',
  refImage: '/static/exchanges/reflinks/grvt/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: true,
  leverageMagnets: [1, 5, 10, 20, 50, 100],
  orderbookTickDelay: 100,
  prettyName: 'Grvt',
  defaultSymbol: 'BTC_USDT_Perp',
  requiresNoSecret: false,
  defaultQuote: 'USDT',
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  supportsTrailingStopLoss: false,
  tickInterval: 2000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Cross],
  supportsSubaccount: false,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: true,
  leverageIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  supportsMaxCrossLeverage: true,
  supportsTunnel: true,
  supportsWsRoute: false,
  leverageSliderDisabled: false,
  supportsFeeRates: false,
  supportsReadOnlyKey: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  tvMaxGetBarFetches: 10,
  candleApiCountLimit: 500,
  requiresCors: true,
  enforceWsTunnel: true,
  // Private WS is event-driven (orders/positions/fills only); long silence
  // is normal when not actively trading. Keep warning threshold aligned with
  // the relaxed idle reconnect window in grvt.ws-private.ts (90s).
  disconnectWarningThreshold: 60 * 1000,
  latencyDomain: 'trades.grvt.io',
  privateProxyIps: [...privateProxyIpsTokyoSmallPP, tokyoK8sNatGatewayIp],
  fetchLeverageOnMarginSymbolChange: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const TxFlowMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  exchangeName: 'txflow' as ExchangeName,
  prettyName: 'TxFlow',
  logo: '/static/exchanges/logos/TXFLOW/logo.svg',
  lightLogo: '/static/exchanges/logos/TXFLOW/light.png',
  darkLogo: '/static/exchanges/logos/TXFLOW/dark.png',
  refImage: '',
  requiresApplicationId: true,
  enabled: false,
  enabledInDev: true,
  enableInPreview: false,
  supportsTestnet: true,
  defaultQuote: 'USDC',
  supportsPartialStops: false,
  supportsInfinityStops: false,
  features: {
    kline: false,
    orderbook: false,
    orders: false,
    positions: false,
    balance: false,
  },
};

const ApexOmniMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'apexomni' as ExchangeName,
  logo: '/static/exchanges/logos/APEX/logo.svg',
  lightLogo: '/static/exchanges/logos/APEX/light.png',
  darkLogo: '/static/exchanges/logos/APEX/dark.png',
  klineCallbackResetThreshold: 60,
  refLink: 'https://teal.link/apex',
  refImage: '/static/exchanges/reflinks/apex/ref.png',
  requiresPassphrase: true,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  leverageMagnets: [1, 2, 3, 4, 5, 10, 15, 20, 50],
  orderbookTickDelay: 60,
  prettyName: 'ApeX Omni',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross],
  positionModes: [PositionMode.Hedged],
  requiresCors: true,
  orderPendingUpdateDelay: 3_000,
  supportsTrailingStopLoss: false,
  supportsTunnel: true,
  tickInterval: 2_000,
  supportsChaser: true,
  supportsPositionMargin: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  forbidCors: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.IndexPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.IndexPrice,
  requiresOmniKeySeed: true,
  tvMaxGetBarFetches: 5,
  enforceWsTunnel: true,
  latencyDomain: 'omni.apex.exchange',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const PhemexMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'phemex' as ExchangeName,
  logo: '/static/exchanges/logos/PHEMEX/logo.svg',
  lightLogo: '/static/exchanges/logos/PHEMEX/light.png',
  darkLogo: '/static/exchanges/logos/PHEMEX/dark.png',
  refLink: 'https://teal.link/Phemex',
  refImage: '/static/exchanges/reflinks/phemex/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Phemex',
  supportsTunnel: true,
  requiresCors: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  tickInterval: 2_000,
  tvMaxGetBarFetches: 20,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  hasIndependentHedgedLeverageInCrossMargin: true,
  hasEmptyPositions: false,
  supportsTrailingStopLoss: false,
  supportsChaser: true,
  supportsPositionMargin: false,
  latencyDomain: 'api.phemex.com',
  requiresTickersForMarketsLoaded: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const CoinbaseMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'coinbase' as ExchangeName,
  logo: '/static/exchanges/logos/COINBASE/logo.svg',
  lightLogo: '/static/exchanges/logos/COINBASE/light.png',
  darkLogo: '/static/exchanges/logos/COINBASE/dark.png',
  refLink: 'https://teal.link/Coinbase',
  refImage: '/static/exchanges/reflinks/coinbase/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  supportsTestnet: false,
  requiresCors: true,
  orderbookTickDelay: 60,
  prettyName: 'Coinbase',
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60', '120', '360'],
  tvMaxGetBarFetches: 20,
  defaultQuote: 'USD',
  tickInterval: 1_000,
  supportsChaser: false,
  supportsPositionMargin: false,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  autoDownSizeReduceOrders: true,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Cross],
  leverageSliderDisabled: true,
  // not really but we fake it anyways...
  hasEmptyPositions: true,
  defaultSymbol: 'BTC-USD',
  latencyDomain: 'api.international.coinbase.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const CoinbaseSpotMetadata: ExchangeMetadata = {
  ...CoinbaseMetadata,
  supportsPublicOnly: false,
  exchangeName: 'coinbasespot' as ExchangeName,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  prettyName: 'Coinbase Spot',
};

const KucoinMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'kucoin' as ExchangeName,
  logo: '/static/exchanges/logos/KUCOIN/teal.svg',
  lightLogo: '/static/exchanges/logos/KUCOIN/light1.svg',
  darkLogo: '/static/exchanges/logos/KUCOIN/teal.svg',
  refLink: 'https://teal.link/Kucoin',
  refImage: '/static/exchanges/reflinks/kucoin/ref.png',
  requiresPassphrase: true,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Kucoin',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  supportsChaser: true,
  supportsPositionMargin: false,
  tickInterval: 500,
  tvMaxGetBarFetches: 10,
  supportsReadOnlyKey: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  forceAccountPositionModeChangeOnExchangeItself: true,
  hideLightweightChartsPopovers: true,
  enforceWsTunnel: true,
  latencyDomain: 'api-futures.kucoin.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const KucoinSpotMetadata: ExchangeMetadata = {
  ...KucoinMetadata,
  exchangeName: 'kucoinspot' as ExchangeName,
  prettyName: 'Kucoin Spot',
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,
  supportsTrailingStopLoss: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.LastPrice,
  supportsChaser: false,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Isolated],
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: false,
  forceAccountPositionModeChangeOnExchangeItself: false,
  doesNotSupportOpenInterest: true,
  latencyDomain: 'api.kucoin.com',
};

const KrakenMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'kraken' as ExchangeName,
  logo: '/static/exchanges/logos/KRAKEN/logo.svg',
  lightLogo: '/static/exchanges/logos/KRAKEN/light.png',
  darkLogo: '/static/exchanges/logos/KRAKEN/dark.png',
  refLink: 'teal.link/kraken',
  refImage: '/static/exchanges/reflinks/kraken/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportedOnMobile: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Kraken Futures',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  positionModes: [PositionMode.OneWay], // Kraken only supports one-way mode
  marginTypes: [MarginType.Cross, MarginType.Isolated], // Kraken supports both cross and isolated margin
  supportsMaxCrossLeverage: false, // Kraken doesn't support setting max leverage in cross margin mode
  tickInterval: 500,
  tvMaxGetBarFetches: 10,
  supportsReadOnlyKey: false,
  // TP/SL are placed as separate orders, not attached to main order
  supportsPartialStops: true, // Kraken allows stops for partial position sizes
  supportsInfinityStops: false,
  supportsTPSLCloseAttribute: false,
  enforceWsTunnel: true,
  latencyDomain: 'futures.kraken.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const KrakenSpotMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'krakenspot' as ExchangeName,
  logo: '/static/exchanges/logos/KRAKEN/logo.svg',
  lightLogo: '/static/exchanges/logos/KRAKEN/light.png',
  darkLogo: '/static/exchanges/logos/KRAKEN/dark.png',
  refLink: 'https://www.kraken.com',
  refImage: '/static/exchanges/reflinks/kraken/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,
  orderbookTickDelay: 60,
  prettyName: 'Kraken Spot',
  privateProxyIps: privateProxyIpsTokyo,
  supportsTrailingStopLoss: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.LastPrice,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Isolated],
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  supportsMaxCrossLeverage: false,
  tickInterval: 500,
  tvMaxGetBarFetches: 10,
  supportsReadOnlyKey: false,
  supportsPartialStops: false,
  supportsInfinityStops: false,
  supportsTPSLCloseAttribute: false,
  enforceWsTunnel: true,
  latencyDomain: 'api.kraken.com',
  doesNotSupportOpenInterest: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const ParadexMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'paradex' as ExchangeName,
  logo: '/static/exchanges/logos/PARADEX/logo.svg',
  lightLogo: '/static/exchanges/logos/PARADEX/logo-white.svg',
  darkLogo: '/static/exchanges/logos/PARADEX/logo-dark.svg',
  refLink: 'https://teal.link/paradex',
  refImage: '/static/exchanges/reflinks/paradex/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  requiresParadexAddress: true,
  requiresNoSecret: true,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: true,
  orderbookTickDelay: 100,
  prettyName: 'Paradex',
  privateProxyIps: privateProxyIpsSingapore,
  supportsTrailingStopLoss: false,
  supportsOpeningTriggerOrders: true,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.MarkPrice, OrderStopTriggerType.LastPrice],
  defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  positionModes: [PositionMode.OneWay], // Paradex only supports one-way mode
  marginTypes: [MarginType.Cross], // Paradex supports cross margin
  tickInterval: 500,
  tvMaxGetBarFetches: 10,
  supportsReadOnlyKey: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  // Paradex is a decentralized exchange on Starknet
  supportsPartialStops: true,
  supportsInfinityStops: false, // Paradex requires explicit TP/SL sizes
  supportsTPSLCloseAttribute: false,
  leverageSliderDisabled: false,
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  leverageMagnets: [1, 5, 10, 20, 50, 100],
  defaultQuote: 'USD',
  defaultTradingViewSymbol: 'BTC-USD-PERP', // Paradex uses hyphenated format
  candleApiCountLimit: 2000,
  // Paradex TradingView endpoint: /v1/tradingview/history
  // Supported resolutions: [1 5 15 30 60 120 240 1D]
  // Note: 3m and 1W are NOT supported - TradingView will rebuild them
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60', '120', '240'],
  tvHasDaily: true, // Paradex TradingView endpoint supports daily candles (1D)
  tvHasWeeklyAndMonthly: false, // Weekly/monthly NOT supported by Paradex API
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: true,
  leverageIsGlobal: false,
  disconnectWarningThreshold: 5000,
  supportsFeeRates: false,
  marketOrderDelayInSeconds: 0,
  privateProxyRegions: {},
  autoDownSizeReduceOrders: false,
  supportsChaser: true,
  supportsPositionMargin: false,
  supportsClosedPnLEndpoint: false,
  requiresCors: false,
  forbidCors: false,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  enforceWsTunnel: true,
  latencyDomain: 'api.prod.paradex.trade',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const AsterDEXMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'asterdex' as ExchangeName,
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  positionModes: [PositionMode.OneWay, PositionMode.Hedged],
  logo: '/static/exchanges/logos/ASTERDEX/logo.svg',
  lightLogo: '/static/exchanges/logos/ASTERDEX/light.png',
  darkLogo: '/static/exchanges/logos/ASTERDEX/dark.png',
  refLink: 'https://teal.link/aster5',
  refImage: '/static/exchanges/reflinks/asterdex/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: true,
  leverageMagnets: [1, 2, 5, 10, 20, 50, 100, 125],
  orderbookTickDelay: 60,
  prettyName: 'Aster',
  requiresNoSecret: false,
  privateProxyIps: privateProxyIpsTokyo,
  defaultQuote: 'USDT',
  orderPendingUpdateDelay: 250,
  stopOrderPendingUpdateDelay: 250,
  // PlaceOrderOpts has no unambiguous callback-rate input. Do not advertise a
  // TRAILING_STOP_MARKET path until it can satisfy Aster's mandatory field.
  supportsTrailingStopLoss: false,
  tickInterval: 1000,
  supportsChaser: true,
  supportsPositionMargin: false,
  forbidCors: false,
  supportsVault: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  leverageIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  supportsInfinityStops: true,
  supportsFeeRates: true,
  tvMaxGetBarFetches: 2,
  candleApiCountLimit: 500,
  klineCallbackResetThreshold: 20,
  disconnectWarningThreshold: 20 * 1000,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.MarkPrice, OrderStopTriggerType.LastPrice],
  defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  enforceWsTunnel: true,
  latencyDomain: 'fapi.asterdex.com',
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const PolymarketMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'polymarket' as ExchangeName,
  logo: '/static/exchanges/logos/POLYMARKET/logo.svg',
  lightLogo: '/static/exchanges/logos/POLYMARKET/light.svg',
  darkLogo: '/static/exchanges/logos/POLYMARKET/dark.svg',
  refLink: 'https://polymarket.com',
  refImage: '',
  requiresPassphrase: false,
  requiresApplicationId: false,
  requiresApiKeyIndex: false,
  requiresNoSecret: true, // Uses Ethereum wallet private key instead
  enabled: false, // Disabled in production
  enabledInDev: true,
  enableInPreview: false, // Disabled in preview
  supportsTestnet: false, // Polymarket only has mainnet
  // Upstream has a trades endpoint, but this adapter does not map it yet.
  orderbookTickDelay: 100,
  prettyName: 'Polymarket',
  defaultQuote: 'pUSD',
  marginTypes: [MarginType.Cross], // No margin in prediction markets
  positionModes: [PositionMode.OneWay], // No hedging in prediction markets
  requiresCors: true, // Polygon CLOB API
  forbidCors: false,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  supportsTrailingStopLoss: false, // Not supported
  tickInterval: 2000,
  supportsChaser: false, // Prediction markets work differently
  supportsPositionMargin: false,
  supportsPartialStops: false,
  supportsInfinityStops: false,
  leverageMagnets: [1], // No leverage in prediction markets
  leverageSliderDisabled: true, // No leverage to adjust
  supportedTPSLTriggerTypes: [], // No TP/SL in prediction markets
  defaultTPTriggerType: undefined,
  defaultSLTriggerType: undefined,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: true,
  leverageIsGlobal: true,
  tvMaxGetBarFetches: 10,
  candleApiCountLimit: 500,
  klineCallbackResetThreshold: 20,
  disconnectWarningThreshold: 20 * 1000,
  supportsFeeRates: false,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  privateProxyIps: [], // No proxy needed for Polygon
  latencyDomain: 'clob.polymarket.com',
  features: {
    kline: false, // Limited historical data
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const LighterMetadata: ExchangeMetadata = {
  ...defaultMetaData,
  supportsPublicOnly: true,
  exchangeName: 'lighter' as ExchangeName,
  logo: '/static/exchanges/logos/LIGHTER/logo.svg',
  lightLogo: '/static/exchanges/logos/LIGHTER/light.png',
  darkLogo: '/static/exchanges/logos/LIGHTER/dark.png',
  refLink: 'https://lighter.xyz',
  refImage: '/static/exchanges/reflinks/lighter/ref.png',
  requiresPassphrase: false,
  requiresApplicationId: true,
  requiresApiKeyIndex: true,
  requiresNoSecret: false,
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportedOnMobile: true,
  supportsTestnet: false,
  orderbookTickDelay: 100,
  prettyName: 'Lighter',
  defaultQuote: 'USDC',
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  positionModes: [PositionMode.OneWay],
  forceAccountPositionModeChangeOnExchangeItself: false,
  hidePositionModeSelector: true,
  requiresCors: false,
  forbidCors: true,
  supportsWsRoute: true,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  supportsTrailingStopLoss: false,
  tickInterval: 2000,
  supportsChaser: false,
  supportsPositionMargin: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  leverageMagnets: [1, 2, 3, 5, 10, 15, 20, 25, 30, 50],
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: true,
  leverageIsGlobal: true,
  tvMaxGetBarFetches: 10,
  candleApiCountLimit: 500,
  klineCallbackResetThreshold: 20,
  // Lighter private/public pongs can be sporadic even while market streams are healthy.
  // Keep warning signal less noisy for normal quiet periods.
  disconnectWarningThreshold: 60 * 1000,
  supportsFeeRates: true,
  alwaysCalculateUnrealizedPnlFromLastPrice: false,
  fetchOHLCVInWorker: true,
  features: {
    kline: true,
    orderbook: true,
    orders: true,
    positions: true,
    balance: true,
  },
};

const LighterRobinhoodMetadata: ExchangeMetadata = {
  ...LighterMetadata,
  exchangeName: 'lighter-robinhood' as ExchangeName,
  refLink: 'https://lighter.xyz',
  prettyName: 'Lighter Robinhood',
  defaultQuote: 'USDG',
  enabled: false,
  enabledInDev: true,
  enableInPreview: true,
  supportedOnMobile: false,
  supportsWsRoute: true,
  latencyDomain: 'api.rh.lighter.xyz',
};

export const exchangeMetadata: {
  [key in ExchangeName]: ExchangeMetadata;
} = {
  asterdex: AsterDEXMetadata,
  backpack: BackpackMetadata,
  bybit: BybitMetadata,
  bybitdemo: BybitDemoMetadata,
  hyrotrader: HyrotraderMetadata,
  bybitspot: BybitSpotMetadata,
  bybitinverse: BybitInverseMetadata,
  binance: BinanceMetadata,
  binanceinverse: BinanceInverseMetadata,
  binanceoptions: BinanceOptionsMetadata,
  binancepm: BinancePMMetadata,
  binancespot: BinanceSpotMetadata,
  bitget: BitgetMetadata,
  bitgetinverse: BitgetInverseMetadata,
  bitgetusdc: BitgetUSDCMetadata,
  bitunix: BitunixMetadata,
  weex: WeexMetadata,
  mexc: MexcMetadata,
  coinbase: CoinbaseMetadata,
  cryptocom: CryptocomMetadata,
  grvt: GrvtMetadata,
  phemex: PhemexMetadata,
  bingx: BingxMetadata,
  bitrue: BitrueMetadata,
  breakoutprop: BreakoutpropMetadata,
  blofin: BlofinMetadata,
  lighter: LighterMetadata,
  'lighter-robinhood': LighterRobinhoodMetadata,
  okx: OkxMetadata,
  okxdemo: OkxDemoMetadata,
  okxeu: OkxEUMetadata,
  okxeudemo: OkxEUDemoMetadata,
  woo: WooMetadata,
  woofi: WooFiMetadata,
  coinbasespot: CoinbaseSpotMetadata,
  kodiakfi: KodiakFIMetadata,
  mode: MODEMetadata,
  salsadex: SalsaDEXMetadata,
  'tealstreet-dex': TealstreetDEXMetadata,
  what: WhatMetadata,
  hyperliquid: HyperliquidMetadata,
  hyperliquidspot: HyperliquidSpotMetadata,
  txflow: TxFlowMetadata,
  apexomni: ApexOmniMetadata,
  kucoin: KucoinMetadata,
  kucoinspot: KucoinSpotMetadata,
  kraken: KrakenMetadata,
  krakenspot: KrakenSpotMetadata,
  paradex: ParadexMetadata,
  polymarket: PolymarketMetadata,
};

export const getExchangeMetadata = (exchange: ExchangeName, opts: Record<string, any> = {}): ExchangeMetadata => {
  if (exchange === 'bybit') {
    return opts.isUnified ? BybitUnifiedMetadata : BybitMetadata;
  }
  const metadata = exchangeMetadata[exchange];
  if (!metadata) {
    throw new Error(`Unknown exchange metadata: ${exchange}`);
  }
  return metadata;
};

export const isExchangeSupportedOnMobile = (exchange: string): boolean => {
  return exchangeMetadata[exchange as ExchangeName]?.supportedOnMobile === true;
};

export const exchangePrettyNames: { [key in ExchangeName]: string } = Object.keys(exchangeMetadata).reduce(
  (exchange, key) => {
    const exchangeName = key as ExchangeName;
    exchange[exchangeName] = exchangeMetadata[exchangeName]?.prettyName || key;
    return exchange;
  },
  {} as { [key in ExchangeName]: string },
);

/** Map of latencyDomain -> array of exchange names that use this domain */
export const latencyDomainToExchanges: Map<string, ExchangeName[]> = (() => {
  const map = new Map<string, ExchangeName[]>();
  for (const [exchangeName, metadata] of Object.entries(exchangeMetadata)) {
    if (metadata.latencyDomain) {
      const existing = map.get(metadata.latencyDomain) || [];
      existing.push(exchangeName as ExchangeName);
      map.set(metadata.latencyDomain, existing);
    }
  }
  return map;
})();

/** Get all unique latency domains */
export const getUniqueLatencyDomains = (): string[] => {
  return Array.from(latencyDomainToExchanges.keys());
};
