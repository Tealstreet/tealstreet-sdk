// Exchange configurations for E2E tests
// Single source of truth — used by exchanges.ts, run-e2e.js, and run-e2e-parallel.js

module.exports = {
  woo: {
    exchange: 'woo',
    // BTC: current WOO perp with both top-of-book sides inside WOO's price band.
    // ETH/XRP can publish asks above the allowed buy cap, so market-buy IOC
    // emulation cannot fill even though the exchange accepts the symbol.
    symbol: 'BTCUSDT',
    // Keep destructive position tests small. The shared Woo test account is
    // currently funded around $20 total / $15 free collateral.
    orderSizeInQuote: 10,
    setupLeverage: 10,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    // WOO caps how far through the touch a limit order may be priced:
    // PERP_ETH_USDT publishes bid_cap_ratio 1.01 / ask_floor_ratio 0.99, so a
    // crossing order at farTouchDelta (1.5%) is rejected with "The order price
    // does not meet the price range requirement of this instrument" and never
    // opens the position the reduce/hedge tests need.
    crossDelta: 0.008,
    expectedWalletBalance: 20.5,
    expectedAvailableMargin: 15,
    balanceVariance: 0.4,
    actionWaitTime: 10000,
    // WOO exposes a fresh fill in positions quickly, but this shared account
    // has been observed flattening again within a few seconds. Assert opens
    // inside that window; keep slowUpdateExtraWaitTime for cleanup/close paths.
    openPositionSettleWaitTime: 1500,
    reduceOnlyLimitPostOnly: true,
    slowUpdateExtraWaitTime: 10000, // WOO's position readback can 429 and asks callers to retry after 10s
    accountExtra: { tickInterval: 10000 },
    skipTests: [],
  },

  phemex: {
    exchange: 'phemex',
    symbol: 'ADAUSDT',
    orderSizeInQuote: 10,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 89,
    expectedAvailableMargin: 89,
    slowUpdateExtraWaitTime: 2000, // Phemex has WebSocket positions but can be slightly delayed
    skipTests: [],
  },

  binance: {
    exchange: 'binance',
    symbol: 'DOGEUSDT',
    orderSizeInQuote: 10,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 127,
    expectedAvailableMargin: 127,
    skipTests: [],
  },

  kraken: {
    exchange: 'kraken',
    // Kraken Futures markets are the raw `PF_`-prefixed symbols — the market
    // dropdown matches on `data-symbol`, which carries `market.symbol`, not the
    // `PF_`-stripped prettySymbol. DOGE is the cheapest liquid contract and
    // Kraken supports whole-contract sizing, so keep orders small to avoid
    // liquidation-risk rejections while retaining negligible rounding drift.
    symbol: 'PF_DOGEUSD',
    orderSizeInQuote: 2,
    marketOrderSizeInQuote: 0.2,
    positionModeOrderSizeInQuote: 0.1,
    marketOrderSides: ['buy'],
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    crossDelta: 0.003,
    expectedWalletBalance: 300,
    expectedAvailableMargin: 300,
    // The shared E2E account is funded in Kraken's flex multi-collateral
    // account. Isolated PF_DOGEUSD orders draw from fi_dogeusd, which has no
    // collateral and rejects even tiny test orders as wouldCauseLiquidation.
    marginTypes: ['cross'],
    // Kraken Futures perps are USD-quoted, so the size cell's unit reads 'USD'.
    quoteCurrency: 'USD',
    // The orders/positions tables render `market.prettySymbol`, and Kraken's strips
    // the PF_ prefix — so they show DOGEUSD while the market dropdown (data-symbol,
    // the raw market.symbol) needs PF_DOGEUSD.
    displaySymbol: 'DOGEUSD',
    skipTests: [],
  },

  blofin: {
    exchange: 'blofin',
    symbol: 'DOGEUSDT',
    orderSizeInQuote: 15,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 100,
    expectedAvailableMargin: 100,
    actionWaitTime: 5000,
    skipTests: [],
  },

  hyperliquid: {
    exchange: 'hyperliquid',
    symbol: 'DOGE-PERP',
    quoteCurrency: 'USDC', // Hyperliquid perps are USDC-quoted, not USDT
    orderSizeInQuote: 12, // HL has $10 minimum notional
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 54,
    expectedAvailableMargin: 54,
    skipTests: [],
  },

  paradex: {
    exchange: 'paradex',
    symbol: 'ETH-USD-PERP',
    quoteCurrency: 'USD',
    orderSizeInQuote: 12,
    marketOrderSizeInQuote: 12,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 16,
    expectedAvailableMargin: 16,
    skipTests: [],
  },

  bybit: {
    exchange: 'bybit',
    symbol: 'DOGEUSDT',
    orderSizeInQuote: 10,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 293,
    expectedAvailableMargin: 293,
    skipTests: [],
    // Opted in for the position-flip test. The test itself is the verification: it
    // asserts the account ends SHORT at ~1x, so a venue that rejects or clamps the
    // oversized order fails rather than silently passing on a flat account.
    supportsPositionFlip: true,
  },

  bitget: {
    exchange: 'bitget',
    symbol: 'TRXUSDT',
    orderSizeInQuote: 5.5,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 6,
    expectedAvailableMargin: 6,
    balanceVariance: 0.25,
    slowUpdateExtraWaitTime: 2000, // Bitget REST API caches responses for a few seconds
    virtualTpSlSyncWait: 12000, // Virtual TPSL orders rely on REST polling; Bitget caches REST responses for several seconds
    hedgeModeLeverage: 50,
    skipTests: [],
  },

  bitunix: {
    exchange: 'bitunix',
    symbol: 'LINKUSDT',
    orderSizeInQuote: 2, // Bitunix LINK min: 0.1 LINK (~$0.86); $2 keeps orders small for low-balance account
    orderSizeVariance: 0.2,
    farTouchDelta: 0.015,
    expectedWalletBalance: 100,
    expectedAvailableMargin: 100,
    skipTests: [],
    // Bitunix change_position_mode API intermittently returns {"code":1,"msg":"Network Error"}
    // which crashes exchange startup and prevents orders from loading. Allow page reload retry.
    ignoredLoadErrors: ['Network Error'],
    badApiKeyMaxHttpRequests: 120,
  },

  okx: {
    exchange: 'okx',
    symbol: 'DOGEUSDT',
    orderSizeInQuote: 10,
    orderSizeVariance: 0.2,
    farTouchDelta: 0.008, // OKX has ~1% price band for DOGE; 1.5% exceeds it
    // Observed 215.44 once the account was flattened; the old 50 predated that
    // and failed even the 100% band (expected 215.44 to be close to 50 +/- 50).
    expectedWalletBalance: 215,
    expectedAvailableMargin: 215,
    skipTests: [],
    // Opted in for the position-flip test. The test itself is the verification: it
    // asserts the account ends SHORT at ~1x, so a venue that rejects or clamps the
    // oversized order fails rather than silently passing on a flat account.
    supportsPositionFlip: true,
  },

  // bingx: {
  //   exchange: 'bingx',
  //   symbol: 'DOGEUSDT',
  //   orderSizeInQuote: 10,
  //   orderSizeVariance: 0.2,
  //   farTouchDelta: 0.015,
  //   expectedWalletBalance: 100,
  //   expectedAvailableMargin: 100,
  //   actionWaitTime: 8000,
  //   slowUpdateExtraWaitTime: 6000,
  // },

  // Default config for audit mode — used when no exchange-specific config exists
  _default: {
    symbol: 'BTCUSDT',
    orderSizeInQuote: 10,
    orderSizeVariance: 0.3,
    farTouchDelta: 0.02,
    expectedWalletBalance: 0.01,
    expectedAvailableMargin: 0.01,
  },
};
